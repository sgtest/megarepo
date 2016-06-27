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

use check::FnCtxt;
use hir::def::Def;
use hir::def_id::DefId;
use rustc::ty::subst;
use rustc::traits;
use rustc::ty::{self, ToPredicate, ToPolyTraitRef, TraitRef, TypeFoldable};
use rustc::ty::adjustment::{AdjustDerefRef, AutoDerefRef, AutoPtr};
use rustc::infer;

use syntax::ast;
use syntax_pos::Span;

use rustc::hir;

pub use self::MethodError::*;
pub use self::CandidateSource::*;

pub use self::suggest::AllTraitsVec;

mod confirm;
mod probe;
mod suggest;

pub enum MethodError<'tcx> {
    // Did not find an applicable method, but we did find various near-misses that may work.
    NoMatch(NoMatchData<'tcx>),

    // Multiple methods might apply.
    Ambiguity(Vec<CandidateSource>),

    // Using a `Fn`/`FnMut`/etc method on a raw closure type before we have inferred its kind.
    ClosureAmbiguity(/* DefId of fn trait */ DefId),

    // Found an applicable method, but it is not visible.
    PrivateMatch(Def),
}

// Contains a list of static methods that may apply, a list of unsatisfied trait predicates which
// could lead to matches if satisfied, and a list of not-in-scope traits which may work.
pub struct NoMatchData<'tcx> {
    pub static_candidates: Vec<CandidateSource>,
    pub unsatisfied_predicates: Vec<TraitRef<'tcx>>,
    pub out_of_scope_traits: Vec<DefId>,
    pub mode: probe::Mode
}

impl<'tcx> NoMatchData<'tcx> {
    pub fn new(static_candidates: Vec<CandidateSource>,
               unsatisfied_predicates: Vec<TraitRef<'tcx>>,
               out_of_scope_traits: Vec<DefId>,
               mode: probe::Mode) -> Self {
        NoMatchData {
            static_candidates: static_candidates,
            unsatisfied_predicates: unsatisfied_predicates,
            out_of_scope_traits: out_of_scope_traits,
            mode: mode
        }
    }
}

// A pared down enum describing just the places from which a method
// candidate can arise. Used for error reporting only.
#[derive(Copy, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CandidateSource {
    ImplSource(DefId),
    TraitSource(/* trait id */ DefId),
}

impl<'a, 'gcx, 'tcx> FnCtxt<'a, 'gcx, 'tcx> {
    /// Determines whether the type `self_ty` supports a method name `method_name` or not.
    pub fn method_exists(&self,
                         span: Span,
                         method_name: ast::Name,
                         self_ty: ty::Ty<'tcx>,
                         call_expr_id: ast::NodeId,
                         allow_private: bool)
                         -> bool
    {
        let mode = probe::Mode::MethodCall;
        match self.probe_method(span, mode, method_name, self_ty, call_expr_id) {
            Ok(..) => true,
            Err(NoMatch(..)) => false,
            Err(Ambiguity(..)) => true,
            Err(ClosureAmbiguity(..)) => true,
            Err(PrivateMatch(..)) => allow_private,
        }
    }

    /// Performs method lookup. If lookup is successful, it will return the callee
    /// and store an appropriate adjustment for the self-expr. In some cases it may
    /// report an error (e.g., invoking the `drop` method).
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
    pub fn lookup_method(&self,
                         span: Span,
                         method_name: ast::Name,
                         self_ty: ty::Ty<'tcx>,
                         supplied_method_types: Vec<ty::Ty<'tcx>>,
                         call_expr: &'gcx hir::Expr,
                         self_expr: &'gcx hir::Expr)
                         -> Result<ty::MethodCallee<'tcx>, MethodError<'tcx>>
    {
        debug!("lookup(method_name={}, self_ty={:?}, call_expr={:?}, self_expr={:?})",
               method_name,
               self_ty,
               call_expr,
               self_expr);

        let mode = probe::Mode::MethodCall;
        let self_ty = self.resolve_type_vars_if_possible(&self_ty);
        let pick = self.probe_method(span, mode, method_name, self_ty, call_expr.id)?;

        if let Some(import_id) = pick.import_id {
            self.tcx.used_trait_imports.borrow_mut().insert(import_id);
        }

        Ok(self.confirm_method(span, self_expr, call_expr, self_ty, pick, supplied_method_types))
    }

    pub fn lookup_method_in_trait(&self,
                                  span: Span,
                                  self_expr: Option<&hir::Expr>,
                                  m_name: ast::Name,
                                  trait_def_id: DefId,
                                  self_ty: ty::Ty<'tcx>,
                                  opt_input_types: Option<Vec<ty::Ty<'tcx>>>)
                                  -> Option<ty::MethodCallee<'tcx>>
    {
        self.lookup_method_in_trait_adjusted(span, self_expr, m_name, trait_def_id,
                                             0, false, self_ty, opt_input_types)
    }

    /// `lookup_in_trait_adjusted` is used for overloaded operators.
    /// It does a very narrow slice of what the normal probe/confirm path does.
    /// In particular, it doesn't really do any probing: it simply constructs
    /// an obligation for aparticular trait with the given self-type and checks
    /// whether that trait is implemented.
    ///
    /// FIXME(#18741) -- It seems likely that we can consolidate some of this
    /// code with the other method-lookup code. In particular, autoderef on
    /// index is basically identical to autoderef with normal probes, except
    /// that the test also looks for built-in indexing. Also, the second half of
    /// this method is basically the same as confirmation.
    pub fn lookup_method_in_trait_adjusted(&self,
                                           span: Span,
                                           self_expr: Option<&hir::Expr>,
                                           m_name: ast::Name,
                                           trait_def_id: DefId,
                                           autoderefs: usize,
                                           unsize: bool,
                                           self_ty: ty::Ty<'tcx>,
                                           opt_input_types: Option<Vec<ty::Ty<'tcx>>>)
                                           -> Option<ty::MethodCallee<'tcx>>
    {
        debug!("lookup_in_trait_adjusted(self_ty={:?}, self_expr={:?}, \
                m_name={}, trait_def_id={:?})",
               self_ty,
               self_expr,
               m_name,
               trait_def_id);

        let trait_def = self.tcx.lookup_trait_def(trait_def_id);

        let type_parameter_defs = trait_def.generics.types.get_slice(subst::TypeSpace);
        let expected_number_of_input_types = type_parameter_defs.len();

        assert_eq!(trait_def.generics.types.len(subst::FnSpace), 0);
        assert!(trait_def.generics.regions.is_empty());

        // Construct a trait-reference `self_ty : Trait<input_tys>`
        let mut substs = subst::Substs::new_trait(Vec::new(), Vec::new(), self_ty);

        match opt_input_types {
            Some(input_types) => {
                assert_eq!(expected_number_of_input_types, input_types.len());
                substs.types.replace(subst::ParamSpace::TypeSpace, input_types);
            }

            None => {
                self.type_vars_for_defs(
                    span,
                    subst::ParamSpace::TypeSpace,
                    &mut substs,
                    type_parameter_defs);
            }
        }

        let trait_ref = ty::TraitRef::new(trait_def_id, self.tcx.mk_substs(substs));

        // Construct an obligation
        let poly_trait_ref = trait_ref.to_poly_trait_ref();
        let obligation = traits::Obligation::misc(span,
                                                  self.body_id,
                                                  poly_trait_ref.to_predicate());

        // Now we want to know if this can be matched
        let mut selcx = traits::SelectionContext::new(self);
        if !selcx.evaluate_obligation(&obligation) {
            debug!("--> Cannot match obligation");
            return None; // Cannot be matched, no such method resolution is possible.
        }

        // Trait must have a method named `m_name` and it should not have
        // type parameters or early-bound regions.
        let tcx = self.tcx;
        let method_item = self.trait_item(trait_def_id, m_name).unwrap();
        let method_ty = method_item.as_opt_method().unwrap();
        assert_eq!(method_ty.generics.types.len(subst::FnSpace), 0);
        assert_eq!(method_ty.generics.regions.len(subst::FnSpace), 0);

        debug!("lookup_in_trait_adjusted: method_item={:?} method_ty={:?}",
               method_item, method_ty);

        // Instantiate late-bound regions and substitute the trait
        // parameters into the method type to get the actual method type.
        //
        // NB: Instantiate late-bound regions first so that
        // `instantiate_type_scheme` can normalize associated types that
        // may reference those regions.
        let fn_sig = self.replace_late_bound_regions_with_fresh_var(span,
                                                                    infer::FnCall,
                                                                    &method_ty.fty.sig).0;
        let fn_sig = self.instantiate_type_scheme(span, trait_ref.substs, &fn_sig);
        let transformed_self_ty = fn_sig.inputs[0];
        let def_id = method_item.def_id();
        let fty = tcx.mk_fn_def(def_id, trait_ref.substs,
                                tcx.mk_bare_fn(ty::BareFnTy {
            sig: ty::Binder(fn_sig),
            unsafety: method_ty.fty.unsafety,
            abi: method_ty.fty.abi.clone(),
        }));

        debug!("lookup_in_trait_adjusted: matched method fty={:?} obligation={:?}",
               fty,
               obligation);

        // Register obligations for the parameters.  This will include the
        // `Self` parameter, which in turn has a bound of the main trait,
        // so this also effectively registers `obligation` as well.  (We
        // used to register `obligation` explicitly, but that resulted in
        // double error messages being reported.)
        //
        // Note that as the method comes from a trait, it should not have
        // any late-bound regions appearing in its bounds.
        let method_bounds = self.instantiate_bounds(span, trait_ref.substs, &method_ty.predicates);
        assert!(!method_bounds.has_escaping_regions());
        self.add_obligations_for_parameters(
            traits::ObligationCause::misc(span, self.body_id),
            &method_bounds);

        // Also register an obligation for the method type being well-formed.
        self.register_wf_obligation(fty, span, traits::MiscObligation);

        // FIXME(#18653) -- Try to resolve obligations, giving us more
        // typing information, which can sometimes be needed to avoid
        // pathological region inference failures.
        self.select_obligations_where_possible();

        // Insert any adjustments needed (always an autoref of some mutability).
        match self_expr {
            None => { }

            Some(self_expr) => {
                debug!("lookup_in_trait_adjusted: inserting adjustment if needed \
                       (self-id={}, autoderefs={}, unsize={}, explicit_self={:?})",
                       self_expr.id, autoderefs, unsize,
                       method_ty.explicit_self);

                match method_ty.explicit_self {
                    ty::ExplicitSelfCategory::ByValue => {
                        // Trait method is fn(self), no transformation needed.
                        assert!(!unsize);
                        self.write_autoderef_adjustment(self_expr.id, autoderefs);
                    }

                    ty::ExplicitSelfCategory::ByReference(..) => {
                        // Trait method is fn(&self) or fn(&mut self), need an
                        // autoref. Pull the region etc out of the type of first argument.
                        match transformed_self_ty.sty {
                            ty::TyRef(region, ty::TypeAndMut { mutbl, ty: _ }) => {
                                self.write_adjustment(self_expr.id,
                                    AdjustDerefRef(AutoDerefRef {
                                        autoderefs: autoderefs,
                                        autoref: Some(AutoPtr(region, mutbl)),
                                        unsize: if unsize {
                                            Some(transformed_self_ty)
                                        } else {
                                            None
                                        }
                                    }));
                            }

                            _ => {
                                span_bug!(
                                    span,
                                    "trait method is &self but first arg is: {}",
                                    transformed_self_ty);
                            }
                        }
                    }

                    _ => {
                        span_bug!(
                            span,
                            "unexpected explicit self type in operator method: {:?}",
                            method_ty.explicit_self);
                    }
                }
            }
        }

        let callee = ty::MethodCallee {
            def_id: def_id,
            ty: fty,
            substs: trait_ref.substs
        };

        debug!("callee = {:?}", callee);

        Some(callee)
    }

    pub fn resolve_ufcs(&self,
                        span: Span,
                        method_name: ast::Name,
                        self_ty: ty::Ty<'tcx>,
                        expr_id: ast::NodeId)
                        -> Result<Def, MethodError<'tcx>>
    {
        let mode = probe::Mode::Path;
        let pick = self.probe_method(span, mode, method_name, self_ty, expr_id)?;

        if let Some(import_id) = pick.import_id {
            self.tcx.used_trait_imports.borrow_mut().insert(import_id);
        }

        let def = pick.item.def();
        if let probe::InherentImplPick = pick.kind {
            if !pick.item.vis().is_accessible_from(self.body_id, &self.tcx.map) {
                let msg = format!("{} `{}` is private", def.kind_name(), &method_name.as_str());
                self.tcx.sess.span_err(span, &msg);
            }
        }
        Ok(def)
    }

    /// Find item with name `item_name` defined in `trait_def_id`
    /// and return it, or `None`, if no such item.
    pub fn trait_item(&self,
                      trait_def_id: DefId,
                      item_name: ast::Name)
                      -> Option<ty::ImplOrTraitItem<'tcx>>
    {
        let trait_items = self.tcx.trait_items(trait_def_id);
        trait_items.iter()
                   .find(|item| item.name() == item_name)
                   .cloned()
    }

    pub fn impl_item(&self,
                     impl_def_id: DefId,
                     item_name: ast::Name)
                     -> Option<ty::ImplOrTraitItem<'tcx>>
    {
        let impl_items = self.tcx.impl_items.borrow();
        let impl_items = impl_items.get(&impl_def_id).unwrap();
        impl_items
            .iter()
            .map(|&did| self.tcx.impl_or_trait_item(did.def_id()))
            .find(|m| m.name() == item_name)
    }
}
