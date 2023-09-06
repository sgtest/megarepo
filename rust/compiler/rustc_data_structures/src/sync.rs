//! This module defines various operations and types that are implemented in
//! one way for the serial compiler, and another way the parallel compiler.
//!
//! Operations
//! ----------
//! The parallel versions of operations use Rayon to execute code in parallel,
//! while the serial versions degenerate straightforwardly to serial execution.
//! The operations include `join`, `parallel`, `par_iter`, and `par_for_each`.
//!
//! Types
//! -----
//! The parallel versions of types provide various kinds of synchronization,
//! while the serial compiler versions do not.
//!
//! The following table shows how the types are implemented internally. Except
//! where noted otherwise, the type in column one is defined as a
//! newtype around the type from column two or three.
//!
//! | Type                    | Serial version      | Parallel version                |
//! | ----------------------- | ------------------- | ------------------------------- |
//! | `Lrc<T>`                | `rc::Rc<T>`         | `sync::Arc<T>`                  |
//! |` Weak<T>`               | `rc::Weak<T>`       | `sync::Weak<T>`                 |
//! |                         |                     |                                 |
//! | `AtomicBool`            | `Cell<bool>`        | `atomic::AtomicBool`            |
//! | `AtomicU32`             | `Cell<u32>`         | `atomic::AtomicU32`             |
//! | `AtomicU64`             | `Cell<u64>`         | `atomic::AtomicU64`             |
//! | `AtomicUsize`           | `Cell<usize>`       | `atomic::AtomicUsize`           |
//! |                         |                     |                                 |
//! | `Lock<T>`               | `RefCell<T>`        | `RefCell<T>` or                 |
//! |                         |                     | `parking_lot::Mutex<T>`         |
//! | `RwLock<T>`             | `RefCell<T>`        | `parking_lot::RwLock<T>`        |
//! | `MTLock<T>`        [^1] | `T`                 | `Lock<T>`                       |
//! | `MTLockRef<'a, T>` [^2] | `&'a mut MTLock<T>` | `&'a MTLock<T>`                 |
//! |                         |                     |                                 |
//! | `ParallelIterator`      | `Iterator`          | `rayon::iter::ParallelIterator` |
//!
//! [^1] `MTLock` is similar to `Lock`, but the serial version avoids the cost
//! of a `RefCell`. This is appropriate when interior mutability is not
//! required.
//!
//! [^2] `MTLockRef` is a typedef.

pub use crate::marker::*;
use parking_lot::Mutex;
use std::any::Any;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash};
use std::ops::{Deref, DerefMut};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};

mod lock;
pub use lock::{Lock, LockGuard};

mod worker_local;
pub use worker_local::{Registry, WorkerLocal};

pub use std::sync::atomic::Ordering;
pub use std::sync::atomic::Ordering::SeqCst;

pub use vec::{AppendOnlyIndexVec, AppendOnlyVec};

mod vec;

mod freeze;
pub use freeze::{FreezeLock, FreezeReadGuard, FreezeWriteGuard};

mod mode {
    use super::Ordering;
    use std::sync::atomic::AtomicU8;

    const UNINITIALIZED: u8 = 0;
    const DYN_NOT_THREAD_SAFE: u8 = 1;
    const DYN_THREAD_SAFE: u8 = 2;

    static DYN_THREAD_SAFE_MODE: AtomicU8 = AtomicU8::new(UNINITIALIZED);

    // Whether thread safety is enabled (due to running under multiple threads).
    #[inline]
    pub fn is_dyn_thread_safe() -> bool {
        match DYN_THREAD_SAFE_MODE.load(Ordering::Relaxed) {
            DYN_NOT_THREAD_SAFE => false,
            DYN_THREAD_SAFE => true,
            _ => panic!("uninitialized dyn_thread_safe mode!"),
        }
    }

    // Whether thread safety might be enabled.
    #[inline]
    #[cfg(parallel_compiler)]
    pub fn might_be_dyn_thread_safe() -> bool {
        DYN_THREAD_SAFE_MODE.load(Ordering::Relaxed) != DYN_NOT_THREAD_SAFE
    }

    // Only set by the `-Z threads` compile option
    pub fn set_dyn_thread_safe_mode(mode: bool) {
        let set: u8 = if mode { DYN_THREAD_SAFE } else { DYN_NOT_THREAD_SAFE };
        let previous = DYN_THREAD_SAFE_MODE.compare_exchange(
            UNINITIALIZED,
            set,
            Ordering::Relaxed,
            Ordering::Relaxed,
        );

        // Check that the mode was either uninitialized or was already set to the requested mode.
        assert!(previous.is_ok() || previous == Err(set));
    }
}

pub use mode::{is_dyn_thread_safe, set_dyn_thread_safe_mode};

/// A guard used to hold panics that occur during a parallel section to later by unwound.
/// This is used for the parallel compiler to prevent fatal errors from non-deterministically
/// hiding errors by ensuring that everything in the section has completed executing before
/// continuing with unwinding. It's also used for the non-parallel code to ensure error message
/// output match the parallel compiler for testing purposes.
pub struct ParallelGuard {
    panic: Mutex<Option<Box<dyn Any + std::marker::Send + 'static>>>,
}

impl ParallelGuard {
    pub fn run<R>(&self, f: impl FnOnce() -> R) -> Option<R> {
        catch_unwind(AssertUnwindSafe(f))
            .map_err(|err| {
                *self.panic.lock() = Some(err);
            })
            .ok()
    }
}

/// This gives access to a fresh parallel guard in the closure and will unwind any panics
/// caught in it after the closure returns.
#[inline]
pub fn parallel_guard<R>(f: impl FnOnce(&ParallelGuard) -> R) -> R {
    let guard = ParallelGuard { panic: Mutex::new(None) };
    let ret = f(&guard);
    if let Some(panic) = guard.panic.into_inner() {
        resume_unwind(panic);
    }
    ret
}

cfg_if! {
    if #[cfg(not(parallel_compiler))] {
        use std::ops::Add;
        use std::cell::Cell;

        pub unsafe auto trait Send {}
        pub unsafe auto trait Sync {}

        unsafe impl<T> Send for T {}
        unsafe impl<T> Sync for T {}

        /// This is a single threaded variant of `AtomicU64`, `AtomicUsize`, etc.
        /// It has explicit ordering arguments and is only intended for use with
        /// the native atomic types.
        /// You should use this type through the `AtomicU64`, `AtomicUsize`, etc, type aliases
        /// as it's not intended to be used separately.
        #[derive(Debug, Default)]
        pub struct Atomic<T: Copy>(Cell<T>);

        impl<T: Copy> Atomic<T> {
            #[inline]
            pub fn new(v: T) -> Self {
                Atomic(Cell::new(v))
            }

            #[inline]
            pub fn into_inner(self) -> T {
                self.0.into_inner()
            }

            #[inline]
            pub fn load(&self, _: Ordering) -> T {
                self.0.get()
            }

            #[inline]
            pub fn store(&self, val: T, _: Ordering) {
                self.0.set(val)
            }

            #[inline]
            pub fn swap(&self, val: T, _: Ordering) -> T {
                self.0.replace(val)
            }
        }

        impl Atomic<bool> {
            pub fn fetch_or(&self, val: bool, _: Ordering) -> bool {
                let old = self.0.get();
                self.0.set(val | old);
                old
            }
            pub fn fetch_and(&self, val: bool, _: Ordering) -> bool {
                let old = self.0.get();
                self.0.set(val & old);
                old
            }
        }

        impl<T: Copy + PartialEq> Atomic<T> {
            #[inline]
            pub fn compare_exchange(&self,
                                    current: T,
                                    new: T,
                                    _: Ordering,
                                    _: Ordering)
                                    -> Result<T, T> {
                let read = self.0.get();
                if read == current {
                    self.0.set(new);
                    Ok(read)
                } else {
                    Err(read)
                }
            }
        }

        impl<T: Add<Output=T> + Copy> Atomic<T> {
            #[inline]
            pub fn fetch_add(&self, val: T, _: Ordering) -> T {
                let old = self.0.get();
                self.0.set(old + val);
                old
            }
        }

        pub type AtomicUsize = Atomic<usize>;
        pub type AtomicBool = Atomic<bool>;
        pub type AtomicU32 = Atomic<u32>;
        pub type AtomicU64 = Atomic<u64>;

        pub fn join<A, B, RA, RB>(oper_a: A, oper_b: B) -> (RA, RB)
            where A: FnOnce() -> RA,
                  B: FnOnce() -> RB
        {
            let (a, b) = parallel_guard(|guard| {
                let a = guard.run(oper_a);
                let b = guard.run(oper_b);
                (a, b)
            });
            (a.unwrap(), b.unwrap())
        }

        #[macro_export]
        macro_rules! parallel {
            ($($blocks:block),*) => {{
                $crate::sync::parallel_guard(|guard| {
                    $(guard.run(|| $blocks);)*
                });
            }}
        }

        pub fn par_for_each_in<T: IntoIterator>(t: T, mut for_each: impl FnMut(T::Item) + Sync + Send) {
            parallel_guard(|guard| {
                t.into_iter().for_each(|i| {
                    guard.run(|| for_each(i));
                });
            })
        }

        pub fn par_map<T: IntoIterator, R, C: FromIterator<R>>(
            t: T,
            mut map: impl FnMut(<<T as IntoIterator>::IntoIter as Iterator>::Item) -> R,
        ) -> C {
            parallel_guard(|guard| {
                t.into_iter().filter_map(|i| guard.run(|| map(i))).collect()
            })
        }

        pub use std::rc::Rc as Lrc;
        pub use std::rc::Weak as Weak;
        pub use std::cell::Ref as ReadGuard;
        pub use std::cell::Ref as MappedReadGuard;
        pub use std::cell::RefMut as WriteGuard;
        pub use std::cell::RefMut as MappedWriteGuard;
        pub use std::cell::RefMut as MappedLockGuard;

        pub use std::cell::OnceCell as OnceLock;

        use std::cell::RefCell as InnerRwLock;

        pub type MTLockRef<'a, T> = &'a mut MTLock<T>;

        #[derive(Debug, Default)]
        pub struct MTLock<T>(T);

        impl<T> MTLock<T> {
            #[inline(always)]
            pub fn new(inner: T) -> Self {
                MTLock(inner)
            }

            #[inline(always)]
            pub fn into_inner(self) -> T {
                self.0
            }

            #[inline(always)]
            pub fn get_mut(&mut self) -> &mut T {
                &mut self.0
            }

            #[inline(always)]
            pub fn lock(&self) -> &T {
                &self.0
            }

            #[inline(always)]
            pub fn lock_mut(&mut self) -> &mut T {
                &mut self.0
            }
        }

        // FIXME: Probably a bad idea (in the threaded case)
        impl<T: Clone> Clone for MTLock<T> {
            #[inline]
            fn clone(&self) -> Self {
                MTLock(self.0.clone())
            }
        }
    } else {
        pub use std::marker::Send as Send;
        pub use std::marker::Sync as Sync;

        pub use parking_lot::RwLockReadGuard as ReadGuard;
        pub use parking_lot::MappedRwLockReadGuard as MappedReadGuard;
        pub use parking_lot::RwLockWriteGuard as WriteGuard;
        pub use parking_lot::MappedRwLockWriteGuard as MappedWriteGuard;

        pub use parking_lot::MappedMutexGuard as MappedLockGuard;

        pub use std::sync::OnceLock;

        pub use std::sync::atomic::{AtomicBool, AtomicUsize, AtomicU32, AtomicU64};

        pub use std::sync::Arc as Lrc;
        pub use std::sync::Weak as Weak;

        pub type MTLockRef<'a, T> = &'a MTLock<T>;

        #[derive(Debug, Default)]
        pub struct MTLock<T>(Lock<T>);

        impl<T> MTLock<T> {
            #[inline(always)]
            pub fn new(inner: T) -> Self {
                MTLock(Lock::new(inner))
            }

            #[inline(always)]
            pub fn into_inner(self) -> T {
                self.0.into_inner()
            }

            #[inline(always)]
            pub fn get_mut(&mut self) -> &mut T {
                self.0.get_mut()
            }

            #[inline(always)]
            pub fn lock(&self) -> LockGuard<'_, T> {
                self.0.lock()
            }

            #[inline(always)]
            pub fn lock_mut(&self) -> LockGuard<'_, T> {
                self.lock()
            }
        }

        use parking_lot::RwLock as InnerRwLock;

        use std::thread;

        #[inline]
        pub fn join<A, B, RA: DynSend, RB: DynSend>(oper_a: A, oper_b: B) -> (RA, RB)
        where
            A: FnOnce() -> RA + DynSend,
            B: FnOnce() -> RB + DynSend,
        {
            if mode::is_dyn_thread_safe() {
                let oper_a = FromDyn::from(oper_a);
                let oper_b = FromDyn::from(oper_b);
                let (a, b) = rayon::join(move || FromDyn::from(oper_a.into_inner()()), move || FromDyn::from(oper_b.into_inner()()));
                (a.into_inner(), b.into_inner())
            } else {
                let (a, b) = parallel_guard(|guard| {
                    let a = guard.run(oper_a);
                    let b = guard.run(oper_b);
                    (a, b)
                });
                (a.unwrap(), b.unwrap())
            }
        }

        // This function only works when `mode::is_dyn_thread_safe()`.
        pub fn scope<'scope, OP, R>(op: OP) -> R
        where
            OP: FnOnce(&rayon::Scope<'scope>) -> R + DynSend,
            R: DynSend,
        {
            let op = FromDyn::from(op);
            rayon::scope(|s| FromDyn::from(op.into_inner()(s))).into_inner()
        }

        /// Runs a list of blocks in parallel. The first block is executed immediately on
        /// the current thread. Use that for the longest running block.
        #[macro_export]
        macro_rules! parallel {
            (impl $fblock:block [$($c:expr,)*] [$block:expr $(, $rest:expr)*]) => {
                parallel!(impl $fblock [$block, $($c,)*] [$($rest),*])
            };
            (impl $fblock:block [$($blocks:expr,)*] []) => {
                ::rustc_data_structures::sync::scope(|s| {
                    $(let block = rustc_data_structures::sync::FromDyn::from(|| $blocks);
                    s.spawn(move |_| block.into_inner()());)*
                    (|| $fblock)();
                });
            };
            ($fblock:block, $($blocks:block),*) => {
                if rustc_data_structures::sync::is_dyn_thread_safe() {
                    // Reverse the order of the later blocks since Rayon executes them in reverse order
                    // when using a single thread. This ensures the execution order matches that
                    // of a single threaded rustc.
                    parallel!(impl $fblock [] [$($blocks),*]);
                } else {
                    $crate::sync::parallel_guard(|guard| {
                        guard.run(|| $fblock);
                        $(guard.run(|| $blocks);)*
                    });
                }
            };
        }

        use rayon::iter::{FromParallelIterator, IntoParallelIterator, ParallelIterator};

        pub fn par_for_each_in<I, T: IntoIterator<Item = I> + IntoParallelIterator<Item = I>>(
            t: T,
            for_each: impl Fn(I) + DynSync + DynSend
        ) {
            parallel_guard(|guard| {
                if mode::is_dyn_thread_safe() {
                    let for_each = FromDyn::from(for_each);
                    t.into_par_iter().for_each(|i| {
                        guard.run(|| for_each(i));
                    });
                } else {
                    t.into_iter().for_each(|i| {
                        guard.run(|| for_each(i));
                    });
                }
            });
        }

        pub fn par_map<
            I,
            T: IntoIterator<Item = I> + IntoParallelIterator<Item = I>,
            R: std::marker::Send,
            C: FromIterator<R> + FromParallelIterator<R>
        >(
            t: T,
            map: impl Fn(I) -> R + DynSync + DynSend
        ) -> C {
            parallel_guard(|guard| {
                if mode::is_dyn_thread_safe() {
                    let map = FromDyn::from(map);
                    t.into_par_iter().filter_map(|i| guard.run(|| map(i))).collect()
                } else {
                    t.into_iter().filter_map(|i| guard.run(|| map(i))).collect()
                }
            })
        }

        /// This makes locks panic if they are already held.
        /// It is only useful when you are running in a single thread
        const ERROR_CHECKING: bool = false;
    }
}

#[derive(Default)]
#[cfg_attr(parallel_compiler, repr(align(64)))]
pub struct CacheAligned<T>(pub T);

pub trait HashMapExt<K, V> {
    /// Same as HashMap::insert, but it may panic if there's already an
    /// entry for `key` with a value not equal to `value`
    fn insert_same(&mut self, key: K, value: V);
}

impl<K: Eq + Hash, V: Eq, S: BuildHasher> HashMapExt<K, V> for HashMap<K, V, S> {
    fn insert_same(&mut self, key: K, value: V) {
        self.entry(key).and_modify(|old| assert!(*old == value)).or_insert(value);
    }
}

#[derive(Debug, Default)]
pub struct RwLock<T>(InnerRwLock<T>);

impl<T> RwLock<T> {
    #[inline(always)]
    pub fn new(inner: T) -> Self {
        RwLock(InnerRwLock::new(inner))
    }

    #[inline(always)]
    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }

    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.0.get_mut()
    }

    #[cfg(not(parallel_compiler))]
    #[inline(always)]
    #[track_caller]
    pub fn read(&self) -> ReadGuard<'_, T> {
        self.0.borrow()
    }

    #[cfg(parallel_compiler)]
    #[inline(always)]
    pub fn read(&self) -> ReadGuard<'_, T> {
        if ERROR_CHECKING {
            self.0.try_read().expect("lock was already held")
        } else {
            self.0.read()
        }
    }

    #[inline(always)]
    #[track_caller]
    pub fn with_read_lock<F: FnOnce(&T) -> R, R>(&self, f: F) -> R {
        f(&*self.read())
    }

    #[cfg(not(parallel_compiler))]
    #[inline(always)]
    pub fn try_write(&self) -> Result<WriteGuard<'_, T>, ()> {
        self.0.try_borrow_mut().map_err(|_| ())
    }

    #[cfg(parallel_compiler)]
    #[inline(always)]
    pub fn try_write(&self) -> Result<WriteGuard<'_, T>, ()> {
        self.0.try_write().ok_or(())
    }

    #[cfg(not(parallel_compiler))]
    #[inline(always)]
    #[track_caller]
    pub fn write(&self) -> WriteGuard<'_, T> {
        self.0.borrow_mut()
    }

    #[cfg(parallel_compiler)]
    #[inline(always)]
    pub fn write(&self) -> WriteGuard<'_, T> {
        if ERROR_CHECKING {
            self.0.try_write().expect("lock was already held")
        } else {
            self.0.write()
        }
    }

    #[inline(always)]
    #[track_caller]
    pub fn with_write_lock<F: FnOnce(&mut T) -> R, R>(&self, f: F) -> R {
        f(&mut *self.write())
    }

    #[inline(always)]
    #[track_caller]
    pub fn borrow(&self) -> ReadGuard<'_, T> {
        self.read()
    }

    #[inline(always)]
    #[track_caller]
    pub fn borrow_mut(&self) -> WriteGuard<'_, T> {
        self.write()
    }

    #[cfg(not(parallel_compiler))]
    #[inline(always)]
    pub fn leak(&self) -> &T {
        ReadGuard::leak(self.read())
    }

    #[cfg(parallel_compiler)]
    #[inline(always)]
    pub fn leak(&self) -> &T {
        let guard = self.read();
        let ret = unsafe { &*(&*guard as *const T) };
        std::mem::forget(guard);
        ret
    }
}

// FIXME: Probably a bad idea
impl<T: Clone> Clone for RwLock<T> {
    #[inline]
    fn clone(&self) -> Self {
        RwLock::new(self.borrow().clone())
    }
}

/// A type which only allows its inner value to be used in one thread.
/// It will panic if it is used on multiple threads.
#[derive(Debug)]
pub struct OneThread<T> {
    #[cfg(parallel_compiler)]
    thread: thread::ThreadId,
    inner: T,
}

#[cfg(parallel_compiler)]
unsafe impl<T> std::marker::Sync for OneThread<T> {}
#[cfg(parallel_compiler)]
unsafe impl<T> std::marker::Send for OneThread<T> {}

impl<T> OneThread<T> {
    #[inline(always)]
    fn check(&self) {
        #[cfg(parallel_compiler)]
        assert_eq!(thread::current().id(), self.thread);
    }

    #[inline(always)]
    pub fn new(inner: T) -> Self {
        OneThread {
            #[cfg(parallel_compiler)]
            thread: thread::current().id(),
            inner,
        }
    }

    #[inline(always)]
    pub fn into_inner(value: Self) -> T {
        value.check();
        value.inner
    }
}

impl<T> Deref for OneThread<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.check();
        &self.inner
    }
}

impl<T> DerefMut for OneThread<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.check();
        &mut self.inner
    }
}
