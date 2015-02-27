// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// aux-build:lint_stability_fields.rs
#![deny(deprecated)]
#![allow(dead_code)]
#![feature(staged_api)]
#![staged_api]

mod cross_crate {
    extern crate lint_stability_fields;

    use self::lint_stability_fields::*;

    pub fn foo() {
        let x = Stable {
            inherit: 1,
            override1: 2, //~ WARN use of unstable
            override2: 3,
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
        };

        let _ = x.inherit;
        let _ = x.override1; //~ WARN use of unstable
        let _ = x.override2;
        //~^ ERROR use of deprecated item
        //~^^ WARN use of unstable

        let Stable {
            inherit: _,
            override1: _, //~ WARN use of unstable
            override2: _
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
        } = x;
        // all fine
        let Stable { .. } = x;

        let x = Stable2(1, 2, 3);

        let _ = x.0;
        let _ = x.1; //~ WARN use of unstable
        let _ = x.2;
        //~^ ERROR use of deprecated item
        //~^^ WARN use of unstable

        let Stable2(_,
                   _, //~ WARN use of unstable
                   _)
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
            = x;
        // all fine
        let Stable2(..) = x;


        let x = Unstable { //~ WARN use of unstable
            inherit: 1, //~ WARN use of unstable
            override1: 2,
            override2: 3,
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
        };

        let _ = x.inherit; //~ WARN use of unstable
        let _ = x.override1;
        let _ = x.override2;
        //~^ ERROR use of deprecated item
        //~^^ WARN use of unstable

        let Unstable { //~ WARN use of unstable
            inherit: _, //~ WARN use of unstable
            override1: _,
            override2: _
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
        } = x;

        let Unstable  //~ WARN use of unstable
            // the patterns are all fine:
            { .. } = x;


        let x = Unstable2(1, 2, 3); //~ WARN use of unstable

        let _ = x.0; //~ WARN use of unstable
        let _ = x.1;
        let _ = x.2;
        //~^ ERROR use of deprecated item
        //~^^ WARN use of unstable

        let Unstable2  //~ WARN use of unstable
            (_, //~ WARN use of unstable
             _,
             _)
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
            = x;
        let Unstable2 //~ WARN use of unstable
            // the patterns are all fine:
            (..) = x;


        let x = Deprecated {
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
            inherit: 1,
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
            override1: 2,
            override2: 3, //~ WARN use of unstable
        };

        let _ = x.inherit;
        //~^ ERROR use of deprecated item
        //~^^ WARN use of unstable
        let _ = x.override1;
        let _ = x.override2; //~ WARN use of unstable

        let Deprecated {
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
            inherit: _,
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
            override1: _,
            override2: _ //~ WARN use of unstable
        } = x;

        let Deprecated
            //~^ ERROR use of deprecated item
            //~^^ WARN use of unstable
            // the patterns are all fine:
            { .. } = x;

        let x = Deprecated2(1, 2, 3);
        //~^ ERROR use of deprecated item
        //~^^ WARN use of unstable

        let _ = x.0;
        //~^ ERROR use of deprecated item
        //~^^ WARN use of unstable
        let _ = x.1;
        let _ = x.2; //~ WARN use of unstable

        let Deprecated2
        //~^ ERROR use of deprecated item
        //~^^ WARN use of unstable
            (_,
             //~^ ERROR use of deprecated item
             //~^^ WARN use of unstable
             _,
             _) //~ WARN use of unstable
            = x;
        let Deprecated2
        //~^ ERROR use of deprecated item
        //~^^ WARN use of unstable
            // the patterns are all fine:
            (..) = x;
    }
}

mod this_crate {
    #[stable(feature = "rust1", since = "1.0.0")]
    struct Stable {
        inherit: u8,
        #[unstable(feature = "test_feature")]
        override1: u8,
        #[deprecated(since = "1.0.0")]
        #[unstable(feature = "test_feature")]
        override2: u8,
    }

    #[stable(feature = "rust1", since = "1.0.0")]
    struct Stable2(u8,
                   #[stable(feature = "rust1", since = "1.0.0")] u8,
                   #[unstable(feature = "test_feature")] #[deprecated(since = "1.0.0")] u8);

    #[unstable(feature = "test_feature")]
    struct Unstable {
        inherit: u8,
        #[stable(feature = "rust1", since = "1.0.0")]
        override1: u8,
        #[deprecated(since = "1.0.0")]
        #[unstable(feature = "test_feature")]
        override2: u8,
    }

    #[unstable(feature = "test_feature")]
    struct Unstable2(u8,
                     #[stable(feature = "rust1", since = "1.0.0")] u8,
                     #[unstable(feature = "test_feature")] #[deprecated(since = "1.0.0")] u8);

    #[unstable(feature = "test_feature")]
    #[deprecated(feature = "rust1", since = "1.0.0")]
    struct Deprecated {
        inherit: u8,
        #[stable(feature = "rust1", since = "1.0.0")]
        override1: u8,
        #[unstable(feature = "test_feature")]
        override2: u8,
    }

    #[unstable(feature = "test_feature")]
    #[deprecated(feature = "rust1", since = "1.0.0")]
    struct Deprecated2(u8,
                       #[stable(feature = "rust1", since = "1.0.0")] u8,
                       #[unstable(feature = "test_feature")] u8);

    pub fn foo() {
        let x = Stable {
            inherit: 1,
            override1: 2,
            override2: 3,
            //~^ ERROR use of deprecated item
        };

        let _ = x.inherit;
        let _ = x.override1;
        let _ = x.override2;
        //~^ ERROR use of deprecated item

        let Stable {
            inherit: _,
            override1: _,
            override2: _
            //~^ ERROR use of deprecated item
        } = x;
        // all fine
        let Stable { .. } = x;

        let x = Stable2(1, 2, 3);

        let _ = x.0;
        let _ = x.1;
        let _ = x.2;
        //~^ ERROR use of deprecated item

        let Stable2(_,
                   _,
                   _)
            //~^ ERROR use of deprecated item
            = x;
        // all fine
        let Stable2(..) = x;


        let x = Unstable {
            inherit: 1,
            override1: 2,
            override2: 3,
            //~^ ERROR use of deprecated item
        };

        let _ = x.inherit;
        let _ = x.override1;
        let _ = x.override2;
        //~^ ERROR use of deprecated item

        let Unstable {
            inherit: _,
            override1: _,
            override2: _
            //~^ ERROR use of deprecated item
        } = x;

        let Unstable
            // the patterns are all fine:
            { .. } = x;


        let x = Unstable2(1, 2, 3);

        let _ = x.0;
        let _ = x.1;
        let _ = x.2;
        //~^ ERROR use of deprecated item

        let Unstable2
            (_,
             _,
             _)
            //~^ ERROR use of deprecated item
            = x;
        let Unstable2
            // the patterns are all fine:
            (..) = x;


        let x = Deprecated {
            //~^ ERROR use of deprecated item
            inherit: 1,
            //~^ ERROR use of deprecated item
            override1: 2,
            override2: 3,
        };

        let _ = x.inherit;
        //~^ ERROR use of deprecated item
        let _ = x.override1;
        let _ = x.override2;

        let Deprecated {
            //~^ ERROR use of deprecated item
            inherit: _,
            //~^ ERROR use of deprecated item
            override1: _,
            override2: _
        } = x;

        let Deprecated
            //~^ ERROR use of deprecated item
            // the patterns are all fine:
            { .. } = x;

        let x = Deprecated2(1, 2, 3);
        //~^ ERROR use of deprecated item

        let _ = x.0;
        //~^ ERROR use of deprecated item
        let _ = x.1;
        let _ = x.2;

        let Deprecated2
        //~^ ERROR use of deprecated item
            (_,
             //~^ ERROR use of deprecated item
             _,
             _)
            = x;
        let Deprecated2
        //~^ ERROR use of deprecated item
            // the patterns are all fine:
            (..) = x;
    }
}

fn main() {}
