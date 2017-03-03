// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt::Debug;
use std::sync::Arc;

macro_rules! try_opt {
    ($e:expr) => (
        match $e {
            Some(r) => r,
            None => return None,
        }
    )
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, RustcEncodable, RustcDecodable)]
pub enum DepNode<D: Clone + Debug> {
    // The `D` type is "how definitions are identified".
    // During compilation, it is always `DefId`, but when serializing
    // it is mapped to `DefPath`.

    // Represents the `Krate` as a whole (the `hir::Krate` value) (as
    // distinct from the krate module). This is basically a hash of
    // the entire krate, so if you read from `Krate` (e.g., by calling
    // `tcx.hir.krate()`), we will have to assume that any change
    // means that you need to be recompiled. This is because the
    // `Krate` value gives you access to all other items. To avoid
    // this fate, do not call `tcx.hir.krate()`; instead, prefer
    // wrappers like `tcx.visit_all_items_in_krate()`.  If there is no
    // suitable wrapper, you can use `tcx.dep_graph.ignore()` to gain
    // access to the krate, but you must remember to add suitable
    // edges yourself for the individual items that you read.
    Krate,

    // Represents the HIR node with the given node-id
    Hir(D),

    // Represents the body of a function or method. The def-id is that of the
    // function/method.
    HirBody(D),

    // Represents the metadata for a given HIR node, typically found
    // in an extern crate.
    MetaData(D),

    // Represents some artifact that we save to disk. Note that these
    // do not have a def-id as part of their identifier.
    WorkProduct(Arc<WorkProductId>),

    // Represents different phases in the compiler.
    CollectLanguageItems,
    CheckStaticRecursion,
    ResolveLifetimes,
    RegionResolveCrate,
    CheckLoops,
    PluginRegistrar,
    StabilityIndex,
    CollectItem(D),
    CollectItemSig(D),
    Coherence,
    EffectCheck,
    Liveness,
    Resolve,
    EntryPoint,
    CheckEntryFn,
    CoherenceCheckTrait(D),
    CoherenceCheckImpl(D),
    CoherenceOverlapCheck(D),
    CoherenceOverlapCheckSpecial(D),
    CoherenceOverlapInherentCheck(D),
    CoherenceOrphanCheck(D),
    Variance,
    WfCheck(D),
    TypeckItemType(D),
    Dropck,
    DropckImpl(D),
    UnusedTraitCheck,
    CheckConst(D),
    Privacy,
    IntrinsicCheck(D),
    MatchCheck(D),

    // Represents the MIR for a fn; also used as the task node for
    // things read/modify that MIR.
    MirKrate,
    Mir(D),

    BorrowCheckKrate,
    BorrowCheck(D),
    RvalueCheck(D),
    Reachability,
    DeadCheck,
    StabilityCheck(D),
    LateLintCheck,
    TransCrate,
    TransCrateItem(D),
    TransInlinedItem(D),
    TransWriteMetadata,
    LinkBinary,

    // Nodes representing bits of computed IR in the tcx. Each shared
    // table in the tcx (or elsewhere) maps to one of these
    // nodes. Often we map multiple tables to the same node if there
    // is no point in distinguishing them (e.g., both the type and
    // predicates for an item wind up in `ItemSignature`).
    AssociatedItems(D),
    ItemSignature(D),
    TypeParamPredicates((D, D)),
    SizedConstraint(D),
    AssociatedItemDefIds(D),
    InherentImpls(D),
    TypeckBodiesKrate,
    TypeckTables(D),
    UsedTraitImports(D),
    MonomorphicConstEval(D),

    // The set of impls for a given trait. Ultimately, it would be
    // nice to get more fine-grained here (e.g., to include a
    // simplified type), but we can't do that until we restructure the
    // HIR to distinguish the *header* of an impl from its body.  This
    // is because changes to the header may change the self-type of
    // the impl and hence would require us to be more conservative
    // than changes in the impl body.
    TraitImpls(D),

    // Nodes representing caches. To properly handle a true cache, we
    // don't use a DepTrackingMap, but rather we push a task node.
    // Otherwise the write into the map would be incorrectly
    // attributed to the first task that happened to fill the cache,
    // which would yield an overly conservative dep-graph.
    TraitItems(D),
    ReprHints(D),

    // Trait selection cache is a little funny. Given a trait
    // reference like `Foo: SomeTrait<Bar>`, there could be
    // arbitrarily many def-ids to map on in there (e.g., `Foo`,
    // `SomeTrait`, `Bar`). We could have a vector of them, but it
    // requires heap-allocation, and trait sel in general can be a
    // surprisingly hot path. So instead we pick two def-ids: the
    // trait def-id, and the first def-id in the input types. If there
    // is no def-id in the input types, then we use the trait def-id
    // again. So for example:
    //
    // - `i32: Clone` -> `TraitSelect { trait_def_id: Clone, self_def_id: Clone }`
    // - `u32: Clone` -> `TraitSelect { trait_def_id: Clone, self_def_id: Clone }`
    // - `Clone: Clone` -> `TraitSelect { trait_def_id: Clone, self_def_id: Clone }`
    // - `Vec<i32>: Clone` -> `TraitSelect { trait_def_id: Clone, self_def_id: Vec }`
    // - `String: Clone` -> `TraitSelect { trait_def_id: Clone, self_def_id: String }`
    // - `Foo: Trait<Bar>` -> `TraitSelect { trait_def_id: Trait, self_def_id: Foo }`
    // - `Foo: Trait<i32>` -> `TraitSelect { trait_def_id: Trait, self_def_id: Foo }`
    // - `(Foo, Bar): Trait` -> `TraitSelect { trait_def_id: Trait, self_def_id: Foo }`
    // - `i32: Trait<Foo>` -> `TraitSelect { trait_def_id: Trait, self_def_id: Foo }`
    //
    // You can see that we map many trait refs to the same
    // trait-select node.  This is not a problem, it just means
    // imprecision in our dep-graph tracking.  The important thing is
    // that for any given trait-ref, we always map to the **same**
    // trait-select node.
    TraitSelect { trait_def_id: D, input_def_id: D },

    // For proj. cache, we just keep a list of all def-ids, since it is
    // not a hotspot.
    ProjectionCache { def_ids: Vec<D> },
}

impl<D: Clone + Debug> DepNode<D> {
    /// Used in testing
    pub fn from_label_string(label: &str, data: D) -> Result<DepNode<D>, ()> {
        macro_rules! check {
            ($($name:ident,)*) => {
                match label {
                    $(stringify!($name) => Ok(DepNode::$name(data)),)*
                    _ => Err(())
                }
            }
        }

        if label == "Krate" {
            // special case
            return Ok(DepNode::Krate);
        }

        check! {
            CollectItem,
            BorrowCheck,
            Hir,
            HirBody,
            TransCrateItem,
            TypeckItemType,
            AssociatedItems,
            ItemSignature,
            AssociatedItemDefIds,
            InherentImpls,
            TypeckTables,
            UsedTraitImports,
            TraitImpls,
            ReprHints,
        }
    }

    pub fn map_def<E, OP>(&self, mut op: OP) -> Option<DepNode<E>>
        where OP: FnMut(&D) -> Option<E>, E: Clone + Debug
    {
        use self::DepNode::*;

        match *self {
            Krate => Some(Krate),
            BorrowCheckKrate => Some(BorrowCheckKrate),
            MirKrate => Some(MirKrate),
            TypeckBodiesKrate => Some(TypeckBodiesKrate),
            CollectLanguageItems => Some(CollectLanguageItems),
            CheckStaticRecursion => Some(CheckStaticRecursion),
            ResolveLifetimes => Some(ResolveLifetimes),
            RegionResolveCrate => Some(RegionResolveCrate),
            CheckLoops => Some(CheckLoops),
            PluginRegistrar => Some(PluginRegistrar),
            StabilityIndex => Some(StabilityIndex),
            Coherence => Some(Coherence),
            EffectCheck => Some(EffectCheck),
            Liveness => Some(Liveness),
            Resolve => Some(Resolve),
            EntryPoint => Some(EntryPoint),
            CheckEntryFn => Some(CheckEntryFn),
            Variance => Some(Variance),
            Dropck => Some(Dropck),
            UnusedTraitCheck => Some(UnusedTraitCheck),
            Privacy => Some(Privacy),
            Reachability => Some(Reachability),
            DeadCheck => Some(DeadCheck),
            LateLintCheck => Some(LateLintCheck),
            TransCrate => Some(TransCrate),
            TransWriteMetadata => Some(TransWriteMetadata),
            LinkBinary => Some(LinkBinary),

            // work product names do not need to be mapped, because
            // they are always absolute.
            WorkProduct(ref id) => Some(WorkProduct(id.clone())),

            Hir(ref d) => op(d).map(Hir),
            HirBody(ref d) => op(d).map(HirBody),
            MetaData(ref d) => op(d).map(MetaData),
            CollectItem(ref d) => op(d).map(CollectItem),
            CollectItemSig(ref d) => op(d).map(CollectItemSig),
            CoherenceCheckTrait(ref d) => op(d).map(CoherenceCheckTrait),
            CoherenceCheckImpl(ref d) => op(d).map(CoherenceCheckImpl),
            CoherenceOverlapCheck(ref d) => op(d).map(CoherenceOverlapCheck),
            CoherenceOverlapCheckSpecial(ref d) => op(d).map(CoherenceOverlapCheckSpecial),
            CoherenceOverlapInherentCheck(ref d) => op(d).map(CoherenceOverlapInherentCheck),
            CoherenceOrphanCheck(ref d) => op(d).map(CoherenceOrphanCheck),
            WfCheck(ref d) => op(d).map(WfCheck),
            TypeckItemType(ref d) => op(d).map(TypeckItemType),
            DropckImpl(ref d) => op(d).map(DropckImpl),
            CheckConst(ref d) => op(d).map(CheckConst),
            IntrinsicCheck(ref d) => op(d).map(IntrinsicCheck),
            MatchCheck(ref d) => op(d).map(MatchCheck),
            Mir(ref d) => op(d).map(Mir),
            BorrowCheck(ref d) => op(d).map(BorrowCheck),
            RvalueCheck(ref d) => op(d).map(RvalueCheck),
            StabilityCheck(ref d) => op(d).map(StabilityCheck),
            TransCrateItem(ref d) => op(d).map(TransCrateItem),
            TransInlinedItem(ref d) => op(d).map(TransInlinedItem),
            AssociatedItems(ref d) => op(d).map(AssociatedItems),
            ItemSignature(ref d) => op(d).map(ItemSignature),
            TypeParamPredicates((ref item, ref param)) => {
                Some(TypeParamPredicates((try_opt!(op(item)), try_opt!(op(param)))))
            }
            SizedConstraint(ref d) => op(d).map(SizedConstraint),
            AssociatedItemDefIds(ref d) => op(d).map(AssociatedItemDefIds),
            InherentImpls(ref d) => op(d).map(InherentImpls),
            TypeckTables(ref d) => op(d).map(TypeckTables),
            UsedTraitImports(ref d) => op(d).map(UsedTraitImports),
            MonomorphicConstEval(ref d) => op(d).map(MonomorphicConstEval),
            TraitImpls(ref d) => op(d).map(TraitImpls),
            TraitItems(ref d) => op(d).map(TraitItems),
            ReprHints(ref d) => op(d).map(ReprHints),
            TraitSelect { ref trait_def_id, ref input_def_id } => {
                op(trait_def_id).and_then(|trait_def_id| {
                    op(input_def_id).and_then(|input_def_id| {
                        Some(TraitSelect { trait_def_id: trait_def_id,
                                           input_def_id: input_def_id })
                    })
                })
            }
            ProjectionCache { ref def_ids } => {
                let def_ids: Option<Vec<E>> = def_ids.iter().map(op).collect();
                def_ids.map(|d| ProjectionCache { def_ids: d })
            }
        }
    }
}

/// A "work product" corresponds to a `.o` (or other) file that we
/// save in between runs. These ids do not have a DefId but rather
/// some independent path or string that persists between runs without
/// the need to be mapped or unmapped. (This ensures we can serialize
/// them even in the absence of a tcx.)
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, RustcEncodable, RustcDecodable)]
pub struct WorkProductId(pub String);

