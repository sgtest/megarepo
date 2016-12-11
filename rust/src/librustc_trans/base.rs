// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Translate the completed AST to the LLVM IR.
//!
//! Some functions here, such as trans_block and trans_expr, return a value --
//! the result of the translation to LLVM -- while others, such as trans_fn
//! and trans_item, are called only for the side effect of adding a
//! particular definition to the LLVM IR output we're producing.
//!
//! Hopefully useful general knowledge about trans:
//!
//!   * There's no way to find out the Ty type of a ValueRef.  Doing so
//!     would be "trying to get the eggs out of an omelette" (credit:
//!     pcwalton).  You can, instead, find out its TypeRef by calling val_ty,
//!     but one TypeRef corresponds to many `Ty`s; for instance, tup(int, int,
//!     int) and rec(x=int, y=int, z=int) will have the same TypeRef.

#![allow(non_camel_case_types)]

use super::CrateTranslation;
use super::ModuleLlvm;
use super::ModuleSource;
use super::ModuleTranslation;

use assert_module_sources;
use back::link;
use back::linker::LinkerInfo;
use back::symbol_export::{self, ExportedSymbols};
use llvm::{Linkage, ValueRef, Vector, get_param};
use llvm;
use rustc::hir::def_id::{DefId, LOCAL_CRATE};
use middle::lang_items::{LangItem, ExchangeMallocFnLangItem, StartFnLangItem};
use rustc::ty::subst::Substs;
use rustc::traits;
use rustc::ty::{self, Ty, TyCtxt, TypeFoldable};
use rustc::ty::adjustment::CustomCoerceUnsized;
use rustc::dep_graph::{DepNode, WorkProduct};
use rustc::hir::map as hir_map;
use rustc::util::common::time;
use session::config::{self, NoDebugInfo};
use rustc_incremental::IncrementalHashesMap;
use session::{self, DataTypeKind, Session};
use abi::{self, Abi, FnType};
use adt;
use attributes;
use build::*;
use builder::{Builder, noname};
use callee::{Callee};
use common::{Block, C_bool, C_bytes_in_context, C_i32, C_uint};
use collector::{self, TransItemCollectionMode};
use common::{C_null, C_struct_in_context, C_u64, C_u8, C_undef};
use common::{CrateContext, FunctionContext};
use common::{Result};
use common::{fulfill_obligation};
use common::{type_is_zero_size, val_ty};
use common;
use consts;
use context::{SharedCrateContext, CrateContextList};
use debuginfo::{self, DebugLoc};
use declare;
use machine;
use machine::{llalign_of_min, llsize_of};
use meth;
use mir;
use monomorphize::{self, Instance};
use partitioning::{self, PartitioningStrategy, CodegenUnit};
use symbol_map::SymbolMap;
use symbol_names_test;
use trans_item::{TransItem, DefPathBasedNames};
use type_::Type;
use type_of;
use value::Value;
use Disr;
use util::nodemap::{NodeSet, FxHashMap, FxHashSet};

use arena::TypedArena;
use libc::c_uint;
use std::ffi::{CStr, CString};
use std::cell::{Cell, RefCell};
use std::ptr;
use std::rc::Rc;
use std::str;
use std::i32;
use syntax_pos::{Span, DUMMY_SP};
use syntax::attr;
use rustc::hir;
use rustc::ty::layout::{self, Layout};
use syntax::ast;

thread_local! {
    static TASK_LOCAL_INSN_KEY: RefCell<Option<Vec<&'static str>>> = {
        RefCell::new(None)
    }
}

pub fn with_insn_ctxt<F>(blk: F)
    where F: FnOnce(&[&'static str])
{
    TASK_LOCAL_INSN_KEY.with(move |slot| {
        slot.borrow().as_ref().map(move |s| blk(s));
    })
}

pub fn init_insn_ctxt() {
    TASK_LOCAL_INSN_KEY.with(|slot| {
        *slot.borrow_mut() = Some(Vec::new());
    });
}

pub struct _InsnCtxt {
    _cannot_construct_outside_of_this_module: (),
}

impl Drop for _InsnCtxt {
    fn drop(&mut self) {
        TASK_LOCAL_INSN_KEY.with(|slot| {
            if let Some(ctx) = slot.borrow_mut().as_mut() {
                ctx.pop();
            }
        })
    }
}

pub fn push_ctxt(s: &'static str) -> _InsnCtxt {
    debug!("new InsnCtxt: {}", s);
    TASK_LOCAL_INSN_KEY.with(|slot| {
        if let Some(ctx) = slot.borrow_mut().as_mut() {
            ctx.push(s)
        }
    });
    _InsnCtxt {
        _cannot_construct_outside_of_this_module: (),
    }
}

pub struct StatRecorder<'a, 'tcx: 'a> {
    ccx: &'a CrateContext<'a, 'tcx>,
    name: Option<String>,
    istart: usize,
}

impl<'a, 'tcx> StatRecorder<'a, 'tcx> {
    pub fn new(ccx: &'a CrateContext<'a, 'tcx>, name: String) -> StatRecorder<'a, 'tcx> {
        let istart = ccx.stats().n_llvm_insns.get();
        StatRecorder {
            ccx: ccx,
            name: Some(name),
            istart: istart,
        }
    }
}

impl<'a, 'tcx> Drop for StatRecorder<'a, 'tcx> {
    fn drop(&mut self) {
        if self.ccx.sess().trans_stats() {
            let iend = self.ccx.stats().n_llvm_insns.get();
            self.ccx
                .stats()
                .fn_stats
                .borrow_mut()
                .push((self.name.take().unwrap(), iend - self.istart));
            self.ccx.stats().n_fns.set(self.ccx.stats().n_fns.get() + 1);
            // Reset LLVM insn count to avoid compound costs.
            self.ccx.stats().n_llvm_insns.set(self.istart);
        }
    }
}

pub fn get_meta(bcx: Block, fat_ptr: ValueRef) -> ValueRef {
    StructGEP(bcx, fat_ptr, abi::FAT_PTR_EXTRA)
}

pub fn get_dataptr(bcx: Block, fat_ptr: ValueRef) -> ValueRef {
    StructGEP(bcx, fat_ptr, abi::FAT_PTR_ADDR)
}

pub fn get_meta_builder(b: &Builder, fat_ptr: ValueRef) -> ValueRef {
    b.struct_gep(fat_ptr, abi::FAT_PTR_EXTRA)
}

pub fn get_dataptr_builder(b: &Builder, fat_ptr: ValueRef) -> ValueRef {
    b.struct_gep(fat_ptr, abi::FAT_PTR_ADDR)
}

fn require_alloc_fn<'blk, 'tcx>(bcx: Block<'blk, 'tcx>, info_ty: Ty<'tcx>, it: LangItem) -> DefId {
    match bcx.tcx().lang_items.require(it) {
        Ok(id) => id,
        Err(s) => {
            bcx.sess().fatal(&format!("allocation of `{}` {}", info_ty, s));
        }
    }
}

// The following malloc_raw_dyn* functions allocate a box to contain
// a given type, but with a potentially dynamic size.

pub fn malloc_raw_dyn<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                                  llty_ptr: Type,
                                  info_ty: Ty<'tcx>,
                                  size: ValueRef,
                                  align: ValueRef,
                                  debug_loc: DebugLoc)
                                  -> Result<'blk, 'tcx> {
    let _icx = push_ctxt("malloc_raw_exchange");

    // Allocate space:
    let def_id = require_alloc_fn(bcx, info_ty, ExchangeMallocFnLangItem);
    let r = Callee::def(bcx.ccx(), def_id, bcx.tcx().intern_substs(&[]))
        .call(bcx, debug_loc, &[size, align], None);

    Result::new(r.bcx, PointerCast(r.bcx, r.val, llty_ptr))
}


pub fn bin_op_to_icmp_predicate(op: hir::BinOp_,
                                signed: bool)
                                -> llvm::IntPredicate {
    match op {
        hir::BiEq => llvm::IntEQ,
        hir::BiNe => llvm::IntNE,
        hir::BiLt => if signed { llvm::IntSLT } else { llvm::IntULT },
        hir::BiLe => if signed { llvm::IntSLE } else { llvm::IntULE },
        hir::BiGt => if signed { llvm::IntSGT } else { llvm::IntUGT },
        hir::BiGe => if signed { llvm::IntSGE } else { llvm::IntUGE },
        op => {
            bug!("comparison_op_to_icmp_predicate: expected comparison operator, \
                  found {:?}",
                 op)
        }
    }
}

pub fn bin_op_to_fcmp_predicate(op: hir::BinOp_) -> llvm::RealPredicate {
    match op {
        hir::BiEq => llvm::RealOEQ,
        hir::BiNe => llvm::RealUNE,
        hir::BiLt => llvm::RealOLT,
        hir::BiLe => llvm::RealOLE,
        hir::BiGt => llvm::RealOGT,
        hir::BiGe => llvm::RealOGE,
        op => {
            bug!("comparison_op_to_fcmp_predicate: expected comparison operator, \
                  found {:?}",
                 op);
        }
    }
}

pub fn compare_simd_types<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                                      lhs: ValueRef,
                                      rhs: ValueRef,
                                      t: Ty<'tcx>,
                                      ret_ty: Type,
                                      op: hir::BinOp_,
                                      debug_loc: DebugLoc)
                                      -> ValueRef {
    let signed = match t.sty {
        ty::TyFloat(_) => {
            let cmp = bin_op_to_fcmp_predicate(op);
            return SExt(bcx, FCmp(bcx, cmp, lhs, rhs, debug_loc), ret_ty);
        },
        ty::TyUint(_) => false,
        ty::TyInt(_) => true,
        _ => bug!("compare_simd_types: invalid SIMD type"),
    };

    let cmp = bin_op_to_icmp_predicate(op, signed);
    // LLVM outputs an `< size x i1 >`, so we need to perform a sign extension
    // to get the correctly sized type. This will compile to a single instruction
    // once the IR is converted to assembly if the SIMD instruction is supported
    // by the target architecture.
    SExt(bcx, ICmp(bcx, cmp, lhs, rhs, debug_loc), ret_ty)
}

/// Retrieve the information we are losing (making dynamic) in an unsizing
/// adjustment.
///
/// The `old_info` argument is a bit funny. It is intended for use
/// in an upcast, where the new vtable for an object will be drived
/// from the old one.
pub fn unsized_info<'ccx, 'tcx>(ccx: &CrateContext<'ccx, 'tcx>,
                                source: Ty<'tcx>,
                                target: Ty<'tcx>,
                                old_info: Option<ValueRef>)
                                -> ValueRef {
    let (source, target) = ccx.tcx().struct_lockstep_tails(source, target);
    match (&source.sty, &target.sty) {
        (&ty::TyArray(_, len), &ty::TySlice(_)) => C_uint(ccx, len),
        (&ty::TyDynamic(..), &ty::TyDynamic(..)) => {
            // For now, upcasts are limited to changes in marker
            // traits, and hence never actually require an actual
            // change to the vtable.
            old_info.expect("unsized_info: missing old info for trait upcast")
        }
        (_, &ty::TyDynamic(ref data, ..)) => {
            consts::ptrcast(meth::get_vtable(ccx, source, data.principal()),
                            Type::vtable_ptr(ccx))
        }
        _ => bug!("unsized_info: invalid unsizing {:?} -> {:?}",
                                     source,
                                     target),
    }
}

/// Coerce `src` to `dst_ty`. `src_ty` must be a thin pointer.
pub fn unsize_thin_ptr<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                                   src: ValueRef,
                                   src_ty: Ty<'tcx>,
                                   dst_ty: Ty<'tcx>)
                                   -> (ValueRef, ValueRef) {
    debug!("unsize_thin_ptr: {:?} => {:?}", src_ty, dst_ty);
    match (&src_ty.sty, &dst_ty.sty) {
        (&ty::TyBox(a), &ty::TyBox(b)) |
        (&ty::TyRef(_, ty::TypeAndMut { ty: a, .. }),
         &ty::TyRef(_, ty::TypeAndMut { ty: b, .. })) |
        (&ty::TyRef(_, ty::TypeAndMut { ty: a, .. }),
         &ty::TyRawPtr(ty::TypeAndMut { ty: b, .. })) |
        (&ty::TyRawPtr(ty::TypeAndMut { ty: a, .. }),
         &ty::TyRawPtr(ty::TypeAndMut { ty: b, .. })) => {
            assert!(common::type_is_sized(bcx.tcx(), a));
            let ptr_ty = type_of::in_memory_type_of(bcx.ccx(), b).ptr_to();
            (PointerCast(bcx, src, ptr_ty),
             unsized_info(bcx.ccx(), a, b, None))
        }
        _ => bug!("unsize_thin_ptr: called on bad types"),
    }
}

/// Coerce `src`, which is a reference to a value of type `src_ty`,
/// to a value of type `dst_ty` and store the result in `dst`
pub fn coerce_unsized_into<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                                       src: ValueRef,
                                       src_ty: Ty<'tcx>,
                                       dst: ValueRef,
                                       dst_ty: Ty<'tcx>) {
    match (&src_ty.sty, &dst_ty.sty) {
        (&ty::TyBox(..), &ty::TyBox(..)) |
        (&ty::TyRef(..), &ty::TyRef(..)) |
        (&ty::TyRef(..), &ty::TyRawPtr(..)) |
        (&ty::TyRawPtr(..), &ty::TyRawPtr(..)) => {
            let (base, info) = if common::type_is_fat_ptr(bcx.tcx(), src_ty) {
                // fat-ptr to fat-ptr unsize preserves the vtable
                // i.e. &'a fmt::Debug+Send => &'a fmt::Debug
                // So we need to pointercast the base to ensure
                // the types match up.
                let (base, info) = load_fat_ptr(bcx, src, src_ty);
                let llcast_ty = type_of::fat_ptr_base_ty(bcx.ccx(), dst_ty);
                let base = PointerCast(bcx, base, llcast_ty);
                (base, info)
            } else {
                let base = load_ty(bcx, src, src_ty);
                unsize_thin_ptr(bcx, base, src_ty, dst_ty)
            };
            store_fat_ptr(bcx, base, info, dst, dst_ty);
        }

        (&ty::TyAdt(def_a, substs_a), &ty::TyAdt(def_b, substs_b)) => {
            assert_eq!(def_a, def_b);

            let src_fields = def_a.variants[0].fields.iter().map(|f| {
                monomorphize::field_ty(bcx.tcx(), substs_a, f)
            });
            let dst_fields = def_b.variants[0].fields.iter().map(|f| {
                monomorphize::field_ty(bcx.tcx(), substs_b, f)
            });

            let src = adt::MaybeSizedValue::sized(src);
            let dst = adt::MaybeSizedValue::sized(dst);

            let iter = src_fields.zip(dst_fields).enumerate();
            for (i, (src_fty, dst_fty)) in iter {
                if type_is_zero_size(bcx.ccx(), dst_fty) {
                    continue;
                }

                let src_f = adt::trans_field_ptr(bcx, src_ty, src, Disr(0), i);
                let dst_f = adt::trans_field_ptr(bcx, dst_ty, dst, Disr(0), i);
                if src_fty == dst_fty {
                    memcpy_ty(bcx, dst_f, src_f, src_fty);
                } else {
                    coerce_unsized_into(bcx, src_f, src_fty, dst_f, dst_fty);
                }
            }
        }
        _ => bug!("coerce_unsized_into: invalid coercion {:?} -> {:?}",
                  src_ty,
                  dst_ty),
    }
}

pub fn custom_coerce_unsize_info<'scx, 'tcx>(scx: &SharedCrateContext<'scx, 'tcx>,
                                             source_ty: Ty<'tcx>,
                                             target_ty: Ty<'tcx>)
                                             -> CustomCoerceUnsized {
    let trait_ref = ty::Binder(ty::TraitRef {
        def_id: scx.tcx().lang_items.coerce_unsized_trait().unwrap(),
        substs: scx.tcx().mk_substs_trait(source_ty, &[target_ty])
    });

    match fulfill_obligation(scx, DUMMY_SP, trait_ref) {
        traits::VtableImpl(traits::VtableImplData { impl_def_id, .. }) => {
            scx.tcx().custom_coerce_unsized_kind(impl_def_id)
        }
        vtable => {
            bug!("invalid CoerceUnsized vtable: {:?}", vtable);
        }
    }
}

pub fn cast_shift_expr_rhs(cx: Block, op: hir::BinOp_, lhs: ValueRef, rhs: ValueRef) -> ValueRef {
    cast_shift_rhs(op, lhs, rhs, |a, b| Trunc(cx, a, b), |a, b| ZExt(cx, a, b))
}

pub fn cast_shift_const_rhs(op: hir::BinOp_, lhs: ValueRef, rhs: ValueRef) -> ValueRef {
    cast_shift_rhs(op,
                   lhs,
                   rhs,
                   |a, b| unsafe { llvm::LLVMConstTrunc(a, b.to_ref()) },
                   |a, b| unsafe { llvm::LLVMConstZExt(a, b.to_ref()) })
}

fn cast_shift_rhs<F, G>(op: hir::BinOp_,
                        lhs: ValueRef,
                        rhs: ValueRef,
                        trunc: F,
                        zext: G)
                        -> ValueRef
    where F: FnOnce(ValueRef, Type) -> ValueRef,
          G: FnOnce(ValueRef, Type) -> ValueRef
{
    // Shifts may have any size int on the rhs
    if op.is_shift() {
        let mut rhs_llty = val_ty(rhs);
        let mut lhs_llty = val_ty(lhs);
        if rhs_llty.kind() == Vector {
            rhs_llty = rhs_llty.element_type()
        }
        if lhs_llty.kind() == Vector {
            lhs_llty = lhs_llty.element_type()
        }
        let rhs_sz = rhs_llty.int_width();
        let lhs_sz = lhs_llty.int_width();
        if lhs_sz < rhs_sz {
            trunc(rhs, lhs_llty)
        } else if lhs_sz > rhs_sz {
            // FIXME (#1877: If shifting by negative
            // values becomes not undefined then this is wrong.
            zext(rhs, lhs_llty)
        } else {
            rhs
        }
    } else {
        rhs
    }
}

pub fn invoke<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                          llfn: ValueRef,
                          llargs: &[ValueRef],
                          debug_loc: DebugLoc)
                          -> (ValueRef, Block<'blk, 'tcx>) {
    let _icx = push_ctxt("invoke_");
    if bcx.unreachable.get() {
        return (C_null(Type::i8(bcx.ccx())), bcx);
    }

    if need_invoke(bcx) {
        debug!("invoking {:?} at {:?}", Value(llfn), bcx.llbb);
        for &llarg in llargs {
            debug!("arg: {:?}", Value(llarg));
        }
        let normal_bcx = bcx.fcx.new_block("normal-return");
        let landing_pad = bcx.fcx.get_landing_pad();

        let llresult = Invoke(bcx,
                              llfn,
                              &llargs[..],
                              normal_bcx.llbb,
                              landing_pad,
                              debug_loc);
        return (llresult, normal_bcx);
    } else {
        debug!("calling {:?} at {:?}", Value(llfn), bcx.llbb);
        for &llarg in llargs {
            debug!("arg: {:?}", Value(llarg));
        }

        let llresult = Call(bcx, llfn, &llargs[..], debug_loc);
        return (llresult, bcx);
    }
}

/// Returns whether this session's target will use SEH-based unwinding.
///
/// This is only true for MSVC targets, and even then the 64-bit MSVC target
/// currently uses SEH-ish unwinding with DWARF info tables to the side (same as
/// 64-bit MinGW) instead of "full SEH".
pub fn wants_msvc_seh(sess: &Session) -> bool {
    sess.target.target.options.is_like_msvc
}

pub fn avoid_invoke(bcx: Block) -> bool {
    bcx.sess().no_landing_pads() || bcx.lpad().is_some()
}

pub fn need_invoke(bcx: Block) -> bool {
    if avoid_invoke(bcx) {
        false
    } else {
        bcx.fcx.needs_invoke()
    }
}

pub fn call_assume<'a, 'tcx>(b: &Builder<'a, 'tcx>, val: ValueRef) {
    let assume_intrinsic = b.ccx.get_intrinsic("llvm.assume");
    b.call(assume_intrinsic, &[val], None);
}

/// Helper for loading values from memory. Does the necessary conversion if the in-memory type
/// differs from the type used for SSA values. Also handles various special cases where the type
/// gives us better information about what we are loading.
pub fn load_ty<'blk, 'tcx>(cx: Block<'blk, 'tcx>, ptr: ValueRef, t: Ty<'tcx>) -> ValueRef {
    if cx.unreachable.get() {
        return C_undef(type_of::type_of(cx.ccx(), t));
    }
    load_ty_builder(&B(cx), ptr, t)
}

pub fn load_ty_builder<'a, 'tcx>(b: &Builder<'a, 'tcx>, ptr: ValueRef, t: Ty<'tcx>) -> ValueRef {
    let ccx = b.ccx;
    if type_is_zero_size(ccx, t) {
        return C_undef(type_of::type_of(ccx, t));
    }

    unsafe {
        let global = llvm::LLVMIsAGlobalVariable(ptr);
        if !global.is_null() && llvm::LLVMIsGlobalConstant(global) == llvm::True {
            let val = llvm::LLVMGetInitializer(global);
            if !val.is_null() {
                if t.is_bool() {
                    return llvm::LLVMConstTrunc(val, Type::i1(ccx).to_ref());
                }
                return val;
            }
        }
    }

    if t.is_bool() {
        b.trunc(b.load_range_assert(ptr, 0, 2, llvm::False), Type::i1(ccx))
    } else if t.is_char() {
        // a char is a Unicode codepoint, and so takes values from 0
        // to 0x10FFFF inclusive only.
        b.load_range_assert(ptr, 0, 0x10FFFF + 1, llvm::False)
    } else if (t.is_region_ptr() || t.is_unique()) &&
              !common::type_is_fat_ptr(ccx.tcx(), t) {
        b.load_nonnull(ptr)
    } else {
        b.load(ptr)
    }
}

/// Helper for storing values in memory. Does the necessary conversion if the in-memory type
/// differs from the type used for SSA values.
pub fn store_ty<'blk, 'tcx>(cx: Block<'blk, 'tcx>, v: ValueRef, dst: ValueRef, t: Ty<'tcx>) {
    if cx.unreachable.get() {
        return;
    }

    debug!("store_ty: {:?} : {:?} <- {:?}", Value(dst), t, Value(v));

    if common::type_is_fat_ptr(cx.tcx(), t) {
        let lladdr = ExtractValue(cx, v, abi::FAT_PTR_ADDR);
        let llextra = ExtractValue(cx, v, abi::FAT_PTR_EXTRA);
        store_fat_ptr(cx, lladdr, llextra, dst, t);
    } else {
        Store(cx, from_immediate(cx, v), dst);
    }
}

pub fn store_fat_ptr<'blk, 'tcx>(cx: Block<'blk, 'tcx>,
                                 data: ValueRef,
                                 extra: ValueRef,
                                 dst: ValueRef,
                                 _ty: Ty<'tcx>) {
    // FIXME: emit metadata
    Store(cx, data, get_dataptr(cx, dst));
    Store(cx, extra, get_meta(cx, dst));
}

pub fn load_fat_ptr<'blk, 'tcx>(cx: Block<'blk, 'tcx>,
                                src: ValueRef,
                                ty: Ty<'tcx>)
                                -> (ValueRef, ValueRef)
{
    if cx.unreachable.get() {
        // FIXME: remove me
        return (Load(cx, get_dataptr(cx, src)),
                Load(cx, get_meta(cx, src)));
    }

    load_fat_ptr_builder(&B(cx), src, ty)
}

pub fn load_fat_ptr_builder<'a, 'tcx>(
    b: &Builder<'a, 'tcx>,
    src: ValueRef,
    t: Ty<'tcx>)
    -> (ValueRef, ValueRef)
{

    let ptr = get_dataptr_builder(b, src);
    let ptr = if t.is_region_ptr() || t.is_unique() {
        b.load_nonnull(ptr)
    } else {
        b.load(ptr)
    };

    // FIXME: emit metadata on `meta`.
    let meta = b.load(get_meta_builder(b, src));

    (ptr, meta)
}

pub fn from_immediate(bcx: Block, val: ValueRef) -> ValueRef {
    if val_ty(val) == Type::i1(bcx.ccx()) {
        ZExt(bcx, val, Type::i8(bcx.ccx()))
    } else {
        val
    }
}

pub fn to_immediate(bcx: Block, val: ValueRef, ty: Ty) -> ValueRef {
    if ty.is_bool() {
        Trunc(bcx, val, Type::i1(bcx.ccx()))
    } else {
        val
    }
}

pub fn with_cond<'blk, 'tcx, F>(bcx: Block<'blk, 'tcx>, val: ValueRef, f: F) -> Block<'blk, 'tcx>
    where F: FnOnce(Block<'blk, 'tcx>) -> Block<'blk, 'tcx>
{
    let _icx = push_ctxt("with_cond");

    if bcx.unreachable.get() || common::const_to_opt_uint(val) == Some(0) {
        return bcx;
    }

    let fcx = bcx.fcx;
    let next_cx = fcx.new_block("next");
    let cond_cx = fcx.new_block("cond");
    CondBr(bcx, val, cond_cx.llbb, next_cx.llbb, DebugLoc::None);
    let after_cx = f(cond_cx);
    if !after_cx.terminated.get() {
        Br(after_cx, next_cx.llbb, DebugLoc::None);
    }
    next_cx
}

pub enum Lifetime { Start, End }

// If LLVM lifetime intrinsic support is enabled (i.e. optimizations
// on), and `ptr` is nonzero-sized, then extracts the size of `ptr`
// and the intrinsic for `lt` and passes them to `emit`, which is in
// charge of generating code to call the passed intrinsic on whatever
// block of generated code is targetted for the intrinsic.
//
// If LLVM lifetime intrinsic support is disabled (i.e.  optimizations
// off) or `ptr` is zero-sized, then no-op (does not call `emit`).
fn core_lifetime_emit<'blk, 'tcx, F>(ccx: &'blk CrateContext<'blk, 'tcx>,
                                     ptr: ValueRef,
                                     lt: Lifetime,
                                     emit: F)
    where F: FnOnce(&'blk CrateContext<'blk, 'tcx>, machine::llsize, ValueRef)
{
    if ccx.sess().opts.optimize == config::OptLevel::No {
        return;
    }

    let _icx = push_ctxt(match lt {
        Lifetime::Start => "lifetime_start",
        Lifetime::End => "lifetime_end"
    });

    let size = machine::llsize_of_alloc(ccx, val_ty(ptr).element_type());
    if size == 0 {
        return;
    }

    let lifetime_intrinsic = ccx.get_intrinsic(match lt {
        Lifetime::Start => "llvm.lifetime.start",
        Lifetime::End => "llvm.lifetime.end"
    });
    emit(ccx, size, lifetime_intrinsic)
}

impl Lifetime {
    pub fn call(self, b: &Builder, ptr: ValueRef) {
        core_lifetime_emit(b.ccx, ptr, self, |ccx, size, lifetime_intrinsic| {
            let ptr = b.pointercast(ptr, Type::i8p(ccx));
            b.call(lifetime_intrinsic, &[C_u64(ccx, size), ptr], None);
        });
    }
}

pub fn call_lifetime_start(bcx: Block, ptr: ValueRef) {
    if !bcx.unreachable.get() {
        Lifetime::Start.call(&bcx.build(), ptr);
    }
}

pub fn call_lifetime_end(bcx: Block, ptr: ValueRef) {
    if !bcx.unreachable.get() {
        Lifetime::End.call(&bcx.build(), ptr);
    }
}

// Generates code for resumption of unwind at the end of a landing pad.
pub fn trans_unwind_resume(bcx: Block, lpval: ValueRef) {
    if !bcx.sess().target.target.options.custom_unwind_resume {
        Resume(bcx, lpval);
    } else {
        let exc_ptr = ExtractValue(bcx, lpval, 0);
        bcx.fcx.eh_unwind_resume()
            .call(bcx, DebugLoc::None, &[exc_ptr], None);
    }
}

pub fn call_memcpy<'bcx, 'tcx>(b: &Builder<'bcx, 'tcx>,
                               dst: ValueRef,
                               src: ValueRef,
                               n_bytes: ValueRef,
                               align: u32) {
    let _icx = push_ctxt("call_memcpy");
    let ccx = b.ccx;
    let ptr_width = &ccx.sess().target.target.target_pointer_width[..];
    let key = format!("llvm.memcpy.p0i8.p0i8.i{}", ptr_width);
    let memcpy = ccx.get_intrinsic(&key);
    let src_ptr = b.pointercast(src, Type::i8p(ccx));
    let dst_ptr = b.pointercast(dst, Type::i8p(ccx));
    let size = b.intcast(n_bytes, ccx.int_type());
    let align = C_i32(ccx, align as i32);
    let volatile = C_bool(ccx, false);
    b.call(memcpy, &[dst_ptr, src_ptr, size, align, volatile], None);
}

pub fn memcpy_ty<'blk, 'tcx>(bcx: Block<'blk, 'tcx>, dst: ValueRef, src: ValueRef, t: Ty<'tcx>) {
    let _icx = push_ctxt("memcpy_ty");
    let ccx = bcx.ccx();

    if type_is_zero_size(ccx, t) || bcx.unreachable.get() {
        return;
    }

    if t.is_structural() {
        let llty = type_of::type_of(ccx, t);
        let llsz = llsize_of(ccx, llty);
        let llalign = type_of::align_of(ccx, t);
        call_memcpy(&B(bcx), dst, src, llsz, llalign as u32);
    } else if common::type_is_fat_ptr(bcx.tcx(), t) {
        let (data, extra) = load_fat_ptr(bcx, src, t);
        store_fat_ptr(bcx, data, extra, dst, t);
    } else {
        store_ty(bcx, load_ty(bcx, src, t), dst, t);
    }
}

pub fn init_zero_mem<'blk, 'tcx>(cx: Block<'blk, 'tcx>, llptr: ValueRef, t: Ty<'tcx>) {
    if cx.unreachable.get() {
        return;
    }
    let _icx = push_ctxt("init_zero_mem");
    let bcx = cx;
    memfill(&B(bcx), llptr, t, 0);
}

// Always use this function instead of storing a constant byte to the memory
// in question. e.g. if you store a zero constant, LLVM will drown in vreg
// allocation for large data structures, and the generated code will be
// awful. (A telltale sign of this is large quantities of
// `mov [byte ptr foo],0` in the generated code.)
fn memfill<'a, 'tcx>(b: &Builder<'a, 'tcx>, llptr: ValueRef, ty: Ty<'tcx>, byte: u8) {
    let _icx = push_ctxt("memfill");
    let ccx = b.ccx;
    let llty = type_of::type_of(ccx, ty);
    let llptr = b.pointercast(llptr, Type::i8(ccx).ptr_to());
    let llzeroval = C_u8(ccx, byte);
    let size = machine::llsize_of(ccx, llty);
    let align = C_i32(ccx, type_of::align_of(ccx, ty) as i32);
    call_memset(b, llptr, llzeroval, size, align, false);
}

pub fn call_memset<'bcx, 'tcx>(b: &Builder<'bcx, 'tcx>,
                               ptr: ValueRef,
                               fill_byte: ValueRef,
                               size: ValueRef,
                               align: ValueRef,
                               volatile: bool) {
    let ccx = b.ccx;
    let ptr_width = &ccx.sess().target.target.target_pointer_width[..];
    let intrinsic_key = format!("llvm.memset.p0i8.i{}", ptr_width);
    let llintrinsicfn = ccx.get_intrinsic(&intrinsic_key);
    let volatile = C_bool(ccx, volatile);
    b.call(llintrinsicfn, &[ptr, fill_byte, size, align, volatile], None);
}

pub fn alloc_ty<'blk, 'tcx>(bcx: Block<'blk, 'tcx>,
                            ty: Ty<'tcx>,
                            name: &str) -> ValueRef {
    assert!(!ty.has_param_types());
    alloca(bcx, type_of::type_of(bcx.ccx(), ty), name)
}

pub fn alloca(cx: Block, ty: Type, name: &str) -> ValueRef {
    let _icx = push_ctxt("alloca");
    if cx.unreachable.get() {
        unsafe {
            return llvm::LLVMGetUndef(ty.ptr_to().to_ref());
        }
    }
    DebugLoc::None.apply(cx.fcx);
    Alloca(cx, ty, name)
}

impl<'blk, 'tcx> FunctionContext<'blk, 'tcx> {
    /// Create a function context for the given function.
    /// Beware that you must call `fcx.init` or `fcx.bind_args`
    /// before doing anything with the returned function context.
    pub fn new(ccx: &'blk CrateContext<'blk, 'tcx>,
               llfndecl: ValueRef,
               fn_ty: FnType,
               definition: Option<(Instance<'tcx>, &ty::FnSig<'tcx>, Abi)>,
               block_arena: &'blk TypedArena<common::BlockS<'blk, 'tcx>>)
               -> FunctionContext<'blk, 'tcx> {
        let (param_substs, def_id) = match definition {
            Some((instance, ..)) => {
                common::validate_substs(instance.substs);
                (instance.substs, Some(instance.def))
            }
            None => (ccx.tcx().intern_substs(&[]), None)
        };

        let local_id = def_id.and_then(|id| ccx.tcx().map.as_local_node_id(id));

        debug!("FunctionContext::new({})",
               definition.map_or(String::new(), |d| d.0.to_string()));

        let no_debug = if let Some(id) = local_id {
            ccx.tcx().map.attrs(id)
               .iter().any(|item| item.check_name("no_debug"))
        } else if let Some(def_id) = def_id {
            ccx.sess().cstore.item_attrs(def_id)
               .iter().any(|item| item.check_name("no_debug"))
        } else {
            false
        };

        let mir = def_id.map(|id| ccx.tcx().item_mir(id));

        let debug_context = if let (false, Some((instance, sig, abi)), &Some(ref mir)) =
                (no_debug, definition, &mir) {
            debuginfo::create_function_debug_context(ccx, instance, sig, abi, llfndecl, mir)
        } else {
            debuginfo::empty_function_debug_context(ccx)
        };

        FunctionContext {
            mir: mir,
            llfn: llfndecl,
            llretslotptr: Cell::new(None),
            param_env: ccx.tcx().empty_parameter_environment(),
            alloca_insert_pt: Cell::new(None),
            landingpad_alloca: Cell::new(None),
            fn_ty: fn_ty,
            param_substs: param_substs,
            span: None,
            block_arena: block_arena,
            lpad_arena: TypedArena::new(),
            ccx: ccx,
            debug_context: debug_context,
            scopes: RefCell::new(Vec::new()),
        }
    }

    /// Performs setup on a newly created function, creating the entry
    /// scope block and allocating space for the return pointer.
    pub fn init(&'blk self, skip_retptr: bool) -> Block<'blk, 'tcx> {
        let entry_bcx = self.new_block("entry-block");

        // Use a dummy instruction as the insertion point for all allocas.
        // This is later removed in FunctionContext::cleanup.
        self.alloca_insert_pt.set(Some(unsafe {
            Load(entry_bcx, C_null(Type::i8p(self.ccx)));
            llvm::LLVMGetFirstInstruction(entry_bcx.llbb)
        }));

        if !self.fn_ty.ret.is_ignore() && !skip_retptr {
            // We normally allocate the llretslotptr, unless we
            // have been instructed to skip it for immediate return
            // values, or there is nothing to return at all.

            // We create an alloca to hold a pointer of type `ret.original_ty`
            // which will hold the pointer to the right alloca which has the
            // final ret value
            let llty = self.fn_ty.ret.memory_ty(self.ccx);
            // But if there are no nested returns, we skip the indirection
            // and have a single retslot
            let slot = if self.fn_ty.ret.is_indirect() {
                get_param(self.llfn, 0)
            } else {
                AllocaFcx(self, llty, "sret_slot")
            };

            self.llretslotptr.set(Some(slot));
        }

        entry_bcx
    }

    /// Ties up the llstaticallocas -> llloadenv -> lltop edges,
    /// and builds the return block.
    pub fn finish(&'blk self, ret_cx: Block<'blk, 'tcx>,
                  ret_debug_loc: DebugLoc) {
        let _icx = push_ctxt("FunctionContext::finish");

        self.build_return_block(ret_cx, ret_debug_loc);

        DebugLoc::None.apply(self);
        self.cleanup();
    }

    // Builds the return block for a function.
    pub fn build_return_block(&self, ret_cx: Block<'blk, 'tcx>,
                              ret_debug_location: DebugLoc) {
        if self.llretslotptr.get().is_none() ||
           ret_cx.unreachable.get() ||
           self.fn_ty.ret.is_indirect() {
            return RetVoid(ret_cx, ret_debug_location);
        }

        let retslot = self.llretslotptr.get().unwrap();
        let retptr = Value(retslot);
        let llty = self.fn_ty.ret.original_ty;
        match (retptr.get_dominating_store(ret_cx), self.fn_ty.ret.cast) {
            // If there's only a single store to the ret slot, we can directly return
            // the value that was stored and omit the store and the alloca.
            // However, we only want to do this when there is no cast needed.
            (Some(s), None) => {
                let mut retval = s.get_operand(0).unwrap().get();
                s.erase_from_parent();

                if retptr.has_no_uses() {
                    retptr.erase_from_parent();
                }

                if self.fn_ty.ret.is_indirect() {
                    Store(ret_cx, retval, get_param(self.llfn, 0));
                    RetVoid(ret_cx, ret_debug_location)
                } else {
                    if llty == Type::i1(self.ccx) {
                        retval = Trunc(ret_cx, retval, llty);
                    }
                    Ret(ret_cx, retval, ret_debug_location)
                }
            }
            (_, cast_ty) if self.fn_ty.ret.is_indirect() => {
                // Otherwise, copy the return value to the ret slot.
                assert_eq!(cast_ty, None);
                let llsz = llsize_of(self.ccx, self.fn_ty.ret.ty);
                let llalign = llalign_of_min(self.ccx, self.fn_ty.ret.ty);
                call_memcpy(&B(ret_cx), get_param(self.llfn, 0),
                            retslot, llsz, llalign as u32);
                RetVoid(ret_cx, ret_debug_location)
            }
            (_, Some(cast_ty)) => {
                let load = Load(ret_cx, PointerCast(ret_cx, retslot, cast_ty.ptr_to()));
                let llalign = llalign_of_min(self.ccx, self.fn_ty.ret.ty);
                unsafe {
                    llvm::LLVMSetAlignment(load, llalign);
                }
                Ret(ret_cx, load, ret_debug_location)
            }
            (_, None) => {
                let retval = if llty == Type::i1(self.ccx) {
                    let val = LoadRangeAssert(ret_cx, retslot, 0, 2, llvm::False);
                    Trunc(ret_cx, val, llty)
                } else {
                    Load(ret_cx, retslot)
                };
                Ret(ret_cx, retval, ret_debug_location)
            }
        }
    }
}

pub fn trans_instance<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>, instance: Instance<'tcx>) {
    let _s = if ccx.sess().trans_stats() {
        let mut instance_name = String::new();
        DefPathBasedNames::new(ccx.tcx(), true, true)
            .push_def_path(instance.def, &mut instance_name);
        Some(StatRecorder::new(ccx, instance_name))
    } else {
        None
    };

    // this is an info! to allow collecting monomorphization statistics
    // and to allow finding the last function before LLVM aborts from
    // release builds.
    info!("trans_instance({})", instance);

    let _icx = push_ctxt("trans_instance");

    let fn_ty = ccx.tcx().item_type(instance.def);
    let fn_ty = ccx.tcx().erase_regions(&fn_ty);
    let fn_ty = monomorphize::apply_param_substs(ccx.shared(), instance.substs, &fn_ty);

    let ty::BareFnTy { abi, ref sig, .. } = *common::ty_fn_ty(ccx, fn_ty);
    let sig = ccx.tcx().erase_late_bound_regions_and_normalize(sig);

    let lldecl = match ccx.instances().borrow().get(&instance) {
        Some(&val) => val,
        None => bug!("Instance `{:?}` not already declared", instance)
    };

    ccx.stats().n_closures.set(ccx.stats().n_closures.get() + 1);

    if !ccx.sess().no_landing_pads() {
        attributes::emit_uwtable(lldecl, true);
    }

    let fn_ty = FnType::new(ccx, abi, &sig, &[]);

    let (arena, fcx): (TypedArena<_>, FunctionContext);
    arena = TypedArena::new();
    fcx = FunctionContext::new(ccx,
                               lldecl,
                               fn_ty,
                               Some((instance, &sig, abi)),
                               &arena);

    if fcx.mir.is_none() {
        bug!("attempted translation of `{}` w/o MIR", instance);
    }

    mir::trans_mir(&fcx);
}

pub fn trans_ctor_shim<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                                 def_id: DefId,
                                 substs: &'tcx Substs<'tcx>,
                                 disr: Disr,
                                 llfndecl: ValueRef) {
    attributes::inline(llfndecl, attributes::InlineAttr::Hint);
    attributes::set_frame_pointer_elimination(ccx, llfndecl);

    let ctor_ty = ccx.tcx().item_type(def_id);
    let ctor_ty = monomorphize::apply_param_substs(ccx.shared(), substs, &ctor_ty);

    let sig = ccx.tcx().erase_late_bound_regions_and_normalize(&ctor_ty.fn_sig());
    let fn_ty = FnType::new(ccx, Abi::Rust, &sig, &[]);

    let (arena, fcx): (TypedArena<_>, FunctionContext);
    arena = TypedArena::new();
    fcx = FunctionContext::new(ccx, llfndecl, fn_ty, None, &arena);
    let bcx = fcx.init(false);

    if !fcx.fn_ty.ret.is_ignore() {
        let dest = fcx.llretslotptr.get().unwrap();
        let dest_val = adt::MaybeSizedValue::sized(dest); // Can return unsized value
        let mut llarg_idx = fcx.fn_ty.ret.is_indirect() as usize;
        let mut arg_idx = 0;
        for (i, arg_ty) in sig.inputs().iter().enumerate() {
            let lldestptr = adt::trans_field_ptr(bcx, sig.output(), dest_val, Disr::from(disr), i);
            let arg = &fcx.fn_ty.args[arg_idx];
            arg_idx += 1;
            let b = &bcx.build();
            if common::type_is_fat_ptr(bcx.tcx(), arg_ty) {
                let meta = &fcx.fn_ty.args[arg_idx];
                arg_idx += 1;
                arg.store_fn_arg(b, &mut llarg_idx, get_dataptr(bcx, lldestptr));
                meta.store_fn_arg(b, &mut llarg_idx, get_meta(bcx, lldestptr));
            } else {
                arg.store_fn_arg(b, &mut llarg_idx, lldestptr);
            }
        }
        adt::trans_set_discr(bcx, sig.output(), dest, disr);
    }

    fcx.finish(bcx, DebugLoc::None);
}

pub fn llvm_linkage_by_name(name: &str) -> Option<Linkage> {
    // Use the names from src/llvm/docs/LangRef.rst here. Most types are only
    // applicable to variable declarations and may not really make sense for
    // Rust code in the first place but whitelist them anyway and trust that
    // the user knows what s/he's doing. Who knows, unanticipated use cases
    // may pop up in the future.
    //
    // ghost, dllimport, dllexport and linkonce_odr_autohide are not supported
    // and don't have to be, LLVM treats them as no-ops.
    match name {
        "appending" => Some(llvm::Linkage::AppendingLinkage),
        "available_externally" => Some(llvm::Linkage::AvailableExternallyLinkage),
        "common" => Some(llvm::Linkage::CommonLinkage),
        "extern_weak" => Some(llvm::Linkage::ExternalWeakLinkage),
        "external" => Some(llvm::Linkage::ExternalLinkage),
        "internal" => Some(llvm::Linkage::InternalLinkage),
        "linkonce" => Some(llvm::Linkage::LinkOnceAnyLinkage),
        "linkonce_odr" => Some(llvm::Linkage::LinkOnceODRLinkage),
        "private" => Some(llvm::Linkage::PrivateLinkage),
        "weak" => Some(llvm::Linkage::WeakAnyLinkage),
        "weak_odr" => Some(llvm::Linkage::WeakODRLinkage),
        _ => None,
    }
}

pub fn set_link_section(ccx: &CrateContext,
                        llval: ValueRef,
                        attrs: &[ast::Attribute]) {
    if let Some(sect) = attr::first_attr_value_str_by_name(attrs, "link_section") {
        if contains_null(&sect.as_str()) {
            ccx.sess().fatal(&format!("Illegal null byte in link_section value: `{}`", &sect));
        }
        unsafe {
            let buf = CString::new(sect.as_str().as_bytes()).unwrap();
            llvm::LLVMSetSection(llval, buf.as_ptr());
        }
    }
}

/// Create the `main` function which will initialise the rust runtime and call
/// users’ main function.
pub fn maybe_create_entry_wrapper(ccx: &CrateContext) {
    let (main_def_id, span) = match *ccx.sess().entry_fn.borrow() {
        Some((id, span)) => {
            (ccx.tcx().map.local_def_id(id), span)
        }
        None => return,
    };

    // check for the #[rustc_error] annotation, which forces an
    // error in trans. This is used to write compile-fail tests
    // that actually test that compilation succeeds without
    // reporting an error.
    if ccx.tcx().has_attr(main_def_id, "rustc_error") {
        ccx.tcx().sess.span_fatal(span, "compilation successful");
    }

    let instance = Instance::mono(ccx.shared(), main_def_id);

    if !ccx.codegen_unit().contains_item(&TransItem::Fn(instance)) {
        // We want to create the wrapper in the same codegen unit as Rust's main
        // function.
        return;
    }

    let main_llfn = Callee::def(ccx, main_def_id, instance.substs).reify(ccx);

    let et = ccx.sess().entry_type.get().unwrap();
    match et {
        config::EntryMain => {
            create_entry_fn(ccx, span, main_llfn, true);
        }
        config::EntryStart => create_entry_fn(ccx, span, main_llfn, false),
        config::EntryNone => {}    // Do nothing.
    }

    fn create_entry_fn(ccx: &CrateContext,
                       sp: Span,
                       rust_main: ValueRef,
                       use_start_lang_item: bool) {
        let llfty = Type::func(&[ccx.int_type(), Type::i8p(ccx).ptr_to()], &ccx.int_type());

        if declare::get_defined_value(ccx, "main").is_some() {
            // FIXME: We should be smart and show a better diagnostic here.
            ccx.sess().struct_span_err(sp, "entry symbol `main` defined multiple times")
                      .help("did you use #[no_mangle] on `fn main`? Use #[start] instead")
                      .emit();
            ccx.sess().abort_if_errors();
            bug!();
        }
        let llfn = declare::declare_cfn(ccx, "main", llfty);

        // `main` should respect same config for frame pointer elimination as rest of code
        attributes::set_frame_pointer_elimination(ccx, llfn);

        let llbb = unsafe {
            llvm::LLVMAppendBasicBlockInContext(ccx.llcx(), llfn, "top\0".as_ptr() as *const _)
        };
        let bld = ccx.raw_builder();
        unsafe {
            llvm::LLVMPositionBuilderAtEnd(bld, llbb);

            debuginfo::gdb::insert_reference_to_gdb_debug_scripts_section_global(ccx);

            let (start_fn, args) = if use_start_lang_item {
                let start_def_id = match ccx.tcx().lang_items.require(StartFnLangItem) {
                    Ok(id) => id,
                    Err(s) => ccx.sess().fatal(&s)
                };
                let empty_substs = ccx.tcx().intern_substs(&[]);
                let start_fn = Callee::def(ccx, start_def_id, empty_substs).reify(ccx);
                let args = {
                    let opaque_rust_main =
                        llvm::LLVMBuildPointerCast(bld,
                                                   rust_main,
                                                   Type::i8p(ccx).to_ref(),
                                                   "rust_main\0".as_ptr() as *const _);

                    vec![opaque_rust_main, get_param(llfn, 0), get_param(llfn, 1)]
                };
                (start_fn, args)
            } else {
                debug!("using user-defined start fn");
                let args = vec![get_param(llfn, 0 as c_uint), get_param(llfn, 1 as c_uint)];

                (rust_main, args)
            };

            let result = llvm::LLVMRustBuildCall(bld,
                                                 start_fn,
                                                 args.as_ptr(),
                                                 args.len() as c_uint,
                                                 ptr::null_mut(),
                                                 noname());

            llvm::LLVMBuildRet(bld, result);
        }
    }
}

fn contains_null(s: &str) -> bool {
    s.bytes().any(|b| b == 0)
}

fn write_metadata(cx: &SharedCrateContext,
                  exported_symbols: &NodeSet) -> Vec<u8> {
    use flate;

    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    enum MetadataKind {
        None,
        Uncompressed,
        Compressed
    }

    let kind = cx.sess().crate_types.borrow().iter().map(|ty| {
        match *ty {
            config::CrateTypeExecutable |
            config::CrateTypeStaticlib |
            config::CrateTypeCdylib => MetadataKind::None,

            config::CrateTypeRlib |
            config::CrateTypeMetadata => MetadataKind::Uncompressed,

            config::CrateTypeDylib |
            config::CrateTypeProcMacro => MetadataKind::Compressed,
        }
    }).max().unwrap();

    if kind == MetadataKind::None {
        return Vec::new();
    }

    let cstore = &cx.tcx().sess.cstore;
    let metadata = cstore.encode_metadata(cx.tcx(),
                                          cx.export_map(),
                                          cx.link_meta(),
                                          exported_symbols);
    if kind == MetadataKind::Uncompressed {
        return metadata;
    }

    assert!(kind == MetadataKind::Compressed);
    let mut compressed = cstore.metadata_encoding_version().to_vec();
    compressed.extend_from_slice(&flate::deflate_bytes(&metadata));

    let llmeta = C_bytes_in_context(cx.metadata_llcx(), &compressed[..]);
    let llconst = C_struct_in_context(cx.metadata_llcx(), &[llmeta], false);
    let name = cx.metadata_symbol_name();
    let buf = CString::new(name).unwrap();
    let llglobal = unsafe {
        llvm::LLVMAddGlobal(cx.metadata_llmod(), val_ty(llconst).to_ref(), buf.as_ptr())
    };
    unsafe {
        llvm::LLVMSetInitializer(llglobal, llconst);
        let section_name =
            cx.tcx().sess.cstore.metadata_section_name(&cx.sess().target.target);
        let name = CString::new(section_name).unwrap();
        llvm::LLVMSetSection(llglobal, name.as_ptr());

        // Also generate a .section directive to force no
        // flags, at least for ELF outputs, so that the
        // metadata doesn't get loaded into memory.
        let directive = format!(".section {}", section_name);
        let directive = CString::new(directive).unwrap();
        llvm::LLVMSetModuleInlineAsm(cx.metadata_llmod(), directive.as_ptr())
    }
    return metadata;
}

/// Find any symbols that are defined in one compilation unit, but not declared
/// in any other compilation unit.  Give these symbols internal linkage.
fn internalize_symbols<'a, 'tcx>(sess: &Session,
                                 ccxs: &CrateContextList<'a, 'tcx>,
                                 symbol_map: &SymbolMap<'tcx>,
                                 exported_symbols: &ExportedSymbols) {
    let export_threshold =
        symbol_export::crates_export_threshold(&sess.crate_types.borrow()[..]);

    let exported_symbols = exported_symbols
        .exported_symbols(LOCAL_CRATE)
        .iter()
        .filter(|&&(_, export_level)| {
            symbol_export::is_below_threshold(export_level, export_threshold)
        })
        .map(|&(ref name, _)| &name[..])
        .collect::<FxHashSet<&str>>();

    let scx = ccxs.shared();
    let tcx = scx.tcx();

    let incr_comp = sess.opts.debugging_opts.incremental.is_some();

    // 'unsafe' because we are holding on to CStr's from the LLVM module within
    // this block.
    unsafe {
        let mut referenced_somewhere = FxHashSet();

        // Collect all symbols that need to stay externally visible because they
        // are referenced via a declaration in some other codegen unit. In
        // incremental compilation, we don't need to collect. See below for more
        // information.
        if !incr_comp {
            for ccx in ccxs.iter_need_trans() {
                for val in iter_globals(ccx.llmod()).chain(iter_functions(ccx.llmod())) {
                    let linkage = llvm::LLVMRustGetLinkage(val);
                    // We only care about external declarations (not definitions)
                    // and available_externally definitions.
                    let is_available_externally =
                        linkage == llvm::Linkage::AvailableExternallyLinkage;
                    let is_decl = llvm::LLVMIsDeclaration(val) == llvm::True;

                    if is_decl || is_available_externally {
                        let symbol_name = CStr::from_ptr(llvm::LLVMGetValueName(val));
                        referenced_somewhere.insert(symbol_name);
                    }
                }
            }
        }

        // Also collect all symbols for which we cannot adjust linkage, because
        // it is fixed by some directive in the source code.
        let (locally_defined_symbols, linkage_fixed_explicitly) = {
            let mut locally_defined_symbols = FxHashSet();
            let mut linkage_fixed_explicitly = FxHashSet();

            for trans_item in scx.translation_items().borrow().iter() {
                let symbol_name = symbol_map.get_or_compute(scx, *trans_item);
                if trans_item.explicit_linkage(tcx).is_some() {
                    linkage_fixed_explicitly.insert(symbol_name.clone());
                }
                locally_defined_symbols.insert(symbol_name);
            }

            (locally_defined_symbols, linkage_fixed_explicitly)
        };

        // Examine each external definition.  If the definition is not used in
        // any other compilation unit, and is not reachable from other crates,
        // then give it internal linkage.
        for ccx in ccxs.iter_need_trans() {
            for val in iter_globals(ccx.llmod()).chain(iter_functions(ccx.llmod())) {
                let linkage = llvm::LLVMRustGetLinkage(val);

                let is_externally_visible = (linkage == llvm::Linkage::ExternalLinkage) ||
                                            (linkage == llvm::Linkage::LinkOnceODRLinkage) ||
                                            (linkage == llvm::Linkage::WeakODRLinkage);

                if !is_externally_visible {
                    // This symbol is not visible outside of its codegen unit,
                    // so there is nothing to do for it.
                    continue;
                }

                let name_cstr = CStr::from_ptr(llvm::LLVMGetValueName(val));
                let name_str = name_cstr.to_str().unwrap();

                if exported_symbols.contains(&name_str) {
                    // This symbol is explicitly exported, so we can't
                    // mark it as internal or hidden.
                    continue;
                }

                let is_declaration = llvm::LLVMIsDeclaration(val) == llvm::True;

                if is_declaration {
                    if locally_defined_symbols.contains(name_str) {
                        // Only mark declarations from the current crate as hidden.
                        // Otherwise we would mark things as hidden that are
                        // imported from other crates or native libraries.
                        llvm::LLVMRustSetVisibility(val, llvm::Visibility::Hidden);
                    }
                } else {
                    let has_fixed_linkage = linkage_fixed_explicitly.contains(name_str);

                    if !has_fixed_linkage {
                        // In incremental compilation mode, we can't be sure that
                        // we saw all references because we don't know what's in
                        // cached compilation units, so we always assume that the
                        // given item has been referenced.
                        if incr_comp || referenced_somewhere.contains(&name_cstr) {
                            llvm::LLVMRustSetVisibility(val, llvm::Visibility::Hidden);
                        } else {
                            llvm::LLVMRustSetLinkage(val, llvm::Linkage::InternalLinkage);
                        }

                        llvm::LLVMSetDLLStorageClass(val, llvm::DLLStorageClass::Default);
                        llvm::UnsetComdat(val);
                    }
                }
            }
        }
    }
}

// Create a `__imp_<symbol> = &symbol` global for every public static `symbol`.
// This is required to satisfy `dllimport` references to static data in .rlibs
// when using MSVC linker.  We do this only for data, as linker can fix up
// code references on its own.
// See #26591, #27438
fn create_imps(cx: &CrateContextList) {
    // The x86 ABI seems to require that leading underscores are added to symbol
    // names, so we need an extra underscore on 32-bit. There's also a leading
    // '\x01' here which disables LLVM's symbol mangling (e.g. no extra
    // underscores added in front).
    let prefix = if cx.shared().sess().target.target.target_pointer_width == "32" {
        "\x01__imp__"
    } else {
        "\x01__imp_"
    };
    unsafe {
        for ccx in cx.iter_need_trans() {
            let exported: Vec<_> = iter_globals(ccx.llmod())
                                       .filter(|&val| {
                                           llvm::LLVMRustGetLinkage(val) ==
                                           llvm::Linkage::ExternalLinkage &&
                                           llvm::LLVMIsDeclaration(val) == 0
                                       })
                                       .collect();

            let i8p_ty = Type::i8p(&ccx);
            for val in exported {
                let name = CStr::from_ptr(llvm::LLVMGetValueName(val));
                let mut imp_name = prefix.as_bytes().to_vec();
                imp_name.extend(name.to_bytes());
                let imp_name = CString::new(imp_name).unwrap();
                let imp = llvm::LLVMAddGlobal(ccx.llmod(),
                                              i8p_ty.to_ref(),
                                              imp_name.as_ptr() as *const _);
                let init = llvm::LLVMConstBitCast(val, i8p_ty.to_ref());
                llvm::LLVMSetInitializer(imp, init);
                llvm::LLVMRustSetLinkage(imp, llvm::Linkage::ExternalLinkage);
            }
        }
    }
}

struct ValueIter {
    cur: ValueRef,
    step: unsafe extern "C" fn(ValueRef) -> ValueRef,
}

impl Iterator for ValueIter {
    type Item = ValueRef;

    fn next(&mut self) -> Option<ValueRef> {
        let old = self.cur;
        if !old.is_null() {
            self.cur = unsafe { (self.step)(old) };
            Some(old)
        } else {
            None
        }
    }
}

fn iter_globals(llmod: llvm::ModuleRef) -> ValueIter {
    unsafe {
        ValueIter {
            cur: llvm::LLVMGetFirstGlobal(llmod),
            step: llvm::LLVMGetNextGlobal,
        }
    }
}

fn iter_functions(llmod: llvm::ModuleRef) -> ValueIter {
    unsafe {
        ValueIter {
            cur: llvm::LLVMGetFirstFunction(llmod),
            step: llvm::LLVMGetNextFunction,
        }
    }
}

/// The context provided lists a set of reachable ids as calculated by
/// middle::reachable, but this contains far more ids and symbols than we're
/// actually exposing from the object file. This function will filter the set in
/// the context to the set of ids which correspond to symbols that are exposed
/// from the object file being generated.
///
/// This list is later used by linkers to determine the set of symbols needed to
/// be exposed from a dynamic library and it's also encoded into the metadata.
pub fn find_exported_symbols(tcx: TyCtxt, reachable: NodeSet) -> NodeSet {
    reachable.into_iter().filter(|&id| {
        // Next, we want to ignore some FFI functions that are not exposed from
        // this crate. Reachable FFI functions can be lumped into two
        // categories:
        //
        // 1. Those that are included statically via a static library
        // 2. Those included otherwise (e.g. dynamically or via a framework)
        //
        // Although our LLVM module is not literally emitting code for the
        // statically included symbols, it's an export of our library which
        // needs to be passed on to the linker and encoded in the metadata.
        //
        // As a result, if this id is an FFI item (foreign item) then we only
        // let it through if it's included statically.
        match tcx.map.get(id) {
            hir_map::NodeForeignItem(..) => {
                let def_id = tcx.map.local_def_id(id);
                tcx.sess.cstore.is_statically_included_foreign_item(def_id)
            }

            // Only consider nodes that actually have exported symbols.
            hir_map::NodeItem(&hir::Item {
                node: hir::ItemStatic(..), .. }) |
            hir_map::NodeItem(&hir::Item {
                node: hir::ItemFn(..), .. }) |
            hir_map::NodeImplItem(&hir::ImplItem {
                node: hir::ImplItemKind::Method(..), .. }) => {
                let def_id = tcx.map.local_def_id(id);
                let generics = tcx.item_generics(def_id);
                let attributes = tcx.get_attrs(def_id);
                (generics.parent_types == 0 && generics.types.is_empty()) &&
                // Functions marked with #[inline] are only ever translated
                // with "internal" linkage and are never exported.
                !attr::requests_inline(&attributes[..])
            }

            _ => false
        }
    }).collect()
}

pub fn trans_crate<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                             analysis: ty::CrateAnalysis,
                             incremental_hashes_map: &IncrementalHashesMap)
                             -> CrateTranslation {
    let _task = tcx.dep_graph.in_task(DepNode::TransCrate);

    // Be careful with this krate: obviously it gives access to the
    // entire contents of the krate. So if you push any subtasks of
    // `TransCrate`, you need to be careful to register "reads" of the
    // particular items that will be processed.
    let krate = tcx.map.krate();

    let ty::CrateAnalysis { export_map, reachable, name, .. } = analysis;
    let exported_symbols = find_exported_symbols(tcx, reachable);

    let check_overflow = if let Some(v) = tcx.sess.opts.debugging_opts.force_overflow_checks {
        v
    } else {
        tcx.sess.opts.debug_assertions
    };

    let link_meta = link::build_link_meta(incremental_hashes_map, &name);

    let shared_ccx = SharedCrateContext::new(tcx,
                                             export_map,
                                             link_meta.clone(),
                                             exported_symbols,
                                             check_overflow);
    // Translate the metadata.
    let metadata = time(tcx.sess.time_passes(), "write metadata", || {
        write_metadata(&shared_ccx, shared_ccx.exported_symbols())
    });

    let metadata_module = ModuleTranslation {
        name: "metadata".to_string(),
        symbol_name_hash: 0, // we always rebuild metadata, at least for now
        source: ModuleSource::Translated(ModuleLlvm {
            llcx: shared_ccx.metadata_llcx(),
            llmod: shared_ccx.metadata_llmod(),
        }),
    };
    let no_builtins = attr::contains_name(&krate.attrs, "no_builtins");

    // Run the translation item collector and partition the collected items into
    // codegen units.
    let (codegen_units, symbol_map) = collect_and_partition_translation_items(&shared_ccx);

    let symbol_map = Rc::new(symbol_map);

    let previous_work_products = trans_reuse_previous_work_products(&shared_ccx,
                                                                    &codegen_units,
                                                                    &symbol_map);

    let crate_context_list = CrateContextList::new(&shared_ccx,
                                                   codegen_units,
                                                   previous_work_products,
                                                   symbol_map.clone());
    let modules: Vec<_> = crate_context_list.iter_all()
        .map(|ccx| {
            let source = match ccx.previous_work_product() {
                Some(buf) => ModuleSource::Preexisting(buf.clone()),
                None => ModuleSource::Translated(ModuleLlvm {
                    llcx: ccx.llcx(),
                    llmod: ccx.llmod(),
                }),
            };

            ModuleTranslation {
                name: String::from(ccx.codegen_unit().name()),
                symbol_name_hash: ccx.codegen_unit()
                                     .compute_symbol_name_hash(&shared_ccx,
                                                               &symbol_map),
                source: source,
            }
        })
        .collect();

    assert_module_sources::assert_module_sources(tcx, &modules);

    // Skip crate items and just output metadata in -Z no-trans mode.
    if tcx.sess.opts.debugging_opts.no_trans ||
       tcx.sess.crate_types.borrow().iter().all(|ct| ct == &config::CrateTypeMetadata) {
        let linker_info = LinkerInfo::new(&shared_ccx, &ExportedSymbols::empty());
        return CrateTranslation {
            modules: modules,
            metadata_module: metadata_module,
            link: link_meta,
            metadata: metadata,
            exported_symbols: ExportedSymbols::empty(),
            no_builtins: no_builtins,
            linker_info: linker_info,
            windows_subsystem: None,
        };
    }

    // Instantiate translation items without filling out definitions yet...
    for ccx in crate_context_list.iter_need_trans() {
        let cgu = ccx.codegen_unit();
        let trans_items = cgu.items_in_deterministic_order(tcx, &symbol_map);

        tcx.dep_graph.with_task(cgu.work_product_dep_node(), || {
            for (trans_item, linkage) in trans_items {
                trans_item.predefine(&ccx, linkage);
            }
        });
    }

    // ... and now that we have everything pre-defined, fill out those definitions.
    for ccx in crate_context_list.iter_need_trans() {
        let cgu = ccx.codegen_unit();
        let trans_items = cgu.items_in_deterministic_order(tcx, &symbol_map);
        tcx.dep_graph.with_task(cgu.work_product_dep_node(), || {
            for (trans_item, _) in trans_items {
                trans_item.define(&ccx);
            }

            // If this codegen unit contains the main function, also create the
            // wrapper here
            maybe_create_entry_wrapper(&ccx);

            // Run replace-all-uses-with for statics that need it
            for &(old_g, new_g) in ccx.statics_to_rauw().borrow().iter() {
                unsafe {
                    let bitcast = llvm::LLVMConstPointerCast(new_g, llvm::LLVMTypeOf(old_g));
                    llvm::LLVMReplaceAllUsesWith(old_g, bitcast);
                    llvm::LLVMDeleteGlobal(old_g);
                }
            }

            // Finalize debuginfo
            if ccx.sess().opts.debuginfo != NoDebugInfo {
                debuginfo::finalize(&ccx);
            }
        });
    }

    symbol_names_test::report_symbol_names(&shared_ccx);

    if shared_ccx.sess().trans_stats() {
        let stats = shared_ccx.stats();
        println!("--- trans stats ---");
        println!("n_glues_created: {}", stats.n_glues_created.get());
        println!("n_null_glues: {}", stats.n_null_glues.get());
        println!("n_real_glues: {}", stats.n_real_glues.get());

        println!("n_fns: {}", stats.n_fns.get());
        println!("n_inlines: {}", stats.n_inlines.get());
        println!("n_closures: {}", stats.n_closures.get());
        println!("fn stats:");
        stats.fn_stats.borrow_mut().sort_by(|&(_, insns_a), &(_, insns_b)| {
            insns_b.cmp(&insns_a)
        });
        for tuple in stats.fn_stats.borrow().iter() {
            match *tuple {
                (ref name, insns) => {
                    println!("{} insns, {}", insns, *name);
                }
            }
        }
    }

    if shared_ccx.sess().count_llvm_insns() {
        for (k, v) in shared_ccx.stats().llvm_insns.borrow().iter() {
            println!("{:7} {}", *v, *k);
        }
    }

    let sess = shared_ccx.sess();

    let exported_symbols = ExportedSymbols::compute_from(&shared_ccx,
                                                         &symbol_map);

    // Now that we have all symbols that are exported from the CGUs of this
    // crate, we can run the `internalize_symbols` pass.
    time(shared_ccx.sess().time_passes(), "internalize symbols", || {
        internalize_symbols(sess,
                            &crate_context_list,
                            &symbol_map,
                            &exported_symbols);
    });

    if tcx.sess.opts.debugging_opts.print_type_sizes {
        gather_type_sizes(tcx);
    }

    if sess.target.target.options.is_like_msvc &&
       sess.crate_types.borrow().iter().any(|ct| *ct == config::CrateTypeRlib) {
        create_imps(&crate_context_list);
    }

    let linker_info = LinkerInfo::new(&shared_ccx, &exported_symbols);

    let subsystem = attr::first_attr_value_str_by_name(&krate.attrs,
                                                       "windows_subsystem");
    let windows_subsystem = subsystem.map(|subsystem| {
        if subsystem != "windows" && subsystem != "console" {
            tcx.sess.fatal(&format!("invalid windows subsystem `{}`, only \
                                     `windows` and `console` are allowed",
                                    subsystem));
        }
        subsystem.to_string()
    });

    CrateTranslation {
        modules: modules,
        metadata_module: metadata_module,
        link: link_meta,
        metadata: metadata,
        exported_symbols: exported_symbols,
        no_builtins: no_builtins,
        linker_info: linker_info,
        windows_subsystem: windows_subsystem,
    }
}

fn gather_type_sizes<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>) {
    let layout_cache = tcx.layout_cache.borrow();
    for (ty, layout) in layout_cache.iter() {

        // (delay format until we actually need it)
        let record = |kind, opt_discr_size, variants| {
            let type_desc = format!("{:?}", ty);
            let overall_size = layout.size(&tcx.data_layout);
            let align = layout.align(&tcx.data_layout);
            tcx.sess.code_stats.borrow_mut().record_type_size(kind,
                                                              type_desc,
                                                              align,
                                                              overall_size,
                                                              opt_discr_size,
                                                              variants);
        };

        let (adt_def, substs) = match ty.sty {
            ty::TyAdt(ref adt_def, substs) => {
                debug!("print-type-size t: `{:?}` process adt", ty);
                (adt_def, substs)
            }

            ty::TyClosure(..) => {
                debug!("print-type-size t: `{:?}` record closure", ty);
                record(DataTypeKind::Closure, None, vec![]);
                continue;
            }

            _ => {
                debug!("print-type-size t: `{:?}` skip non-nominal", ty);
                continue;
            }
        };

        let adt_kind = adt_def.adt_kind();

        let build_field_info = |(field_name, field_ty): (ast::Name, Ty), offset: &layout::Size| {
            match layout_cache.get(&field_ty) {
                None => bug!("no layout found for field {} type: `{:?}`", field_name, field_ty),
                Some(field_layout) => {
                    session::FieldInfo {
                        name: field_name.to_string(),
                        offset: offset.bytes(),
                        size: field_layout.size(&tcx.data_layout).bytes(),
                        align: field_layout.align(&tcx.data_layout).abi(),
                    }
                }
            }
        };

        let build_primitive_info = |name: ast::Name, value: &layout::Primitive| {
            session::VariantInfo {
                name: Some(name.to_string()),
                kind: session::SizeKind::Exact,
                align: value.align(&tcx.data_layout).abi(),
                size: value.size(&tcx.data_layout).bytes(),
                fields: vec![],
            }
        };

        enum Fields<'a> {
            WithDiscrim(&'a layout::Struct),
            NoDiscrim(&'a layout::Struct),
        }

        let build_variant_info = |n: Option<ast::Name>, flds: &[(ast::Name, Ty)], layout: Fields| {
            let (s, field_offsets) = match layout {
                Fields::WithDiscrim(s) => (s, &s.offsets[1..]),
                Fields::NoDiscrim(s) => (s, &s.offsets[0..]),
            };
            let field_info: Vec<_> = flds.iter()
                .zip(field_offsets.iter())
                .map(|(&field_name_ty, offset)| build_field_info(field_name_ty, offset))
                .collect();

            session::VariantInfo {
                name: n.map(|n|n.to_string()),
                kind: if s.sized {
                    session::SizeKind::Exact
                } else {
                    session::SizeKind::Min
                },
                align: s.align.abi(),
                size: s.min_size.bytes(),
                fields: field_info,
            }
        };

        match **layout {
            Layout::StructWrappedNullablePointer { nonnull: ref variant_layout,
                                                   nndiscr,
                                                   discrfield: _ } => {
                debug!("print-type-size t: `{:?}` adt struct-wrapped nullable nndiscr {} is {:?}",
                       ty, nndiscr, variant_layout);
                let variant_def = &adt_def.variants[nndiscr as usize];
                let fields: Vec<_> = variant_def.fields.iter()
                    .map(|field_def| (field_def.name, field_def.ty(tcx, substs)))
                    .collect();
                record(adt_kind.into(),
                       None,
                       vec![build_variant_info(Some(variant_def.name),
                                               &fields,
                                               Fields::NoDiscrim(variant_layout))]);
            }
            Layout::RawNullablePointer { nndiscr, value } => {
                debug!("print-type-size t: `{:?}` adt raw nullable nndiscr {} is {:?}",
                       ty, nndiscr, value);
                let variant_def = &adt_def.variants[nndiscr as usize];
                record(adt_kind.into(), None,
                       vec![build_primitive_info(variant_def.name, &value)]);
            }
            Layout::Univariant { variant: ref variant_layout, non_zero: _ } => {
                let variant_names = || {
                    adt_def.variants.iter().map(|v|format!("{}", v.name)).collect::<Vec<_>>()
                };
                debug!("print-type-size t: `{:?}` adt univariant {:?} variants: {:?}",
                       ty, variant_layout, variant_names());
                assert!(adt_def.variants.len() <= 1,
                        "univariant with variants {:?}", variant_names());
                if adt_def.variants.len() == 1 {
                    let variant_def = &adt_def.variants[0];
                    let fields: Vec<_> = variant_def.fields.iter()
                        .map(|field_def| (field_def.name, field_def.ty(tcx, substs)))
                        .collect();
                    record(adt_kind.into(),
                           None,
                           vec![build_variant_info(Some(variant_def.name),
                                                   &fields,
                                                   Fields::NoDiscrim(variant_layout))]);
                } else {
                    // (This case arises for *empty* enums; so give it
                    // zero variants.)
                    record(adt_kind.into(), None, vec![]);
                }
            }

            Layout::General { ref variants, discr, .. } => {
                debug!("print-type-size t: `{:?}` adt general variants def {} layouts {} {:?}",
                       ty, adt_def.variants.len(), variants.len(), variants);
                let variant_infos: Vec<_> = adt_def.variants.iter()
                    .zip(variants.iter())
                    .map(|(variant_def, variant_layout)| {
                        let fields: Vec<_> = variant_def.fields.iter()
                            .map(|field_def| (field_def.name, field_def.ty(tcx, substs)))
                            .collect();
                        build_variant_info(Some(variant_def.name),
                                           &fields,
                                           Fields::WithDiscrim(variant_layout))
                    })
                    .collect();
                record(adt_kind.into(), Some(discr.size()), variant_infos);
            }

            Layout::UntaggedUnion { ref variants } => {
                debug!("print-type-size t: `{:?}` adt union variants {:?}",
                       ty, variants);
                // layout does not currently store info about each
                // variant...
                record(adt_kind.into(), None, Vec::new());
            }

            Layout::CEnum { discr, .. } => {
                debug!("print-type-size t: `{:?}` adt c-like enum", ty);
                let variant_infos: Vec<_> = adt_def.variants.iter()
                    .map(|variant_def| {
                        build_primitive_info(variant_def.name,
                                             &layout::Primitive::Int(discr))
                    })
                    .collect();
                record(adt_kind.into(), Some(discr.size()), variant_infos);
            }

            // other cases provide little interesting (i.e. adjustable
            // via representation tweaks) size info beyond total size.
            Layout::Scalar { .. } |
            Layout::Vector { .. } |
            Layout::Array { .. } |
            Layout::FatPointer { .. } => {
                debug!("print-type-size t: `{:?}` adt other", ty);
                record(adt_kind.into(), None, Vec::new())
            }
        }
    }
}

/// For each CGU, identify if we can reuse an existing object file (or
/// maybe other context).
fn trans_reuse_previous_work_products(scx: &SharedCrateContext,
                                      codegen_units: &[CodegenUnit],
                                      symbol_map: &SymbolMap)
                                      -> Vec<Option<WorkProduct>> {
    debug!("trans_reuse_previous_work_products()");
    codegen_units
        .iter()
        .map(|cgu| {
            let id = cgu.work_product_id();

            let hash = cgu.compute_symbol_name_hash(scx, symbol_map);

            debug!("trans_reuse_previous_work_products: id={:?} hash={}", id, hash);

            if let Some(work_product) = scx.dep_graph().previous_work_product(&id) {
                if work_product.input_hash == hash {
                    debug!("trans_reuse_previous_work_products: reusing {:?}", work_product);
                    return Some(work_product);
                } else {
                    if scx.sess().opts.debugging_opts.incremental_info {
                        println!("incremental: CGU `{}` invalidated because of \
                                  changed partitioning hash.",
                                  cgu.name());
                    }
                    debug!("trans_reuse_previous_work_products: \
                            not reusing {:?} because hash changed to {:?}",
                           work_product, hash);
                }
            }

            None
        })
        .collect()
}

fn collect_and_partition_translation_items<'a, 'tcx>(scx: &SharedCrateContext<'a, 'tcx>)
                                                     -> (Vec<CodegenUnit<'tcx>>, SymbolMap<'tcx>) {
    let time_passes = scx.sess().time_passes();

    let collection_mode = match scx.sess().opts.debugging_opts.print_trans_items {
        Some(ref s) => {
            let mode_string = s.to_lowercase();
            let mode_string = mode_string.trim();
            if mode_string == "eager" {
                TransItemCollectionMode::Eager
            } else {
                if mode_string != "lazy" {
                    let message = format!("Unknown codegen-item collection mode '{}'. \
                                           Falling back to 'lazy' mode.",
                                           mode_string);
                    scx.sess().warn(&message);
                }

                TransItemCollectionMode::Lazy
            }
        }
        None => TransItemCollectionMode::Lazy
    };

    let (items, inlining_map) =
        time(time_passes, "translation item collection", || {
            collector::collect_crate_translation_items(&scx, collection_mode)
    });

    let symbol_map = SymbolMap::build(scx, items.iter().cloned());

    let strategy = if scx.sess().opts.debugging_opts.incremental.is_some() {
        PartitioningStrategy::PerModule
    } else {
        PartitioningStrategy::FixedUnitCount(scx.sess().opts.cg.codegen_units)
    };

    let codegen_units = time(time_passes, "codegen unit partitioning", || {
        partitioning::partition(scx,
                                items.iter().cloned(),
                                strategy,
                                &inlining_map)
    });

    assert!(scx.tcx().sess.opts.cg.codegen_units == codegen_units.len() ||
            scx.tcx().sess.opts.debugging_opts.incremental.is_some());

    {
        let mut ccx_map = scx.translation_items().borrow_mut();

        for trans_item in items.iter().cloned() {
            ccx_map.insert(trans_item);
        }
    }

    if scx.sess().opts.debugging_opts.print_trans_items.is_some() {
        let mut item_to_cgus = FxHashMap();

        for cgu in &codegen_units {
            for (&trans_item, &linkage) in cgu.items() {
                item_to_cgus.entry(trans_item)
                            .or_insert(Vec::new())
                            .push((cgu.name().clone(), linkage));
            }
        }

        let mut item_keys: Vec<_> = items
            .iter()
            .map(|i| {
                let mut output = i.to_string(scx.tcx());
                output.push_str(" @@");
                let mut empty = Vec::new();
                let mut cgus = item_to_cgus.get_mut(i).unwrap_or(&mut empty);
                cgus.as_mut_slice().sort_by_key(|&(ref name, _)| name.clone());
                cgus.dedup();
                for &(ref cgu_name, linkage) in cgus.iter() {
                    output.push_str(" ");
                    output.push_str(&cgu_name[..]);

                    let linkage_abbrev = match linkage {
                        llvm::Linkage::ExternalLinkage => "External",
                        llvm::Linkage::AvailableExternallyLinkage => "Available",
                        llvm::Linkage::LinkOnceAnyLinkage => "OnceAny",
                        llvm::Linkage::LinkOnceODRLinkage => "OnceODR",
                        llvm::Linkage::WeakAnyLinkage => "WeakAny",
                        llvm::Linkage::WeakODRLinkage => "WeakODR",
                        llvm::Linkage::AppendingLinkage => "Appending",
                        llvm::Linkage::InternalLinkage => "Internal",
                        llvm::Linkage::PrivateLinkage => "Private",
                        llvm::Linkage::ExternalWeakLinkage => "ExternalWeak",
                        llvm::Linkage::CommonLinkage => "Common",
                    };

                    output.push_str("[");
                    output.push_str(linkage_abbrev);
                    output.push_str("]");
                }
                output
            })
            .collect();

        item_keys.sort();

        for item in item_keys {
            println!("TRANS_ITEM {}", item);
        }
    }

    (codegen_units, symbol_map)
}
