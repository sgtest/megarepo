#![feature(const_generics)]
//~^ WARN the feature `const_generics` is incomplete

trait A {}
struct B;
impl A for B {}

fn test<const T: &'static dyn A>() {
    //~^ ERROR must be annotated with `#[derive(PartialEq, Eq)]` to be used
    unimplemented!()
}

fn main() {
    test::<{ &B }>();
}
