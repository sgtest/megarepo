use clippy_utils::diagnostics::{span_lint_and_help, span_lint_and_sugg};
use clippy_utils::source::{snippet, snippet_opt, snippet_with_applicability};
use clippy_utils::{SpanlessEq, SpanlessHash};
use core::hash::{Hash, Hasher};
use if_chain::if_chain;
use itertools::Itertools;
use rustc_data_structures::fx::FxHashMap;
use rustc_data_structures::unhash::UnhashMap;
use rustc_errors::Applicability;
use rustc_hir::def::Res;
use rustc_hir::{
    GenericArg, GenericBound, Generics, Item, ItemKind, Node, Path, PathSegment, PredicateOrigin, QPath,
    TraitBoundModifier, TraitItem, TraitRef, Ty, TyKind, WherePredicate,
};
use rustc_lint::{LateContext, LateLintPass};
use rustc_session::{declare_tool_lint, impl_lint_pass};
use rustc_span::{BytePos, Span};

declare_clippy_lint! {
    /// ### What it does
    /// This lint warns about unnecessary type repetitions in trait bounds
    ///
    /// ### Why is this bad?
    /// Repeating the type for every bound makes the code
    /// less readable than combining the bounds
    ///
    /// ### Example
    /// ```rust
    /// pub fn foo<T>(t: T) where T: Copy, T: Clone {}
    /// ```
    ///
    /// Use instead:
    /// ```rust
    /// pub fn foo<T>(t: T) where T: Copy + Clone {}
    /// ```
    #[clippy::version = "1.38.0"]
    pub TYPE_REPETITION_IN_BOUNDS,
    nursery,
    "types are repeated unnecessary in trait bounds use `+` instead of using `T: _, T: _`"
}

declare_clippy_lint! {
    /// ### What it does
    /// Checks for cases where generics are being used and multiple
    /// syntax specifications for trait bounds are used simultaneously.
    ///
    /// ### Why is this bad?
    /// Duplicate bounds makes the code
    /// less readable than specifying them only once.
    ///
    /// ### Example
    /// ```rust
    /// fn func<T: Clone + Default>(arg: T) where T: Clone + Default {}
    /// ```
    ///
    /// Use instead:
    /// ```rust
    /// # mod hidden {
    /// fn func<T: Clone + Default>(arg: T) {}
    /// # }
    ///
    /// // or
    ///
    /// fn func<T>(arg: T) where T: Clone + Default {}
    /// ```
    ///
    /// ```rust
    /// fn foo<T: Default + Default>(bar: T) {}
    /// ```
    /// Use instead:
    /// ```rust
    /// fn foo<T: Default>(bar: T) {}
    /// ```
    ///
    /// ```rust
    /// fn foo<T>(bar: T) where T: Default + Default {}
    /// ```
    /// Use instead:
    /// ```rust
    /// fn foo<T>(bar: T) where T: Default {}
    /// ```
    #[clippy::version = "1.47.0"]
    pub TRAIT_DUPLICATION_IN_BOUNDS,
    nursery,
    "check if the same trait bounds are specified more than once during a generic declaration"
}

#[derive(Copy, Clone)]
pub struct TraitBounds {
    max_trait_bounds: u64,
}

impl TraitBounds {
    #[must_use]
    pub fn new(max_trait_bounds: u64) -> Self {
        Self { max_trait_bounds }
    }
}

impl_lint_pass!(TraitBounds => [TYPE_REPETITION_IN_BOUNDS, TRAIT_DUPLICATION_IN_BOUNDS]);

impl<'tcx> LateLintPass<'tcx> for TraitBounds {
    fn check_generics(&mut self, cx: &LateContext<'tcx>, gen: &'tcx Generics<'_>) {
        self.check_type_repetition(cx, gen);
        check_trait_bound_duplication(cx, gen);
        check_bounds_or_where_duplication(cx, gen);
    }

    fn check_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx Item<'tcx>) {
        // special handling for self trait bounds as these are not considered generics
        // ie. trait Foo: Display {}
        if let Item {
            kind: ItemKind::Trait(_, _, _, bounds, ..),
            ..
        } = item
        {
            rollup_traits(cx, bounds, "these bounds contain repeated elements");
        }
    }

    fn check_trait_item(&mut self, cx: &LateContext<'tcx>, item: &'tcx TraitItem<'tcx>) {
        let mut self_bounds_map = FxHashMap::default();

        for predicate in item.generics.predicates {
            if_chain! {
                if let WherePredicate::BoundPredicate(ref bound_predicate) = predicate;
                if bound_predicate.origin != PredicateOrigin::ImplTrait;
                if !bound_predicate.span.from_expansion();
                if let TyKind::Path(QPath::Resolved(_, Path { segments, .. })) = bound_predicate.bounded_ty.kind;
                if let Some(PathSegment {
                    res: Some(Res::SelfTy{ trait_: Some(def_id), alias_to: _ }), ..
                }) = segments.first();
                if let Some(
                    Node::Item(
                        Item {
                            kind: ItemKind::Trait(_, _, _, self_bounds, _),
                            .. }
                        )
                    ) = cx.tcx.hir().get_if_local(*def_id);
                then {
                    if self_bounds_map.is_empty() {
                        for bound in self_bounds.iter() {
                            let Some((self_res, self_segments, _)) = get_trait_info_from_bound(bound) else { continue };
                            self_bounds_map.insert(self_res, self_segments);
                        }
                    }

                    bound_predicate
                        .bounds
                        .iter()
                        .filter_map(get_trait_info_from_bound)
                        .for_each(|(trait_item_res, trait_item_segments, span)| {
                            if let Some(self_segments) = self_bounds_map.get(&trait_item_res) {
                                if SpanlessEq::new(cx).eq_path_segments(self_segments, trait_item_segments) {
                                    span_lint_and_help(
                                        cx,
                                        TRAIT_DUPLICATION_IN_BOUNDS,
                                        span,
                                        "this trait bound is already specified in trait declaration",
                                        None,
                                        "consider removing this trait bound",
                                    );
                                }
                            }
                        });
                }
            }
        }
    }
}

impl TraitBounds {
    fn check_type_repetition<'tcx>(self, cx: &LateContext<'tcx>, gen: &'tcx Generics<'_>) {
        struct SpanlessTy<'cx, 'tcx> {
            ty: &'tcx Ty<'tcx>,
            cx: &'cx LateContext<'tcx>,
        }
        impl PartialEq for SpanlessTy<'_, '_> {
            fn eq(&self, other: &Self) -> bool {
                let mut eq = SpanlessEq::new(self.cx);
                eq.inter_expr().eq_ty(self.ty, other.ty)
            }
        }
        impl Hash for SpanlessTy<'_, '_> {
            fn hash<H: Hasher>(&self, h: &mut H) {
                let mut t = SpanlessHash::new(self.cx);
                t.hash_ty(self.ty);
                h.write_u64(t.finish());
            }
        }
        impl Eq for SpanlessTy<'_, '_> {}

        if gen.span.from_expansion() {
            return;
        }
        let mut map: UnhashMap<SpanlessTy<'_, '_>, Vec<&GenericBound<'_>>> = UnhashMap::default();
        let mut applicability = Applicability::MaybeIncorrect;
        for bound in gen.predicates {
            if_chain! {
                if let WherePredicate::BoundPredicate(ref p) = bound;
                if p.origin != PredicateOrigin::ImplTrait;
                if p.bounds.len() as u64 <= self.max_trait_bounds;
                if !p.span.from_expansion();
                if let Some(ref v) = map.insert(
                    SpanlessTy { ty: p.bounded_ty, cx },
                    p.bounds.iter().collect::<Vec<_>>()
                );

                then {
                    let trait_bounds = v
                        .iter()
                        .copied()
                        .chain(p.bounds.iter())
                        .filter_map(get_trait_info_from_bound)
                        .map(|(_, _, span)| snippet_with_applicability(cx, span, "..", &mut applicability))
                        .join(" + ");
                    let hint_string = format!(
                        "consider combining the bounds: `{}: {}`",
                        snippet(cx, p.bounded_ty.span, "_"),
                        trait_bounds,
                    );
                    span_lint_and_help(
                        cx,
                        TYPE_REPETITION_IN_BOUNDS,
                        p.span,
                        "this type has already been used as a bound predicate",
                        None,
                        &hint_string,
                    );
                }
            }
        }
    }
}

fn check_trait_bound_duplication(cx: &LateContext<'_>, gen: &'_ Generics<'_>) {
    if gen.span.from_expansion() || gen.params.is_empty() || gen.predicates.is_empty() {
        return;
    }

    let mut map = FxHashMap::<_, Vec<_>>::default();
    for predicate in gen.predicates {
        if_chain! {
            if let WherePredicate::BoundPredicate(ref bound_predicate) = predicate;
            if bound_predicate.origin != PredicateOrigin::ImplTrait;
            if !bound_predicate.span.from_expansion();
            if let TyKind::Path(QPath::Resolved(_, Path { segments, .. })) = bound_predicate.bounded_ty.kind;
            if let Some(segment) = segments.first();
            then {
                for (res_where, _, span_where) in bound_predicate.bounds.iter().filter_map(get_trait_info_from_bound) {
                    let trait_resolutions_direct = map.entry(segment.ident).or_default();
                    if let Some((_, span_direct)) = trait_resolutions_direct
                                                .iter()
                                                .find(|(res_direct, _)| *res_direct == res_where) {
                        span_lint_and_help(
                            cx,
                            TRAIT_DUPLICATION_IN_BOUNDS,
                            *span_direct,
                            "this trait bound is already specified in the where clause",
                            None,
                            "consider removing this trait bound",
                        );
                    }
                    else {
                        trait_resolutions_direct.push((res_where, span_where));
                    }
                }
            }
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug)]
struct ComparableTraitRef(Res, Vec<Res>);

fn check_bounds_or_where_duplication(cx: &LateContext<'_>, gen: &'_ Generics<'_>) {
    if gen.span.from_expansion() {
        return;
    }

    for predicate in gen.predicates {
        if let WherePredicate::BoundPredicate(ref bound_predicate) = predicate {
            let msg = if predicate.in_where_clause() {
                "these where clauses contain repeated elements"
            } else {
                "these bounds contain repeated elements"
            };
            rollup_traits(cx, bound_predicate.bounds, msg);
        }
    }
}

fn get_trait_info_from_bound<'a>(bound: &'a GenericBound<'_>) -> Option<(Res, &'a [PathSegment<'a>], Span)> {
    if let GenericBound::Trait(t, tbm) = bound {
        let trait_path = t.trait_ref.path;
        let trait_span = {
            let path_span = trait_path.span;
            if let TraitBoundModifier::Maybe = tbm {
                path_span.with_lo(path_span.lo() - BytePos(1)) // include the `?`
            } else {
                path_span
            }
        };
        Some((trait_path.res, trait_path.segments, trait_span))
    } else {
        None
    }
}

// FIXME: ComparableTraitRef does not support nested bounds needed for associated_type_bounds
fn into_comparable_trait_ref(trait_ref: &TraitRef<'_>) -> ComparableTraitRef {
    ComparableTraitRef(
        trait_ref.path.res,
        trait_ref
            .path
            .segments
            .iter()
            .filter_map(|segment| {
                // get trait bound type arguments
                Some(segment.args?.args.iter().filter_map(|arg| {
                    if_chain! {
                        if let GenericArg::Type(ty) = arg;
                        if let TyKind::Path(QPath::Resolved(_, path)) = ty.kind;
                        then { return Some(path.res) }
                    }
                    None
                }))
            })
            .flatten()
            .collect(),
    )
}

fn rollup_traits(cx: &LateContext<'_>, bounds: &[GenericBound<'_>], msg: &str) {
    let mut map = FxHashMap::default();
    let mut repeated_res = false;

    let only_comparable_trait_refs = |bound: &GenericBound<'_>| {
        if let GenericBound::Trait(t, _) = bound {
            Some((into_comparable_trait_ref(&t.trait_ref), t.span))
        } else {
            None
        }
    };

    for bound in bounds.iter().filter_map(only_comparable_trait_refs) {
        let (comparable_bound, span_direct) = bound;
        if map.insert(comparable_bound, span_direct).is_some() {
            repeated_res = true;
        }
    }

    if_chain! {
        if repeated_res;
        if let [first_trait, .., last_trait] = bounds;
        then {
            let all_trait_span = first_trait.span().to(last_trait.span());

            let mut traits = map.values()
                .filter_map(|span| snippet_opt(cx, *span))
                .collect::<Vec<_>>();
            traits.sort_unstable();
            let traits = traits.join(" + ");

            span_lint_and_sugg(
                cx,
                TRAIT_DUPLICATION_IN_BOUNDS,
                all_trait_span,
                msg,
                "try",
                traits,
                Applicability::MachineApplicable
            );
        }
    }
}
