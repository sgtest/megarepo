//! Partitioning Codegen Units for Incremental Compilation
//! ======================================================
//!
//! The task of this module is to take the complete set of monomorphizations of
//! a crate and produce a set of codegen units from it, where a codegen unit
//! is a named set of (mono-item, linkage) pairs. That is, this module
//! decides which monomorphization appears in which codegen units with which
//! linkage. The following paragraphs describe some of the background on the
//! partitioning scheme.
//!
//! The most important opportunity for saving on compilation time with
//! incremental compilation is to avoid re-codegenning and re-optimizing code.
//! Since the unit of codegen and optimization for LLVM is "modules" or, how
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
//! - One for more "volatile" code, i.e., monomorphized instances of functions
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
//!   `#[inline]` are considered for inlining by the partitioner. The current
//!   implementation will not try to determine if a function is likely to be
//!   inlined by looking at the functions definition.
//!
//! Note though that as a side-effect of creating a codegen units per
//! source-level module, functions from the same module will be available for
//! inlining, even when they are not marked `#[inline]`.

use std::cmp;
use std::collections::hash_map::Entry;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_data_structures::sync;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::{DefId, DefIdSet, LOCAL_CRATE};
use rustc_hir::definitions::DefPathDataName;
use rustc_middle::middle::codegen_fn_attrs::CodegenFnAttrFlags;
use rustc_middle::middle::exported_symbols::{SymbolExportInfo, SymbolExportLevel};
use rustc_middle::mir;
use rustc_middle::mir::mono::{
    CodegenUnit, CodegenUnitNameBuilder, InstantiationMode, Linkage, MonoItem, Visibility,
};
use rustc_middle::query::Providers;
use rustc_middle::ty::print::{characteristic_def_id_of_type, with_no_trimmed_paths};
use rustc_middle::ty::{self, visit::TypeVisitableExt, InstanceDef, TyCtxt};
use rustc_session::config::{DumpMonoStatsFormat, SwitchWithOptPath};
use rustc_span::symbol::Symbol;

use crate::collector::InliningMap;
use crate::collector::{self, MonoItemCollectionMode};
use crate::errors::{CouldntDumpMonoStats, SymbolAlreadyDefined, UnknownCguCollectionMode};

struct PartitioningCx<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    target_cgu_count: usize,
    inlining_map: &'a InliningMap<'tcx>,
}

struct PlacedRootMonoItems<'tcx> {
    /// The codegen units, sorted by name to make things deterministic.
    codegen_units: Vec<CodegenUnit<'tcx>>,

    roots: FxHashSet<MonoItem<'tcx>>,
    internalization_candidates: FxHashSet<MonoItem<'tcx>>,
}

// The output CGUs are sorted by name.
fn partition<'tcx, I>(
    tcx: TyCtxt<'tcx>,
    mono_items: &mut I,
    max_cgu_count: usize,
    inlining_map: &InliningMap<'tcx>,
) -> Vec<CodegenUnit<'tcx>>
where
    I: Iterator<Item = MonoItem<'tcx>>,
{
    let _prof_timer = tcx.prof.generic_activity("cgu_partitioning");

    let cx = &PartitioningCx { tcx, target_cgu_count: max_cgu_count, inlining_map };

    // In the first step, we place all regular monomorphizations into their
    // respective 'home' codegen unit. Regular monomorphizations are all
    // functions and statics defined in the local crate.
    let PlacedRootMonoItems { mut codegen_units, roots, internalization_candidates } = {
        let _prof_timer = tcx.prof.generic_activity("cgu_partitioning_place_roots");
        place_root_mono_items(cx, mono_items)
    };

    for cgu in &mut codegen_units {
        cgu.create_size_estimate(tcx);
    }

    debug_dump(tcx, "INITIAL PARTITIONING", &codegen_units);

    // Merge until we have at most `max_cgu_count` codegen units.
    // `merge_codegen_units` is responsible for updating the CGU size
    // estimates.
    {
        let _prof_timer = tcx.prof.generic_activity("cgu_partitioning_merge_cgus");
        merge_codegen_units(cx, &mut codegen_units);
        debug_dump(tcx, "POST MERGING", &codegen_units);
    }

    // In the next step, we use the inlining map to determine which additional
    // monomorphizations have to go into each codegen unit. These additional
    // monomorphizations can be drop-glue, functions from external crates, and
    // local functions the definition of which is marked with `#[inline]`.
    let mono_item_placements = {
        let _prof_timer = tcx.prof.generic_activity("cgu_partitioning_place_inline_items");
        place_inlined_mono_items(cx, &mut codegen_units, roots)
    };

    for cgu in &mut codegen_units {
        cgu.create_size_estimate(tcx);
    }

    debug_dump(tcx, "POST INLINING", &codegen_units);

    // Next we try to make as many symbols "internal" as possible, so LLVM has
    // more freedom to optimize.
    if !tcx.sess.link_dead_code() {
        let _prof_timer = tcx.prof.generic_activity("cgu_partitioning_internalize_symbols");
        internalize_symbols(
            cx,
            &mut codegen_units,
            mono_item_placements,
            internalization_candidates,
        );
    }

    let instrument_dead_code =
        tcx.sess.instrument_coverage() && !tcx.sess.instrument_coverage_except_unused_functions();

    if instrument_dead_code {
        assert!(
            codegen_units.len() > 0,
            "There must be at least one CGU that code coverage data can be generated in."
        );

        // Find the smallest CGU that has exported symbols and put the dead
        // function stubs in that CGU. We look for exported symbols to increase
        // the likelihood the linker won't throw away the dead functions.
        // FIXME(#92165): In order to truly resolve this, we need to make sure
        // the object file (CGU) containing the dead function stubs is included
        // in the final binary. This will probably require forcing these
        // function symbols to be included via `-u` or `/include` linker args.
        let mut cgus: Vec<_> = codegen_units.iter_mut().collect();
        cgus.sort_by_key(|cgu| cgu.size_estimate());

        let dead_code_cgu =
            if let Some(cgu) = cgus.into_iter().rev().find(|cgu| {
                cgu.items().iter().any(|(_, (linkage, _))| *linkage == Linkage::External)
            }) {
                cgu
            } else {
                // If there are no CGUs that have externally linked items,
                // then we just pick the first CGU as a fallback.
                &mut codegen_units[0]
            };
        dead_code_cgu.make_code_coverage_dead_code_cgu();
    }

    // Ensure CGUs are sorted by name, so that we get deterministic results.
    assert!(codegen_units.is_sorted_by(|a, b| Some(a.name().as_str().cmp(b.name().as_str()))));

    debug_dump(tcx, "FINAL", &codegen_units);

    codegen_units
}

fn place_root_mono_items<'tcx, I>(
    cx: &PartitioningCx<'_, 'tcx>,
    mono_items: &mut I,
) -> PlacedRootMonoItems<'tcx>
where
    I: Iterator<Item = MonoItem<'tcx>>,
{
    let mut roots = FxHashSet::default();
    let mut codegen_units = FxHashMap::default();
    let is_incremental_build = cx.tcx.sess.opts.incremental.is_some();
    let mut internalization_candidates = FxHashSet::default();

    // Determine if monomorphizations instantiated in this crate will be made
    // available to downstream crates. This depends on whether we are in
    // share-generics mode and whether the current crate can even have
    // downstream crates.
    let export_generics =
        cx.tcx.sess.opts.share_generics() && cx.tcx.local_crate_exports_generics();

    let cgu_name_builder = &mut CodegenUnitNameBuilder::new(cx.tcx);
    let cgu_name_cache = &mut FxHashMap::default();

    for mono_item in mono_items {
        match mono_item.instantiation_mode(cx.tcx) {
            InstantiationMode::GloballyShared { .. } => {}
            InstantiationMode::LocalCopy => continue,
        }

        let characteristic_def_id = characteristic_def_id_of_mono_item(cx.tcx, mono_item);
        let is_volatile = is_incremental_build && mono_item.is_generic_fn();

        let codegen_unit_name = match characteristic_def_id {
            Some(def_id) => compute_codegen_unit_name(
                cx.tcx,
                cgu_name_builder,
                def_id,
                is_volatile,
                cgu_name_cache,
            ),
            None => fallback_cgu_name(cgu_name_builder),
        };

        let codegen_unit = codegen_units
            .entry(codegen_unit_name)
            .or_insert_with(|| CodegenUnit::new(codegen_unit_name));

        let mut can_be_internalized = true;
        let (linkage, visibility) = mono_item_linkage_and_visibility(
            cx.tcx,
            &mono_item,
            &mut can_be_internalized,
            export_generics,
        );
        if visibility == Visibility::Hidden && can_be_internalized {
            internalization_candidates.insert(mono_item);
        }

        codegen_unit.items_mut().insert(mono_item, (linkage, visibility));
        roots.insert(mono_item);
    }

    // Always ensure we have at least one CGU; otherwise, if we have a
    // crate with just types (for example), we could wind up with no CGU.
    if codegen_units.is_empty() {
        let codegen_unit_name = fallback_cgu_name(cgu_name_builder);
        codegen_units.insert(codegen_unit_name, CodegenUnit::new(codegen_unit_name));
    }

    let mut codegen_units: Vec<_> = codegen_units.into_values().collect();
    codegen_units.sort_by(|a, b| a.name().as_str().cmp(b.name().as_str()));

    PlacedRootMonoItems { codegen_units, roots, internalization_candidates }
}

// This function requires the CGUs to be sorted by name on input, and ensures
// they are sorted by name on return, for deterministic behaviour.
fn merge_codegen_units<'tcx>(
    cx: &PartitioningCx<'_, 'tcx>,
    codegen_units: &mut Vec<CodegenUnit<'tcx>>,
) {
    assert!(cx.target_cgu_count >= 1);

    // A sorted order here ensures merging is deterministic.
    assert!(codegen_units.is_sorted_by(|a, b| Some(a.name().as_str().cmp(b.name().as_str()))));

    // This map keeps track of what got merged into what.
    let mut cgu_contents: FxHashMap<Symbol, Vec<Symbol>> =
        codegen_units.iter().map(|cgu| (cgu.name(), vec![cgu.name()])).collect();

    // Merge the two smallest codegen units until the target size is
    // reached.
    while codegen_units.len() > cx.target_cgu_count {
        // Sort small cgus to the back
        codegen_units.sort_by_cached_key(|cgu| cmp::Reverse(cgu.size_estimate()));
        let mut smallest = codegen_units.pop().unwrap();
        let second_smallest = codegen_units.last_mut().unwrap();

        // Move the mono-items from `smallest` to `second_smallest`
        second_smallest.modify_size_estimate(smallest.size_estimate());
        for (k, v) in smallest.items_mut().drain() {
            second_smallest.items_mut().insert(k, v);
        }

        // Record that `second_smallest` now contains all the stuff that was
        // in `smallest` before.
        let mut consumed_cgu_names = cgu_contents.remove(&smallest.name()).unwrap();
        cgu_contents.get_mut(&second_smallest.name()).unwrap().append(&mut consumed_cgu_names);

        debug!(
            "CodegenUnit {} merged into CodegenUnit {}",
            smallest.name(),
            second_smallest.name()
        );
    }

    let cgu_name_builder = &mut CodegenUnitNameBuilder::new(cx.tcx);

    if cx.tcx.sess.opts.incremental.is_some() {
        // If we are doing incremental compilation, we want CGU names to
        // reflect the path of the source level module they correspond to.
        // For CGUs that contain the code of multiple modules because of the
        // merging done above, we use a concatenation of the names of all
        // contained CGUs.
        let new_cgu_names: FxHashMap<Symbol, String> = cgu_contents
            .into_iter()
            // This `filter` makes sure we only update the name of CGUs that
            // were actually modified by merging.
            .filter(|(_, cgu_contents)| cgu_contents.len() > 1)
            .map(|(current_cgu_name, cgu_contents)| {
                let mut cgu_contents: Vec<&str> = cgu_contents.iter().map(|s| s.as_str()).collect();

                // Sort the names, so things are deterministic and easy to
                // predict. We are sorting primitive `&str`s here so we can
                // use unstable sort.
                cgu_contents.sort_unstable();

                (current_cgu_name, cgu_contents.join("--"))
            })
            .collect();

        for cgu in codegen_units.iter_mut() {
            if let Some(new_cgu_name) = new_cgu_names.get(&cgu.name()) {
                if cx.tcx.sess.opts.unstable_opts.human_readable_cgu_names {
                    cgu.set_name(Symbol::intern(&new_cgu_name));
                } else {
                    // If we don't require CGU names to be human-readable,
                    // we use a fixed length hash of the composite CGU name
                    // instead.
                    let new_cgu_name = CodegenUnit::mangle_name(&new_cgu_name);
                    cgu.set_name(Symbol::intern(&new_cgu_name));
                }
            }
        }
    } else {
        // If we are compiling non-incrementally we just generate simple CGU
        // names containing an index.
        for (index, cgu) in codegen_units.iter_mut().enumerate() {
            let numbered_codegen_unit_name =
                cgu_name_builder.build_cgu_name_no_mangle(LOCAL_CRATE, &["cgu"], Some(index));
            cgu.set_name(numbered_codegen_unit_name);
        }
    }

    // A sorted order here ensures what follows can be deterministic.
    codegen_units.sort_by(|a, b| a.name().as_str().cmp(b.name().as_str()));
}

/// For symbol internalization, we need to know whether a symbol/mono-item is
/// accessed from outside the codegen unit it is defined in. This type is used
/// to keep track of that.
#[derive(Clone, PartialEq, Eq, Debug)]
enum MonoItemPlacement {
    SingleCgu { cgu_name: Symbol },
    MultipleCgus,
}

fn place_inlined_mono_items<'tcx>(
    cx: &PartitioningCx<'_, 'tcx>,
    codegen_units: &mut [CodegenUnit<'tcx>],
    roots: FxHashSet<MonoItem<'tcx>>,
) -> FxHashMap<MonoItem<'tcx>, MonoItemPlacement> {
    let mut mono_item_placements = FxHashMap::default();

    let single_codegen_unit = codegen_units.len() == 1;

    for old_codegen_unit in codegen_units.iter_mut() {
        // Collect all items that need to be available in this codegen unit.
        let mut reachable = FxHashSet::default();
        for root in old_codegen_unit.items().keys() {
            follow_inlining(cx.tcx, *root, cx.inlining_map, &mut reachable);
        }

        let mut new_codegen_unit = CodegenUnit::new(old_codegen_unit.name());

        // Add all monomorphizations that are not already there.
        for mono_item in reachable {
            if let Some(linkage) = old_codegen_unit.items().get(&mono_item) {
                // This is a root, just copy it over.
                new_codegen_unit.items_mut().insert(mono_item, *linkage);
            } else {
                if roots.contains(&mono_item) {
                    bug!(
                        "GloballyShared mono-item inlined into other CGU: \
                          {:?}",
                        mono_item
                    );
                }

                // This is a CGU-private copy.
                new_codegen_unit
                    .items_mut()
                    .insert(mono_item, (Linkage::Internal, Visibility::Default));
            }

            if !single_codegen_unit {
                // If there is more than one codegen unit, we need to keep track
                // in which codegen units each monomorphization is placed.
                match mono_item_placements.entry(mono_item) {
                    Entry::Occupied(e) => {
                        let placement = e.into_mut();
                        debug_assert!(match *placement {
                            MonoItemPlacement::SingleCgu { cgu_name } => {
                                cgu_name != new_codegen_unit.name()
                            }
                            MonoItemPlacement::MultipleCgus => true,
                        });
                        *placement = MonoItemPlacement::MultipleCgus;
                    }
                    Entry::Vacant(e) => {
                        e.insert(MonoItemPlacement::SingleCgu {
                            cgu_name: new_codegen_unit.name(),
                        });
                    }
                }
            }
        }

        *old_codegen_unit = new_codegen_unit;
    }

    return mono_item_placements;

    fn follow_inlining<'tcx>(
        tcx: TyCtxt<'tcx>,
        mono_item: MonoItem<'tcx>,
        inlining_map: &InliningMap<'tcx>,
        visited: &mut FxHashSet<MonoItem<'tcx>>,
    ) {
        if !visited.insert(mono_item) {
            return;
        }

        inlining_map.with_inlining_candidates(tcx, mono_item, |target| {
            follow_inlining(tcx, target, inlining_map, visited);
        });
    }
}

fn internalize_symbols<'tcx>(
    cx: &PartitioningCx<'_, 'tcx>,
    codegen_units: &mut [CodegenUnit<'tcx>],
    mono_item_placements: FxHashMap<MonoItem<'tcx>, MonoItemPlacement>,
    internalization_candidates: FxHashSet<MonoItem<'tcx>>,
) {
    if codegen_units.len() == 1 {
        // Fast path for when there is only one codegen unit. In this case we
        // can internalize all candidates, since there is nowhere else they
        // could be accessed from.
        for cgu in codegen_units {
            for candidate in &internalization_candidates {
                cgu.items_mut().insert(*candidate, (Linkage::Internal, Visibility::Default));
            }
        }

        return;
    }

    // Build a map from every monomorphization to all the monomorphizations that
    // reference it.
    let mut accessor_map: FxHashMap<MonoItem<'tcx>, Vec<MonoItem<'tcx>>> = Default::default();
    cx.inlining_map.iter_accesses(|accessor, accessees| {
        for accessee in accessees {
            accessor_map.entry(*accessee).or_default().push(accessor);
        }
    });

    // For each internalization candidates in each codegen unit, check if it is
    // accessed from outside its defining codegen unit.
    for cgu in codegen_units {
        let home_cgu = MonoItemPlacement::SingleCgu { cgu_name: cgu.name() };

        for (accessee, linkage_and_visibility) in cgu.items_mut() {
            if !internalization_candidates.contains(accessee) {
                // This item is no candidate for internalizing, so skip it.
                continue;
            }
            debug_assert_eq!(mono_item_placements[accessee], home_cgu);

            if let Some(accessors) = accessor_map.get(accessee) {
                if accessors
                    .iter()
                    .filter_map(|accessor| {
                        // Some accessors might not have been
                        // instantiated. We can safely ignore those.
                        mono_item_placements.get(accessor)
                    })
                    .any(|placement| *placement != home_cgu)
                {
                    // Found an accessor from another CGU, so skip to the next
                    // item without marking this one as internal.
                    continue;
                }
            }

            // If we got here, we did not find any accesses from other CGUs,
            // so it's fine to make this monomorphization internal.
            *linkage_and_visibility = (Linkage::Internal, Visibility::Default);
        }
    }
}

fn characteristic_def_id_of_mono_item<'tcx>(
    tcx: TyCtxt<'tcx>,
    mono_item: MonoItem<'tcx>,
) -> Option<DefId> {
    match mono_item {
        MonoItem::Fn(instance) => {
            let def_id = match instance.def {
                ty::InstanceDef::Item(def) => def,
                ty::InstanceDef::VTableShim(..)
                | ty::InstanceDef::ReifyShim(..)
                | ty::InstanceDef::FnPtrShim(..)
                | ty::InstanceDef::ClosureOnceShim { .. }
                | ty::InstanceDef::Intrinsic(..)
                | ty::InstanceDef::DropGlue(..)
                | ty::InstanceDef::Virtual(..)
                | ty::InstanceDef::CloneShim(..)
                | ty::InstanceDef::ThreadLocalShim(..)
                | ty::InstanceDef::FnPtrAddrShim(..) => return None,
            };

            // If this is a method, we want to put it into the same module as
            // its self-type. If the self-type does not provide a characteristic
            // DefId, we use the location of the impl after all.

            if tcx.trait_of_item(def_id).is_some() {
                let self_ty = instance.substs.type_at(0);
                // This is a default implementation of a trait method.
                return characteristic_def_id_of_type(self_ty).or(Some(def_id));
            }

            if let Some(impl_def_id) = tcx.impl_of_method(def_id) {
                if tcx.sess.opts.incremental.is_some()
                    && tcx.trait_id_of_impl(impl_def_id) == tcx.lang_items().drop_trait()
                {
                    // Put `Drop::drop` into the same cgu as `drop_in_place`
                    // since `drop_in_place` is the only thing that can
                    // call it.
                    return None;
                }

                // When polymorphization is enabled, methods which do not depend on their generic
                // parameters, but the self-type of their impl block do will fail to normalize.
                if !tcx.sess.opts.unstable_opts.polymorphize || !instance.has_param() {
                    // This is a method within an impl, find out what the self-type is:
                    let impl_self_ty = tcx.subst_and_normalize_erasing_regions(
                        instance.substs,
                        ty::ParamEnv::reveal_all(),
                        tcx.type_of(impl_def_id),
                    );
                    if let Some(def_id) = characteristic_def_id_of_type(impl_self_ty) {
                        return Some(def_id);
                    }
                }
            }

            Some(def_id)
        }
        MonoItem::Static(def_id) => Some(def_id),
        MonoItem::GlobalAsm(item_id) => Some(item_id.owner_id.to_def_id()),
    }
}

fn compute_codegen_unit_name(
    tcx: TyCtxt<'_>,
    name_builder: &mut CodegenUnitNameBuilder<'_>,
    def_id: DefId,
    volatile: bool,
    cache: &mut CguNameCache,
) -> Symbol {
    // Find the innermost module that is not nested within a function.
    let mut current_def_id = def_id;
    let mut cgu_def_id = None;
    // Walk backwards from the item we want to find the module for.
    loop {
        if current_def_id.is_crate_root() {
            if cgu_def_id.is_none() {
                // If we have not found a module yet, take the crate root.
                cgu_def_id = Some(def_id.krate.as_def_id());
            }
            break;
        } else if tcx.def_kind(current_def_id) == DefKind::Mod {
            if cgu_def_id.is_none() {
                cgu_def_id = Some(current_def_id);
            }
        } else {
            // If we encounter something that is not a module, throw away
            // any module that we've found so far because we now know that
            // it is nested within something else.
            cgu_def_id = None;
        }

        current_def_id = tcx.parent(current_def_id);
    }

    let cgu_def_id = cgu_def_id.unwrap();

    *cache.entry((cgu_def_id, volatile)).or_insert_with(|| {
        let def_path = tcx.def_path(cgu_def_id);

        let components = def_path.data.iter().map(|part| match part.data.name() {
            DefPathDataName::Named(name) => name,
            DefPathDataName::Anon { .. } => unreachable!(),
        });

        let volatile_suffix = volatile.then_some("volatile");

        name_builder.build_cgu_name(def_path.krate, components, volatile_suffix)
    })
}

// Anything we can't find a proper codegen unit for goes into this.
fn fallback_cgu_name(name_builder: &mut CodegenUnitNameBuilder<'_>) -> Symbol {
    name_builder.build_cgu_name(LOCAL_CRATE, &["fallback"], Some("cgu"))
}

fn mono_item_linkage_and_visibility<'tcx>(
    tcx: TyCtxt<'tcx>,
    mono_item: &MonoItem<'tcx>,
    can_be_internalized: &mut bool,
    export_generics: bool,
) -> (Linkage, Visibility) {
    if let Some(explicit_linkage) = mono_item.explicit_linkage(tcx) {
        return (explicit_linkage, Visibility::Default);
    }
    let vis = mono_item_visibility(tcx, mono_item, can_be_internalized, export_generics);
    (Linkage::External, vis)
}

type CguNameCache = FxHashMap<(DefId, bool), Symbol>;

fn static_visibility<'tcx>(
    tcx: TyCtxt<'tcx>,
    can_be_internalized: &mut bool,
    def_id: DefId,
) -> Visibility {
    if tcx.is_reachable_non_generic(def_id) {
        *can_be_internalized = false;
        default_visibility(tcx, def_id, false)
    } else {
        Visibility::Hidden
    }
}

fn mono_item_visibility<'tcx>(
    tcx: TyCtxt<'tcx>,
    mono_item: &MonoItem<'tcx>,
    can_be_internalized: &mut bool,
    export_generics: bool,
) -> Visibility {
    let instance = match mono_item {
        // This is pretty complicated; see below.
        MonoItem::Fn(instance) => instance,

        // Misc handling for generics and such, but otherwise:
        MonoItem::Static(def_id) => return static_visibility(tcx, can_be_internalized, *def_id),
        MonoItem::GlobalAsm(item_id) => {
            return static_visibility(tcx, can_be_internalized, item_id.owner_id.to_def_id());
        }
    };

    let def_id = match instance.def {
        InstanceDef::Item(def_id) | InstanceDef::DropGlue(def_id, Some(_)) => def_id,

        // We match the visibility of statics here
        InstanceDef::ThreadLocalShim(def_id) => {
            return static_visibility(tcx, can_be_internalized, def_id);
        }

        // These are all compiler glue and such, never exported, always hidden.
        InstanceDef::VTableShim(..)
        | InstanceDef::ReifyShim(..)
        | InstanceDef::FnPtrShim(..)
        | InstanceDef::Virtual(..)
        | InstanceDef::Intrinsic(..)
        | InstanceDef::ClosureOnceShim { .. }
        | InstanceDef::DropGlue(..)
        | InstanceDef::CloneShim(..)
        | InstanceDef::FnPtrAddrShim(..) => return Visibility::Hidden,
    };

    // The `start_fn` lang item is actually a monomorphized instance of a
    // function in the standard library, used for the `main` function. We don't
    // want to export it so we tag it with `Hidden` visibility but this symbol
    // is only referenced from the actual `main` symbol which we unfortunately
    // don't know anything about during partitioning/collection. As a result we
    // forcibly keep this symbol out of the `internalization_candidates` set.
    //
    // FIXME: eventually we don't want to always force this symbol to have
    //        hidden visibility, it should indeed be a candidate for
    //        internalization, but we have to understand that it's referenced
    //        from the `main` symbol we'll generate later.
    //
    //        This may be fixable with a new `InstanceDef` perhaps? Unsure!
    if tcx.lang_items().start_fn() == Some(def_id) {
        *can_be_internalized = false;
        return Visibility::Hidden;
    }

    let is_generic = instance.substs.non_erasable_generics().next().is_some();

    // Upstream `DefId` instances get different handling than local ones.
    let Some(def_id) = def_id.as_local() else {
        return if export_generics && is_generic {
            // If it is an upstream monomorphization and we export generics, we must make
            // it available to downstream crates.
            *can_be_internalized = false;
            default_visibility(tcx, def_id, true)
        } else {
            Visibility::Hidden
        };
    };

    if is_generic {
        if export_generics {
            if tcx.is_unreachable_local_definition(def_id) {
                // This instance cannot be used from another crate.
                Visibility::Hidden
            } else {
                // This instance might be useful in a downstream crate.
                *can_be_internalized = false;
                default_visibility(tcx, def_id.to_def_id(), true)
            }
        } else {
            // We are not exporting generics or the definition is not reachable
            // for downstream crates, we can internalize its instantiations.
            Visibility::Hidden
        }
    } else {
        // If this isn't a generic function then we mark this a `Default` if
        // this is a reachable item, meaning that it's a symbol other crates may
        // access when they link to us.
        if tcx.is_reachable_non_generic(def_id.to_def_id()) {
            *can_be_internalized = false;
            debug_assert!(!is_generic);
            return default_visibility(tcx, def_id.to_def_id(), false);
        }

        // If this isn't reachable then we're gonna tag this with `Hidden`
        // visibility. In some situations though we'll want to prevent this
        // symbol from being internalized.
        //
        // There's two categories of items here:
        //
        // * First is weak lang items. These are basically mechanisms for
        //   libcore to forward-reference symbols defined later in crates like
        //   the standard library or `#[panic_handler]` definitions. The
        //   definition of these weak lang items needs to be referencable by
        //   libcore, so we're no longer a candidate for internalization.
        //   Removal of these functions can't be done by LLVM but rather must be
        //   done by the linker as it's a non-local decision.
        //
        // * Second is "std internal symbols". Currently this is primarily used
        //   for allocator symbols. Allocators are a little weird in their
        //   implementation, but the idea is that the compiler, at the last
        //   minute, defines an allocator with an injected object file. The
        //   `alloc` crate references these symbols (`__rust_alloc`) and the
        //   definition doesn't get hooked up until a linked crate artifact is
        //   generated.
        //
        //   The symbols synthesized by the compiler (`__rust_alloc`) are thin
        //   veneers around the actual implementation, some other symbol which
        //   implements the same ABI. These symbols (things like `__rg_alloc`,
        //   `__rdl_alloc`, `__rde_alloc`, etc), are all tagged with "std
        //   internal symbols".
        //
        //   The std-internal symbols here **should not show up in a dll as an
        //   exported interface**, so they return `false` from
        //   `is_reachable_non_generic` above and we'll give them `Hidden`
        //   visibility below. Like the weak lang items, though, we can't let
        //   LLVM internalize them as this decision is left up to the linker to
        //   omit them, so prevent them from being internalized.
        let attrs = tcx.codegen_fn_attrs(def_id);
        if attrs.flags.contains(CodegenFnAttrFlags::RUSTC_STD_INTERNAL_SYMBOL) {
            *can_be_internalized = false;
        }

        Visibility::Hidden
    }
}

fn default_visibility(tcx: TyCtxt<'_>, id: DefId, is_generic: bool) -> Visibility {
    if !tcx.sess.target.default_hidden_visibility {
        return Visibility::Default;
    }

    // Generic functions never have export-level C.
    if is_generic {
        return Visibility::Hidden;
    }

    // Things with export level C don't get instantiated in
    // downstream crates.
    if !id.is_local() {
        return Visibility::Hidden;
    }

    // C-export level items remain at `Default`, all other internal
    // items become `Hidden`.
    match tcx.reachable_non_generics(id.krate).get(&id) {
        Some(SymbolExportInfo { level: SymbolExportLevel::C, .. }) => Visibility::Default,
        _ => Visibility::Hidden,
    }
}

fn debug_dump<'a, 'tcx: 'a>(tcx: TyCtxt<'tcx>, label: &str, cgus: &[CodegenUnit<'tcx>]) {
    let dump = move || {
        use std::fmt::Write;

        let num_cgus = cgus.len();
        let num_items: usize = cgus.iter().map(|cgu| cgu.items().len()).sum();
        let total_size: usize = cgus.iter().map(|cgu| cgu.size_estimate()).sum();
        let max_size = cgus.iter().map(|cgu| cgu.size_estimate()).max().unwrap();
        let min_size = cgus.iter().map(|cgu| cgu.size_estimate()).min().unwrap();
        let max_min_size_ratio = max_size as f64 / min_size as f64;

        let s = &mut String::new();
        let _ = writeln!(
            s,
            "{label} ({num_items} items, total_size={total_size}; {num_cgus} CGUs, \
             max_size={max_size}, min_size={min_size}, max_size/min_size={max_min_size_ratio:.1}):"
        );
        for (i, cgu) in cgus.iter().enumerate() {
            let num_items = cgu.items().len();
            let _ = writeln!(
                s,
                "- CGU[{i}] {} ({num_items} items, size={}):",
                cgu.name(),
                cgu.size_estimate()
            );

            // The order of `cgu.items()` is non-deterministic; sort it by name
            // to give deterministic output.
            let mut items: Vec<_> = cgu.items().iter().collect();
            items.sort_by_key(|(item, _)| item.symbol_name(tcx).name);
            for (item, linkage) in items {
                let symbol_name = item.symbol_name(tcx).name;
                let symbol_hash_start = symbol_name.rfind('h');
                let symbol_hash = symbol_hash_start.map_or("<no hash>", |i| &symbol_name[i..]);

                let size = item.size_estimate(tcx);
                let _ = with_no_trimmed_paths!(writeln!(
                    s,
                    "  - {item} [{linkage:?}] [{symbol_hash}] (size={size})"
                ));
            }

            let _ = writeln!(s);
        }

        std::mem::take(s)
    };

    debug!("{}", dump());
}

#[inline(never)] // give this a place in the profiler
fn assert_symbols_are_distinct<'a, 'tcx, I>(tcx: TyCtxt<'tcx>, mono_items: I)
where
    I: Iterator<Item = &'a MonoItem<'tcx>>,
    'tcx: 'a,
{
    let _prof_timer = tcx.prof.generic_activity("assert_symbols_are_distinct");

    let mut symbols: Vec<_> =
        mono_items.map(|mono_item| (mono_item, mono_item.symbol_name(tcx))).collect();

    symbols.sort_by_key(|sym| sym.1);

    for &[(mono_item1, ref sym1), (mono_item2, ref sym2)] in symbols.array_windows() {
        if sym1 == sym2 {
            let span1 = mono_item1.local_span(tcx);
            let span2 = mono_item2.local_span(tcx);

            // Deterministically select one of the spans for error reporting
            let span = match (span1, span2) {
                (Some(span1), Some(span2)) => {
                    Some(if span1.lo().0 > span2.lo().0 { span1 } else { span2 })
                }
                (span1, span2) => span1.or(span2),
            };

            tcx.sess.emit_fatal(SymbolAlreadyDefined { span, symbol: sym1.to_string() });
        }
    }
}

fn collect_and_partition_mono_items(tcx: TyCtxt<'_>, (): ()) -> (&DefIdSet, &[CodegenUnit<'_>]) {
    let collection_mode = match tcx.sess.opts.unstable_opts.print_mono_items {
        Some(ref s) => {
            let mode = s.to_lowercase();
            let mode = mode.trim();
            if mode == "eager" {
                MonoItemCollectionMode::Eager
            } else {
                if mode != "lazy" {
                    tcx.sess.emit_warning(UnknownCguCollectionMode { mode });
                }

                MonoItemCollectionMode::Lazy
            }
        }
        None => {
            if tcx.sess.link_dead_code() {
                MonoItemCollectionMode::Eager
            } else {
                MonoItemCollectionMode::Lazy
            }
        }
    };

    let (items, inlining_map) = collector::collect_crate_mono_items(tcx, collection_mode);

    tcx.sess.abort_if_errors();

    let (codegen_units, _) = tcx.sess.time("partition_and_assert_distinct_symbols", || {
        sync::join(
            || {
                let mut codegen_units = partition(
                    tcx,
                    &mut items.iter().copied(),
                    tcx.sess.codegen_units(),
                    &inlining_map,
                );
                codegen_units[0].make_primary();
                &*tcx.arena.alloc_from_iter(codegen_units)
            },
            || assert_symbols_are_distinct(tcx, items.iter()),
        )
    });

    if tcx.prof.enabled() {
        // Record CGU size estimates for self-profiling.
        for cgu in codegen_units {
            tcx.prof.artifact_size(
                "codegen_unit_size_estimate",
                cgu.name().as_str(),
                cgu.size_estimate() as u64,
            );
        }
    }

    let mono_items: DefIdSet = items
        .iter()
        .filter_map(|mono_item| match *mono_item {
            MonoItem::Fn(ref instance) => Some(instance.def_id()),
            MonoItem::Static(def_id) => Some(def_id),
            _ => None,
        })
        .collect();

    // Output monomorphization stats per def_id
    if let SwitchWithOptPath::Enabled(ref path) = tcx.sess.opts.unstable_opts.dump_mono_stats {
        if let Err(err) =
            dump_mono_items_stats(tcx, &codegen_units, path, tcx.crate_name(LOCAL_CRATE))
        {
            tcx.sess.emit_fatal(CouldntDumpMonoStats { error: err.to_string() });
        }
    }

    if tcx.sess.opts.unstable_opts.print_mono_items.is_some() {
        let mut item_to_cgus: FxHashMap<_, Vec<_>> = Default::default();

        for cgu in codegen_units {
            for (&mono_item, &linkage) in cgu.items() {
                item_to_cgus.entry(mono_item).or_default().push((cgu.name(), linkage));
            }
        }

        let mut item_keys: Vec<_> = items
            .iter()
            .map(|i| {
                let mut output = with_no_trimmed_paths!(i.to_string());
                output.push_str(" @@");
                let mut empty = Vec::new();
                let cgus = item_to_cgus.get_mut(i).unwrap_or(&mut empty);
                cgus.sort_by_key(|(name, _)| *name);
                cgus.dedup();
                for &(ref cgu_name, (linkage, _)) in cgus.iter() {
                    output.push(' ');
                    output.push_str(cgu_name.as_str());

                    let linkage_abbrev = match linkage {
                        Linkage::External => "External",
                        Linkage::AvailableExternally => "Available",
                        Linkage::LinkOnceAny => "OnceAny",
                        Linkage::LinkOnceODR => "OnceODR",
                        Linkage::WeakAny => "WeakAny",
                        Linkage::WeakODR => "WeakODR",
                        Linkage::Appending => "Appending",
                        Linkage::Internal => "Internal",
                        Linkage::Private => "Private",
                        Linkage::ExternalWeak => "ExternalWeak",
                        Linkage::Common => "Common",
                    };

                    output.push('[');
                    output.push_str(linkage_abbrev);
                    output.push(']');
                }
                output
            })
            .collect();

        item_keys.sort();

        for item in item_keys {
            println!("MONO_ITEM {item}");
        }
    }

    (tcx.arena.alloc(mono_items), codegen_units)
}

/// Outputs stats about instantiation counts and estimated size, per `MonoItem`'s
/// def, to a file in the given output directory.
fn dump_mono_items_stats<'tcx>(
    tcx: TyCtxt<'tcx>,
    codegen_units: &[CodegenUnit<'tcx>],
    output_directory: &Option<PathBuf>,
    crate_name: Symbol,
) -> Result<(), Box<dyn std::error::Error>> {
    let output_directory = if let Some(ref directory) = output_directory {
        fs::create_dir_all(directory)?;
        directory
    } else {
        Path::new(".")
    };

    let format = tcx.sess.opts.unstable_opts.dump_mono_stats_format;
    let ext = format.extension();
    let filename = format!("{crate_name}.mono_items.{ext}");
    let output_path = output_directory.join(&filename);
    let file = File::create(&output_path)?;
    let mut file = BufWriter::new(file);

    // Gather instantiated mono items grouped by def_id
    let mut items_per_def_id: FxHashMap<_, Vec<_>> = Default::default();
    for cgu in codegen_units {
        for (&mono_item, _) in cgu.items() {
            // Avoid variable-sized compiler-generated shims
            if mono_item.is_user_defined() {
                items_per_def_id.entry(mono_item.def_id()).or_default().push(mono_item);
            }
        }
    }

    #[derive(serde::Serialize)]
    struct MonoItem {
        name: String,
        instantiation_count: usize,
        size_estimate: usize,
        total_estimate: usize,
    }

    // Output stats sorted by total instantiated size, from heaviest to lightest
    let mut stats: Vec<_> = items_per_def_id
        .into_iter()
        .map(|(def_id, items)| {
            let name = with_no_trimmed_paths!(tcx.def_path_str(def_id));
            let instantiation_count = items.len();
            let size_estimate = items[0].size_estimate(tcx);
            let total_estimate = instantiation_count * size_estimate;
            MonoItem { name, instantiation_count, size_estimate, total_estimate }
        })
        .collect();
    stats.sort_unstable_by_key(|item| cmp::Reverse(item.total_estimate));

    if !stats.is_empty() {
        match format {
            DumpMonoStatsFormat::Json => serde_json::to_writer(file, &stats)?,
            DumpMonoStatsFormat::Markdown => {
                writeln!(
                    file,
                    "| Item | Instantiation count | Estimated Cost Per Instantiation | Total Estimated Cost |"
                )?;
                writeln!(file, "| --- | ---: | ---: | ---: |")?;

                for MonoItem { name, instantiation_count, size_estimate, total_estimate } in stats {
                    writeln!(
                        file,
                        "| `{name}` | {instantiation_count} | {size_estimate} | {total_estimate} |"
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn codegened_and_inlined_items(tcx: TyCtxt<'_>, (): ()) -> &DefIdSet {
    let (items, cgus) = tcx.collect_and_partition_mono_items(());
    let mut visited = DefIdSet::default();
    let mut result = items.clone();

    for cgu in cgus {
        for (item, _) in cgu.items() {
            if let MonoItem::Fn(ref instance) = item {
                let did = instance.def_id();
                if !visited.insert(did) {
                    continue;
                }
                let body = tcx.instance_mir(instance.def);
                for block in body.basic_blocks.iter() {
                    for statement in &block.statements {
                        let mir::StatementKind::Coverage(_) = statement.kind else { continue };
                        let scope = statement.source_info.scope;
                        if let Some(inlined) = scope.inlined_instance(&body.source_scopes) {
                            result.insert(inlined.def_id());
                        }
                    }
                }
            }
        }
    }

    tcx.arena.alloc(result)
}

pub fn provide(providers: &mut Providers) {
    providers.collect_and_partition_mono_items = collect_and_partition_mono_items;
    providers.codegened_and_inlined_items = codegened_and_inlined_items;

    providers.is_codegened_item = |tcx, def_id| {
        let (all_mono_items, _) = tcx.collect_and_partition_mono_items(());
        all_mono_items.contains(&def_id)
    };

    providers.codegen_unit = |tcx, name| {
        let (_, all) = tcx.collect_and_partition_mono_items(());
        all.iter()
            .find(|cgu| cgu.name() == name)
            .unwrap_or_else(|| panic!("failed to find cgu with name {name:?}"))
    };
}
