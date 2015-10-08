// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_snake_case)]

register_long_diagnostics! {

E0515: r##"
A constant index expression was out of bounds. Erroneous code example:

```
let x = &[0, 1, 2][7]; // error: const index-expr is out of bounds
```

Please specify a valid index (not inferior to 0 or superior to array length).
Example:

```
let x = &[0, 1, 2][2]; // ok!
```
"##,

}

register_diagnostics! {
    E0510, // invalid use of `return_address` intrinsic: function does not use out pointer
    E0511, // invalid monomorphization of `{}` intrinsic
    E0512, // transmute called on types with potentially different sizes...
}
