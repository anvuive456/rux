//! {{display_name}} — the app entry point (like `main.dart` in Flutter).
//!
//! - `state/`: app data, one file per piece of state.
//! - `views/`: UI, one file per view.
//!
//! The native shells live in `android/` and `ios/`.

pub mod state;
pub mod views;

use rux::prelude::*;

use state::CounterState;

/// Root app: owns the state and renders the home screen.
#[derive(Default)]
pub struct {{project_name}}App {
    counter: CounterState,
}

impl {{project_name}}App {
    pub fn new() -> Self {
        Self::default()
    }
}

impl App for {{project_name}}App {
    fn view(&self) -> Element {
        views::home(&self.counter)
    }
}

// Generates the JNI (Android) and C (iOS) entry points.
rux::main!({{project_name}}App::new);
