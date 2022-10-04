use rustc_hir as hir;
use rustc_infer::infer::TyCtxtInferExt;
use rustc_macros::LintDiagnostic;
use rustc_middle::ty::{self, fold::BottomUpFolder, Ty, TypeFoldable};
use rustc_span::Span;
use rustc_trait_selection::traits;
use rustc_trait_selection::traits::query::evaluate_obligation::InferCtxtExt;

use crate::{LateContext, LateLintPass, LintContext};

declare_lint! {
    /// The `opaque_hidden_inferred_bound` lint detects cases in which nested
    /// `impl Trait` in associated type bounds are not written generally enough
    /// to satisfy the bounds of the associated type.
    ///
    /// ### Explanation
    ///
    /// This functionality was removed in #97346, but then rolled back in #99860
    /// because it caused regressions.
    ///
    /// We plan on reintroducing this as a hard error, but in the mean time,
    /// this lint serves to warn and suggest fixes for any use-cases which rely
    /// on this behavior.
    ///
    /// ### Example
    ///
    /// ```
    /// trait Trait {
    ///     type Assoc: Send;
    /// }
    ///
    /// struct Struct;
    ///
    /// impl Trait for Struct {
    ///     type Assoc = i32;
    /// }
    ///
    /// fn test() -> impl Trait<Assoc = impl Sized> {
    ///     Struct
    /// }
    /// ```
    ///
    /// {{produces}}
    ///
    /// In this example, `test` declares that the associated type `Assoc` for
    /// `impl Trait` is `impl Sized`, which does not satisfy the `Send` bound
    /// on the associated type.
    ///
    /// Although the hidden type, `i32` does satisfy this bound, we do not
    /// consider the return type to be well-formed with this lint. It can be
    /// fixed by changing `impl Sized` into `impl Sized + Send`.
    pub OPAQUE_HIDDEN_INFERRED_BOUND,
    Warn,
    "detects the use of nested `impl Trait` types in associated type bounds that are not general enough"
}

declare_lint_pass!(OpaqueHiddenInferredBound => [OPAQUE_HIDDEN_INFERRED_BOUND]);

impl<'tcx> LateLintPass<'tcx> for OpaqueHiddenInferredBound {
    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx hir::Item<'tcx>) {
        let hir::ItemKind::OpaqueTy(_) = &item.kind else { return; };
        let def_id = item.def_id.def_id.to_def_id();
        cx.tcx.infer_ctxt().enter(|ref infcx| {
            // For every projection predicate in the opaque type's explicit bounds,
            // check that the type that we're assigning actually satisfies the bounds
            // of the associated type.
            for &(pred, pred_span) in cx.tcx.explicit_item_bounds(def_id) {
                // Liberate bound regions in the predicate since we
                // don't actually care about lifetimes in this check.
                let predicate = cx.tcx.liberate_late_bound_regions(
                    def_id,
                    pred.kind(),
                );
                let ty::PredicateKind::Projection(proj) = predicate else {
                    continue;
                };
                // Only check types, since those are the only things that may
                // have opaques in them anyways.
                let Some(proj_term) = proj.term.ty() else { continue };

                let proj_ty =
                    cx
                    .tcx
                    .mk_projection(proj.projection_ty.item_def_id, proj.projection_ty.substs);
                // For every instance of the projection type in the bounds,
                // replace them with the term we're assigning to the associated
                // type in our opaque type.
                let proj_replacer = &mut BottomUpFolder {
                    tcx: cx.tcx,
                    ty_op: |ty| if ty == proj_ty { proj_term } else { ty },
                    lt_op: |lt| lt,
                    ct_op: |ct| ct,
                };
                // For example, in `impl Trait<Assoc = impl Send>`, for all of the bounds on `Assoc`,
                // e.g. `type Assoc: OtherTrait`, replace `<impl Trait as Trait>::Assoc: OtherTrait`
                // with `impl Send: OtherTrait`.
                for assoc_pred_and_span in cx
                    .tcx
                    .bound_explicit_item_bounds(proj.projection_ty.item_def_id)
                    .transpose_iter()
                {
                    let assoc_pred_span = assoc_pred_and_span.0.1;
                    let assoc_pred = assoc_pred_and_span
                        .map_bound(|(pred, _)| *pred)
                        .subst(cx.tcx, &proj.projection_ty.substs)
                        .fold_with(proj_replacer);
                    let Ok(assoc_pred) = traits::fully_normalize(infcx, traits::ObligationCause::dummy(), cx.param_env, assoc_pred) else {
                        continue;
                    };
                    // If that predicate doesn't hold modulo regions (but passed during type-check),
                    // then we must've taken advantage of the hack in `project_and_unify_types` where
                    // we replace opaques with inference vars. Emit a warning!
                    if !infcx.predicate_must_hold_modulo_regions(&traits::Obligation::new(
                        traits::ObligationCause::dummy(),
                        cx.param_env,
                        assoc_pred,
                    )) {
                        // If it's a trait bound and an opaque that doesn't satisfy it,
                        // then we can emit a suggestion to add the bound.
                        let (suggestion, suggest_span) =
                            match (proj_term.kind(), assoc_pred.kind().skip_binder()) {
                                (ty::Opaque(def_id, _), ty::PredicateKind::Trait(trait_pred)) => (
                                    format!(" + {}", trait_pred.print_modifiers_and_trait_path()),
                                    Some(cx.tcx.def_span(def_id).shrink_to_hi()),
                                ),
                                _ => (String::new(), None),
                            };
                        cx.emit_spanned_lint(
                            OPAQUE_HIDDEN_INFERRED_BOUND,
                            pred_span,
                            OpaqueHiddenInferredBoundLint {
                                ty: cx.tcx.mk_opaque(def_id, ty::InternalSubsts::identity_for_item(cx.tcx, def_id)),
                                proj_ty: proj_term,
                                assoc_pred_span,
                                suggestion,
                                suggest_span,
                            },
                        );
                    }
                }
            }
        });
    }
}

#[derive(LintDiagnostic)]
#[diag(lint::opaque_hidden_inferred_bound)]
struct OpaqueHiddenInferredBoundLint<'tcx> {
    ty: Ty<'tcx>,
    proj_ty: Ty<'tcx>,
    #[label(lint::specifically)]
    assoc_pred_span: Span,
    #[suggestion_verbose(applicability = "machine-applicable", code = "{suggestion}")]
    suggest_span: Option<Span>,
    suggestion: String,
}
