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
use rustc::ty::{self, TyCtxt};
use rustc::traits::{QuantifierKind, Goal, DomainGoal, Clause, WhereClauseAtom};
use syntax::ast;
use rustc_data_structures::sync::Lrc;

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
/// example), we model them with quantified goals, e.g. as for the previous example:
/// `forall<'a> { T: Fn(&'a i32) }` which corresponds to something like
/// `Binder<Holds(Implemented(TraitPredicate))>`.
///
/// Also, if `self` does not contain generic lifetimes, we can safely drop the binder and we
/// can directly lower to a leaf goal instead of a quantified goal.
impl<'tcx, T> Lower<Goal<'tcx>> for ty::Binder<T>
    where T: Lower<DomainGoal<'tcx>> + ty::fold::TypeFoldable<'tcx> + Copy
{
    fn lower(&self) -> Goal<'tcx> {
        match self.no_late_bound_regions() {
            Some(p) => p.lower().into(),
            None => Goal::Quantified(
                QuantifierKind::Universal,
                Box::new(self.map_bound(|p| p.lower().into()))
            ),
        }
    }
}

impl<'tcx> Lower<Goal<'tcx>> for ty::Predicate<'tcx> {
    fn lower(&self) -> Goal<'tcx> {
        use rustc::ty::Predicate::*;

        match self {
            Trait(predicate) => predicate.lower(),
            RegionOutlives(predicate) => predicate.lower(),
            TypeOutlives(predicate) => predicate.lower(),
            Projection(predicate) => predicate.lower(),
            WellFormed(ty) => DomainGoal::WellFormedTy(*ty).into(),
            ObjectSafe(..) |
            ClosureKind(..) |
            Subtype(..) |
            ConstEvaluatable(..) => unimplemented!(),
        }
    }
}

crate fn program_clauses_for<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, def_id: DefId)
    -> Lrc<Vec<Clause<'tcx>>>
{
    let node_id = tcx.hir.as_local_node_id(def_id).unwrap();
    let item = tcx.hir.expect_item(node_id);
    match item.node {
        hir::ItemImpl(..) => program_clauses_for_impl(tcx, def_id),

        // FIXME: other constructions e.g. traits, associated types...
        _ => Lrc::new(vec![]),
    }
}

fn program_clauses_for_impl<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, def_id: DefId)
    -> Lrc<Vec<Clause<'tcx>>>
{
    if let ImplPolarity::Negative = tcx.impl_polarity(def_id) {
        return Lrc::new(vec![]);
    }

    // Rule Implemented-From-Impl
    //
    // (see rustc guide)

    let trait_ref = tcx.impl_trait_ref(def_id).unwrap();
    let trait_ref = ty::TraitPredicate { trait_ref }.lower();
    let where_clauses = tcx.predicates_of(def_id).predicates.lower();

    let clause = Clause::Implies(where_clauses, trait_ref);
    Lrc::new(vec![clause])
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

impl <'a, 'tcx> ClauseDumper<'a, 'tcx > {
    fn process_attrs(&mut self, node_id: ast::NodeId, attrs: &[ast::Attribute]) {
        let def_id = self.tcx.hir.local_def_id(node_id);
        for attr in attrs {
            if attr.check_name("rustc_dump_program_clauses") {
                let clauses = self.tcx.program_clauses_for(def_id);
                for clause in &*clauses {
                    self.tcx.sess.struct_span_err(attr.span, &format!("{}", clause)).emit();
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
