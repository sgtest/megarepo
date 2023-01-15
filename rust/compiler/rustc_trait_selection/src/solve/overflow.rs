use rustc_infer::infer::canonical::Canonical;
use rustc_infer::traits::query::NoSolution;
use rustc_middle::ty::TyCtxt;
use rustc_session::Limit;

use super::cache::response_no_constraints;
use super::{Certainty, EvalCtxt, MaybeCause, QueryResult};

/// When detecting a solver overflow, we return ambiguity. Overflow can be
/// *hidden* by either a fatal error in an **AND** or a trivial success in an **OR**.
///
/// This is in issue in case of exponential blowup, e.g. if each goal on the stack
/// has multiple nested (overflowing) candidates. To deal with this, we reduce the limit
/// used by the solver when hitting the default limit for the first time.
///
/// FIXME: Get tests where always using the `default_limit` results in a hang and refer
/// to them here. We can also improve the overflow strategy if necessary.
pub(super) struct OverflowData {
    default_limit: Limit,
    current_limit: Limit,
    /// When proving an **AND** we have to repeatedly iterate over the yet unproven goals.
    ///
    /// Because of this each iteration also increases the depth in addition to the stack
    /// depth.
    additional_depth: usize,
}

impl OverflowData {
    pub(super) fn new(tcx: TyCtxt<'_>) -> OverflowData {
        let default_limit = tcx.recursion_limit();
        OverflowData { default_limit, current_limit: default_limit, additional_depth: 0 }
    }

    #[inline]
    pub(super) fn did_overflow(&self) -> bool {
        self.default_limit.0 != self.current_limit.0
    }

    #[inline]
    pub(super) fn has_overflow(&self, depth: usize) -> bool {
        !self.current_limit.value_within_limit(depth + self.additional_depth)
    }

    /// Updating the current limit when hitting overflow.
    fn deal_with_overflow(&mut self) {
        // When first hitting overflow we reduce the overflow limit
        // for all future goals to prevent hangs if there's an exponental
        // blowup.
        self.current_limit.0 = self.default_limit.0 / 8;
    }
}

impl<'tcx> EvalCtxt<'tcx> {
    pub(super) fn deal_with_overflow(
        &mut self,
        goal: Canonical<'tcx, impl Sized>,
    ) -> QueryResult<'tcx> {
        self.overflow_data.deal_with_overflow();
        response_no_constraints(self.tcx, goal, Certainty::Maybe(MaybeCause::Overflow))
    }

    /// A `while`-loop which tracks overflow.
    pub(super) fn repeat_while_none(
        &mut self,
        mut loop_body: impl FnMut(&mut Self) -> Option<Result<Certainty, NoSolution>>,
    ) -> Result<Certainty, NoSolution> {
        let start_depth = self.overflow_data.additional_depth;
        let depth = self.provisional_cache.current_depth();
        while !self.overflow_data.has_overflow(depth) {
            if let Some(result) = loop_body(self) {
                self.overflow_data.additional_depth = start_depth;
                return result;
            }

            self.overflow_data.additional_depth += 1;
        }
        self.overflow_data.additional_depth = start_depth;
        self.overflow_data.deal_with_overflow();
        Ok(Certainty::Maybe(MaybeCause::Overflow))
    }
}
