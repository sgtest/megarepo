use crate::infer::canonical::{Canonical, CanonicalQueryResponse};
use rustc_middle::traits::query::NoSolution;
use rustc_middle::ty::{ParamEnvAnd, TyCtxt};

pub use rustc_middle::traits::query::type_op::Subtype;

impl<'tcx> super::QueryTypeOp<'tcx> for Subtype<'tcx> {
    type QueryResponse = ();

    fn try_fast_path(_tcx: TyCtxt<'tcx>, key: &ParamEnvAnd<'tcx, Self>) -> Option<()> {
        if key.value.sub == key.value.sup { Some(()) } else { None }
    }

    fn perform_query(
        tcx: TyCtxt<'tcx>,
        canonicalized: Canonical<'tcx, ParamEnvAnd<'tcx, Self>>,
    ) -> Result<CanonicalQueryResponse<'tcx, ()>, NoSolution> {
        tcx.type_op_subtype(canonicalized)
    }
}
