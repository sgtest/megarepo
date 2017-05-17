// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-stage1
// ignore-cross-compile
#![feature(quote, rustc_private)]

extern crate syntax;

use syntax::ext::base::{ExtCtxt, DummyResolver};
use syntax::ext::expand::ExpansionConfig;
use syntax::parse::ParseSess;
use syntax::codemap::{FilePathMapping, dummy_spanned};
use syntax::print::pprust::expr_to_string;
use syntax::ast::{Expr, ExprKind, LitKind, StrStyle, RangeLimits};
use syntax::symbol::Symbol;
use syntax::ptr::P;

use std::rc::Rc;

fn main() {
    let parse_sess = ParseSess::new(FilePathMapping::empty());
    let exp_cfg = ExpansionConfig::default("issue_35829".to_owned());
    let mut resolver = DummyResolver;
    let cx = ExtCtxt::new(&parse_sess, exp_cfg, &mut resolver);

    // check byte string
    let byte_string = quote_expr!(&cx, b"one");
    let byte_string_lit_kind = LitKind::ByteStr(Rc::new(b"one".to_vec()));
    assert_eq!(byte_string.node, ExprKind::Lit(P(dummy_spanned(byte_string_lit_kind))));

    // check raw byte string
    let raw_byte_string = quote_expr!(&cx, br###"#"two"#"###);
    let raw_byte_string_lit_kind = LitKind::ByteStr(Rc::new(b"#\"two\"#".to_vec()));
    assert_eq!(raw_byte_string.node, ExprKind::Lit(P(dummy_spanned(raw_byte_string_lit_kind))));

    // check dotdotdot
    let closed_range = quote_expr!(&cx, 0 ... 1);
    assert_eq!(closed_range.node, ExprKind::Range(
        Some(quote_expr!(&cx, 0)),
        Some(quote_expr!(&cx, 1)),
        RangeLimits::Closed
    ));

    // test case from 35829
    let expr_35829 = quote_expr!(&cx, std::io::stdout().write(b"one"));
    assert_eq!(expr_to_string(&expr_35829), r#"std::io::stdout().write(b"one")"#);
}
