//! Android backend.
//!
//! Rust calls methods on `dev.rux.runtime.RuxHost` (Kotlin, see `platforms/android`)
//! to create/update `android.view.View`s. Kotlin calls back through `RuxNative.nativeDispatch`.
//!
//! The JNI functions `Java_dev_rux_runtime_RuxNative_*` are generated in the app crate by
//! the `rux::main!` macro and forward to [`start`], [`dispatch`], [`stop`].
//!
//! All calls happen on Android's main (UI) thread.

use std::cell::RefCell;

pub use jni;
use jni::JNIEnv;
use jni::objects::{GlobalRef, JObject, JString, JValue};
use jni::sys::jlong;
use rux_core::{App, Event, EventKind, Mutation, NodeId};
use rux_platform::{Renderer, Runtime, host};

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

/// Starts the app. Called from `RuxNative.nativeStart(host)`.
pub fn start(mut env: JNIEnv, host: JObject, create_app: fn() -> Box<dyn App>) {
    host::guard("nativeStart", || {
        let global = match env.new_global_ref(host) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("[rux] cannot retain RuxHost: {e}");
                return;
            }
        };
        HOST.with(|h| *h.borrow_mut() = Some(global));
        let runtime = Runtime::new(create_app());
        with_renderer(&mut env, |r| host::install(runtime, r));
    });
}

/// Event from a View. Called from `RuxNative.nativeDispatch(id, event, value)`.
pub fn dispatch(mut env: JNIEnv, id: jlong, event: JString, value: JString) {
    host::guard("nativeDispatch", || {
        let Ok(event) = env.get_string(&event).map(String::from) else {
            return;
        };
        let Some(kind) = EventKind::parse(&event) else {
            eprintln!("[rux] unknown event {event:?}");
            return;
        };
        let value = if value.is_null() {
            None
        } else {
            env.get_string(&value).ok().map(String::from)
        };
        let event = Event { kind, value };
        with_renderer(&mut env, |r| host::dispatch(NodeId(id as u64), &event, r));
    });
}

/// Tears down the runtime. Called from `RuxNative.nativeStop()` (Activity.onDestroy).
pub fn stop() {
    host::guard("nativeStop", || {
        host::uninstall();
        HOST.with(|h| h.borrow_mut().take());
    });
}
