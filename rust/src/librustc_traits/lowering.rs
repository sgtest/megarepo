// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rustc::hir::{self, ImplPolarity};
use rustc::hir::def_id::DefId;
use rustc::hir::intravisit::{self, NestedVisitorMap, Visitor};
use rustc::ty::{self, Slice, TyCtxt};
use rustc::ty::subst::Substs;
use rustc::traits::{WhereClauseAtom, PolyDomainGoal, DomainGoal, ProgramClause, Clause, Goal};
use syntax::ast;
use rustc_data_structures::sync::Lrc;

use std::iter;

trait Lower<T> {
    /// Lower a rustc construction (e.g. `ty::TraitPredicate`) to a chalk-like type.
    fn lower(&self) -> T;
}

impl<T, U> Lower<Vec<U>> for Vec<T> where T: Lower<U> {
    fn lower(&self) -> Vec<U> {
        self.iter().map(|item| item.lower()).collect()
    }
}

impl<'tcx> Lower<WhereClauseAtom<'tcx>> for ty::TraitPredicate<'tcx> {
    fn lower(&self) -> WhereClauseAtom<'tcx> {
        WhereClauseAtom::Implemented(*self)
    }
}

impl<'tcx> Lower<WhereClauseAtom<'tcx>> for ty::ProjectionPredicate<'tcx> {
    fn lower(&self) -> WhereClauseAtom<'tcx> {
        WhereClauseAtom::ProjectionEq(*self)
    }
}

impl<'tcx, T> Lower<DomainGoal<'tcx>> for T where T: Lower<WhereClauseAtom<'tcx>> {
    fn lower(&self) -> DomainGoal<'tcx> {
        DomainGoal::Holds(self.lower())
    }
}

impl<'tcx> Lower<DomainGoal<'tcx>> for ty::RegionOutlivesPredicate<'tcx> {
    fn lower(&self) -> DomainGoal<'tcx> {
        DomainGoal::RegionOutlives(*self)
    }
}

impl<'tcx> Lower<DomainGoal<'tcx>> for ty::TypeOutlivesPredicate<'tcx> {
    fn lower(&self) -> DomainGoal<'tcx> {
        DomainGoal::TypeOutlives(*self)
    }
}

/// `ty::Binder` is used for wrapping a rustc construction possibly containing generic
/// lifetimes, e.g. `for<'a> T: Fn(&'a i32)`. Instead of representing higher-ranked things
/// in that leaf-form (i.e. `Holds(Implemented(Binder<TraitPredicate>))` in the previous
/// example), we model them with quantified domain goals, e.g. as for the previous example:
/// `forall<'a> { T: Fn(&'a i32) }` which corresponds to something like
/// `Binder<Holds(Implemented(TraitPredicate))>`.
impl<'tcx, T> Lower<PolyDomainGoal<'tcx>> for ty::Binder<T>
    where T: Lower<DomainGoal<'tcx>> + ty::fold::TypeFoldable<'tcx>
{
    fn lower(&self) -> PolyDomainGoal<'tcx> {
        self.map_bound_ref(|p| p.lower())
    }
}

impl<'tcx> Lower<PolyDomainGoal<'tcx>> for ty::Predicate<'tcx> {
    fn lower(&self) -> PolyDomainGoal<'tcx> {
        use rustc::ty::Predicate::*;

        match self {
            Trait(predicate) => predicate.lower(),
            RegionOutlives(predicate) => predicate.lower(),
            TypeOutlives(predicate) => predicate.lower(),
            Projection(predicate) => predicate.lower(),
            WellFormed(ty) => ty::Binder::dummy(DomainGoal::WellFormedTy(*ty)),
            ObjectSafe(..) |
            ClosureKind(..) |
            Subtype(..) |
            ConstEvaluatable(..) => unimplemented!(),
        }
    }
}

/// Transforms an existing goal into a FromEnv goal.
///
/// Used for lowered where clauses (see rustc guide).
trait IntoFromEnvGoal {
    fn into_from_env_goal(self) -> Self;
}

impl<'tcx> IntoFromEnvGoal for DomainGoal<'tcx> {
    fn into_from_env_goal(self) -> DomainGoal<'tcx> {
        use self::DomainGoal::*;
        match self {
            Holds(wc_atom) => FromEnv(wc_atom),
            WellFormed(..) |
            FromEnv(..) |
            WellFormedTy(..) |
            FromEnvTy(..) |
            RegionOutlives(..) |
            TypeOutlives(..) => self,
        }
    }
}

crate fn program_clauses_for<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, def_id: DefId)
                                       -> Lrc<&'tcx Slice<Clause<'tcx>>>
{
    let node_id = tcx.hir.as_local_node_id(def_id).unwrap();
    let item = tcx.hir.expect_item(node_id);
    match item.node {
        hir::ItemTrait(..) => program_clauses_for_trait(tcx, def_id),
        hir::ItemImpl(..) => program_clauses_for_impl(tcx, def_id),

        // FIXME: other constructions e.g. traits, associated types...
        _ => Lrc::new(tcx.mk_clauses(iter::empty::<Clause>())),
    }
}

fn program_clauses_for_trait<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, def_id: DefId)
                                       -> Lrc<&'tcx Slice<Clause<'tcx>>>
{
    // `trait Trait<P1..Pn> where WC { .. } // P0 == Self`

    // Rule Implemented-From-Env (see rustc guide)
    //
    // ```
    // forall<Self, P1..Pn> {
    //   Implemented(Self: Trait<P1..Pn>) :- FromEnv(Self: Trait<P1..Pn>)
    // }
    // ```

    // `Self: Trait<P1..Pn>`
    let trait_pred = ty::TraitPredicate {
        trait_ref: ty::TraitRef {
            def_id,
            substs: Substs::identity_for_item(tcx, def_id)
        }
    };
    // `FromEnv(Self: Trait<P1..Pn>)`
    let from_env = Goal::from(DomainGoal::FromEnv(trait_pred.lower()));
    // `Implemented(Self: Trait<P1..Pn>)`
    let impl_trait = DomainGoal::Holds(WhereClauseAtom::Implemented(trait_pred));

    // `Implemented(Self: Trait<P1..Pn>) :- FromEnv(Self: Trait<P1..Pn>)`
    let implemented_from_env = ProgramClause {
        goal: impl_trait,
        hypotheses: tcx.mk_goals(iter::once(from_env)),
    };
    let clauses = iter::once(
        Clause::ForAll(ty::Binder::dummy(implemented_from_env))
    );

    // Rule Implied-Bound-From-Trait
    //
    // For each where clause WC:
    // ```
    // forall<Self, P1..Pn> {
    //   FromEnv(WC) :- FromEnv(Self: Trait<P1..Pn)
    // }
    // ```

    // `FromEnv(WC) :- FromEnv(Self: Trait<P1..Pn>)`, for each where clause WC
    // FIXME: Remove the [1..] slice; this is a hack because the query
    // predicates_of currently includes the trait itself (`Self: Trait<P1..Pn>`).
    let where_clauses = &tcx.predicates_of(def_id).predicates;
    let implied_bound_clauses =
        where_clauses[1..].into_iter()
        .map(|wc| implied_bound_from_trait(tcx, trait_pred, wc));

    Lrc::new(tcx.mk_clauses(clauses.chain(implied_bound_clauses)))
}

/// For a given `where_clause`, returns a clause `FromEnv(WC) :- FromEnv(Self: Trait<P1..Pn>)`.
fn implied_bound_from_trait<'a, 'tcx>(
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    trait_pred: ty::TraitPredicate<'tcx>,
    where_clause: &ty::Predicate<'tcx>,
) -> Clause<'tcx> {
    // `FromEnv(Self: Trait<P1..Pn>)`
    let impl_trait = DomainGoal::FromEnv(WhereClauseAtom::Implemented(trait_pred));

    // `FromEnv(WC) :- FromEnv(Self: Trait<P1..Pn>)`
    Clause::ForAll(
        where_clause.lower().map_bound(|goal| ProgramClause {
            goal: goal.into_from_env_goal(),
            hypotheses: tcx.mk_goals(iter::once(Goal::from(impl_trait))),
        })
    )
}

fn program_clauses_for_impl<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, def_id: DefId)
                                      -> Lrc<&'tcx Slice<Clause<'tcx>>>
{
    if let ImplPolarity::Negative = tcx.impl_polarity(def_id) {
        return Lrc::new(tcx.mk_clauses(iter::empty::<Clause>()));
    }

    // Rule Implemented-From-Impl (see rustc guide)
    //
    // `impl<P0..Pn> Trait<A1..An> for A0 where WC { .. }`
    //
    // ```
    // forall<P0..Pn> {
    //   Implemented(A0: Trait<A1..An>) :- WC
    // }
    // ```

    let trait_ref = tcx.impl_trait_ref(def_id).unwrap();
    // `Implemented(A0: Trait<A1..An>)`
    let trait_pred = ty::TraitPredicate { trait_ref }.lower();
     // `WC`
    let where_clauses = tcx.predicates_of(def_id).predicates.lower();

     // `Implemented(A0: Trait<A1..An>) :- WC`
    let clause = ProgramClause {
        goal: trait_pred,
        hypotheses: tcx.mk_goals(
            where_clauses.into_iter().map(|wc| Goal::from_poly_domain_goal(wc, tcx))
        )
    };
    Lrc::new(tcx.mk_clauses(iter::once(Clause::ForAll(ty::Binder::dummy(clause)))))
}

pub fn dump_program_clauses<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>) {
    if !tcx.features().rustc_attrs {
        return;
    }

    let mut visitor = ClauseDumper { tcx };
    tcx.hir.krate().visit_all_item_likes(&mut visitor.as_deep_visitor());
}

struct ClauseDumper<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
}

impl<'a, 'tcx> ClauseDumper<'a, 'tcx > {
    fn process_attrs(&mut self, node_id: ast::NodeId, attrs: &[ast::Attribute]) {
        let def_id = self.tcx.hir.local_def_id(node_id);
        for attr in attrs {
            if attr.check_name("rustc_dump_program_clauses") {
                let clauses = self.tcx.program_clauses_for(def_id);
                for clause in *clauses {
                    // Skip the top-level binder for a less verbose output
                    let program_clause = match clause {
                        Clause::Implies(program_clause) => program_clause,
                        Clause::ForAll(program_clause) => program_clause.skip_binder(),
                    };
                    self.tcx.sess.struct_span_err(attr.span, &format!("{}", program_clause)).emit();
                }
            }
        }
    }
}

impl<'a, 'tcx> Visitor<'tcx> for ClauseDumper<'a, 'tcx> {
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'tcx> {
        NestedVisitorMap::OnlyBodies(&self.tcx.hir)
    }

    fn visit_item(&mut self, item: &'tcx hir::Item) {
        self.process_attrs(item.id, &item.attrs);
        intravisit::walk_item(self, item);
    }

    fn visit_trait_item(&mut self, trait_item: &'tcx hir::TraitItem) {
        self.process_attrs(trait_item.id, &trait_item.attrs);
        intravisit::walk_trait_item(self, trait_item);
    }

    fn visit_impl_item(&mut self, impl_item: &'tcx hir::ImplItem) {
        self.process_attrs(impl_item.id, &impl_item.attrs);
        intravisit::walk_impl_item(self, impl_item);
    }

    fn visit_struct_field(&mut self, s: &'tcx hir::StructField) {
        self.process_attrs(s.id, &s.attrs);
        intravisit::walk_struct_field(self, s);
    }
}
