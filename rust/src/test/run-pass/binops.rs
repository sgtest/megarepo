// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Binop corner cases

fn test_nil() {
    assert (() == ());
    assert (!(() != ()));
    assert (!(() < ()));
    assert (() <= ());
    assert (!(() > ()));
    assert (() >= ());
}

fn test_bool() {
    assert (!(true < false));
    assert (!(true <= false));
    assert (true > false);
    assert (true >= false);

    assert (false < true);
    assert (false <= true);
    assert (!(false > true));
    assert (!(false >= true));

    // Bools support bitwise binops
    assert (false & false == false);
    assert (true & false == false);
    assert (true & true == true);
    assert (false | false == false);
    assert (true | false == true);
    assert (true | true == true);
    assert (false ^ false == false);
    assert (true ^ false == true);
    assert (true ^ true == false);
}

fn test_char() {
    let ch10 = 10 as char;
    let ch4 = 4 as char;
    let ch2 = 2 as char;
    assert (ch10 + ch4 == 14 as char);
    assert (ch10 - ch4 == 6 as char);
    assert (ch10 * ch4 == 40 as char);
    assert (ch10 / ch4 == ch2);
    assert (ch10 % ch4 == ch2);
    assert (ch10 >> ch2 == ch2);
    assert (ch10 << ch4 == 160 as char);
    assert (ch10 | ch4 == 14 as char);
    assert (ch10 & ch2 == ch2);
    assert (ch10 ^ ch2 == 8 as char);
}

fn test_box() {
    assert (@10 == @10);
}

fn test_ptr() {
    unsafe {
        let p1: *u8 = ::core::cast::reinterpret_cast(&0);
        let p2: *u8 = ::core::cast::reinterpret_cast(&0);
        let p3: *u8 = ::core::cast::reinterpret_cast(&1);

        assert p1 == p2;
        assert p1 != p3;
        assert p1 < p3;
        assert p1 <= p3;
        assert p3 > p1;
        assert p3 >= p3;
        assert p1 <= p2;
        assert p1 >= p2;
    }
}

#[abi = "cdecl"]
#[nolink]
extern mod test {
    #[legacy_exports];
    fn rust_get_sched_id() -> libc::intptr_t;
    fn get_task_id() -> libc::intptr_t;
}

struct p {
  mut x: int,
  mut y: int,
}

fn p(x: int, y: int) -> p {
    p {
        x: x,
        y: y
    }
}

impl p : cmp::Eq {
    pure fn eq(&self, other: &p) -> bool {
        (*self).x == (*other).x && (*self).y == (*other).y
    }
    pure fn ne(&self, other: &p) -> bool { !(*self).eq(other) }
}

fn test_class() {
  let q = p(1, 2);
  let r = p(1, 2);
  
  unsafe {
  error!("q = %x, r = %x",
         (::core::cast::reinterpret_cast::<*p, uint>(&ptr::addr_of(&q))),
         (::core::cast::reinterpret_cast::<*p, uint>(&ptr::addr_of(&r))));
  }
  assert(q == r);
  r.y = 17;
  assert(r.y != q.y);
  assert(r.y == 17);
  assert(q != r);
}

fn main() {
    test_nil();
    test_bool();
    test_char();
    test_box();
    test_ptr();
    test_class();
}
