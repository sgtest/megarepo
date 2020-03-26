//! This module contains everything needed to instantiate an interpreter.
//! This separation exists to ensure that no fancy miri features like
//! interpreting common C functions leak into CTFE.

use std::borrow::{Borrow, Cow};
use std::hash::Hash;

use rustc::mir;
use rustc::ty::{self, Ty};
use rustc_span::{def_id::DefId, Span};

use super::{
    AllocId, Allocation, AllocationExtra, Frame, ImmTy, InterpCx, InterpResult, Memory, MemoryKind,
    OpTy, Operand, PlaceTy, Pointer, Scalar,
};

/// Data returned by Machine::stack_pop,
/// to provide further control over the popping of the stack frame
#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum StackPopJump {
    /// Indicates that no special handling should be
    /// done - we'll either return normally or unwind
    /// based on the terminator for the function
    /// we're leaving.
    Normal,

    /// Indicates that we should *not* jump to the return/unwind address, as the callback already
    /// took care of everything.
    NoJump,
}

/// Whether this kind of memory is allowed to leak
pub trait MayLeak: Copy {
    fn may_leak(self) -> bool;
}

/// The functionality needed by memory to manage its allocations
pub trait AllocMap<K: Hash + Eq, V> {
    /// Tests if the map contains the given key.
    /// Deliberately takes `&mut` because that is sufficient, and some implementations
    /// can be more efficient then (using `RefCell::get_mut`).
    fn contains_key<Q: ?Sized + Hash + Eq>(&mut self, k: &Q) -> bool
    where
        K: Borrow<Q>;

    /// Inserts a new entry into the map.
    fn insert(&mut self, k: K, v: V) -> Option<V>;

    /// Removes an entry from the map.
    fn remove<Q: ?Sized + Hash + Eq>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>;

    /// Returns data based the keys and values in the map.
    fn filter_map_collect<T>(&self, f: impl FnMut(&K, &V) -> Option<T>) -> Vec<T>;

    /// Returns a reference to entry `k`. If no such entry exists, call
    /// `vacant` and either forward its error, or add its result to the map
    /// and return a reference to *that*.
    fn get_or<E>(&self, k: K, vacant: impl FnOnce() -> Result<V, E>) -> Result<&V, E>;

    /// Returns a mutable reference to entry `k`. If no such entry exists, call
    /// `vacant` and either forward its error, or add its result to the map
    /// and return a reference to *that*.
    fn get_mut_or<E>(&mut self, k: K, vacant: impl FnOnce() -> Result<V, E>) -> Result<&mut V, E>;

    /// Read-only lookup.
    fn get(&self, k: K) -> Option<&V> {
        self.get_or(k, || Err(())).ok()
    }

    /// Mutable lookup.
    fn get_mut(&mut self, k: K) -> Option<&mut V> {
        self.get_mut_or(k, || Err(())).ok()
    }
}

/// Methods of this trait signifies a point where CTFE evaluation would fail
/// and some use case dependent behaviour can instead be applied.
pub trait Machine<'mir, 'tcx>: Sized {
    /// Additional memory kinds a machine wishes to distinguish from the builtin ones
    type MemoryKind: ::std::fmt::Debug + MayLeak + Eq + 'static;

    /// Tag tracked alongside every pointer. This is used to implement "Stacked Borrows"
    /// <https://www.ralfj.de/blog/2018/08/07/stacked-borrows.html>.
    /// The `default()` is used for pointers to consts, statics, vtables and functions.
    type PointerTag: ::std::fmt::Debug + Copy + Eq + Hash + 'static;

    /// Machines can define extra (non-instance) things that represent values of function pointers.
    /// For example, Miri uses this to return a function pointer from `dlsym`
    /// that can later be called to execute the right thing.
    type ExtraFnVal: ::std::fmt::Debug + Copy;

    /// Extra data stored in every call frame.
    type FrameExtra;

    /// Extra data stored in memory. A reference to this is available when `AllocExtra`
    /// gets initialized, so you can e.g., have an `Rc` here if there is global state you
    /// need access to in the `AllocExtra` hooks.
    type MemoryExtra;

    /// Extra data stored in every allocation.
    type AllocExtra: AllocationExtra<Self::PointerTag> + 'static;

    /// Memory's allocation map
    type MemoryMap: AllocMap<
            AllocId,
            (MemoryKind<Self::MemoryKind>, Allocation<Self::PointerTag, Self::AllocExtra>),
        > + Default
        + Clone;

    /// The memory kind to use for copied global memory (held in `tcx`) --
    /// or None if such memory should not be mutated and thus any such attempt will cause
    /// a `ModifiedStatic` error to be raised.
    /// Statics are copied under two circumstances: When they are mutated, and when
    /// `tag_allocation` (see below) returns an owned allocation
    /// that is added to the memory so that the work is not done twice.
    const GLOBAL_KIND: Option<Self::MemoryKind>;

    /// Whether memory accesses should be alignment-checked.
    const CHECK_ALIGN: bool;

    /// Whether to enforce the validity invariant
    fn enforce_validity(ecx: &InterpCx<'mir, 'tcx, Self>) -> bool;

    /// Entry point to all function calls.
    ///
    /// Returns either the mir to use for the call, or `None` if execution should
    /// just proceed (which usually means this hook did all the work that the
    /// called function should usually have done). In the latter case, it is
    /// this hook's responsibility to advance the instruction pointer!
    /// (This is to support functions like `__rust_maybe_catch_panic` that neither find a MIR
    /// nor just jump to `ret`, but instead push their own stack frame.)
    /// Passing `dest`and `ret` in the same `Option` proved very annoying when only one of them
    /// was used.
    fn find_mir_or_eval_fn(
        ecx: &mut InterpCx<'mir, 'tcx, Self>,
        span: Span,
        instance: ty::Instance<'tcx>,
        args: &[OpTy<'tcx, Self::PointerTag>],
        ret: Option<(PlaceTy<'tcx, Self::PointerTag>, mir::BasicBlock)>,
        unwind: Option<mir::BasicBlock>,
    ) -> InterpResult<'tcx, Option<&'mir mir::Body<'tcx>>>;

    /// Execute `fn_val`.  It is the hook's responsibility to advance the instruction
    /// pointer as appropriate.
    fn call_extra_fn(
        ecx: &mut InterpCx<'mir, 'tcx, Self>,
        fn_val: Self::ExtraFnVal,
        args: &[OpTy<'tcx, Self::PointerTag>],
        ret: Option<(PlaceTy<'tcx, Self::PointerTag>, mir::BasicBlock)>,
        unwind: Option<mir::BasicBlock>,
    ) -> InterpResult<'tcx>;

    /// Directly process an intrinsic without pushing a stack frame. It is the hook's
    /// responsibility to advance the instruction pointer as appropriate.
    fn call_intrinsic(
        ecx: &mut InterpCx<'mir, 'tcx, Self>,
        span: Span,
        instance: ty::Instance<'tcx>,
        args: &[OpTy<'tcx, Self::PointerTag>],
        ret: Option<(PlaceTy<'tcx, Self::PointerTag>, mir::BasicBlock)>,
        unwind: Option<mir::BasicBlock>,
    ) -> InterpResult<'tcx>;

    /// Called to evaluate `Assert` MIR terminators that trigger a panic.
    fn assert_panic(
        ecx: &mut InterpCx<'mir, 'tcx, Self>,
        msg: &mir::AssertMessage<'tcx>,
        unwind: Option<mir::BasicBlock>,
    ) -> InterpResult<'tcx>;

    /// Called to evaluate `Abort` MIR terminator.
    fn abort(_ecx: &mut InterpCx<'mir, 'tcx, Self>) -> InterpResult<'tcx, !> {
        throw_unsup_format!("aborting execution is not supported")
    }

    /// Called for all binary operations where the LHS has pointer type.
    ///
    /// Returns a (value, overflowed) pair if the operation succeeded
    fn binary_ptr_op(
        ecx: &InterpCx<'mir, 'tcx, Self>,
        bin_op: mir::BinOp,
        left: ImmTy<'tcx, Self::PointerTag>,
        right: ImmTy<'tcx, Self::PointerTag>,
    ) -> InterpResult<'tcx, (Scalar<Self::PointerTag>, bool, Ty<'tcx>)>;

    /// Heap allocations via the `box` keyword.
    fn box_alloc(
        ecx: &mut InterpCx<'mir, 'tcx, Self>,
        dest: PlaceTy<'tcx, Self::PointerTag>,
    ) -> InterpResult<'tcx>;

    /// Called to read the specified `local` from the `frame`.
    #[inline]
    fn access_local(
        _ecx: &InterpCx<'mir, 'tcx, Self>,
        frame: &Frame<'mir, 'tcx, Self::PointerTag, Self::FrameExtra>,
        local: mir::Local,
    ) -> InterpResult<'tcx, Operand<Self::PointerTag>> {
        frame.locals[local].access()
    }

    /// Called before a basic block terminator is executed.
    /// You can use this to detect endlessly running programs.
    #[inline]
    fn before_terminator(_ecx: &mut InterpCx<'mir, 'tcx, Self>) -> InterpResult<'tcx> {
        Ok(())
    }

    /// Called before a global allocation is accessed.
    /// `def_id` is `Some` if this is the "lazy" allocation of a static.
    #[inline]
    fn before_access_global(
        _memory_extra: &Self::MemoryExtra,
        _alloc_id: AllocId,
        _allocation: &Allocation,
        _static_def_id: Option<DefId>,
        _is_write: bool,
    ) -> InterpResult<'tcx> {
        Ok(())
    }

    /// Called for *every* memory access to determine the real ID of the given allocation.
    /// This provides a way for the machine to "redirect" certain allocations as it sees fit.
    ///
    /// This is used by Miri to redirect extern statics to real allocations.
    ///
    /// This function must be idempotent.
    #[inline]
    fn canonical_alloc_id(_mem: &Memory<'mir, 'tcx, Self>, id: AllocId) -> AllocId {
        id
    }

    /// Called to initialize the "extra" state of an allocation and make the pointers
    /// it contains (in relocations) tagged.  The way we construct allocations is
    /// to always first construct it without extra and then add the extra.
    /// This keeps uniform code paths for handling both allocations created by CTFE
    /// for globals, and allocations created by Miri during evaluation.
    ///
    /// `kind` is the kind of the allocation being tagged; it can be `None` when
    /// it's a global and `GLOBAL_KIND` is `None`.
    ///
    /// This should avoid copying if no work has to be done! If this returns an owned
    /// allocation (because a copy had to be done to add tags or metadata), machine memory will
    /// cache the result. (This relies on `AllocMap::get_or` being able to add the
    /// owned allocation to the map even when the map is shared.)
    ///
    /// Also return the "base" tag to use for this allocation: the one that is used for direct
    /// accesses to this allocation. If `kind == STATIC_KIND`, this tag must be consistent
    /// with `tag_global_base_pointer`.
    fn init_allocation_extra<'b>(
        memory_extra: &Self::MemoryExtra,
        id: AllocId,
        alloc: Cow<'b, Allocation>,
        kind: Option<MemoryKind<Self::MemoryKind>>,
    ) -> (Cow<'b, Allocation<Self::PointerTag, Self::AllocExtra>>, Self::PointerTag);

    /// Return the "base" tag for the given *global* allocation: the one that is used for direct
    /// accesses to this static/const/fn allocation. If `id` is not a global allocation,
    /// this will return an unusable tag (i.e., accesses will be UB)!
    ///
    /// Expects `id` to be already canonical, if needed.
    fn tag_global_base_pointer(memory_extra: &Self::MemoryExtra, id: AllocId) -> Self::PointerTag;

    /// Executes a retagging operation
    #[inline]
    fn retag(
        _ecx: &mut InterpCx<'mir, 'tcx, Self>,
        _kind: mir::RetagKind,
        _place: PlaceTy<'tcx, Self::PointerTag>,
    ) -> InterpResult<'tcx> {
        Ok(())
    }

    /// Called immediately before a new stack frame got pushed.
    fn stack_push(ecx: &mut InterpCx<'mir, 'tcx, Self>) -> InterpResult<'tcx, Self::FrameExtra>;

    /// Called immediately after a stack frame gets popped
    fn stack_pop(
        _ecx: &mut InterpCx<'mir, 'tcx, Self>,
        _extra: Self::FrameExtra,
        _unwinding: bool,
    ) -> InterpResult<'tcx, StackPopJump> {
        // By default, we do not support unwinding from panics
        Ok(StackPopJump::Normal)
    }

    fn int_to_ptr(
        _mem: &Memory<'mir, 'tcx, Self>,
        int: u64,
    ) -> InterpResult<'tcx, Pointer<Self::PointerTag>> {
        Err((if int == 0 {
            // This is UB, seriously.
            err_ub!(InvalidIntPointerUsage(0))
        } else {
            // This is just something we cannot support during const-eval.
            err_unsup!(ReadBytesAsPointer)
        })
        .into())
    }

    fn ptr_to_int(
        _mem: &Memory<'mir, 'tcx, Self>,
        _ptr: Pointer<Self::PointerTag>,
    ) -> InterpResult<'tcx, u64>;
}
