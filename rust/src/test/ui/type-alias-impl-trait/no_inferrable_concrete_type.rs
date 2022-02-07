// Issue 52985: user code provides no use case that allows a type alias `impl Trait`
// We now emit a 'unconstrained opaque type' error

#![feature(type_alias_impl_trait)]

type Foo = impl Copy; //~ unconstrained opaque type

// make compiler happy about using 'Foo'
fn bar(x: Foo) -> Foo {
    x
}

fn main() {
    let _: Foo = std::mem::transmute(0u8);
}
