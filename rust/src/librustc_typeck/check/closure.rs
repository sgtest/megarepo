// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Code for type-checking closure expressions.

use super::{check_fn, Expectation, FnCtxt};

use astconv::AstConv;
use rustc::ty::subst;
use rustc::ty::{self, ToPolyTraitRef, Ty};
use std::cmp;
use syntax::abi::Abi;
use rustc::hir;

impl<'a, 'gcx, 'tcx> FnCtxt<'a, 'gcx, 'tcx> {
    pub fn check_expr_closure(&self,
                              expr: &hir::Expr,
                              _capture: hir::CaptureClause,
                              decl: &'gcx hir::FnDecl,
                              body: &'gcx hir::Block,
                              expected: Expectation<'tcx>) {
        debug!("check_expr_closure(expr={:?},expected={:?})",
               expr,
               expected);

        // It's always helpful for inference if we know the kind of
        // closure sooner rather than later, so first examine the expected
        // type, and see if can glean a closure kind from there.
        let (expected_sig,expected_kind) = match expected.to_option(self) {
            Some(ty) => self.deduce_expectations_from_expected_type(ty),
            None => (None, None)
        };
        self.check_closure(expr, expected_kind, decl, body, expected_sig)
    }

    fn check_closure(&self,
                     expr: &hir::Expr,
                     opt_kind: Option<ty::ClosureKind>,
                     decl: &'gcx hir::FnDecl,
                     body: &'gcx hir::Block,
                     expected_sig: Option<ty::FnSig<'tcx>>) {
        let expr_def_id = self.tcx.map.local_def_id(expr.id);

        debug!("check_closure opt_kind={:?} expected_sig={:?}",
               opt_kind,
               expected_sig);

        let mut fn_ty = AstConv::ty_of_closure(self,
                                               hir::Unsafety::Normal,
                                               decl,
                                               Abi::RustCall,
                                               expected_sig);

        // Create type variables (for now) to represent the transformed
        // types of upvars. These will be unified during the upvar
        // inference phase (`upvar.rs`).
        let num_upvars = self.tcx.with_freevars(expr.id, |fv| fv.len());
        let upvar_tys = self.next_ty_vars(num_upvars);

        debug!("check_closure: expr.id={:?} upvar_tys={:?}",
               expr.id, upvar_tys);

        let closure_type = self.tcx.mk_closure(expr_def_id,
            self.parameter_environment.free_substs,
            upvar_tys);

        self.write_ty(expr.id, closure_type);

        let fn_sig = self.tcx.liberate_late_bound_regions(
            self.tcx.region_maps.call_site_extent(expr.id, body.id), &fn_ty.sig);

        check_fn(self, hir::Unsafety::Normal, expr.id, &fn_sig, decl, expr.id, &body);

        // Tuple up the arguments and insert the resulting function type into
        // the `closures` table.
        fn_ty.sig.0.inputs = vec![self.tcx.mk_tup(fn_ty.sig.0.inputs)];

        debug!("closure for {:?} --> sig={:?} opt_kind={:?}",
               expr_def_id,
               fn_ty.sig,
               opt_kind);

        self.tables.borrow_mut().closure_tys.insert(expr_def_id, fn_ty);
        match opt_kind {
            Some(kind) => { self.tables.borrow_mut().closure_kinds.insert(expr_def_id, kind); }
            None => { }
        }
    }

    fn deduce_expectations_from_expected_type(&self, expected_ty: Ty<'tcx>)
        -> (Option<ty::FnSig<'tcx>>,Option<ty::ClosureKind>)
    {
        debug!("deduce_expectations_from_expected_type(expected_ty={:?})",
               expected_ty);

        match expected_ty.sty {
            ty::TyTrait(ref object_type) => {
                let proj_bounds = object_type.projection_bounds_with_self_ty(self.tcx,
                                                                             self.tcx.types.err);
                let sig = proj_bounds.iter()
                                     .filter_map(|pb| self.deduce_sig_from_projection(pb))
                                     .next();
                let kind = self.tcx.lang_items.fn_trait_kind(object_type.principal_def_id());
                (sig, kind)
            }
            ty::TyInfer(ty::TyVar(vid)) => {
                self.deduce_expectations_from_obligations(vid)
            }
            _ => {
                (None, None)
            }
        }
    }

    fn deduce_expectations_from_obligations(&self, expected_vid: ty::TyVid)
        -> (Option<ty::FnSig<'tcx>>, Option<ty::ClosureKind>)
    {
        let fulfillment_cx = self.fulfillment_cx.borrow();
        // Here `expected_ty` is known to be a type inference variable.

        let expected_sig =
            fulfillment_cx
            .pending_obligations()
            .iter()
            .map(|obligation| &obligation.obligation)
            .filter_map(|obligation| {
                debug!("deduce_expectations_from_obligations: obligation.predicate={:?}",
                       obligation.predicate);

                match obligation.predicate {
                    // Given a Projection predicate, we can potentially infer
                    // the complete signature.
                    ty::Predicate::Projection(ref proj_predicate) => {
                        let trait_ref = proj_predicate.to_poly_trait_ref();
                        self.self_type_matches_expected_vid(trait_ref, expected_vid)
                            .and_then(|_| self.deduce_sig_from_projection(proj_predicate))
                    }
                    _ => {
                        None
                    }
                }
            })
            .next();

        // Even if we can't infer the full signature, we may be able to
        // infer the kind. This can occur if there is a trait-reference
        // like `F : Fn<A>`. Note that due to subtyping we could encounter
        // many viable options, so pick the most restrictive.
        let expected_kind =
            fulfillment_cx
            .pending_obligations()
            .iter()
            .map(|obligation| &obligation.obligation)
            .filter_map(|obligation| {
                let opt_trait_ref = match obligation.predicate {
                    ty::Predicate::Projection(ref data) => Some(data.to_poly_trait_ref()),
                    ty::Predicate::Trait(ref data) => Some(data.to_poly_trait_ref()),
                    ty::Predicate::Equate(..) => None,
                    ty::Predicate::RegionOutlives(..) => None,
                    ty::Predicate::TypeOutlives(..) => None,
                    ty::Predicate::WellFormed(..) => None,
                    ty::Predicate::ObjectSafe(..) => None,
                    ty::Predicate::Rfc1592(..) => None,

                    // NB: This predicate is created by breaking down a
                    // `ClosureType: FnFoo()` predicate, where
                    // `ClosureType` represents some `TyClosure`. It can't
                    // possibly be referring to the current closure,
                    // because we haven't produced the `TyClosure` for
                    // this closure yet; this is exactly why the other
                    // code is looking for a self type of a unresolved
                    // inference variable.
                    ty::Predicate::ClosureKind(..) => None,
                };
                opt_trait_ref
                    .and_then(|tr| self.self_type_matches_expected_vid(tr, expected_vid))
                    .and_then(|tr| self.tcx.lang_items.fn_trait_kind(tr.def_id()))
            })
            .fold(None, |best, cur| Some(best.map_or(cur, |best| cmp::min(best, cur))));

        (expected_sig, expected_kind)
    }

    /// Given a projection like "<F as Fn(X)>::Result == Y", we can deduce
    /// everything we need to know about a closure.
    fn deduce_sig_from_projection(&self,
        projection: &ty::PolyProjectionPredicate<'tcx>)
        -> Option<ty::FnSig<'tcx>>
    {
        let tcx = self.tcx;

        debug!("deduce_sig_from_projection({:?})",
               projection);

        let trait_ref = projection.to_poly_trait_ref();

        if tcx.lang_items.fn_trait_kind(trait_ref.def_id()).is_none() {
            return None;
        }

        let arg_param_ty = *trait_ref.substs().types.get(subst::TypeSpace, 0);
        let arg_param_ty = self.resolve_type_vars_if_possible(&arg_param_ty);
        debug!("deduce_sig_from_projection: arg_param_ty {:?}", arg_param_ty);

        let input_tys = match arg_param_ty.sty {
            ty::TyTuple(tys) => tys.to_vec(),
            _ => { return None; }
        };
        debug!("deduce_sig_from_projection: input_tys {:?}", input_tys);

        let ret_param_ty = projection.0.ty;
        let ret_param_ty = self.resolve_type_vars_if_possible(&ret_param_ty);
        debug!("deduce_sig_from_projection: ret_param_ty {:?}", ret_param_ty);

        let fn_sig = ty::FnSig {
            inputs: input_tys,
            output: ret_param_ty,
            variadic: false
        };
        debug!("deduce_sig_from_projection: fn_sig {:?}", fn_sig);

        Some(fn_sig)
    }

    fn self_type_matches_expected_vid(&self,
        trait_ref: ty::PolyTraitRef<'tcx>,
        expected_vid: ty::TyVid)
        -> Option<ty::PolyTraitRef<'tcx>>
    {
        let self_ty = self.shallow_resolve(trait_ref.self_ty());
        debug!("self_type_matches_expected_vid(trait_ref={:?}, self_ty={:?})",
               trait_ref,
               self_ty);
        match self_ty.sty {
            ty::TyInfer(ty::TyVar(v)) if expected_vid == v => Some(trait_ref),
            _ => None,
        }
    }
}
