// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern mod std;
use std::rand;

// random uint less than n
fn under(r : rand::rng, n : uint) -> uint {
    fail_unless!(n != 0u); r.next() as uint % n
}

// random choice from a vec
fn choice<T:copy>(r : rand::rng, v : ~[T]) -> T {
    fail_unless!(vec::len(v) != 0u); v[under(r, vec::len(v))]
}

// 1 in n chance of being true
fn unlikely(r : rand::rng, n : uint) -> bool { under(r, n) == 0u }

// shuffle a vec in place
fn shuffle<T>(r : rand::rng, &v : ~[T]) {
    let i = vec::len(v);
    while i >= 2u {
        // Loop invariant: elements with index >= i have been locked in place.
        i -= 1u;
        vec::swap(v, i, under(r, i + 1u)); // Lock element i in place.
    }
}

// create a shuffled copy of a vec
fn shuffled<T:copy>(r : rand::rng, v : ~[T]) -> ~[T] {
    let w = vec::to_mut(v);
    shuffle(r, w);
    vec::from_mut(w) // Shouldn't this happen automatically?
}

// sample from a population without replacement
//fn sample<T>(r : rand::rng, pop : ~[T], k : uint) -> ~[T] { fail!() }

// Two ways to make a weighted choice.
// * weighted_choice is O(number of choices) time
// * weighted_vec is O(total weight) space
type weighted<T> = { weight: uint, item: T };
fn weighted_choice<T:copy>(r : rand::rng, v : ~[weighted<T>]) -> T {
    fail_unless!(vec::len(v) != 0u);
    let total = 0u;
    for {weight: weight, item: _} in v {
        total += weight;
    }
    fail_unless!(total >= 0u);
    let chosen = under(r, total);
    let so_far = 0u;
    for {weight: weight, item: item} in v {
        so_far += weight;
        if so_far > chosen {
            return item;
        }
    }
    core::unreachable();
}

fn weighted_vec<T:copy>(v : ~[weighted<T>]) -> ~[T] {
    let r = ~[];
    for {weight: weight, item: item} in v {
        let i = 0u;
        while i < weight {
            r.push(item);
            i += 1u;
        }
    }
    r
}

fn main()
{
    let r = rand::mk_rng();

    log(error, under(r, 5u));
    log(error, choice(r, ~[10, 20, 30]));
    log(error, if unlikely(r, 5u) { "unlikely" } else { "likely" });

    let mut a = ~[1, 2, 3];
    shuffle(r, a);
    log(error, a);

    let i = 0u;
    let v = ~[
        {weight:1u, item:"low"},
        {weight:8u, item:"middle"},
        {weight:1u, item:"high"}
    ];
    let w = weighted_vec(v);

    while i < 1000u {
        log(error, "Immed: " + weighted_choice(r, v));
        log(error, "Fast:  " + choice(r, w));
        i += 1u;
    }
}
