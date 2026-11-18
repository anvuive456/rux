//! Built-in UI elements — the everyday API for app authors.
//!
//! - [`layout`]: containers that arrange children (`column`, `row`, `view`, `scroll`).
//! - [`views`]: leaves (`text`, `button`, `text_input`).
//!
//! Every function returns an [`Element`](rux_core::Element); chain styles with methods on
//! `Element` (`.padding()`, `.spacing()`, `.background()`, `.color()`, ...).

pub mod layout;
pub mod views;

pub use layout::{column, row, scroll, view};
pub use views::{button, text, text_input};
