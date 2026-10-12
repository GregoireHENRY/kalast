//! What the UI app's threads share: the simulation, its config, the app's,
//! the meshes, the HUDs.
//!
//! The UI app draws its window and panels on the main thread and runs
//! scripts, their callbacks and the python tab on a thread of their own, so
//! that a slow script never holds the panels. Both read and write the same
//! scene, which was `Rc<RefCell<..>>` throughout and stays `RefCell` inside:
//! a [`Locked`] is an `Arc` around one, and every borrow of any of them is
//! taken under one lock for the whole process, reentrant, so a thread may
//! borrow as it always did -- a value inside another, the same one twice
//! for reading -- and only the other thread waits. One lock: no order to
//! take two in, no deadlock between them; and the UI's thread never takes
//! Python's, so none with that either.
//!
//! A borrow is held for as long as the code using it, which is short on the
//! UI's thread -- a frame's reading of the scene, a panel's -- and must be on
//! the scripts' too: work of seconds is done outside a borrow, and its result
//! put in after.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::ThreadId;

/// Which thread holds the lock, and how many times over.
static OWNER: Mutex<(Option<ThreadId>, usize)> = Mutex::new((None, 0));
static FREED: Condvar = Condvar::new();

/// The lock, held: released when the last guard of the thread holding it
/// drops. Not `Send`: it is released by the thread that took it.
pub struct Held(PhantomData<*const ()>);

/// Take the lock, waiting while another thread holds it; at once again on
/// the thread that does.
pub fn hold() -> Held {
    let me = std::thread::current().id();
    let mut owner = OWNER.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        match owner.0 {
            None => {
                *owner = (Some(me), 1);
                break;
            }
            Some(t) if t == me => {
                owner.1 += 1;
                break;
            }
            Some(_) => owner = FREED.wait(owner).unwrap_or_else(|e| e.into_inner()),
        }
    }
    Held(PhantomData)
}

impl Drop for Held {
    fn drop(&mut self) {
        let mut owner = OWNER.lock().unwrap_or_else(|e| e.into_inner());
        owner.1 -= 1;
        if owner.1 == 0 {
            owner.0 = None;
            FREED.notify_all();
        }
    }
}

/// A value the UI app's threads share, borrowed as a `RefCell` is.
pub struct Locked<T: ?Sized>(Arc<RefCell<T>>);

// SAFETY: the `RefCell` is reached only through `borrow` and `borrow_mut`,
// which take the process-wide lock first and keep it while their guard
// lives, so one thread at a time touches it -- its borrow flag included.
// The value itself crosses threads, hence `T: Send`.
unsafe impl<T: ?Sized + Send> Send for Locked<T> {}
unsafe impl<T: ?Sized + Send> Sync for Locked<T> {}

impl<T> Locked<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(RefCell::new(value)))
    }
}

impl<T: ?Sized> Locked<T> {
    /// Borrowed for reading, as `RefCell::borrow`: panics where that does,
    /// on a value this thread borrows mutably already.
    pub fn borrow(&self) -> Ref<'_, T> {
        let held = hold();
        Ref { value: self.0.borrow(), _held: held }
    }

    /// Borrowed for writing, as `RefCell::borrow_mut`.
    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        let held = hold();
        RefMut { value: self.0.borrow_mut(), _held: held }
    }

    /// As `RefCell::try_borrow`: an error where this thread borrows it
    /// mutably already. Another thread's borrow is waited for, as ever.
    pub fn try_borrow(&self) -> Result<Ref<'_, T>, std::cell::BorrowError> {
        let held = hold();
        self.0.try_borrow().map(|value| Ref { value, _held: held })
    }

    /// As `RefCell::try_borrow_mut`.
    pub fn try_borrow_mut(&self) -> Result<RefMut<'_, T>, std::cell::BorrowMutError> {
        let held = hold();
        self.0.try_borrow_mut().map(|value| RefMut { value, _held: held })
    }

    /// The same value, as `Rc::ptr_eq`.
    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        Arc::ptr_eq(&a.0, &b.0)
    }

    /// Where the value's cell is.
    pub fn as_ptr(&self) -> *const RefCell<T> {
        Arc::as_ptr(&self.0)
    }
}

impl<T> Locked<T> {
    /// The handle as a pointer, for a hosted example handed the scene across
    /// its library's edge (`app::hosted`), as `Rc::into_raw` was.
    pub fn into_raw(this: Self) -> *const RefCell<T> {
        Arc::into_raw(this.0)
    }

    /// The handle back from `into_raw`.
    ///
    /// # Safety
    ///
    /// `ptr` from `into_raw` of a `Locked<T>`, and not taken back before.
    pub unsafe fn from_raw(ptr: *const RefCell<T>) -> Self {
        Self(unsafe { Arc::from_raw(ptr) })
    }
}

impl<T: ?Sized> Clone for Locked<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Default> Default for Locked<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: ?Sized + std::fmt::Debug> std::fmt::Debug for Locked<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.borrow().fmt(f)
    }
}

/// A `Locked` borrowed for reading. The borrow goes before the lock.
pub struct Ref<'a, T: ?Sized> {
    value: std::cell::Ref<'a, T>,
    _held: Held,
}

impl<T: ?Sized + std::fmt::Debug> std::fmt::Debug for Ref<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<T: ?Sized> Deref for Ref<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

/// A `Locked` borrowed for writing.
pub struct RefMut<'a, T: ?Sized> {
    value: std::cell::RefMut<'a, T>,
    _held: Held,
}

impl<T: ?Sized + std::fmt::Debug> std::fmt::Debug for RefMut<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<T: ?Sized> Deref for RefMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T: ?Sized> DerefMut for RefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

#[cfg(test)]
mod tests {
    use super::Locked;

    /// A thread borrows as with a `RefCell`, nested; another waits its turn
    /// and sees what the first wrote.
    #[test]
    fn one_thread_at_a_time_and_nested_on_one() {
        let a = Locked::new(1);
        let b = Locked::new(vec![0u32; 0]);
        {
            let x = a.borrow();
            let y = a.borrow();
            b.borrow_mut().push(*x + *y);
        }
        let (a2, b2) = (a.clone(), b.clone());
        let held = a.borrow_mut();
        let t = std::thread::spawn(move || {
            *a2.borrow_mut() += 10;
            b2.borrow_mut().push(*a2.borrow());
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(*held, 1, "the other thread waited for the lock");
        drop(held);
        t.join().unwrap();
        assert_eq!(*b.borrow(), vec![2, 11]);
        assert!(Locked::ptr_eq(&a, &a.clone()));
    }
}
