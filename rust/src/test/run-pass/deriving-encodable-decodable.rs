// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// This actually tests a lot more than just encodable/decodable, but it gets the
// job done at least

// xfail-fast

extern mod extra;

use std::io;
use std::rand::{random, Rand};
use extra::serialize::*;
use extra::ebml;
use extra::ebml::writer::Encoder;
use extra::ebml::reader::Decoder;

#[deriving(Encodable, Decodable, Eq, Rand)]
struct A;
#[deriving(Encodable, Decodable, Eq, Rand)]
struct B(int);
#[deriving(Encodable, Decodable, Eq, Rand)]
struct C(int, int, uint);

#[deriving(Encodable, Decodable, Eq, Rand)]
struct D {
    a: int,
    b: uint,
}

#[deriving(Encodable, Decodable, Eq, Rand)]
enum E {
    E1,
    E2(uint),
    E3(D),
    E4{ x: uint },
}

#[deriving(Encodable, Decodable, Eq, Rand)]
enum F { F1 }

#[deriving(Encodable, Decodable, Eq, Rand)]
struct G<T> {
    t: T
}

fn roundtrip<T: Rand + Eq + Encodable<Encoder> + Decodable<Decoder>>() {
    let obj: T = random();
    let bytes = do io::with_bytes_writer |w| {
        let mut e = Encoder(w);
        obj.encode(&mut e);
    };
    let doc = ebml::reader::Doc(@bytes);
    let mut dec = Decoder(doc);
    let obj2 = Decodable::decode(&mut dec);
    assert!(obj == obj2);
}

pub fn main() {
    roundtrip::<A>();
    roundtrip::<B>();
    roundtrip::<C>();
    roundtrip::<D>();

    do 20.times {
        roundtrip::<E>();
        roundtrip::<F>();
        roundtrip::<G<int>>();
    }
}
