// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

enum Either<T, U> { Left(T), Right(U) }

    fn f(x: &mut Either<int,f64>, y: &Either<int,f64>) -> int {
        match *y {
            Either::Left(ref z) => {
                *x = Either::Right(1.0);
                *z
            }
            _ => panic!()
        }
    }

    fn g() {
        let mut x: Either<int,f64> = Either::Left(3);
        println!("{}", f(&mut x, &x)); //~ ERROR cannot borrow
    }

    fn h() {
        let mut x: Either<int,f64> = Either::Left(3);
        let y: &Either<int, f64> = &x;
        let z: &mut Either<int, f64> = &mut x; //~ ERROR cannot borrow
        *z = *y;
    }

    fn main() {}
