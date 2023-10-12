use super::Error;

use super::graph;

use graph::{BasicCoverageBlock, BcbBranch, CoverageGraph, TraverseCoverageGraphWithLoops};

use rustc_data_structures::fx::FxHashMap;
use rustc_data_structures::graph::WithNumNodes;
use rustc_index::bit_set::BitSet;
use rustc_index::IndexVec;
use rustc_middle::mir::coverage::*;

use std::fmt::{self, Debug};

const NESTED_INDENT: &str = "    ";

/// The coverage counter or counter expression associated with a particular
/// BCB node or BCB edge.
#[derive(Clone)]
pub(super) enum BcbCounter {
    Counter { id: CounterId },
    Expression { id: ExpressionId, lhs: Operand, op: Op, rhs: Operand },
}

impl BcbCounter {
    fn is_expression(&self) -> bool {
        matches!(self, Self::Expression { .. })
    }

    pub(super) fn as_operand(&self) -> Operand {
        match *self {
            BcbCounter::Counter { id, .. } => Operand::Counter(id),
            BcbCounter::Expression { id, .. } => Operand::Expression(id),
        }
    }
}

impl Debug for BcbCounter {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Counter { id, .. } => write!(fmt, "Counter({:?})", id.index()),
            Self::Expression { id, lhs, op, rhs } => write!(
                fmt,
                "Expression({:?}) = {:?} {} {:?}",
                id.index(),
                lhs,
                match op {
                    Op::Add => "+",
                    Op::Subtract => "-",
                },
                rhs,
            ),
        }
    }
}

/// Generates and stores coverage counter and coverage expression information
/// associated with nodes/edges in the BCB graph.
pub(super) struct CoverageCounters {
    next_counter_id: CounterId,
    next_expression_id: ExpressionId,

    /// Coverage counters/expressions that are associated with individual BCBs.
    bcb_counters: IndexVec<BasicCoverageBlock, Option<BcbCounter>>,
    /// Coverage counters/expressions that are associated with the control-flow
    /// edge between two BCBs.
    bcb_edge_counters: FxHashMap<(BasicCoverageBlock, BasicCoverageBlock), BcbCounter>,
    /// Tracks which BCBs have a counter associated with some incoming edge.
    /// Only used by debug assertions, to verify that BCBs with incoming edge
    /// counters do not have their own physical counters (expressions are allowed).
    bcb_has_incoming_edge_counters: BitSet<BasicCoverageBlock>,
    /// Expression nodes that are not directly associated with any particular
    /// BCB/edge, but are needed as operands to more complex expressions.
    /// These are always [`BcbCounter::Expression`].
    pub(super) intermediate_expressions: Vec<BcbCounter>,
}

impl CoverageCounters {
    pub(super) fn new(basic_coverage_blocks: &CoverageGraph) -> Self {
        let num_bcbs = basic_coverage_blocks.num_nodes();

        Self {
            next_counter_id: CounterId::START,
            next_expression_id: ExpressionId::START,

            bcb_counters: IndexVec::from_elem_n(None, num_bcbs),
            bcb_edge_counters: FxHashMap::default(),
            bcb_has_incoming_edge_counters: BitSet::new_empty(num_bcbs),
            intermediate_expressions: Vec::new(),
        }
    }

    /// Makes [`BcbCounter`] `Counter`s and `Expressions` for the `BasicCoverageBlock`s directly or
    /// indirectly associated with coverage spans, and accumulates additional `Expression`s
    /// representing intermediate values.
    pub fn make_bcb_counters(
        &mut self,
        basic_coverage_blocks: &CoverageGraph,
        bcb_has_coverage_spans: impl Fn(BasicCoverageBlock) -> bool,
    ) -> Result<(), Error> {
        MakeBcbCounters::new(self, basic_coverage_blocks).make_bcb_counters(bcb_has_coverage_spans)
    }

    fn make_counter(&mut self) -> BcbCounter {
        let id = self.next_counter();
        BcbCounter::Counter { id }
    }

    fn make_expression(&mut self, lhs: Operand, op: Op, rhs: Operand) -> BcbCounter {
        let id = self.next_expression();
        BcbCounter::Expression { id, lhs, op, rhs }
    }

    /// Counter IDs start from one and go up.
    fn next_counter(&mut self) -> CounterId {
        let next = self.next_counter_id;
        self.next_counter_id = self.next_counter_id + 1;
        next
    }

    /// Expression IDs start from 0 and go up.
    /// (Counter IDs and Expression IDs are distinguished by the `Operand` enum.)
    fn next_expression(&mut self) -> ExpressionId {
        let next = self.next_expression_id;
        self.next_expression_id = self.next_expression_id + 1;
        next
    }

    fn set_bcb_counter(
        &mut self,
        bcb: BasicCoverageBlock,
        counter_kind: BcbCounter,
    ) -> Result<Operand, Error> {
        debug_assert!(
            // If the BCB has an edge counter (to be injected into a new `BasicBlock`), it can also
            // have an expression (to be injected into an existing `BasicBlock` represented by this
            // `BasicCoverageBlock`).
            counter_kind.is_expression() || !self.bcb_has_incoming_edge_counters.contains(bcb),
            "attempt to add a `Counter` to a BCB target with existing incoming edge counters"
        );
        let operand = counter_kind.as_operand();
        if let Some(replaced) = self.bcb_counters[bcb].replace(counter_kind) {
            Error::from_string(format!(
                "attempt to set a BasicCoverageBlock coverage counter more than once; \
                {bcb:?} already had counter {replaced:?}",
            ))
        } else {
            Ok(operand)
        }
    }

    fn set_bcb_edge_counter(
        &mut self,
        from_bcb: BasicCoverageBlock,
        to_bcb: BasicCoverageBlock,
        counter_kind: BcbCounter,
    ) -> Result<Operand, Error> {
        if level_enabled!(tracing::Level::DEBUG) {
            // If the BCB has an edge counter (to be injected into a new `BasicBlock`), it can also
            // have an expression (to be injected into an existing `BasicBlock` represented by this
            // `BasicCoverageBlock`).
            if self.bcb_counter(to_bcb).is_some_and(|c| !c.is_expression()) {
                return Error::from_string(format!(
                    "attempt to add an incoming edge counter from {from_bcb:?} when the target BCB already \
                    has a `Counter`"
                ));
            }
        }
        self.bcb_has_incoming_edge_counters.insert(to_bcb);
        let operand = counter_kind.as_operand();
        if let Some(replaced) = self.bcb_edge_counters.insert((from_bcb, to_bcb), counter_kind) {
            Error::from_string(format!(
                "attempt to set an edge counter more than once; from_bcb: \
                {from_bcb:?} already had counter {replaced:?}",
            ))
        } else {
            Ok(operand)
        }
    }

    pub(super) fn bcb_counter(&self, bcb: BasicCoverageBlock) -> Option<&BcbCounter> {
        self.bcb_counters[bcb].as_ref()
    }

    pub(super) fn take_bcb_counter(&mut self, bcb: BasicCoverageBlock) -> Option<BcbCounter> {
        self.bcb_counters[bcb].take()
    }

    pub(super) fn drain_bcb_counters(
        &mut self,
    ) -> impl Iterator<Item = (BasicCoverageBlock, BcbCounter)> + '_ {
        self.bcb_counters
            .iter_enumerated_mut()
            .filter_map(|(bcb, counter)| Some((bcb, counter.take()?)))
    }

    pub(super) fn drain_bcb_edge_counters(
        &mut self,
    ) -> impl Iterator<Item = ((BasicCoverageBlock, BasicCoverageBlock), BcbCounter)> + '_ {
        self.bcb_edge_counters.drain()
    }
}

/// Traverse the `CoverageGraph` and add either a `Counter` or `Expression` to every BCB, to be
/// injected with coverage spans. `Expressions` have no runtime overhead, so if a viable expression
/// (adding or subtracting two other counters or expressions) can compute the same result as an
/// embedded counter, an `Expression` should be used.
struct MakeBcbCounters<'a> {
    coverage_counters: &'a mut CoverageCounters,
    basic_coverage_blocks: &'a CoverageGraph,
}

impl<'a> MakeBcbCounters<'a> {
    fn new(
        coverage_counters: &'a mut CoverageCounters,
        basic_coverage_blocks: &'a CoverageGraph,
    ) -> Self {
        Self { coverage_counters, basic_coverage_blocks }
    }

    /// If two `BasicCoverageBlock`s branch from another `BasicCoverageBlock`, one of the branches
    /// can be counted by `Expression` by subtracting the other branch from the branching
    /// block. Otherwise, the `BasicCoverageBlock` executed the least should have the `Counter`.
    /// One way to predict which branch executes the least is by considering loops. A loop is exited
    /// at a branch, so the branch that jumps to a `BasicCoverageBlock` outside the loop is almost
    /// always executed less than the branch that does not exit the loop.
    ///
    /// Returns any non-code-span expressions created to represent intermediate values (such as to
    /// add two counters so the result can be subtracted from another counter), or an Error with
    /// message for subsequent debugging.
    fn make_bcb_counters(
        &mut self,
        bcb_has_coverage_spans: impl Fn(BasicCoverageBlock) -> bool,
    ) -> Result<(), Error> {
        debug!("make_bcb_counters(): adding a counter or expression to each BasicCoverageBlock");

        // Walk the `CoverageGraph`. For each `BasicCoverageBlock` node with an associated
        // coverage span, add a counter. If the `BasicCoverageBlock` branches, add a counter or
        // expression to each branch `BasicCoverageBlock` (if the branch BCB has only one incoming
        // edge) or edge from the branching BCB to the branch BCB (if the branch BCB has multiple
        // incoming edges).
        //
        // The `TraverseCoverageGraphWithLoops` traversal ensures that, when a loop is encountered,
        // all `BasicCoverageBlock` nodes in the loop are visited before visiting any node outside
        // the loop. The `traversal` state includes a `context_stack`, providing a way to know if
        // the current BCB is in one or more nested loops or not.
        let mut traversal = TraverseCoverageGraphWithLoops::new(&self.basic_coverage_blocks);
        while let Some(bcb) = traversal.next() {
            if bcb_has_coverage_spans(bcb) {
                debug!("{:?} has at least one coverage span. Get or make its counter", bcb);
                let branching_counter_operand = self.get_or_make_counter_operand(bcb)?;

                if self.bcb_needs_branch_counters(bcb) {
                    self.make_branch_counters(&traversal, bcb, branching_counter_operand)?;
                }
            } else {
                debug!(
                    "{:?} does not have any coverage spans. A counter will only be added if \
                    and when a covered BCB has an expression dependency.",
                    bcb,
                );
            }
        }

        if traversal.is_complete() {
            Ok(())
        } else {
            Error::from_string(format!(
                "`TraverseCoverageGraphWithLoops` missed some `BasicCoverageBlock`s: {:?}",
                traversal.unvisited(),
            ))
        }
    }

    fn make_branch_counters(
        &mut self,
        traversal: &TraverseCoverageGraphWithLoops<'_>,
        branching_bcb: BasicCoverageBlock,
        branching_counter_operand: Operand,
    ) -> Result<(), Error> {
        let branches = self.bcb_branches(branching_bcb);
        debug!(
            "{:?} has some branch(es) without counters:\n  {}",
            branching_bcb,
            branches
                .iter()
                .map(|branch| { format!("{:?}: {:?}", branch, self.branch_counter(branch)) })
                .collect::<Vec<_>>()
                .join("\n  "),
        );

        // Use the `traversal` state to decide if a subset of the branches exit a loop, making it
        // likely that branch is executed less than branches that do not exit the same loop. In this
        // case, any branch that does not exit the loop (and has not already been assigned a
        // counter) should be counted by expression, if possible. (If a preferred expression branch
        // is not selected based on the loop context, select any branch without an existing
        // counter.)
        let expression_branch = self.choose_preferred_expression_branch(traversal, &branches);

        // Assign a Counter or Expression to each branch, plus additional `Expression`s, as needed,
        // to sum up intermediate results.
        let mut some_sumup_counter_operand = None;
        for branch in branches {
            // Skip the selected `expression_branch`, if any. It's expression will be assigned after
            // all others.
            if branch != expression_branch {
                let branch_counter_operand = if branch.is_only_path_to_target() {
                    debug!(
                        "  {:?} has only one incoming edge (from {:?}), so adding a \
                        counter",
                        branch, branching_bcb
                    );
                    self.get_or_make_counter_operand(branch.target_bcb)?
                } else {
                    debug!("  {:?} has multiple incoming edges, so adding an edge counter", branch);
                    self.get_or_make_edge_counter_operand(branching_bcb, branch.target_bcb)?
                };
                if let Some(sumup_counter_operand) =
                    some_sumup_counter_operand.replace(branch_counter_operand)
                {
                    let intermediate_expression = self.coverage_counters.make_expression(
                        branch_counter_operand,
                        Op::Add,
                        sumup_counter_operand,
                    );
                    debug!("  [new intermediate expression: {:?}]", intermediate_expression);
                    let intermediate_expression_operand = intermediate_expression.as_operand();
                    self.coverage_counters.intermediate_expressions.push(intermediate_expression);
                    some_sumup_counter_operand.replace(intermediate_expression_operand);
                }
            }
        }

        // Assign the final expression to the `expression_branch` by subtracting the total of all
        // other branches from the counter of the branching BCB.
        let sumup_counter_operand =
            some_sumup_counter_operand.expect("sumup_counter_operand should have a value");
        debug!(
            "Making an expression for the selected expression_branch: {:?} \
            (expression_branch predecessors: {:?})",
            expression_branch,
            self.bcb_predecessors(expression_branch.target_bcb),
        );
        let expression = self.coverage_counters.make_expression(
            branching_counter_operand,
            Op::Subtract,
            sumup_counter_operand,
        );
        debug!("{:?} gets an expression: {:?}", expression_branch, expression);
        let bcb = expression_branch.target_bcb;
        if expression_branch.is_only_path_to_target() {
            self.coverage_counters.set_bcb_counter(bcb, expression)?;
        } else {
            self.coverage_counters.set_bcb_edge_counter(branching_bcb, bcb, expression)?;
        }
        Ok(())
    }

    fn get_or_make_counter_operand(&mut self, bcb: BasicCoverageBlock) -> Result<Operand, Error> {
        self.recursive_get_or_make_counter_operand(bcb, 1)
    }

    fn recursive_get_or_make_counter_operand(
        &mut self,
        bcb: BasicCoverageBlock,
        debug_indent_level: usize,
    ) -> Result<Operand, Error> {
        // If the BCB already has a counter, return it.
        if let Some(counter_kind) = &self.coverage_counters.bcb_counters[bcb] {
            debug!(
                "{}{:?} already has a counter: {:?}",
                NESTED_INDENT.repeat(debug_indent_level),
                bcb,
                counter_kind,
            );
            return Ok(counter_kind.as_operand());
        }

        // A BCB with only one incoming edge gets a simple `Counter` (via `make_counter()`).
        // Also, a BCB that loops back to itself gets a simple `Counter`. This may indicate the
        // program results in a tight infinite loop, but it should still compile.
        let one_path_to_target = self.bcb_has_one_path_to_target(bcb);
        if one_path_to_target || self.bcb_predecessors(bcb).contains(&bcb) {
            let counter_kind = self.coverage_counters.make_counter();
            if one_path_to_target {
                debug!(
                    "{}{:?} gets a new counter: {:?}",
                    NESTED_INDENT.repeat(debug_indent_level),
                    bcb,
                    counter_kind,
                );
            } else {
                debug!(
                    "{}{:?} has itself as its own predecessor. It can't be part of its own \
                    Expression sum, so it will get its own new counter: {:?}. (Note, the compiled \
                    code will generate an infinite loop.)",
                    NESTED_INDENT.repeat(debug_indent_level),
                    bcb,
                    counter_kind,
                );
            }
            return self.coverage_counters.set_bcb_counter(bcb, counter_kind);
        }

        // A BCB with multiple incoming edges can compute its count by `Expression`, summing up the
        // counters and/or expressions of its incoming edges. This will recursively get or create
        // counters for those incoming edges first, then call `make_expression()` to sum them up,
        // with additional intermediate expressions as needed.
        let mut predecessors = self.bcb_predecessors(bcb).to_owned().into_iter();
        debug!(
            "{}{:?} has multiple incoming edges and will get an expression that sums them up...",
            NESTED_INDENT.repeat(debug_indent_level),
            bcb,
        );
        let first_edge_counter_operand = self.recursive_get_or_make_edge_counter_operand(
            predecessors.next().unwrap(),
            bcb,
            debug_indent_level + 1,
        )?;
        let mut some_sumup_edge_counter_operand = None;
        for predecessor in predecessors {
            let edge_counter_operand = self.recursive_get_or_make_edge_counter_operand(
                predecessor,
                bcb,
                debug_indent_level + 1,
            )?;
            if let Some(sumup_edge_counter_operand) =
                some_sumup_edge_counter_operand.replace(edge_counter_operand)
            {
                let intermediate_expression = self.coverage_counters.make_expression(
                    sumup_edge_counter_operand,
                    Op::Add,
                    edge_counter_operand,
                );
                debug!(
                    "{}new intermediate expression: {:?}",
                    NESTED_INDENT.repeat(debug_indent_level),
                    intermediate_expression
                );
                let intermediate_expression_operand = intermediate_expression.as_operand();
                self.coverage_counters.intermediate_expressions.push(intermediate_expression);
                some_sumup_edge_counter_operand.replace(intermediate_expression_operand);
            }
        }
        let counter_kind = self.coverage_counters.make_expression(
            first_edge_counter_operand,
            Op::Add,
            some_sumup_edge_counter_operand.unwrap(),
        );
        debug!(
            "{}{:?} gets a new counter (sum of predecessor counters): {:?}",
            NESTED_INDENT.repeat(debug_indent_level),
            bcb,
            counter_kind
        );
        self.coverage_counters.set_bcb_counter(bcb, counter_kind)
    }

    fn get_or_make_edge_counter_operand(
        &mut self,
        from_bcb: BasicCoverageBlock,
        to_bcb: BasicCoverageBlock,
    ) -> Result<Operand, Error> {
        self.recursive_get_or_make_edge_counter_operand(from_bcb, to_bcb, 1)
    }

    fn recursive_get_or_make_edge_counter_operand(
        &mut self,
        from_bcb: BasicCoverageBlock,
        to_bcb: BasicCoverageBlock,
        debug_indent_level: usize,
    ) -> Result<Operand, Error> {
        // If the source BCB has only one successor (assumed to be the given target), an edge
        // counter is unnecessary. Just get or make a counter for the source BCB.
        let successors = self.bcb_successors(from_bcb).iter();
        if successors.len() == 1 {
            return self.recursive_get_or_make_counter_operand(from_bcb, debug_indent_level + 1);
        }

        // If the edge already has a counter, return it.
        if let Some(counter_kind) =
            self.coverage_counters.bcb_edge_counters.get(&(from_bcb, to_bcb))
        {
            debug!(
                "{}Edge {:?}->{:?} already has a counter: {:?}",
                NESTED_INDENT.repeat(debug_indent_level),
                from_bcb,
                to_bcb,
                counter_kind
            );
            return Ok(counter_kind.as_operand());
        }

        // Make a new counter to count this edge.
        let counter_kind = self.coverage_counters.make_counter();
        debug!(
            "{}Edge {:?}->{:?} gets a new counter: {:?}",
            NESTED_INDENT.repeat(debug_indent_level),
            from_bcb,
            to_bcb,
            counter_kind
        );
        self.coverage_counters.set_bcb_edge_counter(from_bcb, to_bcb, counter_kind)
    }

    /// Select a branch for the expression, either the recommended `reloop_branch`, or if none was
    /// found, select any branch.
    fn choose_preferred_expression_branch(
        &self,
        traversal: &TraverseCoverageGraphWithLoops<'_>,
        branches: &[BcbBranch],
    ) -> BcbBranch {
        let good_reloop_branch = self.find_good_reloop_branch(traversal, &branches);
        if let Some(reloop_branch) = good_reloop_branch {
            assert!(self.branch_has_no_counter(&reloop_branch));
            debug!("Selecting reloop branch {reloop_branch:?} to get an expression");
            reloop_branch
        } else {
            let &branch_without_counter =
                branches.iter().find(|&branch| self.branch_has_no_counter(branch)).expect(
                    "needs_branch_counters was `true` so there should be at least one \
                    branch",
                );
            debug!(
                "Selecting any branch={:?} that still needs a counter, to get the \
                `Expression` because there was no `reloop_branch`, or it already had a \
                counter",
                branch_without_counter
            );
            branch_without_counter
        }
    }

    /// Tries to find a branch that leads back to the top of a loop, and that
    /// doesn't already have a counter. Such branches are good candidates to
    /// be given an expression (instead of a physical counter), because they
    /// will tend to be executed more times than a loop-exit branch.
    fn find_good_reloop_branch(
        &self,
        traversal: &TraverseCoverageGraphWithLoops<'_>,
        branches: &[BcbBranch],
    ) -> Option<BcbBranch> {
        // Consider each loop on the current traversal context stack, top-down.
        for reloop_bcbs in traversal.reloop_bcbs_per_loop() {
            let mut all_branches_exit_this_loop = true;

            // Try to find a branch that doesn't exit this loop and doesn't
            // already have a counter.
            for &branch in branches {
                // A branch is a reloop branch if it dominates any BCB that has
                // an edge back to the loop header. (Other branches are exits.)
                let is_reloop_branch = reloop_bcbs.iter().any(|&reloop_bcb| {
                    self.basic_coverage_blocks.dominates(branch.target_bcb, reloop_bcb)
                });

                if is_reloop_branch {
                    all_branches_exit_this_loop = false;
                    if self.branch_has_no_counter(&branch) {
                        // We found a good branch to be given an expression.
                        return Some(branch);
                    }
                    // Keep looking for another reloop branch without a counter.
                } else {
                    // This branch exits the loop.
                }
            }

            if !all_branches_exit_this_loop {
                // We found one or more reloop branches, but all of them already
                // have counters. Let the caller choose one of the exit branches.
                debug!("All reloop branches had counters; skip checking the other loops");
                return None;
            }

            // All of the branches exit this loop, so keep looking for a good
            // reloop branch for one of the outer loops.
        }

        None
    }

    #[inline]
    fn bcb_predecessors(&self, bcb: BasicCoverageBlock) -> &[BasicCoverageBlock] {
        &self.basic_coverage_blocks.predecessors[bcb]
    }

    #[inline]
    fn bcb_successors(&self, bcb: BasicCoverageBlock) -> &[BasicCoverageBlock] {
        &self.basic_coverage_blocks.successors[bcb]
    }

    #[inline]
    fn bcb_branches(&self, from_bcb: BasicCoverageBlock) -> Vec<BcbBranch> {
        self.bcb_successors(from_bcb)
            .iter()
            .map(|&to_bcb| BcbBranch::from_to(from_bcb, to_bcb, &self.basic_coverage_blocks))
            .collect::<Vec<_>>()
    }

    fn bcb_needs_branch_counters(&self, bcb: BasicCoverageBlock) -> bool {
        let branch_needs_a_counter = |branch: &BcbBranch| self.branch_has_no_counter(branch);
        let branches = self.bcb_branches(bcb);
        branches.len() > 1 && branches.iter().any(branch_needs_a_counter)
    }

    fn branch_has_no_counter(&self, branch: &BcbBranch) -> bool {
        self.branch_counter(branch).is_none()
    }

    fn branch_counter(&self, branch: &BcbBranch) -> Option<&BcbCounter> {
        let to_bcb = branch.target_bcb;
        if let Some(from_bcb) = branch.edge_from_bcb {
            self.coverage_counters.bcb_edge_counters.get(&(from_bcb, to_bcb))
        } else {
            self.coverage_counters.bcb_counters[to_bcb].as_ref()
        }
    }

    /// Returns true if the BasicCoverageBlock has zero or one incoming edge. (If zero, it should be
    /// the entry point for the function.)
    #[inline]
    fn bcb_has_one_path_to_target(&self, bcb: BasicCoverageBlock) -> bool {
        self.bcb_predecessors(bcb).len() <= 1
    }
}
