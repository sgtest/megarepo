// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![feature(std_misc, collections, catch_panic, rand, sync_poison)]

use std::__rand::{thread_rng, Rng};
use std::thread;

use std::collections::BinaryHeap;
use std::cmp;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};

static DROP_COUNTER: AtomicUsize = ATOMIC_USIZE_INIT;

// old binaryheap failed this test
//
// Integrity means that all elements are present after a comparison panics,
// even if the order may not be correct.
//
// Destructors must be called exactly once per element.
fn test_integrity() {
    #[derive(Eq, PartialEq, Ord, Clone, Debug)]
    struct PanicOrd<T>(T, bool);

    impl<T> Drop for PanicOrd<T> {
        fn drop(&mut self) {
            // update global drop count
            DROP_COUNTER.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl<T: PartialOrd> PartialOrd for PanicOrd<T> {
        fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
            if self.1 || other.1 {
                panic!("Panicking comparison");
            }
            self.0.partial_cmp(&other.0)
        }
    }
    let mut rng = thread_rng();
    const DATASZ: usize = 32;
    const NTEST: usize = 10;

    // don't use 0 in the data -- we want to catch the zeroed-out case.
    let data = (1..DATASZ + 1).collect::<Vec<_>>();

    // since it's a fuzzy test, run several tries.
    for _ in 0..NTEST {
        for i in 1..DATASZ + 1 {
            DROP_COUNTER.store(0, Ordering::SeqCst);

            let mut panic_ords: Vec<_> = data.iter()
                                             .filter(|&&x| x != i)
                                             .map(|&x| PanicOrd(x, false))
                                             .collect();
            let panic_item = PanicOrd(i, true);

            // heapify the sane items
            rng.shuffle(&mut panic_ords);
            let heap = Arc::new(Mutex::new(BinaryHeap::from_vec(panic_ords)));
            let inner_data;

            {
                let heap_ref = heap.clone();


                // push the panicking item to the heap and catch the panic
                let thread_result = thread::catch_panic(move || {
                    heap.lock().unwrap().push(panic_item);
                });
                assert!(thread_result.is_err());

                // Assert no elements were dropped
                let drops = DROP_COUNTER.load(Ordering::SeqCst);
                //assert!(drops == 0, "Must not drop items. drops={}", drops);

                {
                    // now fetch the binary heap's data vector
                    let mutex_guard = match heap_ref.lock() {
                        Ok(x) => x,
                        Err(poison) => poison.into_inner(),
                    };
                    inner_data = mutex_guard.clone().into_vec();
                }
            }
            let drops = DROP_COUNTER.load(Ordering::SeqCst);
            assert_eq!(drops, DATASZ);

            let mut data_sorted = inner_data.into_iter().map(|p| p.0).collect::<Vec<_>>();
            data_sorted.sort();
            assert_eq!(data_sorted, data);
        }
    }
}

fn main() {
    test_integrity();
}

