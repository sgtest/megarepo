

// -*- rust -*-
use std;
import std::str;


// FIXME: import std::dbg.const_refcount. Currently
// cross-crate const references don't work.
const const_refcount: uint = 0x7bad_face_u;

tag t { make_t(str); clam; }

fn foo(s: str) {
    let x: t = make_t(s); // ref up

    alt x {
      make_t(y) {
        log y; // ref up then down

      }
      _ { log "?"; fail; }
    }
    log str::refcount(s);
    assert (str::refcount(s) == const_refcount);
}

fn main() {
    let s: str = "hi"; // ref up

    foo(s); // ref up then down

    log str::refcount(s);
    assert (str::refcount(s) == const_refcount);
}