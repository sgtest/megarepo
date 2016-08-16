// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use hir::def_id::DefId;
use infer::InferCtxt;
use ty::subst::{Subst, Substs};
use ty::{self, Ty, TyCtxt, ToPredicate, ToPolyTraitRef};
use syntax_pos::Span;
use util::common::ErrorReported;
use util::nodemap::FnvHashSet;

use super::{Obligation, ObligationCause, PredicateObligation, SelectionContext, Normalized};

fn anonymize_predicate<'a, 'gcx, 'tcx>(tcx: TyCtxt<'a, 'gcx, 'tcx>,
                                       pred: &ty::Predicate<'tcx>)
                                       -> ty::Predicate<'tcx> {
    match *pred {
        ty::Predicate::Trait(ref data) =>
            ty::Predicate::Trait(tcx.anonymize_late_bound_regions(data)),

        ty::Predicate::Rfc1592(ref data) =>
            ty::Predicate::Rfc1592(Box::new(anonymize_predicate(tcx, data))),

        ty::Predicate::Equate(ref data) =>
            ty::Predicate::Equate(tcx.anonymize_late_bound_regions(data)),

        ty::Predicate::RegionOutlives(ref data) =>
            ty::Predicate::RegionOutlives(tcx.anonymize_late_bound_regions(data)),

        ty::Predicate::TypeOutlives(ref data) =>
            ty::Predicate::TypeOutlives(tcx.anonymize_late_bound_regions(data)),

        ty::Predicate::Projection(ref data) =>
            ty::Predicate::Projection(tcx.anonymize_late_bound_regions(data)),

        ty::Predicate::WellFormed(data) =>
            ty::Predicate::WellFormed(data),

        ty::Predicate::ObjectSafe(data) =>
            ty::Predicate::ObjectSafe(data),

        ty::Predicate::ClosureKind(closure_def_id, kind) =>
            ty::Predicate::ClosureKind(closure_def_id, kind)
    }
}


struct PredicateSet<'a, 'gcx: 'a+'tcx, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'gcx, 'tcx>,
    set: FnvHashSet<ty::Predicate<'tcx>>,
}

impl<'a, 'gcx, 'tcx> PredicateSet<'a, 'gcx, 'tcx> {
    fn new(tcx: TyCtxt<'a, 'gcx, 'tcx>) -> PredicateSet<'a, 'gcx, 'tcx> {
        PredicateSet { tcx: tcx, set: FnvHashSet() }
    }

    fn insert(&mut self, pred: &ty::Predicate<'tcx>) -> bool {
        // We have to be careful here because we want
        //
        //    for<'a> Foo<&'a int>
        //
        // and
        //
        //    for<'b> Foo<&'b int>
        //
        // to be considered equivalent. So normalize all late-bound
        // regions before we throw things into the underlying set.
        self.set.insert(anonymize_predicate(self.tcx, pred))
    }
}

///////////////////////////////////////////////////////////////////////////
// `Elaboration` iterator
///////////////////////////////////////////////////////////////////////////

/// "Elaboration" is the process of identifying all the predicates that
/// are implied by a source predicate. Currently this basically means
/// walking the "supertraits" and other similar assumptions. For
/// example, if we know that `T : Ord`, the elaborator would deduce
/// that `T : PartialOrd` holds as well. Similarly, if we have `trait
/// Foo : 'static`, and we know that `T : Foo`, then we know that `T :
/// 'static`.
pub struct Elaborator<'a, 'gcx: 'a+'tcx, 'tcx: 'a> {
    stack: Vec<ty::Predicate<'tcx>>,
    visited: PredicateSet<'a, 'gcx, 'tcx>,
}

pub fn elaborate_trait_ref<'cx, 'gcx, 'tcx>(
    tcx: TyCtxt<'cx, 'gcx, 'tcx>,
    trait_ref: ty::PolyTraitRef<'tcx>)
    -> Elaborator<'cx, 'gcx, 'tcx>
{
    elaborate_predicates(tcx, vec![trait_ref.to_predicate()])
}

pub fn elaborate_trait_refs<'cx, 'gcx, 'tcx>(
    tcx: TyCtxt<'cx, 'gcx, 'tcx>,
    trait_refs: &[ty::PolyTraitRef<'tcx>])
    -> Elaborator<'cx, 'gcx, 'tcx>
{
    let predicates = trait_refs.iter()
                               .map(|trait_ref| trait_ref.to_predicate())
                               .collect();
    elaborate_predicates(tcx, predicates)
}

pub fn elaborate_predicates<'cx, 'gcx, 'tcx>(
    tcx: TyCtxt<'cx, 'gcx, 'tcx>,
    mut predicates: Vec<ty::Predicate<'tcx>>)
    -> Elaborator<'cx, 'gcx, 'tcx>
{
    let mut visited = PredicateSet::new(tcx);
    predicates.retain(|pred| visited.insert(pred));
    Elaborator { stack: predicates, visited: visited }
}

impl<'cx, 'gcx, 'tcx> Elaborator<'cx, 'gcx, 'tcx> {
    pub fn filter_to_traits(self) -> FilterToTraits<Self> {
        FilterToTraits::new(self)
    }

    fn push(&mut self, predicate: &ty::Predicate<'tcx>) {
        let tcx = self.visited.tcx;
        match *predicate {
            ty::Predicate::Trait(ref data) => {
                // Predicates declared on the trait.
                let predicates = tcx.lookup_super_predicates(data.def_id());

                let mut predicates: Vec<_> =
                    predicates.predicates
                              .iter()
                              .map(|p| p.subst_supertrait(tcx, &data.to_poly_trait_ref()))
                              .collect();

                debug!("super_predicates: data={:?} predicates={:?}",
                       data, predicates);

                // Only keep those bounds that we haven't already
                // seen.  This is necessary to prevent infinite
                // recursion in some cases.  One common case is when
                // people define `trait Sized: Sized { }` rather than `trait
                // Sized { }`.
                predicates.retain(|r| self.visited.insert(r));

                self.stack.extend(predicates);
            }
            ty::Predicate::Rfc1592(..) => {
                // Nothing to elaborate.
            }
            ty::Predicate::WellFormed(..) => {
                // Currently, we do not elaborate WF predicates,
                // although we easily could.
            }
            ty::Predicate::ObjectSafe(..) => {
                // Currently, we do not elaborate object-safe
                // predicates.
            }
            ty::Predicate::Equate(..) => {
                // Currently, we do not "elaborate" predicates like
                // `X == Y`, though conceivably we might. For example,
                // `&X == &Y` implies that `X == Y`.
            }
            ty::Predicate::Projection(..) => {
                // Nothing to elaborate in a projection predicate.
            }
            ty::Predicate::ClosureKind(..) => {
                // Nothing to elaborate when waiting for a closure's kind to be inferred.
            }
            ty::Predicate::RegionOutlives(..) |
            ty::Predicate::TypeOutlives(..) => {
                // Currently, we do not "elaborate" predicates like
                // `'a : 'b` or `T : 'a`.  We could conceivably do
                // more here.  For example,
                //
                //     &'a int : 'b
                //
                // implies that
                //
                //     'a : 'b
                //
                // and we could get even more if we took WF
                // constraints into account. For example,
                //
                //     &'a &'b int : 'c
                //
                // implies that
                //
                //     'b : 'a
                //     'a : 'c
            }
        }
    }
}

impl<'cx, 'gcx, 'tcx> Iterator for Elaborator<'cx, 'gcx, 'tcx> {
    type Item = ty::Predicate<'tcx>;

    fn next(&mut self) -> Option<ty::Predicate<'tcx>> {
        // Extract next item from top-most stack frame, if any.
        let next_predicate = match self.stack.pop() {
            Some(predicate) => predicate,
            None => {
                // No more stack frames. Done.
                return None;
            }
        };
        self.push(&next_predicate);
        return Some(next_predicate);
    }
}

///////////////////////////////////////////////////////////////////////////
// Supertrait iterator
///////////////////////////////////////////////////////////////////////////

pub type Supertraits<'cx, 'gcx, 'tcx> = FilterToTraits<Elaborator<'cx, 'gcx, 'tcx>>;

pub fn supertraits<'cx, 'gcx, 'tcx>(tcx: TyCtxt<'cx, 'gcx, 'tcx>,
                                    trait_ref: ty::PolyTraitRef<'tcx>)
                                    -> Supertraits<'cx, 'gcx, 'tcx>
{
    elaborate_trait_ref(tcx, trait_ref).filter_to_traits()
}

pub fn transitive_bounds<'cx, 'gcx, 'tcx>(tcx: TyCtxt<'cx, 'gcx, 'tcx>,
                                          bounds: &[ty::PolyTraitRef<'tcx>])
                                          -> Supertraits<'cx, 'gcx, 'tcx>
{
    elaborate_trait_refs(tcx, bounds).filter_to_traits()
}

///////////////////////////////////////////////////////////////////////////
// Iterator over def-ids of supertraits

pub struct SupertraitDefIds<'a, 'gcx: 'a+'tcx, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'gcx, 'tcx>,
    stack: Vec<DefId>,
    visited: FnvHashSet<DefId>,
}

pub fn supertrait_def_ids<'cx, 'gcx, 'tcx>(tcx: TyCtxt<'cx, 'gcx, 'tcx>,
                                           trait_def_id: DefId)
                                           -> SupertraitDefIds<'cx, 'gcx, 'tcx>
{
    SupertraitDefIds {
        tcx: tcx,
        stack: vec![trait_def_id],
        visited: Some(trait_def_id).into_iter().collect(),
    }
}

impl<'cx, 'gcx, 'tcx> Iterator for SupertraitDefIds<'cx, 'gcx, 'tcx> {
    type Item = DefId;

    fn next(&mut self) -> Option<DefId> {
        let def_id = match self.stack.pop() {
            Some(def_id) => def_id,
            None => { return None; }
        };

        let predicates = self.tcx.lookup_super_predicates(def_id);
        let visited = &mut self.visited;
        self.stack.extend(
            predicates.predicates
                      .iter()
                      .filter_map(|p| p.to_opt_poly_trait_ref())
                      .map(|t| t.def_id())
                      .filter(|&super_def_id| visited.insert(super_def_id)));
        Some(def_id)
    }
}

///////////////////////////////////////////////////////////////////////////
// Other
///////////////////////////////////////////////////////////////////////////

/// A filter around an iterator of predicates that makes it yield up
/// just trait references.
pub struct FilterToTraits<I> {
    base_iterator: I
}

impl<I> FilterToTraits<I> {
    fn new(base: I) -> FilterToTraits<I> {
        FilterToTraits { base_iterator: base }
    }
}

impl<'tcx,I:Iterator<Item=ty::Predicate<'tcx>>> Iterator for FilterToTraits<I> {
    type Item = ty::PolyTraitRef<'tcx>;

    fn next(&mut self) -> Option<ty::PolyTraitRef<'tcx>> {
        loop {
            match self.base_iterator.next() {
                None => {
                    return None;
                }
                Some(ty::Predicate::Trait(data)) => {
                    return Some(data.to_poly_trait_ref());
                }
                Some(_) => {
                }
            }
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Other
///////////////////////////////////////////////////////////////////////////

/// Instantiate all bound parameters of the impl with the given substs,
/// returning the resulting trait ref and all obligations that arise.
/// The obligations are closed under normalization.
pub fn impl_trait_ref_and_oblig<'a, 'gcx, 'tcx>(selcx: &mut SelectionContext<'a, 'gcx, 'tcx>,
                                                impl_def_id: DefId,
                                                impl_substs: &Substs<'tcx>)
                                                -> (ty::TraitRef<'tcx>,
                                                    Vec<PredicateObligation<'tcx>>)
{
    let impl_trait_ref =
        selcx.tcx().impl_trait_ref(impl_def_id).unwrap();
    let impl_trait_ref =
        impl_trait_ref.subst(selcx.tcx(), impl_substs);
    let Normalized { value: impl_trait_ref, obligations: normalization_obligations1 } =
        super::normalize(selcx, ObligationCause::dummy(), &impl_trait_ref);

    let predicates = selcx.tcx().lookup_predicates(impl_def_id);
    let predicates = predicates.instantiate(selcx.tcx(), impl_substs);
    let Normalized { value: predicates, obligations: normalization_obligations2 } =
        super::normalize(selcx, ObligationCause::dummy(), &predicates);
    let impl_obligations =
        predicates_for_generics(ObligationCause::dummy(), 0, &predicates);

    let impl_obligations: Vec<_> =
        impl_obligations.into_iter()
        .chain(normalization_obligations1)
        .chain(normalization_obligations2)
        .collect();

    (impl_trait_ref, impl_obligations)
}

// determine the `self` type, using fresh variables for all variables
// declared on the impl declaration e.g., `impl<A,B> for Box<[(A,B)]>`
// would return ($0, $1) where $0 and $1 are freshly instantiated type
// variables.
pub fn fresh_type_vars_for_impl<'a, 'gcx, 'tcx>(infcx: &InferCtxt<'a, 'gcx, 'tcx>,
                                                span: Span,
                                                impl_def_id: DefId)
                                                -> &'tcx Substs<'tcx>
{
    let tcx = infcx.tcx;
    let impl_generics = tcx.lookup_item_type(impl_def_id).generics;
    infcx.fresh_substs_for_generics(span, &impl_generics)
}

/// See `super::obligations_for_generics`
pub fn predicates_for_generics<'tcx>(cause: ObligationCause<'tcx>,
                                     recursion_depth: usize,
                                     generic_bounds: &ty::InstantiatedPredicates<'tcx>)
                                     -> Vec<PredicateObligation<'tcx>>
{
    debug!("predicates_for_generics(generic_bounds={:?})",
           generic_bounds);

    generic_bounds.predicates.iter().map(|predicate| {
        Obligation { cause: cause.clone(),
                     recursion_depth: recursion_depth,
                     predicate: predicate.clone() }
    }).collect()
}

pub fn predicate_for_trait_ref<'tcx>(
    cause: ObligationCause<'tcx>,
    trait_ref: ty::TraitRef<'tcx>,
    recursion_depth: usize)
    -> PredicateObligation<'tcx>
{
    Obligation {
        cause: cause,
        recursion_depth: recursion_depth,
        predicate: trait_ref.to_predicate(),
    }
}

impl<'a, 'gcx, 'tcx> TyCtxt<'a, 'gcx, 'tcx> {
    pub fn trait_ref_for_builtin_bound(self,
        builtin_bound: ty::BuiltinBound,
        param_ty: Ty<'tcx>)
        -> Result<ty::TraitRef<'tcx>, ErrorReported>
    {
        match self.lang_items.from_builtin_kind(builtin_bound) {
            Ok(def_id) => {
                Ok(ty::TraitRef {
                    def_id: def_id,
                    substs: self.mk_substs(Substs::empty().with_self_ty(param_ty))
                })
            }
            Err(e) => {
                self.sess.err(&e);
                Err(ErrorReported)
            }
        }
    }

    pub fn predicate_for_trait_def(self,
        cause: ObligationCause<'tcx>,
        trait_def_id: DefId,
        recursion_depth: usize,
        param_ty: Ty<'tcx>,
        ty_params: Vec<Ty<'tcx>>)
        -> PredicateObligation<'tcx>
    {
        let trait_ref = ty::TraitRef {
            def_id: trait_def_id,
            substs: self.mk_substs(Substs::new_trait(ty_params, vec![], param_ty))
        };
        predicate_for_trait_ref(cause, trait_ref, recursion_depth)
    }

    pub fn predicate_for_builtin_bound(self,
        cause: ObligationCause<'tcx>,
        builtin_bound: ty::BuiltinBound,
        recursion_depth: usize,
        param_ty: Ty<'tcx>)
        -> Result<PredicateObligation<'tcx>, ErrorReported>
    {
        let trait_ref = self.trait_ref_for_builtin_bound(builtin_bound, param_ty)?;
        Ok(predicate_for_trait_ref(cause, trait_ref, recursion_depth))
    }

    /// Cast a trait reference into a reference to one of its super
    /// traits; returns `None` if `target_trait_def_id` is not a
    /// supertrait.
    pub fn upcast_choices(self,
                          source_trait_ref: ty::PolyTraitRef<'tcx>,
                          target_trait_def_id: DefId)
                          -> Vec<ty::PolyTraitRef<'tcx>>
    {
        if source_trait_ref.def_id() == target_trait_def_id {
            return vec![source_trait_ref]; // shorcut the most common case
        }

        supertraits(self, source_trait_ref)
            .filter(|r| r.def_id() == target_trait_def_id)
            .collect()
    }

    /// Given a trait `trait_ref`, returns the number of vtable entries
    /// that come from `trait_ref`, excluding its supertraits. Used in
    /// computing the vtable base for an upcast trait of a trait object.
    pub fn count_own_vtable_entries(self, trait_ref: ty::PolyTraitRef<'tcx>) -> usize {
        let mut entries = 0;
        // Count number of methods and add them to the total offset.
        // Skip over associated types and constants.
        for trait_item in &self.trait_items(trait_ref.def_id())[..] {
            if let ty::MethodTraitItem(_) = *trait_item {
                entries += 1;
            }
        }
        entries
    }

    /// Given an upcast trait object described by `object`, returns the
    /// index of the method `method_def_id` (which should be part of
    /// `object.upcast_trait_ref`) within the vtable for `object`.
    pub fn get_vtable_index_of_object_method<N>(self,
                                                object: &super::VtableObjectData<'tcx, N>,
                                                method_def_id: DefId) -> usize {
        // Count number of methods preceding the one we are selecting and
        // add them to the total offset.
        // Skip over associated types and constants.
        let mut entries = object.vtable_base;
        for trait_item in &self.trait_items(object.upcast_trait_ref.def_id())[..] {
            if trait_item.def_id() == method_def_id {
                // The item with the ID we were given really ought to be a method.
                assert!(match *trait_item {
                    ty::MethodTraitItem(_) => true,
                    _ => false
                });

                return entries;
            }
            if let ty::MethodTraitItem(_) = *trait_item {
                entries += 1;
            }
        }

        bug!("get_vtable_index_of_object_method: {:?} was not found",
             method_def_id);
    }

    pub fn closure_trait_ref_and_return_type(self,
        fn_trait_def_id: DefId,
        self_ty: Ty<'tcx>,
        sig: &ty::PolyFnSig<'tcx>,
        tuple_arguments: TupleArgumentsFlag)
        -> ty::Binder<(ty::TraitRef<'tcx>, Ty<'tcx>)>
    {
        let arguments_tuple = match tuple_arguments {
            TupleArgumentsFlag::No => sig.0.inputs[0],
            TupleArgumentsFlag::Yes => self.mk_tup(sig.0.inputs.to_vec()),
        };
        let trait_substs = Substs::new_trait(vec![arguments_tuple], vec![], self_ty);
        let trait_ref = ty::TraitRef {
            def_id: fn_trait_def_id,
            substs: self.mk_substs(trait_substs),
        };
        ty::Binder((trait_ref, sig.0.output))
    }
}

pub enum TupleArgumentsFlag { Yes, No }
