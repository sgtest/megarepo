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

trait Foo {}
impl Foo for int {}
fn foo(_: [&Foo, ..2]) {}
fn foos(_: &[&Foo]) {}
fn foog<T>(_: &[T], _: &[T]) {}

fn bar(_: [Box<Foo>, ..2]) {}
fn bars(_: &[Box<Foo>]) {}

fn main() {
    let x: [&Foo, ..2] = [&1i, &2i];
    foo(x);
    foo([&1i, &2i]);

    let r = &1i;
    let x: [&Foo, ..2] = [r, ..2];
    foo(x);
    foo([&1i, ..2]);

    let x: &[&Foo] = &[&1i, &2i];
    foos(x);
    foos(&[&1i, &2i]);

    let x: &[&Foo] = &[&1i, &2i];
    let r = &1i;
    foog(x, &[r]);

    let x: [Box<Foo>, ..2] = [box 1i, box 2i];
    bar(x);
    bar([box 1i, box 2i]);

    let x: &[Box<Foo>] = &[box 1i, box 2i];
    bars(x);
    bars(&[box 1i, box 2i]);

    let x: &[Box<Foo>] = &[box 1i, box 2i];
    foog(x, &[box 1i]);

    struct T<'a> {
        t: [&'a Foo, ..2]
    }
    let _n = T {
        t: [&1i, &2i]
    };
    let r = &1i;
    let _n = T {
        t: [r, ..2]
    };
    let x: [&Foo, ..2] = [&1i, &2i];
    let _n = T {
        t: x
    };

    struct F<'b> {
        t: &'b [&'b Foo]
    }
    let _n = F {
        t: &[&1i, &2i]
    };
    let r = &1i;
    let r: [&Foo, ..2] = [r, ..2];
    let _n = F {
        t: r
    };
    let x: [&Foo, ..2] = [&1i, &2i];
    let _n = F {
        t: x
    };

    struct M<'a> {
        t: &'a [Box<Foo>]
    }
    let _n = M {
        t: &[box 1i, box 2i]
    };
    let x: [Box<Foo>, ..2] = [box 1i, box 2i];
    let _n = M {
        t: x
    };
}
