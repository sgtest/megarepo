// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rustc::infer::canonical::{Canonical, QueryResult};
use rustc::infer::InferCtxt;
use rustc::traits::query::type_op::eq::Eq;
use rustc::traits::query::type_op::normalize::Normalize;
use rustc::traits::query::type_op::prove_predicate::ProvePredicate;
use rustc::traits::query::type_op::subtype::Subtype;
use rustc::traits::query::{Fallible, NoSolution};
use rustc::traits::{FulfillmentContext, Normalized, Obligation, ObligationCause, TraitEngine,
                    TraitEngineExt};
use rustc::ty::query::Providers;
use rustc::ty::{FnSig, Lift, ParamEnvAnd, PolyFnSig, Predicate, Ty, TyCtxt, TypeFoldable};
use rustc_data_structures::sync::Lrc;
use std::fmt;

crate fn provide(p: &mut Providers) {
    *p = Providers {
        type_op_eq,
        type_op_prove_predicate,
        type_op_subtype,
        type_op_normalize_ty,
        type_op_normalize_predicate,
        type_op_normalize_fn_sig,
        type_op_normalize_poly_fn_sig,
        ..*p
    };
}

fn type_op_eq<'tcx>(
    tcx: TyCtxt<'_, 'tcx, 'tcx>,
    canonicalized: Canonical<'tcx, ParamEnvAnd<'tcx, Eq<'tcx>>>,
) -> Result<Lrc<Canonical<'tcx, QueryResult<'tcx, ()>>>, NoSolution> {
    tcx.infer_ctxt()
        .enter_canonical_trait_query(&canonicalized, |infcx, fulfill_cx, key| {
            let (param_env, Eq { a, b }) = key.into_parts();
            Ok(infcx
                .at(&ObligationCause::dummy(), param_env)
                .eq(a, b)?
                .into_value_registering_obligations(infcx, fulfill_cx))
        })
}

fn type_op_normalize<T>(
    infcx: &InferCtxt<'_, 'gcx, 'tcx>,
    fulfill_cx: &mut FulfillmentContext<'tcx>,
    key: ParamEnvAnd<'tcx, Normalize<T>>,
) -> Fallible<T>
where
    T: fmt::Debug + TypeFoldable<'tcx> + Lift<'gcx>,
{
    let (param_env, Normalize { value }) = key.into_parts();
    let Normalized { value, obligations } = infcx
        .at(&ObligationCause::dummy(), param_env)
        .normalize(&value)?;
    fulfill_cx.register_predicate_obligations(infcx, obligations);
    Ok(value)
}

fn type_op_normalize_ty(
    tcx: TyCtxt<'_, 'tcx, 'tcx>,
    canonicalized: Canonical<'tcx, ParamEnvAnd<'tcx, Normalize<Ty<'tcx>>>>,
) -> Result<Lrc<Canonical<'tcx, QueryResult<'tcx, Ty<'tcx>>>>, NoSolution> {
    tcx.infer_ctxt()
        .enter_canonical_trait_query(&canonicalized, type_op_normalize)
}

fn type_op_normalize_predicate(
    tcx: TyCtxt<'_, 'tcx, 'tcx>,
    canonicalized: Canonical<'tcx, ParamEnvAnd<'tcx, Normalize<Predicate<'tcx>>>>,
) -> Result<Lrc<Canonical<'tcx, QueryResult<'tcx, Predicate<'tcx>>>>, NoSolution> {
    tcx.infer_ctxt()
        .enter_canonical_trait_query(&canonicalized, type_op_normalize)
}

fn type_op_normalize_fn_sig(
    tcx: TyCtxt<'_, 'tcx, 'tcx>,
    canonicalized: Canonical<'tcx, ParamEnvAnd<'tcx, Normalize<FnSig<'tcx>>>>,
) -> Result<Lrc<Canonical<'tcx, QueryResult<'tcx, FnSig<'tcx>>>>, NoSolution> {
    tcx.infer_ctxt()
        .enter_canonical_trait_query(&canonicalized, type_op_normalize)
}

fn type_op_normalize_poly_fn_sig(
    tcx: TyCtxt<'_, 'tcx, 'tcx>,
    canonicalized: Canonical<'tcx, ParamEnvAnd<'tcx, Normalize<PolyFnSig<'tcx>>>>,
) -> Result<Lrc<Canonical<'tcx, QueryResult<'tcx, PolyFnSig<'tcx>>>>, NoSolution> {
    tcx.infer_ctxt()
        .enter_canonical_trait_query(&canonicalized, type_op_normalize)
}

fn type_op_subtype<'tcx>(
    tcx: TyCtxt<'_, 'tcx, 'tcx>,
    canonicalized: Canonical<'tcx, ParamEnvAnd<'tcx, Subtype<'tcx>>>,
) -> Result<Lrc<Canonical<'tcx, QueryResult<'tcx, ()>>>, NoSolution> {
    tcx.infer_ctxt()
        .enter_canonical_trait_query(&canonicalized, |infcx, fulfill_cx, key| {
            let (param_env, Subtype { sub, sup }) = key.into_parts();
            Ok(infcx
                .at(&ObligationCause::dummy(), param_env)
                .sup(sup, sub)?
                .into_value_registering_obligations(infcx, fulfill_cx))
        })
}

fn type_op_prove_predicate<'tcx>(
    tcx: TyCtxt<'_, 'tcx, 'tcx>,
    canonicalized: Canonical<'tcx, ParamEnvAnd<'tcx, ProvePredicate<'tcx>>>,
) -> Result<Lrc<Canonical<'tcx, QueryResult<'tcx, ()>>>, NoSolution> {
    tcx.infer_ctxt()
        .enter_canonical_trait_query(&canonicalized, |infcx, fulfill_cx, key| {
            let (param_env, ProvePredicate { predicate }) = key.into_parts();
            fulfill_cx.register_predicate_obligation(
                infcx,
                Obligation::new(ObligationCause::dummy(), param_env, predicate),
            );
            Ok(())
        })
}
