use rux::prelude::*;

use crate::state::CounterState;

/// Shows the count with increment/decrement buttons.
pub fn counter_view(state: &CounterState) -> Element {
    let (dec, inc) = (state.clone(), state.clone());

    column([
        text(format!("Count: {}", state.count())).font_size(28.0),
        row([
            button("-", move || dec.decrement()),
            button("+", move || inc.increment()),
        ])
        .spacing(8.0),
    ])
    .spacing(8.0)
}
