// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// The Rust abstract syntax tree.

use codemap::{span, spanned};
use abi::AbiSet;
use opt_vec::OptVec;
use parse::token::{interner_get, str_to_ident};

use std::hashmap::HashMap;
use std::option::Option;
use std::to_str::ToStr;
use extra::serialize::{Encodable, Decodable, Encoder, Decoder};

// an identifier contains a Name (index into the interner
// table) and a SyntaxContext to track renaming and
// macro expansion per Flatt et al., "Macros
// That Work Together"
#[deriving(Clone, Eq, IterBytes)]
pub struct ident { name: Name, ctxt: SyntaxContext }

/// Construct an identifier with the given name and an empty context:
pub fn new_ident(name: Name) -> ident { ident {name: name, ctxt: empty_ctxt}}

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
pub type SyntaxContext = uint;

pub struct SCTable {
    table : ~[SyntaxContext_],
    mark_memo : HashMap<(SyntaxContext,Mrk),SyntaxContext>,
    rename_memo : HashMap<(SyntaxContext,ident,Name),SyntaxContext>
}
// NB: these must be placed in any SCTable...
pub static empty_ctxt : uint = 0;
pub static illegal_ctxt : uint = 1;

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub enum SyntaxContext_ {
    EmptyCtxt,
    Mark (Mrk,SyntaxContext),
    // flattening the name and syntaxcontext into the rename...
    // HIDDEN INVARIANTS:
    // 1) the first name in a Rename node
    // can only be a programmer-supplied name.
    // 2) Every Rename node with a given Name in the
    // "to" slot must have the same name and context
    // in the "from" slot. In essence, they're all
    // pointers to a single "rename" event node.
    Rename (ident,Name,SyntaxContext),
    IllegalCtxt()
}

/// A name is a part of an identifier, representing a string or gensym. It's
/// the result of interning.
pub type Name = uint;
/// A mark represents a unique id associated with a macro expansion
pub type Mrk = uint;

impl<S:Encoder> Encodable<S> for ident {
    fn encode(&self, s: &mut S) {
        s.emit_str(interner_get(self.name));
    }
}

#[deriving(IterBytes)]
impl<D:Decoder> Decodable<D> for ident {
    fn decode(d: &mut D) -> ident {
        str_to_ident(d.read_str())
    }
}

/// Function name (not all functions have names)
pub type fn_ident = Option<ident>;

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct Lifetime {
    id: node_id,
    span: span,
    ident: ident
}

// a "Path" is essentially Rust's notion of a name;
// for instance: core::cmp::Eq  .  It's represented
// as a sequence of identifiers, along with a bunch
// of supporting information.
#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct Path {
    span: span,
    /// A `::foo` path, is relative to the crate root rather than current
    /// module (like paths in an import).
    global: bool,
    /// The segments in the path (the things separated by ::)
    idents: ~[ident],
    /// "Region parameter", currently only one lifetime is allowed in a path.
    rp: Option<Lifetime>,
    /// These are the type parameters, ie, the `a, b` in `foo::bar::<a, b>`
    types: ~[Ty],
}

pub type CrateNum = int;

pub type node_id = int;

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct def_id {
    crate: CrateNum,
    node: node_id,
}

pub static local_crate: CrateNum = 0;
pub static crate_node_id: node_id = 0;

// The AST represents all type param bounds as types.
// typeck::collect::compute_bounds matches these against
// the "special" built-in traits (see middle::lang_items) and
// detects Copy, Send, Send, and Freeze.
#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum TyParamBound {
    TraitTyParamBound(trait_ref),
    RegionTyParamBound
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct TyParam {
    ident: ident,
    id: node_id,
    bounds: OptVec<TyParamBound>
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct Generics {
    lifetimes: OptVec<Lifetime>,
    ty_params: OptVec<TyParam>,
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

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum def {
    def_fn(def_id, purity),
    def_static_method(/* method */ def_id,
                      /* trait */  Option<def_id>,
                      purity),
    def_self(node_id, bool /* is_implicit */),
    def_self_ty(/* trait id */ node_id),
    def_mod(def_id),
    def_foreign_mod(def_id),
    def_static(def_id, bool /* is_mutbl */),
    def_arg(node_id, bool /* is_mutbl */),
    def_local(node_id, bool /* is_mutbl */),
    def_variant(def_id /* enum */, def_id /* variant */),
    def_ty(def_id),
    def_trait(def_id),
    def_prim_ty(prim_ty),
    def_ty_param(def_id, uint),
    def_binding(node_id, binding_mode),
    def_use(def_id),
    def_upvar(node_id,  // id of closed over var
              @def,     // closed over def
              node_id,  // expr node that creates the closure
              node_id), // id for the block/body of the closure expr
    def_struct(def_id),
    def_typaram_binder(node_id), /* struct, impl or trait with ty params */
    def_region(node_id),
    def_label(node_id),
    def_method(def_id /* method */, Option<def_id> /* trait */),
}


// The set of MetaItems that define the compilation environment of the crate,
// used to drive conditional compilation
pub type CrateConfig = ~[@MetaItem];

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct Crate {
    module: _mod,
    attrs: ~[Attribute],
    config: CrateConfig,
    span: span,
}

pub type MetaItem = spanned<MetaItem_>;

#[deriving(Clone, Encodable, Decodable, IterBytes)]
pub enum MetaItem_ {
    MetaWord(@str),
    MetaList(@str, ~[@MetaItem]),
    MetaNameValue(@str, lit),
}

// can't be derived because the MetaList requires an unordered comparison
impl Eq for MetaItem_ {
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

#[deriving(Clone, Eq, Encodable, Decodable,IterBytes)]
pub struct Block {
    view_items: ~[view_item],
    stmts: ~[@stmt],
    expr: Option<@expr>,
    id: node_id,
    rules: blk_check_mode,
    span: span,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct pat {
    id: node_id,
    node: pat_,
    span: span,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct field_pat {
    ident: ident,
    pat: @pat,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum binding_mode {
    bind_by_ref(mutability),
    bind_infer
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum pat_ {
    pat_wild,
    // A pat_ident may either be a new bound variable,
    // or a nullary enum (in which case the second field
    // is None).
    // In the nullary enum case, the parser can't determine
    // which it is. The resolver determines this, and
    // records this pattern's node_id in an auxiliary
    // set (of "pat_idents that refer to nullary enums")
    pat_ident(binding_mode, Path, Option<@pat>),
    pat_enum(Path, Option<~[@pat]>), /* "none" means a * pattern where
                                       * we don't bind the fields to names */
    pat_struct(Path, ~[field_pat], bool),
    pat_tup(~[@pat]),
    pat_box(@pat),
    pat_uniq(@pat),
    pat_region(@pat), // borrowed pointer pattern
    pat_lit(@expr),
    pat_range(@expr, @expr),
    // [a, b, ..i, y, z] is represented as
    // pat_vec(~[a, b], Some(i), ~[y, z])
    pat_vec(~[@pat], Option<@pat>, ~[@pat])
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum mutability { m_mutbl, m_imm, m_const, }

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum Sigil {
    BorrowedSigil,
    OwnedSigil,
    ManagedSigil
}

impl ToStr for Sigil {
    fn to_str(&self) -> ~str {
        match *self {
            BorrowedSigil => ~"&",
            OwnedSigil => ~"~",
            ManagedSigil => ~"@"
         }
    }
}

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub enum vstore {
    // FIXME (#3469): Change uint to @expr (actually only constant exprs)
    vstore_fixed(Option<uint>),     // [1,2,3,4]
    vstore_uniq,                    // ~[1,2,3,4]
    vstore_box,                     // @[1,2,3,4]
    vstore_slice(Option<Lifetime>) // &'foo? [1,2,3,4]
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum expr_vstore {
    expr_vstore_uniq,                  // ~[1,2,3,4]
    expr_vstore_box,                   // @[1,2,3,4]
    expr_vstore_mut_box,               // @mut [1,2,3,4]
    expr_vstore_slice,                 // &[1,2,3,4]
    expr_vstore_mut_slice,             // &mut [1,2,3,4]
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum binop {
    add,
    subtract,
    mul,
    div,
    rem,
    and,
    or,
    bitxor,
    bitand,
    bitor,
    shl,
    shr,
    eq,
    lt,
    le,
    ne,
    ge,
    gt,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum unop {
    box(mutability),
    uniq,
    deref,
    not,
    neg
}

pub type stmt = spanned<stmt_>;

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum stmt_ {
    // could be an item or a local (let) binding:
    stmt_decl(@decl, node_id),

    // expr without trailing semi-colon (must have unit type):
    stmt_expr(@expr, node_id),

    // expr with trailing semi-colon (may have any type):
    stmt_semi(@expr, node_id),

    // bool: is there a trailing sem-colon?
    stmt_mac(mac, bool),
}

// FIXME (pending discussion of #1697, #2178...): local should really be
// a refinement on pat.
#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub struct Local {
    is_mutbl: bool,
    ty: Ty,
    pat: @pat,
    init: Option<@expr>,
    id: node_id,
    span: span,
}

pub type decl = spanned<decl_>;

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub enum decl_ {
    // a local (let) binding:
    decl_local(@Local),
    // an item binding:
    decl_item(@item),
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct arm {
    pats: ~[@pat],
    guard: Option<@expr>,
    body: Block,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct Field {
    ident: ident,
    expr: @expr,
    span: span,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum blk_check_mode {
    default_blk,
    unsafe_blk,
}

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub struct expr {
    id: node_id,
    node: expr_,
    span: span,
}

impl expr {
    pub fn get_callee_id(&self) -> Option<node_id> {
        match self.node {
            expr_method_call(callee_id, _, _, _, _, _) |
            expr_index(callee_id, _, _) |
            expr_binary(callee_id, _, _, _) |
            expr_assign_op(callee_id, _, _, _) |
            expr_unary(callee_id, _, _) => Some(callee_id),
            _ => None,
        }
    }
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum CallSugar {
    NoSugar,
    DoSugar,
    ForSugar
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum expr_ {
    expr_vstore(@expr, expr_vstore),
    expr_vec(~[@expr], mutability),
    expr_call(@expr, ~[@expr], CallSugar),
    expr_method_call(node_id, @expr, ident, ~[Ty], ~[@expr], CallSugar),
    expr_tup(~[@expr]),
    expr_binary(node_id, binop, @expr, @expr),
    expr_unary(node_id, unop, @expr),
    expr_lit(@lit),
    expr_cast(@expr, Ty),
    expr_if(@expr, Block, Option<@expr>),
    expr_while(@expr, Block),
    /* Conditionless loop (can be exited with break, cont, or ret)
       Same semantics as while(true) { body }, but typestate knows that the
       (implicit) condition is always true. */
    expr_loop(Block, Option<ident>),
    expr_match(@expr, ~[arm]),
    expr_fn_block(fn_decl, Block),
    // Inner expr is always an expr_fn_block. We need the wrapping node to
    // easily type this (a function returning nil on the inside but bool on
    // the outside).
    expr_loop_body(@expr),
    // Like expr_loop_body but for 'do' blocks
    expr_do_body(@expr),
    expr_block(Block),

    expr_assign(@expr, @expr),
    expr_assign_op(node_id, binop, @expr, @expr),
    expr_field(@expr, ident, ~[Ty]),
    expr_index(node_id, @expr, @expr),
    expr_path(Path),

    /// The special identifier `self`.
    expr_self,
    expr_addr_of(mutability, @expr),
    expr_break(Option<ident>),
    expr_again(Option<ident>),
    expr_ret(Option<@expr>),
    expr_log(@expr, @expr),

    expr_inline_asm(inline_asm),

    expr_mac(mac),

    // A struct literal expression.
    expr_struct(Path, ~[Field], Option<@expr> /* base */),

    // A vector literal constructed from one repeated element.
    expr_repeat(@expr /* element */, @expr /* count */, mutability),

    // No-op: used solely so we can pretty-print faithfully
    expr_paren(@expr)
}

// When the main rust parser encounters a syntax-extension invocation, it
// parses the arguments to the invocation as a token-tree. This is a very
// loose structure, such that all sorts of different AST-fragments can
// be passed to syntax extensions using a uniform type.
//
// If the syntax extension is an MBE macro, it will attempt to match its
// LHS "matchers" against the provided token tree, and if it finds a
// match, will transcribe the RHS token tree, splicing in any captured
// macro_parser::matched_nonterminals into the tt_nonterminals it finds.
//
// The RHS of an MBE macro is the only place a tt_nonterminal or tt_seq
// makes any real sense. You could write them elsewhere but nothing
// else knows what to do with them, so you'll probably get a syntax
// error.
//
#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
#[doc="For macro invocations; parsing is delegated to the macro"]
pub enum token_tree {
    // a single token
    tt_tok(span, ::parse::token::Token),
    // a delimited sequence (the delimiters appear as the first
    // and last elements of the vector)
    tt_delim(@mut ~[token_tree]),
    // These only make sense for right-hand-sides of MBE macros:

    // a kleene-style repetition sequence with a span, a tt_forest,
    // an optional separator (?), and a boolean where true indicates
    // zero or more (*), and false indicates one or more (+).
    tt_seq(span, @mut ~[token_tree], Option<::parse::token::Token>, bool),

    // a syntactic variable that will be filled in by macro expansion.
    tt_nonterminal(span, ident)
}

//
// Matchers are nodes defined-by and recognized-by the main rust parser and
// language, but they're only ever found inside syntax-extension invocations;
// indeed, the only thing that ever _activates_ the rules in the rust parser
// for parsing a matcher is a matcher looking for the 'matchers' nonterminal
// itself. Matchers represent a small sub-language for pattern-matching
// token-trees, and are thus primarily used by the macro-defining extension
// itself.
//
// match_tok
// ---------
//
//     A matcher that matches a single token, denoted by the token itself. So
//     long as there's no $ involved.
//
//
// match_seq
// ---------
//
//     A matcher that matches a sequence of sub-matchers, denoted various
//     possible ways:
//
//             $(M)*       zero or more Ms
//             $(M)+       one or more Ms
//             $(M),+      one or more comma-separated Ms
//             $(A B C);*  zero or more semi-separated 'A B C' seqs
//
//
// match_nonterminal
// -----------------
//
//     A matcher that matches one of a few interesting named rust
//     nonterminals, such as types, expressions, items, or raw token-trees. A
//     black-box matcher on expr, for example, binds an expr to a given ident,
//     and that ident can re-occur as an interpolation in the RHS of a
//     macro-by-example rule. For example:
//
//        $foo:expr   =>     1 + $foo    // interpolate an expr
//        $foo:tt     =>     $foo        // interpolate a token-tree
//        $foo:tt     =>     bar! $foo   // only other valid interpolation
//                                       // is in arg position for another
//                                       // macro
//
// As a final, horrifying aside, note that macro-by-example's input is
// also matched by one of these matchers. Holy self-referential! It is matched
// by an match_seq, specifically this one:
//
//                   $( $lhs:matchers => $rhs:tt );+
//
// If you understand that, you have closed to loop and understand the whole
// macro system. Congratulations.
//
pub type matcher = spanned<matcher_>;

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum matcher_ {
    // match one token
    match_tok(::parse::token::Token),
    // match repetitions of a sequence: body, separator, zero ok?,
    // lo, hi position-in-match-array used:
    match_seq(~[matcher], Option<::parse::token::Token>, bool, uint, uint),
    // parse a Rust NT: name to bind, name of NT, position in match array:
    match_nonterminal(ident, ident, uint)
}

pub type mac = spanned<mac_>;

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum mac_ {
    mac_invoc_tt(Path,~[token_tree]),   // new macro-invocation
}

pub type lit = spanned<lit_>;

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum lit_ {
    lit_str(@str),
    lit_int(i64, int_ty),
    lit_uint(u64, uint_ty),
    lit_int_unsuffixed(i64),
    lit_float(@str, float_ty),
    lit_float_unsuffixed(@str),
    lit_nil,
    lit_bool(bool),
}

// NB: If you change this, you'll probably want to change the corresponding
// type structure in middle/ty.rs as well.
#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct mt {
    ty: ~Ty,
    mutbl: mutability,
}

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub struct ty_field_ {
    ident: ident,
    mt: mt,
}

pub type ty_field = spanned<ty_field_>;

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct ty_method {
    ident: ident,
    attrs: ~[Attribute],
    purity: purity,
    decl: fn_decl,
    generics: Generics,
    explicit_self: explicit_self,
    id: node_id,
    span: span,
}

// A trait method is either required (meaning it doesn't have an
// implementation, just a signature) or provided (meaning it has a default
// implementation).
#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum trait_method {
    required(ty_method),
    provided(@method),
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum int_ty {
    ty_i,
    ty_char,
    ty_i8,
    ty_i16,
    ty_i32,
    ty_i64,
}

impl ToStr for int_ty {
    fn to_str(&self) -> ~str {
        ::ast_util::int_ty_to_str(*self)
    }
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum uint_ty {
    ty_u,
    ty_u8,
    ty_u16,
    ty_u32,
    ty_u64,
}

impl ToStr for uint_ty {
    fn to_str(&self) -> ~str {
        ::ast_util::uint_ty_to_str(*self)
    }
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum float_ty {
    ty_f,
    ty_f32,
    ty_f64,
}

impl ToStr for float_ty {
    fn to_str(&self) -> ~str {
        ::ast_util::float_ty_to_str(*self)
    }
}

// NB Eq method appears below.
#[deriving(Clone, Eq, Encodable, Decodable,IterBytes)]
pub struct Ty {
    id: node_id,
    node: ty_,
    span: span,
}

// Not represented directly in the AST, referred to by name through a ty_path.
#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum prim_ty {
    ty_int(int_ty),
    ty_uint(uint_ty),
    ty_float(float_ty),
    ty_str,
    ty_bool,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum Onceness {
    Once,
    Many
}

#[deriving(IterBytes)]
impl ToStr for Onceness {
    fn to_str(&self) -> ~str {
        match *self {
            Once => ~"once",
            Many => ~"many"
        }
    }
}

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub struct TyClosure {
    sigil: Sigil,
    region: Option<Lifetime>,
    lifetimes: OptVec<Lifetime>,
    purity: purity,
    onceness: Onceness,
    decl: fn_decl,
    // Optional optvec distinguishes between "fn()" and "fn:()" so we can
    // implement issue #7264. None means "fn()", which means infer a default
    // bound based on pointer sigil during typeck. Some(Empty) means "fn:()",
    // which means use no bounds (e.g., not even Owned on a ~fn()).
    bounds: Option<OptVec<TyParamBound>>,
}

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub struct TyBareFn {
    purity: purity,
    abis: AbiSet,
    lifetimes: OptVec<Lifetime>,
    decl: fn_decl
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum ty_ {
    ty_nil,
    ty_bot, /* bottom type */
    ty_box(mt),
    ty_uniq(mt),
    ty_vec(mt),
    ty_fixed_length_vec(mt, @expr),
    ty_ptr(mt),
    ty_rptr(Option<Lifetime>, mt),
    ty_closure(@TyClosure),
    ty_bare_fn(@TyBareFn),
    ty_tup(~[Ty]),
    ty_path(Path, Option<OptVec<TyParamBound>>, node_id), // for #7264; see above
    ty_mac(mac),
    // ty_infer means the type should be inferred instead of it having been
    // specified. This should only appear at the "top level" of a type and not
    // nested in one.
    ty_infer,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum asm_dialect {
    asm_att,
    asm_intel
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct inline_asm {
    asm: @str,
    clobbers: @str,
    inputs: ~[(@str, @expr)],
    outputs: ~[(@str, @expr)],
    volatile: bool,
    alignstack: bool,
    dialect: asm_dialect
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct arg {
    is_mutbl: bool,
    ty: Ty,
    pat: @pat,
    id: node_id,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct fn_decl {
    inputs: ~[arg],
    output: Ty,
    cf: ret_style,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum purity {
    unsafe_fn, // declared with "unsafe fn"
    impure_fn, // declared with "fn"
    extern_fn, // declared with "extern fn"
}

#[deriving(IterBytes)]
impl ToStr for purity {
    fn to_str(&self) -> ~str {
        match *self {
            impure_fn => ~"impure",
            unsafe_fn => ~"unsafe",
            extern_fn => ~"extern"
        }
    }
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum ret_style {
    noreturn, // functions with return type _|_ that always
              // raise an error or exit (i.e. never return to the caller)
    return_val, // everything else
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum explicit_self_ {
    sty_static,                                // no self
    sty_value,                                 // `self`
    sty_region(Option<Lifetime>, mutability), // `&'lt self`
    sty_box(mutability),                       // `@self`
    sty_uniq                                   // `~self`
}

pub type explicit_self = spanned<explicit_self_>;

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub struct method {
    ident: ident,
    attrs: ~[Attribute],
    generics: Generics,
    explicit_self: explicit_self,
    purity: purity,
    decl: fn_decl,
    body: Block,
    id: node_id,
    span: span,
    self_id: node_id,
    vis: visibility,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct _mod {
    view_items: ~[view_item],
    items: ~[@item],
}

// Foreign mods can be named or anonymous
#[deriving(Clone, Eq, Encodable, Decodable,IterBytes)]
pub enum foreign_mod_sort {
    named,
    anonymous,
}

#[deriving(Clone, Eq, Encodable, Decodable,IterBytes)]
pub struct foreign_mod {
    sort: foreign_mod_sort,
    abis: AbiSet,
    view_items: ~[view_item],
    items: ~[@foreign_item],
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct variant_arg {
    ty: Ty,
    id: node_id,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum variant_kind {
    tuple_variant_kind(~[variant_arg]),
    struct_variant_kind(@struct_def),
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct enum_def {
    variants: ~[variant],
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct variant_ {
    name: ident,
    attrs: ~[Attribute],
    kind: variant_kind,
    id: node_id,
    disr_expr: Option<@expr>,
    vis: visibility,
}

pub type variant = spanned<variant_>;

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct path_list_ident_ {
    name: ident,
    id: node_id,
}

pub type path_list_ident = spanned<path_list_ident_>;

pub type view_path = spanned<view_path_>;

#[deriving(Eq, Encodable, Decodable, IterBytes)]
pub enum view_path_ {

    // quux = foo::bar::baz
    //
    // or just
    //
    // foo::bar::baz  (with 'baz =' implicitly on the left)
    view_path_simple(ident, Path, node_id),

    // foo::bar::*
    view_path_glob(Path, node_id),

    // foo::bar::{a,b,c}
    view_path_list(Path, ~[path_list_ident], node_id)
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct view_item {
    node: view_item_,
    attrs: ~[Attribute],
    vis: visibility,
    span: span,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum view_item_ {
    view_item_extern_mod(ident, ~[@MetaItem], node_id),
    view_item_use(~[@view_path]),
}

// Meta-data associated with an item
pub type Attribute = spanned<Attribute_>;

// Distinguishes between Attributes that decorate items and Attributes that
// are contained as statements within items. These two cases need to be
// distinguished for pretty-printing.
#[deriving(Clone, Eq, Encodable, Decodable,IterBytes)]
pub enum AttrStyle {
    AttrOuter,
    AttrInner,
}

// doc-comments are promoted to attributes that have is_sugared_doc = true
#[deriving(Clone, Eq, Encodable, Decodable,IterBytes)]
pub struct Attribute_ {
    style: AttrStyle,
    value: @MetaItem,
    is_sugared_doc: bool,
}

/*
  trait_refs appear in impls.
  resolve maps each trait_ref's ref_id to its defining trait; that's all
  that the ref_id is for. The impl_id maps to the "self type" of this impl.
  If this impl is an item_impl, the impl_id is redundant (it could be the
  same as the impl's node id).
 */
#[deriving(Clone, Eq, Encodable, Decodable,IterBytes)]
pub struct trait_ref {
    path: Path,
    ref_id: node_id,
}

#[deriving(Clone, Eq, Encodable, Decodable,IterBytes)]
pub enum visibility {
    public,
    private,
    inherited,
}

impl visibility {
    pub fn inherit_from(&self, parent_visibility: visibility) -> visibility {
        match self {
            &inherited => parent_visibility,
            &public | &private => *self
        }
    }
}

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub struct struct_field_ {
    kind: struct_field_kind,
    id: node_id,
    ty: Ty,
    attrs: ~[Attribute],
}

pub type struct_field = spanned<struct_field_>;

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub enum struct_field_kind {
    named_field(ident, visibility),
    unnamed_field   // element of a tuple-like struct
}

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub struct struct_def {
    fields: ~[@struct_field], /* fields, not including ctor */
    /* ID of the constructor. This is only used for tuple- or enum-like
     * structs. */
    ctor_id: Option<node_id>
}

/*
  FIXME (#3300): Should allow items to be anonymous. Right now
  we just use dummy names for anon items.
 */
#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub struct item {
    ident: ident,
    attrs: ~[Attribute],
    id: node_id,
    node: item_,
    vis: visibility,
    span: span,
}

#[deriving(Clone, Eq, Encodable, Decodable, IterBytes)]
pub enum item_ {
    item_static(Ty, mutability, @expr),
    item_fn(fn_decl, purity, AbiSet, Generics, Block),
    item_mod(_mod),
    item_foreign_mod(foreign_mod),
    item_ty(Ty, Generics),
    item_enum(enum_def, Generics),
    item_struct(@struct_def, Generics),
    item_trait(Generics, ~[trait_ref], ~[trait_method]),
    item_impl(Generics,
              Option<trait_ref>, // (optional) trait this impl implements
              Ty, // self
              ~[@method]),
    // a macro invocation (which includes macro definition)
    item_mac(mac),
}

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub struct foreign_item {
    ident: ident,
    attrs: ~[Attribute],
    node: foreign_item_,
    id: node_id,
    span: span,
    vis: visibility,
}

#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub enum foreign_item_ {
    foreign_item_fn(fn_decl, purity, Generics),
    foreign_item_static(Ty, /* is_mutbl */ bool),
}

// The data we save and restore about an inlined item or method.  This is not
// part of the AST that we parse from a file, but it becomes part of the tree
// that we trans.
#[deriving(Eq, Encodable, Decodable,IterBytes)]
pub enum inlined_item {
    ii_item(@item),
    ii_method(def_id /* impl id */, bool /* is provided */, @method),
    ii_foreign(@foreign_item),
}

/* hold off on tests ... they appear in a later merge.
#[cfg(test)]
mod test {
    use core::option::{None, Option, Some};
    use core::uint;
    use extra;
    use codemap::*;
    use super::*;


    #[test] fn xorpush_test () {
        let mut s = ~[];
        xorPush(&mut s,14);
        assert_eq!(s,~[14]);
        xorPush(&mut s,14);
        assert_eq!(s,~[]);
        xorPush(&mut s,14);
        assert_eq!(s,~[14]);
        xorPush(&mut s,15);
        assert_eq!(s,~[14,15]);
        xorPush (&mut s,16);
        assert_eq! (s,~[14,15,16]);
        xorPush (&mut s,16);
        assert_eq! (s,~[14,15]);
        xorPush (&mut s,15);
        assert_eq! (s,~[14]);
    }

    #[test] fn test_marksof () {
        let stopname = uints_to_name(&~[12,14,78]);
        assert_eq!(s,~[]);
        xorPush(&mut s,14);
        assert_eq!(s,~[14]);
        xorPush(&mut s,15);
        assert_eq!(s,~[14,15]);
        xorPush (&mut s,16);
        assert_eq! (s,~[14,15,16]);
        xorPush (&mut s,16);
        assert_eq! (s,~[14,15]);
        xorPush (&mut s,15);
        assert_eq! (s,~[14]);
    }

    #[test] fn test_marksof () {
        let stopname = uints_to_name(&~[12,14,78]);
        let name1 = uints_to_name(&~[4,9,7]);
        assert_eq!(marksof (MT,stopname),~[]);
        assert_eq! (marksof (Mark (4,@Mark(98,@MT)),stopname),~[4,98]);
        // does xoring work?
        assert_eq! (marksof (Mark (5, @Mark (5, @Mark (16,@MT))),stopname),
                     ~[16]);
        // does nested xoring work?
        assert_eq! (marksof (Mark (5,
                                    @Mark (10,
                                           @Mark (10,
                                                  @Mark (5,
                                                         @Mark (16,@MT))))),
                              stopname),
                     ~[16]);
        // stop has no effect on marks
        assert_eq! (marksof (Mark (9, @Mark (14, @Mark (12, @MT))),stopname),
                     ~[9,14,12]);
        // rename where stop doesn't match:
        assert_eq! (marksof (Mark (9, @Rename
                                    (name1,
                                     @Mark (4, @MT),
                                     uints_to_name(&~[100,101,102]),
                                     @Mark (14, @MT))),
                              stopname),
                     ~[9,14]);
        // rename where stop does match
        ;
        assert_eq! (marksof (Mark(9, @Rename (name1,
                                               @Mark (4, @MT),
                                               stopname,
                                               @Mark (14, @MT))),
                              stopname),
                     ~[9]);
    }

    // are ASTs encodable?
    #[test] fn check_asts_encodable() {
        let bogus_span = span {lo:BytePos(10),
                               hi:BytePos(20),
                               expn_info:None};
        let e : crate =
            spanned{
            node: crate_{
                module: _mod {view_items: ~[], items: ~[]},
                attrs: ~[],
                config: ~[]
            },
            span: bogus_span};
        // doesn't matter which encoder we use....
        let _f = (@e as @extra::serialize::Encodable<extra::json::Encoder>);
    }


}

*/
//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
