// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

struct cat<U> {
    priv info : ~[U],
    priv meows : uint,

    how_hungry : int,
}

pub impl<U> cat<U> {
    fn speak<T>(&mut self, stuff: ~[T]) {
        self.meows += stuff.len();
    }
    fn meow_count(&mut self) -> uint { self.meows }
}

fn cat<U>(in_x : uint, in_y : int, +in_info: ~[U]) -> cat<U> {
    cat {
        meows: in_x,
        how_hungry: in_y,
        info: in_info
    }
}

pub fn main() {
  let mut nyan : cat<int> = cat::<int>(52u, 99, ~[9]);
  let mut kitty = cat(1000u, 2, ~[~"tabby"]);
  assert!((nyan.how_hungry == 99));
  assert!((kitty.how_hungry == 2));
  nyan.speak(~[1,2,3]);
  assert!((nyan.meow_count() == 55u));
  kitty.speak(~[~"meow", ~"mew", ~"purr", ~"chirp"]);
  assert!((kitty.meow_count() == 1004u));
}
