// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-win32 Broken because of LLVM bug: http://llvm.org/bugs/show_bug.cgi?id=16249

// GDB doesn't know about UTF-32 character encoding and will print a rust char as only its numerical
// value.

// compile-flags:-Z extra-debug-info
// debugger:break zzz
// debugger:run
// debugger:finish

// debugger:print *stack_val_ref
// check:$1 = {-14, -19}

// debugger:print *ref_to_unnamed
// check:$2 = {-15, -20}

// debugger:print *managed_val_ref
// check:$3 = {-16, -21}

// debugger:print *unique_val_ref
// check:$4 = {-17, -22}

fn main() {
    let stack_val: (i16, f32) = (-14, -19f32);
    let stack_val_ref : &(i16, f32) = &stack_val;
    let ref_to_unnamed : &(i16, f32) = &(-15, -20f32);

    let managed_val : @(i16, f32) = @(-16, -21f32);
    let managed_val_ref : &(i16, f32) = managed_val;

    let unique_val: ~(i16, f32) = ~(-17, -22f32);
    let unique_val_ref : &(i16, f32) = unique_val;

    zzz();
}

fn zzz() {()}