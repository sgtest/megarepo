// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use driver::session::Session;
use middle::def::*;
use middle::resolve;
use middle::ty;
use middle::typeck;
use util::ppaux;

use syntax::ast::*;
use syntax::{ast_util, ast_map};
use syntax::visit::Visitor;
use syntax::visit;

struct CheckCrateVisitor<'a, 'tcx: 'a> {
    tcx: &'a ty::ctxt<'tcx>,
    in_const: bool
}

impl<'a, 'tcx> CheckCrateVisitor<'a, 'tcx> {
    fn with_const(&mut self, in_const: bool, f: |&mut CheckCrateVisitor<'a, 'tcx>|) {
        let was_const = self.in_const;
        self.in_const = in_const;
        f(self);
        self.in_const = was_const;
    }
    fn inside_const(&mut self, f: |&mut CheckCrateVisitor<'a, 'tcx>|) {
        self.with_const(true, f);
    }
    fn outside_const(&mut self, f: |&mut CheckCrateVisitor<'a, 'tcx>|) {
        self.with_const(false, f);
    }
}

impl<'a, 'tcx, 'v> Visitor<'v> for CheckCrateVisitor<'a, 'tcx> {
    fn visit_item(&mut self, i: &Item) {
        check_item(self, i);
    }
    fn visit_pat(&mut self, p: &Pat) {
        check_pat(self, p);
    }
    fn visit_expr(&mut self, ex: &Expr) {
        check_expr(self, ex);
    }
}

pub fn check_crate(tcx: &ty::ctxt) {
    visit::walk_crate(&mut CheckCrateVisitor { tcx: tcx, in_const: false },
                      tcx.map.krate());
    tcx.sess.abort_if_errors();
}

fn check_item(v: &mut CheckCrateVisitor, it: &Item) {
    match it.node {
        ItemStatic(_, _, ref ex) => {
            v.inside_const(|v| v.visit_expr(&**ex));
            check_item_recursion(&v.tcx.sess, &v.tcx.map, &v.tcx.def_map, it);
        }
        ItemEnum(ref enum_definition, _) => {
            for var in (*enum_definition).variants.iter() {
                for ex in var.node.disr_expr.iter() {
                    v.inside_const(|v| v.visit_expr(&**ex));
                }
            }
        }
        _ => v.outside_const(|v| visit::walk_item(v, it))
    }
}

fn check_pat(v: &mut CheckCrateVisitor, p: &Pat) {
    fn is_str(e: &Expr) -> bool {
        match e.node {
            ExprBox(_, ref expr) => {
                match expr.node {
                    ExprLit(ref lit) => ast_util::lit_is_str(&**lit),
                    _ => false,
                }
            }
            _ => false,
        }
    }
    match p.node {
        // Let through plain ~-string literals here
        PatLit(ref a) => if !is_str(&**a) { v.inside_const(|v| v.visit_expr(&**a)); },
        PatRange(ref a, ref b) => {
            if !is_str(&**a) { v.inside_const(|v| v.visit_expr(&**a)); }
            if !is_str(&**b) { v.inside_const(|v| v.visit_expr(&**b)); }
        }
        _ => v.outside_const(|v| visit::walk_pat(v, p))
    }
}

fn check_expr(v: &mut CheckCrateVisitor, e: &Expr) {
    if v.in_const {
        match e.node {
          ExprUnary(UnDeref, _) => { }
          ExprUnary(UnBox, _) | ExprUnary(UnUniq, _) => {
            span_err!(v.tcx.sess, e.span, E0010, "cannot do allocations in constant expressions");
            return;
          }
          ExprLit(ref lit) if ast_util::lit_is_str(&**lit) => {}
          ExprBinary(..) | ExprUnary(..) => {
            let method_call = typeck::MethodCall::expr(e.id);
            if v.tcx.method_map.borrow().contains_key(&method_call) {
                span_err!(v.tcx.sess, e.span, E0011,
                    "user-defined operators are not allowed in constant expressions");
            }
          }
          ExprLit(_) => (),
          ExprCast(_, _) => {
            let ety = ty::expr_ty(v.tcx, e);
            if !ty::type_is_numeric(ety) && !ty::type_is_unsafe_ptr(ety) {
                span_err!(v.tcx.sess, e.span, E0012,
                    "can not cast to `{}` in a constant expression",
                    ppaux::ty_to_string(v.tcx, ety)
                );
            }
          }
          ExprPath(ref pth) => {
            // NB: In the future you might wish to relax this slightly
            // to handle on-demand instantiation of functions via
            // foo::<bar> in a const. Currently that is only done on
            // a path in trans::callee that only works in block contexts.
            if !pth.segments.iter().all(|segment| segment.types.is_empty()) {
                span_err!(v.tcx.sess, e.span, E0013,
                    "paths in constants may only refer to items without type parameters");
            }
            match v.tcx.def_map.borrow().find(&e.id) {
              Some(&DefStatic(..)) |
              Some(&DefFn(_, _)) |
              Some(&DefVariant(_, _, _)) |
              Some(&DefStruct(_)) => { }

              Some(&def) => {
                debug!("(checking const) found bad def: {:?}", def);
                span_err!(v.tcx.sess, e.span, E0014,
                    "paths in constants may only refer to constants or functions");
              }
              None => {
                v.tcx.sess.span_bug(e.span, "unbound path in const?!");
              }
            }
          }
          ExprCall(ref callee, _) => {
            match v.tcx.def_map.borrow().find(&callee.id) {
                Some(&DefStruct(..)) => {}    // OK.
                Some(&DefVariant(..)) => {}    // OK.
                _ => {
                    span_err!(v.tcx.sess, e.span, E0015,
                      "function calls in constants are limited to struct and enum constructors");
                }
            }
          }
          ExprBlock(ref block) => {
            // Check all statements in the block
            for stmt in block.stmts.iter() {
                let block_span_err = |span|
                    span_err!(v.tcx.sess, span, E0016,
                        "blocks in constants are limited to items and tail expressions");
                match stmt.node {
                    StmtDecl(ref span, _) => {
                        match span.node {
                            DeclLocal(_) => block_span_err(span.span),

                            // Item statements are allowed
                            DeclItem(_) => {}
                        }
                    }
                    StmtExpr(ref expr, _) => block_span_err(expr.span),
                    StmtSemi(ref semi, _) => block_span_err(semi.span),
                    StmtMac(..) => v.tcx.sess.span_bug(e.span,
                        "unexpanded statement macro in const?!")
                }
            }
            match block.expr {
                Some(ref expr) => check_expr(v, &**expr),
                None => {}
            }
          }
          ExprVec(_) |
          ExprAddrOf(MutImmutable, _) |
          ExprParen(..) |
          ExprField(..) |
          ExprTupField(..) |
          ExprIndex(..) |
          ExprTup(..) |
          ExprRepeat(..) |
          ExprStruct(..) => { }
          ExprAddrOf(_, ref inner) => {
                match inner.node {
                    // Mutable slices are allowed.
                    ExprVec(_) => {}
                    _ => span_err!(v.tcx.sess, e.span, E0017,
                                   "references in constants may only refer to immutable values")

                }
          },

          _ => {
              span_err!(v.tcx.sess, e.span, E0019,
                  "constant contains unimplemented expression type");
              return;
          }
        }
    }
    visit::walk_expr(v, e);
}

struct CheckItemRecursionVisitor<'a, 'ast: 'a> {
    root_it: &'a Item,
    sess: &'a Session,
    ast_map: &'a ast_map::Map<'ast>,
    def_map: &'a resolve::DefMap,
    idstack: Vec<NodeId>
}

// Make sure a const item doesn't recursively refer to itself
// FIXME: Should use the dependency graph when it's available (#1356)
pub fn check_item_recursion<'a>(sess: &'a Session,
                                ast_map: &'a ast_map::Map,
                                def_map: &'a resolve::DefMap,
                                it: &'a Item) {

    let mut visitor = CheckItemRecursionVisitor {
        root_it: it,
        sess: sess,
        ast_map: ast_map,
        def_map: def_map,
        idstack: Vec::new()
    };
    visitor.visit_item(it);
}

impl<'a, 'ast, 'v> Visitor<'v> for CheckItemRecursionVisitor<'a, 'ast> {
    fn visit_item(&mut self, it: &Item) {
        if self.idstack.iter().any(|x| x == &(it.id)) {
            self.sess.span_fatal(self.root_it.span, "recursive constant");
        }
        self.idstack.push(it.id);
        visit::walk_item(self, it);
        self.idstack.pop();
    }

    fn visit_expr(&mut self, e: &Expr) {
        match e.node {
            ExprPath(..) => {
                match self.def_map.borrow().find(&e.id) {
                    Some(&DefStatic(def_id, _)) if
                            ast_util::is_local(def_id) => {
                        self.visit_item(&*self.ast_map.expect_item(def_id.node));
                    }
                    _ => ()
                }
            },
            _ => ()
        }
        visit::walk_expr(self, e);
    }
}
