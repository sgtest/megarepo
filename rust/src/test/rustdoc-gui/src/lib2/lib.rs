// ignore-tidy-linelength

#![feature(doc_cfg)]

pub mod module {
    pub mod sub_module {
        pub mod sub_sub_module {
            pub fn foo() {}
        }
        pub fn bar() {}
    }
    pub fn whatever() {}
}

pub fn foobar() {}

pub type Alias = u32;

#[doc(cfg(feature = "foo-method"))]
pub struct Foo {
    pub x: Alias,
}

impl Foo {
    pub fn a_method(&self) {}
}

pub trait Trait {
    type X;
    const Y: u32;

    fn foo() {}
}

impl Trait for Foo {
    type X = u32;
    const Y: u32 = 0;
}


impl implementors::Whatever for Foo {}

pub mod sub_mod {
    /// ```txt
    /// aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    /// ```
    ///
    /// ```
    /// aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    /// ```
    pub struct Foo;
}

pub mod long_trait {
    use std::ops::DerefMut;

    pub trait ALongNameBecauseItHelpsTestingTheCurrentProblem: DerefMut<Target = u32>
        + From<u128> + Send + Sync + AsRef<str> + 'static {}
}
