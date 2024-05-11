use std::iter;

use rustc_ast::expand::allocator::AllocatorKind;
use rustc_target::abi::{Align, Size};

use crate::*;

/// Check some basic requirements for this allocation request:
/// non-zero size, power-of-two alignment.
pub(super) fn check_alloc_request<'tcx>(size: u64, align: u64) -> InterpResult<'tcx> {
    if size == 0 {
        throw_ub_format!("creating allocation with size 0");
    }
    if !align.is_power_of_two() {
        throw_ub_format!("creating allocation with non-power-of-two alignment {}", align);
    }
    Ok(())
}

impl<'mir, 'tcx: 'mir> EvalContextExt<'mir, 'tcx> for crate::MiriInterpCx<'mir, 'tcx> {}
pub trait EvalContextExt<'mir, 'tcx: 'mir>: crate::MiriInterpCxExt<'mir, 'tcx> {
    /// Returns the alignment that `malloc` would guarantee for requests of the given size.
    fn malloc_align(&self, size: u64) -> Align {
        let this = self.eval_context_ref();
        // The C standard says: "The pointer returned if the allocation succeeds is suitably aligned
        // so that it may be assigned to a pointer to any type of object with a fundamental
        // alignment requirement and size less than or equal to the size requested."
        // So first we need to figure out what the limits are for "fundamental alignment".
        // This is given by `alignof(max_align_t)`. The following list is taken from
        // `library/std/src/sys/pal/common/alloc.rs` (where this is called `MIN_ALIGN`) and should
        // be kept in sync.
        let max_fundamental_align = match this.tcx.sess.target.arch.as_ref() {
            "x86" | "arm" | "mips" | "mips32r6" | "powerpc" | "powerpc64" | "wasm32" => 8,
            "x86_64" | "aarch64" | "mips64" | "mips64r6" | "s390x" | "sparc64" | "loongarch64" =>
                16,
            arch => bug!("unsupported target architecture for malloc: `{}`", arch),
        };
        // The C standard only requires sufficient alignment for any *type* with size less than or
        // equal to the size requested. Types one can define in standard C seem to never have an alignment
        // bigger than their size. So if the size is 2, then only alignment 2 is guaranteed, even if
        // `max_fundamental_align` is bigger.
        // This matches what some real-world implementations do, see e.g.
        // - https://github.com/jemalloc/jemalloc/issues/1533
        // - https://github.com/llvm/llvm-project/issues/53540
        // - https://www.open-std.org/jtc1/sc22/wg14/www/docs/n2293.htm
        if size >= max_fundamental_align {
            return Align::from_bytes(max_fundamental_align).unwrap();
        }
        // C doesn't have zero-sized types, so presumably nothing is guaranteed here.
        if size == 0 {
            return Align::ONE;
        }
        // We have `size < min_align`. Round `size` *down* to the next power of two and use that.
        fn prev_power_of_two(x: u64) -> u64 {
            let next_pow2 = x.next_power_of_two();
            if next_pow2 == x {
                // x *is* a power of two, just use that.
                x
            } else {
                // x is between two powers, so next = 2*prev.
                next_pow2 / 2
            }
        }
        Align::from_bytes(prev_power_of_two(size)).unwrap()
    }

    /// Emulates calling the internal __rust_* allocator functions
    fn emulate_allocator(
        &mut self,
        default: impl FnOnce(&mut MiriInterpCx<'mir, 'tcx>) -> InterpResult<'tcx>,
    ) -> InterpResult<'tcx, EmulateItemResult> {
        let this = self.eval_context_mut();

        let Some(allocator_kind) = this.tcx.allocator_kind(()) else {
            // in real code, this symbol does not exist without an allocator
            return Ok(EmulateItemResult::NotSupported);
        };

        match allocator_kind {
            AllocatorKind::Global => {
                // When `#[global_allocator]` is used, `__rust_*` is defined by the macro expansion
                // of this attribute. As such we have to call an exported Rust function,
                // and not execute any Miri shim. Somewhat unintuitively doing so is done
                // by returning `NotSupported`, which triggers the `lookup_exported_symbol`
                // fallback case in `emulate_foreign_item`.
                return Ok(EmulateItemResult::NotSupported);
            }
            AllocatorKind::Default => {
                default(this)?;
                Ok(EmulateItemResult::NeedsJumping)
            }
        }
    }

    fn malloc(
        &mut self,
        size: u64,
        zero_init: bool,
    ) -> InterpResult<'tcx, Pointer<Option<Provenance>>> {
        let this = self.eval_context_mut();
        let align = this.malloc_align(size);
        let ptr = this.allocate_ptr(Size::from_bytes(size), align, MiriMemoryKind::C.into())?;
        if zero_init {
            // We just allocated this, the access is definitely in-bounds and fits into our address space.
            this.write_bytes_ptr(
                ptr.into(),
                iter::repeat(0u8).take(usize::try_from(size).unwrap()),
            )
            .unwrap();
        }
        Ok(ptr.into())
    }

    fn free(&mut self, ptr: Pointer<Option<Provenance>>) -> InterpResult<'tcx> {
        let this = self.eval_context_mut();
        if !this.ptr_is_null(ptr)? {
            this.deallocate_ptr(ptr, None, MiriMemoryKind::C.into())?;
        }
        Ok(())
    }

    fn realloc(
        &mut self,
        old_ptr: Pointer<Option<Provenance>>,
        new_size: u64,
    ) -> InterpResult<'tcx, Pointer<Option<Provenance>>> {
        let this = self.eval_context_mut();
        let new_align = this.malloc_align(new_size);
        if this.ptr_is_null(old_ptr)? {
            // Here we must behave like `malloc`.
            self.malloc(new_size, /*zero_init*/ false)
        } else {
            if new_size == 0 {
                // C, in their infinite wisdom, made this UB.
                // <https://www.open-std.org/jtc1/sc22/wg14/www/docs/n2464.pdf>
                throw_ub_format!("`realloc` with a size of zero");
            } else {
                let new_ptr = this.reallocate_ptr(
                    old_ptr,
                    None,
                    Size::from_bytes(new_size),
                    new_align,
                    MiriMemoryKind::C.into(),
                )?;
                Ok(new_ptr.into())
            }
        }
    }
}
