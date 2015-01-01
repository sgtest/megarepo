// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Test how resolving a projection interacts with inference.  In this
// case, we were eagerly unifying the type variable for the iterator
// type with `I` from the where clause, ignoring the in-scope `impl`
// for `ByRef`. The right answer was to consider the result ambiguous
// until more type information was available.

// ignore-pretty -- FIXME(#17362) pretty prints with `<<` which lexes wrong

#![feature(associated_types, lang_items, unboxed_closures)]
#![no_implicit_prelude]

use std::option::Option::{None, Some, mod};

trait Iterator {
    type Item;

    fn next(&mut self) -> Option<Self::Item>;
}

trait IteratorExt: Iterator {
    fn by_ref(&mut self) -> ByRef<Self> {
        ByRef(self)
    }
}

impl<I> IteratorExt for I where I: Iterator {}

struct ByRef<'a, I: 'a + Iterator>(&'a mut I);

impl<'a, A, I> Iterator for ByRef<'a, I> where I: Iterator<Item=A> {
    type Item = A;

    fn next(&mut self) -> Option< <I as Iterator>::Item > {
        self.0.next()
    }
}

fn is_iterator_of<A, I: Iterator<Item=A>>(_: &I) {}

fn test<A, I: Iterator<Item=A>>(mut it: I) {
    is_iterator_of::<A, _>(&it.by_ref());
}

fn main() { }
