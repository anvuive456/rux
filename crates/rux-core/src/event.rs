use std::rc::Rc;

/// Kinds of native events that can be sent to Rust.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EventKind {
    /// Tap (buttons, touchable views).
    Click,
    /// Value changed (text input). `Event::value` holds the new value.
    Change,
}

impl EventKind {
    /// Name used on the wire protocol between Rust and the native host.
    pub const fn as_str(self) -> &'static str {
        match self {
            EventKind::Click => "click",
            EventKind::Change => "change",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "click" => Some(EventKind::Click),
            "change" => Some(EventKind::Change),
            _ => None,
        }
    }
}

/// An event from the native host.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    pub value: Option<String>,
}

impl Event {
    pub fn new(kind: EventKind) -> Self {
        Self { kind, value: None }
    }

    pub fn with_value(kind: EventKind, value: impl Into<String>) -> Self {
        Self {
            kind,
            value: Some(value.into()),
        }
    }
}

/// Event handler callback. Runs only on the UI thread, so it uses `Rc` and needs no `Send`.
pub type Handler = Rc<dyn Fn(&Event)>;
