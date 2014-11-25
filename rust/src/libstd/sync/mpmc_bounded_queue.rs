/* Copyright (c) 2010-2011 Dmitry Vyukov. All rights reserved.
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *
 *    1. Redistributions of source code must retain the above copyright notice,
 *       this list of conditions and the following disclaimer.
 *
 *    2. Redistributions in binary form must reproduce the above copyright
 *       notice, this list of conditions and the following disclaimer in the
 *       documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY DMITRY VYUKOV "AS IS" AND ANY EXPRESS OR IMPLIED
 * WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF
 * MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT
 * SHALL DMITRY VYUKOV OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
 * INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
 * LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
 * PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
 * LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE
 * OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF
 * ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 *
 * The views and conclusions contained in the software and documentation are
 * those of the authors and should not be interpreted as representing official
 * policies, either expressed or implied, of Dmitry Vyukov.
 */

#![experimental]
#![allow(missing_docs, dead_code)]

// http://www.1024cores.net/home/lock-free-algorithms/queues/bounded-mpmc-queue

use core::prelude::*;

use alloc::arc::Arc;
use vec::Vec;
use core::num::UnsignedInt;
use core::cell::UnsafeCell;

use sync::atomic::{AtomicUint,Relaxed,Release,Acquire};

struct Node<T> {
    sequence: AtomicUint,
    value: Option<T>,
}

struct State<T> {
    pad0: [u8, ..64],
    buffer: Vec<UnsafeCell<Node<T>>>,
    mask: uint,
    pad1: [u8, ..64],
    enqueue_pos: AtomicUint,
    pad2: [u8, ..64],
    dequeue_pos: AtomicUint,
    pad3: [u8, ..64],
}

pub struct Queue<T> {
    state: Arc<State<T>>,
}

impl<T: Send> State<T> {
    fn with_capacity(capacity: uint) -> State<T> {
        let capacity = if capacity < 2 || (capacity & (capacity - 1)) != 0 {
            if capacity < 2 {
                2u
            } else {
                // use next power of 2 as capacity
                capacity.next_power_of_two()
            }
        } else {
            capacity
        };
        let buffer = Vec::from_fn(capacity, |i| {
            UnsafeCell::new(Node { sequence:AtomicUint::new(i), value: None })
        });
        State{
            pad0: [0, ..64],
            buffer: buffer,
            mask: capacity-1,
            pad1: [0, ..64],
            enqueue_pos: AtomicUint::new(0),
            pad2: [0, ..64],
            dequeue_pos: AtomicUint::new(0),
            pad3: [0, ..64],
        }
    }

    fn push(&self, value: T) -> bool {
        let mask = self.mask;
        let mut pos = self.enqueue_pos.load(Relaxed);
        loop {
            let node = &self.buffer[pos & mask];
            let seq = unsafe { (*node.get()).sequence.load(Acquire) };
            let diff: int = seq as int - pos as int;

            if diff == 0 {
                let enqueue_pos = self.enqueue_pos.compare_and_swap(pos, pos+1, Relaxed);
                if enqueue_pos == pos {
                    unsafe {
                        (*node.get()).value = Some(value);
                        (*node.get()).sequence.store(pos+1, Release);
                    }
                    break
                } else {
                    pos = enqueue_pos;
                }
            } else if diff < 0 {
                return false
            } else {
                pos = self.enqueue_pos.load(Relaxed);
            }
        }
        true
    }

    fn pop(&self) -> Option<T> {
        let mask = self.mask;
        let mut pos = self.dequeue_pos.load(Relaxed);
        loop {
            let node = &self.buffer[pos & mask];
            let seq = unsafe { (*node.get()).sequence.load(Acquire) };
            let diff: int = seq as int - (pos + 1) as int;
            if diff == 0 {
                let dequeue_pos = self.dequeue_pos.compare_and_swap(pos, pos+1, Relaxed);
                if dequeue_pos == pos {
                    unsafe {
                        let value = (*node.get()).value.take();
                        (*node.get()).sequence.store(pos + mask + 1, Release);
                        return value
                    }
                } else {
                    pos = dequeue_pos;
                }
            } else if diff < 0 {
                return None
            } else {
                pos = self.dequeue_pos.load(Relaxed);
            }
        }
    }
}

impl<T: Send> Queue<T> {
    pub fn with_capacity(capacity: uint) -> Queue<T> {
        Queue{
            state: Arc::new(State::with_capacity(capacity))
        }
    }

    pub fn push(&self, value: T) -> bool {
        self.state.push(value)
    }

    pub fn pop(&self) -> Option<T> {
        self.state.pop()
    }
}

impl<T: Send> Clone for Queue<T> {
    fn clone(&self) -> Queue<T> {
        Queue { state: self.state.clone() }
    }
}

#[cfg(test)]
mod tests {
    use prelude::*;
    use super::Queue;

    #[test]
    fn test() {
        let nthreads = 8u;
        let nmsgs = 1000u;
        let q = Queue::with_capacity(nthreads*nmsgs);
        assert_eq!(None, q.pop());
        let (tx, rx) = channel();

        for _ in range(0, nthreads) {
            let q = q.clone();
            let tx = tx.clone();
            spawn(proc() {
                let q = q;
                for i in range(0, nmsgs) {
                    assert!(q.push(i));
                }
                tx.send(());
            });
        }

        let mut completion_rxs = vec![];
        for _ in range(0, nthreads) {
            let (tx, rx) = channel();
            completion_rxs.push(rx);
            let q = q.clone();
            spawn(proc() {
                let q = q;
                let mut i = 0u;
                loop {
                    match q.pop() {
                        None => {},
                        Some(_) => {
                            i += 1;
                            if i == nmsgs { break }
                        }
                    }
                }
                tx.send(i);
            });
        }

        for rx in completion_rxs.iter_mut() {
            assert_eq!(nmsgs, rx.recv());
        }
        for _ in range(0, nthreads) {
            rx.recv();
        }
    }
}
