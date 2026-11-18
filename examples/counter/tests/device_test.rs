//! The device view against a mocked native side (no emulator needed).

use counter::CounterApp;
use rux::native::{NativeError, ok_reply, testing};
use rux::testing::Harness;
use rux_device_info::{Battery, BatteryState, DeviceInfo, DeviceInfoApi};

#[test]
fn shows_native_device_info_and_battery() {
    let _native = testing::mock(|api, method, _args| {
        assert_eq!(api, DeviceInfoApi::API);
        match method {
            0 => Ok(ok_reply(&DeviceInfo {
                platform: "Android".into(),
                os_version: "16".into(),
                model: "Pixel 10".into(),
                is_simulator: true,
            })),
            1 => Ok(ok_reply(&Battery {
                level: Some(42),
                state: BatteryState::Charging,
            })),
            _ => Err(NativeError::new("not_implemented", "")),
        }
    });

    let mut h = Harness::new(CounterApp::new());
    assert!(h.dump().contains("Pixel 10 · Android 16 (simulator)"));
    assert!(h.dump().contains("Battery: not read yet"));
    h.click_text("Refresh battery");
    assert!(h.dump().contains("Battery: 42%, charging"), "{}", h.dump());
}

#[test]
fn without_native_code_the_view_reports_it() {
    let h = Harness::new(CounterApp::new());
    assert!(h.dump().contains("Device info unavailable"));
}
