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
use traits::query::Fallible;
use ty::{ParamEnvAnd, Predicate, TyCtxt};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct ProvePredicate<'tcx> {
    pub predicate: Predicate<'tcx>,
}

impl<'tcx> ProvePredicate<'tcx> {
    pub fn new(predicate: Predicate<'tcx>) -> Self {
        ProvePredicate { predicate }
    }
}

impl<'gcx: 'tcx, 'tcx> super::QueryTypeOp<'gcx, 'tcx> for ProvePredicate<'tcx> {
    type QueryResult = ();

    fn try_fast_path(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        key: &ParamEnvAnd<'tcx, Self>,
    ) -> Option<Self::QueryResult> {
        // Proving Sized, very often on "obviously sized" types like
        // `&T`, accounts for about 60% percentage of the predicates
        // we have to prove. No need to canonicalize and all that for
        // such cases.
        if let Predicate::Trait(trait_ref) = key.value.predicate {
            if let Some(sized_def_id) = tcx.lang_items().sized_trait() {
                if trait_ref.def_id() == sized_def_id {
                    if trait_ref.skip_binder().self_ty().is_trivially_sized(tcx) {
                        return Some(());
                    }
                }
            }
        }

        None
    }

    fn perform_query(
        tcx: TyCtxt<'_, 'gcx, 'tcx>,
        canonicalized: Canonicalized<'gcx, ParamEnvAnd<'tcx, Self>>,
    ) -> Fallible<CanonicalizedQueryResult<'gcx, ()>> {
        tcx.type_op_prove_predicate(canonicalized)
    }

    fn shrink_to_tcx_lifetime(
        v: &'a CanonicalizedQueryResult<'gcx, ()>,
    ) -> &'a Canonical<'tcx, QueryResult<'tcx, ()>> {
        v
    }
}

BraceStructTypeFoldableImpl! {
    impl<'tcx> TypeFoldable<'tcx> for ProvePredicate<'tcx> {
        predicate,
    }
}

BraceStructLiftImpl! {
    impl<'a, 'tcx> Lift<'tcx> for ProvePredicate<'a> {
        type Lifted = ProvePredicate<'tcx>;
        predicate,
    }
}

impl_stable_hash_for! {
    struct ProvePredicate<'tcx> { predicate }
}
