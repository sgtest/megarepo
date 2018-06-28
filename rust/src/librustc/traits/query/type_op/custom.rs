// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use infer::{InferCtxt, InferOk};
use std::fmt;
use traits::query::Fallible;

use infer::canonical::query_result;
use infer::canonical::QueryRegionConstraint;
use std::rc::Rc;
use syntax::codemap::DUMMY_SP;
use traits::{ObligationCause, TraitEngine, TraitEngineExt};

pub struct CustomTypeOp<F, G> {
    closure: F,
    description: G,
}

impl<F, G> CustomTypeOp<F, G> {
    pub fn new<'gcx, 'tcx, R>(closure: F, description: G) -> Self
    where
        F: FnOnce(&InferCtxt<'_, 'gcx, 'tcx>) -> Fallible<InferOk<'tcx, R>>,
        G: Fn() -> String,
    {
        CustomTypeOp {
            closure,
            description,
        }
    }
}

impl<'gcx, 'tcx, F, R, G> super::TypeOp<'gcx, 'tcx> for CustomTypeOp<F, G>
where
    F: for<'a, 'cx> FnOnce(&'a InferCtxt<'cx, 'gcx, 'tcx>) -> Fallible<InferOk<'tcx, R>>,
    G: Fn() -> String,
{
    type Output = R;

    /// Processes the operation and all resulting obligations,
    /// returning the final result along with any region constraints
    /// (they will be given over to the NLL region solver).
    fn fully_perform(
        self,
        infcx: &InferCtxt<'_, 'gcx, 'tcx>,
    ) -> Fallible<(Self::Output, Option<Rc<Vec<QueryRegionConstraint<'tcx>>>>)> {
        if cfg!(debug_assertions) {
            info!("fully_perform({:?})", self);
        }

        scrape_region_constraints(infcx, || Ok((self.closure)(infcx)?))
    }
}

impl<F, G> fmt::Debug for CustomTypeOp<F, G>
where
    G: Fn() -> String,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", (self.description)())
    }
}

/// Executes `op` and then scrapes out all the "old style" region
/// constraints that result, creating query-region-constraints.
fn scrape_region_constraints<'gcx, 'tcx, R>(
    infcx: &InferCtxt<'_, 'gcx, 'tcx>,
    op: impl FnOnce() -> Fallible<InferOk<'tcx, R>>,
) -> Fallible<(R, Option<Rc<Vec<QueryRegionConstraint<'tcx>>>>)> {
    let mut fulfill_cx = TraitEngine::new(infcx.tcx);
    let dummy_body_id = ObligationCause::dummy().body_id;
    let InferOk { value, obligations } = infcx.commit_if_ok(|_| op())?;
    debug_assert!(obligations.iter().all(|o| o.cause.body_id == dummy_body_id));
    fulfill_cx.register_predicate_obligations(infcx, obligations);
    if let Err(e) = fulfill_cx.select_all_or_error(infcx) {
        infcx.tcx.sess.diagnostic().delay_span_bug(
            DUMMY_SP,
            &format!("errors selecting obligation during MIR typeck: {:?}", e),
        );
    }

    let region_obligations = infcx.take_registered_region_obligations();

    let region_constraint_data = infcx.take_and_reset_region_constraints();

    let outlives =
        query_result::make_query_outlives(infcx.tcx, region_obligations, &region_constraint_data);

    if outlives.is_empty() {
        Ok((value, None))
    } else {
        Ok((value, Some(Rc::new(outlives))))
    }
}
