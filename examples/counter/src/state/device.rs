use rux::Signal;
use rux::native::NativeError;
use rux_device_info::{Battery, DeviceInfo, DeviceInfoApi};

/// Device information from the `rux-device-info` plugin (native code).
#[derive(Clone, Default)]
pub struct DeviceState {
    info: Signal<Option<Result<DeviceInfo, NativeError>>>,
    battery: Signal<Option<Result<Battery, NativeError>>>,
    loading: Signal<bool>,
}

impl DeviceState {
    /// Sync native call.
    pub fn load_info(&self) {
        self.info.set(Some(DeviceInfoApi::new().info()));
    }

    /// Async native call: the result arrives later, on the UI thread.
    pub fn refresh_battery(&self) {
        self.loading.set(true);
        let (battery, loading) = (self.battery.clone(), self.loading.clone());
        DeviceInfoApi::new().battery(move |result| {
            battery.set(Some(result));
            loading.set(false);
        });
    }

    pub fn info(&self) -> Option<Result<DeviceInfo, NativeError>> {
        self.info.get()
    }

    pub fn battery(&self) -> Option<Result<Battery, NativeError>> {
        self.battery.get()
    }

    pub fn loading(&self) -> bool {
        self.loading.get()
    }
}
