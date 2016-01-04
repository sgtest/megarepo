// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Testing candidates
//
// After candidates have been simplified, the only match pairs that
// remain are those that require some sort of test. The functions here
// identify what tests are needed, perform the tests, and then filter
// the candidates based on the result.

use build::Builder;
use build::matches::{Candidate, MatchPair, Test, TestKind};
use hair::*;
use rustc_data_structures::fnv::FnvHashMap;
use rustc::middle::const_eval::ConstVal;
use rustc::middle::ty::{self, Ty};
use rustc::mir::repr::*;
use syntax::codemap::Span;

impl<'a,'tcx> Builder<'a,'tcx> {
    /// Identifies what test is needed to decide if `match_pair` is applicable.
    ///
    /// It is a bug to call this with a simplifyable pattern.
    pub fn test<'pat>(&mut self, match_pair: &MatchPair<'pat, 'tcx>) -> Test<'tcx> {
        match *match_pair.pattern.kind {
            PatternKind::Variant { ref adt_def, variant_index: _, subpatterns: _ } => {
                Test {
                    span: match_pair.pattern.span,
                    kind: TestKind::Switch { adt_def: adt_def.clone() },
                }
            }

            PatternKind::Constant { .. }
            if is_switch_ty(match_pair.pattern.ty) => {
                // for integers, we use a SwitchInt match, which allows
                // us to handle more cases
                Test {
                    span: match_pair.pattern.span,
                    kind: TestKind::SwitchInt {
                        switch_ty: match_pair.pattern.ty,

                        // these maps are empty to start; cases are
                        // added below in add_cases_to_switch
                        options: vec![],
                        indices: FnvHashMap(),
                    }
                }
            }

            PatternKind::Constant { ref value } => {
                Test {
                    span: match_pair.pattern.span,
                    kind: TestKind::Eq {
                        value: value.clone(),
                        ty: match_pair.pattern.ty.clone()
                    }
                }
            }

            PatternKind::Range { ref lo, ref hi } => {
                Test {
                    span: match_pair.pattern.span,
                    kind: TestKind::Range {
                        lo: lo.clone(),
                        hi: hi.clone(),
                        ty: match_pair.pattern.ty.clone(),
                    },
                }
            }

            PatternKind::Slice { ref prefix, ref slice, ref suffix } => {
                let len = prefix.len() + suffix.len();
                let op = if slice.is_some() {
                    BinOp::Ge
                } else {
                    BinOp::Eq
                };
                Test {
                    span: match_pair.pattern.span,
                    kind: TestKind::Len { len: len, op: op },
                }
            }

            PatternKind::Array { .. } |
            PatternKind::Wild |
            PatternKind::Binding { .. } |
            PatternKind::Leaf { .. } |
            PatternKind::Deref { .. } => {
                self.error_simplifyable(match_pair)
            }
        }
    }

    pub fn add_cases_to_switch<'pat>(&mut self,
                                     test_lvalue: &Lvalue<'tcx>,
                                     candidate: &Candidate<'pat, 'tcx>,
                                     switch_ty: Ty<'tcx>,
                                     options: &mut Vec<ConstVal>,
                                     indices: &mut FnvHashMap<ConstVal, usize>)
                                     -> bool
    {
        let match_pair = match candidate.match_pairs.iter().find(|mp| mp.lvalue == *test_lvalue) {
            Some(match_pair) => match_pair,
            _ => { return false; }
        };

        match *match_pair.pattern.kind {
            PatternKind::Constant { ref value } => {
                // if the lvalues match, the type should match
                assert_eq!(match_pair.pattern.ty, switch_ty);

                indices.entry(value.clone())
                       .or_insert_with(|| {
                           options.push(value.clone());
                           options.len() - 1
                       });
                true
            }

            PatternKind::Range { .. } |
            PatternKind::Variant { .. } |
            PatternKind::Slice { .. } |
            PatternKind::Array { .. } |
            PatternKind::Wild |
            PatternKind::Binding { .. } |
            PatternKind::Leaf { .. } |
            PatternKind::Deref { .. } => {
                // don't know how to add these patterns to a switch
                false
            }
        }
    }

    /// Generates the code to perform a test.
    pub fn perform_test(&mut self,
                        block: BasicBlock,
                        lvalue: &Lvalue<'tcx>,
                        test: &Test<'tcx>)
                        -> Vec<BasicBlock> {
        match test.kind {
            TestKind::Switch { adt_def } => {
                let num_enum_variants = self.hir.num_variants(adt_def);
                let target_blocks: Vec<_> =
                    (0..num_enum_variants).map(|_| self.cfg.start_new_block())
                                          .collect();
                self.cfg.terminate(block, Terminator::Switch {
                    discr: lvalue.clone(),
                    adt_def: adt_def,
                    targets: target_blocks.clone()
                });
                target_blocks
            }

            TestKind::SwitchInt { switch_ty, ref options, indices: _ } => {
                let otherwise = self.cfg.start_new_block();
                let targets: Vec<_> =
                    options.iter()
                           .map(|_| self.cfg.start_new_block())
                           .chain(Some(otherwise))
                           .collect();
                self.cfg.terminate(block, Terminator::SwitchInt {
                    discr: lvalue.clone(),
                    switch_ty: switch_ty,
                    values: options.clone(),
                    targets: targets.clone(),
                });
                targets
            }

            TestKind::Eq { ref value, ty } => {
                let expect = self.literal_operand(test.span, ty.clone(), Literal::Value {
                    value: value.clone()
                });
                let val = Operand::Consume(lvalue.clone());
                let fail = self.cfg.start_new_block();
                let block = self.compare(block, fail, test.span, BinOp::Eq, expect, val.clone());
                vec![block, fail]
            }

            TestKind::Range { ref lo, ref hi, ty } => {
                // Test `val` by computing `lo <= val && val <= hi`, using primitive comparisons.
                let lo = self.literal_operand(test.span, ty.clone(), lo.clone());
                let hi = self.literal_operand(test.span, ty.clone(), hi.clone());
                let val = Operand::Consume(lvalue.clone());

                let fail = self.cfg.start_new_block();
                let block = self.compare(block, fail, test.span, BinOp::Le, lo, val.clone());
                let block = self.compare(block, fail, test.span, BinOp::Le, val, hi);

                vec![block, fail]
            }

            TestKind::Len { len, op } => {
                let (usize_ty, bool_ty) = (self.hir.usize_ty(), self.hir.bool_ty());
                let (actual, result) = (self.temp(usize_ty), self.temp(bool_ty));

                // actual = len(lvalue)
                self.cfg.push_assign(block, test.span, &actual, Rvalue::Len(lvalue.clone()));

                // expected = <N>
                let expected = self.push_usize(block, test.span, len);

                // result = actual == expected OR result = actual < expected
                self.cfg.push_assign(block,
                                     test.span,
                                     &result,
                                     Rvalue::BinaryOp(op,
                                                      Operand::Consume(actual),
                                                      Operand::Consume(expected)));

                // branch based on result
                let target_blocks: Vec<_> = vec![self.cfg.start_new_block(),
                                                 self.cfg.start_new_block()];
                self.cfg.terminate(block, Terminator::If {
                    cond: Operand::Consume(result),
                    targets: (target_blocks[0], target_blocks[1])
                });

                target_blocks
            }
        }
    }

    fn compare(&mut self,
               block: BasicBlock,
               fail_block: BasicBlock,
               span: Span,
               op: BinOp,
               left: Operand<'tcx>,
               right: Operand<'tcx>) -> BasicBlock {
        let bool_ty = self.hir.bool_ty();
        let result = self.temp(bool_ty);

        // result = op(left, right)
        self.cfg.push_assign(block, span, &result, Rvalue::BinaryOp(op, left, right));

        // branch based on result
        let target_block = self.cfg.start_new_block();
        self.cfg.terminate(block, Terminator::If {
            cond: Operand::Consume(result),
            targets: (target_block, fail_block)
        });

        target_block
    }

    /// Given that we are performing `test` against `test_lvalue`,
    /// this job sorts out what the status of `candidate` will be
    /// after the test. The `resulting_candidates` vector stores, for
    /// each possible outcome of `test`, a vector of the candidates
    /// that will result. This fn should add a (possibly modified)
    /// clone of candidate into `resulting_candidates` wherever
    /// appropriate.
    ///
    /// So, for example, if this candidate is `x @ Some(P0)` and the
    /// test is a variant test, then we would add `(x as Option).0 @
    /// P0` to the `resulting_candidates` entry corresponding to the
    /// variant `Some`.
    ///
    /// However, in some cases, the test may just not be relevant to
    /// candidate. For example, suppose we are testing whether `foo.x == 22`,
    /// but in one match arm we have `Foo { x: _, ... }`... in that case,
    /// the test for what value `x` has has no particular relevance
    /// to this candidate. In such cases, this function just returns false
    /// without doing anything. This is used by the overall `match_candidates`
    /// algorithm to structure the match as a whole. See `match_candidates` for
    /// more details.
    ///
    /// FIXME(#29623). In some cases, we have some tricky choices to
    /// make.  for example, if we are testing that `x == 22`, but the
    /// candidate is `x @ 13..55`, what should we do? In the event
    /// that the test is true, we know that the candidate applies, but
    /// in the event of false, we don't know that it *doesn't*
    /// apply. For now, we return false, indicate that the test does
    /// not apply to this candidate, but it might be we can get
    /// tighter match code if we do something a bit different.
    pub fn sort_candidate<'pat>(&mut self,
                                test_lvalue: &Lvalue<'tcx>,
                                test: &Test<'tcx>,
                                candidate: &Candidate<'pat, 'tcx>,
                                resulting_candidates: &mut [Vec<Candidate<'pat, 'tcx>>])
                                -> bool {
        // Find the match_pair for this lvalue (if any). At present,
        // afaik, there can be at most one. (In the future, if we
        // adopted a more general `@` operator, there might be more
        // than one, but it'd be very unusual to have two sides that
        // both require tests; you'd expect one side to be simplified
        // away.)
        let tested_match_pair = candidate.match_pairs.iter()
                                                     .enumerate()
                                                     .filter(|&(_, mp)| mp.lvalue == *test_lvalue)
                                                     .next();
        let (match_pair_index, match_pair) = match tested_match_pair {
            Some(pair) => pair,
            None => {
                // We are not testing this lvalue. Therefore, this
                // candidate applies to ALL outcomes.
                return false;
            }
        };

        match test.kind {
            // If we are performing a variant switch, then this
            // informs variant patterns, but nothing else.
            TestKind::Switch { adt_def: tested_adt_def } => {
                match *match_pair.pattern.kind {
                    PatternKind::Variant { adt_def, variant_index, ref subpatterns } => {
                        assert_eq!(adt_def, tested_adt_def);
                        let new_candidate =
                            self.candidate_after_variant_switch(match_pair_index,
                                                                adt_def,
                                                                variant_index,
                                                                subpatterns,
                                                                candidate);
                        resulting_candidates[variant_index].push(new_candidate);
                        true
                    }
                    _ => {
                        false
                    }
                }
            }

            // If we are performing a switch over integers, then this informs integer
            // equality, but nothing else.
            //
            // FIXME(#29623) we could use TestKind::Range to rule
            // things out here, in some cases.
            TestKind::SwitchInt { switch_ty: _, options: _, ref indices } => {
                match *match_pair.pattern.kind {
                    PatternKind::Constant { ref value }
                    if is_switch_ty(match_pair.pattern.ty) => {
                        let index = indices[value];
                        let new_candidate = self.candidate_without_match_pair(match_pair_index,
                                                                              candidate);
                        resulting_candidates[index].push(new_candidate);
                        true
                    }
                    _ => {
                        false
                    }
                }
            }

            TestKind::Eq { .. } |
            TestKind::Range { .. } |
            TestKind::Len { .. } => {
                // These are all binary tests.
                //
                // FIXME(#29623) we can be more clever here
                let pattern_test = self.test(&match_pair);
                if pattern_test.kind == test.kind {
                    let new_candidate = self.candidate_without_match_pair(match_pair_index,
                                                                          candidate);
                    resulting_candidates[0].push(new_candidate);
                    true
                } else {
                    false
                }
            }
        }
    }

    fn candidate_without_match_pair<'pat>(&mut self,
                                          match_pair_index: usize,
                                          candidate: &Candidate<'pat, 'tcx>)
                                          -> Candidate<'pat, 'tcx> {
        let other_match_pairs =
            candidate.match_pairs.iter()
                                 .enumerate()
                                 .filter(|&(index, _)| index != match_pair_index)
                                 .map(|(_, mp)| mp.clone())
                                 .collect();
        Candidate {
            match_pairs: other_match_pairs,
            bindings: candidate.bindings.clone(),
            guard: candidate.guard.clone(),
            arm_index: candidate.arm_index,
        }
    }

    fn candidate_after_variant_switch<'pat>(&mut self,
                                            match_pair_index: usize,
                                            adt_def: ty::AdtDef<'tcx>,
                                            variant_index: usize,
                                            subpatterns: &'pat [FieldPattern<'tcx>],
                                            candidate: &Candidate<'pat, 'tcx>)
                                            -> Candidate<'pat, 'tcx> {
        let match_pair = &candidate.match_pairs[match_pair_index];

        // So, if we have a match-pattern like `x @ Enum::Variant(P1, P2)`,
        // we want to create a set of derived match-patterns like
        // `(x as Variant).0 @ P1` and `(x as Variant).1 @ P1`.
        let elem = ProjectionElem::Downcast(adt_def, variant_index);
        let downcast_lvalue = match_pair.lvalue.clone().elem(elem); // `(x as Variant)`
        let consequent_match_pairs =
            subpatterns.iter()
                       .map(|subpattern| {
                           // e.g., `(x as Variant).0`
                           let lvalue = downcast_lvalue.clone().field(subpattern.field);
                           // e.g., `(x as Variant).0 @ P1`
                           MatchPair::new(lvalue, &subpattern.pattern)
                       });

        // In addition, we need all the other match pairs from the old candidate.
        let other_match_pairs =
            candidate.match_pairs.iter()
                                 .enumerate()
                                 .filter(|&(index, _)| index != match_pair_index)
                                 .map(|(_, mp)| mp.clone());

        let all_match_pairs = consequent_match_pairs.chain(other_match_pairs).collect();

        Candidate {
            match_pairs: all_match_pairs,
            bindings: candidate.bindings.clone(),
            guard: candidate.guard.clone(),
            arm_index: candidate.arm_index,
        }
    }

    fn error_simplifyable<'pat>(&mut self, match_pair: &MatchPair<'pat, 'tcx>) -> ! {
        self.hir.span_bug(match_pair.pattern.span,
                          &format!("simplifyable pattern found: {:?}", match_pair.pattern))
    }
}

fn is_switch_ty<'tcx>(ty: Ty<'tcx>) -> bool {
    ty.is_integral() || ty.is_char() || ty.is_bool()
}
