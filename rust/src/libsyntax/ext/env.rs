// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*
 * The compiler code necessary to support the env! extension.  Eventually this
 * should all get sucked into either the compiler syntax extension plugin
 * interface.
 */

use ast;
use codemap::Span;
use ext::base::*;
use ext::base;
use ext::build::AstBuilder;
use parse::token;

use std::os;

pub fn expand_option_env(cx: &mut ExtCtxt, sp: Span, tts: &[ast::TokenTree])
    -> base::MacResult {
    let var = match get_single_str_from_tts(cx, sp, tts, "option_env!") {
        None => return MacResult::dummy_expr(sp),
        Some(v) => v
    };

    let e = match os::getenv(var) {
      None => quote_expr!(cx, ::std::option::None::<&'static str>),
      Some(s) => quote_expr!(cx, ::std::option::Some($s))
    };
    MRExpr(e)
}

pub fn expand_env(cx: &mut ExtCtxt, sp: Span, tts: &[ast::TokenTree])
    -> base::MacResult {
    let exprs = match get_exprs_from_tts(cx, sp, tts) {
        Some(ref exprs) if exprs.len() == 0 => {
            cx.span_err(sp, "env! takes 1 or 2 arguments");
            return MacResult::dummy_expr(sp);
        }
        None => return MacResult::dummy_expr(sp),
        Some(exprs) => exprs
    };

    let var = match expr_to_str(cx, exprs[0], "expected string literal") {
        None => return MacResult::dummy_expr(sp),
        Some((v, _style)) => v
    };
    let msg = match exprs.len() {
        1 => {
            token::intern_and_get_ident(format!("environment variable `{}` \
                                                 not defined",
                                                var))
        }
        2 => {
            match expr_to_str(cx, exprs[1], "expected string literal") {
                None => return MacResult::dummy_expr(sp),
                Some((s, _style)) => s
            }
        }
        _ => {
            cx.span_err(sp, "env! takes 1 or 2 arguments");
            return MacResult::dummy_expr(sp);
        }
    };

    let e = match os::getenv(var.get()) {
        None => {
            cx.span_err(sp, msg.get());
            cx.expr_uint(sp, 0)
        }
        Some(s) => cx.expr_str(sp, token::intern_and_get_ident(s))
    };
    MRExpr(e)
}
