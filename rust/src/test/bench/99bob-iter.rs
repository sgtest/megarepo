

/* -*- mode::rust;indent-tabs-mode::nil -*-
 * Implementation of 99 Bottles of Beer
 * http://99-bottles-of-beer.net/
 */
use std;
import std::int;
import std::istr;

fn b1() -> istr { ret ~"# of beer on the wall, # of beer."; }

fn b2() -> istr {
    ret ~"Take one down and pass it around, # of beer on the wall.";
}

fn b7() -> istr {
    ret ~"No more bottles of beer on the wall, no more bottles of beer.";
}

fn b8() -> istr {
    ret ~"Go to the store and buy some more, # of beer on the wall.";
}

fn sub(t: &istr, n: int) -> istr {
    let b: istr = ~"";
    let i: uint = 0u;
    let ns: istr;
    alt n {
      0 { ns = ~"no more bottles"; }
      1 { ns = ~"1 bottle"; }
      _ { ns = int::to_str(n, 10u) + ~" bottles"; }
    }
    while i < istr::byte_len(t) {
        if t[i] == '#' as u8 { b += ns; } else { istr::push_byte(b, t[i]); }
        i += 1u;
    }
    ret b;
}


/* Using an interator */
iter ninetynine() -> int { let n: int = 100; while n > 1 { n -= 1; put n; } }

fn main() {
    for each n: int in ninetynine() {
        log sub(b1(), n);
        log sub(b2(), n - 1);
        log "";
    }
    log b7();
    log b8();
}
