// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
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
use self::ResolveReason::*;

use check::FnCtxt;
use hir::def_id::DefId;
use rustc::ty::{self, Ty, TyCtxt, MethodCall, MethodCallee};
use rustc::ty::adjustment;
use rustc::ty::fold::{TypeFolder,TypeFoldable};
use rustc::infer::{InferCtxt, FixupError};
use write_substs_to_tcx;
use write_ty_to_tcx;

use std::cell::Cell;

use syntax::ast;
use syntax_pos::{DUMMY_SP, Span};

use rustc::hir::print::pat_to_string;
use rustc::hir::intravisit::{self, Visitor};
use rustc::hir::{self, PatKind};

///////////////////////////////////////////////////////////////////////////
// Entry point functions

impl<'a, 'gcx, 'tcx> FnCtxt<'a, 'gcx, 'tcx> {
    pub fn resolve_type_vars_in_expr(&self, e: &hir::Expr) {
        assert_eq!(self.writeback_errors.get(), false);
        let mut wbcx = WritebackCx::new(self);
        wbcx.visit_expr(e);
        wbcx.visit_upvar_borrow_map();
        wbcx.visit_closures();
        wbcx.visit_liberated_fn_sigs();
        wbcx.visit_fru_field_types();
    }

    pub fn resolve_type_vars_in_fn(&self, decl: &hir::FnDecl, blk: &hir::Block) {
        assert_eq!(self.writeback_errors.get(), false);
        let mut wbcx = WritebackCx::new(self);
        wbcx.visit_block(blk);
        for arg in &decl.inputs {
            wbcx.visit_node_id(ResolvingPattern(arg.pat.span), arg.id);
            wbcx.visit_pat(&arg.pat);

            // Privacy needs the type for the whole pattern, not just each binding
            if let PatKind::Binding(..) = arg.pat.node {} else {
                wbcx.visit_node_id(ResolvingPattern(arg.pat.span), arg.pat.id);
            }
        }
        wbcx.visit_upvar_borrow_map();
        wbcx.visit_closures();
        wbcx.visit_liberated_fn_sigs();
        wbcx.visit_fru_field_types();
    }
}

///////////////////////////////////////////////////////////////////////////
// The Writerback context. This visitor walks the AST, checking the
// fn-specific tables to find references to types or regions. It
// resolves those regions to remove inference variables and writes the
// final result back into the master tables in the tcx. Here and
// there, it applies a few ad-hoc checks that were not convenient to
// do elsewhere.

struct WritebackCx<'cx, 'gcx: 'cx+'tcx, 'tcx: 'cx> {
    fcx: &'cx FnCtxt<'cx, 'gcx, 'tcx>,
}

impl<'cx, 'gcx, 'tcx> WritebackCx<'cx, 'gcx, 'tcx> {
    fn new(fcx: &'cx FnCtxt<'cx, 'gcx, 'tcx>) -> WritebackCx<'cx, 'gcx, 'tcx> {
        WritebackCx { fcx: fcx }
    }

    fn tcx(&self) -> TyCtxt<'cx, 'gcx, 'tcx> {
        self.fcx.tcx
    }

    // Hacky hack: During type-checking, we treat *all* operators
    // as potentially overloaded. But then, during writeback, if
    // we observe that something like `a+b` is (known to be)
    // operating on scalars, we clear the overload.
    fn fix_scalar_binary_expr(&mut self, e: &hir::Expr) {
        match e.node {
            hir::ExprBinary(ref op, ref lhs, ref rhs) |
            hir::ExprAssignOp(ref op, ref lhs, ref rhs) => {
                let lhs_ty = self.fcx.node_ty(lhs.id);
                let lhs_ty = self.fcx.resolve_type_vars_if_possible(&lhs_ty);

                let rhs_ty = self.fcx.node_ty(rhs.id);
                let rhs_ty = self.fcx.resolve_type_vars_if_possible(&rhs_ty);

                if lhs_ty.is_scalar() && rhs_ty.is_scalar() {
                    self.fcx.tables.borrow_mut().method_map.remove(&MethodCall::expr(e.id));

                    // weird but true: the by-ref binops put an
                    // adjustment on the lhs but not the rhs; the
                    // adjustment for rhs is kind of baked into the
                    // system.
                    match e.node {
                        hir::ExprBinary(..) => {
                            if !op.node.is_by_value() {
                                self.fcx.tables.borrow_mut().adjustments.remove(&lhs.id);
                            }
                        },
                        hir::ExprAssignOp(..) => {
                            self.fcx.tables.borrow_mut().adjustments.remove(&lhs.id);
                        },
                        _ => {},
                    }
                }
            }
            _ => {},
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Impl of Visitor for Resolver
//
// This is the master code which walks the AST. It delegates most of
// the heavy lifting to the generic visit and resolve functions
// below. In general, a function is made into a `visitor` if it must
// traffic in node-ids or update tables in the type context etc.

impl<'cx, 'gcx, 'tcx, 'v> Visitor<'v> for WritebackCx<'cx, 'gcx, 'tcx> {
    fn visit_stmt(&mut self, s: &hir::Stmt) {
        if self.fcx.writeback_errors.get() {
            return;
        }

        self.visit_node_id(ResolvingExpr(s.span), s.node.id());
        intravisit::walk_stmt(self, s);
    }

    fn visit_expr(&mut self, e: &hir::Expr) {
        if self.fcx.writeback_errors.get() {
            return;
        }

        self.fix_scalar_binary_expr(e);

        self.visit_node_id(ResolvingExpr(e.span), e.id);
        self.visit_method_map_entry(ResolvingExpr(e.span),
                                    MethodCall::expr(e.id));

        if let hir::ExprClosure(_, ref decl, _, _) = e.node {
            for input in &decl.inputs {
                self.visit_node_id(ResolvingExpr(e.span), input.id);
            }
        }

        intravisit::walk_expr(self, e);
    }

    fn visit_block(&mut self, b: &hir::Block) {
        if self.fcx.writeback_errors.get() {
            return;
        }

        self.visit_node_id(ResolvingExpr(b.span), b.id);
        intravisit::walk_block(self, b);
    }

    fn visit_pat(&mut self, p: &hir::Pat) {
        if self.fcx.writeback_errors.get() {
            return;
        }

        self.visit_node_id(ResolvingPattern(p.span), p.id);

        debug!("Type for pattern binding {} (id {}) resolved to {:?}",
               pat_to_string(p),
               p.id,
               self.tcx().node_id_to_type(p.id));

        intravisit::walk_pat(self, p);
    }

    fn visit_local(&mut self, l: &hir::Local) {
        if self.fcx.writeback_errors.get() {
            return;
        }

        let var_ty = self.fcx.local_ty(l.span, l.id);
        let var_ty = self.resolve(&var_ty, ResolvingLocal(l.span));
        write_ty_to_tcx(self.fcx.ccx, l.id, var_ty);
        intravisit::walk_local(self, l);
    }

    fn visit_ty(&mut self, t: &hir::Ty) {
        match t.node {
            hir::TyFixedLengthVec(ref ty, ref count_expr) => {
                self.visit_ty(&ty);
                write_ty_to_tcx(self.fcx.ccx, count_expr.id, self.tcx().types.usize);
            }
            hir::TyBareFn(ref function_declaration) => {
                intravisit::walk_fn_decl_nopat(self, &function_declaration.decl);
                walk_list!(self, visit_lifetime_def, &function_declaration.lifetimes);
            }
            _ => intravisit::walk_ty(self, t)
        }
    }
}

impl<'cx, 'gcx, 'tcx> WritebackCx<'cx, 'gcx, 'tcx> {
    fn visit_upvar_borrow_map(&self) {
        if self.fcx.writeback_errors.get() {
            return;
        }

        for (upvar_id, upvar_capture) in self.fcx.tables.borrow().upvar_capture_map.iter() {
            let new_upvar_capture = match *upvar_capture {
                ty::UpvarCapture::ByValue => ty::UpvarCapture::ByValue,
                ty::UpvarCapture::ByRef(ref upvar_borrow) => {
                    let r = upvar_borrow.region;
                    let r = self.resolve(&r, ResolvingUpvar(*upvar_id));
                    ty::UpvarCapture::ByRef(
                        ty::UpvarBorrow { kind: upvar_borrow.kind, region: r })
                }
            };
            debug!("Upvar capture for {:?} resolved to {:?}",
                   upvar_id,
                   new_upvar_capture);
            self.tcx()
                .tables
                .borrow_mut()
                .upvar_capture_map
                .insert(*upvar_id, new_upvar_capture);
        }
    }

    fn visit_closures(&self) {
        if self.fcx.writeback_errors.get() {
            return
        }

        for (def_id, closure_ty) in self.fcx.tables.borrow().closure_tys.iter() {
            let closure_ty = self.resolve(closure_ty, ResolvingClosure(*def_id));
            self.tcx().tables.borrow_mut().closure_tys.insert(*def_id, closure_ty);
        }

        for (def_id, &closure_kind) in self.fcx.tables.borrow().closure_kinds.iter() {
            self.tcx().tables.borrow_mut().closure_kinds.insert(*def_id, closure_kind);
        }
    }

    fn visit_node_id(&self, reason: ResolveReason, id: ast::NodeId) {
        // Resolve any borrowings for the node with id `id`
        self.visit_adjustments(reason, id);

        // Resolve the type of the node with id `id`
        let n_ty = self.fcx.node_ty(id);
        let n_ty = self.resolve(&n_ty, reason);
        write_ty_to_tcx(self.fcx.ccx, id, n_ty);
        debug!("Node {} has type {:?}", id, n_ty);

        // Resolve any substitutions
        self.fcx.opt_node_ty_substs(id, |item_substs| {
            write_substs_to_tcx(self.fcx.ccx, id,
                                self.resolve(item_substs, reason));
        });
    }

    fn visit_adjustments(&self, reason: ResolveReason, id: ast::NodeId) {
        let adjustments = self.fcx.tables.borrow_mut().adjustments.remove(&id);
        match adjustments {
            None => {
                debug!("No adjustments for node {}", id);
            }

            Some(adjustment) => {
                let resolved_adjustment = match adjustment {
                    adjustment::AdjustReifyFnPointer => {
                        adjustment::AdjustReifyFnPointer
                    }

                    adjustment::AdjustMutToConstPointer => {
                        adjustment::AdjustMutToConstPointer
                    }

                    adjustment::AdjustUnsafeFnPointer => {
                        adjustment::AdjustUnsafeFnPointer
                    }

                    adjustment::AdjustDerefRef(adj) => {
                        for autoderef in 0..adj.autoderefs {
                            let method_call = MethodCall::autoderef(id, autoderef as u32);
                            self.visit_method_map_entry(reason, method_call);
                        }

                        adjustment::AdjustDerefRef(adjustment::AutoDerefRef {
                            autoderefs: adj.autoderefs,
                            autoref: self.resolve(&adj.autoref, reason),
                            unsize: self.resolve(&adj.unsize, reason),
                        })
                    }
                };
                debug!("Adjustments for node {}: {:?}", id, resolved_adjustment);
                self.tcx().tables.borrow_mut().adjustments.insert(
                    id, resolved_adjustment);
            }
        }
    }

    fn visit_method_map_entry(&self,
                              reason: ResolveReason,
                              method_call: MethodCall) {
        // Resolve any method map entry
        let new_method = match self.fcx.tables.borrow_mut().method_map.remove(&method_call) {
            Some(method) => {
                debug!("writeback::resolve_method_map_entry(call={:?}, entry={:?})",
                       method_call,
                       method);
                let new_method = MethodCallee {
                    def_id: method.def_id,
                    ty: self.resolve(&method.ty, reason),
                    substs: self.resolve(&method.substs, reason),
                };

                Some(new_method)
            }
            None => None
        };

        //NB(jroesch): We need to match twice to avoid a double borrow which would cause an ICE
        match new_method {
            Some(method) => {
                self.tcx().tables.borrow_mut().method_map.insert(
                    method_call,
                    method);
            }
            None => {}
        }
    }

    fn visit_liberated_fn_sigs(&self) {
        for (&node_id, fn_sig) in self.fcx.tables.borrow().liberated_fn_sigs.iter() {
            let fn_sig = self.resolve(fn_sig, ResolvingFnSig(node_id));
            self.tcx().tables.borrow_mut().liberated_fn_sigs.insert(node_id, fn_sig.clone());
        }
    }

    fn visit_fru_field_types(&self) {
        for (&node_id, ftys) in self.fcx.tables.borrow().fru_field_types.iter() {
            let ftys = self.resolve(ftys, ResolvingFieldTypes(node_id));
            self.tcx().tables.borrow_mut().fru_field_types.insert(node_id, ftys);
        }
    }

    fn resolve<T>(&self, x: &T, reason: ResolveReason) -> T::Lifted
        where T: TypeFoldable<'tcx> + ty::Lift<'gcx>
    {
        let x = x.fold_with(&mut Resolver::new(self.fcx, reason));
        if let Some(lifted) = self.tcx().lift_to_global(&x) {
            lifted
        } else {
            span_bug!(reason.span(self.tcx()),
                      "writeback: `{:?}` missing from the global type context", x);
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Resolution reason.

#[derive(Copy, Clone)]
enum ResolveReason {
    ResolvingExpr(Span),
    ResolvingLocal(Span),
    ResolvingPattern(Span),
    ResolvingUpvar(ty::UpvarId),
    ResolvingClosure(DefId),
    ResolvingFnSig(ast::NodeId),
    ResolvingFieldTypes(ast::NodeId)
}

impl<'a, 'gcx, 'tcx> ResolveReason {
    fn span(&self, tcx: TyCtxt<'a, 'gcx, 'tcx>) -> Span {
        match *self {
            ResolvingExpr(s) => s,
            ResolvingLocal(s) => s,
            ResolvingPattern(s) => s,
            ResolvingUpvar(upvar_id) => {
                tcx.expr_span(upvar_id.closure_expr_id)
            }
            ResolvingFnSig(id) => {
                tcx.map.span(id)
            }
            ResolvingFieldTypes(id) => {
                tcx.map.span(id)
            }
            ResolvingClosure(did) => {
                if let Some(node_id) = tcx.map.as_local_node_id(did) {
                    tcx.expr_span(node_id)
                } else {
                    DUMMY_SP
                }
            }
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// The Resolver. This is the type folding engine that detects
// unresolved types and so forth.

struct Resolver<'cx, 'gcx: 'cx+'tcx, 'tcx: 'cx> {
    tcx: TyCtxt<'cx, 'gcx, 'tcx>,
    infcx: &'cx InferCtxt<'cx, 'gcx, 'tcx>,
    writeback_errors: &'cx Cell<bool>,
    reason: ResolveReason,
}

impl<'cx, 'gcx, 'tcx> Resolver<'cx, 'gcx, 'tcx> {
    fn new(fcx: &'cx FnCtxt<'cx, 'gcx, 'tcx>,
           reason: ResolveReason)
           -> Resolver<'cx, 'gcx, 'tcx>
    {
        Resolver::from_infcx(fcx, &fcx.writeback_errors, reason)
    }

    fn from_infcx(infcx: &'cx InferCtxt<'cx, 'gcx, 'tcx>,
                  writeback_errors: &'cx Cell<bool>,
                  reason: ResolveReason)
                  -> Resolver<'cx, 'gcx, 'tcx>
    {
        Resolver { infcx: infcx,
                   tcx: infcx.tcx,
                   writeback_errors: writeback_errors,
                   reason: reason }
    }

    fn report_error(&self, e: FixupError) {
        self.writeback_errors.set(true);
        if !self.tcx.sess.has_errors() {
            match self.reason {
                ResolvingExpr(span) => {
                    span_err!(self.tcx.sess, span, E0101,
                        "cannot determine a type for this expression: {}", e);
                }

                ResolvingLocal(span) => {
                    span_err!(self.tcx.sess, span, E0102,
                        "cannot determine a type for this local variable: {}", e);
                }

                ResolvingPattern(span) => {
                    span_err!(self.tcx.sess, span, E0103,
                        "cannot determine a type for this pattern binding: {}", e);
                }

                ResolvingUpvar(upvar_id) => {
                    let span = self.reason.span(self.tcx);
                    span_err!(self.tcx.sess, span, E0104,
                        "cannot resolve lifetime for captured variable `{}`: {}",
                        self.tcx.local_var_name_str(upvar_id.var_id), e);
                }

                ResolvingClosure(_) => {
                    let span = self.reason.span(self.tcx);
                    span_err!(self.tcx.sess, span, E0196,
                              "cannot determine a type for this closure")
                }

                ResolvingFnSig(id) | ResolvingFieldTypes(id) => {
                    // any failures here should also fail when
                    // resolving the patterns, closure types, or
                    // something else.
                    let span = self.reason.span(self.tcx);
                    self.tcx.sess.delay_span_bug(
                        span,
                        &format!("cannot resolve some aspect of data for {:?}", id));
                }
            }
        }
    }
}

impl<'cx, 'gcx, 'tcx> TypeFolder<'gcx, 'tcx> for Resolver<'cx, 'gcx, 'tcx> {
    fn tcx<'a>(&'a self) -> TyCtxt<'a, 'gcx, 'tcx> {
        self.tcx
    }

    fn fold_ty(&mut self, t: Ty<'tcx>) -> Ty<'tcx> {
        match self.infcx.fully_resolve(&t) {
            Ok(t) => t,
            Err(e) => {
                debug!("Resolver::fold_ty: input type `{:?}` not fully resolvable",
                       t);
                self.report_error(e);
                self.tcx().types.err
            }
        }
    }

    fn fold_region(&mut self, r: ty::Region) -> ty::Region {
        match self.infcx.fully_resolve(&r) {
            Ok(r) => r,
            Err(e) => {
                self.report_error(e);
                ty::ReStatic
            }
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// During type check, we store promises with the result of trait
// lookup rather than the actual results (because the results are not
// necessarily available immediately). These routines unwind the
// promises. It is expected that we will have already reported any
// errors that may be encountered, so if the promises store an error,
// a dummy result is returned.
