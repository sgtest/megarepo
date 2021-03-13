use crate::utils::{is_type_diagnostic_item, match_qpath, paths, snippet, span_lint_and_sugg};
use rustc_errors::Applicability;
use rustc_hir as hir;
use rustc_lint::LateContext;
use rustc_span::symbol::sym;

use super::OPTION_MAP_OR_NONE;
use super::RESULT_MAP_OR_INTO_OPTION;

/// lint use of `_.map_or(None, _)` for `Option`s and `Result`s
pub(super) fn check<'tcx>(cx: &LateContext<'tcx>, expr: &'tcx hir::Expr<'_>, map_or_args: &'tcx [hir::Expr<'_>]) {
    let is_option = is_type_diagnostic_item(cx, cx.typeck_results().expr_ty(&map_or_args[0]), sym::option_type);
    let is_result = is_type_diagnostic_item(cx, cx.typeck_results().expr_ty(&map_or_args[0]), sym::result_type);

    // There are two variants of this `map_or` lint:
    // (1) using `map_or` as an adapter from `Result<T,E>` to `Option<T>`
    // (2) using `map_or` as a combinator instead of `and_then`
    //
    // (For this lint) we don't care if any other type calls `map_or`
    if !is_option && !is_result {
        return;
    }

    let (lint_name, msg, instead, hint) = {
        let default_arg_is_none = if let hir::ExprKind::Path(ref qpath) = map_or_args[1].kind {
            match_qpath(qpath, &paths::OPTION_NONE)
        } else {
            return;
        };

        if !default_arg_is_none {
            // nothing to lint!
            return;
        }

        let f_arg_is_some = if let hir::ExprKind::Path(ref qpath) = map_or_args[2].kind {
            match_qpath(qpath, &paths::OPTION_SOME)
        } else {
            false
        };

        if is_option {
            let self_snippet = snippet(cx, map_or_args[0].span, "..");
            let func_snippet = snippet(cx, map_or_args[2].span, "..");
            let msg = "called `map_or(None, ..)` on an `Option` value. This can be done more directly by calling \
                       `and_then(..)` instead";
            (
                OPTION_MAP_OR_NONE,
                msg,
                "try using `and_then` instead",
                format!("{0}.and_then({1})", self_snippet, func_snippet),
            )
        } else if f_arg_is_some {
            let msg = "called `map_or(None, Some)` on a `Result` value. This can be done more directly by calling \
                       `ok()` instead";
            let self_snippet = snippet(cx, map_or_args[0].span, "..");
            (
                RESULT_MAP_OR_INTO_OPTION,
                msg,
                "try using `ok` instead",
                format!("{0}.ok()", self_snippet),
            )
        } else {
            // nothing to lint!
            return;
        }
    };

    span_lint_and_sugg(
        cx,
        lint_name,
        expr.span,
        msg,
        instead,
        hint,
        Applicability::MachineApplicable,
    );
}
