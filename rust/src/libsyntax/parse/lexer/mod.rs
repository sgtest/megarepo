// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ast;
use codemap::{BytePos, CharPos, CodeMap, Pos, Span};
use codemap;
use diagnostic::SpanHandler;
use ext::tt::transcribe::tt_next_token;
use parse::token;
use parse::token::{str_to_ident};

use std::char;
use std::mem::replace;
use std::num::from_str_radix;
use std::rc::Rc;
use std::str;

pub use ext::tt::transcribe::{TtReader, new_tt_reader};

pub mod comments;

pub trait Reader {
    fn is_eof(&self) -> bool;
    fn next_token(&mut self) -> TokenAndSpan;
    /// Report a fatal error with the current span.
    fn fatal(&self, &str) -> !;
    /// Report a non-fatal error with the current span.
    fn err(&self, &str);
    fn peek(&self) -> TokenAndSpan;
}

#[deriving(Clone, PartialEq, Eq, Show)]
pub struct TokenAndSpan {
    pub tok: token::Token,
    pub sp: Span,
}

pub struct StringReader<'a> {
    pub span_diagnostic: &'a SpanHandler,
    // The absolute offset within the codemap of the next character to read
    pub pos: BytePos,
    // The absolute offset within the codemap of the last character read(curr)
    pub last_pos: BytePos,
    // The column of the next character to read
    pub col: CharPos,
    // The last character to be read
    pub curr: Option<char>,
    pub filemap: Rc<codemap::FileMap>,
    /* cached: */
    pub peek_tok: token::Token,
    pub peek_span: Span,
}

impl<'a> Reader for StringReader<'a> {
    fn is_eof(&self) -> bool { self.curr.is_none() }
    // return the next token. EFFECT: advances the string_reader.
    fn next_token(&mut self) -> TokenAndSpan {
        let ret_val = TokenAndSpan {
            tok: replace(&mut self.peek_tok, token::UNDERSCORE),
            sp: self.peek_span,
        };
        self.advance_token();
        ret_val
    }
    fn fatal(&self, m: &str) -> ! {
        self.span_diagnostic.span_fatal(self.peek_span, m)
    }
    fn err(&self, m: &str) {
        self.span_diagnostic.span_err(self.peek_span, m)
    }
    fn peek(&self) -> TokenAndSpan {
        // FIXME(pcwalton): Bad copy!
        TokenAndSpan {
            tok: self.peek_tok.clone(),
            sp: self.peek_span,
        }
    }
}

impl<'a> Reader for TtReader<'a> {
    fn is_eof(&self) -> bool {
        self.cur_tok == token::EOF
    }
    fn next_token(&mut self) -> TokenAndSpan {
        let r = tt_next_token(self);
        debug!("TtReader: r={:?}", r);
        r
    }
    fn fatal(&self, m: &str) -> ! {
        self.sp_diag.span_fatal(self.cur_span, m);
    }
    fn err(&self, m: &str) {
        self.sp_diag.span_err(self.cur_span, m);
    }
    fn peek(&self) -> TokenAndSpan {
        TokenAndSpan {
            tok: self.cur_tok.clone(),
            sp: self.cur_span,
        }
    }
}

impl<'a> StringReader<'a> {
    /// For comments.rs, which hackily pokes into pos and curr
    pub fn new_raw<'b>(span_diagnostic: &'b SpanHandler,
                   filemap: Rc<codemap::FileMap>) -> StringReader<'b> {
        let mut sr = StringReader {
            span_diagnostic: span_diagnostic,
            pos: filemap.start_pos,
            last_pos: filemap.start_pos,
            col: CharPos(0),
            curr: Some('\n'),
            filemap: filemap,
            /* dummy values; not read */
            peek_tok: token::EOF,
            peek_span: codemap::DUMMY_SP,
        };
        sr.bump();
        sr
    }

    pub fn new<'b>(span_diagnostic: &'b SpanHandler,
                   filemap: Rc<codemap::FileMap>) -> StringReader<'b> {
        let mut sr = StringReader::new_raw(span_diagnostic, filemap);
        sr.advance_token();
        sr
    }

    pub fn curr_is(&self, c: char) -> bool {
        self.curr == Some(c)
    }

    /// Report a lexical error spanning [`from_pos`, `to_pos`)
    fn fatal_span(&mut self, from_pos: BytePos, to_pos: BytePos, m: &str) -> ! {
        self.peek_span = codemap::mk_sp(from_pos, to_pos);
        self.fatal(m);
    }

    fn err_span(&mut self, from_pos: BytePos, to_pos: BytePos, m: &str) {
        self.peek_span = codemap::mk_sp(from_pos, to_pos);
        self.err(m);
    }

    /// Report a lexical error spanning [`from_pos`, `to_pos`), appending an
    /// escaped character to the error message
    fn fatal_span_char(&mut self, from_pos: BytePos, to_pos: BytePos, m: &str, c: char) -> ! {
        let mut m = m.to_string();
        m.push_str(": ");
        char::escape_default(c, |c| m.push_char(c));
        self.fatal_span(from_pos, to_pos, m.as_slice());
    }

    /// Report a lexical error spanning [`from_pos`, `to_pos`), appending an
    /// escaped character to the error message
    fn err_span_char(&mut self, from_pos: BytePos, to_pos: BytePos, m: &str, c: char) {
        let mut m = m.to_string();
        m.push_str(": ");
        char::escape_default(c, |c| m.push_char(c));
        self.err_span(from_pos, to_pos, m.as_slice());
    }

    /// Report a lexical error spanning [`from_pos`, `to_pos`), appending the
    /// offending string to the error message
    fn fatal_span_verbose(&mut self, from_pos: BytePos, to_pos: BytePos, mut m: String) -> ! {
        m.push_str(": ");
        let from = self.byte_offset(from_pos).to_uint();
        let to = self.byte_offset(to_pos).to_uint();
        m.push_str(self.filemap.src.as_slice().slice(from, to));
        self.fatal_span(from_pos, to_pos, m.as_slice());
    }

    /// Advance peek_tok and peek_span to refer to the next token, and
    /// possibly update the interner.
    fn advance_token(&mut self) {
        match self.consume_whitespace_and_comments() {
            Some(comment) => {
                self.peek_span = comment.sp;
                self.peek_tok = comment.tok;
            },
            None => {
                if self.is_eof() {
                    self.peek_tok = token::EOF;
                } else {
                    let start_bytepos = self.last_pos;
                    self.peek_tok = self.next_token_inner();
                    self.peek_span = codemap::mk_sp(start_bytepos,
                                                    self.last_pos);
                };
            }
        }
    }

    fn byte_offset(&self, pos: BytePos) -> BytePos {
        (pos - self.filemap.start_pos)
    }

    /// Calls `f` with a string slice of the source text spanning from `start`
    /// up to but excluding `self.last_pos`, meaning the slice does not include
    /// the character `self.curr`.
    pub fn with_str_from<T>(&self, start: BytePos, f: |s: &str| -> T) -> T {
        self.with_str_from_to(start, self.last_pos, f)
    }

    /// Calls `f` with a string slice of the source text spanning from `start`
    /// up to but excluding `end`.
    fn with_str_from_to<T>(&self, start: BytePos, end: BytePos, f: |s: &str| -> T) -> T {
        f(self.filemap.src.as_slice().slice(
                self.byte_offset(start).to_uint(),
                self.byte_offset(end).to_uint()))
    }

    /// Advance the StringReader by one character. If a newline is
    /// discovered, add it to the FileMap's list of line start offsets.
    pub fn bump(&mut self) {
        self.last_pos = self.pos;
        let current_byte_offset = self.byte_offset(self.pos).to_uint();
        if current_byte_offset < self.filemap.src.len() {
            assert!(self.curr.is_some());
            let last_char = self.curr.unwrap();
            let next = self.filemap
                          .src
                          .as_slice()
                          .char_range_at(current_byte_offset);
            let byte_offset_diff = next.next - current_byte_offset;
            self.pos = self.pos + Pos::from_uint(byte_offset_diff);
            self.curr = Some(next.ch);
            self.col = self.col + CharPos(1u);
            if last_char == '\n' {
                self.filemap.next_line(self.last_pos);
                self.col = CharPos(0u);
            }

            if byte_offset_diff > 1 {
                self.filemap.record_multibyte_char(self.last_pos, byte_offset_diff);
            }
        } else {
            self.curr = None;
        }
    }

    pub fn nextch(&self) -> Option<char> {
        let offset = self.byte_offset(self.pos).to_uint();
        if offset < self.filemap.src.len() {
            Some(self.filemap.src.as_slice().char_at(offset))
        } else {
            None
        }
    }

    pub fn nextch_is(&self, c: char) -> bool {
        self.nextch() == Some(c)
    }

    pub fn nextnextch(&self) -> Option<char> {
        let offset = self.byte_offset(self.pos).to_uint();
        let s = self.filemap.deref().src.as_slice();
        if offset >= s.len() { return None }
        let str::CharRange { next, .. } = s.char_range_at(offset);
        if next < s.len() {
            Some(s.char_at(next))
        } else {
            None
        }
    }

    pub fn nextnextch_is(&self, c: char) -> bool {
        self.nextnextch() == Some(c)
    }

    /// PRECONDITION: self.curr is not whitespace
    /// Eats any kind of comment.
    /// Returns a Some(sugared-doc-attr) if one exists, None otherwise
    fn consume_any_line_comment(&mut self) -> Option<TokenAndSpan> {
        match self.curr {
            Some(c) => {
                if c.is_whitespace() {
                    self.span_diagnostic.span_err(codemap::mk_sp(self.last_pos, self.last_pos),
                                    "called consume_any_line_comment, but there was whitespace");
                }
            },
            None => { }
        }

        if self.curr_is('/') {
            match self.nextch() {
                Some('/') => {
                    self.bump();
                    self.bump();
                    // line comments starting with "///" or "//!" are doc-comments
                    if self.curr_is('/') || self.curr_is('!') {
                        let start_bpos = self.pos - BytePos(3);
                        while !self.curr_is('\n') && !self.is_eof() {
                            self.bump();
                        }
                        let ret = self.with_str_from(start_bpos, |string| {
                            // but comments with only more "/"s are not
                            if !is_line_non_doc_comment(string) {
                                Some(TokenAndSpan{
                                    tok: token::DOC_COMMENT(str_to_ident(string)),
                                    sp: codemap::mk_sp(start_bpos, self.pos)
                                })
                            } else {
                                None
                            }
                        });

                        if ret.is_some() {
                            return ret;
                        }
                    } else {
                        while !self.curr_is('\n') && !self.is_eof() { self.bump(); }
                    }
                    // Restart whitespace munch.
                    self.consume_whitespace_and_comments()
                }
                Some('*') => { self.bump(); self.bump(); self.consume_block_comment() }
                _ => None
            }
        } else if self.curr_is('#') {
            if self.nextch_is('!') {

                // Parse an inner attribute.
                if self.nextnextch_is('[') {
                    return None;
                }

                // I guess this is the only way to figure out if
                // we're at the beginning of the file...
                let cmap = CodeMap::new();
                cmap.files.borrow_mut().push(self.filemap.clone());
                let loc = cmap.lookup_char_pos_adj(self.last_pos);
                if loc.line == 1u && loc.col == CharPos(0u) {
                    while !self.curr_is('\n') && !self.is_eof() { self.bump(); }
                    return self.consume_whitespace_and_comments();
                }
            }
            None
        } else {
            None
        }
    }

    /// EFFECT: eats whitespace and comments.
    /// Returns a Some(sugared-doc-attr) if one exists, None otherwise.
    fn consume_whitespace_and_comments(&mut self) -> Option<TokenAndSpan> {
        while is_whitespace(self.curr) { self.bump(); }
        return self.consume_any_line_comment();
    }

    // might return a sugared-doc-attr
    fn consume_block_comment(&mut self) -> Option<TokenAndSpan> {
        // block comments starting with "/**" or "/*!" are doc-comments
        let is_doc_comment = self.curr_is('*') || self.curr_is('!');
        let start_bpos = self.pos - BytePos(if is_doc_comment {3} else {2});

        let mut level: int = 1;
        while level > 0 {
            if self.is_eof() {
                let msg = if is_doc_comment {
                    "unterminated block doc-comment"
                } else {
                    "unterminated block comment"
                };
                self.fatal_span(start_bpos, self.last_pos, msg);
            } else if self.curr_is('/') && self.nextch_is('*') {
                level += 1;
                self.bump();
                self.bump();
            } else if self.curr_is('*') && self.nextch_is('/') {
                level -= 1;
                self.bump();
                self.bump();
            } else {
                self.bump();
            }
        }

        let res = if is_doc_comment {
            self.with_str_from(start_bpos, |string| {
                // but comments with only "*"s between two "/"s are not
                if !is_block_non_doc_comment(string) {
                    Some(TokenAndSpan{
                            tok: token::DOC_COMMENT(str_to_ident(string)),
                            sp: codemap::mk_sp(start_bpos, self.pos)
                        })
                } else {
                    None
                }
            })
        } else {
            None
        };

        // restart whitespace munch.
        if res.is_some() { res } else { self.consume_whitespace_and_comments() }
    }

    fn scan_exponent(&mut self, start_bpos: BytePos) -> Option<String> {
        // \x00 hits the `return None` case immediately, so this is fine.
        let mut c = self.curr.unwrap_or('\x00');
        let mut rslt = String::new();
        if c == 'e' || c == 'E' {
            rslt.push_char(c);
            self.bump();
            c = self.curr.unwrap_or('\x00');
            if c == '-' || c == '+' {
                rslt.push_char(c);
                self.bump();
            }
            let exponent = self.scan_digits(10u);
            if exponent.len() > 0u {
                rslt.push_str(exponent.as_slice());
                return Some(rslt);
            } else {
                self.err_span(start_bpos, self.last_pos, "scan_exponent: bad fp literal");
                rslt.push_str("1"); // arbitrary placeholder exponent
                return Some(rslt);
            }
        } else {
            return None::<String>;
        }
    }

    fn scan_digits(&mut self, radix: uint) -> String {
        let mut rslt = String::new();
        loop {
            let c = self.curr;
            if c == Some('_') { self.bump(); continue; }
            match c.and_then(|cc| char::to_digit(cc, radix)) {
              Some(_) => {
                rslt.push_char(c.unwrap());
                self.bump();
              }
              _ => return rslt
            }
        };
    }

    fn check_float_base(&mut self, start_bpos: BytePos, last_bpos: BytePos, base: uint) {
        match base {
          16u => self.err_span(start_bpos, last_bpos, "hexadecimal float literal is not supported"),
          8u => self.err_span(start_bpos, last_bpos, "octal float literal is not supported"),
          2u => self.err_span(start_bpos, last_bpos, "binary float literal is not supported"),
          _ => ()
        }
    }

    fn scan_number(&mut self, c: char) -> token::Token {
        let mut num_str;
        let mut base = 10u;
        let mut c = c;
        let mut n = self.nextch().unwrap_or('\x00');
        let start_bpos = self.last_pos;
        if c == '0' && n == 'x' {
            self.bump();
            self.bump();
            base = 16u;
        } else if c == '0' && n == 'o' {
            self.bump();
            self.bump();
            base = 8u;
        } else if c == '0' && n == 'b' {
            self.bump();
            self.bump();
            base = 2u;
        }
        num_str = self.scan_digits(base);
        c = self.curr.unwrap_or('\x00');
        self.nextch();
        if c == 'u' || c == 'i' {
            enum Result { Signed(ast::IntTy), Unsigned(ast::UintTy) }
            let signed = c == 'i';
            let mut tp = {
                if signed { Signed(ast::TyI) }
                else { Unsigned(ast::TyU) }
            };
            self.bump();
            c = self.curr.unwrap_or('\x00');
            if c == '8' {
                self.bump();
                tp = if signed { Signed(ast::TyI8) }
                          else { Unsigned(ast::TyU8) };
            }
            n = self.nextch().unwrap_or('\x00');
            if c == '1' && n == '6' {
                self.bump();
                self.bump();
                tp = if signed { Signed(ast::TyI16) }
                          else { Unsigned(ast::TyU16) };
            } else if c == '3' && n == '2' {
                self.bump();
                self.bump();
                tp = if signed { Signed(ast::TyI32) }
                          else { Unsigned(ast::TyU32) };
            } else if c == '6' && n == '4' {
                self.bump();
                self.bump();
                tp = if signed { Signed(ast::TyI64) }
                          else { Unsigned(ast::TyU64) };
            }
            if num_str.len() == 0u {
                self.err_span(start_bpos, self.last_pos, "no valid digits found for number");
                num_str = "1".to_string();
            }
            let parsed = match from_str_radix::<u64>(num_str.as_slice(),
                                                     base as uint) {
                Some(p) => p,
                None => {
                    self.err_span(start_bpos, self.last_pos, "int literal is too large");
                    1
                }
            };

            match tp {
              Signed(t) => return token::LIT_INT(parsed as i64, t),
              Unsigned(t) => return token::LIT_UINT(parsed, t)
            }
        }
        let mut is_float = false;
        if self.curr_is('.') && !(ident_start(self.nextch()) || self.nextch_is('.')) {
            is_float = true;
            self.bump();
            let dec_part = self.scan_digits(10u);
            num_str.push_char('.');
            num_str.push_str(dec_part.as_slice());
        }
        match self.scan_exponent(start_bpos) {
          Some(ref s) => {
            is_float = true;
            num_str.push_str(s.as_slice());
          }
          None => ()
        }

        if self.curr_is('f') {
            self.bump();
            c = self.curr.unwrap_or('\x00');
            n = self.nextch().unwrap_or('\x00');
            if c == '3' && n == '2' {
                self.bump();
                self.bump();
                self.check_float_base(start_bpos, self.last_pos, base);
                return token::LIT_FLOAT(str_to_ident(num_str.as_slice()),
                                        ast::TyF32);
            } else if c == '6' && n == '4' {
                self.bump();
                self.bump();
                self.check_float_base(start_bpos, self.last_pos, base);
                return token::LIT_FLOAT(str_to_ident(num_str.as_slice()),
                                        ast::TyF64);
                /* FIXME (#2252): if this is out of range for either a
                32-bit or 64-bit float, it won't be noticed till the
                back-end.  */
            } else if c == '1' && n == '2' && self.nextnextch().unwrap_or('\x00') == '8' {
                self.bump();
                self.bump();
                self.bump();
                self.check_float_base(start_bpos, self.last_pos, base);
                return token::LIT_FLOAT(str_to_ident(num_str.as_slice()), ast::TyF128);
            }
            self.err_span(start_bpos, self.last_pos, "expected `f32`, `f64` or `f128` suffix");
        }
        if is_float {
            self.check_float_base(start_bpos, self.last_pos, base);
            return token::LIT_FLOAT_UNSUFFIXED(str_to_ident(
                    num_str.as_slice()));
        } else {
            if num_str.len() == 0u {
                self.err_span(start_bpos, self.last_pos, "no valid digits found for number");
                num_str = "1".to_string();
            }
            let parsed = match from_str_radix::<u64>(num_str.as_slice(),
                                                     base as uint) {
                Some(p) => p,
                None => {
                    self.err_span(start_bpos, self.last_pos, "int literal is too large");
                    1
                }
            };

            debug!("lexing {} as an unsuffixed integer literal",
                   num_str.as_slice());
            return token::LIT_INT_UNSUFFIXED(parsed as i64);
        }
    }


    fn scan_numeric_escape(&mut self, n_hex_digits: uint, delim: char) -> char {
        let mut accum_int = 0u32;
        let start_bpos = self.last_pos;
        for _ in range(0, n_hex_digits) {
            if self.is_eof() {
                self.fatal_span(start_bpos, self.last_pos, "unterminated numeric character escape");
            }
            if self.curr_is(delim) {
                self.err_span(start_bpos, self.last_pos, "numeric character escape is too short");
                break;
            }
            let c = self.curr.unwrap_or('\x00');
            accum_int *= 16;
            accum_int += c.to_digit(16).unwrap_or_else(|| {
                self.err_span_char(self.last_pos, self.pos,
                              "illegal character in numeric character escape", c);
                0
            }) as u32;
            self.bump();
        }

        match char::from_u32(accum_int) {
            Some(x) => x,
            None => {
                self.err_span(start_bpos, self.last_pos, "illegal numeric character escape");
                '?'
            }
        }
    }

    fn binop(&mut self, op: token::BinOp) -> token::Token {
        self.bump();
        if self.curr_is('=') {
            self.bump();
            return token::BINOPEQ(op);
        } else {
            return token::BINOP(op);
        }
    }

    /// Return the next token from the string, advances the input past that
    /// token, and updates the interner
    fn next_token_inner(&mut self) -> token::Token {
        let c = self.curr;
        if ident_start(c) && !self.nextch_is('"') && !self.nextch_is('#') {
            // Note: r as in r" or r#" is part of a raw string literal,
            // not an identifier, and is handled further down.

            let start = self.last_pos;
            while ident_continue(self.curr) {
                self.bump();
            }

            return self.with_str_from(start, |string| {
                if string == "_" {
                    token::UNDERSCORE
                } else {
                    let is_mod_name = self.curr_is(':') && self.nextch_is(':');

                    // FIXME: perform NFKC normalization here. (Issue #2253)
                    token::IDENT(str_to_ident(string), is_mod_name)
                }
            })
        }

        if is_dec_digit(c) {
            return self.scan_number(c.unwrap());
        }

        match c.expect("next_token_inner called at EOF") {
          // One-byte tokens.
          ';' => { self.bump(); return token::SEMI; }
          ',' => { self.bump(); return token::COMMA; }
          '.' => {
              self.bump();
              return if self.curr_is('.') {
                  self.bump();
                  if self.curr_is('.') {
                      self.bump();
                      token::DOTDOTDOT
                  } else {
                      token::DOTDOT
                  }
              } else {
                  token::DOT
              };
          }
          '(' => { self.bump(); return token::LPAREN; }
          ')' => { self.bump(); return token::RPAREN; }
          '{' => { self.bump(); return token::LBRACE; }
          '}' => { self.bump(); return token::RBRACE; }
          '[' => { self.bump(); return token::LBRACKET; }
          ']' => { self.bump(); return token::RBRACKET; }
          '@' => { self.bump(); return token::AT; }
          '#' => { self.bump(); return token::POUND; }
          '~' => { self.bump(); return token::TILDE; }
          ':' => {
            self.bump();
            if self.curr_is(':') {
                self.bump();
                return token::MOD_SEP;
            } else {
                return token::COLON;
            }
          }

          '$' => { self.bump(); return token::DOLLAR; }

          // Multi-byte tokens.
          '=' => {
            self.bump();
            if self.curr_is('=') {
                self.bump();
                return token::EQEQ;
            } else if self.curr_is('>') {
                self.bump();
                return token::FAT_ARROW;
            } else {
                return token::EQ;
            }
          }
          '!' => {
            self.bump();
            if self.curr_is('=') {
                self.bump();
                return token::NE;
            } else { return token::NOT; }
          }
          '<' => {
            self.bump();
            match self.curr.unwrap_or('\x00') {
              '=' => { self.bump(); return token::LE; }
              '<' => { return self.binop(token::SHL); }
              '-' => {
                self.bump();
                match self.curr.unwrap_or('\x00') {
                  _ => { return token::LARROW; }
                }
              }
              _ => { return token::LT; }
            }
          }
          '>' => {
            self.bump();
            match self.curr.unwrap_or('\x00') {
              '=' => { self.bump(); return token::GE; }
              '>' => { return self.binop(token::SHR); }
              _ => { return token::GT; }
            }
          }
          '\'' => {
            // Either a character constant 'a' OR a lifetime name 'abc
            self.bump();
            let start = self.last_pos;

            // the eof will be picked up by the final `'` check below
            let mut c2 = self.curr.unwrap_or('\x00');
            self.bump();

            // If the character is an ident start not followed by another single
            // quote, then this is a lifetime name:
            if ident_start(Some(c2)) && !self.curr_is('\'') {
                while ident_continue(self.curr) {
                    self.bump();
                }
                let ident = self.with_str_from(start, |lifetime_name| {
                    str_to_ident(lifetime_name)
                });
                let tok = &token::IDENT(ident, false);

                if token::is_keyword(token::keywords::Self, tok) {
                    self.err_span(start, self.last_pos,
                               "invalid lifetime name: 'self \
                                is no longer a special lifetime");
                } else if token::is_any_keyword(tok) &&
                    !token::is_keyword(token::keywords::Static, tok) {
                    self.err_span(start, self.last_pos,
                               "invalid lifetime name");
                }
                return token::LIFETIME(ident);
            }

            // Otherwise it is a character constant:
            match c2 {
                '\\' => {
                    // '\X' for some X must be a character constant:
                    let escaped = self.curr;
                    let escaped_pos = self.last_pos;
                    self.bump();
                    match escaped {
                        None => {}
                        Some(e) => {
                            c2 = match e {
                                'n' => '\n',
                                'r' => '\r',
                                't' => '\t',
                                '\\' => '\\',
                                '\'' => '\'',
                                '"' => '"',
                                '0' => '\x00',
                                'x' => self.scan_numeric_escape(2u, '\''),
                                'u' => self.scan_numeric_escape(4u, '\''),
                                'U' => self.scan_numeric_escape(8u, '\''),
                                c2 => {
                                    self.err_span_char(escaped_pos, self.last_pos,
                                                         "unknown character escape", c2);
                                    c2
                                }
                            }
                        }
                    }
                }
                '\t' | '\n' | '\r' | '\'' => {
                    self.err_span_char( start, self.last_pos,
                        "character constant must be escaped", c2);
                }
                _ => {}
            }
            if !self.curr_is('\'') {
                self.fatal_span_verbose(
                                   // Byte offsetting here is okay because the
                                   // character before position `start` is an
                                   // ascii single quote.
                                   start - BytePos(1), self.last_pos,
                                   "unterminated character constant".to_string());
            }
            self.bump(); // advance curr past token
            return token::LIT_CHAR(c2);
          }
          '"' => {
            let mut accum_str = String::new();
            let start_bpos = self.last_pos;
            self.bump();
            while !self.curr_is('"') {
                if self.is_eof() {
                    self.fatal_span(start_bpos, self.last_pos, "unterminated double quote string");
                }

                let ch = self.curr.unwrap();
                self.bump();
                match ch {
                  '\\' => {
                    if self.is_eof() {
                        self.fatal_span(start_bpos, self.last_pos,
                               "unterminated double quote string");
                    }

                    let escaped = self.curr.unwrap();
                    let escaped_pos = self.last_pos;
                    self.bump();
                    match escaped {
                      'n' => accum_str.push_char('\n'),
                      'r' => accum_str.push_char('\r'),
                      't' => accum_str.push_char('\t'),
                      '\\' => accum_str.push_char('\\'),
                      '\'' => accum_str.push_char('\''),
                      '"' => accum_str.push_char('"'),
                      '\n' => self.consume_whitespace(),
                      '0' => accum_str.push_char('\x00'),
                      'x' => {
                        accum_str.push_char(self.scan_numeric_escape(2u, '"'));
                      }
                      'u' => {
                        accum_str.push_char(self.scan_numeric_escape(4u, '"'));
                      }
                      'U' => {
                        accum_str.push_char(self.scan_numeric_escape(8u, '"'));
                      }
                      c2 => {
                        self.err_span_char(escaped_pos, self.last_pos,
                                        "unknown string escape", c2);
                      }
                    }
                  }
                  _ => accum_str.push_char(ch)
                }
            }
            self.bump();
            return token::LIT_STR(str_to_ident(accum_str.as_slice()));
          }
          'r' => {
            let start_bpos = self.last_pos;
            self.bump();
            let mut hash_count = 0u;
            while self.curr_is('#') {
                self.bump();
                hash_count += 1;
            }

            if self.is_eof() {
                self.fatal_span(start_bpos, self.last_pos, "unterminated raw string");
            } else if !self.curr_is('"') {
                self.fatal_span_char(start_bpos, self.last_pos,
                                "only `#` is allowed in raw string delimitation; \
                                 found illegal character",
                                self.curr.unwrap());
            }
            self.bump();
            let content_start_bpos = self.last_pos;
            let mut content_end_bpos;
            'outer: loop {
                if self.is_eof() {
                    self.fatal_span(start_bpos, self.last_pos, "unterminated raw string");
                }
                if self.curr_is('"') {
                    content_end_bpos = self.last_pos;
                    for _ in range(0, hash_count) {
                        self.bump();
                        if !self.curr_is('#') {
                            continue 'outer;
                        }
                    }
                    break;
                }
                self.bump();
            }
            self.bump();
            let str_content = self.with_str_from_to(
                                               content_start_bpos,
                                               content_end_bpos,
                                               str_to_ident);
            return token::LIT_STR_RAW(str_content, hash_count);
          }
          '-' => {
            if self.nextch_is('>') {
                self.bump();
                self.bump();
                return token::RARROW;
            } else { return self.binop(token::MINUS); }
          }
          '&' => {
            if self.nextch_is('&') {
                self.bump();
                self.bump();
                return token::ANDAND;
            } else { return self.binop(token::AND); }
          }
          '|' => {
            match self.nextch() {
              Some('|') => { self.bump(); self.bump(); return token::OROR; }
              _ => { return self.binop(token::OR); }
            }
          }
          '+' => { return self.binop(token::PLUS); }
          '*' => { return self.binop(token::STAR); }
          '/' => { return self.binop(token::SLASH); }
          '^' => { return self.binop(token::CARET); }
          '%' => { return self.binop(token::PERCENT); }
          c => {
              self.fatal_span_char(self.last_pos, self.pos,
                              "unknown start of token", c);
          }
        }
    }

    fn consume_whitespace(&mut self) {
        while is_whitespace(self.curr) && !self.is_eof() { self.bump(); }
    }

    fn read_to_eol(&mut self) -> String {
        let mut val = String::new();
        while !self.curr_is('\n') && !self.is_eof() {
            val.push_char(self.curr.unwrap());
            self.bump();
        }
        if self.curr_is('\n') { self.bump(); }
        return val
    }

    fn read_one_line_comment(&mut self) -> String {
        let val = self.read_to_eol();
        assert!((val.as_slice()[0] == '/' as u8 && val.as_slice()[1] == '/' as u8)
             || (val.as_slice()[0] == '#' as u8 && val.as_slice()[1] == '!' as u8));
        return val;
    }

    fn consume_non_eol_whitespace(&mut self) {
        while is_whitespace(self.curr) && !self.curr_is('\n') && !self.is_eof() {
            self.bump();
        }
    }

    fn peeking_at_comment(&self) -> bool {
        (self.curr_is('/') && self.nextch_is('/'))
     || (self.curr_is('/') && self.nextch_is('*'))
     // consider shebangs comments, but not inner attributes
     || (self.curr_is('#') && self.nextch_is('!') && !self.nextnextch_is('['))
    }
}

pub fn is_whitespace(c: Option<char>) -> bool {
    match c.unwrap_or('\x00') { // None can be null for now... it's not whitespace
        ' ' | '\n' | '\t' | '\r' => true,
        _ => false
    }
}

fn in_range(c: Option<char>, lo: char, hi: char) -> bool {
    match c {
        Some(c) => lo <= c && c <= hi,
        _ => false
    }
}

fn is_dec_digit(c: Option<char>) -> bool { return in_range(c, '0', '9'); }

pub fn is_line_non_doc_comment(s: &str) -> bool {
    s.starts_with("////")
}

pub fn is_block_non_doc_comment(s: &str) -> bool {
    s.starts_with("/***")
}

fn ident_start(c: Option<char>) -> bool {
    let c = match c { Some(c) => c, None => return false };

    (c >= 'a' && c <= 'z')
        || (c >= 'A' && c <= 'Z')
        || c == '_'
        || (c > '\x7f' && char::is_XID_start(c))
}

fn ident_continue(c: Option<char>) -> bool {
    let c = match c { Some(c) => c, None => return false };

    (c >= 'a' && c <= 'z')
        || (c >= 'A' && c <= 'Z')
        || (c >= '0' && c <= '9')
        || c == '_'
        || (c > '\x7f' && char::is_XID_continue(c))
}

#[cfg(test)]
mod test {
    use super::*;

    use codemap::{BytePos, CodeMap, Span};
    use diagnostic;
    use parse::token;
    use parse::token::{str_to_ident};
    use std::io::util;

    fn mk_sh() -> diagnostic::SpanHandler {
        let emitter = diagnostic::EmitterWriter::new(box util::NullWriter);
        let handler = diagnostic::mk_handler(box emitter);
        diagnostic::mk_span_handler(handler, CodeMap::new())
    }

    // open a string reader for the given string
    fn setup<'a>(span_handler: &'a diagnostic::SpanHandler,
                 teststr: String) -> StringReader<'a> {
        let fm = span_handler.cm.new_filemap("zebra.rs".to_string(), teststr);
        StringReader::new(span_handler, fm)
    }

    #[test] fn t1 () {
        let span_handler = mk_sh();
        let mut string_reader = setup(&span_handler,
            "/* my source file */ \
             fn main() { println!(\"zebra\"); }\n".to_string());
        let id = str_to_ident("fn");
        let tok1 = string_reader.next_token();
        let tok2 = TokenAndSpan{
            tok:token::IDENT(id, false),
            sp:Span {lo:BytePos(21),hi:BytePos(23),expn_info: None}};
        assert_eq!(tok1,tok2);
        // the 'main' id is already read:
        assert_eq!(string_reader.last_pos.clone(), BytePos(28));
        // read another token:
        let tok3 = string_reader.next_token();
        let tok4 = TokenAndSpan{
            tok:token::IDENT(str_to_ident("main"), false),
            sp:Span {lo:BytePos(24),hi:BytePos(28),expn_info: None}};
        assert_eq!(tok3,tok4);
        // the lparen is already read:
        assert_eq!(string_reader.last_pos.clone(), BytePos(29))
    }

    // check that the given reader produces the desired stream
    // of tokens (stop checking after exhausting the expected vec)
    fn check_tokenization (mut string_reader: StringReader, expected: Vec<token::Token> ) {
        for expected_tok in expected.iter() {
            assert_eq!(&string_reader.next_token().tok, expected_tok);
        }
    }

    // make the identifier by looking up the string in the interner
    fn mk_ident (id: &str, is_mod_name: bool) -> token::Token {
        token::IDENT (str_to_ident(id),is_mod_name)
    }

    #[test] fn doublecolonparsing () {
        check_tokenization(setup(&mk_sh(), "a b".to_string()),
                           vec!(mk_ident("a",false),
                             mk_ident("b",false)));
    }

    #[test] fn dcparsing_2 () {
        check_tokenization(setup(&mk_sh(), "a::b".to_string()),
                           vec!(mk_ident("a",true),
                             token::MOD_SEP,
                             mk_ident("b",false)));
    }

    #[test] fn dcparsing_3 () {
        check_tokenization(setup(&mk_sh(), "a ::b".to_string()),
                           vec!(mk_ident("a",false),
                             token::MOD_SEP,
                             mk_ident("b",false)));
    }

    #[test] fn dcparsing_4 () {
        check_tokenization(setup(&mk_sh(), "a:: b".to_string()),
                           vec!(mk_ident("a",true),
                             token::MOD_SEP,
                             mk_ident("b",false)));
    }

    #[test] fn character_a() {
        assert_eq!(setup(&mk_sh(), "'a'".to_string()).next_token().tok,
                   token::LIT_CHAR('a'));
    }

    #[test] fn character_space() {
        assert_eq!(setup(&mk_sh(), "' '".to_string()).next_token().tok,
                   token::LIT_CHAR(' '));
    }

    #[test] fn character_escaped() {
        assert_eq!(setup(&mk_sh(), "'\\n'".to_string()).next_token().tok,
                   token::LIT_CHAR('\n'));
    }

    #[test] fn lifetime_name() {
        assert_eq!(setup(&mk_sh(), "'abc".to_string()).next_token().tok,
                   token::LIFETIME(token::str_to_ident("abc")));
    }

    #[test] fn raw_string() {
        assert_eq!(setup(&mk_sh(),
                         "r###\"\"#a\\b\x00c\"\"###".to_string()).next_token()
                                                                 .tok,
                   token::LIT_STR_RAW(token::str_to_ident("\"#a\\b\x00c\""), 3));
    }

    #[test] fn line_doc_comments() {
        assert!(!is_line_non_doc_comment("///"));
        assert!(!is_line_non_doc_comment("/// blah"));
        assert!(is_line_non_doc_comment("////"));
    }

    #[test] fn nested_block_comments() {
        assert_eq!(setup(&mk_sh(),
                         "/* /* */ */'a'".to_string()).next_token().tok,
                   token::LIT_CHAR('a'));
    }

}
