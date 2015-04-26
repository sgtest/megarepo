// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-cross-compile

#![feature(quote, rustc_private)]

extern crate syntax;

use syntax::ast;
use syntax::codemap::{self, DUMMY_SP};
use syntax::parse;
use syntax::print::pprust;

fn main() {
    let ps = syntax::parse::new_parse_sess();
    let mut cx = syntax::ext::base::ExtCtxt::new(
        &ps, vec![],
        syntax::ext::expand::ExpansionConfig::default("qquote".to_string()));
    cx.bt_push(syntax::codemap::ExpnInfo {
        call_site: DUMMY_SP,
        callee: syntax::codemap::NameAndSpan {
            name: "".to_string(),
            format: syntax::codemap::MacroBang,
            allow_internal_unstable: false,
            span: None,
        }
    });
    let cx = &mut cx;

    assert_eq!(pprust::expr_to_string(&*quote_expr!(&cx, 23)), "23");

    let expr = quote_expr!(&cx, 2 - $abcd + 7); //~ ERROR unresolved name `abcd`
    assert_eq!(pprust::expr_to_string(&*expr), "2 - $abcd + 7");
}
