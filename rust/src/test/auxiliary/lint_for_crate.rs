// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// force-host

#![feature(plugin_registrar)]
#![feature(box_syntax)]

extern crate syntax;
#[macro_use] extern crate rustc;

use syntax::{ast, attr};
use rustc::lint::{Context, LintPass, LintPassObject, LintArray};
use rustc::plugin::Registry;

declare_lint!(CRATE_NOT_OKAY, Warn, "crate not marked with #![crate_okay]");

struct Pass;

impl LintPass for Pass {
    fn get_lints(&self) -> LintArray {
        lint_array!(CRATE_NOT_OKAY)
    }

    fn check_crate(&mut self, cx: &Context, krate: &ast::Crate) {
        if !attr::contains_name(&krate.attrs, "crate_okay") {
            cx.span_lint(CRATE_NOT_OKAY, krate.span,
                         "crate is not marked with #![crate_okay]");
        }
    }
}

#[plugin_registrar]
pub fn plugin_registrar(reg: &mut Registry) {
    reg.register_lint_pass(box Pass as LintPassObject);
}
