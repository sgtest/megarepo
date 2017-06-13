// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Overlap: No two impls for the same trait are implemented for the
//! same type. Likewise, no two inherent impls for a given type
//! constructor provide a method with the same name.

use rustc::traits;
use rustc::ty::{self, TyCtxt, TypeFoldable};
use syntax::ast;
use rustc::hir;
use rustc::hir::itemlikevisit::ItemLikeVisitor;

pub fn check_default_impls<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>) {
    let mut overlap = OverlapChecker { tcx };

    // this secondary walk specifically checks for some other cases,
    // like defaulted traits, for which additional overlap rules exist
    tcx.hir.krate().visit_all_item_likes(&mut overlap);
}

pub fn check_impl<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, node_id: ast::NodeId) {
    let impl_def_id = tcx.hir.local_def_id(node_id);
    let trait_ref = tcx.impl_trait_ref(impl_def_id).unwrap();
    let trait_def_id = trait_ref.def_id;

    if trait_ref.references_error() {
        debug!("coherence: skipping impl {:?} with error {:?}",
               impl_def_id, trait_ref);
        return
    }

    // Trigger building the specialization graph for the trait of this impl.
    // This will detect any overlap errors.
    tcx.specialization_graph_of(trait_def_id);


    // check for overlap with the automatic `impl Trait for Trait`
    if let ty::TyDynamic(ref data, ..) = trait_ref.self_ty().sty {
        // This is something like impl Trait1 for Trait2. Illegal
        // if Trait1 is a supertrait of Trait2 or Trait2 is not object safe.

        if data.principal().map_or(true, |p| !tcx.is_object_safe(p.def_id())) {
            // This is an error, but it will be reported by wfcheck.  Ignore it here.
            // This is tested by `coherence-impl-trait-for-trait-object-safe.rs`.
        } else {
            let mut supertrait_def_ids =
                traits::supertrait_def_ids(tcx,
                                           data.principal().unwrap().def_id());
            if supertrait_def_ids.any(|d| d == trait_def_id) {
                span_err!(tcx.sess,
                          tcx.span_of_impl(impl_def_id).unwrap(),
                          E0371,
                          "the object type `{}` automatically \
                           implements the trait `{}`",
                          trait_ref.self_ty(),
                          tcx.item_path_str(trait_def_id));
            }
        }
    }
}

struct OverlapChecker<'cx, 'tcx: 'cx> {
    tcx: TyCtxt<'cx, 'tcx, 'tcx>,
}

impl<'cx, 'tcx, 'v> ItemLikeVisitor<'v> for OverlapChecker<'cx, 'tcx> {
    fn visit_item(&mut self, item: &'v hir::Item) {
        match item.node {
            hir::ItemDefaultImpl(..) => {
                // look for another default impl; note that due to the
                // general orphan/coherence rules, it must always be
                // in this crate.
                let impl_def_id = self.tcx.hir.local_def_id(item.id);
                let trait_ref = self.tcx.impl_trait_ref(impl_def_id).unwrap();

                let prev_id = self.tcx.hir.trait_default_impl(trait_ref.def_id).unwrap();
                if prev_id != item.id {
                    let mut err = struct_span_err!(self.tcx.sess,
                                                   self.tcx.span_of_impl(impl_def_id).unwrap(),
                                                   E0521,
                                                   "redundant default implementations of trait \
                                                    `{}`:",
                                                   trait_ref);
                    err.span_note(self.tcx
                                      .span_of_impl(self.tcx.hir.local_def_id(prev_id))
                                      .unwrap(),
                                  "redundant implementation is here:");
                    err.emit();
                }
            }
            hir::ItemImpl(.., Some(_), _, _) => {
            }
            _ => {}
        }
    }

    fn visit_trait_item(&mut self, _trait_item: &hir::TraitItem) {
    }

    fn visit_impl_item(&mut self, _impl_item: &hir::ImplItem) {
    }
}
