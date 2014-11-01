// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-tidy-linelength
// ignore-android: FIXME(#10381)
// min-lldb-version: 310

// compile-flags:-g

// === GDB TESTS ===================================================================================

// gdb-command:run

// gdb-command:print int_int
// gdb-check:$1 = {key = 0, value = 1}
// gdb-command:print int_float
// gdb-check:$2 = {key = 2, value = 3.5}
// gdb-command:print float_int
// gdb-check:$3 = {key = 4.5, value = 5}
// gdb-command:print float_int_float
// gdb-check:$4 = {key = 6.5, value = {key = 7, value = 8.5}}

// === LLDB TESTS ==================================================================================

// lldb-command:run

// lldb-command:print int_int
// lldb-check:[...]$0 = AGenericStruct<int, int> { key: 0, value: 1 }
// lldb-command:print int_float
// lldb-check:[...]$1 = AGenericStruct<int, f64> { key: 2, value: 3.5 }
// lldb-command:print float_int
// lldb-check:[...]$2 = AGenericStruct<f64, int> { key: 4.5, value: 5 }

// lldb-command:print float_int_float
// lldb-check:[...]$3 = AGenericStruct<f64, generic-struct::AGenericStruct<int, f64>> { key: 6.5, value: AGenericStruct<int, f64> { key: 7, value: 8.5 } }

struct AGenericStruct<TKey, TValue> {
    key: TKey,
    value: TValue
}

fn main() {

    let int_int = AGenericStruct { key: 0i, value: 1i };
    let int_float = AGenericStruct { key: 2i, value: 3.5f64 };
    let float_int = AGenericStruct { key: 4.5f64, value: 5i };
    let float_int_float = AGenericStruct {
        key: 6.5f64,
        value: AGenericStruct { key: 7i, value: 8.5f64 },
    };

    zzz(); // #break
}

fn zzz() { () }
