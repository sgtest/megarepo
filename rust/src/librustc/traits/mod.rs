//! Trait Resolution. See the [rustc guide] for more information on how this works.
//!
//! [rustc guide]: https://rust-lang.github.io/rustc-guide/traits/resolution.html

#[allow(dead_code)]
pub mod auto_trait;
mod chalk_fulfill;
pub mod codegen;
mod coherence;
mod engine;
pub mod error_reporting;
mod fulfill;
pub mod misc;
mod object_safety;
mod on_unimplemented;
mod project;
pub mod query;
mod select;
mod specialize;
mod structural_impls;
mod structural_match;
mod types;
mod util;
pub mod wf;

use crate::infer::outlives::env::OutlivesEnvironment;
use crate::infer::{InferCtxt, SuppressRegionErrors};
use crate::middle::region;
use crate::ty::error::{ExpectedFound, TypeError};
use crate::ty::fold::TypeFoldable;
use crate::ty::subst::{InternalSubsts, SubstsRef};
use crate::ty::{self, GenericParamDefKind, ToPredicate, Ty, TyCtxt, WithConstness};
use crate::util::common::ErrorReported;
use rustc_hir as hir;
use rustc_hir::def_id::DefId;
use rustc_span::{Span, DUMMY_SP};

use std::fmt::Debug;

pub use self::FulfillmentErrorCode::*;

pub use self::coherence::{add_placeholder_note, orphan_check, overlapping_impls};
pub use self::coherence::{OrphanCheckErr, OverlapResult};
pub use self::engine::{TraitEngine, TraitEngineExt};
pub use self::fulfill::{FulfillmentContext, PendingPredicateObligation};
pub use self::object_safety::astconv_object_safety_violations;
pub use self::object_safety::is_vtable_safe_method;
pub use self::object_safety::object_safety_violations;
pub use self::object_safety::MethodViolationCode;
pub use self::object_safety::ObjectSafetyViolation;
pub use self::on_unimplemented::{OnUnimplementedDirective, OnUnimplementedNote};
pub use self::project::MismatchedProjectionTypes;
pub use self::project::{
    normalize, normalize_projection_type, normalize_to, poly_project_and_unify_type,
};
pub use self::project::{Normalized, ProjectionCache, ProjectionCacheSnapshot};
pub use self::select::{IntercrateAmbiguityCause, SelectionContext};
pub use self::specialize::find_associated_item;
pub use self::specialize::specialization_graph::FutureCompatOverlapError;
pub use self::specialize::specialization_graph::FutureCompatOverlapErrorKind;
pub use self::specialize::{specialization_graph, translate_substs, OverlapError};
pub use self::structural_match::search_for_structural_match_violation;
pub use self::structural_match::type_marked_structural;
pub use self::structural_match::NonStructuralMatchTy;
pub use self::util::{elaborate_predicates, elaborate_trait_ref, elaborate_trait_refs};
pub use self::util::{expand_trait_aliases, TraitAliasExpander};
pub use self::util::{
    get_vtable_index_of_object_method, impl_is_default, impl_item_is_final,
    predicate_for_trait_def, upcast_choices,
};
pub use self::util::{
    supertrait_def_ids, supertraits, transitive_bounds, SupertraitDefIds, Supertraits,
};

pub use self::chalk_fulfill::{
    CanonicalGoal as ChalkCanonicalGoal, FulfillmentContext as ChalkFulfillmentContext,
};

pub use self::types::*;

/// Whether to enable bug compatibility with issue #43355.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum IntercrateMode {
    Issue43355,
    Fixed,
}

/// Whether to skip the leak check, as part of a future compatibility warning step.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SkipLeakCheck {
    Yes,
    No,
}

impl SkipLeakCheck {
    fn is_yes(self) -> bool {
        self == SkipLeakCheck::Yes
    }
}

/// The "default" for skip-leak-check corresponds to the current
/// behavior (do not skip the leak check) -- not the behavior we are
/// transitioning into.
impl Default for SkipLeakCheck {
    fn default() -> Self {
        SkipLeakCheck::No
    }
}

/// The mode that trait queries run in.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TraitQueryMode {
    // Standard/un-canonicalized queries get accurate
    // spans etc. passed in and hence can do reasonable
    // error reporting on their own.
    Standard,
    // Canonicalized queries get dummy spans and hence
    // must generally propagate errors to
    // pre-canonicalization callsites.
    Canonical,
}

/// An `Obligation` represents some trait reference (e.g., `int: Eq`) for
/// which the vtable must be found. The process of finding a vtable is
/// called "resolving" the `Obligation`. This process consists of
/// either identifying an `impl` (e.g., `impl Eq for int`) that
/// provides the required vtable, or else finding a bound that is in
/// scope. The eventual result is usually a `Selection` (defined below).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Obligation<'tcx, T> {
    /// The reason we have to prove this thing.
    pub cause: ObligationCause<'tcx>,

    /// The environment in which we should prove this thing.
    pub param_env: ty::ParamEnv<'tcx>,

    /// The thing we are trying to prove.
    pub predicate: T,

    /// If we started proving this as a result of trying to prove
    /// something else, track the total depth to ensure termination.
    /// If this goes over a certain threshold, we abort compilation --
    /// in such cases, we can not say whether or not the predicate
    /// holds for certain. Stupid halting problem; such a drag.
    pub recursion_depth: usize,
}

pub type PredicateObligation<'tcx> = Obligation<'tcx, ty::Predicate<'tcx>>;
pub type TraitObligation<'tcx> = Obligation<'tcx, ty::PolyTraitPredicate<'tcx>>;

// `PredicateObligation` is used a lot. Make sure it doesn't unintentionally get bigger.
#[cfg(target_arch = "x86_64")]
static_assert_size!(PredicateObligation<'_>, 112);

pub type Obligations<'tcx, O> = Vec<Obligation<'tcx, O>>;
pub type PredicateObligations<'tcx> = Vec<PredicateObligation<'tcx>>;
pub type TraitObligations<'tcx> = Vec<TraitObligation<'tcx>>;

pub type Selection<'tcx> = Vtable<'tcx, PredicateObligation<'tcx>>;

pub struct FulfillmentError<'tcx> {
    pub obligation: PredicateObligation<'tcx>,
    pub code: FulfillmentErrorCode<'tcx>,
    /// Diagnostics only: we opportunistically change the `code.span` when we encounter an
    /// obligation error caused by a call argument. When this is the case, we also signal that in
    /// this field to ensure accuracy of suggestions.
    pub points_at_arg_span: bool,
}

#[derive(Clone)]
pub enum FulfillmentErrorCode<'tcx> {
    CodeSelectionError(SelectionError<'tcx>),
    CodeProjectionError(MismatchedProjectionTypes<'tcx>),
    CodeSubtypeError(ExpectedFound<Ty<'tcx>>, TypeError<'tcx>), // always comes from a SubtypePredicate
    CodeAmbiguity,
}

/// Creates predicate obligations from the generic bounds.
pub fn predicates_for_generics<'tcx>(
    cause: ObligationCause<'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    generic_bounds: &ty::InstantiatedPredicates<'tcx>,
) -> PredicateObligations<'tcx> {
    util::predicates_for_generics(cause, 0, param_env, generic_bounds)
}

/// Determines whether the type `ty` is known to meet `bound` and
/// returns true if so. Returns false if `ty` either does not meet
/// `bound` or is not known to meet bound (note that this is
/// conservative towards *no impl*, which is the opposite of the
/// `evaluate` methods).
pub fn type_known_to_meet_bound_modulo_regions<'a, 'tcx>(
    infcx: &InferCtxt<'a, 'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    ty: Ty<'tcx>,
    def_id: DefId,
    span: Span,
) -> bool {
    debug!(
        "type_known_to_meet_bound_modulo_regions(ty={:?}, bound={:?})",
        ty,
        infcx.tcx.def_path_str(def_id)
    );

    let trait_ref = ty::TraitRef { def_id, substs: infcx.tcx.mk_substs_trait(ty, &[]) };
    let obligation = Obligation {
        param_env,
        cause: ObligationCause::misc(span, hir::DUMMY_HIR_ID),
        recursion_depth: 0,
        predicate: trait_ref.without_const().to_predicate(),
    };

    let result = infcx.predicate_must_hold_modulo_regions(&obligation);
    debug!(
        "type_known_to_meet_ty={:?} bound={} => {:?}",
        ty,
        infcx.tcx.def_path_str(def_id),
        result
    );

    if result && (ty.has_infer_types() || ty.has_closure_types()) {
        // Because of inference "guessing", selection can sometimes claim
        // to succeed while the success requires a guess. To ensure
        // this function's result remains infallible, we must confirm
        // that guess. While imperfect, I believe this is sound.

        // The handling of regions in this area of the code is terrible,
        // see issue #29149. We should be able to improve on this with
        // NLL.
        let mut fulfill_cx = FulfillmentContext::new_ignoring_regions();

        // We can use a dummy node-id here because we won't pay any mind
        // to region obligations that arise (there shouldn't really be any
        // anyhow).
        let cause = ObligationCause::misc(span, hir::DUMMY_HIR_ID);

        fulfill_cx.register_bound(infcx, param_env, ty, def_id, cause);

        // Note: we only assume something is `Copy` if we can
        // *definitively* show that it implements `Copy`. Otherwise,
        // assume it is move; linear is always ok.
        match fulfill_cx.select_all_or_error(infcx) {
            Ok(()) => {
                debug!(
                    "type_known_to_meet_bound_modulo_regions: ty={:?} bound={} success",
                    ty,
                    infcx.tcx.def_path_str(def_id)
                );
                true
            }
            Err(e) => {
                debug!(
                    "type_known_to_meet_bound_modulo_regions: ty={:?} bound={} errors={:?}",
                    ty,
                    infcx.tcx.def_path_str(def_id),
                    e
                );
                false
            }
        }
    } else {
        result
    }
}

fn do_normalize_predicates<'tcx>(
    tcx: TyCtxt<'tcx>,
    region_context: DefId,
    cause: ObligationCause<'tcx>,
    elaborated_env: ty::ParamEnv<'tcx>,
    predicates: Vec<ty::Predicate<'tcx>>,
) -> Result<Vec<ty::Predicate<'tcx>>, ErrorReported> {
    debug!(
        "do_normalize_predicates(predicates={:?}, region_context={:?}, cause={:?})",
        predicates, region_context, cause,
    );
    let span = cause.span;
    tcx.infer_ctxt().enter(|infcx| {
        // FIXME. We should really... do something with these region
        // obligations. But this call just continues the older
        // behavior (i.e., doesn't cause any new bugs), and it would
        // take some further refactoring to actually solve them. In
        // particular, we would have to handle implied bounds
        // properly, and that code is currently largely confined to
        // regionck (though I made some efforts to extract it
        // out). -nmatsakis
        //
        // @arielby: In any case, these obligations are checked
        // by wfcheck anyway, so I'm not sure we have to check
        // them here too, and we will remove this function when
        // we move over to lazy normalization *anyway*.
        let fulfill_cx = FulfillmentContext::new_ignoring_regions();
        let predicates =
            match fully_normalize(&infcx, fulfill_cx, cause, elaborated_env, &predicates) {
                Ok(predicates) => predicates,
                Err(errors) => {
                    infcx.report_fulfillment_errors(&errors, None, false);
                    return Err(ErrorReported);
                }
            };

        debug!("do_normalize_predictes: normalized predicates = {:?}", predicates);

        let region_scope_tree = region::ScopeTree::default();

        // We can use the `elaborated_env` here; the region code only
        // cares about declarations like `'a: 'b`.
        let outlives_env = OutlivesEnvironment::new(elaborated_env);

        infcx.resolve_regions_and_report_errors(
            region_context,
            &region_scope_tree,
            &outlives_env,
            SuppressRegionErrors::default(),
        );

        let predicates = match infcx.fully_resolve(&predicates) {
            Ok(predicates) => predicates,
            Err(fixup_err) => {
                // If we encounter a fixup error, it means that some type
                // variable wound up unconstrained. I actually don't know
                // if this can happen, and I certainly don't expect it to
                // happen often, but if it did happen it probably
                // represents a legitimate failure due to some kind of
                // unconstrained variable, and it seems better not to ICE,
                // all things considered.
                tcx.sess.span_err(span, &fixup_err.to_string());
                return Err(ErrorReported);
            }
        };
        if predicates.has_local_value() {
            // FIXME: shouldn't we, you know, actually report an error here? or an ICE?
            Err(ErrorReported)
        } else {
            Ok(predicates)
        }
    })
}

// FIXME: this is gonna need to be removed ...
/// Normalizes the parameter environment, reporting errors if they occur.
pub fn normalize_param_env_or_error<'tcx>(
    tcx: TyCtxt<'tcx>,
    region_context: DefId,
    unnormalized_env: ty::ParamEnv<'tcx>,
    cause: ObligationCause<'tcx>,
) -> ty::ParamEnv<'tcx> {
    // I'm not wild about reporting errors here; I'd prefer to
    // have the errors get reported at a defined place (e.g.,
    // during typeck). Instead I have all parameter
    // environments, in effect, going through this function
    // and hence potentially reporting errors. This ensures of
    // course that we never forget to normalize (the
    // alternative seemed like it would involve a lot of
    // manual invocations of this fn -- and then we'd have to
    // deal with the errors at each of those sites).
    //
    // In any case, in practice, typeck constructs all the
    // parameter environments once for every fn as it goes,
    // and errors will get reported then; so after typeck we
    // can be sure that no errors should occur.

    debug!(
        "normalize_param_env_or_error(region_context={:?}, unnormalized_env={:?}, cause={:?})",
        region_context, unnormalized_env, cause
    );

    let mut predicates: Vec<_> =
        util::elaborate_predicates(tcx, unnormalized_env.caller_bounds.to_vec()).collect();

    debug!("normalize_param_env_or_error: elaborated-predicates={:?}", predicates);

    let elaborated_env = ty::ParamEnv::new(
        tcx.intern_predicates(&predicates),
        unnormalized_env.reveal,
        unnormalized_env.def_id,
    );

    // HACK: we are trying to normalize the param-env inside *itself*. The problem is that
    // normalization expects its param-env to be already normalized, which means we have
    // a circularity.
    //
    // The way we handle this is by normalizing the param-env inside an unnormalized version
    // of the param-env, which means that if the param-env contains unnormalized projections,
    // we'll have some normalization failures. This is unfortunate.
    //
    // Lazy normalization would basically handle this by treating just the
    // normalizing-a-trait-ref-requires-itself cycles as evaluation failures.
    //
    // Inferred outlives bounds can create a lot of `TypeOutlives` predicates for associated
    // types, so to make the situation less bad, we normalize all the predicates *but*
    // the `TypeOutlives` predicates first inside the unnormalized parameter environment, and
    // then we normalize the `TypeOutlives` bounds inside the normalized parameter environment.
    //
    // This works fairly well because trait matching  does not actually care about param-env
    // TypeOutlives predicates - these are normally used by regionck.
    let outlives_predicates: Vec<_> = predicates
        .drain_filter(|predicate| match predicate {
            ty::Predicate::TypeOutlives(..) => true,
            _ => false,
        })
        .collect();

    debug!(
        "normalize_param_env_or_error: predicates=(non-outlives={:?}, outlives={:?})",
        predicates, outlives_predicates
    );
    let non_outlives_predicates = match do_normalize_predicates(
        tcx,
        region_context,
        cause.clone(),
        elaborated_env,
        predicates,
    ) {
        Ok(predicates) => predicates,
        // An unnormalized env is better than nothing.
        Err(ErrorReported) => {
            debug!("normalize_param_env_or_error: errored resolving non-outlives predicates");
            return elaborated_env;
        }
    };

    debug!("normalize_param_env_or_error: non-outlives predicates={:?}", non_outlives_predicates);

    // Not sure whether it is better to include the unnormalized TypeOutlives predicates
    // here. I believe they should not matter, because we are ignoring TypeOutlives param-env
    // predicates here anyway. Keeping them here anyway because it seems safer.
    let outlives_env: Vec<_> =
        non_outlives_predicates.iter().chain(&outlives_predicates).cloned().collect();
    let outlives_env =
        ty::ParamEnv::new(tcx.intern_predicates(&outlives_env), unnormalized_env.reveal, None);
    let outlives_predicates = match do_normalize_predicates(
        tcx,
        region_context,
        cause,
        outlives_env,
        outlives_predicates,
    ) {
        Ok(predicates) => predicates,
        // An unnormalized env is better than nothing.
        Err(ErrorReported) => {
            debug!("normalize_param_env_or_error: errored resolving outlives predicates");
            return elaborated_env;
        }
    };
    debug!("normalize_param_env_or_error: outlives predicates={:?}", outlives_predicates);

    let mut predicates = non_outlives_predicates;
    predicates.extend(outlives_predicates);
    debug!("normalize_param_env_or_error: final predicates={:?}", predicates);
    ty::ParamEnv::new(
        tcx.intern_predicates(&predicates),
        unnormalized_env.reveal,
        unnormalized_env.def_id,
    )
}

pub fn fully_normalize<'a, 'tcx, T>(
    infcx: &InferCtxt<'a, 'tcx>,
    mut fulfill_cx: FulfillmentContext<'tcx>,
    cause: ObligationCause<'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    value: &T,
) -> Result<T, Vec<FulfillmentError<'tcx>>>
where
    T: TypeFoldable<'tcx>,
{
    debug!("fully_normalize_with_fulfillcx(value={:?})", value);
    let selcx = &mut SelectionContext::new(infcx);
    let Normalized { value: normalized_value, obligations } =
        project::normalize(selcx, param_env, cause, value);
    debug!(
        "fully_normalize: normalized_value={:?} obligations={:?}",
        normalized_value, obligations
    );
    for obligation in obligations {
        fulfill_cx.register_predicate_obligation(selcx.infcx(), obligation);
    }

    debug!("fully_normalize: select_all_or_error start");
    fulfill_cx.select_all_or_error(infcx)?;
    debug!("fully_normalize: select_all_or_error complete");
    let resolved_value = infcx.resolve_vars_if_possible(&normalized_value);
    debug!("fully_normalize: resolved_value={:?}", resolved_value);
    Ok(resolved_value)
}

/// Normalizes the predicates and checks whether they hold in an empty
/// environment. If this returns false, then either normalize
/// encountered an error or one of the predicates did not hold. Used
/// when creating vtables to check for unsatisfiable methods.
pub fn normalize_and_test_predicates<'tcx>(
    tcx: TyCtxt<'tcx>,
    predicates: Vec<ty::Predicate<'tcx>>,
) -> bool {
    debug!("normalize_and_test_predicates(predicates={:?})", predicates);

    let result = tcx.infer_ctxt().enter(|infcx| {
        let param_env = ty::ParamEnv::reveal_all();
        let mut selcx = SelectionContext::new(&infcx);
        let mut fulfill_cx = FulfillmentContext::new();
        let cause = ObligationCause::dummy();
        let Normalized { value: predicates, obligations } =
            normalize(&mut selcx, param_env, cause.clone(), &predicates);
        for obligation in obligations {
            fulfill_cx.register_predicate_obligation(&infcx, obligation);
        }
        for predicate in predicates {
            let obligation = Obligation::new(cause.clone(), param_env, predicate);
            fulfill_cx.register_predicate_obligation(&infcx, obligation);
        }

        fulfill_cx.select_all_or_error(&infcx).is_ok()
    });
    debug!("normalize_and_test_predicates(predicates={:?}) = {:?}", predicates, result);
    result
}

fn substitute_normalize_and_test_predicates<'tcx>(
    tcx: TyCtxt<'tcx>,
    key: (DefId, SubstsRef<'tcx>),
) -> bool {
    debug!("substitute_normalize_and_test_predicates(key={:?})", key);

    let predicates = tcx.predicates_of(key.0).instantiate(tcx, key.1).predicates;
    let result = normalize_and_test_predicates(tcx, predicates);

    debug!("substitute_normalize_and_test_predicates(key={:?}) = {:?}", key, result);
    result
}

/// Given a trait `trait_ref`, iterates the vtable entries
/// that come from `trait_ref`, including its supertraits.
#[inline] // FIXME(#35870): avoid closures being unexported due to `impl Trait`.
fn vtable_methods<'tcx>(
    tcx: TyCtxt<'tcx>,
    trait_ref: ty::PolyTraitRef<'tcx>,
) -> &'tcx [Option<(DefId, SubstsRef<'tcx>)>] {
    debug!("vtable_methods({:?})", trait_ref);

    tcx.arena.alloc_from_iter(supertraits(tcx, trait_ref).flat_map(move |trait_ref| {
        let trait_methods = tcx
            .associated_items(trait_ref.def_id())
            .filter(|item| item.kind == ty::AssocKind::Method);

        // Now list each method's DefId and InternalSubsts (for within its trait).
        // If the method can never be called from this object, produce None.
        trait_methods.map(move |trait_method| {
            debug!("vtable_methods: trait_method={:?}", trait_method);
            let def_id = trait_method.def_id;

            // Some methods cannot be called on an object; skip those.
            if !is_vtable_safe_method(tcx, trait_ref.def_id(), &trait_method) {
                debug!("vtable_methods: not vtable safe");
                return None;
            }

            // The method may have some early-bound lifetimes; add regions for those.
            let substs = trait_ref.map_bound(|trait_ref| {
                InternalSubsts::for_item(tcx, def_id, |param, _| match param.kind {
                    GenericParamDefKind::Lifetime => tcx.lifetimes.re_erased.into(),
                    GenericParamDefKind::Type { .. } | GenericParamDefKind::Const => {
                        trait_ref.substs[param.index as usize]
                    }
                })
            });

            // The trait type may have higher-ranked lifetimes in it;
            // erase them if they appear, so that we get the type
            // at some particular call site.
            let substs =
                tcx.normalize_erasing_late_bound_regions(ty::ParamEnv::reveal_all(), &substs);

            // It's possible that the method relies on where-clauses that
            // do not hold for this particular set of type parameters.
            // Note that this method could then never be called, so we
            // do not want to try and codegen it, in that case (see #23435).
            let predicates = tcx.predicates_of(def_id).instantiate_own(tcx, substs);
            if !normalize_and_test_predicates(tcx, predicates.predicates) {
                debug!("vtable_methods: predicates do not hold");
                return None;
            }

            Some((def_id, substs))
        })
    }))
}

impl<'tcx, O> Obligation<'tcx, O> {
    pub fn new(
        cause: ObligationCause<'tcx>,
        param_env: ty::ParamEnv<'tcx>,
        predicate: O,
    ) -> Obligation<'tcx, O> {
        Obligation { cause, param_env, recursion_depth: 0, predicate }
    }

    fn with_depth(
        cause: ObligationCause<'tcx>,
        recursion_depth: usize,
        param_env: ty::ParamEnv<'tcx>,
        predicate: O,
    ) -> Obligation<'tcx, O> {
        Obligation { cause, param_env, recursion_depth, predicate }
    }

    pub fn misc(
        span: Span,
        body_id: hir::HirId,
        param_env: ty::ParamEnv<'tcx>,
        trait_ref: O,
    ) -> Obligation<'tcx, O> {
        Obligation::new(ObligationCause::misc(span, body_id), param_env, trait_ref)
    }

    pub fn with<P>(&self, value: P) -> Obligation<'tcx, P> {
        Obligation {
            cause: self.cause.clone(),
            param_env: self.param_env,
            recursion_depth: self.recursion_depth,
            predicate: value,
        }
    }
}

impl<'tcx> FulfillmentError<'tcx> {
    fn new(
        obligation: PredicateObligation<'tcx>,
        code: FulfillmentErrorCode<'tcx>,
    ) -> FulfillmentError<'tcx> {
        FulfillmentError { obligation: obligation, code: code, points_at_arg_span: false }
    }
}

impl<'tcx> TraitObligation<'tcx> {
    fn self_ty(&self) -> ty::Binder<Ty<'tcx>> {
        self.predicate.map_bound(|p| p.self_ty())
    }
}

pub fn provide(providers: &mut ty::query::Providers<'_>) {
    misc::provide(providers);
    *providers = ty::query::Providers {
        is_object_safe: object_safety::is_object_safe_provider,
        specialization_graph_of: specialize::specialization_graph_provider,
        specializes: specialize::specializes,
        codegen_fulfill_obligation: codegen::codegen_fulfill_obligation,
        vtable_methods,
        substitute_normalize_and_test_predicates,
        ..*providers
    };
}
