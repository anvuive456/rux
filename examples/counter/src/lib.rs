//! Counter — the app entry point (like `main.dart` in Flutter).
//!
//! - `state/`: app data, one file per piece of state.
//! - `views/`: UI, one file per view.
//!
//! The native shells live in `android/` and `ios/`.

pub mod state;
pub mod views;

use rux::prelude::*;

use state::{CounterState, DeviceState, ProfileState};

/// Root app: owns the state and renders the home screen.
#[derive(Default)]
pub struct CounterApp {
    counter: CounterState,
    profile: ProfileState,
    device: DeviceState,
}

impl CounterApp {
    pub fn new() -> Self {
        let app = Self::default();
        app.device.load_info();
        app
    }
}

impl App for CounterApp {
    fn view(&self) -> Element {
        views::home(&self.counter, &self.profile, &self.device)
    }
}

// Generates the JNI (Android) and C (iOS) entry points.
rux::main!(CounterApp::new);
