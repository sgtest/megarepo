//! This module contains everything needed to instantiate an interpreter.
//! This separation exists to ensure that no fancy miri features like
//! interpreting common C functions leak into CTFE.

use rustc::mir::interpret::{EvalResult, PrimVal, MemoryPointer, AccessKind};
use super::{EvalContext, Place, ValTy, Memory};

use rustc::mir;
use rustc::ty::{self, Ty};
use syntax::codemap::Span;
use syntax::ast::Mutability;

/// Methods of this trait signifies a point where CTFE evaluation would fail
/// and some use case dependent behaviour can instead be applied
pub trait Machine<'tcx>: Sized {
    /// Additional data that can be accessed via the Memory
    type MemoryData;

    /// Additional memory kinds a machine wishes to distinguish from the builtin ones
    type MemoryKinds: ::std::fmt::Debug + PartialEq + Copy + Clone;

    /// Entry point to all function calls.
    ///
    /// Returns Ok(true) when the function was handled completely
    /// e.g. due to missing mir
    ///
    /// Returns Ok(false) if a new stack frame was pushed
    fn eval_fn_call<'a>(
        ecx: &mut EvalContext<'a, 'tcx, Self>,
        instance: ty::Instance<'tcx>,
        destination: Option<(Place, mir::BasicBlock)>,
        args: &[ValTy<'tcx>],
        span: Span,
        sig: ty::FnSig<'tcx>,
    ) -> EvalResult<'tcx, bool>;

    /// directly process an intrinsic without pushing a stack frame.
    fn call_intrinsic<'a>(
        ecx: &mut EvalContext<'a, 'tcx, Self>,
        instance: ty::Instance<'tcx>,
        args: &[ValTy<'tcx>],
        dest: Place,
        dest_layout: ty::layout::TyLayout<'tcx>,
        target: mir::BasicBlock,
    ) -> EvalResult<'tcx>;

    /// Called for all binary operations except on float types.
    ///
    /// Returns `None` if the operation should be handled by the integer
    /// op code in order to share more code between machines
    ///
    /// Returns a (value, overflowed) pair if the operation succeeded
    fn try_ptr_op<'a>(
        ecx: &EvalContext<'a, 'tcx, Self>,
        bin_op: mir::BinOp,
        left: PrimVal,
        left_ty: Ty<'tcx>,
        right: PrimVal,
        right_ty: Ty<'tcx>,
    ) -> EvalResult<'tcx, Option<(PrimVal, bool)>>;

    /// Called when trying to mark machine defined `MemoryKinds` as static
    fn mark_static_initialized(m: Self::MemoryKinds) -> EvalResult<'tcx>;

    /// Heap allocations via the `box` keyword
    ///
    /// Returns a pointer to the allocated memory
    fn box_alloc<'a>(
        ecx: &mut EvalContext<'a, 'tcx, Self>,
        ty: Ty<'tcx>,
        dest: Place,
    ) -> EvalResult<'tcx>;

    /// Called when trying to access a global declared with a `linkage` attribute
    fn global_item_with_linkage<'a>(
        ecx: &mut EvalContext<'a, 'tcx, Self>,
        instance: ty::Instance<'tcx>,
        mutability: Mutability,
    ) -> EvalResult<'tcx>;

    fn check_locks<'a>(
        _mem: &Memory<'a, 'tcx, Self>,
        _ptr: MemoryPointer,
        _size: u64,
        _access: AccessKind,
    ) -> EvalResult<'tcx> {
        Ok(())
    }

    fn add_lock<'a>(
        _mem: &mut Memory<'a, 'tcx, Self>,
        _id: u64,
    ) {}

    fn free_lock<'a>(
        _mem: &mut Memory<'a, 'tcx, Self>,
        _id: u64,
        _len: u64,
    ) -> EvalResult<'tcx> {
        Ok(())
    }

    fn end_region<'a>(
        _ecx: &mut EvalContext<'a, 'tcx, Self>,
        _reg: Option<::rustc::middle::region::Scope>,
    ) -> EvalResult<'tcx> {
        Ok(())
    }

    fn validation_op<'a>(
        _ecx: &mut EvalContext<'a, 'tcx, Self>,
        _op: ::rustc::mir::ValidationOp,
        _operand: &::rustc::mir::ValidationOperand<'tcx, ::rustc::mir::Place<'tcx>>,
    ) -> EvalResult<'tcx> {
        Ok(())
    }
}
