use smallvec::smallvec;

use crate::traits::{Obligation, ObligationCause, PredicateObligation};
use rustc_data_structures::fx::FxHashSet;
use rustc_middle::ty::outlives::Component;
use rustc_middle::ty::{self, ToPolyTraitRef, ToPredicate, TyCtxt, WithConstness};
use rustc_span::Span;

pub fn anonymize_predicate<'tcx>(
    tcx: TyCtxt<'tcx>,
    pred: &ty::Predicate<'tcx>,
) -> ty::Predicate<'tcx> {
    match pred.kind() {
        &ty::PredicateKind::Trait(ref data, constness) => {
            ty::PredicateKind::Trait(tcx.anonymize_late_bound_regions(data), constness)
                .to_predicate(tcx)
        }

        ty::PredicateKind::RegionOutlives(data) => {
            ty::PredicateKind::RegionOutlives(tcx.anonymize_late_bound_regions(data))
                .to_predicate(tcx)
        }

        ty::PredicateKind::TypeOutlives(data) => {
            ty::PredicateKind::TypeOutlives(tcx.anonymize_late_bound_regions(data))
                .to_predicate(tcx)
        }

        ty::PredicateKind::Projection(data) => {
            ty::PredicateKind::Projection(tcx.anonymize_late_bound_regions(data)).to_predicate(tcx)
        }

        &ty::PredicateKind::WellFormed(data) => {
            ty::PredicateKind::WellFormed(data).to_predicate(tcx)
        }

        &ty::PredicateKind::ObjectSafe(data) => {
            ty::PredicateKind::ObjectSafe(data).to_predicate(tcx)
        }

        &ty::PredicateKind::ClosureKind(closure_def_id, closure_substs, kind) => {
            ty::PredicateKind::ClosureKind(closure_def_id, closure_substs, kind).to_predicate(tcx)
        }

        ty::PredicateKind::Subtype(data) => {
            ty::PredicateKind::Subtype(tcx.anonymize_late_bound_regions(data)).to_predicate(tcx)
        }

        &ty::PredicateKind::ConstEvaluatable(def_id, substs) => {
            ty::PredicateKind::ConstEvaluatable(def_id, substs).to_predicate(tcx)
        }

        ty::PredicateKind::ConstEquate(c1, c2) => {
            ty::PredicateKind::ConstEquate(c1, c2).to_predicate(tcx)
        }
    }
}

struct PredicateSet<'tcx> {
    tcx: TyCtxt<'tcx>,
    set: FxHashSet<ty::Predicate<'tcx>>,
}

impl PredicateSet<'tcx> {
    fn new(tcx: TyCtxt<'tcx>) -> Self {
        Self { tcx, set: Default::default() }
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

impl<T: AsRef<ty::Predicate<'tcx>>> Extend<T> for PredicateSet<'tcx> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for pred in iter {
            self.insert(pred.as_ref());
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// `Elaboration` iterator
///////////////////////////////////////////////////////////////////////////

/// "Elaboration" is the process of identifying all the predicates that
/// are implied by a source predicate. Currently, this basically means
/// walking the "supertraits" and other similar assumptions. For example,
/// if we know that `T: Ord`, the elaborator would deduce that `T: PartialOrd`
/// holds as well. Similarly, if we have `trait Foo: 'static`, and we know that
/// `T: Foo`, then we know that `T: 'static`.
pub struct Elaborator<'tcx> {
    stack: Vec<PredicateObligation<'tcx>>,
    visited: PredicateSet<'tcx>,
}

pub fn elaborate_trait_ref<'tcx>(
    tcx: TyCtxt<'tcx>,
    trait_ref: ty::PolyTraitRef<'tcx>,
) -> Elaborator<'tcx> {
    elaborate_predicates(tcx, std::iter::once(trait_ref.without_const().to_predicate(tcx)))
}

pub fn elaborate_trait_refs<'tcx>(
    tcx: TyCtxt<'tcx>,
    trait_refs: impl Iterator<Item = ty::PolyTraitRef<'tcx>>,
) -> Elaborator<'tcx> {
    let predicates = trait_refs.map(|trait_ref| trait_ref.without_const().to_predicate(tcx));
    elaborate_predicates(tcx, predicates)
}

pub fn elaborate_predicates<'tcx>(
    tcx: TyCtxt<'tcx>,
    predicates: impl Iterator<Item = ty::Predicate<'tcx>>,
) -> Elaborator<'tcx> {
    let obligations = predicates.map(|predicate| predicate_obligation(predicate, None)).collect();
    elaborate_obligations(tcx, obligations)
}

pub fn elaborate_obligations<'tcx>(
    tcx: TyCtxt<'tcx>,
    mut obligations: Vec<PredicateObligation<'tcx>>,
) -> Elaborator<'tcx> {
    let mut visited = PredicateSet::new(tcx);
    obligations.retain(|obligation| visited.insert(&obligation.predicate));
    Elaborator { stack: obligations, visited }
}

fn predicate_obligation<'tcx>(
    predicate: ty::Predicate<'tcx>,
    span: Option<Span>,
) -> PredicateObligation<'tcx> {
    let mut cause = ObligationCause::dummy();
    if let Some(span) = span {
        cause.span = span;
    }
    Obligation { cause, param_env: ty::ParamEnv::empty(), recursion_depth: 0, predicate }
}

impl Elaborator<'tcx> {
    pub fn filter_to_traits(self) -> FilterToTraits<Self> {
        FilterToTraits::new(self)
    }

    fn elaborate(&mut self, obligation: &PredicateObligation<'tcx>) {
        let tcx = self.visited.tcx;
        match obligation.predicate.kind() {
            ty::PredicateKind::Trait(ref data, _) => {
                // Get predicates declared on the trait.
                let predicates = tcx.super_predicates_of(data.def_id());

                let obligations = predicates.predicates.iter().map(|(pred, span)| {
                    predicate_obligation(
                        pred.subst_supertrait(tcx, &data.to_poly_trait_ref()),
                        Some(*span),
                    )
                });
                debug!("super_predicates: data={:?}", data);

                // Only keep those bounds that we haven't already seen.
                // This is necessary to prevent infinite recursion in some
                // cases. One common case is when people define
                // `trait Sized: Sized { }` rather than `trait Sized { }`.
                let visited = &mut self.visited;
                let obligations = obligations.filter(|o| visited.insert(&o.predicate));

                self.stack.extend(obligations);
            }
            ty::PredicateKind::WellFormed(..) => {
                // Currently, we do not elaborate WF predicates,
                // although we easily could.
            }
            ty::PredicateKind::ObjectSafe(..) => {
                // Currently, we do not elaborate object-safe
                // predicates.
            }
            ty::PredicateKind::Subtype(..) => {
                // Currently, we do not "elaborate" predicates like `X <: Y`,
                // though conceivably we might.
            }
            ty::PredicateKind::Projection(..) => {
                // Nothing to elaborate in a projection predicate.
            }
            ty::PredicateKind::ClosureKind(..) => {
                // Nothing to elaborate when waiting for a closure's kind to be inferred.
            }
            ty::PredicateKind::ConstEvaluatable(..) => {
                // Currently, we do not elaborate const-evaluatable
                // predicates.
            }
            ty::PredicateKind::ConstEquate(..) => {
                // Currently, we do not elaborate const-equate
                // predicates.
            }
            ty::PredicateKind::RegionOutlives(..) => {
                // Nothing to elaborate from `'a: 'b`.
            }
            ty::PredicateKind::TypeOutlives(ref data) => {
                // We know that `T: 'a` for some type `T`. We can
                // often elaborate this. For example, if we know that
                // `[U]: 'a`, that implies that `U: 'a`. Similarly, if
                // we know `&'a U: 'b`, then we know that `'a: 'b` and
                // `U: 'b`.
                //
                // We can basically ignore bound regions here. So for
                // example `for<'c> Foo<'a,'c>: 'b` can be elaborated to
                // `'a: 'b`.

                // Ignore `for<'a> T: 'a` -- we might in the future
                // consider this as evidence that `T: 'static`, but
                // I'm a bit wary of such constructions and so for now
                // I want to be conservative. --nmatsakis
                let ty_max = data.skip_binder().0;
                let r_min = data.skip_binder().1;
                if r_min.is_late_bound() {
                    return;
                }

                let visited = &mut self.visited;
                let mut components = smallvec![];
                tcx.push_outlives_components(ty_max, &mut components);
                self.stack.extend(
                    components
                        .into_iter()
                        .filter_map(|component| match component {
                            Component::Region(r) => {
                                if r.is_late_bound() {
                                    None
                                } else {
                                    Some(ty::PredicateKind::RegionOutlives(ty::Binder::dummy(
                                        ty::OutlivesPredicate(r, r_min),
                                    )))
                                }
                            }

                            Component::Param(p) => {
                                let ty = tcx.mk_ty_param(p.index, p.name);
                                Some(ty::PredicateKind::TypeOutlives(ty::Binder::dummy(
                                    ty::OutlivesPredicate(ty, r_min),
                                )))
                            }

                            Component::UnresolvedInferenceVariable(_) => None,

                            Component::Projection(_) | Component::EscapingProjection(_) => {
                                // We can probably do more here. This
                                // corresponds to a case like `<T as
                                // Foo<'a>>::U: 'b`.
                                None
                            }
                        })
                        .map(|predicate_kind| predicate_kind.to_predicate(tcx))
                        .filter(|predicate| visited.insert(predicate))
                        .map(|predicate| predicate_obligation(predicate, None)),
                );
            }
        }
    }
}

impl Iterator for Elaborator<'tcx> {
    type Item = PredicateObligation<'tcx>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.stack.len(), None)
    }

    fn next(&mut self) -> Option<Self::Item> {
        // Extract next item from top-most stack frame, if any.
        if let Some(obligation) = self.stack.pop() {
            self.elaborate(&obligation);
            Some(obligation)
        } else {
            None
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Supertrait iterator
///////////////////////////////////////////////////////////////////////////

pub type Supertraits<'tcx> = FilterToTraits<Elaborator<'tcx>>;

pub fn supertraits<'tcx>(
    tcx: TyCtxt<'tcx>,
    trait_ref: ty::PolyTraitRef<'tcx>,
) -> Supertraits<'tcx> {
    elaborate_trait_ref(tcx, trait_ref).filter_to_traits()
}

pub fn transitive_bounds<'tcx>(
    tcx: TyCtxt<'tcx>,
    bounds: impl Iterator<Item = ty::PolyTraitRef<'tcx>>,
) -> Supertraits<'tcx> {
    elaborate_trait_refs(tcx, bounds).filter_to_traits()
}

///////////////////////////////////////////////////////////////////////////
// Other
///////////////////////////////////////////////////////////////////////////

/// A filter around an iterator of predicates that makes it yield up
/// just trait references.
pub struct FilterToTraits<I> {
    base_iterator: I,
}

impl<I> FilterToTraits<I> {
    fn new(base: I) -> FilterToTraits<I> {
        FilterToTraits { base_iterator: base }
    }
}

impl<'tcx, I: Iterator<Item = PredicateObligation<'tcx>>> Iterator for FilterToTraits<I> {
    type Item = ty::PolyTraitRef<'tcx>;

    fn next(&mut self) -> Option<ty::PolyTraitRef<'tcx>> {
        while let Some(obligation) = self.base_iterator.next() {
            if let ty::PredicateKind::Trait(data, _) = obligation.predicate.kind() {
                return Some(data.to_poly_trait_ref());
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, upper) = self.base_iterator.size_hint();
        (0, upper)
    }
}
