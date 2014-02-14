// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Type resolution: the phase that finds all the types in the AST with
// unresolved type variables and replaces "ty_var" types with their
// substitutions.


use middle::pat_util;
use middle::ty;
use middle::typeck::astconv::AstConv;
use middle::typeck::check::FnCtxt;
use middle::typeck::infer::{force_all, resolve_all, resolve_region};
use middle::typeck::infer::resolve_type;
use middle::typeck::infer;
use middle::typeck::{vtable_res, vtable_origin};
use middle::typeck::{vtable_static, vtable_param};
use middle::typeck::write_substs_to_tcx;
use middle::typeck::write_ty_to_tcx;
use util::ppaux;
use util::ppaux::Repr;

use syntax::ast;
use syntax::codemap::Span;
use syntax::print::pprust::pat_to_str;
use syntax::visit;
use syntax::visit::Visitor;

fn resolve_type_vars_in_type(fcx: @FnCtxt, sp: Span, typ: ty::t)
                          -> Option<ty::t> {
    if !ty::type_needs_infer(typ) { return Some(typ); }
    match resolve_type(fcx.infcx(), typ, resolve_all | force_all) {
        Ok(new_type) => return Some(new_type),
        Err(e) => {
            if !fcx.ccx.tcx.sess.has_errors() {
                fcx.ccx.tcx.sess.span_err(
                    sp,
                    format!("cannot determine a type \
                          for this expression: {}",
                         infer::fixup_err_to_str(e)))
            }
            return None;
        }
    }
}

fn resolve_type_vars_in_types(fcx: @FnCtxt, sp: Span, tys: &[ty::t])
                          -> ~[ty::t] {
    tys.map(|t| {
        match resolve_type_vars_in_type(fcx, sp, *t) {
            Some(t1) => t1,
            None => ty::mk_err()
        }
    })
}

fn resolve_method_map_entry(fcx: @FnCtxt, id: ast::NodeId) {
    // Resolve any method map entry
    let method_map_entry_opt = {
        let method_map = fcx.inh.method_map.borrow();
        method_map.get().find_copy(&id)
    };
    match method_map_entry_opt {
        None => {}
        Some(mme) => {
            debug!("writeback::resolve_method_map_entry(id={:?}, entry={:?})", id, mme);
            let mut method_map = fcx.ccx.method_map.borrow_mut();
            method_map.get().insert(id, mme);
        }
    }
}

fn resolve_vtable_map_entry(fcx: @FnCtxt, sp: Span, id: ast::NodeId) {
    // Resolve any method map entry
    {
        let origins_opt = {
            let vtable_map = fcx.inh.vtable_map.borrow();
            vtable_map.get().find_copy(&id)
        };
        match origins_opt {
            None => {}
            Some(origins) => {
                let r_origins = resolve_origins(fcx, sp, origins);
                let mut vtable_map = fcx.ccx.vtable_map.borrow_mut();
                vtable_map.get().insert(id, r_origins);
                debug!("writeback::resolve_vtable_map_entry(id={}, vtables={:?})",
                       id, r_origins.repr(fcx.tcx()));
            }
        }
    }

    fn resolve_origins(fcx: @FnCtxt, sp: Span,
                       vtbls: vtable_res) -> vtable_res {
        @vtbls.map(|os| @os.map(|o| resolve_origin(fcx, sp, o)))
    }

    fn resolve_origin(fcx: @FnCtxt,
                      sp: Span,
                      origin: &vtable_origin) -> vtable_origin {
        match origin {
            &vtable_static(def_id, ref tys, origins) => {
                let r_tys = resolve_type_vars_in_types(fcx, sp, *tys);
                let r_origins = resolve_origins(fcx, sp, origins);
                vtable_static(def_id, r_tys, r_origins)
            }
            &vtable_param(n, b) => {
                vtable_param(n, b)
            }
        }
    }
}

fn resolve_type_vars_for_node(wbcx: &mut WbCtxt, sp: Span, id: ast::NodeId)
                           -> Option<ty::t> {
    let fcx = wbcx.fcx;
    let tcx = fcx.ccx.tcx;

    // Resolve any borrowings for the node with id `id`
    let adjustment = {
        let adjustments = fcx.inh.adjustments.borrow();
        adjustments.get().find_copy(&id)
    };
    match adjustment {
        None => (),

        Some(adjustment) => {
            match *adjustment {
                ty::AutoAddEnv(r, s) => {
                    match resolve_region(fcx.infcx(),
                                         r,
                                         resolve_all | force_all) {
                        Err(e) => {
                            // This should not, I think, happen:
                            tcx.sess.span_err(
                                sp,
                                format!("cannot resolve bound for closure: \
                                         {}",
                                        infer::fixup_err_to_str(e)));
                        }
                        Ok(r1) => {
                            // FIXME(eddyb) #2190 Allow only statically resolved
                            // bare functions to coerce to a closure to avoid
                            // constructing (slower) indirect call wrappers.
                            {
                                let def_map = tcx.def_map.borrow();
                                match def_map.get().find(&id) {
                                    Some(&ast::DefFn(..)) |
                                    Some(&ast::DefStaticMethod(..)) |
                                    Some(&ast::DefVariant(..)) |
                                    Some(&ast::DefStruct(_)) => {}
                                    _ => tcx.sess.span_err(sp,
                                            "cannot coerce non-statically resolved bare fn")
                                }
                            }

                            let resolved_adj = @ty::AutoAddEnv(r1, s);
                            debug!("Adjustments for node {}: {:?}",
                                   id,
                                   resolved_adj);
                            let mut adjustments = tcx.adjustments
                                                     .borrow_mut();
                            adjustments.get().insert(id, resolved_adj);
                        }
                    }
                }

                ty::AutoDerefRef(adj) => {
                    let fixup_region = |r| {
                        match resolve_region(fcx.infcx(),
                                             r,
                                             resolve_all | force_all) {
                            Ok(r1) => r1,
                            Err(e) => {
                                // This should not, I think, happen.
                                tcx.sess.span_err(
                                    sp,
                                    format!("cannot resolve scope of borrow: \
                                             {}",
                                             infer::fixup_err_to_str(e)));
                                r
                            }
                        }
                    };

                    let resolved_autoref = match adj.autoref {
                        None => None,
                        Some(ref r) => Some(r.map_region(fixup_region))
                    };

                    let resolved_adj = @ty::AutoDerefRef(ty::AutoDerefRef {
                        autoderefs: adj.autoderefs,
                        autoref: resolved_autoref,
                    });
                    debug!("Adjustments for node {}: {:?}", id, resolved_adj);
                    let mut adjustments = tcx.adjustments.borrow_mut();
                    adjustments.get().insert(id, resolved_adj);
                }

                ty::AutoObject(..) => {
                    debug!("Adjustments for node {}: {:?}", id, adjustment);
                    let mut adjustments = tcx.adjustments.borrow_mut();
                    adjustments.get().insert(id, adjustment);
                }
            }
        }
    }

    // Resolve the type of the node with id `id`
    let n_ty = fcx.node_ty(id);
    match resolve_type_vars_in_type(fcx, sp, n_ty) {
      None => {
        wbcx.success = false;
        return None;
      }

      Some(t) => {
        debug!("resolve_type_vars_for_node(id={}, n_ty={}, t={})",
               id, ppaux::ty_to_str(tcx, n_ty), ppaux::ty_to_str(tcx, t));
        write_ty_to_tcx(tcx, id, t);
        let mut ret = Some(t);
        fcx.opt_node_ty_substs(id, |substs| {
          let mut new_tps = ~[];
          for subst in substs.tps.iter() {
              match resolve_type_vars_in_type(fcx, sp, *subst) {
                Some(t) => new_tps.push(t),
                None => { wbcx.success = false; ret = None; break }
              }
          }
          write_substs_to_tcx(tcx, id, new_tps);
          ret.is_some()
        });
        ret
      }
    }
}

fn maybe_resolve_type_vars_for_node(wbcx: &mut WbCtxt,
                                    sp: Span,
                                    id: ast::NodeId)
                                 -> Option<ty::t> {
    let contained = {
        let node_types = wbcx.fcx.inh.node_types.borrow();
        node_types.get().contains_key(&id)
    };
    if contained {
        resolve_type_vars_for_node(wbcx, sp, id)
    } else {
        None
    }
}

struct WbCtxt {
    fcx: @FnCtxt,

    // As soon as we hit an error we have to stop resolving
    // the entire function.
    success: bool,
}

fn visit_stmt(s: &ast::Stmt, wbcx: &mut WbCtxt) {
    if !wbcx.success { return; }
    resolve_type_vars_for_node(wbcx, s.span, ty::stmt_node_id(s));
    visit::walk_stmt(wbcx, s, ());
}

fn visit_expr(e: &ast::Expr, wbcx: &mut WbCtxt) {
    if !wbcx.success {
        return;
    }

    resolve_type_vars_for_node(wbcx, e.span, e.id);

    resolve_method_map_entry(wbcx.fcx, e.id);
    {
        let r = e.get_callee_id();
        for callee_id in r.iter() {
            resolve_method_map_entry(wbcx.fcx, *callee_id);
        }
    }

    resolve_vtable_map_entry(wbcx.fcx, e.span, e.id);
    {
        let r = e.get_callee_id();
        for callee_id in r.iter() {
            resolve_vtable_map_entry(wbcx.fcx, e.span, *callee_id);
        }
    }

    match e.node {
        ast::ExprFnBlock(ref decl, _) | ast::ExprProc(ref decl, _) => {
            for input in decl.inputs.iter() {
                let _ = resolve_type_vars_for_node(wbcx, e.span, input.id);
            }
        }

        ast::ExprBinary(callee_id, _, _, _) |
        ast::ExprUnary(callee_id, _, _) |
        ast::ExprAssignOp(callee_id, _, _, _) |
        ast::ExprIndex(callee_id, _, _) => {
            maybe_resolve_type_vars_for_node(wbcx, e.span, callee_id);
        }

        ast::ExprMethodCall(callee_id, _, _, _) => {
            // We must always have written in a callee ID type for these.
            resolve_type_vars_for_node(wbcx, e.span, callee_id);
        }

        _ => ()
    }

    visit::walk_expr(wbcx, e, ());
}

fn visit_block(b: &ast::Block, wbcx: &mut WbCtxt) {
    if !wbcx.success {
        return;
    }

    resolve_type_vars_for_node(wbcx, b.span, b.id);
    visit::walk_block(wbcx, b, ());
}

fn visit_pat(p: &ast::Pat, wbcx: &mut WbCtxt) {
    if !wbcx.success {
        return;
    }

    resolve_type_vars_for_node(wbcx, p.span, p.id);
    debug!("Type for pattern binding {} (id {}) resolved to {}",
           pat_to_str(p), p.id,
           wbcx.fcx.infcx().ty_to_str(
               ty::node_id_to_type(wbcx.fcx.ccx.tcx,
                                   p.id)));
    visit::walk_pat(wbcx, p, ());
}

fn visit_local(l: &ast::Local, wbcx: &mut WbCtxt) {
    if !wbcx.success { return; }
    let var_ty = wbcx.fcx.local_ty(l.span, l.id);
    match resolve_type(wbcx.fcx.infcx(), var_ty, resolve_all | force_all) {
        Ok(lty) => {
            debug!("Type for local {} (id {}) resolved to {}",
                   pat_to_str(l.pat),
                   l.id,
                   wbcx.fcx.infcx().ty_to_str(lty));
            write_ty_to_tcx(wbcx.fcx.ccx.tcx, l.id, lty);
        }
        Err(e) => {
            wbcx.fcx.ccx.tcx.sess.span_err(
                l.span,
                format!("cannot determine a type \
                      for this local variable: {}",
                     infer::fixup_err_to_str(e)));
            wbcx.success = false;
        }
    }
    visit::walk_local(wbcx, l, ());
}
fn visit_item(_item: &ast::Item, _wbcx: &mut WbCtxt) {
    // Ignore items
}

impl Visitor<()> for WbCtxt {
    fn visit_item(&mut self, i: &ast::Item, _: ()) { visit_item(i, self); }
    fn visit_stmt(&mut self, s: &ast::Stmt, _: ()) { visit_stmt(s, self); }
    fn visit_expr(&mut self, ex:&ast::Expr, _: ()) { visit_expr(ex, self); }
    fn visit_block(&mut self, b: &ast::Block, _: ()) { visit_block(b, self); }
    fn visit_pat(&mut self, p: &ast::Pat, _: ()) { visit_pat(p, self); }
    fn visit_local(&mut self, l: &ast::Local, _: ()) { visit_local(l, self); }
    // FIXME(#10894) should continue recursing
    fn visit_ty(&mut self, _t: &ast::Ty, _: ()) {}
}

fn resolve_upvar_borrow_map(wbcx: &mut WbCtxt) {
    if !wbcx.success {
        return;
    }

    let fcx = wbcx.fcx;
    let tcx = fcx.tcx();
    let upvar_borrow_map = fcx.inh.upvar_borrow_map.borrow();
    for (upvar_id, upvar_borrow) in upvar_borrow_map.get().iter() {
        let r = upvar_borrow.region;
        match resolve_region(fcx.infcx(), r, resolve_all | force_all) {
            Ok(r) => {
                let new_upvar_borrow = ty::UpvarBorrow {
                    kind: upvar_borrow.kind,
                    region: r
                };
                debug!("Upvar borrow for {} resolved to {}",
                       upvar_id.repr(tcx), new_upvar_borrow.repr(tcx));
                let mut tcx_upvar_borrow_map = tcx.upvar_borrow_map.borrow_mut();
                tcx_upvar_borrow_map.get().insert(*upvar_id, new_upvar_borrow);
            }
            Err(e) => {
                let span = ty::expr_span(tcx, upvar_id.closure_expr_id);
                fcx.ccx.tcx.sess.span_err(
                    span, format!("cannot resolve lifetime for \
                                  captured variable `{}`: {}",
                                  ty::local_var_name_str(tcx, upvar_id.var_id).get().to_str(),
                                  infer::fixup_err_to_str(e)));
                wbcx.success = false;
            }
        };
    }
}

pub fn resolve_type_vars_in_expr(fcx: @FnCtxt, e: &ast::Expr) -> bool {
    let mut wbcx = WbCtxt { fcx: fcx, success: true };
    let wbcx = &mut wbcx;
    wbcx.visit_expr(e, ());
    resolve_upvar_borrow_map(wbcx);
    return wbcx.success;
}

pub fn resolve_type_vars_in_fn(fcx: @FnCtxt, decl: &ast::FnDecl,
                               blk: &ast::Block) -> bool {
    let mut wbcx = WbCtxt { fcx: fcx, success: true };
    let wbcx = &mut wbcx;
    wbcx.visit_block(blk, ());
    for arg in decl.inputs.iter() {
        wbcx.visit_pat(arg.pat, ());
        // Privacy needs the type for the whole pattern, not just each binding
        if !pat_util::pat_is_binding(fcx.tcx().def_map, arg.pat) {
            resolve_type_vars_for_node(wbcx, arg.pat.span, arg.pat.id);
        }
    }
    resolve_upvar_borrow_map(wbcx);
    return wbcx.success;
}
