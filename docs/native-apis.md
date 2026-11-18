# Native APIs & plugins

Declare an API once in Rust, implement it in Kotlin and Swift, and call it from Rust, with types on
every side.

## 1. Declare

In the app or in a plugin crate:

```rust
use rux::prelude::*;

#[derive(NativeType)]
pub struct Battery {
    pub level: Option<i32>,
    pub state: BatteryState,
}

#[rux::host_api]
pub trait DeviceInfoApi {
    fn info(&self) -> Result<DeviceInfo, NativeError>; // sync
    #[rux::async_call]
    fn battery(&self) -> Result<Battery, NativeError>; // async
}
```

Supported types: `bool`, `i32`, `i64`, `f64`, `String`, `Option<T>`, `Vec<T>`, `()`, and
`#[derive(NativeType)]` structs (named fields) and enums (unit variants).

## 2. Implement

`rux build` generates a Kotlin interface and a Swift protocol from the declaration (in
`<android|ios>/rux-generated/`; Kotlin package `dev.rux.generated.<crate_name>`). Implement them:

```kotlin
class DeviceInfoApiImpl(private val context: Context) : DeviceInfoApi {
    override fun info() = DeviceInfo(platform = "Android", /* … */)
    override fun battery(callback: (Result<Battery>) -> Unit) {
        callback(Result.success(Battery(level = 42, state = BatteryState.CHARGING)))
    }
}
```

```swift
final class DeviceInfoApiImpl: DeviceInfoApi {
    func info() throws -> DeviceInfo { /* … */ }
    func battery(completion: @escaping (Result<Battery, Error>) -> Void) { /* … */ }
}
```

## 3. Register

Name the implementations in `Cargo.toml`. `rux build` generates a registrant that `RuxHost` runs at
startup (Android: constructed with the Activity as `Context`; iOS: `init()`):

```toml
[package.metadata.rux.native]
DeviceInfoApi = { android = "dev.rux.plugins.deviceinfo.DeviceInfoApiImpl", ios = "DeviceInfoApiImpl" }
```

## 4. Call

```rust
let info = DeviceInfoApi::new().info()?;                       // returns right away
DeviceInfoApi::new().battery(move |b| battery.set(Some(b)));   // callback on the UI thread
```

Rust, Kotlin and Swift all run on the UI thread, so a sync call is a plain nested call (no thread
hop).

## Plugins

A plugin is a crate with all of the above, plus where its Kotlin / Swift sources live:

```toml
[package.metadata.rux.plugin]
android = "android"
ios = "ios"
```

An app depends on the plugin crate, and `rux build` picks it up: its generated code, sources and
registrations go into the app's Android and iOS projects. See
[`examples/device-info`](../examples/device-info).

## Testing

In headless tests, fake the native side with `rux::native::testing::mock`; see
[Testing](testing.md#mocking-native-apis).
