// run-pass

#![feature(fn_delegation)]
//~^ WARN the feature `fn_delegation` is incomplete

mod to_reuse {
    pub fn foo(x: i32) -> i32 { x }
    pub mod inner {}
}

reuse to_reuse::foo {{
    use self::to_reuse::foo;
    let x = foo(12);
    x + self
}}

trait Trait {
    fn bar(&self, x: i32) -> i32 { x }
}

struct F;
impl Trait for F {}

struct S(F);
impl Trait for S {
    reuse <F as Trait>::bar {
        #[allow(unused_imports)]
        use self::to_reuse::{foo, inner::self};
        let x = foo(12);
        assert_eq!(x, 12);
        &self.0
    }
}

fn main() {
    assert_eq!(foo(12), 24);
}
