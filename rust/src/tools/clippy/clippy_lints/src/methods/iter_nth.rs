use crate::methods::derefs_to_slice;
use crate::methods::iter_nth_zero;
use crate::utils::{is_type_diagnostic_item, span_lint_and_help};
use rustc_hir as hir;
use rustc_lint::LateContext;
use rustc_span::symbol::sym;

use super::ITER_NTH;

pub(super) fn check<'tcx>(
    cx: &LateContext<'tcx>,
    expr: &hir::Expr<'_>,
    nth_and_iter_args: &[&'tcx [hir::Expr<'tcx>]],
    is_mut: bool,
) {
    let iter_args = nth_and_iter_args[1];
    let mut_str = if is_mut { "_mut" } else { "" };
    let caller_type = if derefs_to_slice(cx, &iter_args[0], cx.typeck_results().expr_ty(&iter_args[0])).is_some() {
        "slice"
    } else if is_type_diagnostic_item(cx, cx.typeck_results().expr_ty(&iter_args[0]), sym::vec_type) {
        "Vec"
    } else if is_type_diagnostic_item(cx, cx.typeck_results().expr_ty(&iter_args[0]), sym::vecdeque_type) {
        "VecDeque"
    } else {
        let nth_args = nth_and_iter_args[0];
        iter_nth_zero::check(cx, expr, &nth_args);
        return; // caller is not a type that we want to lint
    };

    span_lint_and_help(
        cx,
        ITER_NTH,
        expr.span,
        &format!("called `.iter{0}().nth()` on a {1}", mut_str, caller_type),
        None,
        &format!("calling `.get{}()` is both faster and more readable", mut_str),
    );
}
