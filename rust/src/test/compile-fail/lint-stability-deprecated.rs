// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// aux-build:lint_stability.rs
// aux-build:inherited_stability.rs
// aux-build:stability_cfg1.rs
// aux-build:stability_cfg2.rs

#![deny(deprecated)]
#![allow(dead_code)]
#![feature(staged_api, test_feature)]

#![stable(feature = "rust1", since = "1.0.0")]

#[macro_use]
extern crate lint_stability;

mod cross_crate {
    extern crate stability_cfg1;
    extern crate stability_cfg2;

    use lint_stability::*;

    fn test() {
        type Foo = MethodTester;
        let foo = MethodTester;

        deprecated(); //~ ERROR use of deprecated item
        foo.method_deprecated(); //~ ERROR use of deprecated item
        Foo::method_deprecated(&foo); //~ ERROR use of deprecated item
        <Foo>::method_deprecated(&foo); //~ ERROR use of deprecated item
        foo.trait_deprecated(); //~ ERROR use of deprecated item
        Trait::trait_deprecated(&foo); //~ ERROR use of deprecated item
        <Foo>::trait_deprecated(&foo); //~ ERROR use of deprecated item
        <Foo as Trait>::trait_deprecated(&foo); //~ ERROR use of deprecated item

        deprecated_text(); //~ ERROR use of deprecated item: text
        foo.method_deprecated_text(); //~ ERROR use of deprecated item: text
        Foo::method_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        <Foo>::method_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        foo.trait_deprecated_text(); //~ ERROR use of deprecated item: text
        Trait::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        <Foo>::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        <Foo as Trait>::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text

        deprecated_unstable(); //~ ERROR use of deprecated item
        foo.method_deprecated_unstable(); //~ ERROR use of deprecated item
        Foo::method_deprecated_unstable(&foo); //~ ERROR use of deprecated item
        <Foo>::method_deprecated_unstable(&foo); //~ ERROR use of deprecated item
        foo.trait_deprecated_unstable(); //~ ERROR use of deprecated item
        Trait::trait_deprecated_unstable(&foo); //~ ERROR use of deprecated item
        <Foo>::trait_deprecated_unstable(&foo); //~ ERROR use of deprecated item
        <Foo as Trait>::trait_deprecated_unstable(&foo); //~ ERROR use of deprecated item

        deprecated_unstable_text(); //~ ERROR use of deprecated item: text
        foo.method_deprecated_unstable_text(); //~ ERROR use of deprecated item: text
        Foo::method_deprecated_unstable_text(&foo); //~ ERROR use of deprecated item: text
        <Foo>::method_deprecated_unstable_text(&foo); //~ ERROR use of deprecated item: text
        foo.trait_deprecated_unstable_text(); //~ ERROR use of deprecated item: text
        Trait::trait_deprecated_unstable_text(&foo); //~ ERROR use of deprecated item: text
        <Foo>::trait_deprecated_unstable_text(&foo); //~ ERROR use of deprecated item: text
        <Foo as Trait>::trait_deprecated_unstable_text(&foo); //~ ERROR use of deprecated item: text

        unstable();
        foo.method_unstable();
        Foo::method_unstable(&foo);
        <Foo>::method_unstable(&foo);
        foo.trait_unstable();
        Trait::trait_unstable(&foo);
        <Foo>::trait_unstable(&foo);
        <Foo as Trait>::trait_unstable(&foo);

        unstable_text();
        foo.method_unstable_text();
        Foo::method_unstable_text(&foo);
        <Foo>::method_unstable_text(&foo);
        foo.trait_unstable_text();
        Trait::trait_unstable_text(&foo);
        <Foo>::trait_unstable_text(&foo);
        <Foo as Trait>::trait_unstable_text(&foo);

        stable();
        foo.method_stable();
        Foo::method_stable(&foo);
        <Foo>::method_stable(&foo);
        foo.trait_stable();
        Trait::trait_stable(&foo);
        <Foo>::trait_stable(&foo);
        <Foo as Trait>::trait_stable(&foo);

        stable_text();
        foo.method_stable_text();
        Foo::method_stable_text(&foo);
        <Foo>::method_stable_text(&foo);
        foo.trait_stable_text();
        Trait::trait_stable_text(&foo);
        <Foo>::trait_stable_text(&foo);
        <Foo as Trait>::trait_stable_text(&foo);

        struct S1<T: TraitWithAssociatedTypes>(T::TypeUnstable);
        struct S2<T: TraitWithAssociatedTypes>(T::TypeDeprecated);
        //~^ ERROR use of deprecated item

        let _ = DeprecatedStruct { //~ ERROR use of deprecated item
            i: 0 //~ ERROR use of deprecated item
        };
        let _ = DeprecatedUnstableStruct {
            //~^ ERROR use of deprecated item
            i: 0 //~ ERROR use of deprecated item
        };
        let _ = UnstableStruct { i: 0 };
        let _ = StableStruct { i: 0 };

        let _ = DeprecatedUnitStruct; //~ ERROR use of deprecated item
        let _ = DeprecatedUnstableUnitStruct; //~ ERROR use of deprecated item
        let _ = UnstableUnitStruct;
        let _ = StableUnitStruct;

        let _ = Enum::DeprecatedVariant; //~ ERROR use of deprecated item
        let _ = Enum::DeprecatedUnstableVariant; //~ ERROR use of deprecated item
        let _ = Enum::UnstableVariant;
        let _ = Enum::StableVariant;

        let _ = DeprecatedTupleStruct (1); //~ ERROR use of deprecated item
        let _ = DeprecatedUnstableTupleStruct (1); //~ ERROR use of deprecated item
        let _ = UnstableTupleStruct (1);
        let _ = StableTupleStruct (1);

        // At the moment, the lint checker only checks stability in
        // in the arguments of macros.
        // Eventually, we will want to lint the contents of the
        // macro in the module *defining* it. Also, stability levels
        // on macros themselves are not yet linted.
        macro_test_arg!(deprecated_text()); //~ ERROR use of deprecated item: text
        macro_test_arg!(deprecated_unstable_text()); //~ ERROR use of deprecated item: text
        macro_test_arg!(macro_test_arg!(deprecated_text())); //~ ERROR use of deprecated item: text
    }

    fn test_method_param<Foo: Trait>(foo: Foo) {
        foo.trait_deprecated(); //~ ERROR use of deprecated item
        Trait::trait_deprecated(&foo); //~ ERROR use of deprecated item
        <Foo>::trait_deprecated(&foo); //~ ERROR use of deprecated item
        <Foo as Trait>::trait_deprecated(&foo); //~ ERROR use of deprecated item
        foo.trait_deprecated_text(); //~ ERROR use of deprecated item: text
        Trait::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        <Foo>::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        <Foo as Trait>::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        foo.trait_deprecated_unstable(); //~ ERROR use of deprecated item
        Trait::trait_deprecated_unstable(&foo); //~ ERROR use of deprecated item
        <Foo>::trait_deprecated_unstable(&foo); //~ ERROR use of deprecated item
        <Foo as Trait>::trait_deprecated_unstable(&foo); //~ ERROR use of deprecated item
        foo.trait_deprecated_unstable_text(); //~ ERROR use of deprecated item: text
        Trait::trait_deprecated_unstable_text(&foo); //~ ERROR use of deprecated item: text
        <Foo>::trait_deprecated_unstable_text(&foo); //~ ERROR use of deprecated item: text
        <Foo as Trait>::trait_deprecated_unstable_text(&foo); //~ ERROR use of deprecated item: text
        foo.trait_unstable();
        Trait::trait_unstable(&foo);
        <Foo>::trait_unstable(&foo);
        <Foo as Trait>::trait_unstable(&foo);
        foo.trait_unstable_text();
        Trait::trait_unstable_text(&foo);
        <Foo>::trait_unstable_text(&foo);
        <Foo as Trait>::trait_unstable_text(&foo);
        foo.trait_stable();
        Trait::trait_stable(&foo);
        <Foo>::trait_stable(&foo);
        <Foo as Trait>::trait_stable(&foo);
    }

    fn test_method_object(foo: &Trait) {
        foo.trait_deprecated(); //~ ERROR use of deprecated item
        foo.trait_deprecated_text(); //~ ERROR use of deprecated item: text
        foo.trait_deprecated_unstable(); //~ ERROR use of deprecated item
        foo.trait_deprecated_unstable_text(); //~ ERROR use of deprecated item: text
        foo.trait_unstable();
        foo.trait_unstable_text();
        foo.trait_stable();
    }

    struct S;

    impl UnstableTrait for S { }
    impl DeprecatedTrait for S {} //~ ERROR use of deprecated item: text
    trait LocalTrait : UnstableTrait { }
    trait LocalTrait2 : DeprecatedTrait { } //~ ERROR use of deprecated item: text

    impl Trait for S {
        fn trait_stable(&self) {}
        fn trait_unstable(&self) {}
    }
}

mod inheritance {
    extern crate inherited_stability;
    use self::inherited_stability::*;

    fn test_inheritance() {
        unstable();
        stable();

        stable_mod::unstable();
        stable_mod::stable();

        unstable_mod::deprecated(); //~ ERROR use of deprecated item
        unstable_mod::unstable();

        let _ = Unstable::UnstableVariant;
        let _ = Unstable::StableVariant;

        let x: usize = 0;
        x.unstable();
        x.stable();
    }
}

mod this_crate {
    #[unstable(feature = "test_feature", issue = "0")]
    #[rustc_deprecated(since = "1.0.0", reason = "text")]
    pub fn deprecated() {}
    #[unstable(feature = "test_feature", issue = "0")]
    #[rustc_deprecated(since = "1.0.0", reason = "text")]
    pub fn deprecated_text() {}

    #[unstable(feature = "test_feature", issue = "0")]
    pub fn unstable() {}
    #[unstable(feature = "test_feature", reason = "text", issue = "0")]
    pub fn unstable_text() {}

    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn stable() {}
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn stable_text() {}

    #[stable(feature = "rust1", since = "1.0.0")]
    pub struct MethodTester;

    impl MethodTester {
        #[unstable(feature = "test_feature", issue = "0")]
        #[rustc_deprecated(since = "1.0.0", reason = "text")]
        pub fn method_deprecated(&self) {}
        #[unstable(feature = "test_feature", issue = "0")]
        #[rustc_deprecated(since = "1.0.0", reason = "text")]
        pub fn method_deprecated_text(&self) {}

        #[unstable(feature = "test_feature", issue = "0")]
        pub fn method_unstable(&self) {}
        #[unstable(feature = "test_feature", reason = "text", issue = "0")]
        pub fn method_unstable_text(&self) {}

        #[stable(feature = "rust1", since = "1.0.0")]
        pub fn method_stable(&self) {}
        #[stable(feature = "rust1", since = "1.0.0")]
        pub fn method_stable_text(&self) {}
    }

    pub trait Trait {
        #[unstable(feature = "test_feature", issue = "0")]
        #[rustc_deprecated(since = "1.0.0", reason = "text")]
        fn trait_deprecated(&self) {}
        #[unstable(feature = "test_feature", issue = "0")]
        #[rustc_deprecated(since = "1.0.0", reason = "text")]
        fn trait_deprecated_text(&self) {}

        #[unstable(feature = "test_feature", issue = "0")]
        fn trait_unstable(&self) {}
        #[unstable(feature = "test_feature", reason = "text", issue = "0")]
        fn trait_unstable_text(&self) {}

        #[stable(feature = "rust1", since = "1.0.0")]
        fn trait_stable(&self) {}
        #[stable(feature = "rust1", since = "1.0.0")]
        fn trait_stable_text(&self) {}
    }

    impl Trait for MethodTester {}

    #[unstable(feature = "test_feature", issue = "0")]
    #[rustc_deprecated(since = "1.0.0", reason = "text")]
    pub struct DeprecatedStruct {
        #[stable(feature = "test_feature", since = "1.0.0")] i: isize
    }
    #[unstable(feature = "test_feature", issue = "0")]
    pub struct UnstableStruct {
        #[stable(feature = "test_feature", since = "1.0.0")] i: isize
    }
    #[stable(feature = "rust1", since = "1.0.0")]
    pub struct StableStruct {
        #[stable(feature = "test_feature", since = "1.0.0")] i: isize
    }

    #[unstable(feature = "test_feature", issue = "0")]
    #[rustc_deprecated(since = "1.0.0", reason = "text")]
    pub struct DeprecatedUnitStruct;
    #[unstable(feature = "test_feature", issue = "0")]
    pub struct UnstableUnitStruct;
    #[stable(feature = "rust1", since = "1.0.0")]
    pub struct StableUnitStruct;

    pub enum Enum {
        #[unstable(feature = "test_feature", issue = "0")]
        #[rustc_deprecated(since = "1.0.0", reason = "text")]
        DeprecatedVariant,
        #[unstable(feature = "test_feature", issue = "0")]
        UnstableVariant,

        #[stable(feature = "rust1", since = "1.0.0")]
        StableVariant,
    }

    #[unstable(feature = "test_feature", issue = "0")]
    #[rustc_deprecated(since = "1.0.0", reason = "text")]
    pub struct DeprecatedTupleStruct(isize);
    #[unstable(feature = "test_feature", issue = "0")]
    pub struct UnstableTupleStruct(isize);
    #[stable(feature = "rust1", since = "1.0.0")]
    pub struct StableTupleStruct(isize);

    fn test() {
        // Only the deprecated cases of the following should generate
        // errors, because other stability attributes now have meaning
        // only *across* crates, not within a single crate.

        type Foo = MethodTester;
        let foo = MethodTester;

        deprecated(); //~ ERROR use of deprecated item
        foo.method_deprecated(); //~ ERROR use of deprecated item
        Foo::method_deprecated(&foo); //~ ERROR use of deprecated item
        <Foo>::method_deprecated(&foo); //~ ERROR use of deprecated item
        foo.trait_deprecated(); //~ ERROR use of deprecated item
        Trait::trait_deprecated(&foo); //~ ERROR use of deprecated item
        <Foo>::trait_deprecated(&foo); //~ ERROR use of deprecated item
        <Foo as Trait>::trait_deprecated(&foo); //~ ERROR use of deprecated item

        deprecated_text(); //~ ERROR use of deprecated item: text
        foo.method_deprecated_text(); //~ ERROR use of deprecated item: text
        Foo::method_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        <Foo>::method_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        foo.trait_deprecated_text(); //~ ERROR use of deprecated item: text
        Trait::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        <Foo>::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        <Foo as Trait>::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text

        unstable();
        foo.method_unstable();
        Foo::method_unstable(&foo);
        <Foo>::method_unstable(&foo);
        foo.trait_unstable();
        Trait::trait_unstable(&foo);
        <Foo>::trait_unstable(&foo);
        <Foo as Trait>::trait_unstable(&foo);

        unstable_text();
        foo.method_unstable_text();
        Foo::method_unstable_text(&foo);
        <Foo>::method_unstable_text(&foo);
        foo.trait_unstable_text();
        Trait::trait_unstable_text(&foo);
        <Foo>::trait_unstable_text(&foo);
        <Foo as Trait>::trait_unstable_text(&foo);

        stable();
        foo.method_stable();
        Foo::method_stable(&foo);
        <Foo>::method_stable(&foo);
        foo.trait_stable();
        Trait::trait_stable(&foo);
        <Foo>::trait_stable(&foo);
        <Foo as Trait>::trait_stable(&foo);

        stable_text();
        foo.method_stable_text();
        Foo::method_stable_text(&foo);
        <Foo>::method_stable_text(&foo);
        foo.trait_stable_text();
        Trait::trait_stable_text(&foo);
        <Foo>::trait_stable_text(&foo);
        <Foo as Trait>::trait_stable_text(&foo);

        let _ = DeprecatedStruct {
            //~^ ERROR use of deprecated item
            i: 0 //~ ERROR use of deprecated item
        };
        let _ = UnstableStruct { i: 0 };
        let _ = StableStruct { i: 0 };

        let _ = DeprecatedUnitStruct; //~ ERROR use of deprecated item
        let _ = UnstableUnitStruct;
        let _ = StableUnitStruct;

        let _ = Enum::DeprecatedVariant; //~ ERROR use of deprecated item
        let _ = Enum::UnstableVariant;
        let _ = Enum::StableVariant;

        let _ = DeprecatedTupleStruct (1); //~ ERROR use of deprecated item
        let _ = UnstableTupleStruct (1);
        let _ = StableTupleStruct (1);
    }

    fn test_method_param<Foo: Trait>(foo: Foo) {
        foo.trait_deprecated(); //~ ERROR use of deprecated item
        Trait::trait_deprecated(&foo); //~ ERROR use of deprecated item
        <Foo>::trait_deprecated(&foo); //~ ERROR use of deprecated item
        <Foo as Trait>::trait_deprecated(&foo); //~ ERROR use of deprecated item
        foo.trait_deprecated_text(); //~ ERROR use of deprecated item: text
        Trait::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        <Foo>::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        <Foo as Trait>::trait_deprecated_text(&foo); //~ ERROR use of deprecated item: text
        foo.trait_unstable();
        Trait::trait_unstable(&foo);
        <Foo>::trait_unstable(&foo);
        <Foo as Trait>::trait_unstable(&foo);
        foo.trait_unstable_text();
        Trait::trait_unstable_text(&foo);
        <Foo>::trait_unstable_text(&foo);
        <Foo as Trait>::trait_unstable_text(&foo);
        foo.trait_stable();
        Trait::trait_stable(&foo);
        <Foo>::trait_stable(&foo);
        <Foo as Trait>::trait_stable(&foo);
    }

    fn test_method_object(foo: &Trait) {
        foo.trait_deprecated(); //~ ERROR use of deprecated item
        foo.trait_deprecated_text(); //~ ERROR use of deprecated item: text
        foo.trait_unstable();
        foo.trait_unstable_text();
        foo.trait_stable();
    }

    #[unstable(feature = "test_feature", issue = "0")]
    #[rustc_deprecated(since = "1.0.0", reason = "text")]
    fn test_fn_body() {
        fn fn_in_body() {}
        fn_in_body(); //~ ERROR use of deprecated item: text
    }

    impl MethodTester {
        #[unstable(feature = "test_feature", issue = "0")]
        #[rustc_deprecated(since = "1.0.0", reason = "text")]
        fn test_method_body(&self) {
            fn fn_in_body() {}
            fn_in_body(); //~ ERROR use of deprecated item: text
        }
    }

    #[unstable(feature = "test_feature", issue = "0")]
    #[rustc_deprecated(since = "1.0.0", reason = "text")]
    pub trait DeprecatedTrait {
        fn dummy(&self) { }
    }

    struct S;

    impl DeprecatedTrait for S { } //~ ERROR use of deprecated item

    trait LocalTrait : DeprecatedTrait { } //~ ERROR use of deprecated item
}

fn main() {}
