// run-pass

#![feature(const_trait_impl)]
#![feature(const_fn_trait_bound)]

trait Bar {
    fn bar() -> u8;
}

trait Foo {
    #[default_method_body_is_const]
    fn foo() -> u8 where Self: ~const Bar {
        <Self as Bar>::bar() * 6
    }
}

struct NonConst;
struct Const;

impl Bar for NonConst {
    fn bar() -> u8 {
        3
    }
}

impl Foo for NonConst {}

impl const Bar for Const {
    fn bar() -> u8 {
        4
    }
}

impl const Foo for Const {}

fn main() {
    const ANS1: u8 = Const::foo();
    let ans2 = NonConst::foo();

    assert_eq!(ANS1 + ans2, 42);
}
