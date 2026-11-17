//! # rux
//!
//! Write your UI once in Rust, render it with each platform's native widgets.
//!
//! ```ignore
//! use rux::prelude::*;
//!
//! struct Hello;
//! impl App for Hello {
//!     fn view(&self) -> Element {
//!         column([text("Hello from Rust")]).padding(16.0)
//!     }
//! }
//!
//! rux::main!(|| Hello);
//! ```
//!
//! Architecture:
//! - [`rux_core`]: virtual [`Element`] tree, [`Reconciler`] diffs it into [`Mutation`]s.
//! - [`rux_state`]: [`Signal`] — reactive state that schedules a re-render.
//! - [`rux_ui`]: built-in widgets (`column`, `text`, `button`, ...).
//! - [`rux_platform`]: trait [`Renderer`](rux_platform::Renderer), [`Runtime`](rux_platform::Runtime),
//!   and a headless renderer for tests.
//! - `rux-android` / `rux-ios`: backends that apply mutations to Android Views / UIKit.

pub use rux_core::*;
pub use rux_platform::{Renderer, Runtime, testing};
pub use rux_state::Signal;
pub use rux_ui::*;

pub mod prelude {
    pub use rux_core::{App, Color, Element, ElementKind, Event, EventKind};
    pub use rux_state::Signal;
    pub use rux_ui::*;
}

#[cfg(target_os = "android")]
#[doc(hidden)]
pub mod __android {
    pub use rux_android::*;
}

#[cfg(target_os = "ios")]
#[doc(hidden)]
pub mod __ios {
    pub use rux_ios::*;
}

#[doc(hidden)]
pub mod __dev {
    pub use rux_state::hot::HotSignal;
}

/// Declares the app entry point for every platform.
///
/// Takes a callable expression (`fn() -> impl App`), e.g. `Counter::new` or `|| Hello`.
///
/// - Android: generates the JNI functions `Java_dev_rux_runtime_RuxNative_*` (see `platforms/android`).
/// - iOS: generates `rux_start` / `rux_dispatch` / `rux_stop` (see `platforms/ios/.../rux.h`).
/// - Desktop: generates nothing — use [`testing::Harness`] to run headless.
///
/// Debug builds also export the hot reload entry points (`nativeReload` / `rux_reload` for the
/// dev client, `rux_dev_*` for the trampoline — see `rux_platform::dev`).
#[macro_export]
macro_rules! main {
    ($factory:expr) => {
        #[doc(hidden)]
        #[allow(dead_code)]
        fn __rux_create_app() -> ::std::boxed::Box<dyn $crate::App> {
            ::std::boxed::Box::new(($factory)())
        }

        #[cfg(target_os = "android")]
        const _: () = {
            use $crate::__android::jni::JNIEnv;
            use $crate::__android::jni::objects::{JClass, JObject, JString};
            use $crate::__android::jni::sys::jlong;

            #[unsafe(no_mangle)]
            pub extern "system" fn Java_dev_rux_runtime_RuxNative_nativeStart<'l>(
                env: JNIEnv<'l>,
                _class: JClass<'l>,
                host: JObject<'l>,
            ) {
                $crate::__android::start(env, host, __rux_create_app);
            }

            #[unsafe(no_mangle)]
            pub extern "system" fn Java_dev_rux_runtime_RuxNative_nativeDispatch<'l>(
                env: JNIEnv<'l>,
                _class: JClass<'l>,
                id: jlong,
                event: JString<'l>,
                value: JString<'l>,
            ) {
                $crate::__android::dispatch(env, id, event, value);
            }

            #[unsafe(no_mangle)]
            pub extern "system" fn Java_dev_rux_runtime_RuxNative_nativeStop<'l>(
                _env: JNIEnv<'l>,
                _class: JClass<'l>,
            ) {
                $crate::__android::stop();
            }

            #[cfg(debug_assertions)]
            #[unsafe(no_mangle)]
            pub extern "system" fn Java_dev_rux_runtime_RuxNative_nativeReload<'l>(
                env: JNIEnv<'l>,
                _class: JClass<'l>,
                path: JString<'l>,
                keep_state: $crate::__android::jni::sys::jboolean,
            ) -> $crate::__android::jni::sys::jstring {
                $crate::__android::reload(env, path, keep_state != 0)
            }
        };

        #[cfg(all(debug_assertions, any(target_os = "android", target_os = "ios")))]
        const _: () = {
            use ::std::ffi::{c_char, c_void};
            #[cfg(target_os = "android")]
            use $crate::__android as backend;
            use $crate::__dev::HotSignal;
            #[cfg(target_os = "ios")]
            use $crate::__ios as backend;

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn rux_dev_start(
                host: *const c_void,
                prev: *const HotSignal,
                prev_len: usize,
                msg: *mut c_char,
                msg_cap: usize,
            ) -> u32 {
                unsafe { backend::dev_start(host, prev, prev_len, msg, msg_cap, __rux_create_app) }
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn rux_dev_dispatch(
                id: u64,
                event: *const c_char,
                value: *const c_char,
            ) {
                unsafe { backend::dev_dispatch(id, event, value) }
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn rux_dev_stop(len: *mut usize) -> *const HotSignal {
                unsafe { backend::dev_stop(len) }
            }
        };

        #[cfg(target_os = "ios")]
        const _: () = {
            use ::std::os::raw::c_char;

            #[unsafe(no_mangle)]
            pub extern "C" fn rux_start(vtable: $crate::__ios::RuxHostVTable) {
                $crate::__ios::start(vtable, __rux_create_app);
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn rux_dispatch(
                id: u64,
                event: *const c_char,
                value: *const c_char,
            ) {
                unsafe { $crate::__ios::dispatch(id, event, value) };
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn rux_stop() {
                $crate::__ios::stop();
            }

            #[cfg(debug_assertions)]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn rux_reload(
                path: *const c_char,
                keep_state: bool,
            ) -> *const c_char {
                unsafe { $crate::__ios::reload(path, keep_state) }
            }
        };
    };
}
