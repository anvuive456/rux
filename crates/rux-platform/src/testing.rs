//! Headless renderer: builds an in-memory tree instead of real widgets.
//!
//! Used to unit-test UI and to verify the [`Mutation`] contract on desktop,
//! without an emulator.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use rux_core::{App, ElementKind, Event, EventKind, Mutation, NodeId};

use crate::{Renderer, Runtime};

#[derive(Debug)]
pub struct HeadlessNode {
    pub kind: ElementKind,
    pub props: BTreeMap<&'static str, String>,
    pub listeners: BTreeSet<EventKind>,
    pub children: Vec<NodeId>,
    pub parent: Option<NodeId>,
}

#[derive(Debug, Default)]
pub struct HeadlessRenderer {
    pub nodes: HashMap<NodeId, HeadlessNode>,
    pub root: Option<NodeId>,
    /// Total number of mutations applied — useful for checking that diffs are minimal.
    pub applied: usize,
}

impl HeadlessRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    fn detach(&mut self, child: NodeId) {
        let parent = self.nodes.get_mut(&child).and_then(|n| n.parent.take());
        if let Some(p) = parent.and_then(|p| self.nodes.get_mut(&p)) {
            p.children.retain(|c| *c != child);
        }
    }

    /// Finds the first node (depth-first) whose `text` prop equals `text`.
    pub fn find_by_text(&self, text: &str) -> Option<NodeId> {
        self.find(|n| n.props.get("text").is_some_and(|t| t == text))
    }

    pub fn find_by_kind(&self, kind: ElementKind) -> Option<NodeId> {
        self.find(|n| n.kind == kind)
    }

    pub fn find(&self, pred: impl Fn(&HeadlessNode) -> bool) -> Option<NodeId> {
        fn walk(
            r: &HeadlessRenderer,
            id: NodeId,
            pred: &dyn Fn(&HeadlessNode) -> bool,
        ) -> Option<NodeId> {
            let node = r.nodes.get(&id)?;
            if pred(node) {
                return Some(id);
            }
            node.children.iter().find_map(|c| walk(r, *c, pred))
        }
        walk(self, self.root?, &pred)
    }

    /// Prints the current tree as text, handy for snapshot tests.
    pub fn dump(&self) -> String {
        fn walk(r: &HeadlessRenderer, id: NodeId, depth: usize, out: &mut String) {
            let Some(n) = r.nodes.get(&id) else { return };
            let _ = write!(out, "{:indent$}{}", "", n.kind.as_str(), indent = depth * 2);
            for (k, v) in &n.props {
                let _ = write!(out, " {k}={v:?}");
            }
            out.push('\n');
            for c in &n.children {
                walk(r, *c, depth + 1, out);
            }
        }
        let mut out = String::new();
        if let Some(root) = self.root {
            walk(self, root, 0, &mut out);
        }
        out
    }
}

impl Renderer for HeadlessRenderer {
    fn apply(&mut self, mutations: &[Mutation]) {
        self.applied += mutations.len();
        for m in mutations {
            match m {
                Mutation::Create { id, kind } => {
                    let node = HeadlessNode {
                        kind: *kind,
                        props: BTreeMap::new(),
                        listeners: BTreeSet::new(),
                        children: Vec::new(),
                        parent: None,
                    };
                    assert!(
                        self.nodes.insert(*id, node).is_none(),
                        "duplicate node {id}"
                    );
                }
                Mutation::SetProp { id, name, value } => {
                    self.nodes
                        .get_mut(id)
                        .expect("SetProp on unknown node")
                        .props
                        .insert(name, value.to_wire());
                }
                Mutation::RemoveProp { id, name } => {
                    self.nodes
                        .get_mut(id)
                        .expect("RemoveProp on unknown node")
                        .props
                        .remove(name);
                }
                Mutation::InsertChild {
                    parent,
                    child,
                    index,
                } => {
                    self.detach(*child);
                    self.nodes
                        .get_mut(child)
                        .expect("InsertChild of unknown node")
                        .parent = Some(*parent);
                    let p = self
                        .nodes
                        .get_mut(parent)
                        .expect("InsertChild into unknown node");
                    let index = (*index).min(p.children.len());
                    p.children.insert(index, *child);
                }
                Mutation::RemoveChild { parent, child } => {
                    assert_eq!(self.nodes.get(child).and_then(|n| n.parent), Some(*parent));
                    self.detach(*child);
                }
                Mutation::Release { id } => {
                    self.nodes.remove(id).expect("Release of unknown node");
                }
                Mutation::SetRoot { id } => {
                    assert!(self.nodes.contains_key(id), "SetRoot to unknown node");
                    self.root = Some(*id);
                }
                Mutation::Listen { id, event } => {
                    self.nodes
                        .get_mut(id)
                        .expect("Listen on unknown node")
                        .listeners
                        .insert(*event);
                }
                Mutation::Unlisten { id, event } => {
                    self.nodes
                        .get_mut(id)
                        .expect("Unlisten on unknown node")
                        .listeners
                        .remove(event);
                }
            }
        }
    }
}

/// Runs an [`App`] headless and simulates user interaction.
///
/// ```ignore
/// let mut h = Harness::new(Counter::new());
/// h.click_text("+");
/// assert!(h.dump().contains("Count: 1"));
/// ```
pub struct Harness {
    pub runtime: Runtime,
    pub renderer: HeadlessRenderer,
}

impl Harness {
    pub fn new(app: impl App + 'static) -> Self {
        let mut runtime = Runtime::new(Box::new(app));
        let mut renderer = HeadlessRenderer::new();
        runtime.render(&mut renderer);
        Self { runtime, renderer }
    }

    pub fn dispatch(&mut self, id: NodeId, event: Event) {
        self.runtime.dispatch(id, &event, &mut self.renderer);
    }

    /// Taps the node with the matching `text`. Panics if none is found.
    pub fn click_text(&mut self, text: &str) {
        let id = self
            .renderer
            .find_by_text(text)
            .unwrap_or_else(|| panic!("no node with text {text:?}"));
        self.dispatch(id, Event::new(EventKind::Click));
    }

    /// Types a value into the first text input.
    pub fn type_text(&mut self, value: &str) {
        let id = self
            .renderer
            .find_by_kind(ElementKind::TextInput)
            .expect("no text_input in tree");
        self.dispatch(id, Event::with_value(EventKind::Change, value));
    }

    pub fn dump(&self) -> String {
        self.renderer.dump()
    }
}
