//! Values that stay on the thread that made them, inside state that moves.
//!
//! A realm, a compiled app and a render cache are full of `Rc`s, and some of what
//! those `Rc`s point at is shared with the thread they were made on: the VM's
//! cycle collector keeps every object of the thread in one table (raw pointers,
//! walked by every collection), property names are canonical strings from the
//! thread's interner (found by address), compiled code comes from the thread's
//! cache, and computed styles share the thread's initial style. Such a value is
//! correct only on its thread: used or dropped on another, it indexes that
//! thread's tables with its own thread's slots, looks properties up by strings
//! the other interner never made, and races the first thread's collector on its
//! reference counts.
//!
//! Browser and application state holding such values is still `Send`, because a
//! world moves between threads (a Python thread pool running an experiment's
//! forks, a server answering on whichever worker is free). `Confined` is the one
//! place that makes that sound:
//!
//! * the value is reachable only on its home thread: `get`/`get_mut` return
//!   `None` anywhere else, and the owner rebuilds what it needs there (a realm
//!   from its saved state and journal, a cache from scratch);
//! * dropped on another thread, the value is handed back to its home thread and
//!   dropped there the next time that thread makes or reaches a confined value;
//! * when the home thread has exited, its tables are gone with it, and a value
//!   still homed there is never touched again: it is leaked, not dropped, since
//!   dropping it would run its destructors against another thread's tables.
//!   `leaked()` counts them.

use std::any::Any;
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A thread's side of the hand-back: whether it is still running, and the values
/// other threads dropped for it to drop.
struct Home {
    state: Mutex<HomeState>,
}

#[derive(Default)]
struct HomeState {
    exited: bool,
    graves: Vec<Grave>,
}

/// A confined value dropped away from home.
struct Grave(#[allow(dead_code)] Box<dyn Any>);
// SAFETY: a grave is only ever dropped by its home thread (`drain`), or leaked
// (`HomeGuard::drop`, `Confined::drop` once the home exited); the threads that
// move it there never read it.
unsafe impl Send for Grave {}

/// Owns this thread's `Home`; dropped as the thread exits.
struct HomeGuard(Arc<Home>);

impl Drop for HomeGuard {
    fn drop(&mut self) {
        let mut state = lock(&self.0.state);
        state.exited = true;
        // The thread's tables may already be gone: drop nothing now.
        let graves = std::mem::take(&mut state.graves);
        LEAKED.fetch_add(graves.len() as u64, Ordering::Relaxed);
        std::mem::forget(graves);
    }
}

thread_local! {
    static HOME: HomeGuard = HomeGuard(Arc::new(Home {
        state: Mutex::new(HomeState::default()),
    }));
}

static LEAKED: AtomicU64 = AtomicU64::new(0);

/// Confined values left to a thread that had exited (so never dropped), in this
/// process.
pub fn leaked() -> u64 {
    LEAKED.load(Ordering::Relaxed)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// This thread's home, if the thread is not being torn down.
fn here() -> Option<Arc<Home>> {
    HOME.try_with(|h| h.0.clone()).ok()
}

/// Drops what other threads handed back to this one. Called whenever this thread
/// makes or reaches a confined value, which is a safe point for every owner here
/// (none of them holds a borrow into a confined value then).
pub fn drain() {
    let Some(home) = here() else { return };
    let graves = std::mem::take(&mut lock(&home.state).graves);
    drop(graves);
}

/// A value usable only on the thread that made it; `Send` whatever it holds. See
/// the module documentation.
pub struct Confined<T: 'static> {
    value: ManuallyDrop<Box<T>>,
    home: Arc<Home>,
}

// SAFETY: the value is reached only through `get`/`get_mut`, which check that
// the caller is on the home thread, or through `get_unchecked`, whose callers
// promise to read nothing shared with the home thread; it is dropped only on its
// home thread (directly, or through the hand-back) or leaked.
unsafe impl<T: 'static> Send for Confined<T> {}

impl<T: 'static> Confined<T> {
    /// Confines `value` to this thread.
    ///
    /// # Panics
    /// While this thread is being torn down.
    pub fn new(value: T) -> Self {
        drain();
        Confined {
            value: ManuallyDrop::new(Box::new(value)),
            home: here().expect("a confined value made while its thread exits"),
        }
    }

    /// Whether this thread is the value's home.
    pub fn is_home(&self) -> bool {
        // Not while this thread is being torn down: its tables may be gone.
        HOME.try_with(|h| Arc::ptr_eq(&h.0, &self.home))
            .unwrap_or(false)
    }

    pub fn get(&self) -> Option<&T> {
        if !self.is_home() {
            return None;
        }
        drain();
        Some(&self.value)
    }

    pub fn get_mut(&mut self) -> Option<&mut T> {
        if !self.is_home() {
            return None;
        }
        drain();
        Some(&mut self.value)
    }

    /// The value, from any thread.
    ///
    /// # Safety
    /// The caller reads only state of the value's own (plain data behind no `Rc`
    /// the home thread can reach, such as a journal of inputs), clones and drops
    /// no `Rc`, and runs no script: the home thread may be running at the same
    /// time.
    pub unsafe fn get_unchecked(&self) -> &T {
        &self.value
    }
}

impl<T: 'static> Drop for Confined<T> {
    fn drop(&mut self) {
        // SAFETY: `value` is taken exactly once, here.
        let value = unsafe { ManuallyDrop::take(&mut self.value) };
        if self.is_home() {
            drop(value);
            return;
        }
        let mut state = lock(&self.home.state);
        if state.exited {
            LEAKED.fetch_add(1, Ordering::Relaxed);
            std::mem::forget(value);
        } else {
            state.graves.push(Grave(value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    thread_local! {
        static DROPS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }

    struct Counted(#[allow(dead_code)] Rc<()>);
    impl Drop for Counted {
        fn drop(&mut self) {
            DROPS.with(|d| d.set(d.get() + 1));
        }
    }

    #[test]
    fn reachable_only_at_home_and_dropped_there() {
        let mut c = Confined::new(Counted(Rc::new(())));
        assert!(c.get_mut().is_some());
        let c = std::thread::spawn(move || {
            assert!(c.get().is_none());
            assert!(!c.is_home());
            c
        })
        .join()
        .unwrap();
        assert!(c.get().is_some());
        // Dropped away from home: handed back, dropped by the next drain here.
        std::thread::spawn(move || drop(c)).join().unwrap();
        assert_eq!(DROPS.with(|d| d.get()), 0);
        drain();
        assert_eq!(DROPS.with(|d| d.get()), 1);
    }

    #[test]
    fn a_value_whose_home_exited_is_leaked_not_dropped() {
        let before = leaked();
        let c = std::thread::spawn(|| Confined::new(Counted(Rc::new(()))))
            .join()
            .unwrap();
        assert!(c.get().is_none());
        drop(c);
        assert!(leaked() > before);
        assert_eq!(DROPS.with(|d| d.get()), 0);
    }
}
