// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[macro_escape];

use abi;
use abi::AbiSet;
use ast::{Sigil, BorrowedSigil, ManagedSigil, OwnedSigil};
use ast::{CallSugar, NoSugar, DoSugar};
use ast::{TyBareFn, TyClosure};
use ast::{RegionTyParamBound, TraitTyParamBound};
use ast::{provided, public, purity};
use ast::{_mod, BiAdd, arg, Arm, Attribute, BindByRef, BindInfer};
use ast::{BiBitAnd, BiBitOr, BiBitXor, Block};
use ast::{BlockCheckMode, UnBox};
use ast::{Crate, CrateConfig, Decl, DeclItem};
use ast::{DeclLocal, DefaultBlock, UnDeref, BiDiv, EMPTY_CTXT, enum_def, explicit_self};
use ast::{Expr, Expr_, ExprAddrOf, ExprMatch, ExprAgain};
use ast::{ExprAssign, ExprAssignOp, ExprBinary, ExprBlock};
use ast::{ExprBreak, ExprCall, ExprCast, ExprDoBody};
use ast::{ExprField, ExprFnBlock, ExprIf, ExprIndex};
use ast::{ExprLit, ExprLogLevel, ExprLoop, ExprMac};
use ast::{ExprMethodCall, ExprParen, ExprPath, ExprRepeat};
use ast::{ExprRet, ExprSelf, ExprStruct, ExprTup, ExprUnary};
use ast::{ExprVec, ExprVstore, ExprVstoreMutBox};
use ast::{ExprVstoreSlice, ExprVstoreBox};
use ast::{ExprVstoreMutSlice, ExprWhile, ExprForLoop, extern_fn, Field, fn_decl};
use ast::{ExprVstoreUniq, Onceness, Once, Many};
use ast::{foreign_item, foreign_item_static, foreign_item_fn, foreign_mod};
use ast::{Ident, impure_fn, inherited, item, item_, item_static};
use ast::{item_enum, item_fn, item_foreign_mod, item_impl};
use ast::{item_mac, item_mod, item_struct, item_trait, item_ty, lit, lit_};
use ast::{lit_bool, lit_float, lit_float_unsuffixed, lit_int, lit_char};
use ast::{lit_int_unsuffixed, lit_nil, lit_str, lit_uint, Local};
use ast::{MutImmutable, MutMutable, mac_, mac_invoc_tt, matcher, match_nonterminal};
use ast::{match_seq, match_tok, method, mt, BiMul, Mutability};
use ast::{named_field, UnNeg, noreturn, UnNot, Pat, PatBox, PatEnum};
use ast::{PatIdent, PatLit, PatRange, PatRegion, PatStruct};
use ast::{PatTup, PatUniq, PatWild, private};
use ast::{BiRem, required};
use ast::{ret_style, return_val, BiShl, BiShr, Stmt, StmtDecl};
use ast::{StmtExpr, StmtSemi, StmtMac, struct_def, struct_field};
use ast::{struct_variant_kind, BiSub};
use ast::{sty_box, sty_region, sty_static, sty_uniq, sty_value};
use ast::{token_tree, trait_method, trait_ref, tt_delim, tt_seq, tt_tok};
use ast::{tt_nonterminal, tuple_variant_kind, Ty, ty_, ty_bot, ty_box};
use ast::{TypeField, ty_fixed_length_vec, ty_closure, ty_bare_fn, ty_typeof};
use ast::{ty_infer, TypeMethod};
use ast::{ty_nil, TyParam, TyParamBound, ty_path, ty_ptr, ty_rptr};
use ast::{ty_tup, ty_u32, ty_uniq, ty_vec, UnUniq};
use ast::{unnamed_field, UnsafeBlock, unsafe_fn, view_item};
use ast::{view_item_, view_item_extern_mod, view_item_use};
use ast::{view_path, view_path_glob, view_path_list, view_path_simple};
use ast::visibility;
use ast;
use ast_util::{as_prec, operator_prec};
use ast_util;
use codemap::{Span, BytePos, Spanned, spanned, mk_sp};
use codemap;
use parse::attr::parser_attr;
use parse::classify;
use parse::common::{SeqSep, seq_sep_none};
use parse::common::{seq_sep_trailing_disallowed, seq_sep_trailing_allowed};
use parse::lexer::reader;
use parse::lexer::TokenAndSpan;
use parse::obsolete::*;
use parse::token::{can_begin_expr, get_ident_interner, ident_to_str, is_ident};
use parse::token::{is_ident_or_path};
use parse::token::{is_plain_ident, INTERPOLATED, keywords, special_idents};
use parse::token::{token_to_binop};
use parse::token;
use parse::{new_sub_parser_from_file, ParseSess};
use opt_vec;
use opt_vec::OptVec;

use std::hashmap::HashSet;
use std::util;
use std::vec;

#[deriving(Eq)]
enum restriction {
    UNRESTRICTED,
    RESTRICT_STMT_EXPR,
    RESTRICT_NO_BAR_OP,
    RESTRICT_NO_BAR_OR_DOUBLEBAR_OP,
}

type item_info = (Ident, item_, Option<~[Attribute]>);

/// How to parse a path. There are four different kinds of paths, all of which
/// are parsed somewhat differently.
#[deriving(Eq)]
pub enum PathParsingMode {
    /// A path with no type parameters; e.g. `foo::bar::Baz`
    NoTypesAllowed,
    /// A path with a lifetime and type parameters, with no double colons
    /// before the type parameters; e.g. `foo::bar<'self>::Baz<T>`
    LifetimeAndTypesWithoutColons,
    /// A path with a lifetime and type parameters with double colons before
    /// the type parameters; e.g. `foo::bar::<'self>::Baz::<T>`
    LifetimeAndTypesWithColons,
    /// A path with a lifetime and type parameters with bounds before the last
    /// set of type parameters only; e.g. `foo::bar<'self>::Baz:X+Y<T>` This
    /// form does not use extra double colons.
    LifetimeAndTypesAndBounds,
}

/// A pair of a path segment and group of type parameter bounds. (See `ast.rs`
/// for the definition of a path segment.)
struct PathSegmentAndBoundSet {
    segment: ast::PathSegment,
    bound_set: Option<OptVec<TyParamBound>>,
}

/// A path paired with optional type bounds.
struct PathAndBounds {
    path: ast::Path,
    bounds: Option<OptVec<TyParamBound>>,
}

pub enum item_or_view_item {
    // Indicates a failure to parse any kind of item. The attributes are
    // returned.
    iovi_none(~[Attribute]),
    iovi_item(@item),
    iovi_foreign_item(@foreign_item),
    iovi_view_item(view_item)
}

#[deriving(Eq)]
enum view_item_parse_mode {
    VIEW_ITEMS_AND_ITEMS_ALLOWED,
    FOREIGN_ITEMS_ALLOWED,
    IMPORTS_AND_ITEMS_ALLOWED
}

/* The expr situation is not as complex as I thought it would be.
The important thing is to make sure that lookahead doesn't balk
at INTERPOLATED tokens */
macro_rules! maybe_whole_expr (
    ($p:expr) => (
        {
            // This horrible convolution is brought to you by
            // @mut, have a terrible day
            let ret = match *($p).token {
                INTERPOLATED(token::nt_expr(e)) => {
                    Some(e)
                }
                INTERPOLATED(token::nt_path(ref pt)) => {
                    Some($p.mk_expr(
                        ($p).span.lo,
                        ($p).span.hi,
                        ExprPath(/* bad */ (**pt).clone())))
                }
                _ => None
            };
            match ret {
                Some(e) => {
                    $p.bump();
                    return e;
                }
                None => ()
            }
        }
    )
)

macro_rules! maybe_whole (
    ($p:expr, $constructor:ident) => (
        {
            let __found__ = match *($p).token {
                INTERPOLATED(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            match __found__ {
                Some(INTERPOLATED(token::$constructor(x))) => {
                    return x.clone()
                }
                _ => {}
            }
        }
    );
    (deref $p:expr, $constructor:ident) => (
        {
            let __found__ = match *($p).token {
                INTERPOLATED(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            match __found__ {
                Some(INTERPOLATED(token::$constructor(x))) => {
                    return (*x).clone()
                }
                _ => {}
            }
        }
    );
    (Some $p:expr, $constructor:ident) => (
        {
            let __found__ = match *($p).token {
                INTERPOLATED(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            match __found__ {
                Some(INTERPOLATED(token::$constructor(x))) => {
                    return Some(x.clone()),
                }
                _ => {}
            }
        }
    );
    (iovi $p:expr, $constructor:ident) => (
        {
            let __found__ = match *($p).token {
                INTERPOLATED(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            match __found__ {
                Some(INTERPOLATED(token::$constructor(x))) => {
                    return iovi_item(x.clone())
                }
                _ => {}
            }
        }
    );
    (pair_empty $p:expr, $constructor:ident) => (
        {
            let __found__ = match *($p).token {
                INTERPOLATED(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            match __found__ {
                Some(INTERPOLATED(token::$constructor(ref x))) => {
                    return (~[], (**x).clone())
                }
                _ => {}
            }
        }
    )
)


fn maybe_append(lhs: ~[Attribute], rhs: Option<~[Attribute]>)
             -> ~[Attribute] {
    match rhs {
        None => lhs,
        Some(ref attrs) => vec::append(lhs, (*attrs))
    }
}


struct ParsedItemsAndViewItems {
    attrs_remaining: ~[Attribute],
    view_items: ~[view_item],
    items: ~[@item],
    foreign_items: ~[@foreign_item]
}

/* ident is handled by common.rs */

pub fn Parser(sess: @mut ParseSess,
              cfg: ast::CrateConfig,
              rdr: @mut reader)
           -> Parser {
    let tok0 = rdr.next_token();
    let interner = get_ident_interner();
    let span = tok0.sp;
    let placeholder = TokenAndSpan {
        tok: token::UNDERSCORE,
        sp: span,
    };

    Parser {
        reader: rdr,
        interner: interner,
        sess: sess,
        cfg: cfg,
        token: @mut tok0.tok,
        span: @mut span,
        last_span: @mut span,
        last_token: @mut None,
        buffer: @mut ([
            placeholder.clone(),
            placeholder.clone(),
            placeholder.clone(),
            placeholder.clone(),
        ]),
        buffer_start: @mut 0,
        buffer_end: @mut 0,
        tokens_consumed: @mut 0,
        restriction: @mut UNRESTRICTED,
        quote_depth: @mut 0,
        obsolete_set: @mut HashSet::new(),
        mod_path_stack: @mut ~[],
    }
}

// ooh, nasty mutable fields everywhere....
pub struct Parser {
    sess: @mut ParseSess,
    cfg: CrateConfig,
    // the current token:
    token: @mut token::Token,
    // the span of the current token:
    span: @mut Span,
    // the span of the prior token:
    last_span: @mut Span,
    // the previous token or None (only stashed sometimes).
    last_token: @mut Option<~token::Token>,
    buffer: @mut [TokenAndSpan, ..4],
    buffer_start: @mut int,
    buffer_end: @mut int,
    tokens_consumed: @mut uint,
    restriction: @mut restriction,
    quote_depth: @mut uint, // not (yet) related to the quasiquoter
    reader: @mut reader,
    interner: @token::ident_interner,
    /// The set of seen errors about obsolete syntax. Used to suppress
    /// extra detail when the same error is seen twice
    obsolete_set: @mut HashSet<ObsoleteSyntax>,
    /// Used to determine the path to externally loaded source files
    mod_path_stack: @mut ~[@str],
}

#[unsafe_destructor]
impl Drop for Parser {
    /* do not copy the parser; its state is tied to outside state */
    fn drop(&mut self) {}
}

fn is_plain_ident_or_underscore(t: &token::Token) -> bool {
    is_plain_ident(t) || *t == token::UNDERSCORE
}

impl Parser {
    // convert a token to a string using self's reader
    pub fn token_to_str(&self, token: &token::Token) -> ~str {
        token::to_str(get_ident_interner(), token)
    }

    // convert the current token to a string using self's reader
    pub fn this_token_to_str(&self) -> ~str {
        self.token_to_str(self.token)
    }

    pub fn unexpected_last(&self, t: &token::Token) -> ! {
        self.span_fatal(
            *self.last_span,
            fmt!(
                "unexpected token: `%s`",
                self.token_to_str(t)
            )
        );
    }

    pub fn unexpected(&self) -> ! {
        self.fatal(
            fmt!(
                "unexpected token: `%s`",
                self.this_token_to_str()
            )
        );
    }

    // expect and consume the token t. Signal an error if
    // the next token is not t.
    pub fn expect(&self, t: &token::Token) {
        if *self.token == *t {
            self.bump();
        } else {
            self.fatal(
                fmt!(
                    "expected `%s` but found `%s`",
                    self.token_to_str(t),
                    self.this_token_to_str()
                )
            )
        }
    }

    // Expect next token to be edible or inedible token.  If edible,
    // then consume it; if inedible, then return without consuming
    // anything.  Signal a fatal error if next token is unexpected.
    pub fn expect_one_of(&self, edible: &[token::Token], inedible: &[token::Token]) {
        fn tokens_to_str(p:&Parser, tokens: &[token::Token]) -> ~str {
            let mut i = tokens.iter();
            // This might be a sign we need a connect method on Iterator.
            let b = i.next().map_default(~"", |t| p.token_to_str(*t));
            i.fold(b, |b,a| b + " " + p.token_to_str(a))
        }
        if edible.contains(self.token) {
            self.bump();
        } else if inedible.contains(self.token) {
            // leave it in the input
        } else {
            let expected = vec::append(edible.to_owned(), inedible);
            let expect = tokens_to_str(self, expected);
            let actual = self.this_token_to_str();
            self.fatal(
                if expected.len() != 1 {
                    fmt!("expected one of `%s` but found `%s`", expect, actual)
                } else {
                    fmt!("expected `%s` but found `%s`", expect, actual)
                }
            )
        }
    }

    // Check for erroneous `ident { }`; if matches, signal error and
    // recover (without consuming any expected input token).  Returns
    // true if and only if input was consumed for recovery.
    pub fn check_for_erroneous_unit_struct_expecting(&self, expected: &[token::Token]) -> bool {
        if *self.token == token::LBRACE
            && expected.iter().all(|t| *t != token::LBRACE)
            && self.look_ahead(1, |t| *t == token::RBRACE) {
            // matched; signal non-fatal error and recover.
            self.span_err(*self.span,
                          "Unit-like struct construction is written with no trailing `{ }`");
            self.eat(&token::LBRACE);
            self.eat(&token::RBRACE);
            true
        } else {
            false
        }
    }

    // Commit to parsing a complete expression `e` expected to be
    // followed by some token from the set edible + inedible.  Recover
    // from anticipated input errors, discarding erroneous characters.
    pub fn commit_expr(&self, e: @Expr, edible: &[token::Token], inedible: &[token::Token]) {
        debug!("commit_expr %?", e);
        match e.node {
            ExprPath(*) => {
                // might be unit-struct construction; check for recoverableinput error.
                let expected = vec::append(edible.to_owned(), inedible);
                self.check_for_erroneous_unit_struct_expecting(expected);
            }
            _ => {}
        }
        self.expect_one_of(edible, inedible)
    }

    pub fn commit_expr_expecting(&self, e: @Expr, edible: token::Token) {
        self.commit_expr(e, &[edible], &[])
    }

    // Commit to parsing a complete statement `s`, which expects to be
    // followed by some token from the set edible + inedible.  Check
    // for recoverable input errors, discarding erroneous characters.
    pub fn commit_stmt(&self, s: @Stmt, edible: &[token::Token], inedible: &[token::Token]) {
        debug!("commit_stmt %?", s);
        let _s = s; // unused, but future checks might want to inspect `s`.
        if self.last_token.map_default(false, |t|is_ident_or_path(*t)) {
            let expected = vec::append(edible.to_owned(), inedible);
            self.check_for_erroneous_unit_struct_expecting(expected);
        }
        self.expect_one_of(edible, inedible)
    }

    pub fn commit_stmt_expecting(&self, s: @Stmt, edible: token::Token) {
        self.commit_stmt(s, &[edible], &[])
    }

    pub fn parse_ident(&self) -> ast::Ident {
        self.check_strict_keywords();
        self.check_reserved_keywords();
        match *self.token {
            token::IDENT(i, _) => {
                self.bump();
                i
            }
            token::INTERPOLATED(token::nt_ident(*)) => {
                self.bug("ident interpolation not converted to real token");
            }
            _ => {
                self.fatal(
                    fmt!(
                        "expected ident, found `%s`",
                        self.this_token_to_str()
                    )
                );
            }
        }
    }

    pub fn parse_path_list_ident(&self) -> ast::path_list_ident {
        let lo = self.span.lo;
        let ident = self.parse_ident();
        let hi = self.last_span.hi;
        spanned(lo, hi, ast::path_list_ident_ { name: ident,
                                                id: ast::DUMMY_NODE_ID })
    }

    // consume token 'tok' if it exists. Returns true if the given
    // token was present, false otherwise.
    pub fn eat(&self, tok: &token::Token) -> bool {
        let is_present = *self.token == *tok;
        if is_present { self.bump() }
        is_present
    }

    pub fn is_keyword(&self, kw: keywords::Keyword) -> bool {
        token::is_keyword(kw, self.token)
    }

    // if the next token is the given keyword, eat it and return
    // true. Otherwise, return false.
    pub fn eat_keyword(&self, kw: keywords::Keyword) -> bool {
        let is_kw = match *self.token {
            token::IDENT(sid, false) => kw.to_ident().name == sid.name,
            _ => false
        };
        if is_kw { self.bump() }
        is_kw
    }

    // if the given word is not a keyword, signal an error.
    // if the next token is not the given word, signal an error.
    // otherwise, eat it.
    pub fn expect_keyword(&self, kw: keywords::Keyword) {
        if !self.eat_keyword(kw) {
            self.fatal(
                fmt!(
                    "expected `%s`, found `%s`",
                    self.id_to_str(kw.to_ident()).to_str(),
                    self.this_token_to_str()
                )
            );
        }
    }

    // signal an error if the given string is a strict keyword
    pub fn check_strict_keywords(&self) {
        if token::is_strict_keyword(self.token) {
            self.span_err(*self.last_span,
                          fmt!("found `%s` in ident position", self.this_token_to_str()));
        }
    }

    // signal an error if the current token is a reserved keyword
    pub fn check_reserved_keywords(&self) {
        if token::is_reserved_keyword(self.token) {
            self.fatal(fmt!("`%s` is a reserved keyword", self.this_token_to_str()));
        }
    }

    // expect and consume a GT. if a >> is seen, replace it
    // with a single > and continue. If a GT is not seen,
    // signal an error.
    pub fn expect_gt(&self) {
        match *self.token {
            token::GT => self.bump(),
            token::BINOP(token::SHR) => self.replace_token(
                token::GT,
                self.span.lo + BytePos(1u),
                self.span.hi
            ),
            _ => self.fatal(fmt!("expected `%s`, found `%s`",
                                 self.token_to_str(&token::GT),
                                 self.this_token_to_str()))
        }
    }

    // parse a sequence bracketed by '<' and '>', stopping
    // before the '>'.
    pub fn parse_seq_to_before_gt<T>(&self,
                                     sep: Option<token::Token>,
                                     f: &fn(&Parser) -> T)
                                     -> OptVec<T> {
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

    pub fn parse_seq_to_gt<T>(&self,
                              sep: Option<token::Token>,
                              f: &fn(&Parser) -> T)
                              -> OptVec<T> {
        let v = self.parse_seq_to_before_gt(sep, f);
        self.expect_gt();
        return v;
    }

    // parse a sequence, including the closing delimiter. The function
    // f must consume tokens until reaching the next separator or
    // closing bracket.
    pub fn parse_seq_to_end<T>(&self,
                               ket: &token::Token,
                               sep: SeqSep,
                               f: &fn(&Parser) -> T)
                               -> ~[T] {
        let val = self.parse_seq_to_before_end(ket, sep, f);
        self.bump();
        val
    }

    // parse a sequence, not including the closing delimiter. The function
    // f must consume tokens until reaching the next separator or
    // closing bracket.
    pub fn parse_seq_to_before_end<T>(&self,
                                      ket: &token::Token,
                                      sep: SeqSep,
                                      f: &fn(&Parser) -> T)
                                      -> ~[T] {
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
    pub fn parse_unspanned_seq<T>(&self,
                                  bra: &token::Token,
                                  ket: &token::Token,
                                  sep: SeqSep,
                                  f: &fn(&Parser) -> T)
                                  -> ~[T] {
        self.expect(bra);
        let result = self.parse_seq_to_before_end(ket, sep, f);
        self.bump();
        result
    }

    // NB: Do not use this function unless you actually plan to place the
    // spanned list in the AST.
    pub fn parse_seq<T>(&self,
                        bra: &token::Token,
                        ket: &token::Token,
                        sep: SeqSep,
                        f: &fn(&Parser) -> T)
                        -> Spanned<~[T]> {
        let lo = self.span.lo;
        self.expect(bra);
        let result = self.parse_seq_to_before_end(ket, sep, f);
        let hi = self.span.hi;
        self.bump();
        spanned(lo, hi, result)
    }

    // advance the parser by one token
    pub fn bump(&self) {
        *self.last_span = *self.span;
        // Stash token for error recovery (sometimes; clone is not necessarily cheap).
        *self.last_token = if is_ident_or_path(self.token) {
            Some(~(*self.token).clone())
        } else {
            None
        };
        let next = if *self.buffer_start == *self.buffer_end {
            self.reader.next_token()
        } else {
            // Avoid token copies with `util::replace`.
            let buffer_start = *self.buffer_start as uint;
            let next_index = (buffer_start + 1) & 3 as uint;
            *self.buffer_start = next_index as int;

            let placeholder = TokenAndSpan {
                tok: token::UNDERSCORE,
                sp: *self.span,
            };
            util::replace(&mut self.buffer[buffer_start], placeholder)
        };
        *self.span = next.sp;
        *self.token = next.tok;
        *self.tokens_consumed += 1u;
    }

    // Advance the parser by one token and return the bumped token.
    pub fn bump_and_get(&self) -> token::Token {
        let old_token = util::replace(self.token, token::UNDERSCORE);
        self.bump();
        old_token
    }

    // EFFECT: replace the current token and span with the given one
    pub fn replace_token(&self,
                         next: token::Token,
                         lo: BytePos,
                         hi: BytePos) {
        *self.token = next;
        *self.span = mk_sp(lo, hi);
    }
    pub fn buffer_length(&self) -> int {
        if *self.buffer_start <= *self.buffer_end {
            return *self.buffer_end - *self.buffer_start;
        }
        return (4 - *self.buffer_start) + *self.buffer_end;
    }
    pub fn look_ahead<R>(&self, distance: uint, f: &fn(&token::Token) -> R)
                         -> R {
        let dist = distance as int;
        while self.buffer_length() < dist {
            self.buffer[*self.buffer_end] = self.reader.next_token();
            *self.buffer_end = (*self.buffer_end + 1) & 3;
        }
        f(&self.buffer[(*self.buffer_start + dist - 1) & 3].tok)
    }
    pub fn fatal(&self, m: &str) -> ! {
        self.sess.span_diagnostic.span_fatal(*self.span, m)
    }
    pub fn span_fatal(&self, sp: Span, m: &str) -> ! {
        self.sess.span_diagnostic.span_fatal(sp, m)
    }
    pub fn span_note(&self, sp: Span, m: &str) {
        self.sess.span_diagnostic.span_note(sp, m)
    }
    pub fn bug(&self, m: &str) -> ! {
        self.sess.span_diagnostic.span_bug(*self.span, m)
    }
    pub fn warn(&self, m: &str) {
        self.sess.span_diagnostic.span_warn(*self.span, m)
    }
    pub fn span_err(&self, sp: Span, m: &str) {
        self.sess.span_diagnostic.span_err(sp, m)
    }
    pub fn abort_if_errors(&self) {
        self.sess.span_diagnostic.handler().abort_if_errors();
    }

    pub fn id_to_str(&self, id: Ident) -> @str {
        get_ident_interner().get(id.name)
    }

    // is this one of the keywords that signals a closure type?
    pub fn token_is_closure_keyword(&self, tok: &token::Token) -> bool {
        token::is_keyword(keywords::Pure, tok) ||
            token::is_keyword(keywords::Unsafe, tok) ||
            token::is_keyword(keywords::Once, tok) ||
            token::is_keyword(keywords::Fn, tok)
    }

    pub fn token_is_lifetime(&self, tok: &token::Token) -> bool {
        match *tok {
            token::LIFETIME(*) => true,
            _ => false,
        }
    }

    pub fn get_lifetime(&self, tok: &token::Token) -> ast::Ident {
        match *tok {
            token::LIFETIME(ref ident) => *ident,
            _ => self.bug("not a lifetime"),
        }
    }

    // parse a ty_bare_fun type:
    pub fn parse_ty_bare_fn(&self) -> ty_ {
        /*

        extern "ABI" [pure|unsafe] fn <'lt> (S) -> T
               ^~~~^ ^~~~~~~~~~~~^    ^~~~^ ^~^    ^
                 |     |                |    |     |
                 |     |                |    |   Return type
                 |     |                |  Argument types
                 |     |            Lifetimes
                 |     |
                 |   Purity
                ABI

        */

        let opt_abis = self.parse_opt_abis();
        let abis = opt_abis.unwrap_or(AbiSet::Rust());
        let purity = self.parse_unsafety();
        self.expect_keyword(keywords::Fn);
        let (decl, lifetimes) = self.parse_ty_fn_decl();
        return ty_bare_fn(@TyBareFn {
            abis: abis,
            purity: purity,
            lifetimes: lifetimes,
            decl: decl
        });
    }

    // parse a ty_closure type
    pub fn parse_ty_closure(&self,
                            sigil: ast::Sigil,
                            region: Option<ast::Lifetime>)
                            -> ty_ {
        /*

        (&|~|@) ['r] [pure|unsafe] [once] fn [:Bounds] <'lt> (S) -> T
        ^~~~~~^ ^~~^ ^~~~~~~~~~~~^ ^~~~~^    ^~~~~~~~^ ^~~~^ ^~^    ^
           |     |     |             |           |       |    |     |
           |     |     |             |           |       |    |   Return type
           |     |     |             |           |       |  Argument types
           |     |     |             |           |   Lifetimes
           |     |     |             |       Closure bounds
           |     |     |          Once-ness (a.k.a., affine)
           |     |   Purity
           | Lifetime bound
        Allocation type

        */

        // At this point, the allocation type and lifetime bound have been
        // parsed.

        let purity = self.parse_unsafety();
        let onceness = parse_onceness(self);
        self.expect_keyword(keywords::Fn);
        let bounds = self.parse_optional_ty_param_bounds();

        if self.parse_fn_ty_sigil().is_some() {
            self.obsolete(*self.span, ObsoletePostFnTySigil);
        }

        let (decl, lifetimes) = self.parse_ty_fn_decl();

        return ty_closure(@TyClosure {
            sigil: sigil,
            region: region,
            purity: purity,
            onceness: onceness,
            bounds: bounds,
            decl: decl,
            lifetimes: lifetimes,
        });

        fn parse_onceness(this: &Parser) -> Onceness {
            if this.eat_keyword(keywords::Once) {
                Once
            } else {
                Many
            }
        }
    }

    // looks like this should be called parse_unsafety
    pub fn parse_unsafety(&self) -> purity {
        if self.eat_keyword(keywords::Pure) {
            self.obsolete(*self.last_span, ObsoletePurity);
            return impure_fn;
        } else if self.eat_keyword(keywords::Unsafe) {
            return unsafe_fn;
        } else {
            return impure_fn;
        }
    }

    // parse a function type (following the 'fn')
    pub fn parse_ty_fn_decl(&self) -> (fn_decl, OptVec<ast::Lifetime>) {
        /*

        (fn) <'lt> (S) -> T
             ^~~~^ ^~^    ^
               |    |     |
               |    |   Return type
               |  Argument types
           Lifetimes

        */
        let lifetimes = if self.eat(&token::LT) {
            let lifetimes = self.parse_lifetimes();
            self.expect_gt();
            lifetimes
        } else {
            opt_vec::Empty
        };

        let inputs = self.parse_unspanned_seq(
            &token::LPAREN,
            &token::RPAREN,
            seq_sep_trailing_disallowed(token::COMMA),
            |p| p.parse_arg_general(false)
        );
        let (ret_style, ret_ty) = self.parse_ret_ty();
        let decl = ast::fn_decl {
            inputs: inputs,
            output: ret_ty,
            cf: ret_style
        };
        (decl, lifetimes)
    }

    // parse the methods in a trait declaration
    pub fn parse_trait_methods(&self) -> ~[trait_method] {
        do self.parse_unspanned_seq(
            &token::LBRACE,
            &token::RBRACE,
            seq_sep_none()
        ) |p| {
            let attrs = p.parse_outer_attributes();
            let lo = p.span.lo;

            let vis = p.parse_non_priv_visibility();
            let pur = p.parse_fn_purity();
            // NB: at the moment, trait methods are public by default; this
            // could change.
            let ident = p.parse_ident();

            let generics = p.parse_generics();

            let (explicit_self, d) = do self.parse_fn_decl_with_self() |p| {
                // This is somewhat dubious; We don't want to allow argument
                // names to be left off if there is a definition...
                p.parse_arg_general(false)
            };

            let hi = p.last_span.hi;
            debug!("parse_trait_methods(): trait method signature ends in \
                    `%s`",
                   self.this_token_to_str());
            match *p.token {
              token::SEMI => {
                p.bump();
                debug!("parse_trait_methods(): parsing required method");
                // NB: at the moment, visibility annotations on required
                // methods are ignored; this could change.
                if vis != ast::inherited {
                    self.obsolete(*self.last_span,
                                  ObsoleteTraitFuncVisibility);
                }
                required(TypeMethod {
                    ident: ident,
                    attrs: attrs,
                    purity: pur,
                    decl: d,
                    generics: generics,
                    explicit_self: explicit_self,
                    id: ast::DUMMY_NODE_ID,
                    span: mk_sp(lo, hi)
                })
              }
              token::LBRACE => {
                debug!("parse_trait_methods(): parsing provided method");
                let (inner_attrs, body) =
                    p.parse_inner_attrs_and_block();
                let attrs = vec::append(attrs, inner_attrs);
                provided(@ast::method {
                    ident: ident,
                    attrs: attrs,
                    generics: generics,
                    explicit_self: explicit_self,
                    purity: pur,
                    decl: d,
                    body: body,
                    id: ast::DUMMY_NODE_ID,
                    span: mk_sp(lo, hi),
                    self_id: ast::DUMMY_NODE_ID,
                    vis: vis,
                })
              }

              _ => {
                    p.fatal(
                        fmt!(
                            "expected `;` or `{` but found `%s`",
                            self.this_token_to_str()
                        )
                    );
                }
            }
        }
    }

    // parse a possibly mutable type
    pub fn parse_mt(&self) -> mt {
        let mutbl = self.parse_mutability();
        let t = ~self.parse_ty(false);
        mt { ty: t, mutbl: mutbl }
    }

    // parse [mut/const/imm] ID : TY
    // now used only by obsolete record syntax parser...
    pub fn parse_ty_field(&self) -> TypeField {
        let lo = self.span.lo;
        let mutbl = self.parse_mutability();
        let id = self.parse_ident();
        self.expect(&token::COLON);
        let ty = ~self.parse_ty(false);
        let hi = ty.span.hi;
        ast::TypeField {
            ident: id,
            mt: ast::mt { ty: ty, mutbl: mutbl },
            span: mk_sp(lo, hi),
        }
    }

    // parse optional return type [ -> TY ] in function decl
    pub fn parse_ret_ty(&self) -> (ret_style, Ty) {
        return if self.eat(&token::RARROW) {
            let lo = self.span.lo;
            if self.eat(&token::NOT) {
                (
                    noreturn,
                    Ty {
                        id: ast::DUMMY_NODE_ID,
                        node: ty_bot,
                        span: mk_sp(lo, self.last_span.hi)
                    }
                )
            } else {
                (return_val, self.parse_ty(false))
            }
        } else {
            let pos = self.span.lo;
            (
                return_val,
                Ty {
                    id: ast::DUMMY_NODE_ID,
                    node: ty_nil,
                    span: mk_sp(pos, pos),
                }
            )
        }
    }

    // parse a type.
    // Useless second parameter for compatibility with quasiquote macros.
    // Bleh!
    pub fn parse_ty(&self, _: bool) -> Ty {
        maybe_whole!(deref self, nt_ty);

        let lo = self.span.lo;

        let t = if *self.token == token::LPAREN {
            self.bump();
            if *self.token == token::RPAREN {
                self.bump();
                ty_nil
            } else {
                // (t) is a parenthesized ty
                // (t,) is the type of a tuple with only one field,
                // of type t
                let mut ts = ~[self.parse_ty(false)];
                let mut one_tuple = false;
                while *self.token == token::COMMA {
                    self.bump();
                    if *self.token != token::RPAREN {
                        ts.push(self.parse_ty(false));
                    }
                    else {
                        one_tuple = true;
                    }
                }

                if ts.len() == 1 && !one_tuple {
                    self.expect(&token::RPAREN);
                    return ts[0]
                }

                let t = ty_tup(ts);
                self.expect(&token::RPAREN);
                t
            }
        } else if *self.token == token::AT {
            // MANAGED POINTER
            self.bump();
            self.parse_box_or_uniq_pointee(ManagedSigil, ty_box)
        } else if *self.token == token::TILDE {
            // OWNED POINTER
            self.bump();
            self.parse_box_or_uniq_pointee(OwnedSigil, ty_uniq)
        } else if *self.token == token::BINOP(token::STAR) {
            // STAR POINTER (bare pointer?)
            self.bump();
            ty_ptr(self.parse_mt())
        } else if *self.token == token::LBRACE {
            // STRUCTURAL RECORD (remove?)
            let elems = self.parse_unspanned_seq(
                &token::LBRACE,
                &token::RBRACE,
                seq_sep_trailing_allowed(token::COMMA),
                |p| p.parse_ty_field()
            );
            if elems.len() == 0 {
                self.unexpected_last(&token::RBRACE);
            }
            self.obsolete(*self.last_span, ObsoleteRecordType);
            ty_nil
        } else if *self.token == token::LBRACKET {
            // VECTOR
            self.expect(&token::LBRACKET);
            let mt = self.parse_mt();
            if mt.mutbl == MutMutable {    // `m_const` too after snapshot
                self.obsolete(*self.last_span, ObsoleteMutVector);
            }

            // Parse the `, ..e` in `[ int, ..e ]`
            // where `e` is a const expression
            let t = match self.maybe_parse_fixed_vstore() {
                None => ty_vec(mt),
                Some(suffix) => ty_fixed_length_vec(mt, suffix)
            };
            self.expect(&token::RBRACKET);
            t
        } else if *self.token == token::BINOP(token::AND) {
            // BORROWED POINTER
            self.bump();
            self.parse_borrowed_pointee()
        } else if self.eat_keyword(keywords::Extern) {
            // EXTERN FUNCTION
            self.parse_ty_bare_fn()
        } else if self.token_is_closure_keyword(self.token) {
            // CLOSURE
            let result = self.parse_ty_closure(ast::BorrowedSigil, None);
            self.obsolete(*self.last_span, ObsoleteBareFnType);
            result
        } else if self.eat_keyword(keywords::Typeof) {
            // TYPEOF
            // In order to not be ambiguous, the type must be surrounded by parens.
            self.expect(&token::LPAREN);
            let e = self.parse_expr();
            self.expect(&token::RPAREN);
            ty_typeof(e)
        } else if *self.token == token::MOD_SEP
            || is_ident_or_path(self.token) {
            // NAMED TYPE
            let PathAndBounds {
                path,
                bounds
            } = self.parse_path(LifetimeAndTypesAndBounds);
            ty_path(path, bounds, ast::DUMMY_NODE_ID)
        } else {
            self.fatal(fmt!("expected type, found token %?",
                            *self.token));
        };

        let sp = mk_sp(lo, self.last_span.hi);
        Ty {id: ast::DUMMY_NODE_ID, node: t, span: sp}
    }

    // parse the type following a @ or a ~
    pub fn parse_box_or_uniq_pointee(&self,
                                     sigil: ast::Sigil,
                                     ctor: &fn(v: mt) -> ty_) -> ty_ {
        // @'foo fn() or @foo/fn() or @fn() are parsed directly as fn types:
        match *self.token {
            token::LIFETIME(*) => {
                let lifetime = self.parse_lifetime();
                return self.parse_ty_closure(sigil, Some(lifetime));
            }

            token::IDENT(*) => {
                if self.look_ahead(1, |t| *t == token::BINOP(token::SLASH)) &&
                        self.look_ahead(2, |t|
                                        self.token_is_closure_keyword(t)) {
                    let lifetime = self.parse_lifetime();
                    self.obsolete(*self.last_span, ObsoleteLifetimeNotation);
                    return self.parse_ty_closure(sigil, Some(lifetime));
                } else if self.token_is_closure_keyword(self.token) {
                    return self.parse_ty_closure(sigil, None);
                }
            }
            _ => {}
        }

        // other things are parsed as @ + a type.  Note that constructs like
        // @[] and @str will be resolved during typeck to slices and so forth,
        // rather than boxed ptrs.  But the special casing of str/vec is not
        // reflected in the AST type.
        let mt = self.parse_mt();

        if mt.mutbl != MutImmutable && sigil == OwnedSigil {
            self.obsolete(*self.last_span, ObsoleteMutOwnedPointer);
        }

        ctor(mt)
    }

    pub fn parse_borrowed_pointee(&self) -> ty_ {
        // look for `&'lt` or `&'foo ` and interpret `foo` as the region name:
        let opt_lifetime = self.parse_opt_lifetime();

        if self.token_is_closure_keyword(self.token) {
            return self.parse_ty_closure(BorrowedSigil, opt_lifetime);
        }

        let mt = self.parse_mt();
        return ty_rptr(opt_lifetime, mt);
    }

    // parse an optional, obsolete argument mode.
    pub fn parse_arg_mode(&self) {
        if self.eat(&token::BINOP(token::MINUS)) {
            self.obsolete(*self.span, ObsoleteMode);
        } else if self.eat(&token::ANDAND) {
            self.obsolete(*self.span, ObsoleteMode);
        } else if self.eat(&token::BINOP(token::PLUS)) {
            if self.eat(&token::BINOP(token::PLUS)) {
                self.obsolete(*self.span, ObsoleteMode);
            } else {
                self.obsolete(*self.span, ObsoleteMode);
            }
        } else {
            // Ignore.
        }
    }

    pub fn is_named_argument(&self) -> bool {
        let offset = match *self.token {
            token::BINOP(token::AND) => 1,
            token::BINOP(token::MINUS) => 1,
            token::ANDAND => 1,
            token::BINOP(token::PLUS) => {
                if self.look_ahead(1, |t| *t == token::BINOP(token::PLUS)) {
                    2
                } else {
                    1
                }
            },
            _ => 0
        };

        debug!("parser is_named_argument offset:%u", offset);

        if offset == 0 {
            is_plain_ident_or_underscore(&*self.token)
                && self.look_ahead(1, |t| *t == token::COLON)
        } else {
            self.look_ahead(offset, |t| is_plain_ident_or_underscore(t))
                && self.look_ahead(offset + 1, |t| *t == token::COLON)
        }
    }

    // This version of parse arg doesn't necessarily require
    // identifier names.
    pub fn parse_arg_general(&self, require_name: bool) -> arg {
        let is_mutbl = self.eat_keyword(keywords::Mut);
        let pat = if require_name || self.is_named_argument() {
            debug!("parse_arg_general parse_pat (require_name:%?)",
                   require_name);
            self.parse_arg_mode();
            let pat = self.parse_pat();

            if is_mutbl && !ast_util::pat_is_ident(pat) {
                self.obsolete(*self.span, ObsoleteMutWithMultipleBindings)
            }

            self.expect(&token::COLON);
            pat
        } else {
            debug!("parse_arg_general ident_to_pat");
            ast_util::ident_to_pat(ast::DUMMY_NODE_ID,
                                   *self.last_span,
                                   special_idents::invalid)
        };

        let t = self.parse_ty(false);

        ast::arg {
            is_mutbl: is_mutbl,
            ty: t,
            pat: pat,
            id: ast::DUMMY_NODE_ID,
        }
    }

    // parse a single function argument
    pub fn parse_arg(&self) -> arg {
        self.parse_arg_general(true)
    }

    // parse an argument in a lambda header e.g. |arg, arg|
    pub fn parse_fn_block_arg(&self) -> arg {
        self.parse_arg_mode();
        let is_mutbl = self.eat_keyword(keywords::Mut);
        let pat = self.parse_pat();
        let t = if self.eat(&token::COLON) {
            self.parse_ty(false)
        } else {
            Ty {
                id: ast::DUMMY_NODE_ID,
                node: ty_infer,
                span: mk_sp(self.span.lo, self.span.hi),
            }
        };
        ast::arg {
            is_mutbl: is_mutbl,
            ty: t,
            pat: pat,
            id: ast::DUMMY_NODE_ID
        }
    }

    pub fn maybe_parse_fixed_vstore(&self) -> Option<@ast::Expr> {
        if self.eat(&token::BINOP(token::STAR)) {
            self.obsolete(*self.last_span, ObsoleteFixedLengthVectorType);
            Some(self.parse_expr())
        } else if *self.token == token::COMMA &&
                self.look_ahead(1, |t| *t == token::DOTDOT) {
            self.bump();
            self.bump();
            Some(self.parse_expr())
        } else {
            None
        }
    }

    // matches token_lit = LIT_INT | ...
    pub fn lit_from_token(&self, tok: &token::Token) -> lit_ {
        match *tok {
            token::LIT_CHAR(i) => lit_char(i),
            token::LIT_INT(i, it) => lit_int(i, it),
            token::LIT_UINT(u, ut) => lit_uint(u, ut),
            token::LIT_INT_UNSUFFIXED(i) => lit_int_unsuffixed(i),
            token::LIT_FLOAT(s, ft) => lit_float(self.id_to_str(s), ft),
            token::LIT_FLOAT_UNSUFFIXED(s) =>
                lit_float_unsuffixed(self.id_to_str(s)),
            token::LIT_STR(s) => lit_str(self.id_to_str(s)),
            token::LPAREN => { self.expect(&token::RPAREN); lit_nil },
            _ => { self.unexpected_last(tok); }
        }
    }

    // matches lit = true | false | token_lit
    pub fn parse_lit(&self) -> lit {
        let lo = self.span.lo;
        let lit = if self.eat_keyword(keywords::True) {
            lit_bool(true)
        } else if self.eat_keyword(keywords::False) {
            lit_bool(false)
        } else {
            let token = self.bump_and_get();
            let lit = self.lit_from_token(&token);
            lit
        };
        codemap::Spanned { node: lit, span: mk_sp(lo, self.last_span.hi) }
    }

    // matches '-' lit | lit
    pub fn parse_literal_maybe_minus(&self) -> @Expr {
        let minus_lo = self.span.lo;
        let minus_present = self.eat(&token::BINOP(token::MINUS));

        let lo = self.span.lo;
        let literal = @self.parse_lit();
        let hi = self.span.hi;
        let expr = self.mk_expr(lo, hi, ExprLit(literal));

        if minus_present {
            let minus_hi = self.span.hi;
            self.mk_expr(minus_lo, minus_hi, self.mk_unary(UnNeg, expr))
        } else {
            expr
        }
    }

    /// Parses a path and optional type parameter bounds, depending on the
    /// mode. The `mode` parameter determines whether lifetimes, types, and/or
    /// bounds are permitted and whether `::` must precede type parameter
    /// groups.
    pub fn parse_path(&self, mode: PathParsingMode) -> PathAndBounds {
        // Check for a whole path...
        let found = match *self.token {
            INTERPOLATED(token::nt_path(_)) => Some(self.bump_and_get()),
            _ => None,
        };
        match found {
            Some(INTERPOLATED(token::nt_path(~path))) => {
                return PathAndBounds {
                    path: path,
                    bounds: None,
                }
            }
            _ => {}
        }

        let lo = self.span.lo;
        let is_global = self.eat(&token::MOD_SEP);

        // Parse any number of segments and bound sets. A segment is an
        // identifier followed by an optional lifetime and a set of types.
        // A bound set is a set of type parameter bounds.
        let mut segments = ~[];
        loop {
            // First, parse an identifier.
            match *self.token {
                token::IDENT(*) => {}
                _ => break,
            }
            let identifier = self.parse_ident();

            // Next, parse a colon and bounded type parameters, if applicable.
            let bound_set = if mode == LifetimeAndTypesAndBounds {
                self.parse_optional_ty_param_bounds()
            } else {
                None
            };

            // Parse the '::' before type parameters if it's required. If
            // it is required and wasn't present, then we're done.
            if mode == LifetimeAndTypesWithColons &&
                    !self.eat(&token::MOD_SEP) {
                segments.push(PathSegmentAndBoundSet {
                    segment: ast::PathSegment {
                        identifier: identifier,
                        lifetime: None,
                        types: opt_vec::Empty,
                    },
                    bound_set: bound_set
                });
                break
            }

            // Parse the `<` before the lifetime and types, if applicable.
            let (any_lifetime_or_types, optional_lifetime, types) =
                    if mode != NoTypesAllowed && self.eat(&token::LT) {
                // Parse an optional lifetime.
                let optional_lifetime = match *self.token {
                    token::LIFETIME(*) => Some(self.parse_lifetime()),
                    _ => None,
                };

                // Parse type parameters.
                let mut types = opt_vec::Empty;
                let mut need_comma = optional_lifetime.is_some();
                loop {
                    // We're done if we see a `>`.
                    match *self.token {
                        token::GT | token::BINOP(token::SHR) => {
                            self.expect_gt();
                            break
                        }
                        _ => {} // Go on.
                    }

                    if need_comma {
                        self.expect(&token::COMMA)
                    } else {
                        need_comma = true
                    }

                    types.push(self.parse_ty(false))
                }

                (true, optional_lifetime, types)
            } else {
                (false, None, opt_vec::Empty)
            };

            // Assemble and push the result.
            segments.push(PathSegmentAndBoundSet {
                segment: ast::PathSegment {
                    identifier: identifier,
                    lifetime: optional_lifetime,
                    types: types,
                },
                bound_set: bound_set
            });

            // We're done if we don't see a '::', unless the mode required
            // a double colon to get here in the first place.
            if !(mode == LifetimeAndTypesWithColons &&
                    !any_lifetime_or_types) {
                if !self.eat(&token::MOD_SEP) {
                    break
                }
            }
        }

        // Assemble the span.
        let span = mk_sp(lo, self.last_span.hi);

        // Assemble the path segments.
        let mut path_segments = ~[];
        let mut bounds = None;
        let last_segment_index = segments.len() - 1;
        for (i, segment_and_bounds) in segments.move_iter().enumerate() {
            let PathSegmentAndBoundSet {
                segment: segment,
                bound_set: bound_set
            } = segment_and_bounds;
            path_segments.push(segment);

            if bound_set.is_some() {
                if i != last_segment_index {
                    self.span_err(span,
                                  "type parameter bounds are allowed only \
                                   before the last segment in a path")
                }

                bounds = bound_set
            }
        }

        // Assemble the result.
        let path_and_bounds = PathAndBounds {
            path: ast::Path {
                span: span,
                global: is_global,
                segments: path_segments,
            },
            bounds: bounds,
        };

        path_and_bounds
    }

    /// parses 0 or 1 lifetime
    pub fn parse_opt_lifetime(&self) -> Option<ast::Lifetime> {
        match *self.token {
            token::LIFETIME(*) => {
                Some(self.parse_lifetime())
            }

            // Also accept the (obsolete) syntax `foo/`
            token::IDENT(*) => {
                if self.look_ahead(1, |t| *t == token::BINOP(token::SLASH)) {
                    self.obsolete(*self.last_span, ObsoleteLifetimeNotation);
                    Some(self.parse_lifetime())
                } else {
                    None
                }
            }

            _ => {
                None
            }
        }
    }

    /// Parses a single lifetime
    // matches lifetime = ( LIFETIME ) | ( IDENT / )
    pub fn parse_lifetime(&self) -> ast::Lifetime {
        match *self.token {
            token::LIFETIME(i) => {
                let span = self.span;
                self.bump();
                return ast::Lifetime {
                    id: ast::DUMMY_NODE_ID,
                    span: *span,
                    ident: i
                };
            }

            // Also accept the (obsolete) syntax `foo/`
            token::IDENT(i, _) => {
                let span = self.span;
                self.bump();
                self.expect(&token::BINOP(token::SLASH));
                self.obsolete(*self.last_span, ObsoleteLifetimeNotation);
                return ast::Lifetime {
                    id: ast::DUMMY_NODE_ID,
                    span: *span,
                    ident: i
                };
            }

            _ => {
                self.fatal(fmt!("Expected a lifetime name"));
            }
        }
    }

    // matches lifetimes = ( lifetime ) | ( lifetime , lifetimes )
    // actually, it matches the empty one too, but putting that in there
    // messes up the grammar....
    pub fn parse_lifetimes(&self) -> OptVec<ast::Lifetime> {
        /*!
         *
         * Parses zero or more comma separated lifetimes.
         * Expects each lifetime to be followed by either
         * a comma or `>`.  Used when parsing type parameter
         * lists, where we expect something like `<'a, 'b, T>`.
         */

        let mut res = opt_vec::Empty;
        loop {
            match *self.token {
                token::LIFETIME(_) => {
                    res.push(self.parse_lifetime());
                }
                _ => {
                    return res;
                }
            }

            match *self.token {
                token::COMMA => { self.bump();}
                token::GT => { return res; }
                token::BINOP(token::SHR) => { return res; }
                _ => {
                    self.fatal(fmt!("expected `,` or `>` after lifetime name, got: %?",
                                    *self.token));
                }
            }
        }
    }

    pub fn token_is_mutability(&self, tok: &token::Token) -> bool {
        token::is_keyword(keywords::Mut, tok) ||
        token::is_keyword(keywords::Const, tok)
    }

    // parse mutability declaration (mut/const/imm)
    pub fn parse_mutability(&self) -> Mutability {
        if self.eat_keyword(keywords::Mut) {
            MutMutable
        } else if self.eat_keyword(keywords::Const) {
            self.obsolete(*self.last_span, ObsoleteConstPointer);
            MutImmutable
        } else {
            MutImmutable
        }
    }

    // parse ident COLON expr
    pub fn parse_field(&self) -> Field {
        let lo = self.span.lo;
        let i = self.parse_ident();
        self.expect(&token::COLON);
        let e = self.parse_expr();
        ast::Field {
            ident: i,
            expr: e,
            span: mk_sp(lo, e.span.hi),
        }
    }

    pub fn mk_expr(&self, lo: BytePos, hi: BytePos, node: Expr_) -> @Expr {
        @Expr {
            id: ast::DUMMY_NODE_ID,
            node: node,
            span: mk_sp(lo, hi),
        }
    }

    pub fn mk_unary(&self, unop: ast::UnOp, expr: @Expr) -> ast::Expr_ {
        ExprUnary(ast::DUMMY_NODE_ID, unop, expr)
    }

    pub fn mk_binary(&self, binop: ast::BinOp, lhs: @Expr, rhs: @Expr) -> ast::Expr_ {
        ExprBinary(ast::DUMMY_NODE_ID, binop, lhs, rhs)
    }

    pub fn mk_call(&self, f: @Expr, args: ~[@Expr], sugar: CallSugar) -> ast::Expr_ {
        ExprCall(f, args, sugar)
    }

    pub fn mk_method_call(&self,
                      rcvr: @Expr,
                      ident: Ident,
                      tps: ~[Ty],
                      args: ~[@Expr],
                      sugar: CallSugar) -> ast::Expr_ {
        ExprMethodCall(ast::DUMMY_NODE_ID, rcvr, ident, tps, args, sugar)
    }

    pub fn mk_index(&self, expr: @Expr, idx: @Expr) -> ast::Expr_ {
        ExprIndex(ast::DUMMY_NODE_ID, expr, idx)
    }

    pub fn mk_field(&self, expr: @Expr, ident: Ident, tys: ~[Ty]) -> ast::Expr_ {
        ExprField(expr, ident, tys)
    }

    pub fn mk_assign_op(&self, binop: ast::BinOp, lhs: @Expr, rhs: @Expr) -> ast::Expr_ {
        ExprAssignOp(ast::DUMMY_NODE_ID, binop, lhs, rhs)
    }

    pub fn mk_mac_expr(&self, lo: BytePos, hi: BytePos, m: mac_) -> @Expr {
        @Expr {
            id: ast::DUMMY_NODE_ID,
            node: ExprMac(codemap::Spanned {node: m, span: mk_sp(lo, hi)}),
            span: mk_sp(lo, hi),
        }
    }

    pub fn mk_lit_u32(&self, i: u32) -> @Expr {
        let span = self.span;
        let lv_lit = @codemap::Spanned {
            node: lit_uint(i as u64, ty_u32),
            span: *span
        };

        @Expr {
            id: ast::DUMMY_NODE_ID,
            node: ExprLit(lv_lit),
            span: *span,
        }
    }

    // at the bottom (top?) of the precedence hierarchy,
    // parse things like parenthesized exprs,
    // macros, return, etc.
    pub fn parse_bottom_expr(&self) -> @Expr {
        maybe_whole_expr!(self);

        let lo = self.span.lo;
        let mut hi = self.span.hi;

        let ex: Expr_;

        if *self.token == token::LPAREN {
            self.bump();
            // (e) is parenthesized e
            // (e,) is a tuple with only one field, e
            let mut trailing_comma = false;
            if *self.token == token::RPAREN {
                hi = self.span.hi;
                self.bump();
                let lit = @spanned(lo, hi, lit_nil);
                return self.mk_expr(lo, hi, ExprLit(lit));
            }
            let mut es = ~[self.parse_expr()];
            self.commit_expr(*es.last(), &[], &[token::COMMA, token::RPAREN]);
            while *self.token == token::COMMA {
                self.bump();
                if *self.token != token::RPAREN {
                    es.push(self.parse_expr());
                    self.commit_expr(*es.last(), &[], &[token::COMMA, token::RPAREN]);
                }
                else {
                    trailing_comma = true;
                }
            }
            hi = self.span.hi;
            self.commit_expr_expecting(*es.last(), token::RPAREN);

            return if es.len() == 1 && !trailing_comma {
                self.mk_expr(lo, self.span.hi, ExprParen(es[0]))
            }
            else {
                self.mk_expr(lo, hi, ExprTup(es))
            }
        } else if *self.token == token::LBRACE {
            self.bump();
            let blk = self.parse_block_tail(lo, DefaultBlock);
            return self.mk_expr(blk.span.lo, blk.span.hi,
                                 ExprBlock(blk));
        } else if token::is_bar(&*self.token) {
            return self.parse_lambda_expr();
        } else if self.eat_keyword(keywords::Self) {
            ex = ExprSelf;
            hi = self.span.hi;
        } else if self.eat_keyword(keywords::If) {
            return self.parse_if_expr();
        } else if self.eat_keyword(keywords::For) {
            return self.parse_for_expr(None);
        } else if self.eat_keyword(keywords::Do) {
            return self.parse_sugary_call_expr(lo, ~"do", DoSugar,
                                               ExprDoBody);
        } else if self.eat_keyword(keywords::While) {
            return self.parse_while_expr();
        } else if self.token_is_lifetime(&*self.token) {
            let lifetime = self.get_lifetime(&*self.token);
            self.bump();
            self.expect(&token::COLON);
            if self.eat_keyword(keywords::For) {
                return self.parse_for_expr(Some(lifetime))
            } else if self.eat_keyword(keywords::Loop) {
                return self.parse_loop_expr(Some(lifetime))
            } else {
                self.fatal("expected `for` or `loop` after a label")
            }
        } else if self.eat_keyword(keywords::Loop) {
            return self.parse_loop_expr(None);
        } else if self.eat_keyword(keywords::Match) {
            return self.parse_match_expr();
        } else if self.eat_keyword(keywords::Unsafe) {
            return self.parse_block_expr(lo, UnsafeBlock(ast::UserProvided));
        } else if *self.token == token::LBRACKET {
            self.bump();
            let mutbl = self.parse_mutability();
            if mutbl == MutMutable {
                self.obsolete(*self.last_span, ObsoleteMutVector);
            }

            if *self.token == token::RBRACKET {
                // Empty vector.
                self.bump();
                ex = ExprVec(~[], mutbl);
            } else {
                // Nonempty vector.
                let first_expr = self.parse_expr();
                if *self.token == token::COMMA &&
                        self.look_ahead(1, |t| *t == token::DOTDOT) {
                    // Repeating vector syntax: [ 0, ..512 ]
                    self.bump();
                    self.bump();
                    let count = self.parse_expr();
                    self.expect(&token::RBRACKET);
                    ex = ExprRepeat(first_expr, count, mutbl);
                } else if *self.token == token::COMMA {
                    // Vector with two or more elements.
                    self.bump();
                    let remaining_exprs = self.parse_seq_to_end(
                        &token::RBRACKET,
                        seq_sep_trailing_allowed(token::COMMA),
                        |p| p.parse_expr()
                    );
                    ex = ExprVec(~[first_expr] + remaining_exprs, mutbl);
                } else {
                    // Vector with one element.
                    self.expect(&token::RBRACKET);
                    ex = ExprVec(~[first_expr], mutbl);
                }
            }
            hi = self.last_span.hi;
        } else if self.eat_keyword(keywords::__LogLevel) {
            // LOG LEVEL expression
            self.expect(&token::LPAREN);
            ex = ExprLogLevel;
            hi = self.span.hi;
            self.expect(&token::RPAREN);
        } else if self.eat_keyword(keywords::Return) {
            // RETURN expression
            if can_begin_expr(&*self.token) {
                let e = self.parse_expr();
                hi = e.span.hi;
                ex = ExprRet(Some(e));
            } else { ex = ExprRet(None); }
        } else if self.eat_keyword(keywords::Break) {
            // BREAK expression
            if self.token_is_lifetime(&*self.token) {
                let lifetime = self.get_lifetime(&*self.token);
                self.bump();
                ex = ExprBreak(Some(lifetime.name));
            } else {
                ex = ExprBreak(None);
            }
            hi = self.span.hi;
        } else if *self.token == token::MOD_SEP ||
                is_ident(&*self.token) && !self.is_keyword(keywords::True) &&
                !self.is_keyword(keywords::False) {
            let pth = self.parse_path(LifetimeAndTypesWithColons).path;

            // `!`, as an operator, is prefix, so we know this isn't that
            if *self.token == token::NOT {
                // MACRO INVOCATION expression
                self.bump();
                match *self.token {
                    token::LPAREN | token::LBRACE => {}
                    _ => self.fatal("expected open delimiter")
                };

                let ket = token::flip_delimiter(&*self.token);
                self.bump();

                let tts = self.parse_seq_to_end(&ket,
                                                seq_sep_none(),
                                                |p| p.parse_token_tree());
                let hi = self.span.hi;

                return self.mk_mac_expr(lo, hi, mac_invoc_tt(pth, tts, EMPTY_CTXT));
            } else if *self.token == token::LBRACE {
                // This might be a struct literal.
                if self.looking_at_record_literal() {
                    // It's a struct literal.
                    self.bump();
                    let mut fields = ~[];
                    let mut base = None;

                    fields.push(self.parse_field());
                    while *self.token != token::RBRACE {
                        if self.try_parse_obsolete_with() {
                            break;
                        }

                        self.commit_expr(fields.last().expr, &[token::COMMA], &[token::RBRACE]);

                        if self.eat(&token::DOTDOT) {
                            base = Some(self.parse_expr());
                            break;
                        }

                        if *self.token == token::RBRACE {
                            // Accept an optional trailing comma.
                            break;
                        }
                        fields.push(self.parse_field());
                    }

                    hi = pth.span.hi;
                    self.commit_expr_expecting(fields.last().expr, token::RBRACE);
                    ex = ExprStruct(pth, fields, base);
                    return self.mk_expr(lo, hi, ex);
                }
            }

            hi = pth.span.hi;
            ex = ExprPath(pth);
        } else {
            // other literal expression
            let lit = self.parse_lit();
            hi = lit.span.hi;
            ex = ExprLit(@lit);
        }

        return self.mk_expr(lo, hi, ex);
    }

    // parse a block or unsafe block
    pub fn parse_block_expr(&self, lo: BytePos, blk_mode: BlockCheckMode)
                            -> @Expr {
        self.expect(&token::LBRACE);
        let blk = self.parse_block_tail(lo, blk_mode);
        return self.mk_expr(blk.span.lo, blk.span.hi, ExprBlock(blk));
    }

    // parse a.b or a(13) or a[4] or just a
    pub fn parse_dot_or_call_expr(&self) -> @Expr {
        let b = self.parse_bottom_expr();
        self.parse_dot_or_call_expr_with(b)
    }

    pub fn parse_dot_or_call_expr_with(&self, e0: @Expr) -> @Expr {
        let mut e = e0;
        let lo = e.span.lo;
        let mut hi;
        loop {
            // expr.f
            if self.eat(&token::DOT) {
                match *self.token {
                  token::IDENT(i, _) => {
                    hi = self.span.hi;
                    self.bump();
                    let (_, tys) = if self.eat(&token::MOD_SEP) {
                        self.expect(&token::LT);
                        self.parse_generic_values_after_lt()
                    } else {
                        (opt_vec::Empty, ~[])
                    };

                    // expr.f() method call
                    match *self.token {
                        token::LPAREN => {
                            let es = self.parse_unspanned_seq(
                                &token::LPAREN,
                                &token::RPAREN,
                                seq_sep_trailing_disallowed(token::COMMA),
                                |p| p.parse_expr()
                            );
                            hi = self.span.hi;

                            let nd = self.mk_method_call(e, i, tys, es, NoSugar);
                            e = self.mk_expr(lo, hi, nd);
                        }
                        _ => {
                            e = self.mk_expr(lo, hi, self.mk_field(e, i, tys));
                        }
                    }
                  }
                  _ => self.unexpected()
                }
                loop;
            }
            if self.expr_is_complete(e) { break; }
            match *self.token {
              // expr(...)
              token::LPAREN => {
                let es = self.parse_unspanned_seq(
                    &token::LPAREN,
                    &token::RPAREN,
                    seq_sep_trailing_disallowed(token::COMMA),
                    |p| p.parse_expr()
                );
                hi = self.span.hi;

                let nd = self.mk_call(e, es, NoSugar);
                e = self.mk_expr(lo, hi, nd);
              }

              // expr[...]
              token::LBRACKET => {
                self.bump();
                let ix = self.parse_expr();
                hi = ix.span.hi;
                self.commit_expr_expecting(ix, token::RBRACKET);
                e = self.mk_expr(lo, hi, self.mk_index(e, ix));
              }

              _ => return e
            }
        }
        return e;
    }

    // parse an optional separator followed by a kleene-style
    // repetition token (+ or *).
    pub fn parse_sep_and_zerok(&self) -> (Option<token::Token>, bool) {
        fn parse_zerok(parser: &Parser) -> Option<bool> {
            match *parser.token {
                token::BINOP(token::STAR) | token::BINOP(token::PLUS) => {
                    let zerok = *parser.token == token::BINOP(token::STAR);
                    parser.bump();
                    Some(zerok)
                },
                _ => None
            }
        };

        match parse_zerok(self) {
            Some(zerok) => return (None, zerok),
            None => {}
        }

        let separator = self.bump_and_get();
        match parse_zerok(self) {
            Some(zerok) => (Some(separator), zerok),
            None => self.fatal("expected `*` or `+`")
        }
    }

    // parse a single token tree from the input.
    pub fn parse_token_tree(&self) -> token_tree {
        maybe_whole!(deref self, nt_tt);

        // this is the fall-through for the 'match' below.
        // invariants: the current token is not a left-delimiter,
        // not an EOF, and not the desired right-delimiter (if
        // it were, parse_seq_to_before_end would have prevented
        // reaching this point.
        fn parse_non_delim_tt_tok(p: &Parser) -> token_tree {
            maybe_whole!(deref p, nt_tt);
            match *p.token {
              token::RPAREN | token::RBRACE | token::RBRACKET
              => {
                p.fatal(
                    fmt!(
                        "incorrect close delimiter: `%s`",
                        p.this_token_to_str()
                    )
                );
              }
              /* we ought to allow different depths of unquotation */
              token::DOLLAR if *p.quote_depth > 0u => {
                p.bump();
                let sp = *p.span;

                if *p.token == token::LPAREN {
                    let seq = p.parse_seq(
                        &token::LPAREN,
                        &token::RPAREN,
                        seq_sep_none(),
                        |p| p.parse_token_tree()
                    );
                    let (s, z) = p.parse_sep_and_zerok();
                    let seq = match seq {
                        Spanned { node, _ } => node,
                    };
                    tt_seq(
                        mk_sp(sp.lo, p.span.hi),
                        @mut seq,
                        s,
                        z
                    )
                } else {
                    tt_nonterminal(sp, p.parse_ident())
                }
              }
              _ => {
                  parse_any_tt_tok(p)
              }
            }
        }

        // turn the next token into a tt_tok:
        fn parse_any_tt_tok(p: &Parser) -> token_tree{
            tt_tok(*p.span, p.bump_and_get())
        }

        match *self.token {
            token::EOF => {
                self.fatal("file ended with unbalanced delimiters");
            }
            token::LPAREN | token::LBRACE | token::LBRACKET => {
                let close_delim = token::flip_delimiter(&*self.token);

                // Parse the open delimiter.
                let mut result = ~[parse_any_tt_tok(self)];

                let trees =
                    self.parse_seq_to_before_end(&close_delim,
                                                 seq_sep_none(),
                                                 |p| p.parse_token_tree());
                result.push_all_move(trees);

                // Parse the close delimiter.
                result.push(parse_any_tt_tok(self));

                tt_delim(@mut result)
            }
            _ => parse_non_delim_tt_tok(self)
        }
    }

    // parse a stream of tokens into a list of token_trees,
    // up to EOF.
    pub fn parse_all_token_trees(&self) -> ~[token_tree] {
        let mut tts = ~[];
        while *self.token != token::EOF {
            tts.push(self.parse_token_tree());
        }
        tts
    }

    pub fn parse_matchers(&self) -> ~[matcher] {
        // unification of matchers and token_trees would vastly improve
        // the interpolation of matchers
        maybe_whole!(self, nt_matchers);
        let name_idx = @mut 0u;
        match *self.token {
            token::LBRACE | token::LPAREN | token::LBRACKET => {
                let other_delimiter = token::flip_delimiter(self.token);
                self.bump();
                self.parse_matcher_subseq_upto(name_idx, &other_delimiter)
            }
            _ => self.fatal("expected open delimiter")
        }
    }

    // This goofy function is necessary to correctly match parens in matchers.
    // Otherwise, `$( ( )` would be a valid matcher, and `$( () )` would be
    // invalid. It's similar to common::parse_seq.
    pub fn parse_matcher_subseq_upto(&self,
                                     name_idx: @mut uint,
                                     ket: &token::Token)
                                     -> ~[matcher] {
        let mut ret_val = ~[];
        let mut lparens = 0u;

        while *self.token != *ket || lparens > 0u {
            if *self.token == token::LPAREN { lparens += 1u; }
            if *self.token == token::RPAREN { lparens -= 1u; }
            ret_val.push(self.parse_matcher(name_idx));
        }

        self.bump();

        return ret_val;
    }

    pub fn parse_matcher(&self, name_idx: @mut uint) -> matcher {
        let lo = self.span.lo;

        let m = if *self.token == token::DOLLAR {
            self.bump();
            if *self.token == token::LPAREN {
                let name_idx_lo = *name_idx;
                self.bump();
                let ms = self.parse_matcher_subseq_upto(name_idx,
                                                        &token::RPAREN);
                if ms.len() == 0u {
                    self.fatal("repetition body must be nonempty");
                }
                let (sep, zerok) = self.parse_sep_and_zerok();
                match_seq(ms, sep, zerok, name_idx_lo, *name_idx)
            } else {
                let bound_to = self.parse_ident();
                self.expect(&token::COLON);
                let nt_name = self.parse_ident();
                let m = match_nonterminal(bound_to, nt_name, *name_idx);
                *name_idx += 1u;
                m
            }
        } else {
            match_tok(self.bump_and_get())
        };

        return spanned(lo, self.span.hi, m);
    }

    // parse a prefix-operator expr
    pub fn parse_prefix_expr(&self) -> @Expr {
        let lo = self.span.lo;
        let hi;

        let ex;
        match *self.token {
          token::NOT => {
            self.bump();
            let e = self.parse_prefix_expr();
            hi = e.span.hi;
            ex = self.mk_unary(UnNot, e);
          }
          token::BINOP(b) => {
            match b {
              token::MINUS => {
                self.bump();
                let e = self.parse_prefix_expr();
                hi = e.span.hi;
                ex = self.mk_unary(UnNeg, e);
              }
              token::STAR => {
                self.bump();
                let e = self.parse_prefix_expr();
                hi = e.span.hi;
                ex = self.mk_unary(UnDeref, e);
              }
              token::AND => {
                self.bump();
                let _lt = self.parse_opt_lifetime();
                let m = self.parse_mutability();
                let e = self.parse_prefix_expr();
                hi = e.span.hi;
                // HACK: turn &[...] into a &-evec
                ex = match e.node {
                  ExprVec(*) | ExprLit(@codemap::Spanned {
                    node: lit_str(_), span: _
                  })
                  if m == MutImmutable => {
                    ExprVstore(e, ExprVstoreSlice)
                  }
                  ExprVec(*) if m == MutMutable => {
                    ExprVstore(e, ExprVstoreMutSlice)
                  }
                  _ => ExprAddrOf(m, e)
                };
              }
              _ => return self.parse_dot_or_call_expr()
            }
          }
          token::AT => {
            self.bump();
            let m = self.parse_mutability();
            let e = self.parse_prefix_expr();
            hi = e.span.hi;
            // HACK: turn @[...] into a @-evec
            ex = match e.node {
              ExprVec(*) | ExprRepeat(*) if m == MutMutable =>
                ExprVstore(e, ExprVstoreMutBox),
              ExprVec(*) |
              ExprLit(@codemap::Spanned { node: lit_str(_), span: _}) |
              ExprRepeat(*) if m == MutImmutable => ExprVstore(e, ExprVstoreBox),
              _ => self.mk_unary(UnBox(m), e)
            };
          }
          token::TILDE => {
            self.bump();
            let m = self.parse_mutability();
            if m != MutImmutable {
                self.obsolete(*self.last_span, ObsoleteMutOwnedPointer);
            }

            let e = self.parse_prefix_expr();
            hi = e.span.hi;
            // HACK: turn ~[...] into a ~-evec
            ex = match e.node {
              ExprVec(*) |
              ExprLit(@codemap::Spanned { node: lit_str(_), span: _}) |
              ExprRepeat(*) => ExprVstore(e, ExprVstoreUniq),
              _ => self.mk_unary(UnUniq, e)
            };
          }
          _ => return self.parse_dot_or_call_expr()
        }
        return self.mk_expr(lo, hi, ex);
    }

    // parse an expression of binops
    pub fn parse_binops(&self) -> @Expr {
        self.parse_more_binops(self.parse_prefix_expr(), 0)
    }

    // parse an expression of binops of at least min_prec precedence
    pub fn parse_more_binops(&self, lhs: @Expr, min_prec: uint) -> @Expr {
        if self.expr_is_complete(lhs) { return lhs; }

        // Prevent dynamic borrow errors later on by limiting the
        // scope of the borrows.
        {
            let token: &token::Token = self.token;
            let restriction: &restriction = self.restriction;
            match (token, restriction) {
                (&token::BINOP(token::OR), &RESTRICT_NO_BAR_OP) => return lhs,
                (&token::BINOP(token::OR),
                 &RESTRICT_NO_BAR_OR_DOUBLEBAR_OP) => return lhs,
                (&token::OROR, &RESTRICT_NO_BAR_OR_DOUBLEBAR_OP) => return lhs,
                _ => { }
            }
        }

        let cur_opt = token_to_binop(self.token);
        match cur_opt {
            Some(cur_op) => {
                let cur_prec = operator_prec(cur_op);
                if cur_prec > min_prec {
                    self.bump();
                    let expr = self.parse_prefix_expr();
                    let rhs = self.parse_more_binops(expr, cur_prec);
                    let bin = self.mk_expr(lhs.span.lo, rhs.span.hi,
                                           self.mk_binary(cur_op, lhs, rhs));
                    self.parse_more_binops(bin, min_prec)
                } else {
                    lhs
                }
            }
            None => {
                if as_prec > min_prec && self.eat_keyword(keywords::As) {
                    let rhs = self.parse_ty(true);
                    let _as = self.mk_expr(lhs.span.lo,
                                           rhs.span.hi,
                                           ExprCast(lhs, rhs));
                    self.parse_more_binops(_as, min_prec)
                } else {
                    lhs
                }
            }
        }
    }

    // parse an assignment expression....
    // actually, this seems to be the main entry point for
    // parsing an arbitrary expression.
    pub fn parse_assign_expr(&self) -> @Expr {
        let lo = self.span.lo;
        let lhs = self.parse_binops();
        match *self.token {
          token::EQ => {
              self.bump();
              let rhs = self.parse_expr();
              self.mk_expr(lo, rhs.span.hi, ExprAssign(lhs, rhs))
          }
          token::BINOPEQ(op) => {
              self.bump();
              let rhs = self.parse_expr();
              let aop = match op {
                  token::PLUS =>    BiAdd,
                  token::MINUS =>   BiSub,
                  token::STAR =>    BiMul,
                  token::SLASH =>   BiDiv,
                  token::PERCENT => BiRem,
                  token::CARET =>   BiBitXor,
                  token::AND =>     BiBitAnd,
                  token::OR =>      BiBitOr,
                  token::SHL =>     BiShl,
                  token::SHR =>     BiShr
              };
              self.mk_expr(lo, rhs.span.hi,
                           self.mk_assign_op(aop, lhs, rhs))
          }
          token::LARROW => {
              self.obsolete(*self.span, ObsoleteBinaryMove);
              // Bogus value (but it's an error)
              self.bump(); // <-
              self.bump(); // rhs
              self.bump(); // ;
              self.mk_expr(lo, self.span.hi,
                           ExprBreak(None))
          }
          token::DARROW => {
            self.obsolete(*self.span, ObsoleteSwap);
            self.bump();
            // Ignore what we get, this is an error anyway
            self.parse_expr();
            self.mk_expr(lo, self.span.hi, ExprBreak(None))
          }
          _ => {
              lhs
          }
        }
    }

    // parse an 'if' expression ('if' token already eaten)
    pub fn parse_if_expr(&self) -> @Expr {
        let lo = self.last_span.lo;
        let cond = self.parse_expr();
        let thn = self.parse_block();
        let mut els: Option<@Expr> = None;
        let mut hi = thn.span.hi;
        if self.eat_keyword(keywords::Else) {
            let elexpr = self.parse_else_expr();
            els = Some(elexpr);
            hi = elexpr.span.hi;
        }
        self.mk_expr(lo, hi, ExprIf(cond, thn, els))
    }

    // `|args| { ... }` or `{ ...}` like in `do` expressions
    pub fn parse_lambda_block_expr(&self) -> @Expr {
        self.parse_lambda_expr_(
            || {
                match *self.token {
                  token::BINOP(token::OR) | token::OROR => {
                    self.parse_fn_block_decl()
                  }
                  _ => {
                    // No argument list - `do foo {`
                      ast::fn_decl {
                          inputs: ~[],
                          output: Ty {
                              id: ast::DUMMY_NODE_ID,
                              node: ty_infer,
                              span: *self.span
                          },
                          cf: return_val
                      }
                  }
                }
            },
            || {
                let blk = self.parse_block();
                self.mk_expr(blk.span.lo, blk.span.hi, ExprBlock(blk))
            })
    }

    // `|args| expr`
    pub fn parse_lambda_expr(&self) -> @Expr {
        self.parse_lambda_expr_(|| self.parse_fn_block_decl(),
                                || self.parse_expr())
    }

    // parse something of the form |args| expr
    // this is used both in parsing a lambda expr
    // and in parsing a block expr as e.g. in for...
    pub fn parse_lambda_expr_(&self,
                              parse_decl: &fn() -> fn_decl,
                              parse_body: &fn() -> @Expr)
                              -> @Expr {
        let lo = self.last_span.lo;
        let decl = parse_decl();
        let body = parse_body();
        let fakeblock = ast::Block {
            view_items: ~[],
            stmts: ~[],
            expr: Some(body),
            id: ast::DUMMY_NODE_ID,
            rules: DefaultBlock,
            span: body.span,
        };

        return self.mk_expr(lo, body.span.hi,
                            ExprFnBlock(decl, fakeblock));
    }

    pub fn parse_else_expr(&self) -> @Expr {
        if self.eat_keyword(keywords::If) {
            return self.parse_if_expr();
        } else {
            let blk = self.parse_block();
            return self.mk_expr(blk.span.lo, blk.span.hi, ExprBlock(blk));
        }
    }

    // parse a 'for' .. 'in' expression ('for' token already eaten)
    pub fn parse_for_expr(&self, opt_ident: Option<ast::Ident>) -> @Expr {
        // Parse: `for <src_pat> in <src_expr> <src_loop_block>`

        let lo = self.last_span.lo;
        let pat = self.parse_pat();
        self.expect_keyword(keywords::In);
        let expr = self.parse_expr();
        let loop_block = self.parse_block();
        let hi = self.span.hi;

        self.mk_expr(lo, hi, ExprForLoop(pat, expr, loop_block, opt_ident))
    }


    // parse a 'for' or 'do'.
    // the 'for' and 'do' expressions parse as calls, but look like
    // function calls followed by a closure expression.
    pub fn parse_sugary_call_expr(&self, lo: BytePos,
                                  keyword: ~str,
                                  sugar: CallSugar,
                                  ctor: &fn(v: @Expr) -> Expr_)
                                  -> @Expr {
        // Parse the callee `foo` in
        //    for foo || {
        //    for foo.bar || {
        // etc, or the portion of the call expression before the lambda in
        //    for foo() || {
        // or
        //    for foo.bar(a) || {
        // Turn on the restriction to stop at | or || so we can parse
        // them as the lambda arguments
        let e = self.parse_expr_res(RESTRICT_NO_BAR_OR_DOUBLEBAR_OP);
        match e.node {
            ExprCall(f, ref args, NoSugar) => {
                let block = self.parse_lambda_block_expr();
                let last_arg = self.mk_expr(block.span.lo, block.span.hi,
                                            ctor(block));
                let args = vec::append((*args).clone(), [last_arg]);
                self.mk_expr(lo, block.span.hi, ExprCall(f, args, sugar))
            }
            ExprMethodCall(_, f, i, ref tps, ref args, NoSugar) => {
                let block = self.parse_lambda_block_expr();
                let last_arg = self.mk_expr(block.span.lo, block.span.hi,
                                            ctor(block));
                let args = vec::append((*args).clone(), [last_arg]);
                self.mk_expr(lo, block.span.hi,
                             self.mk_method_call(f,
                                                 i,
                                                 (*tps).clone(),
                                                 args,
                                                 sugar))
            }
            ExprField(f, i, ref tps) => {
                let block = self.parse_lambda_block_expr();
                let last_arg = self.mk_expr(block.span.lo, block.span.hi,
                                            ctor(block));
                self.mk_expr(lo, block.span.hi,
                             self.mk_method_call(f,
                                                 i,
                                                 (*tps).clone(),
                                                 ~[last_arg],
                                                 sugar))
            }
            ExprPath(*) | ExprCall(*) | ExprMethodCall(*) |
                ExprParen(*) => {
                let block = self.parse_lambda_block_expr();
                let last_arg = self.mk_expr(block.span.lo, block.span.hi,
                                            ctor(block));
                self.mk_expr(
                    lo,
                    last_arg.span.hi,
                    self.mk_call(e, ~[last_arg], sugar))
            }
            _ => {
                // There may be other types of expressions that can
                // represent the callee in `for` and `do` expressions
                // but they aren't represented by tests
                debug!("sugary call on %?", e.node);
                self.span_fatal(
                    e.span,
                    fmt!("`%s` must be followed by a block call", keyword));
            }
        }
    }

    pub fn parse_while_expr(&self) -> @Expr {
        let lo = self.last_span.lo;
        let cond = self.parse_expr();
        let body = self.parse_block();
        let hi = body.span.hi;
        return self.mk_expr(lo, hi, ExprWhile(cond, body));
    }

    pub fn parse_loop_expr(&self, opt_ident: Option<ast::Ident>) -> @Expr {
        // loop headers look like 'loop {' or 'loop unsafe {'
        let is_loop_header =
            *self.token == token::LBRACE
            || (is_ident(&*self.token)
                && self.look_ahead(1, |t| *t == token::LBRACE));

        if is_loop_header {
            // This is a loop body
            let lo = self.last_span.lo;
            let body = self.parse_block();
            let hi = body.span.hi;
            return self.mk_expr(lo, hi, ExprLoop(body, opt_ident));
        } else {
            // This is a 'continue' expression
            if opt_ident.is_some() {
                self.span_err(*self.last_span,
                              "a label may not be used with a `loop` expression");
            }

            let lo = self.span.lo;
            let ex = if self.token_is_lifetime(&*self.token) {
                let lifetime = self.get_lifetime(&*self.token);
                self.bump();
                ExprAgain(Some(lifetime.name))
            } else {
                ExprAgain(None)
            };
            let hi = self.span.hi;
            return self.mk_expr(lo, hi, ex);
        }
    }

    // For distingishing between record literals and blocks
    fn looking_at_record_literal(&self) -> bool {
        *self.token == token::LBRACE &&
            (self.look_ahead(1, |t| token::is_keyword(keywords::Mut, t)) ||
             (self.look_ahead(1, |t| token::is_plain_ident(t)) &&
              self.look_ahead(2, |t| *t == token::COLON)))
    }

    fn parse_match_expr(&self) -> @Expr {
        let lo = self.last_span.lo;
        let discriminant = self.parse_expr();
        self.commit_expr_expecting(discriminant, token::LBRACE);
        let mut arms: ~[Arm] = ~[];
        while *self.token != token::RBRACE {
            let pats = self.parse_pats();
            let mut guard = None;
            if self.eat_keyword(keywords::If) {
                guard = Some(self.parse_expr());
            }
            self.expect(&token::FAT_ARROW);
            let expr = self.parse_expr_res(RESTRICT_STMT_EXPR);

            let require_comma =
                !classify::expr_is_simple_block(expr)
                && *self.token != token::RBRACE;

            if require_comma {
                self.commit_expr(expr, &[token::COMMA], &[token::RBRACE]);
            } else {
                self.eat(&token::COMMA);
            }

            let blk = ast::Block {
                view_items: ~[],
                stmts: ~[],
                expr: Some(expr),
                id: ast::DUMMY_NODE_ID,
                rules: DefaultBlock,
                span: expr.span,
            };

            arms.push(ast::Arm { pats: pats, guard: guard, body: blk });
        }
        let hi = self.span.hi;
        self.bump();
        return self.mk_expr(lo, hi, ExprMatch(discriminant, arms));
    }

    // parse an expression
    pub fn parse_expr(&self) -> @Expr {
        return self.parse_expr_res(UNRESTRICTED);
    }

    // parse an expression, subject to the given restriction
    fn parse_expr_res(&self, r: restriction) -> @Expr {
        let old = *self.restriction;
        *self.restriction = r;
        let e = self.parse_assign_expr();
        *self.restriction = old;
        return e;
    }

    // parse the RHS of a local variable declaration (e.g. '= 14;')
    fn parse_initializer(&self) -> Option<@Expr> {
        match *self.token {
          token::EQ => {
            self.bump();
            return Some(self.parse_expr());
          }
          token::LARROW => {
              self.obsolete(*self.span, ObsoleteMoveInit);
              self.bump();
              self.bump();
              return None;
          }
          _ => {
            return None;
          }
        }
    }

    // parse patterns, separated by '|' s
    fn parse_pats(&self) -> ~[@Pat] {
        let mut pats = ~[];
        loop {
            pats.push(self.parse_pat());
            if *self.token == token::BINOP(token::OR) { self.bump(); }
            else { return pats; }
        };
    }

    fn parse_pat_vec_elements(
        &self,
    ) -> (~[@Pat], Option<@Pat>, ~[@Pat]) {
        let mut before = ~[];
        let mut slice = None;
        let mut after = ~[];
        let mut first = true;
        let mut before_slice = true;

        while *self.token != token::RBRACKET {
            if first { first = false; }
            else { self.expect(&token::COMMA); }

            let mut is_slice = false;
            if before_slice {
                if *self.token == token::DOTDOT {
                    self.bump();
                    is_slice = true;
                    before_slice = false;
                }
            }

            let subpat = self.parse_pat();
            if is_slice {
                match subpat {
                    @ast::Pat { node: PatWild, _ } => (),
                    @ast::Pat { node: PatIdent(_, _, _), _ } => (),
                    @ast::Pat { span, _ } => self.span_fatal(
                        span, "expected an identifier or `_`"
                    )
                }
                slice = Some(subpat);
            } else {
                if before_slice {
                    before.push(subpat);
                } else {
                    after.push(subpat);
                }
            }
        }

        (before, slice, after)
    }

    // parse the fields of a struct-like pattern
    fn parse_pat_fields(&self) -> (~[ast::FieldPat], bool) {
        let mut fields = ~[];
        let mut etc = false;
        let mut first = true;
        while *self.token != token::RBRACE {
            if first { first = false; }
            else { self.expect(&token::COMMA); }

            if *self.token == token::UNDERSCORE {
                self.bump();
                if *self.token != token::RBRACE {
                    self.fatal(
                        fmt!(
                            "expected `}`, found `%s`",
                            self.this_token_to_str()
                        )
                    );
                }
                etc = true;
                break;
            }

            let lo1 = self.last_span.lo;
            let fieldname = self.parse_ident();
            let hi1 = self.last_span.lo;
            let fieldpath = ast_util::ident_to_path(mk_sp(lo1, hi1),
                                                    fieldname);
            let subpat;
            if *self.token == token::COLON {
                self.bump();
                subpat = self.parse_pat();
            } else {
                subpat = @ast::Pat {
                    id: ast::DUMMY_NODE_ID,
                    node: PatIdent(BindInfer, fieldpath, None),
                    span: *self.last_span
                };
            }
            fields.push(ast::FieldPat { ident: fieldname, pat: subpat });
        }
        return (fields, etc);
    }

    // parse a pattern.
    pub fn parse_pat(&self) -> @Pat {
        maybe_whole!(self, nt_pat);

        let lo = self.span.lo;
        let mut hi;
        let pat;
        match *self.token {
            // parse _
          token::UNDERSCORE => {
            self.bump();
            pat = PatWild;
            hi = self.last_span.hi;
            return @ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            }
          }
          // parse @pat
          token::AT => {
            self.bump();
            let sub = self.parse_pat();
            hi = sub.span.hi;
            // HACK: parse @"..." as a literal of a vstore @str
            pat = match sub.node {
              PatLit(e@@Expr {
                node: ExprLit(@codemap::Spanned {
                    node: lit_str(_),
                    span: _}), _
              }) => {
                let vst = @Expr {
                    id: ast::DUMMY_NODE_ID,
                    node: ExprVstore(e, ExprVstoreBox),
                    span: mk_sp(lo, hi),
                };
                PatLit(vst)
              }
              _ => PatBox(sub)
            };
            hi = self.last_span.hi;
            return @ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            }
          }
          token::TILDE => {
            // parse ~pat
            self.bump();
            let sub = self.parse_pat();
            hi = sub.span.hi;
            // HACK: parse ~"..." as a literal of a vstore ~str
            pat = match sub.node {
              PatLit(e@@Expr {
                node: ExprLit(@codemap::Spanned {
                    node: lit_str(_),
                    span: _}), _
              }) => {
                let vst = @Expr {
                    id: ast::DUMMY_NODE_ID,
                    node: ExprVstore(e, ExprVstoreUniq),
                    span: mk_sp(lo, hi),
                };
                PatLit(vst)
              }
              _ => PatUniq(sub)
            };
            hi = self.last_span.hi;
            return @ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            }
          }
          token::BINOP(token::AND) => {
              // parse &pat
              let lo = self.span.lo;
              self.bump();
              let sub = self.parse_pat();
              hi = sub.span.hi;
              // HACK: parse &"..." as a literal of a borrowed str
              pat = match sub.node {
                  PatLit(e@@Expr {
                      node: ExprLit(@codemap::Spanned {
                            node: lit_str(_), span: _}), _
                  }) => {
                      let vst = @Expr {
                          id: ast::DUMMY_NODE_ID,
                          node: ExprVstore(e, ExprVstoreSlice),
                          span: mk_sp(lo, hi)
                      };
                      PatLit(vst)
                  }
              _ => PatRegion(sub)
            };
            hi = self.last_span.hi;
            return @ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            }
          }
          token::LBRACE => {
            self.bump();
            let (_, _) = self.parse_pat_fields();
            self.bump();
            self.obsolete(*self.span, ObsoleteRecordPattern);
            pat = PatWild;
            hi = self.last_span.hi;
            return @ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            }
          }
          token::LPAREN => {
            // parse (pat,pat,pat,...) as tuple
            self.bump();
            if *self.token == token::RPAREN {
                hi = self.span.hi;
                self.bump();
                let lit = @codemap::Spanned {
                    node: lit_nil,
                    span: mk_sp(lo, hi)};
                let expr = self.mk_expr(lo, hi, ExprLit(lit));
                pat = PatLit(expr);
            } else {
                let mut fields = ~[self.parse_pat()];
                if self.look_ahead(1, |t| *t != token::RPAREN) {
                    while *self.token == token::COMMA {
                        self.bump();
                        fields.push(self.parse_pat());
                    }
                }
                if fields.len() == 1 { self.expect(&token::COMMA); }
                self.expect(&token::RPAREN);
                pat = PatTup(fields);
            }
            hi = self.last_span.hi;
            return @ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            }
          }
          token::LBRACKET => {
            // parse [pat,pat,...] as vector pattern
            self.bump();
            let (before, slice, after) =
                self.parse_pat_vec_elements();

            self.expect(&token::RBRACKET);
            pat = ast::PatVec(before, slice, after);
            hi = self.last_span.hi;
            return @ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            }
          }
          _ => {}
        }

        let tok = self.token;
        if !is_ident_or_path(tok)
                || self.is_keyword(keywords::True)
                || self.is_keyword(keywords::False) {
            // Parse an expression pattern or exp .. exp.
            //
            // These expressions are limited to literals (possibly
            // preceded by unary-minus) or identifiers.
            let val = self.parse_literal_maybe_minus();
            if self.eat(&token::DOTDOT) {
                let end = if is_ident_or_path(tok) {
                    let path = self.parse_path(LifetimeAndTypesWithColons)
                                   .path;
                    let hi = self.span.hi;
                    self.mk_expr(lo, hi, ExprPath(path))
                } else {
                    self.parse_literal_maybe_minus()
                };
                pat = PatRange(val, end);
            } else {
                pat = PatLit(val);
            }
        } else if self.eat_keyword(keywords::Ref) {
            // parse ref pat
            let mutbl = self.parse_mutability();
            pat = self.parse_pat_ident(BindByRef(mutbl));
        } else {
            let can_be_enum_or_struct = do self.look_ahead(1) |t| {
                match *t {
                    token::LPAREN | token::LBRACKET | token::LT |
                    token::LBRACE | token::MOD_SEP => true,
                    _ => false,
                }
            };

            if self.look_ahead(1, |t| *t == token::DOTDOT) {
                let start = self.parse_expr_res(RESTRICT_NO_BAR_OP);
                self.eat(&token::DOTDOT);
                let end = self.parse_expr_res(RESTRICT_NO_BAR_OP);
                pat = PatRange(start, end);
            } else if is_plain_ident(&*self.token) && !can_be_enum_or_struct {
                let name = self.parse_path(NoTypesAllowed).path;
                let sub;
                if self.eat(&token::AT) {
                    // parse foo @ pat
                    sub = Some(self.parse_pat());
                } else {
                    // or just foo
                    sub = None;
                }
                pat = PatIdent(BindInfer, name, sub);
            } else {
                // parse an enum pat
                let enum_path = self.parse_path(LifetimeAndTypesWithColons)
                                    .path;
                match *self.token {
                    token::LBRACE => {
                        self.bump();
                        let (fields, etc) =
                            self.parse_pat_fields();
                        self.bump();
                        pat = PatStruct(enum_path, fields, etc);
                    }
                    _ => {
                        let mut args: ~[@Pat] = ~[];
                        match *self.token {
                          token::LPAREN => {
                            let is_star = do self.look_ahead(1) |t| {
                                match *t {
                                    token::BINOP(token::STAR) => true,
                                    _ => false,
                                }
                            };
                            if is_star {
                                // This is a "top constructor only" pat
                                self.bump();
                                self.bump();
                                self.expect(&token::RPAREN);
                                pat = PatEnum(enum_path, None);
                            } else {
                                args = self.parse_unspanned_seq(
                                    &token::LPAREN,
                                    &token::RPAREN,
                                    seq_sep_trailing_disallowed(token::COMMA),
                                    |p| p.parse_pat()
                                );
                                pat = PatEnum(enum_path, Some(args));
                            }
                          },
                          _ => {
                              if enum_path.segments.len() == 1 {
                                  // it could still be either an enum
                                  // or an identifier pattern, resolve
                                  // will sort it out:
                                  pat = PatIdent(BindInfer,
                                                  enum_path,
                                                  None);
                              } else {
                                  pat = PatEnum(enum_path, Some(args));
                              }
                          }
                        }
                    }
                }
            }
        }
        hi = self.last_span.hi;
        @ast::Pat {
            id: ast::DUMMY_NODE_ID,
            node: pat,
            span: mk_sp(lo, hi),
        }
    }

    // parse ident or ident @ pat
    // used by the copy foo and ref foo patterns to give a good
    // error message when parsing mistakes like ref foo(a,b)
    fn parse_pat_ident(&self,
                       binding_mode: ast::BindingMode)
                       -> ast::Pat_ {
        if !is_plain_ident(&*self.token) {
            self.span_fatal(*self.last_span,
                            "expected identifier, found path");
        }
        // why a path here, and not just an identifier?
        let name = self.parse_path(NoTypesAllowed).path;
        let sub = if self.eat(&token::AT) {
            Some(self.parse_pat())
        } else {
            None
        };

        // just to be friendly, if they write something like
        //   ref Some(i)
        // we end up here with ( as the current token.  This shortly
        // leads to a parse error.  Note that if there is no explicit
        // binding mode then we do not end up here, because the lookahead
        // will direct us over to parse_enum_variant()
        if *self.token == token::LPAREN {
            self.span_fatal(
                *self.last_span,
                "expected identifier, found enum pattern");
        }

        PatIdent(binding_mode, name, sub)
    }

    // parse a local variable declaration
    fn parse_local(&self, is_mutbl: bool) -> @Local {
        let lo = self.span.lo;
        let pat = self.parse_pat();

        if is_mutbl && !ast_util::pat_is_ident(pat) {
            self.obsolete(*self.span, ObsoleteMutWithMultipleBindings)
        }

        let mut ty = Ty {
            id: ast::DUMMY_NODE_ID,
            node: ty_infer,
            span: mk_sp(lo, lo),
        };
        if self.eat(&token::COLON) { ty = self.parse_ty(false); }
        let init = self.parse_initializer();
        @ast::Local {
            is_mutbl: is_mutbl,
            ty: ty,
            pat: pat,
            init: init,
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, self.last_span.hi),
        }
    }

    // parse a "let" stmt
    fn parse_let(&self) -> @Decl {
        let is_mutbl = self.eat_keyword(keywords::Mut);
        let lo = self.span.lo;
        let local = self.parse_local(is_mutbl);
        while self.eat(&token::COMMA) {
            let _ = self.parse_local(is_mutbl);
            self.obsolete(*self.span, ObsoleteMultipleLocalDecl);
        }
        return @spanned(lo, self.last_span.hi, DeclLocal(local));
    }

    // parse a structure field
    fn parse_name_and_ty(&self,
                         pr: visibility,
                         attrs: ~[Attribute]) -> @struct_field {
        let lo = self.span.lo;
        if !is_plain_ident(&*self.token) {
            self.fatal("expected ident");
        }
        let name = self.parse_ident();
        self.expect(&token::COLON);
        let ty = self.parse_ty(false);
        @spanned(lo, self.last_span.hi, ast::struct_field_ {
            kind: named_field(name, pr),
            id: ast::DUMMY_NODE_ID,
            ty: ty,
            attrs: attrs,
        })
    }

    // parse a statement. may include decl.
    // precondition: any attributes are parsed already
    pub fn parse_stmt(&self, item_attrs: ~[Attribute]) -> @Stmt {
        maybe_whole!(self, nt_stmt);

        fn check_expected_item(p: &Parser, found_attrs: bool) {
            // If we have attributes then we should have an item
            if found_attrs {
                p.span_err(*p.last_span, "expected item after attributes");
            }
        }

        let lo = self.span.lo;
        if self.is_keyword(keywords::Let) {
            check_expected_item(self, !item_attrs.is_empty());
            self.expect_keyword(keywords::Let);
            let decl = self.parse_let();
            return @spanned(lo, decl.span.hi, StmtDecl(decl, ast::DUMMY_NODE_ID));
        } else if is_ident(&*self.token)
            && !token::is_any_keyword(self.token)
            && self.look_ahead(1, |t| *t == token::NOT) {
            // parse a macro invocation. Looks like there's serious
            // overlap here; if this clause doesn't catch it (and it
            // won't, for brace-delimited macros) it will fall through
            // to the macro clause of parse_item_or_view_item. This
            // could use some cleanup, it appears to me.

            // whoops! I now have a guess: I'm guessing the "parens-only"
            // rule here is deliberate, to allow macro users to use parens
            // for things that should be parsed as stmt_mac, and braces
            // for things that should expand into items. Tricky, and
            // somewhat awkward... and probably undocumented. Of course,
            // I could just be wrong.

            check_expected_item(self, !item_attrs.is_empty());

            // Potential trouble: if we allow macros with paths instead of
            // idents, we'd need to look ahead past the whole path here...
            let pth = self.parse_path(NoTypesAllowed).path;
            self.bump();

            let id = if *self.token == token::LPAREN {
                token::special_idents::invalid // no special identifier
            } else {
                self.parse_ident()
            };

            let tts = self.parse_unspanned_seq(
                &token::LPAREN,
                &token::RPAREN,
                seq_sep_none(),
                |p| p.parse_token_tree()
            );
            let hi = self.span.hi;

            if id == token::special_idents::invalid {
                return @spanned(lo, hi, StmtMac(
                    spanned(lo, hi, mac_invoc_tt(pth, tts, EMPTY_CTXT)), false));
            } else {
                // if it has a special ident, it's definitely an item
                return @spanned(lo, hi, StmtDecl(
                    @spanned(lo, hi, DeclItem(
                        self.mk_item(
                            lo, hi, id /*id is good here*/,
                            item_mac(spanned(lo, hi, mac_invoc_tt(pth, tts, EMPTY_CTXT))),
                            inherited, ~[/*no attrs*/]))),
                    ast::DUMMY_NODE_ID));
            }

        } else {
            let found_attrs = !item_attrs.is_empty();
            match self.parse_item_or_view_item(item_attrs, false) {
                iovi_item(i) => {
                    let hi = i.span.hi;
                    let decl = @spanned(lo, hi, DeclItem(i));
                    return @spanned(lo, hi, StmtDecl(decl, ast::DUMMY_NODE_ID));
                }
                iovi_view_item(vi) => {
                    self.span_fatal(vi.span,
                                    "view items must be declared at the top of the block");
                }
                iovi_foreign_item(_) => {
                    self.fatal("foreign items are not allowed here");
                }
                iovi_none(_) => { /* fallthrough */ }
            }

            check_expected_item(self, found_attrs);

            // Remainder are line-expr stmts.
            let e = self.parse_expr_res(RESTRICT_STMT_EXPR);
            return @spanned(lo, e.span.hi, StmtExpr(e, ast::DUMMY_NODE_ID));
        }
    }

    // is this expression a successfully-parsed statement?
    fn expr_is_complete(&self, e: @Expr) -> bool {
        return *self.restriction == RESTRICT_STMT_EXPR &&
            !classify::expr_requires_semi_to_be_stmt(e);
    }

    // parse a block. No inner attrs are allowed.
    pub fn parse_block(&self) -> Block {
        maybe_whole!(deref self, nt_block);

        let lo = self.span.lo;
        if self.eat_keyword(keywords::Unsafe) {
            self.obsolete(*self.span, ObsoleteUnsafeBlock);
        }
        self.expect(&token::LBRACE);

        return self.parse_block_tail_(lo, DefaultBlock, ~[]);
    }

    // parse a block. Inner attrs are allowed.
    fn parse_inner_attrs_and_block(&self)
        -> (~[Attribute], Block) {

        maybe_whole!(pair_empty self, nt_block);

        let lo = self.span.lo;
        if self.eat_keyword(keywords::Unsafe) {
            self.obsolete(*self.span, ObsoleteUnsafeBlock);
        }
        self.expect(&token::LBRACE);
        let (inner, next) = self.parse_inner_attrs_and_next();

        (inner, self.parse_block_tail_(lo, DefaultBlock, next))
    }

    // Precondition: already parsed the '{' or '#{'
    // I guess that also means "already parsed the 'impure'" if
    // necessary, and this should take a qualifier.
    // some blocks start with "#{"...
    fn parse_block_tail(&self, lo: BytePos, s: BlockCheckMode) -> Block {
        self.parse_block_tail_(lo, s, ~[])
    }

    // parse the rest of a block expression or function body
    fn parse_block_tail_(&self, lo: BytePos, s: BlockCheckMode,
                         first_item_attrs: ~[Attribute]) -> Block {
        let mut stmts = ~[];
        let mut expr = None;

        // wouldn't it be more uniform to parse view items only, here?
        let ParsedItemsAndViewItems {
            attrs_remaining: attrs_remaining,
            view_items: view_items,
            items: items,
            _
        } = self.parse_items_and_view_items(first_item_attrs,
                                            false, false);

        for item in items.iter() {
            let decl = @spanned(item.span.lo, item.span.hi, DeclItem(*item));
            stmts.push(@spanned(item.span.lo, item.span.hi,
                                StmtDecl(decl, ast::DUMMY_NODE_ID)));
        }

        let mut attributes_box = attrs_remaining;

        while (*self.token != token::RBRACE) {
            // parsing items even when they're not allowed lets us give
            // better error messages and recover more gracefully.
            attributes_box.push_all(self.parse_outer_attributes());
            match *self.token {
                token::SEMI => {
                    if !attributes_box.is_empty() {
                        self.span_err(*self.last_span, "expected item after attributes");
                        attributes_box = ~[];
                    }
                    self.bump(); // empty
                }
                token::RBRACE => {
                    // fall through and out.
                }
                _ => {
                    let stmt = self.parse_stmt(attributes_box);
                    attributes_box = ~[];
                    match stmt.node {
                        StmtExpr(e, stmt_id) => {
                            // expression without semicolon
                            if classify::stmt_ends_with_semi(stmt) {
                                // Just check for errors and recover; do not eat semicolon yet.
                                self.commit_stmt(stmt, &[], &[token::SEMI, token::RBRACE]);
                            }

                            match *self.token {
                                token::SEMI => {
                                    self.bump();
                                    stmts.push(@codemap::Spanned {
                                        node: StmtSemi(e, stmt_id),
                                        span: stmt.span,
                                    });
                                }
                                token::RBRACE => {
                                    expr = Some(e);
                                }
                                _ => {
                                    stmts.push(stmt);
                                }
                            }
                        }
                        StmtMac(ref m, _) => {
                            // statement macro; might be an expr
                            let has_semi;
                            match *self.token {
                                token::SEMI => {
                                    has_semi = true;
                                }
                                token::RBRACE => {
                                    // if a block ends in `m!(arg)` without
                                    // a `;`, it must be an expr
                                    has_semi = false;
                                    expr = Some(
                                        self.mk_mac_expr(stmt.span.lo,
                                                         stmt.span.hi,
                                                         m.node.clone()));
                                }
                                _ => {
                                    has_semi = false;
                                    stmts.push(stmt);
                                }
                            }

                            if has_semi {
                                self.bump();
                                stmts.push(@codemap::Spanned {
                                    node: StmtMac((*m).clone(), true),
                                    span: stmt.span,
                                });
                            }
                        }
                        _ => { // all other kinds of statements:
                            stmts.push(stmt);

                            if classify::stmt_ends_with_semi(stmt) {
                                self.commit_stmt_expecting(stmt, token::SEMI);
                            }
                        }
                    }
                }
            }
        }

        if !attributes_box.is_empty() {
            self.span_err(*self.last_span, "expected item after attributes");
        }

        let hi = self.span.hi;
        self.bump();
        ast::Block {
            view_items: view_items,
            stmts: stmts,
            expr: expr,
            id: ast::DUMMY_NODE_ID,
            rules: s,
            span: mk_sp(lo, hi),
        }
    }

    fn parse_optional_purity(&self) -> ast::purity {
        if self.eat_keyword(keywords::Pure) {
            self.obsolete(*self.last_span, ObsoletePurity);
            ast::impure_fn
        } else if self.eat_keyword(keywords::Unsafe) {
            ast::unsafe_fn
        } else {
            ast::impure_fn
        }
    }

    fn parse_optional_onceness(&self) -> ast::Onceness {
        if self.eat_keyword(keywords::Once) { ast::Once } else { ast::Many }
    }

    // matches optbounds = ( ( : ( boundseq )? )? )
    // where   boundseq  = ( bound + boundseq ) | bound
    // and     bound     = 'static | ty
    // Returns "None" if there's no colon (e.g. "T");
    // Returns "Some(Empty)" if there's a colon but nothing after (e.g. "T:")
    // Returns "Some(stuff)" otherwise (e.g. "T:stuff").
    // NB: The None/Some distinction is important for issue #7264.
    fn parse_optional_ty_param_bounds(&self) -> Option<OptVec<TyParamBound>> {
        if !self.eat(&token::COLON) {
            return None;
        }

        let mut result = opt_vec::Empty;
        loop {
            match *self.token {
                token::LIFETIME(lifetime) => {
                    if "static" == self.id_to_str(lifetime) {
                        result.push(RegionTyParamBound);
                    } else {
                        self.span_err(*self.span,
                                      "`'static` is the only permissible region bound here");
                    }
                    self.bump();
                }
                token::MOD_SEP | token::IDENT(*) => {
                    let tref = self.parse_trait_ref();
                    result.push(TraitTyParamBound(tref));
                }
                _ => break,
            }

            if !self.eat(&token::BINOP(token::PLUS)) {
                break;
            }
        }

        return Some(result);
    }

    // matches typaram = IDENT optbounds
    fn parse_ty_param(&self) -> TyParam {
        let ident = self.parse_ident();
        let opt_bounds = self.parse_optional_ty_param_bounds();
        // For typarams we don't care about the difference b/w "<T>" and "<T:>".
        let bounds = opt_bounds.unwrap_or_default();
        ast::TyParam { ident: ident, id: ast::DUMMY_NODE_ID, bounds: bounds }
    }

    // parse a set of optional generic type parameter declarations
    // matches generics = ( ) | ( < > ) | ( < typaramseq ( , )? > ) | ( < lifetimes ( , )? > )
    //                  | ( < lifetimes , typaramseq ( , )? > )
    // where   typaramseq = ( typaram ) | ( typaram , typaramseq )
    pub fn parse_generics(&self) -> ast::Generics {
        if self.eat(&token::LT) {
            let lifetimes = self.parse_lifetimes();
            let ty_params = self.parse_seq_to_gt(
                Some(token::COMMA),
                |p| p.parse_ty_param());
            ast::Generics { lifetimes: lifetimes, ty_params: ty_params }
        } else {
            ast_util::empty_generics()
        }
    }

    // parse a generic use site
    fn parse_generic_values(&self) -> (OptVec<ast::Lifetime>, ~[Ty]) {
        if !self.eat(&token::LT) {
            (opt_vec::Empty, ~[])
        } else {
            self.parse_generic_values_after_lt()
        }
    }

    fn parse_generic_values_after_lt(&self) -> (OptVec<ast::Lifetime>, ~[Ty]) {
        let lifetimes = self.parse_lifetimes();
        let result = self.parse_seq_to_gt(
            Some(token::COMMA),
            |p| p.parse_ty(false));
        (lifetimes, opt_vec::take_vec(result))
    }

    // parse the argument list and result type of a function declaration
    pub fn parse_fn_decl(&self) -> fn_decl {
        let args: ~[arg] =
            self.parse_unspanned_seq(
                &token::LPAREN,
                &token::RPAREN,
                seq_sep_trailing_disallowed(token::COMMA),
                |p| p.parse_arg()
            );

        let (ret_style, ret_ty) = self.parse_ret_ty();
        ast::fn_decl {
            inputs: args,
            output: ret_ty,
            cf: ret_style,
        }
    }

    fn is_self_ident(&self) -> bool {
        match *self.token {
          token::IDENT(id, false) => id.name == special_idents::self_.name,
          _ => false
        }
    }

    fn expect_self_ident(&self) {
        if !self.is_self_ident() {
            self.fatal(
                fmt!(
                    "expected `self` but found `%s`",
                    self.this_token_to_str()
                )
            );
        }
        self.bump();
    }

    // parse the argument list and result type of a function
    // that may have a self type.
    fn parse_fn_decl_with_self(
        &self,
        parse_arg_fn:
        &fn(&Parser) -> arg
    ) -> (explicit_self, fn_decl) {
        fn maybe_parse_explicit_self(
            cnstr: &fn(v: Mutability) -> ast::explicit_self_,
            p: &Parser
        ) -> ast::explicit_self_ {
            // We need to make sure it isn't a mode or a type
            if p.look_ahead(1, |t| token::is_keyword(keywords::Self, t)) ||
                ((p.look_ahead(1, |t| token::is_keyword(keywords::Const, t)) ||
                  p.look_ahead(1, |t| token::is_keyword(keywords::Mut, t))) &&
                 p.look_ahead(2, |t| token::is_keyword(keywords::Self, t))) {

                p.bump();
                let mutability = p.parse_mutability();
                p.expect_self_ident();
                cnstr(mutability)
            } else {
                sty_static
            }
        }

        fn maybe_parse_borrowed_explicit_self(this: &Parser) -> ast::explicit_self_ {
            // The following things are possible to see here:
            //
            //     fn(&self)
            //     fn(&mut self)
            //     fn(&'lt self)
            //     fn(&'lt mut self)
            //
            // We already know that the current token is `&`.

            if this.look_ahead(1, |t| token::is_keyword(keywords::Self, t)) {
                this.bump();
                this.expect_self_ident();
                sty_region(None, MutImmutable)
            } else if this.look_ahead(1, |t| this.token_is_mutability(t)) &&
                    this.look_ahead(2,
                                    |t| token::is_keyword(keywords::Self,
                                                          t)) {
                this.bump();
                let mutability = this.parse_mutability();
                this.expect_self_ident();
                sty_region(None, mutability)
            } else if this.look_ahead(1, |t| this.token_is_lifetime(t)) &&
                       this.look_ahead(2,
                                       |t| token::is_keyword(keywords::Self,
                                                             t)) {
                this.bump();
                let lifetime = this.parse_lifetime();
                this.expect_self_ident();
                sty_region(Some(lifetime), MutImmutable)
            } else if this.look_ahead(1, |t| this.token_is_lifetime(t)) &&
                      this.look_ahead(2, |t| this.token_is_mutability(t)) &&
                      this.look_ahead(3, |t| token::is_keyword(keywords::Self,
                                                               t)) {
                this.bump();
                let lifetime = this.parse_lifetime();
                let mutability = this.parse_mutability();
                this.expect_self_ident();
                sty_region(Some(lifetime), mutability)
            } else {
                sty_static
            }
        }

        self.expect(&token::LPAREN);

        // A bit of complexity and lookahead is needed here in order to be
        // backwards compatible.
        let lo = self.span.lo;
        let explicit_self = match *self.token {
          token::BINOP(token::AND) => {
            maybe_parse_borrowed_explicit_self(self)
          }
          token::AT => {
            maybe_parse_explicit_self(sty_box, self)
          }
          token::TILDE => {
            maybe_parse_explicit_self(|mutability| {
                if mutability != MutImmutable {
                    self.obsolete(*self.last_span, ObsoleteMutOwnedPointer);
                }
                sty_uniq
            }, self)
          }
          token::IDENT(*) if self.is_self_ident() => {
            self.bump();
            sty_value
          }
          token::BINOP(token::STAR) => {
            // Possibly "*self" or "*mut self" -- not supported. Try to avoid
            // emitting cryptic "unexpected token" errors.
            self.bump();
            if self.token_is_mutability(self.token) {
                self.bump();
            }
            if self.is_self_ident() {
                self.span_err(*self.span, "cannot pass self by unsafe pointer");
                self.bump();
            }
            sty_value
          }
          _ => {
            sty_static
          }
        };

        // If we parsed a self type, expect a comma before the argument list.
        let fn_inputs;
        if explicit_self != sty_static {
            match *self.token {
                token::COMMA => {
                    self.bump();
                    let sep = seq_sep_trailing_disallowed(token::COMMA);
                    fn_inputs = self.parse_seq_to_before_end(
                        &token::RPAREN,
                        sep,
                        parse_arg_fn
                    );
                }
                token::RPAREN => {
                    fn_inputs = ~[];
                }
                _ => {
                    self.fatal(
                        fmt!(
                            "expected `,` or `)`, found `%s`",
                            self.this_token_to_str()
                        )
                    );
                }
            }
        } else {
            let sep = seq_sep_trailing_disallowed(token::COMMA);
            fn_inputs = self.parse_seq_to_before_end(
                &token::RPAREN,
                sep,
                parse_arg_fn
            );
        }

        self.expect(&token::RPAREN);

        let hi = self.span.hi;

        let (ret_style, ret_ty) = self.parse_ret_ty();

        let fn_decl = ast::fn_decl {
            inputs: fn_inputs,
            output: ret_ty,
            cf: ret_style
        };

        (spanned(lo, hi, explicit_self), fn_decl)
    }

    // parse the |arg, arg| header on a lambda
    fn parse_fn_block_decl(&self) -> fn_decl {
        let inputs_captures = {
            if self.eat(&token::OROR) {
                ~[]
            } else {
                self.parse_unspanned_seq(
                    &token::BINOP(token::OR),
                    &token::BINOP(token::OR),
                    seq_sep_trailing_disallowed(token::COMMA),
                    |p| p.parse_fn_block_arg()
                )
            }
        };
        let output = if self.eat(&token::RARROW) {
            self.parse_ty(false)
        } else {
            Ty { id: ast::DUMMY_NODE_ID, node: ty_infer, span: *self.span }
        };

        ast::fn_decl {
            inputs: inputs_captures,
            output: output,
            cf: return_val,
        }
    }

    // parse the name and optional generic types of a function header.
    fn parse_fn_header(&self) -> (Ident, ast::Generics) {
        let id = self.parse_ident();
        let generics = self.parse_generics();
        (id, generics)
    }

    fn mk_item(&self, lo: BytePos, hi: BytePos, ident: Ident,
               node: item_, vis: visibility,
               attrs: ~[Attribute]) -> @item {
        @ast::item { ident: ident,
                     attrs: attrs,
                     id: ast::DUMMY_NODE_ID,
                     node: node,
                     vis: vis,
                     span: mk_sp(lo, hi) }
    }

    // parse an item-position function declaration.
    fn parse_item_fn(&self, purity: purity, abis: AbiSet) -> item_info {
        let (ident, generics) = self.parse_fn_header();
        let decl = self.parse_fn_decl();
        let (inner_attrs, body) = self.parse_inner_attrs_and_block();
        (ident,
         item_fn(decl, purity, abis, generics, body),
         Some(inner_attrs))
    }

    // parse a method in a trait impl
    fn parse_method(&self) -> @method {
        let attrs = self.parse_outer_attributes();
        let lo = self.span.lo;

        let visa = self.parse_non_priv_visibility();
        let pur = self.parse_fn_purity();
        let ident = self.parse_ident();
        let generics = self.parse_generics();
        let (explicit_self, decl) = do self.parse_fn_decl_with_self() |p| {
            p.parse_arg()
        };

        let (inner_attrs, body) = self.parse_inner_attrs_and_block();
        let hi = body.span.hi;
        let attrs = vec::append(attrs, inner_attrs);
        @ast::method {
            ident: ident,
            attrs: attrs,
            generics: generics,
            explicit_self: explicit_self,
            purity: pur,
            decl: decl,
            body: body,
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, hi),
            self_id: ast::DUMMY_NODE_ID,
            vis: visa,
        }
    }

    // parse trait Foo { ... }
    fn parse_item_trait(&self) -> item_info {
        let ident = self.parse_ident();
        self.parse_region_param();
        let tps = self.parse_generics();

        // Parse traits, if necessary.
        let traits;
        if *self.token == token::COLON {
            self.bump();
            traits = self.parse_trait_ref_list(&token::LBRACE);
        } else {
            traits = ~[];
        }

        let meths = self.parse_trait_methods();
        (ident, item_trait(tps, traits, meths), None)
    }

    // Parses two variants (with the region/type params always optional):
    //    impl<T> Foo { ... }
    //    impl<T> ToStr for ~[T] { ... }
    fn parse_item_impl(&self, visibility: ast::visibility) -> item_info {
        // First, parse type parameters if necessary.
        let generics = self.parse_generics();

        // This is a new-style impl declaration.
        // XXX: clownshoes
        let ident = special_idents::clownshoes_extensions;

        // Special case: if the next identifier that follows is '(', don't
        // allow this to be parsed as a trait.
        let could_be_trait = *self.token != token::LPAREN;

        // Parse the trait.
        let mut ty = self.parse_ty(false);

        // Parse traits, if necessary.
        let opt_trait = if could_be_trait && self.eat_keyword(keywords::For) {
            // New-style trait. Reinterpret the type as a trait.
            let opt_trait_ref = match ty.node {
                ty_path(ref path, None, node_id) => {
                    Some(trait_ref {
                        path: /* bad */ (*path).clone(),
                        ref_id: node_id
                    })
                }
                ty_path(*) => {
                    self.span_err(ty.span,
                                  "bounded traits are only valid in type position");
                    None
                }
                _ => {
                    self.span_err(ty.span, "not a trait");
                    None
                }
            };

            ty = self.parse_ty(false);
            opt_trait_ref
        } else if self.eat(&token::COLON) {
            self.obsolete(*self.span, ObsoleteImplSyntax);
            Some(self.parse_trait_ref())
        } else {
            None
        };

        // Do not allow visibility to be specified.
        if visibility != ast::inherited {
            self.obsolete(*self.span, ObsoleteImplVisibility);
        }

        let mut meths = ~[];
        if !self.eat(&token::SEMI) {
            self.expect(&token::LBRACE);
            while !self.eat(&token::RBRACE) {
                meths.push(self.parse_method());
            }
        }

        (ident, item_impl(generics, opt_trait, ty, meths), None)
    }

    // parse a::B<~str,int>
    fn parse_trait_ref(&self) -> trait_ref {
        ast::trait_ref {
            path: self.parse_path(LifetimeAndTypesWithoutColons).path,
            ref_id: ast::DUMMY_NODE_ID,
        }
    }

    // parse B + C<~str,int> + D
    fn parse_trait_ref_list(&self, ket: &token::Token) -> ~[trait_ref] {
        self.parse_seq_to_before_end(
            ket,
            seq_sep_trailing_disallowed(token::BINOP(token::PLUS)),
            |p| p.parse_trait_ref()
        )
    }

    // parse struct Foo { ... }
    fn parse_item_struct(&self) -> item_info {
        let class_name = self.parse_ident();
        self.parse_region_param();
        let generics = self.parse_generics();
        if self.eat(&token::COLON) {
            self.obsolete(*self.span, ObsoleteClassTraits);
            let _ = self.parse_trait_ref_list(&token::LBRACE);
        }

        let mut fields: ~[@struct_field];
        let is_tuple_like;

        if self.eat(&token::LBRACE) {
            // It's a record-like struct.
            is_tuple_like = false;
            fields = ~[];
            while *self.token != token::RBRACE {
                let r = self.parse_struct_decl_field();
                for struct_field in r.iter() {
                    fields.push(*struct_field)
                }
            }
            if fields.len() == 0 {
                self.fatal(fmt!("Unit-like struct definition should be written as `struct %s;`",
                                get_ident_interner().get(class_name.name)));
            }
            self.bump();
        } else if *self.token == token::LPAREN {
            // It's a tuple-like struct.
            is_tuple_like = true;
            fields = do self.parse_unspanned_seq(
                &token::LPAREN,
                &token::RPAREN,
                seq_sep_trailing_allowed(token::COMMA)
            ) |p| {
                let attrs = self.parse_outer_attributes();
                let lo = p.span.lo;
                let struct_field_ = ast::struct_field_ {
                    kind: unnamed_field,
                    id: ast::DUMMY_NODE_ID,
                    ty: p.parse_ty(false),
                    attrs: attrs,
                };
                @spanned(lo, p.span.hi, struct_field_)
            };
            self.expect(&token::SEMI);
        } else if self.eat(&token::SEMI) {
            // It's a unit-like struct.
            is_tuple_like = true;
            fields = ~[];
        } else {
            self.fatal(
                fmt!(
                    "expected `{`, `(`, or `;` after struct name \
                    but found `%s`",
                    self.this_token_to_str()
                )
            );
        }

        let _ = ast::DUMMY_NODE_ID;  // XXX: Workaround for crazy bug.
        let new_id = ast::DUMMY_NODE_ID;
        (class_name,
         item_struct(@ast::struct_def {
             fields: fields,
             ctor_id: if is_tuple_like { Some(new_id) } else { None }
         }, generics),
         None)
    }

    fn token_is_pound_or_doc_comment(&self, tok: token::Token) -> bool {
        match tok {
            token::POUND | token::DOC_COMMENT(_) => true,
            _ => false
        }
    }

    // parse a structure field declaration
    pub fn parse_single_struct_field(&self,
                                     vis: visibility,
                                     attrs: ~[Attribute])
                                     -> @struct_field {
        if self.eat_obsolete_ident("let") {
            self.obsolete(*self.last_span, ObsoleteLet);
        }

        let a_var = self.parse_name_and_ty(vis, attrs);
        match *self.token {
            token::SEMI => {
                self.obsolete(*self.span, ObsoleteFieldTerminator);
                self.bump();
            }
            token::COMMA => {
                self.bump();
            }
            token::RBRACE => {}
            _ => {
                self.span_fatal(*self.span,
                                fmt!("expected `,`, or '}' but found `%s`",
                                     self.this_token_to_str()));
            }
        }
        a_var
    }

    // parse an element of a struct definition
    fn parse_struct_decl_field(&self) -> ~[@struct_field] {

        let attrs = self.parse_outer_attributes();

        if self.try_parse_obsolete_priv_section(attrs) {
            return ~[];
        }

        if self.eat_keyword(keywords::Priv) {
            return ~[self.parse_single_struct_field(private, attrs)]
        }

        if self.eat_keyword(keywords::Pub) {
           return ~[self.parse_single_struct_field(public, attrs)];
        }

        return ~[self.parse_single_struct_field(inherited, attrs)];
    }

    // parse visiility: PUB, PRIV, or nothing
    fn parse_visibility(&self) -> visibility {
        if self.eat_keyword(keywords::Pub) { public }
        else if self.eat_keyword(keywords::Priv) { private }
        else { inherited }
    }

    // parse visibility, but emits an obsolete error if it's private
    fn parse_non_priv_visibility(&self) -> visibility {
        match self.parse_visibility() {
            public => public,
            inherited => inherited,
            private => {
                self.obsolete(*self.last_span, ObsoletePrivVisibility);
                inherited
            }
        }
    }

    fn parse_staticness(&self) -> bool {
        if self.eat_keyword(keywords::Static) {
            self.obsolete(*self.last_span, ObsoleteStaticMethod);
            true
        } else {
            false
        }
    }

    // given a termination token and a vector of already-parsed
    // attributes (of length 0 or 1), parse all of the items in a module
    fn parse_mod_items(&self,
                       term: token::Token,
                       first_item_attrs: ~[Attribute])
                       -> _mod {
        // parse all of the items up to closing or an attribute.
        // view items are legal here.
        let ParsedItemsAndViewItems {
            attrs_remaining: attrs_remaining,
            view_items: view_items,
            items: starting_items,
            _
        } = self.parse_items_and_view_items(first_item_attrs, true, true);
        let mut items: ~[@item] = starting_items;
        let attrs_remaining_len = attrs_remaining.len();

        // don't think this other loop is even necessary....

        let mut first = true;
        while *self.token != term {
            let mut attrs = self.parse_outer_attributes();
            if first {
                attrs = attrs_remaining + attrs;
                first = false;
            }
            debug!("parse_mod_items: parse_item_or_view_item(attrs=%?)",
                   attrs);
            match self.parse_item_or_view_item(attrs,
                                               true /* macros allowed */) {
              iovi_item(item) => items.push(item),
              iovi_view_item(view_item) => {
                self.span_fatal(view_item.span,
                                "view items must be declared at the top of \
                                 the module");
              }
              _ => {
                self.fatal(fmt!("expected item but found `%s`",
                                self.this_token_to_str()));
              }
            }
        }

        if first && attrs_remaining_len > 0u {
            // We parsed attributes for the first item but didn't find it
            self.span_err(*self.last_span, "expected item after attributes");
        }

        ast::_mod { view_items: view_items, items: items }
    }

    fn parse_item_const(&self) -> item_info {
        let m = if self.eat_keyword(keywords::Mut) {MutMutable} else {MutImmutable};
        let id = self.parse_ident();
        self.expect(&token::COLON);
        let ty = self.parse_ty(false);
        self.expect(&token::EQ);
        let e = self.parse_expr();
        self.commit_expr_expecting(e, token::SEMI);
        (id, item_static(ty, m, e), None)
    }

    // parse a `mod <foo> { ... }` or `mod <foo>;` item
    fn parse_item_mod(&self, outer_attrs: &[Attribute]) -> item_info {
        let id_span = *self.span;
        let id = self.parse_ident();
        if *self.token == token::SEMI {
            self.bump();
            // This mod is in an external file. Let's go get it!
            let (m, attrs) = self.eval_src_mod(id, outer_attrs, id_span);
            (id, m, Some(attrs))
        } else {
            self.push_mod_path(id, outer_attrs);
            self.expect(&token::LBRACE);
            let (inner, next) = self.parse_inner_attrs_and_next();
            let m = self.parse_mod_items(token::RBRACE, next);
            self.expect(&token::RBRACE);
            self.pop_mod_path();
            (id, item_mod(m), Some(inner))
        }
    }

    fn push_mod_path(&self, id: Ident, attrs: &[Attribute]) {
        let default_path = token::interner_get(id.name);
        let file_path = match ::attr::first_attr_value_str_by_name(attrs,
                                                                   "path") {
            Some(d) => d,
            None => default_path
        };
        self.mod_path_stack.push(file_path)
    }

    fn pop_mod_path(&self) {
        self.mod_path_stack.pop();
    }

    // read a module from a source file.
    fn eval_src_mod(&self,
                    id: ast::Ident,
                    outer_attrs: &[ast::Attribute],
                    id_sp: Span)
                    -> (ast::item_, ~[ast::Attribute]) {
        let prefix = Path(self.sess.cm.span_to_filename(*self.span));
        let prefix = prefix.dir_path();
        let mod_path_stack = &*self.mod_path_stack;
        let mod_path = Path(".").push_many(*mod_path_stack);
        let dir_path = prefix.push_many(mod_path.components);
        let file_path = match ::attr::first_attr_value_str_by_name(
                outer_attrs, "path") {
            Some(d) => {
                let path = Path(d);
                if !path.is_absolute {
                    dir_path.push(d)
                } else {
                    path
                }
            }
            None => {
                let mod_name = token::interner_get(id.name).to_owned();
                let default_path_str = mod_name + ".rs";
                let secondary_path_str = mod_name + "/mod.rs";
                let default_path = dir_path.push(default_path_str);
                let secondary_path = dir_path.push(secondary_path_str);
                let default_exists = default_path.exists();
                let secondary_exists = secondary_path.exists();
                match (default_exists, secondary_exists) {
                    (true, false) => default_path,
                    (false, true) => secondary_path,
                    (false, false) => {
                        self.span_fatal(id_sp, fmt!("file not found for module `%s`", mod_name));
                    }
                    (true, true) => {
                        self.span_fatal(id_sp,
                                        fmt!("file for module `%s` found at both %s and %s",
                                             mod_name, default_path_str, secondary_path_str));
                    }
                }
            }
        };

        self.eval_src_mod_from_path(file_path,
                                    outer_attrs.to_owned(),
                                    id_sp)
    }

    fn eval_src_mod_from_path(&self,
                              path: Path,
                              outer_attrs: ~[ast::Attribute],
                              id_sp: Span) -> (ast::item_, ~[ast::Attribute]) {
        let full_path = path.normalize();

        let maybe_i = do self.sess.included_mod_stack.iter().position |p| { *p == full_path };
        match maybe_i {
            Some(i) => {
                let stack = &self.sess.included_mod_stack;
                let mut err = ~"circular modules: ";
                for p in stack.slice(i, stack.len()).iter() {
                    err.push_str(p.to_str());
                    err.push_str(" -> ");
                }
                err.push_str(full_path.to_str());
                self.span_fatal(id_sp, err);
            }
            None => ()
        }
        self.sess.included_mod_stack.push(full_path.clone());

        let p0 =
            new_sub_parser_from_file(self.sess,
                                     self.cfg.clone(),
                                     &full_path,
                                     id_sp);
        let (inner, next) = p0.parse_inner_attrs_and_next();
        let mod_attrs = vec::append(outer_attrs, inner);
        let first_item_outer_attrs = next;
        let m0 = p0.parse_mod_items(token::EOF, first_item_outer_attrs);
        self.sess.included_mod_stack.pop();
        return (ast::item_mod(m0), mod_attrs);
    }

    // parse a function declaration from a foreign module
    fn parse_item_foreign_fn(&self,  attrs: ~[Attribute]) -> @foreign_item {
        let lo = self.span.lo;
        let vis = self.parse_non_priv_visibility();

        // Parse obsolete purity.
        let purity = self.parse_fn_purity();
        if purity != impure_fn {
            self.obsolete(*self.last_span, ObsoleteUnsafeExternFn);
        }

        let (ident, generics) = self.parse_fn_header();
        let decl = self.parse_fn_decl();
        let hi = self.span.hi;
        self.expect(&token::SEMI);
        @ast::foreign_item { ident: ident,
                             attrs: attrs,
                             node: foreign_item_fn(decl, generics),
                             id: ast::DUMMY_NODE_ID,
                             span: mk_sp(lo, hi),
                             vis: vis }
    }

    // parse a const definition from a foreign module
    fn parse_item_foreign_const(&self, vis: ast::visibility,
                                attrs: ~[Attribute]) -> @foreign_item {
        let lo = self.span.lo;

        // XXX: Obsolete; remove after snap.
        if self.eat_keyword(keywords::Const) {
            self.obsolete(*self.last_span, ObsoleteConstItem);
        } else {
            self.expect_keyword(keywords::Static);
        }
        let mutbl = self.eat_keyword(keywords::Mut);

        let ident = self.parse_ident();
        self.expect(&token::COLON);
        let ty = self.parse_ty(false);
        let hi = self.span.hi;
        self.expect(&token::SEMI);
        @ast::foreign_item { ident: ident,
                             attrs: attrs,
                             node: foreign_item_static(ty, mutbl),
                             id: ast::DUMMY_NODE_ID,
                             span: mk_sp(lo, hi),
                             vis: vis }
    }

    // parse safe/unsafe and fn
    fn parse_fn_purity(&self) -> purity {
        if self.eat_keyword(keywords::Fn) { impure_fn }
        else if self.eat_keyword(keywords::Pure) {
            self.obsolete(*self.last_span, ObsoletePurity);
            self.expect_keyword(keywords::Fn);
            // NB: We parse this as impure for bootstrapping purposes.
            impure_fn
        } else if self.eat_keyword(keywords::Unsafe) {
            self.expect_keyword(keywords::Fn);
            unsafe_fn
        }
        else { self.unexpected(); }
    }


    // at this point, this is essentially a wrapper for
    // parse_foreign_items.
    fn parse_foreign_mod_items(&self,
                               sort: ast::foreign_mod_sort,
                               abis: AbiSet,
                               first_item_attrs: ~[Attribute])
                               -> foreign_mod {
        let ParsedItemsAndViewItems {
            attrs_remaining: attrs_remaining,
            view_items: view_items,
            items: _,
            foreign_items: foreign_items
        } = self.parse_foreign_items(first_item_attrs, true);
        if (! attrs_remaining.is_empty()) {
            self.span_err(*self.last_span,
                          "expected item after attributes");
        }
        assert!(*self.token == token::RBRACE);
        ast::foreign_mod {
            sort: sort,
            abis: abis,
            view_items: view_items,
            items: foreign_items
        }
    }

    // parse extern foo; or extern mod foo { ... } or extern { ... }
    fn parse_item_foreign_mod(&self,
                              lo: BytePos,
                              opt_abis: Option<AbiSet>,
                              visibility: visibility,
                              attrs: ~[Attribute],
                              items_allowed: bool)
                              -> item_or_view_item {
        let mut must_be_named_mod = false;
        if self.is_keyword(keywords::Mod) {
            must_be_named_mod = true;
            self.expect_keyword(keywords::Mod);
        } else if *self.token != token::LBRACE {
            self.span_fatal(*self.span,
                            fmt!("expected `{` or `mod` but found `%s`",
                                 self.this_token_to_str()));
        }

        let (sort, maybe_path, ident) = match *self.token {
            token::IDENT(*) => {
                let the_ident = self.parse_ident();
                let path = if *self.token == token::EQ {
                    self.bump();
                    Some(self.parse_str())
                }
                else { None };
                (ast::named, path, the_ident)
            }
            _ => {
                if must_be_named_mod {
                    self.span_fatal(*self.span,
                                    fmt!("expected foreign module name but \
                                          found `%s`",
                                         self.this_token_to_str()));
                }

                (ast::anonymous, None,
                 special_idents::clownshoes_foreign_mod)
            }
        };

        // extern mod foo { ... } or extern { ... }
        if items_allowed && self.eat(&token::LBRACE) {
            // `extern mod foo { ... }` is obsolete.
            if sort == ast::named {
                self.obsolete(*self.last_span, ObsoleteNamedExternModule);
            }

            // Do not allow visibility to be specified.
            if visibility != ast::inherited {
                self.obsolete(*self.last_span, ObsoleteExternVisibility);
            }

            let abis = opt_abis.unwrap_or(AbiSet::C());

            let (inner, next) = self.parse_inner_attrs_and_next();
            let m = self.parse_foreign_mod_items(sort, abis, next);
            self.expect(&token::RBRACE);

            return iovi_item(self.mk_item(lo,
                                          self.last_span.hi,
                                          ident,
                                          item_foreign_mod(m),
                                          public,
                                          maybe_append(attrs, Some(inner))));
        }

        if opt_abis.is_some() {
            self.span_err(*self.span, "an ABI may not be specified here");
        }

        // extern mod foo;
        let metadata = self.parse_optional_meta();
        self.expect(&token::SEMI);
        iovi_view_item(ast::view_item {
            node: view_item_extern_mod(ident, maybe_path, metadata, ast::DUMMY_NODE_ID),
            attrs: attrs,
            vis: visibility,
            span: mk_sp(lo, self.last_span.hi)
        })
    }

    // parse type Foo = Bar;
    fn parse_item_type(&self) -> item_info {
        let ident = self.parse_ident();
        self.parse_region_param();
        let tps = self.parse_generics();
        self.expect(&token::EQ);
        let ty = self.parse_ty(false);
        self.expect(&token::SEMI);
        (ident, item_ty(ty, tps), None)
    }

    // parse obsolete region parameter
    fn parse_region_param(&self) {
        if self.eat(&token::BINOP(token::SLASH)) {
            self.obsolete(*self.last_span, ObsoleteLifetimeNotation);
            self.expect(&token::BINOP(token::AND));
        }
    }

    // parse a structure-like enum variant definition
    // this should probably be renamed or refactored...
    fn parse_struct_def(&self) -> @struct_def {
        let mut fields: ~[@struct_field] = ~[];
        while *self.token != token::RBRACE {
            let r = self.parse_struct_decl_field();
            for struct_field in r.iter() {
                fields.push(*struct_field);
            }
        }
        self.bump();

        return @ast::struct_def {
            fields: fields,
            ctor_id: None
        };
    }

    // parse the part of an "enum" decl following the '{'
    fn parse_enum_def(&self, _generics: &ast::Generics) -> enum_def {
        let mut variants = ~[];
        let mut all_nullary = true;
        let mut have_disr = false;
        while *self.token != token::RBRACE {
            let variant_attrs = self.parse_outer_attributes();
            let vlo = self.span.lo;

            let vis = self.parse_visibility();

            let ident;
            let kind;
            let mut args = ~[];
            let mut disr_expr = None;
            ident = self.parse_ident();
            if self.eat(&token::LBRACE) {
                // Parse a struct variant.
                all_nullary = false;
                kind = struct_variant_kind(self.parse_struct_def());
            } else if *self.token == token::LPAREN {
                all_nullary = false;
                let arg_tys = self.parse_unspanned_seq(
                    &token::LPAREN,
                    &token::RPAREN,
                    seq_sep_trailing_disallowed(token::COMMA),
                    |p| p.parse_ty(false)
                );
                for ty in arg_tys.move_iter() {
                    args.push(ast::variant_arg {
                        ty: ty,
                        id: ast::DUMMY_NODE_ID,
                    });
                }
                kind = tuple_variant_kind(args);
            } else if self.eat(&token::EQ) {
                have_disr = true;
                disr_expr = Some(self.parse_expr());
                kind = tuple_variant_kind(args);
            } else {
                kind = tuple_variant_kind(~[]);
            }

            let vr = ast::variant_ {
                name: ident,
                attrs: variant_attrs,
                kind: kind,
                id: ast::DUMMY_NODE_ID,
                disr_expr: disr_expr,
                vis: vis,
            };
            variants.push(spanned(vlo, self.last_span.hi, vr));

            if !self.eat(&token::COMMA) { break; }
        }
        self.expect(&token::RBRACE);
        if (have_disr && !all_nullary) {
            self.fatal("discriminator values can only be used with a c-like \
                        enum");
        }

        ast::enum_def { variants: variants }
    }

    // parse an "enum" declaration
    fn parse_item_enum(&self) -> item_info {
        let id = self.parse_ident();
        self.parse_region_param();
        let generics = self.parse_generics();
        // Newtype syntax
        if *self.token == token::EQ {
            // enum x = ty;
            self.bump();
            let ty = self.parse_ty(false);
            self.expect(&token::SEMI);
            let variant = spanned(ty.span.lo, ty.span.hi, ast::variant_ {
                name: id,
                attrs: ~[],
                kind: tuple_variant_kind(
                    ~[ast::variant_arg {ty: ty, id: ast::DUMMY_NODE_ID}]
                ),
                id: ast::DUMMY_NODE_ID,
                disr_expr: None,
                vis: public,
            });

            self.obsolete(*self.last_span, ObsoleteNewtypeEnum);

            return (
                id,
                item_enum(
                    ast::enum_def { variants: ~[variant] },
                    generics),
                None
            );
        }
        // enum X { ... }
        self.expect(&token::LBRACE);

        let enum_definition = self.parse_enum_def(&generics);
        (id, item_enum(enum_definition, generics), None)
    }

    fn parse_fn_ty_sigil(&self) -> Option<Sigil> {
        match *self.token {
            token::AT => {
                self.bump();
                Some(ManagedSigil)
            }
            token::TILDE => {
                self.bump();
                Some(OwnedSigil)
            }
            token::BINOP(token::AND) => {
                self.bump();
                Some(BorrowedSigil)
            }
            _ => {
                None
            }
        }
    }

    fn fn_expr_lookahead(&self, tok: &token::Token) -> bool {
        match *tok {
          token::LPAREN | token::AT | token::TILDE | token::BINOP(_) => true,
          _ => false
        }
    }

    // parse a string as an ABI spec on an extern type or module
    fn parse_opt_abis(&self) -> Option<AbiSet> {
        match *self.token {
            token::LIT_STR(s) => {
                self.bump();
                let the_string = ident_to_str(&s);
                let mut abis = AbiSet::empty();
                for word in the_string.word_iter() {
                    match abi::lookup(word) {
                        Some(abi) => {
                            if abis.contains(abi) {
                                self.span_err(
                                    *self.span,
                                    fmt!("ABI `%s` appears twice",
                                         word));
                            } else {
                                abis.add(abi);
                            }
                        }

                        None => {
                            self.span_err(
                                *self.span,
                                fmt!("illegal ABI: \
                                      expected one of [%s], \
                                      found `%s`",
                                     abi::all_names().connect(", "),
                                     word));
                        }
                    }
                }
                Some(abis)
            }

            _ => {
                None
            }
        }
    }

    // parse one of the items or view items allowed by the
    // flags; on failure, return iovi_none.
    // NB: this function no longer parses the items inside an
    // extern mod.
    fn parse_item_or_view_item(&self,
                               attrs: ~[Attribute],
                               macros_allowed: bool)
                               -> item_or_view_item {
        match *self.token {
            INTERPOLATED(token::nt_item(item)) => {
                self.bump();
                let new_attrs = vec::append(attrs, item.attrs);
                return iovi_item(@ast::item {
                        attrs: new_attrs,
                        ..(*item).clone()});
            }
            _ => {}
        }

        let lo = self.span.lo;

        let visibility = self.parse_non_priv_visibility();

        // must be a view item:
        if self.eat_keyword(keywords::Use) {
            // USE ITEM (iovi_view_item)
            let view_item = self.parse_use();
            self.expect(&token::SEMI);
            return iovi_view_item(ast::view_item {
                node: view_item,
                attrs: attrs,
                vis: visibility,
                span: mk_sp(lo, self.last_span.hi)
            });
        }
        // either a view item or an item:
        if self.eat_keyword(keywords::Extern) {
            let opt_abis = self.parse_opt_abis();

            if self.eat_keyword(keywords::Fn) {
                // EXTERN FUNCTION ITEM
                let abis = opt_abis.unwrap_or(AbiSet::C());
                let (ident, item_, extra_attrs) =
                    self.parse_item_fn(extern_fn, abis);
                return iovi_item(self.mk_item(lo, self.last_span.hi, ident,
                                              item_, visibility,
                                              maybe_append(attrs,
                                                           extra_attrs)));
            } else  {
                // EXTERN MODULE ITEM (iovi_view_item)
                return self.parse_item_foreign_mod(lo, opt_abis, visibility, attrs,
                                                   true);
            }
        }
        // the rest are all guaranteed to be items:
        if (self.is_keyword(keywords::Const) ||
            (self.is_keyword(keywords::Static) &&
             self.look_ahead(1, |t| !token::is_keyword(keywords::Fn, t)))) {
            // CONST / STATIC ITEM
            if self.is_keyword(keywords::Const) {
                self.obsolete(*self.span, ObsoleteConstItem);
            }
            self.bump();
            let (ident, item_, extra_attrs) = self.parse_item_const();
            return iovi_item(self.mk_item(lo, self.last_span.hi, ident, item_,
                                          visibility,
                                          maybe_append(attrs, extra_attrs)));
        }
        if self.is_keyword(keywords::Fn) &&
                self.look_ahead(1, |f| !self.fn_expr_lookahead(f)) {
            // FUNCTION ITEM
            self.bump();
            let (ident, item_, extra_attrs) =
                self.parse_item_fn(impure_fn, AbiSet::Rust());
            return iovi_item(self.mk_item(lo, self.last_span.hi, ident, item_,
                                          visibility,
                                          maybe_append(attrs, extra_attrs)));
        }
        if self.eat_keyword(keywords::Pure) {
            // PURE FUNCTION ITEM (obsolete)
            self.obsolete(*self.last_span, ObsoletePurity);
            self.expect_keyword(keywords::Fn);
            let (ident, item_, extra_attrs) =
                self.parse_item_fn(impure_fn, AbiSet::Rust());
            return iovi_item(self.mk_item(lo, self.last_span.hi, ident, item_,
                                          visibility,
                                          maybe_append(attrs, extra_attrs)));
        }
        if self.is_keyword(keywords::Unsafe)
            && self.look_ahead(1u, |t| *t != token::LBRACE) {
            // UNSAFE FUNCTION ITEM
            self.bump();
            self.expect_keyword(keywords::Fn);
            let (ident, item_, extra_attrs) =
                self.parse_item_fn(unsafe_fn, AbiSet::Rust());
            return iovi_item(self.mk_item(lo, self.last_span.hi, ident, item_,
                                          visibility,
                                          maybe_append(attrs, extra_attrs)));
        }
        if self.eat_keyword(keywords::Mod) {
            // MODULE ITEM
            let (ident, item_, extra_attrs) = self.parse_item_mod(attrs);
            return iovi_item(self.mk_item(lo, self.last_span.hi, ident, item_,
                                          visibility,
                                          maybe_append(attrs, extra_attrs)));
        }
        if self.eat_keyword(keywords::Type) {
            // TYPE ITEM
            let (ident, item_, extra_attrs) = self.parse_item_type();
            return iovi_item(self.mk_item(lo, self.last_span.hi, ident, item_,
                                          visibility,
                                          maybe_append(attrs, extra_attrs)));
        }
        if self.eat_keyword(keywords::Enum) {
            // ENUM ITEM
            let (ident, item_, extra_attrs) = self.parse_item_enum();
            return iovi_item(self.mk_item(lo, self.last_span.hi, ident, item_,
                                          visibility,
                                          maybe_append(attrs, extra_attrs)));
        }
        if self.eat_keyword(keywords::Trait) {
            // TRAIT ITEM
            let (ident, item_, extra_attrs) = self.parse_item_trait();
            return iovi_item(self.mk_item(lo, self.last_span.hi, ident, item_,
                                          visibility,
                                          maybe_append(attrs, extra_attrs)));
        }
        if self.eat_keyword(keywords::Impl) {
            // IMPL ITEM
            let (ident, item_, extra_attrs) =
                self.parse_item_impl(visibility);
            return iovi_item(self.mk_item(lo, self.last_span.hi, ident, item_,
                                          visibility,
                                          maybe_append(attrs, extra_attrs)));
        }
        if self.eat_keyword(keywords::Struct) {
            // STRUCT ITEM
            let (ident, item_, extra_attrs) = self.parse_item_struct();
            return iovi_item(self.mk_item(lo, self.last_span.hi, ident, item_,
                                          visibility,
                                          maybe_append(attrs, extra_attrs)));
        }
        self.parse_macro_use_or_failure(attrs,macros_allowed,lo,visibility)
    }

    // parse a foreign item; on failure, return iovi_none.
    fn parse_foreign_item(&self,
                          attrs: ~[Attribute],
                          macros_allowed: bool)
                          -> item_or_view_item {
        maybe_whole!(iovi self, nt_item);
        let lo = self.span.lo;

        let visibility = self.parse_non_priv_visibility();

        if (self.is_keyword(keywords::Const) || self.is_keyword(keywords::Static)) {
            // FOREIGN CONST ITEM
            let item = self.parse_item_foreign_const(visibility, attrs);
            return iovi_foreign_item(item);
        }
        if (self.is_keyword(keywords::Fn) || self.is_keyword(keywords::Pure) ||
                self.is_keyword(keywords::Unsafe)) {
            // FOREIGN FUNCTION ITEM
            let item = self.parse_item_foreign_fn(attrs);
            return iovi_foreign_item(item);
        }
        self.parse_macro_use_or_failure(attrs,macros_allowed,lo,visibility)
    }

    // this is the fall-through for parsing items.
    fn parse_macro_use_or_failure(
        &self,
        attrs: ~[Attribute],
        macros_allowed: bool,
        lo : BytePos,
        visibility : visibility
    ) -> item_or_view_item {
        if macros_allowed && !token::is_any_keyword(self.token)
                && self.look_ahead(1, |t| *t == token::NOT)
                && (self.look_ahead(2, |t| is_plain_ident(t))
                    || self.look_ahead(2, |t| *t == token::LPAREN)
                    || self.look_ahead(2, |t| *t == token::LBRACE)) {
            // MACRO INVOCATION ITEM
            if attrs.len() > 0 {
                self.fatal("attrs on macros are not yet supported");
            }

            // item macro.
            let pth = self.parse_path(NoTypesAllowed).path;
            self.expect(&token::NOT);

            // a 'special' identifier (like what `macro_rules!` uses)
            // is optional. We should eventually unify invoc syntax
            // and remove this.
            let id = if is_plain_ident(&*self.token) {
                self.parse_ident()
            } else {
                token::special_idents::invalid // no special identifier
            };
            // eat a matched-delimiter token tree:
            let tts = match *self.token {
                token::LPAREN | token::LBRACE => {
                    let ket = token::flip_delimiter(&*self.token);
                    self.bump();
                    self.parse_seq_to_end(&ket,
                                          seq_sep_none(),
                                          |p| p.parse_token_tree())
                }
                _ => self.fatal("expected open delimiter")
            };
            // single-variant-enum... :
            let m = ast::mac_invoc_tt(pth, tts, EMPTY_CTXT);
            let m: ast::mac = codemap::Spanned { node: m,
                                             span: mk_sp(self.span.lo,
                                                         self.span.hi) };
            let item_ = item_mac(m);
            return iovi_item(self.mk_item(lo, self.last_span.hi, id, item_,
                                          visibility, attrs));
        }

        // FAILURE TO PARSE ITEM
        if visibility != inherited {
            let mut s = ~"unmatched visibility `";
            if visibility == public {
                s.push_str("pub")
            } else {
                s.push_str("priv")
            }
            s.push_char('`');
            self.span_fatal(*self.last_span, s);
        }
        return iovi_none(attrs);
    }

    pub fn parse_item(&self, attrs: ~[Attribute]) -> Option<@ast::item> {
        match self.parse_item_or_view_item(attrs, true) {
            iovi_none(_) => None,
            iovi_view_item(_) =>
                self.fatal("view items are not allowed here"),
            iovi_foreign_item(_) =>
                self.fatal("foreign items are not allowed here"),
            iovi_item(item) => Some(item)
        }
    }

    // parse, e.g., "use a::b::{z,y}"
    fn parse_use(&self) -> view_item_ {
        return view_item_use(self.parse_view_paths());
    }


    // matches view_path : MOD? IDENT EQ non_global_path
    // | MOD? non_global_path MOD_SEP LBRACE RBRACE
    // | MOD? non_global_path MOD_SEP LBRACE ident_seq RBRACE
    // | MOD? non_global_path MOD_SEP STAR
    // | MOD? non_global_path
    fn parse_view_path(&self) -> @view_path {
        let lo = self.span.lo;

        let first_ident = self.parse_ident();
        let mut path = ~[first_ident];
        debug!("parsed view_path: %s", self.id_to_str(first_ident));
        match *self.token {
          token::EQ => {
            // x = foo::bar
            self.bump();
            path = ~[self.parse_ident()];
            while *self.token == token::MOD_SEP {
                self.bump();
                let id = self.parse_ident();
                path.push(id);
            }
            let path = ast::Path {
                span: mk_sp(lo, self.span.hi),
                global: false,
                segments: path.move_iter().map(|identifier| {
                    ast::PathSegment {
                        identifier: identifier,
                        lifetime: None,
                        types: opt_vec::Empty,
                    }
                }).collect()
            };
            return @spanned(lo, self.span.hi,
                            view_path_simple(first_ident,
                                             path,
                                             ast::DUMMY_NODE_ID));
          }

          token::MOD_SEP => {
            // foo::bar or foo::{a,b,c} or foo::*
            while *self.token == token::MOD_SEP {
                self.bump();

                match *self.token {
                  token::IDENT(i, _) => {
                    self.bump();
                    path.push(i);
                  }

                  // foo::bar::{a,b,c}
                  token::LBRACE => {
                    let idents = self.parse_unspanned_seq(
                        &token::LBRACE,
                        &token::RBRACE,
                        seq_sep_trailing_allowed(token::COMMA),
                        |p| p.parse_path_list_ident()
                    );
                    let path = ast::Path {
                        span: mk_sp(lo, self.span.hi),
                        global: false,
                        segments: path.move_iter().map(|identifier| {
                            ast::PathSegment {
                                identifier: identifier,
                                lifetime: None,
                                types: opt_vec::Empty,
                            }
                        }).collect()
                    };
                    return @spanned(lo, self.span.hi,
                                 view_path_list(path, idents, ast::DUMMY_NODE_ID));
                  }

                  // foo::bar::*
                  token::BINOP(token::STAR) => {
                    self.bump();
                    let path = ast::Path {
                        span: mk_sp(lo, self.span.hi),
                        global: false,
                        segments: path.move_iter().map(|identifier| {
                            ast::PathSegment {
                                identifier: identifier,
                                lifetime: None,
                                types: opt_vec::Empty,
                            }
                        }).collect()
                    };
                    return @spanned(lo, self.span.hi,
                                    view_path_glob(path, ast::DUMMY_NODE_ID));
                  }

                  _ => break
                }
            }
          }
          _ => ()
        }
        let last = path[path.len() - 1u];
        let path = ast::Path {
            span: mk_sp(lo, self.span.hi),
            global: false,
            segments: path.move_iter().map(|identifier| {
                ast::PathSegment {
                    identifier: identifier,
                    lifetime: None,
                    types: opt_vec::Empty,
                }
            }).collect()
        };
        return @spanned(lo,
                        self.last_span.hi,
                        view_path_simple(last, path, ast::DUMMY_NODE_ID));
    }

    // matches view_paths = view_path | view_path , view_paths
    fn parse_view_paths(&self) -> ~[@view_path] {
        let mut vp = ~[self.parse_view_path()];
        while *self.token == token::COMMA {
            self.bump();
            vp.push(self.parse_view_path());
        }
        return vp;
    }

    fn is_view_item(&self) -> bool {
        if !self.is_keyword(keywords::Pub) && !self.is_keyword(keywords::Priv) {
            token::is_keyword(keywords::Use, self.token)
                || (token::is_keyword(keywords::Extern, self.token) &&
                    self.look_ahead(1,
                                    |t| token::is_keyword(keywords::Mod, t)))
        } else {
            self.look_ahead(1, |t| token::is_keyword(keywords::Use, t))
                || (self.look_ahead(1,
                                    |t| token::is_keyword(keywords::Extern,
                                                          t)) &&
                    self.look_ahead(2,
                                    |t| token::is_keyword(keywords::Mod, t)))
        }
    }

    // parse a view item.
    fn parse_view_item(
        &self,
        attrs: ~[Attribute],
        vis: visibility
    ) -> view_item {
        let lo = self.span.lo;
        let node = if self.eat_keyword(keywords::Use) {
            self.parse_use()
        } else if self.eat_keyword(keywords::Extern) {
            self.expect_keyword(keywords::Mod);
            let ident = self.parse_ident();
            let path = if *self.token == token::EQ {
                self.bump();
                Some(self.parse_str())
            }
            else { None };
            let metadata = self.parse_optional_meta();
            view_item_extern_mod(ident, path, metadata, ast::DUMMY_NODE_ID)
        } else {
            self.bug("expected view item");
        };
        self.expect(&token::SEMI);
        ast::view_item { node: node,
                          attrs: attrs,
                          vis: vis,
                          span: mk_sp(lo, self.last_span.hi) }
    }

    // Parses a sequence of items. Stops when it finds program
    // text that can't be parsed as an item
    // - mod_items uses extern_mod_allowed = true
    // - block_tail_ uses extern_mod_allowed = false
    fn parse_items_and_view_items(&self,
                                  first_item_attrs: ~[Attribute],
                                  mut extern_mod_allowed: bool,
                                  macros_allowed: bool)
                                  -> ParsedItemsAndViewItems {
        let mut attrs = vec::append(first_item_attrs,
                                    self.parse_outer_attributes());
        // First, parse view items.
        let mut view_items : ~[ast::view_item] = ~[];
        let mut items = ~[];

        // I think this code would probably read better as a single
        // loop with a mutable three-state-variable (for extern mods,
        // view items, and regular items) ... except that because
        // of macros, I'd like to delay that entire check until later.
        loop {
            match self.parse_item_or_view_item(attrs, macros_allowed) {
                iovi_none(attrs) => {
                    return ParsedItemsAndViewItems {
                        attrs_remaining: attrs,
                        view_items: view_items,
                        items: items,
                        foreign_items: ~[]
                    }
                }
                iovi_view_item(view_item) => {
                    match view_item.node {
                        view_item_use(*) => {
                            // `extern mod` must precede `use`.
                            extern_mod_allowed = false;
                        }
                        view_item_extern_mod(*)
                        if !extern_mod_allowed => {
                            self.span_err(view_item.span,
                                          "\"extern mod\" declarations are not allowed here");
                        }
                        view_item_extern_mod(*) => {}
                    }
                    view_items.push(view_item);
                }
                iovi_item(item) => {
                    items.push(item);
                    attrs = self.parse_outer_attributes();
                    break;
                }
                iovi_foreign_item(_) => {
                    fail!();
                }
            }
            attrs = self.parse_outer_attributes();
        }

        // Next, parse items.
        loop {
            match self.parse_item_or_view_item(attrs, macros_allowed) {
                iovi_none(returned_attrs) => {
                    attrs = returned_attrs;
                    break
                }
                iovi_view_item(view_item) => {
                    attrs = self.parse_outer_attributes();
                    self.span_err(view_item.span,
                                  "`use` and `extern mod` declarations must precede items");
                }
                iovi_item(item) => {
                    attrs = self.parse_outer_attributes();
                    items.push(item)
                }
                iovi_foreign_item(_) => {
                    fail!();
                }
            }
        }

        ParsedItemsAndViewItems {
            attrs_remaining: attrs,
            view_items: view_items,
            items: items,
            foreign_items: ~[]
        }
    }

    // Parses a sequence of foreign items. Stops when it finds program
    // text that can't be parsed as an item
    fn parse_foreign_items(&self, first_item_attrs: ~[Attribute],
                           macros_allowed: bool)
        -> ParsedItemsAndViewItems {
        let mut attrs = vec::append(first_item_attrs,
                                    self.parse_outer_attributes());
        let mut foreign_items = ~[];
        loop {
            match self.parse_foreign_item(attrs, macros_allowed) {
                iovi_none(returned_attrs) => {
                    if *self.token == token::RBRACE {
                        attrs = returned_attrs;
                        break
                    }
                    self.unexpected();
                },
                iovi_view_item(view_item) => {
                    // I think this can't occur:
                    self.span_err(view_item.span,
                                  "`use` and `extern mod` declarations must precede items");
                }
                iovi_item(item) => {
                    // FIXME #5668: this will occur for a macro invocation:
                    self.span_fatal(item.span, "macros cannot expand to foreign items");
                }
                iovi_foreign_item(foreign_item) => {
                    foreign_items.push(foreign_item);
                }
            }
            attrs = self.parse_outer_attributes();
        }

        ParsedItemsAndViewItems {
            attrs_remaining: attrs,
            view_items: ~[],
            items: ~[],
            foreign_items: foreign_items
        }
    }

    // Parses a source module as a crate. This is the main
    // entry point for the parser.
    pub fn parse_crate_mod(&self) -> @Crate {
        let lo = self.span.lo;
        // parse the crate's inner attrs, maybe (oops) one
        // of the attrs of an item:
        let (inner, next) = self.parse_inner_attrs_and_next();
        let first_item_outer_attrs = next;
        // parse the items inside the crate:
        let m = self.parse_mod_items(token::EOF, first_item_outer_attrs);

        @ast::Crate {
            module: m,
            attrs: inner,
            config: self.cfg.clone(),
            span: mk_sp(lo, self.span.lo)
        }
    }

    pub fn parse_optional_str(&self) -> Option<@str> {
        match *self.token {
            token::LIT_STR(s) => {
                self.bump();
                Some(ident_to_str(&s))
            }
            _ => None
        }
    }

    pub fn parse_str(&self) -> @str {
        match self.parse_optional_str() {
            Some(s) => { s }
            _ =>  self.fatal("expected string literal")
        }
    }
}
