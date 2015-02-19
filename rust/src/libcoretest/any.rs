// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
use core::any::*;
use test::Bencher;
use test;

#[derive(PartialEq, Debug)]
struct Test;

static TEST: &'static str = "Test";

#[test]
fn any_referenced() {
    let (a, b, c) = (&5 as &Any, &TEST as &Any, &Test as &Any);

    assert!(a.is::<i32>());
    assert!(!b.is::<i32>());
    assert!(!c.is::<i32>());

    assert!(!a.is::<&'static str>());
    assert!(b.is::<&'static str>());
    assert!(!c.is::<&'static str>());

    assert!(!a.is::<Test>());
    assert!(!b.is::<Test>());
    assert!(c.is::<Test>());
}

#[test]
fn any_owning() {
    let (a, b, c) = (box 5_usize as Box<Any>, box TEST as Box<Any>, box Test as Box<Any>);

    assert!(a.is::<uint>());
    assert!(!b.is::<uint>());
    assert!(!c.is::<uint>());

    assert!(!a.is::<&'static str>());
    assert!(b.is::<&'static str>());
    assert!(!c.is::<&'static str>());

    assert!(!a.is::<Test>());
    assert!(!b.is::<Test>());
    assert!(c.is::<Test>());
}

#[test]
fn any_downcast_ref() {
    let a = &5_usize as &Any;

    match a.downcast_ref::<uint>() {
        Some(&5) => {}
        x => panic!("Unexpected value {:?}", x)
    }

    match a.downcast_ref::<Test>() {
        None => {}
        x => panic!("Unexpected value {:?}", x)
    }
}

#[test]
fn any_downcast_mut() {
    let mut a = 5_usize;
    let mut b = box 7_usize;

    let a_r = &mut a as &mut Any;
    let tmp: &mut uint = &mut *b;
    let b_r = tmp as &mut Any;

    match a_r.downcast_mut::<uint>() {
        Some(x) => {
            assert_eq!(*x, 5);
            *x = 612;
        }
        x => panic!("Unexpected value {:?}", x)
    }

    match b_r.downcast_mut::<uint>() {
        Some(x) => {
            assert_eq!(*x, 7);
            *x = 413;
        }
        x => panic!("Unexpected value {:?}", x)
    }

    match a_r.downcast_mut::<Test>() {
        None => (),
        x => panic!("Unexpected value {:?}", x)
    }

    match b_r.downcast_mut::<Test>() {
        None => (),
        x => panic!("Unexpected value {:?}", x)
    }

    match a_r.downcast_mut::<uint>() {
        Some(&mut 612) => {}
        x => panic!("Unexpected value {:?}", x)
    }

    match b_r.downcast_mut::<uint>() {
        Some(&mut 413) => {}
        x => panic!("Unexpected value {:?}", x)
    }
}

#[test]
fn any_fixed_vec() {
    let test = [0_usize; 8];
    let test = &test as &Any;
    assert!(test.is::<[uint; 8]>());
    assert!(!test.is::<[uint; 10]>());
}


#[bench]
fn bench_downcast_ref(b: &mut Bencher) {
    b.iter(|| {
        let mut x = 0;
        let mut y = &mut x as &mut Any;
        test::black_box(&mut y);
        test::black_box(y.downcast_ref::<int>() == Some(&0));
    });
}
