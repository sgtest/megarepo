use std::convert::TryFrom;

use rustc_hir::lang_items::PanicLocationLangItem;
use rustc_middle::ty::subst::Subst;
use rustc_span::{Span, Symbol};
use rustc_target::abi::LayoutOf;

use crate::interpret::{
    intrinsics::{InterpCx, Machine},
    MPlaceTy, MemoryKind, Scalar,
};

impl<'mir, 'tcx: 'mir, M: Machine<'mir, 'tcx>> InterpCx<'mir, 'tcx, M> {
    /// Walks up the callstack from the intrinsic's callsite, searching for the first callsite in a
    /// frame which is not `#[track_caller]`.
    crate fn find_closest_untracked_caller_location(&self) -> Span {
        self.stack()
            .iter()
            .rev()
            // Find first non-`#[track_caller]` frame.
            .find(|frame| !frame.instance.def.requires_caller_location(*self.tcx))
            // Assert that there is always such a frame.
            .unwrap()
            .current_source_info()
            // Assert that the frame we look at is actually executing code currently
            // (`current_source_info` is None when we are unwinding and the frame does
            // not require cleanup).
            .unwrap()
            .span
    }

    /// Allocate a `const core::panic::Location` with the provided filename and line/column numbers.
    crate fn alloc_caller_location(
        &mut self,
        filename: Symbol,
        line: u32,
        col: u32,
    ) -> MPlaceTy<'tcx, M::PointerTag> {
        let file = self.allocate_str(&filename.as_str(), MemoryKind::CallerLocation);
        let line = Scalar::from_u32(line);
        let col = Scalar::from_u32(col);

        // Allocate memory for `CallerLocation` struct.
        let loc_ty = self
            .tcx
            .type_of(self.tcx.require_lang_item(PanicLocationLangItem, None))
            .subst(*self.tcx, self.tcx.mk_substs([self.tcx.lifetimes.re_erased.into()].iter()));
        let loc_layout = self.layout_of(loc_ty).unwrap();
        let location = self.allocate(loc_layout, MemoryKind::CallerLocation);

        // Initialize fields.
        self.write_immediate(file.to_ref(), self.mplace_field(location, 0).unwrap().into())
            .expect("writing to memory we just allocated cannot fail");
        self.write_scalar(line, self.mplace_field(location, 1).unwrap().into())
            .expect("writing to memory we just allocated cannot fail");
        self.write_scalar(col, self.mplace_field(location, 2).unwrap().into())
            .expect("writing to memory we just allocated cannot fail");

        location
    }

    crate fn location_triple_for_span(&self, span: Span) -> (Symbol, u32, u32) {
        let topmost = span.ctxt().outer_expn().expansion_cause().unwrap_or(span);
        let caller = self.tcx.sess.source_map().lookup_char_pos(topmost.lo());
        (
            Symbol::intern(&caller.file.name.to_string()),
            u32::try_from(caller.line).unwrap(),
            u32::try_from(caller.col_display).unwrap().checked_add(1).unwrap(),
        )
    }

    pub fn alloc_caller_location_for_span(&mut self, span: Span) -> MPlaceTy<'tcx, M::PointerTag> {
        let (file, line, column) = self.location_triple_for_span(span);
        self.alloc_caller_location(file, line, column)
    }
}
