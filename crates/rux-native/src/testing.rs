//! Fake native side for headless tests (`rux::testing::Harness`).
//!
//! ```ignore
//! let _native = rux::native::testing::mock(|api, method, _args| match (api, method) {
//!     ("my-plugin.DeviceApi", 0) => Ok(rux::native::ok_reply(&info)),
//!     _ => Err(NativeError::new("not_implemented", api)),
//! });
//! ```

use crate::{NativeError, Transport, deliver, error_reply, set_transport};

type Handler = Box<dyn Fn(&str, u32, &[u8]) -> Result<Vec<u8>, NativeError>>;

struct Mock(Handler);

impl Transport for Mock {
    fn invoke(&self, api: &str, method: u32, call_id: u64, args: &[u8]) {
        let reply = (self.0)(api, method, args).unwrap_or_else(|e| error_reply(&e));
        deliver(call_id, &reply);
    }
}

/// Answers every native call (sync and async, immediately) with `handler`, which returns an
/// encoded reply ([`ok_reply`](crate::ok_reply)) or an error. Removed when the guard drops.
pub fn mock(
    handler: impl Fn(&str, u32, &[u8]) -> Result<Vec<u8>, NativeError> + 'static,
) -> MockGuard {
    set_transport(Some(Box::new(Mock(Box::new(handler)))));
    MockGuard(())
}

pub struct MockGuard(());

impl Drop for MockGuard {
    fn drop(&mut self) {
        set_transport(None);
    }
}
