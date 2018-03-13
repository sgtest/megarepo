// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use dataflow::{FlowAtLocation, FlowsAtLocation};
use borrow_check::nll::region_infer::Cause;
use dataflow::MaybeInitializedPlaces;
use dataflow::move_paths::{HasMoveData, MoveData};
use rustc::mir::{BasicBlock, Location, Mir};
use rustc::mir::Local;
use rustc::ty::{Ty, TyCtxt, TypeFoldable};
use rustc::infer::InferOk;
use borrow_check::nll::type_check::AtLocation;
use util::liveness::LivenessResults;

use super::TypeChecker;

/// Combines liveness analysis with initialization analysis to
/// determine which variables are live at which points, both due to
/// ordinary uses and drops. Returns a set of (ty, location) pairs
/// that indicate which types must be live at which point in the CFG.
/// This vector is consumed by `constraint_generation`.
///
/// NB. This computation requires normalization; therefore, it must be
/// performed before
pub(super) fn generate<'gcx, 'tcx>(
    cx: &mut TypeChecker<'_, 'gcx, 'tcx>,
    mir: &Mir<'tcx>,
    liveness: &LivenessResults,
    flow_inits: &mut FlowAtLocation<MaybeInitializedPlaces<'_, 'gcx, 'tcx>>,
    move_data: &MoveData<'tcx>,
) {
    let tcx = cx.tcx();
    let mut generator = TypeLivenessGenerator {
        cx,
        tcx,
        mir,
        liveness,
        flow_inits,
        move_data,
    };

    for bb in mir.basic_blocks().indices() {
        generator.add_liveness_constraints(bb);
    }
}

struct TypeLivenessGenerator<'gen, 'typeck, 'flow, 'gcx, 'tcx>
where
    'typeck: 'gen,
    'flow: 'gen,
    'tcx: 'typeck + 'flow,
    'gcx: 'tcx,
{
    cx: &'gen mut TypeChecker<'typeck, 'gcx, 'tcx>,
    tcx: TyCtxt<'typeck, 'gcx, 'tcx>,
    mir: &'gen Mir<'tcx>,
    liveness: &'gen LivenessResults,
    flow_inits: &'gen mut FlowAtLocation<MaybeInitializedPlaces<'flow, 'gcx, 'tcx>>,
    move_data: &'gen MoveData<'tcx>,
}

impl<'gen, 'typeck, 'flow, 'gcx, 'tcx> TypeLivenessGenerator<'gen, 'typeck, 'flow, 'gcx, 'tcx> {
    /// Liveness constraints:
    ///
    /// > If a variable V is live at point P, then all regions R in the type of V
    /// > must include the point P.
    fn add_liveness_constraints(&mut self, bb: BasicBlock) {
        debug!("add_liveness_constraints(bb={:?})", bb);

        self.liveness
            .regular
            .simulate_block(self.mir, bb, |location, live_locals| {
                for live_local in live_locals.iter() {
                    let live_local_ty = self.mir.local_decls[live_local].ty;
                    let cause = Cause::LiveVar(live_local, location);
                    self.push_type_live_constraint(live_local_ty, location, cause);
                }
            });

        let mut all_live_locals: Vec<(Location, Vec<Local>)> = vec![];
        self.liveness
            .drop
            .simulate_block(self.mir, bb, |location, live_locals| {
                all_live_locals.push((location, live_locals.iter().collect()));
            });
        debug!(
            "add_liveness_constraints: all_live_locals={:#?}",
            all_live_locals
        );

        let terminator_index = self.mir.basic_blocks()[bb].statements.len();
        self.flow_inits.reset_to_entry_of(bb);
        while let Some((location, live_locals)) = all_live_locals.pop() {
            for live_local in live_locals {
                debug!(
                    "add_liveness_constraints: location={:?} live_local={:?}",
                    location, live_local
                );

                self.flow_inits.each_state_bit(|mpi_init| {
                    debug!(
                        "add_liveness_constraints: location={:?} initialized={:?}",
                        location,
                        &self.flow_inits.operator().move_data().move_paths[mpi_init]
                    );
                });

                let mpi = self.move_data.rev_lookup.find_local(live_local);
                if let Some(initialized_child) = self.flow_inits.has_any_child_of(mpi) {
                    debug!(
                        "add_liveness_constraints: mpi={:?} has initialized child {:?}",
                        self.move_data.move_paths[mpi],
                        self.move_data.move_paths[initialized_child]
                    );

                    let live_local_ty = self.mir.local_decls[live_local].ty;
                    self.add_drop_live_constraint(live_local, live_local_ty, location);
                }
            }

            if location.statement_index == terminator_index {
                debug!(
                    "add_liveness_constraints: reconstruct_terminator_effect from {:#?}",
                    location
                );
                self.flow_inits.reconstruct_terminator_effect(location);
            } else {
                debug!(
                    "add_liveness_constraints: reconstruct_statement_effect from {:#?}",
                    location
                );
                self.flow_inits.reconstruct_statement_effect(location);
            }
            self.flow_inits.apply_local_effect(location);
        }
    }

    /// Some variable with type `live_ty` is "regular live" at
    /// `location` -- i.e., it may be used later. This means that all
    /// regions appearing in the type `live_ty` must be live at
    /// `location`.
    fn push_type_live_constraint<T>(&mut self, value: T, location: Location, cause: Cause)
    where
        T: TypeFoldable<'tcx>,
    {
        debug!(
            "push_type_live_constraint(live_ty={:?}, location={:?})",
            value, location
        );

        self.tcx.for_each_free_region(&value, |live_region| {
            self.cx
                .constraints
                .liveness_set
                .push((live_region, location, cause.clone()));
        });
    }

    /// Some variable with type `live_ty` is "drop live" at `location`
    /// -- i.e., it may be dropped later. This means that *some* of
    /// the regions in its type must be live at `location`. The
    /// precise set will depend on the dropck constraints, and in
    /// particular this takes `#[may_dangle]` into account.
    fn add_drop_live_constraint(
        &mut self,
        dropped_local: Local,
        dropped_ty: Ty<'tcx>,
        location: Location,
    ) {
        debug!(
            "add_drop_live_constraint(dropped_local={:?}, dropped_ty={:?}, location={:?})",
            dropped_local, dropped_ty, location
        );

        // If we end visiting the same type twice (usually due to a cycle involving
        // associated types), we need to ensure that its region types match up with the type
        // we added to the 'known' map the first time around. For this reason, we need
        // our infcx to hold onto its calculated region constraints after each call
        // to dtorck_constraint_for_ty. Otherwise, normalizing the corresponding associated
        // type will end up instantiating the type with a new set of inference variables
        // Since this new type will never be in 'known', we end up looping forever.
        //
        // For this reason, we avoid calling TypeChecker.normalize, instead doing all normalization
        // ourselves in one large 'fully_perform_op' callback.
        let kind_constraints = self.cx
            .fully_perform_op(location.at_self(), |cx| {
                let span = cx.last_span;

                let mut final_obligations = Vec::new();
                let mut kind_constraints = Vec::new();

                let InferOk {
                    value: kinds,
                    obligations,
                } = cx.infcx
                    .at(&cx.misc(span), cx.param_env)
                    .dropck_outlives(dropped_ty);
                for kind in kinds {
                    // All things in the `outlives` array may be touched by
                    // the destructor and must be live at this point.
                    let cause = Cause::DropVar(dropped_local, location);
                    kind_constraints.push((kind, location, cause));
                }

                final_obligations.extend(obligations);

                Ok(InferOk {
                    value: kind_constraints,
                    obligations: final_obligations,
                })
            })
            .unwrap();

        for (kind, location, cause) in kind_constraints {
            self.push_type_live_constraint(kind, location, cause);
        }
    }
}
