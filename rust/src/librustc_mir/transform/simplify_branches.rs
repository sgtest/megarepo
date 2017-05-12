// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A pass that simplifies branches when their condition is known.

use rustc::ty::TyCtxt;
use rustc::middle::const_val::ConstVal;
use rustc::mir::transform::{MirPass, MirSource};
use rustc::mir::*;

use std::borrow::Cow;

pub struct SimplifyBranches { label: String }

impl SimplifyBranches {
    pub fn new(label: &str) -> Self {
        SimplifyBranches { label: format!("SimplifyBranches-{}", label) }
    }
}

impl MirPass for SimplifyBranches {
    fn name<'a>(&'a self) -> Cow<'a, str> {
        Cow::Borrowed(&self.label)
    }

    fn run_pass<'a, 'tcx>(&self,
                          _tcx: TyCtxt<'a, 'tcx, 'tcx>,
                          _src: MirSource,
                          mir: &mut Mir<'tcx>) {
        for block in mir.basic_blocks_mut() {
            let terminator = block.terminator_mut();
            terminator.kind = match terminator.kind {
                TerminatorKind::SwitchInt { discr: Operand::Constant(box Constant {
                    literal: Literal::Value { ref value }, ..
                }), ref values, ref targets, .. } => {
                    if let Some(ref constint) = value.to_const_int() {
                        let (otherwise, targets) = targets.split_last().unwrap();
                        let mut ret = TerminatorKind::Goto { target: *otherwise };
                        for (v, t) in values.iter().zip(targets.iter()) {
                            if v == constint {
                                ret = TerminatorKind::Goto { target: *t };
                                break;
                            }
                        }
                        ret
                    } else {
                        continue
                    }
                },
                TerminatorKind::Assert { target, cond: Operand::Constant(box Constant {
                    literal: Literal::Value {
                        value: ConstVal::Bool(cond)
                    }, ..
                }), expected, .. } if cond == expected => {
                    TerminatorKind::Goto { target: target }
                },
                _ => continue
            };
        }
    }
}
