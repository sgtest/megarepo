// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use abi::{self, Abi};
use ast::BareFnTy;
use ast::{RegionTyParamBound, TraitTyParamBound, TraitBoundModifier};
use ast::Unsafety;
use ast::{Mod, Arg, Arm, Attribute, BindingMode, TraitItemKind};
use ast::Block;
use ast::{BlockCheckMode, CaptureBy};
use ast::{Constness, Crate, CrateConfig};
use ast::Defaultness;
use ast::EnumDef;
use ast::{Expr, ExprKind, RangeLimits};
use ast::{Field, FnDecl};
use ast::{ForeignItem, ForeignItemKind, FunctionRetTy};
use ast::{Ident, ImplItem, Item, ItemKind};
use ast::{Lit, LitKind, UintTy};
use ast::Local;
use ast::MacStmtStyle;
use ast::Mac_;
use ast::{MutTy, Mutability};
use ast::{Pat, PatKind};
use ast::{PolyTraitRef, QSelf};
use ast::{Stmt, StmtKind};
use ast::{VariantData, StructField};
use ast::StrStyle;
use ast::SelfKind;
use ast::{TraitItem, TraitRef};
use ast::{Ty, TyKind, TypeBinding, TyParam, TyParamBounds};
use ast::{ViewPath, ViewPathGlob, ViewPathList, ViewPathSimple};
use ast::{Visibility, WhereClause};
use ast::{BinOpKind, UnOp};
use ast;
use codemap::{self, CodeMap, Spanned, spanned};
use syntax_pos::{self, Span, BytePos, mk_sp};
use errors::{self, DiagnosticBuilder};
use ext::tt::macro_parser;
use parse;
use parse::classify;
use parse::common::SeqSep;
use parse::lexer::{Reader, TokenAndSpan};
use parse::obsolete::{ParserObsoleteMethods, ObsoleteSyntax};
use parse::token::{self, intern, MatchNt, SubstNt, SpecialVarNt, InternedString};
use parse::token::{keywords, SpecialMacroVar};
use parse::{new_sub_parser_from_file, ParseSess};
use util::parser::{AssocOp, Fixity};
use print::pprust;
use ptr::P;
use parse::PResult;
use tokenstream::{self, Delimited, SequenceRepetition, TokenTree};
use util::ThinVec;

use std::collections::HashSet;
use std::mem;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::slice;

bitflags! {
    flags Restrictions: u8 {
        const RESTRICTION_STMT_EXPR         = 1 << 0,
        const RESTRICTION_NO_STRUCT_LITERAL = 1 << 1,
        const NO_NONINLINE_MOD  = 1 << 2,
    }
}

type ItemInfo = (Ident, ItemKind, Option<Vec<Attribute> >);

/// How to parse a path. There are three different kinds of paths, all of which
/// are parsed somewhat differently.
#[derive(Copy, Clone, PartialEq)]
pub enum PathStyle {
    /// A path with no type parameters, e.g. `foo::bar::Baz`, used in imports or visibilities.
    Mod,
    /// A path with a lifetime and type parameters, with no double colons
    /// before the type parameters; e.g. `foo::bar<'a>::Baz<T>`, used in types.
    /// Paths using this style can be passed into macros expecting `path` nonterminals.
    Type,
    /// A path with a lifetime and type parameters with double colons before
    /// the type parameters; e.g. `foo::bar::<'a>::Baz::<T>`, used in expressions or patterns.
    Expr,
}

/// How to parse a bound, whether to allow bound modifiers such as `?`.
#[derive(Copy, Clone, PartialEq)]
pub enum BoundParsingMode {
    Bare,
    Modified,
}

#[derive(Clone, Copy, PartialEq)]
pub enum SemiColonMode {
    Break,
    Ignore,
}

/// Possibly accept an `token::Interpolated` expression (a pre-parsed expression
/// dropped into the token stream, which happens while parsing the result of
/// macro expansion). Placement of these is not as complex as I feared it would
/// be. The important thing is to make sure that lookahead doesn't balk at
/// `token::Interpolated` tokens.
macro_rules! maybe_whole_expr {
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
                    Some($p.mk_expr(span.lo, span.hi, ExprKind::Path(None, pt), ThinVec::new()))
                }
                token::Interpolated(token::NtBlock(_)) => {
                    // FIXME: The following avoids an issue with lexical borrowck scopes,
                    // but the clone is unfortunate.
                    let b = match $p.token {
                        token::Interpolated(token::NtBlock(ref b)) => (*b).clone(),
                        _ => unreachable!()
                    };
                    let span = $p.span;
                    Some($p.mk_expr(span.lo, span.hi, ExprKind::Block(b), ThinVec::new()))
                }
                _ => None
            };
            match found {
                Some(e) => {
                    $p.bump();
                    return Ok(e);
                }
                None => ()
            }
        }
    )
}

/// As maybe_whole_expr, but for things other than expressions
macro_rules! maybe_whole {
    ($p:expr, $constructor:ident) => (
        {
            let found = match ($p).token {
                token::Interpolated(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            if let Some(token::Interpolated(token::$constructor(x))) = found {
                return Ok(x.clone());
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
                return Ok(x);
            }
        }
    );
    (no_clone_from_p $p:expr, $constructor:ident) => (
        {
            let found = match ($p).token {
                token::Interpolated(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            if let Some(token::Interpolated(token::$constructor(x))) = found {
                return Ok(x.unwrap());
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
                return Ok((*x).clone());
            }
        }
    );
    (Some deref $p:expr, $constructor:ident) => (
        {
            let found = match ($p).token {
                token::Interpolated(token::$constructor(_)) => {
                    Some(($p).bump_and_get())
                }
                _ => None
            };
            if let Some(token::Interpolated(token::$constructor(x))) = found {
                return Ok(Some((*x).clone()));
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
                return Ok((Vec::new(), x));
            }
        }
    )
}

fn maybe_append(mut lhs: Vec<Attribute>, rhs: Option<Vec<Attribute>>)
                -> Vec<Attribute> {
    if let Some(ref attrs) = rhs {
        lhs.extend(attrs.iter().cloned())
    }
    lhs
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
    last_token_interpolated: bool,
    last_token_eof: bool,
    pub buffer: [TokenAndSpan; 4],
    pub buffer_start: isize,
    pub buffer_end: isize,
    pub tokens_consumed: usize,
    pub restrictions: Restrictions,
    pub quote_depth: usize, // not (yet) related to the quasiquoter
    pub reader: Box<Reader+'a>,
    pub interner: Rc<token::IdentInterner>,
    /// The set of seen errors about obsolete syntax. Used to suppress
    /// extra detail when the same error is seen twice
    pub obsolete_set: HashSet<ObsoleteSyntax>,
    /// Used to determine the path to externally loaded source files
    pub filename: Option<String>,
    pub mod_path_stack: Vec<InternedString>,
    /// Stack of open delimiters and their spans. Used for error message.
    pub open_braces: Vec<(token::DelimToken, Span)>,
    /// Flag if this parser "owns" the directory that it is currently parsing
    /// in. This will affect how nested files are looked up.
    pub owns_directory: bool,
    /// Name of the root module this parser originated from. If `None`, then the
    /// name is not known. This does not change while the parser is descending
    /// into modules, and sub-parsers have new values for this name.
    pub root_module_name: Option<String>,
    pub expected_tokens: Vec<TokenType>,
}

#[derive(PartialEq, Eq, Clone)]
pub enum TokenType {
    Token(token::Token),
    Keyword(keywords::Keyword),
    Operator,
}

impl TokenType {
    fn to_string(&self) -> String {
        match *self {
            TokenType::Token(ref t) => format!("`{}`", Parser::token_to_string(t)),
            TokenType::Operator => "an operator".to_string(),
            TokenType::Keyword(kw) => format!("`{}`", kw.name()),
        }
    }
}

fn is_ident_or_underscore(t: &token::Token) -> bool {
    t.is_ident() || *t == token::Underscore
}

/// Information about the path to a module.
pub struct ModulePath {
    pub name: String,
    pub path_exists: bool,
    pub result: Result<ModulePathSuccess, ModulePathError>,
}

pub struct ModulePathSuccess {
    pub path: ::std::path::PathBuf,
    pub owns_directory: bool,
}

pub struct ModulePathError {
    pub err_msg: String,
    pub help_msg: String,
}

pub enum LhsExpr {
    NotYetParsed,
    AttributesParsed(ThinVec<Attribute>),
    AlreadyParsed(P<Expr>),
}

impl From<Option<ThinVec<Attribute>>> for LhsExpr {
    fn from(o: Option<ThinVec<Attribute>>) -> Self {
        if let Some(attrs) = o {
            LhsExpr::AttributesParsed(attrs)
        } else {
            LhsExpr::NotYetParsed
        }
    }
}

impl From<P<Expr>> for LhsExpr {
    fn from(expr: P<Expr>) -> Self {
        LhsExpr::AlreadyParsed(expr)
    }
}

impl<'a> Parser<'a> {
    pub fn new(sess: &'a ParseSess,
               cfg: ast::CrateConfig,
               mut rdr: Box<Reader+'a>)
               -> Parser<'a>
    {
        let tok0 = rdr.real_token();
        let span = tok0.sp;
        let filename = if span != syntax_pos::DUMMY_SP {
            Some(sess.codemap().span_to_filename(span))
        } else { None };
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
            last_token_interpolated: false,
            last_token_eof: false,
            buffer: [
                placeholder.clone(),
                placeholder.clone(),
                placeholder.clone(),
                placeholder.clone(),
            ],
            buffer_start: 0,
            buffer_end: 0,
            tokens_consumed: 0,
            restrictions: Restrictions::empty(),
            quote_depth: 0,
            obsolete_set: HashSet::new(),
            mod_path_stack: Vec::new(),
            filename: filename,
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
    pub fn this_token_to_string(&self) -> String {
        Parser::token_to_string(&self.token)
    }

    pub fn this_token_descr(&self) -> String {
        let s = self.this_token_to_string();
        if self.token.is_strict_keyword() {
            format!("keyword `{}`", s)
        } else if self.token.is_reserved_keyword() {
            format!("reserved keyword `{}`", s)
        } else {
            format!("`{}`", s)
        }
    }

    pub fn unexpected_last<T>(&self, t: &token::Token) -> PResult<'a, T> {
        let token_str = Parser::token_to_string(t);
        let last_span = self.last_span;
        Err(self.span_fatal(last_span, &format!("unexpected token: `{}`", token_str)))
    }

    pub fn unexpected<T>(&mut self) -> PResult<'a, T> {
        match self.expect_one_of(&[], &[]) {
            Err(e) => Err(e),
            Ok(_) => unreachable!(),
        }
    }

    /// Expect and consume the token t. Signal an error if
    /// the next token is not t.
    pub fn expect(&mut self, t: &token::Token) -> PResult<'a,  ()> {
        if self.expected_tokens.is_empty() {
            if self.token == *t {
                self.bump();
                Ok(())
            } else {
                let token_str = Parser::token_to_string(t);
                let this_token_str = self.this_token_to_string();
                Err(self.fatal(&format!("expected `{}`, found `{}`",
                                   token_str,
                                   this_token_str)))
            }
        } else {
            self.expect_one_of(unsafe { slice::from_raw_parts(t, 1) }, &[])
        }
    }

    /// Expect next token to be edible or inedible token.  If edible,
    /// then consume it; if inedible, then return without consuming
    /// anything.  Signal a fatal error if next token is unexpected.
    pub fn expect_one_of(&mut self,
                         edible: &[token::Token],
                         inedible: &[token::Token]) -> PResult<'a,  ()>{
        fn tokens_to_string(tokens: &[TokenType]) -> String {
            let mut i = tokens.iter();
            // This might be a sign we need a connect method on Iterator.
            let b = i.next()
                     .map_or("".to_string(), |t| t.to_string());
            i.enumerate().fold(b, |mut b, (i, ref a)| {
                if tokens.len() > 2 && i == tokens.len() - 2 {
                    b.push_str(", or ");
                } else if tokens.len() == 2 && i == tokens.len() - 2 {
                    b.push_str(" or ");
                } else {
                    b.push_str(", ");
                }
                b.push_str(&a.to_string());
                b
            })
        }
        if edible.contains(&self.token) {
            self.bump();
            Ok(())
        } else if inedible.contains(&self.token) {
            // leave it in the input
            Ok(())
        } else {
            let mut expected = edible.iter()
                .map(|x| TokenType::Token(x.clone()))
                .chain(inedible.iter().map(|x| TokenType::Token(x.clone())))
                .chain(self.expected_tokens.iter().cloned())
                .collect::<Vec<_>>();
            expected.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
            expected.dedup();
            let expect = tokens_to_string(&expected[..]);
            let actual = self.this_token_to_string();
            Err(self.fatal(
                &(if expected.len() > 1 {
                    (format!("expected one of {}, found `{}`",
                             expect,
                             actual))
                } else if expected.is_empty() {
                    (format!("unexpected token: `{}`",
                             actual))
                } else {
                    (format!("expected {}, found `{}`",
                             expect,
                             actual))
                })[..]
            ))
        }
    }

    /// Check for erroneous `ident { }`; if matches, signal error and
    /// recover (without consuming any expected input token).  Returns
    /// true if and only if input was consumed for recovery.
    pub fn check_for_erroneous_unit_struct_expecting(&mut self,
                                                     expected: &[token::Token])
                                                     -> bool {
        if self.token == token::OpenDelim(token::Brace)
            && expected.iter().all(|t| *t != token::OpenDelim(token::Brace))
            && self.look_ahead(1, |t| *t == token::CloseDelim(token::Brace)) {
            // matched; signal non-fatal error and recover.
            let span = self.span;
            self.span_err(span, "unit-like struct construction is written with no trailing `{ }`");
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
    pub fn commit_expr(&mut self, e: &Expr, edible: &[token::Token],
                       inedible: &[token::Token]) -> PResult<'a, ()> {
        debug!("commit_expr {:?}", e);
        if let ExprKind::Path(..) = e.node {
            // might be unit-struct construction; check for recoverableinput error.
            let expected = edible.iter()
                .cloned()
                .chain(inedible.iter().cloned())
                .collect::<Vec<_>>();
            self.check_for_erroneous_unit_struct_expecting(&expected[..]);
        }
        self.expect_one_of(edible, inedible)
    }

    pub fn commit_expr_expecting(&mut self, e: &Expr, edible: token::Token) -> PResult<'a, ()> {
        self.commit_expr(e, &[edible], &[])
    }

    /// Commit to parsing a complete statement `s`, which expects to be
    /// followed by some token from the set edible + inedible.  Check
    /// for recoverable input errors, discarding erroneous characters.
    pub fn commit_stmt(&mut self, edible: &[token::Token],
                       inedible: &[token::Token]) -> PResult<'a, ()> {
        if self.last_token
               .as_ref()
               .map_or(false, |t| t.is_ident() || t.is_path()) {
            let expected = edible.iter()
                .cloned()
                .chain(inedible.iter().cloned())
                .collect::<Vec<_>>();
            self.check_for_erroneous_unit_struct_expecting(&expected);
        }
        self.expect_one_of(edible, inedible)
    }

    pub fn commit_stmt_expecting(&mut self, edible: token::Token) -> PResult<'a, ()> {
        self.commit_stmt(&[edible], &[])
    }

    /// returns the span of expr, if it was not interpolated or the span of the interpolated token
    fn interpolated_or_expr_span(&self,
                                 expr: PResult<'a, P<Expr>>)
                                 -> PResult<'a, (Span, P<Expr>)> {
        expr.map(|e| {
            if self.last_token_interpolated {
                (self.last_span, e)
            } else {
                (e.span, e)
            }
        })
    }

    pub fn parse_ident(&mut self) -> PResult<'a, ast::Ident> {
        self.check_strict_keywords();
        self.check_reserved_keywords();
        match self.token {
            token::Ident(i) => {
                self.bump();
                Ok(i)
            }
            token::Interpolated(token::NtIdent(..)) => {
                self.bug("ident interpolation not converted to real token");
            }
            _ => {
                let mut err = self.fatal(&format!("expected identifier, found `{}`",
                                                  self.this_token_to_string()));
                if self.token == token::Underscore {
                    err.note("`_` is a wildcard pattern, not an identifier");
                }
                Err(err)
            }
        }
    }

    fn parse_ident_into_path(&mut self) -> PResult<'a, ast::Path> {
        let ident = self.parse_ident()?;
        Ok(ast::Path::from_ident(self.last_span, ident))
    }

    /// Check if the next token is `tok`, and return `true` if so.
    ///
    /// This method will automatically add `tok` to `expected_tokens` if `tok` is not
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

    pub fn check_keyword(&mut self, kw: keywords::Keyword) -> bool {
        self.expected_tokens.push(TokenType::Keyword(kw));
        self.token.is_keyword(kw)
    }

    /// If the next token is the given keyword, eat it and return
    /// true. Otherwise, return false.
    pub fn eat_keyword(&mut self, kw: keywords::Keyword) -> bool {
        if self.check_keyword(kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub fn eat_keyword_noexpect(&mut self, kw: keywords::Keyword) -> bool {
        if self.token.is_keyword(kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub fn check_contextual_keyword(&mut self, ident: Ident) -> bool {
        self.expected_tokens.push(TokenType::Token(token::Ident(ident)));
        if let token::Ident(ref cur_ident) = self.token {
            cur_ident.name == ident.name
        } else {
            false
        }
    }

    pub fn eat_contextual_keyword(&mut self, ident: Ident) -> bool {
        if self.check_contextual_keyword(ident) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// If the given word is not a keyword, signal an error.
    /// If the next token is not the given word, signal an error.
    /// Otherwise, eat it.
    pub fn expect_keyword(&mut self, kw: keywords::Keyword) -> PResult<'a, ()> {
        if !self.eat_keyword(kw) {
            self.unexpected()
        } else {
            Ok(())
        }
    }

    /// Signal an error if the given string is a strict keyword
    pub fn check_strict_keywords(&mut self) {
        if self.token.is_strict_keyword() {
            let token_str = self.this_token_to_string();
            let span = self.span;
            self.span_err(span,
                          &format!("expected identifier, found keyword `{}`",
                                  token_str));
        }
    }

    /// Signal an error if the current token is a reserved keyword
    pub fn check_reserved_keywords(&mut self) {
        if self.token.is_reserved_keyword() {
            let token_str = self.this_token_to_string();
            self.fatal(&format!("`{}` is a reserved keyword", token_str)).emit()
        }
    }

    /// Expect and consume an `&`. If `&&` is seen, replace it with a single
    /// `&` and continue. If an `&` is not seen, signal an error.
    fn expect_and(&mut self) -> PResult<'a, ()> {
        self.expected_tokens.push(TokenType::Token(token::BinOp(token::And)));
        match self.token {
            token::BinOp(token::And) => {
                self.bump();
                Ok(())
            }
            token::AndAnd => {
                let span = self.span;
                let lo = span.lo + BytePos(1);
                Ok(self.bump_with(token::BinOp(token::And), lo, span.hi))
            }
            _ => self.unexpected()
        }
    }

    pub fn expect_no_suffix(&self, sp: Span, kind: &str, suffix: Option<ast::Name>) {
        match suffix {
            None => {/* everything ok */}
            Some(suf) => {
                let text = suf.as_str();
                if text.is_empty() {
                    self.span_bug(sp, "found empty literal suffix in Some")
                }
                self.span_err(sp, &format!("{} with a suffix is invalid", kind));
            }
        }
    }

    /// Attempt to consume a `<`. If `<<` is seen, replace it with a single
    /// `<` and continue. If a `<` is not seen, return false.
    ///
    /// This is meant to be used when parsing generics on a path to get the
    /// starting token.
    fn eat_lt(&mut self) -> bool {
        self.expected_tokens.push(TokenType::Token(token::Lt));
        match self.token {
            token::Lt => {
                self.bump();
                true
            }
            token::BinOp(token::Shl) => {
                let span = self.span;
                let lo = span.lo + BytePos(1);
                self.bump_with(token::Lt, lo, span.hi);
                true
            }
            _ => false,
        }
    }

    fn expect_lt(&mut self) -> PResult<'a, ()> {
        if !self.eat_lt() {
            self.unexpected()
        } else {
            Ok(())
        }
    }

    /// Expect and consume a GT. if a >> is seen, replace it
    /// with a single > and continue. If a GT is not seen,
    /// signal an error.
    pub fn expect_gt(&mut self) -> PResult<'a, ()> {
        self.expected_tokens.push(TokenType::Token(token::Gt));
        match self.token {
            token::Gt => {
                self.bump();
                Ok(())
            }
            token::BinOp(token::Shr) => {
                let span = self.span;
                let lo = span.lo + BytePos(1);
                Ok(self.bump_with(token::Gt, lo, span.hi))
            }
            token::BinOpEq(token::Shr) => {
                let span = self.span;
                let lo = span.lo + BytePos(1);
                Ok(self.bump_with(token::Ge, lo, span.hi))
            }
            token::Ge => {
                let span = self.span;
                let lo = span.lo + BytePos(1);
                Ok(self.bump_with(token::Eq, lo, span.hi))
            }
            _ => {
                let gt_str = Parser::token_to_string(&token::Gt);
                let this_token_str = self.this_token_to_string();
                Err(self.fatal(&format!("expected `{}`, found `{}`",
                                   gt_str,
                                   this_token_str)))
            }
        }
    }

    pub fn parse_seq_to_before_gt_or_return<T, F>(&mut self,
                                                  sep: Option<token::Token>,
                                                  mut f: F)
                                                  -> PResult<'a, (P<[T]>, bool)>
        where F: FnMut(&mut Parser<'a>) -> PResult<'a, Option<T>>,
    {
        let mut v = Vec::new();
        // This loop works by alternating back and forth between parsing types
        // and commas.  For example, given a string `A, B,>`, the parser would
        // first parse `A`, then a comma, then `B`, then a comma. After that it
        // would encounter a `>` and stop. This lets the parser handle trailing
        // commas in generic parameters, because it can stop either after
        // parsing a type or after parsing a comma.
        for i in 0.. {
            if self.check(&token::Gt)
                || self.token == token::BinOp(token::Shr)
                || self.token == token::Ge
                || self.token == token::BinOpEq(token::Shr) {
                break;
            }

            if i % 2 == 0 {
                match f(self)? {
                    Some(result) => v.push(result),
                    None => return Ok((P::from_vec(v), true))
                }
            } else {
                if let Some(t) = sep.as_ref() {
                    self.expect(t)?;
                }

            }
        }
        return Ok((P::from_vec(v), false));
    }

    /// Parse a sequence bracketed by '<' and '>', stopping
    /// before the '>'.
    pub fn parse_seq_to_before_gt<T, F>(&mut self,
                                        sep: Option<token::Token>,
                                        mut f: F)
                                        -> PResult<'a, P<[T]>> where
        F: FnMut(&mut Parser<'a>) -> PResult<'a, T>,
    {
        let (result, returned) = self.parse_seq_to_before_gt_or_return(sep,
                                                                       |p| Ok(Some(f(p)?)))?;
        assert!(!returned);
        return Ok(result);
    }

    pub fn parse_seq_to_gt<T, F>(&mut self,
                                 sep: Option<token::Token>,
                                 f: F)
                                 -> PResult<'a, P<[T]>> where
        F: FnMut(&mut Parser<'a>) -> PResult<'a, T>,
    {
        let v = self.parse_seq_to_before_gt(sep, f)?;
        self.expect_gt()?;
        return Ok(v);
    }

    pub fn parse_seq_to_gt_or_return<T, F>(&mut self,
                                           sep: Option<token::Token>,
                                           f: F)
                                           -> PResult<'a, (P<[T]>, bool)> where
        F: FnMut(&mut Parser<'a>) -> PResult<'a, Option<T>>,
    {
        let (v, returned) = self.parse_seq_to_before_gt_or_return(sep, f)?;
        if !returned {
            self.expect_gt()?;
        }
        return Ok((v, returned));
    }

    /// Eat and discard tokens until one of `kets` is encountered. Respects token trees,
    /// passes through any errors encountered. Used for error recovery.
    pub fn eat_to_tokens(&mut self, kets: &[&token::Token]) {
        self.parse_seq_to_before_tokens(kets,
                                        SeqSep::none(),
                                        |p| p.parse_token_tree(),
                                        |mut e| e.cancel());
    }

    /// Parse a sequence, including the closing delimiter. The function
    /// f must consume tokens until reaching the next separator or
    /// closing bracket.
    pub fn parse_seq_to_end<T, F>(&mut self,
                                  ket: &token::Token,
                                  sep: SeqSep,
                                  f: F)
                                  -> PResult<'a, Vec<T>> where
        F: FnMut(&mut Parser<'a>) -> PResult<'a,  T>,
    {
        let val = self.parse_seq_to_before_end(ket, sep, f);
        self.bump();
        Ok(val)
    }

    /// Parse a sequence, not including the closing delimiter. The function
    /// f must consume tokens until reaching the next separator or
    /// closing bracket.
    pub fn parse_seq_to_before_end<T, F>(&mut self,
                                         ket: &token::Token,
                                         sep: SeqSep,
                                         f: F)
                                         -> Vec<T>
        where F: FnMut(&mut Parser<'a>) -> PResult<'a,  T>
    {
        self.parse_seq_to_before_tokens(&[ket], sep, f, |mut e| e.emit())
    }

    // `fe` is an error handler.
    fn parse_seq_to_before_tokens<T, F, Fe>(&mut self,
                                            kets: &[&token::Token],
                                            sep: SeqSep,
                                            mut f: F,
                                            mut fe: Fe)
                                            -> Vec<T>
        where F: FnMut(&mut Parser<'a>) -> PResult<'a,  T>,
              Fe: FnMut(DiagnosticBuilder)
    {
        let mut first: bool = true;
        let mut v = vec!();
        while !kets.contains(&&self.token) {
            match sep.sep {
                Some(ref t) => {
                    if first {
                        first = false;
                    } else {
                        if let Err(e) = self.expect(t) {
                            fe(e);
                            break;
                        }
                    }
                }
                _ => ()
            }
            if sep.trailing_sep_allowed && kets.iter().any(|k| self.check(k)) {
                break;
            }

            match f(self) {
                Ok(t) => v.push(t),
                Err(e) => {
                    fe(e);
                    break;
                }
            }
        }

        v
    }

    /// Parse a sequence, including the closing delimiter. The function
    /// f must consume tokens until reaching the next separator or
    /// closing bracket.
    pub fn parse_unspanned_seq<T, F>(&mut self,
                                     bra: &token::Token,
                                     ket: &token::Token,
                                     sep: SeqSep,
                                     f: F)
                                     -> PResult<'a, Vec<T>> where
        F: FnMut(&mut Parser<'a>) -> PResult<'a,  T>,
    {
        self.expect(bra)?;
        let result = self.parse_seq_to_before_end(ket, sep, f);
        if self.token == *ket {
            self.bump();
        }
        Ok(result)
    }

    // NB: Do not use this function unless you actually plan to place the
    // spanned list in the AST.
    pub fn parse_seq<T, F>(&mut self,
                           bra: &token::Token,
                           ket: &token::Token,
                           sep: SeqSep,
                           f: F)
                           -> PResult<'a, Spanned<Vec<T>>> where
        F: FnMut(&mut Parser<'a>) -> PResult<'a,  T>,
    {
        let lo = self.span.lo;
        self.expect(bra)?;
        let result = self.parse_seq_to_before_end(ket, sep, f);
        let hi = self.span.hi;
        self.bump();
        Ok(spanned(lo, hi, result))
    }

    /// Advance the parser by one token
    pub fn bump(&mut self) {
        if self.last_token_eof {
            // Bumping after EOF is a bad sign, usually an infinite loop.
            self.bug("attempted to bump the parser past EOF (may be stuck in a loop)");
        }

        if self.token == token::Eof {
            self.last_token_eof = true;
        }

        self.last_span = self.span;
        // Stash token for error recovery (sometimes; clone is not necessarily cheap).
        self.last_token = if self.token.is_ident() ||
                          self.token.is_path() ||
                          self.token == token::Comma {
            Some(Box::new(self.token.clone()))
        } else {
            None
        };
        self.last_token_interpolated = self.token.is_interpolated();
        let next = if self.buffer_start == self.buffer_end {
            self.reader.real_token()
        } else {
            // Avoid token copies with `replace`.
            let buffer_start = self.buffer_start as usize;
            let next_index = (buffer_start + 1) & 3;
            self.buffer_start = next_index as isize;

            let placeholder = TokenAndSpan {
                tok: token::Underscore,
                sp: self.span,
            };
            mem::replace(&mut self.buffer[buffer_start], placeholder)
        };
        self.span = next.sp;
        self.token = next.tok;
        self.tokens_consumed += 1;
        self.expected_tokens.clear();
        // check after each token
        self.check_unknown_macro_variable();
    }

    /// Advance the parser by one token and return the bumped token.
    pub fn bump_and_get(&mut self) -> token::Token {
        let old_token = mem::replace(&mut self.token, token::Underscore);
        self.bump();
        old_token
    }

    /// Advance the parser using provided token as a next one. Use this when
    /// consuming a part of a token. For example a single `<` from `<<`.
    pub fn bump_with(&mut self,
                     next: token::Token,
                     lo: BytePos,
                     hi: BytePos) {
        self.last_span = mk_sp(self.span.lo, lo);
        // It would be incorrect to just stash current token, but fortunately
        // for tokens currently using `bump_with`, last_token will be of no
        // use anyway.
        self.last_token = None;
        self.last_token_interpolated = false;
        self.span = mk_sp(lo, hi);
        self.token = next;
        self.expected_tokens.clear();
    }

    pub fn buffer_length(&mut self) -> isize {
        if self.buffer_start <= self.buffer_end {
            return self.buffer_end - self.buffer_start;
        }
        return (4 - self.buffer_start) + self.buffer_end;
    }
    pub fn look_ahead<R, F>(&mut self, distance: usize, f: F) -> R where
        F: FnOnce(&token::Token) -> R,
    {
        let dist = distance as isize;
        while self.buffer_length() < dist {
            self.buffer[self.buffer_end as usize] = self.reader.real_token();
            self.buffer_end = (self.buffer_end + 1) & 3;
        }
        f(&self.buffer[((self.buffer_start + dist - 1) & 3) as usize].tok)
    }
    pub fn fatal(&self, m: &str) -> DiagnosticBuilder<'a> {
        self.sess.span_diagnostic.struct_span_fatal(self.span, m)
    }
    pub fn span_fatal(&self, sp: Span, m: &str) -> DiagnosticBuilder<'a> {
        self.sess.span_diagnostic.struct_span_fatal(sp, m)
    }
    pub fn span_fatal_help(&self, sp: Span, m: &str, help: &str) -> DiagnosticBuilder<'a> {
        let mut err = self.sess.span_diagnostic.struct_span_fatal(sp, m);
        err.help(help);
        err
    }
    pub fn bug(&self, m: &str) -> ! {
        self.sess.span_diagnostic.span_bug(self.span, m)
    }
    pub fn warn(&self, m: &str) {
        self.sess.span_diagnostic.span_warn(self.span, m)
    }
    pub fn span_warn(&self, sp: Span, m: &str) {
        self.sess.span_diagnostic.span_warn(sp, m)
    }
    pub fn span_err(&self, sp: Span, m: &str) {
        self.sess.span_diagnostic.span_err(sp, m)
    }
    pub fn span_bug(&self, sp: Span, m: &str) -> ! {
        self.sess.span_diagnostic.span_bug(sp, m)
    }
    pub fn abort_if_errors(&self) {
        self.sess.span_diagnostic.abort_if_errors();
    }

    pub fn diagnostic(&self) -> &'a errors::Handler {
        &self.sess.span_diagnostic
    }

    pub fn id_to_interned_str(&mut self, id: Ident) -> InternedString {
        id.name.as_str()
    }

    /// Is the current token one of the keywords that signals a bare function
    /// type?
    pub fn token_is_bare_fn_keyword(&mut self) -> bool {
        self.check_keyword(keywords::Fn) ||
            self.check_keyword(keywords::Unsafe) ||
            self.check_keyword(keywords::Extern)
    }

    pub fn get_lifetime(&mut self) -> ast::Ident {
        match self.token {
            token::Lifetime(ref ident) => *ident,
            _ => self.bug("not a lifetime"),
        }
    }

    pub fn parse_for_in_type(&mut self) -> PResult<'a, TyKind> {
        /*
        Parses whatever can come after a `for` keyword in a type.
        The `for` has already been consumed.

        Deprecated:

        - for <'lt> |S| -> T

        Eventually:

        - for <'lt> [unsafe] [extern "ABI"] fn (S) -> T
        - for <'lt> path::foo(a, b)

        */

        // parse <'lt>
        let lo = self.span.lo;

        let lifetime_defs = self.parse_late_bound_lifetime_defs()?;

        // examine next token to decide to do
        if self.token_is_bare_fn_keyword() {
            self.parse_ty_bare_fn(lifetime_defs)
        } else {
            let hi = self.span.hi;
            let trait_ref = self.parse_trait_ref()?;
            let poly_trait_ref = ast::PolyTraitRef { bound_lifetimes: lifetime_defs,
                                                     trait_ref: trait_ref,
                                                     span: mk_sp(lo, hi)};
            let other_bounds = if self.eat(&token::BinOp(token::Plus)) {
                self.parse_ty_param_bounds(BoundParsingMode::Bare)?
            } else {
                P::new()
            };
            let all_bounds =
                Some(TraitTyParamBound(poly_trait_ref, TraitBoundModifier::None)).into_iter()
                .chain(other_bounds.into_vec())
                .collect();
            Ok(ast::TyKind::PolyTraitRef(all_bounds))
        }
    }

    pub fn parse_ty_path(&mut self) -> PResult<'a, TyKind> {
        Ok(TyKind::Path(None, self.parse_path(PathStyle::Type)?))
    }

    /// parse a TyKind::BareFn type:
    pub fn parse_ty_bare_fn(&mut self, lifetime_defs: Vec<ast::LifetimeDef>)
                            -> PResult<'a, TyKind> {
        /*

        [unsafe] [extern "ABI"] fn (S) -> T
         ^~~~^           ^~~~^     ^~^    ^
           |               |        |     |
           |               |        |   Return type
           |               |      Argument types
           |               |
           |              ABI
        Function Style
        */

        let unsafety = self.parse_unsafety()?;
        let abi = if self.eat_keyword(keywords::Extern) {
            self.parse_opt_abi()?.unwrap_or(Abi::C)
        } else {
            Abi::Rust
        };

        self.expect_keyword(keywords::Fn)?;
        let (inputs, variadic) = self.parse_fn_args(false, true)?;
        let ret_ty = self.parse_ret_ty()?;
        let decl = P(FnDecl {
            inputs: inputs,
            output: ret_ty,
            variadic: variadic
        });
        Ok(TyKind::BareFn(P(BareFnTy {
            abi: abi,
            unsafety: unsafety,
            lifetimes: lifetime_defs,
            decl: decl
        })))
    }

    /// Parses an obsolete closure kind (`&:`, `&mut:`, or `:`).
    pub fn parse_obsolete_closure_kind(&mut self) -> PResult<'a, ()> {
         let lo = self.span.lo;
        if
            self.check(&token::BinOp(token::And)) &&
            self.look_ahead(1, |t| t.is_keyword(keywords::Mut)) &&
            self.look_ahead(2, |t| *t == token::Colon)
        {
            self.bump();
            self.bump();
            self.bump();
        } else if
            self.token == token::BinOp(token::And) &&
            self.look_ahead(1, |t| *t == token::Colon)
        {
            self.bump();
            self.bump();
        } else if
            self.eat(&token::Colon)
        {
            /* nothing */
        } else {
            return Ok(());
        }

        let span = mk_sp(lo, self.span.hi);
        self.obsolete(span, ObsoleteSyntax::ClosureKind);
        Ok(())
    }

    pub fn parse_unsafety(&mut self) -> PResult<'a, Unsafety> {
        if self.eat_keyword(keywords::Unsafe) {
            return Ok(Unsafety::Unsafe);
        } else {
            return Ok(Unsafety::Normal);
        }
    }

    /// Parse the items in a trait declaration
    pub fn parse_trait_item(&mut self) -> PResult<'a, TraitItem> {
        maybe_whole!(no_clone_from_p self, NtTraitItem);
        let mut attrs = self.parse_outer_attributes()?;
        let lo = self.span.lo;

        let (name, node) = if self.eat_keyword(keywords::Type) {
            let TyParam {ident, bounds, default, ..} = self.parse_ty_param()?;
            self.expect(&token::Semi)?;
            (ident, TraitItemKind::Type(bounds, default))
        } else if self.is_const_item() {
                self.expect_keyword(keywords::Const)?;
            let ident = self.parse_ident()?;
            self.expect(&token::Colon)?;
            let ty = self.parse_ty_sum()?;
            let default = if self.check(&token::Eq) {
                self.bump();
                let expr = self.parse_expr()?;
                self.commit_expr_expecting(&expr, token::Semi)?;
                Some(expr)
            } else {
                self.expect(&token::Semi)?;
                None
            };
            (ident, TraitItemKind::Const(ty, default))
        } else if !self.token.is_any_keyword()
            && self.look_ahead(1, |t| *t == token::Not)
            && (self.look_ahead(2, |t| *t == token::OpenDelim(token::Paren))
                || self.look_ahead(2, |t| *t == token::OpenDelim(token::Brace))) {
                // trait item macro.
                // code copied from parse_macro_use_or_failure... abstraction!
                let lo = self.span.lo;
                let pth = self.parse_ident_into_path()?;
                self.expect(&token::Not)?;

                // eat a matched-delimiter token tree:
                let delim = self.expect_open_delim()?;
                let tts = self.parse_seq_to_end(&token::CloseDelim(delim),
                                             SeqSep::none(),
                                             |pp| pp.parse_token_tree())?;
                let m_ = Mac_ { path: pth, tts: tts };
                let m: ast::Mac = codemap::Spanned { node: m_,
                                                     span: mk_sp(lo,
                                                                 self.last_span.hi) };
                if delim != token::Brace {
                    self.expect(&token::Semi)?
                }
                (keywords::Invalid.ident(), ast::TraitItemKind::Macro(m))
            } else {
                let (constness, unsafety, abi) = match self.parse_fn_front_matter() {
                    Ok(cua) => cua,
                    Err(e) => {
                        loop {
                            match self.token {
                                token::Eof => break,
                                token::CloseDelim(token::Brace) |
                                token::Semi => {
                                    self.bump();
                                    break;
                                }
                                token::OpenDelim(token::Brace) => {
                                    self.parse_token_tree()?;
                                    break;
                                }
                                _ => self.bump()
                            }
                        }

                        return Err(e);
                    }
                };

                let ident = self.parse_ident()?;
                let mut generics = self.parse_generics()?;

                let d = self.parse_fn_decl_with_self(|p: &mut Parser<'a>|{
                    // This is somewhat dubious; We don't want to allow
                    // argument names to be left off if there is a
                    // definition...
                    p.parse_arg_general(false)
                })?;

                generics.where_clause = self.parse_where_clause()?;
                let sig = ast::MethodSig {
                    unsafety: unsafety,
                    constness: constness,
                    decl: d,
                    generics: generics,
                    abi: abi,
                };

                let body = match self.token {
                    token::Semi => {
                        self.bump();
                        debug!("parse_trait_methods(): parsing required method");
                        None
                    }
                    token::OpenDelim(token::Brace) => {
                        debug!("parse_trait_methods(): parsing provided method");
                        let (inner_attrs, body) =
                            self.parse_inner_attrs_and_block()?;
                        attrs.extend(inner_attrs.iter().cloned());
                        Some(body)
                    }

                    _ => {
                        let token_str = self.this_token_to_string();
                        return Err(self.fatal(&format!("expected `;` or `{{`, found `{}`",
                                                    token_str)[..]))
                    }
                };
                (ident, ast::TraitItemKind::Method(sig, body))
            };
        Ok(TraitItem {
            id: ast::DUMMY_NODE_ID,
            ident: name,
            attrs: attrs,
            node: node,
            span: mk_sp(lo, self.last_span.hi),
        })
    }


    /// Parse the items in a trait declaration
    pub fn parse_trait_items(&mut self) -> PResult<'a,  Vec<TraitItem>> {
        self.parse_unspanned_seq(
            &token::OpenDelim(token::Brace),
            &token::CloseDelim(token::Brace),
            SeqSep::none(),
            |p| -> PResult<'a, TraitItem> {
                p.parse_trait_item()
            })
    }

    /// Parse a possibly mutable type
    pub fn parse_mt(&mut self) -> PResult<'a, MutTy> {
        let mutbl = self.parse_mutability()?;
        let t = self.parse_ty()?;
        Ok(MutTy { ty: t, mutbl: mutbl })
    }

    /// Parse optional return type [ -> TY ] in function decl
    pub fn parse_ret_ty(&mut self) -> PResult<'a, FunctionRetTy> {
        if self.eat(&token::RArrow) {
            if self.eat(&token::Not) {
                Ok(FunctionRetTy::None(self.last_span))
            } else {
                Ok(FunctionRetTy::Ty(self.parse_ty()?))
            }
        } else {
            let pos = self.span.lo;
            Ok(FunctionRetTy::Default(mk_sp(pos, pos)))
        }
    }

    /// Parse a type in a context where `T1+T2` is allowed.
    pub fn parse_ty_sum(&mut self) -> PResult<'a, P<Ty>> {
        let lo = self.span.lo;
        let lhs = self.parse_ty()?;

        if !self.eat(&token::BinOp(token::Plus)) {
            return Ok(lhs);
        }

        let bounds = self.parse_ty_param_bounds(BoundParsingMode::Bare)?;

        // In type grammar, `+` is treated like a binary operator,
        // and hence both L and R side are required.
        if bounds.is_empty() {
            let last_span = self.last_span;
            self.span_err(last_span,
                          "at least one type parameter bound \
                          must be specified");
        }

        let sp = mk_sp(lo, self.last_span.hi);
        let sum = ast::TyKind::ObjectSum(lhs, bounds);
        Ok(P(Ty {id: ast::DUMMY_NODE_ID, node: sum, span: sp}))
    }

    /// Parse a type.
    pub fn parse_ty(&mut self) -> PResult<'a, P<Ty>> {
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
                ts.push(self.parse_ty_sum()?);
                if self.check(&token::Comma) {
                    last_comma = true;
                    self.bump();
                } else {
                    last_comma = false;
                    break;
                }
            }

            self.expect(&token::CloseDelim(token::Paren))?;
            if ts.len() == 1 && !last_comma {
                TyKind::Paren(ts.into_iter().nth(0).unwrap())
            } else {
                TyKind::Tup(ts)
            }
        } else if self.check(&token::BinOp(token::Star)) {
            // STAR POINTER (bare pointer?)
            self.bump();
            TyKind::Ptr(self.parse_ptr()?)
        } else if self.check(&token::OpenDelim(token::Bracket)) {
            // VECTOR
            self.expect(&token::OpenDelim(token::Bracket))?;
            let t = self.parse_ty_sum()?;

            // Parse the `; e` in `[ i32; e ]`
            // where `e` is a const expression
            let t = match self.maybe_parse_fixed_length_of_vec()? {
                None => TyKind::Vec(t),
                Some(suffix) => TyKind::FixedLengthVec(t, suffix)
            };
            self.expect(&token::CloseDelim(token::Bracket))?;
            t
        } else if self.check(&token::BinOp(token::And)) ||
                  self.token == token::AndAnd {
            // BORROWED POINTER
            self.expect_and()?;
            self.parse_borrowed_pointee()?
        } else if self.check_keyword(keywords::For) {
            self.parse_for_in_type()?
        } else if self.token_is_bare_fn_keyword() {
            // BARE FUNCTION
            self.parse_ty_bare_fn(Vec::new())?
        } else if self.eat_keyword_noexpect(keywords::Typeof) {
            // TYPEOF
            // In order to not be ambiguous, the type must be surrounded by parens.
            self.expect(&token::OpenDelim(token::Paren))?;
            let e = self.parse_expr()?;
            self.expect(&token::CloseDelim(token::Paren))?;
            TyKind::Typeof(e)
        } else if self.eat_lt() {

            let (qself, path) =
                 self.parse_qualified_path(PathStyle::Type)?;

            TyKind::Path(Some(qself), path)
        } else if self.token.is_path_start() {
            let path = self.parse_path(PathStyle::Type)?;
            if self.check(&token::Not) {
                // MACRO INVOCATION
                self.bump();
                let delim = self.expect_open_delim()?;
                let tts = self.parse_seq_to_end(&token::CloseDelim(delim),
                                                SeqSep::none(),
                                                |p| p.parse_token_tree())?;
                let hi = self.span.hi;
                TyKind::Mac(spanned(lo, hi, Mac_ { path: path, tts: tts }))
            } else {
                // NAMED TYPE
                TyKind::Path(None, path)
            }
        } else if self.eat(&token::Underscore) {
            // TYPE TO BE INFERRED
            TyKind::Infer
        } else {
            let msg = format!("expected type, found {}", self.this_token_descr());
            return Err(self.fatal(&msg));
        };

        let sp = mk_sp(lo, self.last_span.hi);
        Ok(P(Ty {id: ast::DUMMY_NODE_ID, node: t, span: sp}))
    }

    pub fn parse_borrowed_pointee(&mut self) -> PResult<'a, TyKind> {
        // look for `&'lt` or `&'foo ` and interpret `foo` as the region name:
        let opt_lifetime = self.parse_opt_lifetime()?;

        let mt = self.parse_mt()?;
        return Ok(TyKind::Rptr(opt_lifetime, mt));
    }

    pub fn parse_ptr(&mut self) -> PResult<'a, MutTy> {
        let mutbl = if self.eat_keyword(keywords::Mut) {
            Mutability::Mutable
        } else if self.eat_keyword(keywords::Const) {
            Mutability::Immutable
        } else {
            let span = self.last_span;
            self.span_err(span,
                          "expected mut or const in raw pointer type (use \
                           `*mut T` or `*const T` as appropriate)");
            Mutability::Immutable
        };
        let t = self.parse_ty()?;
        Ok(MutTy { ty: t, mutbl: mutbl })
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
            is_ident_or_underscore(&self.token)
                && self.look_ahead(1, |t| *t == token::Colon)
        } else {
            self.look_ahead(offset, |t| is_ident_or_underscore(t))
                && self.look_ahead(offset + 1, |t| *t == token::Colon)
        }
    }

    /// This version of parse arg doesn't necessarily require
    /// identifier names.
    pub fn parse_arg_general(&mut self, require_name: bool) -> PResult<'a, Arg> {
        maybe_whole!(no_clone self, NtArg);

        let pat = if require_name || self.is_named_argument() {
            debug!("parse_arg_general parse_pat (require_name:{})",
                   require_name);
            let pat = self.parse_pat()?;

            self.expect(&token::Colon)?;
            pat
        } else {
            debug!("parse_arg_general ident_to_pat");
            let sp = self.last_span;
            let spanned = Spanned { span: sp, node: keywords::Invalid.ident() };
            P(Pat {
                id: ast::DUMMY_NODE_ID,
                node: PatKind::Ident(BindingMode::ByValue(Mutability::Immutable),
                                     spanned, None),
                span: sp
            })
        };

        let t = self.parse_ty_sum()?;

        Ok(Arg {
            ty: t,
            pat: pat,
            id: ast::DUMMY_NODE_ID,
        })
    }

    /// Parse a single function argument
    pub fn parse_arg(&mut self) -> PResult<'a, Arg> {
        self.parse_arg_general(true)
    }

    /// Parse an argument in a lambda header e.g. |arg, arg|
    pub fn parse_fn_block_arg(&mut self) -> PResult<'a, Arg> {
        let pat = self.parse_pat()?;
        let t = if self.eat(&token::Colon) {
            self.parse_ty_sum()?
        } else {
            P(Ty {
                id: ast::DUMMY_NODE_ID,
                node: TyKind::Infer,
                span: mk_sp(self.span.lo, self.span.hi),
            })
        };
        Ok(Arg {
            ty: t,
            pat: pat,
            id: ast::DUMMY_NODE_ID
        })
    }

    pub fn maybe_parse_fixed_length_of_vec(&mut self) -> PResult<'a, Option<P<ast::Expr>>> {
        if self.check(&token::Semi) {
            self.bump();
            Ok(Some(self.parse_expr()?))
        } else {
            Ok(None)
        }
    }

    /// Matches token_lit = LIT_INTEGER | ...
    pub fn parse_lit_token(&mut self) -> PResult<'a, LitKind> {
        let out = match self.token {
            token::Interpolated(token::NtExpr(ref v)) => {
                match v.node {
                    ExprKind::Lit(ref lit) => { lit.node.clone() }
                    _ => { return self.unexpected_last(&self.token); }
                }
            }
            token::Literal(lit, suf) => {
                let (suffix_illegal, out) = match lit {
                    token::Byte(i) => (true, LitKind::Byte(parse::byte_lit(&i.as_str()).0)),
                    token::Char(i) => (true, LitKind::Char(parse::char_lit(&i.as_str()).0)),

                    // there are some valid suffixes for integer and
                    // float literals, so all the handling is done
                    // internally.
                    token::Integer(s) => {
                        (false, parse::integer_lit(&s.as_str(),
                                                   suf.as_ref().map(|s| s.as_str()),
                                                   &self.sess.span_diagnostic,
                                                   self.span))
                    }
                    token::Float(s) => {
                        (false, parse::float_lit(&s.as_str(),
                                                 suf.as_ref().map(|s| s.as_str()),
                                                  &self.sess.span_diagnostic,
                                                 self.span))
                    }

                    token::Str_(s) => {
                        (true,
                         LitKind::Str(token::intern_and_get_ident(&parse::str_lit(&s.as_str())),
                                      ast::StrStyle::Cooked))
                    }
                    token::StrRaw(s, n) => {
                        (true,
                         LitKind::Str(
                            token::intern_and_get_ident(&parse::raw_str_lit(&s.as_str())),
                            ast::StrStyle::Raw(n)))
                    }
                    token::ByteStr(i) =>
                        (true, LitKind::ByteStr(parse::byte_str_lit(&i.as_str()))),
                    token::ByteStrRaw(i, _) =>
                        (true,
                         LitKind::ByteStr(Rc::new(i.to_string().into_bytes()))),
                };

                if suffix_illegal {
                    let sp = self.span;
                    self.expect_no_suffix(sp, &format!("{} literal", lit.short_name()), suf)
                }

                out
            }
            _ => { return self.unexpected_last(&self.token); }
        };

        self.bump();
        Ok(out)
    }

    /// Matches lit = true | false | token_lit
    pub fn parse_lit(&mut self) -> PResult<'a, Lit> {
        let lo = self.span.lo;
        let lit = if self.eat_keyword(keywords::True) {
            LitKind::Bool(true)
        } else if self.eat_keyword(keywords::False) {
            LitKind::Bool(false)
        } else {
            let lit = self.parse_lit_token()?;
            lit
        };
        Ok(codemap::Spanned { node: lit, span: mk_sp(lo, self.last_span.hi) })
    }

    /// matches '-' lit | lit
    pub fn parse_pat_literal_maybe_minus(&mut self) -> PResult<'a, P<Expr>> {
        let minus_lo = self.span.lo;
        let minus_present = self.eat(&token::BinOp(token::Minus));
        let lo = self.span.lo;
        let literal = P(self.parse_lit()?);
        let hi = self.last_span.hi;
        let expr = self.mk_expr(lo, hi, ExprKind::Lit(literal), ThinVec::new());

        if minus_present {
            let minus_hi = self.last_span.hi;
            let unary = self.mk_unary(UnOp::Neg, expr);
            Ok(self.mk_expr(minus_lo, minus_hi, unary, ThinVec::new()))
        } else {
            Ok(expr)
        }
    }

    pub fn parse_path_segment_ident(&mut self) -> PResult<'a, ast::Ident> {
        match self.token {
            token::Ident(sid) if self.token.is_path_segment_keyword() => {
                self.bump();
                Ok(sid)
            }
            _ => self.parse_ident(),
         }
     }

    /// Parses qualified path.
    ///
    /// Assumes that the leading `<` has been parsed already.
    ///
    /// Qualifed paths are a part of the universal function call
    /// syntax (UFCS).
    ///
    /// `qualified_path = <type [as trait_ref]>::path`
    ///
    /// See `parse_path` for `mode` meaning.
    ///
    /// # Examples:
    ///
    /// `<T as U>::a`
    /// `<T as U>::F::a::<S>`
    pub fn parse_qualified_path(&mut self, mode: PathStyle)
                                -> PResult<'a, (QSelf, ast::Path)> {
        let span = self.last_span;
        let self_type = self.parse_ty_sum()?;
        let mut path = if self.eat_keyword(keywords::As) {
            self.parse_path(PathStyle::Type)?
        } else {
            ast::Path {
                span: span,
                global: false,
                segments: vec![]
            }
        };

        let qself = QSelf {
            ty: self_type,
            position: path.segments.len()
        };

        self.expect(&token::Gt)?;
        self.expect(&token::ModSep)?;

        let segments = match mode {
            PathStyle::Type => {
                self.parse_path_segments_without_colons()?
            }
            PathStyle::Expr => {
                self.parse_path_segments_with_colons()?
            }
            PathStyle::Mod => {
                self.parse_path_segments_without_types()?
            }
        };
        path.segments.extend(segments);

        path.span.hi = self.last_span.hi;

        Ok((qself, path))
    }

    /// Parses a path and optional type parameter bounds, depending on the
    /// mode. The `mode` parameter determines whether lifetimes, types, and/or
    /// bounds are permitted and whether `::` must precede type parameter
    /// groups.
    pub fn parse_path(&mut self, mode: PathStyle) -> PResult<'a, ast::Path> {
        // Check for a whole path...
        let found = match self.token {
            token::Interpolated(token::NtPath(_)) => Some(self.bump_and_get()),
            _ => None,
        };
        if let Some(token::Interpolated(token::NtPath(path))) = found {
            return Ok(*path);
        }

        let lo = self.span.lo;
        let is_global = self.eat(&token::ModSep);

        // Parse any number of segments and bound sets. A segment is an
        // identifier followed by an optional lifetime and a set of types.
        // A bound set is a set of type parameter bounds.
        let segments = match mode {
            PathStyle::Type => {
                self.parse_path_segments_without_colons()?
            }
            PathStyle::Expr => {
                self.parse_path_segments_with_colons()?
            }
            PathStyle::Mod => {
                self.parse_path_segments_without_types()?
            }
        };

        // Assemble the span.
        let span = mk_sp(lo, self.last_span.hi);

        // Assemble the result.
        Ok(ast::Path {
            span: span,
            global: is_global,
            segments: segments,
        })
    }

    /// Examples:
    /// - `a::b<T,U>::c<V,W>`
    /// - `a::b<T,U>::c(V) -> W`
    /// - `a::b<T,U>::c(V)`
    pub fn parse_path_segments_without_colons(&mut self) -> PResult<'a, Vec<ast::PathSegment>> {
        let mut segments = Vec::new();
        loop {
            // First, parse an identifier.
            let identifier = self.parse_path_segment_ident()?;

            // Parse types, optionally.
            let parameters = if self.eat_lt() {
                let (lifetimes, types, bindings) = self.parse_generic_values_after_lt()?;

                ast::PathParameters::AngleBracketed(ast::AngleBracketedParameterData {
                    lifetimes: lifetimes,
                    types: P::from_vec(types),
                    bindings: P::from_vec(bindings),
                })
            } else if self.eat(&token::OpenDelim(token::Paren)) {
                let lo = self.last_span.lo;

                let inputs = self.parse_seq_to_end(
                    &token::CloseDelim(token::Paren),
                    SeqSep::trailing_allowed(token::Comma),
                    |p| p.parse_ty_sum())?;

                let output_ty = if self.eat(&token::RArrow) {
                    Some(self.parse_ty()?)
                } else {
                    None
                };

                let hi = self.last_span.hi;

                ast::PathParameters::Parenthesized(ast::ParenthesizedParameterData {
                    span: mk_sp(lo, hi),
                    inputs: inputs,
                    output: output_ty,
                })
            } else {
                ast::PathParameters::none()
            };

            // Assemble and push the result.
            segments.push(ast::PathSegment { identifier: identifier,
                                             parameters: parameters });

            // Continue only if we see a `::`
            if !self.eat(&token::ModSep) {
                return Ok(segments);
            }
        }
    }

    /// Examples:
    /// - `a::b::<T,U>::c`
    pub fn parse_path_segments_with_colons(&mut self) -> PResult<'a, Vec<ast::PathSegment>> {
        let mut segments = Vec::new();
        loop {
            // First, parse an identifier.
            let identifier = self.parse_path_segment_ident()?;

            // If we do not see a `::`, stop.
            if !self.eat(&token::ModSep) {
                segments.push(ast::PathSegment {
                    identifier: identifier,
                    parameters: ast::PathParameters::none()
                });
                return Ok(segments);
            }

            // Check for a type segment.
            if self.eat_lt() {
                // Consumed `a::b::<`, go look for types
                let (lifetimes, types, bindings) = self.parse_generic_values_after_lt()?;
                let parameters = ast::AngleBracketedParameterData {
                    lifetimes: lifetimes,
                    types: P::from_vec(types),
                    bindings: P::from_vec(bindings),
                };
                segments.push(ast::PathSegment {
                    identifier: identifier,
                    parameters: ast::PathParameters::AngleBracketed(parameters),
                });

                // Consumed `a::b::<T,U>`, check for `::` before proceeding
                if !self.eat(&token::ModSep) {
                    return Ok(segments);
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
    pub fn parse_path_segments_without_types(&mut self)
                                             -> PResult<'a, Vec<ast::PathSegment>> {
        let mut segments = Vec::new();
        loop {
            // First, parse an identifier.
            let identifier = self.parse_path_segment_ident()?;

            // Assemble and push the result.
            segments.push(ast::PathSegment {
                identifier: identifier,
                parameters: ast::PathParameters::none()
            });

            // If we do not see a `::` or see `::{`/`::*`, stop.
            if !self.check(&token::ModSep) || self.is_import_coupler() {
                return Ok(segments);
            } else {
                self.bump();
            }
        }
    }

    /// parses 0 or 1 lifetime
    pub fn parse_opt_lifetime(&mut self) -> PResult<'a, Option<ast::Lifetime>> {
        match self.token {
            token::Lifetime(..) => {
                Ok(Some(self.parse_lifetime()?))
            }
            _ => {
                Ok(None)
            }
        }
    }

    /// Parses a single lifetime
    /// Matches lifetime = LIFETIME
    pub fn parse_lifetime(&mut self) -> PResult<'a, ast::Lifetime> {
        match self.token {
            token::Lifetime(i) => {
                let span = self.span;
                self.bump();
                return Ok(ast::Lifetime {
                    id: ast::DUMMY_NODE_ID,
                    span: span,
                    name: i.name
                });
            }
            _ => {
                return Err(self.fatal("expected a lifetime name"));
            }
        }
    }

    /// Parses `lifetime_defs = [ lifetime_defs { ',' lifetime_defs } ]` where `lifetime_def  =
    /// lifetime [':' lifetimes]`
    pub fn parse_lifetime_defs(&mut self) -> PResult<'a, Vec<ast::LifetimeDef>> {

        let mut res = Vec::new();
        loop {
            match self.token {
                token::Lifetime(_) => {
                    let lifetime = self.parse_lifetime()?;
                    let bounds =
                        if self.eat(&token::Colon) {
                            self.parse_lifetimes(token::BinOp(token::Plus))?
                        } else {
                            Vec::new()
                        };
                    res.push(ast::LifetimeDef { lifetime: lifetime,
                                                bounds: bounds });
                }

                _ => {
                    return Ok(res);
                }
            }

            match self.token {
                token::Comma => { self.bump();}
                token::Gt => { return Ok(res); }
                token::BinOp(token::Shr) => { return Ok(res); }
                _ => {
                    let this_token_str = self.this_token_to_string();
                    let msg = format!("expected `,` or `>` after lifetime \
                                      name, found `{}`",
                                      this_token_str);
                    return Err(self.fatal(&msg[..]));
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
    pub fn parse_lifetimes(&mut self, sep: token::Token) -> PResult<'a, Vec<ast::Lifetime>> {

        let mut res = Vec::new();
        loop {
            match self.token {
                token::Lifetime(_) => {
                    res.push(self.parse_lifetime()?);
                }
                _ => {
                    return Ok(res);
                }
            }

            if self.token != sep {
                return Ok(res);
            }

            self.bump();
        }
    }

    /// Parse mutability (`mut` or nothing).
    pub fn parse_mutability(&mut self) -> PResult<'a, Mutability> {
        if self.eat_keyword(keywords::Mut) {
            Ok(Mutability::Mutable)
        } else {
            Ok(Mutability::Immutable)
        }
    }

    /// Parse ident COLON expr
    pub fn parse_field(&mut self) -> PResult<'a, Field> {
        let lo = self.span.lo;
        let i = self.parse_ident()?;
        let hi = self.last_span.hi;
        self.expect(&token::Colon)?;
        let e = self.parse_expr()?;
        Ok(ast::Field {
            ident: spanned(lo, hi, i),
            span: mk_sp(lo, e.span.hi),
            expr: e,
        })
    }

    pub fn mk_expr(&mut self, lo: BytePos, hi: BytePos, node: ExprKind, attrs: ThinVec<Attribute>)
                   -> P<Expr> {
        P(Expr {
            id: ast::DUMMY_NODE_ID,
            node: node,
            span: mk_sp(lo, hi),
            attrs: attrs.into(),
        })
    }

    pub fn mk_unary(&mut self, unop: ast::UnOp, expr: P<Expr>) -> ast::ExprKind {
        ExprKind::Unary(unop, expr)
    }

    pub fn mk_binary(&mut self, binop: ast::BinOp, lhs: P<Expr>, rhs: P<Expr>) -> ast::ExprKind {
        ExprKind::Binary(binop, lhs, rhs)
    }

    pub fn mk_call(&mut self, f: P<Expr>, args: Vec<P<Expr>>) -> ast::ExprKind {
        ExprKind::Call(f, args)
    }

    fn mk_method_call(&mut self,
                      ident: ast::SpannedIdent,
                      tps: Vec<P<Ty>>,
                      args: Vec<P<Expr>>)
                      -> ast::ExprKind {
        ExprKind::MethodCall(ident, tps, args)
    }

    pub fn mk_index(&mut self, expr: P<Expr>, idx: P<Expr>) -> ast::ExprKind {
        ExprKind::Index(expr, idx)
    }

    pub fn mk_range(&mut self,
                    start: Option<P<Expr>>,
                    end: Option<P<Expr>>,
                    limits: RangeLimits)
                    -> PResult<'a, ast::ExprKind> {
        if end.is_none() && limits == RangeLimits::Closed {
            Err(self.span_fatal_help(self.span,
                                     "inclusive range with no end",
                                     "inclusive ranges must be bounded at the end \
                                      (`...b` or `a...b`)"))
        } else {
            Ok(ExprKind::Range(start, end, limits))
        }
    }

    pub fn mk_field(&mut self, expr: P<Expr>, ident: ast::SpannedIdent) -> ast::ExprKind {
        ExprKind::Field(expr, ident)
    }

    pub fn mk_tup_field(&mut self, expr: P<Expr>, idx: codemap::Spanned<usize>) -> ast::ExprKind {
        ExprKind::TupField(expr, idx)
    }

    pub fn mk_assign_op(&mut self, binop: ast::BinOp,
                        lhs: P<Expr>, rhs: P<Expr>) -> ast::ExprKind {
        ExprKind::AssignOp(binop, lhs, rhs)
    }

    pub fn mk_mac_expr(&mut self, lo: BytePos, hi: BytePos,
                       m: Mac_, attrs: ThinVec<Attribute>) -> P<Expr> {
        P(Expr {
            id: ast::DUMMY_NODE_ID,
            node: ExprKind::Mac(codemap::Spanned {node: m, span: mk_sp(lo, hi)}),
            span: mk_sp(lo, hi),
            attrs: attrs,
        })
    }

    pub fn mk_lit_u32(&mut self, i: u32, attrs: ThinVec<Attribute>) -> P<Expr> {
        let span = &self.span;
        let lv_lit = P(codemap::Spanned {
            node: LitKind::Int(i as u64, ast::LitIntType::Unsigned(UintTy::U32)),
            span: *span
        });

        P(Expr {
            id: ast::DUMMY_NODE_ID,
            node: ExprKind::Lit(lv_lit),
            span: *span,
            attrs: attrs,
        })
    }

    fn expect_open_delim(&mut self) -> PResult<'a, token::DelimToken> {
        self.expected_tokens.push(TokenType::Token(token::Gt));
        match self.token {
            token::OpenDelim(delim) => {
                self.bump();
                Ok(delim)
            },
            _ => Err(self.fatal("expected open delimiter")),
        }
    }

    /// At the bottom (top?) of the precedence hierarchy,
    /// parse things like parenthesized exprs,
    /// macros, return, etc.
    ///
    /// NB: This does not parse outer attributes,
    ///     and is private because it only works
    ///     correctly if called from parse_dot_or_call_expr().
    fn parse_bottom_expr(&mut self) -> PResult<'a, P<Expr>> {
        maybe_whole_expr!(self);

        // Outer attributes are already parsed and will be
        // added to the return value after the fact.
        //
        // Therefore, prevent sub-parser from parsing
        // attributes by giving them a empty "already parsed" list.
        let mut attrs = ThinVec::new();

        let lo = self.span.lo;
        let mut hi = self.span.hi;

        let ex: ExprKind;

        // Note: when adding new syntax here, don't forget to adjust Token::can_begin_expr().
        match self.token {
            token::OpenDelim(token::Paren) => {
                self.bump();

                attrs.extend(self.parse_inner_attributes()?);

                // (e) is parenthesized e
                // (e,) is a tuple with only one field, e
                let mut es = vec![];
                let mut trailing_comma = false;
                while self.token != token::CloseDelim(token::Paren) {
                    es.push(self.parse_expr()?);
                    self.commit_expr(&es.last().unwrap(), &[],
                                     &[token::Comma, token::CloseDelim(token::Paren)])?;
                    if self.check(&token::Comma) {
                        trailing_comma = true;

                        self.bump();
                    } else {
                        trailing_comma = false;
                        break;
                    }
                }
                self.bump();

                hi = self.last_span.hi;
                return if es.len() == 1 && !trailing_comma {
                    Ok(self.mk_expr(lo, hi, ExprKind::Paren(es.into_iter().nth(0).unwrap()), attrs))
                } else {
                    Ok(self.mk_expr(lo, hi, ExprKind::Tup(es), attrs))
                }
            },
            token::OpenDelim(token::Brace) => {
                return self.parse_block_expr(lo, BlockCheckMode::Default, attrs);
            },
            token::BinOp(token::Or) |  token::OrOr => {
                let lo = self.span.lo;
                return self.parse_lambda_expr(lo, CaptureBy::Ref, attrs);
            },
            token::OpenDelim(token::Bracket) => {
                self.bump();

                attrs.extend(self.parse_inner_attributes()?);

                if self.check(&token::CloseDelim(token::Bracket)) {
                    // Empty vector.
                    self.bump();
                    ex = ExprKind::Vec(Vec::new());
                } else {
                    // Nonempty vector.
                    let first_expr = self.parse_expr()?;
                    if self.check(&token::Semi) {
                        // Repeating array syntax: [ 0; 512 ]
                        self.bump();
                        let count = self.parse_expr()?;
                        self.expect(&token::CloseDelim(token::Bracket))?;
                        ex = ExprKind::Repeat(first_expr, count);
                    } else if self.check(&token::Comma) {
                        // Vector with two or more elements.
                        self.bump();
                        let remaining_exprs = self.parse_seq_to_end(
                            &token::CloseDelim(token::Bracket),
                            SeqSep::trailing_allowed(token::Comma),
                            |p| Ok(p.parse_expr()?)
                        )?;
                        let mut exprs = vec!(first_expr);
                        exprs.extend(remaining_exprs);
                        ex = ExprKind::Vec(exprs);
                    } else {
                        // Vector with one element.
                        self.expect(&token::CloseDelim(token::Bracket))?;
                        ex = ExprKind::Vec(vec!(first_expr));
                    }
                }
                hi = self.last_span.hi;
            }
            _ => {
                if self.eat_lt() {
                    let (qself, path) =
                        self.parse_qualified_path(PathStyle::Expr)?;
                    hi = path.span.hi;
                    return Ok(self.mk_expr(lo, hi, ExprKind::Path(Some(qself), path), attrs));
                }
                if self.eat_keyword(keywords::Move) {
                    let lo = self.last_span.lo;
                    return self.parse_lambda_expr(lo, CaptureBy::Value, attrs);
                }
                if self.eat_keyword(keywords::If) {
                    return self.parse_if_expr(attrs);
                }
                if self.eat_keyword(keywords::For) {
                    let lo = self.last_span.lo;
                    return self.parse_for_expr(None, lo, attrs);
                }
                if self.eat_keyword(keywords::While) {
                    let lo = self.last_span.lo;
                    return self.parse_while_expr(None, lo, attrs);
                }
                if self.token.is_lifetime() {
                    let label = Spanned { node: self.get_lifetime(),
                                          span: self.span };
                    let lo = self.span.lo;
                    self.bump();
                    self.expect(&token::Colon)?;
                    if self.eat_keyword(keywords::While) {
                        return self.parse_while_expr(Some(label), lo, attrs)
                    }
                    if self.eat_keyword(keywords::For) {
                        return self.parse_for_expr(Some(label), lo, attrs)
                    }
                    if self.eat_keyword(keywords::Loop) {
                        return self.parse_loop_expr(Some(label), lo, attrs)
                    }
                    return Err(self.fatal("expected `while`, `for`, or `loop` after a label"))
                }
                if self.eat_keyword(keywords::Loop) {
                    let lo = self.last_span.lo;
                    return self.parse_loop_expr(None, lo, attrs);
                }
                if self.eat_keyword(keywords::Continue) {
                    let ex = if self.token.is_lifetime() {
                        let ex = ExprKind::Continue(Some(Spanned{
                            node: self.get_lifetime(),
                            span: self.span
                        }));
                        self.bump();
                        ex
                    } else {
                        ExprKind::Continue(None)
                    };
                    let hi = self.last_span.hi;
                    return Ok(self.mk_expr(lo, hi, ex, attrs));
                }
                if self.eat_keyword(keywords::Match) {
                    return self.parse_match_expr(attrs);
                }
                if self.eat_keyword(keywords::Unsafe) {
                    return self.parse_block_expr(
                        lo,
                        BlockCheckMode::Unsafe(ast::UserProvided),
                        attrs);
                }
                if self.eat_keyword(keywords::Return) {
                    if self.token.can_begin_expr() {
                        let e = self.parse_expr()?;
                        hi = e.span.hi;
                        ex = ExprKind::Ret(Some(e));
                    } else {
                        ex = ExprKind::Ret(None);
                    }
                } else if self.eat_keyword(keywords::Break) {
                    if self.token.is_lifetime() {
                        ex = ExprKind::Break(Some(Spanned {
                            node: self.get_lifetime(),
                            span: self.span
                        }));
                        self.bump();
                    } else {
                        ex = ExprKind::Break(None);
                    }
                    hi = self.last_span.hi;
                } else if self.token.is_keyword(keywords::Let) {
                    // Catch this syntax error here, instead of in `check_strict_keywords`, so
                    // that we can explicitly mention that let is not to be used as an expression
                    let mut db = self.fatal("expected expression, found statement (`let`)");
                    db.note("variable declaration using `let` is a statement");
                    return Err(db);
                } else if self.token.is_path_start() {
                    let pth = self.parse_path(PathStyle::Expr)?;

                    // `!`, as an operator, is prefix, so we know this isn't that
                    if self.check(&token::Not) {
                        // MACRO INVOCATION expression
                        self.bump();

                        let delim = self.expect_open_delim()?;
                        let tts = self.parse_seq_to_end(
                            &token::CloseDelim(delim),
                            SeqSep::none(),
                            |p| p.parse_token_tree())?;
                        let hi = self.last_span.hi;

                        return Ok(self.mk_mac_expr(lo,
                                                   hi,
                                                   Mac_ { path: pth, tts: tts },
                                                   attrs));
                    }
                    if self.check(&token::OpenDelim(token::Brace)) {
                        // This is a struct literal, unless we're prohibited
                        // from parsing struct literals here.
                        let prohibited = self.restrictions.contains(
                            Restrictions::RESTRICTION_NO_STRUCT_LITERAL
                        );
                        if !prohibited {
                            // It's a struct literal.
                            self.bump();
                            let mut fields = Vec::new();
                            let mut base = None;

                            attrs.extend(self.parse_inner_attributes()?);

                            while self.token != token::CloseDelim(token::Brace) {
                                if self.eat(&token::DotDot) {
                                    match self.parse_expr() {
                                        Ok(e) => {
                                            base = Some(e);
                                        }
                                        Err(mut e) => {
                                            e.emit();
                                            self.recover_stmt();
                                        }
                                    }
                                    break;
                                }

                                match self.parse_field() {
                                    Ok(f) => fields.push(f),
                                    Err(mut e) => {
                                        e.emit();
                                        self.recover_stmt();
                                        break;
                                    }
                                }

                                match self.commit_expr(&fields.last().unwrap().expr,
                                                       &[token::Comma],
                                                       &[token::CloseDelim(token::Brace)]) {
                                    Ok(()) => {}
                                    Err(mut e) => {
                                        e.emit();
                                        self.recover_stmt();
                                        break;
                                    }
                                }
                            }

                            hi = self.span.hi;
                            self.expect(&token::CloseDelim(token::Brace))?;
                            ex = ExprKind::Struct(pth, fields, base);
                            return Ok(self.mk_expr(lo, hi, ex, attrs));
                        }
                    }

                    hi = pth.span.hi;
                    ex = ExprKind::Path(None, pth);
                } else {
                    match self.parse_lit() {
                        Ok(lit) => {
                            hi = lit.span.hi;
                            ex = ExprKind::Lit(P(lit));
                        }
                        Err(mut err) => {
                            err.cancel();
                            let msg = format!("expected expression, found {}",
                                              self.this_token_descr());
                            return Err(self.fatal(&msg));
                        }
                    }
                }
            }
        }

        return Ok(self.mk_expr(lo, hi, ex, attrs));
    }

    fn parse_or_use_outer_attributes(&mut self,
                                     already_parsed_attrs: Option<ThinVec<Attribute>>)
                                     -> PResult<'a, ThinVec<Attribute>> {
        if let Some(attrs) = already_parsed_attrs {
            Ok(attrs)
        } else {
            self.parse_outer_attributes().map(|a| a.into())
        }
    }

    /// Parse a block or unsafe block
    pub fn parse_block_expr(&mut self, lo: BytePos, blk_mode: BlockCheckMode,
                            outer_attrs: ThinVec<Attribute>)
                            -> PResult<'a, P<Expr>> {

        self.expect(&token::OpenDelim(token::Brace))?;

        let mut attrs = outer_attrs;
        attrs.extend(self.parse_inner_attributes()?);

        let blk = self.parse_block_tail(lo, blk_mode)?;
        return Ok(self.mk_expr(blk.span.lo, blk.span.hi, ExprKind::Block(blk), attrs));
    }

    /// parse a.b or a(13) or a[4] or just a
    pub fn parse_dot_or_call_expr(&mut self,
                                  already_parsed_attrs: Option<ThinVec<Attribute>>)
                                  -> PResult<'a, P<Expr>> {
        let attrs = self.parse_or_use_outer_attributes(already_parsed_attrs)?;

        let b = self.parse_bottom_expr();
        let (span, b) = self.interpolated_or_expr_span(b)?;
        self.parse_dot_or_call_expr_with(b, span.lo, attrs)
    }

    pub fn parse_dot_or_call_expr_with(&mut self,
                                       e0: P<Expr>,
                                       lo: BytePos,
                                       mut attrs: ThinVec<Attribute>)
                                       -> PResult<'a, P<Expr>> {
        // Stitch the list of outer attributes onto the return value.
        // A little bit ugly, but the best way given the current code
        // structure
        self.parse_dot_or_call_expr_with_(e0, lo)
        .map(|expr|
            expr.map(|mut expr| {
                attrs.extend::<Vec<_>>(expr.attrs.into());
                expr.attrs = attrs;
                match expr.node {
                    ExprKind::If(..) | ExprKind::IfLet(..) => {
                        if !expr.attrs.is_empty() {
                            // Just point to the first attribute in there...
                            let span = expr.attrs[0].span;

                            self.span_err(span,
                                "attributes are not yet allowed on `if` \
                                expressions");
                        }
                    }
                    _ => {}
                }
                expr
            })
        )
    }

    // Assuming we have just parsed `.foo` (i.e., a dot and an ident), continue
    // parsing into an expression.
    fn parse_dot_suffix(&mut self,
                        ident: Ident,
                        ident_span: Span,
                        self_value: P<Expr>,
                        lo: BytePos)
                        -> PResult<'a, P<Expr>> {
        let (_, tys, bindings) = if self.eat(&token::ModSep) {
            self.expect_lt()?;
            self.parse_generic_values_after_lt()?
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };

        if !bindings.is_empty() {
            let last_span = self.last_span;
            self.span_err(last_span, "type bindings are only permitted on trait paths");
        }

        Ok(match self.token {
            // expr.f() method call.
            token::OpenDelim(token::Paren) => {
                let mut es = self.parse_unspanned_seq(
                    &token::OpenDelim(token::Paren),
                    &token::CloseDelim(token::Paren),
                    SeqSep::trailing_allowed(token::Comma),
                    |p| Ok(p.parse_expr()?)
                )?;
                let hi = self.last_span.hi;

                es.insert(0, self_value);
                let id = spanned(ident_span.lo, ident_span.hi, ident);
                let nd = self.mk_method_call(id, tys, es);
                self.mk_expr(lo, hi, nd, ThinVec::new())
            }
            // Field access.
            _ => {
                if !tys.is_empty() {
                    let last_span = self.last_span;
                    self.span_err(last_span,
                                  "field expressions may not \
                                   have type parameters");
                }

                let id = spanned(ident_span.lo, ident_span.hi, ident);
                let field = self.mk_field(self_value, id);
                self.mk_expr(lo, ident_span.hi, field, ThinVec::new())
            }
        })
    }

    fn parse_dot_or_call_expr_with_(&mut self, e0: P<Expr>, lo: BytePos) -> PResult<'a, P<Expr>> {
        let mut e = e0;
        let mut hi;
        loop {
            // expr?
            while self.eat(&token::Question) {
                let hi = self.last_span.hi;
                e = self.mk_expr(lo, hi, ExprKind::Try(e), ThinVec::new());
            }

            // expr.f
            if self.eat(&token::Dot) {
                match self.token {
                  token::Ident(i) => {
                    let dot_pos = self.last_span.hi;
                    hi = self.span.hi;
                    self.bump();

                    e = self.parse_dot_suffix(i, mk_sp(dot_pos, hi), e, lo)?;
                  }
                  token::Literal(token::Integer(n), suf) => {
                    let sp = self.span;

                    // A tuple index may not have a suffix
                    self.expect_no_suffix(sp, "tuple index", suf);

                    let dot = self.last_span.hi;
                    hi = self.span.hi;
                    self.bump();

                    let index = n.as_str().parse::<usize>().ok();
                    match index {
                        Some(n) => {
                            let id = spanned(dot, hi, n);
                            let field = self.mk_tup_field(e, id);
                            e = self.mk_expr(lo, hi, field, ThinVec::new());
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
                    let mut err = self.diagnostic().struct_span_err(last_span,
                        &format!("unexpected token: `{}`", n.as_str()));
                    if fstr.chars().all(|x| "0123456789.".contains(x)) {
                        let float = match fstr.parse::<f64>().ok() {
                            Some(f) => f,
                            None => continue,
                        };
                        err.help(&format!("try parenthesizing the first index; e.g., `(foo.{}){}`",
                                 float.trunc() as usize,
                                 format!(".{}", fstr.splitn(2, ".").last().unwrap())));
                    }
                    return Err(err);

                  }
                  _ => {
                    // FIXME Could factor this out into non_fatal_unexpected or something.
                    let actual = self.this_token_to_string();
                    self.span_err(self.span, &format!("unexpected token: `{}`", actual));

                    let dot_pos = self.last_span.hi;
                    e = self.parse_dot_suffix(keywords::Invalid.ident(),
                                              mk_sp(dot_pos, dot_pos),
                                              e, lo)?;
                  }
                }
                continue;
            }
            if self.expr_is_complete(&e) { break; }
            match self.token {
              // expr(...)
              token::OpenDelim(token::Paren) => {
                let es = self.parse_unspanned_seq(
                    &token::OpenDelim(token::Paren),
                    &token::CloseDelim(token::Paren),
                    SeqSep::trailing_allowed(token::Comma),
                    |p| Ok(p.parse_expr()?)
                )?;
                hi = self.last_span.hi;

                let nd = self.mk_call(e, es);
                e = self.mk_expr(lo, hi, nd, ThinVec::new());
              }

              // expr[...]
              // Could be either an index expression or a slicing expression.
              token::OpenDelim(token::Bracket) => {
                self.bump();
                let ix = self.parse_expr()?;
                hi = self.span.hi;
                self.commit_expr_expecting(&ix, token::CloseDelim(token::Bracket))?;
                let index = self.mk_index(e, ix);
                e = self.mk_expr(lo, hi, index, ThinVec::new())
              }
              _ => return Ok(e)
            }
        }
        return Ok(e);
    }

    // Parse unquoted tokens after a `$` in a token tree
    fn parse_unquoted(&mut self) -> PResult<'a, TokenTree> {
        let mut sp = self.span;
        let name = match self.token {
            token::Dollar => {
                self.bump();

                if self.token == token::OpenDelim(token::Paren) {
                    let Spanned { node: seq, span: seq_span } = self.parse_seq(
                        &token::OpenDelim(token::Paren),
                        &token::CloseDelim(token::Paren),
                        SeqSep::none(),
                        |p| p.parse_token_tree()
                    )?;
                    let (sep, repeat) = self.parse_sep_and_kleene_op()?;
                    let name_num = macro_parser::count_names(&seq);
                    return Ok(TokenTree::Sequence(mk_sp(sp.lo, seq_span.hi), SequenceRepetition {
                        tts: seq,
                        separator: sep,
                        op: repeat,
                        num_captures: name_num
                    }));
                } else if self.token.is_keyword(keywords::Crate) {
                    self.bump();
                    return Ok(TokenTree::Token(sp, SpecialVarNt(SpecialMacroVar::CrateMacroVar)));
                } else {
                    sp = mk_sp(sp.lo, self.span.hi);
                    self.parse_ident().unwrap_or_else(|mut e| {
                        e.emit();
                        keywords::Invalid.ident()
                    })
                }
            }
            token::SubstNt(name) => {
                self.bump();
                name
            }
            _ => unreachable!()
        };
        // continue by trying to parse the `:ident` after `$name`
        if self.token == token::Colon &&
                self.look_ahead(1, |t| t.is_ident() && !t.is_any_keyword()) {
            self.bump();
            sp = mk_sp(sp.lo, self.span.hi);
            let nt_kind = self.parse_ident()?;
            Ok(TokenTree::Token(sp, MatchNt(name, nt_kind)))
        } else {
            Ok(TokenTree::Token(sp, SubstNt(name)))
        }
    }

    pub fn check_unknown_macro_variable(&mut self) {
        if self.quote_depth == 0 {
            match self.token {
                token::SubstNt(name) =>
                    self.fatal(&format!("unknown macro variable `{}`", name)).emit(),
                _ => {}
            }
        }
    }

    /// Parse an optional separator followed by a Kleene-style
    /// repetition token (+ or *).
    pub fn parse_sep_and_kleene_op(&mut self)
                                   -> PResult<'a, (Option<token::Token>, tokenstream::KleeneOp)> {
        fn parse_kleene_op<'a>(parser: &mut Parser<'a>) ->
          PResult<'a,  Option<tokenstream::KleeneOp>> {
            match parser.token {
                token::BinOp(token::Star) => {
                    parser.bump();
                    Ok(Some(tokenstream::KleeneOp::ZeroOrMore))
                },
                token::BinOp(token::Plus) => {
                    parser.bump();
                    Ok(Some(tokenstream::KleeneOp::OneOrMore))
                },
                _ => Ok(None)
            }
        };

        match parse_kleene_op(self)? {
            Some(kleene_op) => return Ok((None, kleene_op)),
            None => {}
        }

        let separator = self.bump_and_get();
        match parse_kleene_op(self)? {
            Some(zerok) => Ok((Some(separator), zerok)),
            None => return Err(self.fatal("expected `*` or `+`"))
        }
    }

    /// parse a single token tree from the input.
    pub fn parse_token_tree(&mut self) -> PResult<'a, TokenTree> {
        // FIXME #6994: currently, this is too eager. It
        // parses token trees but also identifies TokenType::Sequence's
        // and token::SubstNt's; it's too early to know yet
        // whether something will be a nonterminal or a seq
        // yet.
        maybe_whole!(deref self, NtTT);

        match self.token {
            token::Eof => {
                let mut err: DiagnosticBuilder<'a> =
                    self.diagnostic().struct_span_err(self.span,
                                                      "this file contains an un-closed delimiter");
                for &(_, sp) in &self.open_braces {
                    err.span_help(sp, "did you mean to close this delimiter?");
                }

                Err(err)
            },
            token::OpenDelim(delim) => {
                // The span for beginning of the delimited section
                let pre_span = self.span;

                // Parse the open delimiter.
                self.open_braces.push((delim, self.span));
                let open_span = self.span;
                self.bump();

                // Parse the token trees within the delimiters.
                // We stop at any delimiter so we can try to recover if the user
                // uses an incorrect delimiter.
                let tts = self.parse_seq_to_before_tokens(&[&token::CloseDelim(token::Brace),
                                                            &token::CloseDelim(token::Paren),
                                                            &token::CloseDelim(token::Bracket)],
                                                          SeqSep::none(),
                                                          |p| p.parse_token_tree(),
                                                          |mut e| e.emit());

                let close_span = self.span;
                // Expand to cover the entire delimited token tree
                let span = Span { hi: close_span.hi, ..pre_span };

                match self.token {
                    // Correct delimiter.
                    token::CloseDelim(d) if d == delim => {
                        self.open_braces.pop().unwrap();

                        // Parse the close delimiter.
                        self.bump();
                    }
                    // Incorrect delimiter.
                    token::CloseDelim(other) => {
                        let token_str = self.this_token_to_string();
                        let mut err = self.diagnostic().struct_span_err(self.span,
                            &format!("incorrect close delimiter: `{}`", token_str));
                        // This is a conservative error: only report the last unclosed delimiter.
                        // The previous unclosed delimiters could actually be closed! The parser
                        // just hasn't gotten to them yet.
                        if let Some(&(_, sp)) = self.open_braces.last() {
                            err.span_note(sp, "unclosed delimiter");
                        };
                        err.emit();

                        self.open_braces.pop().unwrap();

                        // If the incorrect delimiter matches an earlier opening
                        // delimiter, then don't consume it (it can be used to
                        // close the earlier one). Otherwise, consume it.
                        // E.g., we try to recover from:
                        // fn foo() {
                        //     bar(baz(
                        // }  // Incorrect delimiter but matches the earlier `{`
                        if !self.open_braces.iter().any(|&(b, _)| b == other) {
                            self.bump();
                        }
                    }
                    token::Eof => {
                        // Silently recover, the EOF token will be seen again
                        // and an error emitted then. Thus we don't pop from
                        // self.open_braces here.
                    },
                    _ => {}
                }

                Ok(TokenTree::Delimited(span, Delimited {
                    delim: delim,
                    open_span: open_span,
                    tts: tts,
                    close_span: close_span,
                }))
            },
            _ => {
                // invariants: the current token is not a left-delimiter,
                // not an EOF, and not the desired right-delimiter (if
                // it were, parse_seq_to_before_end would have prevented
                // reaching this point).
                maybe_whole!(deref self, NtTT);
                match self.token {
                    token::CloseDelim(_) => {
                        // An unexpected closing delimiter (i.e., there is no
                        // matching opening delimiter).
                        let token_str = self.this_token_to_string();
                        let err = self.diagnostic().struct_span_err(self.span,
                            &format!("unexpected close delimiter: `{}`", token_str));
                        Err(err)
                    },
                    /* we ought to allow different depths of unquotation */
                    token::Dollar | token::SubstNt(..) if self.quote_depth > 0 => {
                        self.parse_unquoted()
                    }
                    _ => {
                        Ok(TokenTree::Token(self.span, self.bump_and_get()))
                    }
                }
            }
        }
    }

    // parse a stream of tokens into a list of TokenTree's,
    // up to EOF.
    pub fn parse_all_token_trees(&mut self) -> PResult<'a, Vec<TokenTree>> {
        let mut tts = Vec::new();
        while self.token != token::Eof {
            tts.push(self.parse_token_tree()?);
        }
        Ok(tts)
    }

    /// Parse a prefix-unary-operator expr
    pub fn parse_prefix_expr(&mut self,
                             already_parsed_attrs: Option<ThinVec<Attribute>>)
                             -> PResult<'a, P<Expr>> {
        let attrs = self.parse_or_use_outer_attributes(already_parsed_attrs)?;
        let lo = self.span.lo;
        let hi;
        // Note: when adding new unary operators, don't forget to adjust Token::can_begin_expr()
        let ex = match self.token {
            token::Not => {
                self.bump();
                let e = self.parse_prefix_expr(None);
                let (span, e) = self.interpolated_or_expr_span(e)?;
                hi = span.hi;
                self.mk_unary(UnOp::Not, e)
            }
            token::BinOp(token::Minus) => {
                self.bump();
                let e = self.parse_prefix_expr(None);
                let (span, e) = self.interpolated_or_expr_span(e)?;
                hi = span.hi;
                self.mk_unary(UnOp::Neg, e)
            }
            token::BinOp(token::Star) => {
                self.bump();
                let e = self.parse_prefix_expr(None);
                let (span, e) = self.interpolated_or_expr_span(e)?;
                hi = span.hi;
                self.mk_unary(UnOp::Deref, e)
            }
            token::BinOp(token::And) | token::AndAnd => {
                self.expect_and()?;
                let m = self.parse_mutability()?;
                let e = self.parse_prefix_expr(None);
                let (span, e) = self.interpolated_or_expr_span(e)?;
                hi = span.hi;
                ExprKind::AddrOf(m, e)
            }
            token::Ident(..) if self.token.is_keyword(keywords::In) => {
                self.bump();
                let place = self.parse_expr_res(
                    Restrictions::RESTRICTION_NO_STRUCT_LITERAL,
                    None,
                )?;
                let blk = self.parse_block()?;
                let span = blk.span;
                hi = span.hi;
                let blk_expr = self.mk_expr(span.lo, hi, ExprKind::Block(blk), ThinVec::new());
                ExprKind::InPlace(place, blk_expr)
            }
            token::Ident(..) if self.token.is_keyword(keywords::Box) => {
                self.bump();
                let e = self.parse_prefix_expr(None);
                let (span, e) = self.interpolated_or_expr_span(e)?;
                hi = span.hi;
                ExprKind::Box(e)
            }
            _ => return self.parse_dot_or_call_expr(Some(attrs))
        };
        return Ok(self.mk_expr(lo, hi, ex, attrs));
    }

    /// Parse an associative expression
    ///
    /// This parses an expression accounting for associativity and precedence of the operators in
    /// the expression.
    pub fn parse_assoc_expr(&mut self,
                            already_parsed_attrs: Option<ThinVec<Attribute>>)
                            -> PResult<'a, P<Expr>> {
        self.parse_assoc_expr_with(0, already_parsed_attrs.into())
    }

    /// Parse an associative expression with operators of at least `min_prec` precedence
    pub fn parse_assoc_expr_with(&mut self,
                                 min_prec: usize,
                                 lhs: LhsExpr)
                                 -> PResult<'a, P<Expr>> {
        let mut lhs = if let LhsExpr::AlreadyParsed(expr) = lhs {
            expr
        } else {
            let attrs = match lhs {
                LhsExpr::AttributesParsed(attrs) => Some(attrs),
                _ => None,
            };
            if self.token == token::DotDot || self.token == token::DotDotDot {
                return self.parse_prefix_range_expr(attrs);
            } else {
                self.parse_prefix_expr(attrs)?
            }
        };

        if self.expr_is_complete(&lhs) {
            // Semi-statement forms are odd. See https://github.com/rust-lang/rust/issues/29071
            return Ok(lhs);
        }
        self.expected_tokens.push(TokenType::Operator);
        while let Some(op) = AssocOp::from_token(&self.token) {

            let lhs_span = if self.last_token_interpolated {
                self.last_span
            } else {
                lhs.span
            };

            let cur_op_span = self.span;
            let restrictions = if op.is_assign_like() {
                self.restrictions & Restrictions::RESTRICTION_NO_STRUCT_LITERAL
            } else {
                self.restrictions
            };
            if op.precedence() < min_prec {
                break;
            }
            self.bump();
            if op.is_comparison() {
                self.check_no_chained_comparison(&lhs, &op);
            }
            // Special cases:
            if op == AssocOp::As {
                let rhs = self.parse_ty()?;
                let (lo, hi) = (lhs_span.lo, rhs.span.hi);
                lhs = self.mk_expr(lo, hi, ExprKind::Cast(lhs, rhs), ThinVec::new());
                continue
            } else if op == AssocOp::Colon {
                let rhs = self.parse_ty()?;
                let (lo, hi) = (lhs_span.lo, rhs.span.hi);
                lhs = self.mk_expr(lo, hi, ExprKind::Type(lhs, rhs), ThinVec::new());
                continue
            } else if op == AssocOp::DotDot || op == AssocOp::DotDotDot {
                // If we didn’t have to handle `x..`/`x...`, it would be pretty easy to
                // generalise it to the Fixity::None code.
                //
                // We have 2 alternatives here: `x..y`/`x...y` and `x..`/`x...` The other
                // two variants are handled with `parse_prefix_range_expr` call above.
                let rhs = if self.is_at_start_of_range_notation_rhs() {
                    Some(self.parse_assoc_expr_with(op.precedence() + 1,
                                                    LhsExpr::NotYetParsed)?)
                } else {
                    None
                };
                let (lhs_span, rhs_span) = (lhs.span, if let Some(ref x) = rhs {
                    x.span
                } else {
                    cur_op_span
                });
                let limits = if op == AssocOp::DotDot {
                    RangeLimits::HalfOpen
                } else {
                    RangeLimits::Closed
                };

                let r = try!(self.mk_range(Some(lhs), rhs, limits));
                lhs = self.mk_expr(lhs_span.lo, rhs_span.hi, r, ThinVec::new());
                break
            }

            let rhs = match op.fixity() {
                Fixity::Right => self.with_res(
                    restrictions - Restrictions::RESTRICTION_STMT_EXPR,
                    |this| {
                        this.parse_assoc_expr_with(op.precedence(),
                            LhsExpr::NotYetParsed)
                }),
                Fixity::Left => self.with_res(
                    restrictions - Restrictions::RESTRICTION_STMT_EXPR,
                    |this| {
                        this.parse_assoc_expr_with(op.precedence() + 1,
                            LhsExpr::NotYetParsed)
                }),
                // We currently have no non-associative operators that are not handled above by
                // the special cases. The code is here only for future convenience.
                Fixity::None => self.with_res(
                    restrictions - Restrictions::RESTRICTION_STMT_EXPR,
                    |this| {
                        this.parse_assoc_expr_with(op.precedence() + 1,
                            LhsExpr::NotYetParsed)
                }),
            }?;

            let (lo, hi) = (lhs_span.lo, rhs.span.hi);
            lhs = match op {
                AssocOp::Add | AssocOp::Subtract | AssocOp::Multiply | AssocOp::Divide |
                AssocOp::Modulus | AssocOp::LAnd | AssocOp::LOr | AssocOp::BitXor |
                AssocOp::BitAnd | AssocOp::BitOr | AssocOp::ShiftLeft | AssocOp::ShiftRight |
                AssocOp::Equal | AssocOp::Less | AssocOp::LessEqual | AssocOp::NotEqual |
                AssocOp::Greater | AssocOp::GreaterEqual => {
                    let ast_op = op.to_ast_binop().unwrap();
                    let binary = self.mk_binary(codemap::respan(cur_op_span, ast_op), lhs, rhs);
                    self.mk_expr(lo, hi, binary, ThinVec::new())
                }
                AssocOp::Assign =>
                    self.mk_expr(lo, hi, ExprKind::Assign(lhs, rhs), ThinVec::new()),
                AssocOp::Inplace =>
                    self.mk_expr(lo, hi, ExprKind::InPlace(lhs, rhs), ThinVec::new()),
                AssocOp::AssignOp(k) => {
                    let aop = match k {
                        token::Plus =>    BinOpKind::Add,
                        token::Minus =>   BinOpKind::Sub,
                        token::Star =>    BinOpKind::Mul,
                        token::Slash =>   BinOpKind::Div,
                        token::Percent => BinOpKind::Rem,
                        token::Caret =>   BinOpKind::BitXor,
                        token::And =>     BinOpKind::BitAnd,
                        token::Or =>      BinOpKind::BitOr,
                        token::Shl =>     BinOpKind::Shl,
                        token::Shr =>     BinOpKind::Shr,
                    };
                    let aopexpr = self.mk_assign_op(codemap::respan(cur_op_span, aop), lhs, rhs);
                    self.mk_expr(lo, hi, aopexpr, ThinVec::new())
                }
                AssocOp::As | AssocOp::Colon | AssocOp::DotDot | AssocOp::DotDotDot => {
                    self.bug("As, Colon, DotDot or DotDotDot branch reached")
                }
            };

            if op.fixity() == Fixity::None { break }
        }
        Ok(lhs)
    }

    /// Produce an error if comparison operators are chained (RFC #558).
    /// We only need to check lhs, not rhs, because all comparison ops
    /// have same precedence and are left-associative
    fn check_no_chained_comparison(&mut self, lhs: &Expr, outer_op: &AssocOp) {
        debug_assert!(outer_op.is_comparison());
        match lhs.node {
            ExprKind::Binary(op, _, _) if op.node.is_comparison() => {
                // respan to include both operators
                let op_span = mk_sp(op.span.lo, self.span.hi);
                let mut err = self.diagnostic().struct_span_err(op_span,
                    "chained comparison operators require parentheses");
                if op.node == BinOpKind::Lt && *outer_op == AssocOp::Greater {
                    err.help(
                        "use `::<...>` instead of `<...>` if you meant to specify type arguments");
                }
                err.emit();
            }
            _ => {}
        }
    }

    /// Parse prefix-forms of range notation: `..expr`, `..`, `...expr`
    fn parse_prefix_range_expr(&mut self,
                               already_parsed_attrs: Option<ThinVec<Attribute>>)
                               -> PResult<'a, P<Expr>> {
        debug_assert!(self.token == token::DotDot || self.token == token::DotDotDot);
        let tok = self.token.clone();
        let attrs = self.parse_or_use_outer_attributes(already_parsed_attrs)?;
        let lo = self.span.lo;
        let mut hi = self.span.hi;
        self.bump();
        let opt_end = if self.is_at_start_of_range_notation_rhs() {
            // RHS must be parsed with more associativity than the dots.
            let next_prec = AssocOp::from_token(&tok).unwrap().precedence() + 1;
            Some(self.parse_assoc_expr_with(next_prec,
                                            LhsExpr::NotYetParsed)
                .map(|x|{
                    hi = x.span.hi;
                    x
                })?)
         } else {
            None
        };
        let limits = if tok == token::DotDot {
            RangeLimits::HalfOpen
        } else {
            RangeLimits::Closed
        };

        let r = try!(self.mk_range(None,
                                   opt_end,
                                   limits));
        Ok(self.mk_expr(lo, hi, r, attrs))
    }

    fn is_at_start_of_range_notation_rhs(&self) -> bool {
        if self.token.can_begin_expr() {
            // parse `for i in 1.. { }` as infinite loop, not as `for i in (1..{})`.
            if self.token == token::OpenDelim(token::Brace) {
                return !self.restrictions.contains(Restrictions::RESTRICTION_NO_STRUCT_LITERAL);
            }
            true
        } else {
            false
        }
    }

    /// Parse an 'if' or 'if let' expression ('if' token already eaten)
    pub fn parse_if_expr(&mut self, attrs: ThinVec<Attribute>) -> PResult<'a, P<Expr>> {
        if self.check_keyword(keywords::Let) {
            return self.parse_if_let_expr(attrs);
        }
        let lo = self.last_span.lo;
        let cond = self.parse_expr_res(Restrictions::RESTRICTION_NO_STRUCT_LITERAL, None)?;
        let thn = self.parse_block()?;
        let mut els: Option<P<Expr>> = None;
        let mut hi = thn.span.hi;
        if self.eat_keyword(keywords::Else) {
            let elexpr = self.parse_else_expr()?;
            hi = elexpr.span.hi;
            els = Some(elexpr);
        }
        Ok(self.mk_expr(lo, hi, ExprKind::If(cond, thn, els), attrs))
    }

    /// Parse an 'if let' expression ('if' token already eaten)
    pub fn parse_if_let_expr(&mut self, attrs: ThinVec<Attribute>)
                             -> PResult<'a, P<Expr>> {
        let lo = self.last_span.lo;
        self.expect_keyword(keywords::Let)?;
        let pat = self.parse_pat()?;
        self.expect(&token::Eq)?;
        let expr = self.parse_expr_res(Restrictions::RESTRICTION_NO_STRUCT_LITERAL, None)?;
        let thn = self.parse_block()?;
        let (hi, els) = if self.eat_keyword(keywords::Else) {
            let expr = self.parse_else_expr()?;
            (expr.span.hi, Some(expr))
        } else {
            (thn.span.hi, None)
        };
        Ok(self.mk_expr(lo, hi, ExprKind::IfLet(pat, expr, thn, els), attrs))
    }

    // `move |args| expr`
    pub fn parse_lambda_expr(&mut self,
                             lo: BytePos,
                             capture_clause: CaptureBy,
                             attrs: ThinVec<Attribute>)
                             -> PResult<'a, P<Expr>>
    {
        let decl = self.parse_fn_block_decl()?;
        let decl_hi = self.last_span.hi;
        let body = match decl.output {
            FunctionRetTy::Default(_) => {
                // If no explicit return type is given, parse any
                // expr and wrap it up in a dummy block:
                let body_expr = self.parse_expr()?;
                P(ast::Block {
                    id: ast::DUMMY_NODE_ID,
                    span: body_expr.span,
                    stmts: vec![Stmt {
                        span: body_expr.span,
                        node: StmtKind::Expr(body_expr),
                        id: ast::DUMMY_NODE_ID,
                    }],
                    rules: BlockCheckMode::Default,
                })
            }
            _ => {
                // If an explicit return type is given, require a
                // block to appear (RFC 968).
                self.parse_block()?
            }
        };

        Ok(self.mk_expr(
            lo,
            body.span.hi,
            ExprKind::Closure(capture_clause, decl, body, mk_sp(lo, decl_hi)),
            attrs))
    }

    // `else` token already eaten
    pub fn parse_else_expr(&mut self) -> PResult<'a, P<Expr>> {
        if self.eat_keyword(keywords::If) {
            return self.parse_if_expr(ThinVec::new());
        } else {
            let blk = self.parse_block()?;
            return Ok(self.mk_expr(blk.span.lo, blk.span.hi, ExprKind::Block(blk), ThinVec::new()));
        }
    }

    /// Parse a 'for' .. 'in' expression ('for' token already eaten)
    pub fn parse_for_expr(&mut self, opt_ident: Option<ast::SpannedIdent>,
                          span_lo: BytePos,
                          mut attrs: ThinVec<Attribute>) -> PResult<'a, P<Expr>> {
        // Parse: `for <src_pat> in <src_expr> <src_loop_block>`

        let pat = self.parse_pat()?;
        self.expect_keyword(keywords::In)?;
        let expr = self.parse_expr_res(Restrictions::RESTRICTION_NO_STRUCT_LITERAL, None)?;
        let (iattrs, loop_block) = self.parse_inner_attrs_and_block()?;
        attrs.extend(iattrs);

        let hi = self.last_span.hi;

        Ok(self.mk_expr(span_lo, hi,
                        ExprKind::ForLoop(pat, expr, loop_block, opt_ident),
                        attrs))
    }

    /// Parse a 'while' or 'while let' expression ('while' token already eaten)
    pub fn parse_while_expr(&mut self, opt_ident: Option<ast::SpannedIdent>,
                            span_lo: BytePos,
                            mut attrs: ThinVec<Attribute>) -> PResult<'a, P<Expr>> {
        if self.token.is_keyword(keywords::Let) {
            return self.parse_while_let_expr(opt_ident, span_lo, attrs);
        }
        let cond = self.parse_expr_res(Restrictions::RESTRICTION_NO_STRUCT_LITERAL, None)?;
        let (iattrs, body) = self.parse_inner_attrs_and_block()?;
        attrs.extend(iattrs);
        let hi = body.span.hi;
        return Ok(self.mk_expr(span_lo, hi, ExprKind::While(cond, body, opt_ident),
                               attrs));
    }

    /// Parse a 'while let' expression ('while' token already eaten)
    pub fn parse_while_let_expr(&mut self, opt_ident: Option<ast::SpannedIdent>,
                                span_lo: BytePos,
                                mut attrs: ThinVec<Attribute>) -> PResult<'a, P<Expr>> {
        self.expect_keyword(keywords::Let)?;
        let pat = self.parse_pat()?;
        self.expect(&token::Eq)?;
        let expr = self.parse_expr_res(Restrictions::RESTRICTION_NO_STRUCT_LITERAL, None)?;
        let (iattrs, body) = self.parse_inner_attrs_and_block()?;
        attrs.extend(iattrs);
        let hi = body.span.hi;
        return Ok(self.mk_expr(span_lo, hi, ExprKind::WhileLet(pat, expr, body, opt_ident), attrs));
    }

    // parse `loop {...}`, `loop` token already eaten
    pub fn parse_loop_expr(&mut self, opt_ident: Option<ast::SpannedIdent>,
                           span_lo: BytePos,
                           mut attrs: ThinVec<Attribute>) -> PResult<'a, P<Expr>> {
        let (iattrs, body) = self.parse_inner_attrs_and_block()?;
        attrs.extend(iattrs);
        let hi = body.span.hi;
        Ok(self.mk_expr(span_lo, hi, ExprKind::Loop(body, opt_ident), attrs))
    }

    // `match` token already eaten
    fn parse_match_expr(&mut self, mut attrs: ThinVec<Attribute>) -> PResult<'a, P<Expr>> {
        let match_span = self.last_span;
        let lo = self.last_span.lo;
        let discriminant = self.parse_expr_res(Restrictions::RESTRICTION_NO_STRUCT_LITERAL,
                                               None)?;
        if let Err(mut e) = self.commit_expr_expecting(&discriminant,
                                                       token::OpenDelim(token::Brace)) {
            if self.token == token::Token::Semi {
                e.span_note(match_span, "did you mean to remove this `match` keyword?");
            }
            return Err(e)
        }
        attrs.extend(self.parse_inner_attributes()?);

        let mut arms: Vec<Arm> = Vec::new();
        while self.token != token::CloseDelim(token::Brace) {
            match self.parse_arm() {
                Ok(arm) => arms.push(arm),
                Err(mut e) => {
                    // Recover by skipping to the end of the block.
                    e.emit();
                    self.recover_stmt();
                    let hi = self.span.hi;
                    if self.token == token::CloseDelim(token::Brace) {
                        self.bump();
                    }
                    return Ok(self.mk_expr(lo, hi, ExprKind::Match(discriminant, arms), attrs));
                }
            }
        }
        let hi = self.span.hi;
        self.bump();
        return Ok(self.mk_expr(lo, hi, ExprKind::Match(discriminant, arms), attrs));
    }

    pub fn parse_arm(&mut self) -> PResult<'a, Arm> {
        maybe_whole!(no_clone self, NtArm);

        let attrs = self.parse_outer_attributes()?;
        let pats = self.parse_pats()?;
        let mut guard = None;
        if self.eat_keyword(keywords::If) {
            guard = Some(self.parse_expr()?);
        }
        self.expect(&token::FatArrow)?;
        let expr = self.parse_expr_res(Restrictions::RESTRICTION_STMT_EXPR, None)?;

        let require_comma =
            !classify::expr_is_simple_block(&expr)
            && self.token != token::CloseDelim(token::Brace);

        if require_comma {
            self.commit_expr(&expr, &[token::Comma], &[token::CloseDelim(token::Brace)])?;
        } else {
            self.eat(&token::Comma);
        }

        Ok(ast::Arm {
            attrs: attrs,
            pats: pats,
            guard: guard,
            body: expr,
        })
    }

    /// Parse an expression
    pub fn parse_expr(&mut self) -> PResult<'a, P<Expr>> {
        self.parse_expr_res(Restrictions::empty(), None)
    }

    /// Evaluate the closure with restrictions in place.
    ///
    /// After the closure is evaluated, restrictions are reset.
    pub fn with_res<F, T>(&mut self, r: Restrictions, f: F) -> T
        where F: FnOnce(&mut Self) -> T
    {
        let old = self.restrictions;
        self.restrictions = r;
        let r = f(self);
        self.restrictions = old;
        return r;

    }

    /// Parse an expression, subject to the given restrictions
    pub fn parse_expr_res(&mut self, r: Restrictions,
                          already_parsed_attrs: Option<ThinVec<Attribute>>)
                          -> PResult<'a, P<Expr>> {
        self.with_res(r, |this| this.parse_assoc_expr(already_parsed_attrs))
    }

    /// Parse the RHS of a local variable declaration (e.g. '= 14;')
    fn parse_initializer(&mut self) -> PResult<'a, Option<P<Expr>>> {
        if self.check(&token::Eq) {
            self.bump();
            Ok(Some(self.parse_expr()?))
        } else {
            Ok(None)
        }
    }

    /// Parse patterns, separated by '|' s
    fn parse_pats(&mut self) -> PResult<'a, Vec<P<Pat>>> {
        let mut pats = Vec::new();
        loop {
            pats.push(self.parse_pat()?);
            if self.check(&token::BinOp(token::Or)) { self.bump();}
            else { return Ok(pats); }
        };
    }

    fn parse_pat_tuple_elements(&mut self, unary_needs_comma: bool)
                                -> PResult<'a, (Vec<P<Pat>>, Option<usize>)> {
        let mut fields = vec![];
        let mut ddpos = None;

        while !self.check(&token::CloseDelim(token::Paren)) {
            if ddpos.is_none() && self.eat(&token::DotDot) {
                ddpos = Some(fields.len());
                if self.eat(&token::Comma) {
                    // `..` needs to be followed by `)` or `, pat`, `..,)` is disallowed.
                    fields.push(self.parse_pat()?);
                }
            } else if ddpos.is_some() && self.eat(&token::DotDot) {
                // Emit a friendly error, ignore `..` and continue parsing
                self.span_err(self.last_span, "`..` can only be used once per \
                                               tuple or tuple struct pattern");
            } else {
                fields.push(self.parse_pat()?);
            }

            if !self.check(&token::CloseDelim(token::Paren)) ||
                    (unary_needs_comma && fields.len() == 1 && ddpos.is_none()) {
                self.expect(&token::Comma)?;
            }
        }

        Ok((fields, ddpos))
    }

    fn parse_pat_vec_elements(
        &mut self,
    ) -> PResult<'a, (Vec<P<Pat>>, Option<P<Pat>>, Vec<P<Pat>>)> {
        let mut before = Vec::new();
        let mut slice = None;
        let mut after = Vec::new();
        let mut first = true;
        let mut before_slice = true;

        while self.token != token::CloseDelim(token::Bracket) {
            if first {
                first = false;
            } else {
                self.expect(&token::Comma)?;

                if self.token == token::CloseDelim(token::Bracket)
                        && (before_slice || !after.is_empty()) {
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
                            node: PatKind::Wild,
                            span: self.span,
                        }));
                        before_slice = false;
                    }
                    continue
                }
            }

            let subpat = self.parse_pat()?;
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

        Ok((before, slice, after))
    }

    /// Parse the fields of a struct-like pattern
    fn parse_pat_fields(&mut self) -> PResult<'a, (Vec<codemap::Spanned<ast::FieldPat>>, bool)> {
        let mut fields = Vec::new();
        let mut etc = false;
        let mut first = true;
        while self.token != token::CloseDelim(token::Brace) {
            if first {
                first = false;
            } else {
                self.expect(&token::Comma)?;
                // accept trailing commas
                if self.check(&token::CloseDelim(token::Brace)) { break }
            }

            let lo = self.span.lo;
            let hi;

            if self.check(&token::DotDot) {
                self.bump();
                if self.token != token::CloseDelim(token::Brace) {
                    let token_str = self.this_token_to_string();
                    return Err(self.fatal(&format!("expected `{}`, found `{}`", "}",
                                       token_str)))
                }
                etc = true;
                break;
            }

            // Check if a colon exists one ahead. This means we're parsing a fieldname.
            let (subpat, fieldname, is_shorthand) = if self.look_ahead(1, |t| t == &token::Colon) {
                // Parsing a pattern of the form "fieldname: pat"
                let fieldname = self.parse_ident()?;
                self.bump();
                let pat = self.parse_pat()?;
                hi = pat.span.hi;
                (pat, fieldname, false)
            } else {
                // Parsing a pattern of the form "(box) (ref) (mut) fieldname"
                let is_box = self.eat_keyword(keywords::Box);
                let boxed_span_lo = self.span.lo;
                let is_ref = self.eat_keyword(keywords::Ref);
                let is_mut = self.eat_keyword(keywords::Mut);
                let fieldname = self.parse_ident()?;
                hi = self.last_span.hi;

                let bind_type = match (is_ref, is_mut) {
                    (true, true) => BindingMode::ByRef(Mutability::Mutable),
                    (true, false) => BindingMode::ByRef(Mutability::Immutable),
                    (false, true) => BindingMode::ByValue(Mutability::Mutable),
                    (false, false) => BindingMode::ByValue(Mutability::Immutable),
                };
                let fieldpath = codemap::Spanned{span:self.last_span, node:fieldname};
                let fieldpat = P(ast::Pat{
                    id: ast::DUMMY_NODE_ID,
                    node: PatKind::Ident(bind_type, fieldpath, None),
                    span: mk_sp(boxed_span_lo, hi),
                });

                let subpat = if is_box {
                    P(ast::Pat{
                        id: ast::DUMMY_NODE_ID,
                        node: PatKind::Box(fieldpat),
                        span: mk_sp(lo, hi),
                    })
                } else {
                    fieldpat
                };
                (subpat, fieldname, true)
            };

            fields.push(codemap::Spanned { span: mk_sp(lo, hi),
                                              node: ast::FieldPat { ident: fieldname,
                                                                    pat: subpat,
                                                                    is_shorthand: is_shorthand }});
        }
        return Ok((fields, etc));
    }

    fn parse_pat_range_end(&mut self) -> PResult<'a, P<Expr>> {
        if self.token.is_path_start() {
            let lo = self.span.lo;
            let (qself, path) = if self.eat_lt() {
                // Parse a qualified path
                let (qself, path) =
                    self.parse_qualified_path(PathStyle::Expr)?;
                (Some(qself), path)
            } else {
                // Parse an unqualified path
                (None, self.parse_path(PathStyle::Expr)?)
            };
            let hi = self.last_span.hi;
            Ok(self.mk_expr(lo, hi, ExprKind::Path(qself, path), ThinVec::new()))
        } else {
            self.parse_pat_literal_maybe_minus()
        }
    }

    /// Parse a pattern.
    pub fn parse_pat(&mut self) -> PResult<'a, P<Pat>> {
        maybe_whole!(self, NtPat);

        let lo = self.span.lo;
        let pat;
        match self.token {
          token::Underscore => {
            // Parse _
            self.bump();
            pat = PatKind::Wild;
          }
          token::BinOp(token::And) | token::AndAnd => {
            // Parse &pat / &mut pat
            self.expect_and()?;
            let mutbl = self.parse_mutability()?;
            if let token::Lifetime(ident) = self.token {
                return Err(self.fatal(&format!("unexpected lifetime `{}` in pattern", ident)));
            }

            let subpat = self.parse_pat()?;
            pat = PatKind::Ref(subpat, mutbl);
          }
          token::OpenDelim(token::Paren) => {
            // Parse (pat,pat,pat,...) as tuple pattern
            self.bump();
            let (fields, ddpos) = self.parse_pat_tuple_elements(true)?;
            self.expect(&token::CloseDelim(token::Paren))?;
            pat = PatKind::Tuple(fields, ddpos);
          }
          token::OpenDelim(token::Bracket) => {
            // Parse [pat,pat,...] as slice pattern
            self.bump();
            let (before, slice, after) = self.parse_pat_vec_elements()?;
            self.expect(&token::CloseDelim(token::Bracket))?;
            pat = PatKind::Vec(before, slice, after);
          }
          _ => {
            // At this point, token != _, &, &&, (, [
            if self.eat_keyword(keywords::Mut) {
                // Parse mut ident @ pat
                pat = self.parse_pat_ident(BindingMode::ByValue(Mutability::Mutable))?;
            } else if self.eat_keyword(keywords::Ref) {
                // Parse ref ident @ pat / ref mut ident @ pat
                let mutbl = self.parse_mutability()?;
                pat = self.parse_pat_ident(BindingMode::ByRef(mutbl))?;
            } else if self.eat_keyword(keywords::Box) {
                // Parse box pat
                let subpat = self.parse_pat()?;
                pat = PatKind::Box(subpat);
            } else if self.token.is_path_start() {
                // Parse pattern starting with a path
                if self.token.is_ident() && self.look_ahead(1, |t| *t != token::DotDotDot &&
                        *t != token::OpenDelim(token::Brace) &&
                        *t != token::OpenDelim(token::Paren) &&
                        *t != token::ModSep) {
                    // Plain idents have some extra abilities here compared to general paths
                    if self.look_ahead(1, |t| *t == token::Not) {
                        // Parse macro invocation
                        let path = self.parse_ident_into_path()?;
                        self.bump();
                        let delim = self.expect_open_delim()?;
                        let tts = self.parse_seq_to_end(
                            &token::CloseDelim(delim),
                            SeqSep::none(), |p| p.parse_token_tree())?;
                        let mac = Mac_ { path: path, tts: tts };
                        pat = PatKind::Mac(codemap::Spanned {node: mac,
                                                               span: mk_sp(lo, self.last_span.hi)});
                    } else {
                        // Parse ident @ pat
                        // This can give false positives and parse nullary enums,
                        // they are dealt with later in resolve
                        let binding_mode = BindingMode::ByValue(Mutability::Immutable);
                        pat = self.parse_pat_ident(binding_mode)?;
                    }
                } else {
                    let (qself, path) = if self.eat_lt() {
                        // Parse a qualified path
                        let (qself, path) =
                            self.parse_qualified_path(PathStyle::Expr)?;
                        (Some(qself), path)
                    } else {
                        // Parse an unqualified path
                        (None, self.parse_path(PathStyle::Expr)?)
                    };
                    match self.token {
                      token::DotDotDot => {
                        // Parse range
                        let hi = self.last_span.hi;
                        let begin =
                              self.mk_expr(lo, hi, ExprKind::Path(qself, path), ThinVec::new());
                        self.bump();
                        let end = self.parse_pat_range_end()?;
                        pat = PatKind::Range(begin, end);
                      }
                      token::OpenDelim(token::Brace) => {
                         if qself.is_some() {
                            return Err(self.fatal("unexpected `{` after qualified path"));
                        }
                        // Parse struct pattern
                        self.bump();
                        let (fields, etc) = self.parse_pat_fields().unwrap_or_else(|mut e| {
                            e.emit();
                            self.recover_stmt();
                            (vec![], false)
                        });
                        self.bump();
                        pat = PatKind::Struct(path, fields, etc);
                      }
                      token::OpenDelim(token::Paren) => {
                        if qself.is_some() {
                            return Err(self.fatal("unexpected `(` after qualified path"));
                        }
                        // Parse tuple struct or enum pattern
                        self.bump();
                        let (fields, ddpos) = self.parse_pat_tuple_elements(false)?;
                        self.expect(&token::CloseDelim(token::Paren))?;
                        pat = PatKind::TupleStruct(path, fields, ddpos)
                      }
                      _ => {
                        pat = PatKind::Path(qself, path);
                      }
                    }
                }
            } else {
                // Try to parse everything else as literal with optional minus
                match self.parse_pat_literal_maybe_minus() {
                    Ok(begin) => {
                        if self.eat(&token::DotDotDot) {
                            let end = self.parse_pat_range_end()?;
                            pat = PatKind::Range(begin, end);
                        } else {
                            pat = PatKind::Lit(begin);
                        }
                    }
                    Err(mut err) => {
                        err.cancel();
                        let msg = format!("expected pattern, found {}", self.this_token_descr());
                        return Err(self.fatal(&msg));
                    }
                }
            }
          }
        }

        let hi = self.last_span.hi;
        Ok(P(ast::Pat {
            id: ast::DUMMY_NODE_ID,
            node: pat,
            span: mk_sp(lo, hi),
        }))
    }

    /// Parse ident or ident @ pat
    /// used by the copy foo and ref foo patterns to give a good
    /// error message when parsing mistakes like ref foo(a,b)
    fn parse_pat_ident(&mut self,
                       binding_mode: ast::BindingMode)
                       -> PResult<'a, PatKind> {
        let ident = self.parse_ident()?;
        let last_span = self.last_span;
        let name = codemap::Spanned{span: last_span, node: ident};
        let sub = if self.eat(&token::At) {
            Some(self.parse_pat()?)
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
            return Err(self.span_fatal(
                last_span,
                "expected identifier, found enum pattern"))
        }

        Ok(PatKind::Ident(binding_mode, name, sub))
    }

    /// Parse a local variable declaration
    fn parse_local(&mut self, attrs: ThinVec<Attribute>) -> PResult<'a, P<Local>> {
        let lo = self.span.lo;
        let pat = self.parse_pat()?;

        let mut ty = None;
        if self.eat(&token::Colon) {
            ty = Some(self.parse_ty_sum()?);
        }
        let init = self.parse_initializer()?;
        Ok(P(ast::Local {
            ty: ty,
            pat: pat,
            init: init,
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, self.last_span.hi),
            attrs: attrs,
        }))
    }

    /// Parse a structure field
    fn parse_name_and_ty(&mut self, pr: Visibility,
                         attrs: Vec<Attribute> ) -> PResult<'a, StructField> {
        let lo = match pr {
            Visibility::Inherited => self.span.lo,
            _ => self.last_span.lo,
        };
        let name = self.parse_ident()?;
        self.expect(&token::Colon)?;
        let ty = self.parse_ty_sum()?;
        Ok(StructField {
            span: mk_sp(lo, self.last_span.hi),
            ident: Some(name),
            vis: pr,
            id: ast::DUMMY_NODE_ID,
            ty: ty,
            attrs: attrs,
        })
    }

    /// Emit an expected item after attributes error.
    fn expected_item_err(&self, attrs: &[Attribute]) {
        let message = match attrs.last() {
            Some(&Attribute { node: ast::Attribute_ { is_sugared_doc: true, .. }, .. }) => {
                "expected item after doc comment"
            }
            _ => "expected item after attributes",
        };

        self.span_err(self.last_span, message);
    }

    /// Parse a statement. may include decl.
    pub fn parse_stmt(&mut self) -> PResult<'a, Option<Stmt>> {
        Ok(self.parse_stmt_())
    }

    // Eat tokens until we can be relatively sure we reached the end of the
    // statement. This is something of a best-effort heuristic.
    //
    // We terminate when we find an unmatched `}` (without consuming it).
    fn recover_stmt(&mut self) {
        self.recover_stmt_(SemiColonMode::Ignore)
    }
    // If `break_on_semi` is `Break`, then we will stop consuming tokens after
    // finding (and consuming) a `;` outside of `{}` or `[]` (note that this is
    // approximate - it can mean we break too early due to macros, but that
    // shoud only lead to sub-optimal recovery, not inaccurate parsing).
    fn recover_stmt_(&mut self, break_on_semi: SemiColonMode) {
        let mut brace_depth = 0;
        let mut bracket_depth = 0;
        debug!("recover_stmt_ enter loop");
        loop {
            debug!("recover_stmt_ loop {:?}", self.token);
            match self.token {
                token::OpenDelim(token::DelimToken::Brace) => {
                    brace_depth += 1;
                    self.bump();
                }
                token::OpenDelim(token::DelimToken::Bracket) => {
                    bracket_depth += 1;
                    self.bump();
                }
                token::CloseDelim(token::DelimToken::Brace) => {
                    if brace_depth == 0 {
                        debug!("recover_stmt_ return - close delim {:?}", self.token);
                        return;
                    }
                    brace_depth -= 1;
                    self.bump();
                }
                token::CloseDelim(token::DelimToken::Bracket) => {
                    bracket_depth -= 1;
                    if bracket_depth < 0 {
                        bracket_depth = 0;
                    }
                    self.bump();
                }
                token::Eof => {
                    debug!("recover_stmt_ return - Eof");
                    return;
                }
                token::Semi => {
                    self.bump();
                    if break_on_semi == SemiColonMode::Break &&
                       brace_depth == 0 &&
                       bracket_depth == 0 {
                        debug!("recover_stmt_ return - Semi");
                        return;
                    }
                }
                _ => {
                    self.bump()
                }
            }
        }
    }

    fn parse_stmt_(&mut self) -> Option<Stmt> {
        self.parse_stmt_without_recovery().unwrap_or_else(|mut e| {
            e.emit();
            self.recover_stmt_(SemiColonMode::Break);
            None
        })
    }

    fn parse_stmt_without_recovery(&mut self) -> PResult<'a, Option<Stmt>> {
        maybe_whole!(Some deref self, NtStmt);

        let attrs = self.parse_outer_attributes()?;
        let lo = self.span.lo;

        Ok(Some(if self.eat_keyword(keywords::Let) {
            Stmt {
                id: ast::DUMMY_NODE_ID,
                node: StmtKind::Local(self.parse_local(attrs.into())?),
                span: mk_sp(lo, self.last_span.hi),
            }
        } else if self.token.is_ident()
            && !self.token.is_any_keyword()
            && self.look_ahead(1, |t| *t == token::Not) {
            // it's a macro invocation:

            // Potential trouble: if we allow macros with paths instead of
            // idents, we'd need to look ahead past the whole path here...
            let pth = self.parse_ident_into_path()?;
            self.bump();

            let id = match self.token {
                token::OpenDelim(_) => keywords::Invalid.ident(), // no special identifier
                _ => self.parse_ident()?,
            };

            // check that we're pointing at delimiters (need to check
            // again after the `if`, because of `parse_ident`
            // consuming more tokens).
            let delim = match self.token {
                token::OpenDelim(delim) => delim,
                _ => {
                    // we only expect an ident if we didn't parse one
                    // above.
                    let ident_str = if id.name == keywords::Invalid.name() {
                        "identifier, "
                    } else {
                        ""
                    };
                    let tok_str = self.this_token_to_string();
                    return Err(self.fatal(&format!("expected {}`(` or `{{`, found `{}`",
                                       ident_str,
                                       tok_str)))
                },
            };

            let tts = self.parse_unspanned_seq(
                &token::OpenDelim(delim),
                &token::CloseDelim(delim),
                SeqSep::none(),
                |p| p.parse_token_tree()
            )?;
            let hi = self.last_span.hi;

            let style = if delim == token::Brace {
                MacStmtStyle::Braces
            } else {
                MacStmtStyle::NoBraces
            };

            if id.name == keywords::Invalid.name() {
                let mac = spanned(lo, hi, Mac_ { path: pth, tts: tts });
                Stmt {
                    id: ast::DUMMY_NODE_ID,
                    node: StmtKind::Mac(P((mac, style, attrs.into()))),
                    span: mk_sp(lo, hi),
                }
            } else {
                // if it has a special ident, it's definitely an item
                //
                // Require a semicolon or braces.
                if style != MacStmtStyle::Braces {
                    if !self.eat(&token::Semi) {
                        let last_span = self.last_span;
                        self.span_err(last_span,
                                      "macros that expand to items must \
                                       either be surrounded with braces or \
                                       followed by a semicolon");
                    }
                }
                Stmt {
                    id: ast::DUMMY_NODE_ID,
                    span: mk_sp(lo, hi),
                    node: StmtKind::Item({
                        self.mk_item(
                            lo, hi, id /*id is good here*/,
                            ItemKind::Mac(spanned(lo, hi, Mac_ { path: pth, tts: tts })),
                            Visibility::Inherited,
                            attrs)
                    }),
                }
            }
        } else {
            // FIXME: Bad copy of attrs
            let restrictions = self.restrictions | Restrictions::NO_NONINLINE_MOD;
            match self.with_res(restrictions,
                                |this| this.parse_item_(attrs.clone(), false, true))? {
                Some(i) => Stmt {
                    id: ast::DUMMY_NODE_ID,
                    span: mk_sp(lo, i.span.hi),
                    node: StmtKind::Item(i),
                },
                None => {
                    let unused_attrs = |attrs: &[_], s: &mut Self| {
                        if attrs.len() > 0 {
                            s.span_err(s.span,
                                "expected statement after outer attribute");
                        }
                    };

                    // Do not attempt to parse an expression if we're done here.
                    if self.token == token::Semi {
                        unused_attrs(&attrs, self);
                        self.bump();
                        return Ok(None);
                    }

                    if self.token == token::CloseDelim(token::Brace) {
                        unused_attrs(&attrs, self);
                        return Ok(None);
                    }

                    // Remainder are line-expr stmts.
                    let e = self.parse_expr_res(
                        Restrictions::RESTRICTION_STMT_EXPR, Some(attrs.into()))?;
                    Stmt {
                        id: ast::DUMMY_NODE_ID,
                        span: mk_sp(lo, e.span.hi),
                        node: StmtKind::Expr(e),
                    }
                }
            }
        }))
    }

    /// Is this expression a successfully-parsed statement?
    fn expr_is_complete(&mut self, e: &Expr) -> bool {
        self.restrictions.contains(Restrictions::RESTRICTION_STMT_EXPR) &&
            !classify::expr_requires_semi_to_be_stmt(e)
    }

    /// Parse a block. No inner attrs are allowed.
    pub fn parse_block(&mut self) -> PResult<'a, P<Block>> {
        maybe_whole!(no_clone self, NtBlock);

        let lo = self.span.lo;

        if !self.eat(&token::OpenDelim(token::Brace)) {
            let sp = self.span;
            let tok = self.this_token_to_string();
            return Err(self.span_fatal_help(sp,
                                 &format!("expected `{{`, found `{}`", tok),
                                 "place this code inside a block"));
        }

        self.parse_block_tail(lo, BlockCheckMode::Default)
    }

    /// Parse a block. Inner attrs are allowed.
    fn parse_inner_attrs_and_block(&mut self) -> PResult<'a, (Vec<Attribute>, P<Block>)> {
        maybe_whole!(pair_empty self, NtBlock);

        let lo = self.span.lo;
        self.expect(&token::OpenDelim(token::Brace))?;
        Ok((self.parse_inner_attributes()?,
            self.parse_block_tail(lo, BlockCheckMode::Default)?))
    }

    /// Parse the rest of a block expression or function body
    /// Precondition: already parsed the '{'.
    fn parse_block_tail(&mut self, lo: BytePos, s: BlockCheckMode) -> PResult<'a, P<Block>> {
        let mut stmts = vec![];

        while !self.eat(&token::CloseDelim(token::Brace)) {
            let Stmt {node, span, ..} = if let Some(s) = self.parse_stmt_() {
                s
            } else if self.token == token::Eof {
                break;
            } else {
                // Found only `;` or `}`.
                continue;
            };

            match node {
                StmtKind::Expr(e) => {
                    self.handle_expression_like_statement(e, span, &mut stmts)?;
                }
                StmtKind::Mac(mac) => {
                    self.handle_macro_in_block(mac.unwrap(), span, &mut stmts)?;
                }
                _ => { // all other kinds of statements:
                    let mut hi = span.hi;
                    if classify::stmt_ends_with_semi(&node) {
                        self.commit_stmt_expecting(token::Semi)?;
                        hi = self.last_span.hi;
                    }

                    stmts.push(Stmt {
                        id: ast::DUMMY_NODE_ID,
                        node: node,
                        span: mk_sp(span.lo, hi)
                    });
                }
            }
        }

        Ok(P(ast::Block {
            stmts: stmts,
            id: ast::DUMMY_NODE_ID,
            rules: s,
            span: mk_sp(lo, self.last_span.hi),
        }))
    }

    fn handle_macro_in_block(&mut self,
                             (mac, style, attrs): (ast::Mac, MacStmtStyle, ThinVec<Attribute>),
                             span: Span,
                             stmts: &mut Vec<Stmt>)
                             -> PResult<'a, ()> {
        if style == MacStmtStyle::NoBraces {
            // statement macro without braces; might be an
            // expr depending on whether a semicolon follows
            match self.token {
                token::Semi => {
                    stmts.push(Stmt {
                        id: ast::DUMMY_NODE_ID,
                        node: StmtKind::Mac(P((mac, MacStmtStyle::Semicolon, attrs))),
                        span: mk_sp(span.lo, self.span.hi),
                    });
                    self.bump();
                }
                _ => {
                    let e = self.mk_mac_expr(span.lo, span.hi, mac.node, ThinVec::new());
                    let lo = e.span.lo;
                    let e = self.parse_dot_or_call_expr_with(e, lo, attrs)?;
                    let e = self.parse_assoc_expr_with(0, LhsExpr::AlreadyParsed(e))?;
                    self.handle_expression_like_statement(e, span, stmts)?;
                }
            }
        } else {
            // statement macro; might be an expr
            match self.token {
                token::Semi => {
                    stmts.push(Stmt {
                        id: ast::DUMMY_NODE_ID,
                        node: StmtKind::Mac(P((mac, MacStmtStyle::Semicolon, attrs))),
                        span: mk_sp(span.lo, self.span.hi),
                    });
                    self.bump();
                }
                _ => {
                    stmts.push(Stmt {
                        id: ast::DUMMY_NODE_ID,
                        node: StmtKind::Mac(P((mac, style, attrs))),
                        span: span
                    });
                }
            }
        }
        Ok(())
    }

    fn handle_expression_like_statement(&mut self,
                                        e: P<Expr>,
                                        span: Span,
                                        stmts: &mut Vec<Stmt>)
                                        -> PResult<'a, ()> {
        // expression without semicolon
        if classify::expr_requires_semi_to_be_stmt(&e) {
            // Just check for errors and recover; do not eat semicolon yet.
            if let Err(mut e) =
                self.commit_stmt(&[], &[token::Semi, token::CloseDelim(token::Brace)])
            {
                e.emit();
                self.recover_stmt();
            }
        }

        match self.token {
            token::Semi => {
                self.bump();
                let span_with_semi = Span {
                    lo: span.lo,
                    hi: self.last_span.hi,
                    expn_id: span.expn_id,
                };
                stmts.push(Stmt {
                    id: ast::DUMMY_NODE_ID,
                    node: StmtKind::Semi(e),
                    span: span_with_semi,
                });
            }
            _ => {
                stmts.push(Stmt {
                    id: ast::DUMMY_NODE_ID,
                    node: StmtKind::Expr(e),
                    span: span
                });
            }
        }
        Ok(())
    }

    // Parses a sequence of bounds if a `:` is found,
    // otherwise returns empty list.
    fn parse_colon_then_ty_param_bounds(&mut self,
                                        mode: BoundParsingMode)
                                        -> PResult<'a, TyParamBounds>
    {
        if !self.eat(&token::Colon) {
            Ok(P::new())
        } else {
            self.parse_ty_param_bounds(mode)
        }
    }

    // matches bounds    = ( boundseq )?
    // where   boundseq  = ( polybound + boundseq ) | polybound
    // and     polybound = ( 'for' '<' 'region '>' )? bound
    // and     bound     = 'region | trait_ref
    fn parse_ty_param_bounds(&mut self,
                             mode: BoundParsingMode)
                             -> PResult<'a, TyParamBounds>
    {
        let mut result = vec!();
        loop {
            let question_span = self.span;
            let ate_question = self.eat(&token::Question);
            match self.token {
                token::Lifetime(lifetime) => {
                    if ate_question {
                        self.span_err(question_span,
                                      "`?` may only modify trait bounds, not lifetime bounds");
                    }
                    result.push(RegionTyParamBound(ast::Lifetime {
                        id: ast::DUMMY_NODE_ID,
                        span: self.span,
                        name: lifetime.name
                    }));
                    self.bump();
                }
                token::ModSep | token::Ident(..) => {
                    let poly_trait_ref = self.parse_poly_trait_ref()?;
                    let modifier = if ate_question {
                        if mode == BoundParsingMode::Modified {
                            TraitBoundModifier::Maybe
                        } else {
                            self.span_err(question_span,
                                          "unexpected `?`");
                            TraitBoundModifier::None
                        }
                    } else {
                        TraitBoundModifier::None
                    };
                    result.push(TraitTyParamBound(poly_trait_ref, modifier))
                }
                _ => break,
            }

            if !self.eat(&token::BinOp(token::Plus)) {
                break;
            }
        }

        return Ok(P::from_vec(result));
    }

    /// Matches typaram = IDENT (`?` unbound)? optbounds ( EQ ty )?
    fn parse_ty_param(&mut self) -> PResult<'a, TyParam> {
        let span = self.span;
        let ident = self.parse_ident()?;

        let bounds = self.parse_colon_then_ty_param_bounds(BoundParsingMode::Modified)?;

        let default = if self.check(&token::Eq) {
            self.bump();
            Some(self.parse_ty_sum()?)
        } else {
            None
        };

        Ok(TyParam {
            ident: ident,
            id: ast::DUMMY_NODE_ID,
            bounds: bounds,
            default: default,
            span: span,
        })
    }

    /// Parse a set of optional generic type parameter declarations. Where
    /// clauses are not parsed here, and must be added later via
    /// `parse_where_clause()`.
    ///
    /// matches generics = ( ) | ( < > ) | ( < typaramseq ( , )? > ) | ( < lifetimes ( , )? > )
    ///                  | ( < lifetimes , typaramseq ( , )? > )
    /// where   typaramseq = ( typaram ) | ( typaram , typaramseq )
    pub fn parse_generics(&mut self) -> PResult<'a, ast::Generics> {
        maybe_whole!(self, NtGenerics);

        if self.eat(&token::Lt) {
            let lifetime_defs = self.parse_lifetime_defs()?;
            let mut seen_default = false;
            let ty_params = self.parse_seq_to_gt(Some(token::Comma), |p| {
                p.forbid_lifetime()?;
                let ty_param = p.parse_ty_param()?;
                if ty_param.default.is_some() {
                    seen_default = true;
                } else if seen_default {
                    let last_span = p.last_span;
                    p.span_err(last_span,
                               "type parameters with a default must be trailing");
                }
                Ok(ty_param)
            })?;
            Ok(ast::Generics {
                lifetimes: lifetime_defs,
                ty_params: ty_params,
                where_clause: WhereClause {
                    id: ast::DUMMY_NODE_ID,
                    predicates: Vec::new(),
                }
            })
        } else {
            Ok(ast::Generics::default())
        }
    }

    fn parse_generic_values_after_lt(&mut self) -> PResult<'a, (Vec<ast::Lifetime>,
                                                            Vec<P<Ty>>,
                                                            Vec<TypeBinding>)> {
        let span_lo = self.span.lo;
        let lifetimes = self.parse_lifetimes(token::Comma)?;

        let missing_comma = !lifetimes.is_empty() &&
                            !self.token.is_like_gt() &&
                            self.last_token
                                .as_ref().map_or(true,
                                                 |x| &**x != &token::Comma);

        if missing_comma {

            let msg = format!("expected `,` or `>` after lifetime \
                              name, found `{}`",
                              self.this_token_to_string());
            let mut err = self.diagnostic().struct_span_err(self.span, &msg);

            let span_hi = self.span.hi;
            let span_hi = match self.parse_ty() {
                Ok(..) => self.span.hi,
                Err(ref mut err) => {
                    err.cancel();
                    span_hi
                }
            };

            let msg = format!("did you mean a single argument type &'a Type, \
                              or did you mean the comma-separated arguments \
                              'a, Type?");
            err.span_note(mk_sp(span_lo, span_hi), &msg);
            return Err(err);
        }

        // First parse types.
        let (types, returned) = self.parse_seq_to_gt_or_return(
            Some(token::Comma),
            |p| {
                p.forbid_lifetime()?;
                if p.look_ahead(1, |t| t == &token::Eq) {
                    Ok(None)
                } else {
                    Ok(Some(p.parse_ty_sum()?))
                }
            }
        )?;

        // If we found the `>`, don't continue.
        if !returned {
            return Ok((lifetimes, types.into_vec(), Vec::new()));
        }

        // Then parse type bindings.
        let bindings = self.parse_seq_to_gt(
            Some(token::Comma),
            |p| {
                p.forbid_lifetime()?;
                let lo = p.span.lo;
                let ident = p.parse_ident()?;
                p.expect(&token::Eq)?;
                let ty = p.parse_ty()?;
                let hi = ty.span.hi;
                let span = mk_sp(lo, hi);
                return Ok(TypeBinding{id: ast::DUMMY_NODE_ID,
                    ident: ident,
                    ty: ty,
                    span: span,
                });
            }
        )?;
        Ok((lifetimes, types.into_vec(), bindings.into_vec()))
    }

    fn forbid_lifetime(&mut self) -> PResult<'a, ()> {
        if self.token.is_lifetime() {
            let span = self.span;
            return Err(self.diagnostic().struct_span_err(span, "lifetime parameters must be \
                                                                declared prior to type parameters"))
        }
        Ok(())
    }

    /// Parses an optional `where` clause and places it in `generics`.
    ///
    /// ```ignore
    /// where T : Trait<U, V> + 'b, 'a : 'b
    /// ```
    pub fn parse_where_clause(&mut self) -> PResult<'a, ast::WhereClause> {
        maybe_whole!(self, NtWhereClause);

        let mut where_clause = WhereClause {
            id: ast::DUMMY_NODE_ID,
            predicates: Vec::new(),
        };

        if !self.eat_keyword(keywords::Where) {
            return Ok(where_clause);
        }

        let mut parsed_something = false;
        loop {
            let lo = self.span.lo;
            match self.token {
                token::OpenDelim(token::Brace) => {
                    break
                }

                token::Lifetime(..) => {
                    let bounded_lifetime =
                        self.parse_lifetime()?;

                    self.eat(&token::Colon);

                    let bounds =
                        self.parse_lifetimes(token::BinOp(token::Plus))?;

                    let hi = self.last_span.hi;
                    let span = mk_sp(lo, hi);

                    where_clause.predicates.push(ast::WherePredicate::RegionPredicate(
                        ast::WhereRegionPredicate {
                            span: span,
                            lifetime: bounded_lifetime,
                            bounds: bounds
                        }
                    ));

                    parsed_something = true;
                }

                _ => {
                    let bound_lifetimes = if self.eat_keyword(keywords::For) {
                        // Higher ranked constraint.
                        self.expect(&token::Lt)?;
                        let lifetime_defs = self.parse_lifetime_defs()?;
                        self.expect_gt()?;
                        lifetime_defs
                    } else {
                        vec![]
                    };

                    let bounded_ty = self.parse_ty()?;

                    if self.eat(&token::Colon) {
                        let bounds = self.parse_ty_param_bounds(BoundParsingMode::Bare)?;
                        let hi = self.last_span.hi;
                        let span = mk_sp(lo, hi);

                        if bounds.is_empty() {
                            self.span_err(span,
                                          "each predicate in a `where` clause must have \
                                           at least one bound in it");
                        }

                        where_clause.predicates.push(ast::WherePredicate::BoundPredicate(
                                ast::WhereBoundPredicate {
                                    span: span,
                                    bound_lifetimes: bound_lifetimes,
                                    bounded_ty: bounded_ty,
                                    bounds: bounds,
                        }));

                        parsed_something = true;
                    } else if self.eat(&token::Eq) {
                        // let ty = try!(self.parse_ty());
                        let hi = self.last_span.hi;
                        let span = mk_sp(lo, hi);
                        // where_clause.predicates.push(
                        //     ast::WherePredicate::EqPredicate(ast::WhereEqPredicate {
                        //         id: ast::DUMMY_NODE_ID,
                        //         span: span,
                        //         path: panic!("NYI"), //bounded_ty,
                        //         ty: ty,
                        // }));
                        // parsed_something = true;
                        // // FIXME(#18433)
                        self.span_err(span,
                                     "equality constraints are not yet supported \
                                     in where clauses (#20041)");
                    } else {
                        let last_span = self.last_span;
                        self.span_err(last_span,
                              "unexpected token in `where` clause");
                    }
                }
            };

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

        Ok(where_clause)
    }

    fn parse_fn_args(&mut self, named_args: bool, allow_variadic: bool)
                     -> PResult<'a, (Vec<Arg> , bool)> {
        let sp = self.span;
        let mut variadic = false;
        let args: Vec<Option<Arg>> =
            self.parse_unspanned_seq(
                &token::OpenDelim(token::Paren),
                &token::CloseDelim(token::Paren),
                SeqSep::trailing_allowed(token::Comma),
                |p| {
                    if p.token == token::DotDotDot {
                        p.bump();
                        if allow_variadic {
                            if p.token != token::CloseDelim(token::Paren) {
                                let span = p.span;
                                p.span_err(span,
                                    "`...` must be last in argument list for variadic function");
                            }
                        } else {
                            let span = p.span;
                            p.span_err(span,
                                       "only foreign functions are allowed to be variadic");
                        }
                        variadic = true;
                        Ok(None)
                    } else {
                        match p.parse_arg_general(named_args) {
                            Ok(arg) => Ok(Some(arg)),
                            Err(mut e) => {
                                e.emit();
                                p.eat_to_tokens(&[&token::Comma, &token::CloseDelim(token::Paren)]);
                                Ok(None)
                            }
                        }
                    }
                }
            )?;

        let args: Vec<_> = args.into_iter().filter_map(|x| x).collect();

        if variadic && args.is_empty() {
            self.span_err(sp,
                          "variadic function must be declared with at least one named argument");
        }

        Ok((args, variadic))
    }

    /// Parse the argument list and result type of a function declaration
    pub fn parse_fn_decl(&mut self, allow_variadic: bool) -> PResult<'a, P<FnDecl>> {

        let (args, variadic) = self.parse_fn_args(true, allow_variadic)?;
        let ret_ty = self.parse_ret_ty()?;

        Ok(P(FnDecl {
            inputs: args,
            output: ret_ty,
            variadic: variadic
        }))
    }

    /// Returns the parsed optional self argument and whether a self shortcut was used.
    fn parse_self_arg(&mut self) -> PResult<'a, Option<Arg>> {
        let expect_ident = |this: &mut Self| match this.token {
            // Preserve hygienic context.
            token::Ident(ident) => { this.bump(); codemap::respan(this.last_span, ident) }
            _ => unreachable!()
        };

        // Parse optional self parameter of a method.
        // Only a limited set of initial token sequences is considered self parameters, anything
        // else is parsed as a normal function parameter list, so some lookahead is required.
        let eself_lo = self.span.lo;
        let (eself, eself_ident) = match self.token {
            token::BinOp(token::And) => {
                // &self
                // &mut self
                // &'lt self
                // &'lt mut self
                // &not_self
                if self.look_ahead(1, |t| t.is_keyword(keywords::SelfValue)) {
                    self.bump();
                    (SelfKind::Region(None, Mutability::Immutable), expect_ident(self))
                } else if self.look_ahead(1, |t| t.is_keyword(keywords::Mut)) &&
                          self.look_ahead(2, |t| t.is_keyword(keywords::SelfValue)) {
                    self.bump();
                    self.bump();
                    (SelfKind::Region(None, Mutability::Mutable), expect_ident(self))
                } else if self.look_ahead(1, |t| t.is_lifetime()) &&
                          self.look_ahead(2, |t| t.is_keyword(keywords::SelfValue)) {
                    self.bump();
                    let lt = self.parse_lifetime()?;
                    (SelfKind::Region(Some(lt), Mutability::Immutable), expect_ident(self))
                } else if self.look_ahead(1, |t| t.is_lifetime()) &&
                          self.look_ahead(2, |t| t.is_keyword(keywords::Mut)) &&
                          self.look_ahead(3, |t| t.is_keyword(keywords::SelfValue)) {
                    self.bump();
                    let lt = self.parse_lifetime()?;
                    self.bump();
                    (SelfKind::Region(Some(lt), Mutability::Mutable), expect_ident(self))
                } else {
                    return Ok(None);
                }
            }
            token::BinOp(token::Star) => {
                // *self
                // *const self
                // *mut self
                // *not_self
                // Emit special error for `self` cases.
                if self.look_ahead(1, |t| t.is_keyword(keywords::SelfValue)) {
                    self.bump();
                    self.span_err(self.span, "cannot pass `self` by raw pointer");
                    (SelfKind::Value(Mutability::Immutable), expect_ident(self))
                } else if self.look_ahead(1, |t| t.is_mutability()) &&
                          self.look_ahead(2, |t| t.is_keyword(keywords::SelfValue)) {
                    self.bump();
                    self.bump();
                    self.span_err(self.span, "cannot pass `self` by raw pointer");
                    (SelfKind::Value(Mutability::Immutable), expect_ident(self))
                } else {
                    return Ok(None);
                }
            }
            token::Ident(..) => {
                if self.token.is_keyword(keywords::SelfValue) {
                    // self
                    // self: TYPE
                    let eself_ident = expect_ident(self);
                    if self.eat(&token::Colon) {
                        let ty = self.parse_ty_sum()?;
                        (SelfKind::Explicit(ty, Mutability::Immutable), eself_ident)
                    } else {
                        (SelfKind::Value(Mutability::Immutable), eself_ident)
                    }
                } else if self.token.is_keyword(keywords::Mut) &&
                        self.look_ahead(1, |t| t.is_keyword(keywords::SelfValue)) {
                    // mut self
                    // mut self: TYPE
                    self.bump();
                    let eself_ident = expect_ident(self);
                    if self.eat(&token::Colon) {
                        let ty = self.parse_ty_sum()?;
                        (SelfKind::Explicit(ty, Mutability::Mutable), eself_ident)
                    } else {
                        (SelfKind::Value(Mutability::Mutable), eself_ident)
                    }
                } else {
                    return Ok(None);
                }
            }
            _ => return Ok(None),
        };

        let eself = codemap::respan(mk_sp(eself_lo, self.last_span.hi), eself);
        Ok(Some(Arg::from_self(eself, eself_ident)))
    }

    /// Parse the parameter list and result type of a function that may have a `self` parameter.
    fn parse_fn_decl_with_self<F>(&mut self, parse_arg_fn: F) -> PResult<'a, P<FnDecl>>
        where F: FnMut(&mut Parser<'a>) -> PResult<'a,  Arg>,
    {
        self.expect(&token::OpenDelim(token::Paren))?;

        // Parse optional self argument
        let self_arg = self.parse_self_arg()?;

        // Parse the rest of the function parameter list.
        let sep = SeqSep::trailing_allowed(token::Comma);
        let fn_inputs = if let Some(self_arg) = self_arg {
            if self.check(&token::CloseDelim(token::Paren)) {
                vec![self_arg]
            } else if self.eat(&token::Comma) {
                let mut fn_inputs = vec![self_arg];
                fn_inputs.append(&mut self.parse_seq_to_before_end(
                    &token::CloseDelim(token::Paren), sep, parse_arg_fn)
                );
                fn_inputs
            } else {
                return self.unexpected();
            }
        } else {
            self.parse_seq_to_before_end(&token::CloseDelim(token::Paren), sep, parse_arg_fn)
        };

        // Parse closing paren and return type.
        self.expect(&token::CloseDelim(token::Paren))?;
        Ok(P(FnDecl {
            inputs: fn_inputs,
            output: self.parse_ret_ty()?,
            variadic: false
        }))
    }

    // parse the |arg, arg| header on a lambda
    fn parse_fn_block_decl(&mut self) -> PResult<'a, P<FnDecl>> {
        let inputs_captures = {
            if self.eat(&token::OrOr) {
                Vec::new()
            } else {
                self.expect(&token::BinOp(token::Or))?;
                self.parse_obsolete_closure_kind()?;
                let args = self.parse_seq_to_before_end(
                    &token::BinOp(token::Or),
                    SeqSep::trailing_allowed(token::Comma),
                    |p| p.parse_fn_block_arg()
                );
                self.bump();
                args
            }
        };
        let output = self.parse_ret_ty()?;

        Ok(P(FnDecl {
            inputs: inputs_captures,
            output: output,
            variadic: false
        }))
    }

    /// Parse the name and optional generic types of a function header.
    fn parse_fn_header(&mut self) -> PResult<'a, (Ident, ast::Generics)> {
        let id = self.parse_ident()?;
        let generics = self.parse_generics()?;
        Ok((id, generics))
    }

    fn mk_item(&mut self, lo: BytePos, hi: BytePos, ident: Ident,
               node: ItemKind, vis: Visibility,
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
    fn parse_item_fn(&mut self,
                     unsafety: Unsafety,
                     constness: Constness,
                     abi: abi::Abi)
                     -> PResult<'a, ItemInfo> {
        let (ident, mut generics) = self.parse_fn_header()?;
        let decl = self.parse_fn_decl(false)?;
        generics.where_clause = self.parse_where_clause()?;
        let (inner_attrs, body) = self.parse_inner_attrs_and_block()?;
        Ok((ident, ItemKind::Fn(decl, unsafety, constness, abi, generics, body), Some(inner_attrs)))
    }

    /// true if we are looking at `const ID`, false for things like `const fn` etc
    pub fn is_const_item(&mut self) -> bool {
        self.token.is_keyword(keywords::Const) &&
            !self.look_ahead(1, |t| t.is_keyword(keywords::Fn)) &&
            !self.look_ahead(1, |t| t.is_keyword(keywords::Unsafe))
    }

    /// parses all the "front matter" for a `fn` declaration, up to
    /// and including the `fn` keyword:
    ///
    /// - `const fn`
    /// - `unsafe fn`
    /// - `const unsafe fn`
    /// - `extern fn`
    /// - etc
    pub fn parse_fn_front_matter(&mut self)
                                 -> PResult<'a, (ast::Constness, ast::Unsafety, abi::Abi)> {
        let is_const_fn = self.eat_keyword(keywords::Const);
        let unsafety = self.parse_unsafety()?;
        let (constness, unsafety, abi) = if is_const_fn {
            (Constness::Const, unsafety, Abi::Rust)
        } else {
            let abi = if self.eat_keyword(keywords::Extern) {
                self.parse_opt_abi()?.unwrap_or(Abi::C)
            } else {
                Abi::Rust
            };
            (Constness::NotConst, unsafety, abi)
        };
        self.expect_keyword(keywords::Fn)?;
        Ok((constness, unsafety, abi))
    }

    /// Parse an impl item.
    pub fn parse_impl_item(&mut self) -> PResult<'a, ImplItem> {
        maybe_whole!(no_clone_from_p self, NtImplItem);

        let mut attrs = self.parse_outer_attributes()?;
        let lo = self.span.lo;
        let vis = self.parse_visibility(true)?;
        let defaultness = self.parse_defaultness()?;
        let (name, node) = if self.eat_keyword(keywords::Type) {
            let name = self.parse_ident()?;
            self.expect(&token::Eq)?;
            let typ = self.parse_ty_sum()?;
            self.expect(&token::Semi)?;
            (name, ast::ImplItemKind::Type(typ))
        } else if self.is_const_item() {
            self.expect_keyword(keywords::Const)?;
            let name = self.parse_ident()?;
            self.expect(&token::Colon)?;
            let typ = self.parse_ty_sum()?;
            self.expect(&token::Eq)?;
            let expr = self.parse_expr()?;
            self.commit_expr_expecting(&expr, token::Semi)?;
            (name, ast::ImplItemKind::Const(typ, expr))
        } else {
            let (name, inner_attrs, node) = self.parse_impl_method(&vis)?;
            attrs.extend(inner_attrs);
            (name, node)
        };

        Ok(ImplItem {
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, self.last_span.hi),
            ident: name,
            vis: vis,
            defaultness: defaultness,
            attrs: attrs,
            node: node
        })
    }

    fn complain_if_pub_macro(&mut self, visa: &Visibility, span: Span) {
        match *visa {
            Visibility::Inherited => (),
            _ => {
                let is_macro_rules: bool = match self.token {
                    token::Ident(sid) => sid.name == intern("macro_rules"),
                    _ => false,
                };
                if is_macro_rules {
                    self.diagnostic().struct_span_err(span, "can't qualify macro_rules \
                                                             invocation with `pub`")
                                     .help("did you mean #[macro_export]?")
                                     .emit();
                } else {
                    self.diagnostic().struct_span_err(span, "can't qualify macro \
                                                             invocation with `pub`")
                                     .help("try adjusting the macro to put `pub` \
                                            inside the invocation")
                                     .emit();
                }
            }
        }
    }

    /// Parse a method or a macro invocation in a trait impl.
    fn parse_impl_method(&mut self, vis: &Visibility)
                         -> PResult<'a, (Ident, Vec<ast::Attribute>, ast::ImplItemKind)> {
        // code copied from parse_macro_use_or_failure... abstraction!
        if !self.token.is_any_keyword()
            && self.look_ahead(1, |t| *t == token::Not)
            && (self.look_ahead(2, |t| *t == token::OpenDelim(token::Paren))
                || self.look_ahead(2, |t| *t == token::OpenDelim(token::Brace))) {
            // method macro.

            let last_span = self.last_span;
            self.complain_if_pub_macro(&vis, last_span);

            let lo = self.span.lo;
            let pth = self.parse_ident_into_path()?;
            self.expect(&token::Not)?;

            // eat a matched-delimiter token tree:
            let delim = self.expect_open_delim()?;
            let tts = self.parse_seq_to_end(&token::CloseDelim(delim),
                                            SeqSep::none(),
                                            |p| p.parse_token_tree())?;
            let m_ = Mac_ { path: pth, tts: tts };
            let m: ast::Mac = codemap::Spanned { node: m_,
                                                    span: mk_sp(lo,
                                                                self.last_span.hi) };
            if delim != token::Brace {
                self.expect(&token::Semi)?
            }
            Ok((keywords::Invalid.ident(), vec![], ast::ImplItemKind::Macro(m)))
        } else {
            let (constness, unsafety, abi) = self.parse_fn_front_matter()?;
            let ident = self.parse_ident()?;
            let mut generics = self.parse_generics()?;
            let decl = self.parse_fn_decl_with_self(|p| p.parse_arg())?;
            generics.where_clause = self.parse_where_clause()?;
            let (inner_attrs, body) = self.parse_inner_attrs_and_block()?;
            Ok((ident, inner_attrs, ast::ImplItemKind::Method(ast::MethodSig {
                generics: generics,
                abi: abi,
                unsafety: unsafety,
                constness: constness,
                decl: decl
             }, body)))
        }
    }

    /// Parse trait Foo { ... }
    fn parse_item_trait(&mut self, unsafety: Unsafety) -> PResult<'a, ItemInfo> {
        let ident = self.parse_ident()?;
        let mut tps = self.parse_generics()?;

        // Parse supertrait bounds.
        let bounds = self.parse_colon_then_ty_param_bounds(BoundParsingMode::Bare)?;

        tps.where_clause = self.parse_where_clause()?;

        let meths = self.parse_trait_items()?;
        Ok((ident, ItemKind::Trait(unsafety, tps, bounds, meths), None))
    }

    /// Parses items implementations variants
    ///    impl<T> Foo { ... }
    ///    impl<T> ToString for &'static T { ... }
    ///    impl Send for .. {}
    fn parse_item_impl(&mut self, unsafety: ast::Unsafety) -> PResult<'a, ItemInfo> {
        let impl_span = self.span;

        // First, parse type parameters if necessary.
        let mut generics = self.parse_generics()?;

        // Special case: if the next identifier that follows is '(', don't
        // allow this to be parsed as a trait.
        let could_be_trait = self.token != token::OpenDelim(token::Paren);

        let neg_span = self.span;
        let polarity = if self.eat(&token::Not) {
            ast::ImplPolarity::Negative
        } else {
            ast::ImplPolarity::Positive
        };

        // Parse the trait.
        let mut ty = self.parse_ty_sum()?;

        // Parse traits, if necessary.
        let opt_trait = if could_be_trait && self.eat_keyword(keywords::For) {
            // New-style trait. Reinterpret the type as a trait.
            match ty.node {
                TyKind::Path(None, ref path) => {
                    Some(TraitRef {
                        path: (*path).clone(),
                        ref_id: ty.id,
                    })
                }
                _ => {
                    self.span_err(ty.span, "not a trait");
                    None
                }
            }
        } else {
            match polarity {
                ast::ImplPolarity::Negative => {
                    // This is a negated type implementation
                    // `impl !MyType {}`, which is not allowed.
                    self.span_err(neg_span, "inherent implementation can't be negated");
                },
                _ => {}
            }
            None
        };

        if opt_trait.is_some() && self.eat(&token::DotDot) {
            if generics.is_parameterized() {
                self.span_err(impl_span, "default trait implementations are not \
                                          allowed to have generics");
            }

            self.expect(&token::OpenDelim(token::Brace))?;
            self.expect(&token::CloseDelim(token::Brace))?;
            Ok((keywords::Invalid.ident(),
             ItemKind::DefaultImpl(unsafety, opt_trait.unwrap()), None))
        } else {
            if opt_trait.is_some() {
                ty = self.parse_ty_sum()?;
            }
            generics.where_clause = self.parse_where_clause()?;

            self.expect(&token::OpenDelim(token::Brace))?;
            let attrs = self.parse_inner_attributes()?;

            let mut impl_items = vec![];
            while !self.eat(&token::CloseDelim(token::Brace)) {
                impl_items.push(self.parse_impl_item()?);
            }

            Ok((keywords::Invalid.ident(),
             ItemKind::Impl(unsafety, polarity, generics, opt_trait, ty, impl_items),
             Some(attrs)))
        }
    }

    /// Parse a::B<String,i32>
    fn parse_trait_ref(&mut self) -> PResult<'a, TraitRef> {
        Ok(ast::TraitRef {
            path: self.parse_path(PathStyle::Type)?,
            ref_id: ast::DUMMY_NODE_ID,
        })
    }

    fn parse_late_bound_lifetime_defs(&mut self) -> PResult<'a, Vec<ast::LifetimeDef>> {
        if self.eat_keyword(keywords::For) {
            self.expect(&token::Lt)?;
            let lifetime_defs = self.parse_lifetime_defs()?;
            self.expect_gt()?;
            Ok(lifetime_defs)
        } else {
            Ok(Vec::new())
        }
    }

    /// Parse for<'l> a::B<String,i32>
    fn parse_poly_trait_ref(&mut self) -> PResult<'a, PolyTraitRef> {
        let lo = self.span.lo;
        let lifetime_defs = self.parse_late_bound_lifetime_defs()?;

        Ok(ast::PolyTraitRef {
            bound_lifetimes: lifetime_defs,
            trait_ref: self.parse_trait_ref()?,
            span: mk_sp(lo, self.last_span.hi),
        })
    }

    /// Parse struct Foo { ... }
    fn parse_item_struct(&mut self) -> PResult<'a, ItemInfo> {
        let class_name = self.parse_ident()?;
        let mut generics = self.parse_generics()?;

        // There is a special case worth noting here, as reported in issue #17904.
        // If we are parsing a tuple struct it is the case that the where clause
        // should follow the field list. Like so:
        //
        // struct Foo<T>(T) where T: Copy;
        //
        // If we are parsing a normal record-style struct it is the case
        // that the where clause comes before the body, and after the generics.
        // So if we look ahead and see a brace or a where-clause we begin
        // parsing a record style struct.
        //
        // Otherwise if we look ahead and see a paren we parse a tuple-style
        // struct.

        let vdata = if self.token.is_keyword(keywords::Where) {
            generics.where_clause = self.parse_where_clause()?;
            if self.eat(&token::Semi) {
                // If we see a: `struct Foo<T> where T: Copy;` style decl.
                VariantData::Unit(ast::DUMMY_NODE_ID)
            } else {
                // If we see: `struct Foo<T> where T: Copy { ... }`
                VariantData::Struct(self.parse_record_struct_body()?, ast::DUMMY_NODE_ID)
            }
        // No `where` so: `struct Foo<T>;`
        } else if self.eat(&token::Semi) {
            VariantData::Unit(ast::DUMMY_NODE_ID)
        // Record-style struct definition
        } else if self.token == token::OpenDelim(token::Brace) {
            VariantData::Struct(self.parse_record_struct_body()?, ast::DUMMY_NODE_ID)
        // Tuple-style struct definition with optional where-clause.
        } else if self.token == token::OpenDelim(token::Paren) {
            let body = VariantData::Tuple(self.parse_tuple_struct_body()?, ast::DUMMY_NODE_ID);
            generics.where_clause = self.parse_where_clause()?;
            self.expect(&token::Semi)?;
            body
        } else {
            let token_str = self.this_token_to_string();
            return Err(self.fatal(&format!("expected `where`, `{{`, `(`, or `;` after struct \
                                            name, found `{}`", token_str)))
        };

        Ok((class_name, ItemKind::Struct(vdata, generics), None))
    }

    pub fn parse_record_struct_body(&mut self) -> PResult<'a, Vec<StructField>> {
        let mut fields = Vec::new();
        if self.eat(&token::OpenDelim(token::Brace)) {
            while self.token != token::CloseDelim(token::Brace) {
                fields.push(self.parse_struct_decl_field()?);
            }

            self.bump();
        } else {
            let token_str = self.this_token_to_string();
            return Err(self.fatal(&format!("expected `where`, or `{{` after struct \
                                name, found `{}`",
                                token_str)));
        }

        Ok(fields)
    }

    pub fn parse_tuple_struct_body(&mut self) -> PResult<'a, Vec<StructField>> {
        // This is the case where we find `struct Foo<T>(T) where T: Copy;`
        // Unit like structs are handled in parse_item_struct function
        let fields = self.parse_unspanned_seq(
            &token::OpenDelim(token::Paren),
            &token::CloseDelim(token::Paren),
            SeqSep::trailing_allowed(token::Comma),
            |p| {
                let attrs = p.parse_outer_attributes()?;
                let lo = p.span.lo;
                let mut vis = p.parse_visibility(false)?;
                let ty_is_interpolated =
                    p.token.is_interpolated() || p.look_ahead(1, |t| t.is_interpolated());
                let mut ty = p.parse_ty_sum()?;

                // Handle `pub(path) type`, in which `vis` will be `pub` and `ty` will be `(path)`.
                if vis == Visibility::Public && !ty_is_interpolated &&
                   p.token != token::Comma && p.token != token::CloseDelim(token::Paren) {
                    ty = if let TyKind::Paren(ref path_ty) = ty.node {
                        if let TyKind::Path(None, ref path) = path_ty.node {
                            vis = Visibility::Restricted { path: P(path.clone()), id: path_ty.id };
                            Some(p.parse_ty_sum()?)
                        } else {
                            None
                        }
                    } else {
                        None
                    }.unwrap_or(ty);
                }
                Ok(StructField {
                    span: mk_sp(lo, p.span.hi),
                    vis: vis,
                    ident: None,
                    id: ast::DUMMY_NODE_ID,
                    ty: ty,
                    attrs: attrs,
                })
            })?;

        Ok(fields)
    }

    /// Parse a structure field declaration
    pub fn parse_single_struct_field(&mut self,
                                     vis: Visibility,
                                     attrs: Vec<Attribute> )
                                     -> PResult<'a, StructField> {
        let a_var = self.parse_name_and_ty(vis, attrs)?;
        match self.token {
            token::Comma => {
                self.bump();
            }
            token::CloseDelim(token::Brace) => {}
            _ => {
                let span = self.span;
                let token_str = self.this_token_to_string();
                return Err(self.span_fatal_help(span,
                                     &format!("expected `,`, or `}}`, found `{}`",
                                             token_str),
                                     "struct fields should be separated by commas"))
            }
        }
        Ok(a_var)
    }

    /// Parse an element of a struct definition
    fn parse_struct_decl_field(&mut self) -> PResult<'a, StructField> {
        let attrs = self.parse_outer_attributes()?;
        let vis = self.parse_visibility(true)?;
        self.parse_single_struct_field(vis, attrs)
    }

    // If `allow_path` is false, just parse the `pub` in `pub(path)` (but still parse `pub(crate)`)
    fn parse_visibility(&mut self, allow_path: bool) -> PResult<'a, Visibility> {
        let pub_crate = |this: &mut Self| {
            let span = this.last_span;
            this.expect(&token::CloseDelim(token::Paren))?;
            Ok(Visibility::Crate(span))
        };

        if !self.eat_keyword(keywords::Pub) {
            Ok(Visibility::Inherited)
        } else if !allow_path {
            // Look ahead to avoid eating the `(` in `pub(path)` while still parsing `pub(crate)`
            if self.token == token::OpenDelim(token::Paren) &&
               self.look_ahead(1, |t| t.is_keyword(keywords::Crate)) {
                self.bump(); self.bump();
                pub_crate(self)
            } else {
                Ok(Visibility::Public)
            }
        } else if !self.eat(&token::OpenDelim(token::Paren)) {
            Ok(Visibility::Public)
        } else if self.eat_keyword(keywords::Crate) {
            pub_crate(self)
        } else {
            let path = self.parse_path(PathStyle::Mod)?;
            self.expect(&token::CloseDelim(token::Paren))?;
            Ok(Visibility::Restricted { path: P(path), id: ast::DUMMY_NODE_ID })
        }
    }

    /// Parse defaultness: DEFAULT or nothing
    fn parse_defaultness(&mut self) -> PResult<'a, Defaultness> {
        if self.eat_contextual_keyword(keywords::Default.ident()) {
            Ok(Defaultness::Default)
        } else {
            Ok(Defaultness::Final)
        }
    }

    /// Given a termination token, parse all of the items in a module
    fn parse_mod_items(&mut self, term: &token::Token, inner_lo: BytePos) -> PResult<'a, Mod> {
        let mut items = vec![];
        while let Some(item) = self.parse_item()? {
            items.push(item);
        }

        if !self.eat(term) {
            let token_str = self.this_token_to_string();
            return Err(self.fatal(&format!("expected item, found `{}`", token_str)));
        }

        let hi = if self.span == syntax_pos::DUMMY_SP {
            inner_lo
        } else {
            self.last_span.hi
        };

        Ok(ast::Mod {
            inner: mk_sp(inner_lo, hi),
            items: items
        })
    }

    fn parse_item_const(&mut self, m: Option<Mutability>) -> PResult<'a, ItemInfo> {
        let id = self.parse_ident()?;
        self.expect(&token::Colon)?;
        let ty = self.parse_ty_sum()?;
        self.expect(&token::Eq)?;
        let e = self.parse_expr()?;
        self.commit_expr_expecting(&e, token::Semi)?;
        let item = match m {
            Some(m) => ItemKind::Static(ty, m, e),
            None => ItemKind::Const(ty, e),
        };
        Ok((id, item, None))
    }

    /// Parse a `mod <foo> { ... }` or `mod <foo>;` item
    fn parse_item_mod(&mut self, outer_attrs: &[Attribute]) -> PResult<'a, ItemInfo> {
        let id_span = self.span;
        let id = self.parse_ident()?;
        if self.check(&token::Semi) {
            self.bump();
            // This mod is in an external file. Let's go get it!
            let (m, attrs) = self.eval_src_mod(id, outer_attrs, id_span)?;
            Ok((id, m, Some(attrs)))
        } else {
            self.push_mod_path(id, outer_attrs);
            self.expect(&token::OpenDelim(token::Brace))?;
            let mod_inner_lo = self.span.lo;
            let attrs = self.parse_inner_attributes()?;
            let m = self.parse_mod_items(&token::CloseDelim(token::Brace), mod_inner_lo)?;
            self.pop_mod_path();
            Ok((id, ItemKind::Mod(m), Some(attrs)))
        }
    }

    fn push_mod_path(&mut self, id: Ident, attrs: &[Attribute]) {
        let default_path = self.id_to_interned_str(id);
        let file_path = match ::attr::first_attr_value_str_by_name(attrs, "path") {
            Some(d) => d,
            None => default_path,
        };
        self.mod_path_stack.push(file_path)
    }

    fn pop_mod_path(&mut self) {
        self.mod_path_stack.pop().unwrap();
    }

    pub fn submod_path_from_attr(attrs: &[ast::Attribute], dir_path: &Path) -> Option<PathBuf> {
        ::attr::first_attr_value_str_by_name(attrs, "path").map(|d| dir_path.join(&*d))
    }

    /// Returns either a path to a module, or .
    pub fn default_submod_path(id: ast::Ident, dir_path: &Path, codemap: &CodeMap) -> ModulePath
    {
        let mod_name = id.to_string();
        let default_path_str = format!("{}.rs", mod_name);
        let secondary_path_str = format!("{}/mod.rs", mod_name);
        let default_path = dir_path.join(&default_path_str);
        let secondary_path = dir_path.join(&secondary_path_str);
        let default_exists = codemap.file_exists(&default_path);
        let secondary_exists = codemap.file_exists(&secondary_path);

        let result = match (default_exists, secondary_exists) {
            (true, false) => Ok(ModulePathSuccess { path: default_path, owns_directory: false }),
            (false, true) => Ok(ModulePathSuccess { path: secondary_path, owns_directory: true }),
            (false, false) => Err(ModulePathError {
                err_msg: format!("file not found for module `{}`", mod_name),
                help_msg: format!("name the file either {} or {} inside the directory {:?}",
                                  default_path_str,
                                  secondary_path_str,
                                  dir_path.display()),
            }),
            (true, true) => Err(ModulePathError {
                err_msg: format!("file for module `{}` found at both {} and {}",
                                 mod_name,
                                 default_path_str,
                                 secondary_path_str),
                help_msg: "delete or rename one of them to remove the ambiguity".to_owned(),
            }),
        };

        ModulePath {
            name: mod_name,
            path_exists: default_exists || secondary_exists,
            result: result,
        }
    }

    fn submod_path(&mut self,
                   id: ast::Ident,
                   outer_attrs: &[ast::Attribute],
                   id_sp: Span) -> PResult<'a, ModulePathSuccess> {
        let mut prefix = PathBuf::from(self.filename.as_ref().unwrap());
        prefix.pop();
        let mut dir_path = prefix;
        for part in &self.mod_path_stack {
            dir_path.push(&**part);
        }

        if let Some(p) = Parser::submod_path_from_attr(outer_attrs, &dir_path) {
            return Ok(ModulePathSuccess { path: p, owns_directory: true });
        }

        let paths = Parser::default_submod_path(id, &dir_path, self.sess.codemap());

        if self.restrictions.contains(Restrictions::NO_NONINLINE_MOD) {
            let msg =
                "Cannot declare a non-inline module inside a block unless it has a path attribute";
            let mut err = self.diagnostic().struct_span_err(id_sp, msg);
            if paths.path_exists {
                let msg = format!("Maybe `use` the module `{}` instead of redeclaring it",
                                  paths.name);
                err.span_note(id_sp, &msg);
            }
            return Err(err);
        } else if !self.owns_directory {
            let mut err = self.diagnostic().struct_span_err(id_sp,
                "cannot declare a new module at this location");
            let this_module = match self.mod_path_stack.last() {
                Some(name) => name.to_string(),
                None => self.root_module_name.as_ref().unwrap().clone(),
            };
            err.span_note(id_sp,
                          &format!("maybe move this module `{0}` to its own directory \
                                     via `{0}/mod.rs`",
                                    this_module));
            if paths.path_exists {
                err.span_note(id_sp,
                              &format!("... or maybe `use` the module `{}` instead \
                                        of possibly redeclaring it",
                                       paths.name));
            }
            return Err(err);
        }

        match paths.result {
            Ok(succ) => Ok(succ),
            Err(err) => Err(self.span_fatal_help(id_sp, &err.err_msg, &err.help_msg)),
        }
    }

    /// Read a module from a source file.
    fn eval_src_mod(&mut self,
                    id: ast::Ident,
                    outer_attrs: &[ast::Attribute],
                    id_sp: Span)
                    -> PResult<'a, (ast::ItemKind, Vec<ast::Attribute> )> {
        let ModulePathSuccess { path, owns_directory } = self.submod_path(id,
                                                                          outer_attrs,
                                                                          id_sp)?;

        self.eval_src_mod_from_path(path,
                                    owns_directory,
                                    id.to_string(),
                                    id_sp)
    }

    fn eval_src_mod_from_path(&mut self,
                              path: PathBuf,
                              owns_directory: bool,
                              name: String,
                              id_sp: Span) -> PResult<'a, (ast::ItemKind, Vec<ast::Attribute> )> {
        let mut included_mod_stack = self.sess.included_mod_stack.borrow_mut();
        if let Some(i) = included_mod_stack.iter().position(|p| *p == path) {
            let mut err = String::from("circular modules: ");
            let len = included_mod_stack.len();
            for p in &included_mod_stack[i.. len] {
                err.push_str(&p.to_string_lossy());
                err.push_str(" -> ");
            }
            err.push_str(&path.to_string_lossy());
            return Err(self.span_fatal(id_sp, &err[..]));
        }
        included_mod_stack.push(path.clone());
        drop(included_mod_stack);

        let mut p0 = new_sub_parser_from_file(self.sess,
                                              self.cfg.clone(),
                                              &path,
                                              owns_directory,
                                              Some(name),
                                              id_sp);
        let mod_inner_lo = p0.span.lo;
        let mod_attrs = p0.parse_inner_attributes()?;
        let m0 = p0.parse_mod_items(&token::Eof, mod_inner_lo)?;
        self.sess.included_mod_stack.borrow_mut().pop();
        Ok((ast::ItemKind::Mod(m0), mod_attrs))
    }

    /// Parse a function declaration from a foreign module
    fn parse_item_foreign_fn(&mut self, vis: ast::Visibility, lo: BytePos,
                             attrs: Vec<Attribute>) -> PResult<'a, ForeignItem> {
        self.expect_keyword(keywords::Fn)?;

        let (ident, mut generics) = self.parse_fn_header()?;
        let decl = self.parse_fn_decl(true)?;
        generics.where_clause = self.parse_where_clause()?;
        let hi = self.span.hi;
        self.expect(&token::Semi)?;
        Ok(ast::ForeignItem {
            ident: ident,
            attrs: attrs,
            node: ForeignItemKind::Fn(decl, generics),
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, hi),
            vis: vis
        })
    }

    /// Parse a static item from a foreign module
    fn parse_item_foreign_static(&mut self, vis: ast::Visibility, lo: BytePos,
                                 attrs: Vec<Attribute>) -> PResult<'a, ForeignItem> {
        self.expect_keyword(keywords::Static)?;
        let mutbl = self.eat_keyword(keywords::Mut);

        let ident = self.parse_ident()?;
        self.expect(&token::Colon)?;
        let ty = self.parse_ty_sum()?;
        let hi = self.span.hi;
        self.expect(&token::Semi)?;
        Ok(ForeignItem {
            ident: ident,
            attrs: attrs,
            node: ForeignItemKind::Static(ty, mutbl),
            id: ast::DUMMY_NODE_ID,
            span: mk_sp(lo, hi),
            vis: vis
        })
    }

    /// Parse extern crate links
    ///
    /// # Examples
    ///
    /// extern crate foo;
    /// extern crate bar as foo;
    fn parse_item_extern_crate(&mut self,
                               lo: BytePos,
                               visibility: Visibility,
                               attrs: Vec<Attribute>)
                                -> PResult<'a, P<Item>> {

        let crate_name = self.parse_ident()?;
        let (maybe_path, ident) = if let Some(ident) = self.parse_rename()? {
            (Some(crate_name.name), ident)
        } else {
            (None, crate_name)
        };
        self.expect(&token::Semi)?;

        let last_span = self.last_span;
        Ok(self.mk_item(lo,
                        last_span.hi,
                        ident,
                        ItemKind::ExternCrate(maybe_path),
                        visibility,
                        attrs))
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
                              mut attrs: Vec<Attribute>)
                              -> PResult<'a, P<Item>> {
        self.expect(&token::OpenDelim(token::Brace))?;

        let abi = opt_abi.unwrap_or(Abi::C);

        attrs.extend(self.parse_inner_attributes()?);

        let mut foreign_items = vec![];
        while let Some(item) = self.parse_foreign_item()? {
            foreign_items.push(item);
        }
        self.expect(&token::CloseDelim(token::Brace))?;

        let last_span = self.last_span;
        let m = ast::ForeignMod {
            abi: abi,
            items: foreign_items
        };
        Ok(self.mk_item(lo,
                     last_span.hi,
                     keywords::Invalid.ident(),
                     ItemKind::ForeignMod(m),
                     visibility,
                     attrs))
    }

    /// Parse type Foo = Bar;
    fn parse_item_type(&mut self) -> PResult<'a, ItemInfo> {
        let ident = self.parse_ident()?;
        let mut tps = self.parse_generics()?;
        tps.where_clause = self.parse_where_clause()?;
        self.expect(&token::Eq)?;
        let ty = self.parse_ty_sum()?;
        self.expect(&token::Semi)?;
        Ok((ident, ItemKind::Ty(ty, tps), None))
    }

    /// Parse the part of an "enum" decl following the '{'
    fn parse_enum_def(&mut self, _generics: &ast::Generics) -> PResult<'a, EnumDef> {
        let mut variants = Vec::new();
        let mut all_nullary = true;
        let mut any_disr = None;
        while self.token != token::CloseDelim(token::Brace) {
            let variant_attrs = self.parse_outer_attributes()?;
            let vlo = self.span.lo;

            let struct_def;
            let mut disr_expr = None;
            let ident = self.parse_ident()?;
            if self.check(&token::OpenDelim(token::Brace)) {
                // Parse a struct variant.
                all_nullary = false;
                struct_def = VariantData::Struct(self.parse_record_struct_body()?,
                                                 ast::DUMMY_NODE_ID);
            } else if self.check(&token::OpenDelim(token::Paren)) {
                all_nullary = false;
                struct_def = VariantData::Tuple(self.parse_tuple_struct_body()?,
                                                ast::DUMMY_NODE_ID);
            } else if self.eat(&token::Eq) {
                disr_expr = Some(self.parse_expr()?);
                any_disr = disr_expr.as_ref().map(|expr| expr.span);
                struct_def = VariantData::Unit(ast::DUMMY_NODE_ID);
            } else {
                struct_def = VariantData::Unit(ast::DUMMY_NODE_ID);
            }

            let vr = ast::Variant_ {
                name: ident,
                attrs: variant_attrs,
                data: struct_def,
                disr_expr: disr_expr,
            };
            variants.push(spanned(vlo, self.last_span.hi, vr));

            if !self.eat(&token::Comma) { break; }
        }
        self.expect(&token::CloseDelim(token::Brace))?;
        match any_disr {
            Some(disr_span) if !all_nullary =>
                self.span_err(disr_span,
                    "discriminator values can only be used with a c-like enum"),
            _ => ()
        }

        Ok(ast::EnumDef { variants: variants })
    }

    /// Parse an "enum" declaration
    fn parse_item_enum(&mut self) -> PResult<'a, ItemInfo> {
        let id = self.parse_ident()?;
        let mut generics = self.parse_generics()?;
        generics.where_clause = self.parse_where_clause()?;
        self.expect(&token::OpenDelim(token::Brace))?;

        let enum_definition = self.parse_enum_def(&generics)?;
        Ok((id, ItemKind::Enum(enum_definition, generics), None))
    }

    /// Parses a string as an ABI spec on an extern type or module. Consumes
    /// the `extern` keyword, if one is found.
    fn parse_opt_abi(&mut self) -> PResult<'a, Option<abi::Abi>> {
        match self.token {
            token::Literal(token::Str_(s), suf) | token::Literal(token::StrRaw(s, _), suf) => {
                let sp = self.span;
                self.expect_no_suffix(sp, "ABI spec", suf);
                self.bump();
                match abi::lookup(&s.as_str()) {
                    Some(abi) => Ok(Some(abi)),
                    None => {
                        let last_span = self.last_span;
                        self.span_err(
                            last_span,
                            &format!("invalid ABI: expected one of [{}], \
                                     found `{}`",
                                    abi::all_names().join(", "),
                                    s));
                        Ok(None)
                    }
                }
            }

            _ => Ok(None),
        }
    }

    /// Parse one of the items allowed by the flags.
    /// NB: this function no longer parses the items inside an
    /// extern crate.
    fn parse_item_(&mut self, attrs: Vec<Attribute>,
                   macros_allowed: bool, attributes_allowed: bool) -> PResult<'a, Option<P<Item>>> {
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
                item.attrs.extend(attrs);
                return Ok(Some(P(item)));
            }
            None => {}
        }

        let lo = self.span.lo;

        let visibility = self.parse_visibility(true)?;

        if self.eat_keyword(keywords::Use) {
            // USE ITEM
            let item_ = ItemKind::Use(self.parse_view_path()?);
            self.expect(&token::Semi)?;

            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    keywords::Invalid.ident(),
                                    item_,
                                    visibility,
                                    attrs);
            return Ok(Some(item));
        }

        if self.eat_keyword(keywords::Extern) {
            if self.eat_keyword(keywords::Crate) {
                return Ok(Some(self.parse_item_extern_crate(lo, visibility, attrs)?));
            }

            let opt_abi = self.parse_opt_abi()?;

            if self.eat_keyword(keywords::Fn) {
                // EXTERN FUNCTION ITEM
                let abi = opt_abi.unwrap_or(Abi::C);
                let (ident, item_, extra_attrs) =
                    self.parse_item_fn(Unsafety::Normal, Constness::NotConst, abi)?;
                let last_span = self.last_span;
                let item = self.mk_item(lo,
                                        last_span.hi,
                                        ident,
                                        item_,
                                        visibility,
                                        maybe_append(attrs, extra_attrs));
                return Ok(Some(item));
            } else if self.check(&token::OpenDelim(token::Brace)) {
                return Ok(Some(self.parse_item_foreign_mod(lo, opt_abi, visibility, attrs)?));
            }

            self.unexpected()?;
        }

        if self.eat_keyword(keywords::Static) {
            // STATIC ITEM
            let m = if self.eat_keyword(keywords::Mut) {
                Mutability::Mutable
            } else {
                Mutability::Immutable
            };
            let (ident, item_, extra_attrs) = self.parse_item_const(Some(m))?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.eat_keyword(keywords::Const) {
            if self.check_keyword(keywords::Fn)
                || (self.check_keyword(keywords::Unsafe)
                    && self.look_ahead(1, |t| t.is_keyword(keywords::Fn))) {
                // CONST FUNCTION ITEM
                let unsafety = if self.eat_keyword(keywords::Unsafe) {
                    Unsafety::Unsafe
                } else {
                    Unsafety::Normal
                };
                self.bump();
                let (ident, item_, extra_attrs) =
                    self.parse_item_fn(unsafety, Constness::Const, Abi::Rust)?;
                let last_span = self.last_span;
                let item = self.mk_item(lo,
                                        last_span.hi,
                                        ident,
                                        item_,
                                        visibility,
                                        maybe_append(attrs, extra_attrs));
                return Ok(Some(item));
            }

            // CONST ITEM
            if self.eat_keyword(keywords::Mut) {
                let last_span = self.last_span;
                self.diagnostic().struct_span_err(last_span, "const globals cannot be mutable")
                                 .help("did you mean to declare a static?")
                                 .emit();
            }
            let (ident, item_, extra_attrs) = self.parse_item_const(None)?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.check_keyword(keywords::Unsafe) &&
            self.look_ahead(1, |t| t.is_keyword(keywords::Trait))
        {
            // UNSAFE TRAIT ITEM
            self.expect_keyword(keywords::Unsafe)?;
            self.expect_keyword(keywords::Trait)?;
            let (ident, item_, extra_attrs) =
                self.parse_item_trait(ast::Unsafety::Unsafe)?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.check_keyword(keywords::Unsafe) &&
            self.look_ahead(1, |t| t.is_keyword(keywords::Impl))
        {
            // IMPL ITEM
            self.expect_keyword(keywords::Unsafe)?;
            self.expect_keyword(keywords::Impl)?;
            let (ident, item_, extra_attrs) = self.parse_item_impl(ast::Unsafety::Unsafe)?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.check_keyword(keywords::Fn) {
            // FUNCTION ITEM
            self.bump();
            let (ident, item_, extra_attrs) =
                self.parse_item_fn(Unsafety::Normal, Constness::NotConst, Abi::Rust)?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.check_keyword(keywords::Unsafe)
            && self.look_ahead(1, |t| *t != token::OpenDelim(token::Brace)) {
            // UNSAFE FUNCTION ITEM
            self.bump();
            let abi = if self.eat_keyword(keywords::Extern) {
                self.parse_opt_abi()?.unwrap_or(Abi::C)
            } else {
                Abi::Rust
            };
            self.expect_keyword(keywords::Fn)?;
            let (ident, item_, extra_attrs) =
                self.parse_item_fn(Unsafety::Unsafe, Constness::NotConst, abi)?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.eat_keyword(keywords::Mod) {
            // MODULE ITEM
            let (ident, item_, extra_attrs) =
                self.parse_item_mod(&attrs[..])?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.eat_keyword(keywords::Type) {
            // TYPE ITEM
            let (ident, item_, extra_attrs) = self.parse_item_type()?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.eat_keyword(keywords::Enum) {
            // ENUM ITEM
            let (ident, item_, extra_attrs) = self.parse_item_enum()?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.eat_keyword(keywords::Trait) {
            // TRAIT ITEM
            let (ident, item_, extra_attrs) =
                self.parse_item_trait(ast::Unsafety::Normal)?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.eat_keyword(keywords::Impl) {
            // IMPL ITEM
            let (ident, item_, extra_attrs) = self.parse_item_impl(ast::Unsafety::Normal)?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        if self.eat_keyword(keywords::Struct) {
            // STRUCT ITEM
            let (ident, item_, extra_attrs) = self.parse_item_struct()?;
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    ident,
                                    item_,
                                    visibility,
                                    maybe_append(attrs, extra_attrs));
            return Ok(Some(item));
        }
        self.parse_macro_use_or_failure(attrs,macros_allowed,attributes_allowed,lo,visibility)
    }

    /// Parse a foreign item.
    fn parse_foreign_item(&mut self) -> PResult<'a, Option<ForeignItem>> {
        let attrs = self.parse_outer_attributes()?;
        let lo = self.span.lo;
        let visibility = self.parse_visibility(true)?;

        if self.check_keyword(keywords::Static) {
            // FOREIGN STATIC ITEM
            return Ok(Some(self.parse_item_foreign_static(visibility, lo, attrs)?));
        }
        if self.check_keyword(keywords::Fn) {
            // FOREIGN FUNCTION ITEM
            return Ok(Some(self.parse_item_foreign_fn(visibility, lo, attrs)?));
        }

        // FIXME #5668: this will occur for a macro invocation:
        match self.parse_macro_use_or_failure(attrs, true, false, lo, visibility)? {
            Some(item) => {
                return Err(self.span_fatal(item.span, "macros cannot expand to foreign items"));
            }
            None => Ok(None)
        }
    }

    /// This is the fall-through for parsing items.
    fn parse_macro_use_or_failure(
        &mut self,
        attrs: Vec<Attribute> ,
        macros_allowed: bool,
        attributes_allowed: bool,
        lo: BytePos,
        visibility: Visibility
    ) -> PResult<'a, Option<P<Item>>> {
        if macros_allowed && !self.token.is_any_keyword()
                && self.look_ahead(1, |t| *t == token::Not)
                && (self.look_ahead(2, |t| t.is_ident())
                    || self.look_ahead(2, |t| *t == token::OpenDelim(token::Paren))
                    || self.look_ahead(2, |t| *t == token::OpenDelim(token::Brace))) {
            // MACRO INVOCATION ITEM

            let last_span = self.last_span;
            self.complain_if_pub_macro(&visibility, last_span);

            let mac_lo = self.span.lo;

            // item macro.
            let pth = self.parse_ident_into_path()?;
            self.expect(&token::Not)?;

            // a 'special' identifier (like what `macro_rules!` uses)
            // is optional. We should eventually unify invoc syntax
            // and remove this.
            let id = if self.token.is_ident() {
                self.parse_ident()?
            } else {
                keywords::Invalid.ident() // no special identifier
            };
            // eat a matched-delimiter token tree:
            let delim = self.expect_open_delim()?;
            let tts = self.parse_seq_to_end(&token::CloseDelim(delim),
                                            SeqSep::none(),
                                            |p| p.parse_token_tree())?;
            // single-variant-enum... :
            let m = Mac_ { path: pth, tts: tts };
            let m: ast::Mac = codemap::Spanned { node: m,
                                                 span: mk_sp(mac_lo,
                                                             self.last_span.hi) };

            if delim != token::Brace {
                if !self.eat(&token::Semi) {
                    let last_span = self.last_span;
                    self.span_err(last_span,
                                  "macros that expand to items must either \
                                   be surrounded with braces or followed by \
                                   a semicolon");
                }
            }

            let item_ = ItemKind::Mac(m);
            let last_span = self.last_span;
            let item = self.mk_item(lo,
                                    last_span.hi,
                                    id,
                                    item_,
                                    visibility,
                                    attrs);
            return Ok(Some(item));
        }

        // FAILURE TO PARSE ITEM
        match visibility {
            Visibility::Inherited => {}
            _ => {
                let last_span = self.last_span;
                return Err(self.span_fatal(last_span, "unmatched visibility `pub`"));
            }
        }

        if !attributes_allowed && !attrs.is_empty() {
            self.expected_item_err(&attrs);
        }
        Ok(None)
    }

    pub fn parse_item(&mut self) -> PResult<'a, Option<P<Item>>> {
        let attrs = self.parse_outer_attributes()?;
        self.parse_item_(attrs, true, false)
    }

    fn parse_path_list_items(&mut self) -> PResult<'a, Vec<ast::PathListItem>> {
        self.parse_unspanned_seq(&token::OpenDelim(token::Brace),
                                 &token::CloseDelim(token::Brace),
                                 SeqSep::trailing_allowed(token::Comma), |this| {
            let lo = this.span.lo;
            let node = if this.eat_keyword(keywords::SelfValue) {
                let rename = this.parse_rename()?;
                ast::PathListItemKind::Mod { id: ast::DUMMY_NODE_ID, rename: rename }
            } else {
                let ident = this.parse_ident()?;
                let rename = this.parse_rename()?;
                ast::PathListItemKind::Ident { name: ident, rename: rename, id: ast::DUMMY_NODE_ID }
            };
            let hi = this.last_span.hi;
            Ok(spanned(lo, hi, node))
        })
    }

    /// `::{` or `::*`
    fn is_import_coupler(&mut self) -> bool {
        self.check(&token::ModSep) &&
            self.look_ahead(1, |t| *t == token::OpenDelim(token::Brace) ||
                                   *t == token::BinOp(token::Star))
    }

    /// Matches ViewPath:
    /// MOD_SEP? non_global_path
    /// MOD_SEP? non_global_path as IDENT
    /// MOD_SEP? non_global_path MOD_SEP STAR
    /// MOD_SEP? non_global_path MOD_SEP LBRACE item_seq RBRACE
    /// MOD_SEP? LBRACE item_seq RBRACE
    fn parse_view_path(&mut self) -> PResult<'a, P<ViewPath>> {
        let lo = self.span.lo;
        if self.check(&token::OpenDelim(token::Brace)) || self.is_import_coupler() {
            // `{foo, bar}` or `::{foo, bar}`
            let prefix = ast::Path {
                global: self.eat(&token::ModSep),
                segments: Vec::new(),
                span: mk_sp(lo, self.span.hi),
            };
            let items = self.parse_path_list_items()?;
            Ok(P(spanned(lo, self.span.hi, ViewPathList(prefix, items))))
        } else {
            let prefix = self.parse_path(PathStyle::Mod)?;
            if self.is_import_coupler() {
                // `foo::bar::{a, b}` or `foo::bar::*`
                self.bump();
                if self.check(&token::BinOp(token::Star)) {
                    self.bump();
                    Ok(P(spanned(lo, self.span.hi, ViewPathGlob(prefix))))
                } else {
                    let items = self.parse_path_list_items()?;
                    Ok(P(spanned(lo, self.span.hi, ViewPathList(prefix, items))))
                }
            } else {
                // `foo::bar` or `foo::bar as baz`
                let rename = self.parse_rename()?.
                                  unwrap_or(prefix.segments.last().unwrap().identifier);
                Ok(P(spanned(lo, self.last_span.hi, ViewPathSimple(rename, prefix))))
            }
        }
    }

    fn parse_rename(&mut self) -> PResult<'a, Option<Ident>> {
        if self.eat_keyword(keywords::As) {
            self.parse_ident().map(Some)
        } else {
            Ok(None)
        }
    }

    /// Parses a source module as a crate. This is the main
    /// entry point for the parser.
    pub fn parse_crate_mod(&mut self) -> PResult<'a, Crate> {
        let lo = self.span.lo;
        Ok(ast::Crate {
            attrs: self.parse_inner_attributes()?,
            module: self.parse_mod_items(&token::Eof, lo)?,
            config: self.cfg.clone(),
            span: mk_sp(lo, self.span.lo),
            exported_macros: Vec::new(),
        })
    }

    pub fn parse_optional_str(&mut self)
                              -> Option<(InternedString,
                                         ast::StrStyle,
                                         Option<ast::Name>)> {
        let ret = match self.token {
            token::Literal(token::Str_(s), suf) => {
                let s = self.id_to_interned_str(ast::Ident::with_empty_ctxt(s));
                (s, ast::StrStyle::Cooked, suf)
            }
            token::Literal(token::StrRaw(s, n), suf) => {
                let s = self.id_to_interned_str(ast::Ident::with_empty_ctxt(s));
                (s, ast::StrStyle::Raw(n), suf)
            }
            _ => return None
        };
        self.bump();
        Some(ret)
    }

    pub fn parse_str(&mut self) -> PResult<'a, (InternedString, StrStyle)> {
        match self.parse_optional_str() {
            Some((s, style, suf)) => {
                let sp = self.last_span;
                self.expect_no_suffix(sp, "string literal", suf);
                Ok((s, style))
            }
            _ =>  Err(self.fatal("expected string literal"))
        }
    }
}
