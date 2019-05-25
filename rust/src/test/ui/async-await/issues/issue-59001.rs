// compile-pass
// edition:2018

#![feature(async_await, await_macro)]

use std::future::Future;

#[allow(unused)]
async fn enter<'a, F, R>(mut callback: F)
where
    F: FnMut(&'a mut i32) -> R,
    R: Future<Output = ()> + 'a,
{
    unimplemented!()
}

fn main() {}
