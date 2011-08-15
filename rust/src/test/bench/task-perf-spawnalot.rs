use std;
import std::ivec;
import std::task;
import std::uint;
import std::str;

fn f(n: uint) {
    let i = 0u;
    while i < n {
        task::join_id(task::_spawn(bind g()));
        i += 1u;
    }
}

fn g() {}

fn main(args: vec[str]) {

    let iargs = ivec::from_vec(args);
    let n = if ivec::len(iargs) < 2u {
        10u
    } else {
        uint::parse_buf(str::bytes(iargs.(1)), 10u)
    };
    let i = 0u;
    while i < n {
        task::_spawn(bind f(n));
        i += 1u;
    }
}