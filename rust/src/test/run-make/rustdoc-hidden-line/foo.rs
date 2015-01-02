// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![crate_name="foo"]

/// The '# ' lines should be removed from the output, but the #[derive] should be
/// retained.
///
/// ```rust
/// mod to_make_deriving_work { // FIXME #4913
///
/// # #[derive(PartialEq)] // invisible
/// # struct Foo; // invisible
///
/// #[derive(PartialEq)] // Bar
/// struct Bar(Foo);
///
/// fn test() {
///     let x = Bar(Foo);
///     assert!(x == x); // check that the derivings worked
/// }
///
/// }
/// ```
pub fn foo() {}
