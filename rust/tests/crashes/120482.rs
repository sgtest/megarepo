//@ known-bug: #120482
//@ edition:2021
#![feature(object_safe_for_dispatch)]

trait B {
    fn bar(&self, x: &Self);
}

trait A {
    fn g(new: B) -> B;
}

fn main() {}
