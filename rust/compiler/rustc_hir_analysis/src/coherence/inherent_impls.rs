//! The code in this module gathers up all of the inherent impls in
//! the current crate and organizes them in a map. It winds up
//! touching the whole crate and thus must be recomputed completely
//! for any change, but it is very cheap to compute. In practice, most
//! code in the compiler never *directly* requests this map. Instead,
//! it requests the inherent impls specific to some type (via
//! `tcx.inherent_impls(def_id)`). That value, however,
//! is computed by selecting an idea from this table.

use rustc_hir as hir;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_middle::ty::fast_reject::{simplify_type, SimplifiedType, TreatParams};
use rustc_middle::ty::{self, CrateInherentImpls, Ty, TyCtxt};
use rustc_span::symbol::sym;

use crate::errors;

/// On-demand query: yields a map containing all types mapped to their inherent impls.
pub fn crate_inherent_impls(tcx: TyCtxt<'_>, (): ()) -> CrateInherentImpls {
    let mut collect = InherentCollect { tcx, impls_map: Default::default() };
    for id in tcx.hir().items() {
        collect.check_item(id);
    }
    collect.impls_map
}

pub fn crate_incoherent_impls(tcx: TyCtxt<'_>, simp: SimplifiedType) -> &[DefId] {
    let crate_map = tcx.crate_inherent_impls(());
    tcx.arena.alloc_from_iter(
        crate_map.incoherent_impls.get(&simp).unwrap_or(&Vec::new()).iter().map(|d| d.to_def_id()),
    )
}

/// On-demand query: yields a vector of the inherent impls for a specific type.
pub fn inherent_impls(tcx: TyCtxt<'_>, ty_def_id: LocalDefId) -> &[DefId] {
    let crate_map = tcx.crate_inherent_impls(());
    match crate_map.inherent_impls.get(&ty_def_id) {
        Some(v) => &v[..],
        None => &[],
    }
}

struct InherentCollect<'tcx> {
    tcx: TyCtxt<'tcx>,
    impls_map: CrateInherentImpls,
}

impl<'tcx> InherentCollect<'tcx> {
    fn check_def_id(&mut self, impl_def_id: LocalDefId, self_ty: Ty<'tcx>, ty_def_id: DefId) {
        if let Some(ty_def_id) = ty_def_id.as_local() {
            // Add the implementation to the mapping from implementation to base
            // type def ID, if there is a base type for this implementation and
            // the implementation does not have any associated traits.
            let vec = self.impls_map.inherent_impls.entry(ty_def_id).or_default();
            vec.push(impl_def_id.to_def_id());
            return;
        }

        if self.tcx.features().rustc_attrs {
            let items = self.tcx.associated_item_def_ids(impl_def_id);

            if !self.tcx.has_attr(ty_def_id, sym::rustc_has_incoherent_inherent_impls) {
                let impl_span = self.tcx.def_span(impl_def_id);
                self.tcx.dcx().emit_err(errors::InherentTyOutside { span: impl_span });
                return;
            }

            for &impl_item in items {
                if !self.tcx.has_attr(impl_item, sym::rustc_allow_incoherent_impl) {
                    let impl_span = self.tcx.def_span(impl_def_id);
                    self.tcx.dcx().emit_err(errors::InherentTyOutsideRelevant {
                        span: impl_span,
                        help_span: self.tcx.def_span(impl_item),
                    });
                    return;
                }
            }

            if let Some(simp) = simplify_type(self.tcx, self_ty, TreatParams::AsCandidateKey) {
                self.impls_map.incoherent_impls.entry(simp).or_default().push(impl_def_id);
            } else {
                bug!("unexpected self type: {:?}", self_ty);
            }
        } else {
            let impl_span = self.tcx.def_span(impl_def_id);
            self.tcx.dcx().emit_err(errors::InherentTyOutsideNew { span: impl_span });
        }
    }

    fn check_primitive_impl(&mut self, impl_def_id: LocalDefId, ty: Ty<'tcx>) {
        let items = self.tcx.associated_item_def_ids(impl_def_id);
        if !self.tcx.hir().rustc_coherence_is_core() {
            if self.tcx.features().rustc_attrs {
                for &impl_item in items {
                    if !self.tcx.has_attr(impl_item, sym::rustc_allow_incoherent_impl) {
                        let span = self.tcx.def_span(impl_def_id);
                        self.tcx.dcx().emit_err(errors::InherentTyOutsidePrimitive {
                            span,
                            help_span: self.tcx.def_span(impl_item),
                        });
                        return;
                    }
                }
            } else {
                let span = self.tcx.def_span(impl_def_id);
                let mut note = None;
                if let ty::Ref(_, subty, _) = ty.kind() {
                    note = Some(errors::InherentPrimitiveTyNote { subty: *subty });
                }
                self.tcx.dcx().emit_err(errors::InherentPrimitiveTy { span, note });
                return;
            }
        }

        if let Some(simp) = simplify_type(self.tcx, ty, TreatParams::AsCandidateKey) {
            self.impls_map.incoherent_impls.entry(simp).or_default().push(impl_def_id);
        } else {
            bug!("unexpected primitive type: {:?}", ty);
        }
    }

    fn check_item(&mut self, id: hir::ItemId) {
        if !matches!(self.tcx.def_kind(id.owner_id), DefKind::Impl { of_trait: false }) {
            return;
        }

        let id = id.owner_id.def_id;
        let item_span = self.tcx.def_span(id);
        let self_ty = self.tcx.type_of(id).instantiate_identity();
        match *self_ty.kind() {
            ty::Adt(def, _) => self.check_def_id(id, self_ty, def.did()),
            ty::Foreign(did) => self.check_def_id(id, self_ty, did),
            ty::Dynamic(data, ..) if data.principal_def_id().is_some() => {
                self.check_def_id(id, self_ty, data.principal_def_id().unwrap());
            }
            ty::Dynamic(..) => {
                self.tcx.dcx().emit_err(errors::InherentDyn { span: item_span });
            }
            ty::Bool
            | ty::Char
            | ty::Int(_)
            | ty::Uint(_)
            | ty::Float(_)
            | ty::Str
            | ty::Array(..)
            | ty::Slice(_)
            | ty::RawPtr(_)
            | ty::Ref(..)
            | ty::Never
            | ty::FnPtr(_)
            | ty::Tuple(..) => self.check_primitive_impl(id, self_ty),
            ty::Alias(..) | ty::Param(_) => {
                self.tcx.dcx().emit_err(errors::InherentNominal { span: item_span });
            }
            ty::FnDef(..)
            | ty::Closure(..)
            | ty::Coroutine(..)
            | ty::CoroutineWitness(..)
            | ty::Bound(..)
            | ty::Placeholder(_)
            | ty::Infer(_) => {
                bug!("unexpected impl self type of impl: {:?} {:?}", id, self_ty);
            }
            ty::Error(_) => {}
        }
    }
}
