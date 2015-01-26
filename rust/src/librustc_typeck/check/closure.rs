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

use astconv;
use middle::region::CodeExtent;
use middle::subst;
use middle::ty::{self, ToPolyTraitRef, Ty};
use rscope::RegionScope;
use syntax::abi;
use syntax::ast;
use syntax::ast_util;
use util::ppaux::Repr;

pub fn check_expr_closure<'a,'tcx>(fcx: &FnCtxt<'a,'tcx>,
                                   expr: &ast::Expr,
                                   _capture: ast::CaptureClause,
                                   opt_kind: Option<ast::ClosureKind>,
                                   decl: &ast::FnDecl,
                                   body: &ast::Block,
                                   expected: Expectation<'tcx>) {
    debug!("check_expr_closure(expr={},expected={})",
           expr.repr(fcx.tcx()),
           expected.repr(fcx.tcx()));

    let expected_sig_and_kind = expected.to_option(fcx).and_then(|ty| {
        deduce_closure_expectations_from_expected_type(fcx, ty)
    });

    match opt_kind {
        None => {
            // If users didn't specify what sort of closure they want,
            // examine the expected type. For now, if we see explicit
            // evidence than an unboxed closure is desired, we'll use
            // that, otherwise we'll error, requesting an annotation.
            match expected_sig_and_kind {
                None => { // don't have information about the kind, request explicit annotation
                    // NB We still need to typeck the body, so assume `FnMut` kind just for that
                    let kind = ty::FnMutClosureKind;

                    check_closure(fcx, expr, kind, decl, body, None);

                    span_err!(fcx.ccx.tcx.sess, expr.span, E0187,
                        "can't infer the \"kind\" of the closure; explicitly annotate it; e.g. \
                        `|&:| {{}}`");
                },
                Some((sig, kind)) => {
                    check_closure(fcx, expr, kind, decl, body, Some(sig));
                }
            }
        }

        Some(kind) => {
            let kind = match kind {
                ast::FnClosureKind => ty::FnClosureKind,
                ast::FnMutClosureKind => ty::FnMutClosureKind,
                ast::FnOnceClosureKind => ty::FnOnceClosureKind,
            };

            let expected_sig = expected_sig_and_kind.map(|t| t.0);
            check_closure(fcx, expr, kind, decl, body, expected_sig);
        }
    }
}

fn check_closure<'a,'tcx>(fcx: &FnCtxt<'a,'tcx>,
                          expr: &ast::Expr,
                          kind: ty::ClosureKind,
                          decl: &ast::FnDecl,
                          body: &ast::Block,
                          expected_sig: Option<ty::FnSig<'tcx>>) {
    let expr_def_id = ast_util::local_def(expr.id);

    debug!("check_closure kind={:?} expected_sig={}",
           kind,
           expected_sig.repr(fcx.tcx()));

    let mut fn_ty = astconv::ty_of_closure(
        fcx,
        ast::Unsafety::Normal,
        decl,
        abi::RustCall,
        expected_sig);

    let region = match fcx.anon_regions(expr.span, 1) {
        Err(_) => {
            fcx.ccx.tcx.sess.span_bug(expr.span,
                                      "can't make anon regions here?!")
        }
        Ok(regions) => regions[0],
    };

    let closure_type = ty::mk_closure(fcx.ccx.tcx,
                                      expr_def_id,
                                      fcx.ccx.tcx.mk_region(region),
                                      fcx.ccx.tcx.mk_substs(
                                        fcx.inh.param_env.free_substs.clone()));

    fcx.write_ty(expr.id, closure_type);

    let fn_sig =
        ty::liberate_late_bound_regions(fcx.tcx(), CodeExtent::from_node_id(body.id), &fn_ty.sig);

    check_fn(fcx.ccx,
             ast::Unsafety::Normal,
             expr.id,
             &fn_sig,
             decl,
             expr.id,
             &*body,
             fcx.inh);

    // Tuple up the arguments and insert the resulting function type into
    // the `closures` table.
    fn_ty.sig.0.inputs = vec![ty::mk_tup(fcx.tcx(), fn_ty.sig.0.inputs)];

    debug!("closure for {} --> sig={} kind={:?}",
           expr_def_id.repr(fcx.tcx()),
           fn_ty.sig.repr(fcx.tcx()),
           kind);

    let closure = ty::Closure {
        closure_type: fn_ty,
        kind: kind,
    };

    fcx.inh.closures.borrow_mut().insert(expr_def_id, closure);
}

fn deduce_closure_expectations_from_expected_type<'a,'tcx>(
    fcx: &FnCtxt<'a,'tcx>,
    expected_ty: Ty<'tcx>)
    -> Option<(ty::FnSig<'tcx>,ty::ClosureKind)>
{
    match expected_ty.sty {
        ty::ty_trait(ref object_type) => {
            let trait_ref =
                object_type.principal_trait_ref_with_self_ty(fcx.tcx(),
                                                             fcx.tcx().types.err);
            deduce_closure_expectations_from_trait_ref(fcx, &trait_ref)
        }
        ty::ty_infer(ty::TyVar(vid)) => {
            deduce_closure_expectations_from_obligations(fcx, vid)
        }
        _ => {
            None
        }
    }
}

fn deduce_closure_expectations_from_trait_ref<'a,'tcx>(
    fcx: &FnCtxt<'a,'tcx>,
    trait_ref: &ty::PolyTraitRef<'tcx>)
    -> Option<(ty::FnSig<'tcx>, ty::ClosureKind)>
{
    let tcx = fcx.tcx();

    debug!("deduce_closure_expectations_from_object_type({})",
           trait_ref.repr(tcx));

    let kind = match tcx.lang_items.fn_trait_kind(trait_ref.def_id()) {
        Some(k) => k,
        None => { return None; }
    };

    debug!("found object type {:?}", kind);

    let arg_param_ty = *trait_ref.substs().types.get(subst::TypeSpace, 0);
    let arg_param_ty = fcx.infcx().resolve_type_vars_if_possible(&arg_param_ty);
    debug!("arg_param_ty {}", arg_param_ty.repr(tcx));

    let input_tys = match arg_param_ty.sty {
        ty::ty_tup(ref tys) => { (*tys).clone() }
        _ => { return None; }
    };
    debug!("input_tys {}", input_tys.repr(tcx));

    let ret_param_ty = *trait_ref.substs().types.get(subst::TypeSpace, 1);
    let ret_param_ty = fcx.infcx().resolve_type_vars_if_possible(&ret_param_ty);
    debug!("ret_param_ty {}", ret_param_ty.repr(tcx));

    let fn_sig = ty::FnSig {
        inputs: input_tys,
        output: ty::FnConverging(ret_param_ty),
        variadic: false
    };
    debug!("fn_sig {}", fn_sig.repr(tcx));

    return Some((fn_sig, kind));
}

fn deduce_closure_expectations_from_obligations<'a,'tcx>(
    fcx: &FnCtxt<'a,'tcx>,
    expected_vid: ty::TyVid)
    -> Option<(ty::FnSig<'tcx>, ty::ClosureKind)>
{
    // Here `expected_ty` is known to be a type inference variable.
    for obligation in fcx.inh.fulfillment_cx.borrow().pending_obligations().iter() {
        match obligation.predicate {
            ty::Predicate::Trait(ref trait_predicate) => {
                let trait_ref = trait_predicate.to_poly_trait_ref();
                let self_ty = fcx.infcx().shallow_resolve(trait_ref.self_ty());
                match self_ty.sty {
                    ty::ty_infer(ty::TyVar(v)) if expected_vid == v => { }
                    _ => { continue; }
                }

                match deduce_closure_expectations_from_trait_ref(fcx, &trait_ref) {
                    Some(e) => { return Some(e); }
                    None => { }
                }
            }
            _ => { }
        }
    }

    None
}
