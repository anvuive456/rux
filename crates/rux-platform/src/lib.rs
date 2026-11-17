//! The layer between the rux core and the platform backends.
//!
//! A new backend (Android, iOS, later macOS/Windows/Web...) only needs to:
//! 1. Implement [`Renderer`] — apply a list of [`Mutation`]s to native widgets.
//! 2. Call [`host::install`] on app start and [`host::dispatch`] on native events.

pub mod dev;
pub mod host;
pub mod testing;

use rux_core::{App, Event, Mutation, NodeId, Reconciler};

/// Applies mutations to the native widget tree. Always called on the UI thread.
pub trait Renderer {
    fn apply(&mut self, mutations: &[Mutation]);
}

/// An app's lifecycle: owns the `App` and reconciler, and re-renders when state changes.
pub struct Runtime {
    app: Box<dyn App>,
    reconciler: Reconciler,
}

impl Runtime {
    pub fn new(app: Box<dyn App>) -> Self {
        Self {
            app,
            reconciler: Reconciler::new(),
        }
    }

    /// Renders (or re-renders) the whole app.
    pub fn render(&mut self, renderer: &mut dyn Renderer) {
        rux_state::take_dirty();
        let mutations = self.reconciler.render(self.app.view());
        if !mutations.is_empty() {
            renderer.apply(&mutations);
        }
    }

    /// Releases every native node (before handing over to a hot-reloaded build).
    pub fn teardown(&mut self, renderer: &mut dyn Renderer) {
        let mutations = self.reconciler.clear();
        if !mutations.is_empty() {
            renderer.apply(&mutations);
        }
    }

    /// Routes a native event to its handler; re-renders if state changed.
    pub fn dispatch(&mut self, id: NodeId, event: &Event, renderer: &mut dyn Renderer) {
        if self.reconciler.dispatch(id, event) && rux_state::take_dirty() {
            self.render(renderer);
        }
    }
}
