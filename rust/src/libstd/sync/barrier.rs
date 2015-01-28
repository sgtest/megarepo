// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use sync::{Mutex, Condvar};

/// A barrier enables multiple tasks to synchronize the beginning
/// of some computation.
///
/// ```rust
/// use std::sync::{Arc, Barrier};
/// use std::thread::Thread;
///
/// let barrier = Arc::new(Barrier::new(10));
/// for _ in range(0u, 10) {
///     let c = barrier.clone();
///     // The same messages will be printed together.
///     // You will NOT see any interleaving.
///     Thread::spawn(move|| {
///         println!("before wait");
///         c.wait();
///         println!("after wait");
///     });
/// }
/// ```
#[stable(feature = "rust1", since = "1.0.0")]
pub struct Barrier {
    lock: Mutex<BarrierState>,
    cvar: Condvar,
    num_threads: uint,
}

// The inner state of a double barrier
struct BarrierState {
    count: uint,
    generation_id: uint,
}

/// A result returned from wait.
///
/// Currently this opaque structure only has one method, `.is_leader()`. Only
/// one thread will receive a result that will return `true` from this function.
#[allow(missing_copy_implementations)]
pub struct BarrierWaitResult(bool);

impl Barrier {
    /// Create a new barrier that can block a given number of threads.
    ///
    /// A barrier will block `n`-1 threads which call `wait` and then wake up
    /// all threads at once when the `n`th thread calls `wait`.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn new(n: uint) -> Barrier {
        Barrier {
            lock: Mutex::new(BarrierState {
                count: 0,
                generation_id: 0,
            }),
            cvar: Condvar::new(),
            num_threads: n,
        }
    }

    /// Block the current thread until all threads has rendezvoused here.
    ///
    /// Barriers are re-usable after all threads have rendezvoused once, and can
    /// be used continuously.
    ///
    /// A single (arbitrary) thread will receive a `BarrierWaitResult` that
    /// returns `true` from `is_leader` when returning from this function, and
    /// all other threads will receive a result that will return `false` from
    /// `is_leader`
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn wait(&self) -> BarrierWaitResult {
        let mut lock = self.lock.lock().unwrap();
        let local_gen = lock.generation_id;
        lock.count += 1;
        if lock.count < self.num_threads {
            // We need a while loop to guard against spurious wakeups.
            // http://en.wikipedia.org/wiki/Spurious_wakeup
            while local_gen == lock.generation_id &&
                  lock.count < self.num_threads {
                lock = self.cvar.wait(lock).unwrap();
            }
            BarrierWaitResult(false)
        } else {
            lock.count = 0;
            lock.generation_id += 1;
            self.cvar.notify_all();
            BarrierWaitResult(true)
        }
    }
}

impl BarrierWaitResult {
    /// Return whether this thread from `wait` is the "leader thread".
    ///
    /// Only one thread will have `true` returned from their result, all other
    /// threads will have `false` returned.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn is_leader(&self) -> bool { self.0 }
}

#[cfg(test)]
mod tests {
    use prelude::v1::*;

    use sync::{Arc, Barrier};
    use sync::mpsc::{channel, TryRecvError};
    use thread::Thread;

    #[test]
    fn test_barrier() {
        const N: uint = 10;

        let barrier = Arc::new(Barrier::new(N));
        let (tx, rx) = channel();

        for _ in range(0u, N - 1) {
            let c = barrier.clone();
            let tx = tx.clone();
            Thread::spawn(move|| {
                tx.send(c.wait().is_leader()).unwrap();
            });
        }

        // At this point, all spawned tasks should be blocked,
        // so we shouldn't get anything from the port
        assert!(match rx.try_recv() {
            Err(TryRecvError::Empty) => true,
            _ => false,
        });

        let mut leader_found = barrier.wait().is_leader();

        // Now, the barrier is cleared and we should get data.
        for _ in range(0u, N - 1) {
            if rx.recv().unwrap() {
                assert!(!leader_found);
                leader_found = true;
            }
        }
        assert!(leader_found);
    }
}
