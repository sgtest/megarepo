// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
 * A utility class for implementing "snapshottable" things; a
 * snapshottable data structure permits you to take a snapshot (via
 * `start_snapshot`) and then, after making some changes, elect either
 * to rollback to the start of the snapshot or commit those changes.
 *
 * This vector is intended to be used as part of an abstraction, not
 * serve as a complete abstraction on its own. As such, while it will
 * roll back most changes on its own, it also supports a `get_mut`
 * operation that gives you an abitrary mutable pointer into the
 * vector. To ensure that any changes you make this with this pointer
 * are rolled back, you must invoke `record` to record any changes you
 * make and also supplying a delegate capable of reversing those
 * changes.
 */

use std::kinds::marker;
use std::mem;

#[deriving(PartialEq)]
enum UndoLog<T,U> {
    /// Indicates where a snapshot started.
    OpenSnapshot,

    /// Indicates a snapshot that has been committed.
    CommittedSnapshot,

    /// New variable with given index was created.
    NewElem(uint),

    /// Variable with given index was changed *from* the given value.
    SetElem(uint, T),

    /// Extensible set of actions
    Other(U)
}

pub struct SnapshotVec<T,U,D> {
    values: Vec<T>,
    undo_log: Vec<UndoLog<T,U>>,
    delegate: D
}

pub struct Snapshot {
    // Snapshots are tokens that should be created/consumed linearly.
    marker: marker::NoCopy,

    // Length of the undo log at the time the snapshot was taken.
    length: uint,
}

pub trait SnapshotVecDelegate<T,U> {
    fn reverse(&mut self, values: &mut Vec<T>, action: U);
}

impl<T,U,D:SnapshotVecDelegate<T,U>> SnapshotVec<T,U,D> {
    pub fn new(delegate: D) -> SnapshotVec<T,U,D> {
        SnapshotVec {
            values: Vec::new(),
            undo_log: Vec::new(),
            delegate: delegate
        }
    }

    fn in_snapshot(&self) -> bool {
        !self.undo_log.is_empty()
    }

    pub fn record(&mut self, action: U) {
        if self.in_snapshot() {
            self.undo_log.push(Other(action));
        }
    }

    pub fn push(&mut self, elem: T) -> uint {
        let len = self.values.len();
        self.values.push(elem);

        if self.in_snapshot() {
            self.undo_log.push(NewElem(len));
        }

        len
    }

    pub fn get<'a>(&'a self, index: uint) -> &'a T {
        self.values.get(index)
    }

    pub fn get_mut<'a>(&'a mut self, index: uint) -> &'a mut T {
        /*!
         * Returns a mutable pointer into the vec; whatever changes
         * you make here cannot be undone automatically, so you should
         * be sure call `record()` with some sort of suitable undo
         * action.
         */

        self.values.get_mut(index)
    }

    pub fn set(&mut self, index: uint, new_elem: T) {
        /*!
         * Updates the element at the given index. The old value will
         * saved (and perhaps restored) if a snapshot is active.
         */

        let old_elem = mem::replace(self.values.get_mut(index), new_elem);
        if self.in_snapshot() {
            self.undo_log.push(SetElem(index, old_elem));
        }
    }

    pub fn start_snapshot(&mut self) -> Snapshot {
        let length = self.undo_log.len();
        self.undo_log.push(OpenSnapshot);
        Snapshot { length: length,
                   marker: marker::NoCopy }
    }

    fn assert_open_snapshot(&self, snapshot: &Snapshot) {
        // Or else there was a failure to follow a stack discipline:
        assert!(self.undo_log.len() > snapshot.length);

        // Invariant established by start_snapshot():
        assert!(
            match *self.undo_log.get(snapshot.length) {
                OpenSnapshot => true,
                _ => false
            });
    }

    pub fn rollback_to(&mut self, snapshot: Snapshot) {
        debug!("rollback_to({})", snapshot.length);

        self.assert_open_snapshot(&snapshot);

        while self.undo_log.len() > snapshot.length + 1 {
            match self.undo_log.pop().unwrap() {
                OpenSnapshot => {
                    // This indicates a failure to obey the stack discipline.
                    fail!("Cannot rollback an uncommited snapshot");
                }

                CommittedSnapshot => {
                    // This occurs when there are nested snapshots and
                    // the inner is commited but outer is rolled back.
                }

                NewElem(i) => {
                    self.values.pop();
                    assert!(self.values.len() == i);
                }

                SetElem(i, v) => {
                    *self.values.get_mut(i) = v;
                }

                Other(u) => {
                    self.delegate.reverse(&mut self.values, u);
                }
            }
        }

        let v = self.undo_log.pop().unwrap();
        assert!(match v { OpenSnapshot => true, _ => false });
        assert!(self.undo_log.len() == snapshot.length);
    }

    /**
     * Commits all changes since the last snapshot. Of course, they
     * can still be undone if there is a snapshot further out.
     */
    pub fn commit(&mut self, snapshot: Snapshot) {
        debug!("commit({})", snapshot.length);

        self.assert_open_snapshot(&snapshot);

        if snapshot.length == 0 {
            // The root snapshot.
            self.undo_log.truncate(0);
        } else {
            *self.undo_log.get_mut(snapshot.length) = CommittedSnapshot;
        }
    }
}
