// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Macros in statement vs expression position handle backtraces differently.

macro_rules! fake_method_stmt {
     () => {
          1.fake() //~ ERROR no method named `fake` found
     }
}

macro_rules! fake_field_stmt {
     () => {
          1.fake //~ ERROR no field with that name
     }
}

macro_rules! fake_anon_field_stmt {
     () => {
          (1).0 //~ ERROR type was not a tuple
     }
}

macro_rules! fake_method_expr {
     () => {
          1.fake() //~ ERROR no method named `fake` found
     }
}

macro_rules! fake_field_expr {
     () => {
          1.fake //~ ERROR no field with that name
     }
}

macro_rules! fake_anon_field_expr {
     () => {
          (1).0 //~ ERROR type was not a tuple
     }
}

fn main() {
    fake_method_stmt!(); //~ NOTE in this expansion of
    fake_field_stmt!(); //~ NOTE in this expansion of
    fake_anon_field_stmt!(); //~ NOTE in this expansion of

    let _ = fake_method_expr!(); //~ NOTE in this expansion of
    let _ = fake_field_expr!(); //~ NOTE in this expansion of
    let _ = fake_anon_field_expr!(); //~ NOTE in this expansion of
}
