// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Functions for the unit type.

*/

#[cfg(notest)]
use cmp::{Eq, Ord, TotalOrd, Ordering, Equal};

#[cfg(notest)]
impl Eq for () {
    #[inline(always)]
    fn eq(&self, _other: &()) -> bool { true }
    #[inline(always)]
    fn ne(&self, _other: &()) -> bool { false }
}

#[cfg(notest)]
impl Ord for () {
    #[inline(always)]
    fn lt(&self, _other: &()) -> bool { false }
    #[inline(always)]
    fn le(&self, _other: &()) -> bool { true }
    #[inline(always)]
    fn ge(&self, _other: &()) -> bool { true }
    #[inline(always)]
    fn gt(&self, _other: &()) -> bool { false }
}

#[cfg(notest)]
impl TotalOrd for () {
    #[inline(always)]
    fn cmp(&self, _other: &()) -> Ordering { Equal }
}
