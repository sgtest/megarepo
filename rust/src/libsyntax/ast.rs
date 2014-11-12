// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// The Rust abstract syntax tree.

use codemap::{Span, Spanned, DUMMY_SP, ExpnId, respan};
use abi::Abi;
use ast_util;
use owned_slice::OwnedSlice;
use parse::token::{InternedString, str_to_ident};
use parse::token;
use ptr::P;

use std::fmt;
use std::num::Zero;
use std::fmt::Show;
use std::rc::Rc;
use serialize::{Encodable, Decodable, Encoder, Decoder};

// FIXME #6993: in librustc, uses of "ident" should be replaced
// by just "Name".

/// An identifier contains a Name (index into the interner
/// table) and a SyntaxContext to track renaming and
/// macro expansion per Flatt et al., "Macros
/// That Work Together"
#[deriving(Clone, Hash, PartialOrd, Eq, Ord)]
pub struct Ident {
    pub name: Name,
    pub ctxt: SyntaxContext
}

impl Ident {
    /// Construct an identifier with the given name and an empty context:
    pub fn new(name: Name) -> Ident { Ident {name: name, ctxt: EMPTY_CTXT}}

    pub fn as_str<'a>(&'a self) -> &'a str {
        self.name.as_str()
    }

    pub fn encode_with_hygiene(&self) -> String {
        format!("\x00name_{:u},ctxt_{:u}\x00",
                self.name.uint(),
                self.ctxt)
    }
}

impl Show for Ident {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}#{}", self.name, self.ctxt)
    }
}

impl Show for Name {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Name(nm) = *self;
        write!(f, "\"{}\"({})", token::get_name(*self).get(), nm)
    }
}

impl PartialEq for Ident {
    fn eq(&self, other: &Ident) -> bool {
        if self.ctxt == other.ctxt {
            self.name == other.name
        } else {
            // IF YOU SEE ONE OF THESE FAILS: it means that you're comparing
            // idents that have different contexts. You can't fix this without
            // knowing whether the comparison should be hygienic or non-hygienic.
            // if it should be non-hygienic (most things are), just compare the
            // 'name' fields of the idents. Or, even better, replace the idents
            // with Name's.
            //
            // On the other hand, if the comparison does need to be hygienic,
            // one example and its non-hygienic counterpart would be:
            //      syntax::parse::token::Token::mtwt_eq
            //      syntax::ext::tt::macro_parser::token_name_eq
            panic!("not allowed to compare these idents: {}, {}. \
                   Probably related to issue \\#6993", self, other);
        }
    }
    fn ne(&self, other: &Ident) -> bool {
        ! self.eq(other)
    }
}

/// A SyntaxContext represents a chain of macro-expandings
/// and renamings. Each macro expansion corresponds to
/// a fresh uint

// I'm representing this syntax context as an index into
// a table, in order to work around a compiler bug
// that's causing unreleased memory to cause core dumps
// and also perhaps to save some work in destructor checks.
// the special uint '0' will be used to indicate an empty
// syntax context.

// this uint is a reference to a table stored in thread-local
// storage.
pub type SyntaxContext = u32;
pub const EMPTY_CTXT : SyntaxContext = 0;
pub const ILLEGAL_CTXT : SyntaxContext = 1;

/// A name is a part of an identifier, representing a string or gensym. It's
/// the result of interning.
#[deriving(Eq, Ord, PartialEq, PartialOrd, Hash, Encodable, Decodable, Clone)]
pub struct Name(pub u32);

impl Name {
    pub fn as_str<'a>(&'a self) -> &'a str {
        unsafe {
            // FIXME #12938: can't use copy_lifetime since &str isn't a &T
            ::std::mem::transmute::<&str,&str>(token::get_name(*self).get())
        }
    }

    pub fn uint(&self) -> uint {
        let Name(nm) = *self;
        nm as uint
    }

    pub fn ident(&self) -> Ident {
        Ident { name: *self, ctxt: 0 }
    }
}

/// A mark represents a unique id associated with a macro expansion
pub type Mrk = u32;

impl<S: Encoder<E>, E> Encodable<S, E> for Ident {
    fn encode(&self, s: &mut S) -> Result<(), E> {
        s.emit_str(token::get_ident(*self).get())
    }
}

impl<D:Decoder<E>, E> Decodable<D, E> for Ident {
    fn decode(d: &mut D) -> Result<Ident, E> {
        Ok(str_to_ident(try!(d.read_str()).as_slice()))
    }
}

/// Function name (not all functions have names)
pub type FnIdent = Option<Ident>;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Lifetime {
    pub id: NodeId,
    pub span: Span,
    pub name: Name
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct LifetimeDef {
    pub lifetime: Lifetime,
    pub bounds: Vec<Lifetime>
}

/// A "Path" is essentially Rust's notion of a name; for instance:
/// std::cmp::PartialEq  .  It's represented as a sequence of identifiers,
/// along with a bunch of supporting information.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Path {
    pub span: Span,
    /// A `::foo` path, is relative to the crate root rather than current
    /// module (like paths in an import).
    pub global: bool,
    /// The segments in the path: the things separated by `::`.
    pub segments: Vec<PathSegment>,
}

/// A segment of a path: an identifier, an optional lifetime, and a set of
/// types.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct PathSegment {
    /// The identifier portion of this path segment.
    pub identifier: Ident,

    /// Type/lifetime parameters attached to this path. They come in
    /// two flavors: `Path<A,B,C>` and `Path(A,B) -> C`. Note that
    /// this is more than just simple syntactic sugar; the use of
    /// parens affects the region binding rules, so we preserve the
    /// distinction.
    pub parameters: PathParameters,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum PathParameters {
    AngleBracketedParameters(AngleBracketedParameterData),
    ParenthesizedParameters(ParenthesizedParameterData),
}

impl PathParameters {
    pub fn none() -> PathParameters {
        AngleBracketedParameters(AngleBracketedParameterData {
            lifetimes: Vec::new(),
            types: OwnedSlice::empty(),
        })
    }

    pub fn is_empty(&self) -> bool {
        match *self {
            AngleBracketedParameters(ref data) => data.is_empty(),

            // Even if the user supplied no types, something like
            // `X()` is equivalent to `X<(),()>`.
            ParenthesizedParameters(..) => false,
        }
    }

    pub fn has_lifetimes(&self) -> bool {
        match *self {
            AngleBracketedParameters(ref data) => !data.lifetimes.is_empty(),
            ParenthesizedParameters(_) => false,
        }
    }

    pub fn has_types(&self) -> bool {
        match *self {
            AngleBracketedParameters(ref data) => !data.types.is_empty(),
            ParenthesizedParameters(..) => true,
        }
    }

    pub fn types(&self) -> Vec<&P<Ty>> {
        /*!
         * Returns the types that the user wrote. Note that these do not
         * necessarily map to the type parameters in the parenthesized case.
         */
        match *self {
            AngleBracketedParameters(ref data) => {
                data.types.iter().collect()
            }
            ParenthesizedParameters(ref data) => {
                data.inputs.iter()
                    .chain(data.output.iter())
                    .collect()
            }
        }
    }

    pub fn lifetimes(&self) -> Vec<&Lifetime> {
        match *self {
            AngleBracketedParameters(ref data) => {
                data.lifetimes.iter().collect()
            }
            ParenthesizedParameters(_) => {
                Vec::new()
            }
        }
    }
}

/// A path like `Foo<'a, T>`
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct AngleBracketedParameterData {
    /// The lifetime parameters for this path segment.
    pub lifetimes: Vec<Lifetime>,
    /// The type parameters for this path segment, if present.
    pub types: OwnedSlice<P<Ty>>,
}

impl AngleBracketedParameterData {
    fn is_empty(&self) -> bool {
        self.lifetimes.is_empty() && self.types.is_empty()
    }
}

/// A path like `Foo(A,B) -> C`
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct ParenthesizedParameterData {
    /// `(A,B)`
    pub inputs: Vec<P<Ty>>,

    /// `C`
    pub output: Option<P<Ty>>,
}

pub type CrateNum = u32;

pub type NodeId = u32;

#[deriving(Clone, Eq, Ord, PartialOrd, PartialEq, Encodable, Decodable, Hash, Show)]
pub struct DefId {
    pub krate: CrateNum,
    pub node: NodeId,
}

/// Item definitions in the currently-compiled crate would have the CrateNum
/// LOCAL_CRATE in their DefId.
pub const LOCAL_CRATE: CrateNum = 0;
pub const CRATE_NODE_ID: NodeId = 0;

/// When parsing and doing expansions, we initially give all AST nodes this AST
/// node value. Then later, in the renumber pass, we renumber them to have
/// small, positive ids.
pub const DUMMY_NODE_ID: NodeId = -1;

/// The AST represents all type param bounds as types.
/// typeck::collect::compute_bounds matches these against
/// the "special" built-in traits (see middle::lang_items) and
/// detects Copy, Send and Sync.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum TyParamBound {
    TraitTyParamBound(PolyTraitRef),
    RegionTyParamBound(Lifetime)
}

pub type TyParamBounds = OwnedSlice<TyParamBound>;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct TyParam {
    pub ident: Ident,
    pub id: NodeId,
    pub bounds: TyParamBounds,
    pub unbound: Option<TraitRef>,
    pub default: Option<P<Ty>>,
    pub span: Span
}

/// Represents lifetimes and type parameters attached to a declaration
/// of a function, enum, trait, etc.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Generics {
    pub lifetimes: Vec<LifetimeDef>,
    pub ty_params: OwnedSlice<TyParam>,
    pub where_clause: WhereClause,
}

impl Generics {
    pub fn is_parameterized(&self) -> bool {
        self.lifetimes.len() + self.ty_params.len() > 0
    }
    pub fn is_lt_parameterized(&self) -> bool {
        self.lifetimes.len() > 0
    }
    pub fn is_type_parameterized(&self) -> bool {
        self.ty_params.len() > 0
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct WhereClause {
    pub id: NodeId,
    pub predicates: Vec<WherePredicate>,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct WherePredicate {
    pub id: NodeId,
    pub span: Span,
    pub ident: Ident,
    pub bounds: OwnedSlice<TyParamBound>,
}

/// The set of MetaItems that define the compilation environment of the crate,
/// used to drive conditional compilation
pub type CrateConfig = Vec<P<MetaItem>> ;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Crate {
    pub module: Mod,
    pub attrs: Vec<Attribute>,
    pub config: CrateConfig,
    pub span: Span,
    pub exported_macros: Vec<P<Item>>
}

pub type MetaItem = Spanned<MetaItem_>;

#[deriving(Clone, Eq, Encodable, Decodable, Hash, Show)]
pub enum MetaItem_ {
    MetaWord(InternedString),
    MetaList(InternedString, Vec<P<MetaItem>>),
    MetaNameValue(InternedString, Lit),
}

// can't be derived because the MetaList requires an unordered comparison
impl PartialEq for MetaItem_ {
    fn eq(&self, other: &MetaItem_) -> bool {
        match *self {
            MetaWord(ref ns) => match *other {
                MetaWord(ref no) => (*ns) == (*no),
                _ => false
            },
            MetaNameValue(ref ns, ref vs) => match *other {
                MetaNameValue(ref no, ref vo) => {
                    (*ns) == (*no) && vs.node == vo.node
                }
                _ => false
            },
            MetaList(ref ns, ref miss) => match *other {
                MetaList(ref no, ref miso) => {
                    ns == no &&
                        miss.iter().all(|mi| miso.iter().any(|x| x.node == mi.node))
                }
                _ => false
            }
        }
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Block {
    pub view_items: Vec<ViewItem>,
    pub stmts: Vec<P<Stmt>>,
    pub expr: Option<P<Expr>>,
    pub id: NodeId,
    pub rules: BlockCheckMode,
    pub span: Span,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Pat {
    pub id: NodeId,
    pub node: Pat_,
    pub span: Span,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct FieldPat {
    pub ident: Ident,
    pub pat: P<Pat>,
    pub is_shorthand: bool,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum BindingMode {
    BindByRef(Mutability),
    BindByValue(Mutability),
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum PatWildKind {
    /// Represents the wildcard pattern `_`
    PatWildSingle,

    /// Represents the wildcard pattern `..`
    PatWildMulti,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Pat_ {
    /// Represents a wildcard pattern (either `_` or `..`)
    PatWild(PatWildKind),

    /// A PatIdent may either be a new bound variable,
    /// or a nullary enum (in which case the third field
    /// is None).
    /// In the nullary enum case, the parser can't determine
    /// which it is. The resolver determines this, and
    /// records this pattern's NodeId in an auxiliary
    /// set (of "PatIdents that refer to nullary enums")
    PatIdent(BindingMode, SpannedIdent, Option<P<Pat>>),

    /// "None" means a * pattern where we don't bind the fields to names.
    PatEnum(Path, Option<Vec<P<Pat>>>),

    PatStruct(Path, Vec<Spanned<FieldPat>>, bool),
    PatTup(Vec<P<Pat>>),
    PatBox(P<Pat>),
    PatRegion(P<Pat>), // reference pattern
    PatLit(P<Expr>),
    PatRange(P<Expr>, P<Expr>),
    /// [a, b, ..i, y, z] is represented as:
    ///     PatVec(box [a, b], Some(i), box [y, z])
    PatVec(Vec<P<Pat>>, Option<P<Pat>>, Vec<P<Pat>>),
    PatMac(Mac),
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Mutability {
    MutMutable,
    MutImmutable,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum BinOp {
    BiAdd,
    BiSub,
    BiMul,
    BiDiv,
    BiRem,
    BiAnd,
    BiOr,
    BiBitXor,
    BiBitAnd,
    BiBitOr,
    BiShl,
    BiShr,
    BiEq,
    BiLt,
    BiLe,
    BiNe,
    BiGe,
    BiGt,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum UnOp {
    UnUniq,
    UnDeref,
    UnNot,
    UnNeg
}

pub type Stmt = Spanned<Stmt_>;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Stmt_ {
    /// Could be an item or a local (let) binding:
    StmtDecl(P<Decl>, NodeId),

    /// Expr without trailing semi-colon (must have unit type):
    StmtExpr(P<Expr>, NodeId),

    /// Expr with trailing semi-colon (may have any type):
    StmtSemi(P<Expr>, NodeId),

    /// bool: is there a trailing semi-colon?
    StmtMac(Mac, bool),
}

/// Where a local declaration came from: either a true `let ... =
/// ...;`, or one desugared from the pattern of a for loop.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum LocalSource {
    LocalLet,
    LocalFor,
}

// FIXME (pending discussion of #1697, #2178...): local should really be
// a refinement on pat.
/// Local represents a `let` statement, e.g., `let <pat>:<ty> = <expr>;`
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Local {
    pub ty: P<Ty>,
    pub pat: P<Pat>,
    pub init: Option<P<Expr>>,
    pub id: NodeId,
    pub span: Span,
    pub source: LocalSource,
}

pub type Decl = Spanned<Decl_>;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Decl_ {
    /// A local (let) binding:
    DeclLocal(P<Local>),
    /// An item binding:
    DeclItem(P<Item>),
}

/// represents one arm of a 'match'
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Arm {
    pub attrs: Vec<Attribute>,
    pub pats: Vec<P<Pat>>,
    pub guard: Option<P<Expr>>,
    pub body: P<Expr>,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Field {
    pub ident: SpannedIdent,
    pub expr: P<Expr>,
    pub span: Span,
}

pub type SpannedIdent = Spanned<Ident>;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum BlockCheckMode {
    DefaultBlock,
    UnsafeBlock(UnsafeSource),
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum UnsafeSource {
    CompilerGenerated,
    UserProvided,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Expr {
    pub id: NodeId,
    pub node: Expr_,
    pub span: Span,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Expr_ {
    /// First expr is the place; second expr is the value.
    ExprBox(P<Expr>, P<Expr>),
    ExprVec(Vec<P<Expr>>),
    ExprCall(P<Expr>, Vec<P<Expr>>),
    ExprMethodCall(SpannedIdent, Vec<P<Ty>>, Vec<P<Expr>>),
    ExprTup(Vec<P<Expr>>),
    ExprBinary(BinOp, P<Expr>, P<Expr>),
    ExprUnary(UnOp, P<Expr>),
    ExprLit(P<Lit>),
    ExprCast(P<Expr>, P<Ty>),
    ExprIf(P<Expr>, P<Block>, Option<P<Expr>>),
    ExprIfLet(P<Pat>, P<Expr>, P<Block>, Option<P<Expr>>),
    // FIXME #6993: change to Option<Name> ... or not, if these are hygienic.
    ExprWhile(P<Expr>, P<Block>, Option<Ident>),
    // FIXME #6993: change to Option<Name> ... or not, if these are hygienic.
    ExprWhileLet(P<Pat>, P<Expr>, P<Block>, Option<Ident>),
    // FIXME #6993: change to Option<Name> ... or not, if these are hygienic.
    ExprForLoop(P<Pat>, P<Expr>, P<Block>, Option<Ident>),
    // Conditionless loop (can be exited with break, cont, or ret)
    // FIXME #6993: change to Option<Name> ... or not, if these are hygienic.
    ExprLoop(P<Block>, Option<Ident>),
    ExprMatch(P<Expr>, Vec<Arm>, MatchSource),
    ExprFnBlock(CaptureClause, P<FnDecl>, P<Block>),
    ExprProc(P<FnDecl>, P<Block>),
    ExprUnboxedFn(CaptureClause, UnboxedClosureKind, P<FnDecl>, P<Block>),
    ExprBlock(P<Block>),

    ExprAssign(P<Expr>, P<Expr>),
    ExprAssignOp(BinOp, P<Expr>, P<Expr>),
    ExprField(P<Expr>, SpannedIdent, Vec<P<Ty>>),
    ExprTupField(P<Expr>, Spanned<uint>, Vec<P<Ty>>),
    ExprIndex(P<Expr>, P<Expr>),
    ExprSlice(P<Expr>, Option<P<Expr>>, Option<P<Expr>>, Mutability),

    /// Variable reference, possibly containing `::` and/or
    /// type parameters, e.g. foo::bar::<baz>
    ExprPath(Path),

    ExprAddrOf(Mutability, P<Expr>),
    ExprBreak(Option<Ident>),
    ExprAgain(Option<Ident>),
    ExprRet(Option<P<Expr>>),

    ExprInlineAsm(InlineAsm),

    ExprMac(Mac),

    /// A struct literal expression.
    ExprStruct(Path, Vec<Field>, Option<P<Expr>> /* base */),

    /// A vector literal constructed from one repeated element.
    ExprRepeat(P<Expr> /* element */, P<Expr> /* count */),

    /// No-op: used solely so we can pretty-print faithfully
    ExprParen(P<Expr>)
}

/// A "qualified path":
///
///     <Vec<T> as SomeTrait>::SomeAssociatedItem
///      ^~~~~     ^~~~~~~~~   ^~~~~~~~~~~~~~~~~~
///      for_type  trait_name  item_name
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct QPath {
    pub for_type: P<Ty>,
    pub trait_name: Path,
    pub item_name: Ident,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum MatchSource {
    MatchNormal,
    MatchIfLetDesugar,
    MatchWhileLetDesugar,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum CaptureClause {
    CaptureByValue,
    CaptureByRef,
}

/// A delimited sequence of token trees
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Delimited {
    /// The type of delimiter
    pub delim: token::DelimToken,
    /// The span covering the opening delimiter
    pub open_span: Span,
    /// The delimited sequence of token trees
    pub tts: Vec<TokenTree>,
    /// The span covering the closing delimiter
    pub close_span: Span,
}

impl Delimited {
    /// Returns the opening delimiter as a token.
    pub fn open_token(&self) -> token::Token {
        token::OpenDelim(self.delim)
    }

    /// Returns the closing delimiter as a token.
    pub fn close_token(&self) -> token::Token {
        token::CloseDelim(self.delim)
    }

    /// Returns the opening delimiter as a token tree.
    pub fn open_tt(&self) -> TokenTree {
        TtToken(self.open_span, self.open_token())
    }

    /// Returns the closing delimiter as a token tree.
    pub fn close_tt(&self) -> TokenTree {
        TtToken(self.close_span, self.close_token())
    }
}

/// A sequence of token treesee
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct SequenceRepetition {
    /// The sequence of token trees
    pub tts: Vec<TokenTree>,
    /// The optional separator
    pub separator: Option<token::Token>,
    /// Whether the sequence can be repeated zero (*), or one or more times (+)
    pub op: KleeneOp,
    /// The number of `MatchNt`s that appear in the sequence (and subsequences)
    pub num_captures: uint,
}

/// A Kleene-style [repetition operator](http://en.wikipedia.org/wiki/Kleene_star)
/// for token sequences.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum KleeneOp {
    ZeroOrMore,
    OneOrMore,
}

/// When the main rust parser encounters a syntax-extension invocation, it
/// parses the arguments to the invocation as a token-tree. This is a very
/// loose structure, such that all sorts of different AST-fragments can
/// be passed to syntax extensions using a uniform type.
///
/// If the syntax extension is an MBE macro, it will attempt to match its
/// LHS token tree against the provided token tree, and if it finds a
/// match, will transcribe the RHS token tree, splicing in any captured
/// macro_parser::matched_nonterminals into the `SubstNt`s it finds.
///
/// The RHS of an MBE macro is the only place `SubstNt`s are substituted.
/// Nothing special happens to misnamed or misplaced `SubstNt`s.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
#[doc="For macro invocations; parsing is delegated to the macro"]
pub enum TokenTree {
    /// A single token
    TtToken(Span, token::Token),
    /// A delimited sequence of token trees
    TtDelimited(Span, Rc<Delimited>),

    // This only makes sense in MBE macros.

    /// A kleene-style repetition sequence with a span
    // FIXME(eddyb) #12938 Use DST.
    TtSequence(Span, Rc<SequenceRepetition>),
}

impl TokenTree {
    pub fn len(&self) -> uint {
        match *self {
            TtToken(_, token::DocComment(_)) => 2,
            TtToken(_, token::SubstNt(..)) => 2,
            TtToken(_, token::MatchNt(..)) => 3,
            TtDelimited(_, ref delimed) => {
                delimed.tts.len() + 2
            }
            TtSequence(_, ref seq) => {
                seq.tts.len()
            }
            TtToken(..) => 0
        }
    }

    pub fn get_tt(&self, index: uint) -> TokenTree {
        match (self, index) {
            (&TtToken(sp, token::DocComment(_)), 0) => {
                TtToken(sp, token::Pound)
            }
            (&TtToken(sp, token::DocComment(name)), 1) => {
                let doc = MetaNameValue(token::intern_and_get_ident("doc"),
                                        respan(sp, LitStr(token::get_name(name), CookedStr)));
                let doc = token::NtMeta(P(respan(sp, doc)));
                TtDelimited(sp, Rc::new(Delimited {
                    delim: token::Bracket,
                    open_span: sp,
                    tts: vec![TtToken(sp, token::Interpolated(doc))],
                    close_span: sp,
                }))
            }
            (&TtDelimited(_, ref delimed), _) => {
                if index == 0 {
                    return delimed.open_tt();
                }
                if index == delimed.tts.len() + 1 {
                    return delimed.close_tt();
                }
                delimed.tts[index - 1].clone()
            }
            (&TtToken(sp, token::SubstNt(name, name_st)), _) => {
                let v = [TtToken(sp, token::Dollar),
                         TtToken(sp, token::Ident(name, name_st))];
                v[index]
            }
            (&TtToken(sp, token::MatchNt(name, kind, name_st, kind_st)), _) => {
                let v = [TtToken(sp, token::SubstNt(name, name_st)),
                         TtToken(sp, token::Colon),
                         TtToken(sp, token::Ident(kind, kind_st))];
                v[index]
            }
            (&TtSequence(_, ref seq), _) => {
                seq.tts[index].clone()
            }
            _ => panic!("Cannot expand a token tree")
        }
    }

    /// Returns the `Span` corresponding to this token tree.
    pub fn get_span(&self) -> Span {
        match *self {
            TtToken(span, _)     => span,
            TtDelimited(span, _) => span,
            TtSequence(span, _)  => span,
        }
    }
}

pub type Mac = Spanned<Mac_>;

/// Represents a macro invocation. The Path indicates which macro
/// is being invoked, and the vector of token-trees contains the source
/// of the macro invocation.
/// There's only one flavor, now, so this could presumably be simplified.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Mac_ {
    // NB: the additional ident for a macro_rules-style macro is actually
    // stored in the enclosing item. Oog.
    MacInvocTT(Path, Vec<TokenTree> , SyntaxContext),   // new macro-invocation
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum StrStyle {
    CookedStr,
    RawStr(uint)
}

pub type Lit = Spanned<Lit_>;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Sign {
    Minus,
    Plus
}

impl<T: PartialOrd+Zero> Sign {
    pub fn new(n: T) -> Sign {
        if n < Zero::zero() {
            Minus
        } else {
            Plus
        }
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum LitIntType {
    SignedIntLit(IntTy, Sign),
    UnsignedIntLit(UintTy),
    UnsuffixedIntLit(Sign)
}

impl LitIntType {
    pub fn suffix_len(&self) -> uint {
        match *self {
            UnsuffixedIntLit(_) => 0,
            SignedIntLit(s, _) => s.suffix_len(),
            UnsignedIntLit(u) => u.suffix_len()
        }
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Lit_ {
    LitStr(InternedString, StrStyle),
    LitBinary(Rc<Vec<u8> >),
    LitByte(u8),
    LitChar(char),
    LitInt(u64, LitIntType),
    LitFloat(InternedString, FloatTy),
    LitFloatUnsuffixed(InternedString),
    LitNil,
    LitBool(bool),
}

// NB: If you change this, you'll probably want to change the corresponding
// type structure in middle/ty.rs as well.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct MutTy {
    pub ty: P<Ty>,
    pub mutbl: Mutability,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct TypeField {
    pub ident: Ident,
    pub mt: MutTy,
    pub span: Span,
}

/// Represents a required method in a trait declaration,
/// one without a default implementation
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct TypeMethod {
    pub ident: Ident,
    pub attrs: Vec<Attribute>,
    pub fn_style: FnStyle,
    pub abi: Abi,
    pub decl: P<FnDecl>,
    pub generics: Generics,
    pub explicit_self: ExplicitSelf,
    pub id: NodeId,
    pub span: Span,
    pub vis: Visibility,
}

/// Represents a method declaration in a trait declaration, possibly including
/// a default implementation A trait method is either required (meaning it
/// doesn't have an implementation, just a signature) or provided (meaning it
/// has a default implementation).
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum TraitItem {
    RequiredMethod(TypeMethod),
    ProvidedMethod(P<Method>),
    TypeTraitItem(P<AssociatedType>),
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum ImplItem {
    MethodImplItem(P<Method>),
    TypeImplItem(P<Typedef>),
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct AssociatedType {
    pub attrs: Vec<Attribute>,
    pub ty_param: TyParam,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Typedef {
    pub id: NodeId,
    pub span: Span,
    pub ident: Ident,
    pub vis: Visibility,
    pub attrs: Vec<Attribute>,
    pub typ: P<Ty>,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash)]
pub enum IntTy {
    TyI,
    TyI8,
    TyI16,
    TyI32,
    TyI64,
}

impl fmt::Show for IntTy {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", ast_util::int_ty_to_string(*self, None))
    }
}

impl IntTy {
    pub fn suffix_len(&self) -> uint {
        match *self {
            TyI => 1,
            TyI8 => 2,
            TyI16 | TyI32 | TyI64  => 3,
        }
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash)]
pub enum UintTy {
    TyU,
    TyU8,
    TyU16,
    TyU32,
    TyU64,
}

impl UintTy {
    pub fn suffix_len(&self) -> uint {
        match *self {
            TyU => 1,
            TyU8 => 2,
            TyU16 | TyU32 | TyU64  => 3,
        }
    }
}

impl fmt::Show for UintTy {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", ast_util::uint_ty_to_string(*self, None))
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash)]
pub enum FloatTy {
    TyF32,
    TyF64,
}

impl fmt::Show for FloatTy {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", ast_util::float_ty_to_string(*self))
    }
}

impl FloatTy {
    pub fn suffix_len(&self) -> uint {
        match *self {
            TyF32 | TyF64 => 3, // add F128 handling here
        }
    }
}

// NB PartialEq method appears below.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Ty {
    pub id: NodeId,
    pub node: Ty_,
    pub span: Span,
}

/// Not represented directly in the AST, referred to by name through a ty_path.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum PrimTy {
    TyInt(IntTy),
    TyUint(UintTy),
    TyFloat(FloatTy),
    TyStr,
    TyBool,
    TyChar
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash)]
pub enum Onceness {
    Once,
    Many
}

impl fmt::Show for Onceness {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Once => "once".fmt(f),
            Many => "many".fmt(f),
        }
    }
}

/// Represents the type of a closure
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct ClosureTy {
    pub lifetimes: Vec<LifetimeDef>,
    pub fn_style: FnStyle,
    pub onceness: Onceness,
    pub decl: P<FnDecl>,
    pub bounds: TyParamBounds,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct BareFnTy {
    pub fn_style: FnStyle,
    pub abi: Abi,
    pub lifetimes: Vec<LifetimeDef>,
    pub decl: P<FnDecl>
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
/// The different kinds of types recognized by the compiler
pub enum Ty_ {
    /// The unit type (`()`)
    TyNil,
    /// The bottom type (`!`)
    TyBot,
    TyUniq(P<Ty>),
    /// An array (`[T]`)
    TyVec(P<Ty>),
    /// A fixed length array (`[T, ..n]`)
    TyFixedLengthVec(P<Ty>, P<Expr>),
    /// A raw pointer (`*const T` or `*mut T`)
    TyPtr(MutTy),
    /// A reference (`&'a T` or `&'a mut T`)
    TyRptr(Option<Lifetime>, MutTy),
    /// A closure (e.g. `|uint| -> bool`)
    TyClosure(P<ClosureTy>),
    /// A procedure (e.g `proc(uint) -> bool`)
    TyProc(P<ClosureTy>),
    /// A bare function (e.g. `fn(uint) -> bool`)
    TyBareFn(P<BareFnTy>),
    /// A tuple (`(A, B, C, D,...)`)
    TyTup(Vec<P<Ty>> ),
    /// A path (`module::module::...::Type`) or primitive
    ///
    /// Type parameters are stored in the Path itself
    TyPath(Path, Option<TyParamBounds>, NodeId), // for #7264; see above
    /// A type like `for<'a> Foo<&'a Bar>`
    TyPolyTraitRef(P<PolyTraitRef>),
    /// A "qualified path", e.g. `<Vec<T> as SomeTrait>::SomeType`
    TyQPath(P<QPath>),
    /// No-op; kept solely so that we can pretty-print faithfully
    TyParen(P<Ty>),
    /// Unused for now
    TyTypeof(P<Expr>),
    /// TyInfer means the type should be inferred instead of it having been
    /// specified. This can appear anywhere in a type.
    TyInfer,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum AsmDialect {
    AsmAtt,
    AsmIntel
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct InlineAsm {
    pub asm: InternedString,
    pub asm_str_style: StrStyle,
    pub outputs: Vec<(InternedString, P<Expr>, bool)>,
    pub inputs: Vec<(InternedString, P<Expr>)>,
    pub clobbers: InternedString,
    pub volatile: bool,
    pub alignstack: bool,
    pub dialect: AsmDialect,
    pub expn_id: ExpnId,
}

/// represents an argument in a function header
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Arg {
    pub ty: P<Ty>,
    pub pat: P<Pat>,
    pub id: NodeId,
}

impl Arg {
    pub fn new_self(span: Span, mutability: Mutability, self_ident: Ident) -> Arg {
        let path = Spanned{span:span,node:self_ident};
        Arg {
            // HACK(eddyb) fake type for the self argument.
            ty: P(Ty {
                id: DUMMY_NODE_ID,
                node: TyInfer,
                span: DUMMY_SP,
            }),
            pat: P(Pat {
                id: DUMMY_NODE_ID,
                node: PatIdent(BindByValue(mutability), path, None),
                span: span
            }),
            id: DUMMY_NODE_ID
        }
    }
}

/// represents the header (not the body) of a function declaration
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct FnDecl {
    pub inputs: Vec<Arg>,
    pub output: P<Ty>,
    pub cf: RetStyle,
    pub variadic: bool
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash)]
pub enum FnStyle {
    /// Declared with "unsafe fn"
    UnsafeFn,
    /// Declared with "fn"
    NormalFn,
}

impl fmt::Show for FnStyle {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            NormalFn => "normal".fmt(f),
            UnsafeFn => "unsafe".fmt(f),
        }
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum RetStyle {
    /// Functions with return type ! that always
    /// raise an error or exit (i.e. never return to the caller)
    NoReturn,
    /// Everything else
    Return,
}

/// Represents the kind of 'self' associated with a method
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum ExplicitSelf_ {
    /// No self
    SelfStatic,
    /// `self`
    SelfValue(Ident),
    /// `&'lt self`, `&'lt mut self`
    SelfRegion(Option<Lifetime>, Mutability, Ident),
    /// `self: TYPE`
    SelfExplicit(P<Ty>, Ident),
}

pub type ExplicitSelf = Spanned<ExplicitSelf_>;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Method {
    pub attrs: Vec<Attribute>,
    pub id: NodeId,
    pub span: Span,
    pub node: Method_,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Method_ {
    /// Represents a method declaration
    MethDecl(Ident,
             Generics,
             Abi,
             ExplicitSelf,
             FnStyle,
             P<FnDecl>,
             P<Block>,
             Visibility),
    /// Represents a macro in method position
    MethMac(Mac),
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Mod {
    /// A span from the first token past `{` to the last token until `}`.
    /// For `mod foo;`, the inner span ranges from the first token
    /// to the last token in the external file.
    pub inner: Span,
    pub view_items: Vec<ViewItem>,
    pub items: Vec<P<Item>>,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct ForeignMod {
    pub abi: Abi,
    pub view_items: Vec<ViewItem>,
    pub items: Vec<P<ForeignItem>>,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct VariantArg {
    pub ty: P<Ty>,
    pub id: NodeId,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum VariantKind {
    TupleVariantKind(Vec<VariantArg>),
    StructVariantKind(P<StructDef>),
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct EnumDef {
    pub variants: Vec<P<Variant>>,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Variant_ {
    pub name: Ident,
    pub attrs: Vec<Attribute>,
    pub kind: VariantKind,
    pub id: NodeId,
    pub disr_expr: Option<P<Expr>>,
    pub vis: Visibility,
}

pub type Variant = Spanned<Variant_>;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum PathListItem_ {
    PathListIdent { pub name: Ident, pub id: NodeId },
    PathListMod { pub id: NodeId }
}

impl PathListItem_ {
    pub fn id(&self) -> NodeId {
        match *self {
            PathListIdent { id, .. } | PathListMod { id } => id
        }
    }
}

pub type PathListItem = Spanned<PathListItem_>;

pub type ViewPath = Spanned<ViewPath_>;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum ViewPath_ {

    /// `foo::bar::baz as quux`
    ///
    /// or just
    ///
    /// `foo::bar::baz` (with `as baz` implicitly on the right)
    ViewPathSimple(Ident, Path, NodeId),

    /// `foo::bar::*`
    ViewPathGlob(Path, NodeId),

    /// `foo::bar::{a,b,c}`
    ViewPathList(Path, Vec<PathListItem> , NodeId)
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct ViewItem {
    pub node: ViewItem_,
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub span: Span,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum ViewItem_ {
    /// Ident: name used to refer to this crate in the code
    /// optional (InternedString,StrStyle): if present, this is a location
    /// (containing arbitrary characters) from which to fetch the crate sources
    /// For example, extern crate whatever = "github.com/rust-lang/rust"
    ViewItemExternCrate(Ident, Option<(InternedString,StrStyle)>, NodeId),
    ViewItemUse(P<ViewPath>),
}

/// Meta-data associated with an item
pub type Attribute = Spanned<Attribute_>;

/// Distinguishes between Attributes that decorate items and Attributes that
/// are contained as statements within items. These two cases need to be
/// distinguished for pretty-printing.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum AttrStyle {
    AttrOuter,
    AttrInner,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct AttrId(pub uint);

/// Doc-comments are promoted to attributes that have is_sugared_doc = true
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Attribute_ {
    pub id: AttrId,
    pub style: AttrStyle,
    pub value: P<MetaItem>,
    pub is_sugared_doc: bool,
}

/// TraitRef's appear in impls.
/// resolve maps each TraitRef's ref_id to its defining trait; that's all
/// that the ref_id is for. The impl_id maps to the "self type" of this impl.
/// If this impl is an ItemImpl, the impl_id is redundant (it could be the
/// same as the impl's node id).
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct TraitRef {
    pub path: Path,
    pub ref_id: NodeId,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct PolyTraitRef {
    /// The `'a` in `<'a> Foo<&'a T>`
    pub bound_lifetimes: Vec<LifetimeDef>,

    /// The `Foo<&'a T>` in `<'a> Foo<&'a T>`
    pub trait_ref: TraitRef
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Visibility {
    Public,
    Inherited,
}

impl Visibility {
    pub fn inherit_from(&self, parent_visibility: Visibility) -> Visibility {
        match self {
            &Inherited => parent_visibility,
            &Public => *self
        }
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct StructField_ {
    pub kind: StructFieldKind,
    pub id: NodeId,
    pub ty: P<Ty>,
    pub attrs: Vec<Attribute>,
}

impl StructField_ {
    pub fn ident(&self) -> Option<Ident> {
        match self.kind {
            NamedField(ref ident, _) => Some(ident.clone()),
            UnnamedField(_) => None
        }
    }
}

pub type StructField = Spanned<StructField_>;

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum StructFieldKind {
    NamedField(Ident, Visibility),
    /// Element of a tuple-like struct
    UnnamedField(Visibility),
}

impl StructFieldKind {
    pub fn is_unnamed(&self) -> bool {
        match *self {
            UnnamedField(..) => true,
            NamedField(..) => false,
        }
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct StructDef {
    /// Fields, not including ctor
    pub fields: Vec<StructField>,
    /// ID of the constructor. This is only used for tuple- or enum-like
    /// structs.
    pub ctor_id: Option<NodeId>,
}

/*
  FIXME (#3300): Should allow items to be anonymous. Right now
  we just use dummy names for anon items.
 */
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct Item {
    pub ident: Ident,
    pub attrs: Vec<Attribute>,
    pub id: NodeId,
    pub node: Item_,
    pub vis: Visibility,
    pub span: Span,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum Item_ {
    ItemStatic(P<Ty>, Mutability, P<Expr>),
    ItemConst(P<Ty>, P<Expr>),
    ItemFn(P<FnDecl>, FnStyle, Abi, Generics, P<Block>),
    ItemMod(Mod),
    ItemForeignMod(ForeignMod),
    ItemTy(P<Ty>, Generics),
    ItemEnum(EnumDef, Generics),
    ItemStruct(P<StructDef>, Generics),
    /// Represents a Trait Declaration
    ItemTrait(Generics,
              Option<TraitRef>, // (optional) default bound not required for Self.
                                // Currently, only Sized makes sense here.
              TyParamBounds,
              Vec<TraitItem>),
    ItemImpl(Generics,
             Option<TraitRef>, // (optional) trait this impl implements
             P<Ty>, // self
             Vec<ImplItem>),
    /// A macro invocation (which includes macro definition)
    ItemMac(Mac),
}

impl Item_ {
    pub fn descriptive_variant(&self) -> &str {
        match *self {
            ItemStatic(..) => "static item",
            ItemConst(..) => "constant item",
            ItemFn(..) => "function",
            ItemMod(..) => "module",
            ItemForeignMod(..) => "foreign module",
            ItemTy(..) => "type alias",
            ItemEnum(..) => "enum",
            ItemStruct(..) => "struct",
            ItemTrait(..) => "trait",
            ItemMac(..) |
            ItemImpl(..) => "item"
        }
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub struct ForeignItem {
    pub ident: Ident,
    pub attrs: Vec<Attribute>,
    pub node: ForeignItem_,
    pub id: NodeId,
    pub span: Span,
    pub vis: Visibility,
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum ForeignItem_ {
    ForeignItemFn(P<FnDecl>, Generics),
    ForeignItemStatic(P<Ty>, /* is_mutbl */ bool),
}

impl ForeignItem_ {
    pub fn descriptive_variant(&self) -> &str {
        match *self {
            ForeignItemFn(..) => "foreign function",
            ForeignItemStatic(..) => "foreign static item"
        }
    }
}

#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum UnboxedClosureKind {
    FnUnboxedClosureKind,
    FnMutUnboxedClosureKind,
    FnOnceUnboxedClosureKind,
}

/// The data we save and restore about an inlined item or method.  This is not
/// part of the AST that we parse from a file, but it becomes part of the tree
/// that we trans.
#[deriving(Clone, PartialEq, Eq, Encodable, Decodable, Hash, Show)]
pub enum InlinedItem {
    IIItem(P<Item>),
    IITraitItem(DefId /* impl id */, TraitItem),
    IIImplItem(DefId /* impl id */, ImplItem),
    IIForeign(P<ForeignItem>),
}

#[cfg(test)]
mod test {
    use serialize::json;
    use serialize;
    use codemap::*;
    use super::*;

    // are ASTs encodable?
    #[test]
    fn check_asts_encodable() {
        use std::io;
        let e = Crate {
            module: Mod {
                inner: Span {
                    lo: BytePos(11),
                    hi: BytePos(19),
                    expn_id: NO_EXPANSION,
                },
                view_items: Vec::new(),
                items: Vec::new(),
            },
            attrs: Vec::new(),
            config: Vec::new(),
            span: Span {
                lo: BytePos(10),
                hi: BytePos(20),
                expn_id: NO_EXPANSION,
            },
            exported_macros: Vec::new(),
        };
        // doesn't matter which encoder we use....
        let _f = &e as &serialize::Encodable<json::Encoder, io::IoError>;
    }
}
