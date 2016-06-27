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
 * Inline assembly support.
 */
use self::State::*;

use syntax::ast;
use syntax::codemap;
use syntax::ext::base;
use syntax::ext::base::*;
use syntax::feature_gate;
use syntax::parse::token::intern;
use syntax::parse::{self, token};
use syntax::ptr::P;
use syntax::ast::AsmDialect;
use syntax_pos::Span;
use syntax::tokenstream;

enum State {
    Asm,
    Outputs,
    Inputs,
    Clobbers,
    Options,
    StateNone
}

impl State {
    fn next(&self) -> State {
        match *self {
            Asm       => Outputs,
            Outputs   => Inputs,
            Inputs    => Clobbers,
            Clobbers  => Options,
            Options   => StateNone,
            StateNone => StateNone
        }
    }
}

const OPTIONS: &'static [&'static str] = &["volatile", "alignstack", "intel"];

pub fn expand_asm<'cx>(cx: &'cx mut ExtCtxt, sp: Span, tts: &[tokenstream::TokenTree])
                       -> Box<base::MacResult+'cx> {
    if !cx.ecfg.enable_asm() {
        feature_gate::emit_feature_err(
            &cx.parse_sess.span_diagnostic, "asm", sp,
            feature_gate::GateIssue::Language,
            feature_gate::EXPLAIN_ASM);
        return DummyResult::expr(sp);
    }

    // Split the tts before the first colon, to avoid `asm!("x": y)`  being
    // parsed as `asm!(z)` with `z = "x": y` which is type ascription.
    let first_colon = tts.iter().position(|tt| {
        match *tt {
            tokenstream::TokenTree::Token(_, token::Colon) |
            tokenstream::TokenTree::Token(_, token::ModSep) => true,
            _ => false
        }
    }).unwrap_or(tts.len());
    let mut p = cx.new_parser_from_tts(&tts[first_colon..]);
    let mut asm = token::InternedString::new("");
    let mut asm_str_style = None;
    let mut outputs = Vec::new();
    let mut inputs = Vec::new();
    let mut clobs = Vec::new();
    let mut volatile = false;
    let mut alignstack = false;
    let mut dialect = AsmDialect::Att;

    let mut state = Asm;

    'statement: loop {
        match state {
            Asm => {
                if asm_str_style.is_some() {
                    // If we already have a string with instructions,
                    // ending up in Asm state again is an error.
                    cx.span_err(sp, "malformed inline assembly");
                    return DummyResult::expr(sp);
                }
                // Nested parser, stop before the first colon (see above).
                let mut p2 = cx.new_parser_from_tts(&tts[..first_colon]);
                let (s, style) = match expr_to_string(cx, panictry!(p2.parse_expr()),
                                                   "inline assembly must be a string literal") {
                    Some((s, st)) => (s, st),
                    // let compilation continue
                    None => return DummyResult::expr(sp),
                };

                // This is most likely malformed.
                if p2.token != token::Eof {
                    let mut extra_tts = panictry!(p2.parse_all_token_trees());
                    extra_tts.extend(tts[first_colon..].iter().cloned());
                    p = parse::tts_to_parser(cx.parse_sess, extra_tts, cx.cfg());
                }

                asm = s;
                asm_str_style = Some(style);
            }
            Outputs => {
                while p.token != token::Eof &&
                      p.token != token::Colon &&
                      p.token != token::ModSep {

                    if !outputs.is_empty() {
                        p.eat(&token::Comma);
                    }

                    let (constraint, _str_style) = panictry!(p.parse_str());

                    let span = p.last_span;

                    panictry!(p.expect(&token::OpenDelim(token::Paren)));
                    let out = panictry!(p.parse_expr());
                    panictry!(p.expect(&token::CloseDelim(token::Paren)));

                    // Expands a read+write operand into two operands.
                    //
                    // Use '+' modifier when you want the same expression
                    // to be both an input and an output at the same time.
                    // It's the opposite of '=&' which means that the memory
                    // cannot be shared with any other operand (usually when
                    // a register is clobbered early.)
                    let mut ch = constraint.chars();
                    let output = match ch.next() {
                        Some('=') => None,
                        Some('+') => {
                            Some(token::intern_and_get_ident(&format!(
                                        "={}", ch.as_str())))
                        }
                        _ => {
                            cx.span_err(span, "output operand constraint lacks '=' or '+'");
                            None
                        }
                    };

                    let is_rw = output.is_some();
                    let is_indirect = constraint.contains("*");
                    outputs.push(ast::InlineAsmOutput {
                        constraint: output.unwrap_or(constraint.clone()),
                        expr: out,
                        is_rw: is_rw,
                        is_indirect: is_indirect,
                    });
                }
            }
            Inputs => {
                while p.token != token::Eof &&
                      p.token != token::Colon &&
                      p.token != token::ModSep {

                    if !inputs.is_empty() {
                        p.eat(&token::Comma);
                    }

                    let (constraint, _str_style) = panictry!(p.parse_str());

                    if constraint.starts_with("=") {
                        cx.span_err(p.last_span, "input operand constraint contains '='");
                    } else if constraint.starts_with("+") {
                        cx.span_err(p.last_span, "input operand constraint contains '+'");
                    }

                    panictry!(p.expect(&token::OpenDelim(token::Paren)));
                    let input = panictry!(p.parse_expr());
                    panictry!(p.expect(&token::CloseDelim(token::Paren)));

                    inputs.push((constraint, input));
                }
            }
            Clobbers => {
                while p.token != token::Eof &&
                      p.token != token::Colon &&
                      p.token != token::ModSep {

                    if !clobs.is_empty() {
                        p.eat(&token::Comma);
                    }

                    let (s, _str_style) = panictry!(p.parse_str());

                    if OPTIONS.iter().any(|&opt| s == opt) {
                        cx.span_warn(p.last_span, "expected a clobber, found an option");
                    }
                    clobs.push(s);
                }
            }
            Options => {
                let (option, _str_style) = panictry!(p.parse_str());

                if option == "volatile" {
                    // Indicates that the inline assembly has side effects
                    // and must not be optimized out along with its outputs.
                    volatile = true;
                } else if option == "alignstack" {
                    alignstack = true;
                } else if option == "intel" {
                    dialect = AsmDialect::Intel;
                } else {
                    cx.span_warn(p.last_span, "unrecognized option");
                }

                if p.token == token::Comma {
                    p.eat(&token::Comma);
                }
            }
            StateNone => ()
        }

        loop {
            // MOD_SEP is a double colon '::' without space in between.
            // When encountered, the state must be advanced twice.
            match (&p.token, state.next(), state.next().next()) {
                (&token::Colon, StateNone, _)   |
                (&token::ModSep, _, StateNone) => {
                    p.bump();
                    break 'statement;
                }
                (&token::Colon, st, _)   |
                (&token::ModSep, _, st) => {
                    p.bump();
                    state = st;
                }
                (&token::Eof, _, _) => break 'statement,
                _ => break
            }
        }
    }

    let expn_id = cx.codemap().record_expansion(codemap::ExpnInfo {
        call_site: sp,
        callee: codemap::NameAndSpan {
            format: codemap::MacroBang(intern("asm")),
            span: None,
            allow_internal_unstable: false,
        },
    });

    MacEager::expr(P(ast::Expr {
        id: ast::DUMMY_NODE_ID,
        node: ast::ExprKind::InlineAsm(ast::InlineAsm {
            asm: token::intern_and_get_ident(&asm),
            asm_str_style: asm_str_style.unwrap(),
            outputs: outputs,
            inputs: inputs,
            clobbers: clobs,
            volatile: volatile,
            alignstack: alignstack,
            dialect: dialect,
            expn_id: expn_id,
        }),
        span: sp,
        attrs: ast::ThinVec::new(),
    }))
}
