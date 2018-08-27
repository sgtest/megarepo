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
use traits::query::outlives_bounds::OutlivesBound;
use traits::query::Fallible;
use ty::{ParamEnvAnd, Ty, TyCtxt};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct ImpliedOutlivesBounds<'tcx> {
    pub ty: Ty<'tcx>,
}

impl<'tcx> ImpliedOutlivesBounds<'tcx> {
    pub fn new(ty: Ty<'tcx>) -> Self {
        ImpliedOutlivesBounds { ty }
    }
}

impl<'gcx: 'tcx, 'tcx> super::QueryTypeOp<'gcx, 'tcx> for ImpliedOutlivesBounds<'tcx> {
    type QueryResult = Vec<OutlivesBound<'tcx>>;

    fn try_fast_path(
        _tcx: TyCtxt<'_, 'gcx, 'tcx>,
        _key: &ParamEnvAnd<'tcx, Self>,
    ) -> Option<Self::QueryResult> {
        None
    }

    fn perform_query(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        canonicalized: Canonicalized<'gcx, ParamEnvAnd<'tcx, Self>>,
    ) -> Fallible<CanonicalizedQueryResult<'gcx, Self::QueryResult>> {
        // FIXME the query should take a `ImpliedOutlivesBounds`
        let Canonical {
            variables,
            value:
                ParamEnvAnd {
                    param_env,
                    value: ImpliedOutlivesBounds { ty },
                },
        } = canonicalized;
        let canonicalized = Canonical {
            variables,
            value: param_env.and(ty),
        };

        tcx.implied_outlives_bounds(canonicalized)
    }

    fn shrink_to_tcx_lifetime(
        v: &'a CanonicalizedQueryResult<'gcx, Self::QueryResult>,
    ) -> &'a Canonical<'tcx, QueryResult<'tcx, Self::QueryResult>> {
        v
    }
}

BraceStructTypeFoldableImpl! {
    impl<'tcx> TypeFoldable<'tcx> for ImpliedOutlivesBounds<'tcx> {
        ty,
    }
}

BraceStructLiftImpl! {
    impl<'a, 'tcx> Lift<'tcx> for ImpliedOutlivesBounds<'a> {
        type Lifted = ImpliedOutlivesBounds<'tcx>;
        ty,
    }
}

impl_stable_hash_for! {
    struct ImpliedOutlivesBounds<'tcx> { ty }
}
