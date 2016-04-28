// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! An analysis to determine which temporaries require allocas and
//! which do not.

use rustc_data_structures::bitvec::BitVector;
use rustc::mir::repr as mir;
use rustc::mir::visit::{Visitor, LvalueContext};
use common::{self, Block, BlockAndBuilder};
use super::rvalue;

pub fn lvalue_temps<'bcx,'tcx>(bcx: Block<'bcx,'tcx>,
                               mir: &mir::Mir<'tcx>) -> BitVector {
    let bcx = bcx.build();
    let mut analyzer = TempAnalyzer::new(mir, &bcx, mir.temp_decls.len());

    analyzer.visit_mir(mir);

    for (index, temp_decl) in mir.temp_decls.iter().enumerate() {
        let ty = bcx.monomorphize(&temp_decl.ty);
        debug!("temp {:?} has type {:?}", index, ty);
        if ty.is_scalar() ||
            ty.is_unique() ||
            ty.is_region_ptr() ||
            ty.is_simd() ||
            common::type_is_zero_size(bcx.ccx(), ty)
        {
            // These sorts of types are immediates that we can store
            // in an ValueRef without an alloca.
            assert!(common::type_is_immediate(bcx.ccx(), ty) ||
                    common::type_is_fat_ptr(bcx.tcx(), ty));
        } else {
            // These sorts of types require an alloca. Note that
            // type_is_immediate() may *still* be true, particularly
            // for newtypes, but we currently force some types
            // (e.g. structs) into an alloca unconditionally, just so
            // that we don't have to deal with having two pathways
            // (gep vs extractvalue etc).
            analyzer.mark_as_lvalue(index);
        }
    }

    analyzer.lvalue_temps
}

struct TempAnalyzer<'mir, 'bcx: 'mir, 'tcx: 'bcx> {
    mir: &'mir mir::Mir<'tcx>,
    bcx: &'mir BlockAndBuilder<'bcx, 'tcx>,
    lvalue_temps: BitVector,
    seen_assigned: BitVector
}

impl<'mir, 'bcx, 'tcx> TempAnalyzer<'mir, 'bcx, 'tcx> {
    fn new(mir: &'mir mir::Mir<'tcx>,
           bcx: &'mir BlockAndBuilder<'bcx, 'tcx>,
           temp_count: usize) -> TempAnalyzer<'mir, 'bcx, 'tcx> {
        TempAnalyzer {
            mir: mir,
            bcx: bcx,
            lvalue_temps: BitVector::new(temp_count),
            seen_assigned: BitVector::new(temp_count)
        }
    }

    fn mark_as_lvalue(&mut self, temp: usize) {
        debug!("marking temp {} as lvalue", temp);
        self.lvalue_temps.insert(temp);
    }

    fn mark_assigned(&mut self, temp: usize) {
        if !self.seen_assigned.insert(temp) {
            self.mark_as_lvalue(temp);
        }
    }
}

impl<'mir, 'bcx, 'tcx> Visitor<'tcx> for TempAnalyzer<'mir, 'bcx, 'tcx> {
    fn visit_assign(&mut self,
                    block: mir::BasicBlock,
                    lvalue: &mir::Lvalue<'tcx>,
                    rvalue: &mir::Rvalue<'tcx>) {
        debug!("visit_assign(block={:?}, lvalue={:?}, rvalue={:?})", block, lvalue, rvalue);

        match *lvalue {
            mir::Lvalue::Temp(index) => {
                self.mark_assigned(index as usize);
                if !rvalue::rvalue_creates_operand(self.mir, self.bcx, rvalue) {
                    self.mark_as_lvalue(index as usize);
                }
            }
            _ => {
                self.visit_lvalue(lvalue, LvalueContext::Store);
            }
        }

        self.visit_rvalue(rvalue);
    }

    fn visit_lvalue(&mut self,
                    lvalue: &mir::Lvalue<'tcx>,
                    context: LvalueContext) {
        debug!("visit_lvalue(lvalue={:?}, context={:?})", lvalue, context);

        match *lvalue {
            mir::Lvalue::Temp(index) => {
                match context {
                    LvalueContext::Call => {
                        self.mark_assigned(index as usize);
                    }
                    LvalueContext::Consume => {
                    }
                    LvalueContext::Store |
                    LvalueContext::Drop |
                    LvalueContext::Inspect |
                    LvalueContext::Borrow { .. } |
                    LvalueContext::Slice { .. } |
                    LvalueContext::Projection => {
                        self.mark_as_lvalue(index as usize);
                    }
                }
            }
            _ => {
            }
        }

        self.super_lvalue(lvalue, context);
    }
}
