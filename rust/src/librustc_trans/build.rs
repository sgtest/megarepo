// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(dead_code)] // FFI wrappers
#![allow(non_snake_case)]

use llvm;
use llvm::{AtomicBinOp, AtomicOrdering, SynchronizationScope, AsmDialect};
use llvm::{Opcode, IntPredicate, RealPredicate};
use llvm::{ValueRef, BasicBlockRef};
use common::*;
use syntax_pos::Span;

use builder::Builder;
use type_::Type;
use value::Value;
use debuginfo::DebugLoc;

use libc::{c_uint, c_char};

pub fn terminate(cx: Block, _: &str) {
    debug!("terminate({})", cx.to_str());
    cx.terminated.set(true);
}

pub fn check_not_terminated(cx: Block) {
    if cx.terminated.get() {
        bug!("already terminated!");
    }
}

pub fn B<'blk, 'tcx>(cx: Block<'blk, 'tcx>) -> Builder<'blk, 'tcx> {
    let b = cx.fcx.ccx.builder();
    b.position_at_end(cx.llbb);
    b
}

// The difference between a block being unreachable and being terminated is
// somewhat obscure, and has to do with error checking. When a block is
// terminated, we're saying that trying to add any further statements in the
// block is an error. On the other hand, if something is unreachable, that
// means that the block was terminated in some way that we don't want to check
// for (panic/break/return statements, call to diverging functions, etc), and
// further instructions to the block should simply be ignored.

pub fn RetVoid(cx: Block, debug_loc: DebugLoc) {
    if cx.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "RetVoid");
    debug_loc.apply(cx.fcx);
    B(cx).ret_void();
}

pub fn Ret(cx: Block, v: ValueRef, debug_loc: DebugLoc) {
    if cx.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "Ret");
    debug_loc.apply(cx.fcx);
    B(cx).ret(v);
}

pub fn AggregateRet(cx: Block,
                    ret_vals: &[ValueRef],
                    debug_loc: DebugLoc) {
    if cx.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "AggregateRet");
    debug_loc.apply(cx.fcx);
    B(cx).aggregate_ret(ret_vals);
}

pub fn Br(cx: Block, dest: BasicBlockRef, debug_loc: DebugLoc) {
    if cx.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "Br");
    debug_loc.apply(cx.fcx);
    B(cx).br(dest);
}

pub fn CondBr(cx: Block,
              if_: ValueRef,
              then: BasicBlockRef,
              else_: BasicBlockRef,
              debug_loc: DebugLoc) {
    if cx.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "CondBr");
    debug_loc.apply(cx.fcx);
    B(cx).cond_br(if_, then, else_);
}

pub fn Switch(cx: Block, v: ValueRef, else_: BasicBlockRef, num_cases: usize)
    -> ValueRef {
    if cx.unreachable.get() { return _Undef(v); }
    check_not_terminated(cx);
    terminate(cx, "Switch");
    B(cx).switch(v, else_, num_cases)
}

pub fn AddCase(s: ValueRef, on_val: ValueRef, dest: BasicBlockRef) {
    unsafe {
        if llvm::LLVMIsUndef(s) == llvm::True { return; }
        llvm::LLVMAddCase(s, on_val, dest);
    }
}

pub fn IndirectBr(cx: Block,
                  addr: ValueRef,
                  num_dests: usize,
                  debug_loc: DebugLoc) {
    if cx.unreachable.get() {
        return;
    }
    check_not_terminated(cx);
    terminate(cx, "IndirectBr");
    debug_loc.apply(cx.fcx);
    B(cx).indirect_br(addr, num_dests);
}

pub fn Invoke(cx: Block,
              fn_: ValueRef,
              args: &[ValueRef],
              then: BasicBlockRef,
              catch: BasicBlockRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.unreachable.get() {
        return C_null(Type::i8(cx.ccx()));
    }
    check_not_terminated(cx);
    terminate(cx, "Invoke");
    debug!("Invoke({:?} with arguments ({}))",
           Value(fn_),
           args.iter().map(|a| {
                format!("{:?}", Value(*a))
           }).collect::<Vec<String>>().join(", "));
    debug_loc.apply(cx.fcx);
    let bundle = cx.lpad().and_then(|b| b.bundle());
    B(cx).invoke(fn_, args, then, catch, bundle)
}

pub fn Unreachable(cx: Block) {
    if cx.unreachable.get() {
        return
    }
    cx.unreachable.set(true);
    if !cx.terminated.get() {
        B(cx).unreachable();
    }
}

pub fn _Undef(val: ValueRef) -> ValueRef {
    unsafe {
        return llvm::LLVMGetUndef(val_ty(val).to_ref());
    }
}

/* Arithmetic */
pub fn Add(cx: Block,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).add(lhs, rhs)
}

pub fn NSWAdd(cx: Block,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nswadd(lhs, rhs)
}

pub fn NUWAdd(cx: Block,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nuwadd(lhs, rhs)
}

pub fn FAdd(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fadd(lhs, rhs)
}

pub fn FAddFast(cx: Block,
                lhs: ValueRef,
                rhs: ValueRef,
                debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fadd_fast(lhs, rhs)
}

pub fn Sub(cx: Block,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).sub(lhs, rhs)
}

pub fn NSWSub(cx: Block,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nswsub(lhs, rhs)
}

pub fn NUWSub(cx: Block,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nuwsub(lhs, rhs)
}

pub fn FSub(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fsub(lhs, rhs)
}

pub fn FSubFast(cx: Block,
                lhs: ValueRef,
                rhs: ValueRef,
                debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fsub_fast(lhs, rhs)
}

pub fn Mul(cx: Block,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).mul(lhs, rhs)
}

pub fn NSWMul(cx: Block,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nswmul(lhs, rhs)
}

pub fn NUWMul(cx: Block,
              lhs: ValueRef,
              rhs: ValueRef,
              debug_loc: DebugLoc)
              -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nuwmul(lhs, rhs)
}

pub fn FMul(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fmul(lhs, rhs)
}

pub fn FMulFast(cx: Block,
                lhs: ValueRef,
                rhs: ValueRef,
                debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fmul_fast(lhs, rhs)
}

pub fn UDiv(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).udiv(lhs, rhs)
}

pub fn SDiv(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).sdiv(lhs, rhs)
}

pub fn ExactSDiv(cx: Block,
                 lhs: ValueRef,
                 rhs: ValueRef,
                 debug_loc: DebugLoc)
                 -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).exactsdiv(lhs, rhs)
}

pub fn FDiv(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fdiv(lhs, rhs)
}

pub fn FDivFast(cx: Block,
                lhs: ValueRef,
                rhs: ValueRef,
                debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fdiv_fast(lhs, rhs)
}

pub fn URem(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).urem(lhs, rhs)
}

pub fn SRem(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).srem(lhs, rhs)
}

pub fn FRem(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).frem(lhs, rhs)
}

pub fn FRemFast(cx: Block,
                lhs: ValueRef,
                rhs: ValueRef,
                debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).frem_fast(lhs, rhs)
}

pub fn Shl(cx: Block,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).shl(lhs, rhs)
}

pub fn LShr(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).lshr(lhs, rhs)
}

pub fn AShr(cx: Block,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).ashr(lhs, rhs)
}

pub fn And(cx: Block,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).and(lhs, rhs)
}

pub fn Or(cx: Block,
          lhs: ValueRef,
          rhs: ValueRef,
          debug_loc: DebugLoc)
          -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).or(lhs, rhs)
}

pub fn Xor(cx: Block,
           lhs: ValueRef,
           rhs: ValueRef,
           debug_loc: DebugLoc)
           -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).xor(lhs, rhs)
}

pub fn BinOp(cx: Block,
             op: Opcode,
             lhs: ValueRef,
             rhs: ValueRef,
             debug_loc: DebugLoc)
          -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(lhs);
    }
    debug_loc.apply(cx.fcx);
    B(cx).binop(op, lhs, rhs)
}

pub fn Neg(cx: Block, v: ValueRef, debug_loc: DebugLoc) -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(v);
    }
    debug_loc.apply(cx.fcx);
    B(cx).neg(v)
}

pub fn NSWNeg(cx: Block, v: ValueRef, debug_loc: DebugLoc) -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(v);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nswneg(v)
}

pub fn NUWNeg(cx: Block, v: ValueRef, debug_loc: DebugLoc) -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(v);
    }
    debug_loc.apply(cx.fcx);
    B(cx).nuwneg(v)
}
pub fn FNeg(cx: Block, v: ValueRef, debug_loc: DebugLoc) -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(v);
    }
    debug_loc.apply(cx.fcx);
    B(cx).fneg(v)
}

pub fn Not(cx: Block, v: ValueRef, debug_loc: DebugLoc) -> ValueRef {
    if cx.unreachable.get() {
        return _Undef(v);
    }
    debug_loc.apply(cx.fcx);
    B(cx).not(v)
}

pub fn Alloca(cx: Block, ty: Type, name: &str) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(ty.ptr_to().to_ref()); }
        AllocaFcx(cx.fcx, ty, name)
    }
}

pub fn AllocaFcx(fcx: &FunctionContext, ty: Type, name: &str) -> ValueRef {
    let b = fcx.ccx.builder();
    b.position_before(fcx.alloca_insert_pt.get().unwrap());
    DebugLoc::None.apply(fcx);
    b.alloca(ty, name)
}

pub fn Free(cx: Block, pointer_val: ValueRef) {
    if cx.unreachable.get() { return; }
    B(cx).free(pointer_val)
}

pub fn Load(cx: Block, pointer_val: ValueRef) -> ValueRef {
    unsafe {
        let ccx = cx.fcx.ccx;
        if cx.unreachable.get() {
            let ty = val_ty(pointer_val);
            let eltty = if ty.kind() == llvm::Array {
                ty.element_type()
            } else {
                ccx.int_type()
            };
            return llvm::LLVMGetUndef(eltty.to_ref());
        }
        B(cx).load(pointer_val)
    }
}

pub fn VolatileLoad(cx: Block, pointer_val: ValueRef) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).volatile_load(pointer_val)
    }
}

pub fn AtomicLoad(cx: Block, pointer_val: ValueRef, order: AtomicOrdering) -> ValueRef {
    unsafe {
        let ccx = cx.fcx.ccx;
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(ccx.int_type().to_ref());
        }
        B(cx).atomic_load(pointer_val, order)
    }
}


pub fn LoadRangeAssert(cx: Block, pointer_val: ValueRef, lo: u64,
                       hi: u64, signed: llvm::Bool) -> ValueRef {
    if cx.unreachable.get() {
        let ccx = cx.fcx.ccx;
        let ty = val_ty(pointer_val);
        let eltty = if ty.kind() == llvm::Array {
            ty.element_type()
        } else {
            ccx.int_type()
        };
        unsafe {
            llvm::LLVMGetUndef(eltty.to_ref())
        }
    } else {
        B(cx).load_range_assert(pointer_val, lo, hi, signed)
    }
}

pub fn LoadNonNull(cx: Block, ptr: ValueRef) -> ValueRef {
    if cx.unreachable.get() {
        let ccx = cx.fcx.ccx;
        let ty = val_ty(ptr);
        let eltty = if ty.kind() == llvm::Array {
            ty.element_type()
        } else {
            ccx.int_type()
        };
        unsafe {
            llvm::LLVMGetUndef(eltty.to_ref())
        }
    } else {
        B(cx).load_nonnull(ptr)
    }
}

pub fn Store(cx: Block, val: ValueRef, ptr: ValueRef) -> ValueRef {
    if cx.unreachable.get() { return C_nil(cx.ccx()); }
    B(cx).store(val, ptr)
}

pub fn VolatileStore(cx: Block, val: ValueRef, ptr: ValueRef) -> ValueRef {
    if cx.unreachable.get() { return C_nil(cx.ccx()); }
    B(cx).volatile_store(val, ptr)
}

pub fn AtomicStore(cx: Block, val: ValueRef, ptr: ValueRef, order: AtomicOrdering) {
    if cx.unreachable.get() { return; }
    B(cx).atomic_store(val, ptr, order)
}

pub fn GEP(cx: Block, pointer: ValueRef, indices: &[ValueRef]) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).ptr_to().to_ref());
        }
        B(cx).gep(pointer, indices)
    }
}

// Simple wrapper around GEP that takes an array of ints and wraps them
// in C_i32()
#[inline]
pub fn GEPi(cx: Block, base: ValueRef, ixs: &[usize]) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).ptr_to().to_ref());
        }
        B(cx).gepi(base, ixs)
    }
}

pub fn InBoundsGEP(cx: Block, pointer: ValueRef, indices: &[ValueRef]) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).ptr_to().to_ref());
        }
        B(cx).inbounds_gep(pointer, indices)
    }
}

pub fn StructGEP(cx: Block, pointer: ValueRef, idx: usize) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).ptr_to().to_ref());
        }
        B(cx).struct_gep(pointer, idx)
    }
}

pub fn GlobalString(cx: Block, _str: *const c_char) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i8p(cx.ccx()).to_ref());
        }
        B(cx).global_string(_str)
    }
}

pub fn GlobalStringPtr(cx: Block, _str: *const c_char) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i8p(cx.ccx()).to_ref());
        }
        B(cx).global_string_ptr(_str)
    }
}

/* Casts */
pub fn Trunc(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).trunc(val, dest_ty)
    }
}

pub fn ZExt(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).zext(val, dest_ty)
    }
}

pub fn SExt(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).sext(val, dest_ty)
    }
}

pub fn FPToUI(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).fptoui(val, dest_ty)
    }
}

pub fn FPToSI(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).fptosi(val, dest_ty)
    }
}

pub fn UIToFP(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).uitofp(val, dest_ty)
    }
}

pub fn SIToFP(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).sitofp(val, dest_ty)
    }
}

pub fn FPTrunc(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).fptrunc(val, dest_ty)
    }
}

pub fn FPExt(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).fpext(val, dest_ty)
    }
}

pub fn PtrToInt(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).ptrtoint(val, dest_ty)
    }
}

pub fn IntToPtr(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).inttoptr(val, dest_ty)
    }
}

pub fn BitCast(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).bitcast(val, dest_ty)
    }
}

pub fn ZExtOrBitCast(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).zext_or_bitcast(val, dest_ty)
    }
}

pub fn SExtOrBitCast(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).sext_or_bitcast(val, dest_ty)
    }
}

pub fn TruncOrBitCast(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).trunc_or_bitcast(val, dest_ty)
    }
}

pub fn Cast(cx: Block, op: Opcode, val: ValueRef, dest_ty: Type,
            _: *const u8)
     -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).cast(op, val, dest_ty)
    }
}

pub fn PointerCast(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).pointercast(val, dest_ty)
    }
}

pub fn IntCast(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).intcast(val, dest_ty)
    }
}

pub fn FPCast(cx: Block, val: ValueRef, dest_ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(dest_ty.to_ref()); }
        B(cx).fpcast(val, dest_ty)
    }
}


/* Comparisons */
pub fn ICmp(cx: Block,
            op: IntPredicate,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i1(cx.ccx()).to_ref());
        }
        debug_loc.apply(cx.fcx);
        B(cx).icmp(op, lhs, rhs)
    }
}

pub fn FCmp(cx: Block,
            op: RealPredicate,
            lhs: ValueRef,
            rhs: ValueRef,
            debug_loc: DebugLoc)
            -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i1(cx.ccx()).to_ref());
        }
        debug_loc.apply(cx.fcx);
        B(cx).fcmp(op, lhs, rhs)
    }
}

/* Miscellaneous instructions */
pub fn EmptyPhi(cx: Block, ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(ty.to_ref()); }
        B(cx).empty_phi(ty)
    }
}

pub fn Phi(cx: Block, ty: Type, vals: &[ValueRef],
           bbs: &[BasicBlockRef]) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(ty.to_ref()); }
        B(cx).phi(ty, vals, bbs)
    }
}

pub fn AddIncomingToPhi(phi: ValueRef, val: ValueRef, bb: BasicBlockRef) {
    unsafe {
        if llvm::LLVMIsUndef(phi) == llvm::True { return; }
        llvm::LLVMAddIncoming(phi, &val, &bb, 1 as c_uint);
    }
}

pub fn _UndefReturn(cx: Block, fn_: ValueRef) -> ValueRef {
    unsafe {
        let ccx = cx.fcx.ccx;
        let ty = val_ty(fn_);
        let retty = if ty.kind() == llvm::Function {
            ty.return_type()
        } else {
            ccx.int_type()
        };
        B(cx).count_insn("ret_undef");
        llvm::LLVMGetUndef(retty.to_ref())
    }
}

pub fn add_span_comment(cx: Block, sp: Span, text: &str) {
    B(cx).add_span_comment(sp, text)
}

pub fn add_comment(cx: Block, text: &str) {
    B(cx).add_comment(text)
}

pub fn InlineAsmCall(cx: Block, asm: *const c_char, cons: *const c_char,
                     inputs: &[ValueRef], output: Type,
                     volatile: bool, alignstack: bool,
                     dia: AsmDialect) -> ValueRef {
    B(cx).inline_asm_call(asm, cons, inputs, output, volatile, alignstack, dia)
}

pub fn Call(cx: Block,
            fn_: ValueRef,
            args: &[ValueRef],
            debug_loc: DebugLoc)
            -> ValueRef {
    if cx.unreachable.get() {
        return _UndefReturn(cx, fn_);
    }
    debug_loc.apply(cx.fcx);
    let bundle = cx.lpad.get().and_then(|b| b.bundle());
    B(cx).call(fn_, args, bundle)
}

pub fn AtomicFence(cx: Block, order: AtomicOrdering, scope: SynchronizationScope) {
    if cx.unreachable.get() { return; }
    B(cx).atomic_fence(order, scope)
}

pub fn Select(cx: Block, if_: ValueRef, then: ValueRef, else_: ValueRef) -> ValueRef {
    if cx.unreachable.get() { return _Undef(then); }
    B(cx).select(if_, then, else_)
}

pub fn VAArg(cx: Block, list: ValueRef, ty: Type) -> ValueRef {
    unsafe {
        if cx.unreachable.get() { return llvm::LLVMGetUndef(ty.to_ref()); }
        B(cx).va_arg(list, ty)
    }
}

pub fn ExtractElement(cx: Block, vec_val: ValueRef, index: ValueRef) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).extract_element(vec_val, index)
    }
}

pub fn InsertElement(cx: Block, vec_val: ValueRef, elt_val: ValueRef,
                     index: ValueRef) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).insert_element(vec_val, elt_val, index)
    }
}

pub fn ShuffleVector(cx: Block, v1: ValueRef, v2: ValueRef,
                     mask: ValueRef) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).shuffle_vector(v1, v2, mask)
    }
}

pub fn VectorSplat(cx: Block, num_elts: usize, elt_val: ValueRef) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).vector_splat(num_elts, elt_val)
    }
}

pub fn ExtractValue(cx: Block, agg_val: ValueRef, index: usize) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).extract_value(agg_val, index)
    }
}

pub fn InsertValue(cx: Block, agg_val: ValueRef, elt_val: ValueRef, index: usize) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::nil(cx.ccx()).to_ref());
        }
        B(cx).insert_value(agg_val, elt_val, index)
    }
}

pub fn IsNull(cx: Block, val: ValueRef) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i1(cx.ccx()).to_ref());
        }
        B(cx).is_null(val)
    }
}

pub fn IsNotNull(cx: Block, val: ValueRef) -> ValueRef {
    unsafe {
        if cx.unreachable.get() {
            return llvm::LLVMGetUndef(Type::i1(cx.ccx()).to_ref());
        }
        B(cx).is_not_null(val)
    }
}

pub fn PtrDiff(cx: Block, lhs: ValueRef, rhs: ValueRef) -> ValueRef {
    unsafe {
        let ccx = cx.fcx.ccx;
        if cx.unreachable.get() { return llvm::LLVMGetUndef(ccx.int_type().to_ref()); }
        B(cx).ptrdiff(lhs, rhs)
    }
}

pub fn Trap(cx: Block) {
    if cx.unreachable.get() { return; }
    B(cx).trap();
}

pub fn LandingPad(cx: Block, ty: Type, pers_fn: ValueRef,
                  num_clauses: usize) -> ValueRef {
    check_not_terminated(cx);
    assert!(!cx.unreachable.get());
    B(cx).landing_pad(ty, pers_fn, num_clauses, cx.fcx.llfn)
}

pub fn AddClause(cx: Block, landing_pad: ValueRef, clause: ValueRef) {
    B(cx).add_clause(landing_pad, clause)
}

pub fn SetCleanup(cx: Block, landing_pad: ValueRef) {
    B(cx).set_cleanup(landing_pad)
}

pub fn SetPersonalityFn(cx: Block, f: ValueRef) {
    B(cx).set_personality_fn(f)
}

pub fn Resume(cx: Block, exn: ValueRef) -> ValueRef {
    check_not_terminated(cx);
    terminate(cx, "Resume");
    B(cx).resume(exn)
}

// Atomic Operations
pub fn AtomicCmpXchg(cx: Block, dst: ValueRef,
                     cmp: ValueRef, src: ValueRef,
                     order: AtomicOrdering,
                     failure_order: AtomicOrdering,
                     weak: llvm::Bool) -> ValueRef {
    B(cx).atomic_cmpxchg(dst, cmp, src, order, failure_order, weak)
}
pub fn AtomicRMW(cx: Block, op: AtomicBinOp,
                 dst: ValueRef, src: ValueRef,
                 order: AtomicOrdering) -> ValueRef {
    B(cx).atomic_rmw(op, dst, src, order)
}

pub fn CleanupPad(cx: Block,
                  parent: Option<ValueRef>,
                  args: &[ValueRef]) -> ValueRef {
    check_not_terminated(cx);
    assert!(!cx.unreachable.get());
    B(cx).cleanup_pad(parent, args)
}

pub fn CleanupRet(cx: Block,
                  cleanup: ValueRef,
                  unwind: Option<BasicBlockRef>) -> ValueRef {
    check_not_terminated(cx);
    terminate(cx, "CleanupRet");
    B(cx).cleanup_ret(cleanup, unwind)
}

pub fn CatchPad(cx: Block,
                parent: ValueRef,
                args: &[ValueRef]) -> ValueRef {
    check_not_terminated(cx);
    assert!(!cx.unreachable.get());
    B(cx).catch_pad(parent, args)
}

pub fn CatchRet(cx: Block, pad: ValueRef, unwind: BasicBlockRef) -> ValueRef {
    check_not_terminated(cx);
    terminate(cx, "CatchRet");
    B(cx).catch_ret(pad, unwind)
}

pub fn CatchSwitch(cx: Block,
                   parent: Option<ValueRef>,
                   unwind: Option<BasicBlockRef>,
                   num_handlers: usize) -> ValueRef {
    check_not_terminated(cx);
    terminate(cx, "CatchSwitch");
    B(cx).catch_switch(parent, unwind, num_handlers)
}

pub fn AddHandler(cx: Block, catch_switch: ValueRef, handler: BasicBlockRef) {
    B(cx).add_handler(catch_switch, handler)
}
