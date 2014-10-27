// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(dead_code)]
#![feature(unboxed_closures, unboxed_closure_sugar)]

// compile-flags:-g

fn foo<T>() {}

trait Bar1 {}
impl Bar1 for proc():'static {}

trait Bar2 {}
impl Bar2 for proc():Send {}

trait Bar3 {}
impl<'b> Bar3 for <'a>|&'a int|: 'b + Send -> &'a int {}

trait Bar4 {}
impl Bar4 for proc<'a>(&'a int):'static -> &'a int {}

struct Foo<'a> {
    a: ||: 'a,
    b: ||: 'static,
    c: <'b>||: 'a,
    d: ||: 'a + Sync,
    e: <'b>|int|: 'a + Sync -> &'b f32,
    f: proc():'static,
    g: proc():'static+Sync,
    h: proc<'b>(int):'static+Sync -> &'b f32,
}

fn f<'a>(a: &'a int, f: <'b>|&'b int| -> &'b int) -> &'a int {
    f(a)
}

fn g<'a>(a: &'a int, f: proc<'b>(&'b int) -> &'b int) -> &'a int {
    f(a)
}

struct A;

impl A {
    fn foo<T>(&self) {}
}

fn bar<'b>() {
    foo::<||>();
    foo::<|| -> ()>();
    foo::<||:>();
    foo::<||:'b>();
    foo::<||:'b + Sync>();
    foo::<||:Sync>();
    foo::< <'a>|int, f32, &'a int|:'b + Sync -> &'a int>();
    foo::<proc()>();
    foo::<proc() -> ()>();
    foo::<proc():'static>();
    foo::<proc():Sync>();
    foo::<proc<'a>(int, f32, &'a int):'static + Sync -> &'a int>();

    foo::<<'a>||>();

    // issue #11209
    let _: ||: 'b; // for comparison
    let _: <'a> ||;

    let _: Option<||:'b>;
    let _: Option<<'a>||>;
    let _: Option< <'a>||>;

    // issue #11210
    let _: ||: 'static;

    let a = A;
    a.foo::<<'a>||>();

    // issue #13490
    let _ = || -> ! loop {};
    let _ = proc() -> ! loop {};

    // issue #17021
    let c = box |&:| {};
}

struct B<T>;
impl<'b> B<<'a>||: 'b> {}

pub fn main() {
}
