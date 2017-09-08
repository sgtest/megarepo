// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use astconv::ExplicitSelf;
use check::{Inherited, FnCtxt};
use constrained_type_params::{identify_constrained_type_params, Parameter};

use hir::def_id::DefId;
use rustc::traits::{self, ObligationCauseCode};
use rustc::ty::{self, Ty, TyCtxt};
use rustc::util::nodemap::{FxHashSet, FxHashMap};
use rustc::middle::lang_items;

use syntax::ast;
use syntax_pos::Span;
use errors::DiagnosticBuilder;

use rustc::hir::intravisit::{self, Visitor, NestedVisitorMap};
use rustc::hir;

pub struct CheckTypeWellFormedVisitor<'a, 'tcx:'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    code: ObligationCauseCode<'tcx>,
}

/// Helper type of a temporary returned by .for_item(...).
/// Necessary because we can't write the following bound:
/// F: for<'b, 'tcx> where 'gcx: 'tcx FnOnce(FnCtxt<'b, 'gcx, 'tcx>).
struct CheckWfFcxBuilder<'a, 'gcx: 'a+'tcx, 'tcx: 'a> {
    inherited: super::InheritedBuilder<'a, 'gcx, 'tcx>,
    code: ObligationCauseCode<'gcx>,
    id: ast::NodeId,
    span: Span,
    param_env: ty::ParamEnv<'tcx>,
}

impl<'a, 'gcx, 'tcx> CheckWfFcxBuilder<'a, 'gcx, 'tcx> {
    fn with_fcx<F>(&'tcx mut self, f: F) where
        F: for<'b> FnOnce(&FnCtxt<'b, 'gcx, 'tcx>,
                          &mut CheckTypeWellFormedVisitor<'b, 'gcx>) -> Vec<Ty<'tcx>>
    {
        let code = self.code.clone();
        let id = self.id;
        let span = self.span;
        let param_env = self.param_env;
        self.inherited.enter(|inh| {
            let fcx = FnCtxt::new(&inh, param_env, id);
            let wf_tys = f(&fcx, &mut CheckTypeWellFormedVisitor {
                tcx: fcx.tcx.global_tcx(),
                code,
            });
            fcx.select_all_obligations_or_error();
            fcx.regionck_item(id, span, &wf_tys);
        });
    }
}

impl<'a, 'gcx> CheckTypeWellFormedVisitor<'a, 'gcx> {
    pub fn new(tcx: TyCtxt<'a, 'gcx, 'gcx>)
               -> CheckTypeWellFormedVisitor<'a, 'gcx> {
        CheckTypeWellFormedVisitor {
            tcx,
            code: ObligationCauseCode::MiscObligation
        }
    }

    /// Checks that the field types (in a struct def'n) or argument types (in an enum def'n) are
    /// well-formed, meaning that they do not require any constraints not declared in the struct
    /// definition itself. For example, this definition would be illegal:
    ///
    ///     struct Ref<'a, T> { x: &'a T }
    ///
    /// because the type did not declare that `T:'a`.
    ///
    /// We do this check as a pre-pass before checking fn bodies because if these constraints are
    /// not included it frequently leads to confusing errors in fn bodies. So it's better to check
    /// the types first.
    fn check_item_well_formed(&mut self, item: &hir::Item) {
        let tcx = self.tcx;
        debug!("check_item_well_formed(it.id={}, it.name={})",
               item.id,
               tcx.item_path_str(tcx.hir.local_def_id(item.id)));

        match item.node {
            // Right now we check that every default trait implementation
            // has an implementation of itself. Basically, a case like:
            //
            // `impl Trait for T {}`
            //
            // has a requirement of `T: Trait` which was required for default
            // method implementations. Although this could be improved now that
            // there's a better infrastructure in place for this, it's being left
            // for a follow-up work.
            //
            // Since there's such a requirement, we need to check *just* positive
            // implementations, otherwise things like:
            //
            // impl !Send for T {}
            //
            // won't be allowed unless there's an *explicit* implementation of `Send`
            // for `T`
            hir::ItemImpl(_, hir::ImplPolarity::Positive, _, _,
                          ref trait_ref, ref self_ty, _) => {
                self.check_impl(item, self_ty, trait_ref);
            }
            hir::ItemImpl(_, hir::ImplPolarity::Negative, _, _, Some(_), ..) => {
                // FIXME(#27579) what amount of WF checking do we need for neg impls?

                let trait_ref = tcx.impl_trait_ref(tcx.hir.local_def_id(item.id)).unwrap();
                if !tcx.trait_has_default_impl(trait_ref.def_id) {
                    error_192(tcx, item.span);
                }
            }
            hir::ItemFn(..) => {
                self.check_item_fn(item);
            }
            hir::ItemStatic(..) => {
                self.check_item_type(item);
            }
            hir::ItemConst(..) => {
                self.check_item_type(item);
            }
            hir::ItemStruct(ref struct_def, ref ast_generics) => {
                self.check_type_defn(item, false, |fcx| {
                    vec![fcx.struct_variant(struct_def)]
                });

                self.check_variances_for_type_defn(item, ast_generics);
            }
            hir::ItemUnion(ref struct_def, ref ast_generics) => {
                self.check_type_defn(item, true, |fcx| {
                    vec![fcx.struct_variant(struct_def)]
                });

                self.check_variances_for_type_defn(item, ast_generics);
            }
            hir::ItemEnum(ref enum_def, ref ast_generics) => {
                self.check_type_defn(item, true, |fcx| {
                    fcx.enum_variants(enum_def)
                });

                self.check_variances_for_type_defn(item, ast_generics);
            }
            hir::ItemTrait(..) => {
                self.check_trait(item);
            }
            _ => {}
        }
    }

    fn check_associated_item(&mut self,
                             item_id: ast::NodeId,
                             span: Span,
                             sig_if_method: Option<&hir::MethodSig>) {
        let code = self.code.clone();
        self.for_id(item_id, span).with_fcx(|fcx, this| {
            let item = fcx.tcx.associated_item(fcx.tcx.hir.local_def_id(item_id));

            let (mut implied_bounds, self_ty) = match item.container {
                ty::TraitContainer(_) => (vec![], fcx.tcx.mk_self_type()),
                ty::ImplContainer(def_id) => (fcx.impl_implied_bounds(def_id, span),
                                              fcx.tcx.type_of(def_id))
            };

            match item.kind {
                ty::AssociatedKind::Const => {
                    let ty = fcx.tcx.type_of(item.def_id);
                    let ty = fcx.normalize_associated_types_in(span, &ty);
                    fcx.register_wf_obligation(ty, span, code.clone());
                }
                ty::AssociatedKind::Method => {
                    reject_shadowing_type_parameters(fcx.tcx, item.def_id);
                    let sig = fcx.tcx.fn_sig(item.def_id);
                    let sig = fcx.normalize_associated_types_in(span, &sig);
                    let predicates = fcx.tcx.predicates_of(item.def_id)
                        .instantiate_identity(fcx.tcx);
                    let predicates = fcx.normalize_associated_types_in(span, &predicates);
                    this.check_fn_or_method(fcx, span, sig, &predicates,
                                            item.def_id, &mut implied_bounds);
                    let sig_if_method = sig_if_method.expect("bad signature for method");
                    this.check_method_receiver(fcx, sig_if_method, &item, self_ty);
                }
                ty::AssociatedKind::Type => {
                    if item.defaultness.has_value() {
                        let ty = fcx.tcx.type_of(item.def_id);
                        let ty = fcx.normalize_associated_types_in(span, &ty);
                        fcx.register_wf_obligation(ty, span, code.clone());
                    }
                }
            }

            implied_bounds
        })
    }

    fn for_item<'tcx>(&self, item: &hir::Item)
                      -> CheckWfFcxBuilder<'a, 'gcx, 'tcx> {
        self.for_id(item.id, item.span)
    }

    fn for_id<'tcx>(&self, id: ast::NodeId, span: Span)
                    -> CheckWfFcxBuilder<'a, 'gcx, 'tcx> {
        let def_id = self.tcx.hir.local_def_id(id);
        CheckWfFcxBuilder {
            inherited: Inherited::build(self.tcx, def_id),
            code: self.code.clone(),
            id,
            span,
            param_env: self.tcx.param_env(def_id),
        }
    }

    /// In a type definition, we check that to ensure that the types of the fields are well-formed.
    fn check_type_defn<F>(&mut self, item: &hir::Item, all_sized: bool, mut lookup_fields: F)
        where F: for<'fcx, 'tcx> FnMut(&FnCtxt<'fcx, 'gcx, 'tcx>) -> Vec<AdtVariant<'tcx>>
    {
        self.for_item(item).with_fcx(|fcx, this| {
            let variants = lookup_fields(fcx);

            for variant in &variants {
                // For DST, all intermediate types must be sized.
                let unsized_len = if all_sized || variant.fields.is_empty() { 0 } else { 1 };
                for field in &variant.fields[..variant.fields.len() - unsized_len] {
                    fcx.register_bound(
                        field.ty,
                        fcx.tcx.require_lang_item(lang_items::SizedTraitLangItem),
                        traits::ObligationCause::new(field.span,
                                                     fcx.body_id,
                                                     traits::FieldSized(match item.node.adt_kind() {
                                                        Some(i) => i,
                                                        None => bug!(),
                                                     })));
                }

                // All field types must be well-formed.
                for field in &variant.fields {
                    fcx.register_wf_obligation(field.ty, field.span, this.code.clone())
                }
            }

            let def_id = fcx.tcx.hir.local_def_id(item.id);
            let predicates = fcx.tcx.predicates_of(def_id).instantiate_identity(fcx.tcx);
            let predicates = fcx.normalize_associated_types_in(item.span, &predicates);
            this.check_where_clauses(fcx, item.span, &predicates);

            vec![] // no implied bounds in a struct def'n
        });
    }

    fn check_auto_trait(&mut self, trait_def_id: DefId, span: Span) {
        // We want to ensure:
        //
        // 1) that there are no items contained within
        // the trait definition
        //
        // 2) that the definition doesn't violate the no-super trait rule
        // for auto traits.
        //
        // 3) that the trait definition does not have any type parameters

        let predicates = self.tcx.predicates_of(trait_def_id);

        // We must exclude the Self : Trait predicate contained by all
        // traits.
        let has_predicates =
            predicates.predicates.iter().any(|predicate| {
                match predicate {
                    &ty::Predicate::Trait(ref poly_trait_ref) => {
                        let self_ty = poly_trait_ref.0.self_ty();
                        !(self_ty.is_self() && poly_trait_ref.def_id() == trait_def_id)
                    },
                    _ => true,
                }
            });

        let has_ty_params = self.tcx.generics_of(trait_def_id).types.len() > 1;

        // We use an if-else here, since the generics will also trigger
        // an extraneous error message when we find predicates like
        // `T : Sized` for a trait like: `trait Magic<T>`.
        //
        // We also put the check on the number of items here,
        // as it seems confusing to report an error about
        // extraneous predicates created by things like
        // an associated type inside the trait.
        let mut err = None;
        if !self.tcx.associated_item_def_ids(trait_def_id).is_empty() {
            error_380(self.tcx, span);
        } else if has_ty_params {
            err = Some(struct_span_err!(self.tcx.sess, span, E0567,
                "traits with auto impls (`e.g. impl \
                    Trait for ..`) can not have type parameters"));
        } else if has_predicates {
            err = Some(struct_span_err!(self.tcx.sess, span, E0568,
                "traits with auto impls (`e.g. impl \
                    Trait for ..`) cannot have predicates"));
        }

        // Finally if either of the above conditions apply we should add a note
        // indicating that this error is the result of a recent soundness fix.
        match err {
            None => {},
            Some(mut e) => {
                e.note("the new auto trait rules are the result of a \
                          recent soundness fix; see #29859 for more details");
                e.emit();
            }
        }
    }

    fn check_trait(&mut self, item: &hir::Item) {
        let trait_def_id = self.tcx.hir.local_def_id(item.id);

        if self.tcx.trait_has_default_impl(trait_def_id) {
            self.check_auto_trait(trait_def_id, item.span);
        }

        self.for_item(item).with_fcx(|fcx, this| {
            let predicates = fcx.tcx.predicates_of(trait_def_id).instantiate_identity(fcx.tcx);
            let predicates = fcx.normalize_associated_types_in(item.span, &predicates);
            this.check_where_clauses(fcx, item.span, &predicates);
            vec![]
        });
    }

    fn check_item_fn(&mut self, item: &hir::Item) {
        self.for_item(item).with_fcx(|fcx, this| {
            let def_id = fcx.tcx.hir.local_def_id(item.id);
            let sig = fcx.tcx.fn_sig(def_id);
            let sig = fcx.normalize_associated_types_in(item.span, &sig);

            let predicates = fcx.tcx.predicates_of(def_id).instantiate_identity(fcx.tcx);
            let predicates = fcx.normalize_associated_types_in(item.span, &predicates);

            let mut implied_bounds = vec![];
            this.check_fn_or_method(fcx, item.span, sig, &predicates,
                                    def_id, &mut implied_bounds);
            implied_bounds
        })
    }

    fn check_item_type(&mut self,
                       item: &hir::Item)
    {
        debug!("check_item_type: {:?}", item);

        self.for_item(item).with_fcx(|fcx, this| {
            let ty = fcx.tcx.type_of(fcx.tcx.hir.local_def_id(item.id));
            let item_ty = fcx.normalize_associated_types_in(item.span, &ty);

            fcx.register_wf_obligation(item_ty, item.span, this.code.clone());

            vec![] // no implied bounds in a const etc
        });
    }

    fn check_impl(&mut self,
                  item: &hir::Item,
                  ast_self_ty: &hir::Ty,
                  ast_trait_ref: &Option<hir::TraitRef>)
    {
        debug!("check_impl: {:?}", item);

        self.for_item(item).with_fcx(|fcx, this| {
            let item_def_id = fcx.tcx.hir.local_def_id(item.id);

            match *ast_trait_ref {
                Some(ref ast_trait_ref) => {
                    let trait_ref = fcx.tcx.impl_trait_ref(item_def_id).unwrap();
                    let trait_ref =
                        fcx.normalize_associated_types_in(
                            ast_trait_ref.path.span, &trait_ref);
                    let obligations =
                        ty::wf::trait_obligations(fcx,
                                                  fcx.param_env,
                                                  fcx.body_id,
                                                  &trait_ref,
                                                  ast_trait_ref.path.span);
                    for obligation in obligations {
                        fcx.register_predicate(obligation);
                    }
                }
                None => {
                    let self_ty = fcx.tcx.type_of(item_def_id);
                    let self_ty = fcx.normalize_associated_types_in(item.span, &self_ty);
                    fcx.register_wf_obligation(self_ty, ast_self_ty.span, this.code.clone());
                }
            }

            let predicates = fcx.tcx.predicates_of(item_def_id).instantiate_identity(fcx.tcx);
            let predicates = fcx.normalize_associated_types_in(item.span, &predicates);
            this.check_where_clauses(fcx, item.span, &predicates);

            fcx.impl_implied_bounds(item_def_id, item.span)
        });
    }

    fn check_where_clauses<'fcx, 'tcx>(&mut self,
                                       fcx: &FnCtxt<'fcx, 'gcx, 'tcx>,
                                       span: Span,
                                       predicates: &ty::InstantiatedPredicates<'tcx>)
    {
        let obligations =
            predicates.predicates
                      .iter()
                      .flat_map(|p| ty::wf::predicate_obligations(fcx,
                                                                  fcx.param_env,
                                                                  fcx.body_id,
                                                                  p,
                                                                  span));

        for obligation in obligations {
            fcx.register_predicate(obligation);
        }
    }

    fn check_fn_or_method<'fcx, 'tcx>(&mut self,
                                      fcx: &FnCtxt<'fcx, 'gcx, 'tcx>,
                                      span: Span,
                                      sig: ty::PolyFnSig<'tcx>,
                                      predicates: &ty::InstantiatedPredicates<'tcx>,
                                      def_id: DefId,
                                      implied_bounds: &mut Vec<Ty<'tcx>>)
    {
        let sig = fcx.normalize_associated_types_in(span, &sig);
        let sig = fcx.liberate_late_bound_regions(def_id, &sig);

        for input_ty in sig.inputs() {
            fcx.register_wf_obligation(&input_ty, span, self.code.clone());
        }
        implied_bounds.extend(sig.inputs());

        fcx.register_wf_obligation(sig.output(), span, self.code.clone());

        // FIXME(#25759) return types should not be implied bounds
        implied_bounds.push(sig.output());

        self.check_where_clauses(fcx, span, predicates);
    }

    fn check_method_receiver<'fcx, 'tcx>(&mut self,
                                         fcx: &FnCtxt<'fcx, 'gcx, 'tcx>,
                                         method_sig: &hir::MethodSig,
                                         method: &ty::AssociatedItem,
                                         self_ty: ty::Ty<'tcx>)
    {
        // check that the type of the method's receiver matches the
        // method's first parameter.
        debug!("check_method_receiver({:?}, self_ty={:?})",
               method, self_ty);

        if !method.method_has_self_argument {
            return;
        }

        let span = method_sig.decl.inputs[0].span;

        let sig = fcx.tcx.fn_sig(method.def_id);
        let sig = fcx.normalize_associated_types_in(span, &sig);
        let sig = fcx.liberate_late_bound_regions(method.def_id, &sig);

        debug!("check_method_receiver: sig={:?}", sig);

        let self_arg_ty = sig.inputs()[0];
        let rcvr_ty = match ExplicitSelf::determine(self_ty, self_arg_ty) {
            ExplicitSelf::ByValue => self_ty,
            ExplicitSelf::ByReference(region, mutbl) => {
                fcx.tcx.mk_ref(region, ty::TypeAndMut {
                    ty: self_ty,
                    mutbl,
                })
            }
            ExplicitSelf::ByBox => fcx.tcx.mk_box(self_ty)
        };
        let rcvr_ty = fcx.normalize_associated_types_in(span, &rcvr_ty);
        let rcvr_ty = fcx.liberate_late_bound_regions(method.def_id,
                                                      &ty::Binder(rcvr_ty));

        debug!("check_method_receiver: receiver ty = {:?}", rcvr_ty);

        let cause = fcx.cause(span, ObligationCauseCode::MethodReceiver);
        if let Some(mut err) = fcx.demand_eqtype_with_origin(&cause, rcvr_ty, self_arg_ty) {
            err.emit();
        }
    }

    fn check_variances_for_type_defn(&self,
                                     item: &hir::Item,
                                     ast_generics: &hir::Generics)
    {
        let item_def_id = self.tcx.hir.local_def_id(item.id);
        let ty = self.tcx.type_of(item_def_id);
        if self.tcx.has_error_field(ty) {
            return;
        }

        let ty_predicates = self.tcx.predicates_of(item_def_id);
        assert_eq!(ty_predicates.parent, None);
        let variances = self.tcx.variances_of(item_def_id);

        let mut constrained_parameters: FxHashSet<_> =
            variances.iter().enumerate()
                     .filter(|&(_, &variance)| variance != ty::Bivariant)
                     .map(|(index, _)| Parameter(index as u32))
                     .collect();

        identify_constrained_type_params(self.tcx,
                                         ty_predicates.predicates.as_slice(),
                                         None,
                                         &mut constrained_parameters);

        for (index, _) in variances.iter().enumerate() {
            if constrained_parameters.contains(&Parameter(index as u32)) {
                continue;
            }

            let (span, name) = if index < ast_generics.lifetimes.len() {
                (ast_generics.lifetimes[index].lifetime.span,
                 ast_generics.lifetimes[index].lifetime.name)
            } else {
                let index = index - ast_generics.lifetimes.len();
                (ast_generics.ty_params[index].span,
                 ast_generics.ty_params[index].name)
            };
            self.report_bivariance(span, name);
        }
    }

    fn report_bivariance(&self,
                         span: Span,
                         param_name: ast::Name)
    {
        let mut err = error_392(self.tcx, span, param_name);

        let suggested_marker_id = self.tcx.lang_items().phantom_data();
        match suggested_marker_id {
            Some(def_id) => {
                err.help(
                    &format!("consider removing `{}` or using a marker such as `{}`",
                             param_name,
                             self.tcx.item_path_str(def_id)));
            }
            None => {
                // no lang items, no help!
            }
        }
        err.emit();
    }
}

fn reject_shadowing_type_parameters(tcx: TyCtxt, def_id: DefId) {
    let generics = tcx.generics_of(def_id);
    let parent = tcx.generics_of(generics.parent.unwrap());
    let impl_params: FxHashMap<_, _> = parent.types
                                       .iter()
                                       .map(|tp| (tp.name, tp.def_id))
                                       .collect();

    for method_param in &generics.types {
        if impl_params.contains_key(&method_param.name) {
            // Tighten up the span to focus on only the shadowing type
            let type_span = tcx.def_span(method_param.def_id);

            // The expectation here is that the original trait declaration is
            // local so it should be okay to just unwrap everything.
            let trait_def_id = impl_params[&method_param.name];
            let trait_decl_span = tcx.def_span(trait_def_id);
            error_194(tcx, type_span, trait_decl_span, method_param.name);
        }
    }
}

impl<'a, 'tcx, 'v> Visitor<'v> for CheckTypeWellFormedVisitor<'a, 'tcx> {
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'v> {
        NestedVisitorMap::None
    }

    fn visit_item(&mut self, i: &hir::Item) {
        debug!("visit_item: {:?}", i);
        self.check_item_well_formed(i);
        intravisit::walk_item(self, i);
    }

    fn visit_trait_item(&mut self, trait_item: &'v hir::TraitItem) {
        debug!("visit_trait_item: {:?}", trait_item);
        let method_sig = match trait_item.node {
            hir::TraitItemKind::Method(ref sig, _) => Some(sig),
            _ => None
        };
        self.check_associated_item(trait_item.id, trait_item.span, method_sig);
        intravisit::walk_trait_item(self, trait_item)
    }

    fn visit_impl_item(&mut self, impl_item: &'v hir::ImplItem) {
        debug!("visit_impl_item: {:?}", impl_item);
        let method_sig = match impl_item.node {
            hir::ImplItemKind::Method(ref sig, _) => Some(sig),
            _ => None
        };
        self.check_associated_item(impl_item.id, impl_item.span, method_sig);
        intravisit::walk_impl_item(self, impl_item)
    }
}

///////////////////////////////////////////////////////////////////////////
// ADT

struct AdtVariant<'tcx> {
    fields: Vec<AdtField<'tcx>>,
}

struct AdtField<'tcx> {
    ty: Ty<'tcx>,
    span: Span,
}

impl<'a, 'gcx, 'tcx> FnCtxt<'a, 'gcx, 'tcx> {
    fn struct_variant(&self, struct_def: &hir::VariantData) -> AdtVariant<'tcx> {
        let fields =
            struct_def.fields().iter()
            .map(|field| {
                let field_ty = self.tcx.type_of(self.tcx.hir.local_def_id(field.id));
                let field_ty = self.normalize_associated_types_in(field.span,
                                                                  &field_ty);
                AdtField { ty: field_ty, span: field.span }
            })
            .collect();
        AdtVariant { fields: fields }
    }

    fn enum_variants(&self, enum_def: &hir::EnumDef) -> Vec<AdtVariant<'tcx>> {
        enum_def.variants.iter()
            .map(|variant| self.struct_variant(&variant.node.data))
            .collect()
    }

    fn impl_implied_bounds(&self, impl_def_id: DefId, span: Span) -> Vec<Ty<'tcx>> {
        match self.tcx.impl_trait_ref(impl_def_id) {
            Some(ref trait_ref) => {
                // Trait impl: take implied bounds from all types that
                // appear in the trait reference.
                let trait_ref = self.normalize_associated_types_in(span, trait_ref);
                trait_ref.substs.types().collect()
            }

            None => {
                // Inherent impl: take implied bounds from the self type.
                let self_ty = self.tcx.type_of(impl_def_id);
                let self_ty = self.normalize_associated_types_in(span, &self_ty);
                vec![self_ty]
            }
        }
    }
}

fn error_192(tcx: TyCtxt, span: Span) {
    span_err!(tcx.sess, span, E0192,
              "negative impls are only allowed for traits with \
               default impls (e.g., `Send` and `Sync`)")
}

fn error_380(tcx: TyCtxt, span: Span) {
    span_err!(tcx.sess, span, E0380,
              "traits with default impls (`e.g. impl \
               Trait for ..`) must have no methods or associated items")
}

fn error_392<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, span: Span, param_name: ast::Name)
                       -> DiagnosticBuilder<'tcx> {
    let mut err = struct_span_err!(tcx.sess, span, E0392,
                  "parameter `{}` is never used", param_name);
    err.span_label(span, "unused type parameter");
    err
}

fn error_194(tcx: TyCtxt, span: Span, trait_decl_span: Span, name: ast::Name) {
    struct_span_err!(tcx.sess, span, E0194,
              "type parameter `{}` shadows another type parameter of the same name",
              name)
        .span_label(span, "shadows another type parameter")
        .span_label(trait_decl_span, format!("first `{}` declared here", name))
        .emit();
}
