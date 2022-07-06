// ignore-tidy-filelength
//! Name resolution for lifetimes.
//!
//! Name resolution for lifetimes follows *much* simpler rules than the
//! full resolve. For example, lifetime names are never exported or
//! used between functions, and they operate in a purely top-down
//! way. Therefore, we break lifetime name resolution into a separate pass.

use crate::late::diagnostics::{ForLifetimeSpanType, MissingLifetimeSpot};
use rustc_ast::walk_list;
use rustc_data_structures::fx::{FxHashSet, FxIndexMap, FxIndexSet};
use rustc_errors::struct_span_err;
use rustc_hir as hir;
use rustc_hir::def::{DefKind, Res};
use rustc_hir::def_id::{DefIdMap, LocalDefId};
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::{GenericArg, GenericParam, LifetimeName, Node};
use rustc_hir::{GenericParamKind, HirIdMap};
use rustc_middle::hir::map::Map;
use rustc_middle::hir::nested_filter;
use rustc_middle::middle::resolve_lifetime::*;
use rustc_middle::ty::{self, GenericParamDefKind, TyCtxt};
use rustc_middle::{bug, span_bug};
use rustc_span::def_id::DefId;
use rustc_span::symbol::{kw, sym, Ident};
use rustc_span::Span;
use std::borrow::Cow;
use std::cell::Cell;
use std::fmt;
use std::mem::take;

trait RegionExt {
    fn early(hir_map: Map<'_>, index: &mut u32, param: &GenericParam<'_>) -> (LocalDefId, Region);

    fn late(index: u32, hir_map: Map<'_>, param: &GenericParam<'_>) -> (LocalDefId, Region);

    fn late_anon(named_late_bound_vars: u32, index: &Cell<u32>) -> Region;

    fn id(&self) -> Option<DefId>;

    fn shifted(self, amount: u32) -> Region;

    fn shifted_out_to_binder(self, binder: ty::DebruijnIndex) -> Region;

    fn subst<'a, L>(self, params: L, map: &NamedRegionMap) -> Option<Region>
    where
        L: Iterator<Item = &'a hir::Lifetime>;
}

impl RegionExt for Region {
    fn early(hir_map: Map<'_>, index: &mut u32, param: &GenericParam<'_>) -> (LocalDefId, Region) {
        let i = *index;
        *index += 1;
        let def_id = hir_map.local_def_id(param.hir_id);
        debug!("Region::early: index={} def_id={:?}", i, def_id);
        (def_id, Region::EarlyBound(i, def_id.to_def_id()))
    }

    fn late(idx: u32, hir_map: Map<'_>, param: &GenericParam<'_>) -> (LocalDefId, Region) {
        let depth = ty::INNERMOST;
        let def_id = hir_map.local_def_id(param.hir_id);
        debug!(
            "Region::late: idx={:?}, param={:?} depth={:?} def_id={:?}",
            idx, param, depth, def_id,
        );
        (def_id, Region::LateBound(depth, idx, def_id.to_def_id()))
    }

    fn late_anon(named_late_bound_vars: u32, index: &Cell<u32>) -> Region {
        let i = index.get();
        index.set(i + 1);
        let depth = ty::INNERMOST;
        Region::LateBoundAnon(depth, named_late_bound_vars + i, i)
    }

    fn id(&self) -> Option<DefId> {
        match *self {
            Region::Static | Region::LateBoundAnon(..) => None,

            Region::EarlyBound(_, id) | Region::LateBound(_, _, id) | Region::Free(_, id) => {
                Some(id)
            }
        }
    }

    fn shifted(self, amount: u32) -> Region {
        match self {
            Region::LateBound(debruijn, idx, id) => {
                Region::LateBound(debruijn.shifted_in(amount), idx, id)
            }
            Region::LateBoundAnon(debruijn, index, anon_index) => {
                Region::LateBoundAnon(debruijn.shifted_in(amount), index, anon_index)
            }
            _ => self,
        }
    }

    fn shifted_out_to_binder(self, binder: ty::DebruijnIndex) -> Region {
        match self {
            Region::LateBound(debruijn, index, id) => {
                Region::LateBound(debruijn.shifted_out_to_binder(binder), index, id)
            }
            Region::LateBoundAnon(debruijn, index, anon_index) => {
                Region::LateBoundAnon(debruijn.shifted_out_to_binder(binder), index, anon_index)
            }
            _ => self,
        }
    }

    fn subst<'a, L>(self, mut params: L, map: &NamedRegionMap) -> Option<Region>
    where
        L: Iterator<Item = &'a hir::Lifetime>,
    {
        if let Region::EarlyBound(index, _) = self {
            params.nth(index as usize).and_then(|lifetime| map.defs.get(&lifetime.hir_id).cloned())
        } else {
            Some(self)
        }
    }
}

/// Maps the id of each lifetime reference to the lifetime decl
/// that it corresponds to.
///
/// FIXME. This struct gets converted to a `ResolveLifetimes` for
/// actual use. It has the same data, but indexed by `LocalDefId`.  This
/// is silly.
#[derive(Debug, Default)]
struct NamedRegionMap {
    // maps from every use of a named (not anonymous) lifetime to a
    // `Region` describing how that region is bound
    defs: HirIdMap<Region>,

    // Maps relevant hir items to the bound vars on them. These include:
    // - function defs
    // - function pointers
    // - closures
    // - trait refs
    // - bound types (like `T` in `for<'a> T<'a>: Foo`)
    late_bound_vars: HirIdMap<Vec<ty::BoundVariableKind>>,
}

pub(crate) struct LifetimeContext<'a, 'tcx> {
    pub(crate) tcx: TyCtxt<'tcx>,
    map: &'a mut NamedRegionMap,
    scope: ScopeRef<'a>,

    /// Indicates that we only care about the definition of a trait. This should
    /// be false if the `Item` we are resolving lifetimes for is not a trait or
    /// we eventually need lifetimes resolve for trait items.
    trait_definition_only: bool,

    /// Cache for cross-crate per-definition object lifetime defaults.
    xcrate_object_lifetime_defaults: DefIdMap<Vec<ObjectLifetimeDefault>>,

    /// When encountering an undefined named lifetime, we will suggest introducing it in these
    /// places.
    pub(crate) missing_named_lifetime_spots: Vec<MissingLifetimeSpot<'tcx>>,
}

#[derive(Debug)]
enum Scope<'a> {
    /// Declares lifetimes, and each can be early-bound or late-bound.
    /// The `DebruijnIndex` of late-bound lifetimes starts at `1` and
    /// it should be shifted by the number of `Binder`s in between the
    /// declaration `Binder` and the location it's referenced from.
    Binder {
        /// We use an IndexMap here because we want these lifetimes in order
        /// for diagnostics.
        lifetimes: FxIndexMap<LocalDefId, Region>,

        /// if we extend this scope with another scope, what is the next index
        /// we should use for an early-bound region?
        next_early_index: u32,

        /// Whether or not this binder would serve as the parent
        /// binder for opaque types introduced within. For example:
        ///
        /// ```text
        ///     fn foo<'a>() -> impl for<'b> Trait<Item = impl Trait2<'a>>
        /// ```
        ///
        /// Here, the opaque types we create for the `impl Trait`
        /// and `impl Trait2` references will both have the `foo` item
        /// as their parent. When we get to `impl Trait2`, we find
        /// that it is nested within the `for<>` binder -- this flag
        /// allows us to skip that when looking for the parent binder
        /// of the resulting opaque type.
        opaque_type_parent: bool,

        scope_type: BinderScopeType,

        /// The late bound vars for a given item are stored by `HirId` to be
        /// queried later. However, if we enter an elision scope, we have to
        /// later append the elided bound vars to the list and need to know what
        /// to append to.
        hir_id: hir::HirId,

        s: ScopeRef<'a>,

        /// In some cases not allowing late bounds allows us to avoid ICEs.
        /// This is almost ways set to true.
        allow_late_bound: bool,

        /// If this binder comes from a where clause, specify how it was created.
        /// This is used to diagnose inaccessible lifetimes in APIT:
        /// ```ignore (illustrative)
        /// fn foo(x: impl for<'a> Trait<'a, Assoc = impl Copy + 'a>) {}
        /// ```
        where_bound_origin: Option<hir::PredicateOrigin>,
    },

    /// Lifetimes introduced by a fn are scoped to the call-site for that fn,
    /// if this is a fn body, otherwise the original definitions are used.
    /// Unspecified lifetimes are inferred, unless an elision scope is nested,
    /// e.g., `(&T, fn(&T) -> &T);` becomes `(&'_ T, for<'a> fn(&'a T) -> &'a T)`.
    Body {
        id: hir::BodyId,
        s: ScopeRef<'a>,
    },

    /// A scope which either determines unspecified lifetimes or errors
    /// on them (e.g., due to ambiguity). For more details, see `Elide`.
    Elision {
        elide: Elide,
        s: ScopeRef<'a>,
    },

    /// Use a specific lifetime (if `Some`) or leave it unset (to be
    /// inferred in a function body or potentially error outside one),
    /// for the default choice of lifetime in a trait object type.
    ObjectLifetimeDefault {
        lifetime: Option<Region>,
        s: ScopeRef<'a>,
    },

    /// When we have nested trait refs, we concatenate late bound vars for inner
    /// trait refs from outer ones. But we also need to include any HRTB
    /// lifetimes encountered when identifying the trait that an associated type
    /// is declared on.
    Supertrait {
        lifetimes: Vec<ty::BoundVariableKind>,
        s: ScopeRef<'a>,
    },

    TraitRefBoundary {
        s: ScopeRef<'a>,
    },

    Root,
}

#[derive(Copy, Clone, Debug)]
enum BinderScopeType {
    /// Any non-concatenating binder scopes.
    Normal,
    /// Within a syntactic trait ref, there may be multiple poly trait refs that
    /// are nested (under the `associated_type_bounds` feature). The binders of
    /// the inner poly trait refs are extended from the outer poly trait refs
    /// and don't increase the late bound depth. If you had
    /// `T: for<'a>  Foo<Bar: for<'b> Baz<'a, 'b>>`, then the `for<'b>` scope
    /// would be `Concatenating`. This also used in trait refs in where clauses
    /// where we have two binders `for<> T: for<> Foo` (I've intentionally left
    /// out any lifetimes because they aren't needed to show the two scopes).
    /// The inner `for<>` has a scope of `Concatenating`.
    Concatenating,
}

// A helper struct for debugging scopes without printing parent scopes
struct TruncatedScopeDebug<'a>(&'a Scope<'a>);

impl<'a> fmt::Debug for TruncatedScopeDebug<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Scope::Binder {
                lifetimes,
                next_early_index,
                opaque_type_parent,
                scope_type,
                hir_id,
                allow_late_bound,
                where_bound_origin,
                s: _,
            } => f
                .debug_struct("Binder")
                .field("lifetimes", lifetimes)
                .field("next_early_index", next_early_index)
                .field("opaque_type_parent", opaque_type_parent)
                .field("scope_type", scope_type)
                .field("hir_id", hir_id)
                .field("allow_late_bound", allow_late_bound)
                .field("where_bound_origin", where_bound_origin)
                .field("s", &"..")
                .finish(),
            Scope::Body { id, s: _ } => {
                f.debug_struct("Body").field("id", id).field("s", &"..").finish()
            }
            Scope::Elision { elide, s: _ } => {
                f.debug_struct("Elision").field("elide", elide).field("s", &"..").finish()
            }
            Scope::ObjectLifetimeDefault { lifetime, s: _ } => f
                .debug_struct("ObjectLifetimeDefault")
                .field("lifetime", lifetime)
                .field("s", &"..")
                .finish(),
            Scope::Supertrait { lifetimes, s: _ } => f
                .debug_struct("Supertrait")
                .field("lifetimes", lifetimes)
                .field("s", &"..")
                .finish(),
            Scope::TraitRefBoundary { s: _ } => f.debug_struct("TraitRefBoundary").finish(),
            Scope::Root => f.debug_struct("Root").finish(),
        }
    }
}

#[derive(Clone, Debug)]
enum Elide {
    /// Use a fresh anonymous late-bound lifetime each time, by
    /// incrementing the counter to generate sequential indices. All
    /// anonymous lifetimes must start *after* named bound vars.
    FreshLateAnon(u32, Cell<u32>),
    /// Always use this one lifetime.
    Exact(Region),
    /// Less or more than one lifetime were found, error on unspecified.
    Error(Vec<ElisionFailureInfo>),
    /// Forbid lifetime elision inside of a larger scope where it would be
    /// permitted. For example, in let position impl trait.
    Forbid,
}

#[derive(Clone, Debug)]
pub(crate) struct ElisionFailureInfo {
    /// Where we can find the argument pattern.
    pub(crate) parent: Option<hir::BodyId>,
    /// The index of the argument in the original definition.
    pub(crate) index: usize,
    pub(crate) lifetime_count: usize,
    pub(crate) have_bound_regions: bool,
    pub(crate) span: Span,
}

type ScopeRef<'a> = &'a Scope<'a>;

const ROOT_SCOPE: ScopeRef<'static> = &Scope::Root;

pub fn provide(providers: &mut ty::query::Providers) {
    *providers = ty::query::Providers {
        resolve_lifetimes_trait_definition,
        resolve_lifetimes,

        named_region_map: |tcx, id| resolve_lifetimes_for(tcx, id).defs.get(&id),
        is_late_bound_map,
        object_lifetime_defaults: |tcx, id| match tcx.hir().find_by_def_id(id) {
            Some(Node::Item(item)) => compute_object_lifetime_defaults(tcx, item),
            _ => None,
        },
        late_bound_vars_map: |tcx, id| resolve_lifetimes_for(tcx, id).late_bound_vars.get(&id),

        ..*providers
    };
}

/// Like `resolve_lifetimes`, but does not resolve lifetimes for trait items.
/// Also does not generate any diagnostics.
///
/// This is ultimately a subset of the `resolve_lifetimes` work. It effectively
/// resolves lifetimes only within the trait "header" -- that is, the trait
/// and supertrait list. In contrast, `resolve_lifetimes` resolves all the
/// lifetimes within the trait and its items. There is room to refactor this,
/// for example to resolve lifetimes for each trait item in separate queries,
/// but it's convenient to do the entire trait at once because the lifetimes
/// from the trait definition are in scope within the trait items as well.
///
/// The reason for this separate call is to resolve what would otherwise
/// be a cycle. Consider this example:
///
/// ```ignore UNSOLVED (maybe @jackh726 knows what lifetime parameter to give Sub)
/// trait Base<'a> {
///     type BaseItem;
/// }
/// trait Sub<'b>: for<'a> Base<'a> {
///    type SubItem: Sub<BaseItem = &'b u32>;
/// }
/// ```
///
/// When we resolve `Sub` and all its items, we also have to resolve `Sub<BaseItem = &'b u32>`.
/// To figure out the index of `'b`, we have to know about the supertraits
/// of `Sub` so that we can determine that the `for<'a>` will be in scope.
/// (This is because we -- currently at least -- flatten all the late-bound
/// lifetimes into a single binder.) This requires us to resolve the
/// *trait definition* of `Sub`; basically just enough lifetime information
/// to look at the supertraits.
#[tracing::instrument(level = "debug", skip(tcx))]
fn resolve_lifetimes_trait_definition(
    tcx: TyCtxt<'_>,
    local_def_id: LocalDefId,
) -> ResolveLifetimes {
    convert_named_region_map(do_resolve(tcx, local_def_id, true))
}

/// Computes the `ResolveLifetimes` map that contains data for an entire `Item`.
/// You should not read the result of this query directly, but rather use
/// `named_region_map`, `is_late_bound_map`, etc.
#[tracing::instrument(level = "debug", skip(tcx))]
fn resolve_lifetimes(tcx: TyCtxt<'_>, local_def_id: LocalDefId) -> ResolveLifetimes {
    convert_named_region_map(do_resolve(tcx, local_def_id, false))
}

fn do_resolve(
    tcx: TyCtxt<'_>,
    local_def_id: LocalDefId,
    trait_definition_only: bool,
) -> NamedRegionMap {
    let item = tcx.hir().expect_item(local_def_id);
    let mut named_region_map =
        NamedRegionMap { defs: Default::default(), late_bound_vars: Default::default() };
    let mut visitor = LifetimeContext {
        tcx,
        map: &mut named_region_map,
        scope: ROOT_SCOPE,
        trait_definition_only,
        xcrate_object_lifetime_defaults: Default::default(),
        missing_named_lifetime_spots: vec![],
    };
    visitor.visit_item(item);

    named_region_map
}

fn convert_named_region_map(named_region_map: NamedRegionMap) -> ResolveLifetimes {
    let mut rl = ResolveLifetimes::default();

    for (hir_id, v) in named_region_map.defs {
        let map = rl.defs.entry(hir_id.owner).or_default();
        map.insert(hir_id.local_id, v);
    }
    for (hir_id, v) in named_region_map.late_bound_vars {
        let map = rl.late_bound_vars.entry(hir_id.owner).or_default();
        map.insert(hir_id.local_id, v);
    }

    debug!(?rl.defs);
    rl
}

/// Given `any` owner (structs, traits, trait methods, etc.), does lifetime resolution.
/// There are two important things this does.
/// First, we have to resolve lifetimes for
/// the entire *`Item`* that contains this owner, because that's the largest "scope"
/// where we can have relevant lifetimes.
/// Second, if we are asking for lifetimes in a trait *definition*, we use `resolve_lifetimes_trait_definition`
/// instead of `resolve_lifetimes`, which does not descend into the trait items and does not emit diagnostics.
/// This allows us to avoid cycles. Importantly, if we ask for lifetimes for lifetimes that have an owner
/// other than the trait itself (like the trait methods or associated types), then we just use the regular
/// `resolve_lifetimes`.
fn resolve_lifetimes_for<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> &'tcx ResolveLifetimes {
    let item_id = item_for(tcx, def_id);
    if item_id == def_id {
        let item = tcx.hir().item(hir::ItemId { def_id: item_id });
        match item.kind {
            hir::ItemKind::Trait(..) => tcx.resolve_lifetimes_trait_definition(item_id),
            _ => tcx.resolve_lifetimes(item_id),
        }
    } else {
        tcx.resolve_lifetimes(item_id)
    }
}

/// Finds the `Item` that contains the given `LocalDefId`
fn item_for(tcx: TyCtxt<'_>, local_def_id: LocalDefId) -> LocalDefId {
    match tcx.hir().find_by_def_id(local_def_id) {
        Some(Node::Item(item)) => {
            return item.def_id;
        }
        _ => {}
    }
    let item = {
        let hir_id = tcx.hir().local_def_id_to_hir_id(local_def_id);
        let mut parent_iter = tcx.hir().parent_iter(hir_id);
        loop {
            let node = parent_iter.next().map(|n| n.1);
            match node {
                Some(hir::Node::Item(item)) => break item.def_id,
                Some(hir::Node::Crate(_)) | None => bug!("Called `item_for` on an Item."),
                _ => {}
            }
        }
    };
    item
}

/// In traits, there is an implicit `Self` type parameter which comes before the generics.
/// We have to account for this when computing the index of the other generic parameters.
/// This function returns whether there is such an implicit parameter defined on the given item.
fn sub_items_have_self_param(node: &hir::ItemKind<'_>) -> bool {
    matches!(*node, hir::ItemKind::Trait(..) | hir::ItemKind::TraitAlias(..))
}

fn late_region_as_bound_region<'tcx>(tcx: TyCtxt<'tcx>, region: &Region) -> ty::BoundVariableKind {
    match region {
        Region::LateBound(_, _, def_id) => {
            let name = tcx.hir().name(tcx.hir().local_def_id_to_hir_id(def_id.expect_local()));
            ty::BoundVariableKind::Region(ty::BrNamed(*def_id, name))
        }
        Region::LateBoundAnon(_, _, anon_idx) => {
            ty::BoundVariableKind::Region(ty::BrAnon(*anon_idx))
        }
        _ => bug!("{:?} is not a late region", region),
    }
}

impl<'a, 'tcx> LifetimeContext<'a, 'tcx> {
    /// Returns the binders in scope and the type of `Binder` that should be created for a poly trait ref.
    fn poly_trait_ref_binder_info(&mut self) -> (Vec<ty::BoundVariableKind>, BinderScopeType) {
        let mut scope = self.scope;
        let mut supertrait_lifetimes = vec![];
        loop {
            match scope {
                Scope::Body { .. } | Scope::Root => {
                    break (vec![], BinderScopeType::Normal);
                }

                Scope::Elision { s, .. } | Scope::ObjectLifetimeDefault { s, .. } => {
                    scope = s;
                }

                Scope::Supertrait { s, lifetimes } => {
                    supertrait_lifetimes = lifetimes.clone();
                    scope = s;
                }

                Scope::TraitRefBoundary { .. } => {
                    // We should only see super trait lifetimes if there is a `Binder` above
                    assert!(supertrait_lifetimes.is_empty());
                    break (vec![], BinderScopeType::Normal);
                }

                Scope::Binder { hir_id, .. } => {
                    // Nested poly trait refs have the binders concatenated
                    let mut full_binders =
                        self.map.late_bound_vars.entry(*hir_id).or_default().clone();
                    full_binders.extend(supertrait_lifetimes.into_iter());
                    break (full_binders, BinderScopeType::Concatenating);
                }
            }
        }
    }
}
impl<'a, 'tcx> Visitor<'tcx> for LifetimeContext<'a, 'tcx> {
    type NestedFilter = nested_filter::All;

    fn nested_visit_map(&mut self) -> Self::Map {
        self.tcx.hir()
    }

    // We want to nest trait/impl items in their parent, but nothing else.
    fn visit_nested_item(&mut self, _: hir::ItemId) {}

    fn visit_trait_item_ref(&mut self, ii: &'tcx hir::TraitItemRef) {
        if !self.trait_definition_only {
            intravisit::walk_trait_item_ref(self, ii)
        }
    }

    fn visit_nested_body(&mut self, body: hir::BodyId) {
        let body = self.tcx.hir().body(body);
        self.with(Scope::Body { id: body.id(), s: self.scope }, |this| {
            this.visit_body(body);
        });
    }

    fn visit_expr(&mut self, e: &'tcx hir::Expr<'tcx>) {
        if let hir::ExprKind::Closure { bound_generic_params, .. } = e.kind {
            let next_early_index = self.next_early_index();
            let (lifetimes, binders): (FxIndexMap<LocalDefId, Region>, Vec<_>) =
                bound_generic_params
                    .iter()
                    .filter(|param| matches!(param.kind, GenericParamKind::Lifetime { .. }))
                    .enumerate()
                    .map(|(late_bound_idx, param)| {
                        let pair = Region::late(late_bound_idx as u32, self.tcx.hir(), param);
                        let r = late_region_as_bound_region(self.tcx, &pair.1);
                        (pair, r)
                    })
                    .unzip();
            self.map.late_bound_vars.insert(e.hir_id, binders);
            let scope = Scope::Binder {
                hir_id: e.hir_id,
                lifetimes,
                s: self.scope,
                next_early_index,
                opaque_type_parent: false,
                scope_type: BinderScopeType::Normal,
                allow_late_bound: true,
                where_bound_origin: None,
            };
            self.with(scope, |this| {
                // a closure has no bounds, so everything
                // contained within is scoped within its binder.
                intravisit::walk_expr(this, e)
            });
        } else {
            intravisit::walk_expr(self, e)
        }
    }

    fn visit_item(&mut self, item: &'tcx hir::Item<'tcx>) {
        match &item.kind {
            hir::ItemKind::Impl(hir::Impl { of_trait, .. }) => {
                if let Some(of_trait) = of_trait {
                    self.map.late_bound_vars.insert(of_trait.hir_ref_id, Vec::default());
                }
            }
            _ => {}
        }
        match item.kind {
            hir::ItemKind::Fn(_, ref generics, _) => {
                self.missing_named_lifetime_spots.push(generics.into());
                self.visit_early_late(None, item.hir_id(), generics, |this| {
                    intravisit::walk_item(this, item);
                });
                self.missing_named_lifetime_spots.pop();
            }

            hir::ItemKind::ExternCrate(_)
            | hir::ItemKind::Use(..)
            | hir::ItemKind::Macro(..)
            | hir::ItemKind::Mod(..)
            | hir::ItemKind::ForeignMod { .. }
            | hir::ItemKind::GlobalAsm(..) => {
                // These sorts of items have no lifetime parameters at all.
                intravisit::walk_item(self, item);
            }
            hir::ItemKind::Static(..) | hir::ItemKind::Const(..) => {
                // No lifetime parameters, but implied 'static.
                let scope = Scope::Elision { elide: Elide::Exact(Region::Static), s: ROOT_SCOPE };
                self.with(scope, |this| intravisit::walk_item(this, item));
            }
            hir::ItemKind::OpaqueTy(hir::OpaqueTy { .. }) => {
                // Opaque types are visited when we visit the
                // `TyKind::OpaqueDef`, so that they have the lifetimes from
                // their parent opaque_ty in scope.
                //
                // The core idea here is that since OpaqueTys are generated with the impl Trait as
                // their owner, we can keep going until we find the Item that owns that. We then
                // conservatively add all resolved lifetimes. Otherwise we run into problems in
                // cases like `type Foo<'a> = impl Bar<As = impl Baz + 'a>`.
                for (_hir_id, node) in
                    self.tcx.hir().parent_iter(self.tcx.hir().local_def_id_to_hir_id(item.def_id))
                {
                    match node {
                        hir::Node::Item(parent_item) => {
                            let resolved_lifetimes: &ResolveLifetimes =
                                self.tcx.resolve_lifetimes(item_for(self.tcx, parent_item.def_id));
                            // We need to add *all* deps, since opaque tys may want them from *us*
                            for (&owner, defs) in resolved_lifetimes.defs.iter() {
                                defs.iter().for_each(|(&local_id, region)| {
                                    self.map.defs.insert(hir::HirId { owner, local_id }, *region);
                                });
                            }
                            for (&owner, late_bound_vars) in
                                resolved_lifetimes.late_bound_vars.iter()
                            {
                                late_bound_vars.iter().for_each(|(&local_id, late_bound_vars)| {
                                    self.map.late_bound_vars.insert(
                                        hir::HirId { owner, local_id },
                                        late_bound_vars.clone(),
                                    );
                                });
                            }
                            break;
                        }
                        hir::Node::Crate(_) => bug!("No Item about an OpaqueTy"),
                        _ => {}
                    }
                }
            }
            hir::ItemKind::TyAlias(_, ref generics)
            | hir::ItemKind::Enum(_, ref generics)
            | hir::ItemKind::Struct(_, ref generics)
            | hir::ItemKind::Union(_, ref generics)
            | hir::ItemKind::Trait(_, _, ref generics, ..)
            | hir::ItemKind::TraitAlias(ref generics, ..)
            | hir::ItemKind::Impl(hir::Impl { ref generics, .. }) => {
                self.missing_named_lifetime_spots.push(generics.into());

                // These kinds of items have only early-bound lifetime parameters.
                let mut index = if sub_items_have_self_param(&item.kind) {
                    1 // Self comes before lifetimes
                } else {
                    0
                };
                let mut non_lifetime_count = 0;
                let lifetimes = generics
                    .params
                    .iter()
                    .filter_map(|param| match param.kind {
                        GenericParamKind::Lifetime { .. } => {
                            Some(Region::early(self.tcx.hir(), &mut index, param))
                        }
                        GenericParamKind::Type { .. } | GenericParamKind::Const { .. } => {
                            non_lifetime_count += 1;
                            None
                        }
                    })
                    .collect();
                self.map.late_bound_vars.insert(item.hir_id(), vec![]);
                let scope = Scope::Binder {
                    hir_id: item.hir_id(),
                    lifetimes,
                    next_early_index: index + non_lifetime_count,
                    opaque_type_parent: true,
                    scope_type: BinderScopeType::Normal,
                    s: ROOT_SCOPE,
                    allow_late_bound: false,
                    where_bound_origin: None,
                };
                self.with(scope, |this| {
                    let scope = Scope::TraitRefBoundary { s: this.scope };
                    this.with(scope, |this| {
                        intravisit::walk_item(this, item);
                    });
                });
                self.missing_named_lifetime_spots.pop();
            }
        }
    }

    fn visit_foreign_item(&mut self, item: &'tcx hir::ForeignItem<'tcx>) {
        match item.kind {
            hir::ForeignItemKind::Fn(_, _, ref generics) => {
                self.visit_early_late(None, item.hir_id(), generics, |this| {
                    intravisit::walk_foreign_item(this, item);
                })
            }
            hir::ForeignItemKind::Static(..) => {
                intravisit::walk_foreign_item(self, item);
            }
            hir::ForeignItemKind::Type => {
                intravisit::walk_foreign_item(self, item);
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn visit_ty(&mut self, ty: &'tcx hir::Ty<'tcx>) {
        match ty.kind {
            hir::TyKind::BareFn(ref c) => {
                let next_early_index = self.next_early_index();
                let lifetime_span: Option<Span> =
                    c.generic_params.iter().rev().find_map(|param| match param.kind {
                        GenericParamKind::Lifetime { kind: hir::LifetimeParamKind::Explicit } => {
                            Some(param.span)
                        }
                        _ => None,
                    });
                let (span, span_type) = if let Some(span) = lifetime_span {
                    (span.shrink_to_hi(), ForLifetimeSpanType::TypeTail)
                } else {
                    (ty.span.shrink_to_lo(), ForLifetimeSpanType::TypeEmpty)
                };
                self.missing_named_lifetime_spots
                    .push(MissingLifetimeSpot::HigherRanked { span, span_type });
                let (lifetimes, binders): (FxIndexMap<LocalDefId, Region>, Vec<_>) = c
                    .generic_params
                    .iter()
                    .filter(|param| matches!(param.kind, GenericParamKind::Lifetime { .. }))
                    .enumerate()
                    .map(|(late_bound_idx, param)| {
                        let pair = Region::late(late_bound_idx as u32, self.tcx.hir(), param);
                        let r = late_region_as_bound_region(self.tcx, &pair.1);
                        (pair, r)
                    })
                    .unzip();
                self.map.late_bound_vars.insert(ty.hir_id, binders);
                let scope = Scope::Binder {
                    hir_id: ty.hir_id,
                    lifetimes,
                    s: self.scope,
                    next_early_index,
                    opaque_type_parent: false,
                    scope_type: BinderScopeType::Normal,
                    allow_late_bound: true,
                    where_bound_origin: None,
                };
                self.with(scope, |this| {
                    // a bare fn has no bounds, so everything
                    // contained within is scoped within its binder.
                    intravisit::walk_ty(this, ty);
                });
                self.missing_named_lifetime_spots.pop();
            }
            hir::TyKind::TraitObject(bounds, ref lifetime, _) => {
                debug!(?bounds, ?lifetime, "TraitObject");
                let scope = Scope::TraitRefBoundary { s: self.scope };
                self.with(scope, |this| {
                    for bound in bounds {
                        this.visit_poly_trait_ref(bound, hir::TraitBoundModifier::None);
                    }
                });
                match lifetime.name {
                    LifetimeName::Implicit => {
                        // For types like `dyn Foo`, we should
                        // generate a special form of elided.
                        span_bug!(ty.span, "object-lifetime-default expected, not implicit",);
                    }
                    LifetimeName::ImplicitObjectLifetimeDefault => {
                        // If the user does not write *anything*, we
                        // use the object lifetime defaulting
                        // rules. So e.g., `Box<dyn Debug>` becomes
                        // `Box<dyn Debug + 'static>`.
                        self.resolve_object_lifetime_default(lifetime)
                    }
                    LifetimeName::Underscore => {
                        // If the user writes `'_`, we use the *ordinary* elision
                        // rules. So the `'_` in e.g., `Box<dyn Debug + '_>` will be
                        // resolved the same as the `'_` in `&'_ Foo`.
                        //
                        // cc #48468
                        self.resolve_elided_lifetimes(&[lifetime])
                    }
                    LifetimeName::Param(..) | LifetimeName::Static => {
                        // If the user wrote an explicit name, use that.
                        self.visit_lifetime(lifetime);
                    }
                    LifetimeName::Error => {}
                }
            }
            hir::TyKind::Rptr(ref lifetime_ref, ref mt) => {
                self.visit_lifetime(lifetime_ref);
                let scope = Scope::ObjectLifetimeDefault {
                    lifetime: self.map.defs.get(&lifetime_ref.hir_id).cloned(),
                    s: self.scope,
                };
                self.with(scope, |this| this.visit_ty(&mt.ty));
            }
            hir::TyKind::OpaqueDef(item_id, lifetimes) => {
                // Resolve the lifetimes in the bounds to the lifetime defs in the generics.
                // `fn foo<'a>() -> impl MyTrait<'a> { ... }` desugars to
                // `type MyAnonTy<'b> = impl MyTrait<'b>;`
                //                 ^                  ^ this gets resolved in the scope of
                //                                      the opaque_ty generics
                let opaque_ty = self.tcx.hir().item(item_id);
                let (generics, bounds) = match opaque_ty.kind {
                    hir::ItemKind::OpaqueTy(hir::OpaqueTy {
                        origin: hir::OpaqueTyOrigin::TyAlias,
                        ..
                    }) => {
                        intravisit::walk_ty(self, ty);

                        // Elided lifetimes are not allowed in non-return
                        // position impl Trait
                        let scope = Scope::TraitRefBoundary { s: self.scope };
                        self.with(scope, |this| {
                            let scope = Scope::Elision { elide: Elide::Forbid, s: this.scope };
                            this.with(scope, |this| {
                                intravisit::walk_item(this, opaque_ty);
                            })
                        });

                        return;
                    }
                    hir::ItemKind::OpaqueTy(hir::OpaqueTy {
                        origin: hir::OpaqueTyOrigin::FnReturn(..) | hir::OpaqueTyOrigin::AsyncFn(..),
                        ref generics,
                        bounds,
                        ..
                    }) => (generics, bounds),
                    ref i => bug!("`impl Trait` pointed to non-opaque type?? {:#?}", i),
                };

                // Resolve the lifetimes that are applied to the opaque type.
                // These are resolved in the current scope.
                // `fn foo<'a>() -> impl MyTrait<'a> { ... }` desugars to
                // `fn foo<'a>() -> MyAnonTy<'a> { ... }`
                //          ^                 ^this gets resolved in the current scope
                for lifetime in lifetimes {
                    let hir::GenericArg::Lifetime(lifetime) = lifetime else {
                        continue
                    };
                    self.visit_lifetime(lifetime);

                    // Check for predicates like `impl for<'a> Trait<impl OtherTrait<'a>>`
                    // and ban them. Type variables instantiated inside binders aren't
                    // well-supported at the moment, so this doesn't work.
                    // In the future, this should be fixed and this error should be removed.
                    let def = self.map.defs.get(&lifetime.hir_id).cloned();
                    let Some(Region::LateBound(_, _, def_id)) = def else {
                        continue
                    };
                    let Some(def_id) = def_id.as_local() else {
                        continue
                    };
                    let hir_id = self.tcx.hir().local_def_id_to_hir_id(def_id);
                    // Ensure that the parent of the def is an item, not HRTB
                    let parent_id = self.tcx.hir().get_parent_node(hir_id);
                    if !parent_id.is_owner() {
                        if !self.trait_definition_only {
                            struct_span_err!(
                                self.tcx.sess,
                                lifetime.span,
                                E0657,
                                "`impl Trait` can only capture lifetimes \
                                    bound at the fn or impl level"
                            )
                            .emit();
                        }
                        self.uninsert_lifetime_on_error(lifetime, def.unwrap());
                    }
                    if let hir::Node::Item(hir::Item {
                        kind: hir::ItemKind::OpaqueTy { .. }, ..
                    }) = self.tcx.hir().get(parent_id)
                    {
                        if !self.trait_definition_only {
                            let mut err = self.tcx.sess.struct_span_err(
                                lifetime.span,
                                "higher kinded lifetime bounds on nested opaque types are not supported yet",
                            );
                            err.span_note(self.tcx.def_span(def_id), "lifetime declared here");
                            err.emit();
                        }
                        self.uninsert_lifetime_on_error(lifetime, def.unwrap());
                    }
                }

                // We want to start our early-bound indices at the end of the parent scope,
                // not including any parent `impl Trait`s.
                let mut index = self.next_early_index_for_opaque_type();
                debug!(?index);

                let mut elision = None;
                let mut lifetimes = FxIndexMap::default();
                let mut non_lifetime_count = 0;
                for param in generics.params {
                    match param.kind {
                        GenericParamKind::Lifetime { .. } => {
                            let (def_id, reg) = Region::early(self.tcx.hir(), &mut index, &param);
                            if let hir::ParamName::Plain(Ident {
                                name: kw::UnderscoreLifetime,
                                ..
                            }) = param.name
                            {
                                // Pick the elided lifetime "definition" if one exists
                                // and use it to make an elision scope.
                                elision = Some(reg);
                            } else {
                                lifetimes.insert(def_id, reg);
                            }
                        }
                        GenericParamKind::Type { .. } | GenericParamKind::Const { .. } => {
                            non_lifetime_count += 1;
                        }
                    }
                }
                let next_early_index = index + non_lifetime_count;
                self.map.late_bound_vars.insert(ty.hir_id, vec![]);

                if let Some(elision_region) = elision {
                    let scope =
                        Scope::Elision { elide: Elide::Exact(elision_region), s: self.scope };
                    self.with(scope, |this| {
                        let scope = Scope::Binder {
                            hir_id: ty.hir_id,
                            lifetimes,
                            next_early_index,
                            s: this.scope,
                            opaque_type_parent: false,
                            scope_type: BinderScopeType::Normal,
                            allow_late_bound: false,
                            where_bound_origin: None,
                        };
                        this.with(scope, |this| {
                            this.visit_generics(generics);
                            let scope = Scope::TraitRefBoundary { s: this.scope };
                            this.with(scope, |this| {
                                for bound in bounds {
                                    this.visit_param_bound(bound);
                                }
                            })
                        });
                    });
                } else {
                    let scope = Scope::Binder {
                        hir_id: ty.hir_id,
                        lifetimes,
                        next_early_index,
                        s: self.scope,
                        opaque_type_parent: false,
                        scope_type: BinderScopeType::Normal,
                        allow_late_bound: false,
                        where_bound_origin: None,
                    };
                    self.with(scope, |this| {
                        let scope = Scope::TraitRefBoundary { s: this.scope };
                        this.with(scope, |this| {
                            this.visit_generics(generics);
                            for bound in bounds {
                                this.visit_param_bound(bound);
                            }
                        })
                    });
                }
            }
            _ => intravisit::walk_ty(self, ty),
        }
    }

    fn visit_trait_item(&mut self, trait_item: &'tcx hir::TraitItem<'tcx>) {
        use self::hir::TraitItemKind::*;
        match trait_item.kind {
            Fn(_, _) => {
                self.missing_named_lifetime_spots.push((&trait_item.generics).into());
                let tcx = self.tcx;
                self.visit_early_late(
                    Some(tcx.hir().get_parent_item(trait_item.hir_id())),
                    trait_item.hir_id(),
                    &trait_item.generics,
                    |this| intravisit::walk_trait_item(this, trait_item),
                );
                self.missing_named_lifetime_spots.pop();
            }
            Type(bounds, ref ty) => {
                self.missing_named_lifetime_spots.push((&trait_item.generics).into());
                let generics = &trait_item.generics;
                let mut index = self.next_early_index();
                debug!("visit_ty: index = {}", index);
                let mut non_lifetime_count = 0;
                let lifetimes = generics
                    .params
                    .iter()
                    .filter_map(|param| match param.kind {
                        GenericParamKind::Lifetime { .. } => {
                            Some(Region::early(self.tcx.hir(), &mut index, param))
                        }
                        GenericParamKind::Type { .. } | GenericParamKind::Const { .. } => {
                            non_lifetime_count += 1;
                            None
                        }
                    })
                    .collect();
                self.map.late_bound_vars.insert(trait_item.hir_id(), vec![]);
                let scope = Scope::Binder {
                    hir_id: trait_item.hir_id(),
                    lifetimes,
                    next_early_index: index + non_lifetime_count,
                    s: self.scope,
                    opaque_type_parent: true,
                    scope_type: BinderScopeType::Normal,
                    allow_late_bound: false,
                    where_bound_origin: None,
                };
                self.with(scope, |this| {
                    let scope = Scope::TraitRefBoundary { s: this.scope };
                    this.with(scope, |this| {
                        this.visit_generics(generics);
                        for bound in bounds {
                            this.visit_param_bound(bound);
                        }
                        if let Some(ty) = ty {
                            this.visit_ty(ty);
                        }
                    })
                });
                self.missing_named_lifetime_spots.pop();
            }
            Const(_, _) => {
                // Only methods and types support generics.
                assert!(trait_item.generics.params.is_empty());
                self.missing_named_lifetime_spots.push(MissingLifetimeSpot::Static);
                intravisit::walk_trait_item(self, trait_item);
                self.missing_named_lifetime_spots.pop();
            }
        }
    }

    fn visit_impl_item(&mut self, impl_item: &'tcx hir::ImplItem<'tcx>) {
        use self::hir::ImplItemKind::*;
        match impl_item.kind {
            Fn(..) => {
                self.missing_named_lifetime_spots.push((&impl_item.generics).into());
                let tcx = self.tcx;
                self.visit_early_late(
                    Some(tcx.hir().get_parent_item(impl_item.hir_id())),
                    impl_item.hir_id(),
                    &impl_item.generics,
                    |this| intravisit::walk_impl_item(this, impl_item),
                );
                self.missing_named_lifetime_spots.pop();
            }
            TyAlias(ref ty) => {
                let generics = &impl_item.generics;
                self.missing_named_lifetime_spots.push(generics.into());
                let mut index = self.next_early_index();
                let mut non_lifetime_count = 0;
                debug!("visit_ty: index = {}", index);
                let lifetimes: FxIndexMap<LocalDefId, Region> = generics
                    .params
                    .iter()
                    .filter_map(|param| match param.kind {
                        GenericParamKind::Lifetime { .. } => {
                            Some(Region::early(self.tcx.hir(), &mut index, param))
                        }
                        GenericParamKind::Const { .. } | GenericParamKind::Type { .. } => {
                            non_lifetime_count += 1;
                            None
                        }
                    })
                    .collect();
                self.map.late_bound_vars.insert(ty.hir_id, vec![]);
                let scope = Scope::Binder {
                    hir_id: ty.hir_id,
                    lifetimes,
                    next_early_index: index + non_lifetime_count,
                    s: self.scope,
                    opaque_type_parent: true,
                    scope_type: BinderScopeType::Normal,
                    allow_late_bound: true,
                    where_bound_origin: None,
                };
                self.with(scope, |this| {
                    let scope = Scope::TraitRefBoundary { s: this.scope };
                    this.with(scope, |this| {
                        this.visit_generics(generics);
                        this.visit_ty(ty);
                    })
                });
                self.missing_named_lifetime_spots.pop();
            }
            Const(_, _) => {
                // Only methods and types support generics.
                assert!(impl_item.generics.params.is_empty());
                self.missing_named_lifetime_spots.push(MissingLifetimeSpot::Static);
                intravisit::walk_impl_item(self, impl_item);
                self.missing_named_lifetime_spots.pop();
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn visit_lifetime(&mut self, lifetime_ref: &'tcx hir::Lifetime) {
        match lifetime_ref.name {
            hir::LifetimeName::ImplicitObjectLifetimeDefault
            | hir::LifetimeName::Implicit
            | hir::LifetimeName::Underscore => self.resolve_elided_lifetimes(&[lifetime_ref]),
            hir::LifetimeName::Static => self.insert_lifetime(lifetime_ref, Region::Static),
            hir::LifetimeName::Param(param_def_id, _) => {
                self.resolve_lifetime_ref(param_def_id, lifetime_ref)
            }
            // If we've already reported an error, just ignore `lifetime_ref`.
            hir::LifetimeName::Error => {}
        }
    }

    fn visit_path(&mut self, path: &'tcx hir::Path<'tcx>, _: hir::HirId) {
        for (i, segment) in path.segments.iter().enumerate() {
            let depth = path.segments.len() - i - 1;
            if let Some(ref args) = segment.args {
                self.visit_segment_args(path.res, depth, args);
            }
        }
    }

    fn visit_fn_decl(&mut self, fd: &'tcx hir::FnDecl<'tcx>) {
        let output = match fd.output {
            hir::FnRetTy::DefaultReturn(_) => None,
            hir::FnRetTy::Return(ref ty) => Some(&**ty),
        };
        self.visit_fn_like_elision(&fd.inputs, output);
    }

    fn visit_generics(&mut self, generics: &'tcx hir::Generics<'tcx>) {
        let scope = Scope::TraitRefBoundary { s: self.scope };
        self.with(scope, |this| {
            for param in generics.params {
                match param.kind {
                    GenericParamKind::Lifetime { .. } => {}
                    GenericParamKind::Type { ref default, .. } => {
                        if let Some(ref ty) = default {
                            this.visit_ty(&ty);
                        }
                    }
                    GenericParamKind::Const { ref ty, default } => {
                        this.visit_ty(&ty);
                        if let Some(default) = default {
                            this.visit_body(this.tcx.hir().body(default.body));
                        }
                    }
                }
            }
            for predicate in generics.predicates {
                match predicate {
                    &hir::WherePredicate::BoundPredicate(hir::WhereBoundPredicate {
                        ref bounded_ty,
                        bounds,
                        ref bound_generic_params,
                        origin,
                        ..
                    }) => {
                        let (lifetimes, binders): (FxIndexMap<LocalDefId, Region>, Vec<_>) =
                            bound_generic_params
                                .iter()
                                .filter(|param| {
                                    matches!(param.kind, GenericParamKind::Lifetime { .. })
                                })
                                .enumerate()
                                .map(|(late_bound_idx, param)| {
                                    let pair =
                                        Region::late(late_bound_idx as u32, this.tcx.hir(), param);
                                    let r = late_region_as_bound_region(this.tcx, &pair.1);
                                    (pair, r)
                                })
                                .unzip();
                        this.map.late_bound_vars.insert(bounded_ty.hir_id, binders.clone());
                        let next_early_index = this.next_early_index();
                        // Even if there are no lifetimes defined here, we still wrap it in a binder
                        // scope. If there happens to be a nested poly trait ref (an error), that
                        // will be `Concatenating` anyways, so we don't have to worry about the depth
                        // being wrong.
                        let scope = Scope::Binder {
                            hir_id: bounded_ty.hir_id,
                            lifetimes,
                            s: this.scope,
                            next_early_index,
                            opaque_type_parent: false,
                            scope_type: BinderScopeType::Normal,
                            allow_late_bound: true,
                            where_bound_origin: Some(origin),
                        };
                        this.with(scope, |this| {
                            this.visit_ty(&bounded_ty);
                            walk_list!(this, visit_param_bound, bounds);
                        })
                    }
                    &hir::WherePredicate::RegionPredicate(hir::WhereRegionPredicate {
                        ref lifetime,
                        bounds,
                        ..
                    }) => {
                        this.visit_lifetime(lifetime);
                        walk_list!(this, visit_param_bound, bounds);

                        if lifetime.name != hir::LifetimeName::Static {
                            for bound in bounds {
                                let hir::GenericBound::Outlives(ref lt) = bound else {
                                    continue;
                                };
                                if lt.name != hir::LifetimeName::Static {
                                    continue;
                                }
                                this.insert_lifetime(lt, Region::Static);
                                this.tcx
                                    .sess
                                    .struct_span_warn(
                                        lifetime.span,
                                        &format!(
                                            "unnecessary lifetime parameter `{}`",
                                            lifetime.name.ident(),
                                        ),
                                    )
                                    .help(&format!(
                                        "you can use the `'static` lifetime directly, in place of `{}`",
                                        lifetime.name.ident(),
                                    ))
                                    .emit();
                            }
                        }
                    }
                    &hir::WherePredicate::EqPredicate(hir::WhereEqPredicate {
                        ref lhs_ty,
                        ref rhs_ty,
                        ..
                    }) => {
                        this.visit_ty(lhs_ty);
                        this.visit_ty(rhs_ty);
                    }
                }
            }
        })
    }

    fn visit_param_bound(&mut self, bound: &'tcx hir::GenericBound<'tcx>) {
        match bound {
            hir::GenericBound::LangItemTrait(_, _, hir_id, _) => {
                // FIXME(jackh726): This is pretty weird. `LangItemTrait` doesn't go
                // through the regular poly trait ref code, so we don't get another
                // chance to introduce a binder. For now, I'm keeping the existing logic
                // of "if there isn't a Binder scope above us, add one", but I
                // imagine there's a better way to go about this.
                let (binders, scope_type) = self.poly_trait_ref_binder_info();

                self.map.late_bound_vars.insert(*hir_id, binders);
                let scope = Scope::Binder {
                    hir_id: *hir_id,
                    lifetimes: FxIndexMap::default(),
                    s: self.scope,
                    next_early_index: self.next_early_index(),
                    opaque_type_parent: false,
                    scope_type,
                    allow_late_bound: true,
                    where_bound_origin: None,
                };
                self.with(scope, |this| {
                    intravisit::walk_param_bound(this, bound);
                });
            }
            _ => intravisit::walk_param_bound(self, bound),
        }
    }

    fn visit_poly_trait_ref(
        &mut self,
        trait_ref: &'tcx hir::PolyTraitRef<'tcx>,
        _modifier: hir::TraitBoundModifier,
    ) {
        debug!("visit_poly_trait_ref(trait_ref={:?})", trait_ref);

        let should_pop_missing_lt = self.is_trait_ref_fn_scope(trait_ref);

        let next_early_index = self.next_early_index();
        let (mut binders, scope_type) = self.poly_trait_ref_binder_info();

        let initial_bound_vars = binders.len() as u32;
        let mut lifetimes: FxIndexMap<LocalDefId, Region> = FxIndexMap::default();
        let binders_iter = trait_ref
            .bound_generic_params
            .iter()
            .filter(|param| matches!(param.kind, GenericParamKind::Lifetime { .. }))
            .enumerate()
            .map(|(late_bound_idx, param)| {
                let pair =
                    Region::late(initial_bound_vars + late_bound_idx as u32, self.tcx.hir(), param);
                let r = late_region_as_bound_region(self.tcx, &pair.1);
                lifetimes.insert(pair.0, pair.1);
                r
            });
        binders.extend(binders_iter);

        debug!(?binders);
        self.map.late_bound_vars.insert(trait_ref.trait_ref.hir_ref_id, binders);

        // Always introduce a scope here, even if this is in a where clause and
        // we introduced the binders around the bounded Ty. In that case, we
        // just reuse the concatenation functionality also present in nested trait
        // refs.
        let scope = Scope::Binder {
            hir_id: trait_ref.trait_ref.hir_ref_id,
            lifetimes,
            s: self.scope,
            next_early_index,
            opaque_type_parent: false,
            scope_type,
            allow_late_bound: true,
            where_bound_origin: None,
        };
        self.with(scope, |this| {
            walk_list!(this, visit_generic_param, trait_ref.bound_generic_params);
            this.visit_trait_ref(&trait_ref.trait_ref);
        });

        if should_pop_missing_lt {
            self.missing_named_lifetime_spots.pop();
        }
    }
}

fn compute_object_lifetime_defaults<'tcx>(
    tcx: TyCtxt<'tcx>,
    item: &hir::Item<'_>,
) -> Option<&'tcx [ObjectLifetimeDefault]> {
    match item.kind {
        hir::ItemKind::Struct(_, ref generics)
        | hir::ItemKind::Union(_, ref generics)
        | hir::ItemKind::Enum(_, ref generics)
        | hir::ItemKind::OpaqueTy(hir::OpaqueTy {
            ref generics,
            origin: hir::OpaqueTyOrigin::TyAlias,
            ..
        })
        | hir::ItemKind::TyAlias(_, ref generics)
        | hir::ItemKind::Trait(_, _, ref generics, ..) => {
            let result = object_lifetime_defaults_for_item(tcx, generics);

            // Debugging aid.
            let attrs = tcx.hir().attrs(item.hir_id());
            if tcx.sess.contains_name(attrs, sym::rustc_object_lifetime_default) {
                let object_lifetime_default_reprs: String = result
                    .iter()
                    .map(|set| match *set {
                        Set1::Empty => "BaseDefault".into(),
                        Set1::One(Region::Static) => "'static".into(),
                        Set1::One(Region::EarlyBound(mut i, _)) => generics
                            .params
                            .iter()
                            .find_map(|param| match param.kind {
                                GenericParamKind::Lifetime { .. } => {
                                    if i == 0 {
                                        return Some(param.name.ident().to_string().into());
                                    }
                                    i -= 1;
                                    None
                                }
                                _ => None,
                            })
                            .unwrap(),
                        Set1::One(_) => bug!(),
                        Set1::Many => "Ambiguous".into(),
                    })
                    .collect::<Vec<Cow<'static, str>>>()
                    .join(",");
                tcx.sess.span_err(item.span, &object_lifetime_default_reprs);
            }

            Some(result)
        }
        _ => None,
    }
}

/// Scan the bounds and where-clauses on parameters to extract bounds
/// of the form `T:'a` so as to determine the `ObjectLifetimeDefault`
/// for each type parameter.
fn object_lifetime_defaults_for_item<'tcx>(
    tcx: TyCtxt<'tcx>,
    generics: &hir::Generics<'_>,
) -> &'tcx [ObjectLifetimeDefault] {
    fn add_bounds(set: &mut Set1<hir::LifetimeName>, bounds: &[hir::GenericBound<'_>]) {
        for bound in bounds {
            if let hir::GenericBound::Outlives(ref lifetime) = *bound {
                set.insert(lifetime.name.normalize_to_macros_2_0());
            }
        }
    }

    let process_param = |param: &hir::GenericParam<'_>| match param.kind {
        GenericParamKind::Lifetime { .. } => None,
        GenericParamKind::Type { .. } => {
            let mut set = Set1::Empty;

            let param_def_id = tcx.hir().local_def_id(param.hir_id);
            for predicate in generics.predicates {
                // Look for `type: ...` where clauses.
                let hir::WherePredicate::BoundPredicate(ref data) = *predicate else { continue };

                // Ignore `for<'a> type: ...` as they can change what
                // lifetimes mean (although we could "just" handle it).
                if !data.bound_generic_params.is_empty() {
                    continue;
                }

                let res = match data.bounded_ty.kind {
                    hir::TyKind::Path(hir::QPath::Resolved(None, ref path)) => path.res,
                    _ => continue,
                };

                if res == Res::Def(DefKind::TyParam, param_def_id.to_def_id()) {
                    add_bounds(&mut set, &data.bounds);
                }
            }

            Some(match set {
                Set1::Empty => Set1::Empty,
                Set1::One(name) => {
                    if name == hir::LifetimeName::Static {
                        Set1::One(Region::Static)
                    } else {
                        generics
                            .params
                            .iter()
                            .filter_map(|param| match param.kind {
                                GenericParamKind::Lifetime { .. } => {
                                    let param_def_id = tcx.hir().local_def_id(param.hir_id);
                                    Some((
                                        param_def_id,
                                        hir::LifetimeName::Param(param_def_id, param.name),
                                    ))
                                }
                                _ => None,
                            })
                            .enumerate()
                            .find(|&(_, (_, lt_name))| lt_name == name)
                            .map_or(Set1::Many, |(i, (def_id, _))| {
                                Set1::One(Region::EarlyBound(i as u32, def_id.to_def_id()))
                            })
                    }
                }
                Set1::Many => Set1::Many,
            })
        }
        GenericParamKind::Const { .. } => {
            // Generic consts don't impose any constraints.
            //
            // We still store a dummy value here to allow generic parameters
            // in an arbitrary order.
            Some(Set1::Empty)
        }
    };

    tcx.arena.alloc_from_iter(generics.params.iter().filter_map(process_param))
}

impl<'a, 'tcx> LifetimeContext<'a, 'tcx> {
    fn with<F>(&mut self, wrap_scope: Scope<'_>, f: F)
    where
        F: for<'b> FnOnce(&mut LifetimeContext<'b, 'tcx>),
    {
        let LifetimeContext { tcx, map, .. } = self;
        let xcrate_object_lifetime_defaults = take(&mut self.xcrate_object_lifetime_defaults);
        let missing_named_lifetime_spots = take(&mut self.missing_named_lifetime_spots);
        let mut this = LifetimeContext {
            tcx: *tcx,
            map,
            scope: &wrap_scope,
            trait_definition_only: self.trait_definition_only,
            xcrate_object_lifetime_defaults,
            missing_named_lifetime_spots,
        };
        let span = tracing::debug_span!("scope", scope = ?TruncatedScopeDebug(&this.scope));
        {
            let _enter = span.enter();
            f(&mut this);
        }
        self.xcrate_object_lifetime_defaults = this.xcrate_object_lifetime_defaults;
        self.missing_named_lifetime_spots = this.missing_named_lifetime_spots;
    }

    /// Visits self by adding a scope and handling recursive walk over the contents with `walk`.
    ///
    /// Handles visiting fns and methods. These are a bit complicated because we must distinguish
    /// early- vs late-bound lifetime parameters. We do this by checking which lifetimes appear
    /// within type bounds; those are early bound lifetimes, and the rest are late bound.
    ///
    /// For example:
    ///
    ///    fn foo<'a,'b,'c,T:Trait<'b>>(...)
    ///
    /// Here `'a` and `'c` are late bound but `'b` is early bound. Note that early- and late-bound
    /// lifetimes may be interspersed together.
    ///
    /// If early bound lifetimes are present, we separate them into their own list (and likewise
    /// for late bound). They will be numbered sequentially, starting from the lowest index that is
    /// already in scope (for a fn item, that will be 0, but for a method it might not be). Late
    /// bound lifetimes are resolved by name and associated with a binder ID (`binder_id`), so the
    /// ordering is not important there.
    fn visit_early_late<F>(
        &mut self,
        parent_id: Option<LocalDefId>,
        hir_id: hir::HirId,
        generics: &'tcx hir::Generics<'tcx>,
        walk: F,
    ) where
        F: for<'b, 'c> FnOnce(&'b mut LifetimeContext<'c, 'tcx>),
    {
        // Find the start of nested early scopes, e.g., in methods.
        let mut next_early_index = 0;
        if let Some(parent_id) = parent_id {
            let parent = self.tcx.hir().expect_item(parent_id);
            if sub_items_have_self_param(&parent.kind) {
                next_early_index += 1; // Self comes before lifetimes
            }
            match parent.kind {
                hir::ItemKind::Trait(_, _, ref generics, ..)
                | hir::ItemKind::Impl(hir::Impl { ref generics, .. }) => {
                    next_early_index += generics.params.len() as u32;
                }
                _ => {}
            }
        }

        let mut non_lifetime_count = 0;
        let mut named_late_bound_vars = 0;
        let lifetimes: FxIndexMap<LocalDefId, Region> = generics
            .params
            .iter()
            .filter_map(|param| match param.kind {
                GenericParamKind::Lifetime { .. } => {
                    if self.tcx.is_late_bound(param.hir_id) {
                        let late_bound_idx = named_late_bound_vars;
                        named_late_bound_vars += 1;
                        Some(Region::late(late_bound_idx, self.tcx.hir(), param))
                    } else {
                        Some(Region::early(self.tcx.hir(), &mut next_early_index, param))
                    }
                }
                GenericParamKind::Type { .. } | GenericParamKind::Const { .. } => {
                    non_lifetime_count += 1;
                    None
                }
            })
            .collect();
        let next_early_index = next_early_index + non_lifetime_count;

        let binders: Vec<_> = generics
            .params
            .iter()
            .filter(|param| {
                matches!(param.kind, GenericParamKind::Lifetime { .. })
                    && self.tcx.is_late_bound(param.hir_id)
            })
            .enumerate()
            .map(|(late_bound_idx, param)| {
                let pair = Region::late(late_bound_idx as u32, self.tcx.hir(), param);
                late_region_as_bound_region(self.tcx, &pair.1)
            })
            .collect();
        self.map.late_bound_vars.insert(hir_id, binders);
        let scope = Scope::Binder {
            hir_id,
            lifetimes,
            next_early_index,
            s: self.scope,
            opaque_type_parent: true,
            scope_type: BinderScopeType::Normal,
            allow_late_bound: true,
            where_bound_origin: None,
        };
        self.with(scope, walk);
    }

    fn next_early_index_helper(&self, only_opaque_type_parent: bool) -> u32 {
        let mut scope = self.scope;
        loop {
            match *scope {
                Scope::Root => return 0,

                Scope::Binder { next_early_index, opaque_type_parent, .. }
                    if (!only_opaque_type_parent || opaque_type_parent) =>
                {
                    return next_early_index;
                }

                Scope::Binder { s, .. }
                | Scope::Body { s, .. }
                | Scope::Elision { s, .. }
                | Scope::ObjectLifetimeDefault { s, .. }
                | Scope::Supertrait { s, .. }
                | Scope::TraitRefBoundary { s, .. } => scope = s,
            }
        }
    }

    /// Returns the next index one would use for an early-bound-region
    /// if extending the current scope.
    fn next_early_index(&self) -> u32 {
        self.next_early_index_helper(true)
    }

    /// Returns the next index one would use for an `impl Trait` that
    /// is being converted into an opaque type alias `impl Trait`. This will be the
    /// next early index from the enclosing item, for the most
    /// part. See the `opaque_type_parent` field for more info.
    fn next_early_index_for_opaque_type(&self) -> u32 {
        self.next_early_index_helper(false)
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn resolve_lifetime_ref(
        &mut self,
        region_def_id: LocalDefId,
        lifetime_ref: &'tcx hir::Lifetime,
    ) {
        // Walk up the scope chain, tracking the number of fn scopes
        // that we pass through, until we find a lifetime with the
        // given name or we run out of scopes.
        // search.
        let mut late_depth = 0;
        let mut scope = self.scope;
        let mut outermost_body = None;
        let result = loop {
            match *scope {
                Scope::Body { id, s } => {
                    outermost_body = Some(id);
                    scope = s;
                }

                Scope::Root => {
                    break None;
                }

                Scope::Binder { ref lifetimes, scope_type, s, .. } => {
                    if let Some(&def) = lifetimes.get(&region_def_id) {
                        break Some(def.shifted(late_depth));
                    }
                    match scope_type {
                        BinderScopeType::Normal => late_depth += 1,
                        BinderScopeType::Concatenating => {}
                    }
                    scope = s;
                }

                Scope::Elision { s, .. }
                | Scope::ObjectLifetimeDefault { s, .. }
                | Scope::Supertrait { s, .. }
                | Scope::TraitRefBoundary { s, .. } => {
                    scope = s;
                }
            }
        };

        if let Some(mut def) = result {
            if let Region::EarlyBound(..) = def {
                // Do not free early-bound regions, only late-bound ones.
            } else if let Some(body_id) = outermost_body {
                let fn_id = self.tcx.hir().body_owner(body_id);
                match self.tcx.hir().get(fn_id) {
                    Node::Item(&hir::Item { kind: hir::ItemKind::Fn(..), .. })
                    | Node::TraitItem(&hir::TraitItem {
                        kind: hir::TraitItemKind::Fn(..), ..
                    })
                    | Node::ImplItem(&hir::ImplItem { kind: hir::ImplItemKind::Fn(..), .. }) => {
                        let scope = self.tcx.hir().local_def_id(fn_id);
                        def = Region::Free(scope.to_def_id(), def.id().unwrap());
                    }
                    _ => {}
                }
            }

            self.insert_lifetime(lifetime_ref, def);
            return;
        }

        // We may fail to resolve higher-ranked lifetimes that are mentionned by APIT.
        // AST-based resolution does not care for impl-trait desugaring, which are the
        // responibility of lowering.  This may create a mismatch between the resolution
        // AST found (`region_def_id`) which points to HRTB, and what HIR allows.
        // ```
        // fn foo(x: impl for<'a> Trait<'a, Assoc = impl Copy + 'a>) {}
        // ```
        //
        // In such case, walk back the binders to diagnose it properly.
        let mut scope = self.scope;
        loop {
            match *scope {
                Scope::Binder {
                    where_bound_origin: Some(hir::PredicateOrigin::ImplTrait), ..
                } => {
                    let mut err = self.tcx.sess.struct_span_err(
                        lifetime_ref.span,
                        "`impl Trait` can only mention lifetimes bound at the fn or impl level",
                    );
                    err.span_note(self.tcx.def_span(region_def_id), "lifetime declared here");
                    err.emit();
                    return;
                }
                Scope::Root => break,
                Scope::Binder { s, .. }
                | Scope::Body { s, .. }
                | Scope::Elision { s, .. }
                | Scope::ObjectLifetimeDefault { s, .. }
                | Scope::Supertrait { s, .. }
                | Scope::TraitRefBoundary { s, .. } => {
                    scope = s;
                }
            }
        }

        self.tcx.sess.delay_span_bug(
            lifetime_ref.span,
            &format!("Could not resolve {:?} in scope {:#?}", lifetime_ref, self.scope,),
        );
    }

    fn visit_segment_args(
        &mut self,
        res: Res,
        depth: usize,
        generic_args: &'tcx hir::GenericArgs<'tcx>,
    ) {
        debug!(
            "visit_segment_args(res={:?}, depth={:?}, generic_args={:?})",
            res, depth, generic_args,
        );

        if generic_args.parenthesized {
            self.visit_fn_like_elision(generic_args.inputs(), Some(generic_args.bindings[0].ty()));
            return;
        }

        let mut elide_lifetimes = true;
        let lifetimes: Vec<_> = generic_args
            .args
            .iter()
            .filter_map(|arg| match arg {
                hir::GenericArg::Lifetime(lt) => {
                    if !lt.is_elided() {
                        elide_lifetimes = false;
                    }
                    Some(lt)
                }
                _ => None,
            })
            .collect();
        // We short-circuit here if all are elided in order to pluralize
        // possible errors
        if elide_lifetimes {
            self.resolve_elided_lifetimes(&lifetimes);
        } else {
            lifetimes.iter().for_each(|lt| self.visit_lifetime(lt));
        }

        // Figure out if this is a type/trait segment,
        // which requires object lifetime defaults.
        let parent_def_id = |this: &mut Self, def_id: DefId| {
            let def_key = this.tcx.def_key(def_id);
            DefId { krate: def_id.krate, index: def_key.parent.expect("missing parent") }
        };
        let type_def_id = match res {
            Res::Def(DefKind::AssocTy, def_id) if depth == 1 => Some(parent_def_id(self, def_id)),
            Res::Def(DefKind::Variant, def_id) if depth == 0 => Some(parent_def_id(self, def_id)),
            Res::Def(
                DefKind::Struct
                | DefKind::Union
                | DefKind::Enum
                | DefKind::TyAlias
                | DefKind::Trait,
                def_id,
            ) if depth == 0 => Some(def_id),
            _ => None,
        };

        debug!("visit_segment_args: type_def_id={:?}", type_def_id);

        // Compute a vector of defaults, one for each type parameter,
        // per the rules given in RFCs 599 and 1156. Example:
        //
        // ```rust
        // struct Foo<'a, T: 'a, U> { }
        // ```
        //
        // If you have `Foo<'x, dyn Bar, dyn Baz>`, we want to default
        // `dyn Bar` to `dyn Bar + 'x` (because of the `T: 'a` bound)
        // and `dyn Baz` to `dyn Baz + 'static` (because there is no
        // such bound).
        //
        // Therefore, we would compute `object_lifetime_defaults` to a
        // vector like `['x, 'static]`. Note that the vector only
        // includes type parameters.
        let object_lifetime_defaults = type_def_id.map_or_else(Vec::new, |def_id| {
            let in_body = {
                let mut scope = self.scope;
                loop {
                    match *scope {
                        Scope::Root => break false,

                        Scope::Body { .. } => break true,

                        Scope::Binder { s, .. }
                        | Scope::Elision { s, .. }
                        | Scope::ObjectLifetimeDefault { s, .. }
                        | Scope::Supertrait { s, .. }
                        | Scope::TraitRefBoundary { s, .. } => {
                            scope = s;
                        }
                    }
                }
            };

            let map = &self.map;
            let set_to_region = |set: &ObjectLifetimeDefault| match *set {
                Set1::Empty => {
                    if in_body {
                        None
                    } else {
                        Some(Region::Static)
                    }
                }
                Set1::One(r) => {
                    let lifetimes = generic_args.args.iter().filter_map(|arg| match arg {
                        GenericArg::Lifetime(lt) => Some(lt),
                        _ => None,
                    });
                    r.subst(lifetimes, map)
                }
                Set1::Many => None,
            };
            if let Some(def_id) = def_id.as_local() {
                let id = self.tcx.hir().local_def_id_to_hir_id(def_id);
                self.tcx
                    .object_lifetime_defaults(id.owner)
                    .unwrap()
                    .iter()
                    .map(set_to_region)
                    .collect()
            } else {
                let tcx = self.tcx;
                self.xcrate_object_lifetime_defaults
                    .entry(def_id)
                    .or_insert_with(|| {
                        tcx.generics_of(def_id)
                            .params
                            .iter()
                            .filter_map(|param| match param.kind {
                                GenericParamDefKind::Type { object_lifetime_default, .. } => {
                                    Some(object_lifetime_default)
                                }
                                GenericParamDefKind::Const { .. } => Some(Set1::Empty),
                                GenericParamDefKind::Lifetime => None,
                            })
                            .collect()
                    })
                    .iter()
                    .map(set_to_region)
                    .collect()
            }
        });

        debug!("visit_segment_args: object_lifetime_defaults={:?}", object_lifetime_defaults);

        let mut i = 0;
        for arg in generic_args.args {
            match arg {
                GenericArg::Lifetime(_) => {}
                GenericArg::Type(ty) => {
                    if let Some(&lt) = object_lifetime_defaults.get(i) {
                        let scope = Scope::ObjectLifetimeDefault { lifetime: lt, s: self.scope };
                        self.with(scope, |this| this.visit_ty(ty));
                    } else {
                        self.visit_ty(ty);
                    }
                    i += 1;
                }
                GenericArg::Const(ct) => {
                    self.visit_anon_const(&ct.value);
                    i += 1;
                }
                GenericArg::Infer(inf) => {
                    self.visit_id(inf.hir_id);
                    i += 1;
                }
            }
        }

        // Hack: when resolving the type `XX` in binding like `dyn
        // Foo<'b, Item = XX>`, the current object-lifetime default
        // would be to examine the trait `Foo` to check whether it has
        // a lifetime bound declared on `Item`. e.g., if `Foo` is
        // declared like so, then the default object lifetime bound in
        // `XX` should be `'b`:
        //
        // ```rust
        // trait Foo<'a> {
        //   type Item: 'a;
        // }
        // ```
        //
        // but if we just have `type Item;`, then it would be
        // `'static`. However, we don't get all of this logic correct.
        //
        // Instead, we do something hacky: if there are no lifetime parameters
        // to the trait, then we simply use a default object lifetime
        // bound of `'static`, because there is no other possibility. On the other hand,
        // if there ARE lifetime parameters, then we require the user to give an
        // explicit bound for now.
        //
        // This is intended to leave room for us to implement the
        // correct behavior in the future.
        let has_lifetime_parameter =
            generic_args.args.iter().any(|arg| matches!(arg, GenericArg::Lifetime(_)));

        // Resolve lifetimes found in the bindings, so either in the type `XX` in `Item = XX` or
        // in the trait ref `YY<...>` in `Item: YY<...>`.
        for binding in generic_args.bindings {
            let scope = Scope::ObjectLifetimeDefault {
                lifetime: if has_lifetime_parameter { None } else { Some(Region::Static) },
                s: self.scope,
            };
            if let Some(type_def_id) = type_def_id {
                let lifetimes = LifetimeContext::supertrait_hrtb_lifetimes(
                    self.tcx,
                    type_def_id,
                    binding.ident,
                );
                self.with(scope, |this| {
                    let scope = Scope::Supertrait {
                        lifetimes: lifetimes.unwrap_or_default(),
                        s: this.scope,
                    };
                    this.with(scope, |this| this.visit_assoc_type_binding(binding));
                });
            } else {
                self.with(scope, |this| this.visit_assoc_type_binding(binding));
            }
        }
    }

    /// Returns all the late-bound vars that come into scope from supertrait HRTBs, based on the
    /// associated type name and starting trait.
    /// For example, imagine we have
    /// ```ignore (illustrative)
    /// trait Foo<'a, 'b> {
    ///   type As;
    /// }
    /// trait Bar<'b>: for<'a> Foo<'a, 'b> {}
    /// trait Bar: for<'b> Bar<'b> {}
    /// ```
    /// In this case, if we wanted to the supertrait HRTB lifetimes for `As` on
    /// the starting trait `Bar`, we would return `Some(['b, 'a])`.
    fn supertrait_hrtb_lifetimes(
        tcx: TyCtxt<'tcx>,
        def_id: DefId,
        assoc_name: Ident,
    ) -> Option<Vec<ty::BoundVariableKind>> {
        let trait_defines_associated_type_named = |trait_def_id: DefId| {
            tcx.associated_items(trait_def_id)
                .find_by_name_and_kind(tcx, assoc_name, ty::AssocKind::Type, trait_def_id)
                .is_some()
        };

        use smallvec::{smallvec, SmallVec};
        let mut stack: SmallVec<[(DefId, SmallVec<[ty::BoundVariableKind; 8]>); 8]> =
            smallvec![(def_id, smallvec![])];
        let mut visited: FxHashSet<DefId> = FxHashSet::default();
        loop {
            let Some((def_id, bound_vars)) = stack.pop() else {
                break None;
            };
            // See issue #83753. If someone writes an associated type on a non-trait, just treat it as
            // there being no supertrait HRTBs.
            match tcx.def_kind(def_id) {
                DefKind::Trait | DefKind::TraitAlias | DefKind::Impl => {}
                _ => break None,
            }

            if trait_defines_associated_type_named(def_id) {
                break Some(bound_vars.into_iter().collect());
            }
            let predicates =
                tcx.super_predicates_that_define_assoc_type((def_id, Some(assoc_name)));
            let obligations = predicates.predicates.iter().filter_map(|&(pred, _)| {
                let bound_predicate = pred.kind();
                match bound_predicate.skip_binder() {
                    ty::PredicateKind::Trait(data) => {
                        // The order here needs to match what we would get from `subst_supertrait`
                        let pred_bound_vars = bound_predicate.bound_vars();
                        let mut all_bound_vars = bound_vars.clone();
                        all_bound_vars.extend(pred_bound_vars.iter());
                        let super_def_id = data.trait_ref.def_id;
                        Some((super_def_id, all_bound_vars))
                    }
                    _ => None,
                }
            });

            let obligations = obligations.filter(|o| visited.insert(o.0));
            stack.extend(obligations);
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn visit_fn_like_elision(
        &mut self,
        inputs: &'tcx [hir::Ty<'tcx>],
        output: Option<&'tcx hir::Ty<'tcx>>,
    ) {
        debug!("visit_fn_like_elision: enter");
        let mut scope = &*self.scope;
        let hir_id = loop {
            match scope {
                Scope::Binder { hir_id, allow_late_bound: true, .. } => {
                    break *hir_id;
                }
                Scope::ObjectLifetimeDefault { ref s, .. }
                | Scope::Elision { ref s, .. }
                | Scope::Supertrait { ref s, .. }
                | Scope::TraitRefBoundary { ref s, .. } => {
                    scope = *s;
                }
                Scope::Root
                | Scope::Body { .. }
                | Scope::Binder { allow_late_bound: false, .. } => {
                    // See issues #83907 and #83693. Just bail out from looking inside.
                    // See the issue #95023 for not allowing late bound
                    self.tcx.sess.delay_span_bug(
                        rustc_span::DUMMY_SP,
                        "In fn_like_elision without appropriate scope above",
                    );
                    return;
                }
            }
        };
        // While not strictly necessary, we gather anon lifetimes *before* actually
        // visiting the argument types.
        let mut gather = GatherAnonLifetimes { anon_count: 0 };
        for input in inputs {
            gather.visit_ty(input);
        }
        trace!(?gather.anon_count);
        let late_bound_vars = self.map.late_bound_vars.entry(hir_id).or_default();
        let named_late_bound_vars = late_bound_vars.len() as u32;
        late_bound_vars.extend(
            (0..gather.anon_count).map(|var| ty::BoundVariableKind::Region(ty::BrAnon(var))),
        );
        let arg_scope = Scope::Elision {
            elide: Elide::FreshLateAnon(named_late_bound_vars, Cell::new(0)),
            s: self.scope,
        };
        self.with(arg_scope, |this| {
            for input in inputs {
                this.visit_ty(input);
            }
        });

        let Some(output) = output else { return };

        debug!("determine output");

        // Figure out if there's a body we can get argument names from,
        // and whether there's a `self` argument (treated specially).
        let mut assoc_item_kind = None;
        let mut impl_self = None;
        let parent = self.tcx.hir().get_parent_node(output.hir_id);
        let body = match self.tcx.hir().get(parent) {
            // `fn` definitions and methods.
            Node::Item(&hir::Item { kind: hir::ItemKind::Fn(.., body), .. }) => Some(body),

            Node::TraitItem(&hir::TraitItem { kind: hir::TraitItemKind::Fn(_, ref m), .. }) => {
                if let hir::ItemKind::Trait(.., ref trait_items) =
                    self.tcx.hir().expect_item(self.tcx.hir().get_parent_item(parent)).kind
                {
                    assoc_item_kind =
                        trait_items.iter().find(|ti| ti.id.hir_id() == parent).map(|ti| ti.kind);
                }
                match *m {
                    hir::TraitFn::Required(_) => None,
                    hir::TraitFn::Provided(body) => Some(body),
                }
            }

            Node::ImplItem(&hir::ImplItem { kind: hir::ImplItemKind::Fn(_, body), .. }) => {
                if let hir::ItemKind::Impl(hir::Impl { ref self_ty, ref items, .. }) =
                    self.tcx.hir().expect_item(self.tcx.hir().get_parent_item(parent)).kind
                {
                    impl_self = Some(self_ty);
                    assoc_item_kind =
                        items.iter().find(|ii| ii.id.hir_id() == parent).map(|ii| ii.kind);
                }
                Some(body)
            }

            // Foreign functions, `fn(...) -> R` and `Trait(...) -> R` (both types and bounds).
            Node::ForeignItem(_) | Node::Ty(_) | Node::TraitRef(_) => None,

            Node::TypeBinding(_) if let Node::TraitRef(_) = self.tcx.hir().get(self.tcx.hir().get_parent_node(parent)) => None,

            // Everything else (only closures?) doesn't
            // actually enjoy elision in return types.
            _ => {
                self.visit_ty(output);
                return;
            }
        };

        let has_self = match assoc_item_kind {
            Some(hir::AssocItemKind::Fn { has_self }) => has_self,
            _ => false,
        };

        // In accordance with the rules for lifetime elision, we can determine
        // what region to use for elision in the output type in two ways.
        // First (determined here), if `self` is by-reference, then the
        // implied output region is the region of the self parameter.
        if has_self {
            struct SelfVisitor<'a> {
                map: &'a NamedRegionMap,
                impl_self: Option<&'a hir::TyKind<'a>>,
                lifetime: Set1<Region>,
            }

            impl SelfVisitor<'_> {
                // Look for `self: &'a Self` - also desugared from `&'a self`,
                // and if that matches, use it for elision and return early.
                fn is_self_ty(&self, res: Res) -> bool {
                    if let Res::SelfTy { .. } = res {
                        return true;
                    }

                    // Can't always rely on literal (or implied) `Self` due
                    // to the way elision rules were originally specified.
                    if let Some(&hir::TyKind::Path(hir::QPath::Resolved(None, ref path))) =
                        self.impl_self
                    {
                        match path.res {
                            // Permit the types that unambiguously always
                            // result in the same type constructor being used
                            // (it can't differ between `Self` and `self`).
                            Res::Def(DefKind::Struct | DefKind::Union | DefKind::Enum, _)
                            | Res::PrimTy(_) => return res == path.res,
                            _ => {}
                        }
                    }

                    false
                }
            }

            impl<'a> Visitor<'a> for SelfVisitor<'a> {
                fn visit_ty(&mut self, ty: &'a hir::Ty<'a>) {
                    if let hir::TyKind::Rptr(lifetime_ref, ref mt) = ty.kind {
                        if let hir::TyKind::Path(hir::QPath::Resolved(None, ref path)) = mt.ty.kind
                        {
                            if self.is_self_ty(path.res) {
                                if let Some(lifetime) = self.map.defs.get(&lifetime_ref.hir_id) {
                                    self.lifetime.insert(*lifetime);
                                }
                            }
                        }
                    }
                    intravisit::walk_ty(self, ty)
                }
            }

            let mut visitor = SelfVisitor {
                map: self.map,
                impl_self: impl_self.map(|ty| &ty.kind),
                lifetime: Set1::Empty,
            };
            visitor.visit_ty(&inputs[0]);
            if let Set1::One(lifetime) = visitor.lifetime {
                let scope = Scope::Elision { elide: Elide::Exact(lifetime), s: self.scope };
                self.with(scope, |this| this.visit_ty(output));
                return;
            }
        }

        // Second, if there was exactly one lifetime (either a substitution or a
        // reference) in the arguments, then any anonymous regions in the output
        // have that lifetime.
        let mut possible_implied_output_region = None;
        let mut lifetime_count = 0;
        let arg_lifetimes = inputs
            .iter()
            .enumerate()
            .skip(has_self as usize)
            .map(|(i, input)| {
                let mut gather = GatherLifetimes {
                    map: self.map,
                    outer_index: ty::INNERMOST,
                    have_bound_regions: false,
                    lifetimes: Default::default(),
                };
                gather.visit_ty(input);

                lifetime_count += gather.lifetimes.len();

                if lifetime_count == 1 && gather.lifetimes.len() == 1 {
                    // there's a chance that the unique lifetime of this
                    // iteration will be the appropriate lifetime for output
                    // parameters, so lets store it.
                    possible_implied_output_region = gather.lifetimes.iter().cloned().next();
                }

                ElisionFailureInfo {
                    parent: body,
                    index: i,
                    lifetime_count: gather.lifetimes.len(),
                    have_bound_regions: gather.have_bound_regions,
                    span: input.span,
                }
            })
            .collect();

        let elide = if lifetime_count == 1 {
            Elide::Exact(possible_implied_output_region.unwrap())
        } else {
            Elide::Error(arg_lifetimes)
        };

        debug!(?elide);

        let scope = Scope::Elision { elide, s: self.scope };
        self.with(scope, |this| this.visit_ty(output));

        struct GatherLifetimes<'a> {
            map: &'a NamedRegionMap,
            outer_index: ty::DebruijnIndex,
            have_bound_regions: bool,
            lifetimes: FxHashSet<Region>,
        }

        impl<'v, 'a> Visitor<'v> for GatherLifetimes<'a> {
            fn visit_ty(&mut self, ty: &hir::Ty<'_>) {
                if let hir::TyKind::BareFn(_) = ty.kind {
                    self.outer_index.shift_in(1);
                }
                match ty.kind {
                    hir::TyKind::TraitObject(bounds, ref lifetime, _) => {
                        for bound in bounds {
                            self.visit_poly_trait_ref(bound, hir::TraitBoundModifier::None);
                        }

                        // Stay on the safe side and don't include the object
                        // lifetime default (which may not end up being used).
                        if !lifetime.is_elided() {
                            self.visit_lifetime(lifetime);
                        }
                    }
                    _ => {
                        intravisit::walk_ty(self, ty);
                    }
                }
                if let hir::TyKind::BareFn(_) = ty.kind {
                    self.outer_index.shift_out(1);
                }
            }

            fn visit_generic_param(&mut self, param: &hir::GenericParam<'_>) {
                if let hir::GenericParamKind::Lifetime { .. } = param.kind {
                    // FIXME(eddyb) Do we want this? It only makes a difference
                    // if this `for<'a>` lifetime parameter is never used.
                    self.have_bound_regions = true;
                }

                intravisit::walk_generic_param(self, param);
            }

            fn visit_poly_trait_ref(
                &mut self,
                trait_ref: &hir::PolyTraitRef<'_>,
                modifier: hir::TraitBoundModifier,
            ) {
                self.outer_index.shift_in(1);
                intravisit::walk_poly_trait_ref(self, trait_ref, modifier);
                self.outer_index.shift_out(1);
            }

            fn visit_param_bound(&mut self, bound: &hir::GenericBound<'_>) {
                if let hir::GenericBound::LangItemTrait { .. } = bound {
                    self.outer_index.shift_in(1);
                    intravisit::walk_param_bound(self, bound);
                    self.outer_index.shift_out(1);
                } else {
                    intravisit::walk_param_bound(self, bound);
                }
            }

            fn visit_lifetime(&mut self, lifetime_ref: &hir::Lifetime) {
                if let Some(&lifetime) = self.map.defs.get(&lifetime_ref.hir_id) {
                    match lifetime {
                        Region::LateBound(debruijn, _, _)
                        | Region::LateBoundAnon(debruijn, _, _)
                            if debruijn < self.outer_index =>
                        {
                            self.have_bound_regions = true;
                        }
                        _ => {
                            // FIXME(jackh726): nested trait refs?
                            self.lifetimes.insert(lifetime.shifted_out_to_binder(self.outer_index));
                        }
                    }
                }
            }
        }

        struct GatherAnonLifetimes {
            anon_count: u32,
        }
        impl<'v> Visitor<'v> for GatherAnonLifetimes {
            #[instrument(skip(self), level = "trace")]
            fn visit_ty(&mut self, ty: &hir::Ty<'_>) {
                // If we enter a `BareFn`, then we enter a *new* binding scope
                if let hir::TyKind::BareFn(_) = ty.kind {
                    return;
                }
                intravisit::walk_ty(self, ty);
            }

            fn visit_generic_args(
                &mut self,
                path_span: Span,
                generic_args: &'v hir::GenericArgs<'v>,
            ) {
                // parenthesized args enter a new elision scope
                if generic_args.parenthesized {
                    return;
                }
                intravisit::walk_generic_args(self, path_span, generic_args)
            }

            #[instrument(skip(self), level = "trace")]
            fn visit_lifetime(&mut self, lifetime_ref: &hir::Lifetime) {
                if lifetime_ref.is_elided() {
                    self.anon_count += 1;
                }
            }
        }
    }

    fn resolve_elided_lifetimes(&mut self, lifetime_refs: &[&'tcx hir::Lifetime]) {
        debug!("resolve_elided_lifetimes(lifetime_refs={:?})", lifetime_refs);

        if lifetime_refs.is_empty() {
            return;
        }

        let mut late_depth = 0;
        let mut scope = self.scope;
        let mut in_scope_lifetimes = FxIndexSet::default();
        let error = loop {
            match *scope {
                // Do not assign any resolution, it will be inferred.
                Scope::Body { .. } => return,

                Scope::Root => break None,

                Scope::Binder { s, ref lifetimes, scope_type, .. } => {
                    // collect named lifetimes for suggestions
                    in_scope_lifetimes.extend(lifetimes.keys().copied());
                    match scope_type {
                        BinderScopeType::Normal => late_depth += 1,
                        BinderScopeType::Concatenating => {}
                    }
                    scope = s;
                }

                Scope::Elision {
                    elide: Elide::FreshLateAnon(named_late_bound_vars, ref counter),
                    ..
                } => {
                    for lifetime_ref in lifetime_refs {
                        let lifetime =
                            Region::late_anon(named_late_bound_vars, counter).shifted(late_depth);

                        self.insert_lifetime(lifetime_ref, lifetime);
                    }
                    return;
                }

                Scope::Elision { elide: Elide::Exact(l), .. } => {
                    let lifetime = l.shifted(late_depth);
                    for lifetime_ref in lifetime_refs {
                        self.insert_lifetime(lifetime_ref, lifetime);
                    }
                    return;
                }

                Scope::Elision { elide: Elide::Error(ref e), ref s, .. } => {
                    let mut scope = s;
                    loop {
                        match scope {
                            Scope::Binder { ref lifetimes, s, .. } => {
                                // Collect named lifetimes for suggestions.
                                in_scope_lifetimes.extend(lifetimes.keys().copied());
                                scope = s;
                            }
                            Scope::ObjectLifetimeDefault { ref s, .. }
                            | Scope::Elision { ref s, .. }
                            | Scope::TraitRefBoundary { ref s, .. } => {
                                scope = s;
                            }
                            _ => break,
                        }
                    }
                    break Some(&e[..]);
                }

                Scope::Elision { elide: Elide::Forbid, .. } => break None,

                Scope::ObjectLifetimeDefault { s, .. }
                | Scope::Supertrait { s, .. }
                | Scope::TraitRefBoundary { s, .. } => {
                    scope = s;
                }
            }
        };

        let mut spans: Vec<_> = lifetime_refs.iter().map(|lt| lt.span).collect();
        spans.sort();
        let mut spans_dedup = spans.clone();
        spans_dedup.dedup();
        let spans_with_counts: Vec<_> = spans_dedup
            .into_iter()
            .map(|sp| (sp, spans.iter().filter(|nsp| *nsp == &sp).count()))
            .collect();

        let mut err = self.report_missing_lifetime_specifiers(spans.clone(), lifetime_refs.len());

        self.add_missing_lifetime_specifiers_label(
            &mut err,
            spans_with_counts,
            in_scope_lifetimes,
            error,
        );
        err.emit();
    }

    fn resolve_object_lifetime_default(&mut self, lifetime_ref: &'tcx hir::Lifetime) {
        debug!("resolve_object_lifetime_default(lifetime_ref={:?})", lifetime_ref);
        let mut late_depth = 0;
        let mut scope = self.scope;
        let lifetime = loop {
            match *scope {
                Scope::Binder { s, scope_type, .. } => {
                    match scope_type {
                        BinderScopeType::Normal => late_depth += 1,
                        BinderScopeType::Concatenating => {}
                    }
                    scope = s;
                }

                Scope::Root | Scope::Elision { .. } => break Region::Static,

                Scope::Body { .. } | Scope::ObjectLifetimeDefault { lifetime: None, .. } => return,

                Scope::ObjectLifetimeDefault { lifetime: Some(l), .. } => break l,

                Scope::Supertrait { s, .. } | Scope::TraitRefBoundary { s, .. } => {
                    scope = s;
                }
            }
        };
        self.insert_lifetime(lifetime_ref, lifetime.shifted(late_depth));
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn insert_lifetime(&mut self, lifetime_ref: &'tcx hir::Lifetime, def: Region) {
        debug!(
            node = ?self.tcx.hir().node_to_string(lifetime_ref.hir_id),
            span = ?self.tcx.sess.source_map().span_to_diagnostic_string(lifetime_ref.span)
        );
        self.map.defs.insert(lifetime_ref.hir_id, def);
    }

    /// Sometimes we resolve a lifetime, but later find that it is an
    /// error (esp. around impl trait). In that case, we remove the
    /// entry into `map.defs` so as not to confuse later code.
    fn uninsert_lifetime_on_error(&mut self, lifetime_ref: &'tcx hir::Lifetime, bad_def: Region) {
        let old_value = self.map.defs.remove(&lifetime_ref.hir_id);
        assert_eq!(old_value, Some(bad_def));
    }
}

/// Detects late-bound lifetimes and inserts them into
/// `late_bound`.
///
/// A region declared on a fn is **late-bound** if:
/// - it is constrained by an argument type;
/// - it does not appear in a where-clause.
///
/// "Constrained" basically means that it appears in any type but
/// not amongst the inputs to a projection. In other words, `<&'a
/// T as Trait<''b>>::Foo` does not constrain `'a` or `'b`.
fn is_late_bound_map(tcx: TyCtxt<'_>, def_id: LocalDefId) -> Option<&FxIndexSet<LocalDefId>> {
    let hir_id = tcx.hir().local_def_id_to_hir_id(def_id);
    let decl = tcx.hir().fn_decl_by_hir_id(hir_id)?;
    let generics = tcx.hir().get_generics(def_id)?;

    let mut late_bound = FxIndexSet::default();

    let mut constrained_by_input = ConstrainedCollector::default();
    for arg_ty in decl.inputs {
        constrained_by_input.visit_ty(arg_ty);
    }

    let mut appears_in_output = AllCollector::default();
    intravisit::walk_fn_ret_ty(&mut appears_in_output, &decl.output);

    debug!(?constrained_by_input.regions);

    // Walk the lifetimes that appear in where clauses.
    //
    // Subtle point: because we disallow nested bindings, we can just
    // ignore binders here and scrape up all names we see.
    let mut appears_in_where_clause = AllCollector::default();
    appears_in_where_clause.visit_generics(generics);
    debug!(?appears_in_where_clause.regions);

    // Late bound regions are those that:
    // - appear in the inputs
    // - do not appear in the where-clauses
    // - are not implicitly captured by `impl Trait`
    for param in generics.params {
        match param.kind {
            hir::GenericParamKind::Lifetime { .. } => { /* fall through */ }

            // Neither types nor consts are late-bound.
            hir::GenericParamKind::Type { .. } | hir::GenericParamKind::Const { .. } => continue,
        }

        let param_def_id = tcx.hir().local_def_id(param.hir_id);

        // appears in the where clauses? early-bound.
        if appears_in_where_clause.regions.contains(&param_def_id) {
            continue;
        }

        // does not appear in the inputs, but appears in the return type? early-bound.
        if !constrained_by_input.regions.contains(&param_def_id)
            && appears_in_output.regions.contains(&param_def_id)
        {
            continue;
        }

        debug!("lifetime {:?} with id {:?} is late-bound", param.name.ident(), param.hir_id);

        let inserted = late_bound.insert(param_def_id);
        assert!(inserted, "visited lifetime {:?} twice", param.hir_id);
    }

    debug!(?late_bound);
    return Some(tcx.arena.alloc(late_bound));

    #[derive(Default)]
    struct ConstrainedCollector {
        regions: FxHashSet<LocalDefId>,
    }

    impl<'v> Visitor<'v> for ConstrainedCollector {
        fn visit_ty(&mut self, ty: &'v hir::Ty<'v>) {
            match ty.kind {
                hir::TyKind::Path(
                    hir::QPath::Resolved(Some(_), _) | hir::QPath::TypeRelative(..),
                ) => {
                    // ignore lifetimes appearing in associated type
                    // projections, as they are not *constrained*
                    // (defined above)
                }

                hir::TyKind::Path(hir::QPath::Resolved(None, ref path)) => {
                    // consider only the lifetimes on the final
                    // segment; I am not sure it's even currently
                    // valid to have them elsewhere, but even if it
                    // is, those would be potentially inputs to
                    // projections
                    if let Some(last_segment) = path.segments.last() {
                        self.visit_path_segment(path.span, last_segment);
                    }
                }

                _ => {
                    intravisit::walk_ty(self, ty);
                }
            }
        }

        fn visit_lifetime(&mut self, lifetime_ref: &'v hir::Lifetime) {
            if let hir::LifetimeName::Param(def_id, _) = lifetime_ref.name {
                self.regions.insert(def_id);
            }
        }
    }

    #[derive(Default)]
    struct AllCollector {
        regions: FxHashSet<LocalDefId>,
    }

    impl<'v> Visitor<'v> for AllCollector {
        fn visit_lifetime(&mut self, lifetime_ref: &'v hir::Lifetime) {
            if let hir::LifetimeName::Param(def_id, _) = lifetime_ref.name {
                self.regions.insert(def_id);
            }
        }
    }
}
