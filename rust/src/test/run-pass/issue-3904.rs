// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

type ErrPrinter<'a> = |&str, &str|: 'a;

fn example_err(prog: &str, arg: &str) {
    println!("{}: {}", prog, arg)
}

fn exit(print: ErrPrinter, prog: &str, arg: &str) {
    print(prog, arg);
}

struct X<'a> {
    err: ErrPrinter<'a>
}

impl<'a> X<'a> {
    pub fn boom(self) {
        exit(self.err, "prog", "arg");
    }
}

pub fn main(){
    let val = X {
        err: example_err,
    };
    val.boom();
}
