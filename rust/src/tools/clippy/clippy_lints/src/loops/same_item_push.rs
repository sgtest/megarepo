use super::SAME_ITEM_PUSH;
use crate::utils::{implements_trait, is_type_diagnostic_item, snippet_with_macro_callsite, span_lint_and_help};
use if_chain::if_chain;
use rustc_hir::def::{DefKind, Res};
use rustc_hir::intravisit::{walk_expr, NestedVisitorMap, Visitor};
use rustc_hir::{BindingAnnotation, Block, Expr, ExprKind, Node, Pat, PatKind, Stmt, StmtKind};
use rustc_lint::LateContext;
use rustc_middle::hir::map::Map;
use rustc_span::symbol::sym;
use std::iter::Iterator;

/// Detects for loop pushing the same item into a Vec
pub(super) fn check<'tcx>(
    cx: &LateContext<'tcx>,
    pat: &'tcx Pat<'_>,
    _: &'tcx Expr<'_>,
    body: &'tcx Expr<'_>,
    _: &'tcx Expr<'_>,
) {
    fn emit_lint(cx: &LateContext<'_>, vec: &Expr<'_>, pushed_item: &Expr<'_>) {
        let vec_str = snippet_with_macro_callsite(cx, vec.span, "");
        let item_str = snippet_with_macro_callsite(cx, pushed_item.span, "");

        span_lint_and_help(
            cx,
            SAME_ITEM_PUSH,
            vec.span,
            "it looks like the same item is being pushed into this Vec",
            None,
            &format!(
                "try using vec![{};SIZE] or {}.resize(NEW_SIZE, {})",
                item_str, vec_str, item_str
            ),
        )
    }

    if !matches!(pat.kind, PatKind::Wild) {
        return;
    }

    // Determine whether it is safe to lint the body
    let mut same_item_push_visitor = SameItemPushVisitor {
        should_lint: true,
        vec_push: None,
        cx,
    };
    walk_expr(&mut same_item_push_visitor, body);
    if same_item_push_visitor.should_lint {
        if let Some((vec, pushed_item)) = same_item_push_visitor.vec_push {
            let vec_ty = cx.typeck_results().expr_ty(vec);
            let ty = vec_ty.walk().nth(1).unwrap().expect_ty();
            if cx
                .tcx
                .lang_items()
                .clone_trait()
                .map_or(false, |id| implements_trait(cx, ty, id, &[]))
            {
                // Make sure that the push does not involve possibly mutating values
                match pushed_item.kind {
                    ExprKind::Path(ref qpath) => {
                        match cx.qpath_res(qpath, pushed_item.hir_id) {
                            // immutable bindings that are initialized with literal or constant
                            Res::Local(hir_id) => {
                                if_chain! {
                                    let node = cx.tcx.hir().get(hir_id);
                                    if let Node::Binding(pat) = node;
                                    if let PatKind::Binding(bind_ann, ..) = pat.kind;
                                    if !matches!(bind_ann, BindingAnnotation::RefMut | BindingAnnotation::Mutable);
                                    let parent_node = cx.tcx.hir().get_parent_node(hir_id);
                                    if let Some(Node::Local(parent_let_expr)) = cx.tcx.hir().find(parent_node);
                                    if let Some(init) = parent_let_expr.init;
                                    then {
                                        match init.kind {
                                            // immutable bindings that are initialized with literal
                                            ExprKind::Lit(..) => emit_lint(cx, vec, pushed_item),
                                            // immutable bindings that are initialized with constant
                                            ExprKind::Path(ref path) => {
                                                if let Res::Def(DefKind::Const, ..) = cx.qpath_res(path, init.hir_id) {
                                                    emit_lint(cx, vec, pushed_item);
                                                }
                                            }
                                            _ => {},
                                        }
                                    }
                                }
                            },
                            // constant
                            Res::Def(DefKind::Const, ..) => emit_lint(cx, vec, pushed_item),
                            _ => {},
                        }
                    },
                    ExprKind::Lit(..) => emit_lint(cx, vec, pushed_item),
                    _ => {},
                }
            }
        }
    }
}

// Scans the body of the for loop and determines whether lint should be given
struct SameItemPushVisitor<'a, 'tcx> {
    should_lint: bool,
    // this field holds the last vec push operation visited, which should be the only push seen
    vec_push: Option<(&'tcx Expr<'tcx>, &'tcx Expr<'tcx>)>,
    cx: &'a LateContext<'tcx>,
}

impl<'a, 'tcx> Visitor<'tcx> for SameItemPushVisitor<'a, 'tcx> {
    type Map = Map<'tcx>;

    fn visit_expr(&mut self, expr: &'tcx Expr<'_>) {
        match &expr.kind {
            // Non-determinism may occur ... don't give a lint
            ExprKind::Loop(..) | ExprKind::Match(..) => self.should_lint = false,
            ExprKind::Block(block, _) => self.visit_block(block),
            _ => {},
        }
    }

    fn visit_block(&mut self, b: &'tcx Block<'_>) {
        for stmt in b.stmts.iter() {
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, s: &'tcx Stmt<'_>) {
        let vec_push_option = get_vec_push(self.cx, s);
        if vec_push_option.is_none() {
            // Current statement is not a push so visit inside
            match &s.kind {
                StmtKind::Expr(expr) | StmtKind::Semi(expr) => self.visit_expr(&expr),
                _ => {},
            }
        } else {
            // Current statement is a push ...check whether another
            // push had been previously done
            if self.vec_push.is_none() {
                self.vec_push = vec_push_option;
            } else {
                // There are multiple pushes ... don't lint
                self.should_lint = false;
            }
        }
    }

    fn nested_visit_map(&mut self) -> NestedVisitorMap<Self::Map> {
        NestedVisitorMap::None
    }
}

// Given some statement, determine if that statement is a push on a Vec. If it is, return
// the Vec being pushed into and the item being pushed
fn get_vec_push<'tcx>(cx: &LateContext<'tcx>, stmt: &'tcx Stmt<'_>) -> Option<(&'tcx Expr<'tcx>, &'tcx Expr<'tcx>)> {
    if_chain! {
            // Extract method being called
            if let StmtKind::Semi(semi_stmt) = &stmt.kind;
            if let ExprKind::MethodCall(path, _, args, _) = &semi_stmt.kind;
            // Figure out the parameters for the method call
            if let Some(self_expr) = args.get(0);
            if let Some(pushed_item) = args.get(1);
            // Check that the method being called is push() on a Vec
            if is_type_diagnostic_item(cx, cx.typeck_results().expr_ty(self_expr), sym::vec_type);
            if path.ident.name.as_str() == "push";
            then {
                return Some((self_expr, pushed_item))
            }
    }
    None
}
