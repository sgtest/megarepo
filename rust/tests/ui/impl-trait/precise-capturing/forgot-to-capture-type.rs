#![feature(precise_capturing)]
//~^ WARN the feature `precise_capturing` is incomplete

fn type_param<T>() -> impl use<> Sized {}
//~^ ERROR `impl Trait` must mention all type parameters in scope

trait Foo {
    fn bar() -> impl use<> Sized;
    //~^ ERROR `impl Trait` must mention the `Self` type of the trait
}

fn main() {}
