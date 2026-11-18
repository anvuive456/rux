# Testing

UI tests run headless with `cargo test`: no emulator, simulator or device. `rux::testing::Harness`
renders the app into an in-memory tree that tests click, type into and inspect.

```rust
use my_app::MyApp;
use rux::testing::Harness;

#[test]
fn increments() {
    let mut h = Harness::new(MyApp::new());
    h.click_text("+");                     // taps the node with this text
    assert!(h.dump().contains(r#"text="Count: 1""#));
}
```

| `Harness` | |
|-----------|-|
| `click_text(text)` | Taps the node with this text (panics if there is none) |
| `type_text(value)` | Types into the first text input |
| `dump()` | The rendered tree as text, to assert on |
| `renderer.applied` | Number of native mutations so far, to check an update stays minimal |

## Mocking native APIs

`rux::native::testing::mock` answers every native call (sync and async) while its guard lives.
Methods are numbered in declaration order:

```rust
use rux::native::{NativeError, ok_reply, testing};

let _native = testing::mock(|api, method, _args| match (api, method) {
    (DeviceInfoApi::API, 0) => Ok(ok_reply(&DeviceInfo { /* … */ })), // info()
    (DeviceInfoApi::API, 1) => Ok(ok_reply(&Battery { /* … */ })),    // battery()
    _ => Err(NativeError::new("not_implemented", "")),
});
```

Without a mock, native calls fail, so views can be tested for that case too. See
[`examples/counter/tests`](../examples/counter/tests).
