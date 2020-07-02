use crate::transform::{MirPass, MirSource};
use crate::util::patch::MirPatch;
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::stable_hasher::{HashStable, StableHasher};
use rustc_hir::lang_items;
use rustc_middle::hir;
use rustc_middle::ich::StableHashingContext;
use rustc_middle::mir::coverage::*;
use rustc_middle::mir::interpret::Scalar;
use rustc_middle::mir::CoverageInfo;
use rustc_middle::mir::{
    self, traversal, BasicBlock, BasicBlockData, Operand, Place, SourceInfo, StatementKind,
    Terminator, TerminatorKind, START_BLOCK,
};
use rustc_middle::ty;
use rustc_middle::ty::query::Providers;
use rustc_middle::ty::FnDef;
use rustc_middle::ty::TyCtxt;
use rustc_span::def_id::DefId;
use rustc_span::{Pos, Span};

/// Inserts call to count_code_region() as a placeholder to be replaced during code generation with
/// the intrinsic llvm.instrprof.increment.
pub struct InstrumentCoverage;

/// The `query` provider for `CoverageInfo`, requested by `codegen_intrinsic_call()` when
/// constructing the arguments for `llvm.instrprof.increment`.
pub(crate) fn provide(providers: &mut Providers<'_>) {
    providers.coverageinfo = |tcx, def_id| coverageinfo_from_mir(tcx, def_id);
}

fn coverageinfo_from_mir<'tcx>(tcx: TyCtxt<'tcx>, mir_def_id: DefId) -> CoverageInfo {
    let mir_body = tcx.optimized_mir(mir_def_id);
    // FIXME(richkadel): The current implementation assumes the MIR for the given DefId
    // represents a single function. Validate and/or correct if inlining (which should be disabled
    // if -Zinstrument-coverage is enabled) and/or monomorphization invalidates these assumptions.
    let count_code_region_fn = tcx.require_lang_item(lang_items::CountCodeRegionFnLangItem, None);

    // The `num_counters` argument to `llvm.instrprof.increment` is the number of injected
    // counters, with each counter having an index from `0..num_counters-1`. MIR optimization
    // may split and duplicate some BasicBlock sequences. Simply counting the calls may not
    // not work; but computing the num_counters by adding `1` to the highest index (for a given
    // instrumented function) is valid.
    let mut num_counters: u32 = 0;
    for terminator in traversal::preorder(mir_body)
        .map(|(_, data)| (data, count_code_region_fn))
        .filter_map(terminators_that_call_given_fn)
    {
        if let TerminatorKind::Call { args, .. } = &terminator.kind {
            let index_arg = args.get(count_code_region_args::COUNTER_INDEX).expect("arg found");
            let index =
                mir::Operand::scalar_from_const(index_arg).to_u32().expect("index arg is u32");
            num_counters = std::cmp::max(num_counters, index + 1);
        }
    }
    let hash = if num_counters > 0 { hash_mir_source(tcx, mir_def_id) } else { 0 };
    CoverageInfo { num_counters, hash }
}

fn terminators_that_call_given_fn(
    (data, fn_def_id): (&'tcx BasicBlockData<'tcx>, DefId),
) -> Option<&'tcx Terminator<'tcx>> {
    if let Some(terminator) = &data.terminator {
        if let TerminatorKind::Call { func: Operand::Constant(func), .. } = &terminator.kind {
            if let FnDef(called_fn_def_id, _) = func.literal.ty.kind {
                if called_fn_def_id == fn_def_id {
                    return Some(&terminator);
                }
            }
        }
    }
    None
}

struct Instrumentor<'tcx> {
    tcx: TyCtxt<'tcx>,
    num_counters: u32,
}

impl<'tcx> MirPass<'tcx> for InstrumentCoverage {
    fn run_pass(&self, tcx: TyCtxt<'tcx>, src: MirSource<'tcx>, mir_body: &mut mir::Body<'tcx>) {
        if tcx.sess.opts.debugging_opts.instrument_coverage {
            // If the InstrumentCoverage pass is called on promoted MIRs, skip them.
            // See: https://github.com/rust-lang/rust/pull/73011#discussion_r438317601
            if src.promoted.is_none() {
                debug!(
                    "instrumenting {:?}, span: {}",
                    src.def_id(),
                    tcx.sess.source_map().span_to_string(mir_body.span)
                );
                Instrumentor::new(tcx).inject_counters(mir_body);
            }
        }
    }
}

impl<'tcx> Instrumentor<'tcx> {
    fn new(tcx: TyCtxt<'tcx>) -> Self {
        Self { tcx, num_counters: 0 }
    }

    fn next_counter(&mut self) -> u32 {
        let next = self.num_counters;
        self.num_counters += 1;
        next
    }

    fn inject_counters(&mut self, mir_body: &mut mir::Body<'tcx>) {
        // FIXME(richkadel): As a first step, counters are only injected at the top of each
        // function. The complete solution will inject counters at each conditional code branch.
        let code_region = mir_body.span;
        let next_block = START_BLOCK;
        self.inject_counter(mir_body, code_region, next_block);
    }

    fn inject_counter(
        &mut self,
        mir_body: &mut mir::Body<'tcx>,
        code_region: Span,
        next_block: BasicBlock,
    ) {
        let injection_point = code_region.shrink_to_lo();

        let count_code_region_fn = function_handle(
            self.tcx,
            self.tcx.require_lang_item(lang_items::CountCodeRegionFnLangItem, None),
            injection_point,
        );

        let index = self.next_counter();

        let mut args = Vec::new();

        use count_code_region_args::*;
        debug_assert_eq!(COUNTER_INDEX, args.len());
        args.push(self.const_u32(index, injection_point));

        debug_assert_eq!(START_BYTE_POS, args.len());
        args.push(self.const_u32(code_region.lo().to_u32(), injection_point));

        debug_assert_eq!(END_BYTE_POS, args.len());
        args.push(self.const_u32(code_region.hi().to_u32(), injection_point));

        let mut patch = MirPatch::new(mir_body);

        let temp = patch.new_temp(self.tcx.mk_unit(), code_region);
        let new_block = patch.new_block(placeholder_block(code_region));
        patch.patch_terminator(
            new_block,
            TerminatorKind::Call {
                func: count_code_region_fn,
                args,
                // new_block will swapped with the next_block, after applying patch
                destination: Some((Place::from(temp), new_block)),
                cleanup: None,
                from_hir_call: false,
                fn_span: injection_point,
            },
        );

        patch.add_statement(new_block.start_location(), StatementKind::StorageLive(temp));
        patch.add_statement(next_block.start_location(), StatementKind::StorageDead(temp));

        patch.apply(mir_body);

        // To insert the `new_block` in front of the first block in the counted branch (the
        // `next_block`), just swap the indexes, leaving the rest of the graph unchanged.
        mir_body.basic_blocks_mut().swap(next_block, new_block);
    }

    fn const_u32(&self, value: u32, span: Span) -> Operand<'tcx> {
        Operand::const_from_scalar(self.tcx, self.tcx.types.u32, Scalar::from_u32(value), span)
    }
}

fn function_handle<'tcx>(tcx: TyCtxt<'tcx>, fn_def_id: DefId, span: Span) -> Operand<'tcx> {
    let ret_ty = tcx.fn_sig(fn_def_id).output();
    let ret_ty = ret_ty.no_bound_vars().unwrap();
    let substs = tcx.mk_substs(::std::iter::once(ty::subst::GenericArg::from(ret_ty)));
    Operand::function_handle(tcx, fn_def_id, substs, span)
}

fn placeholder_block(span: Span) -> BasicBlockData<'tcx> {
    BasicBlockData {
        statements: vec![],
        terminator: Some(Terminator {
            source_info: SourceInfo::outermost(span),
            // this gets overwritten by the counter Call
            kind: TerminatorKind::Unreachable,
        }),
        is_cleanup: false,
    }
}

fn hash_mir_source<'tcx>(tcx: TyCtxt<'tcx>, def_id: DefId) -> u64 {
    let hir_node = tcx.hir().get_if_local(def_id).expect("DefId is local");
    let fn_body_id = hir::map::associated_body(hir_node).expect("HIR node is a function with body");
    let hir_body = tcx.hir().body(fn_body_id);
    let mut hcx = tcx.create_no_span_stable_hashing_context();
    hash(&mut hcx, &hir_body.value).to_smaller_hash()
}

fn hash(
    hcx: &mut StableHashingContext<'tcx>,
    node: &impl HashStable<StableHashingContext<'tcx>>,
) -> Fingerprint {
    let mut stable_hasher = StableHasher::new();
    node.hash_stable(hcx, &mut stable_hasher);
    stable_hasher.finish()
}
