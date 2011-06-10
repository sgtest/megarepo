import front::ast;
import front::ast::ann;
import front::ast::mutability;
import front::creader;
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
import middle::ty::mo_val;
import middle::ty::mo_alias;
import middle::ty::node_type_table;
import middle::ty::pat_ty;
import middle::ty::path_to_str;
import middle::ty::ty_param_substs_opt_and_ty;
import pretty::ppaux::ty_to_str;
import middle::ty::ty_param_count_and_ty;
import middle::ty::ty_nil;
import middle::ty::unify::ures_ok;
import middle::ty::unify::ures_err;
import middle::ty::unify::fixup_result;
import middle::ty::unify::fix_ok;
import middle::ty::unify::fix_err;

import std::int;
import std::str;
import std::ufind;
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
type fn_purity_table = hashmap[ast::def_id, ast::purity];

type obj_info = rec(vec[ast::obj_field] obj_fields, ast::def_id this_obj);

type crate_ctxt = rec(mutable vec[obj_info] obj_infos,
                      @fn_purity_table fn_purity_table,
                      ty::ctxt tcx);

type fn_ctxt = rec(ty::t ret_ty,
                   ast::purity purity,
                   @ty::unify::var_bindings var_bindings,
                   hashmap[ast::def_id,int] locals,
                   hashmap[ast::def_id,ast::ident] local_names,
                   mutable int next_var_id,
                   mutable vec[uint] fixups,
                   @crate_ctxt ccx);

// Used for ast_ty_to_ty() below.
type ty_getter = fn(&ast::def_id) -> ty::ty_param_count_and_ty;


// Returns the type parameter count and the type for the given definition.
fn ty_param_count_and_ty_for_def(&@fn_ctxt fcx, &span sp, &ast::def defn)
        -> ty_param_count_and_ty {
    alt (defn) {
        case (ast::def_arg(?id)) {
            assert (fcx.locals.contains_key(id));
            auto typ = ty::mk_var(fcx.ccx.tcx, fcx.locals.get(id));
            ret tup(0u, typ);
        }
        case (ast::def_local(?id)) {
            assert (fcx.locals.contains_key(id));
            auto typ = ty::mk_var(fcx.ccx.tcx, fcx.locals.get(id));
            ret tup(0u, typ);
        }
        case (ast::def_obj_field(?id)) {
            assert (fcx.locals.contains_key(id));
            auto typ = ty::mk_var(fcx.ccx.tcx, fcx.locals.get(id));
            ret tup(0u, typ);
        }
        case (ast::def_fn(?id)) {
            ret ty::lookup_item_type(fcx.ccx.tcx, id);
        }
        case (ast::def_native_fn(?id)) {
            ret ty::lookup_item_type(fcx.ccx.tcx, id);
        }
        case (ast::def_const(?id)) {
            ret ty::lookup_item_type(fcx.ccx.tcx, id);
        }
        case (ast::def_variant(_, ?vid)) {
            ret ty::lookup_item_type(fcx.ccx.tcx, vid);
        }
        case (ast::def_binding(?id)) {
            assert (fcx.locals.contains_key(id));
            auto typ = ty::mk_var(fcx.ccx.tcx, fcx.locals.get(id));
            ret tup(0u, typ);
        }
        case (ast::def_obj(?id)) {
            ret ty::lookup_item_type(fcx.ccx.tcx, id);
        }

        case (ast::def_mod(_)) {
            // Hopefully part of a path.
            // TODO: return a type that's more poisonous, perhaps?
            ret tup(0u, ty::mk_nil(fcx.ccx.tcx));
        }

        case (ast::def_ty(_)) {
            fcx.ccx.tcx.sess.span_err(sp, "expected value but found type");
        }

        case (_) {
            // FIXME: handle other names.
            fcx.ccx.tcx.sess.unimpl("definition variant");
        }
    }
}

// Instantiates the given path, which must refer to an item with the given
// number of type parameters and type.
fn instantiate_path(&@fn_ctxt fcx,
                    &ast::path pth,
                    &ty_param_count_and_ty tpt,
                    &span sp) -> ty_param_substs_opt_and_ty {
    auto ty_param_count = tpt._0;

    auto bind_result = bind_params_in_type(fcx.ccx.tcx,
                                           bind next_ty_var_id(fcx),
                                           tpt._1,
                                           ty_param_count);
    auto ty_param_vars = bind_result._0;
    auto t = bind_result._1;

    auto ty_substs_opt;
    auto ty_substs_len = vec::len[@ast::ty](pth.node.types);
    if (ty_substs_len > 0u) {
        let vec[ty::t] ty_substs = [];
        auto i = 0u;
        while (i < ty_substs_len) {
            // TODO: Report an error if the number of type params in the item
            // and the supplied number of type params don't match.
            auto ty_var = ty::mk_var(fcx.ccx.tcx, ty_param_vars.(i));
            auto ty_subst = ast_ty_to_ty_crate(fcx.ccx,
                                               pth.node.types.(i));
            auto res_ty = demand::simple(fcx, pth.span, ty_var, ty_subst);
            ty_substs += [res_ty];
            i += 1u;
        }
        ty_substs_opt = some[vec[ty::t]](ty_substs);

        if (ty_param_count == 0u) {
            fcx.ccx.tcx.sess.span_err(sp, "this item does not take type " +
                                      "parameters");
            fail;
        }
    } else {
        // We will acquire the type parameters through unification.
        let vec[ty::t] ty_substs = [];
        auto i = 0u;
        while (i < ty_param_count) {
            ty_substs += [ty::mk_var(fcx.ccx.tcx, ty_param_vars.(i))];
            i += 1u;
        }
        ty_substs_opt = some[vec[ty::t]](ty_substs);
    }

    ret tup(ty_substs_opt, tpt._1);
}

fn ast_mode_to_mode(ast::mode mode) -> ty::mode {
    auto ty_mode;
    alt (mode) {
        case (ast::val) { ty_mode = mo_val; }
        case (ast::alias(?mut)) { ty_mode = mo_alias(mut); }
    }
    ret ty_mode;
}


// Type tests

fn structurally_resolved_type(&@fn_ctxt fcx, &span sp, ty::t typ) -> ty::t {
    auto r = ty::unify::resolve_type_structure(fcx.ccx.tcx, fcx.var_bindings,
                                               typ);
    alt (r) {
        case (fix_ok(?typ_s)) { ret typ_s; }
        case (fix_err(_)) {
            fcx.ccx.tcx.sess.span_err(sp, "the type of this value must be " +
                "known in this context");
        }
    }
}

// Returns the one-level-deep structure of the given type.
fn structure_of(&@fn_ctxt fcx, &span sp, ty::t typ) -> ty::sty {
    ret ty::struct(fcx.ccx.tcx, structurally_resolved_type(fcx, sp, typ));
}

fn type_is_integral(&@fn_ctxt fcx, &span sp, ty::t typ) -> bool {
    auto typ_s = structurally_resolved_type(fcx, sp, typ);
    ret ty::type_is_integral(fcx.ccx.tcx, typ_s);
}

fn type_is_scalar(&@fn_ctxt fcx, &span sp, ty::t typ) -> bool {
    auto typ_s = structurally_resolved_type(fcx, sp, typ);
    ret ty::type_is_scalar(fcx.ccx.tcx, typ_s);
}


// Parses the programmer's textual representation of a type into our internal
// notion of a type. `getter` is a function that returns the type
// corresponding to a definition ID:
fn ast_ty_to_ty(&ty::ctxt tcx, &ty_getter getter, &@ast::ty ast_ty) -> ty::t {
    fn ast_arg_to_arg(&ty::ctxt tcx,
                      &ty_getter getter,
                      &ast::ty_arg arg)
            -> rec(ty::mode mode, ty::t ty) {
        auto ty_mode = ast_mode_to_mode(arg.node.mode);
        ret rec(mode=ty_mode, ty=ast_ty_to_ty(tcx, getter, arg.node.ty));
    }

    fn ast_mt_to_mt(&ty::ctxt tcx,
                    &ty_getter getter,
                    &ast::mt mt) -> ty::mt {
        ret rec(ty=ast_ty_to_ty(tcx, getter, mt.ty), mut=mt.mut);
    }

    fn instantiate(&ty::ctxt tcx,
                   &span sp,
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
        let vec[ty::t] param_bindings = [];
        for (@ast::ty ast_ty in args) {
            param_bindings += [ast_ty_to_ty(tcx, getter, ast_ty)];
        }

        if (vec::len(param_bindings) !=
            ty::count_ty_params(tcx, params_opt_and_ty._1)) {
            tcx.sess.span_err(sp, "Wrong number of type arguments for a"
                            + " polymorphic tag");
        }


        auto typ = ty::substitute_type_params(tcx, param_bindings,
                                       params_opt_and_ty._1);
        ret typ;
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
        case (ast::ty_istr)         { typ = ty::mk_istr(tcx); }
        case (ast::ty_box(?mt)) {
            typ = ty::mk_box(tcx, ast_mt_to_mt(tcx, getter, mt));
        }
        case (ast::ty_vec(?mt)) {
            typ = ty::mk_vec(tcx, ast_mt_to_mt(tcx, getter, mt));
        }
        case (ast::ty_ivec(?mt)) {
            typ = ty::mk_ivec(tcx, ast_mt_to_mt(tcx, getter, mt));
        }
        case (ast::ty_ptr(?mt)) {
            typ = ty::mk_ptr(tcx, ast_mt_to_mt(tcx, getter, mt));
        }
        case (ast::ty_task) { typ = ty::mk_task(tcx); }
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
                auto tm = ast_mt_to_mt(tcx, getter, f.node.mt);
                vec::push[field](flds, rec(ident=f.node.ident, mt=tm));
            }
            typ = ty::mk_rec(tcx, flds);
        }

        case (ast::ty_fn(?proto, ?inputs, ?output, ?cf, ?constrs)) {
            auto f = bind ast_arg_to_arg(tcx, getter, _);
            auto i = vec::map[ast::ty_arg, arg](f, inputs);
            auto out_ty = ast_ty_to_ty(tcx, getter, output);
            typ = ty::mk_fn(tcx, proto, i, out_ty, cf, constrs);
        }

        case (ast::ty_path(?path, ?ann)) {
            alt (tcx.def_map.get(ann.id)) {
                case (ast::def_ty(?id)) {
                    typ = instantiate(tcx, ast_ty.span, getter, id,
                                      path.node.types);
                }
                case (ast::def_native_ty(?id)) { typ = getter(id)._1; }
                case (ast::def_obj(?id)) {
                    typ = instantiate(tcx, ast_ty.span, getter, id,
                                      path.node.types);
                }
                case (ast::def_ty_arg(?id)) { typ = ty::mk_param(tcx, id); }
                case (_)                   {
                    tcx.sess.span_err(ast_ty.span,
                       "found type name used as a variable");
                }
            }

            cname = some(path_to_str(path));
        }

        case (ast::ty_obj(?meths)) {
            let vec[ty::method] tmeths = [];
            auto f = bind ast_arg_to_arg(tcx, getter, _);
            for (ast::ty_method m in meths) {
                auto ins = vec::map[ast::ty_arg, arg](f, m.node.inputs);
                auto out = ast_ty_to_ty(tcx, getter, m.node.output);
                let ty::method new_m =
                                  rec(proto=m.node.proto,
                                      ident=m.node.ident,
                                      inputs=ins,
                                      output=out,
                                      cf=m.node.cf,
                                      constrs=m.node.constrs);
                vec::push[ty::method](tmeths, new_m);
            }

            typ = ty::mk_obj(tcx, ty::sort_methods(tmeths));
        }
    }

    alt (cname) {
        case (none) { /* no-op */ }
        case (some(?cname_str)) {
            typ = ty::rename(tcx, typ, cname_str);
        }
    }
    ret typ;
}

// A convenience function to use a crate_ctxt to resolve names for
// ast_ty_to_ty.
fn ast_ty_to_ty_crate(@crate_ctxt ccx, &@ast::ty ast_ty) -> ty::t {
    fn getter(@crate_ctxt ccx, &ast::def_id id) -> ty::ty_param_count_and_ty {
        ret ty::lookup_item_type(ccx.tcx, id);
    }
    auto f = bind getter(ccx, _);
    ret ast_ty_to_ty(ccx.tcx, f, ast_ty);
}


// Functions that write types into the node type table.

mod write {
    fn inner(&node_type_table ntt, uint node_id,
             &ty_param_substs_opt_and_ty tpot) {
        auto ntt_ = *ntt;
        vec::grow_set[option::t[ty::ty_param_substs_opt_and_ty]]
            (ntt_,
             node_id,
             none[ty_param_substs_opt_and_ty],
             some[ty_param_substs_opt_and_ty](tpot));
        *ntt = ntt_;
    }

    // Writes a type parameter count and type pair into the node type table.
    fn ty(&ty::ctxt tcx, uint node_id,
          &ty_param_substs_opt_and_ty tpot) {
        assert (!ty::type_contains_vars(tcx, tpot._1));
        be inner(tcx.node_types, node_id, tpot);
    }

    // Writes a type parameter count and type pair into the node type table.
    // This function allows for the possibility of type variables, which will
    // be rewritten later during the fixup phase.
    fn ty_fixup(@fn_ctxt fcx, uint node_id,
                &ty_param_substs_opt_and_ty tpot) {
        inner(fcx.ccx.tcx.node_types, node_id, tpot);
        if (ty::type_contains_vars(fcx.ccx.tcx, tpot._1)) {
            fcx.fixups += [node_id];
        }
    }

    // Writes a type with no type parameters into the node type table.
    fn ty_only(&ty::ctxt tcx, uint node_id, ty::t typ) {
        be ty(tcx, node_id, tup(none[vec[ty::t]], typ));
    }

    // Writes a type with no type parameters into the node type table. This
    // function allows for the possibility of type variables.
    fn ty_only_fixup(@fn_ctxt fcx, uint node_id, ty::t typ) {
        be ty_fixup(fcx, node_id, tup(none[vec[ty::t]], typ));
    }

    // Writes a nil type into the node type table.
    fn nil_ty(&ty::ctxt tcx, uint node_id) {
        be ty(tcx, node_id, tup(none[vec[ty::t]], ty::mk_nil(tcx)));
    }

    // Writes the bottom type into the node type table.
    fn bot_ty(&ty::ctxt tcx, uint node_id) {
        be ty(tcx, node_id, tup(none[vec[ty::t]], ty::mk_bot(tcx)));
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
    type ctxt = rec(ty::ctxt tcx);

    fn ty_of_fn_decl(&@ctxt cx,
                     &fn(&@ast::ty ast_ty) -> ty::t convert,
                     &fn(&ast::arg a) -> arg ty_of_arg,
                     &ast::fn_decl decl,
                     ast::proto proto,
                     &vec[ast::ty_param] ty_params,
                     &ast::def_id def_id) -> ty::ty_param_count_and_ty {
        auto input_tys = vec::map[ast::arg,arg](ty_of_arg, decl.inputs);
        auto output_ty = convert(decl.output);
        auto t_fn = ty::mk_fn(cx.tcx, proto, input_tys, output_ty,
                              decl.cf, decl.constraints);
        auto ty_param_count = vec::len[ast::ty_param](ty_params);
        auto tpt = tup(ty_param_count, t_fn);
        cx.tcx.tcache.insert(def_id, tpt);
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
        cx.tcx.tcache.insert(def_id, tpt);
        ret tpt;
    }

    fn getter(@ctxt cx, &ast::def_id id) -> ty::ty_param_count_and_ty {

        if (id._0 != cx.tcx.sess.get_targ_crate_num()) {
            // This is a type we need to load in from the crate reader.
            ret creader::get_type(cx.tcx, id);
        }

        auto it = cx.tcx.items.get(id);
        auto tpt;
        alt (it) {
            case (ty::any_item_rust(?item)) { tpt = ty_of_item(cx, item); }
            case (ty::any_item_native(?native_item, ?abi)) {
                tpt = ty_of_native_item(cx, native_item, abi);
            }
        }

        ret tpt;
    }

    fn ty_of_arg(@ctxt cx, &ast::arg a) -> ty::arg {
        auto ty_mode = ast_mode_to_mode(a.mode);
        auto f = bind getter(cx, _);
        ret rec(mode=ty_mode, ty=ast_ty_to_ty(cx.tcx, f, a.ty));
    }

    fn ty_of_method(@ctxt cx, &@ast::method m) -> ty::method {
        auto get = bind getter(cx, _);
        auto convert = bind ast_ty_to_ty(cx.tcx, get, _);
        auto f = bind ty_of_arg(cx, _);
        auto inputs = vec::map[ast::arg,arg](f, m.node.meth.decl.inputs);
        auto output = convert(m.node.meth.decl.output);
        ret rec(proto=m.node.meth.proto, ident=m.node.ident,
                inputs=inputs, output=output, cf=m.node.meth.decl.cf,
                constrs=m.node.meth.decl.constraints);
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
            vec::push(t_inputs, rec(mode=ty::mo_alias(false), ty=t_field));
        }

        let vec[@ast::constr] constrs = [];
        auto t_fn = ty::mk_fn(cx.tcx, ast::proto_fn, t_inputs, t_obj._1,
                              ast::return, constrs);

        auto tpt = tup(t_obj._0, t_fn);
        cx.tcx.tcache.insert(ctor_id, tpt);
        ret tpt;
    }

    fn ty_of_item(&@ctxt cx, &@ast::item it) -> ty::ty_param_count_and_ty {

        auto get = bind getter(cx, _);
        auto convert = bind ast_ty_to_ty(cx.tcx, get, _);

        alt (it.node) {

            case (ast::item_const(?ident, ?t, _, ?def_id, _)) {
                auto typ = convert(t);
                auto tpt = tup(0u, typ);
                cx.tcx.tcache.insert(def_id, tpt);
                ret tpt;
            }

            case (ast::item_fn(?ident, ?fn_info, ?tps, ?def_id, _)) {
                auto f = bind ty_of_arg(cx, _);
                ret ty_of_fn_decl(cx, convert, f, fn_info.decl, fn_info.proto,
                                  tps, def_id);
            }

            case (ast::item_obj(?ident, ?obj_info, ?tps, ?odid, _)) {
                auto t_obj = ty_of_obj(cx, ident, obj_info, tps);
                cx.tcx.tcache.insert(odid.ty, t_obj);
                ret t_obj;
            }

            case (ast::item_ty(?ident, ?t, ?tps, ?def_id, _)) {
                alt (cx.tcx.tcache.find(def_id)) {
                    case (some(?tpt)) {
                        ret tpt;
                    }
                    case (none) {}
                }

                // Tell ast_ty_to_ty() that we want to perform a recursive
                // call to resolve any named types.
                auto typ = convert(t);
                auto ty_param_count = vec::len[ast::ty_param](tps);
                auto tpt = tup(ty_param_count, typ);
                cx.tcx.tcache.insert(def_id, tpt);
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
                cx.tcx.tcache.insert(def_id, tpt);
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
                alt (cx.tcx.tcache.find(def_id)) {
                    case (some(?tpt)) {
                        ret tpt;
                    }
                    case (none) {}
                }

                auto t = ty::mk_native(cx.tcx);
                auto tpt = tup(0u, t);
                cx.tcx.tcache.insert(def_id, tpt);
                ret tpt;
            }
        }
    }

    fn get_tag_variant_types(&@ctxt cx, &ast::def_id tag_id,
                             &vec[ast::variant] variants,
                             &vec[ast::ty_param] ty_params) {

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
                    args += [rec(mode=ty::mo_alias(false), ty=arg_ty)];
                }
                auto tag_t = ty::mk_tag(cx.tcx, tag_id, ty_param_tys);
                // FIXME: this will be different for constrained types
                let vec[@ast::constr] res_constrs = [];
                result_ty = ty::mk_fn(cx.tcx, ast::proto_fn, args, tag_t,
                                      ast::return, res_constrs);
            }

            auto tpt = tup(ty_param_count, result_ty);
            cx.tcx.tcache.insert(variant.node.id, tpt);
            write::ty_only(cx.tcx, variant.node.ann.id, result_ty);
        }
    }

    fn get_obj_method_types(&@ctxt cx, &ast::_obj object) -> vec[ty::method] {
        ret vec::map[@ast::method,method](bind ty_of_method(cx, _),
                                          object.methods);
    }

    fn collect(ty::item_table id_to_ty_item, &@ast::item i) {
        alt (i.node) {
            case (ast::item_ty(_, _, _, ?def_id, _)) {
                id_to_ty_item.insert(def_id, ty::any_item_rust(i));
            }
            case (ast::item_tag(_, _, _, ?def_id, _)) {
                id_to_ty_item.insert(def_id, ty::any_item_rust(i));
            }
            case (ast::item_obj(_, _, _, ?odid, _)) {
                id_to_ty_item.insert(odid.ty, ty::any_item_rust(i));
            }
            case (_) { /* empty */ }
        }
    }

    fn collect_native(ty::item_table id_to_ty_item, &@ast::native_item i) {
        alt (i.node) {
            case (ast::native_item_ty(_, ?def_id)) {
                // The abi of types is not used.
                id_to_ty_item.insert(def_id,
                    ty::any_item_native(i, ast::native_abi_cdecl));
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
                write::ty_only(cx.tcx, ann.id, tpt._1);

                get_tag_variant_types(cx, tag_id, variants, ty_params);
            }
            case (ast::item_obj(?ident, ?object, ?ty_params, ?odid, ?ann)) {
                // This calls ty_of_obj().
                auto t_obj = ty_of_item(cx, it);

                // Now we need to call ty_of_obj_ctor(); this is the type that
                // we write into the table for this item.
                auto tpt = ty_of_obj_ctor(cx, ident, object, odid.ctor,
                                          ty_params);
                write::ty_only(cx.tcx, ann.id, tpt._1);

                // Write the methods into the type table.
                //
                // FIXME: Inefficient; this ends up calling
                // get_obj_method_types() twice. (The first time was above in
                // ty_of_obj().)
                auto method_types = get_obj_method_types(cx, object);
                auto i = 0u;
                while (i < vec::len[@ast::method](object.methods)) {
                    write::ty_only(cx.tcx, object.methods.(i).node.ann.id,
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
                    write::ty_only(cx.tcx, fld.ann.id, args.(i).ty);
                    i += 1u;
                }

                // Finally, write in the type of the destructor.
                alt (object.dtor) {
                    case (none) { /* nothing to do */ }
                    case (some(?m)) {
                        let vec[@ast::constr] constrs = [];
                        let vec[arg] res_inputs  = [];
                        auto t = ty::mk_fn(cx.tcx, ast::proto_fn, res_inputs,
                                   ty::mk_nil(cx.tcx), ast::return, constrs);
                        write::ty_only(cx.tcx, m.node.ann.id, t);
                    }
                }
            }
            case (_) {
                // This call populates the type cache with the converted type
                // of the item in passing. All we have to do here is to write
                // it into the node type table.
                auto tpt = ty_of_item(cx, it);
                write::ty_only(cx.tcx, ty::item_ann(it).id, tpt._1);
            }
        }
    }

    fn convert_native(@ctxt cx, @mutable option::t[ast::native_abi] abi,
                      &@ast::native_item i) {
        // As above, this call populates the type table with the converted
        // type of the native item. We simply write it into the node type
        // table.
        auto tpt = ty_of_native_item(cx, i,
                                     option::get[ast::native_abi]({*abi}));

        alt (i.node) {
            case (ast::native_item_ty(_,_)) {
                // FIXME: Native types have no annotation. Should they? --pcw
            }
            case (ast::native_item_fn(_,_,_,_,_,?a)) {
                write::ty_only(cx.tcx, a.id, tpt._1);
            }
        }
    }

    fn collect_item_types(&ty::ctxt tcx, &@ast::crate crate) {
        // First pass: collect all type item IDs.
        auto module = crate.node.module;

        auto visit = rec(
            visit_item_pre = bind collect(tcx.items, _),
            visit_native_item_pre = bind collect_native(tcx.items, _)
            with walk::default_visitor()
        );
        walk::walk_crate(visit, *crate);

        // We have to propagate the surrounding ABI to the native items
        // contained within the native module.
        auto abi = @mutable none[ast::native_abi];

        auto cx = @rec(tcx=tcx);
        visit = rec(
            visit_item_pre = bind convert(cx,abi,_),
            visit_native_item_pre = bind convert_native(cx,abi,_)
            with walk::default_visitor()
        );
        walk::walk_crate(visit, *crate);
    }
}


// Type unification

// TODO: rename to just "unify"
mod unify {
    fn simple(&@fn_ctxt fcx, &ty::t expected, &ty::t actual)
            -> ty::unify::result {
        ret ty::unify::unify(expected, actual, fcx.var_bindings, fcx.ccx.tcx);
    }
}


tag autoderef_kind {
    AUTODEREF_OK;
    NO_AUTODEREF;
}

fn strip_boxes(&@fn_ctxt fcx, &span sp, &ty::t t) -> ty::t {
    auto t1 = t;
    while (true) {
        alt (structure_of(fcx, sp, t1)) {
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


fn count_boxes(&@fn_ctxt fcx, &span sp, &ty::t t) -> uint {
    auto n = 0u;
    auto t1 = t;
    while (true) {
        alt (structure_of(fcx, sp, t1)) {
            case (ty::ty_box(?inner)) { n += 1u; t1 = inner.ty; }
            case (_) { ret n; }
        }
    }
    fail;
}


fn resolve_type_vars_if_possible(&@fn_ctxt fcx, ty::t typ) -> ty::t {
    alt (ty::unify::fixup_vars(fcx.ccx.tcx, fcx.var_bindings, typ)) {
        case (fix_ok(?new_type)) { ret new_type; }
        case (fix_err(_)) { ret typ; }
    }
}


// Demands - procedures that require that two types unify and emit an error
// message if they don't.

type ty_param_substs_and_ty = tup(vec[ty::t], ty::t);

mod demand {
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
            expected_1 = strip_boxes(fcx, sp, expected_1);
            actual_1 = strip_boxes(fcx, sp, actual_1);
            implicit_boxes = count_boxes(fcx, sp, actual);
        }

        let vec[mutable ty::t] ty_param_substs = [mutable];
        let vec[int] ty_param_subst_var_ids = [];
        for (ty::t ty_param_subst in ty_param_substs_0) {
            // Generate a type variable and unify it with the type parameter
            // substitution. We will then pull out these type variables.
            auto t_0 = next_ty_var(fcx);
            ty_param_substs += [mutable t_0];
            ty_param_subst_var_ids += [ty::ty_var_id(fcx.ccx.tcx, t_0)];

            simple(fcx, sp, ty_param_subst, t_0);
        }

        alt (unify::simple(fcx, expected_1, actual_1)) {
            case (ures_ok(?t)) {
                let vec[ty::t] result_ty_param_substs = [];
                for (int var_id in ty_param_subst_var_ids) {
                    auto tp_subst = ty::mk_var(fcx.ccx.tcx, var_id);
                    result_ty_param_substs += [tp_subst];
                }

                ret tup(result_ty_param_substs,
                        add_boxes(fcx.ccx, implicit_boxes, t));
            }

            case (ures_err(?err)) {
                auto e_err = resolve_type_vars_if_possible(fcx, expected_1);
                auto a_err = resolve_type_vars_if_possible(fcx, actual_1);

                fcx.ccx.tcx.sess.span_err
                    (sp, "mismatched types: expected "
                     + ty_to_str(fcx.ccx.tcx, e_err) + " but found "
                     + ty_to_str(fcx.ccx.tcx, a_err) + " ("
                     + ty::type_err_to_str(err) + ")");

                // TODO: In the future, try returning "expected", reporting
                // the error, and continue.
            }
        }
    }
}


// Returns true if the two types unify and false if they don't.
fn are_compatible(&@fn_ctxt fcx, &ty::t expected, &ty::t actual) -> bool {
    alt (unify::simple(fcx, expected, actual)) {
        case (ures_ok(_))   { ret true;  }
        case (ures_err(_))  { ret false; }
    }
}

// Returns the types of the arguments to a tag variant.
fn variant_arg_types(&@crate_ctxt ccx, &span sp, &ast::def_id vid,
                     &vec[ty::t] tag_ty_params) -> vec[ty::t] {
    auto ty_param_count = vec::len[ty::t](tag_ty_params);

    let vec[ty::t] result = [];

    auto tpt = ty::lookup_item_type(ccx.tcx, vid);
    alt (ty::struct(ccx.tcx, tpt._1)) {
        case (ty::ty_fn(_, ?ins, _, _, _)) {
            // N-ary variant.
            for (ty::arg arg in ins) {
                auto arg_ty = ty::substitute_type_params(ccx.tcx,
                    tag_ty_params, arg.ty);
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


// Type resolution: the phase that finds all the types in the AST with
// unresolved type variables and replaces "ty_var" types with their
// substitutions.
//
// TODO: inefficient since not all types have vars in them. It would be better
// to maintain a list of fixups.

mod writeback {
    fn resolve_type_vars_in_type(&@fn_ctxt fcx, &span sp, ty::t typ)
            -> ty::t {
        if (!ty::type_contains_vars(fcx.ccx.tcx, typ)) { ret typ; }

        alt (ty::unify::fixup_vars(fcx.ccx.tcx, fcx.var_bindings, typ)) {
            case (fix_ok(?new_type)) { ret new_type; }
            case (fix_err(?vid)) {
                fcx.ccx.tcx.sess.span_err(sp,
                    "cannot determine a type for this expression");
            }
        }
    }

    fn resolve_type_vars_for_node(&@fn_ctxt fcx, &span sp, &ast::ann ann) {
        auto tpot = ty::ann_to_ty_param_substs_opt_and_ty(fcx.ccx.tcx, ann);
        auto new_ty = resolve_type_vars_in_type(fcx, sp, tpot._1);

        auto new_substs_opt;
        alt (tpot._0) {
            case (none[vec[ty::t]]) { new_substs_opt = none[vec[ty::t]]; }
            case (some[vec[ty::t]](?substs)) {
                let vec[ty::t] new_substs = [];
                for (ty::t subst in substs) {
                    new_substs += [resolve_type_vars_in_type(fcx, sp, subst)];
                }
                new_substs_opt = some[vec[ty::t]](new_substs);
            }
        }

        write::ty(fcx.ccx.tcx, ann.id, tup(new_substs_opt, new_ty));
    }

    fn visit_stmt_pre(@fn_ctxt fcx, &@ast::stmt s) {
        resolve_type_vars_for_node(fcx, s.span, ty::stmt_ann(s));
    }

    fn visit_expr_pre(@fn_ctxt fcx, &@ast::expr e) {
        resolve_type_vars_for_node(fcx, e.span, ty::expr_ann(e));
    }

    fn visit_block_pre(@fn_ctxt fcx, &ast::block b) {
        resolve_type_vars_for_node(fcx, b.span, b.node.a);
    }

    fn visit_pat_pre(@fn_ctxt fcx, &@ast::pat p) {
        resolve_type_vars_for_node(fcx, p.span, ty::pat_ann(p));
    }

    fn visit_decl_pre(@fn_ctxt fcx, &@ast::decl d) {
        alt (d.node) {
            case (ast::decl_local(?l)) {
                auto var_id = fcx.locals.get(l.id);
                auto fix_rslt = ty::unify::resolve_type_var(fcx.ccx.tcx,
                    fcx.var_bindings, var_id);
                alt (fix_rslt) {
                    case (fix_ok(?lty)) {
                        write::ty_only(fcx.ccx.tcx, l.ann.id, lty);
                    }
                    case (fix_err(_)) {
                        fcx.ccx.tcx.sess.span_err(d.span,
                            "cannot determine a type for this local " +
                            "variable");
                    }
                }
            }
            case (_) { /* no-op */ }
        }
    }

    fn resolve_type_vars_in_block(&@fn_ctxt fcx, &ast::block block) {
        // A trick to ignore any contained items.
        auto ignore = @mutable false;
        fn visit_item_pre(@mutable bool ignore, &@ast::item item) {
            *ignore = true;
        }
        fn visit_item_post(@mutable bool ignore, &@ast::item item) {
            *ignore = false;
        }
        fn keep_going(@mutable bool ignore) -> bool { ret !*ignore; }

        auto visit = rec(keep_going=bind keep_going(ignore),
                         visit_item_pre=bind visit_item_pre(ignore, _),
                         visit_item_post=bind visit_item_post(ignore, _),
                         visit_stmt_pre=bind visit_stmt_pre(fcx, _),
                         visit_expr_pre=bind visit_expr_pre(fcx, _),
                         visit_block_pre=bind visit_block_pre(fcx, _),
                         visit_pat_pre=bind visit_pat_pre(fcx, _),
                         visit_decl_pre=bind visit_decl_pre(fcx, _)
                         with walk::default_visitor());
        walk::walk_block(visit, block);
    }
}


// Local variable gathering. We gather up all locals and create variable IDs
// for them before typechecking the function.

type gather_result = rec(
    @ty::unify::var_bindings var_bindings,
    hashmap[ast::def_id,int] locals,
    hashmap[ast::def_id,ast::ident] local_names,
    int next_var_id
);

fn gather_locals(&@crate_ctxt ccx, &ast::fn_decl decl, &ast::block body,
                 &ast::ann ann) -> gather_result {
    fn next_var_id(@mutable int nvi) -> int {
        auto rv = *nvi;
        *nvi += 1;
        ret rv;
    }

    fn assign(&ty::ctxt tcx,
              &@ty::unify::var_bindings var_bindings,
              &hashmap[ast::def_id,int] locals,
              &hashmap[ast::def_id,ast::ident] local_names,
              @mutable int nvi,
              ast::def_id lid,
              &ast::ident ident,
              option::t[ty::t] ty_opt) {
        auto var_id = next_var_id(nvi);
        locals.insert(lid, var_id);
        local_names.insert(lid, ident);

        alt (ty_opt) {
            case (none[ty::t]) { /* nothing to do */ }
            case (some[ty::t](?typ)) {
                ty::unify::unify(ty::mk_var(tcx, var_id), typ, var_bindings,
                                 tcx);
            }
        }
    }

    auto vb = ty::unify::mk_var_bindings();
    auto locals = new_def_hash[int]();
    auto local_names = new_def_hash[ast::ident]();
    auto nvi = @mutable 0;

    // Add object fields, if any.
    alt (get_obj_info(ccx)) {
        case (option::some(?oinfo)) {
            for (ast::obj_field f in oinfo.obj_fields) {
                auto field_ty = ty::ann_to_type(ccx.tcx, f.ann);
                assign(ccx.tcx, vb, locals, local_names, nvi, f.id, f.ident,
                       some[ty::t](field_ty));
            }
        }
        case (option::none) { /* no fields */ }
    }

    // Add formal parameters.
    auto args = ty::ty_fn_args(ccx.tcx, ty::ann_to_type(ccx.tcx, ann));
    auto i = 0u;
    for (ty::arg arg in args) {
        assign(ccx.tcx, vb, locals, local_names, nvi, decl.inputs.(i).id,
               decl.inputs.(i).ident, some[ty::t](arg.ty));
        i += 1u;
    }

    // Add explicitly-declared locals.
    fn visit_decl_pre(@crate_ctxt ccx,
                      @ty::unify::var_bindings vb,
                      hashmap[ast::def_id,int] locals,
                      hashmap[ast::def_id,ast::ident] local_names,
                      @mutable int nvi,
                      &@ast::decl d) {
        alt (d.node) {
            case (ast::decl_local(?local)) {
                alt (local.ty) {
                    case (none) {
                        // Auto slot.
                        assign(ccx.tcx, vb, locals, local_names, nvi,
                               local.id, local.ident, none[ty::t]);
                    }
                    case (some(?ast_ty)) {
                        // Explicitly typed slot.
                        auto local_ty = ast_ty_to_ty_crate(ccx, ast_ty);
                        assign(ccx.tcx, vb, locals, local_names, nvi,
                               local.id, local.ident, some[ty::t](local_ty));
                    }
                }
            }
            case (_) { /* no-op */ }
        }
    }

    // Add pattern bindings.
    fn visit_pat_pre(@crate_ctxt ccx,
                     @ty::unify::var_bindings vb,
                     hashmap[ast::def_id,int] locals,
                     hashmap[ast::def_id,ast::ident] local_names,
                     @mutable int nvi,
                     &@ast::pat p) {
        alt (p.node) {
            case (ast::pat_bind(?ident, ?did, _)) {
                assign(ccx.tcx, vb, locals, local_names, nvi, did, ident,
                       none[ty::t]);
            }
            case (_) { /* no-op */ }
        }
    }

    auto visit =
        rec(visit_decl_pre=bind visit_decl_pre(ccx, vb, locals, local_names,
                                               nvi, _),
            visit_pat_pre=bind visit_pat_pre(ccx, vb, locals, local_names,
                                             nvi, _)
            with walk::default_visitor());
    walk::walk_block(visit, body);

    ret rec(
        var_bindings=vb,
        locals=locals,
        local_names=local_names,
        next_var_id=*nvi
    );
}


// AST fragment utilities

fn replace_expr_type(&@fn_ctxt fcx,
                     &@ast::expr expr,
                     &tup(vec[ty::t], ty::t) new_tyt) {
    auto new_tps;
    if (ty::expr_has_ty_params(fcx.ccx.tcx, expr)) {
        new_tps = some[vec[ty::t]](new_tyt._0);
    } else {
        new_tps = none[vec[ty::t]];
    }

    write::ty_fixup(fcx, ty::expr_ann(expr).id, tup(new_tps, new_tyt._1));
}

fn replace_node_type_only(&ty::ctxt tcx, uint fixup, ty::t new_t) {
    auto fixup_opt = tcx.node_types.(fixup);
    auto tps = option::get[ty::ty_param_substs_opt_and_ty](fixup_opt)._0;
    tcx.node_types.(fixup) =
        some[ty::ty_param_substs_opt_and_ty](tup(tps, new_t));
}


// AST fragment checking

fn check_lit(@crate_ctxt ccx, &@ast::lit lit) -> ty::t {
    alt (lit.node) {
        case (ast::lit_str(_, ast::sk_rc))     { ret ty::mk_str(ccx.tcx); }
        case (ast::lit_str(_, ast::sk_unique)) { ret ty::mk_istr(ccx.tcx); }
        case (ast::lit_char(_))             { ret ty::mk_char(ccx.tcx); }
        case (ast::lit_int(_))              { ret ty::mk_int(ccx.tcx);  }
        case (ast::lit_float(_))            { ret ty::mk_float(ccx.tcx);  }
        case (ast::lit_mach_float(?tm, _))  { ret ty::mk_mach(ccx.tcx, tm); }
        case (ast::lit_uint(_))             { ret ty::mk_uint(ccx.tcx); }
        case (ast::lit_mach_int(?tm, _))    { ret ty::mk_mach(ccx.tcx, tm); }
        case (ast::lit_nil)                 { ret ty::mk_nil(ccx.tcx);  }
        case (ast::lit_bool(_))             { ret ty::mk_bool(ccx.tcx); }
    }
}

// Pattern checking is top-down rather than bottom-up so that bindings get
// their types immediately.
fn check_pat(&@fn_ctxt fcx, &@ast::pat pat, ty::t expected) {
    alt (pat.node) {
        case (ast::pat_wild(?ann)) {
            write::ty_only_fixup(fcx, ann.id, expected);
        }
        case (ast::pat_lit(?lt, ?ann)) {
            auto typ = check_lit(fcx.ccx, lt);
            typ = demand::simple(fcx, pat.span, expected, typ);
            write::ty_only_fixup(fcx, ann.id, typ);
        }
        case (ast::pat_bind(?id, ?def_id, ?ann)) {
            auto vid = fcx.locals.get(def_id);
            auto typ = ty::mk_var(fcx.ccx.tcx, vid);
            typ = demand::simple(fcx, pat.span, expected, typ);
            write::ty_only_fixup(fcx, ann.id, typ);
        }
        case (ast::pat_tag(?path, ?subpats, ?ann)) {
            // Typecheck the path.
            auto v_def = fcx.ccx.tcx.def_map.get(ann.id);
            auto v_def_ids = ast::variant_def_ids(v_def);

            auto tag_tpt = ty::lookup_item_type(fcx.ccx.tcx,
                                                v_def_ids._0);
            auto path_tpot = instantiate_path(fcx, path, tag_tpt, pat.span);

            // Take the tag type params out of `expected`.
            auto expected_tps;
            alt (structure_of(fcx, pat.span, expected)) {
                case (ty::ty_tag(_, ?tps)) { expected_tps = tps; }
                case (_) {
                    // FIXME: Switch expected and actual in this message? I
                    // can never tell.
                    fcx.ccx.tcx.sess.span_err(pat.span,
                        #fmt("mismatched types: expected tag but found %s",
                             ty_to_str(fcx.ccx.tcx, expected)));
                }
            }

            // Unify with the expected tag type.
            auto ctor_ty = ty::ty_param_substs_opt_and_ty_to_monotype(
                fcx.ccx.tcx, path_tpot);
            auto path_tpt = demand::full(fcx, pat.span, expected, ctor_ty,
                                         expected_tps, NO_AUTODEREF);
            path_tpot = tup(some[vec[ty::t]](path_tpt._0), path_tpt._1);

            // Get the number of arguments in this tag variant.
            auto arg_types = variant_arg_types(fcx.ccx, pat.span,
                                               v_def_ids._1, expected_tps);

            auto subpats_len = vec::len[@ast::pat](subpats);

            if (vec::len[ty::t](arg_types) > 0u) {
                // N-ary variant.
                auto arg_len = vec::len[ty::t](arg_types);
                if (arg_len != subpats_len) {
                    // TODO: note definition of tag variant
                    // TODO (issue #448): Wrap a #fmt string over multiple
                    // lines...
                    fcx.ccx.tcx.sess.span_err(pat.span, #fmt(
  "this pattern has %u field%s, but the corresponding variant has %u field%s",
                        subpats_len,
                        if (subpats_len == 1u) { "" } else { "s" },
                        arg_len,
                        if (arg_len == 1u) { "" } else { "s" }));
                }

                // TODO: vec::iter2
                auto i = 0u;
                for (@ast::pat subpat in subpats) {
                    check_pat(fcx, subpat, arg_types.(i));
                    i += 1u;
                }
            } else if (subpats_len > 0u) {
                // TODO: note definition of tag variant
                // TODO (issue #448): Wrap a #fmt string over multiple
                // lines...
                fcx.ccx.tcx.sess.span_err(pat.span, #fmt(
"this pattern has %u field%s, but the corresponding variant has no fields",
                    subpats_len,
                    if (subpats_len == 1u) { "" } else { "s" }));
            }

            write::ty_fixup(fcx, ann.id, path_tpot);
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
                                ccx.tcx.sess.span_err(sp,
                                  "Pure function calls impure function");

                            }
                        }
                }
                case (_) {
                    ccx.tcx.sess.span_err(sp,
                      "Pure function calls unknown function");
                }
            }
        }
    }
}

fn require_pure_function(@crate_ctxt ccx, &ast::def_id d_id, &span sp) -> () {
    alt (get_function_purity(ccx, d_id)) {
        case (ast::impure_fn) {
            ccx.tcx.sess.span_err(sp,
                                  "Found non-predicate in check expression");
        }
        case (_) { ret; }
    }
}

fn check_expr(&@fn_ctxt fcx, &@ast::expr expr) {
    // fcx.ccx.tcx.sess.span_warn(expr.span, "typechecking expr " +
    //                                pretty::pprust::expr_to_str(expr));

    // A generic function to factor out common logic from call and bind
    // expressions.
    fn check_call_or_bind(&@fn_ctxt fcx, &span sp, &@ast::expr f,
                          &vec[option::t[@ast::expr]] args) {
        // Check the function.
        check_expr(fcx, f);

        // Get the function type.
        auto fty = expr_ty(fcx.ccx.tcx, f);

        // Grab the argument types and the return type.
        auto arg_tys;
        alt (structure_of(fcx, sp, fty)) {
            case (ty::ty_fn(_, ?arg_tys_0, _, _, _)) {
                arg_tys = arg_tys_0;
            }
            case (ty::ty_native_fn(_, ?arg_tys_0, _)) {
                arg_tys = arg_tys_0;
            }
            case (_) {
                fcx.ccx.tcx.sess.span_err(f.span, "mismatched types: " +
                    "expected function or native function but found " +
                    ty_to_str(fcx.ccx.tcx, fty));
            }
        }

        // Check that the correct number of arguments were supplied.
        auto expected_arg_count = vec::len[ty::arg](arg_tys);
        auto supplied_arg_count = vec::len[option::t[@ast::expr]](args);
        if (expected_arg_count != supplied_arg_count) {
            fcx.ccx.tcx.sess.span_err(sp,
                #fmt("this function takes %u parameter%s but %u parameter%s \
                     supplied",
                     expected_arg_count,
                     if (expected_arg_count == 1u) { "" } else { "s" },
                     supplied_arg_count,
                     if (supplied_arg_count == 1u) { " was" }
                        else { "s were" }));
        }

        // Check the arguments.
        // TODO: iter2
        auto i = 0u;
        for (option::t[@ast::expr] a_opt in args) {
            alt (a_opt) {
                case (some(?a)) {
                    check_expr(fcx, a);
                    demand::simple(fcx, a.span, arg_tys.(i).ty,
                                   expr_ty(fcx.ccx.tcx, a));
                }
                case (none) { /* no-op */ }
            }
            i += 1u;
        }
    }

    // A generic function for checking assignment expressions
    fn check_assignment(&@fn_ctxt fcx, &span sp, &@ast::expr lhs,
                        &@ast::expr rhs, &ast::ann a) {
        check_expr(fcx, lhs);
        check_expr(fcx, rhs);
        auto typ = demand::simple(fcx, sp,
                                  expr_ty(fcx.ccx.tcx, lhs),
                                  expr_ty(fcx.ccx.tcx, rhs));
        write::ty_only_fixup(fcx, a.id, typ);
    }

    // A generic function for checking call expressions
    fn check_call(&@fn_ctxt fcx, &span sp, &@ast::expr f,
                  &vec[@ast::expr] args) {
        let vec[option::t[@ast::expr]] args_opt_0 = [];
        for (@ast::expr arg in args) {
            args_opt_0 += [some[@ast::expr](arg)];
        }

        // Call the generic checker.
        check_call_or_bind(fcx, sp, f, args_opt_0);
    }

    // A generic function for checking for or for-each loops
    fn check_for_or_for_each(&@fn_ctxt fcx, &@ast::decl decl,
                             &ty::t element_ty, &ast::block body,
                             uint node_id) {
        check_decl_local(fcx, decl);
        check_block(fcx, body);

        // Unify type of decl with element type of the seq
        demand::simple(fcx, decl.span, ty::decl_local_ty(fcx.ccx.tcx,
                                                         decl),
                       element_ty);
        
        auto typ = ty::mk_nil(fcx.ccx.tcx);
        write::ty_only_fixup(fcx, node_id, typ);
    }

    alt (expr.node) {
        case (ast::expr_lit(?lit, ?a)) {
            auto typ = check_lit(fcx.ccx, lit);
            write::ty_only_fixup(fcx, a.id, typ);
        }

        case (ast::expr_binary(?binop, ?lhs, ?rhs, ?a)) {
            check_expr(fcx, lhs);
            check_expr(fcx, rhs);

            auto lhs_t = expr_ty(fcx.ccx.tcx, lhs);

            // FIXME: Binops have a bit more subtlety than this.
            auto t = strip_boxes(fcx, expr.span, lhs_t);
            alt (binop) {
                case (ast::eq) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (ast::lt) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (ast::le) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (ast::ne) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (ast::ge) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (ast::gt) { t = ty::mk_bool(fcx.ccx.tcx); }
                case (_) { /* fall through */ }
            }

            write::ty_only_fixup(fcx, a.id, t);
        }

        case (ast::expr_unary(?unop, ?oper, ?a)) {
            check_expr(fcx, oper);

            auto oper_t = expr_ty(fcx.ccx.tcx, oper);
            alt (unop) {
                case (ast::box(?mut)) {
                    oper_t = ty::mk_box(fcx.ccx.tcx,
                                        rec(ty=oper_t, mut=mut));
                }
                case (ast::deref) {
                    alt (structure_of(fcx, expr.span, oper_t)) {
                        case (ty::ty_box(?inner)) { oper_t = inner.ty; }
                        case (_) {
                            fcx.ccx.tcx.sess.span_err
                                (expr.span,
                                 "dereferencing non-box type: "
                                 + ty_to_str(fcx.ccx.tcx, oper_t));
                        }
                    }
                }
                case (ast::not) {
                    if (!type_is_integral(fcx, oper.span, oper_t) &&
                            structure_of(fcx, oper.span, oper_t)
                                != ty::ty_bool) {
                        fcx.ccx.tcx.sess.span_err(expr.span,
                            #fmt("mismatched types: expected bool or \
                            integer but found %s",
                            ty_to_str(fcx.ccx.tcx, oper_t)));
                    }
                }
                case (_) { oper_t = strip_boxes(fcx, expr.span, oper_t); }
            }

            write::ty_only_fixup(fcx, a.id, oper_t);
        }

        case (ast::expr_path(?pth, ?old_ann)) {
            auto t = ty::mk_nil(fcx.ccx.tcx);
            auto defn = fcx.ccx.tcx.def_map.get(old_ann.id);

            auto tpt = ty_param_count_and_ty_for_def(fcx, expr.span,
                                                     defn);

            if (ty::def_has_ty_params(defn)) {
                auto path_tpot = instantiate_path(fcx, pth, tpt, expr.span);
                write::ty_fixup(fcx, old_ann.id, path_tpot);
                ret;
            }

            // The definition doesn't take type parameters. If the programmer
            // supplied some, that's an error.
            if (vec::len[@ast::ty](pth.node.types) > 0u) {
                fcx.ccx.tcx.sess.span_err(expr.span,
                    "this kind of value does not take type parameters");
            }

            write::ty_only_fixup(fcx, old_ann.id, tpt._1);
        }

        case (ast::expr_ext(?p, ?args, ?body, ?expanded, ?a)) {
            check_expr(fcx, expanded);
            auto t = expr_ty(fcx.ccx.tcx, expanded);
            write::ty_only_fixup(fcx, a.id, t);
        }

        case (ast::expr_fail(?a, _)) {
            write::bot_ty(fcx.ccx.tcx, a.id);
        }

        case (ast::expr_break(?a)) {
            write::bot_ty(fcx.ccx.tcx, a.id);
        }

        case (ast::expr_cont(?a)) {
            write::bot_ty(fcx.ccx.tcx, a.id);
        }

        case (ast::expr_ret(?expr_opt, ?a)) {
            alt (expr_opt) {
                case (none) {
                    auto nil = ty::mk_nil(fcx.ccx.tcx);
                    if (!are_compatible(fcx, fcx.ret_ty, nil)) {
                        fcx.ccx.tcx.sess.span_err(expr.span,
                          "ret; in function returning non-nil");
                    }

                    write::bot_ty(fcx.ccx.tcx, a.id);
                }

                case (some(?e)) {
                    check_expr(fcx, e);
                    demand::simple(fcx, expr.span, fcx.ret_ty,
                                   expr_ty(fcx.ccx.tcx, e));
                    write::bot_ty(fcx.ccx.tcx, a.id);
                }
            }
        }

        case (ast::expr_put(?expr_opt, ?a)) {
            require_impure(fcx.ccx.tcx.sess, fcx.purity, expr.span);

            alt (expr_opt) {
                case (none) {
                    auto nil = ty::mk_nil(fcx.ccx.tcx);
                    if (!are_compatible(fcx, fcx.ret_ty, nil)) {
                         fcx.ccx.tcx.sess.span_err(expr.span,
                            "put; in iterator yielding non-nil");
                    }

                    write::nil_ty(fcx.ccx.tcx, a.id);
                }

                case (some(?e)) {
                    check_expr(fcx, e);
                    write::nil_ty(fcx.ccx.tcx, a.id);
                }
            }
        }

        case (ast::expr_be(?e, ?a)) {
            // FIXME: prove instead of assert
            assert (ast::is_call_expr(e));

            check_expr(fcx, e);
            demand::simple(fcx, e.span, fcx.ret_ty, expr_ty(fcx.ccx.tcx, e));

            write::nil_ty(fcx.ccx.tcx, a.id);
        }

        case (ast::expr_log(?l, ?e, ?a)) {
            auto expr_t = check_expr(fcx, e);
            write::nil_ty(fcx.ccx.tcx, a.id);
        }

        case (ast::expr_check(?e, ?a)) {
            check_expr(fcx, e);
            demand::simple(fcx, expr.span, ty::mk_bool(fcx.ccx.tcx),
                expr_ty(fcx.ccx.tcx, e));
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
                                    fcx.ccx.tcx.sess.span_err(expr.span,
                                       "Constraint args must be "
                                     + "slot variables or literals");
                                }
                            }

                            require_pure_function(fcx.ccx, d_id,
                                                  expr.span);

                            write::nil_ty(fcx.ccx.tcx, a.id);
                        }
                        case (_) {
                           fcx.ccx.tcx.sess.span_err(expr.span,
                             "In a constraint, expected the constraint name "
                           + "to be an explicit name");
                        }
                    }
                }
                case (_) {
                    fcx.ccx.tcx.sess.span_err(expr.span,
                        "check on non-predicate");
                }
            }
        }

        case (ast::expr_assert(?e, ?a)) {
            check_expr(fcx, e);
            auto ety = expr_ty(fcx.ccx.tcx, e);
            demand::simple(fcx, expr.span, ty::mk_bool(fcx.ccx.tcx), ety);

            write::nil_ty(fcx.ccx.tcx, a.id);
        }

        case (ast::expr_move(?lhs, ?rhs, ?a)) {
            require_impure(fcx.ccx.tcx.sess, fcx.purity, expr.span);
            check_assignment(fcx, expr.span, lhs, rhs, a);
        }

        case (ast::expr_assign(?lhs, ?rhs, ?a)) {
            require_impure(fcx.ccx.tcx.sess, fcx.purity, expr.span);
            check_assignment(fcx, expr.span, lhs, rhs, a);
        }

        case (ast::expr_assign_op(?op, ?lhs, ?rhs, ?a)) {
            require_impure(fcx.ccx.tcx.sess, fcx.purity, expr.span);
            check_assignment(fcx, expr.span, lhs, rhs, a);
        }

        case (ast::expr_send(?lhs, ?rhs, ?a)) {
            require_impure(fcx.ccx.tcx.sess, fcx.purity, expr.span);

            check_expr(fcx, lhs);
            check_expr(fcx, rhs);
            auto rhs_t = expr_ty(fcx.ccx.tcx, rhs);

            auto chan_t = ty::mk_chan(fcx.ccx.tcx, rhs_t);

            auto item_t;
            auto lhs_t = expr_ty(fcx.ccx.tcx, lhs);
            alt (structure_of(fcx, expr.span, lhs_t)) {
                case (ty::ty_chan(?it)) { item_t = it; }
                case (_) {
                    fcx.ccx.tcx.sess.span_err(expr.span,
                        #fmt("mismatched types: expected chan but found %s",
                             ty_to_str(fcx.ccx.tcx, lhs_t)));
                }
            }

            write::ty_only_fixup(fcx, a.id, chan_t);
        }

        case (ast::expr_recv(?lhs, ?rhs, ?a)) {
            require_impure(fcx.ccx.tcx.sess, fcx.purity, expr.span);

            check_expr(fcx, lhs);
            check_expr(fcx, rhs);

            auto item_t = expr_ty(fcx.ccx.tcx, lhs);
            auto port_t = ty::mk_port(fcx.ccx.tcx, item_t);
            demand::simple(fcx, expr.span, port_t, expr_ty(fcx.ccx.tcx, rhs));

            write::ty_only_fixup(fcx, a.id, item_t);
        }

        case (ast::expr_if(?cond, ?thn, ?elsopt, ?a)) {
            check_expr(fcx, cond);
            check_block(fcx, thn);

            auto if_t = alt (elsopt) {
                case (some(?els)) {
                    check_expr(fcx, els);

                    auto thn_t = block_ty(fcx.ccx.tcx, thn);
                    auto elsopt_t = expr_ty(fcx.ccx.tcx, els);

                    demand::simple(fcx, expr.span, thn_t, elsopt_t);

                    if (!ty::type_is_bot(fcx.ccx.tcx, elsopt_t)) {
                        elsopt_t
                    } else {
                        thn_t
                    }
                }
                case (none) {
                    ty::mk_nil(fcx.ccx.tcx)
                }
            };

            write::ty_only_fixup(fcx, a.id, if_t);
        }

        case (ast::expr_for(?decl, ?seq, ?body, ?a)) {
            check_expr(fcx, seq);
            alt (structure_of(fcx, expr.span, expr_ty(fcx.ccx.tcx, seq))) {
                // FIXME: I include the check_for_or_each call in 
                // each case because of a bug in typestate.
                // The bug is fixed; once there's a new snapshot,
                // the call can be moved out of the alt expression
                case (ty::ty_vec(?vec_elt_ty)) {
                    auto elt_ty = vec_elt_ty.ty;
                    check_for_or_for_each(fcx, decl, elt_ty, body, a.id);
                }
                case (ty::ty_str) {
                    auto elt_ty = ty::mk_mach(fcx.ccx.tcx, 
                                         util::common::ty_u8);
                    check_for_or_for_each(fcx, decl, elt_ty, body, a.id);
                }
                case (_) {
                    fcx.ccx.tcx.sess.span_err(expr.span,
                      "type of for loop iterator is not a vector or string");
                }
            }
        }

        case (ast::expr_for_each(?decl, ?seq, ?body, ?a)) {
            check_expr(fcx, seq);
            check_for_or_for_each(fcx, decl, expr_ty(fcx.ccx.tcx, seq),
                                  body, a.id);
        }

        case (ast::expr_while(?cond, ?body, ?a)) {
            check_expr(fcx, cond);
            check_block(fcx, body);

            demand::simple(fcx, cond.span, ty::mk_bool(fcx.ccx.tcx),
                           expr_ty(fcx.ccx.tcx, cond));

            auto typ = ty::mk_nil(fcx.ccx.tcx);
            write::ty_only_fixup(fcx, a.id, typ);
        }

        case (ast::expr_do_while(?body, ?cond, ?a)) {
            check_expr(fcx, cond);
            check_block(fcx, body);

            auto typ = block_ty(fcx.ccx.tcx, body);
            write::ty_only_fixup(fcx, a.id, typ);
        }

        case (ast::expr_alt(?expr, ?arms, ?a)) {
            check_expr(fcx, expr);

            // Typecheck the patterns first, so that we get types for all the
            // bindings.
            auto pattern_ty = ty::expr_ty(fcx.ccx.tcx, expr);

            let vec[@ast::pat] pats = [];
            for (ast::arm arm in arms) {
                check_pat(fcx, arm.pat, pattern_ty);
                pats += [arm.pat];
            }

            // Now typecheck the blocks.
            auto result_ty = next_ty_var(fcx);

            let vec[ast::block] blocks = [];
            for (ast::arm arm in arms) {
                check_block(fcx, arm.block);

                auto bty = block_ty(fcx.ccx.tcx, arm.block);
                // Failing alt arms don't need to have a matching type
                if (!ty::type_is_bot(fcx.ccx.tcx, bty)) {
                    result_ty = demand::simple(fcx, arm.block.span,
                                               result_ty, bty);
                }
            }

            write::ty_only_fixup(fcx, a.id, result_ty);
        }

        case (ast::expr_block(?b, ?a)) {
            check_block(fcx, b);
            alt (b.node.expr) {
                case (some(?expr)) {
                    auto typ = expr_ty(fcx.ccx.tcx, expr);
                    write::ty_only_fixup(fcx, a.id, typ);
                }
                case (none) {
                    auto typ = ty::mk_nil(fcx.ccx.tcx);
                    write::ty_only_fixup(fcx, a.id, typ);
                }
            }
        }

        case (ast::expr_bind(?f, ?args, ?a)) {
            // Call the generic checker.
            check_call_or_bind(fcx, expr.span, f, args);

            // Pull the argument and return types out.
            auto proto_1;
            let vec[ty::arg] arg_tys_1 = [];
            auto rt_1;
            auto fty = expr_ty(fcx.ccx.tcx, f);
            auto t_1;
            alt (structure_of(fcx, expr.span, fty)) {
                case (ty::ty_fn(?proto, ?arg_tys, ?rt, ?cf, ?constrs)) {
                    proto_1 = proto;
                    rt_1 = rt;

                    // FIXME:
                    // probably need to munge the constrs to drop constraints
                    // for any bound args

                    // For each blank argument, add the type of that argument
                    // to the resulting function type.
                    auto i = 0u;
                    while (i < vec::len[option::t[@ast::expr]](args)) {
                        alt (args.(i)) {
                            case (some(_)) { /* no-op */ }
                            case (none) {
                                arg_tys_1 += [arg_tys.(i)];
                            }
                        }
                        i += 1u;
                    }
                    t_1 = ty::mk_fn(fcx.ccx.tcx, proto_1, arg_tys_1, rt_1,
                                    cf, constrs);
                }
                case (_) {
                    log_err "LHS of bind expr didn't have a function type?!";
                    fail;
                }
            }
            write::ty_only_fixup(fcx, a.id, t_1);
        }

        case (ast::expr_call(?f, ?args, ?a)) {
            /* here we're kind of hosed, as f can be any expr
             need to restrict it to being an explicit expr_path if we're
            inside a pure function, and need an environment mapping from
            function name onto purity-designation */
            require_pure_call(fcx.ccx, fcx.purity, f, expr.span);

            check_call(fcx, expr.span, f, args);

            // Pull the return type out of the type of the function.
            auto rt_1;
            auto fty = ty::expr_ty(fcx.ccx.tcx, f);
            alt (structure_of(fcx, expr.span, fty)) {
                case (ty::ty_fn(_,_,?rt,_, _))         { rt_1 = rt; }
                case (ty::ty_native_fn(_, _, ?rt))  { rt_1 = rt; }
                case (_) {
                    log_err "LHS of call expr didn't have a function type?!";
                    fail;
                }
            }

            write::ty_only_fixup(fcx, a.id, rt_1);
        }

        case (ast::expr_self_method(?id, ?a)) {
            auto t = ty::mk_nil(fcx.ccx.tcx);
            let ty::t this_obj_ty;

            let option::t[obj_info] this_obj_info = get_obj_info(fcx.ccx);

            alt (this_obj_info) {
                // If we're inside a current object, grab its type.
                case (some(?obj_info)) {
                    // FIXME: In the case of anonymous objects with methods
                    // containing self-calls, this lookup fails because
                    // obj_info.this_obj is not in the type cache
                    this_obj_ty = ty::lookup_item_type(fcx.ccx.tcx, 
                                                       obj_info.this_obj)._1;
                }

                case (none) { fail; }
            }

            // Grab this method's type out of the current object type.
            alt (structure_of(fcx, expr.span, this_obj_ty)) {
                case (ty::ty_obj(?methods)) {
                    for (ty::method method in methods) {
                        if (method.ident == id) {
                            t = ty::method_ty_to_fn_ty(fcx.ccx.tcx,
                                                       method);
                        }
                    }
                }
                case (_) { fail; }
            }

            write::ty_only_fixup(fcx, a.id, t);

            require_impure(fcx.ccx.tcx.sess, fcx.purity, expr.span);
        }

        case (ast::expr_spawn(_, _, ?f, ?args, ?a)) {
            check_call(fcx, expr.span, f, args);

            auto fty = expr_ty(fcx.ccx.tcx, f);
            auto ret_ty = ty::ret_ty_of_fn_ty(fcx.ccx.tcx, fty);

            demand::simple(fcx, f.span, ty::mk_nil(fcx.ccx.tcx), ret_ty);

            // FIXME: Other typechecks needed

            auto typ = ty::mk_task(fcx.ccx.tcx);
            write::ty_only_fixup(fcx, a.id, typ);
        }

        case (ast::expr_cast(?e, ?t, ?a)) {
            check_expr(fcx, e);
            auto t_1 = ast_ty_to_ty_crate(fcx.ccx, t);
            // FIXME: there are more forms of cast to support, eventually.
            if (! (type_is_scalar(fcx, expr.span, expr_ty(fcx.ccx.tcx, e)) &&
                   type_is_scalar(fcx, expr.span, t_1))) {
                fcx.ccx.tcx.sess.span_err(expr.span,
                    "non-scalar cast: " +
                    ty_to_str(fcx.ccx.tcx,
                        expr_ty(fcx.ccx.tcx, e)) +
                    " as " + ty_to_str(fcx.ccx.tcx, t_1));
            }

            write::ty_only_fixup(fcx, a.id, t_1);
        }

        case (ast::expr_vec(?args, ?mut, ?kind, ?a)) {
            let ty::t t;
            if (vec::len[@ast::expr](args) == 0u) {
                t = next_ty_var(fcx);
            } else {
                check_expr(fcx, args.(0));
                t = expr_ty(fcx.ccx.tcx, args.(0));
            }

            for (@ast::expr e in args) {
                check_expr(fcx, e);
                auto expr_t = expr_ty(fcx.ccx.tcx, e);
                demand::simple(fcx, expr.span, t, expr_t);
            }

            auto typ;
            alt (kind) {
                case (ast::sk_rc) {
                    typ = ty::mk_vec(fcx.ccx.tcx, rec(ty=t, mut=mut));
                }
                case (ast::sk_unique) {
                    typ = ty::mk_ivec(fcx.ccx.tcx, rec(ty=t, mut=mut));
                }
            }

            write::ty_only_fixup(fcx, a.id, typ);
        }

        case (ast::expr_tup(?elts, ?a)) {
            let vec[ty::mt] elts_mt = [];

            for (ast::elt e in elts) {
                check_expr(fcx, e.expr);
                auto ety = expr_ty(fcx.ccx.tcx, e.expr);
                elts_mt += [rec(ty=ety, mut=e.mut)];
            }

            auto typ = ty::mk_tup(fcx.ccx.tcx, elts_mt);
            write::ty_only_fixup(fcx, a.id, typ);
        }

        case (ast::expr_rec(?fields, ?base, ?a)) {

            alt (base) {
                case (none) { /* no-op */}
                case (some(?b_0)) { check_expr(fcx, b_0); }
            }

            let vec[field] fields_t = [];

            for (ast::field f in fields) {
                check_expr(fcx, f.node.expr);
                auto expr_t = expr_ty(fcx.ccx.tcx, f.node.expr);

                auto expr_mt = rec(ty=expr_t, mut=f.node.mut);
                vec::push[field](fields_t, rec(ident=f.node.ident,
                                               mt=expr_mt));
            }

            alt (base) {
                case (none) {
                    auto typ = ty::mk_rec(fcx.ccx.tcx, fields_t);
                    write::ty_only_fixup(fcx, a.id, typ);
                }

                case (some(?bexpr)) {
                    check_expr(fcx, bexpr);
                    auto bexpr_t = expr_ty(fcx.ccx.tcx, bexpr);

                    let vec[field] base_fields = [];

                    alt (structure_of(fcx, expr.span, bexpr_t)) {
                        case (ty::ty_rec(?flds)) { base_fields = flds; }
                        case (_) {
                            fcx.ccx.tcx.sess.span_err
                                (expr.span,
                                 "record update non-record base");
                        }
                    }

                    write::ty_only_fixup(fcx, a.id, bexpr_t);

                    for (ty::field f in fields_t) {
                        auto found = false;
                        for (ty::field bf in base_fields) {
                            if (str::eq(f.ident, bf.ident)) {
                                demand::simple(fcx, expr.span, f.mt.ty,
                                               bf.mt.ty);
                                found = true;
                            }
                        }
                        if (!found) {
                            fcx.ccx.tcx.sess.span_err
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
            auto base_t = expr_ty(fcx.ccx.tcx, base);
            base_t = strip_boxes(fcx, expr.span, base_t);
            alt (structure_of(fcx, expr.span, base_t)) {
                case (ty::ty_tup(?args)) {
                    let uint ix = ty::field_num(fcx.ccx.tcx.sess,
                                                expr.span, field);
                    if (ix >= vec::len[ty::mt](args)) {
                        fcx.ccx.tcx.sess.span_err(expr.span,
                                                  "bad index on tuple");
                    }
                    write::ty_only_fixup(fcx, a.id, args.(ix).ty);
                }

                case (ty::ty_rec(?fields)) {
                    let uint ix = ty::field_idx(fcx.ccx.tcx.sess,
                                                expr.span, field, fields);
                    if (ix >= vec::len[ty::field](fields)) {
                        fcx.ccx.tcx.sess.span_err(expr.span,
                                              "bad index on record");
                    }
                    write::ty_only_fixup(fcx, a.id, fields.(ix).mt.ty);
                }

                case (ty::ty_obj(?methods)) {
                    let uint ix = ty::method_idx(fcx.ccx.tcx.sess,
                                                 expr.span, field, methods);

                    if (ix >= vec::len[ty::method](methods)) {
                        fcx.ccx.tcx.sess.span_err(expr.span,
                                                  "bad index on obj");
                    }
                    auto meth = methods.(ix);
                    auto t = ty::mk_fn(fcx.ccx.tcx, meth.proto,
                                       meth.inputs, meth.output, meth.cf,
                                       meth.constrs);
                    write::ty_only_fixup(fcx, a.id, t);
                }

                case (_) {
                    fcx.ccx.tcx.sess.span_unimpl(expr.span,
                        "base type for expr_field in typeck::check_expr: " +
                        ty_to_str(fcx.ccx.tcx, base_t));
                }
            }
        }

        case (ast::expr_index(?base, ?idx, ?a)) {
            check_expr(fcx, base);
            auto base_t = expr_ty(fcx.ccx.tcx, base);
            base_t = strip_boxes(fcx, expr.span, base_t);

            check_expr(fcx, idx);
            auto idx_t = expr_ty(fcx.ccx.tcx, idx);
            alt (structure_of(fcx, expr.span, base_t)) {
                case (ty::ty_vec(?mt)) {
                    if (! type_is_integral(fcx, idx.span, idx_t)) {
                        fcx.ccx.tcx.sess.span_err
                            (idx.span,
                             "non-integral type of vec index: "
                             + ty_to_str(fcx.ccx.tcx, idx_t));
                    }
                    write::ty_only_fixup(fcx, a.id, mt.ty);
                }
                case (ty::ty_str) {
                    if (! type_is_integral(fcx, idx.span, idx_t)) {
                        fcx.ccx.tcx.sess.span_err
                            (idx.span,
                             "non-integral type of str index: "
                             + ty_to_str(fcx.ccx.tcx, idx_t));
                    }
                    auto typ = ty::mk_mach(fcx.ccx.tcx, common::ty_u8);
                    write::ty_only_fixup(fcx, a.id, typ);
                }
                case (_) {
                    fcx.ccx.tcx.sess.span_err
                        (expr.span,
                         "vector-indexing bad type: "
                         + ty_to_str(fcx.ccx.tcx, base_t));
                }
            }
        }

        case (ast::expr_port(?a)) {
            auto t = next_ty_var(fcx);
            auto pt = ty::mk_port(fcx.ccx.tcx, t);
            write::ty_only_fixup(fcx, a.id, pt);
        }

        case (ast::expr_chan(?x, ?a)) {
            check_expr(fcx, x);
            auto port_t = expr_ty(fcx.ccx.tcx, x);
            alt (structure_of(fcx, expr.span, port_t)) {
                case (ty::ty_port(?subtype)) {
                    auto ct = ty::mk_chan(fcx.ccx.tcx, subtype);
                    write::ty_only_fixup(fcx, a.id, ct);
                }
                case (_) {
                    fcx.ccx.tcx.sess.span_err(expr.span,
                        "bad port type: " +
                        ty_to_str(fcx.ccx.tcx, port_t));
                }
            }
        }

        case (ast::expr_anon_obj(?anon_obj, ?tps, ?obj_def_ids, ?a)) {
            // TODO: We probably need to do more work here to be able to
            // handle additional methods that use 'self'

            // We're entering an object, so gather up the info we need.
            let vec[ast::obj_field] fields = [];
            alt (anon_obj.fields) {
                case (none) { }
                case (some(?v)) { fields = v; }
            }
            let ast::def_id di = obj_def_ids.ty;

            vec::push[obj_info](fcx.ccx.obj_infos,
                                rec(obj_fields=fields, this_obj=di));

            // Typecheck 'with_obj', if it exists.
            let option::t[@ast::expr] with_obj = none[@ast::expr];
            alt (anon_obj.with_obj) {
                case (none) { }
                case (some(?e)) {
                    // This had better have object type.  TOOD: report an
                    // error if the user is trying to extend a non-object
                    // with_obj.
                    check_expr(fcx, e);
                }
            }

            // Typecheck the methods.
            for (@ast::method method in anon_obj.methods) {
                check_method(fcx.ccx, method);
            }

            auto t = next_ty_var(fcx);


            // FIXME: These next three functions are largely ripped off from
            // similar ones in collect::.  Is there a better way to do this?

            fn ty_of_arg(@crate_ctxt ccx, &ast::arg a) -> ty::arg {
                auto ty_mode = ast_mode_to_mode(a.mode);
                ret rec(mode=ty_mode, ty=ast_ty_to_ty_crate(ccx, a.ty));
            }

            fn ty_of_method(@crate_ctxt ccx, &@ast::method m) -> ty::method {
                auto convert = bind ast_ty_to_ty_crate(ccx, _);
                auto f = bind ty_of_arg(ccx, _);
                auto inputs = vec::map[ast::arg,arg](f,
                                                     m.node.meth.decl.inputs);
                auto output = convert(m.node.meth.decl.output);
                ret rec(proto=m.node.meth.proto, ident=m.node.ident,
                        inputs=inputs, output=output, cf=m.node.meth.decl.cf,
                        constrs=m.node.meth.decl.constraints);
            }

            fn get_anon_obj_method_types(@crate_ctxt ccx,
                                         &ast::anon_obj anon_obj)
                -> vec[ty::method] {
                ret vec::map[@ast::method,method](bind ty_of_method(ccx, _),
                                                  anon_obj.methods);
            }

            auto methods = get_anon_obj_method_types(fcx.ccx, anon_obj);
            auto ot = ty::mk_obj(fcx.ccx.tcx,
                                 ty::sort_methods(methods));
            write::ty_only_fixup(fcx, a.id, ot);

            // Now remove the info from the stack.
            vec::pop[obj_info](fcx.ccx.obj_infos);
        }

        case (_) {
            fcx.ccx.tcx.sess.unimpl("expr type in typeck::check_expr");
        }
    }
}

fn next_ty_var_id(@fn_ctxt fcx) -> int {
    auto id = fcx.next_var_id;
    fcx.next_var_id += 1;
    ret id;
}

fn next_ty_var(&@fn_ctxt fcx) -> ty::t {
    ret ty::mk_var(fcx.ccx.tcx, next_ty_var_id(fcx));
}

fn get_obj_info(&@crate_ctxt ccx) -> option::t[obj_info] {
    ret vec::last[obj_info](ccx.obj_infos);
}

fn check_decl_initializer(&@fn_ctxt fcx, &ast::def_id lid,
                          &ast::initializer init) {
    check_expr(fcx, init.expr);

    auto lty = ty::mk_var(fcx.ccx.tcx, fcx.locals.get(lid));
    alt (init.op) {
        case (ast::init_assign) {
            demand::simple(fcx, init.expr.span, lty,
                           expr_ty(fcx.ccx.tcx, init.expr));
        }
        case (ast::init_move) {
            demand::simple(fcx, init.expr.span, lty,
                           expr_ty(fcx.ccx.tcx, init.expr));
        }
        case (ast::init_recv) {
            auto port_ty = ty::mk_port(fcx.ccx.tcx, lty);
            demand::simple(fcx, init.expr.span, port_ty,
                           expr_ty(fcx.ccx.tcx, init.expr));
        }
    }
}

fn check_decl_local(&@fn_ctxt fcx, &@ast::decl decl) -> @ast::decl {
    alt (decl.node) {
        case (ast::decl_local(?local)) {
            auto a_res = local.ann;
            auto t = ty::mk_var(fcx.ccx.tcx, fcx.locals.get(local.id));
            write::ty_only_fixup(fcx, a_res.id, t);

            auto initopt = local.init;
            alt (local.init) {
                case (some(?init)) {
                    check_decl_initializer(fcx, local.id, init);
                }
                case (_) { /* fall through */  }
            }
            auto local_1 = @rec(init=initopt, ann=a_res with *local);
            ret @rec(node=ast::decl_local(local_1) with *decl);
        }
    }
}

fn check_stmt(&@fn_ctxt fcx, &@ast::stmt stmt) {
    auto node_id;
    alt (stmt.node) {
        case (ast::stmt_decl(?decl,?a)) {
            node_id = a.id;
            alt (decl.node) {
                case (ast::decl_local(_)) { check_decl_local(fcx, decl); }
                case (ast::decl_item(_)) { /* ignore for now */ }
            }
        }
        case (ast::stmt_expr(?expr,?a)) {
            node_id = a.id;
            check_expr(fcx, expr);
        }
    }

    write::nil_ty(fcx.ccx.tcx, node_id);
}

fn check_block(&@fn_ctxt fcx, &ast::block block) {
    for (@ast::stmt s in block.node.stmts) { check_stmt(fcx, s); }

    alt (block.node.expr) {
        case (none) { write::nil_ty(fcx.ccx.tcx, block.node.a.id); }
        case (some(?e)) {
            check_expr(fcx, e);
            auto ety = expr_ty(fcx.ccx.tcx, e);
            write::ty_only_fixup(fcx, block.node.a.id, ety);
        }
    }

}

fn check_const(&@crate_ctxt ccx, &span sp, &@ast::expr e, &ast::ann ann) {
    // FIXME: this is kinda a kludge; we manufacture a fake function context
    // and statement context for checking the initializer expression.
    auto rty = ann_to_type(ccx.tcx, ann);
    let vec[uint] fixups = [];
    let @fn_ctxt fcx = @rec(ret_ty=rty,
                            purity=ast::pure_fn,
                            var_bindings=ty::unify::mk_var_bindings(),
                            locals=new_def_hash[int](),
                            local_names=new_def_hash[ast::ident](),
                            mutable next_var_id=0,
                            mutable fixups=fixups,
                            ccx=ccx);

    check_expr(fcx, e);
}

fn check_fn(&@crate_ctxt ccx, &ast::fn_decl decl, ast::proto proto,
            &ast::block body, &ast::ann ann) {
    auto gather_result = gather_locals(ccx, decl, body, ann);

    let vec[uint] fixups = [];
    let @fn_ctxt fcx = @rec(ret_ty=ast_ty_to_ty_crate(ccx, decl.output),
                            purity=decl.purity,
                            var_bindings=gather_result.var_bindings,
                            locals=gather_result.locals,
                            local_names=gather_result.local_names,
                            mutable next_var_id=gather_result.next_var_id,
                            mutable fixups=fixups,
                            ccx=ccx);

    // TODO: Make sure the type of the block agrees with the function type.
    check_block(fcx, body);

    alt (decl.purity) {
        case (ast::pure_fn) {
            // per the previous comment, this just checks that the declared
            // type is bool, and trusts that that's the actual return type.
            if (!ty::type_is_bool(ccx.tcx, fcx.ret_ty)) {
              ccx.tcx.sess.span_err(body.span,
                                    "Non-boolean return type in pred");
            }
        }
        case (_) {}
    }

    writeback::resolve_type_vars_in_block(fcx, body);
}

fn check_method(&@crate_ctxt ccx, &@ast::method method) {
    auto m = method.node.meth;
    check_fn(ccx, m.decl, m.proto, m.body, method.node.ann);
}

fn check_item(@crate_ctxt ccx, &@ast::item it) {
    alt (it.node) {
        case (ast::item_const(_, _, ?e, _, ?a)) {
            check_const(ccx, it.span, e, a);
        }
        case (ast::item_fn(_, ?f, _, _, ?a)) {
            check_fn(ccx, f.decl, f.proto, f.body, a);
        }
        case (ast::item_obj(_, ?ob, _, ?obj_def_ids, _)) {
            // We're entering an object, so gather up the info we need.
            let ast::def_id di = obj_def_ids.ty;
            vec::push[obj_info](ccx.obj_infos,
                                rec(obj_fields=ob.fields, this_obj=di));

            // Typecheck the methods.
            for (@ast::method method in ob.methods) {
                check_method(ccx, method);
            }
            option::may[@ast::method](bind check_method(ccx, _), ob.dtor);

            // Now remove the info from the stack.
            vec::pop[obj_info](ccx.obj_infos);
        }
        case (_) { /* nothing to do */ }
    }
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

fn check_crate(&ty::ctxt tcx, &@ast::crate crate) {

    collect::collect_item_types(tcx, crate);

    let vec[obj_info] obj_infos = [];

    auto fpt = mk_fn_purity_table(crate); // use a variation on collect

    auto ccx = @rec(mutable obj_infos=obj_infos,
                    fn_purity_table=fpt,
                    tcx=tcx);

    auto visit = rec(visit_item_pre = bind check_item(ccx, _)
                     with walk::default_visitor());

    walk::walk_crate(visit, *crate);
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
