import front::ast;
import front::ast::ann;
import front::ast::mutability;
import front::creader;
import middle::fold;
import driver::session;
import util::common;
import util::common::span;
import util::common::new_def_hash;
import util::common::log_expr_err;

import middle::ty;
import middle::ty::ann_to_type;
import middle::ty::arg;
import middle::ty::bind_params_in_type;
import middle::ty::block_ty;
import middle::ty::expr_ty;
import middle::ty::field;
import middle::ty::method;
import middle::ty::mk_ann_type;
import middle::ty::mo_val;
import middle::ty::mo_alias;
import middle::ty::mo_either;
import middle::ty::node_type_table;
import middle::ty::pat_ty;
import middle::ty::path_to_str;
import middle::ty::plain_ann;
import middle::ty::bot_ann;
import middle::ty::struct;
import middle::ty::triv_ann;
import middle::ty::ty_param_substs_opt_and_ty;
import middle::ty::ty_to_str;
import middle::ty::type_is_integral;
import middle::ty::type_is_scalar;
import middle::ty::ty_param_count_and_ty;
import middle::ty::ty_nil;
import middle::ty::unify::ures_ok;
import middle::ty::unify::ures_err;

import std::str;
import std::uint;
import std::vec;
import std::map;
import std::map::hashmap;
import std::option;
import std::option::none;
import std::option::some;
import std::option::from_maybe;

import middle::tstate::ann::ts_ann;

type ty_table = hashmap[ast::def_id, ty::t];

tag any_item {
    any_item_rust(@ast::item);
    any_item_native(@ast::native_item, ast::native_abi);
}

type ty_item_table = hashmap[ast::def_id,any_item];
type fn_purity_table = hashmap[ast::def_id, ast::purity];

type unify_cache_entry = tup(ty::t,ty::t,vec[mutable ty::t]);
type unify_cache = hashmap[unify_cache_entry,ty::unify::result];

type crate_ctxt = rec(session::session sess,
                      ty::type_cache type_cache,
                      @ty_item_table item_items,
                      vec[ast::obj_field] obj_fields,
                      option::t[ast::def_id] this_obj,
                      @fn_purity_table fn_purity_table,
                      mutable int next_var_id,
                      unify_cache unify_cache,
                      mutable uint cache_hits,
                      mutable uint cache_misses,
                      ty::ctxt tcx,
                      node_type_table node_types);

type fn_ctxt = rec(ty::t ret_ty,
                   ast::purity purity,
                   @ty_table locals,
                   @crate_ctxt ccx);

// Used for ast_ty_to_ty() below.
type ty_getter = fn(&ast::def_id) -> ty::ty_param_count_and_ty;

// Substitutes the user's explicit types for the parameters in a path
// expression.
fn substitute_ty_params(&@crate_ctxt ccx,
                        &ty::t typ,
                        uint ty_param_count,
                        &vec[ty::t] supplied,
                        &span sp) -> ty::t {
    fn substituter(@crate_ctxt ccx, vec[ty::t] supplied, ty::t typ) -> ty::t {
        alt (struct(ccx.tcx, typ)) {
            case (ty::ty_bound_param(?pid)) { ret supplied.(pid); }
            case (_) { ret typ; }
        }
    }

    auto supplied_len = vec::len[ty::t](supplied);
    if (ty_param_count != supplied_len) {
        ccx.sess.span_err(sp, "expected " +
                          uint::to_str(ty_param_count, 10u) +
                          " type parameter(s) but found " +
                          uint::to_str(supplied_len, 10u) + " parameter(s)");
        fail;
    }

    if (!ty::type_contains_bound_params(ccx.tcx, typ)) {
        ret typ;
    }

    auto f = bind substituter(ccx, supplied, _);
    ret ty::fold_ty(ccx.tcx, f, typ);
}


// Returns the type parameter count and the type for the given definition.
fn ty_param_count_and_ty_for_def(&@fn_ctxt fcx, &ast::span sp, &ast::def defn)
        -> ty_param_count_and_ty {
    alt (defn) {
        case (ast::def_arg(?id)) {
            // assert (fcx.locals.contains_key(id));
            ret tup(0u, fcx.locals.get(id));
        }
        case (ast::def_local(?id)) {
            auto t;
            alt (fcx.locals.find(id)) {
                case (some[ty::t](?t1)) { t = t1; }
                case (none[ty::t]) { t = ty::mk_local(fcx.ccx.tcx, id); }
            }
            ret tup(0u, t);
        }
        case (ast::def_obj_field(?id)) {
            // assert (fcx.locals.contains_key(id));
            ret tup(0u, fcx.locals.get(id));
        }
        case (ast::def_fn(?id)) {
            ret ty::lookup_item_type(fcx.ccx.sess, fcx.ccx.tcx,
                                     fcx.ccx.type_cache, id);
        }
        case (ast::def_native_fn(?id)) {
            ret ty::lookup_item_type(fcx.ccx.sess, fcx.ccx.tcx,
                                     fcx.ccx.type_cache, id);
        }
        case (ast::def_const(?id)) {
            ret ty::lookup_item_type(fcx.ccx.sess, fcx.ccx.tcx,
                                     fcx.ccx.type_cache, id);
        }
        case (ast::def_variant(_, ?vid)) {
            ret ty::lookup_item_type(fcx.ccx.sess, fcx.ccx.tcx,
                                     fcx.ccx.type_cache, vid);
        }
        case (ast::def_binding(?id)) {
            // assert (fcx.locals.contains_key(id));
            ret tup(0u, fcx.locals.get(id));
        }
        case (ast::def_obj(?id)) {
            ret ty::lookup_item_type(fcx.ccx.sess, fcx.ccx.tcx,
                                     fcx.ccx.type_cache, id);
        }

        case (ast::def_mod(_)) {
            // Hopefully part of a path.
            // TODO: return a type that's more poisonous, perhaps?
            ret tup(0u, ty::mk_nil(fcx.ccx.tcx));
        }

        case (ast::def_ty(_)) {
            fcx.ccx.sess.span_err(sp, "expected value but found type");
            fail;
        }

        case (_) {
            // FIXME: handle other names.
            fcx.ccx.sess.unimpl("definition variant");
            fail;
        }
    }
}

// Instantiates the given path, which must refer to an item with the given
// number of type parameters and type.
fn instantiate_path(&@fn_ctxt fcx, &ast::path pth, &ty_param_count_and_ty tpt,
                    &span sp) -> ty_param_substs_opt_and_ty {
    auto ty_param_count = tpt._0;
    auto t = bind_params_in_type(fcx.ccx.tcx, tpt._1);

    auto ty_substs_opt;
    auto ty_substs_len = vec::len[@ast::ty](pth.node.types);
    if (ty_substs_len > 0u) {
        let vec[ty::t] ty_substs = [];
        auto i = 0u;
        while (i < ty_substs_len) {
            ty_substs += [ast_ty_to_ty_crate(fcx.ccx, pth.node.types.(i))];
            i += 1u;
        }
        ty_substs_opt = some[vec[ty::t]](ty_substs);

        if (ty_param_count == 0u) {
            fcx.ccx.sess.span_err(sp, "this item does not take type " +
                                  "parameters");
            fail;
        }
    } else {
        // We will acquire the type parameters through unification.
        let vec[ty::t] ty_substs = [];
        auto i = 0u;
        while (i < ty_param_count) {
            ty_substs += [next_ty_var(fcx.ccx)];
            i += 1u;
        }
        ty_substs_opt = some[vec[ty::t]](ty_substs);
    }

    ret tup(ty_substs_opt, t);
}

fn ast_mode_to_mode(ast::mode mode) -> ty::mode {
    auto ty_mode;
    alt (mode) {
        case (ast::val) { ty_mode = mo_val; }
        case (ast::alias) { ty_mode = mo_alias; }
    }
    ret ty_mode;
}

// Parses the programmer's textual representation of a type into our internal
// notion of a type. `getter` is a function that returns the type
// corresponding to a definition ID:
fn ast_ty_to_ty(&ty::ctxt tcx, &ty_getter getter, &@ast::ty ast_ty) -> ty::t {
    fn ast_arg_to_arg(&ty::ctxt tcx,
                      &ty_getter getter,
                      &rec(ast::mode mode, @ast::ty ty) arg)
            -> rec(ty::mode mode, ty::t ty) {
        auto ty_mode = ast_mode_to_mode(arg.mode);
        ret rec(mode=ty_mode, ty=ast_ty_to_ty(tcx, getter, arg.ty));
    }

    fn ast_mt_to_mt(&ty::ctxt tcx,
                    &ty_getter getter,
                    &ast::mt mt) -> ty::mt {
        ret rec(ty=ast_ty_to_ty(tcx, getter, mt.ty), mut=mt.mut);
    }

    fn instantiate(&ty::ctxt tcx,
                   &ty_getter getter,
                   &ast::def_id id,
                   &vec[@ast::ty] args) -> ty::t {
        // TODO: maybe record cname chains so we can do
        // "foo = int" like OCaml?
        auto params_opt_and_ty = getter(id);

        if (params_opt_and_ty._0 == 0u) {
            ret params_opt_and_ty._1;
        }

        // The typedef is type-parametric. Do the type substitution.
        //
        // TODO: Make sure the number of supplied bindings matches the number
        // of type parameters in the typedef. Emit a friendly error otherwise.
        auto bound_ty = bind_params_in_type(tcx, params_opt_and_ty._1);
        let vec[ty::t] param_bindings = [];
        for (@ast::ty ast_ty in args) {
            param_bindings += [ast_ty_to_ty(tcx, getter, ast_ty)];
        }
        ret ty::substitute_type_params(tcx, param_bindings, bound_ty);
    }

    auto mut = ast::imm;
    auto typ;
    auto cname = none[str];
    alt (ast_ty.node) {
        case (ast::ty_nil)          { typ = ty::mk_nil(tcx); }
        case (ast::ty_bot)          { typ = ty::mk_bot(tcx); }
        case (ast::ty_bool)         { typ = ty::mk_bool(tcx); }
        case (ast::ty_int)          { typ = ty::mk_int(tcx); }
        case (ast::ty_uint)         { typ = ty::mk_uint(tcx); }
        case (ast::ty_float)        { typ = ty::mk_float(tcx); }
        case (ast::ty_machine(?tm)) { typ = ty::mk_mach(tcx, tm); }
        case (ast::ty_char)         { typ = ty::mk_char(tcx); }
        case (ast::ty_str)          { typ = ty::mk_str(tcx); }
        case (ast::ty_box(?mt)) {
            typ = ty::mk_box(tcx, ast_mt_to_mt(tcx, getter, mt));
        }
        case (ast::ty_vec(?mt)) {
            typ = ty::mk_vec(tcx, ast_mt_to_mt(tcx, getter, mt));
        }

        case (ast::ty_port(?t)) {
            typ = ty::mk_port(tcx, ast_ty_to_ty(tcx, getter, t));
        }

        case (ast::ty_chan(?t)) {
            typ = ty::mk_chan(tcx, ast_ty_to_ty(tcx, getter, t));
        }

        case (ast::ty_tup(?fields)) {
            let vec[ty::mt] flds = [];
            for (ast::mt field in fields) {
                vec::push[ty::mt](flds, ast_mt_to_mt(tcx, getter, field));
            }
            typ = ty::mk_tup(tcx, flds);
        }
        case (ast::ty_rec(?fields)) {
            let vec[field] flds = [];
            for (ast::ty_field f in fields) {
                auto tm = ast_mt_to_mt(tcx, getter, f.mt);
                vec::push[field](flds, rec(ident=f.ident, mt=tm));
            }
            typ = ty::mk_rec(tcx, flds);
        }

        case (ast::ty_fn(?proto, ?inputs, ?output)) {
            auto f = bind ast_arg_to_arg(tcx, getter, _);
            auto i = vec::map[ast::ty_arg, arg](f, inputs);
            auto out_ty = ast_ty_to_ty(tcx, getter, output);
            typ = ty::mk_fn(tcx, proto, i, out_ty);
        }

        case (ast::ty_path(?path, ?ann)) {
            alt (tcx.def_map.get(ann.id)) {
                case (ast::def_ty(?id)) {
                    typ = instantiate(tcx, getter, id, path.node.types);
                }
                case (ast::def_native_ty(?id)) { typ = getter(id)._1; }
                case (ast::def_obj(?id)) {
                    typ = instantiate(tcx, getter, id, path.node.types);
                }
                case (ast::def_ty_arg(?id)) { typ = ty::mk_param(tcx, id); }
                case (_)                   {
                    tcx.sess.span_err(ast_ty.span,
                       "found type name used as a variable");
                    fail; }
            }

            cname = some(path_to_str(path));
        }

        case (ast::ty_obj(?meths)) {
            let vec[ty::method] tmeths = [];
            auto f = bind ast_arg_to_arg(tcx, getter, _);
            for (ast::ty_method m in meths) {
                auto ins = vec::map[ast::ty_arg, arg](f, m.inputs);
                auto out = ast_ty_to_ty(tcx, getter, m.output);
                vec::push[ty::method](tmeths,
                                  rec(proto=m.proto,
                                      ident=m.ident,
                                      inputs=ins,
                                      output=out));
            }

            typ = ty::mk_obj(tcx, ty::sort_methods(tmeths));
        }
    }

    alt (cname) {
        case (none[str]) { /* no-op */ }
        case (some[str](?cname_str)) {
            typ = ty::rename(tcx, typ, cname_str);
        }
    }
    ret typ;
}

// A convenience function to use a crate_ctxt to resolve names for
// ast_ty_to_ty.
fn ast_ty_to_ty_crate(@crate_ctxt ccx, &@ast::ty ast_ty) -> ty::t {
    fn getter(@crate_ctxt ccx, &ast::def_id id) -> ty::ty_param_count_and_ty {
        ret ty::lookup_item_type(ccx.sess, ccx.tcx, ccx.type_cache, id);
    }
    auto f = bind getter(ccx, _);
    ret ast_ty_to_ty(ccx.tcx, f, ast_ty);
}


// Writes a type parameter count and type pair into the node type table.
fn write_type(&node_type_table ntt, uint node_id,
              &ty_param_substs_opt_and_ty tpot) {
    vec::grow_set[option::t[ty::ty_param_substs_opt_and_ty]]
        (*ntt,
         node_id,
         none[ty_param_substs_opt_and_ty],
         some[ty_param_substs_opt_and_ty](tpot));
}

// Writes a type with no type parameters into the node type table.
fn write_type_only(&node_type_table ntt, uint node_id, ty::t ty) {
    be write_type(ntt, node_id, tup(none[vec[ty::t]], ty));
}

// Writes a nil type into the node type table.
fn write_nil_type(ty::ctxt tcx, &node_type_table ntt, uint node_id) {
    be write_type_only(ntt, node_id, ty::mk_nil(tcx));
}

// Writes the bottom type into the node type table.
fn write_bot_type(ty::ctxt tcx, &node_type_table ntt, uint node_id) {
    // FIXME: Should be mk_bot(), but this breaks lots of stuff.
    be write_type_only(ntt, node_id, ty::mk_nil(tcx));
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
    type ctxt = rec(session::session sess,
                    @ty_item_table id_to_ty_item,
                    ty::type_cache type_cache,
                    ty::ctxt tcx,
                    node_type_table node_types);
    type env = rec(@ctxt cx, ast::native_abi abi);

    fn ty_of_fn_decl(&@ctxt cx,
                     &fn(&@ast::ty ast_ty) -> ty::t convert,
                     &fn(&ast::arg a) -> arg ty_of_arg,
                     &ast::fn_decl decl,
                     ast::proto proto,
                     &vec[ast::ty_param] ty_params,
                     &ast::def_id def_id) -> ty::ty_param_count_and_ty {
        auto input_tys = vec::map[ast::arg,arg](ty_of_arg, decl.inputs);
        auto output_ty = convert(decl.output);
        auto t_fn = ty::mk_fn(cx.tcx, proto, input_tys, output_ty);
        auto ty_param_count = vec::len[ast::ty_param](ty_params);
        auto tpt = tup(ty_param_count, t_fn);
        cx.type_cache.insert(def_id, tpt);
        ret tpt;
    }

    fn ty_of_native_fn_decl(&@ctxt cx,
                            &fn(&@ast::ty ast_ty) -> ty::t convert,
                            &fn(&ast::arg a) -> arg ty_of_arg,
                            &ast::fn_decl decl,
                            ast::native_abi abi,
                            &vec[ast::ty_param] ty_params,
                            &ast::def_id def_id) -> ty::ty_param_count_and_ty{
        auto input_tys = vec::map[ast::arg,arg](ty_of_arg, decl.inputs);
        auto output_ty = convert(decl.output);
        auto t_fn = ty::mk_native_fn(cx.tcx, abi, input_tys, output_ty);
        auto ty_param_count = vec::len[ast::ty_param](ty_params);
        auto tpt = tup(ty_param_count, t_fn);
        cx.type_cache.insert(def_id, tpt);
        ret tpt;
    }

    fn getter(@ctxt cx, &ast::def_id id) -> ty::ty_param_count_and_ty {

        if (id._0 != cx.sess.get_targ_crate_num()) {
            // This is a type we need to load in from the crate reader.
            ret creader::get_type(cx.sess, cx.tcx, id);
        }

        auto it = cx.id_to_ty_item.get(id);
        auto tpt;
        alt (it) {
            case (any_item_rust(?item)) { tpt = ty_of_item(cx, item); }
            case (any_item_native(?native_item, ?abi)) {
                tpt = ty_of_native_item(cx, native_item, abi);
            }
        }

        ret tpt;
    }

    fn ty_of_arg(@ctxt cx, &ast::arg a) -> arg {
        auto ty_mode = ast_mode_to_mode(a.mode);
        auto f = bind getter(cx, _);
        ret rec(mode=ty_mode, ty=ast_ty_to_ty(cx.tcx, f, a.ty));
    }

    fn ty_of_method(@ctxt cx, &@ast::method m) -> method {
        auto get = bind getter(cx, _);
        auto convert = bind ast_ty_to_ty(cx.tcx, get, _);
        auto f = bind ty_of_arg(cx, _);
        auto inputs = vec::map[ast::arg,arg](f, m.node.meth.decl.inputs);
        auto output = convert(m.node.meth.decl.output);
        ret rec(proto=m.node.meth.proto, ident=m.node.ident,
                inputs=inputs, output=output);
    }

    fn ty_of_obj(@ctxt cx,
                 &ast::ident id,
                 &ast::_obj obj_info,
                 &vec[ast::ty_param] ty_params) -> ty::ty_param_count_and_ty {
        auto methods = get_obj_method_types(cx, obj_info);
        auto t_obj = ty::mk_obj(cx.tcx, ty::sort_methods(methods));
        t_obj = ty::rename(cx.tcx, t_obj, id);
        auto ty_param_count = vec::len[ast::ty_param](ty_params);
        ret tup(ty_param_count, t_obj);
    }

    fn ty_of_obj_ctor(@ctxt cx,
                      &ast::ident id,
                      &ast::_obj obj_info,
                      &ast::def_id ctor_id,
                      &vec[ast::ty_param] ty_params)
            -> ty::ty_param_count_and_ty {
        auto t_obj = ty_of_obj(cx, id, obj_info, ty_params);

        let vec[arg] t_inputs = [];
        for (ast::obj_field f in obj_info.fields) {
            auto g = bind getter(cx, _);
            auto t_field = ast_ty_to_ty(cx.tcx, g, f.ty);
            vec::push[arg](t_inputs, rec(mode=ty::mo_alias, ty=t_field));
        }

        auto t_fn = ty::mk_fn(cx.tcx, ast::proto_fn, t_inputs, t_obj._1);

        auto tpt = tup(t_obj._0, t_fn);
        cx.type_cache.insert(ctor_id, tpt);
        ret tpt;
    }

    fn ty_of_item(&@ctxt cx, &@ast::item it) -> ty::ty_param_count_and_ty {

        auto get = bind getter(cx, _);
        auto convert = bind ast_ty_to_ty(cx.tcx, get, _);

        alt (it.node) {

            case (ast::item_const(?ident, ?t, _, ?def_id, _)) {
                auto typ = convert(t);
                auto tpt = tup(0u, typ);
                cx.type_cache.insert(def_id, tpt);
                ret tpt;
            }

            case (ast::item_fn(?ident, ?fn_info, ?tps, ?def_id, _)) {
                auto f = bind ty_of_arg(cx, _);
                ret ty_of_fn_decl(cx, convert, f, fn_info.decl, fn_info.proto,
                                  tps, def_id);
            }

            case (ast::item_obj(?ident, ?obj_info, ?tps, ?odid, _)) {
                auto t_obj = ty_of_obj(cx, ident, obj_info, tps);
                cx.type_cache.insert(odid.ty, t_obj);
                ret t_obj;
            }

            case (ast::item_ty(?ident, ?t, ?tps, ?def_id, _)) {
                alt (cx.type_cache.find(def_id)) {
                    case (some[ty::ty_param_count_and_ty](?tpt)) {
                        ret tpt;
                    }
                    case (none[ty::ty_param_count_and_ty]) {}
                }

                // Tell ast_ty_to_ty() that we want to perform a recursive
                // call to resolve any named types.
                auto typ = convert(t);
                auto ty_param_count = vec::len[ast::ty_param](tps);
                auto tpt = tup(ty_param_count, typ);
                cx.type_cache.insert(def_id, tpt);
                ret tpt;
            }

            case (ast::item_tag(_, _, ?tps, ?def_id, _)) {
                // Create a new generic polytype.
                let vec[ty::t] subtys = [];

                auto i = 0u;
                for (ast::ty_param tp in tps) {
                    subtys += [ty::mk_param(cx.tcx, i)];
                    i += 1u;
                }

                auto t = ty::mk_tag(cx.tcx, def_id, subtys);

                auto ty_param_count = vec::len[ast::ty_param](tps);
                auto tpt = tup(ty_param_count, t);
                cx.type_cache.insert(def_id, tpt);
                ret tpt;
            }

            case (ast::item_mod(_, _, _)) { fail; }
            case (ast::item_native_mod(_, _, _)) { fail; }
        }
    }

    fn ty_of_native_item(&@ctxt cx, &@ast::native_item it,
                         ast::native_abi abi) -> ty::ty_param_count_and_ty {
        alt (it.node) {
            case (ast::native_item_fn(?ident, ?lname, ?fn_decl,
                                     ?params, ?def_id, _)) {
                auto get = bind getter(cx, _);
                auto convert = bind ast_ty_to_ty(cx.tcx, get, _);
                auto f = bind ty_of_arg(cx, _);
                ret ty_of_native_fn_decl(cx, convert, f, fn_decl, abi, params,
                                         def_id);
            }
            case (ast::native_item_ty(_, ?def_id)) {
                alt (cx.type_cache.find(def_id)) {
                    case (some[ty::ty_param_count_and_ty](?tpt)) {
                        ret tpt;
                    }
                    case (none[ty::ty_param_count_and_ty]) {}
                }

                auto t = ty::mk_native(cx.tcx);
                auto tpt = tup(0u, t);
                cx.type_cache.insert(def_id, tpt);
                ret tpt;
            }
        }
    }

    fn get_tag_variant_types(&@ctxt cx, &ast::def_id tag_id,
                             &vec[ast::variant] variants,
                             &vec[ast::ty_param] ty_params)
            -> vec[ast::variant] {
        let vec[ast::variant] result = [];

        // Create a set of parameter types shared among all the variants.
        let vec[ty::t] ty_param_tys = [];
        auto i = 0u;
        for (ast::ty_param tp in ty_params) {
            ty_param_tys += [ty::mk_param(cx.tcx, i)];
            i += 1u;
        }

        auto ty_param_count = vec::len[ast::ty_param](ty_params);

        for (ast::variant variant in variants) {
            // Nullary tag constructors get turned into constants; n-ary tag
            // constructors get turned into functions.
            auto result_ty;
            if (vec::len[ast::variant_arg](variant.node.args) == 0u) {
                result_ty = ty::mk_tag(cx.tcx, tag_id, ty_param_tys);
            } else {
                // As above, tell ast_ty_to_ty() that trans_ty_item_to_ty()
                // should be called to resolve named types.
                auto f = bind getter(cx, _);

                let vec[arg] args = [];
                for (ast::variant_arg va in variant.node.args) {
                    auto arg_ty = ast_ty_to_ty(cx.tcx, f, va.ty);
                    args += [rec(mode=ty::mo_alias, ty=arg_ty)];
                }
                auto tag_t = ty::mk_tag(cx.tcx, tag_id, ty_param_tys);
                result_ty = ty::mk_fn(cx.tcx, ast::proto_fn, args, tag_t);
            }

            auto tpt = tup(ty_param_count, result_ty);
            cx.type_cache.insert(variant.node.id, tpt);
            auto variant_t = rec(
                ann=triv_ann(variant.node.ann.id, result_ty)
                with variant.node
            );
            write_type_only(cx.node_types, variant.node.ann.id, result_ty);
            result += [fold::respan(variant.span, variant_t)];
        }

        ret result;
    }

    fn get_obj_method_types(&@ctxt cx, &ast::_obj object) -> vec[method] {
        ret vec::map[@ast::method,method](bind ty_of_method(cx, _),
                                           object.methods);
    }

    fn collect(&@ty_item_table id_to_ty_item, &@ast::item i) {
        alt (i.node) {
            case (ast::item_ty(_, _, _, ?def_id, _)) {
                id_to_ty_item.insert(def_id, any_item_rust(i));
            }
            case (ast::item_tag(_, _, _, ?def_id, _)) {
                id_to_ty_item.insert(def_id, any_item_rust(i));
            }
            case (ast::item_obj(_, _, _, ?odid, _)) {
                id_to_ty_item.insert(odid.ty, any_item_rust(i));
            }
            case (_) { /* empty */ }
        }
    }

    fn collect_native(&@ty_item_table id_to_ty_item, &@ast::native_item i) {
        alt (i.node) {
            case (ast::native_item_ty(_, ?def_id)) {
                // The abi of types is not used.
                id_to_ty_item.insert(def_id,
                    any_item_native(i, ast::native_abi_cdecl));
            }
            case (_) { /* no-op */ }
        }
    }

    fn convert(@ctxt cx, @mutable option::t[ast::native_abi] abi,
               &@ast::item it) {
        alt (it.node) {
            case (ast::item_mod(_, _, _)) {
                // ignore item_mod, it has no type.
            }
            case (ast::item_native_mod(_, ?native_mod, _)) {
                // Propagate the native ABI down to convert_native() below,
                // but otherwise do nothing, as native modules have no types.
                *abi = some[ast::native_abi](native_mod.abi);
            }
            case (ast::item_tag(_, ?variants, ?ty_params, ?tag_id, ?ann)) {
                auto tpt = ty_of_item(cx, it);
                write_type_only(cx.node_types, ann.id, tpt._1);

                get_tag_variant_types(cx, tag_id, variants, ty_params);
            }
            case (ast::item_obj(?ident, ?object, ?ty_params, ?odid, ?ann)) {
                // This calls ty_of_obj().
                auto t_obj = ty_of_item(cx, it);

                // Now we need to call ty_of_obj_ctor(); this is the type that
                // we write into the table for this item.
                auto tpt = ty_of_obj_ctor(cx, ident, object, odid.ctor,
                                          ty_params);
                write_type_only(cx.node_types, ann.id, tpt._1);

                // Write the methods into the type table.
                //
                // FIXME: Inefficient; this ends up calling
                // get_obj_method_types() twice. (The first time was above in
                // ty_of_obj().)
                auto method_types = get_obj_method_types(cx, object);
                auto i = 0u;
                while (i < vec::len[@ast::method](object.methods)) {
                    write_type_only(cx.node_types,
                                    object.methods.(i).node.ann.id,
                                    ty::method_ty_to_fn_ty(cx.tcx,
                                        method_types.(i)));
                    i += 1u;
                }

                // Write in the types of the object fields.
                //
                // FIXME: We want to use uint::range() here, but that causes
                // an assertion in trans.
                auto args = ty::ty_fn_args(cx.tcx, tpt._1);
                i = 0u;
                while (i < vec::len[ty::arg](args)) {
                    auto fld = object.fields.(i);
                    write_type_only(cx.node_types, fld.ann.id, args.(i).ty);
                    i += 1u;
                }

                // Finally, write in the type of the destructor.
                alt (object.dtor) {
                    case (none[@ast::method]) { /* nothing to do */ }
                    case (some[@ast::method](?m)) {
                        // TODO: typechecker botch
                        let vec[arg] no_args = [];
                        auto t = ty::mk_fn(cx.tcx, ast::proto_fn, no_args,
                                           ty::mk_nil(cx.tcx));
                        write_type_only(cx.node_types, m.node.ann.id, t);
                    }
                }
            }
            case (_) {
                // This call populates the type cache with the converted type
                // of the item in passing. All we have to do here is to write
                // it into the node type table.
                auto tpt = ty_of_item(cx, it);
                write_type_only(cx.node_types, ty::item_ann(it).id, tpt._1);
            }
        }
    }

    fn convert_native(@ctxt cx, @mutable option::t[ast::native_abi] abi,
                      &@ast::native_item i) {
        // As above, this call populates the type table with the converted
        // type of the native item. We simply write it into the node type
        // table.
        auto tpt = ty_of_native_item(cx, i,
                                     option::get[ast::native_abi](*abi));

        alt (i.node) {
            case (ast::native_item_ty(_,_)) {
                // FIXME: Native types have no annotation. Should they? --pcw
            }
            case (ast::native_item_fn(_,_,_,_,_,?a)) {
                write_type_only(cx.node_types, a.id, tpt._1);
            }
        }
    }

    fn collect_item_types(&session::session sess, &ty::ctxt tcx,
                          &@ast::crate crate)
            -> tup(ty::type_cache, @ty_item_table, node_type_table) {
        // First pass: collect all type item IDs.
        auto module = crate.node.module;
        auto id_to_ty_item = @common::new_def_hash[any_item]();

        let vec[mutable option::t[ty::ty_param_substs_opt_and_ty]] ntt_sub =
            [mutable];
        let node_type_table ntt = @mutable ntt_sub;

        auto visit = rec(
            visit_item_pre = bind collect(id_to_ty_item, _),
            visit_native_item_pre = bind collect_native(id_to_ty_item, _)
            with walk::default_visitor()
        );
        walk::walk_crate(visit, *crate);

        // Second pass: translate the types of all items.
        auto type_cache = common::new_def_hash[ty::ty_param_count_and_ty]();

        // We have to propagate the surrounding ABI to the native items
        // contained within the native module.
        auto abi = @mutable none[ast::native_abi];

        auto cx = @rec(sess=sess,
                       id_to_ty_item=id_to_ty_item,
                       type_cache=type_cache,
                       tcx=tcx,
                       node_types=ntt);

        visit = rec(
            visit_item_pre = bind convert(cx,abi,_),
            visit_native_item_pre = bind convert_native(cx,abi,_)
            with walk::default_visitor()
        );
        walk::walk_crate(visit, *crate);

        ret tup(type_cache, id_to_ty_item, ntt);
    }
}


// Type unification

mod unify {
    fn simple(&@fn_ctxt fcx, &ty::t expected,
              &ty::t actual) -> ty::unify::result {
        // FIXME: horrid botch
        let vec[mutable ty::t] param_substs =
            [mutable ty::mk_nil(fcx.ccx.tcx)];
        vec::pop(param_substs);
        ret with_params(fcx, expected, actual, param_substs);
    }

    fn with_params(&@fn_ctxt fcx, &ty::t expected, &ty::t actual,
                   &vec[mutable ty::t] param_substs) -> ty::unify::result {
        auto cache_key = tup(expected, actual, param_substs);
        alt (fcx.ccx.unify_cache.find(cache_key)) {
            case (some[ty::unify::result](?r)) {
                fcx.ccx.cache_hits += 1u;
                ret r;
            }
            case (none[ty::unify::result]) {
                fcx.ccx.cache_misses += 1u;
            }
        }

        obj unify_handler(@fn_ctxt fcx, vec[mutable ty::t] param_substs) {
            fn resolve_local(ast::def_id id) -> option::t[ty::t] {
                alt (fcx.locals.find(id)) {
                    case (none[ty::t]) { ret none[ty::t]; }
                    case (some[ty::t](?existing_type)) {
                        if (ty::type_contains_vars(fcx.ccx.tcx,
                                                  existing_type)) {
                            // Not fully resolved yet. The writeback phase
                            // will mop up.
                            ret none[ty::t];
                        }
                        ret some[ty::t](existing_type);
                    }
                }
            }
            fn record_local(ast::def_id id, ty::t new_type) {
                auto unified_type;
                alt (fcx.locals.find(id)) {
                    case (none[ty::t]) { unified_type = new_type; }
                    case (some[ty::t](?old_type)) {
                        alt (with_params(fcx, old_type, new_type,
                                         param_substs)) {
                            case (ures_ok(?ut)) { unified_type = ut; }
                            case (_) { fail; /* FIXME */ }
                        }
                    }
                }

                // TODO: "freeze"
                let vec[ty::t] param_substs_1 = [];
                for (ty::t subst in param_substs) {
                    param_substs_1 += [subst];
                }

                unified_type =
                    ty::substitute_type_params(fcx.ccx.tcx, param_substs_1,
                                              unified_type);
                fcx.locals.insert(id, unified_type);
            }
            fn record_param(uint index, ty::t binding) -> ty::unify::result {
                // Unify with the appropriate type in the parameter
                // substitution list:
                auto old_subst = param_substs.(index);

                auto result = with_params(fcx, old_subst, binding,
                                          param_substs);
                alt (result) {
                    case (ures_ok(?new_subst)) {
                        param_substs.(index) = new_subst;
                        ret ures_ok(ty::mk_bound_param(fcx.ccx.tcx, index));
                    }
                    case (_) { ret result; }
                }
            }
        }


        auto handler = unify_handler(fcx, param_substs);
        auto result = ty::unify::unify(expected, actual, handler,
                                       fcx.ccx.tcx);
        fcx.ccx.unify_cache.insert(cache_key, result);
        ret result;
    }
}


tag autoderef_kind {
    AUTODEREF_OK;
    NO_AUTODEREF;
}

fn strip_boxes(&ty::ctxt tcx, &ty::t t) -> ty::t {
    auto t1 = t;
    while (true) {
        alt (struct(tcx, t1)) {
            case (ty::ty_box(?inner)) { t1 = inner.ty; }
            case (_) { ret t1; }
        }
    }
    fail;
}

fn add_boxes(&@crate_ctxt ccx, uint n, &ty::t t) -> ty::t {
    auto t1 = t;
    while (n != 0u) {
        t1 = ty::mk_imm_box(ccx.tcx, t1);
        n -= 1u;
    }
    ret t1;
}


fn count_boxes(&ty::ctxt tcx, &ty::t t) -> uint {
    auto n = 0u;
    auto t1 = t;
    while (true) {
        alt (struct(tcx, t1)) {
            case (ty::ty_box(?inner)) { n += 1u; t1 = inner.ty; }
            case (_) { ret n; }
        }
    }
    fail;
}


// Demands - procedures that require that two types unify and emit an error
// message if they don't.

type ty_param_substs_and_ty = tup(vec[ty::t], ty::t);

mod Demand {
    fn simple(&@fn_ctxt fcx, &span sp, &ty::t expected, &ty::t actual)
        -> ty::t {
        let vec[ty::t] tps = [];
        ret full(fcx, sp, expected, actual, tps, NO_AUTODEREF)._1;
    }

    fn autoderef(&@fn_ctxt fcx, &span sp, &ty::t expected, &ty::t actual,
                 autoderef_kind adk) -> ty::t {
        let vec[ty::t] tps = [];
        ret full(fcx, sp, expected, actual, tps, adk)._1;
    }

    // Requires that the two types unify, and prints an error message if they
    // don't. Returns the unified type and the type parameter substitutions.

    fn full(&@fn_ctxt fcx, &span sp, &ty::t expected, &ty::t actual,
            &vec[ty::t] ty_param_substs_0, autoderef_kind adk)
            -> ty_param_substs_and_ty {

        auto expected_1 = expected;
        auto actual_1 = actual;
        auto implicit_boxes = 0u;

        if (adk == AUTODEREF_OK) {
            expected_1 = strip_boxes(fcx.ccx.tcx, expected_1);
            actual_1 = strip_boxes(fcx.ccx.tcx, actual_1);
            implicit_boxes = count_boxes(fcx.ccx.tcx, actual);
        }

        let vec[mutable ty::t] ty_param_substs =
            [mutable ty::mk_nil(fcx.ccx.tcx)];
        vec::pop(ty_param_substs);   // FIXME: horrid botch
        for (ty::t ty_param_subst in ty_param_substs_0) {
            ty_param_substs += [mutable ty_param_subst];
        }

        alt (unify::with_params(fcx, expected_1, actual_1, ty_param_substs)) {
            case (ures_ok(?t)) {
                // TODO: Use "freeze", when we have it.
                let vec[ty::t] result_ty_param_substs = [];
                for (ty::t ty_param_subst in ty_param_substs) {
                    result_ty_param_substs += [ty_param_subst];
                }

                ret tup(result_ty_param_substs,
                        add_boxes(fcx.ccx, implicit_boxes, t));
            }

            case (ures_err(?err, ?expected, ?actual)) {
                fcx.ccx.sess.span_err(sp, "mismatched types: expected "
                    + ty_to_str(fcx.ccx.tcx, expected) + " but found "
                    + ty_to_str(fcx.ccx.tcx, actual) + " ("
                    + ty::type_err_to_str(err) + ")");

                // TODO: In the future, try returning "expected", reporting
                // the error, and continue.
                fail;
            }
        }
    }
}


// Returns true if the two types unify and false if they don't.
fn are_compatible(&@fn_ctxt fcx, &ty::t expected, &ty::t actual) -> bool {
    alt (unify::simple(fcx, expected, actual)) {
        case (ures_ok(_))        { ret true;  }
        case (ures_err(_, _, _)) { ret false; }
    }
}

// Returns the types of the arguments to a tag variant.
fn variant_arg_types(&@crate_ctxt ccx, &span sp, &ast::def_id vid,
                     &vec[ty::t] tag_ty_params) -> vec[ty::t] {
    auto ty_param_count = vec::len[ty::t](tag_ty_params);

    let vec[ty::t] result = [];

    auto tpt = ty::lookup_item_type(ccx.sess, ccx.tcx, ccx.type_cache, vid);
    alt (struct(ccx.tcx, tpt._1)) {
        case (ty::ty_fn(_, ?ins, _)) {
            // N-ary variant.
            for (ty::arg arg in ins) {
                auto arg_ty = bind_params_in_type(ccx.tcx, arg.ty);
                arg_ty = substitute_ty_params(ccx, arg_ty, ty_param_count,
                                              tag_ty_params, sp);
                result += [arg_ty];
            }
        }
        case (_) {
            // Nullary variant. Do nothing, as there are no arguments.
        }
    }

    /* result is a vector of the *expected* types of all the fields */
    ret result;
}


// The "push-down" phase, which takes a typed grammar production and pushes
// its type down into its constituent parts.
//
// For example, consider "auto x; x = 352;". check_expr() doesn't know the
// type of "x" at the time it sees it, so that function will simply store a
// type variable for the type of "x". However, after checking the entire
// assignment expression, check_expr() will assign the type of int to the
// expression "x = 352" as a whole. In this case, then, the job of these
// functions is to clean up by assigning the type of int to both sides of the
// assignment expression.
//
// TODO: We only need to do this once per statement: check_expr() bubbles the
// types up, and pushdown_expr() pushes the types down. However, in many cases
// we're more eager than we need to be, calling pushdown_expr() and friends
// directly inside check_expr(). This results in a quadratic algorithm.

mod Pushdown {
    // Push-down over typed patterns. Note that the pattern that you pass to
    // this function must have been passed to check_pat() first.
    //
    // TODO: enforce this via a predicate.

    fn pushdown_pat(&@fn_ctxt fcx, &ty::t expected, &@ast::pat pat) {
        alt (pat.node) {
            case (ast::pat_wild(?ann)) {
                auto t = Demand::simple(fcx, pat.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                write_type(fcx.ccx.node_types, ann.id,
                           tup(none[vec[ty::t]], t));
            }
            case (ast::pat_lit(?lit, ?ann)) {
                auto t = Demand::simple(fcx, pat.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                write_type(fcx.ccx.node_types, ann.id,
                           tup(none[vec[ty::t]], t));
            }
            case (ast::pat_bind(?id, ?did, ?ann)) {
                auto t = Demand::simple(fcx, pat.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                fcx.locals.insert(did, t);
                write_type(fcx.ccx.node_types, ann.id,
                           tup(none[vec[ty::t]], t));
            }
            case (ast::pat_tag(?id, ?subpats, ?ann)) {
                // Take the variant's type parameters out of the expected
                // type.
                auto tag_tps;
                alt (struct(fcx.ccx.tcx, expected)) {
                    case (ty::ty_tag(_, ?tps)) { tag_tps = tps; }
                    case (_) {
                        log_err "tag pattern type not actually a tag?!";
                        fail;
                    }
                }

                // Get the types of the arguments of the variant.
              
                let vec[ty::t] tparams = [];
                auto j = 0u;
                auto actual_ty_params =
                  ty::ann_to_type_params(fcx.ccx.node_types, ann);

                for (ty::t some_ty in tag_tps) {
                    let ty::t t1 = some_ty;
                    let ty::t t2 = actual_ty_params.(j);
                    
                    let ty::t res = Demand::simple(fcx, 
                                                   pat.span,
                                                   t1, t2);
                    
                    vec::push(tparams, res);
                    j += 1u;
                }

                auto arg_tys;
                alt (fcx.ccx.tcx.def_map.get(ann.id)) {
                    case (ast::def_variant(_, ?vdefid)) {
                        arg_tys = variant_arg_types(fcx.ccx, pat.span, vdefid,
                                                    tparams);
                    }
                }

                auto i = 0u;
                for (@ast::pat subpat in subpats) {
                    pushdown_pat(fcx, arg_tys.(i), subpat);
                    i += 1u;
                }

                auto tps = ty::ann_to_type_params(fcx.ccx.node_types, ann);
                auto tt  = ann_to_type(fcx.ccx.node_types, ann);
                
                let ty_param_substs_and_ty res_t = Demand::full(fcx, pat.span,
                      expected, tt, tps, NO_AUTODEREF);

                auto a_1 = mk_ann_type(ann.id, res_t._1,
                                       some[vec[ty::t]](res_t._0));

                // TODO: push down type from "expected".
                write_type(fcx.ccx.node_types, ann.id,
                    ty::ann_to_ty_param_substs_opt_and_ty(fcx.ccx.node_types,
                                                          a_1));
                    
            }
        }
    }

    // Push-down over typed expressions. Note that the expression that you
    // pass to this function must have been passed to check_expr() first.
    //
    // TODO: enforce this via a predicate.
    // TODO: This function is incomplete.

    fn pushdown_expr(&@fn_ctxt fcx, &ty::t expected, &@ast::expr e) {
        be pushdown_expr_full(fcx, expected, e, NO_AUTODEREF);
    }

    fn pushdown_expr_full(&@fn_ctxt fcx, &ty::t expected, &@ast::expr e,
                          autoderef_kind adk) {
        alt (e.node) {
            case (ast::expr_vec(?es_0, ?mut, ?ann)) {
                // TODO: enforce mutability

                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                alt (struct(fcx.ccx.tcx, t)) {
                    case (ty::ty_vec(?mt)) {
                        for (@ast::expr e_0 in es_0) {
                            pushdown_expr(fcx, mt.ty, e_0);
                        }
                    }
                    case (_) {
                        log_err "vec expr doesn't have a vec type!";
                        fail;
                    }
                }
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_tup(?es_0, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                alt (struct(fcx.ccx.tcx, t)) {
                    case (ty::ty_tup(?mts)) {
                        auto i = 0u;
                        for (ast::elt elt_0 in es_0) {
                            pushdown_expr(fcx, mts.(i).ty, elt_0.expr);
                            i += 1u;
                        }
                    }
                    case (_) {
                        log_err "tup expr doesn't have a tup type!";
                        fail;
                    }
                }
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_rec(?fields_0, ?base_0, ?ann)) {

                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                alt (struct(fcx.ccx.tcx, t)) {
                    case (ty::ty_rec(?field_mts)) {
                        alt (base_0) {
                            case (none[@ast::expr]) {
                                auto i = 0u;
                                for (ast::field field_0 in fields_0) {
                                    assert (str::eq(field_0.ident,
                                                    field_mts.(i).ident));
                                    pushdown_expr(fcx,
                                                  field_mts.(i).mt.ty,
                                                  field_0.expr);
                                    i += 1u;
                                }
                            }
                            case (some[@ast::expr](?bx)) {

                                let vec[field] base_fields = [];

                                for (ast::field field_0 in fields_0) {

                                    for (ty::field ft in field_mts) {
                                        if (str::eq(field_0.ident,
                                                    ft.ident)) {
                                            pushdown_expr(fcx, ft.mt.ty,
                                                          field_0.expr);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    case (_) {
                        log_err "rec expr doesn't have a rec type!";
                        fail;
                    }
                }
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_bind(?sube, ?es, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_call(?sube, ?es, ?ann)) {
                // NB: we call 'Demand::autoderef' and pass in adk only in
                // cases where e is an expression that could *possibly*
                // produce a box; things like expr_binary or expr_bind can't,
                // so there's no need.
                auto t = Demand::autoderef(fcx, e.span, expected,
                    ann_to_type(fcx.ccx.node_types, ann), adk);
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_self_method(?id, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                       ann_to_type(fcx.ccx.node_types, ann));
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_binary(?bop, ?lhs, ?rhs, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                       ann_to_type(fcx.ccx.node_types, ann));
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_unary(?uop, ?sube, ?ann)) {
                // See note in expr_unary for why we're calling
                // Demand::autoderef.
                auto t = Demand::autoderef(fcx, e.span, expected,
                    ann_to_type(fcx.ccx.node_types, ann), adk);
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_lit(?lit, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_cast(?sube, ?ast_ty, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_if(?cond, ?then_0, ?else_0, ?ann)) {
                auto t = Demand::autoderef(fcx, e.span, expected,
                    ann_to_type(fcx.ccx.node_types, ann), adk);
                pushdown_block(fcx, expected, then_0);

                alt (else_0) {
                    case (none[@ast::expr]) { /* no-op */ }
                    case (some[@ast::expr](?e_0)) {
                        pushdown_expr(fcx, expected, e_0);
                    }
                }
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_for(?decl, ?seq, ?bloc, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_for_each(?decl, ?seq, ?bloc, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_while(?cond, ?bloc, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_do_while(?bloc, ?cond, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_block(?bloc, ?ann)) {
                auto t = Demand::autoderef(fcx, e.span, expected,
                    ann_to_type(fcx.ccx.node_types, ann), adk);
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_assign(?lhs_0, ?rhs_0, ?ann)) {
                auto t = Demand::autoderef(fcx, e.span, expected,
                    ann_to_type(fcx.ccx.node_types, ann), adk);
                pushdown_expr(fcx, expected, lhs_0);
                pushdown_expr(fcx, expected, rhs_0);
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_assign_op(?op, ?lhs_0, ?rhs_0, ?ann)) {
                auto t = Demand::autoderef(fcx, e.span, expected,
                    ann_to_type(fcx.ccx.node_types, ann), adk);
                pushdown_expr(fcx, expected, lhs_0);
                pushdown_expr(fcx, expected, rhs_0);
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_field(?lhs, ?rhs, ?ann)) {
                auto t = Demand::autoderef(fcx, e.span, expected,
                    ann_to_type(fcx.ccx.node_types, ann), adk);
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_index(?base, ?index, ?ann)) {
                auto t = Demand::autoderef(fcx, e.span, expected,
                    ann_to_type(fcx.ccx.node_types, ann), adk);
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            case (ast::expr_path(?pth, ?ann)) {
                auto tp_substs_0 = ty::ann_to_type_params(fcx.ccx.node_types,
                                                          ann);
                auto t_0 = ann_to_type(fcx.ccx.node_types, ann);

                auto result_0 = Demand::full(fcx, e.span, expected, t_0,
                                             tp_substs_0, adk);
                auto t = result_0._1;

                // Fill in the type parameter substitutions if they weren't
                // provided by the programmer.
                auto ty_params_opt;
                alt (ty::ann_to_ty_param_substs_opt_and_ty(fcx.ccx.node_types,
                                                           ann)._0) {
                    case (none[vec[ty::t]]) {
                        ty_params_opt = none[vec[ty::t]];
                    }
                    case (some[vec[ty::t]](?tps)) {
                        ty_params_opt = some[vec[ty::t]](tps);
                    }
                }

                write_type(fcx.ccx.node_types, ann.id, tup(ty_params_opt, t));
            }
            case (ast::expr_ext(?p, ?args, ?body, ?expanded, ?ann)) {
                auto t = Demand::autoderef(fcx, e.span, expected,
                    ann_to_type(fcx.ccx.node_types, ann), adk);
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }
            /* FIXME: should this check the type annotations? */
            case (ast::expr_fail(_))  { /* no-op */ }
            case (ast::expr_log(_,_,_)) { /* no-op */ }
            case (ast::expr_break(_)) { /* no-op */ }
            case (ast::expr_cont(_))  { /* no-op */ }
            case (ast::expr_ret(_,_)) { /* no-op */ }
            case (ast::expr_put(_,_)) { /* no-op */ }
            case (ast::expr_be(_,_))  { /* no-op */ }
            case (ast::expr_check(_,_)) { /* no-op */ }
            case (ast::expr_assert(_,_)) { /* no-op */ }

            case (ast::expr_port(?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }

            case (ast::expr_chan(?es, ?ann)) {
                auto t = Demand::simple(fcx, e.span, expected,
                                        ann_to_type(fcx.ccx.node_types, ann));
                alt (struct(fcx.ccx.tcx, t)) {
                    case (ty::ty_chan(?subty)) {
                        auto pt = ty::mk_port(fcx.ccx.tcx, subty);
                        pushdown_expr(fcx, pt, es);
                    }
                    case (_) {
                        log "chan expr doesn't have a chan type!";
                        fail;
                    }
                }
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }

            case (ast::expr_alt(?discrim, ?arms_0, ?ann)) {
                auto t = expected;
                for (ast::arm arm_0 in arms_0) {
                    pushdown_block(fcx, expected, arm_0.block);
                    auto bty = block_ty(fcx.ccx.tcx, fcx.ccx.node_types,
                                        arm_0.block);
                    t = Demand::simple(fcx, e.span, t, bty);
                }
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }

            case (ast::expr_recv(?lval, ?expr, ?ann)) {
                pushdown_expr(fcx, next_ty_var(fcx.ccx), lval);
                auto t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, lval);
                pushdown_expr(fcx, ty::mk_port(fcx.ccx.tcx, t), expr);
            }

            case (ast::expr_send(?lval, ?expr, ?ann)) {
                pushdown_expr(fcx, next_ty_var(fcx.ccx), expr);
                auto t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, expr);
                pushdown_expr(fcx, ty::mk_chan(fcx.ccx.tcx, t), lval);
            }

            case (ast::expr_spawn(?dom, ?name, ?func, ?args, ?ann)) {
                // NB: we call 'Demand::autoderef' and pass in adk only in
                // cases where e is an expression that could *possibly*
                // produce a box; things like expr_binary or expr_bind can't,
                // so there's no need.
                auto t = Demand::autoderef(fcx, e.span, expected,
                    ann_to_type(fcx.ccx.node_types, ann), adk);
                write_type_only(fcx.ccx.node_types, ann.id, t);
            }

            case (_) {
                fcx.ccx.sess.span_unimpl(e.span,
                    #fmt("type unification for expression variant: %s",
                         util::common::expr_to_str(e)));
                fail;
            }
        }
    }

    // Push-down over typed blocks.
    fn pushdown_block(&@fn_ctxt fcx, &ty::t expected, &ast::block bloc) {
        alt (bloc.node.expr) {
            case (some[@ast::expr](?e_0)) {
                pushdown_expr(fcx, expected, e_0);
                write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types,
                               bloc.node.a.id);
            }
            case (none[@ast::expr]) {
                Demand::simple(fcx, bloc.span, expected,
                               ty::mk_nil(fcx.ccx.tcx));
                write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types,
                               bloc.node.a.id);
            }
        }
    }
}


// Local variable resolution: the phase that finds all the types in the AST
// and replaces opaque "ty_local" types with the resolved local types.

mod writeback {
    fn wb_local(&@fn_ctxt fcx, &span sp, &@ast::local local) {
        auto local_ty;
        alt (fcx.locals.find(local.id)) {
            case (none[ty::t]) {
                fcx.ccx.sess.span_err(sp,
                    "unable to determine type of local: " + local.ident);
                fail;
            }
            case (some[ty::t](?lt)) {
                local_ty = lt;
            }
        }

        write_type_only(fcx.ccx.node_types, local.ann.id, local_ty);
    }

    fn resolve_local_types(&@fn_ctxt fcx, &ast::ann ann) {
        fn resolver(@fn_ctxt fcx, ty::t typ) -> ty::t {
            alt (struct(fcx.ccx.tcx, typ)) {
                case (ty::ty_local(?lid))   { ret fcx.locals.get(lid); }
                case (_)                    { ret typ; }
            }
        }

        auto tpot = ty::ann_to_ty_param_substs_opt_and_ty(fcx.ccx.node_types,
                                                          ann);
        auto tt = tpot._1;
        if (!ty::type_contains_locals(fcx.ccx.tcx, tt)) { ret; }

        auto f = bind resolver(fcx, _);
        auto new_type = ty::fold_ty(fcx.ccx.tcx, f, tt);
        write_type(fcx.ccx.node_types, ann.id, tup(tpot._0, new_type));
    }

    fn visit_stmt_pre(@fn_ctxt fcx, &@ast::stmt s) {
        resolve_local_types(fcx, ty::stmt_ann(s));
    }

    fn visit_expr_pre(@fn_ctxt fcx, &@ast::expr e) {
        resolve_local_types(fcx, ty::expr_ann(e));
    }

    fn visit_block_pre(@fn_ctxt fcx, &ast::block b) {
        resolve_local_types(fcx, b.node.a);
    }

    fn visit_arm_pre(@fn_ctxt fcx, &ast::arm a) {
        // FIXME: Need a visit_pat_pre
        resolve_local_types(fcx, ty::pat_ann(a.pat));
    }

    fn visit_decl_pre(@fn_ctxt fcx, &@ast::decl d) {
        alt (d.node) {
            case (ast::decl_local(?l)) { wb_local(fcx, d.span, l); }
            case (ast::decl_item(_)) { /* no annotation */ }
        }
    }

    fn resolve_local_types_in_block(&@fn_ctxt fcx, &ast::block block) {
        // A trick to ignore any contained items.
        auto ignore = @mutable false;
        fn visit_item_pre(@mutable bool ignore, &@ast::item item) {
            *ignore = true;
        }
        fn visit_item_post(@mutable bool ignore, &@ast::item item) {
            *ignore = false;
        }
        fn keep_going(@mutable bool ignore) -> bool { ret !*ignore; }

        auto fld = fold::new_identity_fold[option::t[@fn_ctxt]]();
        auto visit = rec(keep_going=bind keep_going(ignore),
                         visit_item_pre=bind visit_item_pre(ignore, _),
                         visit_item_post=bind visit_item_post(ignore, _),
                         visit_stmt_pre=bind visit_stmt_pre(fcx, _),
                         visit_expr_pre=bind visit_expr_pre(fcx, _),
                         visit_block_pre=bind visit_block_pre(fcx, _),
                         visit_arm_pre=bind visit_arm_pre(fcx, _),
                         visit_decl_pre=bind visit_decl_pre(fcx, _)
                         with walk::default_visitor());
        walk::walk_block(visit, block);
    }
}


// AST fragment utilities

fn replace_expr_type(&node_type_table ntt,
                     &@ast::expr expr,
                     &tup(vec[ty::t], ty::t) new_tyt) {
    auto new_tps;
    if (ty::expr_has_ty_params(ntt, expr)) {
        new_tps = some[vec[ty::t]](new_tyt._0);
    } else {
        new_tps = none[vec[ty::t]];
    }

    write_type(ntt, ty::expr_ann(expr).id, tup(new_tps, new_tyt._1));
}


// AST fragment checking

fn check_lit(@crate_ctxt ccx, &@ast::lit lit) -> ty::t {
    alt (lit.node) {
        case (ast::lit_str(_))              { ret ty::mk_str(ccx.tcx); }
        case (ast::lit_char(_))             { ret ty::mk_char(ccx.tcx); }
        case (ast::lit_int(_))              { ret ty::mk_int(ccx.tcx);  }
        case (ast::lit_float(_))            { ret ty::mk_float(ccx.tcx);  }
        case (ast::lit_mach_float(?tm, _))  { ret ty::mk_mach(ccx.tcx, tm); }
        case (ast::lit_uint(_))             { ret ty::mk_uint(ccx.tcx); }
        case (ast::lit_mach_int(?tm, _))    { ret ty::mk_mach(ccx.tcx, tm); }
        case (ast::lit_nil)                 { ret ty::mk_nil(ccx.tcx);  }
        case (ast::lit_bool(_))             { ret ty::mk_bool(ccx.tcx); }
    }

    fail; // not reached
}

fn check_pat(&@fn_ctxt fcx, &@ast::pat pat) {
    alt (pat.node) {
        case (ast::pat_wild(?ann)) {
            auto typ = next_ty_var(fcx.ccx);
            write_type_only(fcx.ccx.node_types, ann.id, typ);
        }
        case (ast::pat_lit(?lt, ?ann)) {
            auto typ = check_lit(fcx.ccx, lt);
            write_type_only(fcx.ccx.node_types, ann.id, typ);
        }
        case (ast::pat_bind(?id, ?def_id, ?a)) {
            auto typ = next_ty_var(fcx.ccx);
            auto ann = triv_ann(a.id, typ);
            write_type_only(fcx.ccx.node_types, ann.id, typ);
        }
        case (ast::pat_tag(?p, ?subpats, ?old_ann)) {
            auto vdef = ast::variant_def_ids
                (fcx.ccx.tcx.def_map.get(old_ann.id));
            auto t = ty::lookup_item_type(fcx.ccx.sess, fcx.ccx.tcx,
                                          fcx.ccx.type_cache, vdef._1)._1;
            auto len = vec::len[ast::ident](p.node.idents);
            auto last_id = p.node.idents.(len - 1u);

            auto tpt = ty::lookup_item_type(fcx.ccx.sess, fcx.ccx.tcx,
                                           fcx.ccx.type_cache, vdef._0);

            auto path_tpot = instantiate_path(fcx, p, tpt, pat.span);

            alt (struct(fcx.ccx.tcx, t)) {
                // N-ary variants have function types.
                case (ty::ty_fn(_, ?args, ?tag_ty)) {
                    auto arg_len = vec::len[arg](args);
                    auto subpats_len = vec::len[@ast::pat](subpats);
                    if (arg_len != subpats_len) {
                        // TODO: pluralize properly
                        auto err_msg = "tag type " + last_id + " has " +
                                       uint::to_str(subpats_len, 10u) +
                                       " field(s), but this pattern has " +
                                       uint::to_str(arg_len, 10u) +
                                       " field(s)";

                        fcx.ccx.sess.span_err(pat.span, err_msg);
                        fail;   // TODO: recover
                    }

                    for (@ast::pat subpat in subpats) {
                        check_pat(fcx, subpat);
                    }

                    write_type(fcx.ccx.node_types, old_ann.id, path_tpot);
                }

                // Nullary variants have tag types.
                case (ty::ty_tag(?tid, _)) {
                    auto subpats_len = vec::len[@ast::pat](subpats);
                    if (subpats_len > 0u) {
                        // TODO: pluralize properly
                        auto err_msg = "tag type " + last_id +
                                       " has no field(s)," +
                                       " but this pattern has " +
                                       uint::to_str(subpats_len, 10u) +
                                       " field(s)";

                        fcx.ccx.sess.span_err(pat.span, err_msg);
                        fail;   // TODO: recover
                    }

                    write_type(fcx.ccx.node_types, old_ann.id, path_tpot);
                }
            }
        }
    }
}

fn require_impure(&session::session sess,
                  &ast::purity f_purity, &span sp) -> () {
    alt (f_purity) {
        case (ast::impure_fn) {
            ret;
        }
        case (ast::pure_fn) {
            sess.span_err(sp,
               "Found impure expression in pure function decl");
        }
    }
}

fn get_function_purity(@crate_ctxt ccx, &ast::def_id d_id) -> ast::purity {
    let option::t[ast::purity] o = ccx.fn_purity_table.find(d_id);
    ret from_maybe[ast::purity](ast::impure_fn, o);
}

fn require_pure_call(@crate_ctxt ccx,
                     &ast::purity caller_purity,
                     &@ast::expr callee, &span sp) -> () {
    alt (caller_purity) {
        case (ast::impure_fn) {
            ret;
        }
        case (ast::pure_fn) {
            alt (callee.node) {
                case (ast::expr_path(_, ?ann)) {
                    auto d_id;
                    alt (ccx.tcx.def_map.get(ann.id)) {
                        case (ast::def_fn(?_d_id)) { d_id = _d_id; }
                    }
                    alt (get_function_purity(ccx, d_id)) {
                            case (ast::pure_fn) {
                                ret;
                            }
                            case (_) {
                                ccx.sess.span_err(sp,
                                  "Pure function calls impure function");

                            }
                        }
                }
                case (_) {
                    ccx.sess.span_err(sp,
                      "Pure function calls unknown function");
                }
            }
        }
    }
}

fn require_pure_function(@crate_ctxt ccx, &ast::def_id d_id, &span sp) -> () {
    alt (get_function_purity(ccx, d_id)) {
        case (ast::impure_fn) {
            ccx.sess.span_err(sp, "Found non-predicate in check expression");
        }
        case (_) { ret; }
    }
}

fn check_expr(&@fn_ctxt fcx, &@ast::expr expr) {
    //fcx.ccx.sess.span_warn(expr.span, "typechecking expr " +
    //                       util::common::expr_to_str(expr));

    // A generic function to factor out common logic from call and bind
    // expressions.
    fn check_call_or_bind(&@fn_ctxt fcx, &@ast::expr f,
                          &vec[option::t[@ast::expr]] args) {

        // Check the function.
        check_expr(fcx, f);

        // Check the arguments and generate the argument signature.
        let vec[option::t[@ast::expr]] args_0 = [];
        let vec[arg] arg_tys_0 = [];
        for (option::t[@ast::expr] a_opt in args) {
            alt (a_opt) {
                case (some[@ast::expr](?a)) {
                    check_expr(fcx, a);
                    auto typ = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, a);
                    vec::push[arg](arg_tys_0, rec(mode=mo_either, ty=typ));
                }
                case (none[@ast::expr]) {
                    auto typ = next_ty_var(fcx.ccx);
                    vec::push[arg](arg_tys_0, rec(mode=mo_either, ty=typ));
                }
            }
        }

        auto rt_0 = next_ty_var(fcx.ccx);
        auto t_0;
        alt (struct(fcx.ccx.tcx, expr_ty(fcx.ccx.tcx, fcx.ccx.node_types,
                                         f))) {
            case (ty::ty_fn(?proto, _, _))   {
                t_0 = ty::mk_fn(fcx.ccx.tcx, proto, arg_tys_0, rt_0);
            }
            case (ty::ty_native_fn(?abi, _, _))   {
                t_0 = ty::mk_native_fn(fcx.ccx.tcx, abi, arg_tys_0, rt_0);
            }
            case (?u) {
                fcx.ccx.sess.span_err(f.span,
                "check_call_or_bind(): fn expr doesn't have fn type,"
                + " instead having: " +
                ty_to_str(fcx.ccx.tcx, expr_ty(fcx.ccx.tcx,
                                               fcx.ccx.node_types, f)));
                fail;
            }
        }

        // Unify the callee and arguments.
        auto tpt_0 = ty::expr_ty_params_and_ty(fcx.ccx.tcx,
                                               fcx.ccx.node_types, f);
        auto tpt_1 = Demand::full(fcx, f.span, tpt_0._1, t_0, tpt_0._0,
                                 NO_AUTODEREF);
        replace_expr_type(fcx.ccx.node_types, f, tpt_1);
    }

    // A generic function for checking assignment expressions
    fn check_assignment(&@fn_ctxt fcx, &@ast::expr lhs, &@ast::expr rhs,
                        &ast::ann a) {
        check_expr(fcx, lhs);
        check_expr(fcx, rhs);
        auto lhs_t0 = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, lhs);
        auto rhs_t0 = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, rhs);

        Pushdown::pushdown_expr(fcx, rhs_t0, lhs);
        auto lhs_t1 = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, lhs);
        Pushdown::pushdown_expr(fcx, lhs_t1, rhs);
        auto rhs_t1 = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, rhs);

        auto ann = triv_ann(a.id, rhs_t1);
        write_type_only(fcx.ccx.node_types, a.id, rhs_t1);
    }

    // A generic function for checking call expressions
    fn check_call(&@fn_ctxt fcx, &@ast::expr f, &vec[@ast::expr] args) {
        let vec[option::t[@ast::expr]] args_opt_0 = [];
        for (@ast::expr arg in args) {
            args_opt_0 += [some[@ast::expr](arg)];
        }

        // Call the generic checker.
        check_call_or_bind(fcx, f, args_opt_0);
    }

    alt (expr.node) {
        case (ast::expr_lit(?lit, ?a)) {
            auto typ = check_lit(fcx.ccx, lit);
            write_type_only(fcx.ccx.node_types, a.id, typ);
        }

        case (ast::expr_binary(?binop, ?lhs, ?rhs, ?a)) {
            check_expr(fcx, lhs);
            check_expr(fcx, rhs);
            auto lhs_t0 = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, lhs);
            auto rhs_t0 = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, rhs);

            // FIXME: Binops have a bit more subtlety than this.
            Pushdown::pushdown_expr_full(fcx, rhs_t0, lhs, AUTODEREF_OK);
            auto lhs_t1 = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, lhs);
            Pushdown::pushdown_expr_full(fcx, lhs_t1, rhs, AUTODEREF_OK);

            auto t = strip_boxes(fcx.ccx.tcx, lhs_t0);
            alt (binop) {
                case (ast::eq) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (ast::lt) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (ast::le) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (ast::ne) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (ast::ge) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (ast::gt) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (_) { /* fall through */ }
            }

            write_type_only(fcx.ccx.node_types, a.id, t);
        }

        case (ast::expr_unary(?unop, ?oper, ?a)) {
            check_expr(fcx, oper);

            auto oper_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, oper);
            alt (unop) {
                case (ast::box(?mut)) {
                    oper_t = ty::mk_box(fcx.ccx.tcx, rec(ty=oper_t, mut=mut));
                }
                case (ast::deref) {
                    alt (struct(fcx.ccx.tcx, oper_t)) {
                        case (ty::ty_box(?inner)) { oper_t = inner.ty; }
                        case (_) {
                            fcx.ccx.sess.span_err
                                (expr.span,
                                 "dereferencing non-box type: "
                                 + ty_to_str(fcx.ccx.tcx, oper_t));
                        }
                    }
                }
                case (_) { oper_t = strip_boxes(fcx.ccx.tcx, oper_t); }
            }

            write_type_only(fcx.ccx.node_types, a.id, oper_t);
        }

        case (ast::expr_path(?pth, ?old_ann)) {
            auto t = ty::mk_nil(fcx.ccx.tcx);
            auto defn = fcx.ccx.tcx.def_map.get(old_ann.id);

            auto tpt = ty_param_count_and_ty_for_def(fcx, expr.span, defn);

            if (ty::def_has_ty_params(defn)) {
                auto path_tpot = instantiate_path(fcx, pth, tpt, expr.span);
                write_type(fcx.ccx.node_types, old_ann.id, path_tpot);
                ret;
            }

            // The definition doesn't take type parameters. If the programmer
            // supplied some, that's an error.
            if (vec::len[@ast::ty](pth.node.types) > 0u) {
                fcx.ccx.sess.span_err(expr.span, "this kind of value does " +
                                      "not take type parameters");
                fail;
            }

            write_type_only(fcx.ccx.node_types, old_ann.id, tpt._1);
        }

        case (ast::expr_ext(?p, ?args, ?body, ?expanded, ?a)) {
            check_expr(fcx, expanded);
            auto t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, expanded);
            write_type_only(fcx.ccx.node_types, a.id, t);
        }

        case (ast::expr_fail(?a)) {
            write_bot_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);
        }

        case (ast::expr_break(?a)) {
            write_bot_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);
        }

        case (ast::expr_cont(?a)) {
            write_bot_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);
        }

        case (ast::expr_ret(?expr_opt, ?a)) {
            alt (expr_opt) {
                case (none[@ast::expr]) {
                    auto nil = ty::mk_nil(fcx.ccx.tcx);
                    if (!are_compatible(fcx, fcx.ret_ty, nil)) {
                        // TODO: span_err
                        fcx.ccx.sess.err("ret; in function "
                                         + "returning non-nil");
                    }

                    write_bot_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);
                }

                case (some[@ast::expr](?e)) {
                    check_expr(fcx, e);
                    Pushdown::pushdown_expr(fcx, fcx.ret_ty, e);

                    write_bot_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);
                }
            }
        }

        case (ast::expr_put(?expr_opt, ?a)) {
            require_impure(fcx.ccx.sess, fcx.purity, expr.span);

            alt (expr_opt) {
                case (none[@ast::expr]) {
                    auto nil = ty::mk_nil(fcx.ccx.tcx);
                    if (!are_compatible(fcx, fcx.ret_ty, nil)) {
                        // TODO: span_err
                        fcx.ccx.sess.err("put; in function "
                                         + "putting non-nil");
                    }

                    write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);
                }

                case (some[@ast::expr](?e)) {
                    check_expr(fcx, e);
                    Pushdown::pushdown_expr(fcx, fcx.ret_ty, e);

                    write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);
                }
            }
        }

        case (ast::expr_be(?e, ?a)) {
            // FIXME: prove instead of assert
            assert (ast::is_call_expr(e));

            check_expr(fcx, e);
            Pushdown::pushdown_expr(fcx, fcx.ret_ty, e);

            write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);
        }

        case (ast::expr_log(?l, ?e, ?a)) {
            auto expr_t = check_expr(fcx, e);
            write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);
        }

        case (ast::expr_check(?e, ?a)) {
            check_expr(fcx, e);
            Demand::simple(fcx, expr.span, ty::mk_bool(fcx.ccx.tcx),
                           expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, e));
            /* e must be a call expr where all arguments are either
             literals or slots */
            alt (e.node) {
                case (ast::expr_call(?operator, ?operands, _)) {
                    alt (operator.node) {
                        case (ast::expr_path(?oper_name, ?ann)) {
                            auto d_id;
                            alt (fcx.ccx.tcx.def_map.get(ann.id)) {
                                case (ast::def_fn(?_d_id)) { d_id = _d_id; }
                            }
                            for (@ast::expr operand in operands) {
                                if (! ast::is_constraint_arg(operand)) {
                                    fcx.ccx.sess.span_err(expr.span,
                                       "Constraint args must be "
                                     + "slot variables or literals");
                                }
                            }

                            require_pure_function(fcx.ccx, d_id, expr.span);

                            write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types,
                                           a.id);
                        }
                        case (_) {
                           fcx.ccx.sess.span_err(expr.span,
                             "In a constraint, expected the constraint name "
                           + "to be an explicit name");
                        }
                    }
                }
                case (_) {
                    fcx.ccx.sess.span_err(expr.span,
                        "Check on non-predicate");
                }
            }
        }

        case (ast::expr_assert(?e, ?a)) {
            check_expr(fcx, e);
            auto ety = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, e);
            Demand::simple(fcx, expr.span, ty::mk_bool(fcx.ccx.tcx), ety);

            write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);
        }

        case (ast::expr_assign(?lhs, ?rhs, ?a)) {
            require_impure(fcx.ccx.sess, fcx.purity, expr.span);
            check_assignment(fcx, lhs, rhs, a);
        }

        case (ast::expr_assign_op(?op, ?lhs, ?rhs, ?a)) {
            require_impure(fcx.ccx.sess, fcx.purity, expr.span);
            check_assignment(fcx, lhs, rhs, a);
        }

        case (ast::expr_send(?lhs, ?rhs, ?a)) {
            require_impure(fcx.ccx.sess, fcx.purity, expr.span);

            check_expr(fcx, lhs);
            check_expr(fcx, rhs);
            auto rhs_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, rhs);

            auto chan_t = ty::mk_chan(fcx.ccx.tcx, rhs_t);
            Pushdown::pushdown_expr(fcx, chan_t, lhs);
            auto item_t;
            auto lhs_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, lhs);
            alt (struct(fcx.ccx.tcx, lhs_t)) {
                case (ty::ty_chan(?it)) { item_t = it; }
                case (_) { fail; }
            }
            Pushdown::pushdown_expr(fcx, item_t, rhs);

            write_type_only(fcx.ccx.node_types, a.id, chan_t);
        }

        case (ast::expr_recv(?lhs, ?rhs, ?a)) {
            require_impure(fcx.ccx.sess, fcx.purity, expr.span);

            check_expr(fcx, lhs);
            check_expr(fcx, rhs);
            auto lhs_t1 = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, lhs);

            auto port_t = ty::mk_port(fcx.ccx.tcx, lhs_t1);
            Pushdown::pushdown_expr(fcx, port_t, rhs);
            auto item_t;
            auto rhs_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, rhs);
            alt (struct(fcx.ccx.tcx, rhs_t)) {
                case (ty::ty_port(?it)) { item_t = it; }
                case (_) { fail; }
            }
            Pushdown::pushdown_expr(fcx, item_t, lhs);

            write_type_only(fcx.ccx.node_types, a.id, item_t);
        }

        case (ast::expr_if(?cond, ?thn, ?elsopt, ?a)) {
            check_expr(fcx, cond);
            Pushdown::pushdown_expr(fcx, ty::mk_bool(fcx.ccx.tcx), cond);

            check_block(fcx, thn);
            auto thn_t = block_ty(fcx.ccx.tcx, fcx.ccx.node_types, thn);

            auto elsopt_t;
            alt (elsopt) {
                case (some[@ast::expr](?els)) {
                    check_expr(fcx, els);
                    Pushdown::pushdown_expr(fcx, thn_t, els);
                    elsopt_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, els);
                }
                case (none[@ast::expr]) {
                    elsopt_t = ty::mk_nil(fcx.ccx.tcx);
                }
            }

            Pushdown::pushdown_block(fcx, elsopt_t, thn);

            write_type_only(fcx.ccx.node_types, a.id, elsopt_t);
        }

        case (ast::expr_for(?decl, ?seq, ?body, ?a)) {
            check_decl_local(fcx, decl);
            check_expr(fcx, seq);
            check_block(fcx, body);

            // FIXME: enforce that the type of the decl is the element type
            // of the seq.

            auto typ = ty::mk_nil(fcx.ccx.tcx);
            write_type_only(fcx.ccx.node_types, a.id, typ);
        }

        case (ast::expr_for_each(?decl, ?seq, ?body, ?a)) {
            check_decl_local(fcx, decl);
            check_expr(fcx, seq);
            check_block(fcx, body);

            auto typ = ty::mk_nil(fcx.ccx.tcx);
            write_type_only(fcx.ccx.node_types, a.id, typ);
        }

        case (ast::expr_while(?cond, ?body, ?a)) {
            check_expr(fcx, cond);
            Pushdown::pushdown_expr(fcx, ty::mk_bool(fcx.ccx.tcx), cond);
            check_block(fcx, body);

            auto typ = ty::mk_nil(fcx.ccx.tcx);
            write_type_only(fcx.ccx.node_types, a.id, typ);
        }

        case (ast::expr_do_while(?body, ?cond, ?a)) {
            check_expr(fcx, cond);
            Pushdown::pushdown_expr(fcx, ty::mk_bool(fcx.ccx.tcx), cond);
            check_block(fcx, body);

            auto typ = block_ty(fcx.ccx.tcx, fcx.ccx.node_types, body);
            write_type_only(fcx.ccx.node_types, a.id, typ);
        }

        case (ast::expr_alt(?expr, ?arms, ?a)) {
            check_expr(fcx, expr);

            // Typecheck the patterns first, so that we get types for all the
            // bindings.
            auto pattern_ty = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, expr);

            let vec[@ast::pat] pats = [];
            for (ast::arm arm in arms) {
                check_pat(fcx, arm.pat);
                pattern_ty = Demand::simple(fcx, arm.pat.span, pattern_ty,
                    pat_ty(fcx.ccx.tcx, fcx.ccx.node_types, arm.pat));
                pats += [arm.pat];
            }

            for (@ast::pat pat in pats) {
                Pushdown::pushdown_pat(fcx, pattern_ty, pat);
            }

            // Now typecheck the blocks.
            auto result_ty = next_ty_var(fcx.ccx);

            let vec[ast::block] blocks = [];
            for (ast::arm arm in arms) {
                check_block(fcx, arm.block);

                auto bty = block_ty(fcx.ccx.tcx, fcx.ccx.node_types,
                                    arm.block);
                result_ty = Demand::simple(fcx, arm.block.span, result_ty,
                                           bty);
            }

            auto i = 0u;
            for (ast::block bloc in blocks) {
                Pushdown::pushdown_block(fcx, result_ty, bloc);
            }

            Pushdown::pushdown_expr(fcx, pattern_ty, expr);

            write_type_only(fcx.ccx.node_types, a.id, result_ty);
        }

        case (ast::expr_block(?b, ?a)) {
            check_block(fcx, b);
            alt (b.node.expr) {
                case (some[@ast::expr](?expr)) {
                    auto typ = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, expr);
                    write_type_only(fcx.ccx.node_types, a.id, typ);
                }
                case (none[@ast::expr]) {
                    auto typ = ty::mk_nil(fcx.ccx.tcx);
                    write_type_only(fcx.ccx.node_types, a.id, typ);
                }
            }
        }

        case (ast::expr_bind(?f, ?args, ?a)) {
            // Call the generic checker.
            check_call_or_bind(fcx, f, args);

            // Pull the argument and return types out.
            auto proto_1;
            let vec[ty::arg] arg_tys_1 = [];
            auto rt_1;
            auto fty = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, f);
            alt (struct(fcx.ccx.tcx, fty)) {
                case (ty::ty_fn(?proto, ?arg_tys, ?rt)) {
                    proto_1 = proto;
                    rt_1 = rt;

                    // For each blank argument, add the type of that argument
                    // to the resulting function type.
                    auto i = 0u;
                    while (i < vec::len[option::t[@ast::expr]](args)) {
                        alt (args.(i)) {
                            case (some[@ast::expr](_)) { /* no-op */ }
                            case (none[@ast::expr]) {
                                arg_tys_1 += [arg_tys.(i)];
                            }
                        }
                        i += 1u;
                    }
                }
                case (_) {
                    log_err "LHS of bind expr didn't have a function type?!";
                    fail;
                }
            }

            auto t_1 = ty::mk_fn(fcx.ccx.tcx, proto_1, arg_tys_1, rt_1);
            write_type_only(fcx.ccx.node_types, a.id, t_1);
        }

        case (ast::expr_call(?f, ?args, ?a)) {
            /* here we're kind of hosed, as f can be any expr
             need to restrict it to being an explicit expr_path if we're
            inside a pure function, and need an environment mapping from
            function name onto purity-designation */
            require_pure_call(fcx.ccx, fcx.purity, f, expr.span);

            check_call(fcx, f, args);

            // Pull the return type out of the type of the function.
            auto rt_1 = ty::mk_nil(fcx.ccx.tcx);  // FIXME: typestate botch
            auto fty = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, f);
            alt (struct(fcx.ccx.tcx, fty)) {
                case (ty::ty_fn(_,_,?rt))           { rt_1 = rt; }
                case (ty::ty_native_fn(_, _, ?rt))  { rt_1 = rt; }
                case (_) {
                    log_err "LHS of call expr didn't have a function type?!";
                    fail;
                }
            }

            write_type_only(fcx.ccx.node_types, a.id, rt_1);
        }

        case (ast::expr_self_method(?id, ?a)) {
            auto t = ty::mk_nil(fcx.ccx.tcx);
            let ty::t this_obj_ty;

            auto this_obj_id = fcx.ccx.this_obj;
            alt (this_obj_id) {
                // If we're inside a current object, grab its type.
                case (some[ast::def_id](?def_id)) {
                    this_obj_ty = ty::lookup_item_type(fcx.ccx.sess,
                        fcx.ccx.tcx, fcx.ccx.type_cache, def_id)._1;
                }
                // Otherwise, we should be able to look up the object we're
                // "with".
                case (_) {
                    // TODO.

                    fail;
                }
            }

            // Grab this method's type out of the current object type.
            alt (struct(fcx.ccx.tcx, this_obj_ty)) {
                case (ty::ty_obj(?methods)) {
                    for (ty::method method in methods) {
                        if (method.ident == id) {
                            t = ty::method_ty_to_fn_ty(fcx.ccx.tcx, method);
                        }
                    }
                }
                case (_) { fail; }
            }

            write_type_only(fcx.ccx.node_types, a.id, t);

            require_impure(fcx.ccx.sess, fcx.purity, expr.span);
        }

        case (ast::expr_spawn(_, _, ?f, ?args, ?a)) {
            check_call(fcx, f, args);

            // Check the return type
            auto fty = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, f);
            alt (struct(fcx.ccx.tcx, fty)) {
                case (ty::ty_fn(_,_,?rt)) {
                    alt (struct(fcx.ccx.tcx, rt)) {
                        case (ty::ty_nil) {
                            // This is acceptable
                        }
                        case (_) {
                            auto err = "non-nil return type in "
                                + "spawned function";
                            fcx.ccx.sess.span_err(expr.span, err);
                            fail;
                        }
                    }
                }
            }

            // FIXME: Other typechecks needed

            auto typ = ty::mk_task(fcx.ccx.tcx);
            write_type_only(fcx.ccx.node_types, a.id, typ);
        }

        case (ast::expr_cast(?e, ?t, ?a)) {
            check_expr(fcx, e);
            auto t_1 = ast_ty_to_ty_crate(fcx.ccx, t);
            // FIXME: there are more forms of cast to support, eventually.
            if (! (type_is_scalar(fcx.ccx.tcx,
                    expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, e)) &&
                    type_is_scalar(fcx.ccx.tcx, t_1))) {
                fcx.ccx.sess.span_err(expr.span,
                    "non-scalar cast: " +
                    ty_to_str(fcx.ccx.tcx,
                              expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, e)) +
                    " as " + ty_to_str(fcx.ccx.tcx, t_1));
            }

            write_type_only(fcx.ccx.node_types, a.id, t_1);
        }

        case (ast::expr_vec(?args, ?mut, ?a)) {
            let ty::t t;
            if (vec::len[@ast::expr](args) == 0u) {
                t = next_ty_var(fcx.ccx);
            } else {
                check_expr(fcx, args.(0));
                t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, args.(0));
            }

            for (@ast::expr e in args) {
                check_expr(fcx, e);
                auto expr_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, e);
                Demand::simple(fcx, expr.span, t, expr_t);
            }

            auto typ = ty::mk_vec(fcx.ccx.tcx, rec(ty=t, mut=mut));
            write_type_only(fcx.ccx.node_types, a.id, typ);
        }

        case (ast::expr_tup(?elts, ?a)) {
            let vec[ty::mt] elts_mt = [];

            for (ast::elt e in elts) {
                check_expr(fcx, e.expr);
                auto ety = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, e.expr);
                elts_mt += [rec(ty=ety, mut=e.mut)];
            }

            auto typ = ty::mk_tup(fcx.ccx.tcx, elts_mt);
            write_type_only(fcx.ccx.node_types, a.id, typ);
        }

        case (ast::expr_rec(?fields, ?base, ?a)) {

            alt (base) {
                case (none[@ast::expr]) { /* no-op */}
                case (some[@ast::expr](?b_0)) { check_expr(fcx, b_0); }
            }

            let vec[field] fields_t = [];

            for (ast::field f in fields) {
                check_expr(fcx, f.expr);
                auto expr_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types,
                                      f.expr);

                auto expr_mt = rec(ty=expr_t, mut=f.mut);
                vec::push[field](fields_t, rec(ident=f.ident, mt=expr_mt));
            }

            alt (base) {
                case (none[@ast::expr]) {
                    auto typ = ty::mk_rec(fcx.ccx.tcx, fields_t);
                    write_type_only(fcx.ccx.node_types, a.id, typ);
                }

                case (some[@ast::expr](?bexpr)) {
                    check_expr(fcx, bexpr);
                    auto bexpr_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types,
                                           bexpr);

                    let vec[field] base_fields = [];

                    alt (struct(fcx.ccx.tcx, bexpr_t)) {
                        case (ty::ty_rec(?flds)) { base_fields = flds; }
                        case (_) {
                            fcx.ccx.sess.span_err
                                (expr.span,
                                 "record update non-record base");
                        }
                    }

                    write_type_only(fcx.ccx.node_types, a.id, bexpr_t);

                    for (ty::field f in fields_t) {
                        auto found = false;
                        for (ty::field bf in base_fields) {
                            if (str::eq(f.ident, bf.ident)) {
                                Demand::simple(fcx, expr.span, f.mt.ty,
                                              bf.mt.ty);
                                found = true;
                            }
                        }
                        if (!found) {
                            fcx.ccx.sess.span_err
                                (expr.span,
                                 "unknown field in record update: "
                                 + f.ident);
                        }
                    }
                }
            }
        }

        case (ast::expr_field(?base, ?field, ?a)) {
            check_expr(fcx, base);
            auto base_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, base);
            base_t = strip_boxes(fcx.ccx.tcx, base_t);
            alt (struct(fcx.ccx.tcx, base_t)) {
                case (ty::ty_tup(?args)) {
                    let uint ix = ty::field_num(fcx.ccx.sess,
                                               expr.span, field);
                    if (ix >= vec::len[ty::mt](args)) {
                        fcx.ccx.sess.span_err(expr.span,
                                              "bad index on tuple");
                    }
                    write_type_only(fcx.ccx.node_types, a.id,
                                    args.(ix).ty);
                }

                case (ty::ty_rec(?fields)) {
                    let uint ix = ty::field_idx(fcx.ccx.sess,
                                                expr.span, field, fields);
                    if (ix >= vec::len[typeck::field](fields)) {
                        fcx.ccx.sess.span_err(expr.span,
                                              "bad index on record");
                    }
                    write_type_only(fcx.ccx.node_types, a.id,
                                    fields.(ix).mt.ty);
                }

                case (ty::ty_obj(?methods)) {
                    let uint ix = ty::method_idx(fcx.ccx.sess,
                                                 expr.span, field, methods);
                    if (ix >= vec::len[typeck::method](methods)) {
                        fcx.ccx.sess.span_err(expr.span,
                                              "bad index on obj");
                    }
                    auto meth = methods.(ix);
                    auto t = ty::mk_fn(fcx.ccx.tcx, meth.proto,
                                       meth.inputs, meth.output);
                    write_type_only(fcx.ccx.node_types, a.id, t);
                }

                case (_) {
                    fcx.ccx.sess.span_unimpl(expr.span,
                        "base type for expr_field in typeck::check_expr: " +
                        ty_to_str(fcx.ccx.tcx, base_t));
                }
            }
        }

        case (ast::expr_index(?base, ?idx, ?a)) {
            check_expr(fcx, base);
            auto base_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, base);
            base_t = strip_boxes(fcx.ccx.tcx, base_t);

            check_expr(fcx, idx);
            auto idx_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, idx);
            alt (struct(fcx.ccx.tcx, base_t)) {
                case (ty::ty_vec(?mt)) {
                    if (! type_is_integral(fcx.ccx.tcx, idx_t)) {
                        fcx.ccx.sess.span_err
                            (idx.span,
                             "non-integral type of vec index: "
                             + ty_to_str(fcx.ccx.tcx, idx_t));
                    }
                    write_type_only(fcx.ccx.node_types, a.id, mt.ty);
                }
                case (ty::ty_str) {
                    if (! type_is_integral(fcx.ccx.tcx, idx_t)) {
                        fcx.ccx.sess.span_err
                            (idx.span,
                             "non-integral type of str index: "
                             + ty_to_str(fcx.ccx.tcx, idx_t));
                    }
                    auto typ = ty::mk_mach(fcx.ccx.tcx, common::ty_u8);
                    write_type_only(fcx.ccx.node_types, a.id, typ);
                }
                case (_) {
                    fcx.ccx.sess.span_err
                        (expr.span,
                         "vector-indexing bad type: "
                         + ty_to_str(fcx.ccx.tcx, base_t));
                }
            }
        }

        case (ast::expr_port(?a)) {
            auto t = next_ty_var(fcx.ccx);
            auto pt = ty::mk_port(fcx.ccx.tcx, t);
            write_type_only(fcx.ccx.node_types, a.id, pt);
        }

        case (ast::expr_chan(?x, ?a)) {
            check_expr(fcx, x);
            auto port_t = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, x);
            alt (struct(fcx.ccx.tcx, port_t)) {
                case (ty::ty_port(?subtype)) {
                    auto ct = ty::mk_chan(fcx.ccx.tcx, subtype);
                    write_type_only(fcx.ccx.node_types, a.id, ct);
                }
                case (_) {
                    fcx.ccx.sess.span_err(expr.span,
                        "bad port type: " + ty_to_str(fcx.ccx.tcx, port_t));
                }
            }
        }

        case (_) { fcx.ccx.sess.unimpl("expr type in typeck::check_expr"); }
    }
}

fn next_ty_var(&@crate_ctxt ccx) -> ty::t {
    auto t = ty::mk_var(ccx.tcx, ccx.next_var_id);
    ccx.next_var_id += 1;
    ret t;
}

fn check_decl_local(&@fn_ctxt fcx, &@ast::decl decl) -> @ast::decl {
    alt (decl.node) {
        case (ast::decl_local(?local)) {
            auto t = ty::mk_nil(fcx.ccx.tcx);

            alt (local.ty) {
                case (none[@ast::ty]) {
                    // Auto slot. Do nothing for now.
                }

                case (some[@ast::ty](?ast_ty)) {
                    auto local_ty = ast_ty_to_ty_crate(fcx.ccx, ast_ty);
                    fcx.locals.insert(local.id, local_ty);
                    t = local_ty;
                }
            }

            auto a_res = local.ann;
            write_type_only(fcx.ccx.node_types, a_res.id, t);

            auto initopt = local.init;
            alt (local.init) {
                case (some[ast::initializer](?init)) {
                    check_expr(fcx, init.expr);
                    auto lty = ty::mk_local(fcx.ccx.tcx, local.id);
                    alt (init.op) {
                        case (ast::init_assign) {
                            Pushdown::pushdown_expr(fcx, lty, init.expr);
                        }
                        case (ast::init_recv) {
                            auto port_ty = ty::mk_port(fcx.ccx.tcx, lty);
                            Pushdown::pushdown_expr(fcx, port_ty, init.expr);
                        }
                    }
                }
                case (_) { /* fall through */  }
            }
            auto local_1 = @rec(init = initopt, ann = a_res with *local);
            ret @rec(node=ast::decl_local(local_1) with *decl);
        }
    }
}

fn check_stmt(&@fn_ctxt fcx, &@ast::stmt stmt) -> @ast::stmt {
    alt (stmt.node) {
        case (ast::stmt_decl(?decl,?a)) {
            alt (decl.node) {
                case (ast::decl_local(_)) {
                    auto decl_1 = check_decl_local(fcx, decl);

                    auto new_ann = plain_ann(a.id, fcx.ccx.tcx);
                    write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);

                    ret @fold::respan[ast::stmt_](stmt.span,
                           ast::stmt_decl(decl_1, new_ann));
                }

                case (ast::decl_item(_)) {
                    // Ignore for now. We'll return later.
                    auto new_ann = plain_ann(a.id, fcx.ccx.tcx);
                    write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);

                    ret @fold::respan[ast::stmt_](stmt.span,
                           ast::stmt_decl(decl, new_ann));
                }
            }

            //         ret stmt;
        }

        case (ast::stmt_expr(?expr,?a)) {
            check_expr(fcx, expr);
            auto ety = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, expr);
            Pushdown::pushdown_expr(fcx, ety, expr);

            auto new_ann = plain_ann(a.id, fcx.ccx.tcx);
            write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types, a.id);

            ret @fold::respan(stmt.span, ast::stmt_expr(expr, new_ann));
        }
    }

    fail;
}

fn check_block(&@fn_ctxt fcx, &ast::block block) {
    for (@ast::stmt s in block.node.stmts) { check_stmt(fcx, s); }

    alt (block.node.expr) {
        case (none[@ast::expr]) { /* empty */ }
        case (some[@ast::expr](?e)) {
            check_expr(fcx, e);
            auto ety = expr_ty(fcx.ccx.tcx, fcx.ccx.node_types, e);
            Pushdown::pushdown_expr(fcx, ety, e);
        }
    }

    write_nil_type(fcx.ccx.tcx, fcx.ccx.node_types, block.node.a.id);
}

fn check_const(&@crate_ctxt ccx, &span sp, &ast::ident ident, &@ast::ty t,
               &@ast::expr e, &ast::def_id id, &ast::ann ann) {
    // FIXME: this is kinda a kludge; we manufacture a fake "function context"
    // for checking the initializer expression.
    auto rty = ann_to_type(ccx.node_types, ann);
    let @fn_ctxt fcx = @rec(ret_ty = rty,
                            purity = ast::pure_fn,
                            locals = @common::new_def_hash[ty::t](),
                            ccx = ccx);
    check_expr(fcx, e);
    Pushdown::pushdown_expr(fcx, rty, e);
}

fn check_fn(&@crate_ctxt ccx, &ast::fn_decl decl, ast::proto proto,
            &ast::block body) -> ast::_fn {
    auto local_ty_table = @common::new_def_hash[ty::t]();

    // FIXME: duplicate work: the item annotation already has the arg types
    // and return type translated to typeck::ty values. We don't need do to it
    // again here, we can extract them.


    for (ast::obj_field f in ccx.obj_fields) {
        auto field_ty = ty::ann_to_type(ccx.node_types, f.ann);
        local_ty_table.insert(f.id, field_ty);
    }

    // Store the type of each argument in the table.
    for (ast::arg arg in decl.inputs) {
        auto input_ty = ast_ty_to_ty_crate(ccx, arg.ty);
        local_ty_table.insert(arg.id, input_ty);
    }

    let @fn_ctxt fcx = @rec(ret_ty = ast_ty_to_ty_crate(ccx, decl.output),
                            purity = decl.purity,
                            locals = local_ty_table,
                            ccx = ccx);

    // TODO: Make sure the type of the block agrees with the function type.
    check_block(fcx, body);
    alt (decl.purity) {
        case (ast::pure_fn) {
            // per the previous comment, this just checks that the declared
            // type is bool, and trusts that that's the actual return type.
            if (!ty::type_is_bool(ccx.tcx, fcx.ret_ty)) {
              ccx.sess.span_err(body.span, "Non-boolean return type in pred");
            }
        }
        case (_) {}
    }

    writeback::resolve_local_types_in_block(fcx, body);

    ret rec(decl=decl, proto=proto, body=body);
}

fn update_obj_fields(&@crate_ctxt ccx, &@ast::item i) -> @crate_ctxt {
    alt (i.node) {
        case (ast::item_obj(_, ?ob, _, ?obj_def_ids, _)) {
            let ast::def_id di = obj_def_ids.ty;
            ret @rec(obj_fields = ob.fields,
                     this_obj = some[ast::def_id](di) with *ccx);
        }
        case (_) {
        }
    }
    ret ccx;
}


// Utilities for the unification cache

fn hash_unify_cache_entry(&unify_cache_entry uce) -> uint {
    auto h = ty::hash_ty(uce._0);
    h += h << 5u + ty::hash_ty(uce._1);

    auto i = 0u;
    auto tys_len = vec::len(uce._2);
    while (i < tys_len) {
        h += h << 5u + ty::hash_ty(uce._2.(i));
        i += 1u;
    }

    ret h;
}

fn eq_unify_cache_entry(&unify_cache_entry a, &unify_cache_entry b) -> bool {
    if (!ty::eq_ty(a._0, b._0) || !ty::eq_ty(a._1, b._1)) { ret false; }

    auto i = 0u;
    auto tys_len = vec::len(a._2);
    if (vec::len(b._2) != tys_len) { ret false; }

    while (i < tys_len) {
        if (!ty::eq_ty(a._2.(i), b._2.(i))) { ret false; }
        i += 1u;
    }

    ret true;
}

fn mk_fn_purity_table(&@ast::crate crate) -> @fn_purity_table {
    auto res = @new_def_hash[ast::purity]();

    fn do_one(@fn_purity_table t, &@ast::item i) -> () {
        alt (i.node) {
            case (ast::item_fn(_, ?f, _, ?d_id, _)) {
                t.insert(d_id, f.decl.purity);
            }
            case (_) {}
        }
    }

    auto do_one_fn = bind do_one(res,_);
    auto v = walk::default_visitor();

    auto add_fn_entry_visitor = rec(visit_item_post=do_one_fn with v);

    walk::walk_crate(add_fn_entry_visitor, *crate);
    ret res;
}

// TODO: Remove the third element of this tuple; rely solely on the node type
// table.
type typecheck_result = tup(node_type_table, ty::type_cache, @ast::crate);

fn check_crate(&ty::ctxt tcx, &@ast::crate crate) -> typecheck_result {
    auto sess = tcx.sess;
    auto result = collect::collect_item_types(sess, tcx, crate);

    let vec[ast::obj_field] fields = [];

    auto hasher = hash_unify_cache_entry;
    auto eqer = eq_unify_cache_entry;
    auto unify_cache =
        map::mk_hashmap[unify_cache_entry,ty::unify::result](hasher, eqer);
    auto fpt = mk_fn_purity_table(crate); // use a variation on collect
    let node_type_table node_types = result._2;

    auto ccx = @rec(sess=sess,
                    type_cache=result._0,
                    item_items=result._1,
                    obj_fields=fields,
                    this_obj=none[ast::def_id],
                    fn_purity_table = fpt,
                    mutable next_var_id=0,
                    unify_cache=unify_cache,
                    mutable cache_hits=0u,
                    mutable cache_misses=0u,
                    tcx=tcx,
                    node_types=node_types);

    auto fld = fold::new_identity_fold[@crate_ctxt]();

    fld = @rec(update_env_for_item = bind update_obj_fields(_, _),
               fold_fn      = bind check_fn(_,_,_,_)
               with *fld);

    auto crate_1 = fold::fold_crate[@crate_ctxt](ccx, fld, crate);

    log #fmt("cache hit rate: %u/%u", ccx.cache_hits,
             ccx.cache_hits + ccx.cache_misses);

    ret tup(node_types, ccx.type_cache, crate_1);
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
