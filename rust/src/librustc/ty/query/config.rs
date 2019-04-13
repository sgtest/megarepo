use crate::dep_graph::SerializedDepNodeIndex;
use crate::dep_graph::DepNode;
use crate::hir::def_id::{CrateNum, DefId};
use crate::ty::TyCtxt;
use crate::ty::query::queries;
use crate::ty::query::{Query, QueryName};
use crate::ty::query::QueryCache;
use crate::ty::query::plumbing::CycleError;
use crate::util::profiling::ProfileCategory;

use std::borrow::Cow;
use std::hash::Hash;
use std::fmt::Debug;
use rustc_data_structures::sync::Lock;
use rustc_data_structures::fingerprint::Fingerprint;
use crate::ich::StableHashingContext;

// Query configuration and description traits.

pub trait QueryConfig<'tcx> {
    const NAME: QueryName;
    const CATEGORY: ProfileCategory;

    type Key: Eq + Hash + Clone + Debug;
    type Value: Clone;
}

pub(crate) trait QueryAccessors<'tcx>: QueryConfig<'tcx> {
    fn query(key: Self::Key) -> Query<'tcx>;

    // Don't use this method to access query results, instead use the methods on TyCtxt
    fn query_cache<'a>(tcx: TyCtxt<'a, 'tcx, '_>) -> &'a Lock<QueryCache<'tcx, Self>>;

    fn to_dep_node(tcx: TyCtxt<'_, 'tcx, '_>, key: &Self::Key) -> DepNode;

    // Don't use this method to compute query results, instead use the methods on TyCtxt
    fn compute(tcx: TyCtxt<'_, 'tcx, '_>, key: Self::Key) -> Self::Value;

    fn hash_result(
        hcx: &mut StableHashingContext<'_>,
        result: &Self::Value
    ) -> Option<Fingerprint>;

    fn handle_cycle_error(tcx: TyCtxt<'_, 'tcx, '_>, error: CycleError<'tcx>) -> Self::Value;
}

pub(crate) trait QueryDescription<'tcx>: QueryAccessors<'tcx> {
    fn describe(tcx: TyCtxt<'_, '_, '_>, key: Self::Key) -> Cow<'static, str>;

    #[inline]
    fn cache_on_disk(_: TyCtxt<'_, 'tcx, 'tcx>, _: Self::Key) -> bool {
        false
    }

    fn try_load_from_disk(_: TyCtxt<'_, 'tcx, 'tcx>,
                          _: SerializedDepNodeIndex)
                          -> Option<Self::Value> {
        bug!("QueryDescription::load_from_disk() called for an unsupported query.")
    }
}

impl<'tcx, M: QueryAccessors<'tcx, Key=DefId>> QueryDescription<'tcx> for M {
    default fn describe(tcx: TyCtxt<'_, '_, '_>, def_id: DefId) -> Cow<'static, str> {
        if !tcx.sess.verbose() {
            format!("processing `{}`", tcx.def_path_str(def_id)).into()
        } else {
            let name = unsafe { ::std::intrinsics::type_name::<M>() };
            format!("processing {:?} with query `{}`", def_id, name).into()
        }
    }
}

impl<'tcx> QueryDescription<'tcx> for queries::analysis<'tcx> {
    fn describe(_tcx: TyCtxt<'_, '_, '_>, _: CrateNum) -> Cow<'static, str> {
        "running analysis passes on this crate".into()
    }
}

macro_rules! impl_disk_cacheable_query(
    ($query_name:ident, |$tcx:tt, $key:tt| $cond:expr) => {
        impl<'tcx> QueryDescription<'tcx> for queries::$query_name<'tcx> {
            #[inline]
            fn cache_on_disk($tcx: TyCtxt<'_, 'tcx, 'tcx>, $key: Self::Key) -> bool {
                $cond
            }

            #[inline]
            fn try_load_from_disk<'a>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                      id: SerializedDepNodeIndex)
                                      -> Option<Self::Value> {
                tcx.queries.on_disk_cache.try_load_query_result(tcx, id)
            }
        }
    }
);

impl_disk_cacheable_query!(mir_borrowck, |tcx, def_id| {
    def_id.is_local() && tcx.is_closure(def_id)
});

impl_disk_cacheable_query!(unsafety_check_result, |_, def_id| def_id.is_local());
impl_disk_cacheable_query!(borrowck, |_, def_id| def_id.is_local());
impl_disk_cacheable_query!(check_match, |_, def_id| def_id.is_local());
impl_disk_cacheable_query!(predicates_of, |_, def_id| def_id.is_local());
impl_disk_cacheable_query!(used_trait_imports, |_, def_id| def_id.is_local());
impl_disk_cacheable_query!(codegen_fn_attrs, |_, _| true);
impl_disk_cacheable_query!(specialization_graph_of, |_, _| true);
