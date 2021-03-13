use crate::utils::{match_trait_method, meets_msrv, paths, snippet, span_lint, span_lint_and_sugg};
use rustc_errors::Applicability;
use rustc_hir as hir;
use rustc_lint::LateContext;
use rustc_semver::RustcVersion;

use super::FILTER_MAP_NEXT;

const FILTER_MAP_NEXT_MSRV: RustcVersion = RustcVersion::new(1, 30, 0);

pub(super) fn check<'tcx>(
    cx: &LateContext<'tcx>,
    expr: &'tcx hir::Expr<'_>,
    filter_args: &'tcx [hir::Expr<'_>],
    msrv: Option<&RustcVersion>,
) {
    if match_trait_method(cx, expr, &paths::ITERATOR) {
        if !meets_msrv(msrv, &FILTER_MAP_NEXT_MSRV) {
            return;
        }

        let msg = "called `filter_map(..).next()` on an `Iterator`. This is more succinctly expressed by calling \
                   `.find_map(..)` instead";
        let filter_snippet = snippet(cx, filter_args[1].span, "..");
        if filter_snippet.lines().count() <= 1 {
            let iter_snippet = snippet(cx, filter_args[0].span, "..");
            span_lint_and_sugg(
                cx,
                FILTER_MAP_NEXT,
                expr.span,
                msg,
                "try this",
                format!("{}.find_map({})", iter_snippet, filter_snippet),
                Applicability::MachineApplicable,
            );
        } else {
            span_lint(cx, FILTER_MAP_NEXT, expr.span, msg);
        }
    }
}
