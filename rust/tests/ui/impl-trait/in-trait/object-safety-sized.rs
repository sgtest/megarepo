// check-pass
// revisions: current next
//[next] compile-flags: -Ztrait-solver=next

#![feature(return_position_impl_trait_in_trait)]

fn main() {
    let vec: Vec<Box<dyn Trait>> = Vec::new();

    for i in vec {
        i.fn_2();
    }
}

trait OtherTrait {}

trait Trait {
    fn fn_1(&self) -> impl OtherTrait
    where
        Self: Sized;

    fn fn_2(&self) -> bool;
}
