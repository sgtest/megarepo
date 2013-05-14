// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[allow(ctypes)];
#[allow(heap_memory)];
#[allow(implicit_copies)];
#[allow(managed_heap_memory)];
#[allow(non_camel_case_types)];
#[allow(owned_heap_memory)];
#[allow(path_statement)];
#[allow(unrecognized_lint)];
#[allow(unused_imports)];
#[allow(while_true)];
#[allow(unused_variable)];
#[allow(dead_assignment)];
#[allow(unused_unsafe)];
#[allow(unused_mut)];

extern mod std;

fn print<T>(result: T) {
    io::println(fmt!("%?", result));
}
