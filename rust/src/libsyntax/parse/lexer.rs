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
use ast_util;
use codemap::{span, CodeMap, CharPos, BytePos};
use codemap;
use diagnostic::span_handler;
use ext::tt::transcribe::{tt_next_token};
use ext::tt::transcribe::{tt_reader, new_tt_reader, dup_tt_reader};
use parse::token;

use core::char;
use core::either;
use core::str;
use core::u64;

use std;

export reader, string_reader, new_string_reader, is_whitespace;
export tt_reader,  new_tt_reader;
export nextch, is_eof, bump, get_str_from, new_low_level_string_reader;
export string_reader_as_reader, tt_reader_as_reader;

trait reader {
    fn is_eof() -> bool;
    fn next_token() -> {tok: token::Token, sp: span};
    fn fatal(~str) -> !;
    fn span_diag() -> span_handler;
    pure fn interner() -> @token::ident_interner;
    fn peek() -> {tok: token::Token, sp: span};
    fn dup() -> reader;
}

type string_reader = @{
    span_diagnostic: span_handler,
    src: @~str,
    // The absolute offset within the codemap of the next character to read
    mut pos: BytePos,
    // The absolute offset within the codemap of the last character read(curr)
    mut last_pos: BytePos,
    // The column of the next character to read
    mut col: CharPos,
    // The last character to be read
    mut curr: char,
    filemap: @codemap::FileMap,
    interner: @token::ident_interner,
    /* cached: */
    mut peek_tok: token::Token,
    mut peek_span: span
};

fn new_string_reader(span_diagnostic: span_handler,
                     filemap: @codemap::FileMap,
                     itr: @token::ident_interner) -> string_reader {
    let r = new_low_level_string_reader(span_diagnostic, filemap, itr);
    string_advance_token(r); /* fill in peek_* */
    return r;
}

/* For comments.rs, which hackily pokes into 'pos' and 'curr' */
fn new_low_level_string_reader(span_diagnostic: span_handler,
                               filemap: @codemap::FileMap,
                               itr: @token::ident_interner)
    -> string_reader {
    // Force the initial reader bump to start on a fresh line
    let initial_char = '\n';
    let r = @{span_diagnostic: span_diagnostic, src: filemap.src,
              mut pos: filemap.start_pos,
              mut last_pos: filemap.start_pos,
              mut col: CharPos(0),
              mut curr: initial_char,
              filemap: filemap, interner: itr,
              /* dummy values; not read */
              mut peek_tok: token::EOF,
              mut peek_span: ast_util::dummy_sp()};
    bump(r);
    return r;
}

fn dup_string_reader(&&r: string_reader) -> string_reader {
    @{span_diagnostic: r.span_diagnostic, src: r.src,
      mut pos: r.pos,
      mut last_pos: r.last_pos,
      mut col: r.col, mut curr: r.curr,
      filemap: r.filemap, interner: r.interner,
      mut peek_tok: r.peek_tok, mut peek_span: r.peek_span}
}

impl string_reader: reader {
    fn is_eof() -> bool { is_eof(self) }
    fn next_token() -> {tok: token::Token, sp: span} {
        let ret_val = {tok: self.peek_tok, sp: self.peek_span};
        string_advance_token(self);
        return ret_val;
    }
    fn fatal(m: ~str) -> ! {
        self.span_diagnostic.span_fatal(copy self.peek_span, m)
    }
    fn span_diag() -> span_handler { self.span_diagnostic }
    pure fn interner() -> @token::ident_interner { self.interner }
    fn peek() -> {tok: token::Token, sp: span} {
        {tok: self.peek_tok, sp: self.peek_span}
    }
    fn dup() -> reader { dup_string_reader(self) as reader }
}

impl tt_reader: reader {
    fn is_eof() -> bool { self.cur_tok == token::EOF }
    fn next_token() -> {tok: token::Token, sp: span} {
        /* weird resolve bug: if the following `if`, or any of its
        statements are removed, we get resolution errors */
        if false {
            let _ignore_me = 0;
            let _me_too = self.cur.readme[self.cur.idx];
        }
        tt_next_token(self)
    }
    fn fatal(m: ~str) -> ! {
        self.sp_diag.span_fatal(copy self.cur_span, m);
    }
    fn span_diag() -> span_handler { self.sp_diag }
    pure fn interner() -> @token::ident_interner { self.interner }
    fn peek() -> {tok: token::Token, sp: span} {
        { tok: self.cur_tok, sp: self.cur_span }
    }
    fn dup() -> reader { dup_tt_reader(self) as reader }
}

fn string_advance_token(&&r: string_reader) {
    for consume_whitespace_and_comments(r).each |comment| {
        r.peek_tok = comment.tok;
        r.peek_span = comment.sp;
        return;
    }

    if is_eof(r) {
        r.peek_tok = token::EOF;
    } else {
        let start_bytepos = r.last_pos;
        r.peek_tok = next_token_inner(r);
        r.peek_span = ast_util::mk_sp(start_bytepos, r.last_pos);
    };

}

fn byte_offset(rdr: string_reader) -> BytePos {
    (rdr.pos - rdr.filemap.start_pos)
}

fn get_str_from(rdr: string_reader, start: BytePos) -> ~str {
    unsafe {
        // I'm pretty skeptical about this subtraction. What if there's a
        // multi-byte character before the mark?
        return str::slice(*rdr.src, start.to_uint() - 1u,
                          byte_offset(rdr).to_uint() - 1u);
    }
}

fn bump(rdr: string_reader) {
    rdr.last_pos = rdr.pos;
    let current_byte_offset = byte_offset(rdr).to_uint();;
    if current_byte_offset < (*rdr.src).len() {
        assert rdr.curr != -1 as char;
        let last_char = rdr.curr;
        let next = str::char_range_at(*rdr.src, current_byte_offset);
        let byte_offset_diff = next.next - current_byte_offset;
        rdr.pos = rdr.pos + BytePos(byte_offset_diff);
        rdr.curr = next.ch;
        rdr.col += CharPos(1u);
        if last_char == '\n' {
            rdr.filemap.next_line(rdr.last_pos);
            rdr.col = CharPos(0u);
        }

        if byte_offset_diff > 1 {
            rdr.filemap.record_multibyte_char(
                BytePos(current_byte_offset), byte_offset_diff);
        }
    } else {
        rdr.curr = -1 as char;
    }
}
fn is_eof(rdr: string_reader) -> bool {
    rdr.curr == -1 as char
}
fn nextch(rdr: string_reader) -> char {
    let offset = byte_offset(rdr).to_uint();
    if offset < (*rdr.src).len() {
        return str::char_at(*rdr.src, offset);
    } else { return -1 as char; }
}

fn dec_digit_val(c: char) -> int { return (c as int) - ('0' as int); }

fn hex_digit_val(c: char) -> int {
    if in_range(c, '0', '9') { return (c as int) - ('0' as int); }
    if in_range(c, 'a', 'f') { return (c as int) - ('a' as int) + 10; }
    if in_range(c, 'A', 'F') { return (c as int) - ('A' as int) + 10; }
    fail;
}

fn bin_digit_value(c: char) -> int { if c == '0' { return 0; } return 1; }

fn is_whitespace(c: char) -> bool {
    return c == ' ' || c == '\t' || c == '\r' || c == '\n';
}

fn may_begin_ident(c: char) -> bool { return is_alpha(c) || c == '_'; }

fn in_range(c: char, lo: char, hi: char) -> bool {
    return lo <= c && c <= hi
}

fn is_alpha(c: char) -> bool {
    return in_range(c, 'a', 'z') || in_range(c, 'A', 'Z');
}

fn is_dec_digit(c: char) -> bool { return in_range(c, '0', '9'); }

fn is_alnum(c: char) -> bool { return is_alpha(c) || is_dec_digit(c); }

fn is_hex_digit(c: char) -> bool {
    return in_range(c, '0', '9') || in_range(c, 'a', 'f') ||
            in_range(c, 'A', 'F');
}

fn is_bin_digit(c: char) -> bool { return c == '0' || c == '1'; }

// might return a sugared-doc-attr
fn consume_whitespace_and_comments(rdr: string_reader)
                                -> Option<{tok: token::Token, sp: span}> {
    while is_whitespace(rdr.curr) { bump(rdr); }
    return consume_any_line_comment(rdr);
}

// might return a sugared-doc-attr
fn consume_any_line_comment(rdr: string_reader)
                                -> Option<{tok: token::Token, sp: span}> {
    if rdr.curr == '/' {
        match nextch(rdr) {
          '/' => {
            bump(rdr);
            bump(rdr);
            // line comments starting with "///" or "//!" are doc-comments
            if rdr.curr == '/' || rdr.curr == '!' {
                let start_bpos = rdr.pos - BytePos(2u);
                let mut acc = ~"//";
                while rdr.curr != '\n' && !is_eof(rdr) {
                    str::push_char(&mut acc, rdr.curr);
                    bump(rdr);
                }
                return Some({
                    tok: token::DOC_COMMENT(rdr.interner.intern(@acc)),
                    sp: ast_util::mk_sp(start_bpos, rdr.pos)
                });
            } else {
                while rdr.curr != '\n' && !is_eof(rdr) { bump(rdr); }
                // Restart whitespace munch.
                return consume_whitespace_and_comments(rdr);
            }
          }
          '*' => { bump(rdr); bump(rdr); return consume_block_comment(rdr); }
          _ => ()
        }
    } else if rdr.curr == '#' {
        if nextch(rdr) == '!' {
            let cmap = @CodeMap::new();
            (*cmap).files.push(rdr.filemap);
            let loc = cmap.lookup_char_pos_adj(rdr.last_pos);
            if loc.line == 1u && loc.col == CharPos(0u) {
                while rdr.curr != '\n' && !is_eof(rdr) { bump(rdr); }
                return consume_whitespace_and_comments(rdr);
            }
        }
    }
    return None;
}

// might return a sugared-doc-attr
fn consume_block_comment(rdr: string_reader)
                                -> Option<{tok: token::Token, sp: span}> {

    // block comments starting with "/**" or "/*!" are doc-comments
    if rdr.curr == '*' || rdr.curr == '!' {
        let start_bpos = rdr.pos - BytePos(2u);
        let mut acc = ~"/*";
        while !(rdr.curr == '*' && nextch(rdr) == '/') && !is_eof(rdr) {
            str::push_char(&mut acc, rdr.curr);
            bump(rdr);
        }
        if is_eof(rdr) {
            rdr.fatal(~"unterminated block doc-comment");
        } else {
            acc += ~"*/";
            bump(rdr);
            bump(rdr);
            return Some({
                tok: token::DOC_COMMENT(rdr.interner.intern(@acc)),
                sp: ast_util::mk_sp(start_bpos, rdr.pos)
            });
        }
    } else {
        loop {
            if is_eof(rdr) { rdr.fatal(~"unterminated block comment"); }
            if rdr.curr == '*' && nextch(rdr) == '/' {
                bump(rdr);
                bump(rdr);
                break;
            } else {
                bump(rdr);
            }
        }
    }
    // restart whitespace munch.

    return consume_whitespace_and_comments(rdr);
}

fn scan_exponent(rdr: string_reader) -> Option<~str> {
    let mut c = rdr.curr;
    let mut rslt = ~"";
    if c == 'e' || c == 'E' {
        str::push_char(&mut rslt, c);
        bump(rdr);
        c = rdr.curr;
        if c == '-' || c == '+' {
            str::push_char(&mut rslt, c);
            bump(rdr);
        }
        let exponent = scan_digits(rdr, 10u);
        if str::len(exponent) > 0u {
            return Some(rslt + exponent);
        } else { rdr.fatal(~"scan_exponent: bad fp literal"); }
    } else { return None::<~str>; }
}

fn scan_digits(rdr: string_reader, radix: uint) -> ~str {
    let mut rslt = ~"";
    loop {
        let c = rdr.curr;
        if c == '_' { bump(rdr); loop; }
        match char::to_digit(c, radix) {
          Some(_) => {
            str::push_char(&mut rslt, c);
            bump(rdr);
          }
          _ => return rslt
        }
    };
}

fn scan_number(c: char, rdr: string_reader) -> token::Token {
    let mut num_str, base = 10u, c = c, n = nextch(rdr);
    if c == '0' && n == 'x' {
        bump(rdr);
        bump(rdr);
        base = 16u;
    } else if c == '0' && n == 'b' {
        bump(rdr);
        bump(rdr);
        base = 2u;
    }
    num_str = scan_digits(rdr, base);
    c = rdr.curr;
    nextch(rdr);
    if c == 'u' || c == 'i' {
        let signed = c == 'i';
        let mut tp = {
            if signed { either::Left(ast::ty_i) }
            else { either::Right(ast::ty_u) }
        };
        bump(rdr);
        c = rdr.curr;
        if c == '8' {
            bump(rdr);
            tp = if signed { either::Left(ast::ty_i8) }
                      else { either::Right(ast::ty_u8) };
        }
        n = nextch(rdr);
        if c == '1' && n == '6' {
            bump(rdr);
            bump(rdr);
            tp = if signed { either::Left(ast::ty_i16) }
                      else { either::Right(ast::ty_u16) };
        } else if c == '3' && n == '2' {
            bump(rdr);
            bump(rdr);
            tp = if signed { either::Left(ast::ty_i32) }
                      else { either::Right(ast::ty_u32) };
        } else if c == '6' && n == '4' {
            bump(rdr);
            bump(rdr);
            tp = if signed { either::Left(ast::ty_i64) }
                      else { either::Right(ast::ty_u64) };
        }
        if str::len(num_str) == 0u {
            rdr.fatal(~"no valid digits found for number");
        }
        let parsed = u64::from_str_radix(num_str, base as u64).get();
        match tp {
          either::Left(t) => return token::LIT_INT(parsed as i64, t),
          either::Right(t) => return token::LIT_UINT(parsed, t)
        }
    }
    let mut is_float = false;
    if rdr.curr == '.' && !(is_alpha(nextch(rdr)) || nextch(rdr) == '_' ||
                            nextch(rdr) == '.') {
        is_float = true;
        bump(rdr);
        let dec_part = scan_digits(rdr, 10u);
        num_str += ~"." + dec_part;
    }
    match scan_exponent(rdr) {
      Some(ref s) => {
        is_float = true;
        num_str += (*s);
      }
      None => ()
    }

    let mut is_machine_float = false;
    if rdr.curr == 'f' {
        bump(rdr);
        c = rdr.curr;
        n = nextch(rdr);
        if c == '3' && n == '2' {
            bump(rdr);
            bump(rdr);
            return token::LIT_FLOAT(rdr.interner.intern(@num_str),
                                 ast::ty_f32);
        } else if c == '6' && n == '4' {
            bump(rdr);
            bump(rdr);
            return token::LIT_FLOAT(rdr.interner.intern(@num_str),
                                 ast::ty_f64);
            /* FIXME (#2252): if this is out of range for either a
            32-bit or 64-bit float, it won't be noticed till the
            back-end.  */
        } else {
            is_float = true;
            is_machine_float = true;
        }
    }
    if is_float {
        if is_machine_float {
            return token::LIT_FLOAT(rdr.interner.intern(@num_str), ast::ty_f);
        }
        return token::LIT_FLOAT_UNSUFFIXED(rdr.interner.intern(@num_str));
    } else {
        if str::len(num_str) == 0u {
            rdr.fatal(~"no valid digits found for number");
        }
        let parsed = u64::from_str_radix(num_str, base as u64).get();

        debug!("lexing %s as an unsuffixed integer literal",
               num_str);
        return token::LIT_INT_UNSUFFIXED(parsed as i64);
    }
}

fn scan_numeric_escape(rdr: string_reader, n_hex_digits: uint) -> char {
    let mut accum_int = 0, i = n_hex_digits;
    while i != 0u {
        let n = rdr.curr;
        bump(rdr);
        if !is_hex_digit(n) {
            rdr.fatal(fmt!("illegal numeric character escape: %d", n as int));
        }
        accum_int *= 16;
        accum_int += hex_digit_val(n);
        i -= 1u;
    }
    return accum_int as char;
}

fn next_token_inner(rdr: string_reader) -> token::Token {
    let mut accum_str = ~"";
    let mut c = rdr.curr;
    if (c >= 'a' && c <= 'z')
        || (c >= 'A' && c <= 'Z')
        || c == '_'
        || (c > 'z' && char::is_XID_start(c)) {
        while (c >= 'a' && c <= 'z')
            || (c >= 'A' && c <= 'Z')
            || (c >= '0' && c <= '9')
            || c == '_'
            || (c > 'z' && char::is_XID_continue(c)) {
            str::push_char(&mut accum_str, c);
            bump(rdr);
            c = rdr.curr;
        }
        if accum_str == ~"_" { return token::UNDERSCORE; }
        let is_mod_name = c == ':' && nextch(rdr) == ':';

        // FIXME: perform NFKC normalization here. (Issue #2253)
        return token::IDENT(rdr.interner.intern(@accum_str), is_mod_name);
    }
    if is_dec_digit(c) {
        return scan_number(c, rdr);
    }
    fn binop(rdr: string_reader, op: token::binop) -> token::Token {
        bump(rdr);
        if rdr.curr == '=' {
            bump(rdr);
            return token::BINOPEQ(op);
        } else { return token::BINOP(op); }
    }
    match c {





      // One-byte tokens.
      ';' => { bump(rdr); return token::SEMI; }
      ',' => { bump(rdr); return token::COMMA; }
      '.' => {
        bump(rdr);
        if rdr.curr == '.' && nextch(rdr) != '.' {
            bump(rdr);
            return token::DOTDOT;
        }
        return token::DOT;
      }
      '(' => { bump(rdr); return token::LPAREN; }
      ')' => { bump(rdr); return token::RPAREN; }
      '{' => { bump(rdr); return token::LBRACE; }
      '}' => { bump(rdr); return token::RBRACE; }
      '[' => { bump(rdr); return token::LBRACKET; }
      ']' => { bump(rdr); return token::RBRACKET; }
      '@' => { bump(rdr); return token::AT; }
      '#' => { bump(rdr); return token::POUND; }
      '~' => { bump(rdr); return token::TILDE; }
      ':' => {
        bump(rdr);
        if rdr.curr == ':' {
            bump(rdr);
            return token::MOD_SEP;
        } else { return token::COLON; }
      }

      '$' => { bump(rdr); return token::DOLLAR; }





      // Multi-byte tokens.
      '=' => {
        bump(rdr);
        if rdr.curr == '=' {
            bump(rdr);
            return token::EQEQ;
        } else if rdr.curr == '>' {
            bump(rdr);
            return token::FAT_ARROW;
        } else {
            return token::EQ;
        }
      }
      '!' => {
        bump(rdr);
        if rdr.curr == '=' {
            bump(rdr);
            return token::NE;
        } else { return token::NOT; }
      }
      '<' => {
        bump(rdr);
        match rdr.curr {
          '=' => { bump(rdr); return token::LE; }
          '<' => { return binop(rdr, token::SHL); }
          '-' => {
            bump(rdr);
            match rdr.curr {
              '>' => { bump(rdr); return token::DARROW; }
              _ => { return token::LARROW; }
            }
          }
          _ => { return token::LT; }
        }
      }
      '>' => {
        bump(rdr);
        match rdr.curr {
          '=' => { bump(rdr); return token::GE; }
          '>' => { return binop(rdr, token::SHR); }
          _ => { return token::GT; }
        }
      }
      '\'' => {
        bump(rdr);
        let mut c2 = rdr.curr;
        bump(rdr);
        if c2 == '\\' {
            let escaped = rdr.curr;
            bump(rdr);
            match escaped {
              'n' => { c2 = '\n'; }
              'r' => { c2 = '\r'; }
              't' => { c2 = '\t'; }
              '\\' => { c2 = '\\'; }
              '\'' => { c2 = '\''; }
              '"' => { c2 = '"'; }
              'x' => { c2 = scan_numeric_escape(rdr, 2u); }
              'u' => { c2 = scan_numeric_escape(rdr, 4u); }
              'U' => { c2 = scan_numeric_escape(rdr, 8u); }
              c2 => {
                rdr.fatal(fmt!("unknown character escape: %d", c2 as int));
              }
            }
        }
        if rdr.curr != '\'' {
            rdr.fatal(~"unterminated character constant");
        }
        bump(rdr); // advance curr past token
        return token::LIT_INT(c2 as i64, ast::ty_char);
      }
      '"' => {
        let n = byte_offset(rdr);
        bump(rdr);
        while rdr.curr != '"' {
            if is_eof(rdr) {
                rdr.fatal(fmt!("unterminated double quote string: %s",
                               get_str_from(rdr, n)));
            }

            let ch = rdr.curr;
            bump(rdr);
            match ch {
              '\\' => {
                let escaped = rdr.curr;
                bump(rdr);
                match escaped {
                  'n' => str::push_char(&mut accum_str, '\n'),
                  'r' => str::push_char(&mut accum_str, '\r'),
                  't' => str::push_char(&mut accum_str, '\t'),
                  '\\' => str::push_char(&mut accum_str, '\\'),
                  '\'' => str::push_char(&mut accum_str, '\''),
                  '"' => str::push_char(&mut accum_str, '"'),
                  '\n' => consume_whitespace(rdr),
                  'x' => {
                    str::push_char(&mut accum_str,
                                   scan_numeric_escape(rdr, 2u));
                  }
                  'u' => {
                    str::push_char(&mut accum_str,
                                   scan_numeric_escape(rdr, 4u));
                  }
                  'U' => {
                    str::push_char(&mut accum_str,
                                   scan_numeric_escape(rdr, 8u));
                  }
                  c2 => {
                    rdr.fatal(fmt!("unknown string escape: %d", c2 as int));
                  }
                }
              }
              _ => str::push_char(&mut accum_str, ch)
            }
        }
        bump(rdr);
        return token::LIT_STR(rdr.interner.intern(@accum_str));
      }
      '-' => {
        if nextch(rdr) == '>' {
            bump(rdr);
            bump(rdr);
            return token::RARROW;
        } else { return binop(rdr, token::MINUS); }
      }
      '&' => {
        if nextch(rdr) == '&' {
            bump(rdr);
            bump(rdr);
            return token::ANDAND;
        } else { return binop(rdr, token::AND); }
      }
      '|' => {
        match nextch(rdr) {
          '|' => { bump(rdr); bump(rdr); return token::OROR; }
          _ => { return binop(rdr, token::OR); }
        }
      }
      '+' => { return binop(rdr, token::PLUS); }
      '*' => { return binop(rdr, token::STAR); }
      '/' => { return binop(rdr, token::SLASH); }
      '^' => { return binop(rdr, token::CARET); }
      '%' => { return binop(rdr, token::PERCENT); }
      c => { rdr.fatal(fmt!("unknown start of token: %d", c as int)); }
    }
}

fn consume_whitespace(rdr: string_reader) {
    while is_whitespace(rdr.curr) && !is_eof(rdr) { bump(rdr); }
}


//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
