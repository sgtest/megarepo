// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Miscellaneous builder routines that are not specific to building any particular
//! kind of thing.

use build::Builder;
use hair::*;
use repr::*;
use rustc::middle::ty::Ty;
use std::u32;
use syntax::codemap::Span;

impl<'a,'tcx> Builder<'a,'tcx> {
    /// Add a new temporary value of type `ty` storing the result of
    /// evaluating `expr`.
    ///
    /// NB: **No cleanup is scheduled for this temporary.** You should
    /// call `schedule_drop` once the temporary is initialized.
    pub fn temp(&mut self, ty: Ty<'tcx>) -> Lvalue<'tcx> {
        let index = self.temp_decls.len();
        self.temp_decls.push(TempDecl { ty: ty });
        assert!(index < (u32::MAX) as usize);
        let lvalue = Lvalue::Temp(index as u32);
        debug!("temp: created temp {:?} with type {:?}",
               lvalue, self.temp_decls.last().unwrap().ty);
        lvalue
    }

    pub fn push_literal(&mut self,
                        block: BasicBlock,
                        span: Span,
                        ty: Ty<'tcx>,
                        literal: Literal<'tcx>)
                        -> Lvalue<'tcx> {
        let temp = self.temp(ty.clone());
        let constant = Constant {
            span: span,
            ty: ty,
            literal: literal,
        };
        self.cfg.push_assign_constant(block, span, &temp, constant);
        temp
    }

    pub fn push_usize(&mut self, block: BasicBlock, span: Span, value: usize) -> Lvalue<'tcx> {
        let usize_ty = self.hir.usize_ty();
        let temp = self.temp(usize_ty);
        self.cfg.push_assign_constant(
            block, span, &temp,
            Constant {
                span: span,
                ty: self.hir.usize_ty(),
                literal: self.hir.usize_literal(value),
            });
        temp
    }

    pub fn push_item_ref(&mut self,
                         block: BasicBlock,
                         span: Span,
                         item_ref: ItemRef<'tcx>)
                         -> Lvalue<'tcx> {
        let literal = Literal::Item {
            def_id: item_ref.def_id,
            substs: item_ref.substs,
        };
        self.push_literal(block, span, item_ref.ty, literal)
    }
}
