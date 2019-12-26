use rustc::hir::def_id::DefId;
use rustc::mir;
use rustc::ty::layout::HasTyCtxt;
use rustc::ty::{self, Ty, TyCtxt};
use std::borrow::{Borrow, Cow};
use std::collections::hash_map::Entry;
use std::hash::Hash;

use rustc_data_structures::fx::FxHashMap;

use syntax::source_map::Span;

use crate::interpret::{
    self, snapshot, AllocId, Allocation, AssertMessage, GlobalId, ImmTy, InterpCx, InterpResult,
    Memory, MemoryKind, OpTy, PlaceTy, Pointer, Scalar,
};

use super::error::*;

impl<'mir, 'tcx> InterpCx<'mir, 'tcx, CompileTimeInterpreter<'mir, 'tcx>> {
    /// Evaluate a const function where all arguments (if any) are zero-sized types.
    /// The evaluation is memoized thanks to the query system.
    ///
    /// Returns `true` if the call has been evaluated.
    fn try_eval_const_fn_call(
        &mut self,
        instance: ty::Instance<'tcx>,
        ret: Option<(PlaceTy<'tcx>, mir::BasicBlock)>,
        args: &[OpTy<'tcx>],
    ) -> InterpResult<'tcx, bool> {
        trace!("try_eval_const_fn_call: {:?}", instance);
        // Because `#[track_caller]` adds an implicit non-ZST argument, we also cannot
        // perform this optimization on items tagged with it.
        if instance.def.requires_caller_location(self.tcx()) {
            return Ok(false);
        }
        // For the moment we only do this for functions which take no arguments
        // (or all arguments are ZSTs) so that we don't memoize too much.
        if args.iter().any(|a| !a.layout.is_zst()) {
            return Ok(false);
        }

        let dest = match ret {
            Some((dest, _)) => dest,
            // Don't memoize diverging function calls.
            None => return Ok(false),
        };

        let gid = GlobalId { instance, promoted: None };

        let place = self.const_eval_raw(gid)?;

        self.copy_op(place.into(), dest)?;

        self.return_to_block(ret.map(|r| r.1))?;
        self.dump_place(*dest);
        return Ok(true);
    }
}

/// Number of steps until the detector even starts doing anything.
/// Also, a warning is shown to the user when this number is reached.
const STEPS_UNTIL_DETECTOR_ENABLED: isize = 1_000_000;
/// The number of steps between loop detector snapshots.
/// Should be a power of two for performance reasons.
const DETECTOR_SNAPSHOT_PERIOD: isize = 256;

// Extra machine state for CTFE, and the Machine instance
pub struct CompileTimeInterpreter<'mir, 'tcx> {
    /// When this value is negative, it indicates the number of interpreter
    /// steps *until* the loop detector is enabled. When it is positive, it is
    /// the number of steps after the detector has been enabled modulo the loop
    /// detector period.
    pub(super) steps_since_detector_enabled: isize,

    /// Extra state to detect loops.
    pub(super) loop_detector: snapshot::InfiniteLoopDetector<'mir, 'tcx>,
}

#[derive(Copy, Clone, Debug)]
pub struct MemoryExtra {
    /// Whether this machine may read from statics
    pub(super) can_access_statics: bool,
}

impl<'mir, 'tcx> CompileTimeInterpreter<'mir, 'tcx> {
    pub(super) fn new() -> Self {
        CompileTimeInterpreter {
            loop_detector: Default::default(),
            steps_since_detector_enabled: -STEPS_UNTIL_DETECTOR_ENABLED,
        }
    }
}

impl<K: Hash + Eq, V> interpret::AllocMap<K, V> for FxHashMap<K, V> {
    #[inline(always)]
    fn contains_key<Q: ?Sized + Hash + Eq>(&mut self, k: &Q) -> bool
    where
        K: Borrow<Q>,
    {
        FxHashMap::contains_key(self, k)
    }

    #[inline(always)]
    fn insert(&mut self, k: K, v: V) -> Option<V> {
        FxHashMap::insert(self, k, v)
    }

    #[inline(always)]
    fn remove<Q: ?Sized + Hash + Eq>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
    {
        FxHashMap::remove(self, k)
    }

    #[inline(always)]
    fn filter_map_collect<T>(&self, mut f: impl FnMut(&K, &V) -> Option<T>) -> Vec<T> {
        self.iter().filter_map(move |(k, v)| f(k, &*v)).collect()
    }

    #[inline(always)]
    fn get_or<E>(&self, k: K, vacant: impl FnOnce() -> Result<V, E>) -> Result<&V, E> {
        match self.get(&k) {
            Some(v) => Ok(v),
            None => {
                vacant()?;
                bug!("The CTFE machine shouldn't ever need to extend the alloc_map when reading")
            }
        }
    }

    #[inline(always)]
    fn get_mut_or<E>(&mut self, k: K, vacant: impl FnOnce() -> Result<V, E>) -> Result<&mut V, E> {
        match self.entry(k) {
            Entry::Occupied(e) => Ok(e.into_mut()),
            Entry::Vacant(e) => {
                let v = vacant()?;
                Ok(e.insert(v))
            }
        }
    }
}

crate type CompileTimeEvalContext<'mir, 'tcx> =
    InterpCx<'mir, 'tcx, CompileTimeInterpreter<'mir, 'tcx>>;

impl interpret::MayLeak for ! {
    #[inline(always)]
    fn may_leak(self) -> bool {
        // `self` is uninhabited
        self
    }
}

impl<'mir, 'tcx> interpret::Machine<'mir, 'tcx> for CompileTimeInterpreter<'mir, 'tcx> {
    type MemoryKinds = !;
    type PointerTag = ();
    type ExtraFnVal = !;

    type FrameExtra = ();
    type MemoryExtra = MemoryExtra;
    type AllocExtra = ();

    type MemoryMap = FxHashMap<AllocId, (MemoryKind<!>, Allocation)>;

    const STATIC_KIND: Option<!> = None; // no copying of statics allowed

    // We do not check for alignment to avoid having to carry an `Align`
    // in `ConstValue::ByRef`.
    const CHECK_ALIGN: bool = false;

    #[inline(always)]
    fn enforce_validity(_ecx: &InterpCx<'mir, 'tcx, Self>) -> bool {
        false // for now, we don't enforce validity
    }

    fn find_mir_or_eval_fn(
        ecx: &mut InterpCx<'mir, 'tcx, Self>,
        instance: ty::Instance<'tcx>,
        args: &[OpTy<'tcx>],
        ret: Option<(PlaceTy<'tcx>, mir::BasicBlock)>,
        _unwind: Option<mir::BasicBlock>, // unwinding is not supported in consts
    ) -> InterpResult<'tcx, Option<&'mir mir::Body<'tcx>>> {
        debug!("find_mir_or_eval_fn: {:?}", instance);

        // Only check non-glue functions
        if let ty::InstanceDef::Item(def_id) = instance.def {
            // Execution might have wandered off into other crates, so we cannot do a stability-
            // sensitive check here.  But we can at least rule out functions that are not const
            // at all.
            if ecx.tcx.is_const_fn_raw(def_id) {
                // If this function is a `const fn` then under certain circumstances we
                // can evaluate call via the query system, thus memoizing all future calls.
                if ecx.try_eval_const_fn_call(instance, ret, args)? {
                    return Ok(None);
                }
            } else {
                // Some functions we support even if they are non-const -- but avoid testing
                // that for const fn!  We certainly do *not* want to actually call the fn
                // though, so be sure we return here.
                return if ecx.hook_panic_fn(instance, args, ret)? {
                    Ok(None)
                } else {
                    throw_unsup_format!("calling non-const function `{}`", instance)
                };
            }
        }
        // This is a const fn. Call it.
        Ok(Some(match ecx.load_mir(instance.def, None) {
            Ok(body) => *body,
            Err(err) => {
                if let err_unsup!(NoMirFor(ref path)) = err.kind {
                    return Err(ConstEvalError::NeedsRfc(format!(
                        "calling extern function `{}`",
                        path
                    ))
                    .into());
                }
                return Err(err);
            }
        }))
    }

    fn call_extra_fn(
        _ecx: &mut InterpCx<'mir, 'tcx, Self>,
        fn_val: !,
        _args: &[OpTy<'tcx>],
        _ret: Option<(PlaceTy<'tcx>, mir::BasicBlock)>,
        _unwind: Option<mir::BasicBlock>,
    ) -> InterpResult<'tcx> {
        match fn_val {}
    }

    fn call_intrinsic(
        ecx: &mut InterpCx<'mir, 'tcx, Self>,
        span: Span,
        instance: ty::Instance<'tcx>,
        args: &[OpTy<'tcx>],
        ret: Option<(PlaceTy<'tcx>, mir::BasicBlock)>,
        _unwind: Option<mir::BasicBlock>,
    ) -> InterpResult<'tcx> {
        if ecx.emulate_intrinsic(span, instance, args, ret)? {
            return Ok(());
        }
        // An intrinsic that we do not support
        let intrinsic_name = ecx.tcx.item_name(instance.def_id());
        Err(ConstEvalError::NeedsRfc(format!("calling intrinsic `{}`", intrinsic_name)).into())
    }

    fn assert_panic(
        ecx: &mut InterpCx<'mir, 'tcx, Self>,
        _span: Span,
        msg: &AssertMessage<'tcx>,
        _unwind: Option<mir::BasicBlock>,
    ) -> InterpResult<'tcx> {
        use rustc::mir::interpret::PanicInfo::*;
        Err(match msg {
            BoundsCheck { ref len, ref index } => {
                let len = ecx
                    .read_immediate(ecx.eval_operand(len, None)?)
                    .expect("can't eval len")
                    .to_scalar()?
                    .to_machine_usize(&*ecx)?;
                let index = ecx
                    .read_immediate(ecx.eval_operand(index, None)?)
                    .expect("can't eval index")
                    .to_scalar()?
                    .to_machine_usize(&*ecx)?;
                err_panic!(BoundsCheck { len, index })
            }
            Overflow(op) => err_panic!(Overflow(*op)),
            OverflowNeg => err_panic!(OverflowNeg),
            DivisionByZero => err_panic!(DivisionByZero),
            RemainderByZero => err_panic!(RemainderByZero),
            ResumedAfterReturn(generator_kind) => err_panic!(ResumedAfterReturn(*generator_kind)),
            ResumedAfterPanic(generator_kind) => err_panic!(ResumedAfterPanic(*generator_kind)),
            Panic { .. } => bug!("`Panic` variant cannot occur in MIR"),
        }
        .into())
    }

    fn ptr_to_int(_mem: &Memory<'mir, 'tcx, Self>, _ptr: Pointer) -> InterpResult<'tcx, u64> {
        Err(ConstEvalError::NeedsRfc("pointer-to-integer cast".to_string()).into())
    }

    fn binary_ptr_op(
        _ecx: &InterpCx<'mir, 'tcx, Self>,
        _bin_op: mir::BinOp,
        _left: ImmTy<'tcx>,
        _right: ImmTy<'tcx>,
    ) -> InterpResult<'tcx, (Scalar, bool, Ty<'tcx>)> {
        Err(ConstEvalError::NeedsRfc("pointer arithmetic or comparison".to_string()).into())
    }

    fn find_foreign_static(
        _tcx: TyCtxt<'tcx>,
        _def_id: DefId,
    ) -> InterpResult<'tcx, Cow<'tcx, Allocation<Self::PointerTag>>> {
        throw_unsup!(ReadForeignStatic)
    }

    #[inline(always)]
    fn init_allocation_extra<'b>(
        _memory_extra: &MemoryExtra,
        _id: AllocId,
        alloc: Cow<'b, Allocation>,
        _kind: Option<MemoryKind<!>>,
    ) -> (Cow<'b, Allocation<Self::PointerTag>>, Self::PointerTag) {
        // We do not use a tag so we can just cheaply forward the allocation
        (alloc, ())
    }

    #[inline(always)]
    fn tag_static_base_pointer(_memory_extra: &MemoryExtra, _id: AllocId) -> Self::PointerTag {
        ()
    }

    fn box_alloc(
        _ecx: &mut InterpCx<'mir, 'tcx, Self>,
        _dest: PlaceTy<'tcx>,
    ) -> InterpResult<'tcx> {
        Err(ConstEvalError::NeedsRfc("heap allocations via `box` keyword".to_string()).into())
    }

    fn before_terminator(ecx: &mut InterpCx<'mir, 'tcx, Self>) -> InterpResult<'tcx> {
        {
            let steps = &mut ecx.machine.steps_since_detector_enabled;

            *steps += 1;
            if *steps < 0 {
                return Ok(());
            }

            *steps %= DETECTOR_SNAPSHOT_PERIOD;
            if *steps != 0 {
                return Ok(());
            }
        }

        let span = ecx.frame().span;
        ecx.machine.loop_detector.observe_and_analyze(*ecx.tcx, span, &ecx.memory, &ecx.stack[..])
    }

    #[inline(always)]
    fn stack_push(_ecx: &mut InterpCx<'mir, 'tcx, Self>) -> InterpResult<'tcx> {
        Ok(())
    }

    fn before_access_static(
        memory_extra: &MemoryExtra,
        _allocation: &Allocation,
    ) -> InterpResult<'tcx> {
        if memory_extra.can_access_statics {
            Ok(())
        } else {
            Err(ConstEvalError::ConstAccessesStatic.into())
        }
    }
}

// Please do not add any code below the above `Machine` trait impl. I (oli-obk) plan more cleanups
// so we can end up having a file with just that impl, but for now, let's keep the impl discoverable
// at the bottom of this file.
