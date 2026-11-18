//! Views: the leaves of the tree — text, buttons, inputs.

use rux_core::{Element, ElementKind, EventKind, props};

/// A piece of text.
pub fn text(content: impl Into<String>) -> Element {
    Element::new(ElementKind::Text).prop(props::TEXT, content.into())
}

/// A button with a label; `on_click` runs when it is tapped.
pub fn button(label: impl Into<String>, on_click: impl Fn() + 'static) -> Element {
    Element::new(ElementKind::Button)
        .prop(props::TEXT, label.into())
        .on(EventKind::Click, move |_| on_click())
}

/// Controlled text input: `value` is the source of truth, `on_change` receives the new value.
pub fn text_input(value: impl Into<String>, on_change: impl Fn(String) + 'static) -> Element {
    Element::new(ElementKind::TextInput)
        .prop(props::VALUE, value.into())
        .on(EventKind::Change, move |e| {
            on_change(e.value.clone().unwrap_or_default())
        })
}
