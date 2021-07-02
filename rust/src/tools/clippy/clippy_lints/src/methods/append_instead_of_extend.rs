use clippy_utils::diagnostics::span_lint_and_sugg;
use clippy_utils::source::snippet_with_applicability;
use clippy_utils::ty::{is_type_diagnostic_item, is_type_lang_item};
use if_chain::if_chain;
use rustc_errors::Applicability;
use rustc_hir::{Expr, ExprKind, LangItem};
use rustc_lint::LateContext;
use rustc_span::symbol::sym;

use super::APPEND_INSTEAD_OF_EXTEND;

pub(super) fn check(cx: &LateContext<'_>, expr: &Expr<'_>, recv: &Expr<'_>, arg: &Expr<'_>) {
    let ty = cx.typeck_results().expr_ty(recv).peel_refs();
    if_chain! {
        if is_type_diagnostic_item(cx, ty, sym::vec_type);
        //check source object
        if let ExprKind::MethodCall(src_method, _, [drain_vec, drain_arg], _) = &arg.kind;
        if src_method.ident.as_str() == "drain";
        if let src_ty = cx.typeck_results().expr_ty(drain_vec).peel_refs();
        if is_type_diagnostic_item(cx, src_ty, sym::vec_type);
        //check drain range
        if let src_ty_range = cx.typeck_results().expr_ty(drain_arg).peel_refs();
        if is_type_lang_item(cx, src_ty_range, LangItem::RangeFull);
        then {
            let mut applicability = Applicability::MachineApplicable;
            span_lint_and_sugg(
                cx,
                APPEND_INSTEAD_OF_EXTEND,
                expr.span,
                "use of `extend` instead of `append` for adding the full range of a second vector",
                "try this",
                format!(
                    "{}.append(&mut {})",
                    snippet_with_applicability(cx, recv.span, "..", &mut applicability),
                    snippet_with_applicability(cx, drain_vec.span, "..", &mut applicability)
                ),
                applicability,
            );
        }
    }
}
