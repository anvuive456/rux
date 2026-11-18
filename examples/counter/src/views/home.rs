use rux::prelude::*;

use super::{counter_view, device_view, greeting_view};
use crate::state::{CounterState, DeviceState, ProfileState};

/// Home screen: composes the child views.
pub fn home(counter: &CounterState, profile: &ProfileState, device: &DeviceState) -> Element {
    column([
        counter_view(counter),
        greeting_view(profile),
        device_view(device),
    ])
    .padding(16.0)
    .spacing(24.0)
}
