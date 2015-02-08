// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub trait MyTrait {}

// @has foo/struct.Alpha.html '//pre' "pub struct Alpha<A> where A: MyTrait"
pub struct Alpha<A> where A: MyTrait;
// @has foo/trait.Bravo.html '//pre' "pub trait Bravo<B> where B: MyTrait"
pub trait Bravo<B> where B: MyTrait {}
// @has foo/fn.charlie.html '//pre' "pub fn charlie<C>() where C: MyTrait"
pub fn charlie<C>() where C: MyTrait {}

pub struct Delta<D>;
// @has foo/struct.Delta.html '//*[@class="impl"]//code' \
//          "impl<D> Delta<D> where D: MyTrait"
impl<D> Delta<D> where D: MyTrait {
    pub fn delta() {}
}

pub struct Echo<E>;
// @has foo/struct.Echo.html '//*[@class="impl"]//code' \
//          "impl<E> MyTrait for Echo<E> where E: MyTrait"
// @has foo/trait.MyTrait.html '//*[@id="implementors-list"]//code' \
//          "impl<E> MyTrait for Echo<E> where E: MyTrait"
impl<E> MyTrait for Echo<E> where E: MyTrait {}

pub enum Foxtrot<F> {}
// @has foo/enum.Foxtrot.html '//*[@class="impl"]//code' \
//          "impl<F> MyTrait for Foxtrot<F> where F: MyTrait"
// @has foo/trait.MyTrait.html '//*[@id="implementors-list"]//code' \
//          "impl<F> MyTrait for Foxtrot<F> where F: MyTrait"
impl<F> MyTrait for Foxtrot<F> where F: MyTrait {}
