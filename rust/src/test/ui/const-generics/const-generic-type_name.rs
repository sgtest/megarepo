// run-pass

#![feature(const_generics)]
//~^ WARN the feature `const_generics` is incomplete

#[derive(Debug)]
struct S<const N: usize>;

fn main() {
    assert_eq!(std::any::type_name::<S<3>>(), "const_generic_type_name::S<3>");
}
