import std::{ufind, map, smallintmap};
import std::map::hashmap;
import driver::session;
import session::session;
import syntax::ast;
import syntax::ast::*;
import syntax::ast_util;
import syntax::codemap::span;
import metadata::csearch;
import util::common::*;
import util::ppaux::ty_to_str;
import util::ppaux::ty_constr_to_str;
import syntax::print::pprust::*;

export node_id_to_type;
export node_id_to_type_params;
export arg;
export args_eq;
export ast_constr_to_constr;
export block_ty;
export constr;
export constr_general;
export constr_table;
export count_ty_params;
export ctxt;
export def_has_ty_params;
export expr_has_ty_params;
export expr_ty;
export expr_ty_params_and_ty;
export expr_is_lval;
export fold_ty;
export field;
export field_idx;
export get_field;
export get_fields;
export fm_general;
export get_element_type;
export is_binopable;
export is_pred_ty;
export lookup_item_type;
export method;
export method_idx;
export mk_class;
export mk_ctxt;
export mk_with_id, type_def_id;
export mt;
export node_type_table;
export pat_ty;
export sequence_element_type;
export sort_methods;
export stmt_node_id;
export sty;
export substitute_type_params;
export t;
export new_ty_hash;
export enum_variants, substd_enum_variants;
export iface_methods, store_iface_methods, impl_iface;
export enum_variant_with_id;
export ty_param_bounds_and_ty;
export ty_bool, mk_bool, type_is_bool;
export ty_bot, mk_bot, type_is_bot;
export ty_box, mk_box, mk_imm_box, type_is_box, type_is_boxed;
export ty_constr, mk_constr;
export ty_opaque_closure_ptr, mk_opaque_closure_ptr;
export ty_opaque_box, mk_opaque_box;
export ty_constr_arg;
export ty_float, mk_float, mk_mach_float, type_is_fp;
export ty_fn, fn_ty, mk_fn;
export ty_fn_proto, ty_fn_ret, ty_fn_ret_style;
export ty_int, mk_int, mk_mach_int, mk_char;
export ty_str, mk_str, type_is_str;
export ty_vec, mk_vec, type_is_vec;
export ty_nil, mk_nil, type_is_nil;
export ty_iface, mk_iface;
export ty_res, mk_res;
export ty_param, mk_param;
export ty_ptr, mk_ptr, mk_mut_ptr, type_is_unsafe_ptr;
export ty_rec, mk_rec;
export ty_enum, mk_enum, type_is_enum;
export ty_tup, mk_tup;
export ty_send_type, mk_send_type;
export ty_type, mk_type;
export ty_uint, mk_uint, mk_mach_uint;
export ty_uniq, mk_uniq, mk_imm_uniq, type_is_unique_box;
export ty_var, mk_var;
export ty_self, mk_self;
export get, type_has_params, type_has_vars, type_id;
export same_type;
export ty_var_id;
export ty_fn_args;
export type_constr;
export kind, kind_sendable, kind_copyable, kind_noncopyable;
export kind_can_be_copied, kind_can_be_sent, proto_kind, kind_lteq, type_kind;
export type_err;
export type_err_to_str;
export type_has_dynamic_size;
export type_needs_drop;
export type_allows_implicit_copy;
export type_is_integral;
export type_is_numeric;
export type_is_pod;
export type_is_scalar;
export type_is_immediate;
export type_is_sequence;
export type_is_signed;
export type_is_structural;
export type_is_copyable;
export type_is_tup_like;
export type_is_unique;
export type_is_c_like_enum;
export type_structurally_contains;
export type_structurally_contains_uniques;
export type_autoderef;
export type_param;
export canon_mode;
export resolved_mode;
export arg_mode;
export unify_mode;
export set_default_mode;
export unify;
export variant_info;
export walk_ty;
export occurs_check_fails;
export closure_kind;
export ck_block;
export ck_box;
export ck_uniq;
export param_bound, param_bounds, bound_copy, bound_send, bound_iface;
export param_bounds_to_kind;
export default_arg_mode_for_ty;
export item_path;

// Data types

// Note: after typeck, you should use resolved_mode() to convert this mode
// into an rmode, which will take into account the results of mode inference.
type arg = {mode: ast::mode, ty: t};

type field = {ident: ast::ident, mt: mt};

type param_bounds = @[param_bound];

type method = {ident: ast::ident,
               tps: @[param_bounds],
               fty: fn_ty,
               purity: ast::purity};

type constr_table = hashmap<ast::node_id, [constr]>;

type mt = {ty: t, mutbl: ast::mutability};


// Contains information needed to resolve types and (in the future) look up
// the types of AST nodes.
type creader_cache = hashmap<{cnum: int, pos: uint, len: uint}, t>;

type intern_key = {struct: sty, o_def_id: option<ast::def_id>};

type ctxt =
    @{interner: hashmap<intern_key, t_box>,
      mutable next_id: uint,
      sess: session::session,
      def_map: resolve::def_map,
      node_types: node_type_table,
      node_type_substs: hashmap<node_id, [t]>,
      items: ast_map::map,
      freevars: freevars::freevar_map,
      tcache: type_cache,
      rcache: creader_cache,
      short_names_cache: hashmap<t, @str>,
      needs_drop_cache: hashmap<t, bool>,
      kind_cache: hashmap<t, kind>,
      ast_ty_to_ty_cache: hashmap<@ast::ty, option<t>>,
      enum_var_cache: hashmap<def_id, @[variant_info]>,
      iface_method_cache: hashmap<def_id, @[method]>,
      ty_param_bounds: hashmap<ast::node_id, param_bounds>,
      inferred_modes: hashmap<ast::node_id, ast::mode>};

type t_box = @{struct: sty,
               id: uint,
               has_params: bool,
               has_vars: bool,
               o_def_id: option<ast::def_id>};

// To reduce refcounting cost, we're representing types as unsafe pointers
// throughout the compiler. These are simply casted t_box values. Use ty::get
// to cast them back to a box. (Without the cast, compiler performance suffers
// ~15%.) This does mean that a t value relies on the ctxt to keep its box
// alive, and using ty::get is unsafe when the ctxt is no longer alive.
enum t_opaque {}
type t = *t_opaque;

pure fn get(t: t) -> t_box unsafe {
    let t2 = unsafe::reinterpret_cast::<t, t_box>(t);
    let t3 = t2;
    unsafe::leak(t2);
    t3
}

fn type_has_params(t: t) -> bool { get(t).has_params }
fn type_has_vars(t: t) -> bool { get(t).has_vars }
fn type_def_id(t: t) -> option<ast::def_id> { get(t).o_def_id }
fn type_id(t: t) -> uint { get(t).id }

enum closure_kind {
    ck_block,
    ck_box,
    ck_uniq,
}

type fn_ty = {proto: ast::proto,
              inputs: [arg],
              output: t,
              ret_style: ret_style,
              constraints: [@constr]};

// NB: If you change this, you'll probably want to change the corresponding
// AST structure in front/ast::rs as well.
enum sty {
    ty_nil,
    ty_bot,
    ty_bool,
    ty_int(ast::int_ty),
    ty_uint(ast::uint_ty),
    ty_float(ast::float_ty),
    ty_str,
    ty_enum(def_id, [t]),
    ty_box(mt),
    ty_uniq(mt),
    ty_vec(mt),
    ty_ptr(mt),
    ty_rec([field]),
    ty_fn(fn_ty),
    ty_iface(def_id, [t]),
    ty_class(def_id, [t]),
    ty_res(def_id, t, [t]),
    ty_tup([t]),

    ty_var(int), // type variable during typechecking
    ty_param(uint, def_id), // type parameter
    ty_self([t]), // interface method self type

    ty_type, // type_desc*
    ty_send_type, // type_desc* that has been cloned into exchange heap
    ty_opaque_box, // used by monomorphizer to represend any @ box
    ty_constr(t, [@type_constr]),
    ty_opaque_closure_ptr(closure_kind), // ptr to env for fn, fn@, fn~
}

// In the middle end, constraints have a def_id attached, referring
// to the definition of the operator in the constraint.
type constr_general<ARG> = spanned<constr_general_<ARG, def_id>>;
type type_constr = constr_general<@path>;
type constr = constr_general<uint>;

// Data structures used in type unification
enum type_err {
    terr_mismatch,
    terr_ret_style_mismatch(ast::ret_style, ast::ret_style),
    terr_box_mutability,
    terr_vec_mutability,
    terr_tuple_size(uint, uint),
    terr_record_size(uint, uint),
    terr_record_mutability,
    terr_record_fields(ast::ident, ast::ident),
    terr_arg_count,
    terr_mode_mismatch(mode, mode),
    terr_constr_len(uint, uint),
    terr_constr_mismatch(@type_constr, @type_constr),
}

enum param_bound {
    bound_copy,
    bound_send,
    bound_iface(t),
}

fn param_bounds_to_kind(bounds: param_bounds) -> kind {
    let kind = kind_noncopyable;
    for bound in *bounds {
        alt bound {
          bound_copy {
            if kind != kind_sendable { kind = kind_copyable; }
          }
          bound_send { kind = kind_sendable; }
          _ {}
        }
    }
    kind
}

type ty_param_bounds_and_ty = {bounds: @[param_bounds], ty: t};

type type_cache = hashmap<ast::def_id, ty_param_bounds_and_ty>;

type node_type_table = @smallintmap::smallintmap<t>;

fn mk_rcache() -> creader_cache {
    type val = {cnum: int, pos: uint, len: uint};
    fn hash_cache_entry(k: val) -> uint {
        ret (k.cnum as uint) + k.pos + k.len;
    }
    fn eq_cache_entries(a: val, b: val) -> bool {
        ret a.cnum == b.cnum && a.pos == b.pos && a.len == b.len;
    }
    ret map::mk_hashmap(hash_cache_entry, eq_cache_entries);
}

fn new_ty_hash<V: copy>() -> map::hashmap<t, V> {
    map::mk_hashmap({|&&t: t| type_id(t)},
                    {|&&a: t, &&b: t| type_id(a) == type_id(b)})
}

fn mk_ctxt(s: session::session, dm: resolve::def_map, amap: ast_map::map,
           freevars: freevars::freevar_map) -> ctxt {
    let interner = map::mk_hashmap({|&&k: intern_key|
        hash_type_structure(k.struct) +
            option::maybe(0u, k.o_def_id, ast_util::hash_def_id)
    }, {|&&a, &&b| a == b});
    @{interner: interner,
      mutable next_id: 0u,
      sess: s,
      def_map: dm,
      node_types: @smallintmap::mk(),
      node_type_substs: map::new_int_hash(),
      items: amap,
      freevars: freevars,
      tcache: new_def_hash(),
      rcache: mk_rcache(),
      short_names_cache: new_ty_hash(),
      needs_drop_cache: new_ty_hash(),
      kind_cache: new_ty_hash(),
      ast_ty_to_ty_cache: map::mk_hashmap(ast_util::hash_ty, ast_util::eq_ty),
      enum_var_cache: new_def_hash(),
      iface_method_cache: new_def_hash(),
      ty_param_bounds: map::new_int_hash(),
      inferred_modes: map::new_int_hash()}
}


// Type constructors
fn mk_t(cx: ctxt, st: sty) -> t { mk_t_with_id(cx, st, none) }

// Interns a type/name combination, stores the resulting box in cx.interner,
// and returns the box as cast to an unsafe ptr (see comments for t above).
fn mk_t_with_id(cx: ctxt, st: sty, o_def_id: option<ast::def_id>) -> t {
    let key = {struct: st, o_def_id: o_def_id};
    alt cx.interner.find(key) {
      some(t) { unsafe { ret unsafe::reinterpret_cast(t); } }
      _ {}
    }
    let has_params = false, has_vars = false;
    fn derive_flags(&has_params: bool, &has_vars: bool, tt: t) {
        let t = get(tt);
        has_params |= t.has_params;
        has_vars |= t.has_vars;
    }
    alt st {
      ty_nil | ty_bot | ty_bool | ty_int(_) | ty_float(_) | ty_uint(_) |
      ty_str | ty_type | ty_send_type | ty_opaque_closure_ptr(_) |
      ty_opaque_box {}
      ty_param(_, _) { has_params = true; }
      ty_var(_) | ty_self(_) { has_vars = true; }
      ty_enum(_, tys) | ty_iface(_, tys) | ty_class(_, tys) {
        for tt in tys { derive_flags(has_params, has_vars, tt); }
      }
      ty_box(m) | ty_uniq(m) | ty_vec(m) | ty_ptr(m) {
        derive_flags(has_params, has_vars, m.ty);
      }
      ty_rec(flds) {
        for f in flds { derive_flags(has_params, has_vars, f.mt.ty); }
      }
      ty_tup(ts) {
        for tt in ts { derive_flags(has_params, has_vars, tt); }
      }
      ty_fn(f) {
        for a in f.inputs { derive_flags(has_params, has_vars, a.ty); }
        derive_flags(has_params, has_vars, f.output);
      }
      ty_res(_, tt, tps) {
        derive_flags(has_params, has_vars, tt);
        for tt in tps { derive_flags(has_params, has_vars, tt); }
      }
      ty_constr(tt, _) {
        derive_flags(has_params, has_vars, tt);
      }
    }
    let t = @{struct: st,
              id: cx.next_id,
              has_params: has_params,
              has_vars: has_vars,
              o_def_id: o_def_id};
    cx.interner.insert(key, t);
    cx.next_id += 1u;
    unsafe { unsafe::reinterpret_cast(t) }
}

fn mk_nil(cx: ctxt) -> t { mk_t(cx, ty_nil) }

fn mk_bot(cx: ctxt) -> t { mk_t(cx, ty_bot) }

fn mk_bool(cx: ctxt) -> t { mk_t(cx, ty_bool) }

fn mk_int(cx: ctxt) -> t { mk_t(cx, ty_int(ast::ty_i)) }

fn mk_float(cx: ctxt) -> t { mk_t(cx, ty_float(ast::ty_f)) }

fn mk_uint(cx: ctxt) -> t { mk_t(cx, ty_uint(ast::ty_u)) }

fn mk_mach_int(cx: ctxt, tm: ast::int_ty) -> t { mk_t(cx, ty_int(tm)) }

fn mk_mach_uint(cx: ctxt, tm: ast::uint_ty) -> t { mk_t(cx, ty_uint(tm)) }

fn mk_mach_float(cx: ctxt, tm: ast::float_ty) -> t { mk_t(cx, ty_float(tm)) }

fn mk_char(cx: ctxt) -> t { mk_t(cx, ty_int(ast::ty_char)) }

fn mk_str(cx: ctxt) -> t { mk_t(cx, ty_str) }

fn mk_enum(cx: ctxt, did: ast::def_id, tys: [t]) -> t {
    mk_t(cx, ty_enum(did, tys))
}

fn mk_box(cx: ctxt, tm: mt) -> t { mk_t(cx, ty_box(tm)) }

fn mk_imm_box(cx: ctxt, ty: t) -> t { mk_box(cx, {ty: ty,
                                                  mutbl: ast::m_imm}) }

fn mk_uniq(cx: ctxt, tm: mt) -> t { mk_t(cx, ty_uniq(tm)) }

fn mk_imm_uniq(cx: ctxt, ty: t) -> t { mk_uniq(cx, {ty: ty,
                                                    mutbl: ast::m_imm}) }

fn mk_ptr(cx: ctxt, tm: mt) -> t { mk_t(cx, ty_ptr(tm)) }

fn mk_mut_ptr(cx: ctxt, ty: t) -> t { mk_ptr(cx, {ty: ty,
                                                  mutbl: ast::m_mutbl}) }

fn mk_vec(cx: ctxt, tm: mt) -> t { mk_t(cx, ty_vec(tm)) }

fn mk_rec(cx: ctxt, fs: [field]) -> t { mk_t(cx, ty_rec(fs)) }

fn mk_constr(cx: ctxt, t: t, cs: [@type_constr]) -> t {
    mk_t(cx, ty_constr(t, cs))
}

fn mk_tup(cx: ctxt, ts: [t]) -> t { mk_t(cx, ty_tup(ts)) }

fn mk_fn(cx: ctxt, fty: fn_ty) -> t { mk_t(cx, ty_fn(fty)) }

fn mk_iface(cx: ctxt, did: ast::def_id, tys: [t]) -> t {
    mk_t(cx, ty_iface(did, tys))
}

fn mk_class(cx: ctxt, class_id: ast::def_id, tys: [t]) -> t {
    mk_t(cx, ty_class(class_id, tys))
}

fn mk_res(cx: ctxt, did: ast::def_id, inner: t, tps: [t]) -> t {
    mk_t(cx, ty_res(did, inner, tps))
}

fn mk_var(cx: ctxt, v: int) -> t { mk_t(cx, ty_var(v)) }

fn mk_self(cx: ctxt, tps: [t]) -> t { mk_t(cx, ty_self(tps)) }

fn mk_param(cx: ctxt, n: uint, k: def_id) -> t { mk_t(cx, ty_param(n, k)) }

fn mk_type(cx: ctxt) -> t { mk_t(cx, ty_type) }

fn mk_send_type(cx: ctxt) -> t { mk_t(cx, ty_send_type) }

fn mk_opaque_closure_ptr(cx: ctxt, ck: closure_kind) -> t {
    mk_t(cx, ty_opaque_closure_ptr(ck))
}

fn mk_opaque_box(cx: ctxt) -> t { mk_t(cx, ty_opaque_box) }

fn mk_with_id(cx: ctxt, base: t, def_id: ast::def_id) -> t {
    mk_t_with_id(cx, get(base).struct, some(def_id))
}

// Converts s to its machine type equivalent
pure fn mach_sty(cfg: @session::config, t: t) -> sty {
    alt get(t).struct {
      ty_int(ast::ty_i) { ty_int(cfg.int_type) }
      ty_uint(ast::ty_u) { ty_uint(cfg.uint_type) }
      ty_float(ast::ty_f) { ty_float(cfg.float_type) }
      s { s }
    }
}

fn default_arg_mode_for_ty(ty: ty::t) -> ast::rmode {
    if ty::type_is_immediate(ty) { ast::by_val }
    else { ast::by_ref }
}

fn walk_ty(cx: ctxt, ty: t, f: fn(t)) {
    alt get(ty).struct {
      ty_nil | ty_bot | ty_bool | ty_int(_) | ty_uint(_) | ty_float(_) |
      ty_str | ty_send_type | ty_type | ty_opaque_box |
      ty_opaque_closure_ptr(_) | ty_var(_) | ty_param(_, _) {}
      ty_box(tm) | ty_vec(tm) | ty_ptr(tm) { walk_ty(cx, tm.ty, f); }
      ty_enum(_, subtys) | ty_iface(_, subtys) | ty_class(_, subtys)
       | ty_self(subtys) {
        for subty: t in subtys { walk_ty(cx, subty, f); }
      }
      ty_rec(fields) {
        for fl: field in fields { walk_ty(cx, fl.mt.ty, f); }
      }
      ty_tup(ts) { for tt in ts { walk_ty(cx, tt, f); } }
      ty_fn(ft) {
        for a: arg in ft.inputs { walk_ty(cx, a.ty, f); }
        walk_ty(cx, ft.output, f);
      }
      ty_res(_, sub, tps) {
        walk_ty(cx, sub, f);
        for tp: t in tps { walk_ty(cx, tp, f); }
      }
      ty_constr(sub, _) { walk_ty(cx, sub, f); }
      ty_uniq(tm) { walk_ty(cx, tm.ty, f); }
    }
    f(ty);
}

enum fold_mode {
    fm_var(fn@(int) -> t),
    fm_param(fn@(uint, def_id) -> t),
    fm_general(fn@(t) -> t),
}

fn fold_ty(cx: ctxt, fld: fold_mode, ty_0: t) -> t {
    let ty = ty_0;

    let tb = get(ty);
    alt fld {
      fm_var(_) { if !tb.has_vars { ret ty; } }
      fm_param(_) { if !tb.has_params { ret ty; } }
      fm_general(_) {/* no fast path */ }
    }

    alt tb.struct {
      ty_nil | ty_bot | ty_bool | ty_int(_) | ty_uint(_) | ty_float(_) |
      ty_str | ty_type | ty_send_type | ty_opaque_closure_ptr(_) |
      ty_opaque_box {}
      ty_box(tm) {
        ty = mk_box(cx, {ty: fold_ty(cx, fld, tm.ty), mutbl: tm.mutbl});
      }
      ty_uniq(tm) {
        ty = mk_uniq(cx, {ty: fold_ty(cx, fld, tm.ty), mutbl: tm.mutbl});
      }
      ty_ptr(tm) {
        ty = mk_ptr(cx, {ty: fold_ty(cx, fld, tm.ty), mutbl: tm.mutbl});
      }
      ty_vec(tm) {
        ty = mk_vec(cx, {ty: fold_ty(cx, fld, tm.ty), mutbl: tm.mutbl});
      }
      ty_enum(tid, subtys) {
        ty = mk_enum(cx, tid, vec::map(subtys, {|t| fold_ty(cx, fld, t) }));
      }
      ty_iface(did, subtys) {
        ty = mk_iface(cx, did, vec::map(subtys, {|t| fold_ty(cx, fld, t) }));
      }
      ty_self(subtys) {
        ty = mk_self(cx, vec::map(subtys, {|t| fold_ty(cx, fld, t) }));
      }
      ty_rec(fields) {
        let new_fields: [field] = [];
        for fl: field in fields {
            let new_ty = fold_ty(cx, fld, fl.mt.ty);
            let new_mt = {ty: new_ty, mutbl: fl.mt.mutbl};
            new_fields += [{ident: fl.ident, mt: new_mt}];
        }
        ty = mk_rec(cx, new_fields);
      }
      ty_tup(ts) {
        let new_ts = [];
        for tt in ts { new_ts += [fold_ty(cx, fld, tt)]; }
        ty = mk_tup(cx, new_ts);
      }
      ty_fn(f) {
        let new_args: [arg] = [];
        for a: arg in f.inputs {
            let new_ty = fold_ty(cx, fld, a.ty);
            new_args += [{mode: a.mode, ty: new_ty}];
        }
        ty = mk_fn(cx, {inputs: new_args,
                        output: fold_ty(cx, fld, f.output)
                        with f});
      }
      ty_res(did, subty, tps) {
        let new_tps = [];
        for tp: t in tps { new_tps += [fold_ty(cx, fld, tp)]; }
        ty = mk_res(cx, did, fold_ty(cx, fld, subty), new_tps);
      }
      ty_var(id) {
        alt fld { fm_var(folder) { ty = folder(id); } _ {/* no-op */ } }
      }
      ty_param(id, did) {
        alt fld { fm_param(folder) { ty = folder(id, did); } _ {} }
      }
      ty_constr(subty, cs) {
          ty = mk_constr(cx, fold_ty(cx, fld, subty), cs);
      }
      _ {
          cx.sess.fatal("Unsupported sort of type in fold_ty");
      }
    }

    // If this is a general type fold, then we need to run it now.
    alt fld { fm_general(folder) { ret folder(ty); } _ { ret ty; } }
}


// Type utilities

fn type_is_nil(ty: t) -> bool { get(ty).struct == ty_nil }

fn type_is_bot(ty: t) -> bool { get(ty).struct == ty_bot }

fn type_is_bool(ty: t) -> bool { get(ty).struct == ty_bool }

fn type_is_structural(ty: t) -> bool {
    alt get(ty).struct {
      ty_rec(_) | ty_tup(_) | ty_enum(_, _) | ty_fn(_) |
      ty_iface(_, _) | ty_res(_, _, _) { true }
      _ { false }
    }
}

fn type_is_copyable(cx: ctxt, ty: t) -> bool {
    ret kind_can_be_copied(type_kind(cx, ty));
}

fn type_is_sequence(ty: t) -> bool {
    alt get(ty).struct {
      ty_str { ret true; }
      ty_vec(_) { ret true; }
      _ { ret false; }
    }
}

fn type_is_str(ty: t) -> bool { get(ty).struct == ty_str }

fn sequence_element_type(cx: ctxt, ty: t) -> t {
    alt get(ty).struct {
      ty_str { ret mk_mach_uint(cx, ast::ty_u8); }
      ty_vec(mt) { ret mt.ty; }
      _ { cx.sess.bug("sequence_element_type called on non-sequence value"); }
    }
}

pure fn type_is_tup_like(ty: t) -> bool {
    alt get(ty).struct {
      ty_rec(_) | ty_tup(_) { true }
      _ { false }
    }
}

fn get_element_type(ty: t, i: uint) -> t {
    alt get(ty).struct {
      ty_rec(flds) { ret flds[i].mt.ty; }
      ty_tup(ts) { ret ts[i]; }
      _ { fail "get_element_type called on invalid type"; }
    }
}

pure fn type_is_box(ty: t) -> bool {
    alt get(ty).struct {
      ty_box(_) { ret true; }
      _ { ret false; }
    }
}

pure fn type_is_boxed(ty: t) -> bool {
    alt get(ty).struct {
      ty_box(_) | ty_opaque_box { true }
      _ { false }
    }
}

pure fn type_is_unique_box(ty: t) -> bool {
    alt get(ty).struct {
      ty_uniq(_) { ret true; }
      _ { ret false; }
    }
}

pure fn type_is_unsafe_ptr(ty: t) -> bool {
    alt get(ty).struct {
      ty_ptr(_) { ret true; }
      _ { ret false; }
    }
}

pure fn type_is_vec(ty: t) -> bool {
    ret alt get(ty).struct {
          ty_vec(_) { true }
          ty_str { true }
          _ { false }
        };
}

pure fn type_is_unique(ty: t) -> bool {
    alt get(ty).struct {
      ty_uniq(_) { ret true; }
      ty_vec(_) { true }
      ty_str { true }
      _ { ret false; }
    }
}

pure fn type_is_scalar(ty: t) -> bool {
    alt get(ty).struct {
      ty_nil | ty_bool | ty_int(_) | ty_float(_) | ty_uint(_) |
      ty_send_type | ty_type | ty_ptr(_) { true }
      _ { false }
    }
}

// FIXME maybe inline this for speed?
fn type_is_immediate(ty: t) -> bool {
    ret type_is_scalar(ty) || type_is_boxed(ty) ||
        type_is_unique(ty);
}

fn type_needs_drop(cx: ctxt, ty: t) -> bool {
    alt cx.needs_drop_cache.find(ty) {
      some(result) { ret result; }
      none {/* fall through */ }
    }

    let accum = false;
    let result = alt get(ty).struct {
      // scalar types
      ty_nil | ty_bot | ty_bool | ty_int(_) | ty_float(_) | ty_uint(_) |
      ty_type | ty_ptr(_) { false }
      ty_rec(flds) {
        for f in flds { if type_needs_drop(cx, f.mt.ty) { accum = true; } }
        accum
      }
      ty_tup(elts) {
        for m in elts { if type_needs_drop(cx, m) { accum = true; } }
        accum
      }
      ty_enum(did, tps) {
        let variants = enum_variants(cx, did);
        for variant in *variants {
            for aty in variant.args {
                // Perform any type parameter substitutions.
                let arg_ty = substitute_type_params(cx, tps, aty);
                if type_needs_drop(cx, arg_ty) { accum = true; }
            }
            if accum { break; }
        }
        accum
      }
      _ { true }
    };

    cx.needs_drop_cache.insert(ty, result);
    ret result;
}

enum kind { kind_sendable, kind_copyable, kind_noncopyable, }

// Using these query functons is preferable to direct comparison or matching
// against the kind constants, as we may modify the kind hierarchy in the
// future.
pure fn kind_can_be_copied(k: kind) -> bool {
    ret alt k {
      kind_sendable { true }
      kind_copyable { true }
      kind_noncopyable { false }
    };
}

pure fn kind_can_be_sent(k: kind) -> bool {
    ret alt k {
      kind_sendable { true }
      kind_copyable { false }
      kind_noncopyable { false }
    };
}

fn proto_kind(p: proto) -> kind {
    alt p {
      ast::proto_any { kind_noncopyable }
      ast::proto_block { kind_noncopyable }
      ast::proto_box { kind_copyable }
      ast::proto_uniq { kind_sendable }
      ast::proto_bare { kind_sendable }
    }
}

fn kind_lteq(a: kind, b: kind) -> bool {
    alt a {
      kind_noncopyable { true }
      kind_copyable { b != kind_noncopyable }
      kind_sendable { b == kind_sendable }
    }
}

fn lower_kind(a: kind, b: kind) -> kind {
    if kind_lteq(a, b) { a } else { b }
}

fn type_kind(cx: ctxt, ty: t) -> kind {
    alt cx.kind_cache.find(ty) {
      some(result) { ret result; }
      none {/* fall through */ }
    }

    // Insert a default in case we loop back on self recursively.
    cx.kind_cache.insert(ty, kind_sendable);

    let result = alt get(ty).struct {
      // Scalar and unique types are sendable
      ty_nil | ty_bot | ty_bool | ty_int(_) | ty_uint(_) | ty_float(_) |
      ty_ptr(_) | ty_send_type | ty_str { kind_sendable }
      ty_type { kind_copyable }
      ty_fn(f) { proto_kind(f.proto) }
      ty_opaque_closure_ptr(ck_block) { kind_noncopyable }
      ty_opaque_closure_ptr(ck_box) { kind_copyable }
      ty_opaque_closure_ptr(ck_uniq) { kind_sendable }
      // Those with refcounts-to-inner raise pinned to shared,
      // lower unique to shared. Therefore just set result to shared.
      ty_box(_) | ty_iface(_, _) | ty_opaque_box { kind_copyable }
      // Boxes and unique pointers raise pinned to shared.
      ty_vec(tm) | ty_uniq(tm) { type_kind(cx, tm.ty) }
      // Records lower to the lowest of their members.
      ty_rec(flds) {
        let lowest = kind_sendable;
        for f in flds { lowest = lower_kind(lowest, type_kind(cx, f.mt.ty)); }
        lowest
      }
      // Tuples lower to the lowest of their members.
      ty_tup(tys) {
        let lowest = kind_sendable;
        for ty in tys { lowest = lower_kind(lowest, type_kind(cx, ty)); }
        lowest
      }
      // Enums lower to the lowest of their variants.
      ty_enum(did, tps) {
        let lowest = kind_sendable;
        for variant in *enum_variants(cx, did) {
            for aty in variant.args {
                // Perform any type parameter substitutions.
                let arg_ty = substitute_type_params(cx, tps, aty);
                lowest = lower_kind(lowest, type_kind(cx, arg_ty));
                if lowest == kind_noncopyable { break; }
            }
        }
        lowest
      }
      // Resources are always noncopyable.
      ty_res(did, inner, tps) { kind_noncopyable }
      ty_param(_, did) {
          param_bounds_to_kind(cx.ty_param_bounds.get(did.node))
      }
      ty_constr(t, _) { type_kind(cx, t) }
      _ { cx.sess.bug("Bad type in type_kind"); }
    };

    cx.kind_cache.insert(ty, result);
    ret result;
}

fn type_structurally_contains(cx: ctxt, ty: t, test: fn(sty) -> bool) ->
   bool {
    let sty = get(ty).struct;
    if test(sty) { ret true; }
    alt sty {
      ty_enum(did, tps) {
        for variant in *enum_variants(cx, did) {
            for aty in variant.args {
                let sty = substitute_type_params(cx, tps, aty);
                if type_structurally_contains(cx, sty, test) { ret true; }
            }
        }
        ret false;
      }
      ty_rec(fields) {
        for field in fields {
            if type_structurally_contains(cx, field.mt.ty, test) { ret true; }
        }
        ret false;
      }
      ty_tup(ts) {
        for tt in ts {
            if type_structurally_contains(cx, tt, test) { ret true; }
        }
        ret false;
      }
      ty_res(_, sub, tps) {
        let sty = substitute_type_params(cx, tps, sub);
        ret type_structurally_contains(cx, sty, test);
      }
      _ { ret false; }
    }
}

pure fn type_has_dynamic_size(cx: ctxt, ty: t) -> bool unchecked {
    /* type_structurally_contains can't be declared pure
    because it takes a function argument. But it should be
    referentially transparent, since a given type's size should
    never change once it's created.
    (It would be interesting to think about how to make such properties
    actually checkable. It seems to me like a lot of properties
    that the type context tracks about types should be immutable.)
    */
    type_has_params(ty) && type_structurally_contains(cx, ty) {|sty|
        alt sty {
          ty_param(_, _) { true }
          _ { false }
        }
    }
}

// Returns true for noncopyable types and types where a copy of a value can be
// distinguished from the value itself. I.e. types with mutable content that's
// not shared through a pointer.
fn type_allows_implicit_copy(cx: ctxt, ty: t) -> bool {
    ret !type_structurally_contains(cx, ty, {|sty|
        alt sty {
          ty_param(_, _) { true }
          ty_vec(mt) {
            mt.mutbl != ast::m_imm
          }
          ty_rec(fields) {
            for field in fields {
                if field.mt.mutbl != ast::m_imm {
                    ret true;
                }
            }
            false
          }
          _ { false }
        }
    }) && type_kind(cx, ty) != kind_noncopyable;
}

fn type_structurally_contains_uniques(cx: ctxt, ty: t) -> bool {
    ret type_structurally_contains(cx, ty, {|sty|
        ret alt sty {
          ty_uniq(_) { ret true; }
          ty_vec(_) { true }
          ty_str { true }
          _ { ret false; }
        };
    });
}

fn type_is_integral(ty: t) -> bool {
    alt get(ty).struct {
      ty_int(_) | ty_uint(_) | ty_bool { true }
      _ { false }
    }
}

fn type_is_fp(ty: t) -> bool {
    alt get(ty).struct {
      ty_float(_) { true }
      _ { false }
    }
}

fn type_is_numeric(ty: t) -> bool {
    ret type_is_integral(ty) || type_is_fp(ty);
}

fn type_is_signed(ty: t) -> bool {
    alt get(ty).struct {
      ty_int(_) { true }
      _ { false }
    }
}

// Whether a type is Plain Old Data -- meaning it does not contain pointers
// that the cycle collector might care about.
fn type_is_pod(cx: ctxt, ty: t) -> bool {
    let result = true;
    alt get(ty).struct {
      // Scalar types
      ty_nil | ty_bot | ty_bool | ty_int(_) | ty_float(_) | ty_uint(_) |
      ty_send_type | ty_type | ty_ptr(_) { result = true; }
      // Boxed types
      ty_str | ty_box(_) | ty_uniq(_) | ty_vec(_) | ty_fn(_) |
      ty_iface(_, _) | ty_opaque_box { result = false; }
      // Structural types
      ty_enum(did, tps) {
        let variants = enum_variants(cx, did);
        for variant: variant_info in *variants {
            let tup_ty = mk_tup(cx, variant.args);

            // Perform any type parameter substitutions.
            tup_ty = substitute_type_params(cx, tps, tup_ty);
            if !type_is_pod(cx, tup_ty) { result = false; }
        }
      }
      ty_rec(flds) {
        for f: field in flds {
            if !type_is_pod(cx, f.mt.ty) { result = false; }
        }
      }
      ty_tup(elts) {
        for elt in elts { if !type_is_pod(cx, elt) { result = false; } }
      }
      ty_res(_, inner, tps) {
        result = type_is_pod(cx, substitute_type_params(cx, tps, inner));
      }
      ty_constr(subt, _) { result = type_is_pod(cx, subt); }
      ty_param(_, _) { result = false; }
      ty_opaque_closure_ptr(_) { result = true; }
      _ { cx.sess.bug("unexpected type in type_is_pod"); }
    }

    ret result;
}

fn type_is_enum(ty: t) -> bool {
    alt get(ty).struct {
      ty_enum(_, _) { ret true; }
      _ { ret false;}
    }
}

// Whether a type is enum like, that is a enum type with only nullary
// constructors
fn type_is_c_like_enum(cx: ctxt, ty: t) -> bool {
    alt get(ty).struct {
      ty_enum(did, tps) {
        let variants = enum_variants(cx, did);
        let some_n_ary = vec::any(*variants, {|v| vec::len(v.args) > 0u});
        ret !some_n_ary;
      }
      _ { ret false;}
    }
}

fn type_param(ty: t) -> option<uint> {
    alt get(ty).struct {
      ty_param(id, _) { ret some(id); }
      _ {/* fall through */ }
    }
    ret none;
}

// Returns a vec of all the type variables
// occurring in t. It may contain duplicates.
fn vars_in_type(cx: ctxt, ty: t) -> [int] {
    let rslt = [];
    walk_ty(cx, ty) {|ty|
        alt get(ty).struct { ty_var(v) { rslt += [v]; } _ { } }
    }
    rslt
}

fn type_autoderef(cx: ctxt, t: t) -> t {
    let t1 = t;
    while true {
        alt get(t1).struct {
          ty_box(mt) | ty_uniq(mt) { t1 = mt.ty; }
          ty_res(_, inner, tps) {
            t1 = substitute_type_params(cx, tps, inner);
          }
          ty_enum(did, tps) {
            let variants = enum_variants(cx, did);
            if vec::len(*variants) != 1u || vec::len(variants[0].args) != 1u {
                break;
            }
            t1 = substitute_type_params(cx, tps, variants[0].args[0]);
          }
          _ { break; }
        }
    }
    ret t1;
}

// Type hashing.
fn hash_type_structure(st: sty) -> uint {
    fn hash_uint(id: uint, n: uint) -> uint { (id << 2u) + n }
    fn hash_def(id: uint, did: ast::def_id) -> uint {
        let h = (id << 2u) + (did.crate as uint);
        (h << 2u) + (did.node as uint)
    }
    fn hash_subty(id: uint, subty: t) -> uint { (id << 2u) + type_id(subty) }
    fn hash_subtys(id: uint, subtys: [t]) -> uint {
        let h = id;
        for s in subtys { h = (h << 2u) + type_id(s) }
        h
    }
    fn hash_type_constr(id: uint, c: @type_constr) -> uint {
        let h = id;
        h = (h << 2u) + hash_def(h, c.node.id);
        // FIXME this makes little sense
        for a in c.node.args {
            alt a.node {
              carg_base { h += h << 2u; }
              carg_lit(_) { fail "lit args not implemented yet"; }
              carg_ident(p) { h += h << 2u; }
            }
        }
        h
    }
    alt st {
      ty_nil { 0u } ty_bool { 1u }
      ty_int(t) {
        alt t {
          ast::ty_i { 2u } ast::ty_char { 3u } ast::ty_i8 { 4u }
          ast::ty_i16 { 5u } ast::ty_i32 { 6u } ast::ty_i64 { 7u }
        }
      }
      ty_uint(t) {
        alt t {
          ast::ty_u { 8u } ast::ty_u8 { 9u } ast::ty_u16 { 10u }
          ast::ty_u32 { 11u } ast::ty_u64 { 12u }
        }
      }
      ty_float(t) {
        alt t { ast::ty_f { 13u } ast::ty_f32 { 14u } ast::ty_f64 { 15u } }
      }
      ty_str { 17u }
      ty_enum(did, tys) {
        let h = hash_def(18u, did);
        for typ: t in tys { h = hash_subty(h, typ); }
        h
      }
      ty_box(mt) { hash_subty(19u, mt.ty) }
      ty_vec(mt) { hash_subty(21u, mt.ty) }
      ty_rec(fields) {
        let h = 26u;
        for f in fields { h = hash_subty(h, f.mt.ty); }
        h
      }
      ty_tup(ts) { hash_subtys(25u, ts) }
      ty_fn(f) {
        let h = 27u;
        for a in f.inputs { h = hash_subty(h, a.ty); }
        hash_subty(h, f.output)
      }
      ty_var(v) { hash_uint(30u, v as uint) }
      ty_param(pid, did) { hash_def(hash_uint(31u, pid), did) }
      ty_self(ts) {
        let h = 28u;
        for t in ts { h = hash_subty(h, t); }
        h
      }
      ty_type { 32u }
      ty_bot { 34u }
      ty_ptr(mt) { hash_subty(35u, mt.ty) }
      ty_res(did, sub, tps) {
        let h = hash_subty(hash_def(18u, did), sub);
        hash_subtys(h, tps)
      }
      ty_constr(t, cs) {
        let h = hash_subty(36u, t);
        for c in cs { h = (h << 2u) + hash_type_constr(h, c); }
        h
      }
      ty_uniq(mt) { hash_subty(37u, mt.ty) }
      ty_send_type { 38u }
      ty_iface(did, tys) {
        let h = hash_def(40u, did);
        for typ: t in tys { h = hash_subty(h, typ); }
        h
      }
      ty_opaque_closure_ptr(ck_block) { 41u }
      ty_opaque_closure_ptr(ck_box) { 42u }
      ty_opaque_closure_ptr(ck_uniq) { 43u }
      ty_opaque_box { 44u }
      ty_class(did, tys) {
          let h = hash_def(45u, did);
          for typ: t in tys { h = hash_subty(h, typ); }
          h
      }
    }
}

fn arg_eq<T>(eq: fn(T, T) -> bool,
             a: @sp_constr_arg<T>,
             b: @sp_constr_arg<T>)
   -> bool {
    alt a.node {
      ast::carg_base {
        alt b.node { ast::carg_base { ret true; } _ { ret false; } }
      }
      ast::carg_ident(s) {
        alt b.node { ast::carg_ident(t) { ret eq(s, t); } _ { ret false; } }
      }
      ast::carg_lit(l) {
        alt b.node {
          ast::carg_lit(m) { ret ast_util::lit_eq(l, m); } _ { ret false; }
        }
      }
    }
}

fn args_eq<T>(eq: fn(T, T) -> bool,
              a: [@sp_constr_arg<T>],
              b: [@sp_constr_arg<T>]) -> bool {
    let i: uint = 0u;
    for arg: @sp_constr_arg<T> in a {
        if !arg_eq(eq, arg, b[i]) { ret false; }
        i += 1u;
    }
    ret true;
}

fn constr_eq(c: @constr, d: @constr) -> bool {
    fn eq_int(&&x: uint, &&y: uint) -> bool { ret x == y; }
    ret path_to_str(c.node.path) == path_to_str(d.node.path) &&
            // FIXME: hack
            args_eq(eq_int, c.node.args, d.node.args);
}

fn constrs_eq(cs: [@constr], ds: [@constr]) -> bool {
    if vec::len(cs) != vec::len(ds) { ret false; }
    let i = 0u;
    for c: @constr in cs { if !constr_eq(c, ds[i]) { ret false; } i += 1u; }
    ret true;
}

fn node_id_to_type(cx: ctxt, id: ast::node_id) -> t {
    smallintmap::get(*cx.node_types, id as uint)
}

fn node_id_to_type_params(cx: ctxt, id: ast::node_id) -> [t] {
    alt cx.node_type_substs.find(id) {
      none { ret []; }
      some(ts) { ret ts; }
    }
}

fn node_id_has_type_params(cx: ctxt, id: ast::node_id) -> bool {
    ret cx.node_type_substs.contains_key(id);
}

// Returns the number of distinct type parameters in the given type.
fn count_ty_params(cx: ctxt, ty: t) -> uint {
    let param_indices = [];
    walk_ty(cx, ty) {|t|
        alt get(t).struct {
          ty_param(param_idx, _) {
            if !vec::any(param_indices, {|i| i == param_idx}) {
                param_indices += [param_idx];
            }
          }
          _ {}
        }
    }
    vec::len(param_indices)
}

// Type accessors for substructures of types
fn ty_fn_args(fty: t) -> [arg] {
    alt get(fty).struct {
      ty_fn(f) { f.inputs }
      _ { fail "ty_fn_args() called on non-fn type"; }
    }
}

fn ty_fn_proto(fty: t) -> ast::proto {
    alt get(fty).struct {
      ty_fn(f) { f.proto }
      _ { fail "ty_fn_proto() called on non-fn type"; }
    }
}

pure fn ty_fn_ret(fty: t) -> t {
    alt get(fty).struct {
      ty_fn(f) { f.output }
      _ { fail "ty_fn_ret() called on non-fn type"; }
    }
}

fn ty_fn_ret_style(fty: t) -> ast::ret_style {
    alt get(fty).struct {
      ty_fn(f) { f.ret_style }
      _ { fail "ty_fn_ret_style() called on non-fn type"; }
    }
}

fn is_fn_ty(fty: t) -> bool {
    alt get(fty).struct {
      ty_fn(_) { ret true; }
      _ { ret false; }
    }
}

// Just checks whether it's a fn that returns bool,
// not its purity.
fn is_pred_ty(fty: t) -> bool {
    is_fn_ty(fty) && type_is_bool(ty_fn_ret(fty))
}

fn ty_var_id(typ: t) -> int {
    alt get(typ).struct {
      ty_var(vid) { ret vid; }
      _ { #error("ty_var_id called on non-var ty"); fail; }
    }
}


// Type accessors for AST nodes
fn block_ty(cx: ctxt, b: ast::blk) -> t {
    ret node_id_to_type(cx, b.node.id);
}


// Returns the type of a pattern as a monotype. Like @expr_ty, this function
// doesn't provide type parameter substitutions.
fn pat_ty(cx: ctxt, pat: @ast::pat) -> t {
    ret node_id_to_type(cx, pat.id);
}


// Returns the type of an expression as a monotype.
//
// NB: This type doesn't provide type parameter substitutions; e.g. if you
// ask for the type of "id" in "id(3)", it will return "fn(&int) -> int"
// instead of "fn(t) -> T with T = int". If this isn't what you want, see
// expr_ty_params_and_ty() below.
fn expr_ty(cx: ctxt, expr: @ast::expr) -> t {
    ret node_id_to_type(cx, expr.id);
}

fn expr_ty_params_and_ty(cx: ctxt, expr: @ast::expr) -> {params: [t], ty: t} {
    ret {params: node_id_to_type_params(cx, expr.id),
         ty: node_id_to_type(cx, expr.id)};
}

fn expr_has_ty_params(cx: ctxt, expr: @ast::expr) -> bool {
    ret node_id_has_type_params(cx, expr.id);
}

fn expr_is_lval(method_map: typeck::method_map, e: @ast::expr) -> bool {
    alt e.node {
      ast::expr_path(_) | ast::expr_unary(ast::deref, _) { true }
      ast::expr_field(_, _, _) | ast::expr_index(_, _) {
        !method_map.contains_key(e.id)
      }
      _ { false }
    }
}

fn stmt_node_id(s: @ast::stmt) -> ast::node_id {
    alt s.node {
      ast::stmt_decl(_, id) | stmt_expr(_, id) | stmt_semi(_, id) {
        ret id;
      }
    }
}

fn field_idx(id: ast::ident, fields: [field]) -> option<uint> {
    let i = 0u;
    for f in fields { if f.ident == id { ret some(i); } i += 1u; }
    ret none;
}

fn get_field(rec_ty: t, id: ast::ident) -> field {
    alt vec::find(get_fields(rec_ty), {|f| str::eq(f.ident, id) }) {
      some(f) { f }
      _ { fail #fmt("get_field: bad field id %s", id); }
    }
}

// TODO: could have a precondition instead of failing
fn get_fields(rec_ty:t) -> [field] {
    alt get(rec_ty).struct {
      ty_rec(fields) { fields }
      _ { fail "get_fields called on non-record type"; }
    }
}

fn method_idx(id: ast::ident, meths: [method]) -> option<uint> {
    let i = 0u;
    for m in meths { if m.ident == id { ret some(i); } i += 1u; }
    ret none;
}

fn sort_methods(meths: [method]) -> [method] {
    fn method_lteq(a: method, b: method) -> bool {
        ret str::le(a.ident, b.ident);
    }
    ret std::sort::merge_sort(bind method_lteq(_, _), meths);
}

fn occurs_check_fails(tcx: ctxt, sp: option<span>, vid: int, rt: t) ->
   bool {
    // Fast path
    if !type_has_vars(rt) { ret false; }

    // Occurs check!
    if vec::contains(vars_in_type(tcx, rt), vid) {
        alt sp {
          some(s) {
            // Maybe this should be span_err -- however, there's an
            // assertion later on that the type doesn't contain
            // variables, so in this case we have to be sure to die.
            tcx.sess.span_fatal
                (s, "Type inference failed because I \
                     could not find a type\n that's both of the form "
                 + ty_to_str(tcx, mk_var(tcx, vid)) +
                 " and of the form " + ty_to_str(tcx, rt) +
                 ". Such a type would have to be infinitely large.");
          }
          _ { ret true; }
        }
    } else { ret false; }
}

// Maintains a little union-set tree for inferred modes.  `canon()` returns
// the current head value for `m0`.
fn canon<T:copy>(tbl: hashmap<ast::node_id, ast::inferable<T>>,
                 m0: ast::inferable<T>) -> ast::inferable<T> {
    alt m0 {
      ast::infer(id) {
        alt tbl.find(id) {
          none { m0 }
          some(m1) {
            let cm1 = canon(tbl, m1);
            // path compression:
            if cm1 != m1 { tbl.insert(id, cm1); }
            cm1
          }
        }
      }
      _ { m0 }
    }
}

// Maintains a little union-set tree for inferred modes.  `resolve_mode()`
// returns the current head value for `m0`.
fn canon_mode(cx: ctxt, m0: ast::mode) -> ast::mode {
    canon(cx.inferred_modes, m0)
}

// Returns the head value for mode, failing if `m` was a infer(_) that
// was never inferred.  This should be safe for use after typeck.
fn resolved_mode(cx: ctxt, m: ast::mode) -> ast::rmode {
    alt canon_mode(cx, m) {
      ast::infer(_) {
        cx.sess.bug(#fmt["mode %? was never resolved", m]);
      }
      ast::expl(m0) { m0 }
    }
}

fn arg_mode(cx: ctxt, a: arg) -> ast::rmode { resolved_mode(cx, a.mode) }

// Unifies `m1` and `m2`.  Returns unified value or failure code.
fn unify_mode(cx: ctxt, m1: ast::mode, m2: ast::mode)
    -> result::t<ast::mode, type_err> {
    alt (canon_mode(cx, m1), canon_mode(cx, m2)) {
      (m1, m2) if (m1 == m2) {
        result::ok(m1)
      }
      (ast::infer(id1), ast::infer(id2)) {
        cx.inferred_modes.insert(id2, m1);
        result::ok(m1)
      }
      (ast::infer(id), m) | (m, ast::infer(id)) {
        cx.inferred_modes.insert(id, m);
        result::ok(m1)
      }
      (m1, m2) {
        result::err(terr_mode_mismatch(m1, m2))
      }
    }
}

// If `m` was never unified, unifies it with `m_def`.  Returns the final value
// for `m`.
fn set_default_mode(cx: ctxt, m: ast::mode, m_def: ast::rmode) {
    alt canon_mode(cx, m) {
      ast::infer(id) {
        cx.inferred_modes.insert(id, ast::expl(m_def));
      }
      ast::expl(_) { }
    }
}

// Type unification via Robinson's algorithm (Robinson 1965). Implemented as
// described in Hoder and Voronkov:
//
//     http://www.cs.man.ac.uk/~hoderk/ubench/unification_full.pdf
mod unify {
    export fixup_result;
    export fixup_vars;
    export fix_ok;
    export fix_err;
    export mk_var_bindings;
    export resolve_type_structure;
    export resolve_type_var;
    export result;
    export unify;
    export ures_ok;
    export ures_err;
    export var_bindings;
    export precise, in_bindings;

    enum result { ures_ok(t), ures_err(type_err), }
    enum union_result { unres_ok, unres_err(type_err), }
    enum fixup_result {
        fix_ok(t), // fixup succeeded
        fix_err(int), // fixup failed because a type variable was unresolved
    }
    type var_bindings =
        {sets: ufind::ufind, types: smallintmap::smallintmap<t>};

    enum unify_style {
        precise,
        in_bindings(@var_bindings),
    }
    type uctxt = {st: unify_style, tcx: ctxt};

    fn mk_var_bindings() -> @var_bindings {
        ret @{sets: ufind::make(), types: smallintmap::mk::<t>()};
    }

    // Unifies two sets.
    fn union(cx: @uctxt, set_a: uint, set_b: uint,
             variance: variance) -> union_result {
        let vb = alt cx.st {
            in_bindings(vb) { vb }
            _ { cx.tcx.sess.bug("Someone forgot to document an invariant \
                         in union"); }
        };
        ufind::grow(vb.sets, math::max(set_a, set_b) + 1u);
        let root_a = ufind::find(vb.sets, set_a);
        let root_b = ufind::find(vb.sets, set_b);

        let replace_type = (
            fn@(vb: @var_bindings, t: t) {
                ufind::union(vb.sets, set_a, set_b);
                let root_c: uint = ufind::find(vb.sets, set_a);
                smallintmap::insert::<t>(vb.types, root_c, t);
            }
        );

        alt smallintmap::find(vb.types, root_a) {
          none {
            alt smallintmap::find(vb.types, root_b) {
              none { ufind::union(vb.sets, set_a, set_b); ret unres_ok; }
              some(t_b) { replace_type(vb, t_b); ret unres_ok; }
            }
          }
          some(t_a) {
            alt smallintmap::find(vb.types, root_b) {
              none { replace_type(vb, t_a); ret unres_ok; }
              some(t_b) {
                alt unify_step(cx, t_a, t_b, variance) {
                  ures_ok(t_c) { replace_type(vb, t_c); ret unres_ok; }
                  ures_err(terr) { ret unres_err(terr); }
                }
              }
            }
          }
        }
    }

    fn record_var_binding_for_expected(
        cx: @uctxt, key: int, typ: t, variance: variance) -> result {
        record_var_binding(
            cx, key, typ, variance_transform(variance, covariant))
    }

    fn record_var_binding_for_actual(
        cx: @uctxt, key: int, typ: t, variance: variance) -> result {
        // Unifying in 'the other direction' so flip the variance
        record_var_binding(
            cx, key, typ, variance_transform(variance, contravariant))
    }

    fn record_var_binding(
        cx: @uctxt, key: int, typ: t, variance: variance) -> result {

        let vb = alt cx.st { in_bindings(vb) { vb }
            _ { cx.tcx.sess.bug("Someone forgot to document an invariant \
                 in record_var_binding");  }
        };
        ufind::grow(vb.sets, (key as uint) + 1u);
        let root = ufind::find(vb.sets, key as uint);
        let result_type = typ;
        alt smallintmap::find(vb.types, root) {
          some(old_type) {
            alt unify_step(cx, old_type, typ, variance) {
              ures_ok(unified_type) { result_type = unified_type; }
              rs { ret rs; }
            }
          }
          none {/* fall through */ }
        }
        smallintmap::insert::<t>(vb.types, root, result_type);
        ret ures_ok(typ);
    }

    // Simple structural type comparison.
    fn struct_cmp(cx: @uctxt, expected: t, actual: t) -> result {
        let tcx = cx.tcx;
        let cfg = tcx.sess.targ_cfg;
        if mach_sty(cfg, expected) == mach_sty(cfg, actual) {
            ret ures_ok(expected);
        }
        ret ures_err(terr_mismatch);
    }

    // Right now this just checks that the lists of constraints are
    // pairwise equal.
    fn unify_constrs(base_t: t, expected: [@type_constr],
                     actual: [@type_constr]) -> result {
        let expected_len = vec::len(expected);
        let actual_len = vec::len(actual);

        if expected_len != actual_len {
            ret ures_err(terr_constr_len(expected_len, actual_len));
        }
        let i = 0u;
        let rslt;
        for c: @type_constr in expected {
            rslt = unify_constr(base_t, c, actual[i]);
            alt rslt { ures_ok(_) { } ures_err(_) { ret rslt; } }
            i += 1u;
        }
        ret ures_ok(base_t);
    }
    fn unify_constr(base_t: t, expected: @type_constr,
                    actual_constr: @type_constr) -> result {
        let ok_res = ures_ok(base_t);
        let err_res = ures_err(terr_constr_mismatch(expected, actual_constr));
        if expected.node.id != actual_constr.node.id { ret err_res; }
        let expected_arg_len = vec::len(expected.node.args);
        let actual_arg_len = vec::len(actual_constr.node.args);
        if expected_arg_len != actual_arg_len { ret err_res; }
        let i = 0u;
        let actual;
        for a: @ty_constr_arg in expected.node.args {
            actual = actual_constr.node.args[i];
            alt a.node {
              carg_base {
                alt actual.node { carg_base { } _ { ret err_res; } }
              }
              carg_lit(l) {
                alt actual.node {
                  carg_lit(m) { if l != m { ret err_res; } }
                  _ { ret err_res; }
                }
              }
              carg_ident(p) {
                alt actual.node {
                  carg_ident(q) { if p.node != q.node { ret err_res; } }
                  _ { ret err_res; }
                }
              }
            }
            i += 1u;
        }
        ret ok_res;
    }

    // Unifies two mutability flags.
    fn unify_mut(expected: ast::mutability, actual: ast::mutability,
                 variance: variance) ->
       option<(ast::mutability, variance)> {

        // If you're unifying on something mutable then we have to
        // be invariant on the inner type
        let newvariance = alt expected {
          ast::m_mutbl {
            variance_transform(variance, invariant)
          }
          _ {
            variance_transform(variance, covariant)
          }
        };

        if expected == actual { ret some((expected, newvariance)); }
        if variance == covariant {
            if expected == ast::m_const {
                ret some((actual, newvariance));
            }
        } else if variance == contravariant {
            if actual == ast::m_const {
                ret some((expected, newvariance));
            }
        }
        ret none;
    }
    fn unify_fn_proto(e_proto: ast::proto, a_proto: ast::proto,
                      variance: variance) -> option<result> {
        // Prototypes form a diamond-shaped partial order:
        //
        //        block
        //        ^   ^
        //   shared   send
        //        ^   ^
        //        bare
        //
        // where "^" means "subtype of" (forgive the abuse of the term
        // subtype).
        fn sub_proto(p_sub: ast::proto, p_sup: ast::proto) -> bool {
            ret alt (p_sub, p_sup) {
              (_, ast::proto_any) { true }
              (ast::proto_bare, _) { true }

              // Equal prototypes are always subprotos:
              (_, _) { p_sub == p_sup }
            };
        }

        ret alt variance {
          invariant if e_proto == a_proto { none }
          covariant if sub_proto(a_proto, e_proto) { none }
          contravariant if sub_proto(e_proto, a_proto) { none }
          _ { some(ures_err(terr_mismatch)) }
        };
    }
    fn unify_args(cx: @uctxt, e_args: [arg], a_args: [arg],
                  variance: variance) -> either::t<result, [arg]> {
        if !vec::same_length(e_args, a_args) {
            ret either::left(ures_err(terr_arg_count));
        }
        // The variance changes (flips basically) when descending
        // into arguments of function types
        let variance = variance_transform(variance, contravariant);
        // Would use vec::map2(), but for the need to return in case of
        // error:
        let i = 0u, result = [];
        for expected_input in e_args {
            let actual_input = a_args[i];
            i += 1u;

            // Unify the result modes.
            let result_mode =
                alt unify_mode(cx.tcx, expected_input.mode,
                               actual_input.mode) {
                  result::err(err) { ret either::left(ures_err(err)); }
                  result::ok(m) { m }
                };

            alt unify_step(cx, expected_input.ty, actual_input.ty,
                           variance) {
              ures_ok(rty) { result += [{mode: result_mode, ty: rty}]; }
              err { ret either::left(err); }
            }
        }
        either::right(result)
    }
    fn unify_fn(cx: @uctxt, e_f: fn_ty, a_f: fn_ty, variance: variance)
        -> result {
        alt unify_fn_proto(e_f.proto, a_f.proto, variance) {
          some(err) { ret err; }
          none { /* fall through */ }
        }

        if a_f.ret_style != ast::noreturn && a_f.ret_style != e_f.ret_style {
            /* even though typestate checking is mostly
               responsible for checking control flow annotations,
               this check is necessary to ensure that the
               annotation in an object method matches the
               declared object type */
            ret ures_err(terr_ret_style_mismatch(e_f.ret_style,
                                                 a_f.ret_style));
        }
        let result_ins = alt unify_args(cx, e_f.inputs, a_f.inputs,
                                        variance) {
            either::left(err) { ret err; }
            either::right(ts) { ts }
        };

        // Check the output.
        alt unify_step(cx, e_f.output, a_f.output, variance) {
          ures_ok(rty) {
            ures_ok(mk_fn(cx.tcx, {proto: e_f.proto,
                                   inputs: result_ins,
                                   output: rty
                                   with a_f}))
          }
          x { x }
        }
    }

    // If the given type is a variable, returns the structure of that type.
    fn resolve_type_structure(vb: @var_bindings, typ: t) ->
       fixup_result {
        alt get(typ).struct {
          ty_var(vid) {
            if vid as uint >= ufind::set_count(vb.sets) { ret fix_err(vid); }
            let root_id = ufind::find(vb.sets, vid as uint);
            alt smallintmap::find::<t>(vb.types, root_id) {
              none { ret fix_err(vid); }
              some(rt) { ret fix_ok(rt); }
            }
          }
          _ { ret fix_ok(typ); }
        }
    }

    // Specifies the allowable subtyping between expected and actual types
    enum variance {
        // Actual may be a subtype of expected
        covariant,
        // Actual may be a supertype of expected
        contravariant,
        // Actual must be the same type as expected
        invariant,
    }

    // The calculation for recursive variance
    // "Taming the Wildcards: Combining Definition- and Use-Site Variance"
    // by John Altidor, et. al.
    //
    // I'm just copying the table from figure 1 - haven't actually
    // read the paper (yet).
    fn variance_transform(a: variance, b: variance) -> variance {
        alt a {
          covariant {
            alt b {
              covariant { covariant }
              contravariant { contravariant }
              invariant { invariant }
            }
          }
          contravariant {
            alt b {
              covariant { contravariant }
              contravariant { covariant }
              invariant { invariant }
            }
          }
          invariant {
            alt b {
              covariant { invariant }
              contravariant { invariant }
              invariant { invariant }
            }
          }
        }
    }

    fn unify_tps(cx: @uctxt, expected_tps: [t], actual_tps: [t],
                 variance: variance, finish: fn([t]) -> result) -> result {
        let result_tps = [], i = 0u;
        for exp in expected_tps {
            let act = actual_tps[i];
            i += 1u;
            let result = unify_step(cx, exp, act, variance);
            alt result {
              ures_ok(rty) { result_tps += [rty]; }
              _ { ret result; }
            }
        }
        finish(result_tps)
    }
    fn unify_step(cx: @uctxt, expected: t, actual: t,
                  variance: variance) -> result {
        // FIXME: rewrite this using tuple pattern matching when available, to
        // avoid all this rightward drift and spikiness.
        // NOTE: we have tuple matching now, but that involves copying the
        // matched elements into a tuple first, which is expensive, since sty
        // holds vectors, which are currently unique

        // Fast path.
        if expected == actual { ret ures_ok(expected); }

        // Stage 1: Handle the cases in which one side or another is a type
        // variable

        alt get(actual).struct {
          // If the RHS is a variable type, then just do the
          // appropriate binding.
          ty_var(actual_id) {
            let actual_n = actual_id as uint;
            alt get(expected).struct {
              ty_var(expected_id) {
                let expected_n = expected_id as uint;
                alt union(cx, expected_n, actual_n, variance) {
                  unres_ok {/* fall through */ }
                  unres_err(t_e) { ret ures_err(t_e); }
                }
              }
              _ {
                // Just bind the type variable to the expected type.
                alt record_var_binding_for_actual(
                    cx, actual_id, expected, variance) {
                  ures_ok(_) {/* fall through */ }
                  rs { ret rs; }
                }
              }
            }
            ret ures_ok(mk_var(cx.tcx, actual_id));
          }
          _ {/* empty */ }
        }
        alt get(expected).struct {
          ty_var(expected_id) {
            // Add a binding. (`actual` can't actually be a var here.)
            alt record_var_binding_for_expected(
                cx, expected_id, actual,
                variance) {
              ures_ok(_) {/* fall through */ }
              rs { ret rs; }
            }
            ret ures_ok(mk_var(cx.tcx, expected_id));
          }
          _ {/* fall through */ }
        }
        // Stage 2: Handle all other cases.

        alt get(actual).struct {
          ty_bot { ret ures_ok(expected); }
          _ {/* fall through */ }
        }
        alt get(expected).struct {
          ty_nil { ret struct_cmp(cx, expected, actual); }
          // _|_ unifies with anything
          ty_bot {
            ret ures_ok(actual);
          }
          ty_bool | ty_int(_) | ty_uint(_) | ty_float(_) |
          ty_str | ty_send_type {
            ret struct_cmp(cx, expected, actual);
          }
          ty_param(expected_n, _) {
            alt get(actual).struct {
              ty_param(actual_n, _) if expected_n == actual_n {
                ret ures_ok(expected);
              }
              _ { ret ures_err(terr_mismatch); }
            }
          }
          ty_enum(expected_id, expected_tps) {
            alt get(actual).struct {
              ty_enum(actual_id, actual_tps) {
                if expected_id != actual_id {
                    ret ures_err(terr_mismatch);
                }
                ret unify_tps(cx, expected_tps, actual_tps, variance, {|tps|
                    ures_ok(mk_enum(cx.tcx, expected_id, tps))
                });
              }
              _ {/* fall through */ }
            }
            ret ures_err(terr_mismatch);
          }
          ty_iface(expected_id, expected_tps) {
            alt get(actual).struct {
              ty_iface(actual_id, actual_tps) {
                if expected_id != actual_id {
                    ret ures_err(terr_mismatch);
                }
                ret unify_tps(cx, expected_tps, actual_tps, variance, {|tps|
                    ures_ok(mk_iface(cx.tcx, expected_id, tps))
                });
              }
              _ {}
            }
            ret ures_err(terr_mismatch);
          }
          ty_box(expected_mt) {
            alt get(actual).struct {
              ty_box(actual_mt) {
                let (mutt, var) = alt unify_mut(
                    expected_mt.mutbl, actual_mt.mutbl, variance) {
                  none { ret ures_err(terr_box_mutability); }
                  some(mv) { mv }
                };
                let result = unify_step(
                    cx, expected_mt.ty, actual_mt.ty, var);
                alt result {
                  ures_ok(result_sub) {
                    let mt = {ty: result_sub, mutbl: mutt};
                    ret ures_ok(mk_box(cx.tcx, mt));
                  }
                  _ { ret result; }
                }
              }
              _ { ret ures_err(terr_mismatch); }
            }
          }
          ty_uniq(expected_mt) {
            alt get(actual).struct {
              ty_uniq(actual_mt) {
                let (mutt, var) = alt unify_mut(
                    expected_mt.mutbl, actual_mt.mutbl, variance) {
                  none { ret ures_err(terr_box_mutability); }
                  some(mv) { mv }
                };
                let result = unify_step(
                    cx, expected_mt.ty, actual_mt.ty, var);
                alt result {
                  ures_ok(result_mt) {
                    let mt = {ty: result_mt, mutbl: mutt};
                    ret ures_ok(mk_uniq(cx.tcx, mt));
                  }
                  _ { ret result; }
                }
              }
              _ { ret ures_err(terr_mismatch); }
            }
          }
          ty_vec(expected_mt) {
            alt get(actual).struct {
              ty_vec(actual_mt) {
                let (mutt, var) = alt unify_mut(
                    expected_mt.mutbl, actual_mt.mutbl, variance) {
                  none { ret ures_err(terr_vec_mutability); }
                  some(mv) { mv }
                };
                let result = unify_step(
                    cx, expected_mt.ty, actual_mt.ty, var);
                alt result {
                  ures_ok(result_sub) {
                    let mt = {ty: result_sub, mutbl: mutt};
                    ret ures_ok(mk_vec(cx.tcx, mt));
                  }
                  _ { ret result; }
                }
              }
              _ { ret ures_err(terr_mismatch); }
            }
          }
          ty_ptr(expected_mt) {
            alt get(actual).struct {
              ty_ptr(actual_mt) {
                let (mutt, var) = alt unify_mut(
                    expected_mt.mutbl, actual_mt.mutbl, variance) {
                  none { ret ures_err(terr_vec_mutability); }
                  some(mv) { mv }
                };
                let result = unify_step(
                    cx, expected_mt.ty, actual_mt.ty, var);
                alt result {
                  ures_ok(result_sub) {
                    let mt = {ty: result_sub, mutbl: mutt};
                    ret ures_ok(mk_ptr(cx.tcx, mt));
                  }
                  _ { ret result; }
                }
              }
              _ { ret ures_err(terr_mismatch); }
            }
          }
          ty_res(ex_id, ex_inner, ex_tps) {
            alt get(actual).struct {
              ty_res(act_id, act_inner, act_tps) {
                if ex_id.crate != act_id.crate || ex_id.node != act_id.node {
                    ret ures_err(terr_mismatch);
                }
                let result = unify_step(
                    cx, ex_inner, act_inner, variance);
                alt result {
                  ures_ok(res_inner) {
                    let i = 0u;
                    let res_tps = [];
                    for ex_tp: t in ex_tps {
                        let result = unify_step(
                            cx, ex_tp, act_tps[i], variance);
                        alt result {
                          ures_ok(rty) { res_tps += [rty]; }
                          _ { ret result; }
                        }
                        i += 1u;
                    }
                    ret ures_ok(mk_res(cx.tcx, act_id, res_inner, res_tps));
                  }
                  _ { ret result; }
                }
              }
              _ { ret ures_err(terr_mismatch); }
            }
          }
          ty_rec(expected_fields) {
            alt get(actual).struct {
              ty_rec(actual_fields) {
                let expected_len = vec::len::<field>(expected_fields);
                let actual_len = vec::len::<field>(actual_fields);
                if expected_len != actual_len {
                    let err = terr_record_size(expected_len, actual_len);
                    ret ures_err(err);
                }
                // TODO: implement an iterator that can iterate over
                // two arrays simultaneously.

                let result_fields: [field] = [];
                let i = 0u;
                while i < expected_len {
                    let expected_field = expected_fields[i];
                    let actual_field = actual_fields[i];
                    let u_mut = unify_mut(expected_field.mt.mutbl,
                                          actual_field.mt.mutbl,
                                          variance);
                    let (mutt, var) = alt u_mut {
                      none { ret ures_err(terr_record_mutability); }
                      some(mv) { mv }
                    };
                    if !str::eq(expected_field.ident, actual_field.ident) {
                        let err =
                            terr_record_fields(expected_field.ident,
                                               actual_field.ident);
                        ret ures_err(err);
                    }
                    let result =
                        unify_step(cx, expected_field.mt.ty,
                                   actual_field.mt.ty, var);
                    alt result {
                      ures_ok(rty) {
                        let mt = {ty: rty, mutbl: mutt};
                        result_fields += [{mt: mt with expected_field}];
                      }
                      _ { ret result; }
                    }
                    i += 1u;
                }
                ret ures_ok(mk_rec(cx.tcx, result_fields));
              }
              _ { ret ures_err(terr_mismatch); }
            }
          }
          ty_tup(expected_elems) {
            alt get(actual).struct {
              ty_tup(actual_elems) {
                let expected_len = vec::len(expected_elems);
                let actual_len = vec::len(actual_elems);
                if expected_len != actual_len {
                    let err = terr_tuple_size(expected_len, actual_len);
                    ret ures_err(err);
                }
                // TODO: implement an iterator that can iterate over
                // two arrays simultaneously.

                let result_elems = [];
                let i = 0u;
                while i < expected_len {
                    let expected_elem = expected_elems[i];
                    let actual_elem = actual_elems[i];
                    let result = unify_step(
                        cx, expected_elem, actual_elem, variance);
                    alt result {
                      ures_ok(rty) { result_elems += [rty]; }
                      _ { ret result; }
                    }
                    i += 1u;
                }
                ret ures_ok(mk_tup(cx.tcx, result_elems));
              }
              _ { ret ures_err(terr_mismatch); }
            }
          }
          ty_fn(expected_f) {
            alt get(actual).struct {
              ty_fn(actual_f) {
                ret unify_fn(cx, expected_f, actual_f, variance);
              }
              _ { ret ures_err(terr_mismatch); }
            }
          }
          ty_constr(expected_t, expected_constrs) {

            // unify the base types...
            alt get(actual).struct {
              ty_constr(actual_t, actual_constrs) {
                let rslt = unify_step(
                    cx, expected_t, actual_t, variance);
                alt rslt {
                  ures_ok(rty) {
                    // FIXME: probably too restrictive --
                    // requires the constraints to be
                    // syntactically equal
                    ret unify_constrs(expected, expected_constrs,
                                      actual_constrs);
                  }
                  _ { ret rslt; }
                }
              }
              _ {
                // If the actual type is *not* a constrained type,
                // then we go ahead and just ignore the constraints on
                // the expected type. typestate handles the rest.
                ret unify_step(
                    cx, expected_t, actual, variance);
              }
            }
          }
          _ { cx.tcx.sess.bug("unify: unexpected type"); }
        }
    }
    fn unify(expected: t, actual: t, st: unify_style,
             tcx: ctxt) -> result {
        let cx = @{st: st, tcx: tcx};
        ret unify_step(cx, expected, actual, covariant);
    }
    fn dump_var_bindings(tcx: ctxt, vb: @var_bindings) {
        let i = 0u;
        while i < vec::len::<ufind::node>(vb.sets.nodes) {
            let sets = "";
            let j = 0u;
            while j < vec::len::<option<uint>>(vb.sets.nodes) {
                if ufind::find(vb.sets, j) == i { sets += #fmt[" %u", j]; }
                j += 1u;
            }
            let typespec;
            alt smallintmap::find::<t>(vb.types, i) {
              none { typespec = ""; }
              some(typ) { typespec = " =" + ty_to_str(tcx, typ); }
            }
            #error("set %u:%s%s", i, typespec, sets);
            i += 1u;
        }
    }

    // Fixups and substitutions
    //    Takes an optional span - complain about occurs check violations
    //    iff the span is present (so that if we already know we're going
    //    to error anyway, we don't complain)
    fn fixup_vars(tcx: ctxt, sp: option<span>, vb: @var_bindings,
                  typ: t) -> fixup_result {
        fn subst_vars(tcx: ctxt, sp: option<span>, vb: @var_bindings,
                      unresolved: @mutable option<int>,
                      vars_seen: std::list::list<int>, vid: int) -> t {
            // Should really return a fixup_result instead of a t, but fold_ty
            // doesn't allow returning anything but a t.
            if vid as uint >= ufind::set_count(vb.sets) {
                *unresolved = some(vid);
                ret mk_var(tcx, vid);
            }
            let root_id = ufind::find(vb.sets, vid as uint);
            alt smallintmap::find::<t>(vb.types, root_id) {
              none { *unresolved = some(vid); ret mk_var(tcx, vid); }
              some(rt) {
                let give_up = false;
                std::list::iter(vars_seen) {|v|
                    if v == vid {
                        give_up = true;
                        option::may(sp) {|sp|
                            tcx.sess.span_fatal(
                                sp, "can not instantiate infinite type");
                        }
                    }
                }
                // Return the type unchanged, so we can error out
                // downstream
                if give_up { ret rt; }
                ret fold_ty(tcx, fm_var(bind subst_vars(
                    tcx, sp, vb, unresolved, std::list::cons(vid, @vars_seen),
                    _)), rt);
              }
            }
        }
        let unresolved = @mutable none::<int>;
        let rty = fold_ty(tcx, fm_var(bind subst_vars(
            tcx, sp, vb, unresolved, std::list::nil, _)), typ);
        let ur = *unresolved;
        alt ur {
          none { ret fix_ok(rty); }
          some(var_id) { ret fix_err(var_id); }
        }
    }
    fn resolve_type_var(tcx: ctxt, sp: option<span>, vb: @var_bindings,
                        vid: int) -> fixup_result {
        if vid as uint >= ufind::set_count(vb.sets) { ret fix_err(vid); }
        let root_id = ufind::find(vb.sets, vid as uint);
        alt smallintmap::find::<t>(vb.types, root_id) {
          none { ret fix_err(vid); }
          some(rt) { ret fixup_vars(tcx, sp, vb, rt); }
        }
    }
}

fn same_type(cx: ctxt, a: t, b: t) -> bool {
    alt unify::unify(a, b, unify::precise, cx) {
      unify::ures_ok(_) { true }
      _ { false }
    }
}

fn type_err_to_str(err: type_err) -> str {
    alt err {
      terr_mismatch { ret "types differ"; }
      terr_ret_style_mismatch(expect, actual) {
        fn to_str(s: ast::ret_style) -> str {
            alt s {
              ast::noreturn { "non-returning" }
              ast::return_val { "return-by-value" }
            }
        }
        ret to_str(actual) + " function found where " + to_str(expect) +
            " function was expected";
      }
      terr_box_mutability { ret "boxed values differ in mutability"; }
      terr_vec_mutability { ret "vectors differ in mutability"; }
      terr_tuple_size(e_sz, a_sz) {
        ret "expected a tuple with " + uint::to_str(e_sz, 10u) +
                " elements but found one with " + uint::to_str(a_sz, 10u) +
                " elements";
      }
      terr_record_size(e_sz, a_sz) {
        ret "expected a record with " + uint::to_str(e_sz, 10u) +
                " fields but found one with " + uint::to_str(a_sz, 10u) +
                " fields";
      }
      terr_record_mutability { ret "record elements differ in mutability"; }
      terr_record_fields(e_fld, a_fld) {
        ret "expected a record with field '" + e_fld +
                "' but found one with field '" + a_fld + "'";
      }
      terr_arg_count { ret "incorrect number of function parameters"; }
      terr_mode_mismatch(e_mode, a_mode) {
        ret "expected argument mode " + mode_to_str(e_mode) + " but found " +
                mode_to_str(a_mode);
      }
      terr_constr_len(e_len, a_len) {
        ret "Expected a type with " + uint::str(e_len) +
                " constraints, but found one with " + uint::str(a_len) +
                " constraints";
      }
      terr_constr_mismatch(e_constr, a_constr) {
        ret "Expected a type with constraint " + ty_constr_to_str(e_constr) +
                " but found one with constraint " +
                ty_constr_to_str(a_constr);
      }
    }
}

// Replaces type parameters in the given type using the given list of
// substitions.
fn substitute_type_params(cx: ctxt, substs: [ty::t], typ: t) -> t {
    // Precondition? idx < vec::len(substs)
    fold_ty(cx, fm_param({|idx, _id| substs[idx]}), typ)
}

fn def_has_ty_params(def: ast::def) -> bool {
    alt def {
      ast::def_fn(_, _) | ast::def_variant(_, _) { true }
      _ { false }
    }
}

fn store_iface_methods(cx: ctxt, id: ast::node_id, ms: @[method]) {
    cx.iface_method_cache.insert(ast_util::local_def(id), ms);
}

fn iface_methods(cx: ctxt, id: ast::def_id) -> @[method] {
    alt cx.iface_method_cache.find(id) {
      some(ms) { ret ms; }
      _ {}
    }
    // Local interfaces are supposed to have been added explicitly.
    assert id.crate != ast::local_crate;
    let result = csearch::get_iface_methods(cx, id);
    cx.iface_method_cache.insert(id, result);
    result
}

fn impl_iface(cx: ctxt, id: ast::def_id) -> option<t> {
    if id.crate == ast::local_crate {
        option::map(cx.tcache.find(id), {|it| it.ty})
    } else {
        csearch::get_impl_iface(cx, id)
    }
}

// Enum information
type variant_info = @{args: [t], ctor_ty: t, name: str,
                      id: ast::def_id, disr_val: int};

fn substd_enum_variants(cx: ctxt, id: ast::def_id, tps: [ty::t])
    -> [variant_info] {
    vec::map(*enum_variants(cx, id)) { |variant_info|
        let substd_args = vec::map(variant_info.args) {|aty|
            substitute_type_params(cx, tps, aty)
        };

        let substd_ctor_ty =
            substitute_type_params(cx, tps, variant_info.ctor_ty);

        @{args: substd_args, ctor_ty: substd_ctor_ty with *variant_info}
    }
}

fn item_path(cx: ctxt, id: ast::def_id) -> ast_map::path {
    if id.crate != ast::local_crate {
        csearch::get_item_path(cx, id)
    } else {
        let node = cx.items.get(id.node);
        alt node {
          ast_map::node_item(item, path) {
            let item_elt = alt item.node {
              item_mod(_) | item_native_mod(_) {
                ast_map::path_mod(item.ident)
              }
              _ {
                ast_map::path_name(item.ident)
              }
            };
            *path + [item_elt]
          }

          ast_map::node_native_item(nitem, path) {
            *path + [ast_map::path_name(nitem.ident)]
          }

          ast_map::node_method(method, _, path) {
            *path + [ast_map::path_name(method.ident)]
          }

          ast_map::node_variant(variant, _, path) {
            vec::init(*path) + [ast_map::path_name(variant.node.name)]
          }

          ast_map::node_expr(_) | ast_map::node_arg(_, _) |
          ast_map::node_local(_) | ast_map::node_res_ctor(_) {
            cx.sess.bug(#fmt["cannot find item_path for node %?", node]);
          }
        }
    }
}

fn enum_variants(cx: ctxt, id: ast::def_id) -> @[variant_info] {
    alt cx.enum_var_cache.find(id) {
      some(variants) { ret variants; }
      _ { /* fallthrough */ }
    }
    let result = if ast::local_crate != id.crate {
        @csearch::get_enum_variants(cx, id)
    } else {
        // FIXME: Now that the variants are run through the type checker (to
        // check the disr_expr if it exists), this code should likely be
        // moved there to avoid having to call eval_const_expr twice.
        alt cx.items.get(id.node) {
          ast_map::node_item(@{node: ast::item_enum(variants, _), _}, _) {
            let disr_val = -1;
            @vec::map(variants, {|variant|
                let ctor_ty = node_id_to_type(cx, variant.node.id);
                let arg_tys = if vec::len(variant.node.args) > 0u {
                    vec::map(ty_fn_args(ctor_ty), {|a| a.ty})
                } else { [] };
                alt variant.node.disr_expr {
                  some (ex) {
                    // FIXME: issue #1417
                    disr_val = alt syntax::ast_util::eval_const_expr(ex) {
                      ast_util::const_int(val) {val as int}
                      _ { cx.sess.bug("tag_variants: bad disr expr"); }
                    }
                  }
                  _ {disr_val += 1;}
                }
                @{args: arg_tys,
                  ctor_ty: ctor_ty,
                  name: variant.node.name,
                  id: ast_util::local_def(variant.node.id),
                  disr_val: disr_val
                 }
            })
          }
          _ { cx.sess.bug("tag_variants: id not bound to an enum"); }
        }
    };
    cx.enum_var_cache.insert(id, result);
    result
}


// Returns information about the enum variant with the given ID:
fn enum_variant_with_id(cx: ctxt, enum_id: ast::def_id,
                        variant_id: ast::def_id) -> variant_info {
    let variants = enum_variants(cx, enum_id);
    let i = 0u;
    while i < vec::len::<variant_info>(*variants) {
        let variant = variants[i];
        if def_eq(variant.id, variant_id) { ret variant; }
        i += 1u;
    }
    cx.sess.bug("enum_variant_with_id(): no variant exists with that ID");
}


// If the given item is in an external crate, looks up its type and adds it to
// the type cache. Returns the type parameters and type.
fn lookup_item_type(cx: ctxt, did: ast::def_id) -> ty_param_bounds_and_ty {
    alt cx.tcache.find(did) {
      some(tpt) { ret tpt; }
      none {
          /* where do things get added to the cache?
             Have to add class members */

        // The item is in this crate. The caller should have added it to the
        // type cache already
        assert did.crate != ast::local_crate;
        let tyt = csearch::get_type(cx, did);
        cx.tcache.insert(did, tyt);
        ret tyt;
      }
    }
}

fn is_binopable(_cx: ctxt, ty: t, op: ast::binop) -> bool {
    const tycat_other: int = 0;
    const tycat_bool: int = 1;
    const tycat_int: int = 2;
    const tycat_float: int = 3;
    const tycat_str: int = 4;
    const tycat_vec: int = 5;
    const tycat_struct: int = 6;
    const tycat_bot: int = 7;

    const opcat_add: int = 0;
    const opcat_sub: int = 1;
    const opcat_mult: int = 2;
    const opcat_shift: int = 3;
    const opcat_rel: int = 4;
    const opcat_eq: int = 5;
    const opcat_bit: int = 6;
    const opcat_logic: int = 7;

    fn opcat(op: ast::binop) -> int {
        alt op {
          ast::add { opcat_add }
          ast::subtract { opcat_sub }
          ast::mul { opcat_mult }
          ast::div { opcat_mult }
          ast::rem { opcat_mult }
          ast::and { opcat_logic }
          ast::or { opcat_logic }
          ast::bitxor { opcat_bit }
          ast::bitand { opcat_bit }
          ast::bitor { opcat_bit }
          ast::lsl { opcat_shift }
          ast::lsr { opcat_shift }
          ast::asr { opcat_shift }
          ast::eq { opcat_eq }
          ast::ne { opcat_eq }
          ast::lt { opcat_rel }
          ast::le { opcat_rel }
          ast::ge { opcat_rel }
          ast::gt { opcat_rel }
        }
    }

    fn tycat(ty: t) -> int {
        alt get(ty).struct {
          ty_bool { tycat_bool }
          ty_int(_) { tycat_int }
          ty_uint(_) { tycat_int }
          ty_float(_) { tycat_float }
          ty_str { tycat_str }
          ty_vec(_) { tycat_vec }
          ty_rec(_) { tycat_struct }
          ty_tup(_) { tycat_struct }
          ty_enum(_, _) { tycat_struct }
          ty_bot { tycat_bot }
          _ { tycat_other }
        }
    }

    const t: bool = true;
    const f: bool = false;

    /*.          add,     shift,   bit
      .             sub,     rel,     logic
      .                mult,    eq,         */
    /*other*/
    /*bool*/
    /*int*/
    /*float*/
    /*str*/
    /*vec*/
    /*bot*/
    let tbl =
        [[f, f, f, f, t, t, f, f], [f, f, f, f, t, t, t, t],
         [t, t, t, t, t, t, t, f], [t, t, t, f, t, t, f, f],
         [t, f, f, f, t, t, f, f], [t, f, f, f, t, t, f, f],
         [f, f, f, f, t, t, f, f], [t, t, t, t, t, t, t, t]]; /*struct*/

    ret tbl[tycat(ty)][opcat(op)];
}

fn ast_constr_to_constr<T>(tcx: ctxt, c: @ast::constr_general<T>) ->
   @constr_general<T> {
    alt tcx.def_map.find(c.node.id) {
      some(ast::def_fn(pred_id, ast::pure_fn)) {
        ret @ast_util::respan(c.span,
                              {path: c.node.path,
                               args: c.node.args,
                               id: pred_id});
      }
      _ {
        tcx.sess.span_fatal(c.span,
                            "Predicate " + path_to_str(c.node.path) +
                            " is unbound or bound to a non-function or an \
            impure function");
      }
    }
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
