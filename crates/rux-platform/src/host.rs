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
