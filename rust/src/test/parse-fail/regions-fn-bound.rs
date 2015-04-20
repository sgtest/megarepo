// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// compile-flags: -Z parse-only

// ignore-test
// ignored because the first error does not show up.

// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// compile-flags: -Z parse-only

fn of<T>() -> |T| { panic!(); }
fn subtype<T>(x: |T|) { panic!(); }

fn test_fn<'x, 'y, 'z, T>(_x: &'x T, _y: &'y T, _z: &'z T) {
    // Here, x, y, and z are free.  Other letters
    // are bound.  Note that the arrangement
    // subtype::<T1>(of::<T2>()) will typecheck
    // iff T1 <: T2.

    // should be the default:
    subtype::< ||:'static>(of::<||>());
    subtype::<||>(of::< ||:'static>());

    //
    subtype::< <'x> ||>(of::<||>());    //~ ERROR mismatched types
    subtype::< <'x> ||>(of::< <'y> ||>());  //~ ERROR mismatched types

    subtype::< <'x> ||>(of::< ||:'static>()); //~ ERROR mismatched types
    subtype::< ||:'static>(of::< <'x> ||>());

}

fn main() {}
