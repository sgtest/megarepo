//! Code for the 'normalization' query. This consists of a wrapper
//! which folds deeply, invoking the underlying
//! `normalize_projection_ty` query when it encounters projections.

use crate::infer::at::At;
use crate::infer::canonical::OriginalQueryValues;
use crate::infer::{InferCtxt, InferOk};
use crate::traits::error_reporting::InferCtxtExt;
use crate::traits::project::needs_normalization;
use crate::traits::{Obligation, ObligationCause, PredicateObligation, Reveal};
use rustc_data_structures::sso::SsoHashMap;
use rustc_data_structures::stack::ensure_sufficient_stack;
use rustc_infer::traits::Normalized;
use rustc_middle::mir;
use rustc_middle::ty::fold::{TypeFoldable, TypeFolder};
use rustc_middle::ty::subst::Subst;
use rustc_middle::ty::{self, Ty, TyCtxt};

use super::NoSolution;

pub use rustc_middle::traits::query::NormalizationResult;

pub trait AtExt<'tcx> {
    fn normalize<T>(&self, value: T) -> Result<Normalized<'tcx, T>, NoSolution>
    where
        T: TypeFoldable<'tcx>;
}

impl<'cx, 'tcx> AtExt<'tcx> for At<'cx, 'tcx> {
    /// Normalize `value` in the context of the inference context,
    /// yielding a resulting type, or an error if `value` cannot be
    /// normalized. If you don't care about regions, you should prefer
    /// `normalize_erasing_regions`, which is more efficient.
    ///
    /// If the normalization succeeds and is unambiguous, returns back
    /// the normalized value along with various outlives relations (in
    /// the form of obligations that must be discharged).
    ///
    /// N.B., this will *eventually* be the main means of
    /// normalizing, but for now should be used only when we actually
    /// know that normalization will succeed, since error reporting
    /// and other details are still "under development".
    fn normalize<T>(&self, value: T) -> Result<Normalized<'tcx, T>, NoSolution>
    where
        T: TypeFoldable<'tcx>,
    {
        debug!(
            "normalize::<{}>(value={:?}, param_env={:?})",
            std::any::type_name::<T>(),
            value,
            self.param_env,
        );
        if !needs_normalization(&value, self.param_env.reveal()) {
            return Ok(Normalized { value, obligations: vec![] });
        }

        let mut normalizer = QueryNormalizer {
            infcx: self.infcx,
            cause: self.cause,
            param_env: self.param_env,
            obligations: vec![],
            error: false,
            cache: SsoHashMap::new(),
            anon_depth: 0,
            universes: vec![],
        };

        let result = value.fold_with(&mut normalizer);
        info!(
            "normalize::<{}>: result={:?} with {} obligations",
            std::any::type_name::<T>(),
            result,
            normalizer.obligations.len(),
        );
        debug!(
            "normalize::<{}>: obligations={:?}",
            std::any::type_name::<T>(),
            normalizer.obligations,
        );
        if normalizer.error {
            Err(NoSolution)
        } else {
            Ok(Normalized { value: result, obligations: normalizer.obligations })
        }
    }
}

struct QueryNormalizer<'cx, 'tcx> {
    infcx: &'cx InferCtxt<'cx, 'tcx>,
    cause: &'cx ObligationCause<'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    obligations: Vec<PredicateObligation<'tcx>>,
    cache: SsoHashMap<Ty<'tcx>, Ty<'tcx>>,
    error: bool,
    anon_depth: usize,
    universes: Vec<Option<ty::UniverseIndex>>,
}

impl<'cx, 'tcx> TypeFolder<'tcx> for QueryNormalizer<'cx, 'tcx> {
    fn tcx<'c>(&'c self) -> TyCtxt<'tcx> {
        self.infcx.tcx
    }

    fn fold_binder<T: TypeFoldable<'tcx>>(
        &mut self,
        t: ty::Binder<'tcx, T>,
    ) -> ty::Binder<'tcx, T> {
        self.universes.push(None);
        let t = t.super_fold_with(self);
        self.universes.pop();
        t
    }

    #[instrument(level = "debug", skip(self))]
    fn fold_ty(&mut self, ty: Ty<'tcx>) -> Ty<'tcx> {
        if !needs_normalization(&ty, self.param_env.reveal()) {
            return ty;
        }

        if let Some(ty) = self.cache.get(&ty) {
            return ty;
        }

        let ty = ty.super_fold_with(self);
        let res = (|| match *ty.kind() {
            ty::Opaque(def_id, substs) if !substs.has_escaping_bound_vars() => {
                // Only normalize `impl Trait` after type-checking, usually in codegen.
                match self.param_env.reveal() {
                    Reveal::UserFacing => ty,

                    Reveal::All => {
                        let recursion_limit = self.tcx().recursion_limit();
                        if !recursion_limit.value_within_limit(self.anon_depth) {
                            let obligation = Obligation::with_depth(
                                self.cause.clone(),
                                recursion_limit.0,
                                self.param_env,
                                ty,
                            );
                            self.infcx.report_overflow_error(&obligation, true);
                        }

                        let generic_ty = self.tcx().type_of(def_id);
                        let concrete_ty = generic_ty.subst(self.tcx(), substs);
                        self.anon_depth += 1;
                        if concrete_ty == ty {
                            bug!(
                                "infinite recursion generic_ty: {:#?}, substs: {:#?}, \
                                 concrete_ty: {:#?}, ty: {:#?}",
                                generic_ty,
                                substs,
                                concrete_ty,
                                ty
                            );
                        }
                        let folded_ty = ensure_sufficient_stack(|| self.fold_ty(concrete_ty));
                        self.anon_depth -= 1;
                        folded_ty
                    }
                }
            }

            ty::Projection(data) if !data.has_escaping_bound_vars() => {
                // This is kind of hacky -- we need to be able to
                // handle normalization within binders because
                // otherwise we wind up a need to normalize when doing
                // trait matching (since you can have a trait
                // obligation like `for<'a> T::B: Fn(&'a i32)`), but
                // we can't normalize with bound regions in scope. So
                // far now we just ignore binders but only normalize
                // if all bound regions are gone (and then we still
                // have to renormalize whenever we instantiate a
                // binder). It would be better to normalize in a
                // binding-aware fashion.

                let tcx = self.infcx.tcx;

                let mut orig_values = OriginalQueryValues::default();
                // HACK(matthewjasper) `'static` is special-cased in selection,
                // so we cannot canonicalize it.
                let c_data = self
                    .infcx
                    .canonicalize_query_keep_static(self.param_env.and(data), &mut orig_values);
                debug!("QueryNormalizer: c_data = {:#?}", c_data);
                debug!("QueryNormalizer: orig_values = {:#?}", orig_values);
                match tcx.normalize_projection_ty(c_data) {
                    Ok(result) => {
                        // We don't expect ambiguity.
                        if result.is_ambiguous() {
                            self.error = true;
                            return ty;
                        }

                        match self.infcx.instantiate_query_response_and_region_obligations(
                            self.cause,
                            self.param_env,
                            &orig_values,
                            result,
                        ) {
                            Ok(InferOk { value: result, obligations }) => {
                                debug!("QueryNormalizer: result = {:#?}", result);
                                debug!("QueryNormalizer: obligations = {:#?}", obligations);
                                self.obligations.extend(obligations);
                                result.normalized_ty
                            }

                            Err(_) => {
                                self.error = true;
                                ty
                            }
                        }
                    }

                    Err(NoSolution) => {
                        self.error = true;
                        ty
                    }
                }
            }
            ty::Projection(data) if !data.trait_ref(self.infcx.tcx).has_escaping_bound_vars() => {
                // See note in `rustc_trait_selection::traits::project`

                // One other point mentioning: In `traits::project`, if a
                // projection can't be normalized, we return an inference variable
                // and register an obligation to later resolve that. Here, the query
                // will just return ambiguity. In both cases, the effect is the same: we only want
                // to return `ty` because there are bound vars that we aren't yet handling in a more
                // complete way.

                // `BoundVarReplacer` can't handle escaping bound vars. Ideally, we want this before even calling
                // `QueryNormalizer`, but some const-generics tests pass escaping bound vars.
                // Also, use `ty` so we get that sweet `outer_exclusive_binder` optimization
                assert!(!ty.has_vars_bound_at_or_above(ty::DebruijnIndex::from_usize(
                    self.universes.len()
                )));

                let tcx = self.infcx.tcx;
                let infcx = self.infcx;
                let (data, mapped_regions, mapped_types, mapped_consts) =
                    crate::traits::project::BoundVarReplacer::replace_bound_vars(
                        infcx,
                        &mut self.universes,
                        data,
                    );
                let data = data.super_fold_with(self);

                let mut orig_values = OriginalQueryValues::default();
                // HACK(matthewjasper) `'static` is special-cased in selection,
                // so we cannot canonicalize it.
                let c_data = self
                    .infcx
                    .canonicalize_query_keep_static(self.param_env.and(data), &mut orig_values);
                debug!("QueryNormalizer: c_data = {:#?}", c_data);
                debug!("QueryNormalizer: orig_values = {:#?}", orig_values);
                let normalized_ty = match tcx.normalize_projection_ty(c_data) {
                    Ok(result) => {
                        // We don't expect ambiguity.
                        if result.is_ambiguous() {
                            self.error = true;
                            return ty;
                        }
                        match self.infcx.instantiate_query_response_and_region_obligations(
                            self.cause,
                            self.param_env,
                            &orig_values,
                            result,
                        ) {
                            Ok(InferOk { value: result, obligations }) => {
                                debug!("QueryNormalizer: result = {:#?}", result);
                                debug!("QueryNormalizer: obligations = {:#?}", obligations);
                                self.obligations.extend(obligations);
                                result.normalized_ty
                            }
                            Err(_) => {
                                self.error = true;
                                ty
                            }
                        }
                    }
                    Err(NoSolution) => {
                        self.error = true;
                        ty
                    }
                };
                crate::traits::project::PlaceholderReplacer::replace_placeholders(
                    infcx,
                    mapped_regions,
                    mapped_types,
                    mapped_consts,
                    &self.universes,
                    normalized_ty,
                )
            }

            _ => ty,
        })();
        self.cache.insert(ty, res);
        res
    }

    fn fold_const(&mut self, constant: &'tcx ty::Const<'tcx>) -> &'tcx ty::Const<'tcx> {
        let constant = constant.super_fold_with(self);
        constant.eval(self.infcx.tcx, self.param_env)
    }

    fn fold_mir_const(&mut self, constant: mir::ConstantKind<'tcx>) -> mir::ConstantKind<'tcx> {
        constant.super_fold_with(self)
    }
}
