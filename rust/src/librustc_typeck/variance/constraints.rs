// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Constraint construction and representation
//!
//! The second pass over the AST determines the set of constraints.
//! We walk the set of items and, for each member, generate new constraints.

use hir::def_id::DefId;
use middle::resolve_lifetime as rl;
use rustc::dep_graph::{AssertDepGraphSafe, DepNode};
use rustc::ty::subst::Substs;
use rustc::ty::{self, Ty, TyCtxt};
use rustc::hir::map as hir_map;
use syntax::ast;
use rustc::hir;
use rustc::hir::itemlikevisit::ItemLikeVisitor;

use rustc_data_structures::transitive_relation::TransitiveRelation;

use super::terms::*;
use super::terms::VarianceTerm::*;

pub struct ConstraintContext<'a, 'tcx: 'a> {
    pub terms_cx: TermsContext<'a, 'tcx>,

    // These are pointers to common `ConstantTerm` instances
    covariant: VarianceTermPtr<'a>,
    contravariant: VarianceTermPtr<'a>,
    invariant: VarianceTermPtr<'a>,
    bivariant: VarianceTermPtr<'a>,

    pub constraints: Vec<Constraint<'a>>,

    /// This relation tracks the dependencies between the variance of
    /// various items. In particular, if `a < b`, then the variance of
    /// `a` depends on the sources of `b`.
    pub dependencies: TransitiveRelation<DefId>,
}

/// Declares that the variable `decl_id` appears in a location with
/// variance `variance`.
#[derive(Copy, Clone)]
pub struct Constraint<'a> {
    pub inferred: InferredIndex,
    pub variance: &'a VarianceTerm<'a>,
}

/// To build constriants, we visit one item (type, trait) at a time
/// and look at its contents. So e.g. if we have
///
///     struct Foo<T> {
///         b: Bar<T>
///     }
///
/// then while we are visiting `Bar<T>`, the `CurrentItem` would have
/// the def-id and generics of `Foo`.
pub struct CurrentItem<'a> {
    def_id: DefId,
    generics: &'a ty::Generics,
}

pub fn add_constraints_from_crate<'a, 'tcx>(terms_cx: TermsContext<'a, 'tcx>)
                                            -> ConstraintContext<'a, 'tcx> {
    let tcx = terms_cx.tcx;
    let covariant = terms_cx.arena.alloc(ConstantTerm(ty::Covariant));
    let contravariant = terms_cx.arena.alloc(ConstantTerm(ty::Contravariant));
    let invariant = terms_cx.arena.alloc(ConstantTerm(ty::Invariant));
    let bivariant = terms_cx.arena.alloc(ConstantTerm(ty::Bivariant));
    let mut constraint_cx = ConstraintContext {
        terms_cx: terms_cx,
        covariant: covariant,
        contravariant: contravariant,
        invariant: invariant,
        bivariant: bivariant,
        constraints: Vec::new(),
        dependencies: TransitiveRelation::new(),
    };

    tcx.hir.krate().visit_all_item_likes(&mut constraint_cx);

    constraint_cx
}

impl<'a, 'tcx, 'v> ItemLikeVisitor<'v> for ConstraintContext<'a, 'tcx> {
    fn visit_item(&mut self, item: &hir::Item) {
        let tcx = self.terms_cx.tcx;
        let def_id = tcx.hir.local_def_id(item.id);

        // Encapsulate constructing the constraints into a task we can
        // reference later. This can go away once the red-green
        // algorithm is in place.
        //
        // See README.md for a detailed discussion
        // on dep-graph management.
        match item.node {
            hir::ItemEnum(..) |
            hir::ItemStruct(..) |
            hir::ItemUnion(..) => {
                tcx.dep_graph.with_task(DepNode::ItemVarianceConstraints(def_id),
                                        AssertDepGraphSafe(self),
                                        def_id,
                                        visit_item_task);
            }
            _ => {
                // Nothing to do here, skip the task.
            }
        }

        fn visit_item_task<'a, 'tcx>(ccx: AssertDepGraphSafe<&mut ConstraintContext<'a, 'tcx>>,
                                     def_id: DefId)
        {
            ccx.0.build_constraints_for_item(def_id);
        }
    }

    fn visit_trait_item(&mut self, _trait_item: &hir::TraitItem) {
    }

    fn visit_impl_item(&mut self, _impl_item: &hir::ImplItem) {
    }
}

/// Is `param_id` a lifetime according to `map`?
fn is_lifetime(map: &hir_map::Map, param_id: ast::NodeId) -> bool {
    match map.find(param_id) {
        Some(hir_map::NodeLifetime(..)) => true,
        _ => false,
    }
}

impl<'a, 'tcx> ConstraintContext<'a, 'tcx> {
    fn tcx(&self) -> TyCtxt<'a, 'tcx, 'tcx> {
        self.terms_cx.tcx
    }

    fn build_constraints_for_item(&mut self, def_id: DefId) {
        let tcx = self.tcx();
        let id = self.tcx().hir.as_local_node_id(def_id).unwrap();
        let item = tcx.hir.expect_item(id);
        debug!("visit_item item={}", tcx.hir.node_to_string(item.id));

        match item.node {
            hir::ItemEnum(..) |
            hir::ItemStruct(..) |
            hir::ItemUnion(..) => {
                let generics = tcx.generics_of(def_id);
                let current_item = &CurrentItem { def_id, generics };

                // Not entirely obvious: constraints on structs/enums do not
                // affect the variance of their type parameters. See discussion
                // in comment at top of module.
                //
                // self.add_constraints_from_generics(generics);

                for field in tcx.adt_def(def_id).all_fields() {
                    self.add_constraints_from_ty(current_item,
                                                 tcx.type_of(field.did),
                                                 self.covariant);
                }
            }

            hir::ItemTrait(..) |
            hir::ItemExternCrate(_) |
            hir::ItemUse(..) |
            hir::ItemStatic(..) |
            hir::ItemConst(..) |
            hir::ItemFn(..) |
            hir::ItemMod(..) |
            hir::ItemForeignMod(..) |
            hir::ItemGlobalAsm(..) |
            hir::ItemTy(..) |
            hir::ItemImpl(..) |
            hir::ItemDefaultImpl(..) => {
                span_bug!(item.span, "`build_constraints_for_item` invoked for non-type-def");
            }
        }
    }

    /// Load the generics for another item, adding a corresponding
    /// relation into the dependencies to indicate that the variance
    /// for `current` relies on `def_id`.
    fn read_generics(&mut self, current: &CurrentItem, def_id: DefId) -> &'tcx ty::Generics {
        let generics = self.tcx().generics_of(def_id);
        if self.tcx().dep_graph.is_fully_enabled() {
            self.dependencies.add(current.def_id, def_id);
        }
        generics
    }

    fn opt_inferred_index(&self, param_id: ast::NodeId) -> Option<&InferredIndex> {
        self.terms_cx.inferred_map.get(&param_id)
    }

    fn find_binding_for_lifetime(&self, param_id: ast::NodeId) -> ast::NodeId {
        let tcx = self.terms_cx.tcx;
        assert!(is_lifetime(&tcx.hir, param_id));
        match tcx.named_region_map.defs.get(&param_id) {
            Some(&rl::Region::EarlyBound(_, lifetime_decl_id)) => lifetime_decl_id,
            Some(_) => bug!("should not encounter non early-bound cases"),

            // The lookup should only fail when `param_id` is
            // itself a lifetime binding: use it as the decl_id.
            None => param_id,
        }

    }

    /// Is `param_id` a type parameter for which we infer variance?
    fn is_to_be_inferred(&self, param_id: ast::NodeId) -> bool {
        let result = self.terms_cx.inferred_map.contains_key(&param_id);

        // To safe-guard against invalid inferred_map constructions,
        // double-check if variance is inferred at some use of a type
        // parameter (by inspecting parent of its binding declaration
        // to see if it is introduced by a type or by a fn/impl).

        let check_result = |this: &ConstraintContext| -> bool {
            let tcx = this.terms_cx.tcx;
            let decl_id = this.find_binding_for_lifetime(param_id);
            // Currently only called on lifetimes; double-checking that.
            assert!(is_lifetime(&tcx.hir, param_id));
            let parent_id = tcx.hir.get_parent(decl_id);
            let parent = tcx.hir
                .find(parent_id)
                .unwrap_or_else(|| bug!("tcx.hir missing entry for id: {}", parent_id));

            let is_inferred;
            macro_rules! cannot_happen { () => { {
                bug!("invalid parent: {} for {}",
                     tcx.hir.node_to_string(parent_id),
                     tcx.hir.node_to_string(param_id));
            } } }

            match parent {
                hir_map::NodeItem(p) => {
                    match p.node {
                        hir::ItemTy(..) |
                        hir::ItemEnum(..) |
                        hir::ItemStruct(..) |
                        hir::ItemUnion(..) |
                        hir::ItemTrait(..) => is_inferred = true,
                        hir::ItemFn(..) => is_inferred = false,
                        _ => cannot_happen!(),
                    }
                }
                hir_map::NodeTraitItem(..) => is_inferred = false,
                hir_map::NodeImplItem(..) => is_inferred = false,
                _ => cannot_happen!(),
            }

            return is_inferred;
        };

        assert_eq!(result, check_result(self));

        return result;
    }

    /// Returns a variance term representing the declared variance of the type/region parameter
    /// with the given id.
    fn declared_variance(&self,
                         param_def_id: DefId,
                         item_def_id: DefId,
                         index: usize)
                         -> VarianceTermPtr<'a> {
        assert_eq!(param_def_id.krate, item_def_id.krate);

        if let Some(param_node_id) = self.tcx().hir.as_local_node_id(param_def_id) {
            // Parameter on an item defined within current crate:
            // variance not yet inferred, so return a symbolic
            // variance.
            if let Some(&InferredIndex(index)) = self.opt_inferred_index(param_node_id) {
                self.terms_cx.inferred_infos[index].term
            } else {
                // If there is no inferred entry for a type parameter,
                // it must be declared on a (locally defiend) trait -- they don't
                // get inferreds because they are always invariant.
                if cfg!(debug_assertions) {
                    let item_node_id = self.tcx().hir.as_local_node_id(item_def_id).unwrap();
                    let item = self.tcx().hir.expect_item(item_node_id);
                    let success = match item.node {
                        hir::ItemTrait(..) => true,
                        _ => false,
                    };
                    if !success {
                        bug!("parameter {:?} has no inferred, but declared on non-trait: {:?}",
                             item_def_id,
                             item);
                    }
                }
                self.invariant
            }
        } else {
            // Parameter on an item defined within another crate:
            // variance already inferred, just look it up.
            let variances = self.tcx().variances_of(item_def_id);
            self.constant_term(variances[index])
        }
    }

    fn add_constraint(&mut self,
                      InferredIndex(index): InferredIndex,
                      variance: VarianceTermPtr<'a>) {
        debug!("add_constraint(index={}, variance={:?})", index, variance);
        self.constraints.push(Constraint {
            inferred: InferredIndex(index),
            variance: variance,
        });
    }

    fn contravariant(&mut self, variance: VarianceTermPtr<'a>) -> VarianceTermPtr<'a> {
        self.xform(variance, self.contravariant)
    }

    fn invariant(&mut self, variance: VarianceTermPtr<'a>) -> VarianceTermPtr<'a> {
        self.xform(variance, self.invariant)
    }

    fn constant_term(&self, v: ty::Variance) -> VarianceTermPtr<'a> {
        match v {
            ty::Covariant => self.covariant,
            ty::Invariant => self.invariant,
            ty::Contravariant => self.contravariant,
            ty::Bivariant => self.bivariant,
        }
    }

    fn xform(&mut self, v1: VarianceTermPtr<'a>, v2: VarianceTermPtr<'a>) -> VarianceTermPtr<'a> {
        match (*v1, *v2) {
            (_, ConstantTerm(ty::Covariant)) => {
                // Applying a "covariant" transform is always a no-op
                v1
            }

            (ConstantTerm(c1), ConstantTerm(c2)) => self.constant_term(c1.xform(c2)),

            _ => &*self.terms_cx.arena.alloc(TransformTerm(v1, v2)),
        }
    }

    fn add_constraints_from_trait_ref(&mut self,
                                      current: &CurrentItem,
                                      trait_ref: ty::TraitRef<'tcx>,
                                      variance: VarianceTermPtr<'a>) {
        debug!("add_constraints_from_trait_ref: trait_ref={:?} variance={:?}",
               trait_ref,
               variance);

        let trait_generics = self.tcx().generics_of(trait_ref.def_id);

        self.add_constraints_from_substs(current,
                                         trait_ref.def_id,
                                         &trait_generics.types,
                                         &trait_generics.regions,
                                         trait_ref.substs,
                                         variance);
    }

    /// Adds constraints appropriate for an instance of `ty` appearing
    /// in a context with the generics defined in `generics` and
    /// ambient variance `variance`
    fn add_constraints_from_ty(&mut self,
                               current: &CurrentItem,
                               ty: Ty<'tcx>,
                               variance: VarianceTermPtr<'a>) {
        debug!("add_constraints_from_ty(ty={:?}, variance={:?})",
               ty,
               variance);

        match ty.sty {
            ty::TyBool | ty::TyChar | ty::TyInt(_) | ty::TyUint(_) | ty::TyFloat(_) |
            ty::TyStr | ty::TyNever => {
                // leaf type -- noop
            }

            ty::TyClosure(..) |
            ty::TyAnon(..) => {
                bug!("Unexpected closure type in variance computation");
            }

            ty::TyRef(region, ref mt) => {
                let contra = self.contravariant(variance);
                self.add_constraints_from_region(current, region, contra);
                self.add_constraints_from_mt(current, mt, variance);
            }

            ty::TyArray(typ, _) |
            ty::TySlice(typ) => {
                self.add_constraints_from_ty(current, typ, variance);
            }

            ty::TyRawPtr(ref mt) => {
                self.add_constraints_from_mt(current, mt, variance);
            }

            ty::TyTuple(subtys, _) => {
                for &subty in subtys {
                    self.add_constraints_from_ty(current, subty, variance);
                }
            }

            ty::TyAdt(def, substs) => {
                let adt_generics = self.read_generics(current, def.did);

                self.add_constraints_from_substs(current,
                                                 def.did,
                                                 &adt_generics.types,
                                                 &adt_generics.regions,
                                                 substs,
                                                 variance);
            }

            ty::TyProjection(ref data) => {
                let trait_ref = &data.trait_ref;
                let trait_generics = self.tcx().generics_of(trait_ref.def_id);

                self.add_constraints_from_substs(current,
                                                 trait_ref.def_id,
                                                 &trait_generics.types,
                                                 &trait_generics.regions,
                                                 trait_ref.substs,
                                                 variance);
            }

            ty::TyDynamic(ref data, r) => {
                // The type `Foo<T+'a>` is contravariant w/r/t `'a`:
                let contra = self.contravariant(variance);
                self.add_constraints_from_region(current, r, contra);

                if let Some(p) = data.principal() {
                    let poly_trait_ref = p.with_self_ty(self.tcx(), self.tcx().types.err);
                    self.add_constraints_from_trait_ref(current, poly_trait_ref.0, variance);
                }

                for projection in data.projection_bounds() {
                    self.add_constraints_from_ty(current, projection.0.ty, self.invariant);
                }
            }

            ty::TyParam(ref data) => {
                assert_eq!(current.generics.parent, None);
                let mut i = data.idx as usize;
                if !current.generics.has_self || i > 0 {
                    i -= current.generics.regions.len();
                }
                let def_id = current.generics.types[i].def_id;
                let node_id = self.tcx().hir.as_local_node_id(def_id).unwrap();
                match self.terms_cx.inferred_map.get(&node_id) {
                    Some(&index) => {
                        self.add_constraint(index, variance);
                    }
                    None => {
                        // We do not infer variance for type parameters
                        // declared on methods. They will not be present
                        // in the inferred_map.
                    }
                }
            }

            ty::TyFnDef(.., sig) |
            ty::TyFnPtr(sig) => {
                self.add_constraints_from_sig(current, sig, variance);
            }

            ty::TyError => {
                // we encounter this when walking the trait references for object
                // types, where we use TyError as the Self type
            }

            ty::TyInfer(..) => {
                bug!("unexpected type encountered in \
                      variance inference: {}",
                     ty);
            }
        }
    }

    /// Adds constraints appropriate for a nominal type (enum, struct,
    /// object, etc) appearing in a context with ambient variance `variance`
    fn add_constraints_from_substs(&mut self,
                                   current: &CurrentItem,
                                   def_id: DefId,
                                   type_param_defs: &[ty::TypeParameterDef],
                                   region_param_defs: &[ty::RegionParameterDef],
                                   substs: &Substs<'tcx>,
                                   variance: VarianceTermPtr<'a>) {
        debug!("add_constraints_from_substs(def_id={:?}, substs={:?}, variance={:?})",
               def_id,
               substs,
               variance);

        for p in type_param_defs {
            let variance_decl = self.declared_variance(p.def_id, def_id, p.index as usize);
            let variance_i = self.xform(variance, variance_decl);
            let substs_ty = substs.type_for_def(p);
            debug!("add_constraints_from_substs: variance_decl={:?} variance_i={:?}",
                   variance_decl,
                   variance_i);
            self.add_constraints_from_ty(current, substs_ty, variance_i);
        }

        for p in region_param_defs {
            let variance_decl = self.declared_variance(p.def_id, def_id, p.index as usize);
            let variance_i = self.xform(variance, variance_decl);
            let substs_r = substs.region_for_def(p);
            self.add_constraints_from_region(current, substs_r, variance_i);
        }
    }

    /// Adds constraints appropriate for a function with signature
    /// `sig` appearing in a context with ambient variance `variance`
    fn add_constraints_from_sig(&mut self,
                                current: &CurrentItem,
                                sig: ty::PolyFnSig<'tcx>,
                                variance: VarianceTermPtr<'a>) {
        let contra = self.contravariant(variance);
        for &input in sig.0.inputs() {
            self.add_constraints_from_ty(current, input, contra);
        }
        self.add_constraints_from_ty(current, sig.0.output(), variance);
    }

    /// Adds constraints appropriate for a region appearing in a
    /// context with ambient variance `variance`
    fn add_constraints_from_region(&mut self,
                                   current: &CurrentItem,
                                   region: ty::Region<'tcx>,
                                   variance: VarianceTermPtr<'a>) {
        match *region {
            ty::ReEarlyBound(ref data) => {
                assert_eq!(current.generics.parent, None);
                let i = data.index as usize - current.generics.has_self as usize;
                let def_id = current.generics.regions[i].def_id;
                let node_id = self.tcx().hir.as_local_node_id(def_id).unwrap();
                if self.is_to_be_inferred(node_id) {
                    let &index = self.opt_inferred_index(node_id).unwrap();
                    self.add_constraint(index, variance);
                }
            }

            ty::ReStatic => {}

            ty::ReLateBound(..) => {
                // We do not infer variance for region parameters on
                // methods or in fn types.
            }

            ty::ReFree(..) |
            ty::ReScope(..) |
            ty::ReVar(..) |
            ty::ReSkolemized(..) |
            ty::ReEmpty |
            ty::ReErased => {
                // We don't expect to see anything but 'static or bound
                // regions when visiting member types or method types.
                bug!("unexpected region encountered in variance \
                      inference: {:?}",
                     region);
            }
        }
    }

    /// Adds constraints appropriate for a mutability-type pair
    /// appearing in a context with ambient variance `variance`
    fn add_constraints_from_mt(&mut self,
                               current: &CurrentItem,
                               mt: &ty::TypeAndMut<'tcx>,
                               variance: VarianceTermPtr<'a>) {
        match mt.mutbl {
            hir::MutMutable => {
                let invar = self.invariant(variance);
                self.add_constraints_from_ty(current, mt.ty, invar);
            }

            hir::MutImmutable => {
                self.add_constraints_from_ty(current, mt.ty, variance);
            }
        }
    }
}
