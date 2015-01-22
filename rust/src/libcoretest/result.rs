// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub fn op1() -> Result<int, &'static str> { Ok(666) }
pub fn op2() -> Result<int, &'static str> { Err("sadface") }

#[test]
pub fn test_and() {
    assert_eq!(op1().and(Ok(667i)).unwrap(), 667);
    assert_eq!(op1().and(Err::<i32, &'static str>("bad")).unwrap_err(),
               "bad");

    assert_eq!(op2().and(Ok(667i)).unwrap_err(), "sadface");
    assert_eq!(op2().and(Err::<i32,&'static str>("bad")).unwrap_err(),
               "sadface");
}

#[test]
pub fn test_and_then() {
    assert_eq!(op1().and_then(|i| Ok::<int, &'static str>(i + 1)).unwrap(), 667);
    assert_eq!(op1().and_then(|_| Err::<int, &'static str>("bad")).unwrap_err(),
               "bad");

    assert_eq!(op2().and_then(|i| Ok::<int, &'static str>(i + 1)).unwrap_err(),
               "sadface");
    assert_eq!(op2().and_then(|_| Err::<int, &'static str>("bad")).unwrap_err(),
               "sadface");
}

#[test]
pub fn test_or() {
    assert_eq!(op1().or(Ok(667)).unwrap(), 666);
    assert_eq!(op1().or(Err("bad")).unwrap(), 666);

    assert_eq!(op2().or(Ok(667)).unwrap(), 667);
    assert_eq!(op2().or(Err("bad")).unwrap_err(), "bad");
}

#[test]
pub fn test_or_else() {
    assert_eq!(op1().or_else(|_| Ok::<int, &'static str>(667)).unwrap(), 666);
    assert_eq!(op1().or_else(|e| Err::<int, &'static str>(e)).unwrap(), 666);

    assert_eq!(op2().or_else(|_| Ok::<int, &'static str>(667)).unwrap(), 667);
    assert_eq!(op2().or_else(|e| Err::<int, &'static str>(e)).unwrap_err(),
               "sadface");
}

#[test]
pub fn test_impl_map() {
    assert!(Ok::<int, int>(1).map(|x| x + 1) == Ok(2));
    assert!(Err::<int, int>(1).map(|x| x + 1) == Err(1));
}

#[test]
pub fn test_impl_map_err() {
    assert!(Ok::<int, int>(1).map_err(|x| x + 1) == Ok(1));
    assert!(Err::<int, int>(1).map_err(|x| x + 1) == Err(2));
}

/* FIXME(#20575)
#[test]
fn test_collect() {
    let v: Result<Vec<int>, ()> = range(0i, 0).map(|_| Ok::<int, ()>(0)).collect();
    assert!(v == Ok(vec![]));

    let v: Result<Vec<int>, ()> = range(0i, 3).map(|x| Ok::<int, ()>(x)).collect();
    assert!(v == Ok(vec![0, 1, 2]));

    let v: Result<Vec<int>, int> = range(0i, 3).map(|x| {
        if x > 1 { Err(x) } else { Ok(x) }
    }).collect();
    assert!(v == Err(2));

    // test that it does not take more elements than it needs
    let mut functions: [Box<Fn() -> Result<(), int>>; 3] =
        [box || Ok(()), box || Err(1i), box || panic!()];

    let v: Result<Vec<()>, int> = functions.iter_mut().map(|f| (*f)()).collect();
    assert!(v == Err(1));
}
*/

#[test]
pub fn test_fmt_default() {
    let ok: Result<int, &'static str> = Ok(100);
    let err: Result<int, &'static str> = Err("Err");

    let s = format!("{:?}", ok);
    assert_eq!(s, "Ok(100)");
    let s = format!("{:?}", err);
    assert_eq!(s, "Err(\"Err\")");
}

#[test]
pub fn test_unwrap_or() {
    let ok: Result<int, &'static str> = Ok(100i);
    let ok_err: Result<int, &'static str> = Err("Err");

    assert_eq!(ok.unwrap_or(50), 100);
    assert_eq!(ok_err.unwrap_or(50), 50);
}

#[test]
pub fn test_unwrap_or_else() {
    fn handler(msg: &'static str) -> int {
        if msg == "I got this." {
            50i
        } else {
            panic!("BadBad")
        }
    }

    let ok: Result<int, &'static str> = Ok(100);
    let ok_err: Result<int, &'static str> = Err("I got this.");

    assert_eq!(ok.unwrap_or_else(handler), 100);
    assert_eq!(ok_err.unwrap_or_else(handler), 50);
}

#[test]
#[should_fail]
pub fn test_unwrap_or_else_panic() {
    fn handler(msg: &'static str) -> int {
        if msg == "I got this." {
            50i
        } else {
            panic!("BadBad")
        }
    }

    let bad_err: Result<int, &'static str> = Err("Unrecoverable mess.");
    let _ : int = bad_err.unwrap_or_else(handler);
}
