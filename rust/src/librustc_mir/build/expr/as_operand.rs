// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! See docs in build/expr/mod.rs

use build::{BlockAnd, Builder};
use build::expr::category::Category;
use hair::*;
use repr::*;

impl<H:Hair> Builder<H> {
    /// Compile `expr` into a value that can be used as an operand.
    /// If `expr` is an lvalue like `x`, this will introduce a
    /// temporary `tmp = x`, so that we capture the value of `x` at
    /// this time.
    pub fn as_operand<M>(&mut self,
                         block: BasicBlock,
                         expr: M)
                         -> BlockAnd<Operand<H>>
        where M: Mirror<H, Output=Expr<H>>
    {
        let expr = self.hir.mirror(expr);
        self.expr_as_operand(block, expr)
    }

    fn expr_as_operand(&mut self,
                       mut block: BasicBlock,
                       expr: Expr<H>)
                       -> BlockAnd<Operand<H>>
    {
        debug!("expr_as_operand(block={:?}, expr={:?})",
               block, expr);
        let this = self;

        match expr.kind {
            ExprKind::Scope { extent, value } => {
                return this.in_scope(extent, block, |this| {
                    this.as_operand(block, value)
                });
            }
            ExprKind::Paren { arg } => {
                return this.as_operand(block, arg);
            }
            _ => { }
        }

        let category = Category::of(&expr.kind).unwrap();
        debug!("expr_as_operand: category={:?} for={:?}", category, expr.kind);
        match category {
            Category::Constant => {
                let constant = this.as_constant(expr);
                block.and(Operand::Constant(constant))
            }
            Category::Lvalue |
            Category::Rvalue(..) => {
                let operand = unpack!(block = this.as_temp(block, expr));
                block.and(Operand::Consume(operand))
            }
        }
    }
}
