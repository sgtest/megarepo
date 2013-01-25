// xfail-fast

// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[forbid(deprecated_pattern)];

extern mod std;

// These tests used to be separate files, but I wanted to refactor all
// the oldcommon code.

use cmp::Eq;
use std::ebml;
use EBReader = std::ebml::reader;
use EBWriter = std::ebml::writer;
use io::Writer;
use std::serialize::{Encodable, Decodable};
use std::prettyprint;
use std::time;

fn test_prettyprint<A: Encodable<prettyprint::Serializer>>(
    a: &A,
    expected: &~str
) {
    let s = do io::with_str_writer |w| {
        a.encode(&prettyprint::Serializer(w))
    };
    debug!("s == %?", s);
    assert s == *expected;
}

fn test_ebml<A:
    Eq
    Encodable<EBWriter::Encoder>
    Decodable<EBReader::Decoder>
>(a1: &A) {
    let bytes = do io::with_bytes_writer |wr| {
        let ebml_w = &EBWriter::Encoder(wr);
        a1.encode(ebml_w)
    };
    let d = EBReader::Doc(@move bytes);
    let a2: A = Decodable::decode(&EBReader::Decoder(d));
    assert *a1 == a2;
}

#[auto_encode]
#[auto_decode]
enum Expr {
    Val(uint),
    Plus(@Expr, @Expr),
    Minus(@Expr, @Expr)
}

impl Expr : cmp::Eq {
    pure fn eq(&self, other: &Expr) -> bool {
        match *self {
            Val(e0a) => {
                match *other {
                    Val(e0b) => e0a == e0b,
                    _ => false
                }
            }
            Plus(e0a, e1a) => {
                match *other {
                    Plus(e0b, e1b) => e0a == e0b && e1a == e1b,
                    _ => false
                }
            }
            Minus(e0a, e1a) => {
                match *other {
                    Minus(e0b, e1b) => e0a == e0b && e1a == e1b,
                    _ => false
                }
            }
        }
    }
    pure fn ne(&self, other: &Expr) -> bool { !(*self).eq(other) }
}

impl AnEnum : cmp::Eq {
    pure fn eq(&self, other: &AnEnum) -> bool {
        (*self).v == other.v
    }
    pure fn ne(&self, other: &AnEnum) -> bool { !(*self).eq(other) }
}

impl Point : cmp::Eq {
    pure fn eq(&self, other: &Point) -> bool {
        self.x == other.x && self.y == other.y
    }
    pure fn ne(&self, other: &Point) -> bool { !(*self).eq(other) }
}

impl<T:cmp::Eq> Quark<T> : cmp::Eq {
    pure fn eq(&self, other: &Quark<T>) -> bool {
        match *self {
            Top(ref q) => {
                match *other {
                    Top(ref r) => q == r,
                    Bottom(_) => false
                }
            },
            Bottom(ref q) => {
                match *other {
                    Top(_) => false,
                    Bottom(ref r) => q == r
                }
            },
        }
    }
    pure fn ne(&self, other: &Quark<T>) -> bool { !(*self).eq(other) }
}

impl CLike : cmp::Eq {
    pure fn eq(&self, other: &CLike) -> bool {
        (*self) as int == *other as int
    }
    pure fn ne(&self, other: &CLike) -> bool { !self.eq(other) }
}

#[auto_encode]
#[auto_decode]
struct Spanned<T> {
    lo: uint,
    hi: uint,
    node: T,
}

impl<T:cmp::Eq> Spanned<T> : cmp::Eq {
    pure fn eq(&self, other: &Spanned<T>) -> bool {
        self.lo == other.lo &&
        self.hi == other.hi &&
        self.node == other.node
    }
    pure fn ne(&self, other: &Spanned<T>) -> bool { !self.eq(other) }
}

#[auto_encode]
#[auto_decode]
struct SomeStruct { v: ~[uint] }

#[auto_encode]
#[auto_decode]
enum AnEnum = SomeStruct;

#[auto_encode]
#[auto_decode]
struct Point {x: uint, y: uint}

#[auto_encode]
#[auto_decode]
enum Quark<T> {
    Top(T),
    Bottom(T)
}

#[auto_encode]
#[auto_decode]
enum CLike { A, B, C }

fn main() {
    let a = &Plus(@Minus(@Val(3u), @Val(10u)), @Plus(@Val(22u), @Val(5u)));
    test_prettyprint(a, &~"Plus(@Minus(@Val(3u), @Val(10u)), \
                           @Plus(@Val(22u), @Val(5u)))");
    test_ebml(a);

    let a = &Spanned {lo: 0u, hi: 5u, node: 22u};
    test_prettyprint(a, &~"Spanned {lo: 0u, hi: 5u, node: 22u}");
    test_ebml(a);

    let a = &AnEnum(SomeStruct {v: ~[1u, 2u, 3u]});
    test_prettyprint(a, &~"AnEnum(SomeStruct {v: ~[1u, 2u, 3u]})");
    test_ebml(a);

    let a = &Point {x: 3u, y: 5u};
    test_prettyprint(a, &~"Point {x: 3u, y: 5u}");
    test_ebml(a);

    let a = &@[1u, 2u, 3u];
    test_prettyprint(a, &~"@[1u, 2u, 3u]");
    test_ebml(a);

    let a = &Top(22u);
    test_prettyprint(a, &~"Top(22u)");
    test_ebml(a);

    let a = &Bottom(222u);
    test_prettyprint(a, &~"Bottom(222u)");
    test_ebml(a);

    let a = &A;
    test_prettyprint(a, &~"A");
    test_ebml(a);

    let a = &B;
    test_prettyprint(a, &~"B");
    test_ebml(a);

    let a = &time::now();
    test_ebml(a);
}
