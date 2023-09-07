#![feature(return_position_impl_trait_in_trait, async_fn_in_trait)]
#![deny(refining_impl_trait)]

pub trait Foo {
    fn bar() -> impl Sized;
}

pub struct A;
impl Foo for A {
    fn bar() -> impl Copy {}
    //~^ ERROR impl method signature does not match trait method signature
}

pub struct B;
impl Foo for B {
    fn bar() {}
    //~^ ERROR impl method signature does not match trait method signature
}

pub struct C;
impl Foo for C {
    fn bar() -> () {}
    //~^ ERROR impl method signature does not match trait method signature
}

struct Private;
impl Foo for Private {
    fn bar() -> () {}
}

pub trait Arg<A> {
    fn bar() -> impl Sized;
}
impl Arg<Private> for A {
    fn bar() -> () {}
}

pub trait Late {
    fn bar<'a>(&'a self) -> impl Sized + 'a;
}

pub struct D;
impl Late for D {
    fn bar(&self) -> impl Copy + '_ {}
    //~^ ERROR impl method signature does not match trait method signature
}

fn main() {}
