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

E0511: r##"
Invalid monomorphization of an intrinsic function was used. Erroneous code
example:

```ignore (error-emitted-at-codegen-which-cannot-be-handled-by-compile_fail)
#![feature(platform_intrinsics)]

extern "platform-intrinsic" {
    fn simd_add<T>(a: T, b: T) -> T;
}

fn main() {
    unsafe { simd_add(0, 1); }
    // error: invalid monomorphization of `simd_add` intrinsic
}
```

The generic type has to be a SIMD type. Example:

```
#![feature(repr_simd)]
#![feature(platform_intrinsics)]

#[repr(simd)]
#[derive(Copy, Clone)]
struct i32x2(i32, i32);

extern "platform-intrinsic" {
    fn simd_add<T>(a: T, b: T) -> T;
}

unsafe { simd_add(i32x2(0, 0), i32x2(1, 2)); } // ok!
```
"##,

E0668: r##"
Malformed inline assembly rejected by LLVM.

LLVM checks the validity of the constraints and the assembly string passed to
it. This error implies that LLVM seems something wrong with the inline
assembly call.

In particular, it can happen if you forgot the closing bracket of a register
constraint (see issue #51430):
```ignore (error-emitted-at-codegen-which-cannot-be-handled-by-compile_fail)
#![feature(asm)]

fn main() {
    let rax: u64;
    unsafe {
        asm!("" :"={rax"(rax));
        println!("Accumulator is: {}", rax);
    }
}
```
"##,

}
