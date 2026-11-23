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
//! - [`rux_ui`]: built-in layouts and views (`column`, `text`, `button`, ...).
//! - [`rux_platform`]: trait [`Renderer`](rux_platform::Renderer), [`Runtime`](rux_platform::Runtime),
//!   and a headless renderer for tests.
//! - `rux-android` / `rux-ios`: backends that apply mutations to Android Views / UIKit.

pub use rux_core::*;
pub use rux_platform::{Renderer, Runtime, testing};
pub use rux_state::Signal;
pub use rux_ui::*;

pub use rux_macros::{NativeType, async_call, host_api};
/// Calling native (Kotlin / Swift) code: declare an API with [`host_api`], implement it
/// natively, call it from Rust. See `rux_native` and the `device-info` example plugin.
pub use rux_native as native;

pub mod prelude {
    pub use rux_core::{App, Color, Element, ElementKind, Event, EventKind};
    pub use rux_macros::NativeType;
    pub use rux_native::NativeError;
    pub use rux_state::Signal;
    pub use rux_ui::*;
}

#[cfg(all(target_os = "android", feature = "android"))]
#[doc(hidden)]
pub mod __android {
    pub use rux_android::*;
}

#[cfg(all(target_os = "ios", feature = "ios"))]
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
/// - Android with the `android` feature: generates `Java_dev_rux_runtime_RuxNative_*`.
/// - iOS with the `ios` feature: generates `rux_start` / `rux_dispatch` / `rux_stop`.
/// - Headless builds can use [`testing::Harness`].
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

        $crate::__rux_android_entry_points!(__rux_create_app);
        $crate::__rux_ios_entry_points!(__rux_create_app);
        $crate::__rux_dev_entry_points!(__rux_create_app);
    };
}

// Resolve backend features in the facade, where they are defined. The app supplies its factory.

#[cfg(all(target_os = "android", feature = "android"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __rux_android_entry_points {
    ($factory:ident) => {
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
                $crate::__android::start(env, host, $factory);
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

            #[unsafe(no_mangle)]
            pub extern "system" fn Java_dev_rux_runtime_RuxNative_nativeReply<'l>(
                env: JNIEnv<'l>,
                _class: JClass<'l>,
                call_id: jlong,
                payload: $crate::__android::jni::objects::JByteArray<'l>,
            ) {
                $crate::__android::reply(env, call_id, payload);
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
    };
}

#[cfg(not(all(target_os = "android", feature = "android")))]
#[doc(hidden)]
#[macro_export]
macro_rules! __rux_android_entry_points {
    ($factory:ident) => {};
}

#[cfg(all(target_os = "ios", feature = "ios"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __rux_ios_entry_points {
    ($factory:ident) => {
        const _: () = {
            use ::std::os::raw::c_char;

            #[unsafe(no_mangle)]
            pub extern "C" fn rux_start(vtable: $crate::__ios::RuxHostVTable) {
                $crate::__ios::start(vtable, $factory);
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

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn rux_native_reply(
                call_id: u64,
                payload: *const u8,
                len: usize,
            ) {
                unsafe { $crate::__ios::reply(call_id, payload, len) }
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

#[cfg(not(all(target_os = "ios", feature = "ios")))]
#[doc(hidden)]
#[macro_export]
macro_rules! __rux_ios_entry_points {
    ($factory:ident) => {};
}

#[cfg(any(
    all(target_os = "android", feature = "android"),
    all(target_os = "ios", feature = "ios"),
))]
#[doc(hidden)]
#[macro_export]
macro_rules! __rux_dev_entry_points {
    ($factory:ident) => {
        #[cfg(debug_assertions)]
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
                unsafe { backend::dev_start(host, prev, prev_len, msg, msg_cap, $factory) }
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

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn rux_dev_reply(call_id: u64, payload: *const u8, len: usize) {
                unsafe { backend::dev_reply(call_id, payload, len) }
            }
        };
    };
}

#[cfg(not(any(
    all(target_os = "android", feature = "android"),
    all(target_os = "ios", feature = "ios"),
)))]
#[doc(hidden)]
#[macro_export]
macro_rules! __rux_dev_entry_points {
    ($factory:ident) => {};
}
