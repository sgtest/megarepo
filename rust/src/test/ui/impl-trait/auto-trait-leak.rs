// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-tidy-linelength

#![feature(conservative_impl_trait)]

use std::cell::Cell;
use std::rc::Rc;

// Fast path, main can see the concrete type returned.
fn before() -> impl Fn(i32) {
    let p = Rc::new(Cell::new(0));
    move |x| p.set(x)
}

fn send<T: Send>(_: T) {}

fn main() {
    send(before());
    //~^ ERROR the trait bound `std::rc::Rc<std::cell::Cell<i32>>: std::marker::Send` is not satisfied
    //~| NOTE `std::rc::Rc<std::cell::Cell<i32>>` cannot be sent between threads safely
    //~| NOTE required because it appears within the type `[closure
    //~| NOTE required because it appears within the type `impl std::ops::Fn<(i32,)>`
    //~| NOTE required by `send`

    send(after());
    //~^ ERROR the trait bound `std::rc::Rc<std::cell::Cell<i32>>: std::marker::Send` is not satisfied
    //~| NOTE `std::rc::Rc<std::cell::Cell<i32>>` cannot be sent between threads safely
    //~| NOTE required because it appears within the type `[closure
    //~| NOTE required because it appears within the type `impl std::ops::Fn<(i32,)>`
    //~| NOTE required by `send`
}

// Deferred path, main has to wait until typeck finishes,
// to check if the return type of after is Send.
fn after() -> impl Fn(i32) {
    let p = Rc::new(Cell::new(0));
    move |x| p.set(x)
}

// Cycles should work as the deferred obligations are
// independently resolved and only require the concrete
// return type, which can't depend on the obligation.
fn cycle1() -> impl Clone {
    //~^ ERROR unsupported cyclic reference between types/traits detected
    //~| cyclic reference
    //~| NOTE the cycle begins when processing `cycle1`...
    //~| NOTE ...which then requires processing `cycle1::{{impl-Trait}}`...
    //~| NOTE ...which then again requires processing `cycle1`, completing the cycle.
    send(cycle2().clone());

    Rc::new(Cell::new(5))
}

fn cycle2() -> impl Clone {
    //~^ NOTE ...which then requires processing `cycle2::{{impl-Trait}}`...
    //~| NOTE ...which then requires processing `cycle2`...
    send(cycle1().clone());

    Rc::new(String::from("foo"))
}
