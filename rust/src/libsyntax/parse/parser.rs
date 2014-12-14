// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![macro_escape]

pub use self::PathParsingMode::*;
use self::ItemOrViewItem::*;

use abi;
use ast::{AssociatedType, BareFnTy, ClosureTy};
use ast::{RegionTyParamBound, TraitTyParamBound};
use ast::{ProvidedMethod, Public, FnStyle};
use ast::{Mod, BiAdd, Arg, Arm, Attribute, BindByRef, BindByValue};
use ast::{BiBitAnd, BiBitOr, BiBitXor, BiRem, Block};
use ast::{BlockCheckMode, CaptureByRef, CaptureByValue, CaptureClause};
use ast::{Crate, CrateConfig, Decl, DeclItem};
use ast::{DeclLocal, DefaultBlock, UnDeref, BiDiv, EMPTY_CTXT, EnumDef, ExplicitSelf};
use ast::{Expr, Expr_, ExprAddrOf, ExprMatch, ExprAgain};
use ast::{ExprAssign, ExprAssignOp, ExprBinary, ExprBlock, ExprBox};
use ast::{ExprBreak, ExprCall, ExprCast};
use ast::{ExprField, ExprTupField, ExprClosure, ExprIf, ExprIfLet, ExprIndex, ExprSlice};
use ast::{ExprLit, ExprLoop, ExprMac};
use ast::{ExprMethodCall, ExprParen, ExprPath, ExprProc};
use ast::{ExprRepeat, ExprRet, ExprStruct, ExprTup, ExprUnary};
use ast::{ExprVec, ExprWhile, ExprWhileLet, ExprForLoop, Field, FnDecl};
use ast::{Once, Many};
use ast::{FnUnboxedClosureKind, FnMutUnboxedClosureKind};
use ast::{FnOnceUnboxedClosureKind};
use ast::{ForeignItem, ForeignItemStatic, ForeignItemFn, ForeignMod, FunctionRetTy};
use ast::{Ident, NormalFn, Inherited, ImplItem, Item, Item_, ItemStatic};
use ast::{ItemEnum, ItemFn, ItemForeignMod, ItemImpl, ItemConst};
use ast::{ItemMac, ItemMod, ItemStruct, ItemTrait, ItemTy};
use ast::{LifetimeDef, Lit, Lit_};
use ast::{LitBool, LitChar, LitByte, LitBinary};
use ast::{LitStr, LitInt, Local, LocalLet};
use ast::{MutImmutable, MutMutable, Mac_, MacInvocTT, MatchNormal};
use ast::{Method, MutTy, BiMul, Mutability};
use ast::{MethodImplItem, NamedField, UnNeg, NoReturn, UnNot};
use ast::{Pat, PatEnum, PatIdent, PatLit, PatRange, PatRegion, PatStruct};
use ast::{PatTup, PatBox, PatWild, PatWildMulti, PatWildSingle};
use ast::{PolyTraitRef};
use ast::{QPath, RequiredMethod};
use ast::{Return, BiShl, BiShr, Stmt, StmtDecl};
use ast::{StmtExpr, StmtSemi, StmtMac, StructDef, StructField};
use ast::{StructVariantKind, BiSub, StrStyle};
use ast::{SelfExplicit, SelfRegion, SelfStatic, SelfValue};
use ast::{Delimited, SequenceRepetition, TokenTree, TraitItem, TraitRef};
use ast::{TtDelimited, TtSequence, TtToken};
use ast::{TupleVariantKind, Ty, Ty_, TypeBinding};
use ast::{TypeField, TyFixedLengthVec, TyClosure, TyProc, TyBareFn};
use ast::{TyTypeof, TyInfer, TypeMethod};
use ast::{TyParam, TyParamBound, TyParen, TyPath, TyPolyTraitRef, TyPtr, TyQPath};
use ast::{TyRptr, TyTup, TyU32, TyVec, UnUniq};
use ast::{TypeImplItem, TypeTraitItem, Typedef, UnboxedClosureKind};
use ast::{UnnamedField, UnsafeBlock};
use ast::{UnsafeFn, ViewItem, ViewItem_, ViewItemExternCrate, ViewItemUse};
use ast::{ViewPath, ViewPathGlob, ViewPathList, ViewPathSimple};
use ast::{Visibility, WhereClause};
use ast;
use ast_util::{mod, as_prec, ident_to_path, operator_prec};
use codemap::{mod, Span, BytePos, Spanned, spanned, mk_sp};
use diagnostic;
use ext::tt::macro_parser;
use parse;
use parse::attr::ParserAttr;
use parse::classify;
use parse::common::{SeqSep, seq_sep_none, seq_sep_trailing_allowed};
use parse::lexer::{Reader, TokenAndSpan};
use parse::obsolete::*;
use parse::token::{mod, MatchNt, SubstNt, InternedString};
use parse::token::{keywords, special_idents};
use parse::{new_sub_parser_from_file, ParseSess};
use print::pprust;
use ptr::P;
use owned_slice::OwnedSlice;

use std::collections::HashSet;
use std::io::fs::PathExtensions;
use std::mem;
use std::num::Float;
use std::rc::Rc;
use std::iter;
use std::slice;

bitflags! {
    flags Restrictions: u8 {
        const UNRESTRICTED                  = 0b0000,
        const RESTRICTION_STMT_EXPR         = 0b0001,
        const RESTRICTION_NO_BAR_OP         = 0b0010,
        const RESTRICTION_NO_STRUCT_LITERAL = 0b0100
    }
}

impl Copy for Restrictions {}

type ItemInfo = (Ident, Item_, Option<Vec<Attribute> >);

/// How to parse a path. There are four different kinds of paths, all of which
/// are parsed somewhat differently.
#[deriving(PartialEq)]
pub enum PathParsingMode {
    /// A path with no type parameters; e.g. `foo::bar::Baz`
    NoTypesAllowed,
    /// A path with a lifetime and type parameters, with no double colons
    /// before the type parameters; e.g. `foo::bar<'a>::Baz<T>`
    LifetimeAndTypesWithoutColons,
    /// A path with a lifetime and type parameters with double colons before
    /// the type parameters; e.g. `foo::bar::<'a>::Baz::<T>`
    LifetimeAndTypesWithColons,
}

impl Copy for PathParsingMode {}

enum ItemOrViewItem {
    /// Indicates a failure to parse any kind of item. The attributes are
    /// returned.
    IoviNone(Vec<Attribute>),
    IoviItem(P<Item>),
    IoviForeignItem(P<ForeignItem>),
    IoviViewItem(ViewItem)
}


/// Possibly accept an `token::Interpolated` expression (a pre-parsed expression
/// dropped into the token stream, which happens while parsing the result of
/// macro expansion). Placement of these is not as complex as I feared it would
/// be. The important thing is to make sure that lookahead doesn't balk at
/// `token::Interpolated` tokens.
macro_rules! maybe_whole_expr (
    ($p:expr) => (
        {
            let found = match $p.token {
                token::Interpolated(token::NtExpr(ref e)) => {
                    Some((*e).clone())
                }
                token::Interpolated(token::NtPath(_)) => {
                    // FIXME: The following avoids an issue with lexical borrowck scopes,
                    // but the clone is unfortunate.
                    let pt = match $p.token {
                        token::Interpolated(token::NtPath(ref pt)) => (**pt).clone(),
                        _ => unreachable!()
                    };
                    let span = $p.span;
                    Some($p.mk_expr(span.lo, span.hi, ExprPath(pt)))
                }
                token::Interpolated(token::NtBlock(_)) => {
                    // FIXME: The following avoids an issue with lexical borrowck scopes,
                    // but the clone is unfortunate.
                    let b = match $p.token {
                        token::Interpolated(token::NtBlock(ref b)) => (*b).clone(),
                        _ => unreachable!()
                    };
                    let span = $p.span;
                    Some($p.mk_expr(span.lo, span.hi, ExprBlock(b)))
                }
                _ => None
            };
            match found {
                Some(e) => {
                    $p.bump();
                    return e;
                }
                None => ()
            }
        }
    )
)

/// As maybe_whole_expr, but for things other than expressions
macro_rules! maybe_whole (
    ($p:expr, $constructor:ident) => (
        {
            let found = match ($p).token {
                token::Interpolated(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            if let Some(token::Interpolated(token::$constructor(x))) = found {
                return x.clone();
            }
        }
    );
    (no_clone $p:expr, $constructor:ident) => (
        {
            let found = match ($p).token {
                token::Interpolated(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            if let Some(token::Interpolated(token::$constructor(x))) = found {
                return x;
            }
        }
    );
    (deref $p:expr, $constructor:ident) => (
        {
            let found = match ($p).token {
                token::Interpolated(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            if let Some(token::Interpolated(token::$constructor(x))) = found {
                return (*x).clone();
            }
        }
    );
    (Some $p:expr, $constructor:ident) => (
        {
            let found = match ($p).token {
                token::Interpolated(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            if let Some(token::Interpolated(token::$constructor(x))) = found {
                return Some(x.clone());
            }
        }
    );
    (iovi $p:expr, $constructor:ident) => (
        {
            let found = match ($p).token {
                token::Interpolated(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            if let Some(token::Interpolated(token::$constructor(x))) = found {
                return IoviItem(x.clone());
            }
        }
    );
    (pair_empty $p:expr, $constructor:ident) => (
        {
            let found = match ($p).token {
                token::Interpolated(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            if let Some(token::Interpolated(token::$constructor(x))) = found {
                return (Vec::new(), x);
            }
        }
    )
)


fn maybe_append(mut lhs: Vec<Attribute>, rhs: Option<Vec<Attribute>>)
                -> Vec<Attribute> {
    match rhs {
        Some(ref attrs) => lhs.extend(attrs.iter().map(|a| a.clone())),
        None => {}
    }
    lhs
}


struct ParsedItemsAndViewItems {
    attrs_remaining: Vec<Attribute>,
    view_items: Vec<ViewItem>,
    items: Vec<P<Item>> ,
    foreign_items: Vec<P<ForeignItem>>
}

/* ident is handled by common.rs */

pub struct Parser<'a> {
    pub sess: &'a ParseSess,
    /// the current token:
    pub token: token::Token,
    /// the span of the current token:
    pub span: Span,
    /// the span of the prior token:
    pub last_span: Span,
    pub cfg: CrateConfig,
    /// the previous token or None (only stashed sometimes).
    pub last_token: Option<Box<token::Token>>,
    pub buffer: [TokenAndSpan, ..4],
    pub buffer_start: int,
    pub buffer_end: int,
    pub tokens_consumed: uint,
    pub restrictions: Restrictions,
    pub quote_depth: uint, // not (yet) related to the quasiquoter
    pub reader: Box<Reader+'a>,
    pub interner: Rc<token::IdentInterner>,
    /// The set of seen errors about obsolete syntax. Used to suppress
    /// extra detail when the same error is seen twice
    pub obsolete_set: HashSet<ObsoleteSyntax>,
    /// Used to determine the path to externally loaded source files
    pub mod_path_stack: Vec<InternedString>,
    /// Stack of spans of open delimiters. Used for error message.
    pub open_braces: Vec<Span>,
    /// Flag if this parser "owns" the directory that it is currently parsing
    /// in. This will affect how nested files are looked up.
    pub owns_directory: bool,
    /// Name of the root module this parser originated from. If `None`, then the
    /// name is not known. This does not change while the parser is descending
    /// into modules, and sub-parsers have new values for this name.
    pub root_module_name: Option<String>,
    pub expected_tokens: Vec<TokenType>,
}

#[deriving(PartialEq, Eq, Clone)]
pub enum TokenType {
    Token(token::Token),
    Operator,
}

impl TokenType {
    fn to_string(&self) -> String {
        match *self {
            TokenType::Token(ref t) => format!("`{}`", Parser::token_to_string(t)),
            TokenType::Operator => "an operator".into_string(),
        }
    }
}

fn is_plain_ident_or_underscore(t: &token::Token) -> bool {
    t.is_plain_ident() || *t == token::Underscore
}

impl<'a> Parser<'a> {
    pub fn new(sess: &'a ParseSess,
               cfg: ast::CrateConfig,
               mut rdr: Box<Reader+'a>)
               -> Parser<'a>
    {
        let tok0 = rdr.real_token();
        let span = tok0.sp;
        let placeholder = TokenAndSpan {
            tok: token::Underscore,
            sp: span,
        };

        Parser {
            reader: rdr,
            interner: token::get_ident_interner(),
            sess: sess,
            cfg: cfg,
            token: tok0.tok,
            span: span,
            last_span: span,
            last_token: None,
            buffer: [
                placeholder.clone(),
                placeholder.clone(),
                placeholder.clone(),
                placeholder.clone(),
            ],
            buffer_start: 0,
            buffer_end: 0,
            tokens_consumed: 0,
            restrictions: UNRESTRICTED,
            quote_depth: 0,
            obsolete_set: HashSet::new(),
            mod_path_stack: Vec::new(),
            open_braces: Vec::new(),
            owns_directory: true,
            root_module_name: None,
            expected_tokens: Vec::new(),
        }
    }

    /// Convert a token to a string using self's reader
    pub fn token_to_string(token: &token::Token) -> String {
        pprust::token_to_string(token)
    }

    /// Convert the current token to a string using self's reader
    pub fn this_token_to_string(&mut self) -> String {
        Parser::token_to_string(&self.token)
    }

    pub fn unexpected_last(&mut self, t: &token::Token) -> ! {
        let token_str = Parser::token_to_string(t);
        let last_span = self.last_span;
        self.span_fatal(last_span, format!("unexpected token: `{}`",
                                                token_str).as_slice());
    }

    pub fn unexpected(&mut self) -> ! {
        let this_token = self.this_token_to_string();
        self.fatal(format!("unexpected token: `{}`", this_token).as_slice());
    }

    /// Expect and consume the token t. Signal an error if
    /// the next token is not t.
    pub fn expect(&mut self, t: &token::Token) {
        if self.expected_tokens.is_empty() {
            if self.token == *t {
                self.bump();
            } else {
                let token_str = Parser::token_to_string(t);
                let this_token_str = self.this_token_to_string();
                self.fatal(format!("expected `{}`, found `{}`",
                                   token_str,
                                   this_token_str).as_slice())
            }
        } else {
            self.expect_one_of(slice::ref_slice(t), &[]);
        }
    }

    /// Expect next token to be edible or inedible token.  If edible,
    /// then consume it; if inedible, then return without consuming
    /// anything.  Signal a fatal error if next token is unexpected.
    pub fn expect_one_of(&mut self,
                         edible: &[token::Token],
                         inedible: &[token::Token]) {
        fn tokens_to_string(tokens: &[TokenType]) -> String {
            let mut i = tokens.iter();
            // This might be a sign we need a connect method on Iterator.
            let b = i.next()
                     .map_or("".into_string(), |t| t.to_string());
            i.enumerate().fold(b, |mut b, (i, ref a)| {
                if tokens.len() > 2 && i == tokens.len() - 2 {
                    b.push_str(", or ");
                } else if tokens.len() == 2 && i == tokens.len() - 2 {
                    b.push_str(" or ");
                } else {
                    b.push_str(", ");
                }
                b.push_str(&*a.to_string());
                b
            })
        }
        if edible.contains(&self.token) {
            self.bump();
        } else if inedible.contains(&self.token) {
            // leave it in the input
        } else {
            let mut expected = edible.iter().map(|x| TokenType::Token(x.clone()))
                                            .collect::<Vec<_>>();
            expected.extend(inedible.iter().map(|x| TokenType::Token(x.clone())));
            expected.push_all(&*self.expected_tokens);
            expected.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
            expected.dedup();
            let expect = tokens_to_string(expected.as_slice());
            let actual = self.this_token_to_string();
            self.fatal(
                (if expected.len() != 1 {
                    (format!("expected one of {}, found `{}`",
                             expect,
                             actual))
                } else {
                    (format!("expected {}, found `{}`",
                             expect,
                             actual))
                }).as_slice()
            )
        }
    }

    /// Check for erroneous `ident { }`; if matches, signal error and
    /// recover (without consuming any expected input token).  Returns
    /// true if and only if input was consumed for recovery.
    pub fn check_for_erroneous_unit_struct_expecting(&mut self, expected: &[token::Token]) -> bool {
        if self.token == token::OpenDelim(token::Brace)
            && expected.iter().all(|t| *t != token::OpenDelim(token::Brace))
            && self.look_ahead(1, |t| *t == token::CloseDelim(token::Brace)) {
            // matched; signal non-fatal error and recover.
            let span = self.span;
            self.span_err(span,
                          "unit-like struct construction is written with no trailing `{ }`");
            self.eat(&token::OpenDelim(token::Brace));
            self.eat(&token::CloseDelim(token::Brace));
            true
        } else {
            false
        }
    }

    /// Commit to parsing a complete expression `e` expected to be
    /// followed by some token from the set edible + inedible.  Recover
    /// from anticipated input errors, discarding erroneous characters.
    pub fn commit_expr(&mut self, e: &Expr, edible: &[token::Token], inedible: &[token::Token]) {
        debug!("commit_expr {}", e);
        if let ExprPath(..) = e.node {
            // might be unit-struct construction; check for recoverableinput error.
            let mut expected = edible.iter().map(|x| x.clone()).collect::<Vec<_>>();
            expected.push_all(inedible);
            self.check_for_erroneous_unit_struct_expecting(expected.as_slice());
        }
        self.expect_one_of(edible, inedible)
    }

    pub fn commit_expr_expecting(&mut self, e: &Expr, edible: token::Token) {
        self.commit_expr(e, &[edible], &[])
    }

    /// Commit to parsing a complete statement `s`, which expects to be
    /// followed by some token from the set edible + inedible.  Check
    /// for recoverable input errors, discarding erroneous characters.
    pub fn commit_stmt(&mut self, edible: &[token::Token], inedible: &[token::Token]) {
        if self.last_token
               .as_ref()
               .map_or(false, |t| t.is_ident() || t.is_path()) {
            let mut expected = edible.iter().map(|x| x.clone()).collect::<Vec<_>>();
            expected.push_all(inedible.as_slice());
            self.check_for_erroneous_unit_struct_expecting(
                expected.as_slice());
        }
        self.expect_one_of(edible, inedible)
    }

    pub fn commit_stmt_expecting(&mut self, edible: token::Token) {
        self.commit_stmt(&[edible], &[])
    }

    pub fn parse_ident(&mut self) -> ast::Ident {
        self.check_strict_keywords();
        self.check_reserved_keywords();
        match self.token {
            token::Ident(i, _) => {
                self.bump();
                i
            }
            token::Interpolated(token::NtIdent(..)) => {
                self.bug("ident interpolation not converted to real token");
            }
            _ => {
                let token_str = self.this_token_to_string();
                self.fatal((format!("expected ident, found `{}`",
                                    token_str)).as_slice())
            }
        }
    }

    pub fn parse_path_list_item(&mut self) -> ast::PathListItem {
        let lo = self.span.lo;
        let node = if self.eat_keyword(keywords::Mod) {
            ast::PathListMod { id: ast::DUMMY_NODE_ID }
        } else {
            let ident = self.parse_ident();
            ast::PathListIdent { name: ident, id: ast::DUMMY_NODE_ID }
        };
        let hi = self.last_span.hi;
        spanned(lo, hi, node)
    }

    /// Check if the next token is `tok`, and return `true` if so.
    ///
    /// This method is will automatically add `tok` to `expected_tokens` if `tok` is not
    /// encountered.
    pub fn check(&mut self, tok: &token::Token) -> bool {
        let is_present = self.token == *tok;
        if !is_present { self.expected_tokens.push(TokenType::Token(tok.clone())); }
        is_present
    }

    /// Consume token 'tok' if it exists. Returns true if the given
    /// token was present, false otherwise.
    pub fn eat(&mut self, tok: &token::Token) -> bool {
        let is_present = self.check(tok);
        if is_present { self.bump() }
        is_present
    }

    /// If the next token is the given keyword, eat it and return
    /// true. Otherwise, return false.
    pub fn eat_keyword(&mut self, kw: keywords::Keyword) -> bool {
        if self.token.is_keyword(kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// If the given word is not a keyword, signal an error.
    /// If the next token is not the given word, signal an error.
    /// Otherwise, eat it.
    pub fn expect_keyword(&mut self, kw: keywords::Keyword) {
        if !self.eat_keyword(kw) {
            let id_interned_str = token::get_name(kw.to_name());
            let token_str = self.this_token_to_string();
            self.fatal(format!("expected `{}`, found `{}`",
                               id_interned_str, token_str).as_slice())
        }
    }

    /// Signal an error if the given string is a strict keyword
    pub fn check_strict_keywords(&mut self) {
        if self.token.is_strict_keyword() {
            let token_str = self.this_token_to_string();
            let span = self.span;
            self.span_err(span,
                          format!("expected identifier, found keyword `{}`",
                                  token_str).as_slice());
        }
    }

    /// Signal an error if the current token is a reserved keyword
    pub fn check_reserved_keywords(&mut self) {
        if self.token.is_reserved_keyword() {
            let token_str = self.this_token_to_string();
            self.fatal(format!("`{}` is a reserved keyword",
                               token_str).as_slice())
        }
    }

    /// Expect and consume an `&`. If `&&` is seen, replace it with a single
    /// `&` and continue. If an `&` is not seen, signal an error.
    fn expect_and(&mut self) {
        match self.token {
            token::BinOp(token::And) => self.bump(),
            token::AndAnd => {
                let span = self.span;
                let lo = span.lo + BytePos(1);
                self.replace_token(token::BinOp(token::And), lo, span.hi)
            }
            _ => {
                let token_str = self.this_token_to_string();
                let found_token =
                    Parser::token_to_string(&token::BinOp(token::And));
                self.fatal(format!("expected `{}`, found `{}`",
                                   found_token,
                                   token_str).as_slice())
            }
        }
    }

    /// Expect and consume a `|`. If `||` is seen, replace it with a single
    /// `|` and continue. If a `|` is not seen, signal an error.
    fn expect_or(&mut self) {
        match self.token {
            token::BinOp(token::Or) => self.bump(),
            token::OrOr => {
                let span = self.span;
                let lo = span.lo + BytePos(1);
                self.replace_token(token::BinOp(token::Or), lo, span.hi)
            }
            _ => {
                let found_token = self.this_token_to_string();
                let token_str =
                    Parser::token_to_string(&token::BinOp(token::Or));
                self.fatal(format!("expected `{}`, found `{}`",
                                   token_str,
                                   found_token).as_slice())
            }
        }
    }

    pub fn expect_no_suffix(&mut self, sp: Span, kind: &str, suffix: Option<ast::Name>) {
        match suffix {
            None => {/* everything ok */}
            Some(suf) => {
                let text = suf.as_str();
                if text.is_empty() {
                    self.span_bug(sp, "found empty literal suffix in Some")
                }
                self.span_err(sp, &*format!("{} with a suffix is illegal", kind));
            }
        }
    }


    /// Attempt to consume a `<`. If `<<` is seen, replace it with a single
    /// `<` and continue. If a `<` is not seen, return false.
    ///
    /// This is meant to be used when parsing generics on a path to get the
    /// starting token. The `force` parameter is used to forcefully break up a
    /// `<<` token. If `force` is false, then `<<` is only broken when a lifetime
    /// shows up next. For example, consider the expression:
    ///
    ///      foo as bar << test
    ///
    /// The parser needs to know if `bar <<` is the start of a generic path or if
    /// it's a left-shift token. If `test` were a lifetime, then it's impossible
    /// for the token to be a left-shift, but if it's not a lifetime, then it's
    /// considered a left-shift.
    ///
    /// The reason for this is that the only current ambiguity with `<<` is when
    /// parsing closure types:
    ///
    ///      foo::<<'a> ||>();
    ///      impl Foo<<'a> ||>() { ... }
    fn eat_lt(&mut self, force: bool) -> bool {
        match self.token {
            token::Lt => { self.bump(); true }
            token::BinOp(token::Shl) => {
                let next_lifetime = self.look_ahead(1, |t| match *t {
                    token::Lifetime(..) => true,
                    _ => false,
                });
                if force || next_lifetime {
                    let span = self.span;
                    let lo = span.lo + BytePos(1);
                    self.replace_token(token::Lt, lo, span.hi);
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn expect_lt(&mut self) {
        if !self.eat_lt(true) {
            let found_token = self.this_token_to_string();
            let token_str = Parser::token_to_string(&token::Lt);
            self.fatal(format!("expected `{}`, found `{}`",
                               token_str,
                               found_token).as_slice())
        }
    }

    /// Parse a sequence bracketed by `|` and `|`, stopping before the `|`.
    fn parse_seq_to_before_or<T, F>(&mut self,
                                    sep: &token::Token,
                                    mut f: F)
                                    -> Vec<T> where
        F: FnMut(&mut Parser) -> T,
    {
        let mut first = true;
        let mut vector = Vec::new();
        while self.token != token::BinOp(token::Or) &&
                self.token != token::OrOr {
            if first {
                first = false
            } else {
                self.expect(sep)
            }

            vector.push(f(self))
        }
        vector
    }

    /// Expect and consume a GT. if a >> is seen, replace it
    /// with a single > and continue. If a GT is not seen,
    /// signal an error.
    pub fn expect_gt(&mut self) {
        match self.token {
            token::Gt => self.bump(),
            token::BinOp(token::Shr) => {
                let span = self.span;
                let lo = span.lo + BytePos(1);
                self.replace_token(token::Gt, lo, span.hi)
            }
            token::BinOpEq(token::Shr) => {
                let span = self.span;
                let lo = span.lo + BytePos(1);
                self.replace_token(token::Ge, lo, span.hi)
            }
            token::Ge => {
                let span = self.span;
                let lo = span.lo + BytePos(1);
                self.replace_token(token::Eq, lo, span.hi)
            }
            _ => {
                let gt_str = Parser::token_to_string(&token::Gt);
                let this_token_str = self.this_token_to_string();
                self.fatal(format!("expected `{}`, found `{}`",
                                   gt_str,
                                   this_token_str).as_slice())
            }
        }
    }

    pub fn parse_seq_to_before_gt_or_return<T, F>(&mut self,
                                                  sep: Option<token::Token>,
                                                  mut f: F)
                                                  -> (OwnedSlice<T>, bool) where
        F: FnMut(&mut Parser) -> Option<T>,
    {
        let mut v = Vec::new();
        // This loop works by alternating back and forth between parsing types
        // and commas.  For example, given a string `A, B,>`, the parser would
        // first parse `A`, then a comma, then `B`, then a comma. After that it
        // would encounter a `>` and stop. This lets the parser handle trailing
        // commas in generic parameters, because it can stop either after
        // parsing a type or after parsing a comma.
        for i in iter::count(0u, 1) {
            if self.check(&token::Gt)
                || self.token == token::BinOp(token::Shr)
                || self.token == token::Ge
                || self.token == token::BinOpEq(token::Shr) {
                break;
            }

            if i % 2 == 0 {
                match f(self) {
                    Some(result) => v.push(result),
                    None => return (OwnedSlice::from_vec(v), true)
                }
            } else {
                sep.as_ref().map(|t| self.expect(t));
            }
        }
        return (OwnedSlice::from_vec(v), false);
    }

    /// Parse a sequence bracketed by '<' and '>', stopping
    /// before the '>'.
    pub fn parse_seq_to_before_gt<T, F>(&mut self,
                                        sep: Option<token::Token>,
                                        mut f: F)
                                        -> OwnedSlice<T> where
        F: FnMut(&mut Parser) -> T,
    {
        let (result, returned) = self.parse_seq_to_before_gt_or_return(sep, |p| Some(f(p)));
        assert!(!returned);
        return result;
    }

    pub fn parse_seq_to_gt<T, F>(&mut self,
                                 sep: Option<token::Token>,
                                 f: F)
                                 -> OwnedSlice<T> where
        F: FnMut(&mut Parser) -> T,
    {
        let v = self.parse_seq_to_before_gt(sep, f);
        self.expect_gt();
        return v;
    }

    pub fn parse_seq_to_gt_or_return<T, F>(&mut self,
                                           sep: Option<token::Token>,
                                           f: F)
                                           -> (OwnedSlice<T>, bool) where
        F: FnMut(&mut Parser) -> Option<T>,
    {
        let (v, returned) = self.parse_seq_to_before_gt_or_return(sep, f);
        if !returned {
            self.expect_gt();
        }
        return (v, returned);
    }

    /// Parse a sequence, including the closing delimiter. The function
    /// f must consume tokens until reaching the next separator or
    /// closing bracket.
    pub fn parse_seq_to_end<T, F>(&mut self,
                                  ket: &token::Token,
                                  sep: SeqSep,
                                  f: F)
                                  -> Vec<T> where
        F: FnMut(&mut Parser) -> T,
    {
        let val = self.parse_seq_to_before_end(ket, sep, f);
        self.bump();
        val
    }

    /// Parse a sequence, not including the closing delimiter. The function
    /// f must consume tokens until reaching the next separator or
    /// closing bracket.
    pub fn parse_seq_to_before_end<T, F>(&mut self,
                                         ket: &token::Token,
                                         sep: SeqSep,
                                         mut f: F)
                                         -> Vec<T> where
        F: FnMut(&mut Parser) -> T,
    {
        let mut first: bool = true;
        let mut v = vec!();
        while self.token != *ket {
            match sep.sep {
              Some(ref t) => {
                if first { first = false; }
                else { self.expect(t); }
              }
              _ => ()
            }
            if sep.trailing_sep_allowed && self.check(ket) { break; }
            v.push(f(self));
        }
        return v;
    }

    /// Parse a sequence, including the closing delimiter. The function
    /// f must consume tokens until reaching the next separator or
    /// closing bracket.
    pub fn parse_unspanned_seq<T, F>(&mut self,
                                     bra: &token::Token,
                                     ket: &token::Token,
                                     sep: SeqSep,
                                     f: F)
                                     -> Vec<T> where
        F: FnMut(&mut Parser) -> T,
    {
        self.expect(bra);
        let result = self.parse_seq_to_before_end(ket, sep, f);
        self.bump();
        result
    }

    /// Parse a sequence parameter of enum variant. For consistency purposes,
    /// these should not be empty.
    pub fn parse_enum_variant_seq<T, F>(&mut self,
                                        bra: &token::Token,
                                        ket: &token::Token,
                                        sep: SeqSep,
                                        f: F)
                                        -> Vec<T> where
        F: FnMut(&mut Parser) -> T,
    {
        let result = self.parse_unspanned_seq(bra, ket, sep, f);
        if result.is_empty() {
            let last_span = self.last_span;
            self.span_err(last_span,
            "nullary enum variants are written with no trailing `( )`");
        }
        result
    }

    // NB: Do not use this function unless you actually plan to place the
    // spanned list in the AST.
    pub fn parse_seq<T, F>(&mut self,
                           bra: &token::Token,
                           ket: &token::Token,
                           sep: SeqSep,
                           f: F)
                           -> Spanned<Vec<T>> where
        F: FnMut(&mut Parser) -> T,
    {
        let lo = self.span.lo;
        self.expect(bra);
        let result = self.parse_seq_to_before_end(ket, sep, f);
        let hi = self.span.hi;
        self.bump();
        spanned(lo, hi, result)
    }

    /// Advance the parser by one token
    pub fn bump(&mut self) {
        self.last_span = self.span;
        // Stash token for error recovery (sometimes; clone is not necessarily cheap).
        self.last_token = if self.token.is_ident() || self.token.is_path() {
            Some(box self.token.clone())
        } else {
            None
        };
        let next = if self.buffer_start == self.buffer_end {
            self.reader.real_token()
        } else {
            // Avoid token copies with `replace`.
            let buffer_start = self.buffer_start as uint;
            let next_index = (buffer_start + 1) & 3 as uint;
            self.buffer_start = next_index as int;

            let placeholder = TokenAndSpan {
                tok: token::Underscore,
                sp: self.span,
            };
            mem::replace(&mut self.buffer[buffer_start], placeholder)
        };
        self.span = next.sp;
        self.token = next.tok;
        self.tokens_consumed += 1u;
        self.expected_tokens.clear();
    }

    /// Advance the parser by one token and return the bumped token.
    pub fn bump_and_get(&mut self) -> token::Token {
        let old_token = mem::replace(&mut self.token, token::Underscore);
        self.bump();
        old_token
    }

    /// EFFECT: replace the current token and span with the given one
    pub fn replace_token(&mut self,
                         next: token::Token,
                         lo: BytePos,
                         hi: BytePos) {
        self.last_span = mk_sp(self.span.lo, lo);
        self.token = next;
        self.span = mk_sp(lo, hi);
    }
    pub fn buffer_length(&mut self) -> int {
        if self.buffer_start <= self.buffer_end {
            return self.buffer_end - self.buffer_start;
        }
        return (4 - self.buffer_start) + self.buffer_end;
    }
    pub fn look_ahead<R, F>(&mut self, distance: uint, f: F) -> R where
        F: FnOnce(&token::Token) -> R,
    {
        let dist = distance as int;
        while self.buffer_length() < dist {
            self.buffer[self.buffer_end as uint] = self.reader.real_token();
            self.buffer_end = (self.buffer_end + 1) & 3;
        }
        f(&self.buffer[((self.buffer_start + dist - 1) & 3) as uint].tok)
    }
    pub fn fatal(&mut self, m: &str) -> ! {
        self.sess.span_diagnostic.span_fatal(self.span, m)
    }
    pub fn span_fatal(&mut self, sp: Span, m: &str) -> ! {
        self.sess.span_diagnostic.span_fatal(sp, m)
    }
    pub fn span_fatal_help(&mut self, sp: Span, m: &str, help: &str) -> ! {
        self.span_err(sp, m);
        self.span_help(sp, help);
        panic!(diagnostic::FatalError);
    }
    pub fn span_note(&mut self, sp: Span, m: &str) {
        self.sess.span_diagnostic.span_note(sp, m)
    }
    pub fn span_help(&mut self, sp: Span, m: &str) {
        self.sess.span_diagnostic.span_help(sp, m)
    }
    pub fn bug(&mut self, m: &str) -> ! {
        self.sess.span_diagnostic.span_bug(self.span, m)
    }
    pub fn warn(&mut self, m: &str) {
        self.sess.span_diagnostic.span_warn(self.span, m)
    }
    pub fn span_warn(&mut self, sp: Span, m: &str) {
        self.sess.span_diagnostic.span_warn(sp, m)
    }
    pub fn span_err(&mut self, sp: Span, m: &str) {
        self.sess.span_diagnostic.span_err(sp, m)
    }
    pub fn span_bug(&mut self, sp: Span, m: &str) -> ! {
        self.sess.span_diagnostic.span_bug(sp, m)
    }
    pub fn abort_if_errors(&mut self) {
        self.sess.span_diagnostic.handler().abort_if_errors();
    }

    pub fn id_to_interned_str(&mut self, id: Ident) -> InternedString {
        token::get_ident(id)
    }

    /// Is the current token one of the keywords that signals a bare function
    /// type?
    pub fn token_is_bare_fn_keyword(&mut self) -> bool {
        self.token.is_keyword(keywords::Fn) ||
            self.token.is_keyword(keywords::Unsafe) ||
            self.token.is_keyword(keywords::Extern)
    }

    /// Is the current token one of the keywords that signals a closure type?
    pub fn token_is_closure_keyword(&mut self) -> bool {
        self.token.is_keyword(keywords::Unsafe)
    }

    pub fn get_lifetime(&mut self) -> ast::Ident {
        match self.token {
            token::Lifetime(ref ident) => *ident,
            _ => self.bug("not a lifetime"),
        }
    }

    pub fn parse_for_in_type(&mut self) -> Ty_ {
        /*
        Parses whatever can come after a `for` keyword in a type.
        The `for` has already been consumed.

        Deprecated:

        - for <'lt> |S| -> T
        - for <'lt> proc(S) -> T

        Eventually:

        - for <'lt> [unsafe] [extern "ABI"] fn (S) -> T
        - for <'lt> path::foo(a, b)

        */

        // parse <'lt>
        let lifetime_defs = self.parse_late_bound_lifetime_defs();

        // examine next token to decide to do
        if self.eat_keyword(keywords::Proc) {
            self.parse_proc_type(lifetime_defs)
        } else if self.token_is_bare_fn_keyword() || self.token_is_closure_keyword() {
            self.parse_ty_bare_fn_or_ty_closure(lifetime_defs)
        } else if self.check(&token::ModSep) ||
                  self.token.is_ident() ||
                  self.token.is_path()
        {
            let trait_ref = self.parse_trait_ref();
            let poly_trait_ref = ast::PolyTraitRef { bound_lifetimes: lifetime_defs,
                                                     trait_ref: trait_ref };
            let other_bounds = if self.eat(&token::BinOp(token::Plus)) {
                self.parse_ty_param_bounds()
            } else {
                OwnedSlice::empty()
            };
            let all_bounds =
                Some(TraitTyParamBound(poly_trait_ref)).into_iter()
                .chain(other_bounds.into_vec().into_iter())
                .collect();
            ast::TyPolyTraitRef(all_bounds)
        } else {
            self.parse_ty_closure(lifetime_defs)
        }
    }

    pub fn parse_ty_path(&mut self) -> Ty_ {
        let path = self.parse_path(LifetimeAndTypesWithoutColons);
        TyPath(path, ast::DUMMY_NODE_ID)
    }

    /// parse a TyBareFn type:
    pub fn parse_ty_bare_fn(&mut self, lifetime_defs: Vec<ast::LifetimeDef>) -> Ty_ {
        /*

        [unsafe] [extern "ABI"] fn <'lt> (S) -> T
         ^~~~^           ^~~~^     ^~~~^ ^~^    ^
           |               |         |    |     |
           |               |         |    |   Return type
           |               |         |  Argument types
           |               |     Lifetimes
           |              ABI
        Function Style
        */

        let fn_style = self.parse_unsafety();
        let abi = if self.eat_keyword(keywords::Extern) {
            self.parse_opt_abi().unwrap_or(abi::C)
        } else {
            abi::Rust
        };

        self.expect_keyword(keywords::Fn);
        let lifetime_defs = self.parse_legacy_lifetime_defs(lifetime_defs);
        let (inputs, variadic) = self.parse_fn_args(false, true);
        let ret_ty = self.parse_ret_ty();
        let decl = P(FnDecl {
            inputs: inputs,
            output: ret_ty,
            variadic: variadic
        });
        TyBareFn(P(BareFnTy {
            abi: abi,
            fn_style: fn_style,
            lifetimes: lifetime_defs,
            decl: decl
        }))
    }

    /// Parses a procedure type (`proc`). The initial `proc` keyword must
    /// already have been parsed.
    pub fn parse_proc_type(&mut self, lifetime_defs: Vec<ast::LifetimeDef>) -> Ty_ {
        /*

        proc <'lt> (S) [:Bounds] -> T
        ^~~^ ^~~~^  ^  ^~~~~~~~^    ^
         |     |    |      |        |
         |     |    |      |      Return type
         |     |    |    Bounds
         |     |  Argument types
         |   Legacy lifetimes
        the `proc` keyword

        */

        let lifetime_defs = self.parse_legacy_lifetime_defs(lifetime_defs);
        let (inputs, variadic) = self.parse_fn_args(false, false);
        let bounds = self.parse_colon_then_ty_param_bounds();
        let ret_ty = self.parse_ret_ty();
        let decl = P(FnDecl {
            inputs: inputs,
            output: ret_ty,
            variadic: variadic
        });
        TyProc(P(ClosureTy {
            fn_style: NormalFn,
            onceness: Once,
            bounds: bounds,
            decl: decl,
            lifetimes: lifetime_defs,
        }))
    }

    /// Parses an optional unboxed closure kind (`&:`, `&mut:`, or `:`).
    pub fn parse_optional_unboxed_closure_kind(&mut self)
                                               -> Option<UnboxedClosureKind> {
        if self.check(&token::BinOp(token::And)) &&
                self.look_ahead(1, |t| t.is_keyword(keywords::Mut)) &&
                self.look_ahead(2, |t| *t == token::Colon) {
            self.bump();
            self.bump();
            self.bump();
            return Some(FnMutUnboxedClosureKind)
        }

        if self.token == token::BinOp(token::And) &&
                    self.look_ahead(1, |t| *t == token::Colon) {
            self.bump();
            self.bump();
            return Some(FnUnboxedClosureKind)
        }

        if self.eat(&token::Colon) {
            return Some(FnOnceUnboxedClosureKind)
        }

        return None
    }

    pub fn parse_ty_bare_fn_or_ty_closure(&mut self, lifetime_defs: Vec<LifetimeDef>) -> Ty_ {
        // Both bare fns and closures can begin with stuff like unsafe
        // and extern. So we just scan ahead a few tokens to see if we see
        // a `fn`.
        //
        // Closure:  [unsafe] <'lt> |S| [:Bounds] -> T
        // Fn:       [unsafe] [extern "ABI"] fn <'lt> (S) -> T

        if self.token.is_keyword(keywords::Fn) {
            self.parse_ty_bare_fn(lifetime_defs)
        } else if self.token.is_keyword(keywords::Extern) {
            self.parse_ty_bare_fn(lifetime_defs)
        } else if self.token.is_keyword(keywords::Unsafe) {
            if self.look_ahead(1, |t| t.is_keyword(keywords::Fn) ||
                                      t.is_keyword(keywords::Extern)) {
                self.parse_ty_bare_fn(lifetime_defs)
            } else {
                self.parse_ty_closure(lifetime_defs)
            }
        } else {
            self.parse_ty_closure(lifetime_defs)
        }
    }

    /// Parse a TyClosure type
    pub fn parse_ty_closure(&mut self, lifetime_defs: Vec<ast::LifetimeDef>) -> Ty_ {
        /*

        [unsafe] <'lt> |S| [:Bounds] -> T
        ^~~~~~~^ ^~~~^  ^  ^~~~~~~~^    ^
          |        |       |      |        |
          |        |       |      |      Return type
          |        |       |  Closure bounds
          |        |     Argument types
          |      Deprecated lifetime defs
          |
        Function Style

        */

        let fn_style = self.parse_unsafety();

        let lifetime_defs = self.parse_legacy_lifetime_defs(lifetime_defs);

        let inputs = if self.eat(&token::OrOr) {
            Vec::new()
        } else {
            self.expect_or();

            let inputs = self.parse_seq_to_before_or(
                &token::Comma,
                |p| p.parse_arg_general(false));
            self.expect_or();
            inputs
        };

        let bounds = self.parse_colon_then_ty_param_bounds();

        let output = self.parse_ret_ty();
        let decl = P(FnDecl {
            inputs: inputs,
            output: output,
            variadic: false
        });

        TyClosure(P(ClosureTy {
            fn_style: fn_style,
            onceness: Many,
            bounds: bounds,
            decl: decl,
            lifetimes: lifetime_defs,
        }))
    }

    pub fn parse_unsafety(&mut self) -> FnStyle {
        if self.eat_keyword(keywords::Unsafe) {
            return UnsafeFn;
        } else {
            return NormalFn;
        }
    }

    /// Parses `[ 'for' '<' lifetime_defs '>' ]'
    fn parse_legacy_lifetime_defs(&mut self,
                                  lifetime_defs: Vec<ast::LifetimeDef>)
                                  -> Vec<ast::LifetimeDef>
    {
        if self.token == token::Lt {
            self.bump();
            if lifetime_defs.is_empty() {
                self.warn("deprecated syntax; use the `for` keyword now \
                            (e.g. change `fn<'a>` to `for<'a> fn`)");
                let lifetime_defs = self.parse_lifetime_defs();
                self.expect_gt();
                lifetime_defs
            } else {
                self.fatal("cannot use new `for` keyword and older syntax together");
            }
        } else {
            lifetime_defs
        }
    }

    /// Parses `type Foo;` in a trait declaration only. The `type` keyword has
    /// already been parsed.
    fn parse_associated_type(&mut self, attrs: Vec<Attribute>)
                             -> AssociatedType
    {
        let ty_param = self.parse_ty_param();
        self.expect(&token::Semi);
        AssociatedType {
            attrs: attrs,
            ty_param: ty_param,
        }
    }

    /// Parses `type Foo = TYPE;` in an implementation declaration only. The
    /// `type` keyword has already been parsed.
    fn parse_typedef(&mut self, attrs: Vec<Attribute>, vis: Visibility)
                     -> Typedef {
        let lo = self.span.lo;
        let ident = self.parse_ident();
        self.expect(&token::Eq);
        let typ = self.parse_ty_sum();
        let hi = self.span.hi;
        self.expect(&token::Semi);
        Typedef {
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, hi),
            ident: ident,
            vis: vis,
            attrs: attrs,
            typ: typ,
        }
    }

    /// Parse the items in a trait declaration
    pub fn parse_trait_items(&mut self) -> Vec<TraitItem> {
        self.parse_unspanned_seq(
            &token::OpenDelim(token::Brace),
            &token::CloseDelim(token::Brace),
            seq_sep_none(),
            |p| {
            let attrs = p.parse_outer_attributes();

            if p.eat_keyword(keywords::Type) {
                TypeTraitItem(P(p.parse_associated_type(attrs)))
            } else {
                let lo = p.span.lo;

                let vis = p.parse_visibility();
                let style = p.parse_fn_style();
                let abi = if p.eat_keyword(keywords::Extern) {
                    p.parse_opt_abi().unwrap_or(abi::C)
                } else {
                    abi::Rust
                };
                p.expect_keyword(keywords::Fn);

                let ident = p.parse_ident();
                let mut generics = p.parse_generics();

                let (explicit_self, d) = p.parse_fn_decl_with_self(|p| {
                    // This is somewhat dubious; We don't want to allow
                    // argument names to be left off if there is a
                    // definition...
                    p.parse_arg_general(false)
                });

                p.parse_where_clause(&mut generics);

                let hi = p.last_span.hi;
                match p.token {
                  token::Semi => {
                    p.bump();
                    debug!("parse_trait_methods(): parsing required method");
                    RequiredMethod(TypeMethod {
                        ident: ident,
                        attrs: attrs,
                        fn_style: style,
                        decl: d,
                        generics: generics,
                        abi: abi,
                        explicit_self: explicit_self,
                        id: ast::DUMMY_NODE_ID,
                        span: mk_sp(lo, hi),
                        vis: vis,
                    })
                  }
                  token::OpenDelim(token::Brace) => {
                    debug!("parse_trait_methods(): parsing provided method");
                    let (inner_attrs, body) =
                        p.parse_inner_attrs_and_block();
                    let mut attrs = attrs;
                    attrs.push_all(inner_attrs.as_slice());
                    ProvidedMethod(P(ast::Method {
                        attrs: attrs,
                        id: ast::DUMMY_NODE_ID,
                        span: mk_sp(lo, hi),
                        node: ast::MethDecl(ident,
                                            generics,
                                            abi,
                                            explicit_self,
                                            style,
                                            d,
                                            body,
                                            vis)
                    }))
                  }

                  _ => {
                      let token_str = p.this_token_to_string();
                      p.fatal((format!("expected `;` or `{{`, found `{}`",
                                       token_str)).as_slice())
                  }
                }
            }
        })
    }

    /// Parse a possibly mutable type
    pub fn parse_mt(&mut self) -> MutTy {
        let mutbl = self.parse_mutability();
        let t = self.parse_ty();
        MutTy { ty: t, mutbl: mutbl }
    }

    /// Parse [mut/const/imm] ID : TY
    /// now used only by obsolete record syntax parser...
    pub fn parse_ty_field(&mut self) -> TypeField {
        let lo = self.span.lo;
        let mutbl = self.parse_mutability();
        let id = self.parse_ident();
        self.expect(&token::Colon);
        let ty = self.parse_ty_sum();
        let hi = ty.span.hi;
        ast::TypeField {
            ident: id,
            mt: MutTy { ty: ty, mutbl: mutbl },
            span: mk_sp(lo, hi),
        }
    }

    /// Parse optional return type [ -> TY ] in function decl
    pub fn parse_ret_ty(&mut self) -> FunctionRetTy {
        if self.eat(&token::RArrow) {
            if self.eat(&token::Not) {
                NoReturn(self.span)
            } else {
                let t = self.parse_ty();

                // We used to allow `fn foo() -> &T + U`, but don't
                // anymore. If we see it, report a useful error.  This
                // only makes sense because `parse_ret_ty` is only
                // used in fn *declarations*, not fn types or where
                // clauses (i.e., not when parsing something like
                // `FnMut() -> T + Send`, where the `+` is legal).
                if self.token == token::BinOp(token::Plus) {
                    self.warn("deprecated syntax: `()` are required, see RFC 248 for details");
                }

                Return(t)
            }
        } else {
            let pos = self.span.lo;
            Return(P(Ty {
                id: ast::DUMMY_NODE_ID,
                node: TyTup(vec![]),
                span: mk_sp(pos, pos),
            }))
        }
    }

    /// Parse a type in a context where `T1+T2` is allowed.
    pub fn parse_ty_sum(&mut self) -> P<Ty> {
        let lo = self.span.lo;
        let lhs = self.parse_ty();

        if !self.eat(&token::BinOp(token::Plus)) {
            return lhs;
        }

        let bounds = self.parse_ty_param_bounds();

        // In type grammar, `+` is treated like a binary operator,
        // and hence both L and R side are required.
        if bounds.len() == 0 {
            let last_span = self.last_span;
            self.span_err(last_span,
                          "at least one type parameter bound \
                          must be specified");
        }

        let sp = mk_sp(lo, self.last_span.hi);
        let sum = ast::TyObjectSum(lhs, bounds);
        P(Ty {id: ast::DUMMY_NODE_ID, node: sum, span: sp})
    }

    /// Parse a type.
    ///
    /// The second parameter specifies whether the `+` binary operator is
    /// allowed in the type grammar.
    pub fn parse_ty(&mut self) -> P<Ty> {
        maybe_whole!(no_clone self, NtTy);

        let lo = self.span.lo;

        let t = if self.check(&token::OpenDelim(token::Paren)) {
            self.bump();

            // (t) is a parenthesized ty
            // (t,) is the type of a tuple with only one field,
            // of type t
            let mut ts = vec![];
            let mut last_comma = false;
            while self.token != token::CloseDelim(token::Paren) {
                ts.push(self.parse_ty_sum());
                if self.check(&token::Comma) {
                    last_comma = true;
                    self.bump();
                } else {
                    last_comma = false;
                    break;
                }
            }

            self.expect(&token::CloseDelim(token::Paren));
            if ts.len() == 1 && !last_comma {
                TyParen(ts.into_iter().nth(0).unwrap())
            } else {
                TyTup(ts)
            }
        } else if self.token == token::Tilde {
            // OWNED POINTER
            self.bump();
            let last_span = self.last_span;
            match self.token {
                token::OpenDelim(token::Bracket) => self.obsolete(last_span, ObsoleteOwnedVector),
                _ => self.obsolete(last_span, ObsoleteOwnedType)
            }
            TyTup(vec![self.parse_ty()])
        } else if self.check(&token::BinOp(token::Star)) {
            // STAR POINTER (bare pointer?)
            self.bump();
            TyPtr(self.parse_ptr())
        } else if self.check(&token::OpenDelim(token::Bracket)) {
            // VECTOR
            self.expect(&token::OpenDelim(token::Bracket));
            let t = self.parse_ty_sum();

            // Parse the `, ..e` in `[ int, ..e ]`
            // where `e` is a const expression
            let t = match self.maybe_parse_fixed_vstore() {
                None => TyVec(t),
                Some(suffix) => TyFixedLengthVec(t, suffix)
            };
            self.expect(&token::CloseDelim(token::Bracket));
            t
        } else if self.check(&token::BinOp(token::And)) ||
                  self.token == token::AndAnd {
            // BORROWED POINTER
            self.expect_and();
            self.parse_borrowed_pointee()
        } else if self.token.is_keyword(keywords::For) {
            self.parse_for_in_type()
        } else if self.token_is_bare_fn_keyword() ||
                  self.token_is_closure_keyword() {
            // BARE FUNCTION OR CLOSURE
            self.parse_ty_bare_fn_or_ty_closure(Vec::new())
        } else if self.check(&token::BinOp(token::Or)) ||
                  self.token == token::OrOr ||
                  (self.token == token::Lt &&
                   self.look_ahead(1, |t| {
                       *t == token::Gt || t.is_lifetime()
                   })) {
            // CLOSURE
            self.parse_ty_closure(Vec::new())
        } else if self.eat_keyword(keywords::Typeof) {
            // TYPEOF
            // In order to not be ambiguous, the type must be surrounded by parens.
            self.expect(&token::OpenDelim(token::Paren));
            let e = self.parse_expr();
            self.expect(&token::CloseDelim(token::Paren));
            TyTypeof(e)
        } else if self.eat_keyword(keywords::Proc) {
            self.parse_proc_type(Vec::new())
        } else if self.check(&token::Lt) {
            // QUALIFIED PATH `<TYPE as TRAIT_REF>::item`
            self.bump();
            let self_type = self.parse_ty_sum();
            self.expect_keyword(keywords::As);
            let trait_ref = self.parse_trait_ref();
            self.expect(&token::Gt);
            self.expect(&token::ModSep);
            let item_name = self.parse_ident();
            TyQPath(P(QPath {
                self_type: self_type,
                trait_ref: P(trait_ref),
                item_name: item_name,
            }))
        } else if self.check(&token::ModSep) ||
                  self.token.is_ident() ||
                  self.token.is_path() {
            // NAMED TYPE
            self.parse_ty_path()
        } else if self.eat(&token::Underscore) {
            // TYPE TO BE INFERRED
            TyInfer
        } else {
            let this_token_str = self.this_token_to_string();
            let msg = format!("expected type, found `{}`", this_token_str);
            self.fatal(msg.as_slice());
        };

        let sp = mk_sp(lo, self.last_span.hi);
        P(Ty {id: ast::DUMMY_NODE_ID, node: t, span: sp})
    }

    pub fn parse_borrowed_pointee(&mut self) -> Ty_ {
        // look for `&'lt` or `&'foo ` and interpret `foo` as the region name:
        let opt_lifetime = self.parse_opt_lifetime();

        let mt = self.parse_mt();
        return TyRptr(opt_lifetime, mt);
    }

    pub fn parse_ptr(&mut self) -> MutTy {
        let mutbl = if self.eat_keyword(keywords::Mut) {
            MutMutable
        } else if self.eat_keyword(keywords::Const) {
            MutImmutable
        } else {
            let span = self.last_span;
            self.span_err(span,
                          "bare raw pointers are no longer allowed, you should \
                           likely use `*mut T`, but otherwise `*T` is now \
                           known as `*const T`");
            MutImmutable
        };
        let t = self.parse_ty();
        MutTy { ty: t, mutbl: mutbl }
    }

    pub fn is_named_argument(&mut self) -> bool {
        let offset = match self.token {
            token::BinOp(token::And) => 1,
            token::AndAnd => 1,
            _ if self.token.is_keyword(keywords::Mut) => 1,
            _ => 0
        };

        debug!("parser is_named_argument offset:{}", offset);

        if offset == 0 {
            is_plain_ident_or_underscore(&self.token)
                && self.look_ahead(1, |t| *t == token::Colon)
        } else {
            self.look_ahead(offset, |t| is_plain_ident_or_underscore(t))
                && self.look_ahead(offset + 1, |t| *t == token::Colon)
        }
    }

    /// This version of parse arg doesn't necessarily require
    /// identifier names.
    pub fn parse_arg_general(&mut self, require_name: bool) -> Arg {
        let pat = if require_name || self.is_named_argument() {
            debug!("parse_arg_general parse_pat (require_name:{})",
                   require_name);
            let pat = self.parse_pat();

            self.expect(&token::Colon);
            pat
        } else {
            debug!("parse_arg_general ident_to_pat");
            ast_util::ident_to_pat(ast::DUMMY_NODE_ID,
                                   self.last_span,
                                   special_idents::invalid)
        };

        let t = self.parse_ty_sum();

        Arg {
            ty: t,
            pat: pat,
            id: ast::DUMMY_NODE_ID,
        }
    }

    /// Parse a single function argument
    pub fn parse_arg(&mut self) -> Arg {
        self.parse_arg_general(true)
    }

    /// Parse an argument in a lambda header e.g. |arg, arg|
    pub fn parse_fn_block_arg(&mut self) -> Arg {
        let pat = self.parse_pat();
        let t = if self.eat(&token::Colon) {
            self.parse_ty_sum()
        } else {
            P(Ty {
                id: ast::DUMMY_NODE_ID,
                node: TyInfer,
                span: mk_sp(self.span.lo, self.span.hi),
            })
        };
        Arg {
            ty: t,
            pat: pat,
            id: ast::DUMMY_NODE_ID
        }
    }

    pub fn maybe_parse_fixed_vstore(&mut self) -> Option<P<ast::Expr>> {
        if self.check(&token::Comma) &&
                self.look_ahead(1, |t| *t == token::DotDot) {
            self.bump();
            self.bump();
            Some(self.parse_expr())
        } else {
            None
        }
    }

    /// Matches token_lit = LIT_INTEGER | ...
    pub fn lit_from_token(&mut self, tok: &token::Token) -> Lit_ {
        match *tok {
            token::Interpolated(token::NtExpr(ref v)) => {
                match v.node {
                    ExprLit(ref lit) => { lit.node.clone() }
                    _ => { self.unexpected_last(tok); }
                }
            }
            token::Literal(lit, suf) => {
                let (suffix_illegal, out) = match lit {
                    token::Byte(i) => (true, LitByte(parse::byte_lit(i.as_str()).0)),
                    token::Char(i) => (true, LitChar(parse::char_lit(i.as_str()).0)),

                    // there are some valid suffixes for integer and
                    // float literals, so all the handling is done
                    // internally.
                    token::Integer(s) => {
                        (false, parse::integer_lit(s.as_str(),
                                                   suf.as_ref().map(|s| s.as_str()),
                                                   &self.sess.span_diagnostic,
                                                   self.last_span))
                    }
                    token::Float(s) => {
                        (false, parse::float_lit(s.as_str(),
                                                 suf.as_ref().map(|s| s.as_str()),
                                                  &self.sess.span_diagnostic,
                                                 self.last_span))
                    }

                    token::Str_(s) => {
                        (true,
                         LitStr(token::intern_and_get_ident(parse::str_lit(s.as_str()).as_slice()),
                                ast::CookedStr))
                    }
                    token::StrRaw(s, n) => {
                        (true,
                         LitStr(
                            token::intern_and_get_ident(
                                parse::raw_str_lit(s.as_str()).as_slice()),
                            ast::RawStr(n)))
                    }
                    token::Binary(i) =>
                        (true, LitBinary(parse::binary_lit(i.as_str()))),
                    token::BinaryRaw(i, _) =>
                        (true,
                         LitBinary(Rc::new(i.as_str().as_bytes().iter().map(|&x| x).collect()))),
                };

                if suffix_illegal {
                    let sp = self.last_span;
                    self.expect_no_suffix(sp, &*format!("{} literal", lit.short_name()), suf)
                }

                out
            }
            _ => { self.unexpected_last(tok); }
        }
    }

    /// Matches lit = true | false | token_lit
    pub fn parse_lit(&mut self) -> Lit {
        let lo = self.span.lo;
        let lit = if self.eat_keyword(keywords::True) {
            LitBool(true)
        } else if self.eat_keyword(keywords::False) {
            LitBool(false)
        } else {
            let token = self.bump_and_get();
            let lit = self.lit_from_token(&token);
            lit
        };
        codemap::Spanned { node: lit, span: mk_sp(lo, self.last_span.hi) }
    }

    /// matches '-' lit | lit
    pub fn parse_literal_maybe_minus(&mut self) -> P<Expr> {
        let minus_lo = self.span.lo;
        let minus_present = self.eat(&token::BinOp(token::Minus));

        let lo = self.span.lo;
        let literal = P(self.parse_lit());
        let hi = self.span.hi;
        let expr = self.mk_expr(lo, hi, ExprLit(literal));

        if minus_present {
            let minus_hi = self.span.hi;
            let unary = self.mk_unary(UnNeg, expr);
            self.mk_expr(minus_lo, minus_hi, unary)
        } else {
            expr
        }
    }

    /// Parses a path and optional type parameter bounds, depending on the
    /// mode. The `mode` parameter determines whether lifetimes, types, and/or
    /// bounds are permitted and whether `::` must precede type parameter
    /// groups.
    pub fn parse_path(&mut self, mode: PathParsingMode) -> ast::Path {
        // Check for a whole path...
        let found = match self.token {
            token::Interpolated(token::NtPath(_)) => Some(self.bump_and_get()),
            _ => None,
        };
        if let Some(token::Interpolated(token::NtPath(box path))) = found {
            return path;
        }

        let lo = self.span.lo;
        let is_global = self.eat(&token::ModSep);

        // Parse any number of segments and bound sets. A segment is an
        // identifier followed by an optional lifetime and a set of types.
        // A bound set is a set of type parameter bounds.
        let segments = match mode {
            LifetimeAndTypesWithoutColons => {
                self.parse_path_segments_without_colons()
            }
            LifetimeAndTypesWithColons => {
                self.parse_path_segments_with_colons()
            }
            NoTypesAllowed => {
                self.parse_path_segments_without_types()
            }
        };

        // Assemble the span.
        let span = mk_sp(lo, self.last_span.hi);

        // Assemble the result.
        ast::Path {
            span: span,
            global: is_global,
            segments: segments,
        }
    }

    /// Examples:
    /// - `a::b<T,U>::c<V,W>`
    /// - `a::b<T,U>::c(V) -> W`
    /// - `a::b<T,U>::c(V)`
    pub fn parse_path_segments_without_colons(&mut self) -> Vec<ast::PathSegment> {
        let mut segments = Vec::new();
        loop {
            // First, parse an identifier.
            let identifier = self.parse_ident();

            // Parse types, optionally.
            let parameters = if self.eat_lt(false) {
                let (lifetimes, types, bindings) = self.parse_generic_values_after_lt();

                ast::AngleBracketedParameters(ast::AngleBracketedParameterData {
                    lifetimes: lifetimes,
                    types: OwnedSlice::from_vec(types),
                    bindings: OwnedSlice::from_vec(bindings),
                })
            } else if self.eat(&token::OpenDelim(token::Paren)) {
                let inputs = self.parse_seq_to_end(
                    &token::CloseDelim(token::Paren),
                    seq_sep_trailing_allowed(token::Comma),
                    |p| p.parse_ty_sum());

                let output_ty = if self.eat(&token::RArrow) {
                    Some(self.parse_ty())
                } else {
                    None
                };

                ast::ParenthesizedParameters(ast::ParenthesizedParameterData {
                    inputs: inputs,
                    output: output_ty
                })
            } else {
                ast::PathParameters::none()
            };

            // Assemble and push the result.
            segments.push(ast::PathSegment { identifier: identifier,
                                             parameters: parameters });

            // Continue only if we see a `::`
            if !self.eat(&token::ModSep) {
                return segments;
            }
        }
    }

    /// Examples:
    /// - `a::b::<T,U>::c`
    pub fn parse_path_segments_with_colons(&mut self) -> Vec<ast::PathSegment> {
        let mut segments = Vec::new();
        loop {
            // First, parse an identifier.
            let identifier = self.parse_ident();

            // If we do not see a `::`, stop.
            if !self.eat(&token::ModSep) {
                segments.push(ast::PathSegment {
                    identifier: identifier,
                    parameters: ast::AngleBracketedParameters(ast::AngleBracketedParameterData {
                        lifetimes: Vec::new(),
                        types: OwnedSlice::empty(),
                        bindings: OwnedSlice::empty(),
                    })
                });
                return segments;
            }

            // Check for a type segment.
            if self.eat_lt(false) {
                // Consumed `a::b::<`, go look for types
                let (lifetimes, types, bindings) = self.parse_generic_values_after_lt();
                segments.push(ast::PathSegment {
                    identifier: identifier,
                    parameters: ast::AngleBracketedParameters(ast::AngleBracketedParameterData {
                        lifetimes: lifetimes,
                        types: OwnedSlice::from_vec(types),
                        bindings: OwnedSlice::from_vec(bindings),
                    }),
                });

                // Consumed `a::b::<T,U>`, check for `::` before proceeding
                if !self.eat(&token::ModSep) {
                    return segments;
                }
            } else {
                // Consumed `a::`, go look for `b`
                segments.push(ast::PathSegment {
                    identifier: identifier,
                    parameters: ast::PathParameters::none(),
                });
            }
        }
    }


    /// Examples:
    /// - `a::b::c`
    pub fn parse_path_segments_without_types(&mut self) -> Vec<ast::PathSegment> {
        let mut segments = Vec::new();
        loop {
            // First, parse an identifier.
            let identifier = self.parse_ident();

            // Assemble and push the result.
            segments.push(ast::PathSegment {
                identifier: identifier,
                parameters: ast::PathParameters::none()
            });

            // If we do not see a `::`, stop.
            if !self.eat(&token::ModSep) {
                return segments;
            }
        }
    }

    /// parses 0 or 1 lifetime
    pub fn parse_opt_lifetime(&mut self) -> Option<ast::Lifetime> {
        match self.token {
            token::Lifetime(..) => {
                Some(self.parse_lifetime())
            }
            _ => {
                None
            }
        }
    }

    /// Parses a single lifetime
    /// Matches lifetime = LIFETIME
    pub fn parse_lifetime(&mut self) -> ast::Lifetime {
        match self.token {
            token::Lifetime(i) => {
                let span = self.span;
                self.bump();
                return ast::Lifetime {
                    id: ast::DUMMY_NODE_ID,
                    span: span,
                    name: i.name
                };
            }
            _ => {
                self.fatal(format!("expected a lifetime name").as_slice());
            }
        }
    }

    /// Parses `lifetime_defs = [ lifetime_defs { ',' lifetime_defs } ]` where `lifetime_def  =
    /// lifetime [':' lifetimes]`
    pub fn parse_lifetime_defs(&mut self) -> Vec<ast::LifetimeDef> {

        let mut res = Vec::new();
        loop {
            match self.token {
                token::Lifetime(_) => {
                    let lifetime = self.parse_lifetime();
                    let bounds =
                        if self.eat(&token::Colon) {
                            self.parse_lifetimes(token::BinOp(token::Plus))
                        } else {
                            Vec::new()
                        };
                    res.push(ast::LifetimeDef { lifetime: lifetime,
                                                bounds: bounds });
                }

                _ => {
                    return res;
                }
            }

            match self.token {
                token::Comma => { self.bump(); }
                token::Gt => { return res; }
                token::BinOp(token::Shr) => { return res; }
                _ => {
                    let this_token_str = self.this_token_to_string();
                    let msg = format!("expected `,` or `>` after lifetime \
                                      name, found `{}`",
                                      this_token_str);
                    self.fatal(msg.as_slice());
                }
            }
        }
    }

    /// matches lifetimes = ( lifetime ) | ( lifetime , lifetimes ) actually, it matches the empty
    /// one too, but putting that in there messes up the grammar....
    ///
    /// Parses zero or more comma separated lifetimes. Expects each lifetime to be followed by
    /// either a comma or `>`.  Used when parsing type parameter lists, where we expect something
    /// like `<'a, 'b, T>`.
    pub fn parse_lifetimes(&mut self, sep: token::Token) -> Vec<ast::Lifetime> {

        let mut res = Vec::new();
        loop {
            match self.token {
                token::Lifetime(_) => {
                    res.push(self.parse_lifetime());
                }
                _ => {
                    return res;
                }
            }

            if self.token != sep {
                return res;
            }

            self.bump();
        }
    }

    /// Parse mutability declaration (mut/const/imm)
    pub fn parse_mutability(&mut self) -> Mutability {
        if self.eat_keyword(keywords::Mut) {
            MutMutable
        } else {
            MutImmutable
        }
    }

    /// Parse ident COLON expr
    pub fn parse_field(&mut self) -> Field {
        let lo = self.span.lo;
        let i = self.parse_ident();
        let hi = self.last_span.hi;
        self.expect(&token::Colon);
        let e = self.parse_expr();
        ast::Field {
            ident: spanned(lo, hi, i),
            span: mk_sp(lo, e.span.hi),
            expr: e,
        }
    }

    pub fn mk_expr(&mut self, lo: BytePos, hi: BytePos, node: Expr_) -> P<Expr> {
        P(Expr {
            id: ast::DUMMY_NODE_ID,
            node: node,
            span: mk_sp(lo, hi),
        })
    }

    pub fn mk_unary(&mut self, unop: ast::UnOp, expr: P<Expr>) -> ast::Expr_ {
        ExprUnary(unop, expr)
    }

    pub fn mk_binary(&mut self, binop: ast::BinOp, lhs: P<Expr>, rhs: P<Expr>) -> ast::Expr_ {
        ExprBinary(binop, lhs, rhs)
    }

    pub fn mk_call(&mut self, f: P<Expr>, args: Vec<P<Expr>>) -> ast::Expr_ {
        ExprCall(f, args)
    }

    fn mk_method_call(&mut self,
                      ident: ast::SpannedIdent,
                      tps: Vec<P<Ty>>,
                      args: Vec<P<Expr>>)
                      -> ast::Expr_ {
        ExprMethodCall(ident, tps, args)
    }

    pub fn mk_index(&mut self, expr: P<Expr>, idx: P<Expr>) -> ast::Expr_ {
        ExprIndex(expr, idx)
    }

    pub fn mk_slice(&mut self, expr: P<Expr>,
                    start: Option<P<Expr>>,
                    end: Option<P<Expr>>,
                    mutbl: Mutability)
                    -> ast::Expr_ {
        ExprSlice(expr, start, end, mutbl)
    }

    pub fn mk_field(&mut self, expr: P<Expr>, ident: ast::SpannedIdent) -> ast::Expr_ {
        ExprField(expr, ident)
    }

    pub fn mk_tup_field(&mut self, expr: P<Expr>, idx: codemap::Spanned<uint>) -> ast::Expr_ {
        ExprTupField(expr, idx)
    }

    pub fn mk_assign_op(&mut self, binop: ast::BinOp,
                        lhs: P<Expr>, rhs: P<Expr>) -> ast::Expr_ {
        ExprAssignOp(binop, lhs, rhs)
    }

    pub fn mk_mac_expr(&mut self, lo: BytePos, hi: BytePos, m: Mac_) -> P<Expr> {
        P(Expr {
            id: ast::DUMMY_NODE_ID,
            node: ExprMac(codemap::Spanned {node: m, span: mk_sp(lo, hi)}),
            span: mk_sp(lo, hi),
        })
    }

    pub fn mk_lit_u32(&mut self, i: u32) -> P<Expr> {
        let span = &self.span;
        let lv_lit = P(codemap::Spanned {
            node: LitInt(i as u64, ast::UnsignedIntLit(TyU32)),
            span: *span
        });

        P(Expr {
            id: ast::DUMMY_NODE_ID,
            node: ExprLit(lv_lit),
            span: *span,
        })
    }

    fn expect_open_delim(&mut self) -> token::DelimToken {
        match self.token {
            token::OpenDelim(delim) => {
                self.bump();
                delim
            },
            _ => self.fatal("expected open delimiter"),
        }
    }

    /// At the bottom (top?) of the precedence hierarchy,
    /// parse things like parenthesized exprs,
    /// macros, return, etc.
    pub fn parse_bottom_expr(&mut self) -> P<Expr> {
        maybe_whole_expr!(self);

        let lo = self.span.lo;
        let mut hi = self.span.hi;

        let ex: Expr_;

        match self.token {
            token::OpenDelim(token::Paren) => {
                self.bump();

                // (e) is parenthesized e
                // (e,) is a tuple with only one field, e
                let mut es = vec![];
                let mut trailing_comma = false;
                while self.token != token::CloseDelim(token::Paren) {
                    es.push(self.parse_expr());
                    self.commit_expr(&**es.last().unwrap(), &[],
                                     &[token::Comma, token::CloseDelim(token::Paren)]);
                    if self.check(&token::Comma) {
                        trailing_comma = true;

                        self.bump();
                    } else {
                        trailing_comma = false;
                        break;
                    }
                }
                self.bump();

                hi = self.span.hi;
                return if es.len() == 1 && !trailing_comma {
                    self.mk_expr(lo, hi, ExprParen(es.into_iter().nth(0).unwrap()))
                } else {
                    self.mk_expr(lo, hi, ExprTup(es))
                }
            },
            token::OpenDelim(token::Brace) => {
                self.bump();
                let blk = self.parse_block_tail(lo, DefaultBlock);
                return self.mk_expr(blk.span.lo, blk.span.hi,
                                    ExprBlock(blk));
            },
            token::BinOp(token::Or) |  token::OrOr => {
                return self.parse_lambda_expr(CaptureByRef);
            },
            // FIXME #13626: Should be able to stick in
            // token::SELF_KEYWORD_NAME
            token::Ident(id @ ast::Ident {
                            name: ast::Name(token::SELF_KEYWORD_NAME_NUM),
                            ctxt: _
                         }, token::Plain) => {
                self.bump();
                let path = ast_util::ident_to_path(mk_sp(lo, hi), id);
                ex = ExprPath(path);
                hi = self.last_span.hi;
            }
            token::OpenDelim(token::Bracket) => {
                self.bump();

                if self.check(&token::CloseDelim(token::Bracket)) {
                    // Empty vector.
                    self.bump();
                    ex = ExprVec(Vec::new());
                } else {
                    // Nonempty vector.
                    let first_expr = self.parse_expr();
                    if self.check(&token::Comma) &&
                        self.look_ahead(1, |t| *t == token::DotDot) {
                        // Repeating vector syntax: [ 0, ..512 ]
                        self.bump();
                        self.bump();
                        let count = self.parse_expr();
                        self.expect(&token::CloseDelim(token::Bracket));
                        ex = ExprRepeat(first_expr, count);
                    } else if self.check(&token::Comma) {
                        // Vector with two or more elements.
                        self.bump();
                        let remaining_exprs = self.parse_seq_to_end(
                            &token::CloseDelim(token::Bracket),
                            seq_sep_trailing_allowed(token::Comma),
                            |p| p.parse_expr()
                                );
                        let mut exprs = vec!(first_expr);
                        exprs.extend(remaining_exprs.into_iter());
                        ex = ExprVec(exprs);
                    } else {
                        // Vector with one element.
                        self.expect(&token::CloseDelim(token::Bracket));
                        ex = ExprVec(vec!(first_expr));
                    }
                }
                hi = self.last_span.hi;
            }
            _ => {
                if self.eat_keyword(keywords::Move) {
                    return self.parse_lambda_expr(CaptureByValue);
                }
                if self.eat_keyword(keywords::Proc) {
                    let decl = self.parse_proc_decl();
                    let body = self.parse_expr();
                    let fakeblock = P(ast::Block {
                            id: ast::DUMMY_NODE_ID,
                            view_items: Vec::new(),
                            stmts: Vec::new(),
                            rules: DefaultBlock,
                            span: body.span,
                            expr: Some(body),
                        });
                    return self.mk_expr(lo, fakeblock.span.hi, ExprProc(decl, fakeblock));
                }
                if self.eat_keyword(keywords::If) {
                    return self.parse_if_expr();
                }
                if self.eat_keyword(keywords::For) {
                    return self.parse_for_expr(None);
                }
                if self.eat_keyword(keywords::While) {
                    return self.parse_while_expr(None);
                }
                if self.token.is_lifetime() {
                    let lifetime = self.get_lifetime();
                    self.bump();
                    self.expect(&token::Colon);
                    if self.eat_keyword(keywords::While) {
                        return self.parse_while_expr(Some(lifetime))
                    }
                    if self.eat_keyword(keywords::For) {
                        return self.parse_for_expr(Some(lifetime))
                    }
                    if self.eat_keyword(keywords::Loop) {
                        return self.parse_loop_expr(Some(lifetime))
                    }
                    self.fatal("expected `while`, `for`, or `loop` after a label")
                }
                if self.eat_keyword(keywords::Loop) {
                    return self.parse_loop_expr(None);
                }
                if self.eat_keyword(keywords::Continue) {
                    let lo = self.span.lo;
                    let ex = if self.token.is_lifetime() {
                        let lifetime = self.get_lifetime();
                        self.bump();
                        ExprAgain(Some(lifetime))
                    } else {
                        ExprAgain(None)
                    };
                    let hi = self.span.hi;
                    return self.mk_expr(lo, hi, ex);
                }
                if self.eat_keyword(keywords::Match) {
                    return self.parse_match_expr();
                }
                if self.eat_keyword(keywords::Unsafe) {
                    return self.parse_block_expr(
                        lo,
                        UnsafeBlock(ast::UserProvided));
                }
                if self.eat_keyword(keywords::Return) {
                    // RETURN expression
                    if self.token.can_begin_expr() {
                        let e = self.parse_expr();
                        hi = e.span.hi;
                        ex = ExprRet(Some(e));
                    } else {
                        ex = ExprRet(None);
                    }
                } else if self.eat_keyword(keywords::Break) {
                    // BREAK expression
                    if self.token.is_lifetime() {
                        let lifetime = self.get_lifetime();
                        self.bump();
                        ex = ExprBreak(Some(lifetime));
                    } else {
                        ex = ExprBreak(None);
                    }
                    hi = self.span.hi;
                } else if self.check(&token::ModSep) ||
                        self.token.is_ident() &&
                        !self.token.is_keyword(keywords::True) &&
                        !self.token.is_keyword(keywords::False) {
                    let pth =
                        self.parse_path(LifetimeAndTypesWithColons);

                    // `!`, as an operator, is prefix, so we know this isn't that
                    if self.check(&token::Not) {
                        // MACRO INVOCATION expression
                        self.bump();

                        let delim = self.expect_open_delim();
                        let tts = self.parse_seq_to_end(
                            &token::CloseDelim(delim),
                            seq_sep_none(),
                            |p| p.parse_token_tree());
                        let hi = self.span.hi;

                        return self.mk_mac_expr(lo,
                                                hi,
                                                MacInvocTT(pth,
                                                           tts,
                                                           EMPTY_CTXT));
                    }
                    if self.check(&token::OpenDelim(token::Brace)) {
                        // This is a struct literal, unless we're prohibited
                        // from parsing struct literals here.
                        if !self.restrictions.contains(RESTRICTION_NO_STRUCT_LITERAL) {
                            // It's a struct literal.
                            self.bump();
                            let mut fields = Vec::new();
                            let mut base = None;

                            while self.token != token::CloseDelim(token::Brace) {
                                if self.eat(&token::DotDot) {
                                    base = Some(self.parse_expr());
                                    break;
                                }

                                fields.push(self.parse_field());
                                self.commit_expr(&*fields.last().unwrap().expr,
                                                 &[token::Comma],
                                                 &[token::CloseDelim(token::Brace)]);
                            }

                            if fields.len() == 0 && base.is_none() {
                                let last_span = self.last_span;
                                self.span_err(last_span,
                                              "structure literal must either \
                                              have at least one field or use \
                                              functional structure update \
                                              syntax");
                            }

                            hi = self.span.hi;
                            self.expect(&token::CloseDelim(token::Brace));
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
                    ex = ExprLit(P(lit));
                }
            }
        }

        return self.mk_expr(lo, hi, ex);
    }

    /// Parse a block or unsafe block
    pub fn parse_block_expr(&mut self, lo: BytePos, blk_mode: BlockCheckMode)
                            -> P<Expr> {
        self.expect(&token::OpenDelim(token::Brace));
        let blk = self.parse_block_tail(lo, blk_mode);
        return self.mk_expr(blk.span.lo, blk.span.hi, ExprBlock(blk));
    }

    /// parse a.b or a(13) or a[4] or just a
    pub fn parse_dot_or_call_expr(&mut self) -> P<Expr> {
        let b = self.parse_bottom_expr();
        self.parse_dot_or_call_expr_with(b)
    }

    pub fn parse_dot_or_call_expr_with(&mut self, e0: P<Expr>) -> P<Expr> {
        let mut e = e0;
        let lo = e.span.lo;
        let mut hi;
        loop {
            // expr.f
            if self.eat(&token::Dot) {
                match self.token {
                  token::Ident(i, _) => {
                    let dot = self.last_span.hi;
                    hi = self.span.hi;
                    self.bump();
                    let (_, tys, bindings) = if self.eat(&token::ModSep) {
                        self.expect_lt();
                        self.parse_generic_values_after_lt()
                    } else {
                        (Vec::new(), Vec::new(), Vec::new())
                    };

                    if bindings.len() > 0 {
                        let last_span = self.last_span;
                        self.span_err(last_span, "type bindings are only permitted on trait paths");
                    }

                    // expr.f() method call
                    match self.token {
                        token::OpenDelim(token::Paren) => {
                            let mut es = self.parse_unspanned_seq(
                                &token::OpenDelim(token::Paren),
                                &token::CloseDelim(token::Paren),
                                seq_sep_trailing_allowed(token::Comma),
                                |p| p.parse_expr()
                            );
                            hi = self.last_span.hi;

                            es.insert(0, e);
                            let id = spanned(dot, hi, i);
                            let nd = self.mk_method_call(id, tys, es);
                            e = self.mk_expr(lo, hi, nd);
                        }
                        _ => {
                            if !tys.is_empty() {
                                let last_span = self.last_span;
                                self.span_err(last_span,
                                              "field expressions may not \
                                               have type parameters");
                            }

                            let id = spanned(dot, hi, i);
                            let field = self.mk_field(e, id);
                            e = self.mk_expr(lo, hi, field);
                        }
                    }
                  }
                  token::Literal(token::Integer(n), suf) => {
                    let sp = self.span;

                    // A tuple index may not have a suffix
                    self.expect_no_suffix(sp, "tuple index", suf);

                    let dot = self.last_span.hi;
                    hi = self.span.hi;
                    self.bump();

                    let index = from_str::<uint>(n.as_str());
                    match index {
                        Some(n) => {
                            let id = spanned(dot, hi, n);
                            let field = self.mk_tup_field(e, id);
                            e = self.mk_expr(lo, hi, field);
                        }
                        None => {
                            let last_span = self.last_span;
                            self.span_err(last_span, "invalid tuple or tuple struct index");
                        }
                    }
                  }
                  token::Literal(token::Float(n), _suf) => {
                    self.bump();
                    let last_span = self.last_span;
                    let fstr = n.as_str();
                    self.span_err(last_span,
                                  format!("unexpected token: `{}`", n.as_str()).as_slice());
                    if fstr.chars().all(|x| "0123456789.".contains_char(x)) {
                        let float = match from_str::<f64>(fstr) {
                            Some(f) => f,
                            None => continue,
                        };
                        self.span_help(last_span,
                            format!("try parenthesizing the first index; e.g., `(foo.{}){}`",
                                    float.trunc() as uint,
                                    float.fract().to_string()[1..]).as_slice());
                    }
                    self.abort_if_errors();

                  }
                  _ => self.unexpected()
                }
                continue;
            }
            if self.expr_is_complete(&*e) { break; }
            match self.token {
              // expr(...)
              token::OpenDelim(token::Paren) => {
                let es = self.parse_unspanned_seq(
                    &token::OpenDelim(token::Paren),
                    &token::CloseDelim(token::Paren),
                    seq_sep_trailing_allowed(token::Comma),
                    |p| p.parse_expr()
                );
                hi = self.last_span.hi;

                let nd = self.mk_call(e, es);
                e = self.mk_expr(lo, hi, nd);
              }

              // expr[...]
              // Could be either an index expression or a slicing expression.
              // Any slicing non-terminal can have a mutable version with `mut`
              // after the opening square bracket.
              token::OpenDelim(token::Bracket) => {
                self.bump();
                let mutbl = if self.eat_keyword(keywords::Mut) {
                    MutMutable
                } else {
                    MutImmutable
                };
                match self.token {
                    // e[]
                    token::CloseDelim(token::Bracket) => {
                        self.bump();
                        hi = self.span.hi;
                        let slice = self.mk_slice(e, None, None, mutbl);
                        e = self.mk_expr(lo, hi, slice)
                    }
                    // e[..e]
                    token::DotDot => {
                        self.bump();
                        match self.token {
                            // e[..]
                            token::CloseDelim(token::Bracket) => {
                                self.bump();
                                hi = self.span.hi;
                                let slice = self.mk_slice(e, None, None, mutbl);
                                e = self.mk_expr(lo, hi, slice);

                                self.span_err(e.span, "incorrect slicing expression: `[..]`");
                                self.span_note(e.span,
                                    "use `expr[]` to construct a slice of the whole of expr");
                            }
                            // e[..e]
                            _ => {
                                hi = self.span.hi;
                                let e2 = self.parse_expr();
                                self.commit_expr_expecting(&*e2, token::CloseDelim(token::Bracket));
                                let slice = self.mk_slice(e, None, Some(e2), mutbl);
                                e = self.mk_expr(lo, hi, slice)
                            }
                        }
                    }
                    // e[e] | e[e..] | e[e..e]
                    _ => {
                        let ix = self.parse_expr();
                        match self.token {
                            // e[e..] | e[e..e]
                            token::DotDot => {
                                self.bump();
                                let e2 = match self.token {
                                    // e[e..]
                                    token::CloseDelim(token::Bracket) => {
                                        self.bump();
                                        None
                                    }
                                    // e[e..e]
                                    _ => {
                                        let e2 = self.parse_expr();
                                        self.commit_expr_expecting(&*e2,
                                            token::CloseDelim(token::Bracket));
                                        Some(e2)
                                    }
                                };
                                hi = self.span.hi;
                                let slice = self.mk_slice(e, Some(ix), e2, mutbl);
                                e = self.mk_expr(lo, hi, slice)
                            }
                            // e[e]
                            _ => {
                                if mutbl == ast::MutMutable {
                                    self.span_err(e.span,
                                                  "`mut` keyword is invalid in index expressions");
                                }
                                hi = self.span.hi;
                                self.commit_expr_expecting(&*ix, token::CloseDelim(token::Bracket));
                                let index = self.mk_index(e, ix);
                                e = self.mk_expr(lo, hi, index)
                            }
                        }
                    }
                }
              }

              _ => return e
            }
        }
        return e;
    }

    /// Parse an optional separator followed by a Kleene-style
    /// repetition token (+ or *).
    pub fn parse_sep_and_kleene_op(&mut self) -> (Option<token::Token>, ast::KleeneOp) {
        fn parse_kleene_op(parser: &mut Parser) -> Option<ast::KleeneOp> {
            match parser.token {
                token::BinOp(token::Star) => {
                    parser.bump();
                    Some(ast::ZeroOrMore)
                },
                token::BinOp(token::Plus) => {
                    parser.bump();
                    Some(ast::OneOrMore)
                },
                _ => None
            }
        };

        match parse_kleene_op(self) {
            Some(kleene_op) => return (None, kleene_op),
            None => {}
        }

        let separator = self.bump_and_get();
        match parse_kleene_op(self) {
            Some(zerok) => (Some(separator), zerok),
            None => self.fatal("expected `*` or `+`")
        }
    }

    /// parse a single token tree from the input.
    pub fn parse_token_tree(&mut self) -> TokenTree {
        // FIXME #6994: currently, this is too eager. It
        // parses token trees but also identifies TtSequence's
        // and token::SubstNt's; it's too early to know yet
        // whether something will be a nonterminal or a seq
        // yet.
        maybe_whole!(deref self, NtTT);

        // this is the fall-through for the 'match' below.
        // invariants: the current token is not a left-delimiter,
        // not an EOF, and not the desired right-delimiter (if
        // it were, parse_seq_to_before_end would have prevented
        // reaching this point.
        fn parse_non_delim_tt_tok(p: &mut Parser) -> TokenTree {
            maybe_whole!(deref p, NtTT);
            match p.token {
              token::CloseDelim(_) => {
                  // This is a conservative error: only report the last unclosed delimiter. The
                  // previous unclosed delimiters could actually be closed! The parser just hasn't
                  // gotten to them yet.
                  match p.open_braces.last() {
                      None => {}
                      Some(&sp) => p.span_note(sp, "unclosed delimiter"),
                  };
                  let token_str = p.this_token_to_string();
                  p.fatal(format!("incorrect close delimiter: `{}`",
                                  token_str).as_slice())
              },
              /* we ought to allow different depths of unquotation */
              token::Dollar if p.quote_depth > 0u => {
                p.bump();
                let sp = p.span;

                if p.token == token::OpenDelim(token::Paren) {
                    let seq = p.parse_seq(
                        &token::OpenDelim(token::Paren),
                        &token::CloseDelim(token::Paren),
                        seq_sep_none(),
                        |p| p.parse_token_tree()
                    );
                    let (sep, repeat) = p.parse_sep_and_kleene_op();
                    let seq = match seq {
                        Spanned { node, .. } => node,
                    };
                    let name_num = macro_parser::count_names(seq.as_slice());
                    TtSequence(mk_sp(sp.lo, p.span.hi),
                               Rc::new(SequenceRepetition {
                                   tts: seq,
                                   separator: sep,
                                   op: repeat,
                                   num_captures: name_num
                               }))
                } else {
                    // A nonterminal that matches or not
                    let namep = match p.token { token::Ident(_, p) => p, _ => token::Plain };
                    let name = p.parse_ident();
                    if p.token == token::Colon && p.look_ahead(1, |t| t.is_ident()) {
                        p.bump();
                        let kindp = match p.token { token::Ident(_, p) => p, _ => token::Plain };
                        let nt_kind = p.parse_ident();
                        let m = TtToken(sp, MatchNt(name, nt_kind, namep, kindp));
                        m
                    } else {
                        TtToken(sp, SubstNt(name, namep))
                    }
                }
              }
              _ => {
                  TtToken(p.span, p.bump_and_get())
              }
            }
        }

        match self.token {
            token::Eof => {
                let open_braces = self.open_braces.clone();
                for sp in open_braces.iter() {
                    self.span_help(*sp, "did you mean to close this delimiter?");
                }
                // There shouldn't really be a span, but it's easier for the test runner
                // if we give it one
                self.fatal("this file contains an un-closed delimiter ");
            },
            token::OpenDelim(delim) => {
                // The span for beginning of the delimited section
                let pre_span = self.span;

                // Parse the open delimiter.
                self.open_braces.push(self.span);
                let open_span = self.span;
                self.bump();

                // Parse the token trees within the delimeters
                let tts = self.parse_seq_to_before_end(
                    &token::CloseDelim(delim),
                    seq_sep_none(),
                    |p| p.parse_token_tree()
                );

                // Parse the close delimiter.
                let close_span = self.span;
                self.bump();
                self.open_braces.pop().unwrap();

                // Expand to cover the entire delimited token tree
                let span = Span { hi: self.span.hi, ..pre_span };

                TtDelimited(span, Rc::new(Delimited {
                    delim: delim,
                    open_span: open_span,
                    tts: tts,
                    close_span: close_span,
                }))
            },
            _ => parse_non_delim_tt_tok(self),
        }
    }

    // parse a stream of tokens into a list of TokenTree's,
    // up to EOF.
    pub fn parse_all_token_trees(&mut self) -> Vec<TokenTree> {
        let mut tts = Vec::new();
        while self.token != token::Eof {
            tts.push(self.parse_token_tree());
        }
        tts
    }

    /// Parse a prefix-operator expr
    pub fn parse_prefix_expr(&mut self) -> P<Expr> {
        let lo = self.span.lo;
        let hi;

        let ex;
        match self.token {
          token::Not => {
            self.bump();
            let e = self.parse_prefix_expr();
            hi = e.span.hi;
            ex = self.mk_unary(UnNot, e);
          }
          token::BinOp(token::Minus) => {
            self.bump();
            let e = self.parse_prefix_expr();
            hi = e.span.hi;
            ex = self.mk_unary(UnNeg, e);
          }
          token::BinOp(token::Star) => {
            self.bump();
            let e = self.parse_prefix_expr();
            hi = e.span.hi;
            ex = self.mk_unary(UnDeref, e);
          }
          token::BinOp(token::And) | token::AndAnd => {
            self.expect_and();
            let m = self.parse_mutability();
            let e = self.parse_prefix_expr();
            hi = e.span.hi;
            ex = ExprAddrOf(m, e);
          }
          token::Tilde => {
            self.bump();
            let last_span = self.last_span;
            match self.token {
                token::OpenDelim(token::Bracket) => {
                    self.obsolete(last_span, ObsoleteOwnedVector)
                },
                _ => self.obsolete(last_span, ObsoleteOwnedExpr)
            }

            let e = self.parse_prefix_expr();
            hi = e.span.hi;
            ex = self.mk_unary(UnUniq, e);
          }
          token::Ident(_, _) => {
            if !self.token.is_keyword(keywords::Box) {
                return self.parse_dot_or_call_expr();
            }

            let lo = self.span.lo;

            self.bump();

            // Check for a place: `box(PLACE) EXPR`.
            if self.eat(&token::OpenDelim(token::Paren)) {
                // Support `box() EXPR` as the default.
                if !self.eat(&token::CloseDelim(token::Paren)) {
                    let place = self.parse_expr();
                    self.expect(&token::CloseDelim(token::Paren));
                    // Give a suggestion to use `box()` when a parenthesised expression is used
                    if !self.token.can_begin_expr() {
                        let span = self.span;
                        let this_token_to_string = self.this_token_to_string();
                        self.span_err(span,
                                      format!("expected expression, found `{}`",
                                              this_token_to_string).as_slice());
                        let box_span = mk_sp(lo, self.last_span.hi);
                        self.span_help(box_span,
                                       "perhaps you meant `box() (foo)` instead?");
                        self.abort_if_errors();
                    }
                    let subexpression = self.parse_prefix_expr();
                    hi = subexpression.span.hi;
                    ex = ExprBox(place, subexpression);
                    return self.mk_expr(lo, hi, ex);
                }
            }

            // Otherwise, we use the unique pointer default.
            let subexpression = self.parse_prefix_expr();
            hi = subexpression.span.hi;
            ex = self.mk_unary(UnUniq, subexpression);
          }
          _ => return self.parse_dot_or_call_expr()
        }
        return self.mk_expr(lo, hi, ex);
    }

    /// Parse an expression of binops
    pub fn parse_binops(&mut self) -> P<Expr> {
        let prefix_expr = self.parse_prefix_expr();
        self.parse_more_binops(prefix_expr, 0)
    }

    /// Parse an expression of binops of at least min_prec precedence
    pub fn parse_more_binops(&mut self, lhs: P<Expr>, min_prec: uint) -> P<Expr> {
        if self.expr_is_complete(&*lhs) { return lhs; }

        // Prevent dynamic borrow errors later on by limiting the
        // scope of the borrows.
        if self.token == token::BinOp(token::Or) &&
            self.restrictions.contains(RESTRICTION_NO_BAR_OP) {
            return lhs;
        }
        self.expected_tokens.push(TokenType::Operator);

        let cur_opt = self.token.to_binop();
        match cur_opt {
            Some(cur_op) => {
                let cur_prec = operator_prec(cur_op);
                if cur_prec > min_prec {
                    self.bump();
                    let expr = self.parse_prefix_expr();
                    let rhs = self.parse_more_binops(expr, cur_prec);
                    let lhs_span = lhs.span;
                    let rhs_span = rhs.span;
                    let binary = self.mk_binary(cur_op, lhs, rhs);
                    let bin = self.mk_expr(lhs_span.lo, rhs_span.hi, binary);
                    self.parse_more_binops(bin, min_prec)
                } else {
                    lhs
                }
            }
            None => {
                if as_prec > min_prec && self.eat_keyword(keywords::As) {
                    let rhs = self.parse_ty();
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

    /// Parse an assignment expression....
    /// actually, this seems to be the main entry point for
    /// parsing an arbitrary expression.
    pub fn parse_assign_expr(&mut self) -> P<Expr> {
        let lo = self.span.lo;
        let lhs = self.parse_binops();
        let restrictions = self.restrictions & RESTRICTION_NO_STRUCT_LITERAL;
        match self.token {
          token::Eq => {
              self.bump();
              let rhs = self.parse_expr_res(restrictions);
              self.mk_expr(lo, rhs.span.hi, ExprAssign(lhs, rhs))
          }
          token::BinOpEq(op) => {
              self.bump();
              let rhs = self.parse_expr_res(restrictions);
              let aop = match op {
                  token::Plus =>    BiAdd,
                  token::Minus =>   BiSub,
                  token::Star =>    BiMul,
                  token::Slash =>   BiDiv,
                  token::Percent => BiRem,
                  token::Caret =>   BiBitXor,
                  token::And =>     BiBitAnd,
                  token::Or =>      BiBitOr,
                  token::Shl =>     BiShl,
                  token::Shr =>     BiShr
              };
              let rhs_span = rhs.span;
              let assign_op = self.mk_assign_op(aop, lhs, rhs);
              self.mk_expr(lo, rhs_span.hi, assign_op)
          }
          _ => {
              lhs
          }
        }
    }

    /// Parse an 'if' or 'if let' expression ('if' token already eaten)
    pub fn parse_if_expr(&mut self) -> P<Expr> {
        if self.token.is_keyword(keywords::Let) {
            return self.parse_if_let_expr();
        }
        let lo = self.last_span.lo;
        let cond = self.parse_expr_res(RESTRICTION_NO_STRUCT_LITERAL);
        let thn = self.parse_block();
        let mut els: Option<P<Expr>> = None;
        let mut hi = thn.span.hi;
        if self.eat_keyword(keywords::Else) {
            let elexpr = self.parse_else_expr();
            hi = elexpr.span.hi;
            els = Some(elexpr);
        }
        self.mk_expr(lo, hi, ExprIf(cond, thn, els))
    }

    /// Parse an 'if let' expression ('if' token already eaten)
    pub fn parse_if_let_expr(&mut self) -> P<Expr> {
        let lo = self.last_span.lo;
        self.expect_keyword(keywords::Let);
        let pat = self.parse_pat();
        self.expect(&token::Eq);
        let expr = self.parse_expr_res(RESTRICTION_NO_STRUCT_LITERAL);
        let thn = self.parse_block();
        let (hi, els) = if self.eat_keyword(keywords::Else) {
            let expr = self.parse_else_expr();
            (expr.span.hi, Some(expr))
        } else {
            (thn.span.hi, None)
        };
        self.mk_expr(lo, hi, ExprIfLet(pat, expr, thn, els))
    }

    // `|args| expr`
    pub fn parse_lambda_expr(&mut self, capture_clause: CaptureClause)
                             -> P<Expr>
    {
        let lo = self.span.lo;
        let (decl, optional_unboxed_closure_kind) =
            self.parse_fn_block_decl();
        let body = self.parse_expr();
        let fakeblock = P(ast::Block {
            id: ast::DUMMY_NODE_ID,
            view_items: Vec::new(),
            stmts: Vec::new(),
            span: body.span,
            expr: Some(body),
            rules: DefaultBlock,
        });

        self.mk_expr(
            lo,
            fakeblock.span.hi,
            ExprClosure(capture_clause, optional_unboxed_closure_kind, decl, fakeblock))
    }

    pub fn parse_else_expr(&mut self) -> P<Expr> {
        if self.eat_keyword(keywords::If) {
            return self.parse_if_expr();
        } else {
            let blk = self.parse_block();
            return self.mk_expr(blk.span.lo, blk.span.hi, ExprBlock(blk));
        }
    }

    /// Parse a 'for' .. 'in' expression ('for' token already eaten)
    pub fn parse_for_expr(&mut self, opt_ident: Option<ast::Ident>) -> P<Expr> {
        // Parse: `for <src_pat> in <src_expr> <src_loop_block>`

        let lo = self.last_span.lo;
        let pat = self.parse_pat();
        self.expect_keyword(keywords::In);
        let expr = self.parse_expr_res(RESTRICTION_NO_STRUCT_LITERAL);
        let loop_block = self.parse_block();
        let hi = self.span.hi;

        self.mk_expr(lo, hi, ExprForLoop(pat, expr, loop_block, opt_ident))
    }

    /// Parse a 'while' or 'while let' expression ('while' token already eaten)
    pub fn parse_while_expr(&mut self, opt_ident: Option<ast::Ident>) -> P<Expr> {
        if self.token.is_keyword(keywords::Let) {
            return self.parse_while_let_expr(opt_ident);
        }
        let lo = self.last_span.lo;
        let cond = self.parse_expr_res(RESTRICTION_NO_STRUCT_LITERAL);
        let body = self.parse_block();
        let hi = body.span.hi;
        return self.mk_expr(lo, hi, ExprWhile(cond, body, opt_ident));
    }

    /// Parse a 'while let' expression ('while' token already eaten)
    pub fn parse_while_let_expr(&mut self, opt_ident: Option<ast::Ident>) -> P<Expr> {
        let lo = self.last_span.lo;
        self.expect_keyword(keywords::Let);
        let pat = self.parse_pat();
        self.expect(&token::Eq);
        let expr = self.parse_expr_res(RESTRICTION_NO_STRUCT_LITERAL);
        let body = self.parse_block();
        let hi = body.span.hi;
        return self.mk_expr(lo, hi, ExprWhileLet(pat, expr, body, opt_ident));
    }

    pub fn parse_loop_expr(&mut self, opt_ident: Option<ast::Ident>) -> P<Expr> {
        let lo = self.last_span.lo;
        let body = self.parse_block();
        let hi = body.span.hi;
        self.mk_expr(lo, hi, ExprLoop(body, opt_ident))
    }

    fn parse_match_expr(&mut self) -> P<Expr> {
        let lo = self.last_span.lo;
        let discriminant = self.parse_expr_res(RESTRICTION_NO_STRUCT_LITERAL);
        self.commit_expr_expecting(&*discriminant, token::OpenDelim(token::Brace));
        let mut arms: Vec<Arm> = Vec::new();
        while self.token != token::CloseDelim(token::Brace) {
            arms.push(self.parse_arm());
        }
        let hi = self.span.hi;
        self.bump();
        return self.mk_expr(lo, hi, ExprMatch(discriminant, arms, MatchNormal));
    }

    pub fn parse_arm(&mut self) -> Arm {
        let attrs = self.parse_outer_attributes();
        let pats = self.parse_pats();
        let mut guard = None;
        if self.eat_keyword(keywords::If) {
            guard = Some(self.parse_expr());
        }
        self.expect(&token::FatArrow);
        let expr = self.parse_expr_res(RESTRICTION_STMT_EXPR);

        let require_comma =
            !classify::expr_is_simple_block(&*expr)
            && self.token != token::CloseDelim(token::Brace);

        if require_comma {
            self.commit_expr(&*expr, &[token::Comma], &[token::CloseDelim(token::Brace)]);
        } else {
            self.eat(&token::Comma);
        }

        ast::Arm {
            attrs: attrs,
            pats: pats,
            guard: guard,
            body: expr,
        }
    }

    /// Parse an expression
    pub fn parse_expr(&mut self) -> P<Expr> {
        return self.parse_expr_res(UNRESTRICTED);
    }

    /// Parse an expression, subject to the given restrictions
    pub fn parse_expr_res(&mut self, r: Restrictions) -> P<Expr> {
        let old = self.restrictions;
        self.restrictions = r;
        let e = self.parse_assign_expr();
        self.restrictions = old;
        return e;
    }

    /// Parse the RHS of a local variable declaration (e.g. '= 14;')
    fn parse_initializer(&mut self) -> Option<P<Expr>> {
        if self.check(&token::Eq) {
            self.bump();
            Some(self.parse_expr())
        } else {
            None
        }
    }

    /// Parse patterns, separated by '|' s
    fn parse_pats(&mut self) -> Vec<P<Pat>> {
        let mut pats = Vec::new();
        loop {
            pats.push(self.parse_pat());
            if self.check(&token::BinOp(token::Or)) { self.bump(); }
            else { return pats; }
        };
    }

    fn parse_pat_vec_elements(
        &mut self,
    ) -> (Vec<P<Pat>>, Option<P<Pat>>, Vec<P<Pat>>) {
        let mut before = Vec::new();
        let mut slice = None;
        let mut after = Vec::new();
        let mut first = true;
        let mut before_slice = true;

        while self.token != token::CloseDelim(token::Bracket) {
            if first {
                first = false;
            } else {
                self.expect(&token::Comma);

                if self.token == token::CloseDelim(token::Bracket)
                        && (before_slice || after.len() != 0) {
                    break
                }
            }

            if before_slice {
                if self.check(&token::DotDot) {
                    self.bump();

                    if self.check(&token::Comma) ||
                            self.check(&token::CloseDelim(token::Bracket)) {
                        slice = Some(P(ast::Pat {
                            id: ast::DUMMY_NODE_ID,
                            node: PatWild(PatWildMulti),
                            span: self.span,
                        }));
                        before_slice = false;
                    } else {
                        let _ = self.parse_pat();
                        let span = self.span;
                        self.obsolete(span, ObsoleteSubsliceMatch);
                    }
                    continue
                }
            }

            let subpat = self.parse_pat();
            if before_slice && self.check(&token::DotDot) {
                self.bump();
                slice = Some(subpat);
                before_slice = false;
            } else if before_slice {
                before.push(subpat);
            } else {
                after.push(subpat);
            }
        }

        (before, slice, after)
    }

    /// Parse the fields of a struct-like pattern
    fn parse_pat_fields(&mut self) -> (Vec<codemap::Spanned<ast::FieldPat>> , bool) {
        let mut fields = Vec::new();
        let mut etc = false;
        let mut first = true;
        while self.token != token::CloseDelim(token::Brace) {
            if first {
                first = false;
            } else {
                self.expect(&token::Comma);
                // accept trailing commas
                if self.check(&token::CloseDelim(token::Brace)) { break }
            }

            let lo = self.span.lo;
            let hi;

            if self.check(&token::DotDot) {
                self.bump();
                if self.token != token::CloseDelim(token::Brace) {
                    let token_str = self.this_token_to_string();
                    self.fatal(format!("expected `{}`, found `{}`", "}",
                                       token_str).as_slice())
                }
                etc = true;
                break;
            }

            let bind_type = if self.eat_keyword(keywords::Mut) {
                BindByValue(MutMutable)
            } else if self.eat_keyword(keywords::Ref) {
                BindByRef(self.parse_mutability())
            } else {
                BindByValue(MutImmutable)
            };

            let fieldname = self.parse_ident();

            let (subpat, is_shorthand) = if self.check(&token::Colon) {
                match bind_type {
                    BindByRef(..) | BindByValue(MutMutable) => {
                        let token_str = self.this_token_to_string();
                        self.fatal(format!("unexpected `{}`",
                                           token_str).as_slice())
                    }
                    _ => {}
                }

                self.bump();
                let pat = self.parse_pat();
                hi = pat.span.hi;
                (pat, false)
            } else {
                hi = self.last_span.hi;
                let fieldpath = codemap::Spanned{span:self.last_span, node: fieldname};
                (P(ast::Pat {
                    id: ast::DUMMY_NODE_ID,
                    node: PatIdent(bind_type, fieldpath, None),
                    span: self.last_span
                }), true)
            };
            fields.push(codemap::Spanned { span: mk_sp(lo, hi),
                                           node: ast::FieldPat { ident: fieldname,
                                                                 pat: subpat,
                                                                 is_shorthand: is_shorthand }});
        }
        return (fields, etc);
    }

    /// Parse a pattern.
    pub fn parse_pat(&mut self) -> P<Pat> {
        maybe_whole!(self, NtPat);

        let lo = self.span.lo;
        let mut hi;
        let pat;
        match self.token {
            // parse _
          token::Underscore => {
            self.bump();
            pat = PatWild(PatWildSingle);
            hi = self.last_span.hi;
            return P(ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            })
          }
          token::Tilde => {
            // parse ~pat
            self.bump();
            let sub = self.parse_pat();
            pat = PatBox(sub);
            let last_span = self.last_span;
            hi = last_span.hi;
            self.obsolete(last_span, ObsoleteOwnedPattern);
            return P(ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            })
          }
          token::BinOp(token::And) | token::AndAnd => {
            // parse &pat
            let lo = self.span.lo;
            self.expect_and();
            let sub = self.parse_pat();
            pat = PatRegion(sub);
            hi = self.last_span.hi;
            return P(ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            })
          }
          token::OpenDelim(token::Paren) => {
            // parse (pat,pat,pat,...) as tuple
            self.bump();
            if self.check(&token::CloseDelim(token::Paren)) {
                self.bump();
                pat = PatTup(vec![]);
            } else {
                let mut fields = vec!(self.parse_pat());
                if self.look_ahead(1, |t| *t != token::CloseDelim(token::Paren)) {
                    while self.check(&token::Comma) {
                        self.bump();
                        if self.check(&token::CloseDelim(token::Paren)) { break; }
                        fields.push(self.parse_pat());
                    }
                }
                if fields.len() == 1 { self.expect(&token::Comma); }
                self.expect(&token::CloseDelim(token::Paren));
                pat = PatTup(fields);
            }
            hi = self.last_span.hi;
            return P(ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            })
          }
          token::OpenDelim(token::Bracket) => {
            // parse [pat,pat,...] as vector pattern
            self.bump();
            let (before, slice, after) =
                self.parse_pat_vec_elements();

            self.expect(&token::CloseDelim(token::Bracket));
            pat = ast::PatVec(before, slice, after);
            hi = self.last_span.hi;
            return P(ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            })
          }
          _ => {}
        }
        // at this point, token != _, ~, &, &&, (, [

        if (!(self.token.is_ident() || self.token.is_path())
              && self.token != token::ModSep)
                || self.token.is_keyword(keywords::True)
                || self.token.is_keyword(keywords::False) {
            // Parse an expression pattern or exp .. exp.
            //
            // These expressions are limited to literals (possibly
            // preceded by unary-minus) or identifiers.
            let val = self.parse_literal_maybe_minus();
            if (self.check(&token::DotDotDot)) &&
                    self.look_ahead(1, |t| {
                        *t != token::Comma && *t != token::CloseDelim(token::Bracket)
                    }) {
                self.bump();
                let end = if self.token.is_ident() || self.token.is_path() {
                    let path = self.parse_path(LifetimeAndTypesWithColons);
                    let hi = self.span.hi;
                    self.mk_expr(lo, hi, ExprPath(path))
                } else {
                    self.parse_literal_maybe_minus()
                };
                pat = PatRange(val, end);
            } else {
                pat = PatLit(val);
            }
        } else if self.eat_keyword(keywords::Mut) {
            pat = self.parse_pat_ident(BindByValue(MutMutable));
        } else if self.eat_keyword(keywords::Ref) {
            // parse ref pat
            let mutbl = self.parse_mutability();
            pat = self.parse_pat_ident(BindByRef(mutbl));
        } else if self.eat_keyword(keywords::Box) {
            // `box PAT`
            //
            // FIXME(#13910): Rename to `PatBox` and extend to full DST
            // support.
            let sub = self.parse_pat();
            pat = PatBox(sub);
            hi = self.last_span.hi;
            return P(ast::Pat {
                id: ast::DUMMY_NODE_ID,
                node: pat,
                span: mk_sp(lo, hi)
            })
        } else {
            let can_be_enum_or_struct = self.look_ahead(1, |t| {
                match *t {
                    token::OpenDelim(_) | token::Lt | token::ModSep => true,
                    _ => false,
                }
            });

            if self.look_ahead(1, |t| *t == token::DotDotDot) &&
                    self.look_ahead(2, |t| {
                        *t != token::Comma && *t != token::CloseDelim(token::Bracket)
                    }) {
                let start = self.parse_expr_res(RESTRICTION_NO_BAR_OP);
                self.eat(&token::DotDotDot);
                let end = self.parse_expr_res(RESTRICTION_NO_BAR_OP);
                pat = PatRange(start, end);
            } else if self.token.is_plain_ident() && !can_be_enum_or_struct {
                let id = self.parse_ident();
                let id_span = self.last_span;
                let pth1 = codemap::Spanned{span:id_span, node: id};
                if self.eat(&token::Not) {
                    // macro invocation
                    let delim = self.expect_open_delim();
                    let tts = self.parse_seq_to_end(&token::CloseDelim(delim),
                                                    seq_sep_none(),
                                                    |p| p.parse_token_tree());

                    let mac = MacInvocTT(ident_to_path(id_span,id), tts, EMPTY_CTXT);
                    pat = ast::PatMac(codemap::Spanned {node: mac, span: self.span});
                } else {
                    let sub = if self.eat(&token::At) {
                        // parse foo @ pat
                        Some(self.parse_pat())
                    } else {
                        // or just foo
                        None
                    };
                    pat = PatIdent(BindByValue(MutImmutable), pth1, sub);
                }
            } else {
                // parse an enum pat
                let enum_path = self.parse_path(LifetimeAndTypesWithColons);
                match self.token {
                    token::OpenDelim(token::Brace) => {
                        self.bump();
                        let (fields, etc) =
                            self.parse_pat_fields();
                        self.bump();
                        pat = PatStruct(enum_path, fields, etc);
                    }
                    _ => {
                        let mut args: Vec<P<Pat>> = Vec::new();
                        match self.token {
                          token::OpenDelim(token::Paren) => {
                            let is_dotdot = self.look_ahead(1, |t| {
                                match *t {
                                    token::DotDot => true,
                                    _ => false,
                                }
                            });
                            if is_dotdot {
                                // This is a "top constructor only" pat
                                self.bump();
                                self.bump();
                                self.expect(&token::CloseDelim(token::Paren));
                                pat = PatEnum(enum_path, None);
                            } else {
                                args = self.parse_enum_variant_seq(
                                    &token::OpenDelim(token::Paren),
                                    &token::CloseDelim(token::Paren),
                                    seq_sep_trailing_allowed(token::Comma),
                                    |p| p.parse_pat()
                                );
                                pat = PatEnum(enum_path, Some(args));
                            }
                          },
                          _ => {
                              if !enum_path.global &&
                                  enum_path.segments.len() == 1 &&
                                  enum_path.segments[0].parameters.is_empty()
                              {
                                  // it could still be either an enum
                                  // or an identifier pattern, resolve
                                  // will sort it out:
                                  pat = PatIdent(BindByValue(MutImmutable),
                                                 codemap::Spanned{
                                                    span: enum_path.span,
                                                    node: enum_path.segments[0]
                                                           .identifier},
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
        P(ast::Pat {
            id: ast::DUMMY_NODE_ID,
            node: pat,
            span: mk_sp(lo, hi),
        })
    }

    /// Parse ident or ident @ pat
    /// used by the copy foo and ref foo patterns to give a good
    /// error message when parsing mistakes like ref foo(a,b)
    fn parse_pat_ident(&mut self,
                       binding_mode: ast::BindingMode)
                       -> ast::Pat_ {
        if !self.token.is_plain_ident() {
            let span = self.span;
            let tok_str = self.this_token_to_string();
            self.span_fatal(span,
                            format!("expected identifier, found `{}`", tok_str).as_slice());
        }
        let ident = self.parse_ident();
        let last_span = self.last_span;
        let name = codemap::Spanned{span: last_span, node: ident};
        let sub = if self.eat(&token::At) {
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
        if self.token == token::OpenDelim(token::Paren) {
            let last_span = self.last_span;
            self.span_fatal(
                last_span,
                "expected identifier, found enum pattern");
        }

        PatIdent(binding_mode, name, sub)
    }

    /// Parse a local variable declaration
    fn parse_local(&mut self) -> P<Local> {
        let lo = self.span.lo;
        let pat = self.parse_pat();

        let mut ty = P(Ty {
            id: ast::DUMMY_NODE_ID,
            node: TyInfer,
            span: mk_sp(lo, lo),
        });
        if self.eat(&token::Colon) {
            ty = self.parse_ty_sum();
        }
        let init = self.parse_initializer();
        P(ast::Local {
            ty: ty,
            pat: pat,
            init: init,
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, self.last_span.hi),
            source: LocalLet,
        })
    }

    /// Parse a "let" stmt
    fn parse_let(&mut self) -> P<Decl> {
        let lo = self.span.lo;
        let local = self.parse_local();
        P(spanned(lo, self.last_span.hi, DeclLocal(local)))
    }

    /// Parse a structure field
    fn parse_name_and_ty(&mut self, pr: Visibility,
                         attrs: Vec<Attribute> ) -> StructField {
        let lo = self.span.lo;
        if !self.token.is_plain_ident() {
            self.fatal("expected ident");
        }
        let name = self.parse_ident();
        self.expect(&token::Colon);
        let ty = self.parse_ty_sum();
        spanned(lo, self.last_span.hi, ast::StructField_ {
            kind: NamedField(name, pr),
            id: ast::DUMMY_NODE_ID,
            ty: ty,
            attrs: attrs,
        })
    }

    /// Get an expected item after attributes error message.
    fn expected_item_err(attrs: &[Attribute]) -> &'static str {
        match attrs.last() {
            Some(&Attribute { node: ast::Attribute_ { is_sugared_doc: true, .. }, .. }) => {
                "expected item after doc comment"
            }
            _ => "expected item after attributes",
        }
    }

    /// Parse a statement. may include decl.
    /// Precondition: any attributes are parsed already
    pub fn parse_stmt(&mut self, item_attrs: Vec<Attribute>) -> P<Stmt> {
        maybe_whole!(self, NtStmt);

        fn check_expected_item(p: &mut Parser, attrs: &[Attribute]) {
            // If we have attributes then we should have an item
            if !attrs.is_empty() {
                let last_span = p.last_span;
                p.span_err(last_span, Parser::expected_item_err(attrs));
            }
        }

        let lo = self.span.lo;
        if self.token.is_keyword(keywords::Let) {
            check_expected_item(self, item_attrs.as_slice());
            self.expect_keyword(keywords::Let);
            let decl = self.parse_let();
            P(spanned(lo, decl.span.hi, StmtDecl(decl, ast::DUMMY_NODE_ID)))
        } else if self.token.is_ident()
            && !self.token.is_any_keyword()
            && self.look_ahead(1, |t| *t == token::Not) {
            // it's a macro invocation:

            check_expected_item(self, item_attrs.as_slice());

            // Potential trouble: if we allow macros with paths instead of
            // idents, we'd need to look ahead past the whole path here...
            let pth = self.parse_path(NoTypesAllowed);
            self.bump();

            let id = match self.token {
                token::OpenDelim(_) => token::special_idents::invalid, // no special identifier
                _ => self.parse_ident(),
            };

            // check that we're pointing at delimiters (need to check
            // again after the `if`, because of `parse_ident`
            // consuming more tokens).
            let delim = match self.token {
                token::OpenDelim(delim) => delim,
                _ => {
                    // we only expect an ident if we didn't parse one
                    // above.
                    let ident_str = if id.name == token::special_idents::invalid.name {
                        "identifier, "
                    } else {
                        ""
                    };
                    let tok_str = self.this_token_to_string();
                    self.fatal(format!("expected {}`(` or `{{`, found `{}`",
                                       ident_str,
                                       tok_str).as_slice())
                },
            };

            let tts = self.parse_unspanned_seq(
                &token::OpenDelim(delim),
                &token::CloseDelim(delim),
                seq_sep_none(),
                |p| p.parse_token_tree()
            );
            let hi = self.span.hi;

            if id.name == token::special_idents::invalid.name {
                if self.check(&token::Dot) {
                    let span = self.span;
                    let token_string = self.this_token_to_string();
                    self.span_err(span,
                                  format!("expected statement, found `{}`",
                                          token_string).as_slice());
                    let mac_span = mk_sp(lo, hi);
                    self.span_help(mac_span, "try parenthesizing this macro invocation");
                    self.abort_if_errors();
                }
                P(spanned(lo, hi, StmtMac(
                    spanned(lo, hi, MacInvocTT(pth, tts, EMPTY_CTXT)), false)))
            } else {
                // if it has a special ident, it's definitely an item
                P(spanned(lo, hi, StmtDecl(
                    P(spanned(lo, hi, DeclItem(
                        self.mk_item(
                            lo, hi, id /*id is good here*/,
                            ItemMac(spanned(lo, hi, MacInvocTT(pth, tts, EMPTY_CTXT))),
                            Inherited, Vec::new(/*no attrs*/))))),
                    ast::DUMMY_NODE_ID)))
            }

        } else {
            let found_attrs = !item_attrs.is_empty();
            let item_err = Parser::expected_item_err(item_attrs.as_slice());
            match self.parse_item_or_view_item(item_attrs, false) {
                IoviItem(i) => {
                    let hi = i.span.hi;
                    let decl = P(spanned(lo, hi, DeclItem(i)));
                    P(spanned(lo, hi, StmtDecl(decl, ast::DUMMY_NODE_ID)))
                }
                IoviViewItem(vi) => {
                    self.span_fatal(vi.span,
                                    "view items must be declared at the top of the block");
                }
                IoviForeignItem(_) => {
                    self.fatal("foreign items are not allowed here");
                }
                IoviNone(_) => {
                    if found_attrs {
                        let last_span = self.last_span;
                        self.span_err(last_span, item_err);
                    }

                    // Remainder are line-expr stmts.
                    let e = self.parse_expr_res(RESTRICTION_STMT_EXPR);
                    P(spanned(lo, e.span.hi, StmtExpr(e, ast::DUMMY_NODE_ID)))
                }
            }
        }
    }

    /// Is this expression a successfully-parsed statement?
    fn expr_is_complete(&mut self, e: &Expr) -> bool {
        self.restrictions.contains(RESTRICTION_STMT_EXPR) &&
            !classify::expr_requires_semi_to_be_stmt(e)
    }

    /// Parse a block. No inner attrs are allowed.
    pub fn parse_block(&mut self) -> P<Block> {
        maybe_whole!(no_clone self, NtBlock);

        let lo = self.span.lo;

        if !self.eat(&token::OpenDelim(token::Brace)) {
            let sp = self.span;
            let tok = self.this_token_to_string();
            self.span_fatal_help(sp,
                                 format!("expected `{{`, found `{}`", tok).as_slice(),
                                 "place this code inside a block");
        }

        return self.parse_block_tail_(lo, DefaultBlock, Vec::new());
    }

    /// Parse a block. Inner attrs are allowed.
    fn parse_inner_attrs_and_block(&mut self)
        -> (Vec<Attribute> , P<Block>) {

        maybe_whole!(pair_empty self, NtBlock);

        let lo = self.span.lo;
        self.expect(&token::OpenDelim(token::Brace));
        let (inner, next) = self.parse_inner_attrs_and_next();

        (inner, self.parse_block_tail_(lo, DefaultBlock, next))
    }

    /// Precondition: already parsed the '{' or '#{'
    /// I guess that also means "already parsed the 'impure'" if
    /// necessary, and this should take a qualifier.
    /// Some blocks start with "#{"...
    fn parse_block_tail(&mut self, lo: BytePos, s: BlockCheckMode) -> P<Block> {
        self.parse_block_tail_(lo, s, Vec::new())
    }

    /// Parse the rest of a block expression or function body
    fn parse_block_tail_(&mut self, lo: BytePos, s: BlockCheckMode,
                         first_item_attrs: Vec<Attribute> ) -> P<Block> {
        let mut stmts = Vec::new();
        let mut expr = None;

        // wouldn't it be more uniform to parse view items only, here?
        let ParsedItemsAndViewItems {
            attrs_remaining,
            view_items,
            items,
            ..
        } = self.parse_items_and_view_items(first_item_attrs,
                                            false, false);

        for item in items.into_iter() {
            let span = item.span;
            let decl = P(spanned(span.lo, span.hi, DeclItem(item)));
            stmts.push(P(spanned(span.lo, span.hi, StmtDecl(decl, ast::DUMMY_NODE_ID))));
        }

        let mut attributes_box = attrs_remaining;

        while self.token != token::CloseDelim(token::Brace) {
            // parsing items even when they're not allowed lets us give
            // better error messages and recover more gracefully.
            attributes_box.push_all(self.parse_outer_attributes().as_slice());
            match self.token {
                token::Semi => {
                    if !attributes_box.is_empty() {
                        let last_span = self.last_span;
                        self.span_err(last_span,
                                      Parser::expected_item_err(attributes_box.as_slice()));
                        attributes_box = Vec::new();
                    }
                    self.bump(); // empty
                }
                token::CloseDelim(token::Brace) => {
                    // fall through and out.
                }
                _ => {
                    let stmt = self.parse_stmt(attributes_box);
                    attributes_box = Vec::new();
                    stmt.and_then(|Spanned {node, span}| match node {
                        StmtExpr(e, stmt_id) => {
                            // expression without semicolon
                            if classify::expr_requires_semi_to_be_stmt(&*e) {
                                // Just check for errors and recover; do not eat semicolon yet.
                                self.commit_stmt(&[], &[token::Semi,
                                    token::CloseDelim(token::Brace)]);
                            }

                            match self.token {
                                token::Semi => {
                                    self.bump();
                                    let span_with_semi = Span {
                                        lo: span.lo,
                                        hi: self.last_span.hi,
                                        expn_id: span.expn_id,
                                    };
                                    stmts.push(P(Spanned {
                                        node: StmtSemi(e, stmt_id),
                                        span: span_with_semi,
                                    }));
                                }
                                token::CloseDelim(token::Brace) => {
                                    expr = Some(e);
                                }
                                _ => {
                                    stmts.push(P(Spanned {
                                        node: StmtExpr(e, stmt_id),
                                        span: span
                                    }));
                                }
                            }
                        }
                        StmtMac(m, semi) => {
                            // statement macro; might be an expr
                            match self.token {
                                token::Semi => {
                                    stmts.push(P(Spanned {
                                        node: StmtMac(m, true),
                                        span: span,
                                    }));
                                    self.bump();
                                }
                                token::CloseDelim(token::Brace) => {
                                    // if a block ends in `m!(arg)` without
                                    // a `;`, it must be an expr
                                    expr = Some(
                                        self.mk_mac_expr(span.lo,
                                                         span.hi,
                                                         m.node));
                                }
                                _ => {
                                    stmts.push(P(Spanned {
                                        node: StmtMac(m, semi),
                                        span: span
                                    }));
                                }
                            }
                        }
                        _ => { // all other kinds of statements:
                            if classify::stmt_ends_with_semi(&node) {
                                self.commit_stmt_expecting(token::Semi);
                            }

                            stmts.push(P(Spanned {
                                node: node,
                                span: span
                            }));
                        }
                    })
                }
            }
        }

        if !attributes_box.is_empty() {
            let last_span = self.last_span;
            self.span_err(last_span,
                          Parser::expected_item_err(attributes_box.as_slice()));
        }

        let hi = self.span.hi;
        self.bump();
        P(ast::Block {
            view_items: view_items,
            stmts: stmts,
            expr: expr,
            id: ast::DUMMY_NODE_ID,
            rules: s,
            span: mk_sp(lo, hi),
        })
    }

    // Parses a sequence of bounds if a `:` is found,
    // otherwise returns empty list.
    fn parse_colon_then_ty_param_bounds(&mut self)
                                        -> OwnedSlice<TyParamBound>
    {
        if !self.eat(&token::Colon) {
            OwnedSlice::empty()
        } else {
            self.parse_ty_param_bounds()
        }
    }

    // matches bounds    = ( boundseq )?
    // where   boundseq  = ( polybound + boundseq ) | polybound
    // and     polybound = ( 'for' '<' 'region '>' )? bound
    // and     bound     = 'region | trait_ref
    // NB: The None/Some distinction is important for issue #7264.
    fn parse_ty_param_bounds(&mut self)
                             -> OwnedSlice<TyParamBound>
    {
        let mut result = vec!();
        loop {
            match self.token {
                token::Lifetime(lifetime) => {
                    result.push(RegionTyParamBound(ast::Lifetime {
                        id: ast::DUMMY_NODE_ID,
                        span: self.span,
                        name: lifetime.name
                    }));
                    self.bump();
                }
                token::ModSep | token::Ident(..) => {
                    let poly_trait_ref = self.parse_poly_trait_ref();
                    result.push(TraitTyParamBound(poly_trait_ref))
                }
                _ => break,
            }

            if !self.eat(&token::BinOp(token::Plus)) {
                break;
            }
        }

        return OwnedSlice::from_vec(result);
    }

    fn trait_ref_from_ident(ident: Ident, span: Span) -> TraitRef {
        let segment = ast::PathSegment {
            identifier: ident,
            parameters: ast::PathParameters::none()
        };
        let path = ast::Path {
            span: span,
            global: false,
            segments: vec![segment],
        };
        ast::TraitRef {
            path: path,
            ref_id: ast::DUMMY_NODE_ID,
        }
    }

    /// Matches typaram = (unbound`?`)? IDENT optbounds ( EQ ty )?
    fn parse_ty_param(&mut self) -> TyParam {
        // This is a bit hacky. Currently we are only interested in a single
        // unbound, and it may only be `Sized`. To avoid backtracking and other
        // complications, we parse an ident, then check for `?`. If we find it,
        // we use the ident as the unbound, otherwise, we use it as the name of
        // type param.
        let mut span = self.span;
        let mut ident = self.parse_ident();
        let mut unbound = None;
        if self.eat(&token::Question) {
            let tref = Parser::trait_ref_from_ident(ident, span);
            unbound = Some(tref);
            span = self.span;
            ident = self.parse_ident();
        }

        let bounds = self.parse_colon_then_ty_param_bounds();

        let default = if self.check(&token::Eq) {
            self.bump();
            Some(self.parse_ty_sum())
        }
        else { None };

        TyParam {
            ident: ident,
            id: ast::DUMMY_NODE_ID,
            bounds: bounds,
            unbound: unbound,
            default: default,
            span: span,
        }
    }

    /// Parse a set of optional generic type parameter declarations. Where
    /// clauses are not parsed here, and must be added later via
    /// `parse_where_clause()`.
    ///
    /// matches generics = ( ) | ( < > ) | ( < typaramseq ( , )? > ) | ( < lifetimes ( , )? > )
    ///                  | ( < lifetimes , typaramseq ( , )? > )
    /// where   typaramseq = ( typaram ) | ( typaram , typaramseq )
    pub fn parse_generics(&mut self) -> ast::Generics {
        if self.eat(&token::Lt) {
            let lifetime_defs = self.parse_lifetime_defs();
            let mut seen_default = false;
            let ty_params = self.parse_seq_to_gt(Some(token::Comma), |p| {
                p.forbid_lifetime();
                let ty_param = p.parse_ty_param();
                if ty_param.default.is_some() {
                    seen_default = true;
                } else if seen_default {
                    let last_span = p.last_span;
                    p.span_err(last_span,
                               "type parameters with a default must be trailing");
                }
                ty_param
            });
            ast::Generics {
                lifetimes: lifetime_defs,
                ty_params: ty_params,
                where_clause: WhereClause {
                    id: ast::DUMMY_NODE_ID,
                    predicates: Vec::new(),
                }
            }
        } else {
            ast_util::empty_generics()
        }
    }

    fn parse_generic_values_after_lt(&mut self)
                                     -> (Vec<ast::Lifetime>, Vec<P<Ty>>, Vec<P<TypeBinding>>) {
        let lifetimes = self.parse_lifetimes(token::Comma);

        // First parse types.
        let (types, returned) = self.parse_seq_to_gt_or_return(
            Some(token::Comma),
            |p| {
                p.forbid_lifetime();
                if p.look_ahead(1, |t| t == &token::Eq) {
                    None
                } else {
                    Some(p.parse_ty_sum())
                }
            }
        );

        // If we found the `>`, don't continue.
        if !returned {
            return (lifetimes, types.into_vec(), Vec::new());
        }

        // Then parse type bindings.
        let bindings = self.parse_seq_to_gt(
            Some(token::Comma),
            |p| {
                p.forbid_lifetime();
                let lo = p.span.lo;
                let ident = p.parse_ident();
                let found_eq = p.eat(&token::Eq);
                if !found_eq {
                    let span = p.span;
                    p.span_warn(span, "whoops, no =?");
                }
                let ty = p.parse_ty();
                let hi = p.span.hi;
                let span = mk_sp(lo, hi);
                return P(TypeBinding{id: ast::DUMMY_NODE_ID,
                    ident: ident,
                    ty: ty,
                    span: span,
                });
            }
        );
        (lifetimes, types.into_vec(), bindings.into_vec())
    }

    fn forbid_lifetime(&mut self) {
        if self.token.is_lifetime() {
            let span = self.span;
            self.span_fatal(span, "lifetime parameters must be declared \
                                        prior to type parameters");
        }
    }

    /// Parses an optional `where` clause and places it in `generics`.
    fn parse_where_clause(&mut self, generics: &mut ast::Generics) {
        if !self.eat_keyword(keywords::Where) {
            return
        }

        let mut parsed_something = false;
        loop {
            let lo = self.span.lo;
            let path = match self.token {
                token::Ident(..) => self.parse_path(NoTypesAllowed),
                _ => break,
            };

            if self.eat(&token::Colon) {
                let bounds = self.parse_ty_param_bounds();
                let hi = self.span.hi;
                let span = mk_sp(lo, hi);

                if bounds.len() == 0 {
                    self.span_err(span,
                                  "each predicate in a `where` clause must have \
                                   at least one bound in it");
                }

                let ident = match ast_util::path_to_ident(&path) {
                    Some(ident) => ident,
                    None => {
                        self.span_err(path.span, "expected a single identifier \
                                                  in bound where clause");
                        break;
                    }
                };

                generics.where_clause.predicates.push(
                    ast::WherePredicate::BoundPredicate(ast::WhereBoundPredicate {
                        id: ast::DUMMY_NODE_ID,
                        span: span,
                        ident: ident,
                        bounds: bounds,
                }));
                parsed_something = true;
            } else if self.eat(&token::Eq) {
                let ty = self.parse_ty();
                let hi = self.span.hi;
                let span = mk_sp(lo, hi);
                generics.where_clause.predicates.push(
                    ast::WherePredicate::EqPredicate(ast::WhereEqPredicate {
                        id: ast::DUMMY_NODE_ID,
                        span: span,
                        path: path,
                        ty: ty,
                }));
                parsed_something = true;
                // FIXME(#18433)
                self.span_err(span, "equality constraints are not yet supported in where clauses");
            } else {
                let last_span = self.last_span;
                self.span_err(last_span,
                              "unexpected token in `where` clause");
            }

            if !self.eat(&token::Comma) {
                break
            }
        }

        if !parsed_something {
            let last_span = self.last_span;
            self.span_err(last_span,
                          "a `where` clause must have at least one predicate \
                           in it");
        }
    }

    fn parse_fn_args(&mut self, named_args: bool, allow_variadic: bool)
                     -> (Vec<Arg> , bool) {
        let sp = self.span;
        let mut args: Vec<Option<Arg>> =
            self.parse_unspanned_seq(
                &token::OpenDelim(token::Paren),
                &token::CloseDelim(token::Paren),
                seq_sep_trailing_allowed(token::Comma),
                |p| {
                    if p.token == token::DotDotDot {
                        p.bump();
                        if allow_variadic {
                            if p.token != token::CloseDelim(token::Paren) {
                                let span = p.span;
                                p.span_fatal(span,
                                    "`...` must be last in argument list for variadic function");
                            }
                        } else {
                            let span = p.span;
                            p.span_fatal(span,
                                         "only foreign functions are allowed to be variadic");
                        }
                        None
                    } else {
                        Some(p.parse_arg_general(named_args))
                    }
                }
            );

        let variadic = match args.pop() {
            Some(None) => true,
            Some(x) => {
                // Need to put back that last arg
                args.push(x);
                false
            }
            None => false
        };

        if variadic && args.is_empty() {
            self.span_err(sp,
                          "variadic function must be declared with at least one named argument");
        }

        let args = args.into_iter().map(|x| x.unwrap()).collect();

        (args, variadic)
    }

    /// Parse the argument list and result type of a function declaration
    pub fn parse_fn_decl(&mut self, allow_variadic: bool) -> P<FnDecl> {

        let (args, variadic) = self.parse_fn_args(true, allow_variadic);
        let ret_ty = self.parse_ret_ty();

        P(FnDecl {
            inputs: args,
            output: ret_ty,
            variadic: variadic
        })
    }

    fn is_self_ident(&mut self) -> bool {
        match self.token {
          token::Ident(id, token::Plain) => id.name == special_idents::self_.name,
          _ => false
        }
    }

    fn expect_self_ident(&mut self) -> ast::Ident {
        match self.token {
            token::Ident(id, token::Plain) if id.name == special_idents::self_.name => {
                self.bump();
                id
            },
            _ => {
                let token_str = self.this_token_to_string();
                self.fatal(format!("expected `self`, found `{}`",
                                   token_str).as_slice())
            }
        }
    }

    /// Parse the argument list and result type of a function
    /// that may have a self type.
    fn parse_fn_decl_with_self<F>(&mut self, parse_arg_fn: F) -> (ExplicitSelf, P<FnDecl>) where
        F: FnMut(&mut Parser) -> Arg,
    {
        fn maybe_parse_borrowed_explicit_self(this: &mut Parser)
                                              -> ast::ExplicitSelf_ {
            // The following things are possible to see here:
            //
            //     fn(&mut self)
            //     fn(&mut self)
            //     fn(&'lt self)
            //     fn(&'lt mut self)
            //
            // We already know that the current token is `&`.

            if this.look_ahead(1, |t| t.is_keyword(keywords::Self)) {
                this.bump();
                SelfRegion(None, MutImmutable, this.expect_self_ident())
            } else if this.look_ahead(1, |t| t.is_mutability()) &&
                      this.look_ahead(2, |t| t.is_keyword(keywords::Self)) {
                this.bump();
                let mutability = this.parse_mutability();
                SelfRegion(None, mutability, this.expect_self_ident())
            } else if this.look_ahead(1, |t| t.is_lifetime()) &&
                      this.look_ahead(2, |t| t.is_keyword(keywords::Self)) {
                this.bump();
                let lifetime = this.parse_lifetime();
                SelfRegion(Some(lifetime), MutImmutable, this.expect_self_ident())
            } else if this.look_ahead(1, |t| t.is_lifetime()) &&
                      this.look_ahead(2, |t| t.is_mutability()) &&
                      this.look_ahead(3, |t| t.is_keyword(keywords::Self)) {
                this.bump();
                let lifetime = this.parse_lifetime();
                let mutability = this.parse_mutability();
                SelfRegion(Some(lifetime), mutability, this.expect_self_ident())
            } else {
                SelfStatic
            }
        }

        self.expect(&token::OpenDelim(token::Paren));

        // A bit of complexity and lookahead is needed here in order to be
        // backwards compatible.
        let lo = self.span.lo;
        let mut self_ident_lo = self.span.lo;
        let mut self_ident_hi = self.span.hi;

        let mut mutbl_self = MutImmutable;
        let explicit_self = match self.token {
            token::BinOp(token::And) => {
                let eself = maybe_parse_borrowed_explicit_self(self);
                self_ident_lo = self.last_span.lo;
                self_ident_hi = self.last_span.hi;
                eself
            }
            token::Tilde => {
                // We need to make sure it isn't a type
                if self.look_ahead(1, |t| t.is_keyword(keywords::Self)) {
                    self.bump();
                    drop(self.expect_self_ident());
                    let last_span = self.last_span;
                    self.obsolete(last_span, ObsoleteOwnedSelf)
                }
                SelfStatic
            }
            token::BinOp(token::Star) => {
                // Possibly "*self" or "*mut self" -- not supported. Try to avoid
                // emitting cryptic "unexpected token" errors.
                self.bump();
                let _mutability = if self.token.is_mutability() {
                    self.parse_mutability()
                } else {
                    MutImmutable
                };
                if self.is_self_ident() {
                    let span = self.span;
                    self.span_err(span, "cannot pass self by unsafe pointer");
                    self.bump();
                }
                // error case, making bogus self ident:
                SelfValue(special_idents::self_)
            }
            token::Ident(..) => {
                if self.is_self_ident() {
                    let self_ident = self.expect_self_ident();

                    // Determine whether this is the fully explicit form, `self:
                    // TYPE`.
                    if self.eat(&token::Colon) {
                        SelfExplicit(self.parse_ty_sum(), self_ident)
                    } else {
                        SelfValue(self_ident)
                    }
                } else if self.token.is_mutability() &&
                        self.look_ahead(1, |t| t.is_keyword(keywords::Self)) {
                    mutbl_self = self.parse_mutability();
                    let self_ident = self.expect_self_ident();

                    // Determine whether this is the fully explicit form,
                    // `self: TYPE`.
                    if self.eat(&token::Colon) {
                        SelfExplicit(self.parse_ty_sum(), self_ident)
                    } else {
                        SelfValue(self_ident)
                    }
                } else if self.token.is_mutability() &&
                        self.look_ahead(1, |t| *t == token::Tilde) &&
                        self.look_ahead(2, |t| t.is_keyword(keywords::Self)) {
                    mutbl_self = self.parse_mutability();
                    self.bump();
                    drop(self.expect_self_ident());
                    let last_span = self.last_span;
                    self.obsolete(last_span, ObsoleteOwnedSelf);
                    SelfStatic
                } else {
                    SelfStatic
                }
            }
            _ => SelfStatic,
        };

        let explicit_self_sp = mk_sp(self_ident_lo, self_ident_hi);

        // shared fall-through for the three cases below. borrowing prevents simply
        // writing this as a closure
        macro_rules! parse_remaining_arguments {
            ($self_id:ident) =>
            {
            // If we parsed a self type, expect a comma before the argument list.
            match self.token {
                token::Comma => {
                    self.bump();
                    let sep = seq_sep_trailing_allowed(token::Comma);
                    let mut fn_inputs = self.parse_seq_to_before_end(
                        &token::CloseDelim(token::Paren),
                        sep,
                        parse_arg_fn
                    );
                    fn_inputs.insert(0, Arg::new_self(explicit_self_sp, mutbl_self, $self_id));
                    fn_inputs
                }
                token::CloseDelim(token::Paren) => {
                    vec!(Arg::new_self(explicit_self_sp, mutbl_self, $self_id))
                }
                _ => {
                    let token_str = self.this_token_to_string();
                    self.fatal(format!("expected `,` or `)`, found `{}`",
                                       token_str).as_slice())
                }
            }
            }
        }

        let fn_inputs = match explicit_self {
            SelfStatic =>  {
                let sep = seq_sep_trailing_allowed(token::Comma);
                self.parse_seq_to_before_end(&token::CloseDelim(token::Paren), sep, parse_arg_fn)
            }
            SelfValue(id) => parse_remaining_arguments!(id),
            SelfRegion(_,_,id) => parse_remaining_arguments!(id),
            SelfExplicit(_,id) => parse_remaining_arguments!(id),
        };


        self.expect(&token::CloseDelim(token::Paren));

        let hi = self.span.hi;

        let ret_ty = self.parse_ret_ty();

        let fn_decl = P(FnDecl {
            inputs: fn_inputs,
            output: ret_ty,
            variadic: false
        });

        (spanned(lo, hi, explicit_self), fn_decl)
    }

    // parse the |arg, arg| header on a lambda
    fn parse_fn_block_decl(&mut self)
                           -> (P<FnDecl>, Option<UnboxedClosureKind>) {
        let (optional_unboxed_closure_kind, inputs_captures) = {
            if self.eat(&token::OrOr) {
                (None, Vec::new())
            } else {
                self.expect(&token::BinOp(token::Or));
                let optional_unboxed_closure_kind =
                    self.parse_optional_unboxed_closure_kind();
                let args = self.parse_seq_to_before_end(
                    &token::BinOp(token::Or),
                    seq_sep_trailing_allowed(token::Comma),
                    |p| p.parse_fn_block_arg()
                );
                self.bump();
                (optional_unboxed_closure_kind, args)
            }
        };
        let output = if self.check(&token::RArrow) {
            self.parse_ret_ty()
        } else {
            Return(P(Ty {
                id: ast::DUMMY_NODE_ID,
                node: TyInfer,
                span: self.span,
            }))
        };

        (P(FnDecl {
            inputs: inputs_captures,
            output: output,
            variadic: false
        }), optional_unboxed_closure_kind)
    }

    /// Parses the `(arg, arg) -> return_type` header on a procedure.
    fn parse_proc_decl(&mut self) -> P<FnDecl> {
        let inputs =
            self.parse_unspanned_seq(&token::OpenDelim(token::Paren),
                                     &token::CloseDelim(token::Paren),
                                     seq_sep_trailing_allowed(token::Comma),
                                     |p| p.parse_fn_block_arg());

        let output = if self.check(&token::RArrow) {
            self.parse_ret_ty()
        } else {
            Return(P(Ty {
                id: ast::DUMMY_NODE_ID,
                node: TyInfer,
                span: self.span,
            }))
        };

        P(FnDecl {
            inputs: inputs,
            output: output,
            variadic: false
        })
    }

    /// Parse the name and optional generic types of a function header.
    fn parse_fn_header(&mut self) -> (Ident, ast::Generics) {
        let id = self.parse_ident();
        let generics = self.parse_generics();
        (id, generics)
    }

    fn mk_item(&mut self, lo: BytePos, hi: BytePos, ident: Ident,
               node: Item_, vis: Visibility,
               attrs: Vec<Attribute>) -> P<Item> {
        P(Item {
            ident: ident,
            attrs: attrs,
            id: ast::DUMMY_NODE_ID,
            node: node,
            vis: vis,
            span: mk_sp(lo, hi)
        })
    }

    /// Parse an item-position function declaration.
    fn parse_item_fn(&mut self, fn_style: FnStyle, abi: abi::Abi) -> ItemInfo {
        let (ident, mut generics) = self.parse_fn_header();
        let decl = self.parse_fn_decl(false);
        self.parse_where_clause(&mut generics);
        let (inner_attrs, body) = self.parse_inner_attrs_and_block();
        (ident, ItemFn(decl, fn_style, abi, generics, body), Some(inner_attrs))
    }

    /// Parse a method in a trait impl
    pub fn parse_method_with_outer_attributes(&mut self) -> P<Method> {
        let attrs = self.parse_outer_attributes();
        let visa = self.parse_visibility();
        self.parse_method(attrs, visa)
    }

    /// Parse a method in a trait impl, starting with `attrs` attributes.
    pub fn parse_method(&mut self,
                        attrs: Vec<Attribute>,
                        visa: Visibility)
                        -> P<Method> {
        let lo = self.span.lo;

        // code copied from parse_macro_use_or_failure... abstraction!
        let (method_, hi, new_attrs) = {
            if !self.token.is_any_keyword()
                && self.look_ahead(1, |t| *t == token::Not)
                && (self.look_ahead(2, |t| *t == token::OpenDelim(token::Paren))
                    || self.look_ahead(2, |t| *t == token::OpenDelim(token::Brace))) {
                // method macro.
                let pth = self.parse_path(NoTypesAllowed);
                self.expect(&token::Not);

                // eat a matched-delimiter token tree:
                let delim = self.expect_open_delim();
                let tts = self.parse_seq_to_end(&token::CloseDelim(delim),
                                                seq_sep_none(),
                                                |p| p.parse_token_tree());
                let m_ = ast::MacInvocTT(pth, tts, EMPTY_CTXT);
                let m: ast::Mac = codemap::Spanned { node: m_,
                                                 span: mk_sp(self.span.lo,
                                                             self.span.hi) };
                (ast::MethMac(m), self.span.hi, attrs)
            } else {
                let fn_style = self.parse_fn_style();
                let abi = if self.eat_keyword(keywords::Extern) {
                    self.parse_opt_abi().unwrap_or(abi::C)
                } else {
                    abi::Rust
                };
                self.expect_keyword(keywords::Fn);
                let ident = self.parse_ident();
                let mut generics = self.parse_generics();
                let (explicit_self, decl) = self.parse_fn_decl_with_self(|p| {
                        p.parse_arg()
                    });
                self.parse_where_clause(&mut generics);
                let (inner_attrs, body) = self.parse_inner_attrs_and_block();
                let body_span = body.span;
                let mut new_attrs = attrs;
                new_attrs.push_all(inner_attrs.as_slice());
                (ast::MethDecl(ident,
                               generics,
                               abi,
                               explicit_self,
                               fn_style,
                               decl,
                               body,
                               visa),
                 body_span.hi, new_attrs)
            }
        };
        P(ast::Method {
            attrs: new_attrs,
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, hi),
            node: method_,
        })
    }

    /// Parse trait Foo { ... }
    fn parse_item_trait(&mut self) -> ItemInfo {
        let ident = self.parse_ident();
        let mut tps = self.parse_generics();
        let sized = self.parse_for_sized();

        // Parse supertrait bounds.
        let bounds = self.parse_colon_then_ty_param_bounds();

        self.parse_where_clause(&mut tps);

        let meths = self.parse_trait_items();
        (ident, ItemTrait(tps, sized, bounds, meths), None)
    }

    fn parse_impl_items(&mut self) -> (Vec<ImplItem>, Vec<Attribute>) {
        let mut impl_items = Vec::new();
        self.expect(&token::OpenDelim(token::Brace));
        let (inner_attrs, mut method_attrs) =
            self.parse_inner_attrs_and_next();
        while !self.eat(&token::CloseDelim(token::Brace)) {
            method_attrs.extend(self.parse_outer_attributes().into_iter());
            let vis = self.parse_visibility();
            if self.eat_keyword(keywords::Type) {
                impl_items.push(TypeImplItem(P(self.parse_typedef(
                            method_attrs,
                            vis))))
            } else {
                impl_items.push(MethodImplItem(self.parse_method(
                            method_attrs,
                            vis)));
            }
            method_attrs = self.parse_outer_attributes();
        }
        (impl_items, inner_attrs)
    }

    /// Parses two variants (with the region/type params always optional):
    ///    impl<T> Foo { ... }
    ///    impl<T> ToString for ~[T] { ... }
    fn parse_item_impl(&mut self) -> ItemInfo {
        // First, parse type parameters if necessary.
        let mut generics = self.parse_generics();

        // Special case: if the next identifier that follows is '(', don't
        // allow this to be parsed as a trait.
        let could_be_trait = self.token != token::OpenDelim(token::Paren);

        // Parse the trait.
        let mut ty = self.parse_ty_sum();

        // Parse traits, if necessary.
        let opt_trait = if could_be_trait && self.eat_keyword(keywords::For) {
            // New-style trait. Reinterpret the type as a trait.
            let opt_trait_ref = match ty.node {
                TyPath(ref path, node_id) => {
                    Some(TraitRef {
                        path: (*path).clone(),
                        ref_id: node_id,
                    })
                }
                _ => {
                    self.span_err(ty.span, "not a trait");
                    None
                }
            };

            ty = self.parse_ty_sum();
            opt_trait_ref
        } else {
            None
        };

        self.parse_where_clause(&mut generics);
        let (impl_items, attrs) = self.parse_impl_items();

        let ident = ast_util::impl_pretty_name(&opt_trait, &*ty);

        (ident,
         ItemImpl(generics, opt_trait, ty, impl_items),
         Some(attrs))
    }

    /// Parse a::B<String,int>
    fn parse_trait_ref(&mut self) -> TraitRef {
        ast::TraitRef {
            path: self.parse_path(LifetimeAndTypesWithoutColons),
            ref_id: ast::DUMMY_NODE_ID,
        }
    }

    fn parse_late_bound_lifetime_defs(&mut self) -> Vec<ast::LifetimeDef> {
        if self.eat_keyword(keywords::For) {
            self.expect(&token::Lt);
            let lifetime_defs = self.parse_lifetime_defs();
            self.expect_gt();
            lifetime_defs
        } else {
            Vec::new()
        }
    }

    /// Parse for<'l> a::B<String,int>
    fn parse_poly_trait_ref(&mut self) -> PolyTraitRef {
        let lifetime_defs = self.parse_late_bound_lifetime_defs();

        ast::PolyTraitRef {
            bound_lifetimes: lifetime_defs,
            trait_ref: self.parse_trait_ref()
        }
    }

    /// Parse struct Foo { ... }
    fn parse_item_struct(&mut self) -> ItemInfo {
        let class_name = self.parse_ident();
        let mut generics = self.parse_generics();

        if self.eat(&token::Colon) {
            let ty = self.parse_ty_sum();
            self.span_err(ty.span, "`virtual` structs have been removed from the language");
        }

        self.parse_where_clause(&mut generics);

        let mut fields: Vec<StructField>;
        let is_tuple_like;

        if self.eat(&token::OpenDelim(token::Brace)) {
            // It's a record-like struct.
            is_tuple_like = false;
            fields = Vec::new();
            while self.token != token::CloseDelim(token::Brace) {
                fields.push(self.parse_struct_decl_field(true));
            }
            if fields.len() == 0 {
                self.fatal(format!("unit-like struct definition should be \
                                    written as `struct {};`",
                                   token::get_ident(class_name)).as_slice());
            }
            self.bump();
        } else if self.check(&token::OpenDelim(token::Paren)) {
            // It's a tuple-like struct.
            is_tuple_like = true;
            fields = self.parse_unspanned_seq(
                &token::OpenDelim(token::Paren),
                &token::CloseDelim(token::Paren),
                seq_sep_trailing_allowed(token::Comma),
                |p| {
                let attrs = p.parse_outer_attributes();
                let lo = p.span.lo;
                let struct_field_ = ast::StructField_ {
                    kind: UnnamedField(p.parse_visibility()),
                    id: ast::DUMMY_NODE_ID,
                    ty: p.parse_ty_sum(),
                    attrs: attrs,
                };
                spanned(lo, p.span.hi, struct_field_)
            });
            if fields.len() == 0 {
                self.fatal(format!("unit-like struct definition should be \
                                    written as `struct {};`",
                                   token::get_ident(class_name)).as_slice());
            }
            self.expect(&token::Semi);
        } else if self.eat(&token::Semi) {
            // It's a unit-like struct.
            is_tuple_like = true;
            fields = Vec::new();
        } else {
            let token_str = self.this_token_to_string();
            self.fatal(format!("expected `{}`, `(`, or `;` after struct \
                                name, found `{}`", "{",
                               token_str).as_slice())
        }

        let _ = ast::DUMMY_NODE_ID;  // FIXME: Workaround for crazy bug.
        let new_id = ast::DUMMY_NODE_ID;
        (class_name,
         ItemStruct(P(ast::StructDef {
             fields: fields,
             ctor_id: if is_tuple_like { Some(new_id) } else { None },
         }), generics),
         None)
    }

    /// Parse a structure field declaration
    pub fn parse_single_struct_field(&mut self,
                                     vis: Visibility,
                                     attrs: Vec<Attribute> )
                                     -> StructField {
        let a_var = self.parse_name_and_ty(vis, attrs);
        match self.token {
            token::Comma => {
                self.bump();
            }
            token::CloseDelim(token::Brace) => {}
            _ => {
                let span = self.span;
                let token_str = self.this_token_to_string();
                self.span_fatal_help(span,
                                     format!("expected `,`, or `}}`, found `{}`",
                                             token_str).as_slice(),
                                     "struct fields should be separated by commas")
            }
        }
        a_var
    }

    /// Parse an element of a struct definition
    fn parse_struct_decl_field(&mut self, allow_pub: bool) -> StructField {

        let attrs = self.parse_outer_attributes();

        if self.eat_keyword(keywords::Pub) {
            if !allow_pub {
                let span = self.last_span;
                self.span_err(span, "`pub` is not allowed here");
            }
            return self.parse_single_struct_field(Public, attrs);
        }

        return self.parse_single_struct_field(Inherited, attrs);
    }

    /// Parse visibility: PUB, PRIV, or nothing
    fn parse_visibility(&mut self) -> Visibility {
        if self.eat_keyword(keywords::Pub) { Public }
        else { Inherited }
    }

    fn parse_for_sized(&mut self) -> Option<ast::TraitRef> {
        if self.eat_keyword(keywords::For) {
            let span = self.span;
            let ident = self.parse_ident();
            if !self.eat(&token::Question) {
                self.span_err(span,
                    "expected 'Sized?' after `for` in trait item");
                return None;
            }
            let tref = Parser::trait_ref_from_ident(ident, span);
            Some(tref)
        } else {
            None
        }
    }

    /// Given a termination token and a vector of already-parsed
    /// attributes (of length 0 or 1), parse all of the items in a module
    fn parse_mod_items(&mut self,
                       term: token::Token,
                       first_item_attrs: Vec<Attribute>,
                       inner_lo: BytePos)
                       -> Mod {
        // parse all of the items up to closing or an attribute.
        // view items are legal here.
        let ParsedItemsAndViewItems {
            attrs_remaining,
            view_items,
            items: starting_items,
            ..
        } = self.parse_items_and_view_items(first_item_attrs, true, true);
        let mut items: Vec<P<Item>> = starting_items;
        let attrs_remaining_len = attrs_remaining.len();

        // don't think this other loop is even necessary....

        let mut first = true;
        while self.token != term {
            let mut attrs = self.parse_outer_attributes();
            if first {
                let mut tmp = attrs_remaining.clone();
                tmp.push_all(attrs.as_slice());
                attrs = tmp;
                first = false;
            }
            debug!("parse_mod_items: parse_item_or_view_item(attrs={})",
                   attrs);
            match self.parse_item_or_view_item(attrs,
                                               true /* macros allowed */) {
              IoviItem(item) => items.push(item),
              IoviViewItem(view_item) => {
                self.span_fatal(view_item.span,
                                "view items must be declared at the top of \
                                 the module");
              }
              _ => {
                  let token_str = self.this_token_to_string();
                  self.fatal(format!("expected item, found `{}`",
                                     token_str).as_slice())
              }
            }
        }

        if first && attrs_remaining_len > 0u {
            // We parsed attributes for the first item but didn't find it
            let last_span = self.last_span;
            self.span_err(last_span,
                          Parser::expected_item_err(attrs_remaining.as_slice()));
        }

        ast::Mod {
            inner: mk_sp(inner_lo, self.span.lo),
            view_items: view_items,
            items: items
        }
    }

    fn parse_item_const(&mut self, m: Option<Mutability>) -> ItemInfo {
        let id = self.parse_ident();
        self.expect(&token::Colon);
        let ty = self.parse_ty_sum();
        self.expect(&token::Eq);
        let e = self.parse_expr();
        self.commit_expr_expecting(&*e, token::Semi);
        let item = match m {
            Some(m) => ItemStatic(ty, m, e),
            None => ItemConst(ty, e),
        };
        (id, item, None)
    }

    /// Parse a `mod <foo> { ... }` or `mod <foo>;` item
    fn parse_item_mod(&mut self, outer_attrs: &[Attribute]) -> ItemInfo {
        let id_span = self.span;
        let id = self.parse_ident();
        if self.check(&token::Semi) {
            self.bump();
            // This mod is in an external file. Let's go get it!
            let (m, attrs) = self.eval_src_mod(id, outer_attrs, id_span);
            (id, m, Some(attrs))
        } else {
            self.push_mod_path(id, outer_attrs);
            self.expect(&token::OpenDelim(token::Brace));
            let mod_inner_lo = self.span.lo;
            let old_owns_directory = self.owns_directory;
            self.owns_directory = true;
            let (inner, next) = self.parse_inner_attrs_and_next();
            let m = self.parse_mod_items(token::CloseDelim(token::Brace), next, mod_inner_lo);
            self.expect(&token::CloseDelim(token::Brace));
            self.owns_directory = old_owns_directory;
            self.pop_mod_path();
            (id, ItemMod(m), Some(inner))
        }
    }

    fn push_mod_path(&mut self, id: Ident, attrs: &[Attribute]) {
        let default_path = self.id_to_interned_str(id);
        let file_path = match ::attr::first_attr_value_str_by_name(attrs,
                                                                   "path") {
            Some(d) => d,
            None => default_path,
        };
        self.mod_path_stack.push(file_path)
    }

    fn pop_mod_path(&mut self) {
        self.mod_path_stack.pop().unwrap();
    }

    /// Read a module from a source file.
    fn eval_src_mod(&mut self,
                    id: ast::Ident,
                    outer_attrs: &[ast::Attribute],
                    id_sp: Span)
                    -> (ast::Item_, Vec<ast::Attribute> ) {
        let mut prefix = Path::new(self.sess.span_diagnostic.cm.span_to_filename(self.span));
        prefix.pop();
        let mod_path = Path::new(".").join_many(self.mod_path_stack.as_slice());
        let dir_path = prefix.join(&mod_path);
        let mod_string = token::get_ident(id);
        let (file_path, owns_directory) = match ::attr::first_attr_value_str_by_name(
                outer_attrs, "path") {
            Some(d) => (dir_path.join(d), true),
            None => {
                let mod_name = mod_string.get().to_string();
                let default_path_str = format!("{}.rs", mod_name);
                let secondary_path_str = format!("{}/mod.rs", mod_name);
                let default_path = dir_path.join(default_path_str.as_slice());
                let secondary_path = dir_path.join(secondary_path_str.as_slice());
                let default_exists = default_path.exists();
                let secondary_exists = secondary_path.exists();

                if !self.owns_directory {
                    self.span_err(id_sp,
                                  "cannot declare a new module at this location");
                    let this_module = match self.mod_path_stack.last() {
                        Some(name) => name.get().to_string(),
                        None => self.root_module_name.as_ref().unwrap().clone(),
                    };
                    self.span_note(id_sp,
                                   format!("maybe move this module `{0}` \
                                            to its own directory via \
                                            `{0}/mod.rs`",
                                           this_module).as_slice());
                    if default_exists || secondary_exists {
                        self.span_note(id_sp,
                                       format!("... or maybe `use` the module \
                                                `{}` instead of possibly \
                                                redeclaring it",
                                               mod_name).as_slice());
                    }
                    self.abort_if_errors();
                }

                match (default_exists, secondary_exists) {
                    (true, false) => (default_path, false),
                    (false, true) => (secondary_path, true),
                    (false, false) => {
                        self.span_fatal_help(id_sp,
                                             format!("file not found for module `{}`",
                                                     mod_name).as_slice(),
                                             format!("name the file either {} or {} inside \
                                                     the directory {}",
                                                     default_path_str,
                                                     secondary_path_str,
                                                     dir_path.display()).as_slice());
                    }
                    (true, true) => {
                        self.span_fatal_help(
                            id_sp,
                            format!("file for module `{}` found at both {} \
                                     and {}",
                                    mod_name,
                                    default_path_str,
                                    secondary_path_str).as_slice(),
                            "delete or rename one of them to remove the ambiguity");
                    }
                }
            }
        };

        self.eval_src_mod_from_path(file_path, owns_directory,
                                    mod_string.get().to_string(), id_sp)
    }

    fn eval_src_mod_from_path(&mut self,
                              path: Path,
                              owns_directory: bool,
                              name: String,
                              id_sp: Span) -> (ast::Item_, Vec<ast::Attribute> ) {
        let mut included_mod_stack = self.sess.included_mod_stack.borrow_mut();
        match included_mod_stack.iter().position(|p| *p == path) {
            Some(i) => {
                let mut err = String::from_str("circular modules: ");
                let len = included_mod_stack.len();
                for p in included_mod_stack.slice(i, len).iter() {
                    err.push_str(p.display().as_cow().as_slice());
                    err.push_str(" -> ");
                }
                err.push_str(path.display().as_cow().as_slice());
                self.span_fatal(id_sp, err.as_slice());
            }
            None => ()
        }
        included_mod_stack.push(path.clone());
        drop(included_mod_stack);

        let mut p0 =
            new_sub_parser_from_file(self.sess,
                                     self.cfg.clone(),
                                     &path,
                                     owns_directory,
                                     Some(name),
                                     id_sp);
        let mod_inner_lo = p0.span.lo;
        let (mod_attrs, next) = p0.parse_inner_attrs_and_next();
        let first_item_outer_attrs = next;
        let m0 = p0.parse_mod_items(token::Eof, first_item_outer_attrs, mod_inner_lo);
        self.sess.included_mod_stack.borrow_mut().pop();
        return (ast::ItemMod(m0), mod_attrs);
    }

    /// Parse a function declaration from a foreign module
    fn parse_item_foreign_fn(&mut self, vis: ast::Visibility,
                             attrs: Vec<Attribute>) -> P<ForeignItem> {
        let lo = self.span.lo;
        self.expect_keyword(keywords::Fn);

        let (ident, mut generics) = self.parse_fn_header();
        let decl = self.parse_fn_decl(true);
        self.parse_where_clause(&mut generics);
        let hi = self.span.hi;
        self.expect(&token::Semi);
        P(ast::ForeignItem {
            ident: ident,
            attrs: attrs,
            node: ForeignItemFn(decl, generics),
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, hi),
            vis: vis
        })
    }

    /// Parse a static item from a foreign module
    fn parse_item_foreign_static(&mut self, vis: ast::Visibility,
                                 attrs: Vec<Attribute>) -> P<ForeignItem> {
        let lo = self.span.lo;

        self.expect_keyword(keywords::Static);
        let mutbl = self.eat_keyword(keywords::Mut);

        let ident = self.parse_ident();
        self.expect(&token::Colon);
        let ty = self.parse_ty_sum();
        let hi = self.span.hi;
        self.expect(&token::Semi);
        P(ForeignItem {
            ident: ident,
            attrs: attrs,
            node: ForeignItemStatic(ty, mutbl),
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, hi),
            vis: vis
        })
    }

    /// Parse unsafe or not
    fn parse_fn_style(&mut self) -> FnStyle {
        if self.eat_keyword(keywords::Unsafe) {
            UnsafeFn
        } else {
            NormalFn
        }
    }


    /// At this point, this is essentially a wrapper for
    /// parse_foreign_items.
    fn parse_foreign_mod_items(&mut self,
                               abi: abi::Abi,
                               first_item_attrs: Vec<Attribute> )
                               -> ForeignMod {
        let ParsedItemsAndViewItems {
            attrs_remaining,
            view_items,
            items: _,
            foreign_items,
        } = self.parse_foreign_items(first_item_attrs, true);
        if !attrs_remaining.is_empty() {
            let last_span = self.last_span;
            self.span_err(last_span,
                          Parser::expected_item_err(attrs_remaining.as_slice()));
        }
        assert!(self.token == token::CloseDelim(token::Brace));
        ast::ForeignMod {
            abi: abi,
            view_items: view_items,
            items: foreign_items
        }
    }

    /// Parse extern crate links
    ///
    /// # Example
    ///
    /// extern crate url;
    /// extern crate foo = "bar"; //deprecated
    /// extern crate "bar" as foo;
    fn parse_item_extern_crate(&mut self,
                                lo: BytePos,
                                visibility: Visibility,
                                attrs: Vec<Attribute> )
                                -> ItemOrViewItem {

        let span = self.span;
        let (maybe_path, ident) = match self.token {
            token::Ident(..) => {
                let the_ident = self.parse_ident();
                let path = if self.token == token::Eq {
                    self.bump();
                    let path = self.parse_str();
                    let span = self.span;
                    self.obsolete(span, ObsoleteExternCrateRenaming);
                    Some(path)
                } else if self.eat_keyword(keywords::As) {
                    // skip the ident if there is one
                    if self.token.is_ident() { self.bump(); }

                    self.span_err(span, "expected `;`, found `as`");
                    self.span_help(span,
                                   format!("perhaps you meant to enclose the crate name `{}` in \
                                           a string?",
                                          the_ident.as_str()).as_slice());
                    None
                } else {
                    None
                };
                self.expect(&token::Semi);
                (path, the_ident)
            },
            token::Literal(token::Str_(..), suf) | token::Literal(token::StrRaw(..), suf) => {
                let sp = self.span;
                self.expect_no_suffix(sp, "extern crate name", suf);
                // forgo the internal suffix check of `parse_str` to
                // avoid repeats (this unwrap will always succeed due
                // to the restriction of the `match`)
                let (s, style, _) = self.parse_optional_str().unwrap();
                self.expect_keyword(keywords::As);
                let the_ident = self.parse_ident();
                self.expect(&token::Semi);
                (Some((s, style)), the_ident)
            },
            _ => {
                let span = self.span;
                let token_str = self.this_token_to_string();
                self.span_fatal(span,
                                format!("expected extern crate name but \
                                         found `{}`",
                                        token_str).as_slice());
            }
        };

        IoviViewItem(ast::ViewItem {
                node: ViewItemExternCrate(ident, maybe_path, ast::DUMMY_NODE_ID),
                attrs: attrs,
                vis: visibility,
                span: mk_sp(lo, self.last_span.hi)
            })
    }

    /// Parse `extern` for foreign ABIs
    /// modules.
    ///
    /// `extern` is expected to have been
    /// consumed before calling this method
    ///
    /// # Examples:
    ///
    /// extern "C" {}
    /// extern {}
    fn parse_item_foreign_mod(&mut self,
                              lo: BytePos,
                              opt_abi: Option<abi::Abi>,
                              visibility: Visibility,
                              attrs: Vec<Attribute> )
                              -> ItemOrViewItem {

        self.expect(&token::OpenDelim(token::Brace));

        let abi = opt_abi.unwrap_or(abi::C);

        let (inner, next) = self.parse_inner_attrs_and_next();
        let m = self.parse_foreign_mod_items(abi, next);
        self.expect(&token::CloseDelim(token::Brace));

        let last_span = self.last_span;
        let item = self.mk_item(lo,
                                last_span.hi,
                                special_idents::invalid,
                                ItemForeignMod(m),
                                visibility,
                                maybe_append(attrs, Some(inner)));
        return IoviItem(item);
    }

    /// Parse type Foo = Bar;
    fn parse_item_type(&mut self) -> ItemInfo {
        let ident = self.parse_ident();
        let mut tps = self.parse_generics();
        self.parse_where_clause(&mut tps);
        self.expect(&token::Eq);
        let ty = self.parse_ty_sum();
        self.expect(&token::Semi);
        (ident, ItemTy(ty, tps), None)
    }

    /// Parse a structure-like enum variant definition
    /// this should probably be renamed or refactored...
    fn parse_struct_def(&mut self) -> P<StructDef> {
        let mut fields: Vec<StructField> = Vec::new();
        while self.token != token::CloseDelim(token::Brace) {
            fields.push(self.parse_struct_decl_field(false));
        }
        self.bump();

        P(StructDef {
            fields: fields,
            ctor_id: None,
        })
    }

    /// Parse the part of an "enum" decl following the '{'
    fn parse_enum_def(&mut self, _generics: &ast::Generics) -> EnumDef {
        let mut variants = Vec::new();
        let mut all_nullary = true;
        let mut any_disr = None;
        while self.token != token::CloseDelim(token::Brace) {
            let variant_attrs = self.parse_outer_attributes();
            let vlo = self.span.lo;

            let vis = self.parse_visibility();

            let ident;
            let kind;
            let mut args = Vec::new();
            let mut disr_expr = None;
            ident = self.parse_ident();
            if self.eat(&token::OpenDelim(token::Brace)) {
                // Parse a struct variant.
                all_nullary = false;
                let start_span = self.span;
                let struct_def = self.parse_struct_def();
                if struct_def.fields.len() == 0 {
                    self.span_err(start_span,
                        format!("unit-like struct variant should be written \
                                 without braces, as `{},`",
                                token::get_ident(ident)).as_slice());
                }
                kind = StructVariantKind(struct_def);
            } else if self.check(&token::OpenDelim(token::Paren)) {
                all_nullary = false;
                let arg_tys = self.parse_enum_variant_seq(
                    &token::OpenDelim(token::Paren),
                    &token::CloseDelim(token::Paren),
                    seq_sep_trailing_allowed(token::Comma),
                    |p| p.parse_ty_sum()
                );
                for ty in arg_tys.into_iter() {
                    args.push(ast::VariantArg {
                        ty: ty,
                        id: ast::DUMMY_NODE_ID,
                    });
                }
                kind = TupleVariantKind(args);
            } else if self.eat(&token::Eq) {
                disr_expr = Some(self.parse_expr());
                any_disr = disr_expr.as_ref().map(|expr| expr.span);
                kind = TupleVariantKind(args);
            } else {
                kind = TupleVariantKind(Vec::new());
            }

            let vr = ast::Variant_ {
                name: ident,
                attrs: variant_attrs,
                kind: kind,
                id: ast::DUMMY_NODE_ID,
                disr_expr: disr_expr,
                vis: vis,
            };
            variants.push(P(spanned(vlo, self.last_span.hi, vr)));

            if !self.eat(&token::Comma) { break; }
        }
        self.expect(&token::CloseDelim(token::Brace));
        match any_disr {
            Some(disr_span) if !all_nullary =>
                self.span_err(disr_span,
                    "discriminator values can only be used with a c-like enum"),
            _ => ()
        }

        ast::EnumDef { variants: variants }
    }

    /// Parse an "enum" declaration
    fn parse_item_enum(&mut self) -> ItemInfo {
        let id = self.parse_ident();
        let mut generics = self.parse_generics();
        self.parse_where_clause(&mut generics);
        self.expect(&token::OpenDelim(token::Brace));

        let enum_definition = self.parse_enum_def(&generics);
        (id, ItemEnum(enum_definition, generics), None)
    }

    fn fn_expr_lookahead(tok: &token::Token) -> bool {
        match *tok {
          token::OpenDelim(token::Paren) | token::At | token::Tilde | token::BinOp(_) => true,
          _ => false
        }
    }

    /// Parses a string as an ABI spec on an extern type or module. Consumes
    /// the `extern` keyword, if one is found.
    fn parse_opt_abi(&mut self) -> Option<abi::Abi> {
        match self.token {
            token::Literal(token::Str_(s), suf) | token::Literal(token::StrRaw(s, _), suf) => {
                let sp = self.span;
                self.expect_no_suffix(sp, "ABI spec", suf);
                self.bump();
                let the_string = s.as_str();
                match abi::lookup(the_string) {
                    Some(abi) => Some(abi),
                    None => {
                        let last_span = self.last_span;
                        self.span_err(
                            last_span,
                            format!("illegal ABI: expected one of [{}], \
                                     found `{}`",
                                    abi::all_names().connect(", "),
                                    the_string).as_slice());
                        None
                    }
                }
            }

            _ => None,
        }
    }

    /// Parse one of the items or view items allowed by the
    /// flags; on failure, return IoviNone.
    /// NB: this function no longer parses the items inside an
    /// extern crate.
    fn parse_item_or_view_item(&mut self,
                               attrs: Vec<Attribute> ,
                               macros_allowed: bool)
                               -> ItemOrViewItem {
        let nt_item = match self.token {
            token::Interpolated(token::NtItem(ref item)) => {
                Some((**item).clone())
            }
            _ => None
        };
        match nt_item {
            Some(mut item) => {
                self.bump();
                let mut attrs = attrs;
                mem::swap(&mut item.attrs, &mut attrs);
                item.attrs.extend(attrs.into_iter());
                return IoviItem(P(item));
            }
            None => {}
        }

        let lo = self.span.lo;

        let visibility = self.parse_visibility();

        // must be a view item:
        if self.eat_keyword(keywords::Use) {
            // USE ITEM (IoviViewItem)
            let view_item = self.parse_use();
            self.expect(&token::Semi);
            return IoviViewItem(ast::ViewItem {
                node: view_item,
                attrs: attrs,
                vis: visibility,
                span: mk_sp(lo, self.last_span.hi)
            });
        }
        // either a view item or an item:
        if self.eat_keyword(keywords::Extern) {
            let next_is_mod = self.eat_keyword(keywords::Mod);

            if next_is_mod || self.eat_keyword(keywords::Crate) {
                if next_is_mod {
                    let last_span = self.last_span;
                    self.span_err(mk_sp(lo, last_span.hi),
                                 format!("`extern mod` is obsolete, use \
                                          `extern crate` instead \
                                          to refer to external \
                                          crates.").as_slice())
                }
                return self.parse_item_extern_crate(lo, visibility, attrs);
            }

            let opt_abi = self.parse_opt_abi();

            if self.eat_keyword(keywords::Fn) {
                // EXTERN FUNCTION ITEM
                let abi = opt_abi.unwrap_or(abi::C);
                let (ident, item_, extra_attrs) =
                    self.parse_item_fn(NormalFn, abi);
                let last_span = self.last_span;
                let item = self.mk_item(lo,
                                        last_span.hi,
                                        ident,
                                        item_,
                                        visibility,
                                        maybe_append(attrs, extra_attrs));
                return IoviItem(item);
            } else if self.check(&token::OpenDelim(token::Brace)) {
                return self.parse_item_foreign_mod(lo, opt_abi, visibility, attrs);
            }

            let span = self.span;
            let token_str = self.this_token_to_string();
            self.span_fatal(span,
                            format!("expected `{}` or `fn`, found `{}`", "{",
                                    token_str).as_slice());
        }

        if self.eat_keyword(keywords::Virtual) {
            let span = self.span;
            self.span_err(span, "`virtual` structs have been removed from the language");
        }

        // the rest are all guaranteed to be items:
        if self.token.is_keyword(keywords::Static) {
            // STATIC ITEM
            self.bump();
            let m = if self.eat_keyword(keywords::Mut) {MutMutable} else {MutImmutable};
            let (ident, item_, extra_attrs) = self.parse_item_const(Some(m));
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return IoviItem(item);
        }
        if self.token.is_keyword(keywords::Const) {
            // CONST ITEM
            self.bump();
            if self.eat_keyword(keywords::Mut) {
                let last_span = self.last_span;
                self.span_err(last_span, "const globals cannot be mutable");
                self.span_help(last_span, "did you mean to declare a static?");
            }
            let (ident, item_, extra_attrs) = self.parse_item_const(None);
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return IoviItem(item);
        }
        if self.token.is_keyword(keywords::Fn) &&
                self.look_ahead(1, |f| !Parser::fn_expr_lookahead(f)) {
            // FUNCTION ITEM
            self.bump();
            let (ident, item_, extra_attrs) =
                self.parse_item_fn(NormalFn, abi::Rust);
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return IoviItem(item);
        }
        if self.token.is_keyword(keywords::Unsafe)
            && self.look_ahead(1u, |t| *t != token::OpenDelim(token::Brace)) {
            // UNSAFE FUNCTION ITEM
            self.bump();
            let abi = if self.eat_keyword(keywords::Extern) {
                self.parse_opt_abi().unwrap_or(abi::C)
            } else {
                abi::Rust
            };
            self.expect_keyword(keywords::Fn);
            let (ident, item_, extra_attrs) =
                self.parse_item_fn(UnsafeFn, abi);
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return IoviItem(item);
        }
        if self.eat_keyword(keywords::Mod) {
            // MODULE ITEM
            let (ident, item_, extra_attrs) =
                self.parse_item_mod(attrs.as_slice());
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return IoviItem(item);
        }
        if self.eat_keyword(keywords::Type) {
            // TYPE ITEM
            let (ident, item_, extra_attrs) = self.parse_item_type();
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return IoviItem(item);
        }
        if self.eat_keyword(keywords::Enum) {
            // ENUM ITEM
            let (ident, item_, extra_attrs) = self.parse_item_enum();
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return IoviItem(item);
        }
        if self.eat_keyword(keywords::Trait) {
            // TRAIT ITEM
            let (ident, item_, extra_attrs) = self.parse_item_trait();
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return IoviItem(item);
        }
        if self.eat_keyword(keywords::Impl) {
            // IMPL ITEM
            let (ident, item_, extra_attrs) = self.parse_item_impl();
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return IoviItem(item);
        }
        if self.eat_keyword(keywords::Struct) {
            // STRUCT ITEM
            let (ident, item_, extra_attrs) = self.parse_item_struct();
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return IoviItem(item);
        }
        self.parse_macro_use_or_failure(attrs,macros_allowed,lo,visibility)
    }

    /// Parse a foreign item; on failure, return IoviNone.
    fn parse_foreign_item(&mut self,
                          attrs: Vec<Attribute> ,
                          macros_allowed: bool)
                          -> ItemOrViewItem {
        maybe_whole!(iovi self, NtItem);
        let lo = self.span.lo;

        let visibility = self.parse_visibility();

        if self.token.is_keyword(keywords::Static) {
            // FOREIGN STATIC ITEM
            let item = self.parse_item_foreign_static(visibility, attrs);
            return IoviForeignItem(item);
        }
        if self.token.is_keyword(keywords::Fn) || self.token.is_keyword(keywords::Unsafe) {
            // FOREIGN FUNCTION ITEM
            let item = self.parse_item_foreign_fn(visibility, attrs);
            return IoviForeignItem(item);
        }
        self.parse_macro_use_or_failure(attrs,macros_allowed,lo,visibility)
    }

    /// This is the fall-through for parsing items.
    fn parse_macro_use_or_failure(
        &mut self,
        attrs: Vec<Attribute> ,
        macros_allowed: bool,
        lo: BytePos,
        visibility: Visibility
    ) -> ItemOrViewItem {
        if macros_allowed && !self.token.is_any_keyword()
                && self.look_ahead(1, |t| *t == token::Not)
                && (self.look_ahead(2, |t| t.is_plain_ident())
                    || self.look_ahead(2, |t| *t == token::OpenDelim(token::Paren))
                    || self.look_ahead(2, |t| *t == token::OpenDelim(token::Brace))) {
            // MACRO INVOCATION ITEM

            // item macro.
            let pth = self.parse_path(NoTypesAllowed);
            self.expect(&token::Not);

            // a 'special' identifier (like what `macro_rules!` uses)
            // is optional. We should eventually unify invoc syntax
            // and remove this.
            let id = if self.token.is_plain_ident() {
                self.parse_ident()
            } else {
                token::special_idents::invalid // no special identifier
            };
            // eat a matched-delimiter token tree:
            let delim = self.expect_open_delim();
            let tts = self.parse_seq_to_end(&token::CloseDelim(delim),
                                            seq_sep_none(),
                                            |p| p.parse_token_tree());
            // single-variant-enum... :
            let m = ast::MacInvocTT(pth, tts, EMPTY_CTXT);
            let m: ast::Mac = codemap::Spanned { node: m,
                                             span: mk_sp(self.span.lo,
                                                         self.span.hi) };
            let item_ = ItemMac(m);
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    id,
                                    item_,
                                    visibility,
                                    attrs);
            return IoviItem(item);
        }

        // FAILURE TO PARSE ITEM
        match visibility {
            Inherited => {}
            Public => {
                let last_span = self.last_span;
                self.span_fatal(last_span, "unmatched visibility `pub`");
            }
        }
        return IoviNone(attrs);
    }

    pub fn parse_item_with_outer_attributes(&mut self) -> Option<P<Item>> {
        let attrs = self.parse_outer_attributes();
        self.parse_item(attrs)
    }

    pub fn parse_item(&mut self, attrs: Vec<Attribute>) -> Option<P<Item>> {
        match self.parse_item_or_view_item(attrs, true) {
            IoviNone(_) => None,
            IoviViewItem(_) =>
                self.fatal("view items are not allowed here"),
            IoviForeignItem(_) =>
                self.fatal("foreign items are not allowed here"),
            IoviItem(item) => Some(item)
        }
    }

    /// Parse a ViewItem, e.g. `use foo::bar` or `extern crate foo`
    pub fn parse_view_item(&mut self, attrs: Vec<Attribute>) -> ViewItem {
        match self.parse_item_or_view_item(attrs, false) {
            IoviViewItem(vi) => vi,
            _ => self.fatal("expected `use` or `extern crate`"),
        }
    }

    /// Parse, e.g., "use a::b::{z,y}"
    fn parse_use(&mut self) -> ViewItem_ {
        return ViewItemUse(self.parse_view_path());
    }


    /// Matches view_path : MOD? IDENT EQ non_global_path
    /// | MOD? non_global_path MOD_SEP LBRACE RBRACE
    /// | MOD? non_global_path MOD_SEP LBRACE ident_seq RBRACE
    /// | MOD? non_global_path MOD_SEP STAR
    /// | MOD? non_global_path
    fn parse_view_path(&mut self) -> P<ViewPath> {
        let lo = self.span.lo;

        if self.check(&token::OpenDelim(token::Brace)) {
            // use {foo,bar}
            let idents = self.parse_unspanned_seq(
                &token::OpenDelim(token::Brace),
                &token::CloseDelim(token::Brace),
                seq_sep_trailing_allowed(token::Comma),
                |p| p.parse_path_list_item());
            let path = ast::Path {
                span: mk_sp(lo, self.span.hi),
                global: false,
                segments: Vec::new()
            };
            return P(spanned(lo, self.span.hi,
                             ViewPathList(path, idents, ast::DUMMY_NODE_ID)));
        }

        let first_ident = self.parse_ident();
        let mut path = vec!(first_ident);
        match self.token {
          token::Eq => {
            // x = foo::bar
            self.bump();
            let path_lo = self.span.lo;
            path = vec!(self.parse_ident());
            while self.check(&token::ModSep) {
                self.bump();
                let id = self.parse_ident();
                path.push(id);
            }
            let span = mk_sp(path_lo, self.span.hi);
            self.obsolete(span, ObsoleteImportRenaming);
            let path = ast::Path {
                span: span,
                global: false,
                segments: path.into_iter().map(|identifier| {
                    ast::PathSegment {
                        identifier: identifier,
                        parameters: ast::PathParameters::none(),
                    }
                }).collect()
            };
            return P(spanned(lo, self.span.hi,
                             ViewPathSimple(first_ident, path,
                                           ast::DUMMY_NODE_ID)));
          }

          token::ModSep => {
            // foo::bar or foo::{a,b,c} or foo::*
            while self.check(&token::ModSep) {
                self.bump();

                match self.token {
                  token::Ident(i, _) => {
                    self.bump();
                    path.push(i);
                  }

                  // foo::bar::{a,b,c}
                  token::OpenDelim(token::Brace) => {
                    let idents = self.parse_unspanned_seq(
                        &token::OpenDelim(token::Brace),
                        &token::CloseDelim(token::Brace),
                        seq_sep_trailing_allowed(token::Comma),
                        |p| p.parse_path_list_item()
                    );
                    let path = ast::Path {
                        span: mk_sp(lo, self.span.hi),
                        global: false,
                        segments: path.into_iter().map(|identifier| {
                            ast::PathSegment {
                                identifier: identifier,
                                parameters: ast::PathParameters::none(),
                            }
                        }).collect()
                    };
                    return P(spanned(lo, self.span.hi,
                                     ViewPathList(path, idents, ast::DUMMY_NODE_ID)));
                  }

                  // foo::bar::*
                  token::BinOp(token::Star) => {
                    self.bump();
                    let path = ast::Path {
                        span: mk_sp(lo, self.span.hi),
                        global: false,
                        segments: path.into_iter().map(|identifier| {
                            ast::PathSegment {
                                identifier: identifier,
                                parameters: ast::PathParameters::none(),
                            }
                        }).collect()
                    };
                    return P(spanned(lo, self.span.hi,
                                     ViewPathGlob(path, ast::DUMMY_NODE_ID)));
                  }

                  _ => break
                }
            }
          }
          _ => ()
        }
        let mut rename_to = path[path.len() - 1u];
        let path = ast::Path {
            span: mk_sp(lo, self.span.hi),
            global: false,
            segments: path.into_iter().map(|identifier| {
                ast::PathSegment {
                    identifier: identifier,
                    parameters: ast::PathParameters::none(),
                }
            }).collect()
        };
        if self.eat_keyword(keywords::As) {
            rename_to = self.parse_ident()
        }
        P(spanned(lo, self.last_span.hi,
                  ViewPathSimple(rename_to, path, ast::DUMMY_NODE_ID)))
    }

    /// Parses a sequence of items. Stops when it finds program
    /// text that can't be parsed as an item
    /// - mod_items uses extern_mod_allowed = true
    /// - block_tail_ uses extern_mod_allowed = false
    fn parse_items_and_view_items(&mut self,
                                  first_item_attrs: Vec<Attribute> ,
                                  mut extern_mod_allowed: bool,
                                  macros_allowed: bool)
                                  -> ParsedItemsAndViewItems {
        let mut attrs = first_item_attrs;
        attrs.push_all(self.parse_outer_attributes().as_slice());
        // First, parse view items.
        let mut view_items : Vec<ast::ViewItem> = Vec::new();
        let mut items = Vec::new();

        // I think this code would probably read better as a single
        // loop with a mutable three-state-variable (for extern crates,
        // view items, and regular items) ... except that because
        // of macros, I'd like to delay that entire check until later.
        loop {
            match self.parse_item_or_view_item(attrs, macros_allowed) {
                IoviNone(attrs) => {
                    return ParsedItemsAndViewItems {
                        attrs_remaining: attrs,
                        view_items: view_items,
                        items: items,
                        foreign_items: Vec::new()
                    }
                }
                IoviViewItem(view_item) => {
                    match view_item.node {
                        ViewItemUse(..) => {
                            // `extern crate` must precede `use`.
                            extern_mod_allowed = false;
                        }
                        ViewItemExternCrate(..) if !extern_mod_allowed => {
                            self.span_err(view_item.span,
                                          "\"extern crate\" declarations are \
                                           not allowed here");
                        }
                        ViewItemExternCrate(..) => {}
                    }
                    view_items.push(view_item);
                }
                IoviItem(item) => {
                    items.push(item);
                    attrs = self.parse_outer_attributes();
                    break;
                }
                IoviForeignItem(_) => {
                    panic!();
                }
            }
            attrs = self.parse_outer_attributes();
        }

        // Next, parse items.
        loop {
            match self.parse_item_or_view_item(attrs, macros_allowed) {
                IoviNone(returned_attrs) => {
                    attrs = returned_attrs;
                    break
                }
                IoviViewItem(view_item) => {
                    attrs = self.parse_outer_attributes();
                    self.span_err(view_item.span,
                                  "`use` and `extern crate` declarations must precede items");
                }
                IoviItem(item) => {
                    attrs = self.parse_outer_attributes();
                    items.push(item)
                }
                IoviForeignItem(_) => {
                    panic!();
                }
            }
        }

        ParsedItemsAndViewItems {
            attrs_remaining: attrs,
            view_items: view_items,
            items: items,
            foreign_items: Vec::new()
        }
    }

    /// Parses a sequence of foreign items. Stops when it finds program
    /// text that can't be parsed as an item
    fn parse_foreign_items(&mut self, first_item_attrs: Vec<Attribute> ,
                           macros_allowed: bool)
        -> ParsedItemsAndViewItems {
        let mut attrs = first_item_attrs;
        attrs.push_all(self.parse_outer_attributes().as_slice());
        let mut foreign_items = Vec::new();
        loop {
            match self.parse_foreign_item(attrs, macros_allowed) {
                IoviNone(returned_attrs) => {
                    if self.check(&token::CloseDelim(token::Brace)) {
                        attrs = returned_attrs;
                        break
                    }
                    self.unexpected();
                },
                IoviViewItem(view_item) => {
                    // I think this can't occur:
                    self.span_err(view_item.span,
                                  "`use` and `extern crate` declarations must precede items");
                }
                IoviItem(item) => {
                    // FIXME #5668: this will occur for a macro invocation:
                    self.span_fatal(item.span, "macros cannot expand to foreign items");
                }
                IoviForeignItem(foreign_item) => {
                    foreign_items.push(foreign_item);
                }
            }
            attrs = self.parse_outer_attributes();
        }

        ParsedItemsAndViewItems {
            attrs_remaining: attrs,
            view_items: Vec::new(),
            items: Vec::new(),
            foreign_items: foreign_items
        }
    }

    /// Parses a source module as a crate. This is the main
    /// entry point for the parser.
    pub fn parse_crate_mod(&mut self) -> Crate {
        let lo = self.span.lo;
        // parse the crate's inner attrs, maybe (oops) one
        // of the attrs of an item:
        let (inner, next) = self.parse_inner_attrs_and_next();
        let first_item_outer_attrs = next;
        // parse the items inside the crate:
        let m = self.parse_mod_items(token::Eof, first_item_outer_attrs, lo);

        ast::Crate {
            module: m,
            attrs: inner,
            config: self.cfg.clone(),
            span: mk_sp(lo, self.span.lo),
            exported_macros: Vec::new(),
        }
    }

    pub fn parse_optional_str(&mut self)
                              -> Option<(InternedString, ast::StrStyle, Option<ast::Name>)> {
        let ret = match self.token {
            token::Literal(token::Str_(s), suf) => {
                (self.id_to_interned_str(s.ident()), ast::CookedStr, suf)
            }
            token::Literal(token::StrRaw(s, n), suf) => {
                (self.id_to_interned_str(s.ident()), ast::RawStr(n), suf)
            }
            _ => return None
        };
        self.bump();
        Some(ret)
    }

    pub fn parse_str(&mut self) -> (InternedString, StrStyle) {
        match self.parse_optional_str() {
            Some((s, style, suf)) => {
                let sp = self.last_span;
                self.expect_no_suffix(sp, "str literal", suf);
                (s, style)
            }
            _ =>  self.fatal("expected string literal")
        }
    }
}
