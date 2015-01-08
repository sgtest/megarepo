// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use astconv::AstConv;
use check::{FnCtxt, Inherited, blank_fn_ctxt, vtable, regionck};
use CrateCtxt;
use middle::region;
use middle::subst;
use middle::traits;
use middle::ty::{self, Ty};
use middle::ty::liberate_late_bound_regions;
use middle::ty_fold::{TypeFolder, TypeFoldable, super_fold_ty};
use util::ppaux::Repr;

use std::collections::HashSet;
use syntax::ast;
use syntax::ast_util::{local_def};
use syntax::attr;
use syntax::codemap::Span;
use syntax::parse::token;
use syntax::visit;
use syntax::visit::Visitor;

pub struct CheckTypeWellFormedVisitor<'ccx, 'tcx:'ccx> {
    ccx: &'ccx CrateCtxt<'ccx, 'tcx>,
    cache: HashSet<Ty<'tcx>>
}

impl<'ccx, 'tcx> CheckTypeWellFormedVisitor<'ccx, 'tcx> {
    pub fn new(ccx: &'ccx CrateCtxt<'ccx, 'tcx>) -> CheckTypeWellFormedVisitor<'ccx, 'tcx> {
        CheckTypeWellFormedVisitor { ccx: ccx, cache: HashSet::new() }
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
    fn check_item_well_formed(&mut self, item: &ast::Item) {
        let ccx = self.ccx;
        debug!("check_item_well_formed(it.id={}, it.ident={})",
               item.id,
               ty::item_path_str(ccx.tcx, local_def(item.id)));

        match item.node {
            ast::ItemImpl(..) => {
                self.check_impl(item);
            }
            ast::ItemFn(..) => {
                self.check_item_type(item);
            }
            ast::ItemStatic(..) => {
                self.check_item_type(item);
            }
            ast::ItemConst(..) => {
                self.check_item_type(item);
            }
            ast::ItemStruct(ref struct_def, _) => {
                self.check_type_defn(item, |fcx| {
                    vec![struct_variant(fcx, &**struct_def)]
                });
            }
            ast::ItemEnum(ref enum_def, _) => {
                self.check_type_defn(item, |fcx| {
                    enum_variants(fcx, enum_def)
                });
            }
            ast::ItemTrait(..) => {
                let trait_def =
                    ty::lookup_trait_def(ccx.tcx, local_def(item.id));
                reject_non_type_param_bounds(
                    ccx.tcx,
                    item.span,
                    &trait_def.generics);
            }
            _ => {}
        }
    }

    fn with_fcx<F>(&mut self, item: &ast::Item, mut f: F) where
        F: for<'fcx> FnMut(&mut CheckTypeWellFormedVisitor<'ccx, 'tcx>, &FnCtxt<'fcx, 'tcx>),
    {
        let ccx = self.ccx;
        let item_def_id = local_def(item.id);
        let type_scheme = ty::lookup_item_type(ccx.tcx, item_def_id);
        reject_non_type_param_bounds(ccx.tcx, item.span, &type_scheme.generics);
        let param_env =
            ty::construct_parameter_environment(ccx.tcx,
                                                &type_scheme.generics,
                                                item.id);
        let inh = Inherited::new(ccx.tcx, param_env);
        let fcx = blank_fn_ctxt(ccx, &inh, ty::FnConverging(type_scheme.ty), item.id);
        f(self, &fcx);
        vtable::select_all_fcx_obligations_or_error(&fcx);
        regionck::regionck_item(&fcx, item);
    }

    /// In a type definition, we check that to ensure that the types of the fields are well-formed.
    fn check_type_defn<F>(&mut self, item: &ast::Item, mut lookup_fields: F) where
        F: for<'fcx> FnMut(&FnCtxt<'fcx, 'tcx>) -> Vec<AdtVariant<'tcx>>,
    {
        self.with_fcx(item, |this, fcx| {
            let variants = lookup_fields(fcx);
            let mut bounds_checker = BoundsChecker::new(fcx,
                                                        item.span,
                                                        region::CodeExtent::from_node_id(item.id),
                                                        Some(&mut this.cache));
            for variant in variants.iter() {
                for field in variant.fields.iter() {
                    // Regions are checked below.
                    bounds_checker.check_traits_in_ty(field.ty);
                }

                // For DST, all intermediate types must be sized.
                if variant.fields.len() > 0 {
                    for field in variant.fields.init().iter() {
                        fcx.register_builtin_bound(
                            field.ty,
                            ty::BoundSized,
                            traits::ObligationCause::new(field.span,
                                                         fcx.body_id,
                                                         traits::FieldSized));
                    }
                }
            }

            let field_tys: Vec<Ty> =
                variants.iter().flat_map(|v| v.fields.iter().map(|f| f.ty)).collect();

            regionck::regionck_ensure_component_tys_wf(
                fcx, item.span, field_tys.as_slice());
        });
    }

    fn check_item_type(&mut self,
                       item: &ast::Item)
    {
        self.with_fcx(item, |this, fcx| {
            let mut bounds_checker = BoundsChecker::new(fcx,
                                                        item.span,
                                                        region::CodeExtent::from_node_id(item.id),
                                                        Some(&mut this.cache));

            let type_scheme = ty::lookup_item_type(fcx.tcx(), local_def(item.id));
            let item_ty = fcx.instantiate_type_scheme(item.span,
                                                      &fcx.inh.param_env.free_substs,
                                                      &type_scheme.ty);

            bounds_checker.check_traits_in_ty(item_ty);
        });
    }

    fn check_impl(&mut self,
                  item: &ast::Item)
    {
        self.with_fcx(item, |this, fcx| {
            let item_scope = region::CodeExtent::from_node_id(item.id);

            let mut bounds_checker = BoundsChecker::new(fcx,
                                                        item.span,
                                                        item_scope,
                                                        Some(&mut this.cache));

            // Find the impl self type as seen from the "inside" --
            // that is, with all type parameters converted from bound
            // to free.
            let self_ty = ty::node_id_to_type(fcx.tcx(), item.id);
            let self_ty = fcx.instantiate_type_scheme(item.span,
                                                      &fcx.inh.param_env.free_substs,
                                                      &self_ty);

            bounds_checker.check_traits_in_ty(self_ty);

            // Similarly, obtain an "inside" reference to the trait
            // that the impl implements.
            let trait_ref = match ty::impl_trait_ref(fcx.tcx(), local_def(item.id)) {
                None => { return; }
                Some(t) => { t }
            };

            let trait_ref = fcx.instantiate_type_scheme(item.span,
                                                        &fcx.inh.param_env.free_substs,
                                                        &trait_ref);

            // There are special rules that apply to drop.
            if
                fcx.tcx().lang_items.drop_trait() == Some(trait_ref.def_id) &&
                !attr::contains_name(item.attrs.as_slice(), "unsafe_destructor")
            {
                match self_ty.sty {
                    ty::ty_struct(def_id, _) |
                    ty::ty_enum(def_id, _) => {
                        check_struct_safe_for_destructor(fcx, item.span, def_id);
                    }
                    _ => {
                        // Coherence already reports an error in this case.
                    }
                }
            }

            if fcx.tcx().lang_items.copy_trait() == Some(trait_ref.def_id) {
                // This is checked in coherence.
                return
            }

            // We are stricter on the trait-ref in an impl than the
            // self-type.  In particular, we enforce region
            // relationships. The reason for this is that (at least
            // presently) "applying" an impl does not require that the
            // application site check the well-formedness constraints on the
            // trait reference. Instead, this is done at the impl site.
            // Arguably this is wrong and we should treat the trait-reference
            // the same way as we treat the self-type.
            bounds_checker.check_trait_ref(&*trait_ref);

            let cause =
                traits::ObligationCause::new(
                    item.span,
                    fcx.body_id,
                    traits::ItemObligation(trait_ref.def_id));

            // Find the supertrait bounds. This will add `int:Bar`.
            let poly_trait_ref = ty::Binder(trait_ref);
            let predicates = ty::predicates_for_trait_ref(fcx.tcx(), &poly_trait_ref);
            for predicate in predicates.into_iter() {
                fcx.register_predicate(traits::Obligation::new(cause.clone(), predicate));
            }
        });
    }
}

// Reject any predicates that do not involve a type parameter.
fn reject_non_type_param_bounds<'tcx>(tcx: &ty::ctxt<'tcx>,
                                      span: Span,
                                      generics: &ty::Generics<'tcx>) {

    for predicate in generics.predicates.iter() {
        match predicate {
            &ty::Predicate::Trait(ty::Binder(ref tr)) => {
                let found_param = tr.input_types().iter()
                                    .flat_map(|ty| ty.walk())
                                    .any(is_ty_param);
                if !found_param { report_bound_error(tcx, span, tr.self_ty() )}
            }
            &ty::Predicate::TypeOutlives(ty::Binder(ty::OutlivesPredicate(ty, _))) => {
                let found_param = ty.walk().any(|t| is_ty_param(t));
                if !found_param { report_bound_error(tcx, span, ty) }
            }
            _ => {}
        };
    }

    fn report_bound_error<'t>(tcx: &ty::ctxt<'t>,
                          span: Span,
                          bounded_ty: ty::Ty<'t>) {
        tcx.sess.span_err(
            span,
            format!("cannot bound type `{}`, where clause \
                bounds may only be attached to types involving \
                type parameters",
                bounded_ty.repr(tcx)).as_slice())
    }

    fn is_ty_param(ty: ty::Ty) -> bool {
        match &ty.sty {
            &ty::sty::ty_param(_) => true,
            _ => false
        }
    }
}

fn reject_shadowing_type_parameters<'tcx>(tcx: &ty::ctxt<'tcx>,
                                          span: Span,
                                          generics: &ty::Generics<'tcx>) {
    let impl_params = generics.types.get_slice(subst::TypeSpace).iter()
        .map(|tp| tp.name).collect::<HashSet<_>>();

    for method_param in generics.types.get_slice(subst::FnSpace).iter() {
        if impl_params.contains(&method_param.name) {
            tcx.sess.span_err(
                span,
                &*format!("type parameter `{}` shadows another type parameter of the same name",
                          token::get_name(method_param.name)));
        }
    }
}

impl<'ccx, 'tcx, 'v> Visitor<'v> for CheckTypeWellFormedVisitor<'ccx, 'tcx> {
    fn visit_item(&mut self, i: &ast::Item) {
        self.check_item_well_formed(i);
        visit::walk_item(self, i);
    }

    fn visit_fn(&mut self,
                fk: visit::FnKind<'v>, fd: &'v ast::FnDecl,
                b: &'v ast::Block, span: Span, id: ast::NodeId) {
        match fk {
            visit::FkFnBlock | visit::FkItemFn(..) => {}
            visit::FkMethod(..) => {
                match ty::impl_or_trait_item(self.ccx.tcx, local_def(id)) {
                    ty::ImplOrTraitItem::MethodTraitItem(ty_method) => {
                        reject_shadowing_type_parameters(self.ccx.tcx, span, &ty_method.generics)
                    }
                    _ => {}
                }
            }
        }
        visit::walk_fn(self, fk, fd, b, span)
    }

    fn visit_trait_item(&mut self, t: &'v ast::TraitItem) {
        match t {
            &ast::TraitItem::ProvidedMethod(_) |
            &ast::TraitItem::TypeTraitItem(_) => {},
            &ast::TraitItem::RequiredMethod(ref method) => {
                match ty::impl_or_trait_item(self.ccx.tcx, local_def(method.id)) {
                    ty::ImplOrTraitItem::MethodTraitItem(ty_method) => {
                        reject_non_type_param_bounds(
                            self.ccx.tcx,
                            method.span,
                            &ty_method.generics);
                        reject_shadowing_type_parameters(
                            self.ccx.tcx,
                            method.span,
                            &ty_method.generics);
                    }
                    _ => {}
                }
            }
        }

        visit::walk_trait_item(self, t)
    }
}

pub struct BoundsChecker<'cx,'tcx:'cx> {
    fcx: &'cx FnCtxt<'cx,'tcx>,
    span: Span,
    scope: region::CodeExtent,
    binding_count: uint,
    cache: Option<&'cx mut HashSet<Ty<'tcx>>>,
}

impl<'cx,'tcx> BoundsChecker<'cx,'tcx> {
    pub fn new(fcx: &'cx FnCtxt<'cx,'tcx>,
               span: Span,
               scope: region::CodeExtent,
               cache: Option<&'cx mut HashSet<Ty<'tcx>>>)
               -> BoundsChecker<'cx,'tcx> {
        BoundsChecker { fcx: fcx, span: span, scope: scope,
                        cache: cache, binding_count: 0 }
    }

    /// Given a trait ref like `A : Trait<B>`, where `Trait` is defined as (say):
    ///
    ///     trait Trait<B:OtherTrait> : Copy { ... }
    ///
    /// This routine will check that `B : OtherTrait` and `A : Trait<B>`. It will also recursively
    /// check that the types `A` and `B` are well-formed.
    ///
    /// Note that it does not (currently, at least) check that `A : Copy` (that check is delegated
    /// to the point where impl `A : Trait<B>` is implemented).
    pub fn check_trait_ref(&mut self, trait_ref: &ty::TraitRef<'tcx>) {
        let trait_def = ty::lookup_trait_def(self.fcx.tcx(), trait_ref.def_id);

        let bounds = self.fcx.instantiate_bounds(self.span, trait_ref.substs, &trait_def.generics);

        self.fcx.add_obligations_for_parameters(
            traits::ObligationCause::new(
                self.span,
                self.fcx.body_id,
                traits::ItemObligation(trait_ref.def_id)),
            &bounds);

        for &ty in trait_ref.substs.types.iter() {
            self.check_traits_in_ty(ty);
        }
    }

    pub fn check_ty(&mut self, ty: Ty<'tcx>) {
        ty.fold_with(self);
    }

    fn check_traits_in_ty(&mut self, ty: Ty<'tcx>) {
        // When checking types outside of a type def'n, we ignore
        // region obligations. See discussion below in fold_ty().
        self.binding_count += 1;
        ty.fold_with(self);
        self.binding_count -= 1;
    }
}

impl<'cx,'tcx> TypeFolder<'tcx> for BoundsChecker<'cx,'tcx> {
    fn tcx(&self) -> &ty::ctxt<'tcx> {
        self.fcx.tcx()
    }

    fn fold_binder<T>(&mut self, binder: &ty::Binder<T>) -> ty::Binder<T>
        where T : TypeFoldable<'tcx> + Repr<'tcx>
    {
        self.binding_count += 1;
        let value = liberate_late_bound_regions(self.fcx.tcx(), self.scope, binder);
        debug!("BoundsChecker::fold_binder: late-bound regions replaced: {}",
               value.repr(self.tcx()));
        let value = value.fold_with(self);
        self.binding_count -= 1;
        ty::Binder(value)
    }

    fn fold_ty(&mut self, t: Ty<'tcx>) -> Ty<'tcx> {
        debug!("BoundsChecker t={}",
               t.repr(self.tcx()));

        match self.cache {
            Some(ref mut cache) => {
                if !cache.insert(t) {
                    // Already checked this type! Don't check again.
                    debug!("cached");
                    return t;
                }
            }
            None => { }
        }

        match t.sty{
            ty::ty_struct(type_id, substs) |
            ty::ty_enum(type_id, substs) => {
                let type_scheme = ty::lookup_item_type(self.fcx.tcx(), type_id);
                let bounds = self.fcx.instantiate_bounds(self.span, substs, &type_scheme.generics);

                if self.binding_count == 0 {
                    self.fcx.add_obligations_for_parameters(
                        traits::ObligationCause::new(self.span,
                                                     self.fcx.body_id,
                                                     traits::ItemObligation(type_id)),
                        &bounds);
                } else {
                    // There are two circumstances in which we ignore
                    // region obligations.
                    //
                    // The first is when we are inside of a closure
                    // type. This is because in that case the region
                    // obligations for the parameter types are things
                    // that the closure body gets to assume and the
                    // caller must prove at the time of call. In other
                    // words, if there is a type like `<'a, 'b> | &'a
                    // &'b int |`, it is well-formed, and caller will
                    // have to show that `'b : 'a` at the time of
                    // call.
                    //
                    // The second is when we are checking for
                    // well-formedness outside of a type def'n or fn
                    // body. This is for a similar reason: in general,
                    // we only do WF checking for regions in the
                    // result of expressions and type definitions, so
                    // to as allow for implicit where clauses.
                    //
                    // (I believe we should do the same for traits, but
                    // that will require an RFC. -nmatsakis)
                    let bounds = filter_to_trait_obligations(bounds);
                    self.fcx.add_obligations_for_parameters(
                        traits::ObligationCause::new(self.span,
                                                     self.fcx.body_id,
                                                     traits::ItemObligation(type_id)),
                        &bounds);
                }

                self.fold_substs(substs);
            }
            _ => {
                super_fold_ty(self, t);
            }
        }

        t // we're not folding to produce a new type, so just return `t` here
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

fn struct_variant<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                            struct_def: &ast::StructDef)
                            -> AdtVariant<'tcx> {
    let fields =
        struct_def.fields
        .iter()
        .map(|field| {
            let field_ty = ty::node_id_to_type(fcx.tcx(), field.node.id);
            let field_ty = fcx.instantiate_type_scheme(field.span,
                                                       &fcx.inh.param_env.free_substs,
                                                       &field_ty);
            AdtField { ty: field_ty, span: field.span }
        })
        .collect();
    AdtVariant { fields: fields }
}

fn enum_variants<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                           enum_def: &ast::EnumDef)
                           -> Vec<AdtVariant<'tcx>> {
    enum_def.variants.iter()
        .map(|variant| {
            match variant.node.kind {
                ast::TupleVariantKind(ref args) if args.len() > 0 => {
                    let ctor_ty = ty::node_id_to_type(fcx.tcx(), variant.node.id);

                    // the regions in the argument types come from the
                    // enum def'n, and hence will all be early bound
                    let arg_tys =
                        ty::assert_no_late_bound_regions(
                            fcx.tcx(), &ty::ty_fn_args(ctor_ty));
                    AdtVariant {
                        fields: args.iter().enumerate().map(|(index, arg)| {
                            let arg_ty = arg_tys[index];
                            let arg_ty =
                                fcx.instantiate_type_scheme(variant.span,
                                                            &fcx.inh.param_env.free_substs,
                                                            &arg_ty);
                            AdtField {
                                ty: arg_ty,
                                span: arg.ty.span
                            }
                        }).collect()
                    }
                }
                ast::TupleVariantKind(_) => {
                    AdtVariant {
                        fields: Vec::new()
                    }
                }
                ast::StructVariantKind(ref struct_def) => {
                    struct_variant(fcx, &**struct_def)
                }
            }
        })
        .collect()
}

fn filter_to_trait_obligations<'tcx>(bounds: ty::GenericBounds<'tcx>)
                                     -> ty::GenericBounds<'tcx>
{
    let mut result = ty::GenericBounds::empty();
    for (space, _, predicate) in bounds.predicates.iter_enumerated() {
        match *predicate {
            ty::Predicate::Trait(..) |
            ty::Predicate::Projection(..) => {
                result.predicates.push(space, predicate.clone())
            }
            ty::Predicate::Equate(..) |
            ty::Predicate::TypeOutlives(..) |
            ty::Predicate::RegionOutlives(..) => {
            }
        }
    }
    result
}

///////////////////////////////////////////////////////////////////////////
// Special drop trait checking

fn check_struct_safe_for_destructor<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                              span: Span,
                                              struct_did: ast::DefId) {
    let struct_tpt = ty::lookup_item_type(fcx.tcx(), struct_did);
    if struct_tpt.generics.has_type_params(subst::TypeSpace)
        || struct_tpt.generics.has_region_params(subst::TypeSpace)
    {
        span_err!(fcx.tcx().sess, span, E0141,
                  "cannot implement a destructor on a structure \
                   with type parameters");
        span_note!(fcx.tcx().sess, span,
                   "use \"#[unsafe_destructor]\" on the implementation \
                    to force the compiler to allow this");
    }
}
