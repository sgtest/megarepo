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
use parse::token;
use util::interner::Interner;
use util::interner;

use core::cast;
use core::char;
use core::cmp;
use core::str;
use core::task;
use std::map::HashMap;

#[auto_encode]
#[auto_decode]
enum binop {
    PLUS,
    MINUS,
    STAR,
    SLASH,
    PERCENT,
    CARET,
    AND,
    OR,
    SHL,
    SHR,
}

#[auto_encode]
#[auto_decode]
enum Token {
    /* Expression-operator symbols. */
    EQ,
    LT,
    LE,
    EQEQ,
    NE,
    GE,
    GT,
    ANDAND,
    OROR,
    NOT,
    TILDE,
    BINOP(binop),
    BINOPEQ(binop),

    /* Structural symbols */
    AT,
    DOT,
    DOTDOT,
    COMMA,
    SEMI,
    COLON,
    MOD_SEP,
    RARROW,
    LARROW,
    DARROW,
    FAT_ARROW,
    LPAREN,
    RPAREN,
    LBRACKET,
    RBRACKET,
    LBRACE,
    RBRACE,
    POUND,
    DOLLAR,

    /* Literals */
    LIT_INT(i64, ast::int_ty),
    LIT_UINT(u64, ast::uint_ty),
    LIT_INT_UNSUFFIXED(i64),
    LIT_FLOAT(ast::ident, ast::float_ty),
    LIT_FLOAT_UNSUFFIXED(ast::ident),
    LIT_STR(ast::ident),

    /* Name components */
    IDENT(ast::ident, bool),
    UNDERSCORE,

    /* For interpolation */
    INTERPOLATED(nonterminal),

    DOC_COMMENT(ast::ident),
    EOF,
}

#[auto_encode]
#[auto_decode]
/// For interpolation during macro expansion.
enum nonterminal {
    nt_item(@ast::item),
    nt_block(ast::blk),
    nt_stmt(@ast::stmt),
    nt_pat( @ast::pat),
    nt_expr(@ast::expr),
    nt_ty(  @ast::Ty),
    nt_ident(ast::ident, bool),
    nt_path(@ast::path),
    nt_tt(  @ast::token_tree), //needs @ed to break a circularity
    nt_matchers(~[ast::matcher])
}

fn binop_to_str(o: binop) -> ~str {
    match o {
      PLUS => ~"+",
      MINUS => ~"-",
      STAR => ~"*",
      SLASH => ~"/",
      PERCENT => ~"%",
      CARET => ~"^",
      AND => ~"&",
      OR => ~"|",
      SHL => ~"<<",
      SHR => ~">>"
    }
}

fn to_str(in: @ident_interner, t: Token) -> ~str {
    match t {
      EQ => ~"=",
      LT => ~"<",
      LE => ~"<=",
      EQEQ => ~"==",
      NE => ~"!=",
      GE => ~">=",
      GT => ~">",
      NOT => ~"!",
      TILDE => ~"~",
      OROR => ~"||",
      ANDAND => ~"&&",
      BINOP(op) => binop_to_str(op),
      BINOPEQ(op) => binop_to_str(op) + ~"=",

      /* Structural symbols */
      AT => ~"@",
      DOT => ~".",
      DOTDOT => ~"..",
      COMMA => ~",",
      SEMI => ~";",
      COLON => ~":",
      MOD_SEP => ~"::",
      RARROW => ~"->",
      LARROW => ~"<-",
      DARROW => ~"<->",
      FAT_ARROW => ~"=>",
      LPAREN => ~"(",
      RPAREN => ~")",
      LBRACKET => ~"[",
      RBRACKET => ~"]",
      LBRACE => ~"{",
      RBRACE => ~"}",
      POUND => ~"#",
      DOLLAR => ~"$",

      /* Literals */
      LIT_INT(c, ast::ty_char) => {
        ~"'" + char::escape_default(c as char) + ~"'"
      }
      LIT_INT(i, t) => {
          i.to_str() + ast_util::int_ty_to_str(t)
      }
      LIT_UINT(u, t) => {
          u.to_str() + ast_util::uint_ty_to_str(t)
      }
      LIT_INT_UNSUFFIXED(i) => { i.to_str() }
      LIT_FLOAT(s, t) => {
        let mut body = *in.get(s);
        if body.ends_with(~".") {
            body = body + ~"0";  // `10.f` is not a float literal
        }
        body + ast_util::float_ty_to_str(t)
      }
      LIT_FLOAT_UNSUFFIXED(s) => {
        let mut body = *in.get(s);
        if body.ends_with(~".") {
            body = body + ~"0";  // `10.f` is not a float literal
        }
        body
      }
      LIT_STR(s) => { ~"\"" + str::escape_default(*in.get(s)) + ~"\"" }

      /* Name components */
      IDENT(s, _) => *in.get(s),

      UNDERSCORE => ~"_",

      /* Other */
      DOC_COMMENT(s) => *in.get(s),
      EOF => ~"<eof>",
      INTERPOLATED(ref nt) => {
        match nt {
            &nt_expr(e) => ::print::pprust::expr_to_str(e, in),
            _ => {
                ~"an interpolated " +
                    match (*nt) {
                      nt_item(*) => ~"item",
                      nt_block(*) => ~"block",
                      nt_stmt(*) => ~"statement",
                      nt_pat(*) => ~"pattern",
                      nt_expr(*) => fail ~"should have been handled above",
                      nt_ty(*) => ~"type",
                      nt_ident(*) => ~"identifier",
                      nt_path(*) => ~"path",
                      nt_tt(*) => ~"tt",
                      nt_matchers(*) => ~"matcher sequence"
                    }
            }
        }
      }
    }
}

pure fn can_begin_expr(t: Token) -> bool {
    match t {
      LPAREN => true,
      LBRACE => true,
      LBRACKET => true,
      IDENT(_, _) => true,
      UNDERSCORE => true,
      TILDE => true,
      LIT_INT(_, _) => true,
      LIT_UINT(_, _) => true,
      LIT_INT_UNSUFFIXED(_) => true,
      LIT_FLOAT(_, _) => true,
      LIT_FLOAT_UNSUFFIXED(_) => true,
      LIT_STR(_) => true,
      POUND => true,
      AT => true,
      NOT => true,
      BINOP(MINUS) => true,
      BINOP(STAR) => true,
      BINOP(AND) => true,
      BINOP(OR) => true, // in lambda syntax
      OROR => true, // in lambda syntax
      MOD_SEP => true,
      INTERPOLATED(nt_expr(*))
      | INTERPOLATED(nt_ident(*))
      | INTERPOLATED(nt_block(*))
      | INTERPOLATED(nt_path(*)) => true,
      _ => false
    }
}

/// what's the opposite delimiter?
fn flip_delimiter(t: token::Token) -> token::Token {
    match t {
      token::LPAREN => token::RPAREN,
      token::LBRACE => token::RBRACE,
      token::LBRACKET => token::RBRACKET,
      token::RPAREN => token::LPAREN,
      token::RBRACE => token::LBRACE,
      token::RBRACKET => token::LBRACKET,
      _ => fail
    }
}



fn is_lit(t: Token) -> bool {
    match t {
      LIT_INT(_, _) => true,
      LIT_UINT(_, _) => true,
      LIT_INT_UNSUFFIXED(_) => true,
      LIT_FLOAT(_, _) => true,
      LIT_FLOAT_UNSUFFIXED(_) => true,
      LIT_STR(_) => true,
      _ => false
    }
}

pure fn is_ident(t: Token) -> bool {
    match t { IDENT(_, _) => true, _ => false }
}

pure fn is_ident_or_path(t: Token) -> bool {
    match t {
      IDENT(_, _) | INTERPOLATED(nt_path(*)) => true,
      _ => false
    }
}

pure fn is_plain_ident(t: Token) -> bool {
    match t { IDENT(_, false) => true, _ => false }
}

pure fn is_bar(t: Token) -> bool {
    match t { BINOP(OR) | OROR => true, _ => false }
}


mod special_idents {
    #[legacy_exports];
    use ast::ident;
    const underscore : ident = ident { repr: 0u };
    const anon : ident = ident { repr: 1u };
    const dtor : ident = ident { repr: 2u }; // 'drop', but that's reserved
    const invalid : ident = ident { repr: 3u }; // ''
    const unary : ident = ident { repr: 4u };
    const not_fn : ident = ident { repr: 5u };
    const idx_fn : ident = ident { repr: 6u };
    const unary_minus_fn : ident = ident { repr: 7u };
    const clownshoes_extensions : ident = ident { repr: 8u };

    const self_ : ident = ident { repr: 9u }; // 'self'

    /* for matcher NTs */
    const item : ident = ident { repr: 10u };
    const block : ident = ident { repr: 11u };
    const stmt : ident = ident { repr: 12u };
    const pat : ident = ident { repr: 13u };
    const expr : ident = ident { repr: 14u };
    const ty : ident = ident { repr: 15u };
    const ident : ident = ident { repr: 16u };
    const path : ident = ident { repr: 17u };
    const tt : ident = ident { repr: 18u };
    const matchers : ident = ident { repr: 19u };

    const str : ident = ident { repr: 20u }; // for the type

    /* outside of libsyntax */
    const ty_visitor : ident = ident { repr: 21u };
    const arg : ident = ident { repr: 22u };
    const descrim : ident = ident { repr: 23u };
    const clownshoe_abi : ident = ident { repr: 24u };
    const clownshoe_stack_shim : ident = ident { repr: 25u };
    const tydesc : ident = ident { repr: 26u };
    const literally_dtor : ident = ident { repr: 27u };
    const main : ident = ident { repr: 28u };
    const opaque : ident = ident { repr: 29u };
    const blk : ident = ident { repr: 30u };
    const static : ident = ident { repr: 31u };
    const intrinsic : ident = ident { repr: 32u };
    const clownshoes_foreign_mod: ident = ident { repr: 33 };
    const unnamed_field: ident = ident { repr: 34 };
    const c_abi: ident = ident { repr: 35 };
    const type_self: ident = ident { repr: 36 };    // `Self`
}

struct ident_interner {
    priv interner: Interner<@~str>,
}

impl ident_interner {
    fn intern(val: @~str) -> ast::ident {
        ast::ident { repr: self.interner.intern(val) }
    }
    fn gensym(val: @~str) -> ast::ident {
        ast::ident { repr: self.interner.gensym(val) }
    }
    pure fn get(idx: ast::ident) -> @~str {
        self.interner.get(idx.repr)
    }
    fn len() -> uint {
        self.interner.len()
    }
}

/* Key for thread-local data for sneaking interner information to the
 * encoder/decoder. It sounds like a hack because it is one.
 * Bonus ultra-hack: functions as keys don't work across crates,
 * so we have to use a unique number. See taskgroup_key! in task.rs
 * for another case of this. */
macro_rules! interner_key (
    () => (cast::transmute::<(uint, uint), &fn(+v: @@token::ident_interner)>(
        (-3 as uint, 0u)))
)

fn mk_ident_interner() -> @ident_interner {
    unsafe {
        match task::local_data::local_data_get(interner_key!()) {
            Some(interner) => *interner,
            None => {
                // the indices here must correspond to the numbers in
                // special_idents.
                let init_vec = ~[
                    @~"_",                  // 0
                    @~"anon",               // 1
                    @~"drop",               // 2
                    @~"",                   // 3
                    @~"unary",              // 4
                    @~"!",                  // 5
                    @~"[]",                 // 6
                    @~"unary-",             // 7
                    @~"__extensions__",     // 8
                    @~"self",               // 9
                    @~"item",               // 10
                    @~"block",              // 11
                    @~"stmt",               // 12
                    @~"pat",                // 13
                    @~"expr",               // 14
                    @~"ty",                 // 15
                    @~"ident",              // 16
                    @~"path",               // 17
                    @~"tt",                 // 18
                    @~"matchers",           // 19
                    @~"str",                // 20
                    @~"TyVisitor",          // 21
                    @~"arg",                // 22
                    @~"descrim",            // 23
                    @~"__rust_abi",         // 24
                    @~"__rust_stack_shim",  // 25
                    @~"TyDesc",             // 26
                    @~"dtor",               // 27
                    @~"main",               // 28
                    @~"<opaque>",           // 29
                    @~"blk",                // 30
                    @~"static",             // 31
                    @~"intrinsic",          // 32
                    @~"__foreign_mod__",    // 33
                    @~"__field__",          // 34
                    @~"C",                  // 35
                    @~"Self",               // 36
                ];

                let rv = @ident_interner {
                    interner: interner::mk_prefill(init_vec)
                };

                task::local_data::local_data_set(interner_key!(), @rv);

                rv
            }
        }
    }
}

/* for when we don't care about the contents; doesn't interact with TLD or
   serialization */
fn mk_fake_ident_interner() -> @ident_interner {
    @ident_interner { interner: interner::mk::<@~str>() }
}

/**
 * All the valid words that have meaning in the Rust language.
 *
 * Rust keywords are either 'temporary', 'strict' or 'reserved'.  Temporary
 * keywords are contextual and may be used as identifiers anywhere.  They are
 * expected to disappear from the grammar soon.  Strict keywords may not
 * appear as identifiers at all. Reserved keywords are not used anywhere in
 * the language and may not appear as identifiers.
 */
fn keyword_table() -> HashMap<~str, ()> {
    let keywords = HashMap();
    for temporary_keyword_table().each_key |word| {
        keywords.insert(word, ());
    }
    for strict_keyword_table().each_key |word| {
        keywords.insert(word, ());
    }
    for reserved_keyword_table().each_key |word| {
        keywords.insert(word, ());
    }
    keywords
}

/// Keywords that may be used as identifiers
fn temporary_keyword_table() -> HashMap<~str, ()> {
    let words = HashMap();
    let keys = ~[
        ~"self", ~"static",
    ];
    for keys.each |word| {
        words.insert(*word, ());
    }
    words
}

/// Full keywords. May not appear anywhere else.
fn strict_keyword_table() -> HashMap<~str, ()> {
    let words = HashMap();
    let keys = ~[
        ~"as", ~"assert",
        ~"break",
        ~"const", ~"copy",
        ~"do", ~"drop",
        ~"else", ~"enum", ~"export", ~"extern",
        ~"fail", ~"false", ~"fn", ~"for",
        ~"if", ~"impl",
        ~"let", ~"log", ~"loop",
        ~"match", ~"mod", ~"move", ~"mut",
        ~"once",
        ~"priv", ~"pub", ~"pure",
        ~"ref", ~"return",
        ~"struct",
        ~"true", ~"trait", ~"type",
        ~"unsafe", ~"use",
        ~"while"
    ];
    for keys.each |word| {
        words.insert(*word, ());
    }
    words
}

fn reserved_keyword_table() -> HashMap<~str, ()> {
    let words = HashMap();
    let keys = ~[
        ~"be"
    ];
    for keys.each |word| {
        words.insert(*word, ());
    }
    words
}

impl binop : cmp::Eq {
    pure fn eq(&self, other: &binop) -> bool {
        ((*self) as uint) == ((*other) as uint)
    }
    pure fn ne(&self, other: &binop) -> bool { !(*self).eq(other) }
}

impl Token : cmp::Eq {
    pure fn eq(&self, other: &Token) -> bool {
        match (*self) {
            EQ => {
                match (*other) {
                    EQ => true,
                    _ => false
                }
            }
            LT => {
                match (*other) {
                    LT => true,
                    _ => false
                }
            }
            LE => {
                match (*other) {
                    LE => true,
                    _ => false
                }
            }
            EQEQ => {
                match (*other) {
                    EQEQ => true,
                    _ => false
                }
            }
            NE => {
                match (*other) {
                    NE => true,
                    _ => false
                }
            }
            GE => {
                match (*other) {
                    GE => true,
                    _ => false
                }
            }
            GT => {
                match (*other) {
                    GT => true,
                    _ => false
                }
            }
            ANDAND => {
                match (*other) {
                    ANDAND => true,
                    _ => false
                }
            }
            OROR => {
                match (*other) {
                    OROR => true,
                    _ => false
                }
            }
            NOT => {
                match (*other) {
                    NOT => true,
                    _ => false
                }
            }
            TILDE => {
                match (*other) {
                    TILDE => true,
                    _ => false
                }
            }
            BINOP(e0a) => {
                match (*other) {
                    BINOP(e0b) => e0a == e0b,
                    _ => false
                }
            }
            BINOPEQ(e0a) => {
                match (*other) {
                    BINOPEQ(e0b) => e0a == e0b,
                    _ => false
                }
            }
            AT => {
                match (*other) {
                    AT => true,
                    _ => false
                }
            }
            DOT => {
                match (*other) {
                    DOT => true,
                    _ => false
                }
            }
            DOTDOT => {
                match (*other) {
                    DOTDOT => true,
                    _ => false
                }
            }
            COMMA => {
                match (*other) {
                    COMMA => true,
                    _ => false
                }
            }
            SEMI => {
                match (*other) {
                    SEMI => true,
                    _ => false
                }
            }
            COLON => {
                match (*other) {
                    COLON => true,
                    _ => false
                }
            }
            MOD_SEP => {
                match (*other) {
                    MOD_SEP => true,
                    _ => false
                }
            }
            RARROW => {
                match (*other) {
                    RARROW => true,
                    _ => false
                }
            }
            LARROW => {
                match (*other) {
                    LARROW => true,
                    _ => false
                }
            }
            DARROW => {
                match (*other) {
                    DARROW => true,
                    _ => false
                }
            }
            FAT_ARROW => {
                match (*other) {
                    FAT_ARROW => true,
                    _ => false
                }
            }
            LPAREN => {
                match (*other) {
                    LPAREN => true,
                    _ => false
                }
            }
            RPAREN => {
                match (*other) {
                    RPAREN => true,
                    _ => false
                }
            }
            LBRACKET => {
                match (*other) {
                    LBRACKET => true,
                    _ => false
                }
            }
            RBRACKET => {
                match (*other) {
                    RBRACKET => true,
                    _ => false
                }
            }
            LBRACE => {
                match (*other) {
                    LBRACE => true,
                    _ => false
                }
            }
            RBRACE => {
                match (*other) {
                    RBRACE => true,
                    _ => false
                }
            }
            POUND => {
                match (*other) {
                    POUND => true,
                    _ => false
                }
            }
            DOLLAR => {
                match (*other) {
                    DOLLAR => true,
                    _ => false
                }
            }
            LIT_INT(e0a, e1a) => {
                match (*other) {
                    LIT_INT(e0b, e1b) => e0a == e0b && e1a == e1b,
                    _ => false
                }
            }
            LIT_UINT(e0a, e1a) => {
                match (*other) {
                    LIT_UINT(e0b, e1b) => e0a == e0b && e1a == e1b,
                    _ => false
                }
            }
            LIT_INT_UNSUFFIXED(e0a) => {
                match (*other) {
                    LIT_INT_UNSUFFIXED(e0b) => e0a == e0b,
                    _ => false
                }
            }
            LIT_FLOAT(e0a, e1a) => {
                match (*other) {
                    LIT_FLOAT(e0b, e1b) => e0a == e0b && e1a == e1b,
                    _ => false
                }
            }
            LIT_FLOAT_UNSUFFIXED(e0a) => {
                match (*other) {
                    LIT_FLOAT_UNSUFFIXED(e0b) => e0a == e0b,
                    _ => false
                }
            }
            LIT_STR(e0a) => {
                match (*other) {
                    LIT_STR(e0b) => e0a == e0b,
                    _ => false
                }
            }
            IDENT(e0a, e1a) => {
                match (*other) {
                    IDENT(e0b, e1b) => e0a == e0b && e1a == e1b,
                    _ => false
                }
            }
            UNDERSCORE => {
                match (*other) {
                    UNDERSCORE => true,
                    _ => false
                }
            }
            INTERPOLATED(_) => {
                match (*other) {
                    INTERPOLATED(_) => true,
                    _ => false
                }
            }
            DOC_COMMENT(e0a) => {
                match (*other) {
                    DOC_COMMENT(e0b) => e0a == e0b,
                    _ => false
                }
            }
            EOF => {
                match (*other) {
                    EOF => true,
                    _ => false
                }
            }
        }
    }
    pure fn ne(&self, other: &Token) -> bool { !(*self).eq(other) }
}

// Local Variables:
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
