//! Android backend.
//!
//! Rust calls methods on `dev.rux.runtime.RuxHost` (Kotlin, see `platforms/android`)
//! to create/update `android.view.View`s. Kotlin calls back through `RuxNative.nativeDispatch`.
//!
//! The JNI functions `Java_dev_rux_runtime_RuxNative_*` are generated in the app crate by
//! the `rux::main!` macro and forward to [`start`], [`dispatch`], [`stop`].
//!
//! All calls happen on Android's main (UI) thread.

mod logcat;

use std::cell::{Cell, RefCell};
use std::ffi::{CStr, c_char, c_void};

pub use jni;
use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JString, JValue};
use jni::sys::{jlong, jstring};
use jni::{JNIEnv, JavaVM};
use rux_core::{App, Event, EventKind, Mutation, NodeId};
use rux_platform::{Renderer, Runtime, dev, host};
use rux_state::hot::HotSignal;

thread_local! {
    /// Global reference to the Kotlin `RuxHost` instance.
    static HOST: RefCell<Option<GlobalRef>> = const { RefCell::new(None) };
}

/// Renderer that applies mutations by calling JNI methods on `RuxHost`.
struct JniRenderer<'a, 'local> {
    env: &'a mut JNIEnv<'local>,
    host: &'a JObject<'static>,
}

impl JniRenderer<'_, '_> {
    fn call(&mut self, name: &str, sig: &str, args: &[JValue]) -> jni::errors::Result<()> {
        self.env.call_method(self.host, name, sig, args)?;
        Ok(())
    }

    fn call_with_strings(
        &mut self,
        name: &str,
        sig: &str,
        id: NodeId,
        strings: &[&str],
    ) -> jni::errors::Result<()> {
        let mut locals = Vec::with_capacity(strings.len());
        for s in strings {
            locals.push(self.env.new_string(s)?);
        }
        let mut args = vec![JValue::Long(id.0 as jlong)];
        args.extend(locals.iter().map(|s| JValue::Object(s)));
        let result = self.call(name, sig, &args);
        for s in locals {
            self.env.delete_local_ref(s)?;
        }
        result
    }

    fn apply_one(&mut self, m: &Mutation) -> jni::errors::Result<()> {
        const S: &str = "Ljava/lang/String;";
        match m {
            Mutation::Create { id, kind } => {
                self.call_with_strings("createNode", &format!("(J{S})V"), *id, &[kind.as_str()])
            }
            Mutation::SetProp { id, name, value } => self.call_with_strings(
                "setProp",
                &format!("(J{S}{S})V"),
                *id,
                &[name, &value.to_wire()],
            ),
            Mutation::RemoveProp { id, name } => {
                self.call_with_strings("removeProp", &format!("(J{S})V"), *id, &[name])
            }
            Mutation::InsertChild {
                parent,
                child,
                index,
            } => self.call(
                "insertChild",
                "(JJI)V",
                &[
                    JValue::Long(parent.0 as jlong),
                    JValue::Long(child.0 as jlong),
                    JValue::Int(*index as i32),
                ],
            ),
            Mutation::RemoveChild { parent, child } => self.call(
                "removeChild",
                "(JJ)V",
                &[
                    JValue::Long(parent.0 as jlong),
                    JValue::Long(child.0 as jlong),
                ],
            ),
            Mutation::Release { id } => {
                self.call("releaseNode", "(J)V", &[JValue::Long(id.0 as jlong)])
            }
            Mutation::SetRoot { id } => {
                self.call("setRoot", "(J)V", &[JValue::Long(id.0 as jlong)])
            }
            Mutation::Listen { id, event } => {
                self.call_with_strings("listen", &format!("(J{S})V"), *id, &[event.as_str()])
            }
            Mutation::Unlisten { id, event } => {
                self.call_with_strings("unlisten", &format!("(J{S})V"), *id, &[event.as_str()])
            }
        }
    }
}

impl Renderer for JniRenderer<'_, '_> {
    fn apply(&mut self, mutations: &[Mutation]) {
        for m in mutations {
            if let Err(e) = self.apply_one(m) {
                eprintln!("[rux] failed to apply {m:?}: {e}");
                if self.env.exception_check().unwrap_or(false) {
                    let _ = self.env.exception_describe();
                    let _ = self.env.exception_clear();
                }
            }
        }
    }
}

fn with_renderer<R>(env: &mut JNIEnv, f: impl FnOnce(&mut JniRenderer) -> R) -> Option<R> {
    HOST.with(|h| {
        let host = h.borrow();
        let host: &JObject<'static> = host.as_ref()?.as_obj();
        let mut renderer = JniRenderer { env, host };
        Some(f(&mut renderer))
    })
}

/// Raw handles a hot-reloaded build needs to render (see `rux_platform::dev`).
#[repr(C)]
struct HostHandle {
    vm: *mut jni::sys::JavaVM,
    host: jni::sys::jobject,
}

thread_local! {
    /// The JVM, to get a `JNIEnv` outside JNI entry points (native API calls; events coming
    /// through the hot reload trampoline).
    static VM: Cell<Option<&'static JavaVM>> = const { Cell::new(None) };
    /// `dev.rux.runtime.RuxChannels`, looked up from a JNI entry point (where the app's class
    /// loader is in scope) for native API calls.
    static CHANNELS: RefCell<Option<GlobalRef>> = const { RefCell::new(None) };
}

/// Remembers the JVM (once) and connects `rux::native` calls to `RuxChannels`.
fn install_native(env: &mut JNIEnv) {
    if VM.with(Cell::get).is_none()
        && let Ok(vm) = env.get_java_vm()
    {
        VM.with(|v| v.set(Some(Box::leak(Box::new(vm)))));
    }
    match env
        .find_class("dev/rux/runtime/RuxChannels")
        .and_then(|c| env.new_global_ref(c))
    {
        Ok(class) => {
            CHANNELS.with(|c| *c.borrow_mut() = Some(class));
            rux_native::set_transport(Some(Box::new(JniTransport)));
        }
        Err(e) => {
            let _ = env.exception_clear();
            eprintln!("[rux] native APIs unavailable: {e}");
        }
    }
}

/// Native API calls → `RuxChannels.invoke(api, method, callId, args)` (Kotlin).
struct JniTransport;

impl rux_native::Transport for JniTransport {
    fn invoke(&self, api: &str, method: u32, call_id: u64, args: &[u8]) {
        let Some(mut env) = vm_env() else { return };
        CHANNELS.with(|c| {
            let c = c.borrow();
            let Some(class) = c.as_ref() else { return };
            let class: &JClass = class.as_obj().into();
            let result = (|| -> jni::errors::Result<()> {
                let api = env.new_string(api)?;
                let args = env.byte_array_from_slice(args)?;
                env.call_static_method(
                    class,
                    "invoke",
                    "(Ljava/lang/String;IJ[B)V",
                    &[
                        JValue::Object(&api),
                        JValue::Int(method as i32),
                        JValue::Long(call_id as jlong),
                        JValue::Object(&args),
                    ],
                )?;
                env.delete_local_ref(api)?;
                env.delete_local_ref(args)?;
                Ok(())
            })();
            if let Err(e) = result {
                eprintln!("[rux] native call {api}#{method} failed: {e}");
                if env.exception_check().unwrap_or(false) {
                    let _ = env.exception_describe();
                    let _ = env.exception_clear();
                }
            }
        });
    }
}

/// A native API reply: completes the call, re-rendering if an async callback changed state.
fn deliver(env: &mut JNIEnv, call_id: u64, payload: &[u8]) {
    if rux_native::deliver(call_id, payload) {
        with_renderer(env, |r| host::refresh(r));
    }
}

/// Reply to a native API call. Called from `RuxNative.nativeReply(callId, payload)`.
pub fn reply(mut env: JNIEnv, call_id: jlong, payload: JByteArray) {
    host::guard("nativeReply", || {
        let payload = env.convert_byte_array(&payload).unwrap_or_default();
        match dev::active() {
            Some(lib) => lib.reply(call_id as u64, &payload),
            None => deliver(&mut env, call_id as u64, &payload),
        }
    });
}

/// The handle to pass to a hot-reloaded build: this thread's JVM + the `RuxHost` global ref.
fn host_handle(env: &JNIEnv) -> Option<HostHandle> {
    let vm = env.get_java_vm().ok()?.get_java_vm_pointer();
    let host = HOST.with(|h| h.borrow().as_ref().map(|g| g.as_obj().as_raw()))?;
    Some(HostHandle { vm, host })
}

/// Starts the app. Called from `RuxNative.nativeStart(host)`.
pub fn start(mut env: JNIEnv, host: JObject, create_app: fn() -> Box<dyn App>) {
    logcat::redirect_stdio();
    host::guard("nativeStart", || {
        let global = match env.new_global_ref(host) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("[rux] cannot retain RuxHost: {e}");
                return;
            }
        };
        HOST.with(|h| *h.borrow_mut() = Some(global));
        // After a hot reload (e.g. the Activity is recreated), keep running the newest build.
        if let Some(lib) = dev::active() {
            let Some(handle) = host_handle(&env) else {
                return;
            };
            // SAFETY: `handle` is this backend's host handle and outlives the call.
            if let Err(e) = unsafe { lib.start(&raw const handle as *const c_void, Vec::new()) } {
                eprintln!("[rux] {e}");
            }
            return;
        }
        install_native(&mut env);
        let (app, _) = dev::create_app(create_app, Vec::new());
        with_renderer(&mut env, |r| host::install(Runtime::new(app), r));
    });
}

/// Event from a View. Called from `RuxNative.nativeDispatch(id, event, value)`.
pub fn dispatch(mut env: JNIEnv, id: jlong, event: JString, value: JString) {
    host::guard("nativeDispatch", || {
        let Ok(event) = env.get_string(&event).map(String::from) else {
            return;
        };
        let value = if value.is_null() {
            None
        } else {
            env.get_string(&value).ok().map(String::from)
        };
        match dev::active() {
            Some(lib) => lib.dispatch(id as u64, &event, value.as_deref()),
            None => dispatch_event(&mut env, id as u64, &event, value),
        }
    });
}

fn dispatch_event(env: &mut JNIEnv, id: u64, event: &str, value: Option<String>) {
    let Some(kind) = EventKind::parse(event) else {
        eprintln!("[rux] unknown event {event:?}");
        return;
    };
    let event = Event { kind, value };
    with_renderer(env, |r| host::dispatch(NodeId(id), &event, r));
}

/// Tears down the runtime. Called from `RuxNative.nativeStop()` (Activity.onDestroy).
pub fn stop() {
    host::guard("nativeStop", || {
        match dev::active() {
            Some(lib) => drop(lib.stop()),
            None => host::uninstall(),
        }
        HOST.with(|h| h.borrow_mut().take());
        rux_native::set_transport(None);
    });
}

/// Hot reload (`keep_state`) / hot restart to the build at `path`. Called from
/// `RuxNative.nativeReload(path, keepState)` by the dev client (debug builds).
pub fn reload(mut env: JNIEnv, path: JString, keep_state: bool) -> jstring {
    let report = host::guard("nativeReload", || {
        let path: String = env.get_string(&path).map(Into::into).unwrap_or_default();
        let Some(handle) = host_handle(&env) else {
            return "err the app is not running".to_owned();
        };
        // SAFETY: `handle` is this backend's host handle and outlives the call.
        unsafe {
            dev::reload(
                &path,
                keep_state,
                &raw const handle as *const c_void,
                || {
                    with_renderer(&mut env, |r| host::teardown(r));
                },
            )
        }
    })
    .unwrap_or_else(|| "err panic while reloading (see logcat)".into());
    env.new_string(report)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

// --- exported by every build, called by the trampoline (see `rux_platform::dev`) ---

/// `rux_dev_start`: start the app in this build on the trampoline's host.
///
/// # Safety
/// `handle` must point to a [`HostHandle`]; see [`dev::export_start`] for the rest.
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
            let handle = &*handle.cast::<HostHandle>();
            let vm = JavaVM::from_raw(handle.vm).expect("JavaVM");
            let mut env = vm.get_env().expect("JNIEnv on the UI thread");
            install_native(&mut env);
            // The trampoline keeps its own global ref alive; take ours for this build.
            let host = JObject::from_raw(handle.host);
            let global = env.new_global_ref(&host).expect("RuxHost global ref");
            HOST.with(|h| *h.borrow_mut() = Some(global));
            let (app, started) = dev::create_app(create_app, previous);
            with_renderer(&mut env, |r| host::install(Runtime::new(app), r));
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
        let Some(mut env) = vm_env() else { return };
        dispatch_event(&mut env, id, &event, value);
    });
}

/// `rux_dev_stop`: tear the app down, returning its signals.
///
/// # Safety
/// `len` must be valid for writes.
pub unsafe fn dev_stop(len: *mut usize) -> *const HotSignal {
    unsafe {
        dev::export_stop(len, || {
            if let Some(mut env) = vm_env() {
                with_renderer(&mut env, |r| host::teardown(r));
            }
            HOST.with(|h| h.borrow_mut().take());
            rux_native::set_transport(None);
        })
    }
}

/// `rux_dev_reply`: a native API reply forwarded by the trampoline.
///
/// # Safety
/// `payload` must point to `len` readable bytes.
pub unsafe fn dev_reply(call_id: u64, payload: *const u8, len: usize) {
    host::guard("rux_dev_reply", || {
        let payload = unsafe { std::slice::from_raw_parts(payload, len) };
        if let Some(mut env) = vm_env() {
            deliver(&mut env, call_id, payload);
        }
    });
}

/// A `JNIEnv` for the current (UI) thread.
fn vm_env() -> Option<JNIEnv<'static>> {
    VM.with(Cell::get)?.get_env().ok()
}
