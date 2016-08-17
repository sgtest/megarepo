// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use dep_graph::{DepNode, DepTrackingMapConfig};
use hir::def_id::DefId;
use ty::{self, Ty};
use std::marker::PhantomData;
use std::rc::Rc;
use syntax::{attr, ast};

macro_rules! dep_map_ty {
    ($ty_name:ident : $node_name:ident ($key:ty) -> $value:ty) => {
        pub struct $ty_name<'tcx> {
            data: PhantomData<&'tcx ()>
        }

        impl<'tcx> DepTrackingMapConfig for $ty_name<'tcx> {
            type Key = $key;
            type Value = $value;
            fn to_dep_node(key: &$key) -> DepNode<DefId> { DepNode::$node_name(*key) }
        }
    }
}

dep_map_ty! { ImplOrTraitItems: ImplOrTraitItems(DefId) -> ty::ImplOrTraitItem<'tcx> }
dep_map_ty! { Tcache: ItemSignature(DefId) -> Ty<'tcx> }
dep_map_ty! { Generics: ItemSignature(DefId) -> &'tcx ty::Generics<'tcx> }
dep_map_ty! { Predicates: ItemSignature(DefId) -> ty::GenericPredicates<'tcx> }
dep_map_ty! { SuperPredicates: ItemSignature(DefId) -> ty::GenericPredicates<'tcx> }
dep_map_ty! { TraitItemDefIds: TraitItemDefIds(DefId) -> Rc<Vec<ty::ImplOrTraitItemId>> }
dep_map_ty! { ImplTraitRefs: ItemSignature(DefId) -> Option<ty::TraitRef<'tcx>> }
dep_map_ty! { TraitDefs: ItemSignature(DefId) -> &'tcx ty::TraitDef<'tcx> }
dep_map_ty! { AdtDefs: ItemSignature(DefId) -> ty::AdtDefMaster<'tcx> }
dep_map_ty! { ItemVariances: ItemSignature(DefId) -> Rc<ty::ItemVariances> }
dep_map_ty! { InherentImpls: InherentImpls(DefId) -> Rc<Vec<DefId>> }
dep_map_ty! { ImplItems: ImplItems(DefId) -> Vec<ty::ImplOrTraitItemId> }
dep_map_ty! { TraitItems: TraitItems(DefId) -> Rc<Vec<ty::ImplOrTraitItem<'tcx>>> }
dep_map_ty! { ReprHints: ReprHints(DefId) -> Rc<Vec<attr::ReprAttr>> }
dep_map_ty! { InlinedClosures: Hir(DefId) -> ast::NodeId }
