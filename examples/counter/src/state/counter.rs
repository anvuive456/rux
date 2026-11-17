use rux::Signal;

/// The counter value. Cheap to clone: clones share the same data.
#[derive(Clone, Default)]
pub struct CounterState {
    count: Signal<i32>,
}

impl CounterState {
    pub fn count(&self) -> i32 {
        self.count.get()
    }

    pub fn increment(&self) {
        self.count.update(|c| *c += 1);
    }

    pub fn decrement(&self) {
        self.count.update(|c| *c -= 1);
    }
}
