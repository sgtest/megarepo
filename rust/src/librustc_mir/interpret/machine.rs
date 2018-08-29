// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This module contains everything needed to instantiate an interpreter.
//! This separation exists to ensure that no fancy miri features like
//! interpreting common C functions leak into CTFE.

use std::hash::Hash;

use rustc::hir::def_id::DefId;
use rustc::mir::interpret::{Allocation, EvalResult, Scalar};
use rustc::mir;
use rustc::ty::{self, layout::TyLayout, query::TyCtxtAt};

use super::{EvalContext, PlaceTy, OpTy};

/// Methods of this trait signifies a point where CTFE evaluation would fail
/// and some use case dependent behaviour can instead be applied
pub trait Machine<'mir, 'tcx>: Clone + Eq + Hash {
    /// Additional data that can be accessed via the Memory
    type MemoryData: Clone + Eq + Hash;

    /// Additional memory kinds a machine wishes to distinguish from the builtin ones
    type MemoryKinds: ::std::fmt::Debug + Copy + Clone + Eq + Hash;

    /// The memory kind to use for mutated statics -- or None if those are not supported.
    const MUT_STATIC_KIND: Option<Self::MemoryKinds>;

    /// Entry point to all function calls.
    ///
    /// Returns either the mir to use for the call, or `None` if execution should
    /// just proceed (which usually means this hook did all the work that the
    /// called function should usually have done).  In the latter case, it is
    /// this hook's responsibility to call `goto_block(ret)` to advance the instruction pointer!
    /// (This is to support functions like `__rust_maybe_catch_panic` that neither find a MIR
    /// nor just jump to `ret`, but instead push their own stack frame.)
    /// Passing `dest`and `ret` in the same `Option` proved very annoying when only one of them
    /// was used.
    fn find_fn<'a>(
        ecx: &mut EvalContext<'a, 'mir, 'tcx, Self>,
        instance: ty::Instance<'tcx>,
        args: &[OpTy<'tcx>],
        dest: Option<PlaceTy<'tcx>>,
        ret: Option<mir::BasicBlock>,
    ) -> EvalResult<'tcx, Option<&'mir mir::Mir<'tcx>>>;

    /// Directly process an intrinsic without pushing a stack frame.
    /// If this returns successfully, the engine will take care of jumping to the next block.
    fn call_intrinsic<'a>(
        ecx: &mut EvalContext<'a, 'mir, 'tcx, Self>,
        instance: ty::Instance<'tcx>,
        args: &[OpTy<'tcx>],
        dest: PlaceTy<'tcx>,
    ) -> EvalResult<'tcx>;

    /// Called for read access to a foreign static item.
    /// This can be called multiple times for the same static item and should return consistent
    /// results.  Once the item is *written* the first time, as usual for statics a copy is
    /// made and this function is not called again.
    fn find_foreign_static<'a>(
        tcx: TyCtxtAt<'a, 'tcx, 'tcx>,
        def_id: DefId,
    ) -> EvalResult<'tcx, &'tcx Allocation>;

    /// Called for all binary operations except on float types.
    ///
    /// Returns `None` if the operation should be handled by the integer
    /// op code in order to share more code between machines
    ///
    /// Returns a (value, overflowed) pair if the operation succeeded
    fn try_ptr_op<'a>(
        ecx: &EvalContext<'a, 'mir, 'tcx, Self>,
        bin_op: mir::BinOp,
        left: Scalar,
        left_layout: TyLayout<'tcx>,
        right: Scalar,
        right_layout: TyLayout<'tcx>,
    ) -> EvalResult<'tcx, Option<(Scalar, bool)>>;

    /// Heap allocations via the `box` keyword
    ///
    /// Returns a pointer to the allocated memory
    fn box_alloc<'a>(
        ecx: &mut EvalContext<'a, 'mir, 'tcx, Self>,
        dest: PlaceTy<'tcx>,
    ) -> EvalResult<'tcx>;

    /// Execute a validation operation
    fn validation_op<'a>(
        _ecx: &mut EvalContext<'a, 'mir, 'tcx, Self>,
        _op: ::rustc::mir::ValidationOp,
        _operand: &::rustc::mir::ValidationOperand<'tcx, ::rustc::mir::Place<'tcx>>,
    ) -> EvalResult<'tcx> {
        Ok(())
    }
}
