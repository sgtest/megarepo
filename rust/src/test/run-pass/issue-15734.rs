// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// If `Index` used an associated type for its output, this test would
// work more smoothly.
#![feature(old_orphan_check)]

use std::ops::Index;

struct Mat<T> { data: Vec<T>, cols: uint, }

impl<T> Mat<T> {
    fn new(data: Vec<T>, cols: uint) -> Mat<T> {
        Mat { data: data, cols: cols }
    }
    fn row<'a>(&'a self, row: uint) -> Row<&'a Mat<T>> {
        Row { mat: self, row: row, }
    }
}

impl<T> Index<(uint, uint), T> for Mat<T> {
    fn index<'a>(&'a self, &(row, col): &(uint, uint)) -> &'a T {
        &self.data[row * self.cols + col]
    }
}

impl<'a, T> Index<(uint, uint), T> for &'a Mat<T> {
    fn index<'b>(&'b self, index: &(uint, uint)) -> &'b T {
        (*self).index(index)
    }
}

struct Row<M> { mat: M, row: uint, }

impl<T, M: Index<(uint, uint), T>> Index<uint, T> for Row<M> {
    fn index<'a>(&'a self, col: &uint) -> &'a T {
        &self.mat[(self.row, *col)]
    }
}

fn main() {
    let m = Mat::new(vec!(1u, 2, 3, 4, 5, 6), 3);
    let r = m.row(1);

    assert!(r.index(&2) == &6);
    assert!(r[2] == 6);
    assert!(r[2u] == 6u);
    assert!(6 == r[2]);

    let e = r[2];
    assert!(e == 6);

    let e: uint = r[2];
    assert!(e == 6);
}
