// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use ast;
use codemap::{BytePos, spanned};
use parse::lexer::reader;
use parse::parser::Parser;
use parse::token;

use core::option::{None, Option, Some};

use opt_vec;
use opt_vec::OptVec;

// SeqSep : a sequence separator (token)
// and whether a trailing separator is allowed.
pub struct SeqSep {
    sep: Option<token::Token>,
    trailing_sep_allowed: bool
}

pub fn seq_sep_trailing_disallowed(+t: token::Token) -> SeqSep {
    SeqSep {
        sep: Some(t),
        trailing_sep_allowed: false,
    }
}
pub fn seq_sep_trailing_allowed(+t: token::Token) -> SeqSep {
    SeqSep {
        sep: Some(t),
        trailing_sep_allowed: true,
    }
}
pub fn seq_sep_none() -> SeqSep {
    SeqSep {
        sep: None,
        trailing_sep_allowed: false,
    }
}

pub fn token_to_str(reader: @reader, token: &token::Token) -> ~str {
    token::to_str(reader.interner(), token)
}

pub impl Parser {
    fn unexpected_last(&self, t: &token::Token) -> ! {
        self.span_fatal(
            *self.last_span,
            fmt!(
                "unexpected token: `%s`",
                token_to_str(self.reader, t)
            )
        );
    }

    fn unexpected(&self) -> ! {
        self.fatal(
            fmt!(
                "unexpected token: `%s`",
                token_to_str(self.reader, &copy *self.token)
            )
        );
    }

    // expect and consume the token t. Signal an error if
    // the next token is not t.
    fn expect(&self, t: &token::Token) {
        if *self.token == *t {
            self.bump();
        } else {
            self.fatal(
                fmt!(
                    "expected `%s` but found `%s`",
                    token_to_str(self.reader, t),
                    token_to_str(self.reader, &copy *self.token)
                )
            )
        }
    }

    fn parse_ident(&self) -> ast::ident {
        self.check_strict_keywords();
        self.check_reserved_keywords();
        match *self.token {
            token::IDENT(i, _) => {
                self.bump();
                i
            }
            token::INTERPOLATED(token::nt_ident(*)) => {
                self.bug(
                    ~"ident interpolation not converted to real token"
                );
            }
            _ => {
                self.fatal(
                    fmt!(
                        "expected ident, found `%s`",
                        token_to_str(self.reader, &copy *self.token)
                    )
                );
            }
        }
    }

    fn parse_path_list_ident(&self) -> ast::path_list_ident {
        let lo = self.span.lo;
        let ident = self.parse_ident();
        let hi = self.span.hi;
        spanned(lo, hi, ast::path_list_ident_ { name: ident,
                                                id: self.get_id() })
    }

    // consume token 'tok' if it exists. Returns true if the given
    // token was present, false otherwise.
    fn eat(&self, tok: &token::Token) -> bool {
        return if *self.token == *tok { self.bump(); true } else { false };
    }

    // Storing keywords as interned idents instead of strings would be nifty.

    // A sanity check that the word we are asking for is a known keyword
    fn require_keyword(&self, word: &~str) {
        if !self.keywords.contains_key(word) {
            self.bug(fmt!("unknown keyword: %s", *word));
        }
    }

    pure fn token_is_word(&self, word: &~str, tok: &token::Token) -> bool {
        match *tok {
            token::IDENT(sid, false) => { *self.id_to_str(sid) == *word }
             _ => { false }
        }
    }

    fn token_is_keyword(&self, word: &~str, tok: &token::Token) -> bool {
        self.require_keyword(word);
        self.token_is_word(word, tok)
    }

    fn is_keyword(&self, word: &~str) -> bool {
        self.token_is_keyword(word, &copy *self.token)
    }

    fn is_any_keyword(&self, tok: &token::Token) -> bool {
        match *tok {
          token::IDENT(sid, false) => {
            self.keywords.contains_key(self.id_to_str(sid))
          }
          _ => false
        }
    }

    fn eat_keyword(&self, word: &~str) -> bool {
        self.require_keyword(word);
        let is_kw = match *self.token {
            token::IDENT(sid, false) => *word == *self.id_to_str(sid),
            _ => false
        };
        if is_kw { self.bump() }
        is_kw
    }

    fn expect_keyword(&self, word: &~str) {
        self.require_keyword(word);
        if !self.eat_keyword(word) {
            self.fatal(
                fmt!(
                    "expected `%s`, found `%s`",
                    *word,
                    token_to_str(self.reader, &copy *self.token)
                )
            );
        }
    }

    fn is_strict_keyword(&self, word: &~str) -> bool {
        self.strict_keywords.contains_key(word)
    }

    fn check_strict_keywords(&self) {
        match *self.token {
            token::IDENT(_, false) => {
                let w = token_to_str(self.reader, &copy *self.token);
                self.check_strict_keywords_(&w);
            }
            _ => ()
        }
    }

    fn check_strict_keywords_(&self, w: &~str) {
        if self.is_strict_keyword(w) {
            self.fatal(fmt!("found `%s` in ident position", *w));
        }
    }

    fn is_reserved_keyword(&self, word: &~str) -> bool {
        self.reserved_keywords.contains_key(word)
    }

    fn check_reserved_keywords(&self) {
        match *self.token {
            token::IDENT(_, false) => {
                let w = token_to_str(self.reader, &copy *self.token);
                self.check_reserved_keywords_(&w);
            }
            _ => ()
        }
    }

    fn check_reserved_keywords_(&self, w: &~str) {
        if self.is_reserved_keyword(w) {
            self.fatal(fmt!("`%s` is a reserved keyword", *w));
        }
    }

    // expect and consume a GT. if a >> is seen, replace it
    // with a single > and continue.
    fn expect_gt(&self) {
        if *self.token == token::GT {
            self.bump();
        } else if *self.token == token::BINOP(token::SHR) {
            self.replace_token(
                token::GT,
                self.span.lo + BytePos(1u),
                self.span.hi
            );
        } else {
            let mut s: ~str = ~"expected `";
            s += token_to_str(self.reader, &token::GT);
            s += ~"`, found `";
            s += token_to_str(self.reader, &copy *self.token);
            s += ~"`";
            self.fatal(s);
        }
    }

    // parse a sequence bracketed by '<' and '>', stopping
    // before the '>'.
    fn parse_seq_to_before_gt<T: Copy>(
        &self,
        sep: Option<token::Token>,
        f: &fn(&Parser) -> T
    ) -> OptVec<T> {
        let mut first = true;
        let mut v = opt_vec::Empty;
        while *self.token != token::GT
            && *self.token != token::BINOP(token::SHR) {
            match sep {
              Some(ref t) => {
                if first { first = false; }
                else { self.expect(t); }
              }
              _ => ()
            }
            v.push(f(self));
        }
        return v;
    }

    fn parse_seq_to_gt<T: Copy>(
        &self,
        sep: Option<token::Token>,
        f: &fn(&Parser) -> T
    ) -> OptVec<T> {
        let v = self.parse_seq_to_before_gt(sep, f);
        self.expect_gt();
        return v;
    }

    // parse a sequence, including the closing delimiter. The function
    // f must consume tokens until reaching the next separator or
    // closing bracket.
    fn parse_seq_to_end<T: Copy>(
        &self,
        ket: &token::Token,
        sep: SeqSep,
        f: &fn(&Parser) -> T
    ) -> ~[T] {
        let val = self.parse_seq_to_before_end(ket, sep, f);
        self.bump();
        val
    }

    // parse a sequence, not including the closing delimiter. The function
    // f must consume tokens until reaching the next separator or
    // closing bracket.
    fn parse_seq_to_before_end<T: Copy>(
        &self,
        ket: &token::Token,
        sep: SeqSep,
        f: &fn(&Parser) -> T
    ) -> ~[T] {
        let mut first: bool = true;
        let mut v: ~[T] = ~[];
        while *self.token != *ket {
            match sep.sep {
              Some(ref t) => {
                if first { first = false; }
                else { self.expect(t); }
              }
              _ => ()
            }
            if sep.trailing_sep_allowed && *self.token == *ket { break; }
            v.push(f(self));
        }
        return v;
    }

    // parse a sequence, including the closing delimiter. The function
    // f must consume tokens until reaching the next separator or
    // closing bracket.
    fn parse_unspanned_seq<T: Copy>(
        &self,
        bra: &token::Token,
        ket: &token::Token,
        sep: SeqSep,
        f: &fn(&Parser) -> T
    ) -> ~[T] {
        self.expect(bra);
        let result = self.parse_seq_to_before_end(ket, sep, f);
        self.bump();
        result
    }

    // NB: Do not use this function unless you actually plan to place the
    // spanned list in the AST.
    fn parse_seq<T: Copy>(
        &self,
        bra: &token::Token,
        ket: &token::Token,
        sep: SeqSep,
        f: &fn(&Parser) -> T
    ) -> spanned<~[T]> {
        let lo = self.span.lo;
        self.expect(bra);
        let result = self.parse_seq_to_before_end(ket, sep, f);
        let hi = self.span.hi;
        self.bump();
        spanned(lo, hi, result)
    }
}
