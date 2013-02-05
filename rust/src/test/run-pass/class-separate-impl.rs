// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// xfail-fast
use core::to_str::*;

struct cat {
  priv mut meows : uint,

  mut how_hungry : int,
  name : ~str,
}

impl cat {

  fn speak() { self.meow(); }

  fn eat() -> bool {
    if self.how_hungry > 0 {
        error!("OM NOM NOM");
        self.how_hungry -= 2;
        return true;
    }
    else {
        error!("Not hungry!");
        return false;
    }
  }
}

priv impl cat {
    fn meow() {
      error!("Meow");
      self.meows += 1u;
      if self.meows % 5u == 0u {
          self.how_hungry += 1;
      }
    }
}

fn cat(in_x : uint, in_y : int, in_name: ~str) -> cat {
    cat {
        meows: in_x,
        how_hungry: in_y,
        name: in_name
    }
}

impl cat: ToStr {
  pure fn to_str(&self) -> ~str { copy self.name }
}

fn print_out<T: ToStr>(thing: T, expected: ~str) {
  let actual = thing.to_str();
  debug!("%s", actual);
  assert(actual == expected);
}

pub fn main() {
  let nyan : ToStr = cat(0u, 2, ~"nyan") as ToStr;
  print_out(nyan, ~"nyan");
}
