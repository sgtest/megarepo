// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use infer::canonical::{Canonical, Canonicalized, CanonicalizedQueryResult, QueryResult};
use std::fmt;
use traits::query::Fallible;
use ty::fold::TypeFoldable;
use ty::{self, Lift, ParamEnvAnd, Ty, TyCtxt};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct Normalize<T> {
    pub value: T,
}

impl<'tcx, T> Normalize<T>
where
    T: fmt::Debug + TypeFoldable<'tcx>,
{
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

impl<'gcx: 'tcx, 'tcx, T> super::QueryTypeOp<'gcx, 'tcx> for Normalize<T>
where
    T: Normalizable<'gcx, 'tcx>,
{
    type QueryResult = T;

    fn try_fast_path(_tcx: TyCtxt<'_, 'gcx, 'tcx>, key: &ParamEnvAnd<'tcx, Self>) -> Option<T> {
        if !key.value.value.has_projections() {
            Some(key.value.value)
        } else {
            None
        }
    }

    fn perform_query(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        canonicalized: Canonicalized<'gcx, ParamEnvAnd<'tcx, Self>>,
    ) -> Fallible<CanonicalizedQueryResult<'gcx, Self::QueryResult>> {
        T::type_op_method(tcx, canonicalized)
    }

    fn shrink_to_tcx_lifetime(
        v: &'a CanonicalizedQueryResult<'gcx, T>,
    ) -> &'a Canonical<'tcx, QueryResult<'tcx, T>> {
        T::shrink_to_tcx_lifetime(v)
    }
}

pub trait Normalizable<'gcx, 'tcx>: fmt::Debug + TypeFoldable<'tcx> + Lift<'gcx> + Copy {
    fn type_op_method(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        canonicalized: Canonicalized<'gcx, ParamEnvAnd<'tcx, Normalize<Self>>>,
    ) -> Fallible<CanonicalizedQueryResult<'gcx, Self>>;

    /// Convert from the `'gcx` (lifted) form of `Self` into the `tcx`
    /// form of `Self`.
    fn shrink_to_tcx_lifetime(
        v: &'a CanonicalizedQueryResult<'gcx, Self>,
    ) -> &'a Canonical<'tcx, QueryResult<'tcx, Self>>;
}

impl Normalizable<'gcx, 'tcx> for Ty<'tcx>
where
    'gcx: 'tcx,
{
    fn type_op_method(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        canonicalized: Canonicalized<'gcx, ParamEnvAnd<'tcx, Normalize<Self>>>,
    ) -> Fallible<CanonicalizedQueryResult<'gcx, Self>> {
        tcx.type_op_normalize_ty(canonicalized)
    }

    fn shrink_to_tcx_lifetime(
        v: &'a CanonicalizedQueryResult<'gcx, Self>,
    ) -> &'a Canonical<'tcx, QueryResult<'tcx, Self>> {
        v
    }
}

impl Normalizable<'gcx, 'tcx> for ty::Predicate<'tcx>
where
    'gcx: 'tcx,
{
    fn type_op_method(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        canonicalized: Canonicalized<'gcx, ParamEnvAnd<'tcx, Normalize<Self>>>,
    ) -> Fallible<CanonicalizedQueryResult<'gcx, Self>> {
        tcx.type_op_normalize_predicate(canonicalized)
    }

    fn shrink_to_tcx_lifetime(
        v: &'a CanonicalizedQueryResult<'gcx, Self>,
    ) -> &'a Canonical<'tcx, QueryResult<'tcx, Self>> {
        v
    }
}

impl Normalizable<'gcx, 'tcx> for ty::PolyFnSig<'tcx>
where
    'gcx: 'tcx,
{
    fn type_op_method(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        canonicalized: Canonicalized<'gcx, ParamEnvAnd<'tcx, Normalize<Self>>>,
    ) -> Fallible<CanonicalizedQueryResult<'gcx, Self>> {
        tcx.type_op_normalize_poly_fn_sig(canonicalized)
    }

    fn shrink_to_tcx_lifetime(
        v: &'a CanonicalizedQueryResult<'gcx, Self>,
    ) -> &'a Canonical<'tcx, QueryResult<'tcx, Self>> {
        v
    }
}

impl Normalizable<'gcx, 'tcx> for ty::FnSig<'tcx>
where
    'gcx: 'tcx,
{
    fn type_op_method(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        canonicalized: Canonicalized<'gcx, ParamEnvAnd<'tcx, Normalize<Self>>>,
    ) -> Fallible<CanonicalizedQueryResult<'gcx, Self>> {
        tcx.type_op_normalize_fn_sig(canonicalized)
    }

    fn shrink_to_tcx_lifetime(
        v: &'a CanonicalizedQueryResult<'gcx, Self>,
    ) -> &'a Canonical<'tcx, QueryResult<'tcx, Self>> {
        v
    }
}

BraceStructTypeFoldableImpl! {
    impl<'tcx, T> TypeFoldable<'tcx> for Normalize<T> {
        value,
    } where T: TypeFoldable<'tcx>,
}

BraceStructLiftImpl! {
    impl<'tcx, T> Lift<'tcx> for Normalize<T> {
        type Lifted = Normalize<T::Lifted>;
        value,
    } where T: Lift<'tcx>,
}

impl_stable_hash_for! {
    impl<'tcx, T> for struct Normalize<T> {
        value
    }
}
