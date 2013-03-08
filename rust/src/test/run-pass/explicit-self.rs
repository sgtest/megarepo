// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


const tau: float = 2.0*3.14159265358979323;

struct Point {x: float, y: float}
struct Size {w: float, h: float}
enum shape {
    circle(Point, float),
    rectangle(Point, Size)
}


fn compute_area(shape: &shape) -> float {
    match *shape {
        circle(_, radius) => 0.5 * tau * radius * radius,
        rectangle(_, ref size) => size.w * size.h
    }
}

pub impl shape {
    // self is in the implicit self region
    fn select<T>(&self, threshold: float,
                 a: &r/T, b: &r/T) -> &r/T {
        if compute_area(self) > threshold {a} else {b}
    }
}

fn select_based_on_unit_circle<T>(
    threshold: float, a: &r/T, b: &r/T) -> &r/T {

    let shape = &circle(Point{x: 0.0, y: 0.0}, 1.0);
    shape.select(threshold, a, b)
}


struct thing {
    x: A
}

struct A { a: @int }

fn thing(x: A) -> thing {
    thing {
        x: copy x
    }
}

pub impl thing {
    fn foo(@self) -> int { *self.x.a }
    fn bar(~self) -> int { *self.x.a }
    fn quux(&self) -> int { *self.x.a }
    fn baz(&self) -> &self/A { &self.x }
    fn spam(self) -> int { *self.x.a }
}

trait Nus { fn f(&self); }
impl Nus for thing { fn f(&self) {} }

pub fn main() {

    let x = @thing(A {a: @10});
    fail_unless!(x.foo() == 10);
    fail_unless!(x.quux() == 10);

    let y = ~thing(A {a: @10});
    fail_unless!((copy y).bar() == 10);
    fail_unless!(y.quux() == 10);

    let z = thing(A {a: @11});
    fail_unless!(z.spam() == 11);
}
