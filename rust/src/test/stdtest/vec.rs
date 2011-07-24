
use std;

import std::vec::*;
import std::option;

#[test]
fn test_init_elt() {
    let vec[uint] v = init_elt[uint](5u, 3u);
    assert (len[uint](v) == 3u);
    assert (v.(0) == 5u);
    assert (v.(1) == 5u);
    assert (v.(2) == 5u);
}

fn id(uint x) -> uint { ret x; }

#[test]
fn test_init_fn() {
    let fn(uint) -> uint  op = id;
    let vec[uint] v = init_fn[uint](op, 5u);
    assert (len[uint](v) == 5u);
    assert (v.(0) == 0u);
    assert (v.(1) == 1u);
    assert (v.(2) == 2u);
    assert (v.(3) == 3u);
    assert (v.(4) == 4u);
}

#[test]
fn test_slice() {
    let vec[int] v = [1, 2, 3, 4, 5];
    auto v2 = slice[int](v, 2u, 4u);
    assert (len[int](v2) == 2u);
    assert (v2.(0) == 3);
    assert (v2.(1) == 4);
}

#[test]
fn test_map() {
    fn square(&int x) -> int { ret x * x; }
    let option::operator[int, int] op = square;
    let vec[int] v = [1, 2, 3, 4, 5];
    let vec[int] s = map[int, int](op, v);
    let int i = 0;
    while (i < 5) { assert (v.(i) * v.(i) == s.(i)); i += 1; }
}

#[test]
fn test_map2() {
    fn times(&int x, &int y) -> int { ret x * y; }
    auto f = times;
    auto v0 = [1, 2, 3, 4, 5];
    auto v1 = [5, 4, 3, 2, 1];
    auto u = map2[int, int, int](f, v0, v1);
    auto i = 0;
    while (i < 5) { assert (v0.(i) * v1.(i) == u.(i)); i += 1; }
}

#[test]
fn test_filter_map() {
    fn halve(&int i) -> option::t[int] {
        if (i % 2 == 0) {
            ret option::some[int](i / 2);
        } else { ret option::none[int]; }
    }
    fn halve_for_sure(&int i) -> int { ret i / 2; }
    let vec[int] all_even = [0, 2, 8, 6];
    let vec[int] all_odd1 = [1, 7, 3];
    let vec[int] all_odd2 = [];
    let vec[int] mix = [9, 2, 6, 7, 1, 0, 0, 3];
    let vec[int] mix_dest = [1, 3, 0, 0];
    assert (filter_map(halve, all_even) ==
                map(halve_for_sure, all_even));
    assert (filter_map(halve, all_odd1) == empty[int]());
    assert (filter_map(halve, all_odd2) == empty[int]());
    assert (filter_map(halve, mix) == mix_dest);
}

#[test]
fn test_position() {
  let vec[int] v1 = [1, 2, 3, 3, 2, 5];
  assert (position(1, v1) == option::some[uint](0u));
  assert (position(2, v1) == option::some[uint](1u));
  assert (position(5, v1) == option::some[uint](5u));
  assert (position(4, v1) == option::none[uint]);
}

#[test]
fn test_position_pred() {
  fn less_than_three(&int i) -> bool {
    ret i <3;
  }
  fn is_eighteen(&int i) -> bool {
    ret i == 18;
  }
  let vec[int] v1 = [5, 4, 3, 2, 1];
  assert (position_pred(less_than_three, v1) == option::some[uint](3u));
  assert (position_pred(is_eighteen, v1) == option::none[uint]);
}
