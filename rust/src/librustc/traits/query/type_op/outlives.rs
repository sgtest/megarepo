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
use traits::query::dropck_outlives::trivial_dropck_outlives;
use traits::query::dropck_outlives::DropckOutlivesResult;
use traits::query::Fallible;
use ty::{ParamEnvAnd, Ty, TyCtxt};

#[derive(Copy, Clone, Debug)]
pub struct DropckOutlives<'tcx> {
    dropped_ty: Ty<'tcx>,
}

impl<'tcx> DropckOutlives<'tcx> {
    pub fn new(dropped_ty: Ty<'tcx>) -> Self {
        DropckOutlives { dropped_ty }
    }
}

impl super::QueryTypeOp<'gcx, 'tcx> for DropckOutlives<'tcx>
where
    'gcx: 'tcx,
{
    type QueryResult = DropckOutlivesResult<'tcx>;

    fn try_fast_path(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        key: &ParamEnvAnd<'tcx, Self>,
    ) -> Option<Self::QueryResult> {
        if trivial_dropck_outlives(tcx, key.value.dropped_ty) {
            Some(DropckOutlivesResult::default())
        } else {
            None
        }
    }

    fn perform_query(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        canonicalized: Canonicalized<'gcx, ParamEnvAnd<'tcx, Self>>,
    ) -> Fallible<CanonicalizedQueryResult<'gcx, Self::QueryResult>> {
        // Subtle: note that we are not invoking
        // `infcx.at(...).dropck_outlives(...)` here, but rather the
        // underlying `dropck_outlives` query. This same underlying
        // query is also used by the
        // `infcx.at(...).dropck_outlives(...)` fn. Avoiding the
        // wrapper means we don't need an infcx in this code, which is
        // good because the interface doesn't give us one (so that we
        // know we are not registering any subregion relations or
        // other things).

        // FIXME convert to the type expected by the `dropck_outlives`
        // query. This should eventually be fixed by changing the
        // *underlying query*.
        let Canonical {
            variables,
            value:
                ParamEnvAnd {
                    param_env,
                    value: DropckOutlives { dropped_ty },
                },
        } = canonicalized;
        let canonicalized = Canonical {
            variables,
            value: param_env.and(dropped_ty),
        };

        tcx.dropck_outlives(canonicalized)
    }

    fn shrink_to_tcx_lifetime(
        lifted_query_result: &'a CanonicalizedQueryResult<'gcx, Self::QueryResult>,
    ) -> &'a Canonical<'tcx, QueryResult<'tcx, Self::QueryResult>> {
        lifted_query_result
    }
}

BraceStructTypeFoldableImpl! {
    impl<'tcx> TypeFoldable<'tcx> for DropckOutlives<'tcx> {
        dropped_ty
    }
}

BraceStructLiftImpl! {
    impl<'a, 'tcx> Lift<'tcx> for DropckOutlives<'a> {
        type Lifted = DropckOutlives<'tcx>;
        dropped_ty
    }
}

impl_stable_hash_for! {
    struct DropckOutlives<'tcx> { dropped_ty }
}
