// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Code for projecting associated types out of trait references.

use super::elaborate_predicates;
use super::specialization_graph;
use super::translate_substs;
use super::Obligation;
use super::ObligationCause;
use super::PredicateObligation;
use super::SelectionContext;
use super::SelectionError;
use super::VtableClosureData;
use super::VtableFnPointerData;
use super::VtableImplData;
use super::util;

use hir::def_id::DefId;
use infer::{self, InferOk, TypeOrigin};
use ty::subst::Subst;
use ty::{self, ToPredicate, ToPolyTraitRef, Ty, TyCtxt};
use ty::fold::{TypeFoldable, TypeFolder};
use syntax::parse::token;
use syntax::ast;
use util::common::FN_OUTPUT_NAME;

use std::rc::Rc;

/// Depending on the stage of compilation, we want projection to be
/// more or less conservative.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ProjectionMode {
    /// FIXME (#32205)
    /// At coherence-checking time, we're still constructing the
    /// specialization graph, and thus we only project
    /// non-`default` associated types that are defined directly in
    /// the applicable impl. (This behavior should be improved over
    /// time, to allow for successful projections modulo cycles
    /// between different impls).
    ///
    /// Here's an example that will fail due to the restriction:
    ///
    /// ```
    /// trait Assoc {
    ///     type Output;
    /// }
    ///
    /// impl<T> Assoc for T {
    ///     type Output = bool;
    /// }
    ///
    /// impl Assoc for u8 {} // <- inherits the non-default type from above
    ///
    /// trait Foo {}
    /// impl Foo for u32 {}
    /// impl Foo for <u8 as Assoc>::Output {}  // <- this projection will fail
    /// ```
    ///
    /// The projection would succeed if `Output` had been defined
    /// directly in the impl for `u8`.
    Topmost,

    /// At type-checking time, we refuse to project any associated
    /// type that is marked `default`. Non-`default` ("final") types
    /// are always projected. This is necessary in general for
    /// soundness of specialization. However, we *could* allow
    /// projections in fully-monomorphic cases. We choose not to,
    /// because we prefer for `default type` to force the type
    /// definition to be treated abstractly by any consumers of the
    /// impl. Concretely, that means that the following example will
    /// fail to compile:
    ///
    /// ```
    /// trait Assoc {
    ///     type Output;
    /// }
    ///
    /// impl<T> Assoc for T {
    ///     default type Output = bool;
    /// }
    ///
    /// fn main() {
    ///     let <() as Assoc>::Output = true;
    /// }
    AnyFinal,

    /// At trans time, all projections will succeed.
    Any,
}

impl ProjectionMode {
    pub fn is_topmost(&self) -> bool {
        match *self {
            ProjectionMode::Topmost => true,
            _ => false,
        }
    }

    pub fn is_any_final(&self) -> bool {
        match *self {
            ProjectionMode::AnyFinal => true,
            _ => false,
        }
    }

    pub fn is_any(&self) -> bool {
        match *self {
            ProjectionMode::Any => true,
            _ => false,
        }
    }
}


pub type PolyProjectionObligation<'tcx> =
    Obligation<'tcx, ty::PolyProjectionPredicate<'tcx>>;

pub type ProjectionObligation<'tcx> =
    Obligation<'tcx, ty::ProjectionPredicate<'tcx>>;

pub type ProjectionTyObligation<'tcx> =
    Obligation<'tcx, ty::ProjectionTy<'tcx>>;

/// When attempting to resolve `<T as TraitRef>::Name` ...
#[derive(Debug)]
pub enum ProjectionTyError<'tcx> {
    /// ...we found multiple sources of information and couldn't resolve the ambiguity.
    TooManyCandidates,

    /// ...an error occurred matching `T : TraitRef`
    TraitSelectionError(SelectionError<'tcx>),
}

#[derive(Clone)]
pub struct MismatchedProjectionTypes<'tcx> {
    pub err: ty::error::TypeError<'tcx>
}

#[derive(PartialEq, Eq, Debug)]
enum ProjectionTyCandidate<'tcx> {
    // from a where-clause in the env or object type
    ParamEnv(ty::PolyProjectionPredicate<'tcx>),

    // from the definition of `Trait` when you have something like <<A as Trait>::B as Trait2>::C
    TraitDef(ty::PolyProjectionPredicate<'tcx>),

    // defined in an impl
    Impl(VtableImplData<'tcx, PredicateObligation<'tcx>>),

    // closure return type
    Closure(VtableClosureData<'tcx, PredicateObligation<'tcx>>),

    // fn pointer return type
    FnPointer(VtableFnPointerData<'tcx, PredicateObligation<'tcx>>),
}

struct ProjectionTyCandidateSet<'tcx> {
    vec: Vec<ProjectionTyCandidate<'tcx>>,
    ambiguous: bool
}

/// Evaluates constraints of the form:
///
///     for<...> <T as Trait>::U == V
///
/// If successful, this may result in additional obligations.
pub fn poly_project_and_unify_type<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &PolyProjectionObligation<'tcx>)
    -> Result<Option<Vec<PredicateObligation<'tcx>>>, MismatchedProjectionTypes<'tcx>>
{
    debug!("poly_project_and_unify_type(obligation={:?})",
           obligation);

    let infcx = selcx.infcx();
    infcx.commit_if_ok(|snapshot| {
        let (skol_predicate, skol_map) =
            infcx.skolemize_late_bound_regions(&obligation.predicate, snapshot);

        let skol_obligation = obligation.with(skol_predicate);
        match project_and_unify_type(selcx, &skol_obligation) {
            Ok(result) => {
                match infcx.leak_check(false, &skol_map, snapshot) {
                    Ok(()) => Ok(infcx.plug_leaks(skol_map, snapshot, &result)),
                    Err(e) => Err(MismatchedProjectionTypes { err: e }),
                }
            }
            Err(e) => {
                Err(e)
            }
        }
    })
}

/// Evaluates constraints of the form:
///
///     <T as Trait>::U == V
///
/// If successful, this may result in additional obligations.
fn project_and_unify_type<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionObligation<'tcx>)
    -> Result<Option<Vec<PredicateObligation<'tcx>>>, MismatchedProjectionTypes<'tcx>>
{
    debug!("project_and_unify_type(obligation={:?})",
           obligation);

    let Normalized { value: normalized_ty, obligations } =
        match opt_normalize_projection_type(selcx,
                                            obligation.predicate.projection_ty.clone(),
                                            obligation.cause.clone(),
                                            obligation.recursion_depth) {
            Some(n) => n,
            None => return Ok(None),
        };

    debug!("project_and_unify_type: normalized_ty={:?} obligations={:?}",
           normalized_ty,
           obligations);

    let infcx = selcx.infcx();
    let origin = TypeOrigin::RelateOutputImplTypes(obligation.cause.span);
    match infcx.eq_types(true, origin, normalized_ty, obligation.predicate.ty) {
        Ok(InferOk { obligations: inferred_obligations, .. }) => {
            // FIXME(#32730) propagate obligations
            assert!(inferred_obligations.is_empty());
            Ok(Some(obligations))
        },
        Err(err) => Err(MismatchedProjectionTypes { err: err }),
    }
}

/// Normalizes any associated type projections in `value`, replacing
/// them with a fully resolved type where possible. The return value
/// combines the normalized result and any additional obligations that
/// were incurred as result.
pub fn normalize<'a, 'b, 'gcx, 'tcx, T>(selcx: &'a mut SelectionContext<'b, 'gcx, 'tcx>,
                                        cause: ObligationCause<'tcx>,
                                        value: &T)
                                        -> Normalized<'tcx, T>
    where T : TypeFoldable<'tcx>
{
    normalize_with_depth(selcx, cause, 0, value)
}

/// As `normalize`, but with a custom depth.
pub fn normalize_with_depth<'a, 'b, 'gcx, 'tcx, T>(
    selcx: &'a mut SelectionContext<'b, 'gcx, 'tcx>,
    cause: ObligationCause<'tcx>,
    depth: usize,
    value: &T)
    -> Normalized<'tcx, T>

    where T : TypeFoldable<'tcx>
{
    let mut normalizer = AssociatedTypeNormalizer::new(selcx, cause, depth);
    let result = normalizer.fold(value);

    Normalized {
        value: result,
        obligations: normalizer.obligations,
    }
}

struct AssociatedTypeNormalizer<'a, 'b: 'a, 'gcx: 'b+'tcx, 'tcx: 'b> {
    selcx: &'a mut SelectionContext<'b, 'gcx, 'tcx>,
    cause: ObligationCause<'tcx>,
    obligations: Vec<PredicateObligation<'tcx>>,
    depth: usize,
}

impl<'a, 'b, 'gcx, 'tcx> AssociatedTypeNormalizer<'a, 'b, 'gcx, 'tcx> {
    fn new(selcx: &'a mut SelectionContext<'b, 'gcx, 'tcx>,
           cause: ObligationCause<'tcx>,
           depth: usize)
           -> AssociatedTypeNormalizer<'a, 'b, 'gcx, 'tcx>
    {
        AssociatedTypeNormalizer {
            selcx: selcx,
            cause: cause,
            obligations: vec!(),
            depth: depth,
        }
    }

    fn fold<T:TypeFoldable<'tcx>>(&mut self, value: &T) -> T {
        let value = self.selcx.infcx().resolve_type_vars_if_possible(value);

        if !value.has_projection_types() {
            value.clone()
        } else {
            value.fold_with(self)
        }
    }
}

impl<'a, 'b, 'gcx, 'tcx> TypeFolder<'gcx, 'tcx> for AssociatedTypeNormalizer<'a, 'b, 'gcx, 'tcx> {
    fn tcx<'c>(&'c self) -> TyCtxt<'c, 'gcx, 'tcx> {
        self.selcx.tcx()
    }

    fn fold_ty(&mut self, ty: Ty<'tcx>) -> Ty<'tcx> {
        // We don't want to normalize associated types that occur inside of region
        // binders, because they may contain bound regions, and we can't cope with that.
        //
        // Example:
        //
        //     for<'a> fn(<T as Foo<&'a>>::A)
        //
        // Instead of normalizing `<T as Foo<&'a>>::A` here, we'll
        // normalize it when we instantiate those bound regions (which
        // should occur eventually).

        let ty = ty.super_fold_with(self);
        match ty.sty {
            ty::TyProjection(ref data) if !data.has_escaping_regions() => { // (*)

                // (*) This is kind of hacky -- we need to be able to
                // handle normalization within binders because
                // otherwise we wind up a need to normalize when doing
                // trait matching (since you can have a trait
                // obligation like `for<'a> T::B : Fn(&'a int)`), but
                // we can't normalize with bound regions in scope. So
                // far now we just ignore binders but only normalize
                // if all bound regions are gone (and then we still
                // have to renormalize whenever we instantiate a
                // binder). It would be better to normalize in a
                // binding-aware fashion.

                let Normalized { value: ty, obligations } =
                    normalize_projection_type(self.selcx,
                                              data.clone(),
                                              self.cause.clone(),
                                              self.depth);
                self.obligations.extend(obligations);
                ty
            }

            _ => {
                ty
            }
        }
    }
}

#[derive(Clone)]
pub struct Normalized<'tcx,T> {
    pub value: T,
    pub obligations: Vec<PredicateObligation<'tcx>>,
}

pub type NormalizedTy<'tcx> = Normalized<'tcx, Ty<'tcx>>;

impl<'tcx,T> Normalized<'tcx,T> {
    pub fn with<U>(self, value: U) -> Normalized<'tcx,U> {
        Normalized { value: value, obligations: self.obligations }
    }
}

/// The guts of `normalize`: normalize a specific projection like `<T
/// as Trait>::Item`. The result is always a type (and possibly
/// additional obligations). If ambiguity arises, which implies that
/// there are unresolved type variables in the projection, we will
/// substitute a fresh type variable `$X` and generate a new
/// obligation `<T as Trait>::Item == $X` for later.
pub fn normalize_projection_type<'a, 'b, 'gcx, 'tcx>(
    selcx: &'a mut SelectionContext<'b, 'gcx, 'tcx>,
    projection_ty: ty::ProjectionTy<'tcx>,
    cause: ObligationCause<'tcx>,
    depth: usize)
    -> NormalizedTy<'tcx>
{
    opt_normalize_projection_type(selcx, projection_ty.clone(), cause.clone(), depth)
        .unwrap_or_else(move || {
            // if we bottom out in ambiguity, create a type variable
            // and a deferred predicate to resolve this when more type
            // information is available.

            let ty_var = selcx.infcx().next_ty_var();
            let projection = ty::Binder(ty::ProjectionPredicate {
                projection_ty: projection_ty,
                ty: ty_var
            });
            let obligation = Obligation::with_depth(
                cause, depth + 1, projection.to_predicate());
            Normalized {
                value: ty_var,
                obligations: vec!(obligation)
            }
        })
}

/// The guts of `normalize`: normalize a specific projection like `<T
/// as Trait>::Item`. The result is always a type (and possibly
/// additional obligations). Returns `None` in the case of ambiguity,
/// which indicates that there are unbound type variables.
fn opt_normalize_projection_type<'a, 'b, 'gcx, 'tcx>(
    selcx: &'a mut SelectionContext<'b, 'gcx, 'tcx>,
    projection_ty: ty::ProjectionTy<'tcx>,
    cause: ObligationCause<'tcx>,
    depth: usize)
    -> Option<NormalizedTy<'tcx>>
{
    debug!("normalize_projection_type(\
           projection_ty={:?}, \
           depth={})",
           projection_ty,
           depth);

    let obligation = Obligation::with_depth(cause.clone(), depth, projection_ty.clone());
    match project_type(selcx, &obligation) {
        Ok(ProjectedTy::Progress(projected_ty, mut obligations)) => {
            // if projection succeeded, then what we get out of this
            // is also non-normalized (consider: it was derived from
            // an impl, where-clause etc) and hence we must
            // re-normalize it

            debug!("normalize_projection_type: projected_ty={:?} depth={} obligations={:?}",
                   projected_ty,
                   depth,
                   obligations);

            if projected_ty.has_projection_types() {
                let mut normalizer = AssociatedTypeNormalizer::new(selcx, cause, depth+1);
                let normalized_ty = normalizer.fold(&projected_ty);

                debug!("normalize_projection_type: normalized_ty={:?} depth={}",
                       normalized_ty,
                       depth);

                obligations.extend(normalizer.obligations);
                Some(Normalized {
                    value: normalized_ty,
                    obligations: obligations,
                })
            } else {
                Some(Normalized {
                    value: projected_ty,
                    obligations: obligations,
                })
            }
        }
        Ok(ProjectedTy::NoProgress(projected_ty)) => {
            debug!("normalize_projection_type: projected_ty={:?} no progress",
                   projected_ty);
            Some(Normalized {
                value: projected_ty,
                obligations: vec!()
            })
        }
        Err(ProjectionTyError::TooManyCandidates) => {
            debug!("normalize_projection_type: too many candidates");
            None
        }
        Err(ProjectionTyError::TraitSelectionError(_)) => {
            debug!("normalize_projection_type: ERROR");
            // if we got an error processing the `T as Trait` part,
            // just return `ty::err` but add the obligation `T :
            // Trait`, which when processed will cause the error to be
            // reported later

            Some(normalize_to_error(selcx, projection_ty, cause, depth))
        }
    }
}

/// If we are projecting `<T as Trait>::Item`, but `T: Trait` does not
/// hold. In various error cases, we cannot generate a valid
/// normalized projection. Therefore, we create an inference variable
/// return an associated obligation that, when fulfilled, will lead to
/// an error.
///
/// Note that we used to return `TyError` here, but that was quite
/// dubious -- the premise was that an error would *eventually* be
/// reported, when the obligation was processed. But in general once
/// you see a `TyError` you are supposed to be able to assume that an
/// error *has been* reported, so that you can take whatever heuristic
/// paths you want to take. To make things worse, it was possible for
/// cycles to arise, where you basically had a setup like `<MyType<$0>
/// as Trait>::Foo == $0`. Here, normalizing `<MyType<$0> as
/// Trait>::Foo> to `[type error]` would lead to an obligation of
/// `<MyType<[type error]> as Trait>::Foo`.  We are supposed to report
/// an error for this obligation, but we legitimately should not,
/// because it contains `[type error]`. Yuck! (See issue #29857 for
/// one case where this arose.)
fn normalize_to_error<'a, 'gcx, 'tcx>(selcx: &mut SelectionContext<'a, 'gcx, 'tcx>,
                                      projection_ty: ty::ProjectionTy<'tcx>,
                                      cause: ObligationCause<'tcx>,
                                      depth: usize)
                                      -> NormalizedTy<'tcx>
{
    let trait_ref = projection_ty.trait_ref.to_poly_trait_ref();
    let trait_obligation = Obligation { cause: cause,
                                        recursion_depth: depth,
                                        predicate: trait_ref.to_predicate() };
    let new_value = selcx.infcx().next_ty_var();
    Normalized {
        value: new_value,
        obligations: vec!(trait_obligation)
    }
}

enum ProjectedTy<'tcx> {
    Progress(Ty<'tcx>, Vec<PredicateObligation<'tcx>>),
    NoProgress(Ty<'tcx>),
}

/// Compute the result of a projection type (if we can).
fn project_type<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>)
    -> Result<ProjectedTy<'tcx>, ProjectionTyError<'tcx>>
{
    debug!("project(obligation={:?})",
           obligation);

    let recursion_limit = selcx.tcx().sess.recursion_limit.get();
    if obligation.recursion_depth >= recursion_limit {
        debug!("project: overflow!");
        selcx.infcx().report_overflow_error(&obligation, true);
    }

    let obligation_trait_ref =
        selcx.infcx().resolve_type_vars_if_possible(&obligation.predicate.trait_ref);

    debug!("project: obligation_trait_ref={:?}", obligation_trait_ref);

    if obligation_trait_ref.references_error() {
        return Ok(ProjectedTy::Progress(selcx.tcx().types.err, vec!()));
    }

    let mut candidates = ProjectionTyCandidateSet {
        vec: Vec::new(),
        ambiguous: false,
    };

    assemble_candidates_from_param_env(selcx,
                                       obligation,
                                       &obligation_trait_ref,
                                       &mut candidates);

    assemble_candidates_from_trait_def(selcx,
                                       obligation,
                                       &obligation_trait_ref,
                                       &mut candidates);

    if let Err(e) = assemble_candidates_from_impls(selcx,
                                                   obligation,
                                                   &obligation_trait_ref,
                                                   &mut candidates) {
        return Err(ProjectionTyError::TraitSelectionError(e));
    }

    debug!("{} candidates, ambiguous={}",
           candidates.vec.len(),
           candidates.ambiguous);

    // Inherent ambiguity that prevents us from even enumerating the
    // candidates.
    if candidates.ambiguous {
        return Err(ProjectionTyError::TooManyCandidates);
    }

    // Drop duplicates.
    //
    // Note: `candidates.vec` seems to be on the critical path of the
    // compiler. Replacing it with an hash set was also tried, which would
    // render the following dedup unnecessary. It led to cleaner code but
    // prolonged compiling time of `librustc` from 5m30s to 6m in one test, or
    // ~9% performance lost.
    if candidates.vec.len() > 1 {
        let mut i = 0;
        while i < candidates.vec.len() {
            let has_dup = (0..i).any(|j| candidates.vec[i] == candidates.vec[j]);
            if has_dup {
                candidates.vec.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    // Prefer where-clauses. As in select, if there are multiple
    // candidates, we prefer where-clause candidates over impls.  This
    // may seem a bit surprising, since impls are the source of
    // "truth" in some sense, but in fact some of the impls that SEEM
    // applicable are not, because of nested obligations. Where
    // clauses are the safer choice. See the comment on
    // `select::SelectionCandidate` and #21974 for more details.
    if candidates.vec.len() > 1 {
        debug!("retaining param-env candidates only from {:?}", candidates.vec);
        candidates.vec.retain(|c| match *c {
            ProjectionTyCandidate::ParamEnv(..) => true,
            ProjectionTyCandidate::Impl(..) |
            ProjectionTyCandidate::Closure(..) |
            ProjectionTyCandidate::TraitDef(..) |
            ProjectionTyCandidate::FnPointer(..) => false,
        });
        debug!("resulting candidate set: {:?}", candidates.vec);
        if candidates.vec.len() != 1 {
            return Err(ProjectionTyError::TooManyCandidates);
        }
    }

    assert!(candidates.vec.len() <= 1);

    let possible_candidate = candidates.vec.pop().and_then(|candidate| {
        // In Any (i.e. trans) mode, all projections succeed;
        // otherwise, we need to be sensitive to `default` and
        // specialization.
        if !selcx.projection_mode().is_any() {
            if let ProjectionTyCandidate::Impl(ref impl_data) = candidate {
                if let Some(node_item) = assoc_ty_def(selcx,
                                                      impl_data.impl_def_id,
                                                      obligation.predicate.item_name) {
                    if node_item.node.is_from_trait() {
                        if node_item.item.ty.is_some() {
                            // If the associated type has a default from the
                            // trait, that should be considered `default` and
                            // hence not projected.
                            //
                            // Note, however, that we allow a projection from
                            // the trait specifically in the case that the trait
                            // does *not* give a default. This is purely to
                            // avoid spurious errors: the situation can only
                            // arise when *no* impl in the specialization chain
                            // has provided a definition for the type. When we
                            // confirm the candidate, we'll turn the projection
                            // into a TyError, since the actual error will be
                            // reported in `check_impl_items_against_trait`.
                            return None;
                        }
                    } else if node_item.item.defaultness.is_default() {
                        return None;
                    }
                } else {
                    // Normally this situation could only arise througha
                    // compiler bug, but at coherence-checking time we only look
                    // at the topmost impl (we don't even consider the trait
                    // itself) for the definition -- so we can fail to find a
                    // definition of the type even if it exists.

                    // For now, we just unconditionally ICE, because otherwise,
                    // examples like the following will succeed:
                    //
                    // ```
                    // trait Assoc {
                    //     type Output;
                    // }
                    //
                    // impl<T> Assoc for T {
                    //     default type Output = bool;
                    // }
                    //
                    // impl Assoc for u8 {}
                    // impl Assoc for u16 {}
                    //
                    // trait Foo {}
                    // impl Foo for <u8 as Assoc>::Output {}
                    // impl Foo for <u16 as Assoc>::Output {}
                    //     return None;
                    // }
                    // ```
                    //
                    // The essential problem here is that the projection fails,
                    // leaving two unnormalized types, which appear not to unify
                    // -- so the overlap check succeeds, when it should fail.
                    bug!("Tried to project an inherited associated type during \
                          coherence checking, which is currently not supported.");
                }
            }
        }
        Some(candidate)
    });

    match possible_candidate {
        Some(candidate) => {
            let (ty, obligations) = confirm_candidate(selcx, obligation, candidate);
            Ok(ProjectedTy::Progress(ty, obligations))
        }
        None => {
            Ok(ProjectedTy::NoProgress(selcx.tcx().mk_projection(
                obligation.predicate.trait_ref.clone(),
                obligation.predicate.item_name)))
        }
    }
}

/// The first thing we have to do is scan through the parameter
/// environment to see whether there are any projection predicates
/// there that can answer this question.
fn assemble_candidates_from_param_env<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>,
    obligation_trait_ref: &ty::TraitRef<'tcx>,
    candidate_set: &mut ProjectionTyCandidateSet<'tcx>)
{
    debug!("assemble_candidates_from_param_env(..)");
    let env_predicates = selcx.param_env().caller_bounds.iter().cloned();
    assemble_candidates_from_predicates(selcx,
                                        obligation,
                                        obligation_trait_ref,
                                        candidate_set,
                                        ProjectionTyCandidate::ParamEnv,
                                        env_predicates);
}

/// In the case of a nested projection like <<A as Foo>::FooT as Bar>::BarT, we may find
/// that the definition of `Foo` has some clues:
///
/// ```
/// trait Foo {
///     type FooT : Bar<BarT=i32>
/// }
/// ```
///
/// Here, for example, we could conclude that the result is `i32`.
fn assemble_candidates_from_trait_def<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>,
    obligation_trait_ref: &ty::TraitRef<'tcx>,
    candidate_set: &mut ProjectionTyCandidateSet<'tcx>)
{
    debug!("assemble_candidates_from_trait_def(..)");

    // Check whether the self-type is itself a projection.
    let trait_ref = match obligation_trait_ref.self_ty().sty {
        ty::TyProjection(ref data) => data.trait_ref.clone(),
        ty::TyInfer(ty::TyVar(_)) => {
            // If the self-type is an inference variable, then it MAY wind up
            // being a projected type, so induce an ambiguity.
            candidate_set.ambiguous = true;
            return;
        }
        _ => { return; }
    };

    // If so, extract what we know from the trait and try to come up with a good answer.
    let trait_predicates = selcx.tcx().lookup_predicates(trait_ref.def_id);
    let bounds = trait_predicates.instantiate(selcx.tcx(), trait_ref.substs);
    let bounds = elaborate_predicates(selcx.tcx(), bounds.predicates.into_vec());
    assemble_candidates_from_predicates(selcx,
                                        obligation,
                                        obligation_trait_ref,
                                        candidate_set,
                                        ProjectionTyCandidate::TraitDef,
                                        bounds)
}

fn assemble_candidates_from_predicates<'cx, 'gcx, 'tcx, I>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>,
    obligation_trait_ref: &ty::TraitRef<'tcx>,
    candidate_set: &mut ProjectionTyCandidateSet<'tcx>,
    ctor: fn(ty::PolyProjectionPredicate<'tcx>) -> ProjectionTyCandidate<'tcx>,
    env_predicates: I)
    where I: Iterator<Item=ty::Predicate<'tcx>>
{
    debug!("assemble_candidates_from_predicates(obligation={:?})",
           obligation);
    let infcx = selcx.infcx();
    for predicate in env_predicates {
        debug!("assemble_candidates_from_predicates: predicate={:?}",
               predicate);
        match predicate {
            ty::Predicate::Projection(ref data) => {
                let same_name = data.item_name() == obligation.predicate.item_name;

                let is_match = same_name && infcx.probe(|_| {
                    let origin = TypeOrigin::Misc(obligation.cause.span);
                    let data_poly_trait_ref =
                        data.to_poly_trait_ref();
                    let obligation_poly_trait_ref =
                        obligation_trait_ref.to_poly_trait_ref();
                    infcx.sub_poly_trait_refs(false,
                                              origin,
                                              data_poly_trait_ref,
                                              obligation_poly_trait_ref)
                        // FIXME(#32730) propagate obligations
                        .map(|InferOk { obligations, .. }| assert!(obligations.is_empty()))
                        .is_ok()
                });

                debug!("assemble_candidates_from_predicates: candidate={:?} \
                                                             is_match={} same_name={}",
                       data, is_match, same_name);

                if is_match {
                    candidate_set.vec.push(ctor(data.clone()));
                }
            }
            _ => { }
        }
    }
}

fn assemble_candidates_from_object_type<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation:  &ProjectionTyObligation<'tcx>,
    obligation_trait_ref: &ty::TraitRef<'tcx>,
    candidate_set: &mut ProjectionTyCandidateSet<'tcx>)
{
    let self_ty = obligation_trait_ref.self_ty();
    let object_ty = selcx.infcx().shallow_resolve(self_ty);
    debug!("assemble_candidates_from_object_type(object_ty={:?})",
           object_ty);
    let data = match object_ty.sty {
        ty::TyTrait(ref data) => data,
        _ => {
            span_bug!(
                obligation.cause.span,
                "assemble_candidates_from_object_type called with non-object: {:?}",
                object_ty);
        }
    };
    let projection_bounds = data.projection_bounds_with_self_ty(selcx.tcx(), object_ty);
    let env_predicates = projection_bounds.iter()
                                          .map(|p| p.to_predicate())
                                          .collect();
    let env_predicates = elaborate_predicates(selcx.tcx(), env_predicates);
    assemble_candidates_from_predicates(selcx,
                                        obligation,
                                        obligation_trait_ref,
                                        candidate_set,
                                        ProjectionTyCandidate::ParamEnv,
                                        env_predicates)
}

fn assemble_candidates_from_impls<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>,
    obligation_trait_ref: &ty::TraitRef<'tcx>,
    candidate_set: &mut ProjectionTyCandidateSet<'tcx>)
    -> Result<(), SelectionError<'tcx>>
{
    // If we are resolving `<T as TraitRef<...>>::Item == Type`,
    // start out by selecting the predicate `T as TraitRef<...>`:
    let poly_trait_ref = obligation_trait_ref.to_poly_trait_ref();
    let trait_obligation = obligation.with(poly_trait_ref.to_poly_trait_predicate());
    let vtable = match selcx.select(&trait_obligation) {
        Ok(Some(vtable)) => vtable,
        Ok(None) => {
            candidate_set.ambiguous = true;
            return Ok(());
        }
        Err(e) => {
            debug!("assemble_candidates_from_impls: selection error {:?}",
                   e);
            return Err(e);
        }
    };

    match vtable {
        super::VtableImpl(data) => {
            debug!("assemble_candidates_from_impls: impl candidate {:?}",
                   data);

            candidate_set.vec.push(
                ProjectionTyCandidate::Impl(data));
        }
        super::VtableObject(_) => {
            assemble_candidates_from_object_type(
                selcx, obligation, obligation_trait_ref, candidate_set);
        }
        super::VtableClosure(data) => {
            candidate_set.vec.push(
                ProjectionTyCandidate::Closure(data));
        }
        super::VtableFnPointer(data) => {
            candidate_set.vec.push(
                ProjectionTyCandidate::FnPointer(data));
        }
        super::VtableParam(..) => {
            // This case tell us nothing about the value of an
            // associated type. Consider:
            //
            // ```
            // trait SomeTrait { type Foo; }
            // fn foo<T:SomeTrait>(...) { }
            // ```
            //
            // If the user writes `<T as SomeTrait>::Foo`, then the `T
            // : SomeTrait` binding does not help us decide what the
            // type `Foo` is (at least, not more specifically than
            // what we already knew).
            //
            // But wait, you say! What about an example like this:
            //
            // ```
            // fn bar<T:SomeTrait<Foo=usize>>(...) { ... }
            // ```
            //
            // Doesn't the `T : Sometrait<Foo=usize>` predicate help
            // resolve `T::Foo`? And of course it does, but in fact
            // that single predicate is desugared into two predicates
            // in the compiler: a trait predicate (`T : SomeTrait`) and a
            // projection. And the projection where clause is handled
            // in `assemble_candidates_from_param_env`.
        }
        super::VtableDefaultImpl(..) |
        super::VtableBuiltin(..) => {
            // These traits have no associated types.
            span_bug!(
                obligation.cause.span,
                "Cannot project an associated type from `{:?}`",
                vtable);
        }
    }

    Ok(())
}

fn confirm_candidate<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>,
    candidate: ProjectionTyCandidate<'tcx>)
    -> (Ty<'tcx>, Vec<PredicateObligation<'tcx>>)
{
    debug!("confirm_candidate(candidate={:?}, obligation={:?})",
           candidate,
           obligation);

    match candidate {
        ProjectionTyCandidate::ParamEnv(poly_projection) |
        ProjectionTyCandidate::TraitDef(poly_projection) => {
            confirm_param_env_candidate(selcx, obligation, poly_projection)
        }

        ProjectionTyCandidate::Impl(impl_vtable) => {
            confirm_impl_candidate(selcx, obligation, impl_vtable)
        }

        ProjectionTyCandidate::Closure(closure_vtable) => {
            confirm_closure_candidate(selcx, obligation, closure_vtable)
        }

        ProjectionTyCandidate::FnPointer(fn_pointer_vtable) => {
            confirm_fn_pointer_candidate(selcx, obligation, fn_pointer_vtable)
        }
    }
}

fn confirm_fn_pointer_candidate<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>,
    fn_pointer_vtable: VtableFnPointerData<'tcx, PredicateObligation<'tcx>>)
    -> (Ty<'tcx>, Vec<PredicateObligation<'tcx>>)
{
    // FIXME(#32730) propagate obligations (fn pointer vtable nested obligations ONLY come from
    // unification in inference)
    assert!(fn_pointer_vtable.nested.is_empty());
    let fn_type = selcx.infcx().shallow_resolve(fn_pointer_vtable.fn_ty);
    let sig = fn_type.fn_sig();
    confirm_callable_candidate(selcx, obligation, sig, util::TupleArgumentsFlag::Yes)
}

fn confirm_closure_candidate<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>,
    vtable: VtableClosureData<'tcx, PredicateObligation<'tcx>>)
    -> (Ty<'tcx>, Vec<PredicateObligation<'tcx>>)
{
    let closure_typer = selcx.closure_typer();
    let closure_type = closure_typer.closure_type(vtable.closure_def_id, vtable.substs);
    let Normalized {
        value: closure_type,
        mut obligations
    } = normalize_with_depth(selcx,
                             obligation.cause.clone(),
                             obligation.recursion_depth+1,
                             &closure_type);
    let (ty, mut cc_obligations) = confirm_callable_candidate(selcx,
                                                              obligation,
                                                              &closure_type.sig,
                                                              util::TupleArgumentsFlag::No);
    obligations.append(&mut cc_obligations);
    (ty, obligations)
}

fn confirm_callable_candidate<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>,
    fn_sig: &ty::PolyFnSig<'tcx>,
    flag: util::TupleArgumentsFlag)
    -> (Ty<'tcx>, Vec<PredicateObligation<'tcx>>)
{
    let tcx = selcx.tcx();

    debug!("confirm_callable_candidate({:?},{:?})",
           obligation,
           fn_sig);

    // the `Output` associated type is declared on `FnOnce`
    let fn_once_def_id = tcx.lang_items.fn_once_trait().unwrap();

    // Note: we unwrap the binder here but re-create it below (1)
    let ty::Binder((trait_ref, ret_type)) =
        tcx.closure_trait_ref_and_return_type(fn_once_def_id,
                                              obligation.predicate.trait_ref.self_ty(),
                                              fn_sig,
                                              flag);

    let predicate = ty::Binder(ty::ProjectionPredicate { // (1) recreate binder here
        projection_ty: ty::ProjectionTy {
            trait_ref: trait_ref,
            item_name: token::intern(FN_OUTPUT_NAME),
        },
        ty: ret_type
    });

    confirm_param_env_candidate(selcx, obligation, predicate)
}

fn confirm_param_env_candidate<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>,
    poly_projection: ty::PolyProjectionPredicate<'tcx>)
    -> (Ty<'tcx>, Vec<PredicateObligation<'tcx>>)
{
    let infcx = selcx.infcx();

    let projection =
        infcx.replace_late_bound_regions_with_fresh_var(
            obligation.cause.span,
            infer::LateBoundRegionConversionTime::HigherRankedType,
            &poly_projection).0;

    assert_eq!(projection.projection_ty.item_name,
               obligation.predicate.item_name);

    let origin = TypeOrigin::RelateOutputImplTypes(obligation.cause.span);
    match infcx.eq_trait_refs(false,
                              origin,
                              obligation.predicate.trait_ref.clone(),
                              projection.projection_ty.trait_ref.clone()) {
        Ok(InferOk { obligations, .. }) => {
            // FIXME(#32730) propagate obligations
            assert!(obligations.is_empty());
        }
        Err(e) => {
            span_bug!(
                obligation.cause.span,
                "Failed to unify `{:?}` and `{:?}` in projection: {}",
                obligation,
                projection,
                e);
        }
    }

    (projection.ty, vec!())
}

fn confirm_impl_candidate<'cx, 'gcx, 'tcx>(
    selcx: &mut SelectionContext<'cx, 'gcx, 'tcx>,
    obligation: &ProjectionTyObligation<'tcx>,
    impl_vtable: VtableImplData<'tcx, PredicateObligation<'tcx>>)
    -> (Ty<'tcx>, Vec<PredicateObligation<'tcx>>)
{
    let VtableImplData { substs, nested, impl_def_id } = impl_vtable;

    let tcx = selcx.tcx();
    let trait_ref = obligation.predicate.trait_ref;
    let assoc_ty = assoc_ty_def(selcx, impl_def_id, obligation.predicate.item_name);

    match assoc_ty {
        Some(node_item) => {
            let ty = node_item.item.ty.unwrap_or_else(|| {
                // This means that the impl is missing a definition for the
                // associated type. This error will be reported by the type
                // checker method `check_impl_items_against_trait`, so here we
                // just return TyError.
                debug!("confirm_impl_candidate: no associated type {:?} for {:?}",
                       node_item.item.name,
                       obligation.predicate.trait_ref);
                tcx.types.err
            });
            let substs = translate_substs(selcx.infcx(), impl_def_id, substs, node_item.node);
            (ty.subst(tcx, substs), nested)
        }
        None => {
            span_bug!(obligation.cause.span,
                      "No associated type for {:?}",
                      trait_ref);
        }
    }
}

/// Locate the definition of an associated type in the specialization hierarchy,
/// starting from the given impl.
///
/// Based on the "projection mode", this lookup may in fact only examine the
/// topmost impl. See the comments for `ProjectionMode` for more details.
fn assoc_ty_def<'cx, 'gcx, 'tcx>(
    selcx: &SelectionContext<'cx, 'gcx, 'tcx>,
    impl_def_id: DefId,
    assoc_ty_name: ast::Name)
    -> Option<specialization_graph::NodeItem<Rc<ty::AssociatedType<'tcx>>>>
{
    let trait_def_id = selcx.tcx().impl_trait_ref(impl_def_id).unwrap().def_id;

    if selcx.projection_mode().is_topmost() {
        let impl_node = specialization_graph::Node::Impl(impl_def_id);
        for item in impl_node.items(selcx.tcx()) {
            if let ty::TypeTraitItem(assoc_ty) = item {
                if assoc_ty.name == assoc_ty_name {
                    return Some(specialization_graph::NodeItem {
                        node: specialization_graph::Node::Impl(impl_def_id),
                        item: assoc_ty,
                    });
                }
            }
        }
        None
    } else {
        selcx.tcx().lookup_trait_def(trait_def_id)
            .ancestors(impl_def_id)
            .type_defs(selcx.tcx(), assoc_ty_name)
            .next()
    }
}
