//! iOS backend.
//!
//! Swift (`platforms/ios/Sources/RuxRuntime/RuxHost.swift`) passes in a [`RuxHostVTable`]
//! of C function pointers; Rust calls them to create/update `UIView`s. Swift calls back
//! into `rux_dispatch` on events.
//!
//! The `rux_start` / `rux_dispatch` / `rux_stop` functions are generated in the app crate by
//! the `rux::main!` macro. Matching C header: `platforms/ios/Sources/CRux/include/rux.h`
//! — **must be kept in sync with the struct below**.
//!
//! All calls happen on the main thread.

use std::cell::Cell;
use std::ffi::{CStr, CString, c_char, c_void};

use rux_core::{App, Event, EventKind, Mutation, NodeId};
use rux_platform::{Renderer, Runtime, host};

type Ctx = *mut c_void;

/// Callback table provided by the Swift host. Layout matches `RuxHostVTable` in `rux.h`.
///
/// Strings passed across are NUL-terminated UTF-8, valid only for the duration of the call.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RuxHostVTable {
    pub ctx: Ctx,
    pub create_node: unsafe extern "C" fn(Ctx, u64, *const c_char),
    pub set_prop: unsafe extern "C" fn(Ctx, u64, *const c_char, *const c_char),
    pub remove_prop: unsafe extern "C" fn(Ctx, u64, *const c_char),
    pub insert_child: unsafe extern "C" fn(Ctx, u64, u64, u32),
    pub remove_child: unsafe extern "C" fn(Ctx, u64, u64),
    pub release_node: unsafe extern "C" fn(Ctx, u64),
    pub set_root: unsafe extern "C" fn(Ctx, u64),
    pub listen: unsafe extern "C" fn(Ctx, u64, *const c_char),
    pub unlisten: unsafe extern "C" fn(Ctx, u64, *const c_char),
}

thread_local! {
    static VTABLE: Cell<Option<RuxHostVTable>> = const { Cell::new(None) };
}

struct VTableRenderer(RuxHostVTable);

fn cstr(s: &str) -> CString {
    // Replace interior NULs so the C string is not truncated.
    CString::new(s.replace('\0', "\u{FFFD}")).expect("no interior NUL")
}

impl Renderer for VTableRenderer {
    fn apply(&mut self, mutations: &[Mutation]) {
        let v = &self.0;
        for m in mutations {
            // SAFETY: the function pointers and ctx come from Swift and live as long as RuxHost;
            // the CStrings outlive each call.
            unsafe {
                match m {
                    Mutation::Create { id, kind } => {
                        let k = cstr(kind.as_str());
                        (v.create_node)(v.ctx, id.0, k.as_ptr());
                    }
                    Mutation::SetProp { id, name, value } => {
                        let (n, val) = (cstr(name), cstr(&value.to_wire()));
                        (v.set_prop)(v.ctx, id.0, n.as_ptr(), val.as_ptr());
                    }
                    Mutation::RemoveProp { id, name } => {
                        let n = cstr(name);
                        (v.remove_prop)(v.ctx, id.0, n.as_ptr());
                    }
                    Mutation::InsertChild {
                        parent,
                        child,
                        index,
                    } => {
                        (v.insert_child)(v.ctx, parent.0, child.0, *index as u32);
                    }
                    Mutation::RemoveChild { parent, child } => {
                        (v.remove_child)(v.ctx, parent.0, child.0)
                    }
                    Mutation::Release { id } => (v.release_node)(v.ctx, id.0),
                    Mutation::SetRoot { id } => (v.set_root)(v.ctx, id.0),
                    Mutation::Listen { id, event } => {
                        let e = cstr(event.as_str());
                        (v.listen)(v.ctx, id.0, e.as_ptr());
                    }
                    Mutation::Unlisten { id, event } => {
                        let e = cstr(event.as_str());
                        (v.unlisten)(v.ctx, id.0, e.as_ptr());
                    }
                }
            }
        }
    }
}

/// Starts the app. Called from `rux_start(vtable)`.
pub fn start(vtable: RuxHostVTable, create_app: fn() -> Box<dyn App>) {
    host::guard("rux_start", || {
        VTABLE.with(|v| v.set(Some(vtable)));
        host::install(Runtime::new(create_app()), &mut VTableRenderer(vtable));
    });
}

/// Event from UIKit. Called from `rux_dispatch(id, event, value)`.
///
/// # Safety
/// `event` must be a valid C string; `value` must be a valid C string or NULL.
pub unsafe fn dispatch(id: u64, event: *const c_char, value: *const c_char) {
    host::guard("rux_dispatch", || {
        let Some(vtable) = VTABLE.with(Cell::get) else {
            return;
        };
        if event.is_null() {
            return;
        }
        let event = unsafe { CStr::from_ptr(event) }.to_string_lossy();
        let Some(kind) = EventKind::parse(&event) else {
            eprintln!("[rux] unknown event {event:?}");
            return;
        };
        let value = (!value.is_null()).then(|| {
            unsafe { CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned()
        });
        host::dispatch(
            NodeId(id),
            &Event { kind, value },
            &mut VTableRenderer(vtable),
        );
    });
}

/// Tears down the runtime. Called from `rux_stop()`.
pub fn stop() {
    host::guard("rux_stop", || {
        host::uninstall();
        VTABLE.with(|v| v.set(None));
    });
}
