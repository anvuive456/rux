use std::fmt;

use crate::element::{ElementKind, PropValue};
use crate::event::EventKind;

/// Identifier of a native node. Allocated by Rust; the host only uses it as a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// A change to the native tree — this is the entire "protocol" a backend must implement.
///
/// Semantics the host must follow:
/// - `InsertChild`: if `child` is already a child of `parent`, detach it first, then insert at `index`
///   (this is also how nodes are moved).
/// - `RemoveChild` only detaches the node from its parent; descendants stay attached to it until `Release`.
/// - `Release`: Rust no longer references the node; the host may free it.
#[derive(Clone, Debug, PartialEq)]
pub enum Mutation {
    Create {
        id: NodeId,
        kind: ElementKind,
    },
    SetProp {
        id: NodeId,
        name: &'static str,
        value: PropValue,
    },
    RemoveProp {
        id: NodeId,
        name: &'static str,
    },
    InsertChild {
        parent: NodeId,
        child: NodeId,
        index: usize,
    },
    RemoveChild {
        parent: NodeId,
        child: NodeId,
    },
    Release {
        id: NodeId,
    },
    SetRoot {
        id: NodeId,
    },
    Listen {
        id: NodeId,
        event: EventKind,
    },
    Unlisten {
        id: NodeId,
        event: EventKind,
    },
}
