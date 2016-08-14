// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Regression test for issue #3645

fn main() {
    let n = 1;
    let a = [0; n];
    //~^ ERROR constant evaluation error
    //~| non-constant path in constant expression
    let b = [0; ()];
    //~^ ERROR mismatched types
    //~| expected type `usize`
    //~| found type `()`
    //~| expected usize, found ()
    //~| ERROR expected `usize` for repeat count, found tuple [E0306]
    //~| expected `usize`
    let c = [0; true];
    //~^ ERROR mismatched types
    //~| expected usize, found bool
    //~| ERROR expected `usize` for repeat count, found boolean [E0306]
    //~| expected `usize`
    let d = [0; 0.5];
    //~^ ERROR mismatched types
    //~| expected type `usize`
    //~| found type `{float}`
    //~| expected usize, found floating-point variable
    //~| ERROR expected `usize` for repeat count, found float [E0306]
    //~| expected `usize`
    let e = [0; "foo"];
    //~^ ERROR mismatched types
    //~| expected type `usize`
    //~| found type `&'static str`
    //~| expected usize, found reference
    //~| ERROR expected `usize` for repeat count, found string literal [E0306]
    //~| expected `usize`
    let f = [0; -4_isize];
    //~^ ERROR constant evaluation error
    //~| expected usize, found isize
    //~| ERROR mismatched types
    //~| expected usize, found isize
    let f = [0_usize; -1_isize];
    //~^ ERROR constant evaluation error
    //~| expected usize, found isize
    //~| ERROR mismatched types
    //~| expected usize, found isize
    struct G {
        g: (),
    }
    let g = [0; G { g: () }];
    //~^ ERROR mismatched types
    //~| expected type `usize`
    //~| found type `main::G`
    //~| expected usize, found struct `main::G`
    //~| ERROR expected `usize` for repeat count, found struct [E0306]
    //~| expected `usize`
}
