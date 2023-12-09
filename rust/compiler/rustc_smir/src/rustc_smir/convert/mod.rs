//! Conversion of internal Rust compiler items to stable ones.

use rustc_target::abi::FieldIdx;
use stable_mir::ty::{IndexedVal, VariantIdx};

use crate::rustc_smir::{Stable, Tables};

mod error;
mod mir;
mod ty;

impl<'tcx> Stable<'tcx> for rustc_hir::Unsafety {
    type T = stable_mir::mir::Safety;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        match self {
            rustc_hir::Unsafety::Unsafe => stable_mir::mir::Safety::Unsafe,
            rustc_hir::Unsafety::Normal => stable_mir::mir::Safety::Normal,
        }
    }
}

impl<'tcx> Stable<'tcx> for FieldIdx {
    type T = usize;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        self.as_usize()
    }
}

impl<'tcx> Stable<'tcx> for rustc_target::abi::VariantIdx {
    type T = VariantIdx;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        VariantIdx::to_val(self.as_usize())
    }
}

impl<'tcx> Stable<'tcx> for rustc_hir::CoroutineSource {
    type T = stable_mir::mir::CoroutineSource;
    fn stable(&self, _: &mut Tables<'tcx>) -> Self::T {
        use rustc_hir::CoroutineSource;
        match self {
            CoroutineSource::Block => stable_mir::mir::CoroutineSource::Block,
            CoroutineSource::Closure => stable_mir::mir::CoroutineSource::Closure,
            CoroutineSource::Fn => stable_mir::mir::CoroutineSource::Fn,
        }
    }
}

impl<'tcx> Stable<'tcx> for rustc_hir::CoroutineKind {
    type T = stable_mir::mir::CoroutineKind;
    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        use rustc_hir::CoroutineKind;
        match self {
            CoroutineKind::Async(source) => {
                stable_mir::mir::CoroutineKind::Async(source.stable(tables))
            }
            CoroutineKind::Gen(source) => {
                stable_mir::mir::CoroutineKind::Gen(source.stable(tables))
            }
            CoroutineKind::Coroutine => stable_mir::mir::CoroutineKind::Coroutine,
            CoroutineKind::AsyncGen(_) => todo!(),
        }
    }
}

impl<'tcx> Stable<'tcx> for rustc_span::Symbol {
    type T = stable_mir::Symbol;

    fn stable(&self, _tables: &mut Tables<'tcx>) -> Self::T {
        self.to_string()
    }
}

impl<'tcx> Stable<'tcx> for rustc_span::Span {
    type T = stable_mir::ty::Span;

    fn stable(&self, tables: &mut Tables<'tcx>) -> Self::T {
        tables.create_span(*self)
    }
}

impl<'tcx> Stable<'tcx> for rustc_abi::Endian {
    type T = stable_mir::target::Endian;

    fn stable(&self, _tables: &mut Tables<'tcx>) -> Self::T {
        match self {
            rustc_abi::Endian::Little => stable_mir::target::Endian::Little,
            rustc_abi::Endian::Big => stable_mir::target::Endian::Big,
        }
    }
}
