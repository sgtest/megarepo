// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Issue 4691: Ensure that functional-struct-updates operates
// correctly and moves rather than copy when appropriate.

use NC = std::util::NonCopyable;

struct ncint { nc: NC, v: int }
fn ncint(v: int) -> ncint { ncint { nc: NC, v: v } }

struct NoFoo { copied: int, noncopy: ncint, }
impl NoFoo {
    fn new(x:int,y:int) -> NoFoo { NoFoo { copied: x, noncopy: ncint(y) } }
}

struct MoveFoo { copied: int, moved: ~int, }
impl MoveFoo {
    fn new(x:int,y:int) -> MoveFoo { MoveFoo { copied: x, moved: ~y } }
}

struct DropNoFoo { inner: NoFoo }
impl DropNoFoo {
    fn new(x:int,y:int) -> DropNoFoo { DropNoFoo { inner: NoFoo::new(x,y) } }
}
impl Drop for DropNoFoo { fn drop(&mut self) { } }

struct DropMoveFoo { inner: MoveFoo }
impl DropMoveFoo {
    fn new(x:int,y:int) -> DropMoveFoo { DropMoveFoo { inner: MoveFoo::new(x,y) } }
}
impl Drop for DropMoveFoo { fn drop(&mut self) { } }


fn test0() {
    // just copy implicitly copyable fields from `f`, no moves
    // (and thus it is okay that these are Drop; compare against
    // compile-fail test: borrowck-struct-update-with-dtor.rs).

    // Case 1: NonCopyable
    let f = DropNoFoo::new(1, 2);
    let b = DropNoFoo { inner: NoFoo { noncopy: ncint(3), ..f.inner }};
    let c = DropNoFoo { inner: NoFoo { noncopy: ncint(4), ..f.inner }};
    assert_eq!(f.inner.copied,    1);
    assert_eq!(f.inner.noncopy.v, 2);

    assert_eq!(b.inner.copied,    1);
    assert_eq!(b.inner.noncopy.v, 3);

    assert_eq!(c.inner.copied,    1);
    assert_eq!(c.inner.noncopy.v, 4);

    // Case 2: Owned
    let f = DropMoveFoo::new(5, 6);
    let b = DropMoveFoo { inner: MoveFoo { moved: ~7, ..f.inner }};
    let c = DropMoveFoo { inner: MoveFoo { moved: ~8, ..f.inner }};
    assert_eq!(f.inner.copied,    5);
    assert_eq!(*f.inner.moved,    6);

    assert_eq!(b.inner.copied,    5);
    assert_eq!(*b.inner.moved,    7);

    assert_eq!(c.inner.copied,    5);
    assert_eq!(*c.inner.moved,    8);
}

fn test1() {
    // copying move-by-default fields from `f`, so it moves:
    let f = MoveFoo::new(11, 12);

    let b = MoveFoo {moved: ~13, ..f};
    let c = MoveFoo {copied: 14, ..f};
    assert_eq!(b.copied,    11);
    assert_eq!(*b.moved,    13);
    assert_eq!(c.copied,    14);
    assert_eq!(*c.moved,    12);
}

fn test2() {
    // move non-copyable field
    let f = NoFoo::new(21, 22);
    let b = NoFoo {noncopy: ncint(23), ..f};
    let c = NoFoo {copied: 24, ..f};
    assert_eq!(b.copied,    21);
    assert_eq!(b.noncopy.v, 23);
    assert_eq!(c.copied,    24);
    assert_eq!(c.noncopy.v, 22);
}

fn main() {
    test0();
    test1();
    test2();
}
