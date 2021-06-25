// run-rustfix

#![feature(rust_2018_preview, crate_visibility_modifier)]
#![deny(absolute_paths_not_starting_with_crate)]
#![allow(unused_imports)]
#![allow(dead_code)]

crate mod foo {
    crate mod bar {
        crate mod baz { }
        crate mod baz1 { }

        crate struct XX;
    }
}

use foo::{bar::{baz::{}}};
//~^ ERROR absolute paths must start with
//~| WARN this is accepted in the current edition

use foo::{bar::{XX, baz::{}}};
//~^ ERROR absolute paths must start with
//~| WARN this is accepted in the current edition

use foo::{bar::{baz::{}, baz1::{}}};
//~^ ERROR absolute paths must start with
//~| WARN this is accepted in the current edition

fn main() {
}
