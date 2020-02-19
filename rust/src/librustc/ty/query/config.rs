use crate::dep_graph::SerializedDepNodeIndex;
use crate::dep_graph::{DepKind, DepNode};
use crate::ty::query::caches::QueryCache;
use crate::ty::query::plumbing::CycleError;
use crate::ty::query::queries;
use crate::ty::query::{Query, QueryState};
use crate::ty::TyCtxt;
use rustc_data_structures::profiling::ProfileCategory;
use rustc_hir::def_id::{CrateNum, DefId};

use crate::ich::StableHashingContext;
use rustc_data_structures::fingerprint::Fingerprint;
use std::borrow::Cow;
use std::fmt::Debug;
use std::hash::Hash;

// Query configuration and description traits.

// FIXME(eddyb) false positive, the lifetime parameter is used for `Key`/`Value`.
#[allow(unused_lifetimes)]
pub trait QueryConfig<'tcx> {
    const NAME: &'static str;
    const CATEGORY: ProfileCategory;

    type Key: Eq + Hash + Clone + Debug;
    type Value: Clone;
}

pub(crate) trait QueryAccessors<'tcx>: QueryConfig<'tcx> {
    const ANON: bool;
    const EVAL_ALWAYS: bool;

    type Cache: QueryCache<Self::Key, Self::Value>;

    fn query(key: Self::Key) -> Query<'tcx>;

    // Don't use this method to access query results, instead use the methods on TyCtxt
    fn query_state<'a>(tcx: TyCtxt<'tcx>) -> &'a QueryState<'tcx, Self>;

    fn to_dep_node(tcx: TyCtxt<'tcx>, key: &Self::Key) -> DepNode;

    fn dep_kind() -> DepKind;

    // Don't use this method to compute query results, instead use the methods on TyCtxt
    fn compute(tcx: TyCtxt<'tcx>, key: Self::Key) -> Self::Value;

    fn hash_result(hcx: &mut StableHashingContext<'_>, result: &Self::Value)
    -> Option<Fingerprint>;

    fn handle_cycle_error(tcx: TyCtxt<'tcx>, error: CycleError<'tcx>) -> Self::Value;
}

pub(crate) trait QueryDescription<'tcx>: QueryAccessors<'tcx> {
    fn describe(tcx: TyCtxt<'_>, key: Self::Key) -> Cow<'static, str>;

    #[inline]
    fn cache_on_disk(_: TyCtxt<'tcx>, _: Self::Key, _: Option<&Self::Value>) -> bool {
        false
    }

    fn try_load_from_disk(_: TyCtxt<'tcx>, _: SerializedDepNodeIndex) -> Option<Self::Value> {
        bug!("QueryDescription::load_from_disk() called for an unsupported query.")
    }
}

impl<'tcx, M: QueryAccessors<'tcx, Key = DefId>> QueryDescription<'tcx> for M
where
    <M as QueryAccessors<'tcx>>::Cache: QueryCache<DefId, <M as QueryConfig<'tcx>>::Value>,
{
    default fn describe(tcx: TyCtxt<'_>, def_id: DefId) -> Cow<'static, str> {
        if !tcx.sess.verbose() {
            format!("processing `{}`", tcx.def_path_str(def_id)).into()
        } else {
            let name = ::std::any::type_name::<M>();
            format!("processing {:?} with query `{}`", def_id, name).into()
        }
    }

    default fn cache_on_disk(_: TyCtxt<'tcx>, _: Self::Key, _: Option<&Self::Value>) -> bool {
        false
    }

    default fn try_load_from_disk(
        _: TyCtxt<'tcx>,
        _: SerializedDepNodeIndex,
    ) -> Option<Self::Value> {
        bug!("QueryDescription::load_from_disk() called for an unsupported query.")
    }
}

impl<'tcx> QueryDescription<'tcx> for queries::analysis<'tcx> {
    fn describe(_tcx: TyCtxt<'_>, _: CrateNum) -> Cow<'static, str> {
        "running analysis passes on this crate".into()
    }
}
