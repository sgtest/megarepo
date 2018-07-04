// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![unstable(feature = "futures_api",
            reason = "futures in libcore are unstable",
            issue = "50547")]

use fmt;
use marker::Unpin;
use ptr::NonNull;

/// A `Waker` is a handle for waking up a task by notifying its executor that it
/// is ready to be run.
///
/// This handle contains a trait object pointing to an instance of the `UnsafeWake`
/// trait, allowing notifications to get routed through it.
#[repr(transparent)]
pub struct Waker {
    inner: NonNull<UnsafeWake>,
}

impl Unpin for Waker {}
unsafe impl Send for Waker {}
unsafe impl Sync for Waker {}

impl Waker {
    /// Constructs a new `Waker` directly.
    ///
    /// Note that most code will not need to call this. Implementers of the
    /// `UnsafeWake` trait will typically provide a wrapper that calls this
    /// but you otherwise shouldn't call it directly.
    ///
    /// If you're working with the standard library then it's recommended to
    /// use the `Waker::from` function instead which works with the safe
    /// `Arc` type and the safe `Wake` trait.
    #[inline]
    pub unsafe fn new(inner: NonNull<UnsafeWake>) -> Self {
        Waker { inner: inner }
    }

    /// Wake up the task associated with this `Waker`.
    #[inline]
    pub fn wake(&self) {
        unsafe { self.inner.as_ref().wake() }
    }

    /// Returns whether or not this `Waker` and `other` awaken the same task.
    ///
    /// This function works on a best-effort basis, and may return false even
    /// when the `Waker`s would awaken the same task. However, if this function
    /// returns true, it is guaranteed that the `Waker`s will awaken the same
    /// task.
    ///
    /// This function is primarily used for optimization purposes.
    #[inline]
    pub fn will_wake(&self, other: &Waker) -> bool {
        self.inner == other.inner
    }
}

impl Clone for Waker {
    #[inline]
    fn clone(&self) -> Self {
        unsafe {
            self.inner.as_ref().clone_raw()
        }
    }
}

impl fmt::Debug for Waker {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Waker")
            .finish()
    }
}

impl Drop for Waker {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            self.inner.as_ref().drop_raw()
        }
    }
}

/// A `LocalWaker` is a handle for waking up a task by notifying its executor that it
/// is ready to be run.
///
/// This is similar to the `Waker` type, but cannot be sent across threads.
/// Task executors can use this type to implement more optimized singlethreaded wakeup
/// behavior.
#[repr(transparent)]
pub struct LocalWaker {
    inner: NonNull<UnsafeWake>,
}

impl Unpin for LocalWaker {}
impl !Send for LocalWaker {}
impl !Sync for LocalWaker {}

impl LocalWaker {
    /// Constructs a new `LocalWaker` directly.
    ///
    /// Note that most code will not need to call this. Implementers of the
    /// `UnsafeWake` trait will typically provide a wrapper that calls this
    /// but you otherwise shouldn't call it directly.
    ///
    /// If you're working with the standard library then it's recommended to
    /// use the `LocalWaker::from` function instead which works with the safe
    /// `Rc` type and the safe `LocalWake` trait.
    ///
    /// For this function to be used safely, it must be sound to call `inner.wake_local()`
    /// on the current thread.
    #[inline]
    pub unsafe fn new(inner: NonNull<UnsafeWake>) -> Self {
        LocalWaker { inner: inner }
    }

    /// Wake up the task associated with this `LocalWaker`.
    #[inline]
    pub fn wake(&self) {
        unsafe { self.inner.as_ref().wake_local() }
    }

    /// Returns whether or not this `LocalWaker` and `other` `LocalWaker` awaken the same task.
    ///
    /// This function works on a best-effort basis, and may return false even
    /// when the `LocalWaker`s would awaken the same task. However, if this function
    /// returns true, it is guaranteed that the `LocalWaker`s will awaken the same
    /// task.
    ///
    /// This function is primarily used for optimization purposes.
    #[inline]
    pub fn will_wake(&self, other: &LocalWaker) -> bool {
        self.inner == other.inner
    }

    /// Returns whether or not this `LocalWaker` and `other` `Waker` awaken the same task.
    ///
    /// This function works on a best-effort basis, and may return false even
    /// when the `Waker`s would awaken the same task. However, if this function
    /// returns true, it is guaranteed that the `LocalWaker`s will awaken the same
    /// task.
    ///
    /// This function is primarily used for optimization purposes.
    #[inline]
    pub fn will_wake_nonlocal(&self, other: &Waker) -> bool {
        self.inner == other.inner
    }
}

impl From<LocalWaker> for Waker {
    #[inline]
    fn from(local_waker: LocalWaker) -> Self {
        Waker { inner: local_waker.inner }
    }
}

impl Clone for LocalWaker {
    #[inline]
    fn clone(&self) -> Self {
        unsafe {
            LocalWaker { inner: self.inner.as_ref().clone_raw().inner }
        }
    }
}

impl fmt::Debug for LocalWaker {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Waker")
            .finish()
    }
}

impl Drop for LocalWaker {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            self.inner.as_ref().drop_raw()
        }
    }
}

/// An unsafe trait for implementing custom memory management for a `Waker` or `LocalWaker`.
///
/// A `Waker` conceptually is a cloneable trait object for `Wake`, and is
/// most often essentially just `Arc<dyn Wake>`. However, in some contexts
/// (particularly `no_std`), it's desirable to avoid `Arc` in favor of some
/// custom memory management strategy. This trait is designed to allow for such
/// customization.
///
/// When using `std`, a default implementation of the `UnsafeWake` trait is provided for
/// `Arc<T>` where `T: Wake` and `Rc<T>` where `T: LocalWake`.
///
/// Although the methods on `UnsafeWake` take pointers rather than references,
pub unsafe trait UnsafeWake: Send + Sync {
    /// Creates a clone of this `UnsafeWake` and stores it behind a `Waker`.
    ///
    /// This function will create a new uniquely owned handle that under the
    /// hood references the same notification instance. In other words calls
    /// to `wake` on the returned handle should be equivalent to calls to
    /// `wake` on this handle.
    ///
    /// # Unsafety
    ///
    /// This function is unsafe to call because it's asserting the `UnsafeWake`
    /// value is in a consistent state, i.e. hasn't been dropped.
    unsafe fn clone_raw(&self) -> Waker;

    /// Drops this instance of `UnsafeWake`, deallocating resources
    /// associated with it.
    ///
    /// FIXME(cramertj)
    /// This method is intended to have a signature such as:
    ///
    /// ```ignore (not-a-doctest)
    /// fn drop_raw(self: *mut Self);
    /// ```
    ///
    /// Unfortunately in Rust today that signature is not object safe.
    /// Nevertheless it's recommended to implement this function *as if* that
    /// were its signature. As such it is not safe to call on an invalid
    /// pointer, nor is the validity of the pointer guaranteed after this
    /// function returns.
    ///
    /// # Unsafety
    ///
    /// This function is unsafe to call because it's asserting the `UnsafeWake`
    /// value is in a consistent state, i.e. hasn't been dropped.
    unsafe fn drop_raw(&self);

    /// Indicates that the associated task is ready to make progress and should
    /// be `poll`ed.
    ///
    /// Executors generally maintain a queue of "ready" tasks; `wake` should place
    /// the associated task onto this queue.
    ///
    /// # Panics
    ///
    /// Implementations should avoid panicking, but clients should also be prepared
    /// for panics.
    ///
    /// # Unsafety
    ///
    /// This function is unsafe to call because it's asserting the `UnsafeWake`
    /// value is in a consistent state, i.e. hasn't been dropped.
    unsafe fn wake(&self);

    /// Indicates that the associated task is ready to make progress and should
    /// be `poll`ed. This function is the same as `wake`, but can only be called
    /// from the thread that this `UnsafeWake` is "local" to. This allows for
    /// implementors to provide specialized wakeup behavior specific to the current
    /// thread. This function is called by `LocalWaker::wake`.
    ///
    /// Executors generally maintain a queue of "ready" tasks; `wake_local` should place
    /// the associated task onto this queue.
    ///
    /// # Panics
    ///
    /// Implementations should avoid panicking, but clients should also be prepared
    /// for panics.
    ///
    /// # Unsafety
    ///
    /// This function is unsafe to call because it's asserting the `UnsafeWake`
    /// value is in a consistent state, i.e. hasn't been dropped, and that the
    /// `UnsafeWake` hasn't moved from the thread on which it was created.
    unsafe fn wake_local(&self) {
        self.wake()
    }
}
