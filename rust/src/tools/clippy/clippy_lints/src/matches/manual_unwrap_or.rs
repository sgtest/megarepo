use clippy_utils::consts::constant_simple;
use clippy_utils::diagnostics::span_lint_and_sugg;
use clippy_utils::source::{indent_of, reindent_multiline, snippet_opt};
use clippy_utils::ty::is_type_diagnostic_item;
use clippy_utils::usage::contains_return_break_continue_macro;
use clippy_utils::{is_lang_ctor, path_to_local_id, sugg};
use if_chain::if_chain;
use rustc_errors::Applicability;
use rustc_hir::LangItem::{OptionNone, OptionSome, ResultErr, ResultOk};
use rustc_hir::{Arm, Expr, PatKind};
use rustc_lint::LateContext;
use rustc_span::sym;

use super::MANUAL_UNWRAP_OR;

pub(super) fn check<'tcx>(cx: &LateContext<'tcx>, expr: &Expr<'tcx>, scrutinee: &'tcx Expr<'_>, arms: &'tcx [Arm<'_>]) {
    let ty = cx.typeck_results().expr_ty(scrutinee);
    if_chain! {
        if let Some(ty_name) = if is_type_diagnostic_item(cx, ty, sym::Option) {
            Some("Option")
        } else if is_type_diagnostic_item(cx, ty, sym::Result) {
            Some("Result")
        } else {
            None
        };
        if let Some(or_arm) = applicable_or_arm(cx, arms);
        if let Some(or_body_snippet) = snippet_opt(cx, or_arm.body.span);
        if let Some(indent) = indent_of(cx, expr.span);
        if constant_simple(cx, cx.typeck_results(), or_arm.body).is_some();
        then {
            let reindented_or_body =
                reindent_multiline(or_body_snippet.into(), true, Some(indent));

            let suggestion = if scrutinee.span.from_expansion() {
                    // we don't want parentheses around macro, e.g. `(some_macro!()).unwrap_or(0)`
                    sugg::Sugg::hir_with_macro_callsite(cx, scrutinee, "..")
                }
                else {
                    sugg::Sugg::hir(cx, scrutinee, "..").maybe_par()
                };

            span_lint_and_sugg(
                cx,
                MANUAL_UNWRAP_OR, expr.span,
                &format!("this pattern reimplements `{}::unwrap_or`", ty_name),
                "replace with",
                format!(
                    "{}.unwrap_or({})",
                    suggestion,
                    reindented_or_body,
                ),
                Applicability::MachineApplicable,
            );
        }
    }
}

fn applicable_or_arm<'a>(cx: &LateContext<'_>, arms: &'a [Arm<'a>]) -> Option<&'a Arm<'a>> {
    if_chain! {
        if arms.len() == 2;
        if arms.iter().all(|arm| arm.guard.is_none());
        if let Some((idx, or_arm)) = arms.iter().enumerate().find(|(_, arm)| {
            match arm.pat.kind {
                PatKind::Path(ref qpath) => is_lang_ctor(cx, qpath, OptionNone),
                PatKind::TupleStruct(ref qpath, [pat], _) =>
                    matches!(pat.kind, PatKind::Wild) && is_lang_ctor(cx, qpath, ResultErr),
                _ => false,
            }
        });
        let unwrap_arm = &arms[1 - idx];
        if let PatKind::TupleStruct(ref qpath, [unwrap_pat], _) = unwrap_arm.pat.kind;
        if is_lang_ctor(cx, qpath, OptionSome) || is_lang_ctor(cx, qpath, ResultOk);
        if let PatKind::Binding(_, binding_hir_id, ..) = unwrap_pat.kind;
        if path_to_local_id(unwrap_arm.body, binding_hir_id);
        if cx.typeck_results().expr_adjustments(unwrap_arm.body).is_empty();
        if !contains_return_break_continue_macro(or_arm.body);
        then {
            Some(or_arm)
        } else {
            None
        }
    }
}
