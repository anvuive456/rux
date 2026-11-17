use rux::prelude::*;

use super::{counter_view, greeting_view};
use crate::state::{CounterState, ProfileState};

/// Home screen: composes the child views.
pub fn home(counter: &CounterState, profile: &ProfileState) -> Element {
    column([counter_view(counter), greeting_view(profile)])
        .padding(16.0)
        .spacing(24.0)
}
