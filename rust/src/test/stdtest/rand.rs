import core::*;

// -*- rust -*-
use std;
import std::rand;
import str;

#[test]
fn test() {
    let r1: rand::rng = rand::mk_rng();
    log_full(core::debug, r1.next());
    log_full(core::debug, r1.next());
    {
        let r2 = rand::mk_rng();
        log_full(core::debug, r1.next());
        log_full(core::debug, r2.next());
        log_full(core::debug, r1.next());
        log_full(core::debug, r1.next());
        log_full(core::debug, r2.next());
        log_full(core::debug, r2.next());
        log_full(core::debug, r1.next());
        log_full(core::debug, r1.next());
        log_full(core::debug, r1.next());
        log_full(core::debug, r2.next());
        log_full(core::debug, r2.next());
        log_full(core::debug, r2.next());
    }
    log_full(core::debug, r1.next());
    log_full(core::debug, r1.next());
}

#[test]
fn genstr() {
    let r: rand::rng = rand::mk_rng();
    log_full(core::debug, r.gen_str(10u));
    log_full(core::debug, r.gen_str(10u));
    log_full(core::debug, r.gen_str(10u));
    assert(str::char_len(r.gen_str(10u)) == 10u);
    assert(str::char_len(r.gen_str(16u)) == 16u);
}
