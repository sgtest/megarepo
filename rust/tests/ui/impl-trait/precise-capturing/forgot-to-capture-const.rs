#![feature(precise_capturing)]
//~^ WARN the feature `precise_capturing` is incomplete

fn constant<const C: usize>() -> impl use<> Sized {}
//~^ ERROR `impl Trait` must mention all const parameters in scope

fn main() {}
