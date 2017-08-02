// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Partitioning Codegen Units for Incremental Compilation
//! ======================================================
//!
//! The task of this module is to take the complete set of translation items of
//! a crate and produce a set of codegen units from it, where a codegen unit
//! is a named set of (translation-item, linkage) pairs. That is, this module
//! decides which translation item appears in which codegen units with which
//! linkage. The following paragraphs describe some of the background on the
//! partitioning scheme.
//!
//! The most important opportunity for saving on compilation time with
//! incremental compilation is to avoid re-translating and re-optimizing code.
//! Since the unit of translation and optimization for LLVM is "modules" or, how
//! we call them "codegen units", the particulars of how much time can be saved
//! by incremental compilation are tightly linked to how the output program is
//! partitioned into these codegen units prior to passing it to LLVM --
//! especially because we have to treat codegen units as opaque entities once
//! they are created: There is no way for us to incrementally update an existing
//! LLVM module and so we have to build any such module from scratch if it was
//! affected by some change in the source code.
//!
//! From that point of view it would make sense to maximize the number of
//! codegen units by, for example, putting each function into its own module.
//! That way only those modules would have to be re-compiled that were actually
//! affected by some change, minimizing the number of functions that could have
//! been re-used but just happened to be located in a module that is
//! re-compiled.
//!
//! However, since LLVM optimization does not work across module boundaries,
//! using such a highly granular partitioning would lead to very slow runtime
//! code since it would effectively prohibit inlining and other inter-procedure
//! optimizations. We want to avoid that as much as possible.
//!
//! Thus we end up with a trade-off: The bigger the codegen units, the better
//! LLVM's optimizer can do its work, but also the smaller the compilation time
//! reduction we get from incremental compilation.
//!
//! Ideally, we would create a partitioning such that there are few big codegen
//! units with few interdependencies between them. For now though, we use the
//! following heuristic to determine the partitioning:
//!
//! - There are two codegen units for every source-level module:
//! - One for "stable", that is non-generic, code
//! - One for more "volatile" code, i.e. monomorphized instances of functions
//!   defined in that module
//!
//! In order to see why this heuristic makes sense, let's take a look at when a
//! codegen unit can get invalidated:
//!
//! 1. The most straightforward case is when the BODY of a function or global
//! changes. Then any codegen unit containing the code for that item has to be
//! re-compiled. Note that this includes all codegen units where the function
//! has been inlined.
//!
//! 2. The next case is when the SIGNATURE of a function or global changes. In
//! this case, all codegen units containing a REFERENCE to that item have to be
//! re-compiled. This is a superset of case 1.
//!
//! 3. The final and most subtle case is when a REFERENCE to a generic function
//! is added or removed somewhere. Even though the definition of the function
//! might be unchanged, a new REFERENCE might introduce a new monomorphized
//! instance of this function which has to be placed and compiled somewhere.
//! Conversely, when removing a REFERENCE, it might have been the last one with
//! that particular set of generic arguments and thus we have to remove it.
//!
//! From the above we see that just using one codegen unit per source-level
//! module is not such a good idea, since just adding a REFERENCE to some
//! generic item somewhere else would invalidate everything within the module
//! containing the generic item. The heuristic above reduces this detrimental
//! side-effect of references a little by at least not touching the non-generic
//! code of the module.
//!
//! A Note on Inlining
//! ------------------
//! As briefly mentioned above, in order for LLVM to be able to inline a
//! function call, the body of the function has to be available in the LLVM
//! module where the call is made. This has a few consequences for partitioning:
//!
//! - The partitioning algorithm has to take care of placing functions into all
//!   codegen units where they should be available for inlining. It also has to
//!   decide on the correct linkage for these functions.
//!
//! - The partitioning algorithm has to know which functions are likely to get
//!   inlined, so it can distribute function instantiations accordingly. Since
//!   there is no way of knowing for sure which functions LLVM will decide to
//!   inline in the end, we apply a heuristic here: Only functions marked with
//!   #[inline] are considered for inlining by the partitioner. The current
//!   implementation will not try to determine if a function is likely to be
//!   inlined by looking at the functions definition.
//!
//! Note though that as a side-effect of creating a codegen units per
//! source-level module, functions from the same module will be available for
//! inlining, even when they are not marked #[inline].

use back::symbol_export::ExportedSymbols;
use collector::InliningMap;
use common;
use context::SharedCrateContext;
use llvm;
use rustc::dep_graph::{DepNode, WorkProductId};
use rustc::hir::def_id::DefId;
use rustc::hir::map::DefPathData;
use rustc::session::config::NUMBERED_CODEGEN_UNIT_MARKER;
use rustc::ty::{self, TyCtxt, InstanceDef};
use rustc::ty::item_path::characteristic_def_id_of_type;
use rustc::util::nodemap::{FxHashMap, FxHashSet};
use rustc_incremental::IchHasher;
use std::collections::hash_map::Entry;
use std::hash::Hash;
use syntax::ast::NodeId;
use syntax::symbol::{Symbol, InternedString};
use trans_item::{TransItem, InstantiationMode};

pub enum PartitioningStrategy {
    /// Generate one codegen unit per source-level module.
    PerModule,

    /// Partition the whole crate into a fixed number of codegen units.
    FixedUnitCount(usize)
}

pub struct CodegenUnit<'tcx> {
    /// A name for this CGU. Incremental compilation requires that
    /// name be unique amongst **all** crates.  Therefore, it should
    /// contain something unique to this crate (e.g., a module path)
    /// as well as the crate name and disambiguator.
    name: InternedString,

    items: FxHashMap<TransItem<'tcx>, (llvm::Linkage, llvm::Visibility)>,
}

impl<'tcx> CodegenUnit<'tcx> {
    pub fn new(name: InternedString,
               items: FxHashMap<TransItem<'tcx>, (llvm::Linkage, llvm::Visibility)>)
               -> Self {
        CodegenUnit {
            name,
            items,
        }
    }

    pub fn empty(name: InternedString) -> Self {
        Self::new(name, FxHashMap())
    }

    pub fn contains_item(&self, item: &TransItem<'tcx>) -> bool {
        self.items.contains_key(item)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn items(&self) -> &FxHashMap<TransItem<'tcx>, (llvm::Linkage, llvm::Visibility)> {
        &self.items
    }

    pub fn work_product_id(&self) -> WorkProductId {
        WorkProductId::from_cgu_name(self.name())
    }

    pub fn work_product_dep_node(&self) -> DepNode {
        self.work_product_id().to_dep_node()
    }

    pub fn compute_symbol_name_hash<'a>(&self,
                                        scx: &SharedCrateContext<'a, 'tcx>)
                                        -> u64 {
        let mut state = IchHasher::new();
        let all_items = self.items_in_deterministic_order(scx.tcx());
        for (item, (linkage, visibility)) in all_items {
            let symbol_name = item.symbol_name(scx.tcx());
            symbol_name.len().hash(&mut state);
            symbol_name.hash(&mut state);
            linkage.hash(&mut state);
            visibility.hash(&mut state);
        }
        state.finish().to_smaller_hash()
    }

    pub fn items_in_deterministic_order<'a>(&self,
                                            tcx: TyCtxt<'a, 'tcx, 'tcx>)
                                            -> Vec<(TransItem<'tcx>,
                                                   (llvm::Linkage, llvm::Visibility))> {
        // The codegen tests rely on items being process in the same order as
        // they appear in the file, so for local items, we sort by node_id first
        #[derive(PartialEq, Eq, PartialOrd, Ord)]
        pub struct ItemSortKey(Option<NodeId>, ty::SymbolName);

        fn item_sort_key<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                   item: TransItem<'tcx>) -> ItemSortKey {
            ItemSortKey(match item {
                TransItem::Fn(instance) => {
                    tcx.hir.as_local_node_id(instance.def_id())
                }
                TransItem::Static(node_id) | TransItem::GlobalAsm(node_id) => {
                    Some(node_id)
                }
            }, item.symbol_name(tcx))
        }

        let items: Vec<_> = self.items.iter().map(|(&i, &l)| (i, l)).collect();
        let mut items : Vec<_> = items.iter()
            .map(|il| (il, item_sort_key(tcx, il.0))).collect();
        items.sort_by(|&(_, ref key1), &(_, ref key2)| key1.cmp(key2));
        items.into_iter().map(|(&item_linkage, _)| item_linkage).collect()
    }
}


// Anything we can't find a proper codegen unit for goes into this.
const FALLBACK_CODEGEN_UNIT: &'static str = "__rustc_fallback_codegen_unit";

pub fn partition<'a, 'tcx, I>(scx: &SharedCrateContext<'a, 'tcx>,
                              trans_items: I,
                              strategy: PartitioningStrategy,
                              inlining_map: &InliningMap<'tcx>,
                              exported_symbols: &ExportedSymbols)
                              -> Vec<CodegenUnit<'tcx>>
    where I: Iterator<Item = TransItem<'tcx>>
{
    let tcx = scx.tcx();

    // In the first step, we place all regular translation items into their
    // respective 'home' codegen unit. Regular translation items are all
    // functions and statics defined in the local crate.
    let mut initial_partitioning = place_root_translation_items(scx,
                                                                exported_symbols,
                                                                trans_items);

    debug_dump(tcx, "INITIAL PARTITIONING:", initial_partitioning.codegen_units.iter());

    // If the partitioning should produce a fixed count of codegen units, merge
    // until that count is reached.
    if let PartitioningStrategy::FixedUnitCount(count) = strategy {
        merge_codegen_units(&mut initial_partitioning, count, &tcx.crate_name.as_str());

        debug_dump(tcx, "POST MERGING:", initial_partitioning.codegen_units.iter());
    }

    // In the next step, we use the inlining map to determine which additional
    // translation items have to go into each codegen unit. These additional
    // translation items can be drop-glue, functions from external crates, and
    // local functions the definition of which is marked with #[inline].
    let mut post_inlining = place_inlined_translation_items(initial_partitioning,
                                                            inlining_map);

    debug_dump(tcx, "POST INLINING:", post_inlining.codegen_units.iter());

    // Next we try to make as many symbols "internal" as possible, so LLVM has
    // more freedom to optimize.
    internalize_symbols(tcx, &mut post_inlining, inlining_map);

    // Finally, sort by codegen unit name, so that we get deterministic results
    let PostInliningPartitioning {
        codegen_units: mut result,
        trans_item_placements: _,
        internalization_candidates: _,
    } = post_inlining;

    result.sort_by(|cgu1, cgu2| {
        (&cgu1.name[..]).cmp(&cgu2.name[..])
    });

    if scx.sess().opts.enable_dep_node_debug_strs() {
        for cgu in &result {
            let dep_node = cgu.work_product_dep_node();
            scx.tcx().dep_graph.register_dep_node_debug_str(dep_node,
                                                            || cgu.name().to_string());
        }
    }

    result
}

struct PreInliningPartitioning<'tcx> {
    codegen_units: Vec<CodegenUnit<'tcx>>,
    roots: FxHashSet<TransItem<'tcx>>,
    internalization_candidates: FxHashSet<TransItem<'tcx>>,
}

/// For symbol internalization, we need to know whether a symbol/trans-item is
/// accessed from outside the codegen unit it is defined in. This type is used
/// to keep track of that.
#[derive(Clone, PartialEq, Eq, Debug)]
enum TransItemPlacement {
    SingleCgu { cgu_name: InternedString },
    MultipleCgus,
}

struct PostInliningPartitioning<'tcx> {
    codegen_units: Vec<CodegenUnit<'tcx>>,
    trans_item_placements: FxHashMap<TransItem<'tcx>, TransItemPlacement>,
    internalization_candidates: FxHashSet<TransItem<'tcx>>,
}

fn place_root_translation_items<'a, 'tcx, I>(scx: &SharedCrateContext<'a, 'tcx>,
                                             exported_symbols: &ExportedSymbols,
                                             trans_items: I)
                                             -> PreInliningPartitioning<'tcx>
    where I: Iterator<Item = TransItem<'tcx>>
{
    let tcx = scx.tcx();
    let exported_symbols = exported_symbols.local_exports();

    let mut roots = FxHashSet();
    let mut codegen_units = FxHashMap();
    let is_incremental_build = tcx.sess.opts.incremental.is_some();
    let mut internalization_candidates = FxHashSet();

    for trans_item in trans_items {
        let is_root = trans_item.instantiation_mode(tcx) == InstantiationMode::GloballyShared;

        if is_root {
            let characteristic_def_id = characteristic_def_id_of_trans_item(scx, trans_item);
            let is_volatile = is_incremental_build &&
                              trans_item.is_generic_fn();

            let codegen_unit_name = match characteristic_def_id {
                Some(def_id) => compute_codegen_unit_name(tcx, def_id, is_volatile),
                None => Symbol::intern(FALLBACK_CODEGEN_UNIT).as_str(),
            };

            let make_codegen_unit = || {
                CodegenUnit::empty(codegen_unit_name.clone())
            };

            let mut codegen_unit = codegen_units.entry(codegen_unit_name.clone())
                                                .or_insert_with(make_codegen_unit);

            let (linkage, visibility) = match trans_item.explicit_linkage(tcx) {
                Some(explicit_linkage) => (explicit_linkage, llvm::Visibility::Default),
                None => {
                    match trans_item {
                        TransItem::Fn(ref instance) => {
                            let visibility = match instance.def {
                                InstanceDef::Item(def_id) => {
                                    if let Some(node_id) = tcx.hir.as_local_node_id(def_id) {
                                        if exported_symbols.contains(&node_id) {
                                            llvm::Visibility::Default
                                        } else {
                                            internalization_candidates.insert(trans_item);
                                            llvm::Visibility::Hidden
                                        }
                                    } else {
                                        internalization_candidates.insert(trans_item);
                                        llvm::Visibility::Hidden
                                    }
                                }
                                InstanceDef::FnPtrShim(..) |
                                InstanceDef::Virtual(..) |
                                InstanceDef::Intrinsic(..) |
                                InstanceDef::ClosureOnceShim { .. } |
                                InstanceDef::DropGlue(..) => {
                                    bug!("partitioning: Encountered unexpected
                                          root translation item: {:?}",
                                          trans_item)
                                }
                            };
                            (llvm::ExternalLinkage, visibility)
                        }
                        TransItem::Static(node_id) |
                        TransItem::GlobalAsm(node_id) => {
                            let visibility = if exported_symbols.contains(&node_id) {
                                llvm::Visibility::Default
                            } else {
                                internalization_candidates.insert(trans_item);
                                llvm::Visibility::Hidden
                            };
                            (llvm::ExternalLinkage, visibility)
                        }
                    }
                }
            };

            codegen_unit.items.insert(trans_item, (linkage, visibility));
            roots.insert(trans_item);
        }
    }

    // always ensure we have at least one CGU; otherwise, if we have a
    // crate with just types (for example), we could wind up with no CGU
    if codegen_units.is_empty() {
        let codegen_unit_name = Symbol::intern(FALLBACK_CODEGEN_UNIT).as_str();
        codegen_units.insert(codegen_unit_name.clone(),
                             CodegenUnit::empty(codegen_unit_name.clone()));
    }

    PreInliningPartitioning {
        codegen_units: codegen_units.into_iter()
                                    .map(|(_, codegen_unit)| codegen_unit)
                                    .collect(),
        roots,
        internalization_candidates,
    }
}

fn merge_codegen_units<'tcx>(initial_partitioning: &mut PreInliningPartitioning<'tcx>,
                             target_cgu_count: usize,
                             crate_name: &str) {
    assert!(target_cgu_count >= 1);
    let codegen_units = &mut initial_partitioning.codegen_units;

    // Merge the two smallest codegen units until the target size is reached.
    // Note that "size" is estimated here rather inaccurately as the number of
    // translation items in a given unit. This could be improved on.
    while codegen_units.len() > target_cgu_count {
        // Sort small cgus to the back
        codegen_units.sort_by_key(|cgu| -(cgu.items.len() as i64));
        let smallest = codegen_units.pop().unwrap();
        let second_smallest = codegen_units.last_mut().unwrap();

        for (k, v) in smallest.items.into_iter() {
            second_smallest.items.insert(k, v);
        }
    }

    for (index, cgu) in codegen_units.iter_mut().enumerate() {
        cgu.name = numbered_codegen_unit_name(crate_name, index);
    }

    // If the initial partitioning contained less than target_cgu_count to begin
    // with, we won't have enough codegen units here, so add a empty units until
    // we reach the target count
    while codegen_units.len() < target_cgu_count {
        let index = codegen_units.len();
        codegen_units.push(
            CodegenUnit::empty(numbered_codegen_unit_name(crate_name, index)));
    }
}

fn place_inlined_translation_items<'tcx>(initial_partitioning: PreInliningPartitioning<'tcx>,
                                         inlining_map: &InliningMap<'tcx>)
                                         -> PostInliningPartitioning<'tcx> {
    let mut new_partitioning = Vec::new();
    let mut trans_item_placements = FxHashMap();

    let PreInliningPartitioning {
        codegen_units: initial_cgus,
        roots,
        internalization_candidates,
    } = initial_partitioning;

    let single_codegen_unit = initial_cgus.len() == 1;

    for old_codegen_unit in initial_cgus {
        // Collect all items that need to be available in this codegen unit
        let mut reachable = FxHashSet();
        for root in old_codegen_unit.items.keys() {
            follow_inlining(*root, inlining_map, &mut reachable);
        }

        let mut new_codegen_unit = CodegenUnit {
            name: old_codegen_unit.name,
            items: FxHashMap(),
        };

        // Add all translation items that are not already there
        for trans_item in reachable {
            if let Some(linkage) = old_codegen_unit.items.get(&trans_item) {
                // This is a root, just copy it over
                new_codegen_unit.items.insert(trans_item, *linkage);
            } else {
                if roots.contains(&trans_item) {
                    bug!("GloballyShared trans-item inlined into other CGU: \
                          {:?}", trans_item);
                }

                // This is a cgu-private copy
                new_codegen_unit.items.insert(trans_item,
                                              (llvm::InternalLinkage, llvm::Visibility::Default));
            }

            if !single_codegen_unit {
                // If there is more than one codegen unit, we need to keep track
                // in which codegen units each translation item is placed:
                match trans_item_placements.entry(trans_item) {
                    Entry::Occupied(e) => {
                        let placement = e.into_mut();
                        debug_assert!(match *placement {
                            TransItemPlacement::SingleCgu { ref cgu_name } => {
                                *cgu_name != new_codegen_unit.name
                            }
                            TransItemPlacement::MultipleCgus => true,
                        });
                        *placement = TransItemPlacement::MultipleCgus;
                    }
                    Entry::Vacant(e) => {
                        e.insert(TransItemPlacement::SingleCgu {
                            cgu_name: new_codegen_unit.name.clone()
                        });
                    }
                }
            }
        }

        new_partitioning.push(new_codegen_unit);
    }

    return PostInliningPartitioning {
        codegen_units: new_partitioning,
        trans_item_placements,
        internalization_candidates,
    };

    fn follow_inlining<'tcx>(trans_item: TransItem<'tcx>,
                             inlining_map: &InliningMap<'tcx>,
                             visited: &mut FxHashSet<TransItem<'tcx>>) {
        if !visited.insert(trans_item) {
            return;
        }

        inlining_map.with_inlining_candidates(trans_item, |target| {
            follow_inlining(target, inlining_map, visited);
        });
    }
}

fn internalize_symbols<'a, 'tcx>(_tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                 partitioning: &mut PostInliningPartitioning<'tcx>,
                                 inlining_map: &InliningMap<'tcx>) {
    if partitioning.codegen_units.len() == 1 {
        // Fast path for when there is only one codegen unit. In this case we
        // can internalize all candidates, since there is nowhere else they
        // could be accessed from.
        for cgu in &mut partitioning.codegen_units {
            for candidate in &partitioning.internalization_candidates {
                cgu.items.insert(*candidate, (llvm::InternalLinkage,
                                              llvm::Visibility::Default));
            }
        }

        return;
    }

    // Build a map from every translation item to all the translation items that
    // reference it.
    let mut accessor_map: FxHashMap<TransItem<'tcx>, Vec<TransItem<'tcx>>> = FxHashMap();
    inlining_map.iter_accesses(|accessor, accessees| {
        for accessee in accessees {
            accessor_map.entry(*accessee)
                        .or_insert(Vec::new())
                        .push(accessor);
        }
    });

    let trans_item_placements = &partitioning.trans_item_placements;

    // For each internalization candidates in each codegen unit, check if it is
    // accessed from outside its defining codegen unit.
    for cgu in &mut partitioning.codegen_units {
        let home_cgu = TransItemPlacement::SingleCgu {
            cgu_name: cgu.name.clone()
        };

        for (accessee, linkage_and_visibility) in &mut cgu.items {
            if !partitioning.internalization_candidates.contains(accessee) {
                // This item is no candidate for internalizing, so skip it.
                continue
            }
            debug_assert_eq!(trans_item_placements[accessee], home_cgu);

            if let Some(accessors) = accessor_map.get(accessee) {
                if accessors.iter()
                            .filter_map(|accessor| {
                                // Some accessors might not have been
                                // instantiated. We can safely ignore those.
                                trans_item_placements.get(accessor)
                            })
                            .any(|placement| *placement != home_cgu) {
                    // Found an accessor from another CGU, so skip to the next
                    // item without marking this one as internal.
                    continue
                }
            }

            // If we got here, we did not find any accesses from other CGUs,
            // so it's fine to make this translation item internal.
            *linkage_and_visibility = (llvm::InternalLinkage, llvm::Visibility::Default);
        }
    }
}

fn characteristic_def_id_of_trans_item<'a, 'tcx>(scx: &SharedCrateContext<'a, 'tcx>,
                                                 trans_item: TransItem<'tcx>)
                                                 -> Option<DefId> {
    let tcx = scx.tcx();
    match trans_item {
        TransItem::Fn(instance) => {
            let def_id = match instance.def {
                ty::InstanceDef::Item(def_id) => def_id,
                ty::InstanceDef::FnPtrShim(..) |
                ty::InstanceDef::ClosureOnceShim { .. } |
                ty::InstanceDef::Intrinsic(..) |
                ty::InstanceDef::DropGlue(..) |
                ty::InstanceDef::Virtual(..) => return None
            };

            // If this is a method, we want to put it into the same module as
            // its self-type. If the self-type does not provide a characteristic
            // DefId, we use the location of the impl after all.

            if tcx.trait_of_item(def_id).is_some() {
                let self_ty = instance.substs.type_at(0);
                // This is an implementation of a trait method.
                return characteristic_def_id_of_type(self_ty).or(Some(def_id));
            }

            if let Some(impl_def_id) = tcx.impl_of_method(def_id) {
                // This is a method within an inherent impl, find out what the
                // self-type is:
                let impl_self_ty = common::def_ty(scx, impl_def_id, instance.substs);
                if let Some(def_id) = characteristic_def_id_of_type(impl_self_ty) {
                    return Some(def_id);
                }
            }

            Some(def_id)
        }
        TransItem::Static(node_id) |
        TransItem::GlobalAsm(node_id) => Some(tcx.hir.local_def_id(node_id)),
    }
}

fn compute_codegen_unit_name<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                       def_id: DefId,
                                       volatile: bool)
                                       -> InternedString {
    // Unfortunately we cannot just use the `ty::item_path` infrastructure here
    // because we need paths to modules and the DefIds of those are not
    // available anymore for external items.
    let mut mod_path = String::with_capacity(64);

    let def_path = tcx.def_path(def_id);
    mod_path.push_str(&tcx.crate_name(def_path.krate).as_str());

    for part in tcx.def_path(def_id)
                   .data
                   .iter()
                   .take_while(|part| {
                        match part.data {
                            DefPathData::Module(..) => true,
                            _ => false,
                        }
                    }) {
        mod_path.push_str("-");
        mod_path.push_str(&part.data.as_interned_str());
    }

    if volatile {
        mod_path.push_str(".volatile");
    }

    return Symbol::intern(&mod_path[..]).as_str();
}

fn numbered_codegen_unit_name(crate_name: &str, index: usize) -> InternedString {
    Symbol::intern(&format!("{}{}{}", crate_name, NUMBERED_CODEGEN_UNIT_MARKER, index)).as_str()
}

fn debug_dump<'a, 'b, 'tcx, I>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                               label: &str,
                               cgus: I)
    where I: Iterator<Item=&'b CodegenUnit<'tcx>>,
          'tcx: 'a + 'b
{
    if cfg!(debug_assertions) {
        debug!("{}", label);
        for cgu in cgus {
            debug!("CodegenUnit {}:", cgu.name);

            for (trans_item, linkage) in &cgu.items {
                let symbol_name = trans_item.symbol_name(tcx);
                let symbol_hash_start = symbol_name.rfind('h');
                let symbol_hash = symbol_hash_start.map(|i| &symbol_name[i ..])
                                                   .unwrap_or("<no hash>");

                debug!(" - {} [{:?}] [{}]",
                       trans_item.to_string(tcx),
                       linkage,
                       symbol_hash);
            }

            debug!("");
        }
    }
}
