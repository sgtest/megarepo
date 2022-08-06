//! Miscellaneous type-system utilities that are too small to deserve their own modules.

use crate::infer::InferCtxtExt as _;
use crate::traits::{self, ObligationCause};

use rustc_hir as hir;
use rustc_infer::infer::TyCtxtInferExt;
use rustc_middle::ty::{self, Ty, TyCtxt, TypeVisitable};

use crate::traits::error_reporting::InferCtxtExt;

#[derive(Clone)]
pub enum CopyImplementationError<'tcx> {
    InfrigingFields(Vec<(&'tcx ty::FieldDef, Ty<'tcx>)>),
    NotAnAdt,
    HasDestructor,
}

pub fn can_type_implement_copy<'tcx>(
    tcx: TyCtxt<'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    self_type: Ty<'tcx>,
    parent_cause: ObligationCause<'tcx>,
) -> Result<(), CopyImplementationError<'tcx>> {
    // FIXME: (@jroesch) float this code up
    tcx.infer_ctxt().enter(|infcx| {
        let (adt, substs) = match self_type.kind() {
            // These types used to have a builtin impl.
            // Now libcore provides that impl.
            ty::Uint(_)
            | ty::Int(_)
            | ty::Bool
            | ty::Float(_)
            | ty::Char
            | ty::RawPtr(..)
            | ty::Never
            | ty::Ref(_, _, hir::Mutability::Not)
            | ty::Array(..) => return Ok(()),

            ty::Adt(adt, substs) => (adt, substs),

            _ => return Err(CopyImplementationError::NotAnAdt),
        };

        let mut infringing = Vec::new();
        for variant in adt.variants() {
            for field in &variant.fields {
                let ty = field.ty(tcx, substs);
                if ty.references_error() {
                    continue;
                }
                let span = tcx.def_span(field.did);
                // FIXME(compiler-errors): This gives us better spans for bad
                // projection types like in issue-50480.
                // If the ADT has substs, point to the cause we are given.
                // If it does not, then this field probably doesn't normalize
                // to begin with, and point to the bad field's span instead.
                let cause = if field
                    .ty(tcx, traits::InternalSubsts::identity_for_item(tcx, adt.did()))
                    .has_param_types_or_consts()
                {
                    parent_cause.clone()
                } else {
                    ObligationCause::dummy_with_span(span)
                };
                match traits::fully_normalize(&infcx, cause, param_env, ty) {
                    Ok(ty) => {
                        if !infcx.type_is_copy_modulo_regions(param_env, ty, span) {
                            infringing.push((field, ty));
                        }
                    }
                    Err(errors) => {
                        infcx.report_fulfillment_errors(&errors, None, false);
                    }
                };
            }
        }
        if !infringing.is_empty() {
            return Err(CopyImplementationError::InfrigingFields(infringing));
        }
        if adt.has_dtor(tcx) {
            return Err(CopyImplementationError::HasDestructor);
        }

        Ok(())
    })
}
