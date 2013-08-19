// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::cast;
use std::libc;
use std::sys;

struct arena(());

struct Bcx<'self> {
    fcx: &'self Fcx<'self>
}

struct Fcx<'self> {
    arena: &'self arena,
    ccx: &'self Ccx
}

struct Ccx {
    x: int
}

#[fixed_stack_segment] #[inline(never)]
fn alloc<'a>(_bcx : &'a arena) -> &'a Bcx<'a> {
    unsafe {
        cast::transmute(libc::malloc(sys::size_of::<Bcx<'blk>>()
            as libc::size_t))
    }
}

fn h<'a>(bcx : &'a Bcx<'a>) -> &'a Bcx<'a> {
    return alloc(bcx.fcx.arena);
}

#[fixed_stack_segment] #[inline(never)]
fn g(fcx : &Fcx) {
    let bcx = Bcx { fcx: fcx };
    let bcx2 = h(&bcx);
    unsafe {
        libc::free(cast::transmute(bcx2));
    }
}

fn f(ccx : &Ccx) {
    let a = arena(());
    let fcx = Fcx { arena: &a, ccx: ccx };
    return g(&fcx);
}

pub fn main() {
    let ccx = Ccx { x: 0 };
    f(&ccx);
}
