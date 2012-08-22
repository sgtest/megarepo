import libc::c_uint;
import base::*;
import common::*;
import type_of::*;
import build::*;
import driver::session::session;
import syntax::{ast, ast_map};
import ast_map::{path, path_mod, path_name, node_id_to_str};
import driver::session::expect;
import syntax::ast_util::local_def;
import metadata::csearch;
import back::{link, abi};
import lib::llvm::llvm;
import lib::llvm::{ValueRef, TypeRef};
import lib::llvm::llvm::LLVMGetParam;
import std::map::hashmap;
import util::ppaux::{ty_to_str, tys_to_str};

import syntax::print::pprust::expr_to_str;

/**
The main "translation" pass for methods.  Generates code
for non-monomorphized methods only.  Other methods will
be generated once they are invoked with specific type parameters,
see `trans::base::lval_static_fn()` or `trans::base::monomorphic_fn()`.
*/
fn trans_impl(ccx: @crate_ctxt, path: path, name: ast::ident,
              methods: ~[@ast::method], tps: ~[ast::ty_param]) {
    let _icx = ccx.insn_ctxt("impl::trans_impl");
    if tps.len() > 0u { return; }
    let sub_path = vec::append_one(path, path_name(name));
    for vec::each(methods) |method| {
        if method.tps.len() == 0u {
            let llfn = get_item_val(ccx, method.id);
            let path = vec::append_one(sub_path, path_name(method.ident));
            trans_method(ccx, path, method, none, llfn);
        }
    }
}

/**
Translates a (possibly monomorphized) method body.

# Parameters

- `path`: the path to the method
- `method`: the AST node for the method
- `param_substs`: if this is a generic method, the current values for
  type parameters and so forth, else none
- `llfn`: the LLVM ValueRef for the method
*/
fn trans_method(ccx: @crate_ctxt,
                path: path,
                method: &ast::method,
                param_substs: option<param_substs>,
                llfn: ValueRef) {
    // determine the (monomorphized) type that `self` maps to for this method
    let self_ty = ty::node_id_to_type(ccx.tcx, method.self_id);
    let self_ty = match param_substs {
      none => self_ty,
      some({tys: ref tys, _}) => ty::subst_tps(ccx.tcx, *tys, self_ty)
    };

    // apply any transformations from the explicit self declaration
    let self_arg = match method.self_ty.node {
      ast::sty_static => {
        no_self
      }
      ast::sty_box(_) => {
        impl_self(ty::mk_imm_box(ccx.tcx, self_ty))
      }
      ast::sty_uniq(_) => {
        impl_self(ty::mk_imm_uniq(ccx.tcx, self_ty))
      }
      ast::sty_region(*) => {
        impl_self(ty::mk_imm_ptr(ccx.tcx, self_ty))
      }
      ast::sty_value => {
        impl_owned_self(self_ty)
      }
      ast::sty_by_ref => {
        impl_self(self_ty)
      }
    };

    // generate the actual code
    trans_fn(ccx,
             path,
             method.decl,
             method.body,
             llfn,
             self_arg,
             param_substs,
             method.id);
}

fn trans_self_arg(bcx: block, base: @ast::expr,
                  mentry: typeck::method_map_entry) -> result {
    let _icx = bcx.insn_ctxt("impl::trans_self_arg");
    let basety = expr_ty(bcx, base);
    let mode = ast::expl(mentry.self_mode);
    let mut temp_cleanups = ~[];
    let result = trans_arg_expr(bcx, {mode: mode, ty: basety},
                                T_ptr(type_of::type_of(bcx.ccx(), basety)),
                                base, temp_cleanups, none, mentry.derefs);

    // by-ref self argument should not require cleanup in the case of
    // other arguments failing:
    //assert temp_cleanups == ~[];
    //do vec::iter(temp_cleanups) |c| {
    //    revoke_clean(bcx, c)
    //}

    return result;
}

fn trans_method_callee(bcx: block, callee_id: ast::node_id,
                       self: @ast::expr, mentry: typeck::method_map_entry)
    -> lval_maybe_callee {
    let _icx = bcx.insn_ctxt("impl::trans_method_callee");
    match mentry.origin {
      typeck::method_static(did) => {


        let {bcx, val} = trans_self_arg(bcx, self, mentry);
        {env: self_env(val, node_id_type(bcx, self.id), none,
                       mentry.self_mode)
         with lval_static_fn(bcx, did, callee_id)}
      }
      typeck::method_param({trait_id:trait_id, method_num:off,
                            param_num:p, bound_num:b}) => {
        match check bcx.fcx.param_substs {
          some(substs) => {
            let vtbl = find_vtable_in_fn_ctxt(substs, p, b);
            trans_monomorphized_callee(bcx, callee_id, self, mentry,
                                       trait_id, off, vtbl)
          }
        }
      }
      typeck::method_trait(_, off) => {
        let {bcx, val} = trans_temp_expr(bcx, self);
        let fty = node_id_type(bcx, callee_id);
        let self_ty = node_id_type(bcx, self.id);
        let {bcx, val, _} = autoderef(bcx, self.id, val, self_ty,
                                      uint::max_value);
        trans_trait_callee(bcx, val, fty, off)
      }
    }
}

fn trans_static_method_callee(bcx: block, method_id: ast::def_id,
                              callee_id: ast::node_id) -> lval_maybe_callee {
    let _icx = bcx.insn_ctxt("impl::trans_static_method_callee");
    let ccx = bcx.ccx();

    let mname = if method_id.crate == ast::local_crate {
        match check bcx.tcx().items.get(method_id.node) {
          ast_map::node_trait_method(trait_method, _, _) => {
            ast_util::trait_method_to_ty_method(*trait_method).ident
          }
        }
    } else {
        let path = csearch::get_item_path(bcx.tcx(), method_id);
        match path[path.len()-1] {
          path_name(s) => { s }
          path_mod(_) => { fail ~"path doesn't have a name?" }
        }
    };
    debug!("trans_static_method_callee: method_id=%?, callee_id=%?, \
            name=%s", method_id, callee_id, *mname);

    let vtbls = resolve_vtables_in_fn_ctxt(
        bcx.fcx, ccx.maps.vtable_map.get(callee_id));

    match vtbls[0] { // is index 0 always the one we want?
      typeck::vtable_static(impl_did, impl_substs, sub_origins) => {

        let mth_id = method_with_name(bcx.ccx(), impl_did, mname);
        let n_m_tps = method_ty_param_count(ccx, mth_id, impl_did);
        let node_substs = node_id_type_params(bcx, callee_id);
        let ty_substs
            = vec::append(impl_substs,
                          vec::tailn(node_substs,
                                     node_substs.len() - n_m_tps));

        let lval = lval_static_fn_inner(bcx, mth_id, callee_id, ty_substs,
                                        some(sub_origins));
        {env: null_env,
         val: PointerCast(bcx, lval.val, T_ptr(type_of_fn_from_ty(
             ccx, node_id_type(bcx, callee_id))))
         with lval}
      }
      _ => {
        fail ~"vtable_param left in monomorphized function's vtable substs";
      }
    }
}

fn method_from_methods(ms: ~[@ast::method], name: ast::ident)
    -> ast::def_id {
  local_def(option::get(vec::find(ms, |m| m.ident == name)).id)
}

fn method_with_name(ccx: @crate_ctxt, impl_id: ast::def_id,
                    name: ast::ident) -> ast::def_id {
    if impl_id.crate == ast::local_crate {
        match check ccx.tcx.items.get(impl_id.node) {
          ast_map::node_item(@{node: ast::item_impl(_, _, _, ms), _}, _) => {
            method_from_methods(ms, name)
          }
          ast_map::node_item(@{node:
              ast::item_class(struct_def, _), _}, _) => {
            method_from_methods(struct_def.methods, name)
          }
        }
    } else {
        csearch::get_impl_method(ccx.sess.cstore, impl_id, name)
    }
}

fn method_ty_param_count(ccx: @crate_ctxt, m_id: ast::def_id,
                         i_id: ast::def_id) -> uint {
    if m_id.crate == ast::local_crate {
        match check ccx.tcx.items.get(m_id.node) {
          ast_map::node_method(m, _, _) => vec::len(m.tps),
        }
    } else {
        csearch::get_type_param_count(ccx.sess.cstore, m_id) -
            csearch::get_type_param_count(ccx.sess.cstore, i_id)
    }
}

fn trans_monomorphized_callee(bcx: block, callee_id: ast::node_id,
                              base: @ast::expr,
                              mentry: typeck::method_map_entry,
                              trait_id: ast::def_id, n_method: uint,
                              vtbl: typeck::vtable_origin)
    -> lval_maybe_callee {
    let _icx = bcx.insn_ctxt("impl::trans_monomorphized_callee");
    match vtbl {
      typeck::vtable_static(impl_did, impl_substs, sub_origins) => {
        let ccx = bcx.ccx();
        let mname = ty::trait_methods(ccx.tcx, trait_id)[n_method].ident;
        let mth_id = method_with_name(bcx.ccx(), impl_did, mname);
        let n_m_tps = method_ty_param_count(ccx, mth_id, impl_did);
        let node_substs = node_id_type_params(bcx, callee_id);
        let ty_substs
            = vec::append(impl_substs,
                          vec::tailn(node_substs,
                                     node_substs.len() - n_m_tps));
        let {bcx, val} = trans_self_arg(bcx, base, mentry);
        let lval = lval_static_fn_inner(bcx, mth_id, callee_id, ty_substs,
                                        some(sub_origins));
        {env: self_env(val, node_id_type(bcx, base.id),
                       none, mentry.self_mode),
         val: PointerCast(bcx, lval.val, T_ptr(type_of_fn_from_ty(
             ccx, node_id_type(bcx, callee_id))))
         with lval}
      }
      typeck::vtable_trait(trait_id, tps) => {
        let {bcx, val} = trans_temp_expr(bcx, base);
        let fty = node_id_type(bcx, callee_id);
        trans_trait_callee(bcx, val, fty, n_method)
      }
      typeck::vtable_param(n_param, n_bound) => {
        fail ~"vtable_param left in monomorphized function's vtable substs";
      }
    }
}

// Method callee where the vtable comes from a boxed trait
fn trans_trait_callee(bcx: block, val: ValueRef,
                      callee_ty: ty::t, n_method: uint)
    -> lval_maybe_callee {
    let _icx = bcx.insn_ctxt("impl::trans_trait_callee");
    let ccx = bcx.ccx();
    let vtable = Load(bcx, PointerCast(bcx, GEPi(bcx, val, ~[0u, 0u]),
                                       T_ptr(T_ptr(T_vtable()))));
    let llbox = Load(bcx, GEPi(bcx, val, ~[0u, 1u]));
    // FIXME[impl] I doubt this is alignment-safe (#2534)
    let self = GEPi(bcx, llbox, ~[0u, abi::box_field_body]);
    let env = self_env(self, ty::mk_opaque_box(bcx.tcx()), some(llbox),
                       // XXX: is this bogosity?
                       ast::by_ref);
    let llfty = type_of::type_of_fn_from_ty(ccx, callee_ty);
    let vtable = PointerCast(bcx, vtable,
                             T_ptr(T_array(T_ptr(llfty), n_method + 1u)));
    let mptr = Load(bcx, GEPi(bcx, vtable, ~[0u, n_method]));
    {bcx: bcx, val: mptr, kind: lv_owned, env: env}
}

fn find_vtable_in_fn_ctxt(ps: param_substs, n_param: uint, n_bound: uint)
    -> typeck::vtable_origin {
    let mut vtable_off = n_bound, i = 0u;
    // Vtables are stored in a flat array, finding the right one is
    // somewhat awkward
    for vec::each(*ps.bounds) |bounds| {
        if i >= n_param { break; }
        for vec::each(*bounds) |bound| {
            match bound { ty::bound_trait(_) => vtable_off += 1u, _ => () }
        }
        i += 1u;
    }
    option::get(ps.vtables)[vtable_off]
}

fn resolve_vtables_in_fn_ctxt(fcx: fn_ctxt, vts: typeck::vtable_res)
    -> typeck::vtable_res {
    @vec::map(*vts, |d| resolve_vtable_in_fn_ctxt(fcx, d))
}

// Apply the typaram substitutions in the fn_ctxt to a vtable. This should
// eliminate any vtable_params.
fn resolve_vtable_in_fn_ctxt(fcx: fn_ctxt, vt: typeck::vtable_origin)
    -> typeck::vtable_origin {
    match vt {
      typeck::vtable_static(trait_id, tys, sub) => {
        let tys = match fcx.param_substs {
          some(substs) => {
            vec::map(tys, |t| ty::subst_tps(fcx.ccx.tcx, substs.tys, t))
          }
          _ => tys
        };
        typeck::vtable_static(trait_id, tys,
                              resolve_vtables_in_fn_ctxt(fcx, sub))
      }
      typeck::vtable_param(n_param, n_bound) => {
        match check fcx.param_substs {
          some(substs) => {
            find_vtable_in_fn_ctxt(substs, n_param, n_bound)
          }
        }
      }
      _ => vt
    }
}

fn vtable_id(ccx: @crate_ctxt, origin: typeck::vtable_origin) -> mono_id {
    match check origin {
      typeck::vtable_static(impl_id, substs, sub_vtables) => {
        make_mono_id(ccx, impl_id, substs,
                     if (*sub_vtables).len() == 0u { none }
                     else { some(sub_vtables) }, none)
      }
      typeck::vtable_trait(trait_id, substs) => {
        @{def: trait_id,
          params: vec::map(substs, |t| mono_precise(t, none))}
      }
    }
}

fn get_vtable(ccx: @crate_ctxt, origin: typeck::vtable_origin)
    -> ValueRef {
    let hash_id = vtable_id(ccx, origin);
    match ccx.vtables.find(hash_id) {
      some(val) => val,
      none => match check origin {
        typeck::vtable_static(id, substs, sub_vtables) => {
            make_impl_vtable(ccx, id, substs, sub_vtables)
        }
      }
    }
}

fn make_vtable(ccx: @crate_ctxt, ptrs: ~[ValueRef]) -> ValueRef {
    let _icx = ccx.insn_ctxt("impl::make_vtable");
    let tbl = C_struct(ptrs);
    let vt_gvar = str::as_c_str(ccx.names(~"vtable"), |buf| {
        llvm::LLVMAddGlobal(ccx.llmod, val_ty(tbl), buf)
    });
    llvm::LLVMSetInitializer(vt_gvar, tbl);
    llvm::LLVMSetGlobalConstant(vt_gvar, lib::llvm::True);
    lib::llvm::SetLinkage(vt_gvar, lib::llvm::InternalLinkage);
    vt_gvar
}

fn make_impl_vtable(ccx: @crate_ctxt, impl_id: ast::def_id, substs: ~[ty::t],
                    vtables: typeck::vtable_res) -> ValueRef {
    let _icx = ccx.insn_ctxt("impl::make_impl_vtable");
    let tcx = ccx.tcx;

    // XXX: This should support multiple traits.
    let trt_id = expect(ccx.sess,
                        ty::ty_to_def_id(ty::impl_traits(tcx, impl_id)[0]),
                        || ~"make_impl_vtable: non-trait-type implemented");

    let has_tps = (*ty::lookup_item_type(ccx.tcx, impl_id).bounds).len() > 0u;
    make_vtable(ccx, vec::map(*ty::trait_methods(tcx, trt_id), |im| {
        let fty = ty::subst_tps(tcx, substs, ty::mk_fn(tcx, im.fty));
        if (*im.tps).len() > 0u || ty::type_has_self(fty) {
            C_null(T_ptr(T_nil()))
        } else {
            let mut m_id = method_with_name(ccx, impl_id, im.ident);
            if has_tps {
                // If the method is in another crate, need to make an inlined
                // copy first
                if m_id.crate != ast::local_crate {
                    m_id = maybe_instantiate_inline(ccx, m_id);
                }
                monomorphic_fn(ccx, m_id, substs, some(vtables), none).val
            } else if m_id.crate == ast::local_crate {
                get_item_val(ccx, m_id.node)
            } else {
                trans_external_path(ccx, m_id, fty)
            }
        }
    }))
}

fn trans_cast(bcx: block, val: @ast::expr, id: ast::node_id, dest: dest)
    -> block {
    let _icx = bcx.insn_ctxt("impl::trans_cast");
    if dest == ignore { return trans_expr(bcx, val, ignore); }
    let ccx = bcx.ccx();
    let v_ty = expr_ty(bcx, val);
    let {bcx: bcx, box: llbox, body: body} = malloc_boxed(bcx, v_ty);
    add_clean_free(bcx, llbox, heap_shared);
    let bcx = trans_expr_save_in(bcx, val, body);
    revoke_clean(bcx, llbox);
    let result = get_dest_addr(dest);
    Store(bcx, llbox, PointerCast(bcx, GEPi(bcx, result, ~[0u, 1u]),
                                  T_ptr(val_ty(llbox))));
    let orig = ccx.maps.vtable_map.get(id)[0];
    let orig = resolve_vtable_in_fn_ctxt(bcx.fcx, orig);
    let vtable = get_vtable(bcx.ccx(), orig);
    Store(bcx, vtable, PointerCast(bcx, GEPi(bcx, result, ~[0u, 0u]),
                                   T_ptr(val_ty(vtable))));
    bcx
}
