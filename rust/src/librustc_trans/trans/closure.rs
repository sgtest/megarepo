// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use arena::TypedArena;
use back::link::{self, mangle_internal_name_by_path_and_seq};
use llvm::{ValueRef, get_param, get_params};
use middle::def_id::DefId;
use middle::infer;
use middle::traits::ProjectionMode;
use trans::abi::{Abi, FnType};
use trans::adt;
use trans::attributes;
use trans::base::*;
use trans::build::*;
use trans::callee::{self, ArgVals, Callee};
use trans::cleanup::{CleanupMethods, CustomScope, ScopeId};
use trans::common::*;
use trans::datum::{ByRef, Datum, lvalue_scratch_datum};
use trans::datum::{rvalue_scratch_datum, Rvalue};
use trans::debuginfo::{self, DebugLoc};
use trans::declare;
use trans::expr;
use trans::monomorphize::{Instance};
use trans::value::Value;
use trans::Disr;
use middle::ty::{self, Ty, TyCtxt};
use session::config::FullDebugInfo;

use syntax::ast;

use rustc_front::hir;

use libc::c_uint;

fn load_closure_environment<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                                        closure_def_id: DefId,
                                        arg_scope_id: ScopeId,
                                        id: ast::NodeId) {
    let _icx = push_ctxt("closure::load_closure_environment");
    let kind = kind_for_closure(bcx.ccx(), closure_def_id);

    let env_arg = &bcx.fcx.fn_ty.args[0];
    let mut env_idx = bcx.fcx.fn_ty.ret.is_indirect() as usize;

    // Special case for small by-value selfs.
    let llenv = if kind == ty::ClosureKind::FnOnce && !env_arg.is_indirect() {
        let closure_ty = node_id_type(bcx, id);
        let llenv = rvalue_scratch_datum(bcx, closure_ty, "closure_env").val;
        env_arg.store_fn_arg(&bcx.build(), &mut env_idx, llenv);
        llenv
    } else {
        get_param(bcx.fcx.llfn, env_idx as c_uint)
    };

    // Store the pointer to closure data in an alloca for debug info because that's what the
    // llvm.dbg.declare intrinsic expects
    let env_pointer_alloca = if bcx.sess().opts.debuginfo == FullDebugInfo {
        let alloc = alloca(bcx, val_ty(llenv), "__debuginfo_env_ptr");
        Store(bcx, llenv, alloc);
        Some(alloc)
    } else {
        None
    };

    bcx.tcx().with_freevars(id, |fv| {
        for (i, freevar) in fv.iter().enumerate() {
            let upvar_id = ty::UpvarId { var_id: freevar.def.var_id(),
                                        closure_expr_id: id };
            let upvar_capture = bcx.tcx().upvar_capture(upvar_id).unwrap();
            let mut upvar_ptr = StructGEP(bcx, llenv, i);
            let captured_by_ref = match upvar_capture {
                ty::UpvarCapture::ByValue => false,
                ty::UpvarCapture::ByRef(..) => {
                    upvar_ptr = Load(bcx, upvar_ptr);
                    true
                }
            };
            let node_id = freevar.def.var_id();
            bcx.fcx.llupvars.borrow_mut().insert(node_id, upvar_ptr);

            if kind == ty::ClosureKind::FnOnce && !captured_by_ref {
                let hint = bcx.fcx.lldropflag_hints.borrow().hint_datum(upvar_id.var_id);
                bcx.fcx.schedule_drop_mem(arg_scope_id,
                                        upvar_ptr,
                                        node_id_type(bcx, node_id),
                                        hint)
            }

            if let Some(env_pointer_alloca) = env_pointer_alloca {
                debuginfo::create_captured_var_metadata(
                    bcx,
                    node_id,
                    env_pointer_alloca,
                    i,
                    captured_by_ref,
                    freevar.span);
            }
        }
    })
}

pub enum ClosureEnv {
    NotClosure,
    Closure(DefId, ast::NodeId),
}

impl ClosureEnv {
    pub fn load<'blk,'tcx>(self, bcx: Block<'blk, 'tcx>, arg_scope: ScopeId) {
        if let ClosureEnv::Closure(def_id, id) = self {
            load_closure_environment(bcx, def_id, arg_scope, id);
        }
    }
}

fn get_self_type<'tcx>(tcx: &TyCtxt<'tcx>,
                       closure_id: DefId,
                       fn_ty: Ty<'tcx>)
                       -> Ty<'tcx> {
    match tcx.closure_kind(closure_id) {
        ty::ClosureKind::Fn => {
            tcx.mk_imm_ref(tcx.mk_region(ty::ReStatic), fn_ty)
        }
        ty::ClosureKind::FnMut => {
            tcx.mk_mut_ref(tcx.mk_region(ty::ReStatic), fn_ty)
        }
        ty::ClosureKind::FnOnce => fn_ty,
    }
}

/// Returns the LLVM function declaration for a closure, creating it if
/// necessary. If the ID does not correspond to a closure ID, returns None.
fn get_or_create_closure_declaration<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                                               closure_id: DefId,
                                               substs: &ty::ClosureSubsts<'tcx>)
                                               -> ValueRef {
    // Normalize type so differences in regions and typedefs don't cause
    // duplicate declarations
    let tcx = ccx.tcx();
    let substs = tcx.erase_regions(substs);
    let instance = Instance {
        def: closure_id,
        params: &substs.func_substs.types
    };

    if let Some(&llfn) = ccx.instances().borrow().get(&instance) {
        debug!("get_or_create_closure_declaration(): found closure {:?}: {:?}",
               instance, Value(llfn));
        return llfn;
    }

    let path = tcx.def_path(closure_id);
    let symbol = mangle_internal_name_by_path_and_seq(path, "closure");

    // Compute the rust-call form of the closure call method.
    let infcx = infer::normalizing_infer_ctxt(tcx, &tcx.tables, ProjectionMode::Any);
    let sig = &infcx.closure_type(closure_id, &substs).sig;
    let sig = tcx.erase_late_bound_regions(sig);
    let sig = infer::normalize_associated_type(tcx, &sig);
    let closure_type = tcx.mk_closure_from_closure_substs(closure_id, Box::new(substs));
    let function_type = tcx.mk_fn_ptr(ty::BareFnTy {
        unsafety: hir::Unsafety::Normal,
        abi: Abi::RustCall,
        sig: ty::Binder(ty::FnSig {
            inputs: Some(get_self_type(tcx, closure_id, closure_type))
                        .into_iter().chain(sig.inputs).collect(),
            output: sig.output,
            variadic: false
        })
    });
    let llfn = declare::define_internal_fn(ccx, &symbol, function_type);

    // set an inline hint for all closures
    attributes::inline(llfn, attributes::InlineAttr::Hint);

    debug!("get_or_create_declaration_if_closure(): inserting new \
            closure {:?}: {:?}",
           instance, Value(llfn));
    ccx.instances().borrow_mut().insert(instance, llfn);

    llfn
}

pub enum Dest<'a, 'tcx: 'a> {
    SaveIn(Block<'a, 'tcx>, ValueRef),
    Ignore(&'a CrateContext<'a, 'tcx>)
}

pub fn trans_closure_expr<'a, 'tcx>(dest: Dest<'a, 'tcx>,
                                    decl: &hir::FnDecl,
                                    body: &hir::Block,
                                    id: ast::NodeId,
                                    closure_def_id: DefId, // (*)
                                    closure_substs: &ty::ClosureSubsts<'tcx>)
                                    -> Option<Block<'a, 'tcx>>
{
    // (*) Note that in the case of inlined functions, the `closure_def_id` will be the
    // defid of the closure in its original crate, whereas `id` will be the id of the local
    // inlined copy.

    let param_substs = closure_substs.func_substs;

    let ccx = match dest {
        Dest::SaveIn(bcx, _) => bcx.ccx(),
        Dest::Ignore(ccx) => ccx
    };
    let tcx = ccx.tcx();
    let _icx = push_ctxt("closure::trans_closure_expr");

    debug!("trans_closure_expr(id={:?}, closure_def_id={:?}, closure_substs={:?})",
           id, closure_def_id, closure_substs);

    let llfn = get_or_create_closure_declaration(ccx, closure_def_id, closure_substs);

    // Get the type of this closure. Use the current `param_substs` as
    // the closure substitutions. This makes sense because the closure
    // takes the same set of type arguments as the enclosing fn, and
    // this function (`trans_closure`) is invoked at the point
    // of the closure expression.

    let infcx = infer::normalizing_infer_ctxt(ccx.tcx(), &ccx.tcx().tables, ProjectionMode::Any);
    let function_type = infcx.closure_type(closure_def_id, closure_substs);

    let sig = tcx.erase_late_bound_regions(&function_type.sig);
    let sig = infer::normalize_associated_type(ccx.tcx(), &sig);

    let closure_type = tcx.mk_closure_from_closure_substs(closure_def_id,
        Box::new(closure_substs.clone()));
    let sig = ty::FnSig {
        inputs: Some(get_self_type(tcx, closure_def_id, closure_type))
                    .into_iter().chain(sig.inputs).collect(),
        output: sig.output,
        variadic: false
    };
    let fn_ty = FnType::new(ccx, Abi::RustCall, &sig, &[]);

    trans_closure(ccx,
                  decl,
                  body,
                  llfn,
                  param_substs,
                  closure_def_id,
                  id,
                  fn_ty,
                  Abi::RustCall,
                  ClosureEnv::Closure(closure_def_id, id));

    // Don't hoist this to the top of the function. It's perfectly legitimate
    // to have a zero-size closure (in which case dest will be `Ignore`) and
    // we must still generate the closure body.
    let (mut bcx, dest_addr) = match dest {
        Dest::SaveIn(bcx, p) => (bcx, p),
        Dest::Ignore(_) => {
            debug!("trans_closure_expr() ignoring result");
            return None;
        }
    };

    let repr = adt::represent_type(ccx, node_id_type(bcx, id));

    // Create the closure.
    tcx.with_freevars(id, |fv| {
        for (i, freevar) in fv.iter().enumerate() {
            let datum = expr::trans_var(bcx, freevar.def);
            let upvar_slot_dest = adt::trans_field_ptr(
                bcx, &repr, adt::MaybeSizedValue::sized(dest_addr), Disr(0), i);
            let upvar_id = ty::UpvarId { var_id: freevar.def.var_id(),
                                        closure_expr_id: id };
            match tcx.upvar_capture(upvar_id).unwrap() {
                ty::UpvarCapture::ByValue => {
                    bcx = datum.store_to(bcx, upvar_slot_dest);
                }
                ty::UpvarCapture::ByRef(..) => {
                    Store(bcx, datum.to_llref(), upvar_slot_dest);
                }
            }
        }
    });
    adt::trans_set_discr(bcx, &repr, dest_addr, Disr(0));

    Some(bcx)
}

pub fn trans_closure_method<'a, 'tcx>(ccx: &'a CrateContext<'a, 'tcx>,
                                      closure_def_id: DefId,
                                      substs: ty::ClosureSubsts<'tcx>,
                                      trait_closure_kind: ty::ClosureKind)
                                      -> ValueRef
{
    // If this is a closure, redirect to it.
    let llfn = get_or_create_closure_declaration(ccx, closure_def_id, &substs);

    // If the closure is a Fn closure, but a FnOnce is needed (etc),
    // then adapt the self type
    let llfn_closure_kind = ccx.tcx().closure_kind(closure_def_id);

    let _icx = push_ctxt("trans_closure_adapter_shim");
    let tcx = ccx.tcx();

    debug!("trans_closure_adapter_shim(llfn_closure_kind={:?}, \
           trait_closure_kind={:?}, llfn={:?})",
           llfn_closure_kind, trait_closure_kind, Value(llfn));

    match (llfn_closure_kind, trait_closure_kind) {
        (ty::ClosureKind::Fn, ty::ClosureKind::Fn) |
        (ty::ClosureKind::FnMut, ty::ClosureKind::FnMut) |
        (ty::ClosureKind::FnOnce, ty::ClosureKind::FnOnce) => {
            // No adapter needed.
            llfn
        }
        (ty::ClosureKind::Fn, ty::ClosureKind::FnMut) => {
            // The closure fn `llfn` is a `fn(&self, ...)`.  We want a
            // `fn(&mut self, ...)`. In fact, at trans time, these are
            // basically the same thing, so we can just return llfn.
            llfn
        }
        (ty::ClosureKind::Fn, ty::ClosureKind::FnOnce) |
        (ty::ClosureKind::FnMut, ty::ClosureKind::FnOnce) => {
            // The closure fn `llfn` is a `fn(&self, ...)` or `fn(&mut
            // self, ...)`.  We want a `fn(self, ...)`. We can produce
            // this by doing something like:
            //
            //     fn call_once(self, ...) { call_mut(&self, ...) }
            //     fn call_once(mut self, ...) { call_mut(&mut self, ...) }
            //
            // These are both the same at trans time.
            trans_fn_once_adapter_shim(ccx, closure_def_id, substs, llfn)
        }
        _ => {
            tcx.sess.bug(&format!("trans_closure_adapter_shim: cannot convert {:?} to {:?}",
                                  llfn_closure_kind,
                                  trait_closure_kind));
        }
    }
}

fn trans_fn_once_adapter_shim<'a, 'tcx>(
    ccx: &'a CrateContext<'a, 'tcx>,
    closure_def_id: DefId,
    substs: ty::ClosureSubsts<'tcx>,
    llreffn: ValueRef)
    -> ValueRef
{
    debug!("trans_fn_once_adapter_shim(closure_def_id={:?}, substs={:?}, llreffn={:?})",
           closure_def_id, substs, Value(llreffn));

    let tcx = ccx.tcx();
    let infcx = infer::normalizing_infer_ctxt(ccx.tcx(), &ccx.tcx().tables, ProjectionMode::Any);

    // Find a version of the closure type. Substitute static for the
    // region since it doesn't really matter.
    let closure_ty = tcx.mk_closure_from_closure_substs(closure_def_id, Box::new(substs.clone()));
    let ref_closure_ty = tcx.mk_imm_ref(tcx.mk_region(ty::ReStatic), closure_ty);

    // Make a version with the type of by-ref closure.
    let ty::ClosureTy { unsafety, abi, mut sig } = infcx.closure_type(closure_def_id, &substs);
    sig.0.inputs.insert(0, ref_closure_ty); // sig has no self type as of yet
    let llref_fn_ty = tcx.mk_fn_ptr(ty::BareFnTy {
        unsafety: unsafety,
        abi: abi,
        sig: sig.clone()
    });
    debug!("trans_fn_once_adapter_shim: llref_fn_ty={:?}",
           llref_fn_ty);


    // Make a version of the closure type with the same arguments, but
    // with argument #0 being by value.
    assert_eq!(abi, Abi::RustCall);
    sig.0.inputs[0] = closure_ty;

    let sig = tcx.erase_late_bound_regions(&sig);
    let sig = infer::normalize_associated_type(ccx.tcx(), &sig);
    let fn_ty = FnType::new(ccx, abi, &sig, &[]);

    let llonce_fn_ty = tcx.mk_fn_ptr(ty::BareFnTy {
        unsafety: unsafety,
        abi: abi,
        sig: ty::Binder(sig)
    });

    // Create the by-value helper.
    let function_name = link::mangle_internal_name_by_type_and_seq(ccx, llonce_fn_ty, "once_shim");
    let lloncefn = declare::define_internal_fn(ccx, &function_name, llonce_fn_ty);

    let (block_arena, fcx): (TypedArena<_>, FunctionContext);
    block_arena = TypedArena::new();
    fcx = FunctionContext::new(ccx, lloncefn, fn_ty, None, substs.func_substs, &block_arena);
    let mut bcx = fcx.init(false, None);


    // the first argument (`self`) will be the (by value) closure env.
    let self_scope = fcx.push_custom_cleanup_scope();
    let self_scope_id = CustomScope(self_scope);

    let mut llargs = get_params(fcx.llfn);
    let mut self_idx = fcx.fn_ty.ret.is_indirect() as usize;
    let env_arg = &fcx.fn_ty.args[0];
    let llenv = if env_arg.is_indirect() {
        Datum::new(llargs[self_idx], closure_ty, Rvalue::new(ByRef))
            .add_clean(&fcx, self_scope_id)
    } else {
        unpack_datum!(bcx, lvalue_scratch_datum(bcx, closure_ty, "self",
                                                InitAlloca::Dropped,
                                                self_scope_id, |bcx, llval| {
            let mut llarg_idx = self_idx;
            env_arg.store_fn_arg(&bcx.build(), &mut llarg_idx, llval);
            bcx.fcx.schedule_lifetime_end(self_scope_id, llval);
            bcx
        })).val
    };

    debug!("trans_fn_once_adapter_shim: env={:?}", Value(llenv));
    // Adjust llargs such that llargs[self_idx..] has the call arguments.
    // For zero-sized closures that means sneaking in a new argument.
    if env_arg.is_ignore() {
        if self_idx > 0 {
            self_idx -= 1;
            llargs[self_idx] = llenv;
        } else {
            llargs.insert(0, llenv);
        }
    } else {
        llargs[self_idx] = llenv;
    }

    let dest =
        fcx.llretslotptr.get().map(
            |_| expr::SaveIn(fcx.get_ret_slot(bcx, "ret_slot")));

    let callee = Callee {
        data: callee::Fn(llreffn),
        ty: llref_fn_ty
    };
    bcx = callee.call(bcx, DebugLoc::None, ArgVals(&llargs[self_idx..]), dest).bcx;

    fcx.pop_and_trans_custom_cleanup_scope(bcx, self_scope);

    fcx.finish(bcx, DebugLoc::None);

    lloncefn
}
