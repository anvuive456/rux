//! Hot reload support: carrying [`Signal`](crate::Signal) values over to a newer build of the
//! app library loaded into the same process.
//!
//! While the app is constructed (`begin` … `end`), every `Signal::new` is recorded in order.
//! On hot reload the new build constructs its app again; the n-th `Signal::new` *adopts* the
//! n-th signal of the previous build — the very same `Rc<RefCell<T>>` allocation — instead of
//! its initial value, provided the value type matches (`type_name`, size, align). Sharing the
//! allocation (rather than moving values) keeps signals nested in other signals consistent.
//!
//! If anything doesn't match (a signal was added/removed/retyped), the session reports why and
//! the caller rebuilds the app from scratch (hot restart).
//!
//! Previous builds are never unloaded, so pointers into their code/statics stay valid.

use std::any::type_name;
use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::{align_of, size_of};
use std::rc::Rc;

/// A signal handed from one build of the app to the next. FFI-safe (`#[repr(C)]`) because it
/// crosses between separately compiled copies of this crate.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct HotSignal {
    type_name: *const u8,
    type_name_len: usize,
    size: usize,
    align: usize,
    /// `Rc::into_raw` of the signal's `Rc<RefCell<T>>`; this entry owns one strong count.
    ptr: *const c_void,
}

impl HotSignal {
    fn of<T>(rc: &Rc<RefCell<T>>) -> Self {
        let name = type_name::<T>();
        Self {
            type_name: name.as_ptr(),
            type_name_len: name.len(),
            size: size_of::<T>(),
            align: align_of::<T>(),
            ptr: Rc::into_raw(rc.clone()).cast(),
        }
    }

    /// The value's type name.
    pub fn type_name(&self) -> &str {
        // SAFETY: points into the `&'static str` of a build that is never unloaded.
        unsafe {
            std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                self.type_name,
                self.type_name_len,
            ))
        }
    }

    fn matches<T>(&self) -> bool {
        self.type_name() == type_name::<T>()
            && self.size == size_of::<T>()
            && self.align == align_of::<T>()
    }

    /// Takes a new strong reference to the shared cell.
    ///
    /// # Safety
    /// `self.matches::<T>()` must hold.
    unsafe fn adopt<T>(&self) -> Rc<RefCell<T>> {
        let ptr = self.ptr.cast::<RefCell<T>>();
        unsafe {
            Rc::increment_strong_count(ptr);
            Rc::from_raw(ptr)
        }
    }
}

struct Session {
    previous: Vec<HotSignal>,
    captured: Vec<HotSignal>,
    mismatch: Option<String>,
}

thread_local! {
    /// Open while the app is being constructed.
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    /// Signals of the running app, in creation order (handed to the next build).
    static CURRENT: RefCell<Vec<HotSignal>> = const { RefCell::new(Vec::new()) };
}

/// Starts recording the app's signals; `previous` are the signals of the previous build to
/// adopt (empty for a fresh start).
pub fn begin(previous: Vec<HotSignal>) {
    SESSION.with(|s| {
        *s.borrow_mut() = Some(Session {
            previous,
            captured: Vec::new(),
            mismatch: None,
        })
    });
}

/// Stops recording. Returns why the previous state could not be carried over, if it couldn't
/// (always `None` for a fresh start). The recorded signals become [`take_current`]'s.
pub fn end() -> Option<String> {
    let Session {
        previous,
        captured,
        mut mismatch,
    } = SESSION.with(|s| s.borrow_mut().take())?;
    if mismatch.is_none() && captured.len() < previous.len() {
        mismatch = Some(format!(
            "{} state value(s) removed",
            previous.len() - captured.len()
        ));
    }
    CURRENT.with(|c| *c.borrow_mut() = captured);
    mismatch
}

/// The running app's signals, to hand to the next build.
pub fn take_current() -> Vec<HotSignal> {
    CURRENT.with(|c| std::mem::take(&mut *c.borrow_mut()))
}

/// Creates a signal's cell — adopting the previous build's cell when a session is open.
pub(crate) fn new_cell<T>(value: T) -> Rc<RefCell<T>> {
    let mut unused = None;
    let rc = SESSION.with(|s| {
        let mut s = s.borrow_mut();
        let Some(s) = s.as_mut() else {
            return Rc::new(RefCell::new(value));
        };
        let index = s.captured.len();
        let rc = match s.previous.get(index) {
            Some(prev) if s.mismatch.is_none() && prev.matches::<T>() => {
                unused = Some(value);
                // SAFETY: the type was just checked.
                unsafe { prev.adopt::<T>() }
            }
            prev => {
                if s.mismatch.is_none() && !s.previous.is_empty() {
                    s.mismatch = Some(match prev {
                        Some(prev) => format!(
                            "state #{} changed type: `{}` → `{}`",
                            index + 1,
                            prev.type_name(),
                            type_name::<T>()
                        ),
                        None => format!("new state added: `{}`", type_name::<T>()),
                    });
                }
                Rc::new(RefCell::new(value))
            }
        };
        s.captured.push(HotSignal::of(&rc));
        rc
    });
    drop(unused); // outside the borrow: dropping `T` may run arbitrary code
    rc
}

#[cfg(test)]
mod tests {
    use super::{begin, end, take_current};
    use crate::Signal;

    #[test]
    fn adopts_previous_signals() {
        begin(Vec::new());
        let a = Signal::new(1);
        let b = Signal::new(String::from("x"));
        assert_eq!(end(), None);
        a.set(5);

        begin(take_current());
        let a2 = Signal::new(0);
        let b2 = Signal::new(String::new());
        assert_eq!(end(), None);
        assert_eq!(a2.get(), 5);
        assert_eq!(b2.get(), "x");
        // Same cell: updates are shared.
        a2.set(7);
        assert_eq!(a.get(), 7);
        drop(b);
    }

    #[test]
    fn reports_mismatches() {
        begin(Vec::new());
        let _a = Signal::new(1i32);
        end();
        let prev = take_current();

        begin(prev.clone());
        let changed = Signal::new(1u8);
        let reason = end().unwrap();
        assert!(reason.contains("changed type"), "{reason}");
        assert_eq!(changed.get(), 1); // not adopted

        begin(prev.clone());
        let _a = Signal::new(0i32);
        let _extra = Signal::new(0i32);
        assert!(end().unwrap().contains("new state added"));

        begin(prev);
        assert!(end().unwrap().contains("removed"));
    }

    #[test]
    fn signals_outside_a_session_are_not_recorded() {
        take_current();
        let _s = Signal::new(1);
        assert!(take_current().is_empty());
    }
}
