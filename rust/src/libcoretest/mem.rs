// Copyright 2014-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use core::mem::*;
use test::Bencher;

#[test]
fn size_of_basic() {
    assert_eq!(size_of::<u8>(), 1);
    assert_eq!(size_of::<u16>(), 2);
    assert_eq!(size_of::<u32>(), 4);
    assert_eq!(size_of::<u64>(), 8);
}

#[test]
#[cfg(target_pointer_width = "32")]
fn size_of_32() {
    assert_eq!(size_of::<uint>(), 4);
    assert_eq!(size_of::<*const uint>(), 4);
}

#[test]
#[cfg(target_pointer_width = "64")]
fn size_of_64() {
    assert_eq!(size_of::<uint>(), 8);
    assert_eq!(size_of::<*const uint>(), 8);
}

#[test]
fn size_of_val_basic() {
    assert_eq!(size_of_val(&1u8), 1);
    assert_eq!(size_of_val(&1u16), 2);
    assert_eq!(size_of_val(&1u32), 4);
    assert_eq!(size_of_val(&1u64), 8);
}

#[test]
fn align_of_basic() {
    assert_eq!(align_of::<u8>(), 1);
    assert_eq!(align_of::<u16>(), 2);
    assert_eq!(align_of::<u32>(), 4);
}

#[test]
#[cfg(target_pointer_width = "32")]
fn align_of_32() {
    assert_eq!(align_of::<uint>(), 4);
    assert_eq!(align_of::<*const uint>(), 4);
}

#[test]
#[cfg(target_pointer_width = "64")]
fn align_of_64() {
    assert_eq!(align_of::<uint>(), 8);
    assert_eq!(align_of::<*const uint>(), 8);
}

#[test]
fn align_of_val_basic() {
    assert_eq!(align_of_val(&1u8), 1);
    assert_eq!(align_of_val(&1u16), 2);
    assert_eq!(align_of_val(&1u32), 4);
}

#[test]
fn test_swap() {
    let mut x = 31337;
    let mut y = 42;
    swap(&mut x, &mut y);
    assert_eq!(x, 42);
    assert_eq!(y, 31337);
}

#[test]
fn test_replace() {
    let mut x = Some("test".to_string());
    let y = replace(&mut x, None);
    assert!(x.is_none());
    assert!(y.is_some());
}

#[test]
fn test_transmute_copy() {
    assert_eq!(1, unsafe { transmute_copy(&1) });
}

#[test]
fn test_transmute() {
    trait Foo { fn dummy(&self) { } }
    impl Foo for int {}

    let a = box 100 as Box<Foo>;
    unsafe {
        let x: ::core::raw::TraitObject = transmute(a);
        assert!(*(x.data as *const int) == 100);
        let _x: Box<Foo> = transmute(x);
    }

    unsafe {
        assert_eq!([76u8], transmute::<_, Vec<u8>>("L".to_string()));
    }
}

// FIXME #13642 (these benchmarks should be in another place)
/// Completely miscellaneous language-construct benchmarks.
// Static/dynamic method dispatch

struct Struct {
    field: int
}

trait Trait {
    fn method(&self) -> int;
}

impl Trait for Struct {
    fn method(&self) -> int {
        self.field
    }
}

#[bench]
fn trait_vtable_method_call(b: &mut Bencher) {
    let s = Struct { field: 10 };
    let t = &s as &Trait;
    b.iter(|| {
        t.method()
    });
}

#[bench]
fn trait_static_method_call(b: &mut Bencher) {
    let s = Struct { field: 10 };
    b.iter(|| {
        s.method()
    });
}

// Overhead of various match forms

#[bench]
fn match_option_some(b: &mut Bencher) {
    let x = Some(10);
    b.iter(|| {
        match x {
            Some(y) => y,
            None => 11
        }
    });
}

#[bench]
fn match_vec_pattern(b: &mut Bencher) {
    let x = [1,2,3,4,5,6];
    b.iter(|| {
        match x {
            [1,2,3,..] => 10,
            _ => 11,
        }
    });
}
