// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A mutable, nullable memory location

#[missing_doc];

use cast::transmute_mut;
use prelude::*;
use util::replace;

/*
A dynamic, mutable location.

Similar to a mutable option type, but friendlier.
*/

#[mutable] // XXX remove after snap
#[no_freeze]
#[deriving(Clone, DeepClone, Eq)]
#[allow(missing_doc)]
pub struct Cell<T> {
    priv value: Option<T>
}

impl<T> Cell<T> {
    /// Creates a new full cell with the given value.
    pub fn new(value: T) -> Cell<T> {
        Cell { value: Some(value) }
    }

    /// Creates a new empty cell with no value inside.
    pub fn new_empty() -> Cell<T> {
        Cell { value: None }
    }

    /// Yields the value, failing if the cell is empty.
    pub fn take(&self) -> T {
        let this = unsafe { transmute_mut(self) };
        if this.is_empty() {
            fail!("attempt to take an empty cell");
        }

        replace(&mut this.value, None).unwrap()
    }

    /// Returns the value, failing if the cell is full.
    pub fn put_back(&self, value: T) {
        let this = unsafe { transmute_mut(self) };
        if !this.is_empty() {
            fail!("attempt to put a value back into a full cell");
        }
        this.value = Some(value);
    }

    /// Returns true if the cell is empty and false if the cell is full.
    pub fn is_empty(&self) -> bool {
        self.value.is_none()
    }

    /// Calls a closure with a reference to the value.
    pub fn with_ref<R>(&self, op: &fn(v: &T) -> R) -> R {
        let v = self.take();
        let r = op(&v);
        self.put_back(v);
        r
    }

    /// Calls a closure with a mutable reference to the value.
    pub fn with_mut_ref<R>(&self, op: &fn(v: &mut T) -> R) -> R {
        let mut v = self.take();
        let r = op(&mut v);
        self.put_back(v);
        r
    }
}

#[test]
fn test_basic() {
    let value_cell = Cell::new(~10);
    assert!(!value_cell.is_empty());
    let value = value_cell.take();
    assert!(value == ~10);
    assert!(value_cell.is_empty());
    value_cell.put_back(value);
    assert!(!value_cell.is_empty());
}

#[test]
#[should_fail]
#[ignore(cfg(windows))]
fn test_take_empty() {
    let value_cell = Cell::new_empty::<~int>();
    value_cell.take();
}

#[test]
#[should_fail]
#[ignore(cfg(windows))]
fn test_put_back_non_empty() {
    let value_cell = Cell::new(~10);
    value_cell.put_back(~20);
}

#[test]
fn test_with_ref() {
    let good = 6;
    let c = Cell::new(~[1, 2, 3, 4, 5, 6]);
    let l = do c.with_ref() |v| { v.len() };
    assert_eq!(l, good);
}

#[test]
fn test_with_mut_ref() {
    let good = ~[1, 2, 3];
    let v = ~[1, 2];
    let c = Cell::new(v);
    do c.with_mut_ref() |v| { v.push(3); }
    let v = c.take();
    assert_eq!(v, good);
}
