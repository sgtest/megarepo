use rustc_middle::bug;
use rustc_middle::mir::coverage::CoverageKind;
use rustc_middle::mir::{
    self, AggregateKind, FakeReadCause, Rvalue, Statement, StatementKind, Terminator,
    TerminatorKind,
};
use rustc_span::{ExpnKind, MacroKind, Span, Symbol};

use crate::coverage::graph::{
    BasicCoverageBlock, BasicCoverageBlockData, CoverageGraph, START_BCB,
};
use crate::coverage::spans::Covspan;
use crate::coverage::ExtractedHirInfo;

pub(crate) struct ExtractedCovspans {
    pub(crate) covspans: Vec<SpanFromMir>,
    pub(crate) holes: Vec<Hole>,
}

/// Traverses the MIR body to produce an initial collection of coverage-relevant
/// spans, each associated with a node in the coverage graph (BCB) and possibly
/// other metadata.
pub(crate) fn extract_covspans_and_holes_from_mir(
    mir_body: &mir::Body<'_>,
    hir_info: &ExtractedHirInfo,
    basic_coverage_blocks: &CoverageGraph,
) -> ExtractedCovspans {
    let &ExtractedHirInfo { body_span, .. } = hir_info;

    let mut covspans = vec![];
    let mut holes = vec![];

    for (bcb, bcb_data) in basic_coverage_blocks.iter_enumerated() {
        bcb_to_initial_coverage_spans(
            mir_body,
            body_span,
            bcb,
            bcb_data,
            &mut covspans,
            &mut holes,
        );
    }

    // Only add the signature span if we found at least one span in the body.
    if !covspans.is_empty() || !holes.is_empty() {
        // If there is no usable signature span, add a fake one (before refinement)
        // to avoid an ugly gap between the body start and the first real span.
        // FIXME: Find a more principled way to solve this problem.
        let fn_sig_span = hir_info.fn_sig_span_extended.unwrap_or_else(|| body_span.shrink_to_lo());
        covspans.push(SpanFromMir::for_fn_sig(fn_sig_span));
    }

    ExtractedCovspans { covspans, holes }
}

// Generate a set of coverage spans from the filtered set of `Statement`s and `Terminator`s of
// the `BasicBlock`(s) in the given `BasicCoverageBlockData`. One coverage span is generated
// for each `Statement` and `Terminator`. (Note that subsequent stages of coverage analysis will
// merge some coverage spans, at which point a coverage span may represent multiple
// `Statement`s and/or `Terminator`s.)
fn bcb_to_initial_coverage_spans<'a, 'tcx>(
    mir_body: &'a mir::Body<'tcx>,
    body_span: Span,
    bcb: BasicCoverageBlock,
    bcb_data: &'a BasicCoverageBlockData,
    initial_covspans: &mut Vec<SpanFromMir>,
    holes: &mut Vec<Hole>,
) {
    for &bb in &bcb_data.basic_blocks {
        let data = &mir_body[bb];

        let unexpand = move |expn_span| {
            unexpand_into_body_span_with_visible_macro(expn_span, body_span)
                // Discard any spans that fill the entire body, because they tend
                // to represent compiler-inserted code, e.g. implicitly returning `()`.
                .filter(|(span, _)| !span.source_equal(body_span))
        };

        let mut extract_statement_span = |statement| {
            let expn_span = filtered_statement_span(statement)?;
            let (span, visible_macro) = unexpand(expn_span)?;

            // A statement that looks like the assignment of a closure expression
            // is treated as a "hole" span, to be carved out of other spans.
            if is_closure_like(statement) {
                holes.push(Hole { span });
            } else {
                initial_covspans.push(SpanFromMir::new(span, visible_macro, bcb));
            }
            Some(())
        };
        for statement in data.statements.iter() {
            extract_statement_span(statement);
        }

        let mut extract_terminator_span = |terminator| {
            let expn_span = filtered_terminator_span(terminator)?;
            let (span, visible_macro) = unexpand(expn_span)?;

            initial_covspans.push(SpanFromMir::new(span, visible_macro, bcb));
            Some(())
        };
        extract_terminator_span(data.terminator());
    }
}

fn is_closure_like(statement: &Statement<'_>) -> bool {
    match statement.kind {
        StatementKind::Assign(box (_, Rvalue::Aggregate(box ref agg_kind, _))) => match agg_kind {
            AggregateKind::Closure(_, _)
            | AggregateKind::Coroutine(_, _)
            | AggregateKind::CoroutineClosure(..) => true,
            _ => false,
        },
        _ => false,
    }
}

/// If the MIR `Statement` has a span contributive to computing coverage spans,
/// return it; otherwise return `None`.
fn filtered_statement_span(statement: &Statement<'_>) -> Option<Span> {
    match statement.kind {
        // These statements have spans that are often outside the scope of the executed source code
        // for their parent `BasicBlock`.
        StatementKind::StorageLive(_)
        | StatementKind::StorageDead(_)
        | StatementKind::ConstEvalCounter
        | StatementKind::Nop => None,

        // FIXME(#78546): MIR InstrumentCoverage - Can the source_info.span for `FakeRead`
        // statements be more consistent?
        //
        // FakeReadCause::ForGuardBinding, in this example:
        //     match somenum {
        //         x if x < 1 => { ... }
        //     }...
        // The BasicBlock within the match arm code included one of these statements, but the span
        // for it covered the `1` in this source. The actual statements have nothing to do with that
        // source span:
        //     FakeRead(ForGuardBinding, _4);
        // where `_4` is:
        //     _4 = &_1; (at the span for the first `x`)
        // and `_1` is the `Place` for `somenum`.
        //
        // If and when the Issue is resolved, remove this special case match pattern:
        StatementKind::FakeRead(box (FakeReadCause::ForGuardBinding, _)) => None,

        // Retain spans from most other statements.
        StatementKind::FakeRead(_)
        | StatementKind::Intrinsic(..)
        | StatementKind::Coverage(
            // The purpose of `SpanMarker` is to be matched and accepted here.
            CoverageKind::SpanMarker,
        )
        | StatementKind::Assign(_)
        | StatementKind::SetDiscriminant { .. }
        | StatementKind::Deinit(..)
        | StatementKind::Retag(_, _)
        | StatementKind::PlaceMention(..)
        | StatementKind::AscribeUserType(_, _) => Some(statement.source_info.span),

        // Block markers are used for branch coverage, so ignore them here.
        StatementKind::Coverage(CoverageKind::BlockMarker { .. }) => None,

        // These coverage statements should not exist prior to coverage instrumentation.
        StatementKind::Coverage(
            CoverageKind::CounterIncrement { .. }
            | CoverageKind::ExpressionUsed { .. }
            | CoverageKind::CondBitmapUpdate { .. }
            | CoverageKind::TestVectorBitmapUpdate { .. },
        ) => bug!(
            "Unexpected coverage statement found during coverage instrumentation: {statement:?}"
        ),
    }
}

/// If the MIR `Terminator` has a span contributive to computing coverage spans,
/// return it; otherwise return `None`.
fn filtered_terminator_span(terminator: &Terminator<'_>) -> Option<Span> {
    match terminator.kind {
        // These terminators have spans that don't positively contribute to computing a reasonable
        // span of actually executed source code. (For example, SwitchInt terminators extracted from
        // an `if condition { block }` has a span that includes the executed block, if true,
        // but for coverage, the code region executed, up to *and* through the SwitchInt,
        // actually stops before the if's block.)
        TerminatorKind::Unreachable // Unreachable blocks are not connected to the MIR CFG
        | TerminatorKind::Assert { .. }
        | TerminatorKind::Drop { .. }
        | TerminatorKind::SwitchInt { .. }
        // For `FalseEdge`, only the `real` branch is taken, so it is similar to a `Goto`.
        | TerminatorKind::FalseEdge { .. }
        | TerminatorKind::Goto { .. } => None,

        // Call `func` operand can have a more specific span when part of a chain of calls
        | TerminatorKind::Call { ref func, .. } => {
            let mut span = terminator.source_info.span;
            if let mir::Operand::Constant(box constant) = func {
                if constant.span.lo() > span.lo() {
                    span = span.with_lo(constant.span.lo());
                }
            }
            Some(span)
        }

        // Retain spans from all other terminators
        TerminatorKind::UnwindResume
        | TerminatorKind::UnwindTerminate(_)
        | TerminatorKind::Return
        | TerminatorKind::Yield { .. }
        | TerminatorKind::CoroutineDrop
        | TerminatorKind::FalseUnwind { .. }
        | TerminatorKind::InlineAsm { .. } => {
            Some(terminator.source_info.span)
        }
    }
}

/// Returns an extrapolated span (pre-expansion[^1]) corresponding to a range
/// within the function's body source. This span is guaranteed to be contained
/// within, or equal to, the `body_span`. If the extrapolated span is not
/// contained within the `body_span`, `None` is returned.
///
/// [^1]Expansions result from Rust syntax including macros, syntactic sugar,
/// etc.).
pub(crate) fn unexpand_into_body_span_with_visible_macro(
    original_span: Span,
    body_span: Span,
) -> Option<(Span, Option<Symbol>)> {
    let (span, prev) = unexpand_into_body_span_with_prev(original_span, body_span)?;

    let visible_macro = prev
        .map(|prev| match prev.ctxt().outer_expn_data().kind {
            ExpnKind::Macro(MacroKind::Bang, name) => Some(name),
            _ => None,
        })
        .flatten();

    Some((span, visible_macro))
}

/// Walks through the expansion ancestors of `original_span` to find a span that
/// is contained in `body_span` and has the same [`SyntaxContext`] as `body_span`.
/// The ancestor that was traversed just before the matching span (if any) is
/// also returned.
///
/// For example, a return value of `Some((ancestor, Some(prev))` means that:
/// - `ancestor == original_span.find_ancestor_inside_same_ctxt(body_span)`
/// - `ancestor == prev.parent_callsite()`
///
/// [`SyntaxContext`]: rustc_span::SyntaxContext
fn unexpand_into_body_span_with_prev(
    original_span: Span,
    body_span: Span,
) -> Option<(Span, Option<Span>)> {
    let mut prev = None;
    let mut curr = original_span;

    while !body_span.contains(curr) || !curr.eq_ctxt(body_span) {
        prev = Some(curr);
        curr = curr.parent_callsite()?;
    }

    debug_assert_eq!(Some(curr), original_span.find_ancestor_in_same_ctxt(body_span));
    if let Some(prev) = prev {
        debug_assert_eq!(Some(curr), prev.parent_callsite());
    }

    Some((curr, prev))
}

#[derive(Debug)]
pub(crate) struct Hole {
    pub(crate) span: Span,
}

impl Hole {
    pub(crate) fn merge_if_overlapping_or_adjacent(&mut self, other: &mut Self) -> bool {
        if !self.span.overlaps_or_adjacent(other.span) {
            return false;
        }

        self.span = self.span.to(other.span);
        true
    }
}

#[derive(Debug)]
pub(crate) struct SpanFromMir {
    /// A span that has been extracted from MIR and then "un-expanded" back to
    /// within the current function's `body_span`. After various intermediate
    /// processing steps, this span is emitted as part of the final coverage
    /// mappings.
    ///
    /// With the exception of `fn_sig_span`, this should always be contained
    /// within `body_span`.
    pub(crate) span: Span,
    pub(crate) visible_macro: Option<Symbol>,
    pub(crate) bcb: BasicCoverageBlock,
}

impl SpanFromMir {
    fn for_fn_sig(fn_sig_span: Span) -> Self {
        Self::new(fn_sig_span, None, START_BCB)
    }

    pub(crate) fn new(span: Span, visible_macro: Option<Symbol>, bcb: BasicCoverageBlock) -> Self {
        Self { span, visible_macro, bcb }
    }

    pub(crate) fn into_covspan(self) -> Covspan {
        let Self { span, visible_macro: _, bcb } = self;
        Covspan { span, bcb }
    }
}
