// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Module for inferring the variance of type and lifetime
//! parameters. See README.md for details.

use arena;
use rustc::dep_graph::DepKind;
use rustc::hir;
use rustc::hir::def_id::{CrateNum, DefId, LOCAL_CRATE};
use rustc::ty::{self, CrateVariancesMap, TyCtxt};
use rustc::ty::maps::Providers;
use std::rc::Rc;

/// Defines the `TermsContext` basically houses an arena where we can
/// allocate terms.
mod terms;

/// Code to gather up constraints.
mod constraints;

/// Code to solve constraints and write out the results.
mod solve;

/// Code to write unit tests of variance.
pub mod test;

/// Code for transforming variances.
mod xform;

pub fn provide(providers: &mut Providers) {
    *providers = Providers {
        variances_of,
        crate_variances,
        ..*providers
    };
}

fn crate_variances<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, crate_num: CrateNum)
                             -> Rc<CrateVariancesMap> {
    assert_eq!(crate_num, LOCAL_CRATE);
    let mut arena = arena::TypedArena::new();
    let terms_cx = terms::determine_parameters_to_be_inferred(tcx, &mut arena);
    let constraints_cx = constraints::add_constraints_from_crate(terms_cx);
    Rc::new(solve::solve_constraints(constraints_cx))
}

fn variances_of<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, item_def_id: DefId)
                            -> Rc<Vec<ty::Variance>> {
    let id = tcx.hir.as_local_node_id(item_def_id).expect("expected local def-id");
    let unsupported = || {
        // Variance not relevant.
        span_bug!(tcx.hir.span(id), "asked to compute variance for wrong kind of item")
    };
    match tcx.hir.get(id) {
        hir::map::NodeItem(item) => match item.node {
            hir::ItemEnum(..) |
            hir::ItemStruct(..) |
            hir::ItemUnion(..) |
            hir::ItemFn(..) => {}

            _ => unsupported()
        },

        hir::map::NodeTraitItem(item) => match item.node {
            hir::TraitItemKind::Method(..) => {}

            _ => unsupported()
        },

        hir::map::NodeImplItem(item) => match item.node {
            hir::ImplItemKind::Method(..) => {}

            _ => unsupported()
        },

        hir::map::NodeForeignItem(item) => match item.node {
            hir::ForeignItemFn(..) => {}

            _ => unsupported()
        },

        hir::map::NodeVariant(_) | hir::map::NodeStructCtor(_) => {}

        _ => unsupported()
    }

    // Everything else must be inferred.

    // Lacking red/green, we read the variances for all items here
    // but ignore the dependencies, then re-synthesize the ones we need.
    let crate_map = tcx.dep_graph.with_ignore(|| tcx.crate_variances(LOCAL_CRATE));
    let dep_node = item_def_id.to_dep_node(tcx, DepKind::ItemVarianceConstraints);
    tcx.dep_graph.read(dep_node);
    for &dep_def_id in crate_map.dependencies.less_than(&item_def_id) {
        if dep_def_id.is_local() {
            let dep_node = dep_def_id.to_dep_node(tcx, DepKind::ItemVarianceConstraints);
            tcx.dep_graph.read(dep_node);
        } else {
            let dep_node = dep_def_id.to_dep_node(tcx, DepKind::ItemVariances);
            tcx.dep_graph.read(dep_node);
        }
    }

    crate_map.variances.get(&item_def_id)
                       .unwrap_or(&crate_map.empty_variance)
                       .clone()
}

