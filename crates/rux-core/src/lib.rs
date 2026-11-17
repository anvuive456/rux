//! The platform-independent core of rux.
//!
//! Data flow:
//!
//! ```text
//! App::view() ──► Element (virtual) ──► Reconciler::render() ──► Vec<Mutation> ──► Renderer (native)
//!                                                                                      │
//! Signal::set() ◄── handler ◄── Reconciler::dispatch(NodeId, Event) ◄──────────────────┘
//! ```

mod element;
mod event;
mod mutation;
mod reconciler;

pub use element::{App, Color, Element, ElementKind, PropValue, props};
pub use event::{Event, EventKind, Handler};
pub use mutation::{Mutation, NodeId};
pub use reconciler::Reconciler;
