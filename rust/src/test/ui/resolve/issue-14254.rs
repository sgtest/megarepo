// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

trait Foo {
    fn bar(&self);
    fn baz(&self) { }
    fn bah(_: Option<&Self>) { }
}

struct BarTy {
    x : isize,
    y : f64,
}

impl BarTy {
    fn a() {}
    fn b(&self) {}
}

impl Foo for *const BarTy {
    fn bar(&self) {
        baz();
        //~^ ERROR cannot find function `baz`
        a;
        //~^ ERROR cannot find value `a`
        //~| NOTE not found in this scope
    }
}

impl<'a> Foo for &'a BarTy {
    fn bar(&self) {
        baz();
        //~^ ERROR cannot find function `baz`
        x;
        //~^ ERROR cannot find value `x`
        y;
        //~^ ERROR cannot find value `y`
        a;
        //~^ ERROR cannot find value `a`
        //~| NOTE not found in this scope
        bah;
        //~^ ERROR cannot find value `bah`
        b;
        //~^ ERROR cannot find value `b`
        //~| NOTE not found in this scope
    }
}

impl<'a> Foo for &'a mut BarTy {
    fn bar(&self) {
        baz();
        //~^ ERROR cannot find function `baz`
        x;
        //~^ ERROR cannot find value `x`
        y;
        //~^ ERROR cannot find value `y`
        a;
        //~^ ERROR cannot find value `a`
        //~| NOTE not found in this scope
        bah;
        //~^ ERROR cannot find value `bah`
        b;
        //~^ ERROR cannot find value `b`
        //~| NOTE not found in this scope
    }
}

impl Foo for Box<BarTy> {
    fn bar(&self) {
        baz();
        //~^ ERROR cannot find function `baz`
        bah;
        //~^ ERROR cannot find value `bah`
    }
}

impl Foo for *const isize {
    fn bar(&self) {
        baz();
        //~^ ERROR cannot find function `baz`
        bah;
        //~^ ERROR cannot find value `bah`
    }
}

impl<'a> Foo for &'a isize {
    fn bar(&self) {
        baz();
        //~^ ERROR cannot find function `baz`
        bah;
        //~^ ERROR cannot find value `bah`
    }
}

impl<'a> Foo for &'a mut isize {
    fn bar(&self) {
        baz();
        //~^ ERROR cannot find function `baz`
        bah;
        //~^ ERROR cannot find value `bah`
    }
}

impl Foo for Box<isize> {
    fn bar(&self) {
        baz();
        //~^ ERROR cannot find function `baz`
        bah;
        //~^ ERROR cannot find value `bah`
    }
}
