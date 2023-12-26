// Not in interpret to make sure we do not use private implementation details

use crate::errors::MaxNumNodesInConstErr;
use crate::interpret::InterpCx;
use rustc_middle::mir;
use rustc_middle::mir::interpret::{EvalToValTreeResult, GlobalId};
use rustc_middle::query::TyCtxtAt;
use rustc_middle::ty::{self, Ty, TyCtxt};
use rustc_span::DUMMY_SP;

mod error;
mod eval_queries;
mod fn_queries;
mod machine;
mod valtrees;

pub use error::*;
pub use eval_queries::*;
pub use fn_queries::*;
pub use machine::*;
pub(crate) use valtrees::{const_to_valtree_inner, valtree_to_const_value};

// We forbid type-level constants that contain more than `VALTREE_MAX_NODES` nodes.
const VALTREE_MAX_NODES: usize = 100000;

pub(crate) enum ValTreeCreationError {
    NodesOverflow,
    NonSupportedType,
    Other,
}
pub(crate) type ValTreeCreationResult<'tcx> = Result<ty::ValTree<'tcx>, ValTreeCreationError>;

/// Evaluates a constant and turns it into a type-level constant value.
pub(crate) fn eval_to_valtree<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    cid: GlobalId<'tcx>,
) -> EvalToValTreeResult<'tcx> {
    let const_alloc = tcx.eval_to_allocation_raw(param_env.and(cid))?;

    // FIXME Need to provide a span to `eval_to_valtree`
    let ecx = mk_eval_cx(
        tcx,
        DUMMY_SP,
        param_env,
        // It is absolutely crucial for soundness that
        // we do not read from static items or other mutable memory.
        CanAccessStatics::No,
    );
    let place = ecx.raw_const_to_mplace(const_alloc).unwrap();
    debug!(?place);

    let mut num_nodes = 0;
    let valtree_result = const_to_valtree_inner(&ecx, &place, &mut num_nodes);

    match valtree_result {
        Ok(valtree) => Ok(Some(valtree)),
        Err(err) => {
            let did = cid.instance.def_id();
            let global_const_id = cid.display(tcx);
            match err {
                ValTreeCreationError::NodesOverflow => {
                    let span = tcx.hir().span_if_local(did);
                    tcx.dcx().emit_err(MaxNumNodesInConstErr { span, global_const_id });

                    Ok(None)
                }
                ValTreeCreationError::NonSupportedType | ValTreeCreationError::Other => Ok(None),
            }
        }
    }
}

#[instrument(skip(tcx), level = "debug")]
pub(crate) fn try_destructure_mir_constant_for_user_output<'tcx>(
    tcx: TyCtxtAt<'tcx>,
    val: mir::ConstValue<'tcx>,
    ty: Ty<'tcx>,
) -> Option<mir::DestructuredConstant<'tcx>> {
    let param_env = ty::ParamEnv::reveal_all();
    let ecx = mk_eval_cx(tcx.tcx, tcx.span, param_env, CanAccessStatics::No);
    let op = ecx.const_val_to_op(val, ty, None).ok()?;

    // We go to `usize` as we cannot allocate anything bigger anyway.
    let (field_count, variant, down) = match ty.kind() {
        ty::Array(_, len) => (len.eval_target_usize(tcx.tcx, param_env) as usize, None, op),
        ty::Adt(def, _) if def.variants().is_empty() => {
            return None;
        }
        ty::Adt(def, _) => {
            let variant = ecx.read_discriminant(&op).ok()?;
            let down = ecx.project_downcast(&op, variant).ok()?;
            (def.variants()[variant].fields.len(), Some(variant), down)
        }
        ty::Tuple(args) => (args.len(), None, op),
        _ => bug!("cannot destructure mir constant {:?}", val),
    };

    let fields_iter = (0..field_count)
        .map(|i| {
            let field_op = ecx.project_field(&down, i).ok()?;
            let val = op_to_const(&ecx, &field_op, /* for diagnostics */ true);
            Some((val, field_op.layout.ty))
        })
        .collect::<Option<Vec<_>>>()?;
    let fields = tcx.arena.alloc_from_iter(fields_iter);

    Some(mir::DestructuredConstant { variant, fields })
}
