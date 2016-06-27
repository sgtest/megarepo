// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::{
    FulfillmentError,
    FulfillmentErrorCode,
    MismatchedProjectionTypes,
    Obligation,
    ObligationCause,
    ObligationCauseCode,
    OutputTypeParameterMismatch,
    TraitNotObjectSafe,
    PredicateObligation,
    SelectionContext,
    SelectionError,
    ObjectSafetyViolation,
    MethodViolationCode,
};

use fmt_macros::{Parser, Piece, Position};
use hir::def_id::DefId;
use infer::{InferCtxt};
use ty::{self, ToPredicate, ToPolyTraitRef, Ty, TyCtxt, TypeFoldable};
use ty::fast_reject;
use ty::fold::TypeFolder;
use ty::subst::{self, Subst, TypeSpace};
use util::nodemap::{FnvHashMap, FnvHashSet};

use std::cmp;
use std::fmt;
use syntax::ast;
use syntax::attr::{AttributeMethods, AttrMetaMethods};
use syntax_pos::Span;
use errors::DiagnosticBuilder;

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct TraitErrorKey<'tcx> {
    span: Span,
    warning_node_id: Option<ast::NodeId>,
    predicate: ty::Predicate<'tcx>
}

impl<'a, 'gcx, 'tcx> TraitErrorKey<'tcx> {
    fn from_error(infcx: &InferCtxt<'a, 'gcx, 'tcx>,
                  e: &FulfillmentError<'tcx>,
                  warning_node_id: Option<ast::NodeId>) -> Self {
        let predicate =
            infcx.resolve_type_vars_if_possible(&e.obligation.predicate);
        TraitErrorKey {
            span: e.obligation.cause.span,
            predicate: infcx.tcx.erase_regions(&predicate),
            warning_node_id: warning_node_id
        }
    }
}

impl<'a, 'gcx, 'tcx> InferCtxt<'a, 'gcx, 'tcx> {
    pub fn report_fulfillment_errors(&self, errors: &Vec<FulfillmentError<'tcx>>) {
        for error in errors {
            self.report_fulfillment_error(error, None);
        }
    }

    pub fn report_fulfillment_errors_as_warnings(&self,
                                                 errors: &Vec<FulfillmentError<'tcx>>,
                                                 node_id: ast::NodeId) {
        for error in errors {
            self.report_fulfillment_error(error, Some(node_id));
        }
    }

    fn report_fulfillment_error(&self,
                                error: &FulfillmentError<'tcx>,
                                warning_node_id: Option<ast::NodeId>) {
        let error_key = TraitErrorKey::from_error(self, error, warning_node_id);
        debug!("report_fulfillment_errors({:?}) - key={:?}",
               error, error_key);
        if !self.reported_trait_errors.borrow_mut().insert(error_key) {
            debug!("report_fulfillment_errors: skipping duplicate");
            return;
        }
        match error.code {
            FulfillmentErrorCode::CodeSelectionError(ref e) => {
                self.report_selection_error(&error.obligation, e, warning_node_id);
            }
            FulfillmentErrorCode::CodeProjectionError(ref e) => {
                self.report_projection_error(&error.obligation, e, warning_node_id);
            }
            FulfillmentErrorCode::CodeAmbiguity => {
                self.maybe_report_ambiguity(&error.obligation);
            }
        }
    }

    fn report_projection_error(&self,
                               obligation: &PredicateObligation<'tcx>,
                               error: &MismatchedProjectionTypes<'tcx>,
                               warning_node_id: Option<ast::NodeId>)
    {
        let predicate =
            self.resolve_type_vars_if_possible(&obligation.predicate);

        if !predicate.references_error() {
            if let Some(warning_node_id) = warning_node_id {
                self.tcx.sess.add_lint(
                    ::lint::builtin::UNSIZED_IN_TUPLE,
                    warning_node_id,
                    obligation.cause.span,
                    format!("type mismatch resolving `{}`: {}",
                            predicate,
                            error.err));
            } else {
                let mut err = struct_span_err!(self.tcx.sess, obligation.cause.span, E0271,
                                               "type mismatch resolving `{}`: {}",
                                               predicate,
                                               error.err);
                self.note_obligation_cause(&mut err, obligation);
                err.emit();
            }
        }
    }

    fn impl_substs(&self,
                   did: DefId,
                   obligation: PredicateObligation<'tcx>)
                   -> subst::Substs<'tcx> {
        let tcx = self.tcx;

        let ity = tcx.lookup_item_type(did);
        let (tps, rps, _) =
            (ity.generics.types.get_slice(TypeSpace),
             ity.generics.regions.get_slice(TypeSpace),
             ity.ty);

        let rps = self.region_vars_for_defs(obligation.cause.span, rps);
        let mut substs = subst::Substs::new(
            subst::VecPerParamSpace::empty(),
            subst::VecPerParamSpace::new(rps, Vec::new(), Vec::new()));
        self.type_vars_for_defs(obligation.cause.span,
                                TypeSpace,
                                &mut substs,
                                tps);
        substs
    }

    fn fuzzy_match_tys(&self, a: Ty<'tcx>, b: Ty<'tcx>) -> bool {
        /// returns the fuzzy category of a given type, or None
        /// if the type can be equated to any type.
        fn type_category<'tcx>(t: Ty<'tcx>) -> Option<u32> {
            match t.sty {
                ty::TyBool => Some(0),
                ty::TyChar => Some(1),
                ty::TyStr => Some(2),
                ty::TyInt(..) | ty::TyUint(..) |
                ty::TyInfer(ty::IntVar(..)) => Some(3),
                ty::TyFloat(..) | ty::TyInfer(ty::FloatVar(..)) => Some(4),
                ty::TyEnum(..) => Some(5),
                ty::TyStruct(..) => Some(6),
                ty::TyBox(..) | ty::TyRef(..) | ty::TyRawPtr(..) => Some(7),
                ty::TyArray(..) | ty::TySlice(..) => Some(8),
                ty::TyFnDef(..) | ty::TyFnPtr(..) => Some(9),
                ty::TyTrait(..) => Some(10),
                ty::TyClosure(..) => Some(11),
                ty::TyTuple(..) => Some(12),
                ty::TyProjection(..) => Some(13),
                ty::TyParam(..) => Some(14),
                ty::TyInfer(..) | ty::TyError => None
            }
        }

        match (type_category(a), type_category(b)) {
            (Some(cat_a), Some(cat_b)) => match (&a.sty, &b.sty) {
                (&ty::TyStruct(def_a, _), &ty::TyStruct(def_b, _)) |
                (&ty::TyEnum(def_a, _), &ty::TyEnum(def_b, _)) =>
                    def_a == def_b,
                _ => cat_a == cat_b
            },
            // infer and error can be equated to all types
            _ => true
        }
    }

    fn impl_similar_to(&self,
                       trait_ref: ty::PolyTraitRef<'tcx>,
                       obligation: &PredicateObligation<'tcx>)
                       -> Option<DefId>
    {
        let tcx = self.tcx;

        let trait_ref = tcx.erase_late_bound_regions(&trait_ref);
        let trait_self_ty = trait_ref.self_ty();

        let mut self_match_impls = vec![];
        let mut fuzzy_match_impls = vec![];

        self.tcx.lookup_trait_def(trait_ref.def_id)
            .for_each_relevant_impl(self.tcx, trait_self_ty, |def_id| {
                let impl_trait_ref = tcx
                    .impl_trait_ref(def_id)
                    .unwrap()
                    .subst(tcx, &self.impl_substs(def_id, obligation.clone()));

                let impl_self_ty = impl_trait_ref.self_ty();

                if let Ok(..) = self.can_equate(&trait_self_ty, &impl_self_ty) {
                    self_match_impls.push(def_id);

                    if trait_ref.substs.types.get_slice(TypeSpace).iter()
                        .zip(impl_trait_ref.substs.types.get_slice(TypeSpace))
                        .all(|(u,v)| self.fuzzy_match_tys(u, v))
                    {
                        fuzzy_match_impls.push(def_id);
                    }
                }
            });

        let impl_def_id = if self_match_impls.len() == 1 {
            self_match_impls[0]
        } else if fuzzy_match_impls.len() == 1 {
            fuzzy_match_impls[0]
        } else {
            return None
        };

        if tcx.has_attr(impl_def_id, "rustc_on_unimplemented") {
            Some(impl_def_id)
        } else {
            None
        }
    }

    fn on_unimplemented_note(&self,
                             trait_ref: ty::PolyTraitRef<'tcx>,
                             obligation: &PredicateObligation<'tcx>) -> Option<String> {
        let def_id = self.impl_similar_to(trait_ref, obligation)
            .unwrap_or(trait_ref.def_id());
        let trait_ref = trait_ref.skip_binder();

        let span = obligation.cause.span;
        let mut report = None;
        for item in self.tcx.get_attrs(def_id).iter() {
            if item.check_name("rustc_on_unimplemented") {
                let err_sp = item.meta().span.substitute_dummy(span);
                let def = self.tcx.lookup_trait_def(trait_ref.def_id);
                let trait_str = def.trait_ref.to_string();
                if let Some(ref istring) = item.value_str() {
                    let mut generic_map = def.generics.types.iter_enumerated()
                                             .map(|(param, i, gen)| {
                                                   (gen.name.as_str().to_string(),
                                                    trait_ref.substs.types.get(param, i)
                                                             .to_string())
                                                  }).collect::<FnvHashMap<String, String>>();
                    generic_map.insert("Self".to_string(),
                                       trait_ref.self_ty().to_string());
                    let parser = Parser::new(&istring);
                    let mut errored = false;
                    let err: String = parser.filter_map(|p| {
                        match p {
                            Piece::String(s) => Some(s),
                            Piece::NextArgument(a) => match a.position {
                                Position::ArgumentNamed(s) => match generic_map.get(s) {
                                    Some(val) => Some(val),
                                    None => {
                                        span_err!(self.tcx.sess, err_sp, E0272,
                                                       "the #[rustc_on_unimplemented] \
                                                                attribute on \
                                                                trait definition for {} refers to \
                                                                non-existent type parameter {}",
                                                               trait_str, s);
                                        errored = true;
                                        None
                                    }
                                },
                                _ => {
                                    span_err!(self.tcx.sess, err_sp, E0273,
                                              "the #[rustc_on_unimplemented] attribute \
                                               on trait definition for {} must have \
                                               named format arguments, eg \
                                               `#[rustc_on_unimplemented = \
                                                \"foo {{T}}\"]`", trait_str);
                                    errored = true;
                                    None
                                }
                            }
                        }
                    }).collect();
                    // Report only if the format string checks out
                    if !errored {
                        report = Some(err);
                    }
                } else {
                    span_err!(self.tcx.sess, err_sp, E0274,
                                            "the #[rustc_on_unimplemented] attribute on \
                                                     trait definition for {} must have a value, \
                                                     eg `#[rustc_on_unimplemented = \"foo\"]`",
                                                     trait_str);
                }
                break;
            }
        }
        report
    }

    fn find_similar_impl_candidates(&self,
                                    trait_ref: ty::PolyTraitRef<'tcx>)
                                    -> Vec<ty::TraitRef<'tcx>>
    {
        let simp = fast_reject::simplify_type(self.tcx,
                                              trait_ref.skip_binder().self_ty(),
                                              true);
        let mut impl_candidates = Vec::new();
        let trait_def = self.tcx.lookup_trait_def(trait_ref.def_id());

        match simp {
            Some(simp) => trait_def.for_each_impl(self.tcx, |def_id| {
                let imp = self.tcx.impl_trait_ref(def_id).unwrap();
                let imp_simp = fast_reject::simplify_type(self.tcx,
                                                          imp.self_ty(),
                                                          true);
                if let Some(imp_simp) = imp_simp {
                    if simp != imp_simp {
                        return;
                    }
                }
                impl_candidates.push(imp);
            }),
            None => trait_def.for_each_impl(self.tcx, |def_id| {
                impl_candidates.push(
                    self.tcx.impl_trait_ref(def_id).unwrap());
            })
        };
        impl_candidates
    }

    fn report_similar_impl_candidates(&self,
                                      trait_ref: ty::PolyTraitRef<'tcx>,
                                      err: &mut DiagnosticBuilder)
    {
        let simp = fast_reject::simplify_type(self.tcx,
                                              trait_ref.skip_binder().self_ty(),
                                              true);
        let mut impl_candidates = Vec::new();
        let trait_def = self.tcx.lookup_trait_def(trait_ref.def_id());

        match simp {
            Some(simp) => trait_def.for_each_impl(self.tcx, |def_id| {
                let imp = self.tcx.impl_trait_ref(def_id).unwrap();
                let imp_simp = fast_reject::simplify_type(self.tcx,
                                                          imp.self_ty(),
                                                          true);
                if let Some(imp_simp) = imp_simp {
                    if simp != imp_simp {
                        return;
                    }
                }
                impl_candidates.push(imp);
            }),
            None => trait_def.for_each_impl(self.tcx, |def_id| {
                impl_candidates.push(
                    self.tcx.impl_trait_ref(def_id).unwrap());
            })
        };

        if impl_candidates.is_empty() {
            return;
        }

        err.help(&format!("the following implementations were found:"));

        let end = cmp::min(4, impl_candidates.len());
        for candidate in &impl_candidates[0..end] {
            err.help(&format!("  {:?}", candidate));
        }
        if impl_candidates.len() > 4 {
            err.help(&format!("and {} others", impl_candidates.len()-4));
        }
    }

    /// Reports that an overflow has occurred and halts compilation. We
    /// halt compilation unconditionally because it is important that
    /// overflows never be masked -- they basically represent computations
    /// whose result could not be truly determined and thus we can't say
    /// if the program type checks or not -- and they are unusual
    /// occurrences in any case.
    pub fn report_overflow_error<T>(&self,
                                    obligation: &Obligation<'tcx, T>,
                                    suggest_increasing_limit: bool) -> !
        where T: fmt::Display + TypeFoldable<'tcx>
    {
        let predicate =
            self.resolve_type_vars_if_possible(&obligation.predicate);
        let mut err = struct_span_err!(self.tcx.sess, obligation.cause.span, E0275,
                                       "overflow evaluating the requirement `{}`",
                                       predicate);

        if suggest_increasing_limit {
            self.suggest_new_overflow_limit(&mut err);
        }

        self.note_obligation_cause(&mut err, obligation);

        err.emit();
        self.tcx.sess.abort_if_errors();
        bug!();
    }

    /// Reports that a cycle was detected which led to overflow and halts
    /// compilation. This is equivalent to `report_overflow_error` except
    /// that we can give a more helpful error message (and, in particular,
    /// we do not suggest increasing the overflow limit, which is not
    /// going to help).
    pub fn report_overflow_error_cycle(&self, cycle: &[PredicateObligation<'tcx>]) -> ! {
        let cycle = self.resolve_type_vars_if_possible(&cycle.to_owned());
        assert!(cycle.len() > 0);

        debug!("report_overflow_error_cycle: cycle={:?}", cycle);

        self.report_overflow_error(&cycle[0], false);
    }

    pub fn report_selection_error(&self,
                                  obligation: &PredicateObligation<'tcx>,
                                  error: &SelectionError<'tcx>,
                                  warning_node_id: Option<ast::NodeId>)
    {
        let span = obligation.cause.span;
        let mut err = match *error {
            SelectionError::Unimplemented => {
                if let ObligationCauseCode::CompareImplMethodObligation = obligation.cause.code {
                    span_err!(
                        self.tcx.sess, span, E0276,
                        "the requirement `{}` appears on the impl \
                         method but not on the corresponding trait method",
                        obligation.predicate);
                    return;
                } else {
                    match obligation.predicate {
                        ty::Predicate::Trait(ref trait_predicate) => {
                            let trait_predicate =
                                self.resolve_type_vars_if_possible(trait_predicate);

                            if self.tcx.sess.has_errors() && trait_predicate.references_error() {
                                return;
                            } else {
                                let trait_ref = trait_predicate.to_poly_trait_ref();

                                if let Some(warning_node_id) = warning_node_id {
                                    self.tcx.sess.add_lint(
                                        ::lint::builtin::UNSIZED_IN_TUPLE,
                                        warning_node_id,
                                        obligation.cause.span,
                                        format!("the trait bound `{}` is not satisfied",
                                                trait_ref.to_predicate()));
                                    return;
                                }

                                let mut err = struct_span_err!(
                                    self.tcx.sess, span, E0277,
                                    "the trait bound `{}` is not satisfied",
                                    trait_ref.to_predicate());

                                // Try to report a help message

                                if !trait_ref.has_infer_types() &&
                                    self.predicate_can_apply(trait_ref) {
                                    // If a where-clause may be useful, remind the
                                    // user that they can add it.
                                    //
                                    // don't display an on-unimplemented note, as
                                    // these notes will often be of the form
                                    //     "the type `T` can't be frobnicated"
                                    // which is somewhat confusing.
                                    err.help(&format!("consider adding a `where {}` bound",
                                                      trait_ref.to_predicate()));
                                } else if let Some(s) = self.on_unimplemented_note(trait_ref,
                                                                                   obligation) {
                                    // If it has a custom "#[rustc_on_unimplemented]"
                                    // error message, let's display it!
                                    err.note(&s);
                                } else {
                                    // If we can't show anything useful, try to find
                                    // similar impls.
                                    let impl_candidates =
                                        self.find_similar_impl_candidates(trait_ref);
                                    if impl_candidates.len() > 0 {
                                        self.report_similar_impl_candidates(trait_ref, &mut err);
                                    }
                                }
                                err
                            }
                        }

                        ty::Predicate::Equate(ref predicate) => {
                            let predicate = self.resolve_type_vars_if_possible(predicate);
                            let err = self.equality_predicate(span,
                                                              &predicate).err().unwrap();
                            struct_span_err!(self.tcx.sess, span, E0278,
                                "the requirement `{}` is not satisfied (`{}`)",
                                predicate, err)
                        }

                        ty::Predicate::RegionOutlives(ref predicate) => {
                            let predicate = self.resolve_type_vars_if_possible(predicate);
                            let err = self.region_outlives_predicate(span,
                                                                     &predicate).err().unwrap();
                            struct_span_err!(self.tcx.sess, span, E0279,
                                "the requirement `{}` is not satisfied (`{}`)",
                                predicate, err)
                        }

                        ty::Predicate::Projection(..) | ty::Predicate::TypeOutlives(..) => {
                            let predicate =
                                self.resolve_type_vars_if_possible(&obligation.predicate);
                            struct_span_err!(self.tcx.sess, span, E0280,
                                "the requirement `{}` is not satisfied",
                                predicate)
                        }

                        ty::Predicate::ObjectSafe(trait_def_id) => {
                            let violations = self.tcx.object_safety_violations(trait_def_id);
                            let err = self.tcx.report_object_safety_error(span,
                                                                          trait_def_id,
                                                                          warning_node_id,
                                                                          violations);
                            if let Some(err) = err {
                                err
                            } else {
                                return;
                            }
                        }

                        ty::Predicate::ClosureKind(closure_def_id, kind) => {
                            let found_kind = self.closure_kind(closure_def_id).unwrap();
                            let closure_span = self.tcx.map.span_if_local(closure_def_id).unwrap();
                            let mut err = struct_span_err!(
                                self.tcx.sess, closure_span, E0525,
                                "expected a closure that implements the `{}` trait, \
                                 but this closure only implements `{}`",
                                kind,
                                found_kind);
                            err.span_note(
                                obligation.cause.span,
                                &format!("the requirement to implement \
                                          `{}` derives from here", kind));
                            err.emit();
                            return;
                        }

                        ty::Predicate::WellFormed(ty) => {
                            // WF predicates cannot themselves make
                            // errors. They can only block due to
                            // ambiguity; otherwise, they always
                            // degenerate into other obligations
                            // (which may fail).
                            span_bug!(span, "WF predicate not satisfied for {:?}", ty);
                        }

                        ty::Predicate::Rfc1592(ref data) => {
                            span_bug!(
                                obligation.cause.span,
                                "RFC1592 predicate not satisfied for {:?}",
                                data);
                        }
                    }
                }
            }

            OutputTypeParameterMismatch(ref expected_trait_ref, ref actual_trait_ref, ref e) => {
                let expected_trait_ref = self.resolve_type_vars_if_possible(&*expected_trait_ref);
                let actual_trait_ref = self.resolve_type_vars_if_possible(&*actual_trait_ref);
                if actual_trait_ref.self_ty().references_error() {
                    return;
                }
                struct_span_err!(self.tcx.sess, span, E0281,
                    "type mismatch: the type `{}` implements the trait `{}`, \
                     but the trait `{}` is required ({})",
                    expected_trait_ref.self_ty(),
                    expected_trait_ref,
                    actual_trait_ref,
                    e)
            }

            TraitNotObjectSafe(did) => {
                let violations = self.tcx.object_safety_violations(did);
                let err = self.tcx.report_object_safety_error(span, did,
                                                              warning_node_id,
                                                              violations);
                if let Some(err) = err {
                    err
                } else {
                    return;
                }
            }
        };
        self.note_obligation_cause(&mut err, obligation);
        err.emit();
    }
}

impl<'a, 'gcx, 'tcx> TyCtxt<'a, 'gcx, 'tcx> {
    pub fn recursive_type_with_infinite_size_error(self,
                                                   type_def_id: DefId)
                                                   -> DiagnosticBuilder<'tcx>
    {
        assert!(type_def_id.is_local());
        let span = self.map.span_if_local(type_def_id).unwrap();
        let mut err = struct_span_err!(self.sess, span, E0072,
                                       "recursive type `{}` has infinite size",
                                       self.item_path_str(type_def_id));
        err.help(&format!("insert indirection (e.g., a `Box`, `Rc`, or `&`) \
                           at some point to make `{}` representable",
                          self.item_path_str(type_def_id)));
        err
    }

    pub fn report_object_safety_error(self,
                                      span: Span,
                                      trait_def_id: DefId,
                                      warning_node_id: Option<ast::NodeId>,
                                      violations: Vec<ObjectSafetyViolation>)
                                      -> Option<DiagnosticBuilder<'tcx>>
    {
        let mut err = match warning_node_id {
            Some(_) => None,
            None => {
                Some(struct_span_err!(
                    self.sess, span, E0038,
                    "the trait `{}` cannot be made into an object",
                    self.item_path_str(trait_def_id)))
            }
        };

        let mut reported_violations = FnvHashSet();
        for violation in violations {
            if !reported_violations.insert(violation.clone()) {
                continue;
            }
            let buf;
            let note = match violation {
                ObjectSafetyViolation::SizedSelf => {
                    "the trait cannot require that `Self : Sized`"
                }

                ObjectSafetyViolation::SupertraitSelf => {
                    "the trait cannot use `Self` as a type parameter \
                         in the supertrait listing"
                }

                ObjectSafetyViolation::Method(method,
                                              MethodViolationCode::StaticMethod) => {
                    buf = format!("method `{}` has no receiver",
                                  method.name);
                    &buf
                }

                ObjectSafetyViolation::Method(method,
                                              MethodViolationCode::ReferencesSelf) => {
                    buf = format!("method `{}` references the `Self` type \
                                       in its arguments or return type",
                                  method.name);
                    &buf
                }

                ObjectSafetyViolation::Method(method,
                                              MethodViolationCode::Generic) => {
                    buf = format!("method `{}` has generic type parameters",
                                  method.name);
                    &buf
                }
            };
            match (warning_node_id, &mut err) {
                (Some(node_id), &mut None) => {
                    self.sess.add_lint(
                        ::lint::builtin::OBJECT_UNSAFE_FRAGMENT,
                        node_id,
                        span,
                        note.to_string());
                }
                (None, &mut Some(ref mut err)) => {
                    err.note(note);
                }
                _ => unreachable!()
            }
        }
        err
    }
}

impl<'a, 'gcx, 'tcx> InferCtxt<'a, 'gcx, 'tcx> {
    fn maybe_report_ambiguity(&self, obligation: &PredicateObligation<'tcx>) {
        // Unable to successfully determine, probably means
        // insufficient type information, but could mean
        // ambiguous impls. The latter *ought* to be a
        // coherence violation, so we don't report it here.

        let predicate = self.resolve_type_vars_if_possible(&obligation.predicate);

        debug!("maybe_report_ambiguity(predicate={:?}, obligation={:?})",
               predicate,
               obligation);

        // Ambiguity errors are often caused as fallout from earlier
        // errors. So just ignore them if this infcx is tainted.
        if self.is_tainted_by_errors() {
            return;
        }

        match predicate {
            ty::Predicate::Trait(ref data) => {
                let trait_ref = data.to_poly_trait_ref();
                let self_ty = trait_ref.self_ty();
                let all_types = &trait_ref.substs().types;
                if all_types.references_error() {
                } else {
                    // Typically, this ambiguity should only happen if
                    // there are unresolved type inference variables
                    // (otherwise it would suggest a coherence
                    // failure). But given #21974 that is not necessarily
                    // the case -- we can have multiple where clauses that
                    // are only distinguished by a region, which results
                    // in an ambiguity even when all types are fully
                    // known, since we don't dispatch based on region
                    // relationships.

                    // This is kind of a hack: it frequently happens that some earlier
                    // error prevents types from being fully inferred, and then we get
                    // a bunch of uninteresting errors saying something like "<generic
                    // #0> doesn't implement Sized".  It may even be true that we
                    // could just skip over all checks where the self-ty is an
                    // inference variable, but I was afraid that there might be an
                    // inference variable created, registered as an obligation, and
                    // then never forced by writeback, and hence by skipping here we'd
                    // be ignoring the fact that we don't KNOW the type works
                    // out. Though even that would probably be harmless, given that
                    // we're only talking about builtin traits, which are known to be
                    // inhabited. But in any case I just threw in this check for
                    // has_errors() to be sure that compilation isn't happening
                    // anyway. In that case, why inundate the user.
                    if !self.tcx.sess.has_errors() {
                        if
                            self.tcx.lang_items.sized_trait()
                            .map_or(false, |sized_id| sized_id == trait_ref.def_id())
                        {
                            self.need_type_info(obligation.cause.span, self_ty);
                        } else {
                            let mut err = struct_span_err!(self.tcx.sess,
                                                           obligation.cause.span, E0283,
                                                           "type annotations required: \
                                                            cannot resolve `{}`",
                                                           predicate);
                            self.note_obligation_cause(&mut err, obligation);
                            err.emit();
                        }
                    }
                }
            }

            ty::Predicate::WellFormed(ty) => {
                // Same hacky approach as above to avoid deluging user
                // with error messages.
                if !ty.references_error() && !self.tcx.sess.has_errors() {
                    self.need_type_info(obligation.cause.span, ty);
                }
            }

            _ => {
                if !self.tcx.sess.has_errors() {
                    let mut err = struct_span_err!(self.tcx.sess,
                                                   obligation.cause.span, E0284,
                                                   "type annotations required: \
                                                    cannot resolve `{}`",
                                                   predicate);
                    self.note_obligation_cause(&mut err, obligation);
                    err.emit();
                }
            }
        }
    }

    /// Returns whether the trait predicate may apply for *some* assignment
    /// to the type parameters.
    fn predicate_can_apply(&self, pred: ty::PolyTraitRef<'tcx>) -> bool {
        struct ParamToVarFolder<'a, 'gcx: 'a+'tcx, 'tcx: 'a> {
            infcx: &'a InferCtxt<'a, 'gcx, 'tcx>,
            var_map: FnvHashMap<Ty<'tcx>, Ty<'tcx>>
        }

        impl<'a, 'gcx, 'tcx> TypeFolder<'gcx, 'tcx> for ParamToVarFolder<'a, 'gcx, 'tcx> {
            fn tcx<'b>(&'b self) -> TyCtxt<'b, 'gcx, 'tcx> { self.infcx.tcx }

            fn fold_ty(&mut self, ty: Ty<'tcx>) -> Ty<'tcx> {
                if let ty::TyParam(..) = ty.sty {
                    let infcx = self.infcx;
                    self.var_map.entry(ty).or_insert_with(|| infcx.next_ty_var())
                } else {
                    ty.super_fold_with(self)
                }
            }
        }

        self.probe(|_| {
            let mut selcx = SelectionContext::new(self);

            let cleaned_pred = pred.fold_with(&mut ParamToVarFolder {
                infcx: self,
                var_map: FnvHashMap()
            });

            let cleaned_pred = super::project::normalize(
                &mut selcx,
                ObligationCause::dummy(),
                &cleaned_pred
            ).value;

            let obligation = Obligation::new(
                ObligationCause::dummy(),
                cleaned_pred.to_predicate()
            );

            selcx.evaluate_obligation(&obligation)
        })
    }


    fn need_type_info(&self, span: Span, ty: Ty<'tcx>) {
        span_err!(self.tcx.sess, span, E0282,
                  "unable to infer enough type information about `{}`; \
                   type annotations or generic parameter binding required",
                  ty);
    }

    fn note_obligation_cause<T>(&self,
                                err: &mut DiagnosticBuilder,
                                obligation: &Obligation<'tcx, T>)
        where T: fmt::Display
    {
        self.note_obligation_cause_code(err,
                                        &obligation.predicate,
                                        &obligation.cause.code);
    }

    fn note_obligation_cause_code<T>(&self,
                                     err: &mut DiagnosticBuilder,
                                     predicate: &T,
                                     cause_code: &ObligationCauseCode<'tcx>)
        where T: fmt::Display
    {
        let tcx = self.tcx;
        match *cause_code {
            ObligationCauseCode::MiscObligation => { }
            ObligationCauseCode::SliceOrArrayElem => {
                err.note("slice and array elements must have `Sized` type");
            }
            ObligationCauseCode::TupleElem => {
                err.note("tuple elements must have `Sized` type");
            }
            ObligationCauseCode::ProjectionWf(data) => {
                err.note(&format!("required so that the projection `{}` is well-formed",
                                  data));
            }
            ObligationCauseCode::ReferenceOutlivesReferent(ref_ty) => {
                err.note(&format!("required so that reference `{}` does not outlive its referent",
                                  ref_ty));
            }
            ObligationCauseCode::ItemObligation(item_def_id) => {
                let item_name = tcx.item_path_str(item_def_id);
                err.note(&format!("required by `{}`", item_name));
            }
            ObligationCauseCode::ObjectCastObligation(object_ty) => {
                err.note(&format!("required for the cast to the object type `{}`",
                                  self.ty_to_string(object_ty)));
            }
            ObligationCauseCode::RepeatVec => {
                err.note("the `Copy` trait is required because the \
                          repeated element will be copied");
            }
            ObligationCauseCode::VariableType(_) => {
                err.note("all local variables must have a statically known size");
            }
            ObligationCauseCode::ReturnType => {
                err.note("the return type of a function must have a \
                          statically known size");
            }
            ObligationCauseCode::AssignmentLhsSized => {
                err.note("the left-hand-side of an assignment must have a statically known size");
            }
            ObligationCauseCode::StructInitializerSized => {
                err.note("structs must have a statically known size to be initialized");
            }
            ObligationCauseCode::ClosureCapture(var_id, _, builtin_bound) => {
                let def_id = tcx.lang_items.from_builtin_kind(builtin_bound).unwrap();
                let trait_name = tcx.item_path_str(def_id);
                let name = tcx.local_var_name_str(var_id);
                err.note(
                    &format!("the closure that captures `{}` requires that all captured variables \
                              implement the trait `{}`",
                             name,
                             trait_name));
            }
            ObligationCauseCode::FieldSized => {
                err.note("only the last field of a struct or enum variant \
                          may have a dynamically sized type");
            }
            ObligationCauseCode::SharedStatic => {
                err.note("shared static variables must have a type that implements `Sync`");
            }
            ObligationCauseCode::BuiltinDerivedObligation(ref data) => {
                let parent_trait_ref = self.resolve_type_vars_if_possible(&data.parent_trait_ref);
                err.note(&format!("required because it appears within the type `{}`",
                                  parent_trait_ref.0.self_ty()));
                let parent_predicate = parent_trait_ref.to_predicate();
                self.note_obligation_cause_code(err,
                                                &parent_predicate,
                                                &data.parent_code);
            }
            ObligationCauseCode::ImplDerivedObligation(ref data) => {
                let parent_trait_ref = self.resolve_type_vars_if_possible(&data.parent_trait_ref);
                err.note(
                    &format!("required because of the requirements on the impl of `{}` for `{}`",
                             parent_trait_ref,
                             parent_trait_ref.0.self_ty()));
                let parent_predicate = parent_trait_ref.to_predicate();
                self.note_obligation_cause_code(err,
                                                &parent_predicate,
                                                &data.parent_code);
            }
            ObligationCauseCode::CompareImplMethodObligation => {
                err.note(
                    &format!("the requirement `{}` appears on the impl method \
                              but not on the corresponding trait method",
                             predicate));
            }
        }
    }

    fn suggest_new_overflow_limit(&self, err: &mut DiagnosticBuilder) {
        let current_limit = self.tcx.sess.recursion_limit.get();
        let suggested_limit = current_limit * 2;
        err.note(&format!(
                          "consider adding a `#![recursion_limit=\"{}\"]` attribute to your crate",
                          suggested_limit));
    }
}
