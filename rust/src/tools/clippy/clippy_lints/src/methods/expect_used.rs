use crate::utils::{is_type_diagnostic_item, span_lint_and_help};
use rustc_hir as hir;
use rustc_lint::LateContext;
use rustc_span::sym;

use super::EXPECT_USED;

/// lint use of `expect()` for `Option`s and `Result`s
pub(super) fn check(cx: &LateContext<'_>, expr: &hir::Expr<'_>, expect_args: &[hir::Expr<'_>]) {
    let obj_ty = cx.typeck_results().expr_ty(&expect_args[0]).peel_refs();

    let mess = if is_type_diagnostic_item(cx, obj_ty, sym::option_type) {
        Some((EXPECT_USED, "an Option", "None"))
    } else if is_type_diagnostic_item(cx, obj_ty, sym::result_type) {
        Some((EXPECT_USED, "a Result", "Err"))
    } else {
        None
    };

    if let Some((lint, kind, none_value)) = mess {
        span_lint_and_help(
            cx,
            lint,
            expr.span,
            &format!("used `expect()` on `{}` value", kind,),
            None,
            &format!("if this value is an `{}`, it will panic", none_value,),
        );
    }
}
