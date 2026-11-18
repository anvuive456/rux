use rux::prelude::*;
use rux_device_info::{Battery, BatteryState};

use crate::state::DeviceState;

const MUTED: Color = Color::hex(0x666666);

/// Device and battery information, from native code (the `rux-device-info` plugin).
pub fn device_view(state: &DeviceState) -> Element {
    let device = match state.info() {
        Some(Ok(info)) => {
            let kind = if info.is_simulator {
                " (simulator)"
            } else {
                ""
            };
            format!(
                "{} · {} {}{kind}",
                info.model, info.platform, info.os_version
            )
        }
        Some(Err(e)) => format!("Device info unavailable: {}", e.message),
        None => "Device info: …".into(),
    };
    let battery = match state.battery() {
        _ if state.loading() => "Battery: reading…".into(),
        Some(Ok(battery)) => format!("Battery: {}", describe(&battery)),
        Some(Err(e)) => format!("Battery unavailable: {}", e.message),
        None => "Battery: not read yet".into(),
    };
    let s = state.clone();

    column([
        text(device).color(MUTED),
        text(battery).color(MUTED),
        button("Refresh battery", move || s.refresh_battery()),
    ])
    .spacing(8.0)
}

fn describe(battery: &Battery) -> String {
    let level = battery
        .level
        .map_or_else(|| "unknown level".to_owned(), |l| format!("{l}%"));
    let state = match battery.state {
        BatteryState::Unknown => "state unknown",
        BatteryState::Charging => "charging",
        BatteryState::Discharging => "on battery",
        BatteryState::Full => "full",
    };
    format!("{level}, {state}")
}
