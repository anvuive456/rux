use rux::prelude::*;

use crate::state::ProfileState;

const ACCENT: Color = Color::hex(0x3366FF);

/// Name input and greeting.
pub fn greeting_view(state: &ProfileState) -> Element {
    let s = state.clone();

    column([
        text_input(state.name(), move |v| s.set_name(v)).placeholder("Your name"),
        text(state.greeting()).color(ACCENT),
    ])
    .spacing(8.0)
}
