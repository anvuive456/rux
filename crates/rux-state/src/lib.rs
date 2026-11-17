//! Reactive state for rux.
//!
//! The current model is simple: any [`Signal`] change sets the UI thread's "dirty" flag;
//! the runtime checks the flag after each event and re-renders the whole `App::view()`.
//! The reconciler ensures only the difference is sent to native.
//!
//! TODO: fine-grained dependency tracking and updates from other threads.

use std::cell::{Cell, Ref, RefCell};
use std::fmt;
use std::rc::Rc;

pub mod hot;

thread_local! {
    static DIRTY: Cell<bool> = const { Cell::new(false) };
}

/// Marks the UI as needing a re-render.
pub fn mark_dirty() {
    DIRTY.with(|d| d.set(true));
}

/// Reads and clears the dirty flag.
pub fn take_dirty() -> bool {
    DIRTY.with(|d| d.replace(false))
}

/// A shared value cell; changing it triggers a re-render.
///
/// Cheap to clone (just a refcount bump) — clone it into handler closures to update it.
pub struct Signal<T>(Rc<RefCell<T>>);

impl<T> Signal<T> {
    pub fn new(value: T) -> Self {
        Self(hot::new_cell(value))
    }

    /// Borrows the value for reading.
    pub fn borrow(&self) -> Ref<'_, T> {
        self.0.borrow()
    }

    /// Reads through a closure.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.0.borrow())
    }

    /// Replaces the value.
    pub fn set(&self, value: T) {
        *self.0.borrow_mut() = value;
        mark_dirty();
    }

    /// Modifies the value in place.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        f(&mut self.0.borrow_mut());
        mark_dirty();
    }
}

impl<T: Clone> Signal<T> {
    pub fn get(&self) -> T {
        self.0.borrow().clone()
    }
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Default> Default for Signal<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: fmt::Debug> fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Signal").field(&*self.0.borrow()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_marks_dirty() {
        take_dirty();
        let s = Signal::new(1);
        assert!(!take_dirty());
        s.clone().update(|v| *v += 1);
        assert_eq!(s.get(), 2);
        assert!(take_dirty());
        assert!(!take_dirty());
    }
}
