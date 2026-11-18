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

use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString, c_char, c_void};

use rux_core::{App, Event, EventKind, Mutation, NodeId};
use rux_platform::{Renderer, Runtime, dev, host};
use rux_state::hot::HotSignal;

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
    /// A native API call (`rux::native`): `RuxChannels.invoke` answers with `rux_native_reply`.
    pub invoke_native: unsafe extern "C" fn(Ctx, *const c_char, u32, u64, *const u8, usize),
}

/// Native API calls → the vtable's `invoke_native` (Swift `RuxChannels`).
struct VTableTransport;

impl rux_native::Transport for VTableTransport {
    fn invoke(&self, api: &str, method: u32, call_id: u64, args: &[u8]) {
        let Some(v) = VTABLE.with(Cell::get) else {
            return;
        };
        let api = cstr(api);
        // SAFETY: the pointers outlive the call; Swift copies what it keeps.
        unsafe {
            (v.invoke_native)(
                v.ctx,
                api.as_ptr(),
                method,
                call_id,
                args.as_ptr(),
                args.len(),
            )
        };
    }
}

/// A native API reply: completes the call, re-rendering if an async callback changed state.
fn deliver(call_id: u64, payload: &[u8]) {
    if rux_native::deliver(call_id, payload)
        && let Some(vtable) = VTABLE.with(Cell::get)
    {
        host::refresh(&mut VTableRenderer(vtable));
    }
}

/// Reply to a native API call. Called from `rux_native_reply` (Swift `RuxReply`).
///
/// # Safety
/// `payload` must point to `len` readable bytes (or be null with `len == 0`).
pub unsafe fn reply(call_id: u64, payload: *const u8, len: usize) {
    host::guard("rux_native_reply", || {
        let payload = if payload.is_null() {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(payload, len) }
        };
        match dev::active() {
            Some(lib) => lib.reply(call_id, payload),
            None => deliver(call_id, payload),
        }
    });
}

/// `rux_dev_reply`: a native API reply forwarded by the trampoline.
///
/// # Safety
/// `payload` must point to `len` readable bytes.
pub unsafe fn dev_reply(call_id: u64, payload: *const u8, len: usize) {
    host::guard("rux_dev_reply", || {
        deliver(call_id, unsafe { std::slice::from_raw_parts(payload, len) })
    });
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
        // After a hot reload, keep running the newest build.
        if let Some(lib) = dev::active() {
            // SAFETY: `vtable` is this backend's host handle and outlives the call.
            if let Err(e) = unsafe { lib.start(&raw const vtable as *const c_void, Vec::new()) } {
                eprintln!("[rux] {e}");
            }
            return;
        }
        rux_native::set_transport(Some(Box::new(VTableTransport)));
        let (app, _) = dev::create_app(create_app, Vec::new());
        host::install(Runtime::new(app), &mut VTableRenderer(vtable));
    });
}

/// Event from UIKit. Called from `rux_dispatch(id, event, value)`.
///
/// # Safety
/// `event` must be a valid C string; `value` must be a valid C string or NULL.
pub unsafe fn dispatch(id: u64, event: *const c_char, value: *const c_char) {
    host::guard("rux_dispatch", || {
        if event.is_null() {
            return;
        }
        let event = unsafe { CStr::from_ptr(event) }.to_string_lossy();
        let value = (!value.is_null()).then(|| {
            unsafe { CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned()
        });
        match dev::active() {
            Some(lib) => lib.dispatch(id, &event, value.as_deref()),
            None => dispatch_event(id, &event, value),
        }
    });
}

fn dispatch_event(id: u64, event: &str, value: Option<String>) {
    let Some(vtable) = VTABLE.with(Cell::get) else {
        return;
    };
    let Some(kind) = EventKind::parse(event) else {
        eprintln!("[rux] unknown event {event:?}");
        return;
    };
    host::dispatch(
        NodeId(id),
        &Event { kind, value },
        &mut VTableRenderer(vtable),
    );
}

/// Tears down the runtime. Called from `rux_stop()`.
pub fn stop() {
    host::guard("rux_stop", || {
        match dev::active() {
            Some(lib) => drop(lib.stop()),
            None => host::uninstall(),
        }
        VTABLE.with(|v| v.set(None));
        rux_native::set_transport(None);
    });
}

thread_local! {
    /// Keeps the last `rux_reload` report alive for the caller.
    static REPORT: RefCell<CString> = RefCell::new(CString::default());
}

/// Hot reload (`keep_state`) / hot restart to the build at `path`. Called from `rux_reload`
/// by the dev client (debug builds). The returned string is valid until the next call.
///
/// # Safety
/// `path` must be a valid C string.
pub unsafe fn reload(path: *const c_char, keep_state: bool) -> *const c_char {
    let report = host::guard("rux_reload", || {
        let path = unsafe { CStr::from_ptr(path) }.to_string_lossy();
        let Some(vtable) = VTABLE.with(Cell::get) else {
            return "err the app is not running".to_owned();
        };
        // SAFETY: `vtable` is this backend's host handle and outlives the call.
        unsafe {
            dev::reload(
                &path,
                keep_state,
                &raw const vtable as *const c_void,
                || host::teardown(&mut VTableRenderer(vtable)),
            )
        }
    })
    .unwrap_or_else(|| "err panic while reloading".into());
    REPORT.with(|r| {
        *r.borrow_mut() = cstr(&report);
        r.borrow().as_ptr()
    })
}

// --- exported by every build, called by the trampoline (see `rux_platform::dev`) ---

/// `rux_dev_start`: start the app in this build on the trampoline's host.
///
/// # Safety
/// `handle` must point to a [`RuxHostVTable`]; see [`dev::export_start`] for the rest.
pub unsafe fn dev_start(
    handle: *const c_void,
    prev: *const HotSignal,
    prev_len: usize,
    msg: *mut c_char,
    msg_cap: usize,
    create_app: fn() -> Box<dyn App>,
) -> u32 {
    unsafe {
        dev::export_start(prev, prev_len, msg, msg_cap, |previous| {
            let vtable = *handle.cast::<RuxHostVTable>();
            VTABLE.with(|v| v.set(Some(vtable)));
            rux_native::set_transport(Some(Box::new(VTableTransport)));
            let (app, started) = dev::create_app(create_app, previous);
            host::install(Runtime::new(app), &mut VTableRenderer(vtable));
            started
        })
    }
}

/// `rux_dev_dispatch`: an event forwarded by the trampoline.
///
/// # Safety
/// `event` must be a valid C string; `value` a valid C string or null.
pub unsafe fn dev_dispatch(id: u64, event: *const c_char, value: *const c_char) {
    host::guard("rux_dev_dispatch", || {
        let event = unsafe { CStr::from_ptr(event) }.to_string_lossy();
        let value = (!value.is_null()).then(|| {
            unsafe { CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned()
        });
        dispatch_event(id, &event, value);
    });
}

/// `rux_dev_stop`: tear the app down, returning its signals.
///
/// # Safety
/// `len` must be valid for writes.
pub unsafe fn dev_stop(len: *mut usize) -> *const HotSignal {
    unsafe {
        dev::export_stop(len, || {
            if let Some(vtable) = VTABLE.with(Cell::get) {
                host::teardown(&mut VTableRenderer(vtable));
            }
            VTABLE.with(|v| v.set(None));
            rux_native::set_transport(None);
            rux_native::set_transport(None);
        })
    }
}
