// revisions: full min
#![cfg_attr(full, feature(const_generics, generic_arg_infer))]
#![cfg_attr(full, allow(incomplete_features))]

fn foo<const N: usize, const A: [u8; N]>() {}
//~^ ERROR the type of const parameters must not
//[min]~| ERROR `[u8; _]` is forbidden as the type of a const generic parameter

fn main() {
    foo::<_, {[1]}>();
    //[full]~^ ERROR mismatched types
    //[full]~| ERROR constant expression
}
