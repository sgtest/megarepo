// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// edition:2018

#![feature(arbitrary_self_types, async_await, await_macro, futures_api, pin)]

use std::pin::Pin;
use std::future::Future;
use std::sync::{
    Arc,
    atomic::{self, AtomicUsize},
};
use std::future::FutureObj;
use std::task::{
    Context, Poll, Wake,
    Spawn, SpawnObjError,
    local_waker_from_nonlocal,
};

struct Counter {
    wakes: AtomicUsize,
}

impl Wake for Counter {
    fn wake(this: &Arc<Self>) {
        this.wakes.fetch_add(1, atomic::Ordering::SeqCst);
    }
}

struct NoopSpawner;
impl Spawn for NoopSpawner {
    fn spawn_obj(&mut self, _: FutureObj<'static, ()>) -> Result<(), SpawnObjError> {
        Ok(())
    }
}

struct WakeOnceThenComplete(bool);

fn wake_and_yield_once() -> WakeOnceThenComplete { WakeOnceThenComplete(false) }

impl Future for WakeOnceThenComplete {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            cx.waker().wake();
            self.0 = true;
            Poll::Pending
        }
    }
}

fn async_block(x: u8) -> impl Future<Output = u8> {
    async move {
        await!(wake_and_yield_once());
        x
    }
}

fn async_block_with_borrow_named_lifetime<'a>(x: &'a u8) -> impl Future<Output = u8> + 'a {
    async move {
        await!(wake_and_yield_once());
        *x
    }
}

fn async_nonmove_block(x: u8) -> impl Future<Output = u8> {
    async move {
        let future = async {
            await!(wake_and_yield_once());
            x
        };
        await!(future)
    }
}

fn async_closure(x: u8) -> impl Future<Output = u8> {
    (async move |x: u8| -> u8 {
        await!(wake_and_yield_once());
        x
    })(x)
}

async fn async_fn(x: u8) -> u8 {
    await!(wake_and_yield_once());
    x
}

async fn async_fn_with_borrow(x: &u8) -> u8 {
    await!(wake_and_yield_once());
    *x
}

async fn async_fn_with_borrow_named_lifetime<'a>(x: &'a u8) -> u8 {
    await!(wake_and_yield_once());
    *x
}

fn async_fn_with_impl_future_named_lifetime<'a>(x: &'a u8) -> impl Future<Output = u8> + 'a {
    async move {
        await!(wake_and_yield_once());
        *x
    }
}

async fn async_fn_with_named_lifetime_multiple_args<'a>(x: &'a u8, _y: &'a u8) -> u8 {
    await!(wake_and_yield_once());
    *x
}

fn async_fn_with_internal_borrow(y: u8) -> impl Future<Output = u8> {
    async move {
        await!(async_fn_with_borrow(&y))
    }
}

unsafe async fn unsafe_async_fn(x: u8) -> u8 {
    await!(wake_and_yield_once());
    x
}

struct Foo;

trait Bar {
    fn foo() {}
}

impl Foo {
    async fn async_method(x: u8) -> u8 {
        unsafe {
            await!(unsafe_async_fn(x))
        }
    }
}

fn test_future_yields_once_then_returns<F, Fut>(f: F)
where
    F: FnOnce(u8) -> Fut,
    Fut: Future<Output = u8>,
{
    let mut fut = Box::pinned(f(9));
    let counter = Arc::new(Counter { wakes: AtomicUsize::new(0) });
    let waker = local_waker_from_nonlocal(counter.clone());
    let spawner = &mut NoopSpawner;
    let cx = &mut Context::new(&waker, spawner);

    assert_eq!(0, counter.wakes.load(atomic::Ordering::SeqCst));
    assert_eq!(Poll::Pending, fut.as_mut().poll(cx));
    assert_eq!(1, counter.wakes.load(atomic::Ordering::SeqCst));
    assert_eq!(Poll::Ready(9), fut.as_mut().poll(cx));
}

fn main() {
    macro_rules! test {
        ($($fn_name:expr,)*) => { $(
            test_future_yields_once_then_returns($fn_name);
        )* }
    }

    macro_rules! test_with_borrow {
        ($($fn_name:expr,)*) => { $(
            test_future_yields_once_then_returns(|x| {
                async move {
                    await!($fn_name(&x))
                }
            });
        )* }
    }

    test! {
        async_block,
        async_nonmove_block,
        async_closure,
        async_fn,
        async_fn_with_internal_borrow,
        |x| {
            async move {
                unsafe { await!(unsafe_async_fn(x)) }
            }
        },
    }

    test_with_borrow! {
        async_block_with_borrow_named_lifetime,
        async_fn_with_borrow,
        async_fn_with_borrow_named_lifetime,
        async_fn_with_impl_future_named_lifetime,
        |x| {
            async move {
                await!(async_fn_with_named_lifetime_multiple_args(x, x))
            }
        },
    }
}
