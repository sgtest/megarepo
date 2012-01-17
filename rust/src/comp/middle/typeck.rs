import syntax::{ast, ast_util};
import ast::spanned;
import syntax::ast_util::{local_def, respan};
import syntax::visit;
import metadata::csearch;
import driver::session::session;
import util::common::*;
import syntax::codemap::span;
import pat_util::*;
import middle::ty;
import middle::ty::{node_id_to_type, arg, block_ty,
                    expr_ty, field, node_type_table, mk_nil,
                    ty_param_substs_opt_and_ty, ty_param_bounds_and_ty};
import util::ppaux::ty_to_str;
import middle::ty::unify::{ures_ok, ures_err, fix_ok, fix_err};
import core::{int, vec, str, option};
import std::smallintmap;
import std::map::{hashmap, new_int_hash};
import option::{none, some};
import syntax::print::pprust::*;

export check_crate;
export method_map, method_origin, method_static, method_param, method_iface;
export dict_map, dict_res, dict_origin, dict_static, dict_param, dict_iface;

tag method_origin {
    method_static(ast::def_id);
    // iface id, method num, param num, bound num
    method_param(ast::def_id, uint, uint, uint);
    method_iface(uint);
}
type method_map = hashmap<ast::node_id, method_origin>;

// Resolutions for bounds of all parameters, left to right, for a given path.
type dict_res = @[dict_origin];
tag dict_origin {
    dict_static(ast::def_id, [ty::t], dict_res);
    // Param number, bound number
    dict_param(uint, uint);
    dict_iface(ast::def_id);
}
type dict_map = hashmap<ast::node_id, dict_res>;

type ty_table = hashmap<ast::def_id, ty::t>;

// Used for typechecking the methods of an impl
tag self_info {
    self_impl(ty::t);
}

type crate_ctxt = {mutable self_infos: [self_info],
                   impl_map: resolve::impl_map,
                   method_map: method_map,
                   dict_map: dict_map,
                   tcx: ty::ctxt};

type fn_ctxt =
    // var_bindings, locals and next_var_id are shared
    // with any nested functions that capture the environment
    // (and with any functions whose environment is being captured).
    {ret_ty: ty::t,
     purity: ast::purity,
     proto: ast::proto,
     var_bindings: @ty::unify::var_bindings,
     locals: hashmap<ast::node_id, int>,
     next_var_id: @mutable int,
     mutable fixups: [ast::node_id],
     ccx: @crate_ctxt};


fn lookup_local(fcx: @fn_ctxt, sp: span, id: ast::node_id) -> int {
    alt fcx.locals.find(id) {
      some(x) { x }
      _ {
        fcx.ccx.tcx.sess.span_fatal(sp,
                                    "internal error looking up a local var")
      }
    }
}

fn lookup_def(fcx: @fn_ctxt, sp: span, id: ast::node_id) -> ast::def {
    alt fcx.ccx.tcx.def_map.find(id) {
      some(x) { x }
      _ {
        fcx.ccx.tcx.sess.span_fatal(sp,
                                    "internal error looking up a definition")
      }
    }
}

// Returns the type parameter count and the type for the given definition.
fn ty_param_bounds_and_ty_for_def(fcx: @fn_ctxt, sp: span, defn: ast::def) ->
   ty_param_bounds_and_ty {
    alt defn {
      ast::def_arg(id, _) {
        assert (fcx.locals.contains_key(id.node));
        let typ = ty::mk_var(fcx.ccx.tcx, lookup_local(fcx, sp, id.node));
        ret {bounds: @[], ty: typ};
      }
      ast::def_local(id, _) {
        assert (fcx.locals.contains_key(id.node));
        let typ = ty::mk_var(fcx.ccx.tcx, lookup_local(fcx, sp, id.node));
        ret {bounds: @[], ty: typ};
      }
      ast::def_self(id) {
        alt get_self_info(fcx.ccx) {
          some(self_impl(impl_t)) {
            ret {bounds: @[], ty: impl_t};
          }
        }
      }
      ast::def_fn(id, _) { ret ty::lookup_item_type(fcx.ccx.tcx, id); }
      ast::def_native_fn(id, _) { ret ty::lookup_item_type(fcx.ccx.tcx, id); }
      ast::def_const(id) { ret ty::lookup_item_type(fcx.ccx.tcx, id); }
      ast::def_variant(_, vid) { ret ty::lookup_item_type(fcx.ccx.tcx, vid); }
      ast::def_binding(id) {
        assert (fcx.locals.contains_key(id.node));
        let typ = ty::mk_var(fcx.ccx.tcx, lookup_local(fcx, sp, id.node));
        ret {bounds: @[], ty: typ};
      }
      ast::def_mod(_) {
        // Hopefully part of a path.
        // TODO: return a type that's more poisonous, perhaps?
        ret {bounds: @[], ty: ty::mk_nil(fcx.ccx.tcx)};
      }
      ast::def_ty(_) {
        fcx.ccx.tcx.sess.span_fatal(sp, "expected value but found type");
      }
      ast::def_upvar(_, inner, _) {
        ret ty_param_bounds_and_ty_for_def(fcx, sp, *inner);
      }
      _ {
        // FIXME: handle other names.
        fcx.ccx.tcx.sess.unimpl("definition variant");
      }
    }
}

// Instantiates the given path, which must refer to an item with the given
// number of type parameters and type.
fn instantiate_path(fcx: @fn_ctxt, pth: @ast::path,
                    tpt: ty_param_bounds_and_ty, sp: span)
    -> ty_param_substs_opt_and_ty {
    let ty_param_count = vec::len(*tpt.bounds);
    let vars = vec::init_fn({|_i| next_ty_var(fcx)}, ty_param_count);
    let ty_substs_len = vec::len(pth.node.types);
    if ty_substs_len > 0u {
        let param_var_len = vec::len(vars);
        if param_var_len == 0u {
            fcx.ccx.tcx.sess.span_fatal
                (sp, "this item does not take type parameters");
        } else if ty_substs_len > param_var_len {
            fcx.ccx.tcx.sess.span_fatal
                (sp, "too many type parameter provided for this item");
        } else if ty_substs_len < param_var_len {
            fcx.ccx.tcx.sess.span_fatal
                (sp, "not enough type parameters provided for this item");
        }
        vec::iter2(pth.node.types, vars) {|sub, var|
            let ty_subst = ast_ty_to_ty_crate(fcx.ccx, sub);
            demand::simple(fcx, pth.span, var, ty_subst);
        }
        if ty_param_count == 0u {
            fcx.ccx.tcx.sess.span_fatal(
                sp, "this item does not take type parameters");
        }
    }
    {substs: some(vars), ty: tpt.ty}
}

// Type tests
fn structurally_resolved_type(fcx: @fn_ctxt, sp: span, tp: ty::t) -> ty::t {
    alt ty::unify::resolve_type_structure(fcx.ccx.tcx, fcx.var_bindings, tp) {
      fix_ok(typ_s) { ret typ_s; }
      fix_err(_) {
        fcx.ccx.tcx.sess.span_fatal
            (sp, "the type of this value must be known in this context");
      }
    }
}


// Returns the one-level-deep structure of the given type.f
fn structure_of(fcx: @fn_ctxt, sp: span, typ: ty::t) -> ty::sty {
    ret ty::struct(fcx.ccx.tcx, structurally_resolved_type(fcx, sp, typ));
}

// Returns the one-level-deep structure of the given type or none if it
// is not known yet.
fn structure_of_maybe(fcx: @fn_ctxt, _sp: span, typ: ty::t) ->
   option::t<ty::sty> {
    let r =
        ty::unify::resolve_type_structure(fcx.ccx.tcx, fcx.var_bindings, typ);
    ret alt r {
          fix_ok(typ_s) { some(ty::struct(fcx.ccx.tcx, typ_s)) }
          fix_err(_) { none }
        }
}

fn type_is_integral(fcx: @fn_ctxt, sp: span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    ret ty::type_is_integral(fcx.ccx.tcx, typ_s);
}

fn type_is_scalar(fcx: @fn_ctxt, sp: span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    ret ty::type_is_scalar(fcx.ccx.tcx, typ_s);
}

fn type_is_c_like_enum(fcx: @fn_ctxt, sp: span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    ret ty::type_is_c_like_enum(fcx.ccx.tcx, typ_s);
}

// Parses the programmer's textual representation of a type into our internal
// notion of a type. `getter` is a function that returns the type
// corresponding to a definition ID:
fn default_arg_mode_for_ty(tcx: ty::ctxt, m: ast::mode,
                           ty: ty::t) -> ast::mode {
    alt m {
      ast::mode_infer. {
        alt ty::struct(tcx, ty) {
            ty::ty_var(_) { ast::mode_infer }
            _ {
                if ty::type_is_immediate(tcx, ty) { ast::by_val }
                else { ast::by_ref }
            }
        }
      }
      _ { m }
    }
}

tag mode { m_collect; m_check; m_check_tyvar(@fn_ctxt); }

fn ast_ty_to_ty(tcx: ty::ctxt, mode: mode, &&ast_ty: @ast::ty) -> ty::t {
    fn getter(tcx: ty::ctxt, mode: mode, id: ast::def_id)
        -> ty::ty_param_bounds_and_ty {
        alt mode {
          m_check. | m_check_tyvar(_) { ty::lookup_item_type(tcx, id) }
          m_collect. {
            if id.crate != ast::local_crate { csearch::get_type(tcx, id) }
            else {
                alt tcx.items.find(id.node) {
                  some(ast_map::node_item(item)) {
                    ty_of_item(tcx, mode, item)
                  }
                  some(ast_map::node_native_item(native_item)) {
                    ty_of_native_item(tcx, mode, native_item)
                  }
                }
            }
          }
        }
    }
    fn ast_arg_to_arg(tcx: ty::ctxt, mode: mode, arg: ast::arg)
        -> {mode: ty::mode, ty: ty::t} {
        let ty = ast_ty_to_ty(tcx, mode, arg.ty);
        ret {mode: default_arg_mode_for_ty(tcx, arg.mode, ty), ty: ty};
    }
    alt tcx.ast_ty_to_ty_cache.find(ast_ty) {
      some(some(ty)) { ret ty; }
      some(none.) {
        tcx.sess.span_fatal(ast_ty.span,
                            "illegal recursive type \
                              insert a tag in the cycle, \
                              if this is desired)");
      }
      none. { }
    } /* go on */

    tcx.ast_ty_to_ty_cache.insert(ast_ty, none::<ty::t>);
    fn ast_mt_to_mt(tcx: ty::ctxt, mode: mode, mt: ast::mt) -> ty::mt {
        ret {ty: ast_ty_to_ty(tcx, mode, mt.ty), mut: mt.mut};
    }
    fn instantiate(tcx: ty::ctxt, sp: span, mode: mode,
                   id: ast::def_id, args: [@ast::ty]) -> ty::t {
        // TODO: maybe record cname chains so we can do
        // "foo = int" like OCaml?

        let ty_param_bounds_and_ty = getter(tcx, mode, id);
        if vec::len(*ty_param_bounds_and_ty.bounds) == 0u {
            ret ty_param_bounds_and_ty.ty;
        }

        // The typedef is type-parametric. Do the type substitution.
        let param_bindings: [ty::t] = [];
        if vec::len(args) != vec::len(*ty_param_bounds_and_ty.bounds) {
            tcx.sess.span_fatal(sp, "Wrong number of type arguments for a \
                                     polymorphic type");
        }
        for ast_ty: @ast::ty in args {
            param_bindings += [ast_ty_to_ty(tcx, mode, ast_ty)];
        }
        let typ =
            ty::substitute_type_params(tcx, param_bindings,
                                       ty_param_bounds_and_ty.ty);
        ret typ;
    }
    let typ;
    alt ast_ty.node {
      ast::ty_nil. { typ = ty::mk_nil(tcx); }
      ast::ty_bot. { typ = ty::mk_bot(tcx); }
      ast::ty_bool. { typ = ty::mk_bool(tcx); }
      ast::ty_int(it) { typ = ty::mk_mach_int(tcx, it); }
      ast::ty_uint(uit) { typ = ty::mk_mach_uint(tcx, uit); }
      ast::ty_float(ft) { typ = ty::mk_mach_float(tcx, ft); }
      ast::ty_str. { typ = ty::mk_str(tcx); }
      ast::ty_box(mt) {
        typ = ty::mk_box(tcx, ast_mt_to_mt(tcx, mode, mt));
      }
      ast::ty_uniq(mt) {
        typ = ty::mk_uniq(tcx, ast_mt_to_mt(tcx, mode, mt));
      }
      ast::ty_vec(mt) {
        typ = ty::mk_vec(tcx, ast_mt_to_mt(tcx, mode, mt));
      }
      ast::ty_ptr(mt) {
        typ = ty::mk_ptr(tcx, ast_mt_to_mt(tcx, mode, mt));
      }
      ast::ty_tup(fields) {
        let flds = vec::map(fields, bind ast_ty_to_ty(tcx, mode, _));
        typ = ty::mk_tup(tcx, flds);
      }
      ast::ty_rec(fields) {
        let flds: [field] = [];
        for f: ast::ty_field in fields {
            let tm = ast_mt_to_mt(tcx, mode, f.node.mt);
            flds += [{ident: f.node.ident, mt: tm}];
        }
        typ = ty::mk_rec(tcx, flds);
      }
      ast::ty_fn(proto, decl) {
        typ = ty::mk_fn(tcx, ty_of_fn_decl(tcx, mode, proto, decl));
      }
      ast::ty_path(path, id) {
        alt tcx.def_map.find(id) {
          some(ast::def_ty(id)) {
            typ = instantiate(tcx, ast_ty.span, mode, id, path.node.types);
          }
          some(ast::def_native_ty(id)) { typ = getter(tcx, mode, id).ty; }
          some(ast::def_ty_param(id, n)) {
            typ = ty::mk_param(tcx, n, id);
          }
          some(_) {
            tcx.sess.span_fatal(ast_ty.span,
                                "found type name used as a variable");
          }
          _ {
            tcx.sess.span_fatal(ast_ty.span, "internal error in instantiate");
          }
        }
      }
      ast::ty_constr(t, cs) {
        let out_cs = [];
        for constr: @ast::ty_constr in cs {
            out_cs += [ty::ast_constr_to_constr(tcx, constr)];
        }
        typ = ty::mk_constr(tcx, ast_ty_to_ty(tcx, mode, t), out_cs);
      }
      ast::ty_infer. {
        alt mode {
          m_check_tyvar(fcx) { ret next_ty_var(fcx); }
          _ { tcx.sess.span_bug(ast_ty.span,
                                "found `ty_infer` in unexpected place"); }
        }
      }
    }
    tcx.ast_ty_to_ty_cache.insert(ast_ty, some(typ));
    ret typ;
}

fn ty_of_item(tcx: ty::ctxt, mode: mode, it: @ast::item)
    -> ty::ty_param_bounds_and_ty {
    alt it.node {
      ast::item_const(t, _) {
        let typ = ast_ty_to_ty(tcx, mode, t);
        let tpt = {bounds: @[], ty: typ};
        tcx.tcache.insert(local_def(it.id), tpt);
        ret tpt;
      }
      ast::item_fn(decl, tps, _) {
        ret ty_of_fn(tcx, mode, decl, tps, local_def(it.id));
      }
      ast::item_ty(t, tps) {
        alt tcx.tcache.find(local_def(it.id)) {
          some(tpt) { ret tpt; }
          none. { }
        }
        // Tell ast_ty_to_ty() that we want to perform a recursive
        // call to resolve any named types.
        let tpt = {bounds: ty_param_bounds(tcx, mode, tps),
                   ty: ty::mk_named(tcx, ast_ty_to_ty(tcx, mode, t),
                                    @it.ident)};
        tcx.tcache.insert(local_def(it.id), tpt);
        ret tpt;
      }
      ast::item_res(decl, tps, _, _, _) {
        let {bounds, params} = mk_ty_params(tcx, tps);
        let t_arg = ty_of_arg(tcx, mode, decl.inputs[0]);
        let t = ty::mk_named(tcx, ty::mk_res(tcx, local_def(it.id), t_arg.ty,
                                             params),
                             @it.ident);
        let t_res = {bounds: bounds, ty: t};
        tcx.tcache.insert(local_def(it.id), t_res);
        ret t_res;
      }
      ast::item_tag(_, tps) {
        // Create a new generic polytype.
        let {bounds, params} = mk_ty_params(tcx, tps);
        let t = ty::mk_named(tcx, ty::mk_tag(tcx, local_def(it.id), params),
                             @it.ident);
        let tpt = {bounds: bounds, ty: t};
        tcx.tcache.insert(local_def(it.id), tpt);
        ret tpt;
      }
      ast::item_iface(tps, ms) {
        let {bounds, params} = mk_ty_params(tcx, tps);
        let t = ty::mk_named(tcx, ty::mk_iface(tcx, local_def(it.id),
                                               params),
                             @it.ident);
        let tpt = {bounds: bounds, ty: t};
        tcx.tcache.insert(local_def(it.id), tpt);
        ret tpt;
      }
      ast::item_impl(_, _, _, _) | ast::item_mod(_) |
      ast::item_native_mod(_) { fail; }
    }
}
fn ty_of_native_item(tcx: ty::ctxt, mode: mode, it: @ast::native_item)
    -> ty::ty_param_bounds_and_ty {
    alt it.node {
      ast::native_item_fn(fn_decl, params) {
        ret ty_of_native_fn_decl(tcx, mode, fn_decl, params,
                                 ast_util::local_def(it.id));
      }
      ast::native_item_ty. {
        alt tcx.tcache.find(local_def(it.id)) {
          some(tpt) { ret tpt; }
          none. { }
        }
        let t = ty::mk_native(tcx, ast_util::local_def(it.id));
        let t = ty::mk_named(tcx, t, @it.ident);
        let tpt = {bounds: @[], ty: t};
        tcx.tcache.insert(local_def(it.id), tpt);
        ret tpt;
      }
    }
}
fn ty_of_arg(tcx: ty::ctxt, mode: mode, a: ast::arg) -> ty::arg {
    let ty = ast_ty_to_ty(tcx, mode, a.ty);
    {mode: default_arg_mode_for_ty(tcx, a.mode, ty), ty: ty}
}
fn ty_of_fn_decl(tcx: ty::ctxt, mode: mode,
                 proto: ast::proto, decl: ast::fn_decl) -> ty::fn_ty {
    let input_tys = [];
    for a: ast::arg in decl.inputs { input_tys += [ty_of_arg(tcx, mode, a)]; }
    let output_ty = ast_ty_to_ty(tcx, mode, decl.output);

    let out_constrs = [];
    for constr: @ast::constr in decl.constraints {
        out_constrs += [ty::ast_constr_to_constr(tcx, constr)];
    }
    {proto: proto, inputs: input_tys,
     output: output_ty, ret_style: decl.cf, constraints: out_constrs}
}
fn ty_of_fn(tcx: ty::ctxt, mode: mode, decl: ast::fn_decl,
            ty_params: [ast::ty_param], def_id: ast::def_id)
    -> ty::ty_param_bounds_and_ty {
    let bounds = ty_param_bounds(tcx, mode, ty_params);
    let tofd = ty_of_fn_decl(tcx, mode, ast::proto_bare, decl);
    let tpt = {bounds: bounds, ty: ty::mk_fn(tcx, tofd)};
    tcx.tcache.insert(def_id, tpt);
    ret tpt;
}
fn ty_of_native_fn_decl(tcx: ty::ctxt, mode: mode, decl: ast::fn_decl,
                        ty_params: [ast::ty_param], def_id: ast::def_id)
    -> ty::ty_param_bounds_and_ty {
    let input_tys = [], bounds = ty_param_bounds(tcx, mode, ty_params);
    for a: ast::arg in decl.inputs { input_tys += [ty_of_arg(tcx, mode, a)]; }
    let output_ty = ast_ty_to_ty(tcx, mode, decl.output);

    let t_fn = ty::mk_native_fn(tcx, input_tys, output_ty);
    let tpt = {bounds: bounds, ty: t_fn};
    tcx.tcache.insert(def_id, tpt);
    ret tpt;
}
fn ty_param_bounds(tcx: ty::ctxt, mode: mode, params: [ast::ty_param])
    -> @[ty::param_bounds] {
    let result = [];
    for param in params {
        result += [alt tcx.ty_param_bounds.find(param.id) {
          some(bs) { bs }
          none. {
            let bounds = [];
            for b in *param.bounds {
                bounds += [alt b {
                  ast::bound_send. { ty::bound_send }
                  ast::bound_copy. { ty::bound_copy }
                  ast::bound_iface(t) {
                    let ity = ast_ty_to_ty(tcx, mode, t);
                    alt ty::struct(tcx, ity) {
                      ty::ty_iface(_, _) {}
                      _ {
                        tcx.sess.span_fatal(
                            t.span, "type parameter bounds must be \
                                     interface types");
                      }
                    }
                    ty::bound_iface(ity)
                  }
                }];
            }
            let boxed = @bounds;
            tcx.ty_param_bounds.insert(param.id, boxed);
            boxed
          }
        }];
    }
    @result
}
fn ty_of_method(tcx: ty::ctxt, mode: mode, m: @ast::method) -> ty::method {
    {ident: m.ident, tps: ty_param_bounds(tcx, mode, m.tps),
     fty: ty_of_fn_decl(tcx, mode, ast::proto_bare, m.decl)}
}
fn ty_of_ty_method(tcx: ty::ctxt, mode: mode, m: ast::ty_method)
    -> ty::method {
    {ident: m.ident, tps: ty_param_bounds(tcx, mode, m.tps),
     fty: ty_of_fn_decl(tcx, mode, ast::proto_bare, m.decl)}
}

// A convenience function to use a crate_ctxt to resolve names for
// ast_ty_to_ty.
fn ast_ty_to_ty_crate(ccx: @crate_ctxt, &&ast_ty: @ast::ty) -> ty::t {
    ret ast_ty_to_ty(ccx.tcx, m_check, ast_ty);
}

// A wrapper around ast_ty_to_ty_crate that handles ty_infer.
fn ast_ty_to_ty_crate_infer(ccx: @crate_ctxt, &&ast_ty: @ast::ty) ->
   option::t<ty::t> {
    alt ast_ty.node {
      ast::ty_infer. { none }
      _ { some(ast_ty_to_ty_crate(ccx, ast_ty)) }
    }
}


// Functions that write types into the node type table.
mod write {
    fn inner(ntt: node_type_table, node_id: ast::node_id,
             tpot: ty_param_substs_opt_and_ty) {
        smallintmap::insert(*ntt, node_id as uint, tpot);
    }

    // Writes a type parameter count and type pair into the node type table.
    fn ty(tcx: ty::ctxt, node_id: ast::node_id,
          tpot: ty_param_substs_opt_and_ty) {
        assert (!ty::type_contains_vars(tcx, tpot.ty));
        inner(tcx.node_types, node_id, tpot);
    }

    // Writes a type parameter count and type pair into the node type table.
    // This function allows for the possibility of type variables, which will
    // be rewritten later during the fixup mode.
    fn ty_fixup(fcx: @fn_ctxt, node_id: ast::node_id,
                tpot: ty_param_substs_opt_and_ty) {
        inner(fcx.ccx.tcx.node_types, node_id, tpot);
        if ty::type_contains_vars(fcx.ccx.tcx, tpot.ty) {
            fcx.fixups += [node_id];
        }
    }

    // Writes a type with no type parameters into the node type table.
    fn ty_only(tcx: ty::ctxt, node_id: ast::node_id, typ: ty::t) {
        ty(tcx, node_id, {substs: none::<[ty::t]>, ty: typ});
    }

    // Writes a type with no type parameters into the node type table. This
    // function allows for the possibility of type variables.
    fn ty_only_fixup(fcx: @fn_ctxt, node_id: ast::node_id, typ: ty::t) {
        ret ty_fixup(fcx, node_id, {substs: none::<[ty::t]>, ty: typ});
    }

    // Writes a nil type into the node type table.
    fn nil_ty(tcx: ty::ctxt, node_id: ast::node_id) {
        ret ty(tcx, node_id, {substs: none::<[ty::t]>, ty: ty::mk_nil(tcx)});
    }

    // Writes the bottom type into the node type table.
    fn bot_ty(tcx: ty::ctxt, node_id: ast::node_id) {
        ret ty(tcx, node_id, {substs: none::<[ty::t]>, ty: ty::mk_bot(tcx)});
    }
}

fn mk_ty_params(tcx: ty::ctxt, atps: [ast::ty_param])
    -> {bounds: @[ty::param_bounds], params: [ty::t]} {
    let i = 0u, bounds = ty_param_bounds(tcx, m_collect, atps);
    {bounds: bounds,
     params: vec::map(atps, {|atp|
         let t = ty::mk_param(tcx, i, local_def(atp.id));
         i += 1u;
         t
     })}
}

fn compare_impl_method(tcx: ty::ctxt, sp: span, impl_m: ty::method,
                       impl_tps: uint, if_m: ty::method, substs: [ty::t]) {
    if impl_m.tps != if_m.tps {
        tcx.sess.span_err(sp, "method `" + if_m.ident +
                          "` has an incompatible set of type parameters");
    } else {
        let impl_fty = ty::mk_fn(tcx, impl_m.fty);
        // Add dummy substs for the parameters of the impl method
        let substs = substs + vec::init_fn({|i|
            ty::mk_param(tcx, i + impl_tps, {crate: 0, node: 0})
        }, vec::len(*if_m.tps));
        let if_fty = ty::substitute_type_params(tcx, substs,
                                                ty::mk_fn(tcx, if_m.fty));
        alt ty::unify::unify(impl_fty, if_fty, ty::unify::precise, tcx) {
          ty::unify::ures_err(err) {
            tcx.sess.span_err(sp, "method `" + if_m.ident +
                              "` has an incompatible type: " +
                              ty::type_err_to_str(err));
          }
          _ {}
        }
    }
}

// Item collection - a pair of bootstrap passes:
//
// (1) Collect the IDs of all type items (typedefs) and store them in a table.
//
// (2) Translate the AST fragments that describe types to determine a type for
//     each item. When we encounter a named type, we consult the table built
//     in pass 1 to find its item, and recursively translate it.
//
// We then annotate the AST with the resulting types and return the annotated
// AST, along with a table mapping item IDs to their types.
//
// TODO: This logic is quite convoluted; it's a relic of the time when we
// actually wrote types directly into the AST and didn't have a type cache.
// Could use some cleanup. Consider topologically sorting in phase (1) above.
mod collect {
    type ctxt = {tcx: ty::ctxt};

    fn get_tag_variant_types(cx: @ctxt, tag_ty: ty::t,
                             variants: [ast::variant],
                             ty_params: [ast::ty_param]) {
        // Create a set of parameter types shared among all the variants.

        for variant: ast::variant in variants {
            // Nullary tag constructors get turned into constants; n-ary tag
            // constructors get turned into functions.

            let result_ty = if vec::len(variant.node.args) == 0u {
                tag_ty
            } else {
                // As above, tell ast_ty_to_ty() that trans_ty_item_to_ty()
                // should be called to resolve named types.
                let args: [arg] = [];
                for va: ast::variant_arg in variant.node.args {
                    let arg_ty = ast_ty_to_ty(cx.tcx, m_collect, va.ty);
                    args += [{mode: ast::by_copy, ty: arg_ty}];
                }
                // FIXME: this will be different for constrained types
                ty::mk_fn(cx.tcx,
                          {proto: ast::proto_box,
                           inputs: args, output: tag_ty,
                           ret_style: ast::return_val, constraints: []})
            };
            let tpt = {bounds: ty_param_bounds(cx.tcx, m_collect, ty_params),
                       ty: result_ty};
            cx.tcx.tcache.insert(local_def(variant.node.id), tpt);
            write::ty_only(cx.tcx, variant.node.id, result_ty);
        }
    }
    fn convert(cx: @ctxt, it: @ast::item) {
        alt it.node {
          // These don't define types.
          ast::item_mod(_) | ast::item_native_mod(_) {}
          ast::item_tag(variants, ty_params) {
            let tpt = ty_of_item(cx.tcx, m_collect, it);
            write::ty_only(cx.tcx, it.id, tpt.ty);
            get_tag_variant_types(cx, tpt.ty, variants, ty_params);
          }
          ast::item_impl(tps, ifce, selfty, ms) {
            let i_bounds = ty_param_bounds(cx.tcx, m_collect, tps);
            let my_methods = [];
            for m in ms {
                let bounds = ty_param_bounds(cx.tcx, m_collect, m.tps);
                let mty = ty_of_method(cx.tcx, m_collect, m);
                my_methods += [mty];
                let fty = ty::mk_fn(cx.tcx, mty.fty);
                cx.tcx.tcache.insert(local_def(m.id),
                                     {bounds: @(*i_bounds + *bounds),
                                      ty: fty});
                write::ty_only(cx.tcx, m.id, fty);
            }
            write::ty_only(cx.tcx, it.id, ast_ty_to_ty(cx.tcx, m_collect,
                                                       selfty));
            alt ifce {
              some(t) {
                let iface_ty = ast_ty_to_ty(cx.tcx, m_collect, t);
                cx.tcx.tcache.insert(local_def(it.id),
                                     {bounds: i_bounds, ty: iface_ty});
                alt ty::struct(cx.tcx, iface_ty) {
                  ty::ty_iface(did, tys) {
                    for if_m in *ty::iface_methods(cx.tcx, did) {
                        alt vec::find(my_methods,
                                      {|m| if_m.ident == m.ident}) {
                          some(m) {
                            compare_impl_method(cx.tcx, t.span, m,
                                                vec::len(tps), if_m, tys);
                          }
                          none. {
                            cx.tcx.sess.span_err(t.span, "missing method `" +
                                                 if_m.ident + "`");
                          }
                        }
                    }
                  }
                  _ {
                    cx.tcx.sess.span_fatal(t.span, "can only implement \
                                                    interface types");
                  }
                }
              }
              _ {}
            }
          }
          ast::item_res(decl, tps, _, dtor_id, ctor_id) {
            let {bounds, params} = mk_ty_params(cx.tcx, tps);
            let t_arg = ty_of_arg(cx.tcx, m_collect, decl.inputs[0]);
            let t_res = ty::mk_res(cx.tcx, local_def(it.id), t_arg.ty,
                                   params);
            let t_ctor = ty::mk_fn(cx.tcx, {
                proto: ast::proto_box,
                inputs: [{mode: ast::by_copy with t_arg}],
                output: t_res,
                ret_style: ast::return_val, constraints: []
            });
            let t_dtor = ty::mk_fn(cx.tcx, {
                proto: ast::proto_box,
                inputs: [t_arg], output: ty::mk_nil(cx.tcx),
                ret_style: ast::return_val, constraints: []
            });
            write::ty_only(cx.tcx, it.id, t_res);
            write::ty_only(cx.tcx, ctor_id, t_ctor);
            cx.tcx.tcache.insert(local_def(ctor_id),
                                 {bounds: bounds, ty: t_ctor});
            write::ty_only(cx.tcx, dtor_id, t_dtor);
          }
          ast::item_iface(_, ms) {
            let tpt = ty_of_item(cx.tcx, m_collect, it);
            write::ty_only(cx.tcx, it.id, tpt.ty);
            ty::store_iface_methods(cx.tcx, it.id, @vec::map(ms, {|m|
                ty_of_ty_method(cx.tcx, m_collect, m)
            }));
          }
          _ {
            // This call populates the type cache with the converted type
            // of the item in passing. All we have to do here is to write
            // it into the node type table.
            let tpt = ty_of_item(cx.tcx, m_collect, it);
            write::ty_only(cx.tcx, it.id, tpt.ty);
          }
        }
    }
    fn convert_native(cx: @ctxt, i: @ast::native_item) {
        // As above, this call populates the type table with the converted
        // type of the native item. We simply write it into the node type
        // table.
        let tpt = ty_of_native_item(cx.tcx, m_collect, i);
        alt i.node {
          ast::native_item_ty. {
            // FIXME: Native types have no annotation. Should they? --pcw
          }
          ast::native_item_fn(_, _) {
            write::ty_only(cx.tcx, i.id, tpt.ty);
          }
        }
    }
    fn collect_item_types(tcx: ty::ctxt, crate: @ast::crate) {
        let cx = @{tcx: tcx};
        let visit =
            visit::mk_simple_visitor(@{visit_item: bind convert(cx, _),
                                       visit_native_item:
                                           bind convert_native(cx, _)
                                       with
                                          *visit::default_simple_visitor()});
        visit::visit_crate(*crate, (), visit);
    }
}


// Type unification
mod unify {
    fn unify(fcx: @fn_ctxt, expected: ty::t, actual: ty::t) ->
       ty::unify::result {
        ret ty::unify::unify(expected, actual,
                             ty::unify::in_bindings(fcx.var_bindings),
                             fcx.ccx.tcx);
    }
}


// FIXME This is almost a duplicate of ty::type_autoderef, with structure_of
// instead of ty::struct.
fn do_autoderef(fcx: @fn_ctxt, sp: span, t: ty::t) -> ty::t {
    let t1 = t;
    while true {
        alt structure_of(fcx, sp, t1) {
          ty::ty_box(inner) | ty::ty_uniq(inner) {
            alt ty::struct(fcx.ccx.tcx, t1) {
              ty::ty_var(v1) {
                if ty::occurs_check_fails(fcx.ccx.tcx, some(sp), v1,
                                          ty::mk_box(fcx.ccx.tcx, inner)) {
                    break;
                }
              }
              _ { }
            }
            t1 = inner.ty;
          }
          ty::ty_res(_, inner, tps) {
            t1 = ty::substitute_type_params(fcx.ccx.tcx, tps, inner);
          }
          ty::ty_tag(did, tps) {
            let variants = ty::tag_variants(fcx.ccx.tcx, did);
            if vec::len(*variants) != 1u || vec::len(variants[0].args) != 1u {
                ret t1;
            }
            t1 =
                ty::substitute_type_params(fcx.ccx.tcx, tps,
                                           variants[0].args[0]);
          }
          _ { ret t1; }
        }
    }
    fail;
}

fn resolve_type_vars_if_possible(fcx: @fn_ctxt, typ: ty::t) -> ty::t {
    alt ty::unify::fixup_vars(fcx.ccx.tcx, none, fcx.var_bindings, typ) {
      fix_ok(new_type) { ret new_type; }
      fix_err(_) { ret typ; }
    }
}


// Demands - procedures that require that two types unify and emit an error
// message if they don't.
type ty_param_substs_and_ty = {substs: [ty::t], ty: ty::t};

mod demand {
    fn simple(fcx: @fn_ctxt, sp: span, expected: ty::t, actual: ty::t) ->
       ty::t {
        full(fcx, sp, expected, actual, []).ty
    }

    fn with_substs(fcx: @fn_ctxt, sp: span, expected: ty::t, actual: ty::t,
                   ty_param_substs_0: [ty::t]) -> ty_param_substs_and_ty {
        full(fcx, sp, expected, actual, ty_param_substs_0)
    }

    // Requires that the two types unify, and prints an error message if they
    // don't. Returns the unified type and the type parameter substitutions.
    fn full(fcx: @fn_ctxt, sp: span, expected: ty::t, actual: ty::t,
            ty_param_substs_0: [ty::t]) ->
       ty_param_substs_and_ty {

        let ty_param_substs: [mutable ty::t] = [mutable];
        let ty_param_subst_var_ids: [int] = [];
        for ty_param_subst: ty::t in ty_param_substs_0 {
            // Generate a type variable and unify it with the type parameter
            // substitution. We will then pull out these type variables.
            let t_0 = next_ty_var(fcx);
            ty_param_substs += [mutable t_0];
            ty_param_subst_var_ids += [ty::ty_var_id(fcx.ccx.tcx, t_0)];
            simple(fcx, sp, ty_param_subst, t_0);
        }

        fn mk_result(fcx: @fn_ctxt, result_ty: ty::t,
                     ty_param_subst_var_ids: [int]) ->
           ty_param_substs_and_ty {
            let result_ty_param_substs: [ty::t] = [];
            for var_id: int in ty_param_subst_var_ids {
                let tp_subst = ty::mk_var(fcx.ccx.tcx, var_id);
                result_ty_param_substs += [tp_subst];
            }
            ret {substs: result_ty_param_substs, ty: result_ty};
        }


        alt unify::unify(fcx, expected, actual) {
          ures_ok(t) { ret mk_result(fcx, t, ty_param_subst_var_ids); }
          ures_err(err) {
            let e_err = resolve_type_vars_if_possible(fcx, expected);
            let a_err = resolve_type_vars_if_possible(fcx, actual);
            fcx.ccx.tcx.sess.span_err(sp,
                                      "mismatched types: expected `" +
                                          ty_to_str(fcx.ccx.tcx, e_err) +
                                          "` but found `" +
                                          ty_to_str(fcx.ccx.tcx, a_err) +
                                          "` (" + ty::type_err_to_str(err) +
                                          ")");
            ret mk_result(fcx, expected, ty_param_subst_var_ids);
          }
        }
    }
}


// Returns true if the two types unify and false if they don't.
fn are_compatible(fcx: @fn_ctxt, expected: ty::t, actual: ty::t) -> bool {
    alt unify::unify(fcx, expected, actual) {
      ures_ok(_) { ret true; }
      ures_err(_) { ret false; }
    }
}


// Returns the types of the arguments to a tag variant.
fn variant_arg_types(ccx: @crate_ctxt, _sp: span, vid: ast::def_id,
                     tag_ty_params: [ty::t]) -> [ty::t] {
    let result: [ty::t] = [];
    let tpt = ty::lookup_item_type(ccx.tcx, vid);
    alt ty::struct(ccx.tcx, tpt.ty) {
      ty::ty_fn(f) {
        // N-ary variant.
        for arg: ty::arg in f.inputs {
            let arg_ty =
                ty::substitute_type_params(ccx.tcx, tag_ty_params, arg.ty);
            result += [arg_ty];
        }
      }
      _ {
        // Nullary variant. Do nothing, as there are no arguments.
      }
    }
    /* result is a vector of the *expected* types of all the fields */

    ret result;
}


// Type resolution: the phase that finds all the types in the AST with
// unresolved type variables and replaces "ty_var" types with their
// substitutions.
//
// TODO: inefficient since not all types have vars in them. It would be better
// to maintain a list of fixups.
mod writeback {

    export resolve_type_vars_in_block;
    export resolve_type_vars_in_expr;

    fn resolve_type_vars_in_type(fcx: @fn_ctxt, sp: span, typ: ty::t) ->
       option::t<ty::t> {
        if !ty::type_contains_vars(fcx.ccx.tcx, typ) { ret some(typ); }
        alt ty::unify::fixup_vars(fcx.ccx.tcx, some(sp), fcx.var_bindings,
                                  typ) {
          fix_ok(new_type) { ret some(new_type); }
          fix_err(vid) {
            fcx.ccx.tcx.sess.span_err(sp, "cannot determine a type \
                                           for this expression");
            ret none;
          }
        }
    }
    fn resolve_type_vars_for_node(wbcx: wb_ctxt, sp: span, id: ast::node_id) {
        let fcx = wbcx.fcx;
        let tpot = ty::node_id_to_ty_param_substs_opt_and_ty(fcx.ccx.tcx, id);
        let new_ty =
            alt resolve_type_vars_in_type(fcx, sp, tpot.ty) {
              some(t) { t }
              none. { wbcx.success = false; ret }
            };
        let new_substs_opt;
        alt tpot.substs {
          none. { new_substs_opt = none; }
          some(substs) {
            let new_substs: [ty::t] = [];
            for subst: ty::t in substs {
                alt resolve_type_vars_in_type(fcx, sp, subst) {
                  some(t) { new_substs += [t]; }
                  none. { wbcx.success = false; ret; }
                }
            }
            new_substs_opt = some(new_substs);
          }
        }
        write::ty(fcx.ccx.tcx, id, {substs: new_substs_opt, ty: new_ty});
    }

    type wb_ctxt =
        // As soon as we hit an error we have to stop resolving
        // the entire function
        {fcx: @fn_ctxt, mutable success: bool};
    type wb_vt = visit::vt<wb_ctxt>;

    fn visit_stmt(s: @ast::stmt, wbcx: wb_ctxt, v: wb_vt) {
        if !wbcx.success { ret; }
        resolve_type_vars_for_node(wbcx, s.span, ty::stmt_node_id(s));
        visit::visit_stmt(s, wbcx, v);
    }
    fn visit_expr(e: @ast::expr, wbcx: wb_ctxt, v: wb_vt) {
        if !wbcx.success { ret; }
        resolve_type_vars_for_node(wbcx, e.span, e.id);
        alt e.node {
          ast::expr_fn(_, decl, _, _) |
          ast::expr_fn_block(decl, _) {
            for input in decl.inputs {
                resolve_type_vars_for_node(wbcx, e.span, input.id);
            }
          }
          _ { }
        }
        visit::visit_expr(e, wbcx, v);
    }
    fn visit_block(b: ast::blk, wbcx: wb_ctxt, v: wb_vt) {
        if !wbcx.success { ret; }
        resolve_type_vars_for_node(wbcx, b.span, b.node.id);
        visit::visit_block(b, wbcx, v);
    }
    fn visit_pat(p: @ast::pat, wbcx: wb_ctxt, v: wb_vt) {
        if !wbcx.success { ret; }
        resolve_type_vars_for_node(wbcx, p.span, p.id);
        visit::visit_pat(p, wbcx, v);
    }
    fn visit_local(l: @ast::local, wbcx: wb_ctxt, v: wb_vt) {
        if !wbcx.success { ret; }
        let var_id = lookup_local(wbcx.fcx, l.span, l.node.id);
        let fix_rslt =
            ty::unify::resolve_type_var(wbcx.fcx.ccx.tcx, some(l.span),
                                        wbcx.fcx.var_bindings, var_id);
        alt fix_rslt {
          fix_ok(lty) { write::ty_only(wbcx.fcx.ccx.tcx, l.node.id, lty); }
          fix_err(_) {
            wbcx.fcx.ccx.tcx.sess.span_err(l.span,
                                           "cannot determine a type \
                                                for this local variable");
            wbcx.success = false;
          }
        }
        visit::visit_local(l, wbcx, v);
    }
    fn visit_item(_item: @ast::item, _wbcx: wb_ctxt, _v: wb_vt) {
        // Ignore items
    }

    fn resolve_type_vars_in_expr(fcx: @fn_ctxt, e: @ast::expr) -> bool {
        let wbcx = {fcx: fcx, mutable success: true};
        let visit =
            visit::mk_vt(@{visit_item: visit_item,
                           visit_stmt: visit_stmt,
                           visit_expr: visit_expr,
                           visit_block: visit_block,
                           visit_pat: visit_pat,
                           visit_local: visit_local
                              with *visit::default_visitor()});
        visit::visit_expr(e, wbcx, visit);
        ret wbcx.success;
    }

    fn resolve_type_vars_in_block(fcx: @fn_ctxt, blk: ast::blk) -> bool {
        let wbcx = {fcx: fcx, mutable success: true};
        let visit =
            visit::mk_vt(@{visit_item: visit_item,
                           visit_stmt: visit_stmt,
                           visit_expr: visit_expr,
                           visit_block: visit_block,
                           visit_pat: visit_pat,
                           visit_local: visit_local
                              with *visit::default_visitor()});
        visit.visit_block(blk, wbcx, visit);
        ret wbcx.success;
    }
}


// Local variable gathering. We gather up all locals and create variable IDs
// for them before typechecking the function.
type gather_result =
    {var_bindings: @ty::unify::var_bindings,
     locals: hashmap<ast::node_id, int>,
     next_var_id: @mutable int};

// Used only as a helper for check_fn.
fn gather_locals(ccx: @crate_ctxt,
                 decl: ast::fn_decl,
                 body: ast::blk,
                 id: ast::node_id,
                 old_fcx: option::t<@fn_ctxt>) -> gather_result {
    let {vb: vb, locals: locals, nvi: nvi} =
        alt old_fcx {
          none. {
            {vb: ty::unify::mk_var_bindings(),
             locals: new_int_hash::<int>(),
             nvi: @mutable 0}
          }
          some(fcx) {
            {vb: fcx.var_bindings,
             locals: fcx.locals,
             nvi: fcx.next_var_id}
          }
        };
    let tcx = ccx.tcx;

    let next_var_id = fn@() -> int { let rv = *nvi; *nvi += 1; ret rv; };
    let assign = fn@(nid: ast::node_id, ty_opt: option::t<ty::t>) {
            let var_id = next_var_id();
            locals.insert(nid, var_id);
            alt ty_opt {
              none. {/* nothing to do */ }
              some(typ) {
                ty::unify::unify(ty::mk_var(tcx, var_id), typ,
                                 ty::unify::in_bindings(vb), tcx);
              }
            }
        };

    // Add formal parameters.
    let args = ty::ty_fn_args(ccx.tcx, ty::node_id_to_type(ccx.tcx, id));
    let i = 0u;
    for arg: ty::arg in args {
        assign(decl.inputs[i].id, some(arg.ty));
        i += 1u;
    }

    // Add explicitly-declared locals.
    let visit_local = fn@(local: @ast::local, &&e: (), v: visit::vt<()>) {
            let local_ty = ast_ty_to_ty_crate_infer(ccx, local.node.ty);
            assign(local.node.id, local_ty);
            visit::visit_local(local, e, v);
        };

    // Add pattern bindings.
    let visit_pat = fn@(p: @ast::pat, &&e: (), v: visit::vt<()>) {
        alt normalize_pat(ccx.tcx, p).node {
              ast::pat_ident(_, _) { assign(p.id, none); }
              _ {/* no-op */ }
            }
            visit::visit_pat(p, e, v);
        };

    // Don't descend into fns and items
    fn visit_fn<T>(_fk: visit::fn_kind, _decl: ast::fn_decl, _body: ast::blk,
                   _sp: span, _id: ast::node_id, _t: T, _v: visit::vt<T>) {
    }
    fn visit_item<E>(_i: @ast::item, _e: E, _v: visit::vt<E>) { }

    let visit =
        @{visit_local: visit_local,
          visit_pat: visit_pat,
          visit_fn: bind visit_fn(_, _, _, _, _, _, _),
          visit_item: bind visit_item(_, _, _)
              with *visit::default_visitor()};

    visit::visit_block(body, (), visit::mk_vt(visit));
    ret {var_bindings: vb,
         locals: locals,
         next_var_id: nvi};
}

// AST fragment checking
fn check_lit(ccx: @crate_ctxt, lit: @ast::lit) -> ty::t {
    alt lit.node {
      ast::lit_str(_) { ty::mk_str(ccx.tcx) }
      ast::lit_int(_, t) { ty::mk_mach_int(ccx.tcx, t) }
      ast::lit_uint(_, t) { ty::mk_mach_uint(ccx.tcx, t) }
      ast::lit_float(_, t) { ty::mk_mach_float(ccx.tcx, t) }
      ast::lit_nil. { ty::mk_nil(ccx.tcx) }
      ast::lit_bool(_) { ty::mk_bool(ccx.tcx) }
    }
}

fn valid_range_bounds(from: @ast::expr, to: @ast::expr) -> bool {
    ast_util::compare_lit_exprs(from, to) <= 0
}

// Pattern checking is top-down rather than bottom-up so that bindings get
// their types immediately.
fn check_pat(fcx: @fn_ctxt, map: pat_util::pat_id_map, pat: @ast::pat,
             expected: ty::t) {
    alt normalize_pat(fcx.ccx.tcx, pat).node {
      ast::pat_wild. {
          alt structure_of(fcx, pat.span, expected) {
                  ty::ty_tag(_, expected_tps) {
                      let path_tpt = {substs: some(expected_tps),
                                      ty: expected};
                      write::ty_fixup(fcx, pat.id, path_tpt);
                  }
                  _ {
                      write::ty_only_fixup(fcx, pat.id, expected);
                  }
              }
      }
      ast::pat_lit(lt) {
        check_expr_with(fcx, lt, expected);
        write::ty_only_fixup(fcx, pat.id, expr_ty(fcx.ccx.tcx, lt));
      }
      ast::pat_range(begin, end) {
        check_expr_with(fcx, begin, expected);
        check_expr_with(fcx, end, expected);
        let b_ty = resolve_type_vars_if_possible(fcx, expr_ty(fcx.ccx.tcx,
                                                              begin));
        if !ty::same_type(fcx.ccx.tcx, b_ty, resolve_type_vars_if_possible(
            fcx, expr_ty(fcx.ccx.tcx, end))) {
            fcx.ccx.tcx.sess.span_err(pat.span, "mismatched types in range");
        } else if !ty::type_is_numeric(fcx.ccx.tcx, b_ty) {
            fcx.ccx.tcx.sess.span_err(pat.span,
                                      "non-numeric type used in range");
        } else if !valid_range_bounds(begin, end) {
            fcx.ccx.tcx.sess.span_err(begin.span,
                                      "lower range bound must be less \
                                       than upper");
        }
        write::ty_only_fixup(fcx, pat.id, b_ty);
      }
      ast::pat_ident(name, sub) {
        let vid = lookup_local(fcx, pat.span, pat.id);
        let typ = ty::mk_var(fcx.ccx.tcx, vid);
        typ = demand::simple(fcx, pat.span, expected, typ);
        let canon_id = map.get(path_to_ident(name));
        if canon_id != pat.id {
            let ct =
                ty::mk_var(fcx.ccx.tcx,
                           lookup_local(fcx, pat.span, canon_id));
            typ = demand::simple(fcx, pat.span, ct, typ);
        }
        write::ty_only_fixup(fcx, pat.id, typ);
        alt sub {
          some(p) { check_pat(fcx, map, p, expected); }
          _ {}
        }
      }
      ast::pat_tag(path, subpats) {
        // Typecheck the path.
        let v_def = lookup_def(fcx, path.span, pat.id);
        let v_def_ids = ast_util::variant_def_ids(v_def);
        let tag_tpt = ty::lookup_item_type(fcx.ccx.tcx, v_def_ids.tg);
        let path_tpot = instantiate_path(fcx, path, tag_tpt, pat.span);

        // Take the tag type params out of `expected`.
        alt structure_of(fcx, pat.span, expected) {
          ty::ty_tag(_, expected_tps) {
            // Unify with the expected tag type.
            let ctor_ty =
                ty::ty_param_substs_opt_and_ty_to_monotype(fcx.ccx.tcx,
                                                           path_tpot);

            let path_tpt =
                demand::with_substs(fcx, pat.span, expected, ctor_ty,
                                    expected_tps);
            path_tpot =
                {substs: some::<[ty::t]>(path_tpt.substs), ty: path_tpt.ty};

            // Get the number of arguments in this tag variant.
            let arg_types =
                variant_arg_types(fcx.ccx, pat.span, v_def_ids.var,
                                  expected_tps);
            let subpats_len = vec::len::<@ast::pat>(subpats);
            if vec::len::<ty::t>(arg_types) > 0u {
                // N-ary variant.

                let arg_len = vec::len::<ty::t>(arg_types);
                if arg_len != subpats_len {
                    // TODO: note definition of tag variant
                    // TODO (issue #448): Wrap a #fmt string over multiple
                    // lines...
                    let s =
                        #fmt["this pattern has %u field%s, but the \
                                       corresponding variant has %u field%s",
                             subpats_len,
                             if subpats_len == 1u { "" } else { "s" },
                             arg_len, if arg_len == 1u { "" } else { "s" }];
                    fcx.ccx.tcx.sess.span_fatal(pat.span, s);
                }

                // TODO: vec::iter2

                let i = 0u;
                for subpat: @ast::pat in subpats {
                    check_pat(fcx, map, subpat, arg_types[i]);
                    i += 1u;
                }
            } else if subpats_len > 0u {
                // TODO: note definition of tag variant
                fcx.ccx.tcx.sess.span_fatal
                    (pat.span,
                     #fmt["this pattern has %u field%s, \
                          but the corresponding \
                          variant has no fields",
                                                 subpats_len,
                                                 if subpats_len == 1u {
                                                     ""
                                                 } else { "s" }]);
            }
            write::ty_fixup(fcx, pat.id, path_tpot);
          }
          _ {
            // FIXME: Switch expected and actual in this message? I
            // can never tell.
            fcx.ccx.tcx.sess.span_fatal
                (pat.span,
                 #fmt["mismatched types: expected `%s` but found tag",
                      ty_to_str(fcx.ccx.tcx, expected)]);
          }
        }
        write::ty_fixup(fcx, pat.id, path_tpot);
      }
      ast::pat_rec(fields, etc) {
        let ex_fields;
        alt structure_of(fcx, pat.span, expected) {
          ty::ty_rec(fields) { ex_fields = fields; }
          _ {
            fcx.ccx.tcx.sess.span_fatal
                (pat.span,
                #fmt["mismatched types: expected `%s` but found record",
                                ty_to_str(fcx.ccx.tcx, expected)]);
          }
        }
        let f_count = vec::len(fields);
        let ex_f_count = vec::len(ex_fields);
        if ex_f_count < f_count || !etc && ex_f_count > f_count {
            fcx.ccx.tcx.sess.span_fatal
                (pat.span, #fmt["mismatched types: expected a record \
                      with %u fields, found one with %u \
                      fields",
                                ex_f_count, f_count]);
        }
        fn matches(name: str, f: ty::field) -> bool {
            ret str::eq(name, f.ident);
        }
        for f: ast::field_pat in fields {
            alt vec::find(ex_fields, bind matches(f.ident, _)) {
              some(field) { check_pat(fcx, map, f.pat, field.mt.ty); }
              none. {
                fcx.ccx.tcx.sess.span_fatal(pat.span,
                                            #fmt["mismatched types: did not \
                                           expect a record with a field `%s`",
                                                 f.ident]);
              }
            }
        }
        write::ty_only_fixup(fcx, pat.id, expected);
      }
      ast::pat_tup(elts) {
        let ex_elts;
        alt structure_of(fcx, pat.span, expected) {
          ty::ty_tup(elts) { ex_elts = elts; }
          _ {
            fcx.ccx.tcx.sess.span_fatal
                (pat.span,
                 #fmt["mismatched types: expected `%s`, found tuple",
                        ty_to_str(fcx.ccx.tcx, expected)]);
          }
        }
        let e_count = vec::len(elts);
        if e_count != vec::len(ex_elts) {
            fcx.ccx.tcx.sess.span_fatal
                (pat.span, #fmt["mismatched types: expected a tuple \
                      with %u fields, found one with %u \
                      fields", vec::len(ex_elts), e_count]);
        }
        let i = 0u;
        for elt in elts { check_pat(fcx, map, elt, ex_elts[i]); i += 1u; }
        write::ty_only_fixup(fcx, pat.id, expected);
      }
      ast::pat_box(inner) {
        alt structure_of(fcx, pat.span, expected) {
          ty::ty_box(e_inner) {
            check_pat(fcx, map, inner, e_inner.ty);
            write::ty_only_fixup(fcx, pat.id, expected);
          }
          _ {
            fcx.ccx.tcx.sess.span_fatal(pat.span,
                                        "mismatched types: expected `" +
                                            ty_to_str(fcx.ccx.tcx, expected) +
                                            "` found box");
          }
        }
      }
      ast::pat_uniq(inner) {
        alt structure_of(fcx, pat.span, expected) {
          ty::ty_uniq(e_inner) {
            check_pat(fcx, map, inner, e_inner.ty);
            write::ty_only_fixup(fcx, pat.id, expected);
          }
          _ {
            fcx.ccx.tcx.sess.span_fatal(pat.span,
                                        "mismatched types: expected `" +
                                            ty_to_str(fcx.ccx.tcx, expected) +
                                            "` found uniq");
          }
        }
      }
    }
}

fn require_unsafe(sess: session, f_purity: ast::purity, sp: span) {
    alt f_purity {
      ast::unsafe_fn. { ret; }
      _ {
        sess.span_err(
            sp,
            "unsafe operation requires unsafe function or block");
      }
    }
}

fn require_impure(sess: session, f_purity: ast::purity, sp: span) {
    alt f_purity {
      ast::unsafe_fn. { ret; }
      ast::impure_fn. { ret; }
      ast::pure_fn. {
        sess.span_err(sp, "Found impure expression in pure function decl");
      }
    }
}

fn require_pure_call(ccx: @crate_ctxt, caller_purity: ast::purity,
                     callee: @ast::expr, sp: span) {
    alt caller_purity {
      ast::unsafe_fn. { ret; }
      ast::impure_fn. {
        alt ccx.tcx.def_map.find(callee.id) {
          some(ast::def_fn(_, ast::unsafe_fn.)) |
          some(ast::def_native_fn(_, ast::unsafe_fn.)) {
            ccx.tcx.sess.span_err(
                sp,
                "safe function calls function marked unsafe");
          }
          _ {
          }
        }
        ret;
      }
      ast::pure_fn. {
        alt ccx.tcx.def_map.find(callee.id) {
          some(ast::def_fn(_, ast::pure_fn.)) |
          some(ast::def_native_fn(_, ast::pure_fn.)) |
          some(ast::def_variant(_, _)) { ret; }
          _ {
            ccx.tcx.sess.span_err
                (sp, "pure function calls function not known to be pure");
          }
        }
      }
    }
}

type unifier = fn@(@fn_ctxt, span, ty::t, ty::t) -> ty::t;

fn check_expr(fcx: @fn_ctxt, expr: @ast::expr) -> bool {
    fn dummy_unify(_fcx: @fn_ctxt, _sp: span, _expected: ty::t, actual: ty::t)
       -> ty::t {
        actual
    }
    ret check_expr_with_unifier(fcx, expr, dummy_unify, 0u);
}
fn check_expr_with(fcx: @fn_ctxt, expr: @ast::expr, expected: ty::t) -> bool {
    ret check_expr_with_unifier(fcx, expr, demand::simple, expected);
}

fn impl_self_ty(tcx: ty::ctxt, did: ast::def_id) -> {n_tps: uint, ty: ty::t} {
    if did.crate == ast::local_crate {
        alt tcx.items.get(did.node) {
          ast_map::node_item(@{node: ast::item_impl(ts, _, st, _),
                               _}) {
            {n_tps: vec::len(ts), ty: ast_ty_to_ty(tcx, m_check, st)}
          }
        }
    } else {
        let tpt = csearch::get_type(tcx, did);
        {n_tps: vec::len(*tpt.bounds), ty: tpt.ty}
    }
}

fn lookup_method(fcx: @fn_ctxt, isc: resolve::iscopes,
                 name: ast::ident, ty: ty::t, sp: span)
    -> option::t<{method_ty: ty::t, n_tps: uint, substs: [ty::t],
                  origin: method_origin}> {
    let tcx = fcx.ccx.tcx;

    // First, see whether this is an interface-bounded parameter
    alt ty::struct(tcx, ty) {
      ty::ty_param(n, did) {
        let bound_n = 0u;
        for bound in *tcx.ty_param_bounds.get(did.node) {
            alt bound {
              ty::bound_iface(t) {
                let (iid, tps) = alt ty::struct(tcx, t) {
                    ty::ty_iface(i, tps) { (i, tps) }
                };
                let ifce_methods = ty::iface_methods(tcx, iid);
                alt vec::position_pred(*ifce_methods, {|m| m.ident == name}) {
                  some(pos) {
                    let m = ifce_methods[pos];
                    ret some({method_ty: ty::mk_fn(tcx, m.fty),
                              n_tps: vec::len(*m.tps),
                              substs: tps,
                              origin: method_param(iid, pos, n, bound_n)});
                  }
                  _ {}
                }
                bound_n += 1u;
              }
              _ {}
            }
        }
        ret none;
      }
      ty::ty_iface(did, tps) {
        let i = 0u;
        for m in *ty::iface_methods(tcx, did) {
            if m.ident == name {
                ret some({method_ty: ty::mk_fn(tcx, m.fty),
                          n_tps: vec::len(*m.tps),
                          substs: tps,
                          origin: method_iface(i)});
            }
            i += 1u;
        }
      }
      _ {}
    }

    fn ty_from_did(tcx: ty::ctxt, did: ast::def_id) -> ty::t {
        if did.crate == ast::local_crate {
            alt tcx.items.get(did.node) {
              ast_map::node_method(m) {
                let mt = ty_of_method(tcx, m_check, m);
                ty::mk_fn(tcx, mt.fty)
              }
            }
        } else { csearch::get_type(tcx, did).ty }
    }

    let result = none;
    std::list::iter(isc) {|impls|
        if option::is_some(result) { ret; }
        for @{did, methods, _} in *impls {
            alt vec::find(methods, {|m| m.ident == name}) {
              some(m) {
                let {n_tps, ty: self_ty} = impl_self_ty(tcx, did);
                let {vars, ty: self_ty} = if n_tps > 0u {
                    bind_params(fcx, self_ty, n_tps)
                } else { {vars: [], ty: self_ty} };
                alt unify::unify(fcx, ty, self_ty) {
                  ures_ok(_) {
                    if option::is_some(result) {
                        // FIXME[impl] score specificity to resolve ambiguity?
                        tcx.sess.span_err(
                            sp, "multiple applicable methods in scope");
                    } else {
                        result = some({
                            method_ty: ty_from_did(tcx, m.did),
                            n_tps: m.n_tps,
                            substs: vars,
                            origin: method_static(m.did)
                        });
                    }
                  }
                  _ {}
                }
              }
              _ {}
            }
        }
    }
    result
}

fn check_expr_fn_with_unifier(fcx: @fn_ctxt,
                              expr: @ast::expr,
                              proto: ast::proto,
                              decl: ast::fn_decl,
                              body: ast::blk,
                              unify: unifier,
                              expected: ty::t) {
    let tcx = fcx.ccx.tcx;

    let fty = ty::mk_fn(tcx, ty_of_fn_decl(tcx, m_check_tyvar(fcx),
                                           proto, decl));

    #debug("check_expr_fn_with_unifier %s fty=%s",
           expr_to_str(expr),
           ty_to_str(tcx, fty));

    write::ty_only_fixup(fcx, expr.id, fty);

    // Unify the type of the function with the expected type before we
    // typecheck the body so that we have more information about the
    // argument types in the body. This is needed to make binops and
    // record projection work on type inferred arguments.
    unify(fcx, expr.span, expected, fty);

    check_fn(fcx.ccx, proto, decl, body, expr.id, some(fcx));
}

fn check_expr_with_unifier(fcx: @fn_ctxt, expr: @ast::expr, unify: unifier,
                           expected: ty::t) -> bool {
    #debug("typechecking expr %s",
           syntax::print::pprust::expr_to_str(expr));

    // A generic function to factor out common logic from call and bind
    // expressions.
    fn check_call_or_bind(fcx: @fn_ctxt, sp: span, f: @ast::expr,
                          args: [option::t<@ast::expr>]) -> bool {
        // Check the function.
        let bot = check_expr(fcx, f);

        // Get the function type.
        let fty = expr_ty(fcx.ccx.tcx, f);

        let sty = structure_of(fcx, sp, fty);

        // Grab the argument types
        let arg_tys =
            alt sty {
              ty::ty_fn({inputs: arg_tys, _}) | ty::ty_native_fn(arg_tys, _) {
                arg_tys
              }
              _ {
                fcx.ccx.tcx.sess.span_fatal(f.span,
                                            "mismatched types: \
                     expected function or native \
                     function but found "
                                                + ty_to_str(fcx.ccx.tcx, fty))
              }
            };

        // Check that the correct number of arguments were supplied.
        let expected_arg_count = vec::len(arg_tys);
        let supplied_arg_count = vec::len(args);
        if expected_arg_count != supplied_arg_count {
            fcx.ccx.tcx.sess.span_err(sp,
                                      #fmt["this function takes %u \
                      parameter%s but %u parameter%s supplied",
                                           expected_arg_count,
                                           if expected_arg_count == 1u {
                                               ""
                                           } else { "s" }, supplied_arg_count,
                                           if supplied_arg_count == 1u {
                                               " was"
                                           } else { "s were" }]);
            // HACK: build an arguments list with dummy arguments to
            // check against
            let dummy = {mode: ast::by_ref, ty: ty::mk_bot(fcx.ccx.tcx)};
            arg_tys = vec::init_elt(dummy, supplied_arg_count);
        }

        // Check the arguments.
        // We do this in a pretty awful way: first we typecheck any arguments
        // that are not anonymous functions, then we typecheck the anonymous
        // functions. This is so that we have more information about the types
        // of arguments when we typecheck the functions. This isn't really the
        // right way to do this.
        let check_args = fn@(check_blocks: bool) -> bool {
                let i = 0u;
                let bot = false;
                for a_opt: option::t<@ast::expr> in args {
                    alt a_opt {
                      some(a) {
                        let is_block =
                            alt a.node {
                              ast::expr_fn_block(_, _) { true }
                              _ { false }
                            };
                        if is_block == check_blocks {
                            bot |=
                                check_expr_with_unifier(fcx, a,
                                                        demand::simple,
                                                        arg_tys[i].ty);
                        }
                      }
                      none. { }
                    }
                    i += 1u;
                }
                ret bot;
            };
        bot |= check_args(false);
        bot |= check_args(true);

        ret bot;
    }

    // A generic function for checking assignment expressions
    fn check_assignment(fcx: @fn_ctxt, _sp: span, lhs: @ast::expr,
                        rhs: @ast::expr, id: ast::node_id) -> bool {
        let t = next_ty_var(fcx);
        let bot = check_expr_with(fcx, lhs, t) | check_expr_with(fcx, rhs, t);
        write::ty_only_fixup(fcx, id, ty::mk_nil(fcx.ccx.tcx));
        ret bot;
    }

    // A generic function for checking call expressions
    fn check_call(fcx: @fn_ctxt, sp: span, f: @ast::expr, args: [@ast::expr])
        -> bool {
        let args_opt_0: [option::t<@ast::expr>] = [];
        for arg: @ast::expr in args {
            args_opt_0 += [some::<@ast::expr>(arg)];
        }

        // Call the generic checker.
        ret check_call_or_bind(fcx, sp, f, args_opt_0);
    }

    // A generic function for doing all of the checking for call expressions
    fn check_call_full(fcx: @fn_ctxt, sp: span, f: @ast::expr,
                       args: [@ast::expr], id: ast::node_id) -> bool {
        /* here we're kind of hosed, as f can be any expr
        need to restrict it to being an explicit expr_path if we're
        inside a pure function, and need an environment mapping from
        function name onto purity-designation */
        require_pure_call(fcx.ccx, fcx.purity, f, sp);
        let bot = check_call(fcx, sp, f, args);

        // Pull the return type out of the type of the function.
        let rt_1;
        let fty = ty::expr_ty(fcx.ccx.tcx, f);
        alt structure_of(fcx, sp, fty) {
          ty::ty_fn(f) {
            bot |= f.ret_style == ast::noreturn;
            rt_1 = f.output;
          }
          ty::ty_native_fn(_, rt) { rt_1 = rt; }
          _ { fcx.ccx.tcx.sess.span_fatal(sp, "calling non-function"); }
        }
        write::ty_only_fixup(fcx, id, rt_1);
        ret bot;
    }

    // A generic function for checking for or for-each loops
    fn check_for(fcx: @fn_ctxt, local: @ast::local,
                 element_ty: ty::t, body: ast::blk,
                 node_id: ast::node_id) -> bool {
        let locid = lookup_local(fcx, local.span, local.node.id);
        let element_ty = demand::simple(fcx, local.span, element_ty,
                                        ty::mk_var(fcx.ccx.tcx, locid));
        let bot = check_decl_local(fcx, local);
        check_block_no_value(fcx, body);
        // Unify type of decl with element type of the seq
        demand::simple(fcx, local.span,
                       ty::node_id_to_type(fcx.ccx.tcx, local.node.id),
                       element_ty);
        write::nil_ty(fcx.ccx.tcx, node_id);
        ret bot;
    }


    // A generic function for checking the then and else in an if
    // or if-check
    fn check_then_else(fcx: @fn_ctxt, thn: ast::blk,
                       elsopt: option::t<@ast::expr>, id: ast::node_id,
                       _sp: span) -> bool {
        let (if_t, if_bot) =
            alt elsopt {
              some(els) {
                let thn_bot = check_block(fcx, thn);
                let thn_t = block_ty(fcx.ccx.tcx, thn);
                let els_bot = check_expr_with(fcx, els, thn_t);
                let els_t = expr_ty(fcx.ccx.tcx, els);
                let if_t = if !ty::type_is_bot(fcx.ccx.tcx, els_t) {
                    els_t
                } else {
                    thn_t
                };
                (if_t, thn_bot & els_bot)
              }
              none. {
                check_block_no_value(fcx, thn);
                (ty::mk_nil(fcx.ccx.tcx), false)
              }
            };
        write::ty_only_fixup(fcx, id, if_t);
        ret if_bot;
    }

    // Checks the compatibility
    fn check_binop_type_compat(fcx: @fn_ctxt, span: span, ty: ty::t,
                               binop: ast::binop) {
        let resolved_t = resolve_type_vars_if_possible(fcx, ty);
        if !ty::is_binopable(fcx.ccx.tcx, resolved_t, binop) {
            let binopstr = ast_util::binop_to_str(binop);
            let t_str = ty_to_str(fcx.ccx.tcx, resolved_t);
            let errmsg =
                "binary operation " + binopstr +
                    " cannot be applied to type `" + t_str + "`";
            fcx.ccx.tcx.sess.span_err(span, errmsg);
        }
    }

    let tcx = fcx.ccx.tcx;
    let id = expr.id;
    let bot = false;
    alt expr.node {
      ast::expr_lit(lit) {
        let typ = check_lit(fcx.ccx, lit);
        write::ty_only_fixup(fcx, id, typ);
      }
      ast::expr_binary(binop, lhs, rhs) {
        let lhs_t = next_ty_var(fcx);
        bot = check_expr_with(fcx, lhs, lhs_t);

        let rhs_bot = check_expr_with(fcx, rhs, lhs_t);
        if !ast_util::lazy_binop(binop) { bot |= rhs_bot; }

        check_binop_type_compat(fcx, expr.span, lhs_t, binop);

        let t =
            alt binop {
              ast::eq. { ty::mk_bool(tcx) }
              ast::lt. { ty::mk_bool(tcx) }
              ast::le. { ty::mk_bool(tcx) }
              ast::ne. { ty::mk_bool(tcx) }
              ast::ge. { ty::mk_bool(tcx) }
              ast::gt. { ty::mk_bool(tcx) }
              _ { lhs_t }
            };
        write::ty_only_fixup(fcx, id, t);
      }
      ast::expr_unary(unop, oper) {
        bot = check_expr(fcx, oper);
        let oper_t = expr_ty(tcx, oper);
        alt unop {
          ast::box(mut) { oper_t = ty::mk_box(tcx, {ty: oper_t, mut: mut}); }
          ast::uniq(mut) {
            oper_t = ty::mk_uniq(tcx, {ty: oper_t, mut: mut});
          }
          ast::deref. {
            alt structure_of(fcx, expr.span, oper_t) {
              ty::ty_box(inner) { oper_t = inner.ty; }
              ty::ty_uniq(inner) { oper_t = inner.ty; }
              ty::ty_res(_, inner, _) { oper_t = inner; }
              ty::ty_tag(id, tps) {
                let variants = ty::tag_variants(tcx, id);
                if vec::len(*variants) != 1u ||
                       vec::len(variants[0].args) != 1u {
                    tcx.sess.span_fatal(expr.span,
                                        "can only dereference tags " +
                                        "with a single variant which has a "
                                            + "single argument");
                }
                oper_t =
                    ty::substitute_type_params(tcx, tps, variants[0].args[0]);
              }
              ty::ty_ptr(inner) {
                oper_t = inner.ty;
                require_unsafe(fcx.ccx.tcx.sess, fcx.purity, expr.span);
              }
              _ {
                tcx.sess.span_fatal(expr.span,
                                    "dereferencing non-" +
                                        "dereferenceable type: " +
                                        ty_to_str(tcx, oper_t));
              }
            }
          }
          ast::not. {
            if !type_is_integral(fcx, oper.span, oper_t) &&
                   structure_of(fcx, oper.span, oper_t) != ty::ty_bool {
                tcx.sess.span_err(expr.span,
                                  #fmt["mismatched types: expected `bool` \
                          or `integer` but found `%s`",
                                       ty_to_str(tcx, oper_t)]);
            }
          }
          ast::neg. {
            oper_t = structurally_resolved_type(fcx, oper.span, oper_t);
            if !(ty::type_is_integral(tcx, oper_t) ||
                     ty::type_is_fp(tcx, oper_t)) {
                tcx.sess.span_err(expr.span,
                                  "applying unary minus to \
                   non-numeric type `"
                                      + ty_to_str(tcx, oper_t) + "`");
            }
          }
        }
        write::ty_only_fixup(fcx, id, oper_t);
      }
      ast::expr_path(pth) {
        let defn = lookup_def(fcx, pth.span, id);

        let tpt = ty_param_bounds_and_ty_for_def(fcx, expr.span, defn);
        if ty::def_has_ty_params(defn) {
            let path_tpot = instantiate_path(fcx, pth, tpt, expr.span);
            write::ty_fixup(fcx, id, path_tpot);
        } else {
            // The definition doesn't take type parameters. If the programmer
            // supplied some, that's an error.
            if vec::len::<@ast::ty>(pth.node.types) > 0u {
                tcx.sess.span_fatal(expr.span,
                                    "this kind of value does not \
                                     take type parameters");
            }
            write::ty_only_fixup(fcx, id, tpt.ty);
        }
      }
      ast::expr_mac(_) { tcx.sess.bug("unexpanded macro"); }
      ast::expr_fail(expr_opt) {
        bot = true;
        alt expr_opt {
          none. {/* do nothing */ }
          some(e) { check_expr_with(fcx, e, ty::mk_str(tcx)); }
        }
        write::bot_ty(tcx, id);
      }
      ast::expr_break. { write::bot_ty(tcx, id); bot = true; }
      ast::expr_cont. { write::bot_ty(tcx, id); bot = true; }
      ast::expr_ret(expr_opt) {
        bot = true;
        alt expr_opt {
          none. {
            let nil = ty::mk_nil(tcx);
            if !are_compatible(fcx, fcx.ret_ty, nil) {
                tcx.sess.span_err(expr.span,
                                  "ret; in function returning non-nil");
            }
          }
          some(e) { check_expr_with(fcx, e, fcx.ret_ty); }
        }
        write::bot_ty(tcx, id);
      }
      ast::expr_be(e) {
        // FIXME: prove instead of assert
        assert (ast_util::is_call_expr(e));
        check_expr_with(fcx, e, fcx.ret_ty);
        bot = true;
        write::nil_ty(tcx, id);
      }
      ast::expr_log(_, lv, e) {
        bot = check_expr_with(fcx, lv, ty::mk_mach_uint(tcx, ast::ty_u32));
        bot |= check_expr(fcx, e);
        write::nil_ty(tcx, id);
      }
      ast::expr_check(_, e) {
        bot = check_pred_expr(fcx, e);
        write::nil_ty(tcx, id);
      }
      ast::expr_if_check(cond, thn, elsopt) {
        bot =
            check_pred_expr(fcx, cond) |
                check_then_else(fcx, thn, elsopt, id, expr.span);
      }
      ast::expr_ternary(_, _, _) {
        bot = check_expr(fcx, ast_util::ternary_to_if(expr));
      }
      ast::expr_assert(e) {
        bot = check_expr_with(fcx, e, ty::mk_bool(tcx));
        write::nil_ty(tcx, id);
      }
      ast::expr_copy(a) {
        bot = check_expr_with_unifier(fcx, a, unify, expected);
        let tpot =
            ty::node_id_to_ty_param_substs_opt_and_ty(fcx.ccx.tcx, a.id);
        write::ty_fixup(fcx, id, tpot);

      }
      ast::expr_move(lhs, rhs) {
        require_impure(tcx.sess, fcx.purity, expr.span);
        bot = check_assignment(fcx, expr.span, lhs, rhs, id);
      }
      ast::expr_assign(lhs, rhs) {
        require_impure(tcx.sess, fcx.purity, expr.span);
        bot = check_assignment(fcx, expr.span, lhs, rhs, id);
      }
      ast::expr_swap(lhs, rhs) {
        require_impure(tcx.sess, fcx.purity, expr.span);
        bot = check_assignment(fcx, expr.span, lhs, rhs, id);
      }
      ast::expr_assign_op(op, lhs, rhs) {
        require_impure(tcx.sess, fcx.purity, expr.span);
        bot = check_assignment(fcx, expr.span, lhs, rhs, id);
        check_binop_type_compat(fcx, expr.span, expr_ty(tcx, lhs), op);
      }
      ast::expr_if(cond, thn, elsopt) {
        bot =
            check_expr_with(fcx, cond, ty::mk_bool(tcx)) |
                check_then_else(fcx, thn, elsopt, id, expr.span);
      }
      ast::expr_for(decl, seq, body) {
        bot = check_expr(fcx, seq);
        let elt_ty;
        let ety = expr_ty(tcx, seq);
        alt structure_of(fcx, expr.span, ety) {
          ty::ty_vec(vec_elt_ty) { elt_ty = vec_elt_ty.ty; }
          ty::ty_str. { elt_ty = ty::mk_mach_uint(tcx, ast::ty_u8); }
          _ {
            tcx.sess.span_fatal(expr.span,
                                "mismatched types: expected vector or string "
                                + "but found `" + ty_to_str(tcx, ety) + "`");
          }
        }
        bot |= check_for(fcx, decl, elt_ty, body, id);
      }
      ast::expr_while(cond, body) {
        bot = check_expr_with(fcx, cond, ty::mk_bool(tcx));
        check_block_no_value(fcx, body);
        write::ty_only_fixup(fcx, id, ty::mk_nil(tcx));
      }
      ast::expr_do_while(body, cond) {
        bot = check_expr_with(fcx, cond, ty::mk_bool(tcx)) |
              check_block_no_value(fcx, body);
        write::ty_only_fixup(fcx, id, block_ty(tcx, body));
      }
      ast::expr_alt(expr, arms) {
        bot = check_expr(fcx, expr);

        // Typecheck the patterns first, so that we get types for all the
        // bindings.
        let pattern_ty = ty::expr_ty(tcx, expr);
        for arm: ast::arm in arms {
            let id_map = pat_util::pat_id_map(tcx, arm.pats[0]);
            for p: @ast::pat in arm.pats {
                check_pat(fcx, id_map, p, pattern_ty);
            }
        }
        // Now typecheck the blocks.
        let result_ty = next_ty_var(fcx);
        let arm_non_bot = false;
        for arm: ast::arm in arms {
            alt arm.guard {
              some(e) { check_expr_with(fcx, e, ty::mk_bool(tcx)); }
              none. { }
            }
            if !check_block(fcx, arm.body) { arm_non_bot = true; }
            let bty = block_ty(tcx, arm.body);
            result_ty = demand::simple(fcx, arm.body.span, result_ty, bty);
        }
        bot |= !arm_non_bot;
        if !arm_non_bot { result_ty = ty::mk_bot(tcx); }
        write::ty_only_fixup(fcx, id, result_ty);
      }
      ast::expr_fn(proto, decl, body, captures) {
        check_expr_fn_with_unifier(fcx, expr, proto, decl, body,
                                   unify, expected);
        capture::check_capture_clause(tcx, expr.id, proto, *captures);
      }
      ast::expr_fn_block(decl, body) {
        // Take the prototype from the expected type, but default to block:
        let proto = alt ty::struct(tcx, expected) {
          ty::ty_fn({proto, _}) { proto }
          _ {
            fcx.ccx.tcx.sess.span_warn(
                expr.span,
                "unable to infer kind of closure, defaulting to block");
            ast::proto_block
          }
        };
        #debug("checking expr_fn_block %s expected=%s",
               expr_to_str(expr),
               ty_to_str(tcx, expected));
        check_expr_fn_with_unifier(fcx, expr, proto, decl, body,
                                   unify, expected);
        write::ty_only_fixup(fcx, id, expected);
      }
      ast::expr_block(b) {
        // If this is an unchecked block, turn off purity-checking
        bot = check_block(fcx, b);
        let typ =
            alt b.node.expr {
              some(expr) { expr_ty(tcx, expr) }
              none. { ty::mk_nil(tcx) }
            };
        write::ty_only_fixup(fcx, id, typ);
      }
      ast::expr_bind(f, args) {
        // Call the generic checker.
        bot = check_call_or_bind(fcx, expr.span, f, args);

        // Pull the argument and return types out.
        let proto, arg_tys, rt, cf, constrs;
        alt structure_of(fcx, expr.span, expr_ty(tcx, f)) {
          // FIXME:
          // probably need to munge the constrs to drop constraints
          // for any bound args
          ty::ty_fn(f) {
            proto = f.proto;
            arg_tys = f.inputs;
            rt = f.output;
            cf = f.ret_style;
            constrs = f.constraints;
          }
          ty::ty_native_fn(arg_tys_, rt_) {
            proto = ast::proto_bare;
            arg_tys = arg_tys_;
            rt = rt_;
            cf = ast::return_val;
            constrs = [];
          }
          _ { fail "LHS of bind expr didn't have a function type?!"; }
        }

        // For each blank argument, add the type of that argument
        // to the resulting function type.
        let out_args = [];
        let i = 0u;
        while i < vec::len(args) {
            alt args[i] {
              some(_) {/* no-op */ }
              none. { out_args += [arg_tys[i]]; }
            }
            i += 1u;
        }

        // Determine what fn prototype results from binding
        fn lower_bound_proto(proto: ast::proto) -> ast::proto {
            // FIXME: This is right for bare fns, possibly not others
            alt proto {
              ast::proto_bare. { ast::proto_box }
              _ { proto }
            }
        }

        let ft = ty::mk_fn(tcx, {proto: lower_bound_proto(proto),
                                 inputs: out_args, output: rt,
                                 ret_style: cf, constraints: constrs});
        write::ty_only_fixup(fcx, id, ft);
      }
      ast::expr_call(f, args, _) {
        bot = check_call_full(fcx, expr.span, f, args, expr.id);
      }
      ast::expr_cast(e, t) {
        bot = check_expr(fcx, e);
        let t_1 = ast_ty_to_ty_crate(fcx.ccx, t);
        let t_e = ty::expr_ty(tcx, e);

        alt ty::struct(tcx, t_1) {
          // This will be looked up later on
          ty::ty_iface(_, _) {}
          _ {
            if ty::type_is_nil(tcx, t_e) {
                tcx.sess.span_err(expr.span, "cast from nil: " +
                                  ty_to_str(tcx, t_e) + " as " +
                                  ty_to_str(tcx, t_1));
            } else if ty::type_is_nil(tcx, t_1) {
                tcx.sess.span_err(expr.span, "cast to nil: " +
                                  ty_to_str(tcx, t_e) + " as " +
                                  ty_to_str(tcx, t_1));
            }

            let t_1_is_scalar = type_is_scalar(fcx, expr.span, t_1);
            if type_is_c_like_enum(fcx,expr.span,t_e) && t_1_is_scalar {
                /* this case is allowed */
            } else if !(type_is_scalar(fcx,expr.span,t_e) && t_1_is_scalar) {
                // FIXME there are more forms of cast to support, eventually.
                tcx.sess.span_err(expr.span,
                                  "non-scalar cast: " +
                                  ty_to_str(tcx, t_e) + " as " +
                                  ty_to_str(tcx, t_1));
            }
          }
        }
        write::ty_only_fixup(fcx, id, t_1);
      }
      ast::expr_vec(args, mut) {
        let t: ty::t = next_ty_var(fcx);
        for e: @ast::expr in args { bot |= check_expr_with(fcx, e, t); }
        let typ = ty::mk_vec(tcx, {ty: t, mut: mut});
        write::ty_only_fixup(fcx, id, typ);
      }
      ast::expr_tup(elts) {
        let elt_ts = [];
        vec::reserve(elt_ts, vec::len(elts));
        for e in elts {
            check_expr(fcx, e);
            let ety = expr_ty(fcx.ccx.tcx, e);
            elt_ts += [ety];
        }
        let typ = ty::mk_tup(fcx.ccx.tcx, elt_ts);
        write::ty_only_fixup(fcx, id, typ);
      }
      ast::expr_rec(fields, base) {
        alt base { none. {/* no-op */ } some(b_0) { check_expr(fcx, b_0); } }
        let fields_t: [spanned<field>] = [];
        for f: ast::field in fields {
            bot |= check_expr(fcx, f.node.expr);
            let expr_t = expr_ty(tcx, f.node.expr);
            let expr_mt = {ty: expr_t, mut: f.node.mut};
            // for the most precise error message,
            // should be f.node.expr.span, not f.span
            fields_t +=
                [respan(f.node.expr.span,
                        {ident: f.node.ident, mt: expr_mt})];
        }
        alt base {
          none. {
            fn get_node(f: spanned<field>) -> field { f.node }
            let typ = ty::mk_rec(tcx, vec::map(fields_t, get_node));
            write::ty_only_fixup(fcx, id, typ);
          }
          some(bexpr) {
            bot |= check_expr(fcx, bexpr);
            let bexpr_t = expr_ty(tcx, bexpr);
            let base_fields: [field] = [];
            alt structure_of(fcx, expr.span, bexpr_t) {
              ty::ty_rec(flds) { base_fields = flds; }
              _ {
                tcx.sess.span_fatal(expr.span,
                                    "record update has non-record base");
              }
            }
            write::ty_only_fixup(fcx, id, bexpr_t);
            for f: spanned<ty::field> in fields_t {
                let found = false;
                for bf: ty::field in base_fields {
                    if str::eq(f.node.ident, bf.ident) {
                        demand::simple(fcx, f.span, bf.mt.ty, f.node.mt.ty);
                        found = true;
                    }
                }
                if !found {
                    tcx.sess.span_fatal(f.span,
                                        "unknown field in record update: " +
                                            f.node.ident);
                }
            }
          }
        }
      }
      ast::expr_field(base, field, tys) {
        bot |= check_expr(fcx, base);
        let expr_t = structurally_resolved_type(fcx, expr.span,
                                                expr_ty(tcx, base));
        let base_t = do_autoderef(fcx, expr.span, expr_t);
        let handled = false, n_tys = vec::len(tys);
        alt structure_of(fcx, expr.span, base_t) {
          ty::ty_rec(fields) {
            alt ty::field_idx(field, fields) {
              some(ix) {
                if n_tys > 0u {
                    tcx.sess.span_err(expr.span,
                                      "can't provide type parameters \
                                       to a field access");
                }
                write::ty_only_fixup(fcx, id, fields[ix].mt.ty);
                handled = true;
              }
              _ {}
            }
          }
          _ {}
        }
        if !handled {
            let iscope = fcx.ccx.impl_map.get(expr.id);
            alt lookup_method(fcx, iscope, field, expr_t, expr.span) {
              some({method_ty: fty, n_tps: method_n_tps, substs, origin}) {
                let substs = substs, n_tps = vec::len(substs);
                if method_n_tps + n_tps > 0u {
                    if n_tys > 0u {
                        if n_tys != method_n_tps {
                            tcx.sess.span_fatal
                                (expr.span, "incorrect number of type \
                                           parameters given for this method");

                        }
                        for ty in tys {
                            substs += [ast_ty_to_ty_crate(fcx.ccx, ty)];
                        }
                    } else {
                        let i = 0u;
                        while i < method_n_tps {
                            substs += [ty::mk_var(tcx, next_ty_var_id(fcx))];
                            i += 1u;
                        }
                    }
                    write::ty_fixup(fcx, id, {substs: some(substs), ty: fty});
                } else if n_tys > 0u {
                    tcx.sess.span_fatal(expr.span,
                                        "this method does not take type \
                                         parameters");
                } else {
                    write::ty_only_fixup(fcx, id, fty);
                }
                fcx.ccx.method_map.insert(id, origin);
              }
              none. {
                let t_err = resolve_type_vars_if_possible(fcx, expr_t);
                let msg = #fmt["attempted access of field %s on type %s, but \
                                no method implementation was found",
                               field, ty_to_str(tcx, t_err)];
                tcx.sess.span_fatal(expr.span, msg);
              }
            }
        }
      }
      ast::expr_index(base, idx) {
        bot |= check_expr(fcx, base);
        let base_t = expr_ty(tcx, base);
        base_t = do_autoderef(fcx, expr.span, base_t);
        bot |= check_expr(fcx, idx);
        let idx_t = expr_ty(tcx, idx);
        if !type_is_integral(fcx, idx.span, idx_t) {
            tcx.sess.span_err(idx.span,
                              "mismatched types: expected \
                               `integer` but found `"
                                  + ty_to_str(tcx, idx_t) + "`");
        }
        alt structure_of(fcx, expr.span, base_t) {
          ty::ty_vec(mt) { write::ty_only_fixup(fcx, id, mt.ty); }
          ty::ty_str. {
            let typ = ty::mk_mach_uint(tcx, ast::ty_u8);
            write::ty_only_fixup(fcx, id, typ);
          }
          _ {
            tcx.sess.span_fatal(expr.span,
                                "vector-indexing bad type: " +
                                    ty_to_str(tcx, base_t));
          }
        }
      }
      _ { tcx.sess.unimpl("expr type in typeck::check_expr"); }
    }
    if bot { write::ty_only_fixup(fcx, expr.id, ty::mk_bot(tcx)); }

    unify(fcx, expr.span, expected, expr_ty(tcx, expr));
    ret bot;
}

fn next_ty_var_id(fcx: @fn_ctxt) -> int {
    let id = *fcx.next_var_id;
    *fcx.next_var_id += 1;
    ret id;
}

fn next_ty_var(fcx: @fn_ctxt) -> ty::t {
    ret ty::mk_var(fcx.ccx.tcx, next_ty_var_id(fcx));
}

fn bind_params(fcx: @fn_ctxt, tp: ty::t, count: uint)
    -> {vars: [ty::t], ty: ty::t} {
    let vars = vec::init_fn({|_i| next_ty_var(fcx)}, count);
    {vars: vars, ty: ty::substitute_type_params(fcx.ccx.tcx, vars, tp)}
}

fn get_self_info(ccx: @crate_ctxt) -> option::t<self_info> {
    ret vec::last(ccx.self_infos);
}

fn check_decl_initializer(fcx: @fn_ctxt, nid: ast::node_id,
                          init: ast::initializer) -> bool {
    let lty = ty::mk_var(fcx.ccx.tcx, lookup_local(fcx, init.expr.span, nid));
    ret check_expr_with(fcx, init.expr, lty);
}

fn check_decl_local(fcx: @fn_ctxt, local: @ast::local) -> bool {
    let bot = false;

    alt fcx.locals.find(local.node.id) {
      some(i) {
        let t = ty::mk_var(fcx.ccx.tcx, i);
        write::ty_only_fixup(fcx, local.node.id, t);
        alt local.node.init {
          some(init) {
            bot = check_decl_initializer(fcx, local.node.id, init);
          }
          _ {/* fall through */ }
        }
        let id_map = pat_util::pat_id_map(fcx.ccx.tcx, local.node.pat);
        check_pat(fcx, id_map, local.node.pat, t);
      }
    }
    ret bot;
}

fn check_stmt(fcx: @fn_ctxt, stmt: @ast::stmt) -> bool {
    let node_id;
    let bot = false;
    alt stmt.node {
      ast::stmt_decl(decl, id) {
        node_id = id;
        alt decl.node {
          ast::decl_local(ls) {
            for (_, l) in ls { bot |= check_decl_local(fcx, l); }
          }
          ast::decl_item(_) {/* ignore for now */ }
        }
      }
      ast::stmt_expr(expr, id) {
        node_id = id;
        bot = check_expr_with(fcx, expr, ty::mk_nil(fcx.ccx.tcx));
      }
      ast::stmt_semi(expr, id) {
        node_id = id;
        bot = check_expr(fcx, expr);
      }
    }
    write::nil_ty(fcx.ccx.tcx, node_id);
    ret bot;
}

fn check_block_no_value(fcx: @fn_ctxt, blk: ast::blk) -> bool {
    let bot = check_block(fcx, blk);
    if !bot {
        let blkty = ty::node_id_to_monotype(fcx.ccx.tcx, blk.node.id);
        let nilty = ty::mk_nil(fcx.ccx.tcx);
        demand::simple(fcx, blk.span, nilty, blkty);
    }
    ret bot;
}

fn check_block(fcx0: @fn_ctxt, blk: ast::blk) -> bool {
    let fcx = alt blk.node.rules {
      ast::unchecked_blk. { @{purity: ast::impure_fn with *fcx0} }
      ast::unsafe_blk. { @{purity: ast::unsafe_fn with *fcx0} }
      ast::default_blk. { fcx0 }
    };
    let bot = false;
    let warned = false;
    for s: @ast::stmt in blk.node.stmts {
        if bot && !warned &&
               alt s.node {
                 ast::stmt_decl(@{node: ast::decl_local(_), _}, _) |
                 ast::stmt_expr(_, _) | ast::stmt_semi(_, _) {
                   true
                 }
                 _ { false }
               } {
            fcx.ccx.tcx.sess.span_warn(s.span, "unreachable statement");
            warned = true;
        }
        bot |= check_stmt(fcx, s);
    }
    alt blk.node.expr {
      none. { write::nil_ty(fcx.ccx.tcx, blk.node.id); }
      some(e) {
        if bot && !warned {
            fcx.ccx.tcx.sess.span_warn(e.span, "unreachable expression");
        }
        bot |= check_expr(fcx, e);
        let ety = expr_ty(fcx.ccx.tcx, e);
        write::ty_only_fixup(fcx, blk.node.id, ety);
      }
    }
    if bot {
        write::ty_only_fixup(fcx, blk.node.id, ty::mk_bot(fcx.ccx.tcx));
    }
    ret bot;
}

fn check_const(ccx: @crate_ctxt, _sp: span, e: @ast::expr, id: ast::node_id) {
    // FIXME: this is kinda a kludge; we manufacture a fake function context
    // and statement context for checking the initializer expression.
    let rty = node_id_to_type(ccx.tcx, id);
    let fixups: [ast::node_id] = [];
    let fcx: @fn_ctxt =
        @{ret_ty: rty,
          purity: ast::pure_fn,
          proto: ast::proto_box,
          var_bindings: ty::unify::mk_var_bindings(),
          locals: new_int_hash::<int>(),
          next_var_id: @mutable 0,
          mutable fixups: fixups,
          ccx: ccx};
    check_expr(fcx, e);
    let cty = expr_ty(fcx.ccx.tcx, e);
    let declty = fcx.ccx.tcx.tcache.get(local_def(id)).ty;
    demand::simple(fcx, e.span, declty, cty);
}

fn check_tag_variants(ccx: @crate_ctxt, _sp: span, vs: [ast::variant],
                      id: ast::node_id) {
    // FIXME: this is kinda a kludge; we manufacture a fake function context
    // and statement context for checking the initializer expression.
    let rty = node_id_to_type(ccx.tcx, id);
    let fixups: [ast::node_id] = [];
    let fcx: @fn_ctxt =
        @{ret_ty: rty,
          purity: ast::pure_fn,
          proto: ast::proto_box,
          var_bindings: ty::unify::mk_var_bindings(),
          locals: new_int_hash::<int>(),
          next_var_id: @mutable 0,
          mutable fixups: fixups,
          ccx: ccx};
    let disr_vals: [int] = [];
    let disr_val = 0;
    for v in vs {
        alt v.node.disr_expr {
          some(e) {
            check_expr(fcx, e);
            let cty = expr_ty(fcx.ccx.tcx, e);
            let declty =ty::mk_int(fcx.ccx.tcx);
            demand::simple(fcx, e.span, declty, cty);
            // FIXME: issue #1417
            // Also, check_expr (from check_const pass) doesn't guarantee that
            // the expression in an form that eval_const_expr can handle, so
            // we may still get an internal compiler error.
            alt syntax::ast_util::eval_const_expr(e) {
              syntax::ast_util::const_int(val) {
                disr_val = val as int;
              }
              _ {
                ccx.tcx.sess.span_err(e.span,
                                      "expected signed integer constant");
              }
            }
          }
          _ {}
        }
        if vec::member(disr_val, disr_vals) {
            ccx.tcx.sess.span_err(v.span,
                                  "discriminator value already exists.");
        }
        disr_vals += [disr_val];
        disr_val += 1;
    }
}

// A generic function for checking the pred in a check
// or if-check
fn check_pred_expr(fcx: @fn_ctxt, e: @ast::expr) -> bool {
    let bot = check_expr_with(fcx, e, ty::mk_bool(fcx.ccx.tcx));

    /* e must be a call expr where all arguments are either
    literals or slots */
    alt e.node {
      ast::expr_call(operator, operands, _) {
        if !ty::is_pred_ty(fcx.ccx.tcx, expr_ty(fcx.ccx.tcx, operator)) {
            fcx.ccx.tcx.sess.span_err
                (operator.span,
                 "operator in constraint has non-boolean return type");
        }

        alt operator.node {
          ast::expr_path(oper_name) {
            alt fcx.ccx.tcx.def_map.find(operator.id) {
              some(ast::def_fn(_, ast::pure_fn.)) {
                // do nothing
              }
              _ {
                fcx.ccx.tcx.sess.span_err(operator.span,
                                            "Impure function as operator \
                                             in constraint");
              }
            }
            for operand: @ast::expr in operands {
                if !ast_util::is_constraint_arg(operand) {
                    let s =
                        "Constraint args must be slot variables or literals";
                    fcx.ccx.tcx.sess.span_err(e.span, s);
                }
            }
          }
          _ {
            let s = "In a constraint, expected the \
                     constraint name to be an explicit name";
            fcx.ccx.tcx.sess.span_err(e.span, s);
          }
        }
      }
      _ { fcx.ccx.tcx.sess.span_err(e.span, "check on non-predicate"); }
    }
    ret bot;
}

fn check_constraints(fcx: @fn_ctxt, cs: [@ast::constr], args: [ast::arg]) {
    let c_args;
    let num_args = vec::len(args);
    for c: @ast::constr in cs {
        c_args = [];
        for a: @spanned<ast::fn_constr_arg> in c.node.args {
            c_args += [
                 // "base" should not occur in a fn type thing, as of
                 // yet, b/c we don't allow constraints on the return type

                 // Works b/c no higher-order polymorphism
                 /*
                 This is kludgy, and we probably shouldn't be assigning
                 node IDs here, but we're creating exprs that are
                 ephemeral, just for the purposes of typechecking. So
                 that's my justification.
                 */
                 @alt a.node {
                    ast::carg_base. {
                      fcx.ccx.tcx.sess.span_bug(a.span,
                                                "check_constraints:\
                    unexpected carg_base");
                    }
                    ast::carg_lit(l) {
                      let tmp_node_id = fcx.ccx.tcx.sess.next_node_id();
                      {id: tmp_node_id, node: ast::expr_lit(l), span: a.span}
                    }
                    ast::carg_ident(i) {
                      if i < num_args {
                          let p: ast::path_ =
                              {global: false,
                               idents: [args[i].ident],
                               types: []};
                          let arg_occ_node_id =
                              fcx.ccx.tcx.sess.next_node_id();
                          fcx.ccx.tcx.def_map.insert
                              (arg_occ_node_id,
                               ast::def_arg(local_def(args[i].id),
                                            args[i].mode));
                          {id: arg_occ_node_id,
                           node: ast::expr_path(@respan(a.span, p)),
                           span: a.span}
                      } else {
                          fcx.ccx.tcx.sess.span_bug(a.span,
                                                    "check_constraints:\
                     carg_ident index out of bounds");
                      }
                    }
                  }];
        }
        let p_op: ast::expr_ = ast::expr_path(c.node.path);
        let oper: @ast::expr = @{id: c.node.id, node: p_op, span: c.span};
        // Another ephemeral expr
        let call_expr_id = fcx.ccx.tcx.sess.next_node_id();
        let call_expr =
            @{id: call_expr_id,
              node: ast::expr_call(oper, c_args, false),
              span: c.span};
        check_pred_expr(fcx, call_expr);
    }
}

fn check_fn(ccx: @crate_ctxt,
            proto: ast::proto,
            decl: ast::fn_decl,
            body: ast::blk,
            id: ast::node_id,
            old_fcx: option::t<@fn_ctxt>) {
    // If old_fcx is some(...), this is a block fn { |x| ... }.
    // In that case, the purity is inherited from the context.
    let purity = alt old_fcx {
      none. { decl.purity }
      some(f) { assert decl.purity == ast::impure_fn; f.purity }
    };

    let gather_result = gather_locals(ccx, decl, body, id, old_fcx);
    let fixups: [ast::node_id] = [];
    let fcx: @fn_ctxt =
        @{ret_ty: ty::ty_fn_ret(ccx.tcx, ty::node_id_to_type(ccx.tcx, id)),
          purity: purity,
          proto: proto,
          var_bindings: gather_result.var_bindings,
          locals: gather_result.locals,
          next_var_id: gather_result.next_var_id,
          mutable fixups: fixups,
          ccx: ccx};

    check_constraints(fcx, decl.constraints, decl.inputs);
    check_block(fcx, body);

    // We unify the tail expr's type with the
    // function result type, if there is a tail expr.
    alt body.node.expr {
      some(tail_expr) {
        let tail_expr_ty = expr_ty(ccx.tcx, tail_expr);
        demand::simple(fcx, tail_expr.span, fcx.ret_ty, tail_expr_ty);
      }
      none. { }
    }

    let args = ty::ty_fn_args(ccx.tcx, ty::node_id_to_type(ccx.tcx, id));
    let i = 0u;
    for arg: ty::arg in args {
        write::ty_only_fixup(fcx, decl.inputs[i].id, arg.ty);
        i += 1u;
    }

    // If we don't have any enclosing function scope, it is time to
    // force any remaining type vars to be resolved.
    // If we have an enclosing function scope, our type variables will be
    // resolved when the enclosing scope finishes up.
    if option::is_none(old_fcx) {
        dict::resolve_in_block(fcx, body);
        writeback::resolve_type_vars_in_block(fcx, body);
    }
}

fn check_method(ccx: @crate_ctxt, method: @ast::method) {
    check_fn(ccx, ast::proto_bare, method.decl, method.body, method.id, none);
}

fn check_item(ccx: @crate_ctxt, it: @ast::item) {
    alt it.node {
      ast::item_const(_, e) { check_const(ccx, it.span, e, it.id); }
      ast::item_tag(vs, _) { check_tag_variants(ccx, it.span, vs, it.id); }
      ast::item_fn(decl, tps, body) {
        check_fn(ccx, ast::proto_bare, decl, body, it.id, none);
      }
      ast::item_res(decl, tps, body, dtor_id, _) {
        check_fn(ccx, ast::proto_bare, decl, body, dtor_id, none);
      }
      ast::item_impl(tps, _, ty, ms) {
        ccx.self_infos += [self_impl(ast_ty_to_ty(ccx.tcx, m_check, ty))];
        for m in ms { check_method(ccx, m); }
        vec::pop(ccx.self_infos);
      }
      _ {/* nothing to do */ }
    }
}

fn arg_is_argv_ty(tcx: ty::ctxt, a: ty::arg) -> bool {
    alt ty::struct(tcx, a.ty) {
      ty::ty_vec(mt) {
        if mt.mut != ast::imm { ret false; }
        alt ty::struct(tcx, mt.ty) {
          ty::ty_str. { ret true; }
          _ { ret false; }
        }
      }
      _ { ret false; }
    }
}

fn check_main_fn_ty(tcx: ty::ctxt, main_id: ast::node_id) {
    let main_t = ty::node_id_to_monotype(tcx, main_id);
    alt ty::struct(tcx, main_t) {
      ty::ty_fn({proto: ast::proto_bare., inputs, output,
                 ret_style: ast::return_val., constraints}) {
        let ok = vec::len(constraints) == 0u;
        ok &= ty::type_is_nil(tcx, output);
        let num_args = vec::len(inputs);
        ok &= num_args == 0u || num_args == 1u &&
              arg_is_argv_ty(tcx, inputs[0]);
        if !ok {
            let span = ast_map::node_span(tcx.items.get(main_id));
            tcx.sess.span_err(span,
                              "wrong type in main function: found `" +
                                  ty_to_str(tcx, main_t) + "`");
        }
      }
      _ {
        let span = ast_map::node_span(tcx.items.get(main_id));
        tcx.sess.span_bug(span,
                          "main has a non-function type: found `" +
                              ty_to_str(tcx, main_t) + "`");
      }
    }
}

fn check_for_main_fn(tcx: ty::ctxt, crate: @ast::crate) {
    if !tcx.sess.building_library {
        alt tcx.sess.main_fn {
          some(id) { check_main_fn_ty(tcx, id); }
          none. { tcx.sess.span_err(crate.span, "main function not found"); }
        }
    }
}

mod dict {
    fn has_iface_bounds(tps: [ty::param_bounds]) -> bool {
        vec::any(tps, {|bs|
            vec::any(*bs, {|b|
                alt b { ty::bound_iface(_) { true } _ { false } }
            })
        })
    }

    fn lookup_dicts(fcx: @fn_ctxt, isc: resolve::iscopes, sp: span,
                    bounds: @[ty::param_bounds], tys: [ty::t])
        -> dict_res {
        let tcx = fcx.ccx.tcx, result = [], i = 0u;
        for ty in tys {
            for bound in *bounds[i] {
                alt bound {
                  ty::bound_iface(i_ty) {
                    let i_ty = ty::substitute_type_params(tcx, tys, i_ty);
                    result += [lookup_dict(fcx, isc, sp, ty, i_ty)];
                  }
                  _ {}
                }
            }
            i += 1u;
        }
        @result
    }

    fn lookup_dict(fcx: @fn_ctxt, isc: resolve::iscopes, sp: span,
                   ty: ty::t, iface_ty: ty::t) -> dict_origin {
        let tcx = fcx.ccx.tcx;
        let (iface_id, iface_tps) = alt ty::struct(tcx, iface_ty) {
            ty::ty_iface(did, tps) { (did, tps) }
        };
        let ty = fixup_ty(fcx, sp, ty);
        alt ty::struct(tcx, ty) {
          ty::ty_param(n, did) {
            let n_bound = 0u;
            for bound in *tcx.ty_param_bounds.get(did.node) {
                alt bound {
                  ty::bound_iface(ity) {
                    alt ty::struct(tcx, ity) {
                      ty::ty_iface(idid, _) {
                        if iface_id == idid { ret dict_param(n, n_bound); }
                      }
                    }
                    n_bound += 1u;
                  }
                  _ {}
                }
            }
          }
          ty::ty_iface(did, _) {
            ret dict_iface(did);
          }
          _ {
            let found = none;
            std::list::iter(isc) {|impls|
                if option::is_some(found) { ret; }
                for im in *impls {
                    let match = alt ty::impl_iface(tcx, im.did) {
                      some(ity) {
                        alt ty::struct(tcx, ity) {
                          ty::ty_iface(id, _) { id == iface_id }
                        }
                      }
                      _ { false }
                    };
                    if match {
                        let {n_tps, ty: self_ty} = impl_self_ty(tcx, im.did);
                        let {vars, ty: self_ty} = if n_tps > 0u {
                            bind_params(fcx, self_ty, n_tps)
                        } else { {vars: [], ty: self_ty} };
                        let im_bs = ty::lookup_item_type(tcx, im.did).bounds;
                        alt unify::unify(fcx, ty, self_ty) {
                          ures_ok(_) {
                            if option::is_some(found) {
                                tcx.sess.span_err(
                                    sp, "multiple applicable implementations \
                                         in scope");
                            } else {
                                connect_iface_tps(fcx, sp, vars, iface_tps,
                                                  im.did);
                                let params = vec::map(vars, {|t|
                                    fixup_ty(fcx, sp, t)});
                                let subres = lookup_dicts(fcx, isc, sp, im_bs,
                                                          params);
                                found = some(dict_static(im.did, params,
                                                         subres));
                            }
                          }
                          _ {}
                        }
                    }
                }
            }
            alt found {
              some(rslt) { ret rslt; }
              _ {}
            }
          }
        }

        tcx.sess.span_fatal(
            sp, "failed to find an implementation of interface " +
            ty_to_str(tcx, iface_ty) + " for " +
            ty_to_str(tcx, ty));
    }

    fn fixup_ty(fcx: @fn_ctxt, sp: span, ty: ty::t) -> ty::t {
        let tcx = fcx.ccx.tcx;
        alt ty::unify::fixup_vars(tcx, some(sp), fcx.var_bindings, ty) {
          fix_ok(new_type) { new_type }
          fix_err(vid) {
            tcx.sess.span_fatal(sp, "could not determine a type for a \
                                     bounded type parameter");
          }
        }
    }

    fn connect_iface_tps(fcx: @fn_ctxt, sp: span, impl_tys: [ty::t],
                         iface_tys: [ty::t], impl_did: ast::def_id) {
        let tcx = fcx.ccx.tcx;
        let ity = option::get(ty::impl_iface(tcx, impl_did));
        let iface_ty = ty::substitute_type_params(tcx, impl_tys, ity);
        alt ty::struct(tcx, iface_ty) {
          ty::ty_iface(_, tps) {
            vec::iter2(tps, iface_tys,
                       {|a, b| demand::simple(fcx, sp, a, b);});
          }
        }
    }

    fn resolve_expr(ex: @ast::expr, &&fcx: @fn_ctxt, v: visit::vt<@fn_ctxt>) {
        let cx = fcx.ccx;
        alt ex.node {
          ast::expr_path(_) {
            let substs = ty::node_id_to_ty_param_substs_opt_and_ty(
                cx.tcx, ex.id);
            alt substs.substs {
              some(ts) {
                let did = ast_util::def_id_of_def(cx.tcx.def_map.get(ex.id));
                let item_ty = ty::lookup_item_type(cx.tcx, did);
                if has_iface_bounds(*item_ty.bounds) {
                    let impls = cx.impl_map.get(ex.id);
                    cx.dict_map.insert(ex.id, lookup_dicts(
                        fcx, impls, ex.span, item_ty.bounds, ts));
                }
              }
              _ {}
            }
          }
          // Must resolve bounds on methods with bounded params
          ast::expr_field(_, _, _) {
            alt cx.method_map.find(ex.id) {
              some(method_static(did)) {
                let bounds = ty::lookup_item_type(cx.tcx, did).bounds;
                if has_iface_bounds(*bounds) {
                    let ts = ty::node_id_to_type_params(cx.tcx, ex.id);
                    let iscs = cx.impl_map.get(ex.id);
                    cx.dict_map.insert(ex.id, lookup_dicts(
                        fcx, iscs, ex.span, bounds, ts));
                }
              }
              _ {}
            }
          }
          ast::expr_cast(src, _) {
            let target_ty = expr_ty(cx.tcx, ex);
            alt ty::struct(cx.tcx, target_ty) {
              ty::ty_iface(_, _) {
                let impls = cx.impl_map.get(ex.id);
                let dict = lookup_dict(fcx, impls, ex.span,
                                       expr_ty(cx.tcx, src), target_ty);
                cx.dict_map.insert(ex.id, @[dict]);
              }
              _ {}
            }
          }
          ast::expr_fn(p, _, _, _) if ast::is_blockish(p) {}
          ast::expr_fn(_, _, _, _) { ret; }
          _ {}
        }
        visit::visit_expr(ex, fcx, v);
    }

    // Detect points where an interface-bounded type parameter is
    // instantiated, resolve the impls for the parameters.
    fn resolve_in_block(fcx: @fn_ctxt, bl: ast::blk) {
        visit::visit_block(bl, fcx, visit::mk_vt(@{
            visit_expr: resolve_expr,
            visit_item: fn@(_i: @ast::item, &&_e: @fn_ctxt,
                            _v: visit::vt<@fn_ctxt>) {}
            with *visit::default_visitor()
        }));
    }
}

fn check_crate(tcx: ty::ctxt, impl_map: resolve::impl_map,
               crate: @ast::crate) -> (method_map, dict_map) {
    collect::collect_item_types(tcx, crate);

    let ccx = @{mutable self_infos: [],
                impl_map: impl_map,
                method_map: std::map::new_int_hash(),
                dict_map: std::map::new_int_hash(),
                tcx: tcx};
    let visit = visit::mk_simple_visitor(@{
        visit_item: bind check_item(ccx, _)
        with *visit::default_simple_visitor()
    });
    visit::visit_crate(*crate, (), visit);
    check_for_main_fn(tcx, crate);
    tcx.sess.abort_if_errors();
    (ccx.method_map, ccx.dict_map)
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
