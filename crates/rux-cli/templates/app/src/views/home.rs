use rux::prelude::*;

use crate::state::CounterState;

/// Home screen: a counter with increment / decrement buttons.
pub fn home(counter: &CounterState) -> Element {
    let (dec, inc) = (counter.clone(), counter.clone());

    column([
        text("Welcome to {{display_name}}!").font_size(22.0),
        text(format!("Count: {}", counter.count())).font_size(28.0),
        row([
            button("-", move || dec.decrement()),
            button("+", move || inc.increment()),
        ])
        .spacing(8.0),
    ])
    .padding(16.0)
    .spacing(16.0)
}
