// The Rust abstract syntax tree.

import codemap::{span, filename};
import std::serialization::{serializer,
                            deserializer,
                            serialize_option,
                            deserialize_option,
                            serialize_uint,
                            deserialize_uint,
                            serialize_int,
                            deserialize_int,
                            serialize_i64,
                            deserialize_i64,
                            serialize_u64,
                            deserialize_u64,
                            serialize_str,
                            deserialize_str,
                            serialize_bool,
                            deserialize_bool};
import parse::token;

/* Note #1972 -- spans are serialized but not deserialized */
fn serialize_span<S>(_s: S, _v: span) {
}

fn deserialize_span<D>(_d: D) -> span {
    ast_util::dummy_sp()
}

#[auto_serialize]
type spanned<T> = {node: T, span: span};

#[auto_serialize]
type ident = @~str;

// Functions may or may not have names.
#[auto_serialize]
type fn_ident = option<ident>;

#[auto_serialize]
type path = {span: span,
             global: bool,
             idents: ~[ident],
             rp: option<@region>,
             types: ~[@ty]};

#[auto_serialize]
type crate_num = int;

#[auto_serialize]
type node_id = int;

#[auto_serialize]
type def_id = {crate: crate_num, node: node_id};

const local_crate: crate_num = 0;
const crate_node_id: node_id = 0;

#[auto_serialize]
enum ty_param_bound {
    bound_copy,
    bound_send,
    bound_const,
    bound_owned,
    bound_trait(@ty),
}

#[auto_serialize]
type ty_param = {ident: ident, id: node_id, bounds: @~[ty_param_bound]};

#[auto_serialize]
enum def {
    def_fn(def_id, purity),
    def_self(node_id),
    def_mod(def_id),
    def_foreign_mod(def_id),
    def_const(def_id),
    def_arg(node_id, mode),
    def_local(node_id, bool /* is_mutbl */),
    def_variant(def_id /* enum */, def_id /* variant */),
    def_ty(def_id),
    def_prim_ty(prim_ty),
    def_ty_param(def_id, uint),
    def_binding(node_id),
    def_use(def_id),
    def_upvar(node_id /* local id of closed over var */,
              @def    /* closed over def */,
              node_id /* expr node that creates the closure */),
    def_class(def_id, bool /* has constructor */),
    def_typaram_binder(node_id), /* class, impl or trait that has ty params */
    def_region(node_id)
}

// The set of meta_items that define the compilation environment of the crate,
// used to drive conditional compilation
type crate_cfg = ~[@meta_item];

type crate = spanned<crate_>;

type crate_ =
    {directives: ~[@crate_directive],
     module: _mod,
     attrs: ~[attribute],
     config: crate_cfg};

enum crate_directive_ {
    cdir_src_mod(ident, ~[attribute]),
    cdir_dir_mod(ident, ~[@crate_directive], ~[attribute]),

    // NB: cdir_view_item is *not* processed by the rest of the compiler, the
    // attached view_items are sunk into the crate's module during parsing,
    // and processed (resolved, imported, etc.) there. This enum-variant
    // exists only to preserve the view items in order in case we decide to
    // pretty-print crates in the future.
    cdir_view_item(@view_item),

    cdir_syntax(@path),
}

type crate_directive = spanned<crate_directive_>;

#[auto_serialize]
type meta_item = spanned<meta_item_>;

#[auto_serialize]
enum meta_item_ {
    meta_word(ident),
    meta_list(ident, ~[@meta_item]),
    meta_name_value(ident, lit),
}

#[auto_serialize]
type blk = spanned<blk_>;

#[auto_serialize]
type blk_ = {view_items: ~[@view_item],
             stmts: ~[@stmt],
             expr: option<@expr>,
             id: node_id,
             rules: blk_check_mode};

#[auto_serialize]
type pat = {id: node_id, node: pat_, span: span};

#[auto_serialize]
type field_pat = {ident: ident, pat: @pat};

#[auto_serialize]
enum binding_mode {
    bind_by_value,
    bind_by_ref
}

#[auto_serialize]
enum pat_ {
    pat_wild,
    // A pat_ident may either be a new bound variable,
    // or a nullary enum (in which case the second field
    // is none).
    // In the nullary enum case, the parser can't determine
    // which it is. The resolver determines this, and
    // records this pattern's node_id in an auxiliary
    // set (of "pat_idents that refer to nullary enums")
    pat_ident(binding_mode, @path, option<@pat>),
    pat_enum(@path, option<~[@pat]>), // "none" means a * pattern where
                                  // we don't bind the fields to names
    pat_rec(~[field_pat], bool),
    pat_tup(~[@pat]),
    pat_box(@pat),
    pat_uniq(@pat),
    pat_lit(@expr),
    pat_range(@expr, @expr),
}

#[auto_serialize]
enum mutability { m_mutbl, m_imm, m_const, }

#[auto_serialize]
enum proto {
    proto_bare,    // foreign fn
    proto_any,     // fn
    proto_uniq,    // fn~
    proto_box,     // fn@
    proto_block,   // fn&
}

#[auto_serialize]
enum vstore {
    // FIXME (#2112): Change uint to @expr (actually only constant exprs)
    vstore_fixed(option<uint>),   // [1,2,3,4]/_ or 4
    vstore_uniq,                  // ~[1,2,3,4]
    vstore_box,                   // @[1,2,3,4]
    vstore_slice(@region)         // &[1,2,3,4](foo)?
}

pure fn is_blockish(p: ast::proto) -> bool {
    alt p {
      proto_any | proto_block { true }
      proto_bare | proto_uniq | proto_box { false }
    }
}

#[auto_serialize]
enum binop {
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

#[auto_serialize]
enum unop {
    box(mutability),
    uniq(mutability),
    deref, not, neg
}

// Generally, after typeck you can get the inferred value
// using ty::resolved_T(...).
#[auto_serialize]
enum inferable<T> {
    expl(T), infer(node_id)
}

// "resolved" mode: the real modes.
#[auto_serialize]
enum rmode { by_ref, by_val, by_mutbl_ref, by_move, by_copy }

// inferable mode.
#[auto_serialize]
type mode = inferable<rmode>;

#[auto_serialize]
type stmt = spanned<stmt_>;

#[auto_serialize]
enum stmt_ {
    stmt_decl(@decl, node_id),

    // expr without trailing semi-colon (must have unit type):
    stmt_expr(@expr, node_id),

    // expr with trailing semi-colon (may have any type):
    stmt_semi(@expr, node_id),
}

#[auto_serialize]
enum init_op { init_assign, init_move, }

#[auto_serialize]
type initializer = {op: init_op, expr: @expr};

// FIXME (pending discussion of #1697, #2178...): local should really be
// a refinement on pat.
#[auto_serialize]
type local_ =  {is_mutbl: bool, ty: @ty, pat: @pat,
                init: option<initializer>, id: node_id};

#[auto_serialize]
type local = spanned<local_>;

#[auto_serialize]
type decl = spanned<decl_>;

#[auto_serialize]
enum decl_ { decl_local(~[@local]), decl_item(@item), }

#[auto_serialize]
type arm = {pats: ~[@pat], guard: option<@expr>, body: blk};

#[auto_serialize]
type field_ = {mutbl: mutability, ident: ident, expr: @expr};

#[auto_serialize]
type field = spanned<field_>;

#[auto_serialize]
enum blk_check_mode { default_blk, unchecked_blk, unsafe_blk, }

#[auto_serialize]
type expr = {id: node_id, callee_id: node_id, node: expr_, span: span};
// Extra node ID is only used for index, assign_op, unary, binary

#[auto_serialize]
enum alt_mode { alt_check, alt_exhaustive, }

#[auto_serialize]
enum expr_ {
    expr_vstore(@expr, vstore),
    expr_vec(~[@expr], mutability),
    expr_rec(~[field], option<@expr>),
    expr_call(@expr, ~[@expr], bool), // True iff last argument is a block
    expr_tup(~[@expr]),
    expr_binary(binop, @expr, @expr),
    expr_unary(unop, @expr),
    expr_lit(@lit),
    expr_cast(@expr, @ty),
    expr_if(@expr, blk, option<@expr>),
    expr_while(@expr, blk),
    /* Conditionless loop (can be exited with break, cont, ret, or fail)
       Same semantics as while(true) { body }, but typestate knows that the
       (implicit) condition is always true. */
    expr_loop(blk),
    expr_alt(@expr, ~[arm], alt_mode),
    expr_fn(proto, fn_decl, blk, capture_clause),
    expr_fn_block(fn_decl, blk, capture_clause),
    // Inner expr is always an expr_fn_block. We need the wrapping node to
    // easily type this (a function returning nil on the inside but bool on
    // the outside).
    expr_loop_body(@expr),
    // Like expr_loop_body but for 'do' blocks
    expr_do_body(@expr),
    expr_block(blk),

    expr_copy(@expr),
    expr_move(@expr, @expr),
    expr_unary_move(@expr),
    expr_assign(@expr, @expr),
    expr_swap(@expr, @expr),
    expr_assign_op(binop, @expr, @expr),
    expr_field(@expr, ident, ~[@ty]),
    expr_index(@expr, @expr),
    expr_path(@path),
    expr_addr_of(mutability, @expr),
    expr_fail(option<@expr>),
    expr_break,
    expr_again,
    expr_ret(option<@expr>),
    expr_log(int, @expr, @expr),

    expr_new(/* arena */ @expr,
             /* id for the alloc() call */ node_id,
             /* value */ @expr),

    /* just an assert */
    expr_assert(@expr),

    expr_mac(mac),

    // A struct literal expression.
    //
    // XXX: Add functional record update.
    expr_struct(@path, ~[field])
}

#[auto_serialize]
type capture_item = @{
    id: int,
    is_move: bool,
    name: ident, // Currently, can only capture a local var.
    span: span
};

#[auto_serialize]
type capture_clause = @~[capture_item];

//
// When the main rust parser encounters a syntax-extension invocation, it
// parses the arguments to the invocation as a token-tree. This is a very
// loose structure, such that all sorts of different AST-fragments can
// be passed to syntax extensions using a uniform type.
//
// If the syntax extension is an MBE macro, it will attempt to match its
// LHS "matchers" against the provided token tree, and if it finds a
// match, will transcribe the RHS token tree, splicing in any captured
// earley_parser::matched_nonterminals into the tt_nonterminals it finds.
//
// The RHS of an MBE macro is the only place a tt_nonterminal or tt_seq
// makes any real sense. You could write them elsewhere but nothing
// else knows what to do with them, so you'll probably get a syntax
// error.
//
#[auto_serialize]
#[doc="For macro invocations; parsing is delegated to the macro"]
enum token_tree {
    tt_tok(span, token::token),
    tt_delim(~[token_tree]),
    // These only make sense for right-hand-sides of MBE macros
    tt_seq(span, ~[token_tree], option<token::token>, bool),
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
#[auto_serialize]
type matcher = spanned<matcher_>;

#[auto_serialize]
enum matcher_ {
    // match one token
    match_tok(token::token),
    // match repetitions of a sequence: body, separator, zero ok?,
    // lo, hi position-in-match-array used:
    match_seq(~[matcher], option<token::token>, bool, uint, uint),
    // parse a Rust NT: name to bind, name of NT, position in match array:
    match_nonterminal(ident, ident, uint)
}

#[auto_serialize]
type mac = spanned<mac_>;

#[auto_serialize]
type mac_arg = option<@expr>;

#[auto_serialize]
type mac_body_ = {span: span};

#[auto_serialize]
type mac_body = option<mac_body_>;

#[auto_serialize]
enum mac_ {
    mac_invoc(@path, mac_arg, mac_body), // old macro-invocation
    mac_invoc_tt(@path,~[token_tree]),   // new macro-invocation
    mac_ellipsis,                        // old pattern-match (obsolete)

    // the span is used by the quoter/anti-quoter ...
    mac_aq(span /* span of quote */, @expr), // anti-quote
    mac_var(uint)
}

#[auto_serialize]
type lit = spanned<lit_>;

#[auto_serialize]
enum lit_ {
    lit_str(@~str),
    lit_int(i64, int_ty),
    lit_uint(u64, uint_ty),
    lit_int_unsuffixed(i64),
    lit_float(@~str, float_ty),
    lit_nil,
    lit_bool(bool),
}

// NB: If you change this, you'll probably want to change the corresponding
// type structure in middle/ty.rs as well.
#[auto_serialize]
type mt = {ty: @ty, mutbl: mutability};

#[auto_serialize]
type ty_field_ = {ident: ident, mt: mt};

#[auto_serialize]
type ty_field = spanned<ty_field_>;

#[auto_serialize]
type ty_method = {ident: ident, attrs: ~[attribute],
                  decl: fn_decl, tps: ~[ty_param], self_ty: self_ty,
                  id: node_id, span: span};

#[auto_serialize]
// A trait method is either required (meaning it doesn't have an
// implementation, just a signature) or provided (meaning it has a default
// implementation).
enum trait_method {
    required(ty_method),
    provided(@method),
}

#[auto_serialize]
enum int_ty { ty_i, ty_char, ty_i8, ty_i16, ty_i32, ty_i64, }

#[auto_serialize]
enum uint_ty { ty_u, ty_u8, ty_u16, ty_u32, ty_u64, }

#[auto_serialize]
enum float_ty { ty_f, ty_f32, ty_f64, }

#[auto_serialize]
type ty = {id: node_id, node: ty_, span: span};

// Not represented directly in the AST, referred to by name through a ty_path.
#[auto_serialize]
enum prim_ty {
    ty_int(int_ty),
    ty_uint(uint_ty),
    ty_float(float_ty),
    ty_str,
    ty_bool,
}

#[auto_serialize]
type region = {id: node_id, node: region_};

#[auto_serialize]
enum region_ { re_anon, re_named(ident) }

#[auto_serialize]
enum ty_ {
    ty_nil,
    ty_bot, /* bottom type */
    ty_box(mt),
    ty_uniq(mt),
    ty_vec(mt),
    ty_ptr(mt),
    ty_rptr(@region, mt),
    ty_rec(~[ty_field]),
    ty_fn(proto, fn_decl),
    ty_tup(~[@ty]),
    ty_path(@path, node_id),
    ty_fixed_length(@ty, option<uint>),
    ty_mac(mac),
    // ty_infer means the type should be inferred instead of it having been
    // specified. This should only appear at the "top level" of a type and not
    // nested in one.
    ty_infer,
}

#[auto_serialize]
type arg = {mode: mode, ty: @ty, ident: ident, id: node_id};

#[auto_serialize]
type fn_decl =
    {inputs: ~[arg],
     output: @ty,
     purity: purity,
     cf: ret_style};

#[auto_serialize]
enum purity {
    pure_fn, // declared with "pure fn"
    unsafe_fn, // declared with "unsafe fn"
    impure_fn, // declared with "fn"
    extern_fn, // declared with "extern fn"
}

#[auto_serialize]
enum ret_style {
    noreturn, // functions with return type _|_ that always
              // raise an error or exit (i.e. never return to the caller)
    return_val, // everything else
}

#[auto_serialize]
enum self_ty_ {
    sty_by_ref,                         // old by-reference self: ``
    sty_value,                          // by-value self: `self`
    sty_region(@region, mutability),    // by-region self: `&self`
    sty_box(mutability),                // by-managed-pointer self: `@self`
    sty_uniq(mutability)                // by-unique-pointer self: `~self`
}

#[auto_serialize]
type self_ty = spanned<self_ty_>;

#[auto_serialize]
type method = {ident: ident, attrs: ~[attribute],
               tps: ~[ty_param], self_ty: self_ty, decl: fn_decl, body: blk,
               id: node_id, span: span, self_id: node_id,
               vis: visibility};  // always public, unless it's a
                                  // class method

#[auto_serialize]
type _mod = {view_items: ~[@view_item], items: ~[@item]};

#[auto_serialize]
enum foreign_abi {
    foreign_abi_rust_intrinsic,
    foreign_abi_cdecl,
    foreign_abi_stdcall,
}

#[auto_serialize]
type foreign_mod =
    {view_items: ~[@view_item],
     items: ~[@foreign_item]};

#[auto_serialize]
type variant_arg = {ty: @ty, id: node_id};

#[auto_serialize]
type variant_ = {name: ident, attrs: ~[attribute], args: ~[variant_arg],
                 id: node_id, disr_expr: option<@expr>, vis: visibility};

#[auto_serialize]
type variant = spanned<variant_>;

#[auto_serialize]
type path_list_ident_ = {name: ident, id: node_id};

#[auto_serialize]
type path_list_ident = spanned<path_list_ident_>;

#[auto_serialize]
type view_path = spanned<view_path_>;

#[auto_serialize]
enum view_path_ {

    // quux = foo::bar::baz
    //
    // or just
    //
    // foo::bar::baz  (with 'baz =' implicitly on the left)
    view_path_simple(ident, @path, node_id),

    // foo::bar::*
    view_path_glob(@path, node_id),

    // foo::bar::{a,b,c}
    view_path_list(@path, ~[path_list_ident], node_id)
}

#[auto_serialize]
type view_item = {node: view_item_, attrs: ~[attribute],
                  vis: visibility, span: span};

#[auto_serialize]
enum view_item_ {
    view_item_use(ident, ~[@meta_item], node_id),
    view_item_import(~[@view_path]),
    view_item_export(~[@view_path])
}

// Meta-data associated with an item
#[auto_serialize]
type attribute = spanned<attribute_>;

// Distinguishes between attributes that decorate items and attributes that
// are contained as statements within items. These two cases need to be
// distinguished for pretty-printing.
#[auto_serialize]
enum attr_style { attr_outer, attr_inner, }

// doc-comments are promoted to attributes that have is_sugared_doc = true
#[auto_serialize]
type attribute_ = {style: attr_style, value: meta_item, is_sugared_doc: bool};

/*
  trait_refs appear in both impls and in classes that implement traits.
  resolve maps each trait_ref's ref_id to its defining trait; that's all
  that the ref_id is for. The impl_id maps to the "self type" of this impl.
  If this impl is an item_impl, the impl_id is redundant (it could be the
  same as the impl's node id). If this impl is actually an impl_class, then
  conceptually, the impl_id stands in for the pair of (this class, this
  trait)
 */
#[auto_serialize]
type trait_ref = {path: @path, ref_id: node_id, impl_id: node_id};

#[auto_serialize]
enum visibility { public, private }

#[auto_serialize]
type item = {ident: ident, attrs: ~[attribute],
             id: node_id, node: item_,
             vis: visibility, span: span};

#[auto_serialize]
enum item_ {
    item_const(@ty, @expr),
    item_fn(fn_decl, ~[ty_param], blk),
    item_mod(_mod),
    item_foreign_mod(foreign_mod),
    item_ty(@ty, ~[ty_param]),
    item_enum(~[variant], ~[ty_param]),
    item_class(~[ty_param], /* ty params for class */
               ~[@trait_ref],   /* traits this class implements */
               ~[@class_member], /* methods, etc. */
                               /* (not including ctor or dtor) */
               /* ctor is optional, and will soon go away */
               option<class_ctor>,
               /* dtor is optional */
               option<class_dtor>
               ),
    item_trait(~[ty_param], ~[trait_method]),
    item_impl(~[ty_param],
              ~[@trait_ref], /* traits this impl implements */
              @ty, /* self */
              ~[@method]),
    item_mac(mac),
}

#[auto_serialize]
type class_member = spanned<class_member_>;

#[auto_serialize]
enum class_member_ {
    instance_var(ident, @ty, class_mutability, node_id, visibility),
    class_method(@method)
}

#[auto_serialize]
enum class_mutability { class_mutable, class_immutable }

#[auto_serialize]
type class_ctor = spanned<class_ctor_>;

#[auto_serialize]
type class_ctor_ = {id: node_id,
                    attrs: ~[attribute],
                    self_id: node_id,
                    dec: fn_decl,
                    body: blk};

#[auto_serialize]
type class_dtor = spanned<class_dtor_>;

#[auto_serialize]
type class_dtor_ = {id: node_id,
                    attrs: ~[attribute],
                    self_id: node_id,
                    body: blk};

#[auto_serialize]
type foreign_item =
    {ident: ident,
     attrs: ~[attribute],
     node: foreign_item_,
     id: node_id,
     span: span};

#[auto_serialize]
enum foreign_item_ {
    foreign_item_fn(fn_decl, ~[ty_param]),
}

// The data we save and restore about an inlined item or method.  This is not
// part of the AST that we parse from a file, but it becomes part of the tree
// that we trans.
#[auto_serialize]
enum inlined_item {
    ii_item(@item),
    ii_method(def_id /* impl id */, @method),
    ii_foreign(@foreign_item),
    ii_ctor(class_ctor, ident, ~[ty_param], def_id /* parent id */),
    ii_dtor(class_dtor, ident, ~[ty_param], def_id /* parent id */)
}

// Convenience functions

pure fn simple_path(id: ident, span: span) -> @path {
    @{span: span,
      global: false,
      idents: ~[id],
      rp: none,
      types: ~[]}
}

pure fn empty_span() -> span {
    {lo: 0, hi: 0, expn_info: none}
}

// Convenience implementations

// Remove after snapshot!
trait path_concat {
    pure fn +(&&id: ident) -> @path;
}

// Remove after snapshot!
impl methods of path_concat for ident {
    pure fn +(&&id: ident) -> @path {
        simple_path(self, empty_span()) + id
    }
}

impl methods of ops::add<ident,@path> for ident {
    pure fn add(&&id: ident) -> @path {
        simple_path(self, empty_span()) + id
    }
}

// Remove after snapshot!
impl methods of path_concat for @path {
    pure fn +(&&id: ident) -> @path {
        @{
            idents: vec::append_one(self.idents, id)
            with *self
        }
    }
}

impl methods of ops::add<ident,@path> for @path {
    pure fn add(&&id: ident) -> @path {
        @{
            idents: vec::append_one(self.idents, id)
            with *self
        }
    }
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
