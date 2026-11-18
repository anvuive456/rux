//! Calling native (Kotlin / Swift) code from Rust — the runtime behind `#[rux::host_api]`.
//!
//! An API is declared once in Rust (`#[rux::host_api] trait …`); `rux build` generates the
//! matching Kotlin / Swift interfaces, which the app or a plugin implements. A call:
//!
//! 1. the generated Rust client encodes the arguments ([`codec`]) and calls [`call_sync`] /
//!    [`call_async`] with the API name (`"<crate>.<Trait>"`) and the method's index;
//! 2. the backend's [`Transport`] hands it to the native runtime (`RuxChannels`), which runs
//!    the registered implementation;
//! 3. the native side answers with [`deliver`] — immediately for sync methods (the whole app
//!    runs on the UI thread, so a sync call is a plain nested call), later for async ones.
//!
//! Reply payload: `0` + encoded value, or `1` + error code + message (two strings).

pub mod codec;
pub mod testing;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;

pub use codec::{Codec, DecodeError, Reader, Writer, decode, encode};

/// An error from native code (or from getting there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeError {
    /// `not_implemented`, `no_transport`, `no_reply`, `decode`, or a code set by native code.
    pub code: String,
    pub message: String,
}

impl NativeError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for NativeError {}

impl From<DecodeError> for NativeError {
    fn from(e: DecodeError) -> Self {
        Self::new("decode", e.to_string())
    }
}

/// Delivers calls to the native runtime; installed by the platform backend.
pub trait Transport {
    /// Calls `method` of `api`. The native side answers through [`deliver`] with `call_id`.
    fn invoke(&self, api: &str, method: u32, call_id: u64, args: &[u8]);
}

type Callback = Box<dyn FnOnce(Result<Vec<u8>, NativeError>)>;

/// Anchor whose address seeds call ids (see [`next_id`]).
static ID_SEED: u8 = 0;

thread_local! {
    static TRANSPORT: RefCell<Option<Box<dyn Transport>>> = const { RefCell::new(None) };
    static NEXT_ID: Cell<u64> = const { Cell::new(0) };
    /// Replies to sync calls, picked up when the call returns.
    static REPLIES: RefCell<HashMap<u64, Vec<u8>>> = RefCell::new(HashMap::new());
    /// Pending async calls.
    static CALLBACKS: RefCell<HashMap<u64, Callback>> = RefCell::new(HashMap::new());
}

/// Installs (or removes) the transport. Called by the platform backend on start.
pub fn set_transport(transport: Option<Box<dyn Transport>>) {
    TRANSPORT.with(|t| *t.borrow_mut() = transport);
}

/// Call ids are unique across hot-reloaded builds (each build has its own copy of this crate):
/// they start at an address inside this build, so a late reply to an older build's call never
/// matches a newer build's call.
fn next_id() -> u64 {
    NEXT_ID.with(|n| {
        let id = match n.get() {
            0 => &raw const ID_SEED as u64,
            id => id + 1,
        };
        n.set(id);
        id
    })
}

fn invoke(api: &str, method: u32, call_id: u64, args: &[u8]) -> Result<(), NativeError> {
    TRANSPORT.with(|t| match t.borrow().as_ref() {
        Some(t) => {
            t.invoke(api, method, call_id, args);
            Ok(())
        }
        None => Err(NativeError::new(
            "no_transport",
            format!("no native runtime to call {api} (headless? see rux::native::testing)"),
        )),
    })
}

/// Calls a sync native method; returns the encoded result.
pub fn call_sync(api: &str, method: u32, args: Vec<u8>) -> Result<Vec<u8>, NativeError> {
    let id = next_id();
    invoke(api, method, id, &args)?;
    let reply = REPLIES
        .with(|r| r.borrow_mut().remove(&id))
        .ok_or_else(|| {
            NativeError::new(
                "no_reply",
                format!("{api} method #{method} did not reply synchronously"),
            )
        })?;
    parse_reply(&reply)
}

/// Calls an async native method; `callback` runs on the UI thread with the encoded result.
pub fn call_async(
    api: &str,
    method: u32,
    args: Vec<u8>,
    callback: impl FnOnce(Result<Vec<u8>, NativeError>) + 'static,
) {
    let id = next_id();
    CALLBACKS.with(|c| c.borrow_mut().insert(id, Box::new(callback)));
    if let Err(e) = invoke(api, method, id, &args)
        && let Some(cb) = CALLBACKS.with(|c| c.borrow_mut().remove(&id))
    {
        cb(Err(e));
    }
}

/// A reply from native code. Returns `true` if it completed an async call (whose callback
/// may have changed state: the backend should re-render).
pub fn deliver(call_id: u64, payload: &[u8]) -> bool {
    match CALLBACKS.with(|c| c.borrow_mut().remove(&call_id)) {
        Some(cb) => {
            cb(parse_reply(payload));
            true
        }
        None => {
            REPLIES.with(|r| r.borrow_mut().insert(call_id, payload.to_vec()));
            false
        }
    }
}

fn parse_reply(payload: &[u8]) -> Result<Vec<u8>, NativeError> {
    let mut r = Reader::new(payload);
    match r.u8()? {
        0 => Ok(payload[1..].to_vec()),
        1 => {
            let code = String::decode(&mut r)?;
            let message = String::decode(&mut r)?;
            Err(NativeError { code, message })
        }
        s => Err(DecodeError::new(format!("invalid reply status {s}")).into()),
    }
}

/// Encodes a successful reply (used by tests and mocks).
pub fn ok_reply<T: Codec>(value: &T) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(0);
    value.encode(&mut w);
    w.finish()
}

/// Encodes an error reply (used by tests and mocks).
pub fn error_reply(error: &NativeError) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(1);
    error.code.encode(&mut w);
    error.message.encode(&mut w);
    w.finish()
}

/// Decodes a method's result (used by the generated clients).
#[doc(hidden)]
pub fn decode_result<T: Codec>(bytes: Result<Vec<u8>, NativeError>) -> Result<T, NativeError> {
    Ok(decode(&bytes?)?)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn sync_and_async_calls_through_a_mock() {
        let _mock = testing::mock(|api, method, args| {
            assert_eq!(api, "demo.Api");
            match method {
                0 => Ok(ok_reply(&(decode::<i32>(args).unwrap() * 2))),
                _ => Err(NativeError::new("boom", "nope")),
            }
        });
        let doubled = decode_result::<i32>(call_sync("demo.Api", 0, encode(&21)));
        assert_eq!(doubled, Ok(42));

        let got = Rc::new(RefCell::new(None));
        let g = got.clone();
        call_async("demo.Api", 1, Vec::new(), move |r| {
            *g.borrow_mut() = Some(decode_result::<()>(r))
        });
        assert_eq!(
            got.borrow_mut().take(),
            Some(Err(NativeError::new("boom", "nope")))
        );
    }

    #[test]
    fn no_transport_is_an_error() {
        set_transport(None);
        assert_eq!(
            call_sync("x.Y", 0, vec![]).unwrap_err().code,
            "no_transport"
        );
        let failed = Rc::new(RefCell::new(false));
        let f = failed.clone();
        call_async("x.Y", 0, vec![], move |r| *f.borrow_mut() = r.is_err());
        assert!(*failed.borrow());
    }
}
