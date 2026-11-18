//! Example rux plugin: device information and battery state, implemented natively.
//!
//! A plugin is a crate with:
//! - the API, declared once in Rust (below) — `#[rux::host_api]` turns the trait into a typed
//!   client, and `rux build` generates the matching Kotlin interface / Swift protocol;
//! - its implementations: `android/…/DeviceInfoApiImpl.kt` and `ios/DeviceInfoApiImpl.swift`;
//! - `[package.metadata.rux.native]` / `[package.metadata.rux.plugin]` in `Cargo.toml`, which
//!   tell `rux build` which classes implement the API and where their sources are.
//!
//! An app just depends on the crate; `rux build` wires the implementations up at startup.
//!
//! ```ignore
//! let info = DeviceInfoApi::new().info()?;            // sync
//! DeviceInfoApi::new().battery(|battery| { … });     // async, callback on the UI thread
//! ```

use rux::NativeType;
use rux::native::NativeError;

#[derive(NativeType, Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    /// `Android` / `iOS`.
    pub platform: String,
    pub os_version: String,
    pub model: String,
    /// Emulator / simulator.
    pub is_simulator: bool,
}

#[derive(NativeType, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryState {
    Unknown,
    Charging,
    Discharging,
    Full,
}

#[derive(NativeType, Debug, Clone, PartialEq)]
pub struct Battery {
    /// 0–100; `None` when unknown (e.g. on the iOS simulator).
    pub level: Option<i32>,
    pub state: BatteryState,
}

/// Device information, implemented in Kotlin / Swift.
#[rux::host_api]
pub trait DeviceInfoApi {
    /// Device and OS (answered synchronously).
    fn info(&self) -> Result<DeviceInfo, NativeError>;

    /// Battery level and state (answered asynchronously).
    #[rux::async_call]
    fn battery(&self) -> Result<Battery, NativeError>;
}
