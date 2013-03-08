// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Just a grab bag of stuff that you wouldn't want to actually write.

fn strange() -> bool { let _x: bool = return true; }

fn funny() {
    fn f(_x: ()) { }
    f(return);
}

fn what() {
    fn the(x: @mut bool) { return while !*x { *x = true; }; }
    let i = @mut false;
    let dont = {||the(i)};
    dont();
    fail_unless!((*i));
}

fn zombiejesus() {
    loop {
        while (return) {
            if (return) {
                match (return) {
                    1 => {
                        if (return) {
                            return
                        } else {
                            return
                        }
                    }
                    _ => { return }
                };
            } else if (return) {
                return;
            }
        }
        if (return) { break; }
    }
}

fn notsure() {
    let mut _x;
    let mut _y = (_x = 0) == (_x = 0);
    let mut _z = (_x = 0) < (_x = 0);
    let _a = (_x += 0) == (_x = 0);
    let _b = (_y <-> _z) == (_y <-> _z);
}

fn hammertime() -> int {
    let _x = log(debug, true == (return 0));
}

fn canttouchthis() -> uint {
    pure fn p() -> bool { true }
    let _a = (fail_unless!((true)) == (fail_unless!(p())));
    let _c = (fail_unless!((p())) == ());
    let _b: bool = (debug!("%d", 0) == (return 0u));
}

fn angrydome() {
    loop { if break { } }
    let mut i = 0;
    loop { i += 1; if i == 1 { match (loop) { 1 => { }, _ => fail!(~"wat") } }
      break; }
}

fn evil_lincoln() { let evil = debug!("lincoln"); }

pub fn main() {
    strange();
    funny();
    what();
    zombiejesus();
    notsure();
    hammertime();
    canttouchthis();
    angrydome();
    evil_lincoln();
}
