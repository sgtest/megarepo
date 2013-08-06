// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use middle::cfg::*;
use middle::graph;
use middle::typeck;
use middle::ty;
use std::hashmap::HashMap;
use syntax::ast;
use syntax::ast_util;
use syntax::opt_vec;

struct CFGBuilder {
    tcx: ty::ctxt,
    method_map: typeck::method_map,
    exit_map: HashMap<ast::NodeId, CFGIndex>,
    graph: CFGGraph,
    loop_scopes: ~[LoopScope],
}

struct LoopScope {
    loop_id: ast::NodeId,     // id of loop/while node
    continue_index: CFGIndex, // where to go on a `loop`
    break_index: CFGIndex,    // where to go on a `break
}

pub fn construct(tcx: ty::ctxt,
                 method_map: typeck::method_map,
                 blk: &ast::Block) -> CFG {
    let mut cfg_builder = CFGBuilder {
        exit_map: HashMap::new(),
        graph: graph::Graph::new(),
        tcx: tcx,
        method_map: method_map,
        loop_scopes: ~[]
    };
    let entry = cfg_builder.add_node(0, []);
    let exit = cfg_builder.block(blk, entry);
    let CFGBuilder {exit_map, graph, _} = cfg_builder;
    CFG {exit_map: exit_map,
         graph: graph,
         entry: entry,
         exit: exit}
}

impl CFGBuilder {
    fn block(&mut self, blk: &ast::Block, pred: CFGIndex) -> CFGIndex {
        let mut stmts_exit = pred;
        for &stmt in blk.stmts.iter() {
            stmts_exit = self.stmt(stmt, stmts_exit);
        }

        let expr_exit = self.opt_expr(blk.expr, stmts_exit);

        self.add_node(blk.id, [expr_exit])
    }

    fn stmt(&mut self, stmt: @ast::stmt, pred: CFGIndex) -> CFGIndex {
        match stmt.node {
            ast::stmt_decl(decl, _) => {
                self.decl(decl, pred)
            }

            ast::stmt_expr(expr, _) | ast::stmt_semi(expr, _) => {
                self.expr(expr, pred)
            }

            ast::stmt_mac(*) => {
                self.tcx.sess.span_bug(stmt.span, "unexpanded macro");
            }
        }
    }

    fn decl(&mut self, decl: @ast::decl, pred: CFGIndex) -> CFGIndex {
        match decl.node {
            ast::decl_local(local) => {
                let init_exit = self.opt_expr(local.init, pred);
                self.pat(local.pat, init_exit)
            }

            ast::decl_item(_) => {
                pred
            }
        }
    }

    fn pat(&mut self, pat: @ast::pat, pred: CFGIndex) -> CFGIndex {
        match pat.node {
            ast::pat_ident(_, _, None) |
            ast::pat_enum(_, None) |
            ast::pat_lit(*) |
            ast::pat_range(*) |
            ast::pat_wild => {
                self.add_node(pat.id, [pred])
            }

            ast::pat_box(subpat) |
            ast::pat_uniq(subpat) |
            ast::pat_region(subpat) |
            ast::pat_ident(_, _, Some(subpat)) => {
                let subpat_exit = self.pat(subpat, pred);
                self.add_node(pat.id, [subpat_exit])
            }

            ast::pat_enum(_, Some(ref subpats)) |
            ast::pat_tup(ref subpats) => {
                let pats_exit =
                    self.pats_all(subpats.iter().transform(|p| *p), pred);
                self.add_node(pat.id, [pats_exit])
            }

            ast::pat_struct(_, ref subpats, _) => {
                let pats_exit =
                    self.pats_all(subpats.iter().transform(|f| f.pat), pred);
                self.add_node(pat.id, [pats_exit])
            }

            ast::pat_vec(ref pre, ref vec, ref post) => {
                let pre_exit =
                    self.pats_all(pre.iter().transform(|p| *p), pred);
                let vec_exit =
                    self.pats_all(vec.iter().transform(|p| *p), pre_exit);
                let post_exit =
                    self.pats_all(post.iter().transform(|p| *p), vec_exit);
                self.add_node(pat.id, [post_exit])
            }
        }
    }

    fn pats_all<I: Iterator<@ast::pat>>(&mut self,
                                        pats: I,
                                        pred: CFGIndex) -> CFGIndex {
        //! Handles case where all of the patterns must match.
        let mut pats = pats;
        pats.fold(pred, |pred, pat| self.pat(pat, pred))
    }

    fn pats_any(&mut self,
                pats: &[@ast::pat],
                pred: CFGIndex) -> CFGIndex {
        //! Handles case where just one of the patterns must match.

        if pats.len() == 1 {
            self.pat(pats[0], pred)
        } else {
            let collect = self.add_dummy_node([]);
            for &pat in pats.iter() {
                let pat_exit = self.pat(pat, pred);
                self.add_contained_edge(pat_exit, collect);
            }
            collect
        }
    }

    fn expr(&mut self, expr: @ast::expr, pred: CFGIndex) -> CFGIndex {
        match expr.node {
            ast::expr_block(ref blk) => {
                let blk_exit = self.block(blk, pred);
                self.add_node(expr.id, [blk_exit])
            }

            ast::expr_if(cond, ref then, None) => {
                //
                //     [pred]
                //       |
                //       v 1
                //     [cond]
                //       |
                //      / \
                //     /   \
                //    v 2   *
                //  [then]  |
                //    |     |
                //    v 3   v 4
                //   [..expr..]
                //
                let cond_exit = self.expr(cond, pred);                // 1
                let then_exit = self.block(then, cond_exit);          // 2
                self.add_node(expr.id, [cond_exit, then_exit])        // 3,4
            }

            ast::expr_if(cond, ref then, Some(otherwise)) => {
                //
                //     [pred]
                //       |
                //       v 1
                //     [cond]
                //       |
                //      / \
                //     /   \
                //    v 2   v 3
                //  [then][otherwise]
                //    |     |
                //    v 4   v 5
                //   [..expr..]
                //
                let cond_exit = self.expr(cond, pred);                // 1
                let then_exit = self.block(then, cond_exit);          // 2
                let else_exit = self.expr(otherwise, cond_exit);      // 3
                self.add_node(expr.id, [then_exit, else_exit])        // 4, 5
            }

            ast::expr_while(cond, ref body) => {
                //
                //         [pred]
                //           |
                //           v 1
                //       [loopback] <--+ 5
                //           |         |
                //           v 2       |
                //   +-----[cond]      |
                //   |       |         |
                //   |       v 4       |
                //   |     [body] -----+
                //   v 3
                // [expr]
                //
                // Note that `break` and `loop` statements
                // may cause additional edges.

                // Is the condition considered part of the loop?
                let loopback = self.add_dummy_node([pred]);           // 1
                let cond_exit = self.expr(cond, loopback);            // 2
                let expr_exit = self.add_node(expr.id, [cond_exit]);  // 3
                self.loop_scopes.push(LoopScope {
                    loop_id: expr.id,
                    continue_index: loopback,
                    break_index: expr_exit
                });
                let body_exit = self.block(body, cond_exit);          // 4
                self.add_contained_edge(body_exit, loopback);         // 5
                expr_exit
            }

            ast::expr_for_loop(*) => fail!("non-desugared expr_for_loop"),

            ast::expr_loop(ref body, _) => {
                //
                //     [pred]
                //       |
                //       v 1
                //   [loopback] <---+
                //       |      4   |
                //       v 3        |
                //     [body] ------+
                //
                //     [expr] 2
                //
                // Note that `break` and `loop` statements
                // may cause additional edges.

                let loopback = self.add_dummy_node([pred]);           // 1
                let expr_exit = self.add_node(expr.id, []);           // 2
                self.loop_scopes.push(LoopScope {
                    loop_id: expr.id,
                    continue_index: loopback,
                    break_index: expr_exit,
                });
                let body_exit = self.block(body, loopback);           // 3
                self.add_contained_edge(body_exit, loopback);         // 4
                self.loop_scopes.pop();
                expr_exit
            }

            ast::expr_match(discr, ref arms) => {
                //
                //     [pred]
                //       |
                //       v 1
                //    [discr]
                //       |
                //       v 2
                //    [guard1]
                //      /  \
                //     |    \
                //     v 3  |
                //  [pat1]  |
                //     |
                //     v 4  |
                // [body1]  v
                //     |  [guard2]
                //     |    /   \
                //     | [body2] \
                //     |    |   ...
                //     |    |    |
                //     v 5  v    v
                //   [....expr....]
                //
                let discr_exit = self.expr(discr, pred);                 // 1

                let expr_exit = self.add_node(expr.id, []);
                let mut guard_exit = discr_exit;
                for arm in arms.iter() {
                    guard_exit = self.opt_expr(arm.guard, guard_exit); // 2
                    let pats_exit = self.pats_any(arm.pats, guard_exit); // 3
                    let body_exit = self.block(&arm.body, pats_exit);    // 4
                    self.add_contained_edge(body_exit, expr_exit);       // 5
                }
                expr_exit
            }

            ast::expr_binary(_, op, l, r) if ast_util::lazy_binop(op) => {
                //
                //     [pred]
                //       |
                //       v 1
                //      [l]
                //       |
                //      / \
                //     /   \
                //    v 2  *
                //   [r]   |
                //    |    |
                //    v 3  v 4
                //   [..exit..]
                //
                let l_exit = self.expr(l, pred);                         // 1
                let r_exit = self.expr(r, l_exit);                       // 2
                self.add_node(expr.id, [l_exit, r_exit])                 // 3,4
            }

            ast::expr_ret(v) => {
                let v_exit = self.opt_expr(v, pred);
                let loop_scope = self.loop_scopes[0];
                self.add_exiting_edge(expr, v_exit,
                                      loop_scope, loop_scope.break_index);
                self.add_node(expr.id, [])
            }

            ast::expr_break(label) => {
                let loop_scope = self.find_scope(expr, label);
                self.add_exiting_edge(expr, pred,
                                      loop_scope, loop_scope.break_index);
                self.add_node(expr.id, [])
            }

            ast::expr_again(label) => {
                let loop_scope = self.find_scope(expr, label);
                self.add_exiting_edge(expr, pred,
                                      loop_scope, loop_scope.continue_index);
                self.add_node(expr.id, [])
            }

            ast::expr_vec(ref elems, _) => {
                self.straightline(expr, pred, *elems)
            }

            ast::expr_call(func, ref args, _) => {
                self.call(expr, pred, func, *args)
            }

            ast::expr_method_call(_, rcvr, _, _, ref args, _) => {
                self.call(expr, pred, rcvr, *args)
            }

            ast::expr_index(_, l, r) |
            ast::expr_binary(_, _, l, r) if self.is_method_call(expr) => {
                self.call(expr, pred, l, [r])
            }

            ast::expr_unary(_, _, e) if self.is_method_call(expr) => {
                self.call(expr, pred, e, [])
            }

            ast::expr_tup(ref exprs) => {
                self.straightline(expr, pred, *exprs)
            }

            ast::expr_struct(_, ref fields, base) => {
                let base_exit = self.opt_expr(base, pred);
                let field_exprs: ~[@ast::expr] =
                    fields.iter().transform(|f| f.expr).collect();
                self.straightline(expr, base_exit, field_exprs)
            }

            ast::expr_repeat(elem, count, _) => {
                self.straightline(expr, pred, [elem, count])
            }

            ast::expr_assign(l, r) |
            ast::expr_assign_op(_, _, l, r) => {
                self.straightline(expr, pred, [r, l])
            }

            ast::expr_log(l, r) |
            ast::expr_index(_, l, r) |
            ast::expr_binary(_, _, l, r) => { // NB: && and || handled earlier
                self.straightline(expr, pred, [l, r])
            }

            ast::expr_addr_of(_, e) |
            ast::expr_do_body(e) |
            ast::expr_cast(e, _) |
            ast::expr_unary(_, _, e) |
            ast::expr_paren(e) |
            ast::expr_vstore(e, _) |
            ast::expr_field(e, _, _) => {
                self.straightline(expr, pred, [e])
            }

            ast::expr_mac(*) |
            ast::expr_inline_asm(*) |
            ast::expr_self |
            ast::expr_fn_block(*) |
            ast::expr_lit(*) |
            ast::expr_path(*) => {
                self.straightline(expr, pred, [])
            }
        }
    }

    fn call(&mut self,
            call_expr: @ast::expr,
            pred: CFGIndex,
            func_or_rcvr: @ast::expr,
            args: &[@ast::expr]) -> CFGIndex {
        let func_or_rcvr_exit = self.expr(func_or_rcvr, pred);
        self.straightline(call_expr, func_or_rcvr_exit, args)
    }

    fn exprs(&mut self,
             exprs: &[@ast::expr],
             pred: CFGIndex) -> CFGIndex {
        //! Constructs graph for `exprs` evaluated in order

        exprs.iter().fold(pred, |p, &e| self.expr(e, p))
    }

    fn opt_expr(&mut self,
                opt_expr: Option<@ast::expr>,
                pred: CFGIndex) -> CFGIndex {
        //! Constructs graph for `opt_expr` evaluated, if Some

        opt_expr.iter().fold(pred, |p, &e| self.expr(e, p))
    }

    fn straightline(&mut self,
                    expr: @ast::expr,
                    pred: CFGIndex,
                    subexprs: &[@ast::expr]) -> CFGIndex {
        //! Handles case of an expression that evaluates `subexprs` in order

        let subexprs_exit = self.exprs(subexprs, pred);
        self.add_node(expr.id, [subexprs_exit])
    }

    fn add_dummy_node(&mut self, preds: &[CFGIndex]) -> CFGIndex {
        self.add_node(0, preds)
    }

    fn add_node(&mut self, id: ast::NodeId, preds: &[CFGIndex]) -> CFGIndex {
        assert!(!self.exit_map.contains_key(&id));
        let node = self.graph.add_node(CFGNodeData {id: id});
        self.exit_map.insert(id, node);
        for &pred in preds.iter() {
            self.add_contained_edge(pred, node);
        }
        node
    }

    fn add_contained_edge(&mut self,
                          source: CFGIndex,
                          target: CFGIndex) {
        let data = CFGEdgeData {exiting_scopes: opt_vec::Empty};
        self.graph.add_edge(source, target, data);
    }

    fn add_exiting_edge(&mut self,
                        from_expr: @ast::expr,
                        from_index: CFGIndex,
                        to_loop: LoopScope,
                        to_index: CFGIndex) {
        let mut data = CFGEdgeData {exiting_scopes: opt_vec::Empty};
        let mut scope_id = from_expr.id;
        while scope_id != to_loop.loop_id {
            data.exiting_scopes.push(scope_id);
            scope_id = self.tcx.region_maps.encl_scope(scope_id);
        }
        self.graph.add_edge(from_index, to_index, data);
    }

    fn find_scope(&self,
                  expr: @ast::expr,
                  label: Option<ast::ident>) -> LoopScope {
        match label {
            None => {
                return *self.loop_scopes.last();
            }

            Some(_) => {
                match self.tcx.def_map.find(&expr.id) {
                    Some(&ast::def_label(loop_id)) => {
                        for l in self.loop_scopes.iter() {
                            if l.loop_id == loop_id {
                                return *l;
                            }
                        }
                        self.tcx.sess.span_bug(
                            expr.span,
                            fmt!("No loop scope for id %?", loop_id));
                    }

                    r => {
                        self.tcx.sess.span_bug(
                            expr.span,
                            fmt!("Bad entry `%?` in def_map for label", r));
                    }
                }
            }
        }
    }

    fn is_method_call(&self, expr: &ast::expr) -> bool {
        self.method_map.contains_key(&expr.id)
    }
}
