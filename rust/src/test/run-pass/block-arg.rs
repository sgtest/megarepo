// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Check usage and precedence of block arguments in expressions:
pub fn main() {
    let v = ~[-1f, 0f, 1f, 2f, 3f];

    // Statement form does not require parentheses:
    for vec::each(v) |i| {
        info!("%?", *i);
    }

    // Usable at all:
    let mut any_negative = do vec::any(v) |e| { float::is_negative(*e) };
    assert!(any_negative);

    // Higher precedence than assignments:
    any_negative = do vec::any(v) |e| { float::is_negative(*e) };
    assert!(any_negative);

    // Higher precedence than unary operations:
    let abs_v = do vec::map(v) |e| { float::abs(*e) };
    assert!(do vec::all(abs_v) |e| { float::is_nonnegative(*e) });
    assert!(!do vec::any(abs_v) |e| { float::is_negative(*e) });

    // Usable in funny statement-like forms:
    if !do vec::any(v) |e| { float::is_positive(*e) } {
        assert!(false);
    }
    match do vec::all(v) |e| { float::is_negative(*e) } {
        true => { fail!(~"incorrect answer."); }
        false => { }
    }
    match 3 {
      _ if do vec::any(v) |e| { float::is_negative(*e) } => {
      }
      _ => {
        fail!(~"wrong answer.");
      }
    }


    // Lower precedence than binary operations:
    let w = do vec::foldl(0f, v) |x, y| { x + *y } + 10f;
    let y = do vec::foldl(0f, v) |x, y| { x + *y } + 10f;
    let z = 10f + do vec::foldl(0f, v) |x, y| { x + *y };
    assert!(w == y);
    assert!(y == z);

    // In the tail of a block
    let w =
        if true { do vec::any(abs_v) |e| { float::is_nonnegative(*e) } }
      else { false };
    assert!(w);
}
