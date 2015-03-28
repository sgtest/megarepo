// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Method lookup: the secret sauce of Rust. See `README.md`.

use astconv::AstConv;
use check::FnCtxt;
use check::vtable;
use check::vtable::select_new_fcx_obligations;
use middle::def;
use middle::privacy::{AllPublic, DependsOn, LastPrivate, LastMod};
use middle::subst;
use middle::traits;
use middle::ty::*;
use middle::ty;
use middle::infer;
use util::ppaux::Repr;

use std::rc::Rc;
use syntax::ast::DefId;
use syntax::ast;
use syntax::codemap::Span;

pub use self::MethodError::*;
pub use self::CandidateSource::*;

pub use self::suggest::{report_error, AllTraitsVec};

mod confirm;
mod probe;
mod suggest;

pub enum MethodError {
    // Did not find an applicable method, but we did find various
    // static methods that may apply, as well as a list of
    // not-in-scope traits which may work.
    NoMatch(Vec<CandidateSource>, Vec<ast::DefId>),

    // Multiple methods might apply.
    Ambiguity(Vec<CandidateSource>),

    // Using a `Fn`/`FnMut`/etc method on a raw closure type before we have inferred its kind.
    ClosureAmbiguity(/* DefId of fn trait */ ast::DefId),
}

// A pared down enum describing just the places from which a method
// candidate can arise. Used for error reporting only.
#[derive(Copy, PartialOrd, Ord, PartialEq, Eq)]
pub enum CandidateSource {
    ImplSource(ast::DefId),
    TraitSource(/* trait id */ ast::DefId),
}

type MethodIndex = usize; // just for doc purposes

/// Determines whether the type `self_ty` supports a method name `method_name` or not.
pub fn exists<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                        span: Span,
                        method_name: ast::Name,
                        self_ty: Ty<'tcx>,
                        call_expr_id: ast::NodeId)
                        -> bool
{
    let mode = probe::Mode::MethodCall;
    match probe::probe(fcx, span, mode, method_name, self_ty, call_expr_id) {
        Ok(..) => true,
        Err(NoMatch(..)) => false,
        Err(Ambiguity(..)) => true,
        Err(ClosureAmbiguity(..)) => true,
    }
}

/// Performs method lookup. If lookup is successful, it will return the callee and store an
/// appropriate adjustment for the self-expr. In some cases it may report an error (e.g., invoking
/// the `drop` method).
///
/// # Arguments
///
/// Given a method call like `foo.bar::<T1,...Tn>(...)`:
///
/// * `fcx`:                   the surrounding `FnCtxt` (!)
/// * `span`:                  the span for the method call
/// * `method_name`:           the name of the method being called (`bar`)
/// * `self_ty`:               the (unadjusted) type of the self expression (`foo`)
/// * `supplied_method_types`: the explicit method type parameters, if any (`T1..Tn`)
/// * `self_expr`:             the self expression (`foo`)
pub fn lookup<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                        span: Span,
                        method_name: ast::Name,
                        self_ty: Ty<'tcx>,
                        supplied_method_types: Vec<Ty<'tcx>>,
                        call_expr: &'tcx ast::Expr,
                        self_expr: &'tcx ast::Expr)
                        -> Result<MethodCallee<'tcx>, MethodError>
{
    debug!("lookup(method_name={}, self_ty={}, call_expr={}, self_expr={})",
           method_name.repr(fcx.tcx()),
           self_ty.repr(fcx.tcx()),
           call_expr.repr(fcx.tcx()),
           self_expr.repr(fcx.tcx()));

    let mode = probe::Mode::MethodCall;
    let self_ty = fcx.infcx().resolve_type_vars_if_possible(&self_ty);
    let pick = try!(probe::probe(fcx, span, mode, method_name, self_ty, call_expr.id));
    Ok(confirm::confirm(fcx, span, self_expr, call_expr, self_ty, pick, supplied_method_types))
}

pub fn lookup_in_trait<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                 span: Span,
                                 self_expr: Option<&ast::Expr>,
                                 m_name: ast::Name,
                                 trait_def_id: DefId,
                                 self_ty: Ty<'tcx>,
                                 opt_input_types: Option<Vec<Ty<'tcx>>>)
                                 -> Option<MethodCallee<'tcx>>
{
    lookup_in_trait_adjusted(fcx, span, self_expr, m_name, trait_def_id,
                             ty::AutoDerefRef { autoderefs: 0, autoref: None },
                             self_ty, opt_input_types)
}

/// `lookup_in_trait_adjusted` is used for overloaded operators. It does a very narrow slice of
/// what the normal probe/confirm path does. In particular, it doesn't really do any probing: it
/// simply constructs an obligation for a particular trait with the given self-type and checks
/// whether that trait is implemented.
///
/// FIXME(#18741) -- It seems likely that we can consolidate some of this code with the other
/// method-lookup code. In particular, autoderef on index is basically identical to autoderef with
/// normal probes, except that the test also looks for built-in indexing. Also, the second half of
/// this method is basically the same as confirmation.
pub fn lookup_in_trait_adjusted<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                          span: Span,
                                          self_expr: Option<&ast::Expr>,
                                          m_name: ast::Name,
                                          trait_def_id: DefId,
                                          autoderefref: ty::AutoDerefRef<'tcx>,
                                          self_ty: Ty<'tcx>,
                                          opt_input_types: Option<Vec<Ty<'tcx>>>)
                                          -> Option<MethodCallee<'tcx>>
{
    debug!("lookup_in_trait_adjusted(self_ty={}, self_expr={}, m_name={}, trait_def_id={})",
           self_ty.repr(fcx.tcx()),
           self_expr.repr(fcx.tcx()),
           m_name.repr(fcx.tcx()),
           trait_def_id.repr(fcx.tcx()));

    let trait_def = ty::lookup_trait_def(fcx.tcx(), trait_def_id);

    let expected_number_of_input_types = trait_def.generics.types.len(subst::TypeSpace);
    let input_types = match opt_input_types {
        Some(input_types) => {
            assert_eq!(expected_number_of_input_types, input_types.len());
            input_types
        }

        None => {
            fcx.inh.infcx.next_ty_vars(expected_number_of_input_types)
        }
    };

    assert_eq!(trait_def.generics.types.len(subst::FnSpace), 0);
    assert!(trait_def.generics.regions.is_empty());

    // Construct a trait-reference `self_ty : Trait<input_tys>`
    let substs = subst::Substs::new_trait(input_types, Vec::new(), self_ty);
    let trait_ref = Rc::new(ty::TraitRef::new(trait_def_id, fcx.tcx().mk_substs(substs)));

    // Construct an obligation
    let poly_trait_ref = trait_ref.to_poly_trait_ref();
    let obligation = traits::Obligation::misc(span,
                                              fcx.body_id,
                                              poly_trait_ref.as_predicate());

    // Now we want to know if this can be matched
    let mut selcx = traits::SelectionContext::new(fcx.infcx(), fcx);
    if !selcx.evaluate_obligation(&obligation) {
        debug!("--> Cannot match obligation");
        return None; // Cannot be matched, no such method resolution is possible.
    }

    // Trait must have a method named `m_name` and it should not have
    // type parameters or early-bound regions.
    let tcx = fcx.tcx();
    let (method_num, method_ty) = trait_method(tcx, trait_def_id, m_name).unwrap();
    assert_eq!(method_ty.generics.types.len(subst::FnSpace), 0);
    assert_eq!(method_ty.generics.regions.len(subst::FnSpace), 0);

    debug!("lookup_in_trait_adjusted: method_num={} method_ty={}",
           method_num, method_ty.repr(fcx.tcx()));

    // Instantiate late-bound regions and substitute the trait
    // parameters into the method type to get the actual method type.
    //
    // NB: Instantiate late-bound regions first so that
    // `instantiate_type_scheme` can normalize associated types that
    // may reference those regions.
    let fn_sig = fcx.infcx().replace_late_bound_regions_with_fresh_var(span,
                                                                       infer::FnCall,
                                                                       &method_ty.fty.sig).0;
    let fn_sig = fcx.instantiate_type_scheme(span, trait_ref.substs, &fn_sig);
    let transformed_self_ty = fn_sig.inputs[0];
    let fty = ty::mk_bare_fn(tcx, None, tcx.mk_bare_fn(ty::BareFnTy {
        sig: ty::Binder(fn_sig),
        unsafety: method_ty.fty.unsafety,
        abi: method_ty.fty.abi.clone(),
    }));

    debug!("lookup_in_trait_adjusted: matched method fty={} obligation={}",
           fty.repr(fcx.tcx()),
           obligation.repr(fcx.tcx()));

    // Register obligations for the parameters.  This will include the
    // `Self` parameter, which in turn has a bound of the main trait,
    // so this also effectively registers `obligation` as well.  (We
    // used to register `obligation` explicitly, but that resulted in
    // double error messages being reported.)
    //
    // Note that as the method comes from a trait, it should not have
    // any late-bound regions appearing in its bounds.
    let method_bounds = fcx.instantiate_bounds(span, trait_ref.substs, &method_ty.predicates);
    assert!(!method_bounds.has_escaping_regions());
    fcx.add_obligations_for_parameters(
        traits::ObligationCause::misc(span, fcx.body_id),
        &method_bounds);

    // FIXME(#18653) -- Try to resolve obligations, giving us more
    // typing information, which can sometimes be needed to avoid
    // pathological region inference failures.
    vtable::select_new_fcx_obligations(fcx);

    // Insert any adjustments needed (always an autoref of some mutability).
    match self_expr {
        None => { }

        Some(self_expr) => {
            debug!("lookup_in_trait_adjusted: inserting adjustment if needed \
                   (self-id={}, base adjustment={:?}, explicit_self={:?})",
                   self_expr.id, autoderefref, method_ty.explicit_self);

            match method_ty.explicit_self {
                ty::ByValueExplicitSelfCategory => {
                    // Trait method is fn(self), no transformation needed.
                    if !autoderefref.is_identity() {
                        fcx.write_adjustment(
                            self_expr.id,
                            span,
                            ty::AdjustDerefRef(autoderefref));
                    }
                }

                ty::ByReferenceExplicitSelfCategory(..) => {
                    // Trait method is fn(&self) or fn(&mut self), need an
                    // autoref. Pull the region etc out of the type of first argument.
                    match transformed_self_ty.sty {
                        ty::ty_rptr(region, ty::mt { mutbl, ty: _ }) => {
                            let ty::AutoDerefRef { autoderefs, autoref } = autoderefref;
                            let autoref = autoref.map(|r| box r);
                            fcx.write_adjustment(
                                self_expr.id,
                                span,
                                ty::AdjustDerefRef(ty::AutoDerefRef {
                                    autoderefs: autoderefs,
                                    autoref: Some(ty::AutoPtr(*region, mutbl, autoref))
                                }));
                        }

                        _ => {
                            fcx.tcx().sess.span_bug(
                                span,
                                &format!(
                                    "trait method is &self but first arg is: {}",
                                    transformed_self_ty.repr(fcx.tcx())));
                        }
                    }
                }

                _ => {
                    fcx.tcx().sess.span_bug(
                        span,
                        &format!(
                            "unexpected explicit self type in operator method: {:?}",
                            method_ty.explicit_self));
                }
            }
        }
    }

    let callee = MethodCallee {
        origin: MethodTypeParam(MethodParam{trait_ref: trait_ref.clone(),
                                            method_num: method_num,
                                            impl_def_id: None}),
        ty: fty,
        substs: trait_ref.substs.clone()
    };

    debug!("callee = {}", callee.repr(fcx.tcx()));

    Some(callee)
}

pub fn resolve_ufcs<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                              span: Span,
                              method_name: ast::Name,
                              self_ty: Ty<'tcx>,
                              expr_id: ast::NodeId)
                              -> Result<(def::Def, LastPrivate), MethodError>
{
    let mode = probe::Mode::Path;
    let pick = try!(probe::probe(fcx, span, mode, method_name, self_ty, expr_id));
    let def_id = pick.method_ty.def_id;
    let mut lp = LastMod(AllPublic);
    let provenance = match pick.kind {
        probe::InherentImplPick(impl_def_id) => {
            if pick.method_ty.vis != ast::Public {
                lp = LastMod(DependsOn(def_id));
            }
            def::FromImpl(impl_def_id)
        }
        _ => def::FromTrait(pick.method_ty.container.id())
    };
    Ok((def::DefMethod(def_id, provenance), lp))
}


/// Find method with name `method_name` defined in `trait_def_id` and return it, along with its
/// index (or `None`, if no such method).
fn trait_method<'tcx>(tcx: &ty::ctxt<'tcx>,
                      trait_def_id: ast::DefId,
                      method_name: ast::Name)
                      -> Option<(usize, Rc<ty::Method<'tcx>>)>
{
    let trait_items = ty::trait_items(tcx, trait_def_id);
    trait_items
        .iter()
        .enumerate()
        .find(|&(_, ref item)| item.name() == method_name)
        .and_then(|(idx, item)| item.as_opt_method().map(|m| (idx, m)))
}

fn impl_method<'tcx>(tcx: &ty::ctxt<'tcx>,
                     impl_def_id: ast::DefId,
                     method_name: ast::Name)
                     -> Option<Rc<ty::Method<'tcx>>>
{
    let impl_items = tcx.impl_items.borrow();
    let impl_items = impl_items.get(&impl_def_id).unwrap();
    impl_items
        .iter()
        .map(|&did| ty::impl_or_trait_item(tcx, did.def_id()))
        .find(|m| m.name() == method_name)
        .and_then(|item| item.as_opt_method())
}
