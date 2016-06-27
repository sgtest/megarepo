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
// Code relating to drop glue.

use std;

use attributes;
use back::symbol_names;
use llvm;
use llvm::{ValueRef, get_param};
use middle::lang_items::ExchangeFreeFnLangItem;
use rustc::ty::subst::{Substs};
use rustc::traits;
use rustc::ty::{self, Ty, TyCtxt, TypeFoldable};
use abi::{Abi, FnType};
use adt;
use adt::GetDtorType; // for tcx.dtor_type()
use base::*;
use build::*;
use callee::{Callee, ArgVals};
use cleanup;
use cleanup::CleanupMethods;
use collector;
use common::*;
use debuginfo::DebugLoc;
use declare;
use expr;
use machine::*;
use monomorphize;
use trans_item::TransItem;
use type_of::{type_of, sizing_type_of, align_of};
use type_::Type;
use value::Value;

use arena::TypedArena;
use syntax_pos::DUMMY_SP;

pub fn trans_exchange_free_dyn<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                                           v: ValueRef,
                                           size: ValueRef,
                                           align: ValueRef,
                                           debug_loc: DebugLoc)
                                           -> Block<'blk, 'tcx> {
    let _icx = push_ctxt("trans_exchange_free");

    let def_id = langcall(bcx.tcx(), None, "", ExchangeFreeFnLangItem);
    let args = [PointerCast(bcx, v, Type::i8p(bcx.ccx())), size, align];
    Callee::def(bcx.ccx(), def_id, bcx.tcx().mk_substs(Substs::empty()))
        .call(bcx, debug_loc, ArgVals(&args), None).bcx
}

pub fn trans_exchange_free<'blk, 'tcx>(cx: Block<'blk, 'tcx>,
                                       v: ValueRef,
                                       size: u64,
                                       align: u32,
                                       debug_loc: DebugLoc)
                                       -> Block<'blk, 'tcx> {
    trans_exchange_free_dyn(cx,
                            v,
                            C_uint(cx.ccx(), size),
                            C_uint(cx.ccx(), align),
                            debug_loc)
}

pub fn trans_exchange_free_ty<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                                          ptr: ValueRef,
                                          content_ty: Ty<'tcx>,
                                          debug_loc: DebugLoc)
                                          -> Block<'blk, 'tcx> {
    assert!(type_is_sized(bcx.ccx().tcx(), content_ty));
    let sizing_type = sizing_type_of(bcx.ccx(), content_ty);
    let content_size = llsize_of_alloc(bcx.ccx(), sizing_type);

    // `Box<ZeroSizeType>` does not allocate.
    if content_size != 0 {
        let content_align = align_of(bcx.ccx(), content_ty);
        trans_exchange_free(bcx, ptr, content_size, content_align, debug_loc)
    } else {
        bcx
    }
}

pub fn type_needs_drop<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                 ty: Ty<'tcx>) -> bool {
    tcx.type_needs_drop_given_env(ty, &tcx.empty_parameter_environment())
}

pub fn get_drop_glue_type<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                    t: Ty<'tcx>) -> Ty<'tcx> {
    assert!(t.is_normalized_for_trans());

    // Even if there is no dtor for t, there might be one deeper down and we
    // might need to pass in the vtable ptr.
    if !type_is_sized(tcx, t) {
        return t;
    }

    // FIXME (#22815): note that type_needs_drop conservatively
    // approximates in some cases and may say a type expression
    // requires drop glue when it actually does not.
    //
    // (In this case it is not clear whether any harm is done, i.e.
    // erroneously returning `t` in some cases where we could have
    // returned `tcx.types.i8` does not appear unsound. The impact on
    // code quality is unknown at this time.)

    if !type_needs_drop(tcx, t) {
        return tcx.types.i8;
    }
    match t.sty {
        ty::TyBox(typ) if !type_needs_drop(tcx, typ)
                         && type_is_sized(tcx, typ) => {
            tcx.normalizing_infer_ctxt(traits::ProjectionMode::Any).enter(|infcx| {
                let layout = t.layout(&infcx).unwrap();
                if layout.size(&tcx.data_layout).bytes() == 0 {
                    // `Box<ZeroSizeType>` does not allocate.
                    tcx.types.i8
                } else {
                    t
                }
            })
        }
        _ => t
    }
}

pub fn drop_ty<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                           v: ValueRef,
                           t: Ty<'tcx>,
                           debug_loc: DebugLoc) -> Block<'blk, 'tcx> {
    drop_ty_core(bcx, v, t, debug_loc, false, None)
}

pub fn drop_ty_core<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                                v: ValueRef,
                                t: Ty<'tcx>,
                                debug_loc: DebugLoc,
                                skip_dtor: bool,
                                drop_hint: Option<cleanup::DropHintValue>)
                                -> Block<'blk, 'tcx> {
    // NB: v is an *alias* of type t here, not a direct value.
    debug!("drop_ty_core(t={:?}, skip_dtor={} drop_hint={:?})", t, skip_dtor, drop_hint);
    let _icx = push_ctxt("drop_ty");
    let mut bcx = bcx;
    if bcx.fcx.type_needs_drop(t) {
        let ccx = bcx.ccx();
        let g = if skip_dtor {
            DropGlueKind::TyContents(t)
        } else {
            DropGlueKind::Ty(t)
        };
        let glue = get_drop_glue_core(ccx, g);
        let glue_type = get_drop_glue_type(ccx.tcx(), t);
        let ptr = if glue_type != t {
            PointerCast(bcx, v, type_of(ccx, glue_type).ptr_to())
        } else {
            v
        };

        match drop_hint {
            Some(drop_hint) => {
                let hint_val = load_ty(bcx, drop_hint.value(), bcx.tcx().types.u8);
                let moved_val =
                    C_integral(Type::i8(bcx.ccx()), adt::DTOR_MOVED_HINT as u64, false);
                let may_need_drop =
                    ICmp(bcx, llvm::IntNE, hint_val, moved_val, DebugLoc::None);
                bcx = with_cond(bcx, may_need_drop, |cx| {
                    Call(cx, glue, &[ptr], debug_loc);
                    cx
                })
            }
            None => {
                // No drop-hint ==> call standard drop glue
                Call(bcx, glue, &[ptr], debug_loc);
            }
        }
    }
    bcx
}

pub fn drop_ty_immediate<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                                     v: ValueRef,
                                     t: Ty<'tcx>,
                                     debug_loc: DebugLoc,
                                     skip_dtor: bool)
                                     -> Block<'blk, 'tcx> {
    let _icx = push_ctxt("drop_ty_immediate");
    let vp = alloc_ty(bcx, t, "");
    call_lifetime_start(bcx, vp);
    store_ty(bcx, v, vp, t);
    let bcx = drop_ty_core(bcx, vp, t, debug_loc, skip_dtor, None);
    call_lifetime_end(bcx, vp);
    bcx
}

pub fn get_drop_glue<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>, t: Ty<'tcx>) -> ValueRef {
    get_drop_glue_core(ccx, DropGlueKind::Ty(t))
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum DropGlueKind<'tcx> {
    /// The normal path; runs the dtor, and then recurs on the contents
    Ty(Ty<'tcx>),
    /// Skips the dtor, if any, for ty; drops the contents directly.
    /// Note that the dtor is only skipped at the most *shallow*
    /// level, namely, an `impl Drop for Ty` itself. So, for example,
    /// if Ty is Newtype(S) then only the Drop impl for Newtype itself
    /// will be skipped, while the Drop impl for S, if any, will be
    /// invoked.
    TyContents(Ty<'tcx>),
}

impl<'tcx> DropGlueKind<'tcx> {
    pub fn ty(&self) -> Ty<'tcx> {
        match *self { DropGlueKind::Ty(t) | DropGlueKind::TyContents(t) => t }
    }

    pub fn map_ty<F>(&self, mut f: F) -> DropGlueKind<'tcx> where F: FnMut(Ty<'tcx>) -> Ty<'tcx>
    {
        match *self {
            DropGlueKind::Ty(t) => DropGlueKind::Ty(f(t)),
            DropGlueKind::TyContents(t) => DropGlueKind::TyContents(f(t)),
        }
    }
}

fn get_drop_glue_core<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                                g: DropGlueKind<'tcx>) -> ValueRef {
    debug!("make drop glue for {:?}", g);
    let g = g.map_ty(|t| get_drop_glue_type(ccx.tcx(), t));
    debug!("drop glue type {:?}", g);
    match ccx.drop_glues().borrow().get(&g) {
        Some(&glue) => return glue,
        _ => { }
    }
    let t = g.ty();

    let tcx = ccx.tcx();
    let sig = ty::FnSig {
        inputs: vec![tcx.mk_mut_ptr(tcx.types.i8)],
        output: ty::FnOutput::FnConverging(tcx.mk_nil()),
        variadic: false,
    };
    // Create a FnType for fn(*mut i8) and substitute the real type in
    // later - that prevents FnType from splitting fat pointers up.
    let mut fn_ty = FnType::new(ccx, Abi::Rust, &sig, &[]);
    fn_ty.args[0].original_ty = type_of(ccx, t).ptr_to();
    let llfnty = fn_ty.llvm_type(ccx);

    // To avoid infinite recursion, don't `make_drop_glue` until after we've
    // added the entry to the `drop_glues` cache.
    if let Some(old_sym) = ccx.available_drop_glues().borrow().get(&g) {
        let llfn = declare::declare_cfn(ccx, &old_sym, llfnty);
        ccx.drop_glues().borrow_mut().insert(g, llfn);
        return llfn;
    };

    let suffix = match g {
        DropGlueKind::Ty(_) => "drop",
        DropGlueKind::TyContents(_) => "drop_contents",
    };

    let fn_nm = symbol_names::internal_name_from_type_and_suffix(ccx, t, suffix);
    assert!(declare::get_defined_value(ccx, &fn_nm).is_none());
    let llfn = declare::declare_cfn(ccx, &fn_nm, llfnty);
    attributes::set_frame_pointer_elimination(ccx, llfn);
    ccx.available_drop_glues().borrow_mut().insert(g, fn_nm);
    ccx.drop_glues().borrow_mut().insert(g, llfn);

    let _s = StatRecorder::new(ccx, format!("drop {:?}", t));

    let (arena, fcx): (TypedArena<_>, FunctionContext);
    arena = TypedArena::new();
    fcx = FunctionContext::new(ccx, llfn, fn_ty, None, &arena);

    let bcx = fcx.init(false, None);

    update_linkage(ccx, llfn, None, OriginalTranslation);

    ccx.stats().n_glues_created.set(ccx.stats().n_glues_created.get() + 1);
    // All glue functions take values passed *by alias*; this is a
    // requirement since in many contexts glue is invoked indirectly and
    // the caller has no idea if it's dealing with something that can be
    // passed by value.
    //
    // llfn is expected be declared to take a parameter of the appropriate
    // type, so we don't need to explicitly cast the function parameter.

    let bcx = make_drop_glue(bcx, get_param(llfn, 0), g);
    fcx.finish(bcx, DebugLoc::None);

    llfn
}

fn trans_struct_drop_flag<'blk, 'tcx>(mut bcx: Block<'blk, 'tcx>,
                                      t: Ty<'tcx>,
                                      struct_data: ValueRef)
                                      -> Block<'blk, 'tcx> {
    assert!(type_is_sized(bcx.tcx(), t), "Precondition: caller must ensure t is sized");

    let repr = adt::represent_type(bcx.ccx(), t);
    let drop_flag = unpack_datum!(bcx, adt::trans_drop_flag_ptr(bcx, &repr, struct_data));
    let loaded = load_ty(bcx, drop_flag.val, bcx.tcx().dtor_type());
    let drop_flag_llty = type_of(bcx.fcx.ccx, bcx.tcx().dtor_type());
    let init_val = C_integral(drop_flag_llty, adt::DTOR_NEEDED as u64, false);

    let bcx = if !bcx.ccx().check_drop_flag_for_sanity() {
        bcx
    } else {
        let drop_flag_llty = type_of(bcx.fcx.ccx, bcx.tcx().dtor_type());
        let done_val = C_integral(drop_flag_llty, adt::DTOR_DONE as u64, false);
        let not_init = ICmp(bcx, llvm::IntNE, loaded, init_val, DebugLoc::None);
        let not_done = ICmp(bcx, llvm::IntNE, loaded, done_val, DebugLoc::None);
        let drop_flag_neither_initialized_nor_cleared =
            And(bcx, not_init, not_done, DebugLoc::None);
        with_cond(bcx, drop_flag_neither_initialized_nor_cleared, |cx| {
            let llfn = cx.ccx().get_intrinsic(&("llvm.debugtrap"));
            Call(cx, llfn, &[], DebugLoc::None);
            cx
        })
    };

    let drop_flag_dtor_needed = ICmp(bcx, llvm::IntEQ, loaded, init_val, DebugLoc::None);
    with_cond(bcx, drop_flag_dtor_needed, |cx| {
        trans_struct_drop(cx, t, struct_data)
    })
}
fn trans_struct_drop<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                                 t: Ty<'tcx>,
                                 v0: ValueRef)
                                 -> Block<'blk, 'tcx>
{
    debug!("trans_struct_drop t: {}", t);
    let tcx = bcx.tcx();
    let mut bcx = bcx;

    let def = t.ty_adt_def().unwrap();

    // Be sure to put the contents into a scope so we can use an invoke
    // instruction to call the user destructor but still call the field
    // destructors if the user destructor panics.
    //
    // FIXME (#14875) panic-in-drop semantics might be unsupported; we
    // might well consider changing below to more direct code.
    let contents_scope = bcx.fcx.push_custom_cleanup_scope();

    // Issue #23611: schedule cleanup of contents, re-inspecting the
    // discriminant (if any) in case of variant swap in drop code.
    bcx.fcx.schedule_drop_adt_contents(cleanup::CustomScope(contents_scope), v0, t);

    let (sized_args, unsized_args);
    let args: &[ValueRef] = if type_is_sized(tcx, t) {
        sized_args = [v0];
        &sized_args
    } else {
        unsized_args = [Load(bcx, expr::get_dataptr(bcx, v0)), Load(bcx, expr::get_meta(bcx, v0))];
        &unsized_args
    };

    let trait_ref = ty::Binder(ty::TraitRef {
        def_id: tcx.lang_items.drop_trait().unwrap(),
        substs: tcx.mk_substs(Substs::empty().with_self_ty(t))
    });
    let vtbl = match fulfill_obligation(bcx.ccx().shared(), DUMMY_SP, trait_ref) {
        traits::VtableImpl(data) => data,
        _ => bug!("dtor for {:?} is not an impl???", t)
    };
    let dtor_did = def.destructor().unwrap();
    bcx = Callee::def(bcx.ccx(), dtor_did, vtbl.substs)
        .call(bcx, DebugLoc::None, ArgVals(args), None).bcx;

    bcx.fcx.pop_and_trans_custom_cleanup_scope(bcx, contents_scope)
}

pub fn size_and_align_of_dst<'blk, 'tcx>(bcx: &BlockAndBuilder<'blk, 'tcx>,
                                         t: Ty<'tcx>, info: ValueRef)
                                         -> (ValueRef, ValueRef) {
    debug!("calculate size of DST: {}; with lost info: {:?}",
           t, Value(info));
    if type_is_sized(bcx.tcx(), t) {
        let sizing_type = sizing_type_of(bcx.ccx(), t);
        let size = llsize_of_alloc(bcx.ccx(), sizing_type);
        let align = align_of(bcx.ccx(), t);
        debug!("size_and_align_of_dst t={} info={:?} size: {} align: {}",
               t, Value(info), size, align);
        let size = C_uint(bcx.ccx(), size);
        let align = C_uint(bcx.ccx(), align);
        return (size, align);
    }
    if bcx.is_unreachable() {
        let llty = Type::int(bcx.ccx());
        return (C_undef(llty), C_undef(llty));
    }
    match t.sty {
        ty::TyStruct(def, substs) => {
            let ccx = bcx.ccx();
            // First get the size of all statically known fields.
            // Don't use type_of::sizing_type_of because that expects t to be sized.
            assert!(!t.is_simd());
            let repr = adt::represent_type(ccx, t);
            let sizing_type = adt::sizing_type_context_of(ccx, &repr, true);
            debug!("DST {} sizing_type: {:?}", t, sizing_type);
            let sized_size = llsize_of_alloc(ccx, sizing_type.prefix());
            let sized_align = llalign_of_min(ccx, sizing_type.prefix());
            debug!("DST {} statically sized prefix size: {} align: {}",
                   t, sized_size, sized_align);
            let sized_size = C_uint(ccx, sized_size);
            let sized_align = C_uint(ccx, sized_align);

            // Recurse to get the size of the dynamically sized field (must be
            // the last field).
            let last_field = def.struct_variant().fields.last().unwrap();
            let field_ty = monomorphize::field_ty(bcx.tcx(), substs, last_field);
            let (unsized_size, unsized_align) = size_and_align_of_dst(bcx, field_ty, info);

            // FIXME (#26403, #27023): We should be adding padding
            // to `sized_size` (to accommodate the `unsized_align`
            // required of the unsized field that follows) before
            // summing it with `sized_size`. (Note that since #26403
            // is unfixed, we do not yet add the necessary padding
            // here. But this is where the add would go.)

            // Return the sum of sizes and max of aligns.
            let mut size = bcx.add(sized_size, unsized_size);

            // Issue #27023: If there is a drop flag, *now* we add 1
            // to the size.  (We can do this without adding any
            // padding because drop flags do not have any alignment
            // constraints.)
            if sizing_type.needs_drop_flag() {
                size = bcx.add(size, C_uint(bcx.ccx(), 1_u64));
            }

            // Choose max of two known alignments (combined value must
            // be aligned according to more restrictive of the two).
            let align = match (const_to_opt_uint(sized_align), const_to_opt_uint(unsized_align)) {
                (Some(sized_align), Some(unsized_align)) => {
                    // If both alignments are constant, (the sized_align should always be), then
                    // pick the correct alignment statically.
                    C_uint(ccx, std::cmp::max(sized_align, unsized_align))
                }
                _ => bcx.select(bcx.icmp(llvm::IntUGT, sized_align, unsized_align),
                                sized_align,
                                unsized_align)
            };

            // Issue #27023: must add any necessary padding to `size`
            // (to make it a multiple of `align`) before returning it.
            //
            // Namely, the returned size should be, in C notation:
            //
            //   `size + ((size & (align-1)) ? align : 0)`
            //
            // emulated via the semi-standard fast bit trick:
            //
            //   `(size + (align-1)) & -align`

            let addend = bcx.sub(align, C_uint(bcx.ccx(), 1_u64));
            let size = bcx.and(bcx.add(size, addend), bcx.neg(align));

            (size, align)
        }
        ty::TyTrait(..) => {
            // info points to the vtable and the second entry in the vtable is the
            // dynamic size of the object.
            let info = bcx.pointercast(info, Type::int(bcx.ccx()).ptr_to());
            let size_ptr = bcx.gepi(info, &[1]);
            let align_ptr = bcx.gepi(info, &[2]);
            (bcx.load(size_ptr), bcx.load(align_ptr))
        }
        ty::TySlice(_) | ty::TyStr => {
            let unit_ty = t.sequence_element_type(bcx.tcx());
            // The info in this case is the length of the str, so the size is that
            // times the unit size.
            let llunit_ty = sizing_type_of(bcx.ccx(), unit_ty);
            let unit_align = llalign_of_min(bcx.ccx(), llunit_ty);
            let unit_size = llsize_of_alloc(bcx.ccx(), llunit_ty);
            (bcx.mul(info, C_uint(bcx.ccx(), unit_size)),
             C_uint(bcx.ccx(), unit_align))
        }
        _ => bug!("Unexpected unsized type, found {}", t)
    }
}

fn make_drop_glue<'blk, 'tcx>(bcx: Block<'blk, 'tcx>, v0: ValueRef, g: DropGlueKind<'tcx>)
                              -> Block<'blk, 'tcx> {
    if collector::collecting_debug_information(bcx.ccx().shared()) {
        bcx.ccx()
           .record_translation_item_as_generated(TransItem::DropGlue(g));
    }

    let t = g.ty();

    let skip_dtor = match g { DropGlueKind::Ty(_) => false, DropGlueKind::TyContents(_) => true };
    // NB: v0 is an *alias* of type t here, not a direct value.
    let _icx = push_ctxt("make_drop_glue");

    // Only drop the value when it ... well, we used to check for
    // non-null, (and maybe we need to continue doing so), but we now
    // must definitely check for special bit-patterns corresponding to
    // the special dtor markings.

    let inttype = Type::int(bcx.ccx());
    let dropped_pattern = C_integral(inttype, adt::DTOR_DONE_U64, false);

    match t.sty {
        ty::TyBox(content_ty) => {
            // Support for TyBox is built-in and its drop glue is
            // special. It may move to library and have Drop impl. As
            // a safe-guard, assert TyBox not used with TyContents.
            assert!(!skip_dtor);
            if !type_is_sized(bcx.tcx(), content_ty) {
                let llval = expr::get_dataptr(bcx, v0);
                let llbox = Load(bcx, llval);
                let llbox_as_usize = PtrToInt(bcx, llbox, Type::int(bcx.ccx()));
                let drop_flag_not_dropped_already =
                    ICmp(bcx, llvm::IntNE, llbox_as_usize, dropped_pattern, DebugLoc::None);
                with_cond(bcx, drop_flag_not_dropped_already, |bcx| {
                    let bcx = drop_ty(bcx, v0, content_ty, DebugLoc::None);
                    let info = expr::get_meta(bcx, v0);
                    let info = Load(bcx, info);
                    let (llsize, llalign) =
                        size_and_align_of_dst(&bcx.build(), content_ty, info);

                    // `Box<ZeroSizeType>` does not allocate.
                    let needs_free = ICmp(bcx,
                                          llvm::IntNE,
                                          llsize,
                                          C_uint(bcx.ccx(), 0u64),
                                          DebugLoc::None);
                    with_cond(bcx, needs_free, |bcx| {
                        trans_exchange_free_dyn(bcx, llbox, llsize, llalign, DebugLoc::None)
                    })
                })
            } else {
                let llval = v0;
                let llbox = Load(bcx, llval);
                let llbox_as_usize = PtrToInt(bcx, llbox, inttype);
                let drop_flag_not_dropped_already =
                    ICmp(bcx, llvm::IntNE, llbox_as_usize, dropped_pattern, DebugLoc::None);
                with_cond(bcx, drop_flag_not_dropped_already, |bcx| {
                    let bcx = drop_ty(bcx, llbox, content_ty, DebugLoc::None);
                    trans_exchange_free_ty(bcx, llbox, content_ty, DebugLoc::None)
                })
            }
        }
        ty::TyStruct(def, _) | ty::TyEnum(def, _) => {
            match (def.dtor_kind(), skip_dtor) {
                (ty::TraitDtor(true), false) => {
                    // FIXME(16758) Since the struct is unsized, it is hard to
                    // find the drop flag (which is at the end of the struct).
                    // Lets just ignore the flag and pretend everything will be
                    // OK.
                    if type_is_sized(bcx.tcx(), t) {
                        trans_struct_drop_flag(bcx, t, v0)
                    } else {
                        // Give the user a heads up that we are doing something
                        // stupid and dangerous.
                        bcx.sess().warn(&format!("Ignoring drop flag in destructor for {} \
                                                 because the struct is unsized. See issue \
                                                 #16758", t));
                        trans_struct_drop(bcx, t, v0)
                    }
                }
                (ty::TraitDtor(false), false) => {
                    trans_struct_drop(bcx, t, v0)
                }
                (ty::NoDtor, _) | (_, true) => {
                    // No dtor? Just the default case
                    iter_structural_ty(bcx, v0, t, |bb, vv, tt| drop_ty(bb, vv, tt, DebugLoc::None))
                }
            }
        }
        ty::TyTrait(..) => {
            // No support in vtable for distinguishing destroying with
            // versus without calling Drop::drop. Assert caller is
            // okay with always calling the Drop impl, if any.
            assert!(!skip_dtor);
            let data_ptr = expr::get_dataptr(bcx, v0);
            let vtable_ptr = Load(bcx, expr::get_meta(bcx, v0));
            let dtor = Load(bcx, vtable_ptr);
            Call(bcx,
                 dtor,
                 &[PointerCast(bcx, Load(bcx, data_ptr), Type::i8p(bcx.ccx()))],
                 DebugLoc::None);
            bcx
        }
        _ => {
            if bcx.fcx.type_needs_drop(t) {
                iter_structural_ty(bcx,
                                   v0,
                                   t,
                                   |bb, vv, tt| drop_ty(bb, vv, tt, DebugLoc::None))
            } else {
                bcx
            }
        }
    }
}
