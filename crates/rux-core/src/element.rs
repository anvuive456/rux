use std::fmt;
use std::rc::Rc;

use crate::event::{Event, EventKind, Handler};

/// Root of a rux application: describes the UI as a function of state.
///
/// `view` is called again whenever a [`Signal`](../rux_state/struct.Signal.html) changes;
/// the reconciler only sends the difference down to native.
pub trait App {
    fn view(&self) -> Element;
}

/// Native node kinds. Each backend maps them to a matching widget:
///
/// | kind         | Android                  | iOS                    |
/// |--------------|--------------------------|------------------------|
/// | `View`       | `FrameLayout`            | `UIView`               |
/// | `Column`     | `LinearLayout(VERTICAL)` | `UIStackView(.vertical)` |
/// | `Row`        | `LinearLayout(HORIZONTAL)` | `UIStackView(.horizontal)` |
/// | `Scroll`     | `ScrollView`             | `UIScrollView`         |
/// | `Text`       | `TextView`               | `UILabel`              |
/// | `Button`     | `Button`                 | `UIButton`             |
/// | `TextInput`  | `EditText`               | `UITextField`          |
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ElementKind {
    View,
    Column,
    Row,
    Scroll,
    Text,
    Button,
    TextInput,
}

impl ElementKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            ElementKind::View => "view",
            ElementKind::Column => "column",
            ElementKind::Row => "row",
            ElementKind::Scroll => "scroll",
            ElementKind::Text => "text",
            ElementKind::Button => "button",
            ElementKind::TextInput => "text_input",
        }
    }
}

/// 32-bit ARGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color(pub u32);

impl Color {
    pub const TRANSPARENT: Color = Color(0x0000_0000);
    pub const BLACK: Color = Color(0xFF00_0000);
    pub const WHITE: Color = Color(0xFFFF_FFFF);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::argb(0xFF, r, g, b)
    }

    pub const fn argb(a: u8, r: u8, g: u8, b: u8) -> Self {
        Color((a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32)
    }

    /// `0xRRGGBB`, alpha = 255.
    pub const fn hex(rgb: u32) -> Self {
        Color(0xFF00_0000 | (rgb & 0x00FF_FFFF))
    }
}

/// A prop value. Encoded as a string when sent across FFI (see [`PropValue::to_wire`]).
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Str(String),
    Num(f32),
    Bool(bool),
    Color(Color),
}

impl PropValue {
    /// Encoding for the native host:
    /// - `Str`   → verbatim
    /// - `Num`   → decimal number (units: dp on Android, pt on iOS)
    /// - `Bool`  → `"true"` / `"false"`
    /// - `Color` → `"#AARRGGBB"`
    pub fn to_wire(&self) -> String {
        match self {
            PropValue::Str(s) => s.clone(),
            PropValue::Num(n) => n.to_string(),
            PropValue::Bool(b) => b.to_string(),
            PropValue::Color(c) => format!("#{:08X}", c.0),
        }
    }
}

impl From<&str> for PropValue {
    fn from(v: &str) -> Self {
        PropValue::Str(v.to_owned())
    }
}
impl From<String> for PropValue {
    fn from(v: String) -> Self {
        PropValue::Str(v)
    }
}
impl From<f32> for PropValue {
    fn from(v: f32) -> Self {
        PropValue::Num(v)
    }
}
impl From<bool> for PropValue {
    fn from(v: bool) -> Self {
        PropValue::Bool(v)
    }
}
impl From<Color> for PropValue {
    fn from(v: Color) -> Self {
        PropValue::Color(v)
    }
}

/// Standard prop names that every backend must support.
pub mod props {
    pub const TEXT: &str = "text";
    pub const VALUE: &str = "value";
    pub const PLACEHOLDER: &str = "placeholder";
    pub const FONT_SIZE: &str = "font_size";
    pub const COLOR: &str = "color";
    pub const BACKGROUND: &str = "background";
    pub const PADDING: &str = "padding";
    pub const SPACING: &str = "spacing";
    pub const ENABLED: &str = "enabled";
}

/// A node in the virtual UI tree. Cheap to create; rebuilt every time `App::view()` runs.
#[derive(Clone)]
pub struct Element {
    pub kind: ElementKind,
    pub key: Option<String>,
    pub props: Vec<(&'static str, PropValue)>,
    pub handlers: Vec<(EventKind, Handler)>,
    pub children: Vec<Element>,
}

impl Element {
    pub fn new(kind: ElementKind) -> Self {
        Self {
            kind,
            key: None,
            props: Vec::new(),
            handlers: Vec::new(),
            children: Vec::new(),
        }
    }

    /// A key lets the reconciler identify the node when a child list is reordered.
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Sets a prop (overwrites an existing one).
    pub fn prop(mut self, name: &'static str, value: impl Into<PropValue>) -> Self {
        let value = value.into();
        match self.props.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = value,
            None => self.props.push((name, value)),
        }
        self
    }

    /// Attaches an event handler (overwrites an existing one).
    pub fn on(mut self, kind: EventKind, f: impl Fn(&Event) + 'static) -> Self {
        let h: Handler = Rc::new(f);
        match self.handlers.iter_mut().find(|(k, _)| *k == kind) {
            Some(slot) => slot.1 = h,
            None => self.handlers.push((kind, h)),
        }
        self
    }

    pub fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }

    // --- style helpers -----------------------------------------------------

    pub fn padding(self, v: f32) -> Self {
        self.prop(props::PADDING, v)
    }
    pub fn spacing(self, v: f32) -> Self {
        self.prop(props::SPACING, v)
    }
    pub fn background(self, c: Color) -> Self {
        self.prop(props::BACKGROUND, c)
    }
    pub fn color(self, c: Color) -> Self {
        self.prop(props::COLOR, c)
    }
    pub fn font_size(self, v: f32) -> Self {
        self.prop(props::FONT_SIZE, v)
    }
    pub fn enabled(self, v: bool) -> Self {
        self.prop(props::ENABLED, v)
    }
    pub fn placeholder(self, v: impl Into<String>) -> Self {
        self.prop(props::PLACEHOLDER, v.into())
    }
}

impl fmt::Debug for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Element")
            .field("kind", &self.kind)
            .field("key", &self.key)
            .field("props", &self.props)
            .field(
                "handlers",
                &self.handlers.iter().map(|(k, _)| k).collect::<Vec<_>>(),
            )
            .field("children", &self.children)
            .finish()
    }
}
