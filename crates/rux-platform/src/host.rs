//! Per-UI-thread global runtime, shared by the FFI backends.
//!
//! The native host calls into Rust through stateless `extern` functions, so the runtime
//! is kept in a `thread_local!` on the UI thread.

use std::cell::RefCell;

use rux_core::{Event, NodeId};

use crate::{Renderer, Runtime};

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Installs the runtime and performs the first render.
pub fn install(mut runtime: Runtime, renderer: &mut dyn Renderer) {
    runtime.render(renderer);
    RUNTIME.with(|rt| *rt.borrow_mut() = Some(runtime));
}

/// Sends an event to the running runtime.
///
/// Returns `false` if there is no runtime or it is busy (an event fired
/// synchronously while mutations are being applied — e.g. `setText` triggering a TextWatcher).
pub fn dispatch(id: NodeId, event: &Event, renderer: &mut dyn Renderer) -> bool {
    RUNTIME.with(|rt| match rt.try_borrow_mut() {
        Ok(mut guard) => match guard.as_mut() {
            Some(runtime) => {
                runtime.dispatch(id, event, renderer);
                true
            }
            None => false,
        },
        Err(_) => false,
    })
}

/// Re-renders if state changed outside an event (e.g. in an async native call's callback).
/// No-op while the runtime is busy (the change is picked up by the render in progress).
pub fn refresh(renderer: &mut dyn Renderer) {
    RUNTIME.with(|rt| {
        if let Ok(mut guard) = rt.try_borrow_mut()
            && let Some(runtime) = guard.as_mut()
            && rux_state::take_dirty()
        {
            runtime.render(renderer);
        }
    });
}

/// Releases the runtime's native nodes and drops it (hot reload hands over to a new build).
pub fn teardown(renderer: &mut dyn Renderer) {
    if let Some(mut runtime) = RUNTIME.with(|rt| rt.borrow_mut().take()) {
        runtime.teardown(renderer);
    }
}

/// Moving the running app to a new native host, keeping its state (e.g. an Android Activity
/// recreated for a configuration change), step 1: releases the app's nodes on the old host
/// (`renderer`) but keeps the app. Returns `false` if no runtime is installed. Then call
/// [`mount`] with the new host.
pub fn unmount(renderer: &mut dyn Renderer) -> bool {
    RUNTIME.with(|rt| match rt.borrow_mut().as_mut() {
        Some(runtime) => {
            runtime.teardown(renderer);
            true
        }
        None => false,
    })
}

/// Step 2 of moving the app to a new host (see [`unmount`]): renders the whole tree on it.
pub fn mount(renderer: &mut dyn Renderer) {
    RUNTIME.with(|rt| {
        if let Some(runtime) = rt.borrow_mut().as_mut() {
            runtime.render(renderer);
        }
    });
}

/// Tears down the runtime (Activity/Scene destroyed).
pub fn uninstall() {
    RUNTIME.with(|rt| rt.borrow_mut().take());
}

/// Runs `f`, catching panics so they never unwind across the FFI boundary.
pub fn guard<R>(context: &str, f: impl FnOnce() -> R) -> Option<R> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => Some(r),
        Err(e) => {
            let msg = e
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| e.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic>".into());
            eprintln!("[rux] panic in {context}: {msg}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use rux_core::{App, Element, ElementKind};
    use rux_state::Signal;

    use crate::Runtime;
    use crate::testing::HeadlessRenderer;

    struct Counter(Signal<i32>);
    impl App for Counter {
        fn view(&self) -> Element {
            Element::new(ElementKind::Column).child(
                Element::new(ElementKind::Text).prop("text", format!("Count: {}", self.0.get())),
            )
        }
    }

    #[test]
    fn moves_the_running_app_to_a_new_host_keeping_its_state() {
        let count = Signal::new(0);
        let mut old = HeadlessRenderer::new();
        super::install(Runtime::new(Box::new(Counter(count.clone()))), &mut old);
        count.set(3);
        super::refresh(&mut old);
        assert!(old.find_by_text("Count: 3").is_some());

        assert!(super::unmount(&mut old));
        assert!(
            old.nodes.is_empty(),
            "every node is released on the old host"
        );
        let mut new = HeadlessRenderer::new();
        super::mount(&mut new);
        assert!(new.find_by_text("Count: 3").is_some());

        super::uninstall();
        assert!(!super::unmount(&mut new));
    }
}
