#![feature(const_generics)]
//~^ WARN the feature `const_generics` is incomplete

struct Const<const V: [usize; 1]> {}

fn main() {
    let mut x = Const::<{ [3] }> {};
    x = Const::<{ [4] }> {};
    //~^ ERROR mismatched types

}
