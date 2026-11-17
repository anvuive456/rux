//! Built-in widgets — the everyday API for app authors.
//!
//! Every function returns an [`Element`]; chain styles with methods on `Element`
//! (`.padding()`, `.spacing()`, `.background()`, ...).

use rux_core::{Element, ElementKind, EventKind, props};

/// Container that stacks its children vertically.
pub fn column(children: impl IntoIterator<Item = Element>) -> Element {
    Element::new(ElementKind::Column).children(children)
}

/// Container that lays out its children horizontally.
pub fn row(children: impl IntoIterator<Item = Element>) -> Element {
    Element::new(ElementKind::Row).children(children)
}

/// Plain container.
pub fn view(children: impl IntoIterator<Item = Element>) -> Element {
    Element::new(ElementKind::View).children(children)
}

/// Vertically scrolling area with a single child.
pub fn scroll(child: Element) -> Element {
    Element::new(ElementKind::Scroll).child(child)
}

pub fn text(content: impl Into<String>) -> Element {
    Element::new(ElementKind::Text).prop(props::TEXT, content.into())
}

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
