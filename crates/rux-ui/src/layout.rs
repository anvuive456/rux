//! Layouts: containers that arrange their children.

use rux_core::{Element, ElementKind};

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
