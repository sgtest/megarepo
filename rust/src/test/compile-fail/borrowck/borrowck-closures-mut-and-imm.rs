// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Tests that two closures cannot simultaneously have mutable
// and immutable access to the variable. Issue #6801.

// ignore-tidy-linelength
// revisions: ast mir
//[mir]compile-flags: -Z emit-end-regions -Z borrowck-mir

#![feature(box_syntax)]

fn get(x: &isize) -> isize {
    *x
}

fn set(x: &mut isize) {
    *x = 4;
}

fn a() {
    let mut x = 3;
    let c1 = || x = 4;
    let c2 = || x * 5; //[ast]~ ERROR cannot borrow `x`
                       //[mir]~^ ERROR cannot borrow `x` as immutable because it is also borrowed as mutable (Ast)
                       //[mir]~| ERROR cannot borrow `x` as immutable because it is also borrowed as mutable (Mir)
}

fn b() {
    let mut x = 3;
    let c1 = || set(&mut x);
    let c2 = || get(&x); //[ast]~ ERROR cannot borrow `x`
                         //[mir]~^ ERROR cannot borrow `x` as immutable because it is also borrowed as mutable (Ast)
                         //[mir]~| ERROR cannot borrow `x` as immutable because it is also borrowed as mutable (Mir)
}

fn c() {
    let mut x = 3;
    let c1 = || set(&mut x);
    let c2 = || x * 5; //[ast]~ ERROR cannot borrow `x`
                       //[mir]~^ ERROR cannot borrow `x` as immutable because it is also borrowed as mutable (Ast)
                       //[mir]~| ERROR cannot borrow `x` as immutable because it is also borrowed as mutable (Mir)
}

fn d() {
    let mut x = 3;
    let c2 = || x * 5;
    x = 5; //[ast]~ ERROR cannot assign
           //[mir]~^ ERROR cannot assign to `x` because it is borrowed (Ast)
           //[mir]~| ERROR cannot assign to `x` because it is borrowed (Mir)
}

fn e() {
    let mut x = 3;
    let c1 = || get(&x);
    x = 5; //[ast]~ ERROR cannot assign
           //[mir]~^ ERROR cannot assign to `x` because it is borrowed (Ast)
           //[mir]~| ERROR cannot assign to `x` because it is borrowed (Mir)
}

fn f() {
    let mut x: Box<_> = box 3;
    let c1 = || get(&*x);
    *x = 5; //[ast]~ ERROR cannot assign
            //[mir]~^ ERROR cannot assign to `*x` because it is borrowed (Ast)
            //[mir]~| ERROR cannot assign to `*x` because it is borrowed (Mir)
}

fn g() {
    struct Foo {
        f: Box<isize>
    }

    let mut x: Box<_> = box Foo { f: box 3 };
    let c1 = || get(&*x.f);
    *x.f = 5; //[ast]~ ERROR cannot assign to `*x.f`
              //[mir]~^ ERROR cannot assign to `*x.f` because it is borrowed (Ast)
              //[mir]~| ERROR cannot assign to `*x.f` because it is borrowed (Mir)
}

fn h() {
    struct Foo {
        f: Box<isize>
    }

    let mut x: Box<_> = box Foo { f: box 3 };
    let c1 = || get(&*x.f);
    let c2 = || *x.f = 5; //[ast]~ ERROR cannot borrow `x` as mutable
                          //[mir]~^ ERROR cannot borrow `x` as mutable because it is also borrowed as immutable (Ast)
                          //[mir]~| ERROR cannot borrow `x` as mutable because it is also borrowed as immutable (Mir)
}

fn main() {
}
