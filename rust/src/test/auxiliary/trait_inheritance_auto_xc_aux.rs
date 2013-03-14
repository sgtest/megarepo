// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

trait Foo { fn f(&self) -> int; }
trait Bar { fn g(&self) -> int; }
trait Baz { fn h(&self) -> int; }

trait Quux: Foo + Bar + Baz { }

impl<T:Foo + Bar + Baz> Quux for T { }
