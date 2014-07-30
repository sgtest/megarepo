// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//!
//
// Code relating to taking, dropping, etc as well as type descriptors.


use back::abi;
use back::link::*;
use llvm::{ValueRef, True, get_param};
use llvm;
use middle::lang_items::{FreeFnLangItem, ExchangeFreeFnLangItem};
use middle::subst;
use middle::trans::adt;
use middle::trans::base::*;
use middle::trans::build::*;
use middle::trans::callee;
use middle::trans::cleanup;
use middle::trans::cleanup::CleanupMethods;
use middle::trans::common::*;
use middle::trans::expr;
use middle::trans::machine::*;
use middle::trans::reflect;
use middle::trans::tvec;
use middle::trans::type_::Type;
use middle::trans::type_of::{type_of, sizing_type_of};
use middle::ty;
use util::ppaux::ty_to_short_str;
use util::ppaux;

use arena::TypedArena;
use std::c_str::ToCStr;
use std::cell::Cell;
use libc::c_uint;
use syntax::ast;
use syntax::parse::token;

pub fn trans_free<'a>(cx: &'a Block<'a>, v: ValueRef) -> &'a Block<'a> {
    let _icx = push_ctxt("trans_free");
    callee::trans_lang_call(cx,
        langcall(cx, None, "", FreeFnLangItem),
        [PointerCast(cx, v, Type::i8p(cx.ccx()))],
        Some(expr::Ignore)).bcx
}

fn trans_exchange_free<'a>(cx: &'a Block<'a>, v: ValueRef, size: u64,
                               align: u64) -> &'a Block<'a> {
    let _icx = push_ctxt("trans_exchange_free");
    let ccx = cx.ccx();
    callee::trans_lang_call(cx,
        langcall(cx, None, "", ExchangeFreeFnLangItem),
        [PointerCast(cx, v, Type::i8p(ccx)), C_uint(ccx, size as uint), C_uint(ccx, align as uint)],
        Some(expr::Ignore)).bcx
}

pub fn trans_exchange_free_ty<'a>(bcx: &'a Block<'a>, ptr: ValueRef,
                                  content_ty: ty::t) -> &'a Block<'a> {
    let sizing_type = sizing_type_of(bcx.ccx(), content_ty);
    let content_size = llsize_of_alloc(bcx.ccx(), sizing_type);

    // `Box<ZeroSizeType>` does not allocate.
    if content_size != 0 {
        let content_align = llalign_of_min(bcx.ccx(), sizing_type);
        trans_exchange_free(bcx, ptr, content_size, content_align)
    } else {
        bcx
    }
}

pub fn take_ty<'a>(bcx: &'a Block<'a>, v: ValueRef, t: ty::t)
               -> &'a Block<'a> {
    // NB: v is an *alias* of type t here, not a direct value.
    let _icx = push_ctxt("take_ty");
    match ty::get(t).sty {
        ty::ty_box(_) => incr_refcnt_of_boxed(bcx, v),
        _ if ty::type_is_structural(t)
          && ty::type_needs_drop(bcx.tcx(), t) => {
            iter_structural_ty(bcx, v, t, take_ty)
        }
        _ => bcx
    }
}

pub fn get_drop_glue_type(ccx: &CrateContext, t: ty::t) -> ty::t {
    let tcx = ccx.tcx();
    if !ty::type_needs_drop(tcx, t) {
        return ty::mk_i8();
    }
    match ty::get(t).sty {
        ty::ty_box(typ) if !ty::type_needs_drop(tcx, typ) =>
            ty::mk_box(tcx, ty::mk_i8()),

        ty::ty_uniq(typ) if !ty::type_needs_drop(tcx, typ) => {
            match ty::get(typ).sty {
                ty::ty_vec(_, None) | ty::ty_str | ty::ty_trait(..) => t,
                _ => {
                    let llty = sizing_type_of(ccx, typ);
                    // `Box<ZeroSizeType>` does not allocate.
                    if llsize_of_alloc(ccx, llty) == 0 {
                        ty::mk_i8()
                    } else {
                        ty::mk_uniq(tcx, ty::mk_i8())
                    }
                }
            }
        }
        _ => t
    }
}

pub fn drop_ty<'a>(bcx: &'a Block<'a>, v: ValueRef, t: ty::t)
               -> &'a Block<'a> {
    // NB: v is an *alias* of type t here, not a direct value.
    let _icx = push_ctxt("drop_ty");
    let ccx = bcx.ccx();
    if ty::type_needs_drop(bcx.tcx(), t) {
        let glue = get_drop_glue(ccx, t);
        let glue_type = get_drop_glue_type(ccx, t);
        let ptr = if glue_type != t {
            PointerCast(bcx, v, type_of(ccx, glue_type).ptr_to())
        } else {
            v
        };
        Call(bcx, glue, [ptr], None);
    }
    bcx
}

pub fn drop_ty_immediate<'a>(bcx: &'a Block<'a>, v: ValueRef, t: ty::t)
                         -> &'a Block<'a> {
    let _icx = push_ctxt("drop_ty_immediate");
    let vp = alloca(bcx, type_of(bcx.ccx(), t), "");
    Store(bcx, v, vp);
    drop_ty(bcx, vp, t)
}

pub fn get_drop_glue(ccx: &CrateContext, t: ty::t) -> ValueRef {
    let t = get_drop_glue_type(ccx, t);
    match ccx.drop_glues.borrow().find(&t) {
        Some(&glue) => return glue,
        _ => { }
    }

    let llfnty = Type::glue_fn(ccx, type_of(ccx, t).ptr_to());
    let glue = declare_generic_glue(ccx, t, llfnty, "drop");

    ccx.drop_glues.borrow_mut().insert(t, glue);

    make_generic_glue(ccx, t, glue, make_drop_glue, "drop");

    glue
}

pub fn lazily_emit_visit_glue(ccx: &CrateContext, ti: &tydesc_info) -> ValueRef {
    let _icx = push_ctxt("lazily_emit_visit_glue");

    let llfnty = Type::glue_fn(ccx, type_of(ccx, ti.ty).ptr_to());

    match ti.visit_glue.get() {
        Some(visit_glue) => visit_glue,
        None => {
            debug!("+++ lazily_emit_tydesc_glue VISIT {}", ppaux::ty_to_string(ccx.tcx(), ti.ty));
            let glue_fn = declare_generic_glue(ccx, ti.ty, llfnty, "visit");
            ti.visit_glue.set(Some(glue_fn));
            make_generic_glue(ccx, ti.ty, glue_fn, make_visit_glue, "visit");
            debug!("--- lazily_emit_tydesc_glue VISIT {}", ppaux::ty_to_string(ccx.tcx(), ti.ty));
            glue_fn
        }
    }
}

// See [Note-arg-mode]
pub fn call_visit_glue(bcx: &Block, v: ValueRef, tydesc: ValueRef) {
    let _icx = push_ctxt("call_visit_glue");

    // Select the glue function to call from the tydesc
    let llfn = Load(bcx, GEPi(bcx, tydesc, [0u, abi::tydesc_field_visit_glue]));
    let llrawptr = PointerCast(bcx, v, Type::i8p(bcx.ccx()));

    Call(bcx, llfn, [llrawptr], None);
}

fn make_visit_glue<'a>(bcx: &'a Block<'a>, v: ValueRef, t: ty::t)
                   -> &'a Block<'a> {
    let _icx = push_ctxt("make_visit_glue");
    let mut bcx = bcx;
    let (visitor_trait, object_ty) = match ty::visitor_object_ty(bcx.tcx(),
                                                                 ty::ReStatic) {
        Ok(pair) => pair,
        Err(s) => {
            bcx.tcx().sess.fatal(s.as_slice());
        }
    };
    let v = PointerCast(bcx, v, type_of(bcx.ccx(), object_ty).ptr_to());
    bcx = reflect::emit_calls_to_trait_visit_ty(bcx, t, v, visitor_trait.def_id);
    bcx
}

fn trans_struct_drop_flag<'a>(mut bcx: &'a Block<'a>,
                              t: ty::t,
                              v0: ValueRef,
                              dtor_did: ast::DefId,
                              class_did: ast::DefId,
                              substs: &subst::Substs)
                              -> &'a Block<'a> {
    let repr = adt::represent_type(bcx.ccx(), t);
    let drop_flag = unpack_datum!(bcx, adt::trans_drop_flag_ptr(bcx, &*repr, v0));
    with_cond(bcx, load_ty(bcx, drop_flag.val, ty::mk_bool()), |cx| {
        trans_struct_drop(cx, t, v0, dtor_did, class_did, substs)
    })
}

fn trans_struct_drop<'a>(bcx: &'a Block<'a>,
                         t: ty::t,
                         v0: ValueRef,
                         dtor_did: ast::DefId,
                         class_did: ast::DefId,
                         substs: &subst::Substs)
                         -> &'a Block<'a> {
    let repr = adt::represent_type(bcx.ccx(), t);

    // Find and call the actual destructor
    let dtor_addr = get_res_dtor(bcx.ccx(), dtor_did, t,
                                 class_did, substs);

    // The second argument is the "self" argument for drop
    let params = unsafe {
        let ty = Type::from_ref(llvm::LLVMTypeOf(dtor_addr));
        ty.element_type().func_params()
    };

    adt::fold_variants(bcx, &*repr, v0, |variant_cx, st, value| {
        // Be sure to put all of the fields into a scope so we can use an invoke
        // instruction to call the user destructor but still call the field
        // destructors if the user destructor fails.
        let field_scope = variant_cx.fcx.push_custom_cleanup_scope();

        // Class dtors have no explicit args, so the params should
        // just consist of the environment (self).
        assert_eq!(params.len(), 1);
        let self_arg = PointerCast(variant_cx, value, *params.get(0));
        let args = vec!(self_arg);

        // Add all the fields as a value which needs to be cleaned at the end of
        // this scope.
        for (i, ty) in st.fields.iter().enumerate() {
            let llfld_a = adt::struct_field_ptr(variant_cx, &*st, value, i, false);
            variant_cx.fcx.schedule_drop_mem(cleanup::CustomScope(field_scope),
                                             llfld_a, *ty);
        }

        let dtor_ty = ty::mk_ctor_fn(variant_cx.tcx(), ast::DUMMY_NODE_ID,
                                     [get_drop_glue_type(bcx.ccx(), t)], ty::mk_nil());
        let (_, variant_cx) = invoke(variant_cx, dtor_addr, args, dtor_ty, None);

        variant_cx.fcx.pop_and_trans_custom_cleanup_scope(variant_cx, field_scope);
        variant_cx
    })
}

fn make_drop_glue<'a>(bcx: &'a Block<'a>, v0: ValueRef, t: ty::t) -> &'a Block<'a> {
    // NB: v0 is an *alias* of type t here, not a direct value.
    let _icx = push_ctxt("make_drop_glue");
    match ty::get(t).sty {
        ty::ty_box(body_ty) => {
            decr_refcnt_maybe_free(bcx, v0, body_ty)
        }
        ty::ty_uniq(content_ty) => {
            match ty::get(content_ty).sty {
                ty::ty_vec(mt, None) => {
                    let llbox = Load(bcx, v0);
                    let not_null = IsNotNull(bcx, llbox);
                    with_cond(bcx, not_null, |bcx| {
                        let bcx = tvec::make_drop_glue_unboxed(bcx, llbox, mt.ty);
                        // FIXME: #13994: the old `Box<[T]>` will not support sized deallocation
                        trans_exchange_free(bcx, llbox, 0, 8)
                    })
                }
                ty::ty_str => {
                    let llbox = Load(bcx, v0);
                    let not_null = IsNotNull(bcx, llbox);
                    with_cond(bcx, not_null, |bcx| {
                        let unit_ty = ty::sequence_element_type(bcx.tcx(), t);
                        let bcx = tvec::make_drop_glue_unboxed(bcx, llbox, unit_ty);
                        // FIXME: #13994: the old `Box<str>` will not support sized deallocation
                        trans_exchange_free(bcx, llbox, 0, 8)
                    })
                }
                ty::ty_trait(..) => {
                    let lluniquevalue = GEPi(bcx, v0, [0, abi::trt_field_box]);
                    // Only drop the value when it is non-null
                    with_cond(bcx, IsNotNull(bcx, Load(bcx, lluniquevalue)), |bcx| {
                        let dtor_ptr = Load(bcx, GEPi(bcx, v0, [0, abi::trt_field_vtable]));
                        let dtor = Load(bcx, dtor_ptr);
                        Call(bcx,
                             dtor,
                             [PointerCast(bcx, lluniquevalue, Type::i8p(bcx.ccx()))],
                             None);
                        bcx
                    })
                }
                _ => {
                    let llbox = Load(bcx, v0);
                    let not_null = IsNotNull(bcx, llbox);
                    with_cond(bcx, not_null, |bcx| {
                        let bcx = drop_ty(bcx, llbox, content_ty);
                        trans_exchange_free_ty(bcx, llbox, content_ty)
                    })
                }
            }
        }
        ty::ty_struct(did, ref substs) | ty::ty_enum(did, ref substs) => {
            let tcx = bcx.tcx();
            match ty::ty_dtor(tcx, did) {
                ty::TraitDtor(dtor, true) => {
                    trans_struct_drop_flag(bcx, t, v0, dtor, did, substs)
                }
                ty::TraitDtor(dtor, false) => {
                    trans_struct_drop(bcx, t, v0, dtor, did, substs)
                }
                ty::NoDtor => {
                    // No dtor? Just the default case
                    iter_structural_ty(bcx, v0, t, drop_ty)
                }
            }
        }
        ty::ty_unboxed_closure(..) => iter_structural_ty(bcx, v0, t, drop_ty),
        ty::ty_closure(ref f) if f.store == ty::UniqTraitStore => {
            let box_cell_v = GEPi(bcx, v0, [0u, abi::fn_field_box]);
            let env = Load(bcx, box_cell_v);
            let env_ptr_ty = Type::at_box(bcx.ccx(), Type::i8(bcx.ccx())).ptr_to();
            let env = PointerCast(bcx, env, env_ptr_ty);
            with_cond(bcx, IsNotNull(bcx, env), |bcx| {
                let dtor_ptr = GEPi(bcx, env, [0u, abi::box_field_tydesc]);
                let dtor = Load(bcx, dtor_ptr);
                let cdata = GEPi(bcx, env, [0u, abi::box_field_body]);
                Call(bcx, dtor, [PointerCast(bcx, cdata, Type::i8p(bcx.ccx()))], None);

                // Free the environment itself
                // FIXME: #13994: pass align and size here
                trans_exchange_free(bcx, env, 0, 8)
            })
        }
        _ => {
            if ty::type_needs_drop(bcx.tcx(), t) &&
                ty::type_is_structural(t) {
                iter_structural_ty(bcx, v0, t, drop_ty)
            } else {
                bcx
            }
        }
    }
}

fn decr_refcnt_maybe_free<'a>(bcx: &'a Block<'a>,
                              box_ptr_ptr: ValueRef,
                              t: ty::t) -> &'a Block<'a> {
    let _icx = push_ctxt("decr_refcnt_maybe_free");
    let fcx = bcx.fcx;
    let ccx = bcx.ccx();

    let decr_bcx = fcx.new_temp_block("decr");
    let free_bcx = fcx.new_temp_block("free");
    let next_bcx = fcx.new_temp_block("next");

    let box_ptr = Load(bcx, box_ptr_ptr);
    let llnotnull = IsNotNull(bcx, box_ptr);
    CondBr(bcx, llnotnull, decr_bcx.llbb, next_bcx.llbb);

    let rc_ptr = GEPi(decr_bcx, box_ptr, [0u, abi::box_field_refcnt]);
    let rc = Sub(decr_bcx, Load(decr_bcx, rc_ptr), C_int(ccx, 1));
    Store(decr_bcx, rc, rc_ptr);
    CondBr(decr_bcx, IsNull(decr_bcx, rc), free_bcx.llbb, next_bcx.llbb);

    let v = Load(free_bcx, box_ptr_ptr);
    let body = GEPi(free_bcx, v, [0u, abi::box_field_body]);
    let free_bcx = drop_ty(free_bcx, body, t);
    let free_bcx = trans_free(free_bcx, v);
    Br(free_bcx, next_bcx.llbb);

    next_bcx
}

fn incr_refcnt_of_boxed<'a>(bcx: &'a Block<'a>,
                            box_ptr_ptr: ValueRef) -> &'a Block<'a> {
    let _icx = push_ctxt("incr_refcnt_of_boxed");
    let ccx = bcx.ccx();
    let box_ptr = Load(bcx, box_ptr_ptr);
    let rc_ptr = GEPi(bcx, box_ptr, [0u, abi::box_field_refcnt]);
    let rc = Load(bcx, rc_ptr);
    let rc = Add(bcx, rc, C_int(ccx, 1));
    Store(bcx, rc, rc_ptr);
    bcx
}


// Generates the declaration for (but doesn't emit) a type descriptor.
pub fn declare_tydesc(ccx: &CrateContext, t: ty::t) -> tydesc_info {
    // If emit_tydescs already ran, then we shouldn't be creating any new
    // tydescs.
    assert!(!ccx.finished_tydescs.get());

    let llty = type_of(ccx, t);

    if ccx.sess().count_type_sizes() {
        println!("{}\t{}", llsize_of_real(ccx, llty),
                 ppaux::ty_to_string(ccx.tcx(), t));
    }

    let llsize = llsize_of(ccx, llty);
    let llalign = llalign_of(ccx, llty);
    let name = mangle_internal_name_by_type_and_seq(ccx, t, "tydesc");
    debug!("+++ declare_tydesc {} {}", ppaux::ty_to_string(ccx.tcx(), t), name);
    let gvar = name.as_slice().with_c_str(|buf| {
        unsafe {
            llvm::LLVMAddGlobal(ccx.llmod, ccx.tydesc_type().to_ref(), buf)
        }
    });
    note_unique_llvm_symbol(ccx, name);

    let ty_name = token::intern_and_get_ident(
        ppaux::ty_to_string(ccx.tcx(), t).as_slice());
    let ty_name = C_str_slice(ccx, ty_name);

    debug!("--- declare_tydesc {}", ppaux::ty_to_string(ccx.tcx(), t));
    tydesc_info {
        ty: t,
        tydesc: gvar,
        size: llsize,
        align: llalign,
        name: ty_name,
        visit_glue: Cell::new(None),
    }
}

fn declare_generic_glue(ccx: &CrateContext, t: ty::t, llfnty: Type,
                        name: &str) -> ValueRef {
    let _icx = push_ctxt("declare_generic_glue");
    let fn_nm = mangle_internal_name_by_type_and_seq(
        ccx,
        t,
        format!("glue_{}", name).as_slice());
    debug!("{} is for type {}", fn_nm, ppaux::ty_to_string(ccx.tcx(), t));
    let llfn = decl_cdecl_fn(ccx, fn_nm.as_slice(), llfnty, ty::mk_nil());
    note_unique_llvm_symbol(ccx, fn_nm);
    return llfn;
}

fn make_generic_glue(ccx: &CrateContext,
                     t: ty::t,
                     llfn: ValueRef,
                     helper: <'a> |&'a Block<'a>, ValueRef, ty::t|
                                  -> &'a Block<'a>,
                     name: &str)
                     -> ValueRef {
    let _icx = push_ctxt("make_generic_glue");
    let glue_name = format!("glue {} {}", name, ty_to_short_str(ccx.tcx(), t));
    let _s = StatRecorder::new(ccx, glue_name);

    let arena = TypedArena::new();
    let empty_param_substs = param_substs::empty();
    let fcx = new_fn_ctxt(ccx, llfn, -1, false, ty::mk_nil(),
                          &empty_param_substs, None, &arena, TranslateItems);

    let bcx = init_function(&fcx, false, ty::mk_nil());

    llvm::SetLinkage(llfn, llvm::InternalLinkage);
    ccx.stats.n_glues_created.set(ccx.stats.n_glues_created.get() + 1u);
    // All glue functions take values passed *by alias*; this is a
    // requirement since in many contexts glue is invoked indirectly and
    // the caller has no idea if it's dealing with something that can be
    // passed by value.
    //
    // llfn is expected be declared to take a parameter of the appropriate
    // type, so we don't need to explicitly cast the function parameter.

    let llrawptr0 = get_param(llfn, fcx.arg_pos(0) as c_uint);
    let bcx = helper(bcx, llrawptr0, t);
    finish_fn(&fcx, bcx, ty::mk_nil());

    llfn
}

pub fn emit_tydescs(ccx: &CrateContext) {
    let _icx = push_ctxt("emit_tydescs");
    // As of this point, allow no more tydescs to be created.
    ccx.finished_tydescs.set(true);
    let glue_fn_ty = Type::generic_glue_fn(ccx).ptr_to();
    for (_, ti) in ccx.tydescs.borrow().iter() {
        // Each of the glue functions needs to be cast to a generic type
        // before being put into the tydesc because we only have a singleton
        // tydesc type. Then we'll recast each function to its real type when
        // calling it.
        let drop_glue = unsafe {
            llvm::LLVMConstPointerCast(get_drop_glue(ccx, ti.ty), glue_fn_ty.to_ref())
        };
        ccx.stats.n_real_glues.set(ccx.stats.n_real_glues.get() + 1);
        let visit_glue =
            match ti.visit_glue.get() {
              None => {
                  ccx.stats.n_null_glues.set(ccx.stats.n_null_glues.get() +
                                             1u);
                  C_null(glue_fn_ty)
              }
              Some(v) => {
                unsafe {
                    ccx.stats.n_real_glues.set(ccx.stats.n_real_glues.get() +
                                               1);
                    llvm::LLVMConstPointerCast(v, glue_fn_ty.to_ref())
                }
              }
            };

        let tydesc = C_named_struct(ccx.tydesc_type(),
                                    [ti.size, // size
                                     ti.align, // align
                                     drop_glue, // drop_glue
                                     visit_glue, // visit_glue
                                     ti.name]); // name

        unsafe {
            let gvar = ti.tydesc;
            llvm::LLVMSetInitializer(gvar, tydesc);
            llvm::LLVMSetGlobalConstant(gvar, True);
            llvm::SetLinkage(gvar, llvm::InternalLinkage);
        }
    };
}
