use std::collections::HashMap;

use crate::element::{Element, ElementKind, PropValue};
use crate::event::{Event, EventKind, Handler};
use crate::mutation::{Mutation, NodeId};

/// Record of a node that has been mounted on the host.
struct Mounted {
    id: NodeId,
    kind: ElementKind,
    key: Option<String>,
    props: Vec<(&'static str, PropValue)>,
    events: Vec<EventKind>,
    children: Vec<Mounted>,
}

/// Keeps the mounted tree and computes the difference between renders.
#[derive(Default)]
pub struct Reconciler {
    root: Option<Mounted>,
    next_id: u64,
    handlers: HashMap<(NodeId, EventKind), Handler>,
}

impl Reconciler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Renders a new tree and returns the mutations to apply on the host.
    pub fn render(&mut self, element: Element) -> Vec<Mutation> {
        let mut out = Vec::new();
        let root = match self.root.take() {
            Some(old) if same_node(&old, &element) => self.patch(old, element, &mut out),
            old => {
                let new = self.mount(element, &mut out);
                out.push(Mutation::SetRoot { id: new.id });
                if let Some(old) = old {
                    self.unmount(old, &mut out);
                }
                new
            }
        };
        self.root = Some(root);
        out
    }

    /// Calls the node's handler. Returns `false` if the node does not listen for this event.
    pub fn dispatch(&self, id: NodeId, event: &Event) -> bool {
        match self.handlers.get(&(id, event.kind)).cloned() {
            Some(h) => {
                h(event);
                true
            }
            None => false,
        }
    }

    pub fn root_id(&self) -> Option<NodeId> {
        self.root.as_ref().map(|r| r.id)
    }

    fn alloc(&mut self) -> NodeId {
        self.next_id += 1;
        NodeId(self.next_id)
    }

    fn mount(&mut self, el: Element, out: &mut Vec<Mutation>) -> Mounted {
        let id = self.alloc();
        out.push(Mutation::Create { id, kind: el.kind });
        for (name, value) in &el.props {
            out.push(Mutation::SetProp {
                id,
                name,
                value: value.clone(),
            });
        }
        let mut events = Vec::with_capacity(el.handlers.len());
        for (kind, h) in el.handlers {
            self.handlers.insert((id, kind), h);
            events.push(kind);
            out.push(Mutation::Listen { id, event: kind });
        }
        let mut children = Vec::with_capacity(el.children.len());
        for (index, child) in el.children.into_iter().enumerate() {
            let m = self.mount(child, out);
            out.push(Mutation::InsertChild {
                parent: id,
                child: m.id,
                index,
            });
            children.push(m);
        }
        Mounted {
            id,
            kind: el.kind,
            key: el.key,
            props: el.props,
            events,
            children,
        }
    }

    fn unmount(&mut self, m: Mounted, out: &mut Vec<Mutation>) {
        for c in m.children {
            self.unmount(c, out);
        }
        for e in m.events {
            self.handlers.remove(&(m.id, e));
        }
        out.push(Mutation::Release { id: m.id });
    }

    fn patch(&mut self, mut old: Mounted, el: Element, out: &mut Vec<Mutation>) -> Mounted {
        let id = old.id;

        // Props
        for (name, value) in &el.props {
            let unchanged = old.props.iter().any(|(n, v)| n == name && v == value);
            if !unchanged {
                out.push(Mutation::SetProp {
                    id,
                    name,
                    value: value.clone(),
                });
            }
        }
        for (name, _) in &old.props {
            if !el.props.iter().any(|(n, _)| n == name) {
                out.push(Mutation::RemoveProp { id, name });
            }
        }
        old.props = el.props;

        // Handlers: closures are always replaced (they capture fresh state); the host is only told when the set of events changes.
        let mut events = Vec::with_capacity(el.handlers.len());
        for (kind, h) in el.handlers {
            if !old.events.contains(&kind) {
                out.push(Mutation::Listen { id, event: kind });
            }
            self.handlers.insert((id, kind), h);
            events.push(kind);
        }
        for kind in &old.events {
            if !events.contains(kind) {
                self.handlers.remove(&(id, *kind));
                out.push(Mutation::Unlisten { id, event: *kind });
            }
        }
        old.events = events;

        old.children = self.patch_children(id, old.children, el.children, out);
        old
    }

    fn patch_children(
        &mut self,
        parent: NodeId,
        old: Vec<Mounted>,
        new: Vec<Element>,
        out: &mut Vec<Mutation>,
    ) -> Vec<Mounted> {
        // Current order on the host.
        let mut current: Vec<NodeId> = old.iter().map(|m| m.id).collect();
        let mut old: Vec<Option<Mounted>> = old.into_iter().map(Some).collect();

        let key_index: HashMap<String, usize> = old
            .iter()
            .enumerate()
            .filter_map(|(i, m)| Some((m.as_ref()?.key.clone()?, i)))
            .collect();

        // 1. Match each new child to an old one (by key, or by position when unkeyed).
        let mut result = Vec::with_capacity(new.len());
        let mut pending_mount = Vec::new();
        for (i, el) in new.into_iter().enumerate() {
            let candidate = match &el.key {
                Some(k) => key_index.get(k).copied(),
                None => Some(i).filter(|&i| {
                    old.get(i)
                        .and_then(|m| m.as_ref())
                        .is_some_and(|m| m.key.is_none())
                }),
            };
            let matched = candidate
                .filter(|&j| old[j].as_ref().is_some_and(|m| same_node(m, &el)))
                .and_then(|j| old[j].take());
            match matched {
                Some(m) => result.push(Some(self.patch(m, el, out))),
                None => {
                    result.push(None);
                    pending_mount.push((i, el));
                }
            }
        }

        // 2. Remove old children that are no longer used.
        for m in old.into_iter().flatten() {
            current.retain(|id| *id != m.id);
            out.push(Mutation::RemoveChild {
                parent,
                child: m.id,
            });
            self.unmount(m, out);
        }

        // 3. Mount new children.
        for (i, el) in pending_mount {
            result[i] = Some(self.mount(el, out));
        }
        let result: Vec<Mounted> = result
            .into_iter()
            .map(|m| m.expect("every slot filled"))
            .collect();

        // 4. Reorder to match the new order, emitting InsertChild only where positions differ.
        for (index, m) in result.iter().enumerate() {
            if current.get(index) == Some(&m.id) {
                continue;
            }
            if let Some(pos) = current.iter().position(|id| *id == m.id) {
                current.remove(pos);
            }
            current.insert(index, m.id);
            out.push(Mutation::InsertChild {
                parent,
                child: m.id,
                index,
            });
        }

        result
    }
}

fn same_node(old: &Mounted, el: &Element) -> bool {
    old.kind == el.kind && old.key == el.key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::props;

    fn text(s: &str) -> Element {
        Element::new(ElementKind::Text).prop(props::TEXT, s)
    }

    fn column(children: impl IntoIterator<Item = Element>) -> Element {
        Element::new(ElementKind::Column).children(children)
    }

    #[test]
    fn initial_render_creates_tree_and_sets_root() {
        let mut r = Reconciler::new();
        let muts = r.render(column([text("a")]));
        assert_eq!(
            muts,
            vec![
                Mutation::Create {
                    id: NodeId(1),
                    kind: ElementKind::Column
                },
                Mutation::Create {
                    id: NodeId(2),
                    kind: ElementKind::Text
                },
                Mutation::SetProp {
                    id: NodeId(2),
                    name: "text",
                    value: "a".into()
                },
                Mutation::InsertChild {
                    parent: NodeId(1),
                    child: NodeId(2),
                    index: 0
                },
                Mutation::SetRoot { id: NodeId(1) },
            ]
        );
    }

    #[test]
    fn rerender_only_emits_changed_props() {
        let mut r = Reconciler::new();
        r.render(column([text("a"), text("b")]));
        let muts = r.render(column([text("a"), text("c")]));
        assert_eq!(
            muts,
            vec![Mutation::SetProp {
                id: NodeId(3),
                name: "text",
                value: "c".into()
            }]
        );
    }

    #[test]
    fn identical_render_is_noop() {
        let mut r = Reconciler::new();
        r.render(column([text("a")]));
        assert!(r.render(column([text("a")])).is_empty());
    }

    #[test]
    fn removed_children_are_detached_and_released() {
        let mut r = Reconciler::new();
        r.render(column([text("a"), text("b")]));
        let muts = r.render(column([text("a")]));
        assert_eq!(
            muts,
            vec![
                Mutation::RemoveChild {
                    parent: NodeId(1),
                    child: NodeId(3)
                },
                Mutation::Release { id: NodeId(3) },
            ]
        );
    }

    #[test]
    fn keyed_children_are_moved_not_recreated() {
        let mut r = Reconciler::new();
        r.render(column([
            text("a").key("a"),
            text("b").key("b"),
            text("c").key("c"),
        ]));
        let muts = r.render(column([
            text("c").key("c"),
            text("a").key("a"),
            text("b").key("b"),
        ]));
        assert_eq!(
            muts,
            vec![Mutation::InsertChild {
                parent: NodeId(1),
                child: NodeId(4),
                index: 0
            }]
        );
    }

    #[test]
    fn kind_change_replaces_node() {
        let mut r = Reconciler::new();
        r.render(column([text("a")]));
        let muts = r.render(column([Element::new(ElementKind::Button)]));
        assert_eq!(
            muts,
            vec![
                Mutation::RemoveChild {
                    parent: NodeId(1),
                    child: NodeId(2)
                },
                Mutation::Release { id: NodeId(2) },
                Mutation::Create {
                    id: NodeId(3),
                    kind: ElementKind::Button
                },
                Mutation::InsertChild {
                    parent: NodeId(1),
                    child: NodeId(3),
                    index: 0
                },
            ]
        );
    }

    #[test]
    fn dispatch_calls_latest_handler() {
        use std::cell::Cell;
        use std::rc::Rc;

        let hits = Rc::new(Cell::new(0));
        let mut r = Reconciler::new();
        let make = |n: i32| {
            let hits = hits.clone();
            Element::new(ElementKind::Button)
                .on(EventKind::Click, move |_| hits.set(hits.get() + n))
        };
        r.render(make(1));
        r.render(make(10));
        assert!(r.dispatch(NodeId(1), &Event::new(EventKind::Click)));
        assert_eq!(hits.get(), 10);
        assert!(!r.dispatch(NodeId(1), &Event::new(EventKind::Change)));
    }
}
