pub mod query;

mod counters;
mod graph;
mod spans;

#[cfg(test)]
mod tests;

use self::counters::{BcbCounter, CoverageCounters};
use self::graph::{BasicCoverageBlock, BasicCoverageBlockData, CoverageGraph};
use self::spans::CoverageSpans;

use crate::MirPass;

use rustc_data_structures::sync::Lrc;
use rustc_middle::hir;
use rustc_middle::middle::codegen_fn_attrs::CodegenFnAttrFlags;
use rustc_middle::mir::coverage::*;
use rustc_middle::mir::{
    self, BasicBlock, BasicBlockData, Coverage, SourceInfo, Statement, StatementKind, Terminator,
    TerminatorKind,
};
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::DefId;
use rustc_span::source_map::SourceMap;
use rustc_span::{ExpnKind, SourceFile, Span, Symbol};

/// A simple error message wrapper for `coverage::Error`s.
#[derive(Debug)]
struct Error {
    message: String,
}

impl Error {
    pub fn from_string<T>(message: String) -> Result<T, Error> {
        Err(Self { message })
    }
}

/// Inserts `StatementKind::Coverage` statements that either instrument the binary with injected
/// counters, via intrinsic `llvm.instrprof.increment`, and/or inject metadata used during codegen
/// to construct the coverage map.
pub struct InstrumentCoverage;

impl<'tcx> MirPass<'tcx> for InstrumentCoverage {
    fn is_enabled(&self, sess: &rustc_session::Session) -> bool {
        sess.instrument_coverage()
    }

    fn run_pass(&self, tcx: TyCtxt<'tcx>, mir_body: &mut mir::Body<'tcx>) {
        let mir_source = mir_body.source;

        // If the InstrumentCoverage pass is called on promoted MIRs, skip them.
        // See: https://github.com/rust-lang/rust/pull/73011#discussion_r438317601
        if mir_source.promoted.is_some() {
            trace!(
                "InstrumentCoverage skipped for {:?} (already promoted for Miri evaluation)",
                mir_source.def_id()
            );
            return;
        }

        let is_fn_like =
            tcx.hir().get_by_def_id(mir_source.def_id().expect_local()).fn_kind().is_some();

        // Only instrument functions, methods, and closures (not constants since they are evaluated
        // at compile time by Miri).
        // FIXME(#73156): Handle source code coverage in const eval, but note, if and when const
        // expressions get coverage spans, we will probably have to "carve out" space for const
        // expressions from coverage spans in enclosing MIR's, like we do for closures. (That might
        // be tricky if const expressions have no corresponding statements in the enclosing MIR.
        // Closures are carved out by their initial `Assign` statement.)
        if !is_fn_like {
            trace!("InstrumentCoverage skipped for {:?} (not an fn-like)", mir_source.def_id());
            return;
        }

        match mir_body.basic_blocks[mir::START_BLOCK].terminator().kind {
            TerminatorKind::Unreachable => {
                trace!("InstrumentCoverage skipped for unreachable `START_BLOCK`");
                return;
            }
            _ => {}
        }

        let codegen_fn_attrs = tcx.codegen_fn_attrs(mir_source.def_id());
        if codegen_fn_attrs.flags.contains(CodegenFnAttrFlags::NO_COVERAGE) {
            return;
        }

        trace!("InstrumentCoverage starting for {:?}", mir_source.def_id());
        Instrumentor::new(tcx, mir_body).inject_counters();
        trace!("InstrumentCoverage done for {:?}", mir_source.def_id());
    }
}

struct Instrumentor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    mir_body: &'a mut mir::Body<'tcx>,
    source_file: Lrc<SourceFile>,
    fn_sig_span: Span,
    body_span: Span,
    function_source_hash: u64,
    basic_coverage_blocks: CoverageGraph,
    coverage_counters: CoverageCounters,
}

impl<'a, 'tcx> Instrumentor<'a, 'tcx> {
    fn new(tcx: TyCtxt<'tcx>, mir_body: &'a mut mir::Body<'tcx>) -> Self {
        let source_map = tcx.sess.source_map();
        let def_id = mir_body.source.def_id();
        let (some_fn_sig, hir_body) = fn_sig_and_body(tcx, def_id);

        let body_span = get_body_span(tcx, hir_body, mir_body);

        let source_file = source_map.lookup_source_file(body_span.lo());
        let fn_sig_span = match some_fn_sig.filter(|fn_sig| {
            fn_sig.span.eq_ctxt(body_span)
                && Lrc::ptr_eq(&source_file, &source_map.lookup_source_file(fn_sig.span.lo()))
        }) {
            Some(fn_sig) => fn_sig.span.with_hi(body_span.lo()),
            None => body_span.shrink_to_lo(),
        };

        debug!(
            "instrumenting {}: {:?}, fn sig span: {:?}, body span: {:?}",
            if tcx.is_closure(def_id) { "closure" } else { "function" },
            def_id,
            fn_sig_span,
            body_span
        );

        let function_source_hash = hash_mir_source(tcx, hir_body);
        let basic_coverage_blocks = CoverageGraph::from_mir(mir_body);
        let coverage_counters = CoverageCounters::new(&basic_coverage_blocks);

        Self {
            tcx,
            mir_body,
            source_file,
            fn_sig_span,
            body_span,
            function_source_hash,
            basic_coverage_blocks,
            coverage_counters,
        }
    }

    fn inject_counters(&'a mut self) {
        let fn_sig_span = self.fn_sig_span;
        let body_span = self.body_span;

        ////////////////////////////////////////////////////
        // Compute coverage spans from the `CoverageGraph`.
        let coverage_spans = CoverageSpans::generate_coverage_spans(
            &self.mir_body,
            fn_sig_span,
            body_span,
            &self.basic_coverage_blocks,
        );

        ////////////////////////////////////////////////////
        // Create an optimized mix of `Counter`s and `Expression`s for the `CoverageGraph`. Ensure
        // every coverage span has a `Counter` or `Expression` assigned to its `BasicCoverageBlock`
        // and all `Expression` dependencies (operands) are also generated, for any other
        // `BasicCoverageBlock`s not already associated with a coverage span.
        //
        // Intermediate expressions (used to compute other `Expression` values), which have no
        // direct association with any `BasicCoverageBlock`, are accumulated inside `coverage_counters`.
        let bcb_has_coverage_spans = |bcb| coverage_spans.bcb_has_coverage_spans(bcb);
        let result = self
            .coverage_counters
            .make_bcb_counters(&mut self.basic_coverage_blocks, bcb_has_coverage_spans);

        if let Ok(()) = result {
            ////////////////////////////////////////////////////
            // Remove the counter or edge counter from of each coverage cpan's associated
            // `BasicCoverageBlock`, and inject a `Coverage` statement into the MIR.
            //
            // `Coverage` statements injected from coverage spans will include the code regions
            // (source code start and end positions) to be counted by the associated counter.
            //
            // These coverage-span-associated counters are removed from their associated
            // `BasicCoverageBlock`s so that the only remaining counters in the `CoverageGraph`
            // are indirect counters (to be injected next, without associated code regions).
            self.inject_coverage_span_counters(&coverage_spans);

            ////////////////////////////////////////////////////
            // For any remaining `BasicCoverageBlock` counters (that were not associated with
            // any coverage span), inject `Coverage` statements (_without_ code region spans)
            // to ensure `BasicCoverageBlock` counters that other `Expression`s may depend on
            // are in fact counted, even though they don't directly contribute to counting
            // their own independent code region's coverage.
            self.inject_indirect_counters();

            // Intermediate expressions will be injected as the final step, after generating
            // debug output, if any.
            ////////////////////////////////////////////////////
        };

        if let Err(e) = result {
            bug!("Error processing: {:?}: {:?}", self.mir_body.source.def_id(), e.message)
        };

        ////////////////////////////////////////////////////
        // Finally, inject the intermediate expressions collected along the way.
        for intermediate_expression in &self.coverage_counters.intermediate_expressions {
            inject_intermediate_expression(
                self.mir_body,
                self.make_mir_coverage_kind(intermediate_expression),
            );
        }
    }

    /// Injects a single [`StatementKind::Coverage`] for each BCB that has one
    /// or more coverage spans.
    fn inject_coverage_span_counters(&mut self, coverage_spans: &CoverageSpans) {
        let tcx = self.tcx;
        let source_map = tcx.sess.source_map();
        let body_span = self.body_span;
        let file_name = Symbol::intern(&self.source_file.name.prefer_remapped().to_string_lossy());

        for (bcb, spans) in coverage_spans.bcbs_with_coverage_spans() {
            let counter_kind = self.coverage_counters.take_bcb_counter(bcb).unwrap_or_else(|| {
                bug!("Every BasicCoverageBlock should have a Counter or Expression");
            });

            // Convert the coverage spans into a vector of code regions to be
            // associated with this BCB's coverage statement.
            let code_regions = spans
                .iter()
                .map(|&span| make_code_region(source_map, file_name, span, body_span))
                .collect::<Vec<_>>();

            inject_statement(
                self.mir_body,
                self.make_mir_coverage_kind(&counter_kind),
                self.bcb_leader_bb(bcb),
                code_regions,
            );
        }
    }

    /// At this point, any BCB with coverage counters has already had its counter injected
    /// into MIR, and had its counter removed from `coverage_counters` (via `take_counter()`).
    ///
    /// Any other counter associated with a `BasicCoverageBlock`, or its incoming edge, but not
    /// associated with a coverage span, should only exist if the counter is an `Expression`
    /// dependency (one of the expression operands). Collect them, and inject the additional
    /// counters into the MIR, without a reportable coverage span.
    fn inject_indirect_counters(&mut self) {
        let mut bcb_counters_without_direct_coverage_spans = Vec::new();
        for (target_bcb, counter_kind) in self.coverage_counters.drain_bcb_counters() {
            bcb_counters_without_direct_coverage_spans.push((None, target_bcb, counter_kind));
        }
        for ((from_bcb, target_bcb), counter_kind) in
            self.coverage_counters.drain_bcb_edge_counters()
        {
            bcb_counters_without_direct_coverage_spans.push((
                Some(from_bcb),
                target_bcb,
                counter_kind,
            ));
        }

        for (edge_from_bcb, target_bcb, counter_kind) in bcb_counters_without_direct_coverage_spans
        {
            match counter_kind {
                BcbCounter::Counter { .. } => {
                    let inject_to_bb = if let Some(from_bcb) = edge_from_bcb {
                        // The MIR edge starts `from_bb` (the outgoing / last BasicBlock in
                        // `from_bcb`) and ends at `to_bb` (the incoming / first BasicBlock in the
                        // `target_bcb`; also called the `leader_bb`).
                        let from_bb = self.bcb_last_bb(from_bcb);
                        let to_bb = self.bcb_leader_bb(target_bcb);

                        let new_bb = inject_edge_counter_basic_block(self.mir_body, from_bb, to_bb);
                        debug!(
                            "Edge {:?} (last {:?}) -> {:?} (leader {:?}) requires a new MIR \
                            BasicBlock {:?}, for unclaimed edge counter {:?}",
                            edge_from_bcb, from_bb, target_bcb, to_bb, new_bb, counter_kind,
                        );
                        new_bb
                    } else {
                        let target_bb = self.bcb_last_bb(target_bcb);
                        debug!(
                            "{:?} ({:?}) gets a new Coverage statement for unclaimed counter {:?}",
                            target_bcb, target_bb, counter_kind,
                        );
                        target_bb
                    };

                    inject_statement(
                        self.mir_body,
                        self.make_mir_coverage_kind(&counter_kind),
                        inject_to_bb,
                        Vec::new(),
                    );
                }
                BcbCounter::Expression { .. } => inject_intermediate_expression(
                    self.mir_body,
                    self.make_mir_coverage_kind(&counter_kind),
                ),
            }
        }
    }

    #[inline]
    fn bcb_leader_bb(&self, bcb: BasicCoverageBlock) -> BasicBlock {
        self.bcb_data(bcb).leader_bb()
    }

    #[inline]
    fn bcb_last_bb(&self, bcb: BasicCoverageBlock) -> BasicBlock {
        self.bcb_data(bcb).last_bb()
    }

    #[inline]
    fn bcb_data(&self, bcb: BasicCoverageBlock) -> &BasicCoverageBlockData {
        &self.basic_coverage_blocks[bcb]
    }

    fn make_mir_coverage_kind(&self, counter_kind: &BcbCounter) -> CoverageKind {
        match *counter_kind {
            BcbCounter::Counter { id } => {
                CoverageKind::Counter { function_source_hash: self.function_source_hash, id }
            }
            BcbCounter::Expression { id, lhs, op, rhs } => {
                CoverageKind::Expression { id, lhs, op, rhs }
            }
        }
    }
}

fn inject_edge_counter_basic_block(
    mir_body: &mut mir::Body<'_>,
    from_bb: BasicBlock,
    to_bb: BasicBlock,
) -> BasicBlock {
    let span = mir_body[from_bb].terminator().source_info.span.shrink_to_hi();
    let new_bb = mir_body.basic_blocks_mut().push(BasicBlockData {
        statements: vec![], // counter will be injected here
        terminator: Some(Terminator {
            source_info: SourceInfo::outermost(span),
            kind: TerminatorKind::Goto { target: to_bb },
        }),
        is_cleanup: false,
    });
    let edge_ref = mir_body[from_bb]
        .terminator_mut()
        .successors_mut()
        .find(|successor| **successor == to_bb)
        .expect("from_bb should have a successor for to_bb");
    *edge_ref = new_bb;
    new_bb
}

fn inject_statement(
    mir_body: &mut mir::Body<'_>,
    counter_kind: CoverageKind,
    bb: BasicBlock,
    code_regions: Vec<CodeRegion>,
) {
    debug!("  injecting statement {counter_kind:?} for {bb:?} at code regions: {code_regions:?}");
    let data = &mut mir_body[bb];
    let source_info = data.terminator().source_info;
    let statement = Statement {
        source_info,
        kind: StatementKind::Coverage(Box::new(Coverage { kind: counter_kind, code_regions })),
    };
    data.statements.insert(0, statement);
}

// Non-code expressions are injected into the coverage map, without generating executable code.
fn inject_intermediate_expression(mir_body: &mut mir::Body<'_>, expression: CoverageKind) {
    debug_assert!(matches!(expression, CoverageKind::Expression { .. }));
    debug!("  injecting non-code expression {:?}", expression);
    let inject_in_bb = mir::START_BLOCK;
    let data = &mut mir_body[inject_in_bb];
    let source_info = data.terminator().source_info;
    let statement = Statement {
        source_info,
        kind: StatementKind::Coverage(Box::new(Coverage {
            kind: expression,
            code_regions: Vec::new(),
        })),
    };
    data.statements.push(statement);
}

/// Convert the Span into its file name, start line and column, and end line and column
fn make_code_region(
    source_map: &SourceMap,
    file_name: Symbol,
    span: Span,
    body_span: Span,
) -> CodeRegion {
    debug!(
        "Called make_code_region(file_name={}, span={}, body_span={})",
        file_name,
        source_map.span_to_diagnostic_string(span),
        source_map.span_to_diagnostic_string(body_span)
    );

    let (file, mut start_line, mut start_col, mut end_line, mut end_col) =
        source_map.span_to_location_info(span);
    if span.hi() == span.lo() {
        // Extend an empty span by one character so the region will be counted.
        if span.hi() == body_span.hi() {
            start_col = start_col.saturating_sub(1);
        } else {
            end_col = start_col + 1;
        }
    };
    if let Some(file) = file {
        start_line = source_map.doctest_offset_line(&file.name, start_line);
        end_line = source_map.doctest_offset_line(&file.name, end_line);
    }
    CodeRegion {
        file_name,
        start_line: start_line as u32,
        start_col: start_col as u32,
        end_line: end_line as u32,
        end_col: end_col as u32,
    }
}

fn fn_sig_and_body(
    tcx: TyCtxt<'_>,
    def_id: DefId,
) -> (Option<&rustc_hir::FnSig<'_>>, &rustc_hir::Body<'_>) {
    // FIXME(#79625): Consider improving MIR to provide the information needed, to avoid going back
    // to HIR for it.
    let hir_node = tcx.hir().get_if_local(def_id).expect("expected DefId is local");
    let (_, fn_body_id) =
        hir::map::associated_body(hir_node).expect("HIR node is a function with body");
    (hir_node.fn_sig(), tcx.hir().body(fn_body_id))
}

fn get_body_span<'tcx>(
    tcx: TyCtxt<'tcx>,
    hir_body: &rustc_hir::Body<'tcx>,
    mir_body: &mut mir::Body<'tcx>,
) -> Span {
    let mut body_span = hir_body.value.span;
    let def_id = mir_body.source.def_id();

    if tcx.is_closure(def_id) {
        // If the MIR function is a closure, and if the closure body span
        // starts from a macro, but it's content is not in that macro, try
        // to find a non-macro callsite, and instrument the spans there
        // instead.
        loop {
            let expn_data = body_span.ctxt().outer_expn_data();
            if expn_data.is_root() {
                break;
            }
            if let ExpnKind::Macro { .. } = expn_data.kind {
                body_span = expn_data.call_site;
            } else {
                break;
            }
        }
    }

    body_span
}

fn hash_mir_source<'tcx>(tcx: TyCtxt<'tcx>, hir_body: &'tcx rustc_hir::Body<'tcx>) -> u64 {
    // FIXME(cjgillot) Stop hashing HIR manually here.
    let owner = hir_body.id().hir_id.owner;
    tcx.hir_owner_nodes(owner)
        .unwrap()
        .opt_hash_including_bodies
        .unwrap()
        .to_smaller_hash()
        .as_u64()
}
