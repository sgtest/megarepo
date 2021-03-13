use crate::utils::{
    get_parent_expr, match_trait_method, paths, snippet_with_applicability, span_lint_and_sugg, walk_ptrs_ty_depth,
};
use if_chain::if_chain;
use rustc_errors::Applicability;
use rustc_hir as hir;
use rustc_lint::LateContext;

use super::USELESS_ASREF;

/// Checks for the `USELESS_ASREF` lint.
pub(super) fn check(cx: &LateContext<'_>, expr: &hir::Expr<'_>, call_name: &str, as_ref_args: &[hir::Expr<'_>]) {
    // when we get here, we've already checked that the call name is "as_ref" or "as_mut"
    // check if the call is to the actual `AsRef` or `AsMut` trait
    if match_trait_method(cx, expr, &paths::ASREF_TRAIT) || match_trait_method(cx, expr, &paths::ASMUT_TRAIT) {
        // check if the type after `as_ref` or `as_mut` is the same as before
        let recvr = &as_ref_args[0];
        let rcv_ty = cx.typeck_results().expr_ty(recvr);
        let res_ty = cx.typeck_results().expr_ty(expr);
        let (base_res_ty, res_depth) = walk_ptrs_ty_depth(res_ty);
        let (base_rcv_ty, rcv_depth) = walk_ptrs_ty_depth(rcv_ty);
        if base_rcv_ty == base_res_ty && rcv_depth >= res_depth {
            // allow the `as_ref` or `as_mut` if it is followed by another method call
            if_chain! {
                if let Some(parent) = get_parent_expr(cx, expr);
                if let hir::ExprKind::MethodCall(_, ref span, _, _) = parent.kind;
                if span != &expr.span;
                then {
                    return;
                }
            }

            let mut applicability = Applicability::MachineApplicable;
            span_lint_and_sugg(
                cx,
                USELESS_ASREF,
                expr.span,
                &format!("this call to `{}` does nothing", call_name),
                "try this",
                snippet_with_applicability(cx, recvr.span, "..", &mut applicability).to_string(),
                applicability,
            );
        }
    }
}
