//! HIR ty lowering: Lowers type-system entities[^1] from the [HIR][hir] to
//! the [`rustc_middle::ty`] representation.
//!
//! Not to be confused with *AST lowering* which lowers AST constructs to HIR ones
//! or with *THIR* / *MIR* *lowering* / *building* which lowers HIR *bodies*
//! (i.e., “executable code”) to THIR / MIR.
//!
//! Most lowering routines are defined on [`dyn HirTyLowerer`](HirTyLowerer) directly,
//! like the main routine of this module, `lower_ty`.
//!
//! This module used to be called `astconv`.
//!
//! [^1]: This includes types, lifetimes / regions, constants in type positions,
//! trait references and bounds.

mod bounds;
mod errors;
pub mod generics;
mod lint;
mod object_safety;

use crate::bounds::Bounds;
use crate::collect::HirPlaceholderCollector;
use crate::errors::AmbiguousLifetimeBound;
use crate::hir_ty_lowering::errors::prohibit_assoc_item_binding;
use crate::hir_ty_lowering::generics::{check_generic_arg_count, lower_generic_args};
use crate::middle::resolve_bound_vars as rbv;
use crate::require_c_abi_if_c_variadic;
use rustc_ast::TraitObjectSyntax;
use rustc_data_structures::fx::{FxHashSet, FxIndexMap};
use rustc_errors::{
    codes::*, struct_span_code_err, Applicability, Diag, ErrorGuaranteed, FatalError, MultiSpan,
};
use rustc_hir as hir;
use rustc_hir::def::{CtorOf, DefKind, Namespace, Res};
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_hir::intravisit::{walk_generics, Visitor as _};
use rustc_hir::{GenericArg, GenericArgs};
use rustc_infer::infer::{InferCtxt, TyCtxtInferExt};
use rustc_infer::traits::ObligationCause;
use rustc_middle::middle::stability::AllowUnstable;
use rustc_middle::ty::{
    self, Const, GenericArgKind, GenericArgsRef, GenericParamDefKind, ParamEnv, Ty, TyCtxt,
    TypeVisitableExt,
};
use rustc_session::lint::builtin::AMBIGUOUS_ASSOCIATED_ITEMS;
use rustc_span::edit_distance::find_best_match_for_name;
use rustc_span::symbol::{kw, Ident, Symbol};
use rustc_span::{sym, BytePos, Span, DUMMY_SP};
use rustc_target::spec::abi;
use rustc_trait_selection::traits::wf::object_region_bounds;
use rustc_trait_selection::traits::{self, ObligationCtxt};

use std::fmt::Display;
use std::slice;

/// A path segment that is semantically allowed to have generic arguments.
#[derive(Debug)]
pub struct GenericPathSegment(pub DefId, pub usize);

#[derive(Copy, Clone, Debug)]
pub struct OnlySelfBounds(pub bool);

#[derive(Copy, Clone, Debug)]
pub enum PredicateFilter {
    /// All predicates may be implied by the trait.
    All,

    /// Only traits that reference `Self: ..` are implied by the trait.
    SelfOnly,

    /// Only traits that reference `Self: ..` and define an associated type
    /// with the given ident are implied by the trait.
    SelfThatDefines(Ident),

    /// Only traits that reference `Self: ..` and their associated type bounds.
    /// For example, given `Self: Tr<A: B>`, this would expand to `Self: Tr`
    /// and `<Self as Tr>::A: B`.
    SelfAndAssociatedTypeBounds,
}

/// A context which can lower type-system entities from the [HIR][hir] to
/// the [`rustc_middle::ty`] representation.
///
/// This trait used to be called `AstConv`.
pub trait HirTyLowerer<'tcx> {
    fn tcx(&self) -> TyCtxt<'tcx>;

    /// Returns the [`DefId`] of the overarching item whose constituents get lowered.
    fn item_def_id(&self) -> DefId;

    /// Returns `true` if the current context allows the use of inference variables.
    fn allow_infer(&self) -> bool;

    /// Returns the region to use when a lifetime is omitted (and not elided).
    fn re_infer(&self, param: Option<&ty::GenericParamDef>, span: Span)
    -> Option<ty::Region<'tcx>>;

    /// Returns the type to use when a type is omitted.
    fn ty_infer(&self, param: Option<&ty::GenericParamDef>, span: Span) -> Ty<'tcx>;

    /// Returns the const to use when a const is omitted.
    fn ct_infer(
        &self,
        ty: Ty<'tcx>,
        param: Option<&ty::GenericParamDef>,
        span: Span,
    ) -> Const<'tcx>;

    /// Probe bounds in scope where the bounded type coincides with the given type parameter.
    ///
    /// Rephrased, this returns bounds of the form `T: Trait`, where `T` is a type parameter
    /// with the given `def_id`. This is a subset of the full set of bounds.
    ///
    /// This method may use the given `assoc_name` to disregard bounds whose trait reference
    /// doesn't define an associated item with the provided name.
    ///
    /// This is used for one specific purpose: Resolving “short-hand” associated type references
    /// like `T::Item` where `T` is a type parameter. In principle, we would do that by first
    /// getting the full set of predicates in scope and then filtering down to find those that
    /// apply to `T`, but this can lead to cycle errors. The problem is that we have to do this
    /// resolution *in order to create the predicates in the first place*.
    /// Hence, we have this “special pass”.
    fn probe_ty_param_bounds(
        &self,
        span: Span,
        def_id: LocalDefId,
        assoc_name: Ident,
    ) -> ty::GenericPredicates<'tcx>;

    /// Lower an associated type to a projection.
    ///
    /// This method has to be defined by the concrete lowering context because
    /// dealing with higher-ranked trait references depends on its capabilities:
    ///
    /// If the context can make use of type inference, it can simply instantiate
    /// any late-bound vars bound by the trait reference with inference variables.
    /// If it doesn't support type inference, there is nothing reasonable it can
    /// do except reject the associated type.
    ///
    /// The canonical example of this is associated type `T::P` where `T` is a type
    /// param constrained by `T: for<'a> Trait<'a>` and where `Trait` defines `P`.
    fn lower_assoc_ty(
        &self,
        span: Span,
        item_def_id: DefId,
        item_segment: &hir::PathSegment<'tcx>,
        poly_trait_ref: ty::PolyTraitRef<'tcx>,
    ) -> Ty<'tcx>;

    /// Returns `AdtDef` if `ty` is an ADT.
    ///
    /// Note that `ty` might be a alias type that needs normalization.
    /// This used to get the enum variants in scope of the type.
    /// For example, `Self::A` could refer to an associated type
    /// or to an enum variant depending on the result of this function.
    fn probe_adt(&self, span: Span, ty: Ty<'tcx>) -> Option<ty::AdtDef<'tcx>>;

    /// Record the lowered type of a HIR node in this context.
    fn record_ty(&self, hir_id: hir::HirId, ty: Ty<'tcx>, span: Span);

    /// The inference context of the lowering context if applicable.
    fn infcx(&self) -> Option<&InferCtxt<'tcx>>;

    /// Taint the context with errors.
    ///
    /// Invoke this when you encounter an error from some prior pass like name resolution.
    /// This is used to help suppress derived errors typeck might otherwise report.
    fn set_tainted_by_errors(&self, e: ErrorGuaranteed);

    /// Convenience method for coercing the lowering context into a trait object type.
    ///
    /// Most lowering routines are defined on the trait object type directly
    /// necessitating a coercion step from the concrete lowering context.
    fn lowerer(&self) -> &dyn HirTyLowerer<'tcx>
    where
        Self: Sized,
    {
        self
    }
}

/// New-typed boolean indicating whether explicit late-bound lifetimes
/// are present in a set of generic arguments.
///
/// For example if we have some method `fn f<'a>(&'a self)` implemented
/// for some type `T`, although `f` is generic in the lifetime `'a`, `'a`
/// is late-bound so should not be provided explicitly. Thus, if `f` is
/// instantiated with some generic arguments providing `'a` explicitly,
/// we taint those arguments with `ExplicitLateBound::Yes` so that we
/// can provide an appropriate diagnostic later.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ExplicitLateBound {
    Yes,
    No,
}

#[derive(Copy, Clone, PartialEq)]
pub enum IsMethodCall {
    Yes,
    No,
}

/// Denotes the "position" of a generic argument, indicating if it is a generic type,
/// generic function or generic method call.
#[derive(Copy, Clone, PartialEq)]
pub(crate) enum GenericArgPosition {
    Type,
    Value, // e.g., functions
    MethodCall,
}

/// A marker denoting that the generic arguments that were
/// provided did not match the respective generic parameters.
#[derive(Clone, Default, Debug)]
pub struct GenericArgCountMismatch {
    /// Indicates whether a fatal error was reported (`Some`), or just a lint (`None`).
    pub reported: Option<ErrorGuaranteed>,
    /// A list of spans of arguments provided that were not valid.
    pub invalid_args: Vec<Span>,
}

/// Decorates the result of a generic argument count mismatch
/// check with whether explicit late bounds were provided.
#[derive(Clone, Debug)]
pub struct GenericArgCountResult {
    pub explicit_late_bound: ExplicitLateBound,
    pub correct: Result<(), GenericArgCountMismatch>,
}

/// A context which can lower HIR's [`GenericArg`] to `rustc_middle`'s [`ty::GenericArg`].
///
/// Its only consumer is [`generics::lower_generic_args`].
/// Read its documentation to learn more.
pub trait GenericArgsLowerer<'a, 'tcx> {
    fn args_for_def_id(&mut self, def_id: DefId) -> (Option<&'a GenericArgs<'tcx>>, bool);

    fn provided_kind(
        &mut self,
        param: &ty::GenericParamDef,
        arg: &GenericArg<'tcx>,
    ) -> ty::GenericArg<'tcx>;

    fn inferred_kind(
        &mut self,
        args: Option<&[ty::GenericArg<'tcx>]>,
        param: &ty::GenericParamDef,
        infer_args: bool,
    ) -> ty::GenericArg<'tcx>;
}

impl<'tcx> dyn HirTyLowerer<'tcx> + '_ {
    /// Lower a lifetime from the HIR to our internal notion of a lifetime called a *region*.
    #[instrument(level = "debug", skip(self), ret)]
    pub fn lower_lifetime(
        &self,
        lifetime: &hir::Lifetime,
        def: Option<&ty::GenericParamDef>,
    ) -> ty::Region<'tcx> {
        let tcx = self.tcx();
        let lifetime_name = |def_id| tcx.hir().name(tcx.local_def_id_to_hir_id(def_id));

        match tcx.named_bound_var(lifetime.hir_id) {
            Some(rbv::ResolvedArg::StaticLifetime) => tcx.lifetimes.re_static,

            Some(rbv::ResolvedArg::LateBound(debruijn, index, def_id)) => {
                let name = lifetime_name(def_id.expect_local());
                let br = ty::BoundRegion {
                    var: ty::BoundVar::from_u32(index),
                    kind: ty::BrNamed(def_id, name),
                };
                ty::Region::new_bound(tcx, debruijn, br)
            }

            Some(rbv::ResolvedArg::EarlyBound(def_id)) => {
                let name = tcx.hir().ty_param_name(def_id.expect_local());
                let item_def_id = tcx.hir().ty_param_owner(def_id.expect_local());
                let generics = tcx.generics_of(item_def_id);
                let index = generics.param_def_id_to_index[&def_id];
                ty::Region::new_early_param(tcx, ty::EarlyParamRegion { def_id, index, name })
            }

            Some(rbv::ResolvedArg::Free(scope, id)) => {
                let name = lifetime_name(id.expect_local());
                ty::Region::new_late_param(tcx, scope, ty::BrNamed(id, name))

                // (*) -- not late-bound, won't change
            }

            Some(rbv::ResolvedArg::Error(guar)) => ty::Region::new_error(tcx, guar),

            None => {
                self.re_infer(def, lifetime.ident.span).unwrap_or_else(|| {
                    debug!(?lifetime, "unelided lifetime in signature");

                    // This indicates an illegal lifetime
                    // elision. `resolve_lifetime` should have
                    // reported an error in this case -- but if
                    // not, let's error out.
                    ty::Region::new_error_with_message(
                        tcx,
                        lifetime.ident.span,
                        "unelided lifetime in signature",
                    )
                })
            }
        }
    }

    pub fn lower_generic_args_of_path_segment(
        &self,
        span: Span,
        def_id: DefId,
        item_segment: &hir::PathSegment<'tcx>,
    ) -> GenericArgsRef<'tcx> {
        let (args, _) = self.lower_generic_args_of_path(
            span,
            def_id,
            &[],
            item_segment,
            None,
            ty::BoundConstness::NotConst,
        );
        if let Some(b) = item_segment.args().bindings.first() {
            prohibit_assoc_item_binding(self.tcx(), b.span, Some((item_segment, span)));
        }
        args
    }

    /// Lower the generic arguments provided to some path.
    ///
    /// If this is a trait reference, you also need to pass the self type `self_ty`.
    /// The lowering process may involve applying defaulted type parameters.
    ///
    /// Associated item bindings are not handled here!
    ///
    /// ### Example
    ///
    /// ```ignore (illustrative)
    ///    T: std::ops::Index<usize, Output = u32>
    /// // ^1 ^^^^^^^^^^^^^^2 ^^^^3  ^^^^^^^^^^^4
    /// ```
    ///
    /// 1. The `self_ty` here would refer to the type `T`.
    /// 2. The path in question is the path to the trait `std::ops::Index`,
    ///    which will have been resolved to a `def_id`
    /// 3. The `generic_args` contains info on the `<...>` contents. The `usize` type
    ///    parameters are returned in the `GenericArgsRef`
    /// 4. Associated type bindings like `Output = u32` are contained in `generic_args.bindings`.
    ///
    /// Note that the type listing given here is *exactly* what the user provided.
    ///
    /// For (generic) associated types
    ///
    /// ```ignore (illustrative)
    /// <Vec<u8> as Iterable<u8>>::Iter::<'a>
    /// ```
    ///
    /// We have the parent args are the args for the parent trait:
    /// `[Vec<u8>, u8]` and `generic_args` are the arguments for the associated
    /// type itself: `['a]`. The returned `GenericArgsRef` concatenates these two
    /// lists: `[Vec<u8>, u8, 'a]`.
    #[instrument(level = "debug", skip(self, span), ret)]
    fn lower_generic_args_of_path(
        &self,
        span: Span,
        def_id: DefId,
        parent_args: &[ty::GenericArg<'tcx>],
        segment: &hir::PathSegment<'tcx>,
        self_ty: Option<Ty<'tcx>>,
        constness: ty::BoundConstness,
    ) -> (GenericArgsRef<'tcx>, GenericArgCountResult) {
        // If the type is parameterized by this region, then replace this
        // region with the current anon region binding (in other words,
        // whatever & would get replaced with).

        let tcx = self.tcx();
        let generics = tcx.generics_of(def_id);
        debug!(?generics);

        if generics.has_self {
            if generics.parent.is_some() {
                // The parent is a trait so it should have at least one
                // generic parameter for the `Self` type.
                assert!(!parent_args.is_empty())
            } else {
                // This item (presumably a trait) needs a self-type.
                assert!(self_ty.is_some());
            }
        } else {
            assert!(self_ty.is_none());
        }

        let mut arg_count = check_generic_arg_count(
            tcx,
            def_id,
            segment,
            generics,
            GenericArgPosition::Type,
            self_ty.is_some(),
        );

        if let Err(err) = &arg_count.correct
            && let Some(reported) = err.reported
        {
            self.set_tainted_by_errors(reported);
        }

        // Skip processing if type has no generic parameters.
        // Traits always have `Self` as a generic parameter, which means they will not return early
        // here and so associated type bindings will be handled regardless of whether there are any
        // non-`Self` generic parameters.
        if generics.params.is_empty() {
            return (tcx.mk_args(parent_args), arg_count);
        }

        struct GenericArgsCtxt<'a, 'tcx> {
            lowerer: &'a dyn HirTyLowerer<'tcx>,
            def_id: DefId,
            generic_args: &'a GenericArgs<'tcx>,
            span: Span,
            inferred_params: Vec<Span>,
            infer_args: bool,
        }

        impl<'a, 'tcx> GenericArgsLowerer<'a, 'tcx> for GenericArgsCtxt<'a, 'tcx> {
            fn args_for_def_id(&mut self, did: DefId) -> (Option<&'a GenericArgs<'tcx>>, bool) {
                if did == self.def_id {
                    (Some(self.generic_args), self.infer_args)
                } else {
                    // The last component of this tuple is unimportant.
                    (None, false)
                }
            }

            fn provided_kind(
                &mut self,
                param: &ty::GenericParamDef,
                arg: &GenericArg<'tcx>,
            ) -> ty::GenericArg<'tcx> {
                let tcx = self.lowerer.tcx();

                let mut handle_ty_args = |has_default, ty: &hir::Ty<'tcx>| {
                    if has_default {
                        tcx.check_optional_stability(
                            param.def_id,
                            Some(arg.hir_id()),
                            arg.span(),
                            None,
                            AllowUnstable::No,
                            |_, _| {
                                // Default generic parameters may not be marked
                                // with stability attributes, i.e. when the
                                // default parameter was defined at the same time
                                // as the rest of the type. As such, we ignore missing
                                // stability attributes.
                            },
                        );
                    }
                    if let (hir::TyKind::Infer, false) = (&ty.kind, self.lowerer.allow_infer()) {
                        self.inferred_params.push(ty.span);
                        Ty::new_misc_error(tcx).into()
                    } else {
                        self.lowerer.lower_ty(ty).into()
                    }
                };

                match (&param.kind, arg) {
                    (GenericParamDefKind::Lifetime, GenericArg::Lifetime(lt)) => {
                        self.lowerer.lower_lifetime(lt, Some(param)).into()
                    }
                    (&GenericParamDefKind::Type { has_default, .. }, GenericArg::Type(ty)) => {
                        handle_ty_args(has_default, ty)
                    }
                    (&GenericParamDefKind::Type { has_default, .. }, GenericArg::Infer(inf)) => {
                        handle_ty_args(has_default, &inf.to_ty())
                    }
                    (GenericParamDefKind::Const { .. }, GenericArg::Const(ct)) => {
                        let did = ct.value.def_id;
                        tcx.feed_anon_const_type(did, tcx.type_of(param.def_id));
                        ty::Const::from_anon_const(tcx, did).into()
                    }
                    (&GenericParamDefKind::Const { .. }, hir::GenericArg::Infer(inf)) => {
                        let ty = tcx
                            .at(self.span)
                            .type_of(param.def_id)
                            .no_bound_vars()
                            .expect("const parameter types cannot be generic");
                        if self.lowerer.allow_infer() {
                            self.lowerer.ct_infer(ty, Some(param), inf.span).into()
                        } else {
                            self.inferred_params.push(inf.span);
                            ty::Const::new_misc_error(tcx, ty).into()
                        }
                    }
                    (kind, arg) => span_bug!(
                        self.span,
                        "mismatched path argument for kind {kind:?}: found arg {arg:?}"
                    ),
                }
            }

            fn inferred_kind(
                &mut self,
                args: Option<&[ty::GenericArg<'tcx>]>,
                param: &ty::GenericParamDef,
                infer_args: bool,
            ) -> ty::GenericArg<'tcx> {
                let tcx = self.lowerer.tcx();
                match param.kind {
                    GenericParamDefKind::Lifetime => self
                        .lowerer
                        .re_infer(Some(param), self.span)
                        .unwrap_or_else(|| {
                            debug!(?param, "unelided lifetime in signature");

                            // This indicates an illegal lifetime in a non-assoc-trait position
                            ty::Region::new_error_with_message(
                                tcx,
                                self.span,
                                "unelided lifetime in signature",
                            )
                        })
                        .into(),
                    GenericParamDefKind::Type { has_default, .. } => {
                        if !infer_args && has_default {
                            // No type parameter provided, but a default exists.
                            let args = args.unwrap();
                            if args.iter().any(|arg| match arg.unpack() {
                                GenericArgKind::Type(ty) => ty.references_error(),
                                _ => false,
                            }) {
                                // Avoid ICE #86756 when type error recovery goes awry.
                                return Ty::new_misc_error(tcx).into();
                            }
                            tcx.at(self.span).type_of(param.def_id).instantiate(tcx, args).into()
                        } else if infer_args {
                            self.lowerer.ty_infer(Some(param), self.span).into()
                        } else {
                            // We've already errored above about the mismatch.
                            Ty::new_misc_error(tcx).into()
                        }
                    }
                    GenericParamDefKind::Const { has_default, .. } => {
                        let ty = tcx
                            .at(self.span)
                            .type_of(param.def_id)
                            .no_bound_vars()
                            .expect("const parameter types cannot be generic");
                        if let Err(guar) = ty.error_reported() {
                            return ty::Const::new_error(tcx, guar, ty).into();
                        }
                        // FIXME(effects) see if we should special case effect params here
                        if !infer_args && has_default {
                            tcx.const_param_default(param.def_id)
                                .instantiate(tcx, args.unwrap())
                                .into()
                        } else {
                            if infer_args {
                                self.lowerer.ct_infer(ty, Some(param), self.span).into()
                            } else {
                                // We've already errored above about the mismatch.
                                ty::Const::new_misc_error(tcx, ty).into()
                            }
                        }
                    }
                }
            }
        }

        let mut args_ctx = GenericArgsCtxt {
            lowerer: self,
            def_id,
            span,
            generic_args: segment.args(),
            inferred_params: vec![],
            infer_args: segment.infer_args,
        };
        if let ty::BoundConstness::Const | ty::BoundConstness::ConstIfConst = constness
            && generics.has_self
            && !tcx.has_attr(def_id, sym::const_trait)
        {
            let e = tcx.dcx().emit_err(crate::errors::ConstBoundForNonConstTrait {
                span,
                modifier: constness.as_str(),
            });
            self.set_tainted_by_errors(e);
            arg_count.correct =
                Err(GenericArgCountMismatch { reported: Some(e), invalid_args: vec![] });
        }
        let args = lower_generic_args(
            tcx,
            def_id,
            parent_args,
            self_ty.is_some(),
            self_ty,
            &arg_count,
            &mut args_ctx,
        );

        (args, arg_count)
    }

    #[instrument(level = "debug", skip_all)]
    pub fn lower_generic_args_of_assoc_item(
        &self,
        span: Span,
        item_def_id: DefId,
        item_segment: &hir::PathSegment<'tcx>,
        parent_args: GenericArgsRef<'tcx>,
    ) -> GenericArgsRef<'tcx> {
        debug!(?span, ?item_def_id, ?item_segment);
        let (args, _) = self.lower_generic_args_of_path(
            span,
            item_def_id,
            parent_args,
            item_segment,
            None,
            ty::BoundConstness::NotConst,
        );
        if let Some(b) = item_segment.args().bindings.first() {
            prohibit_assoc_item_binding(self.tcx(), b.span, Some((item_segment, span)));
        }
        args
    }

    /// Lower a trait reference as found in an impl header as the implementee.
    ///
    /// The self type `self_ty` is the implementer of the trait.
    pub fn lower_impl_trait_ref(
        &self,
        trait_ref: &hir::TraitRef<'tcx>,
        self_ty: Ty<'tcx>,
    ) -> ty::TraitRef<'tcx> {
        self.prohibit_generic_args(trait_ref.path.segments.split_last().unwrap().1.iter(), |_| {});

        self.lower_mono_trait_ref(
            trait_ref.path.span,
            trait_ref.trait_def_id().unwrap_or_else(|| FatalError.raise()),
            self_ty,
            trait_ref.path.segments.last().unwrap(),
            true,
            ty::BoundConstness::NotConst,
        )
    }

    /// Lower a polymorphic trait reference given a self type into `bounds`.
    ///
    /// *Polymorphic* in the sense that it may bind late-bound vars.
    ///
    /// This may generate auxiliary bounds if the trait reference contains associated item bindings.
    ///
    /// ### Example
    ///
    /// Given the trait ref `Iterator<Item = u32>` and the self type `Ty`, this will add the
    ///
    /// 1. *trait predicate* `<Ty as Iterator>` (known as `Foo: Iterator` in surface syntax) and the
    /// 2. *projection predicate* `<Ty as Iterator>::Item = u32`
    ///
    /// to `bounds`.
    ///
    /// ### A Note on Binders
    ///
    /// Against our usual convention, there is an implied binder around the `self_ty` and the
    /// `trait_ref` here. So they may reference late-bound vars.
    ///
    /// If for example you had `for<'a> Foo<'a>: Bar<'a>`, then the `self_ty` would be `Foo<'a>`
    /// where `'a` is a bound region at depth 0. Similarly, the `trait_ref` would be `Bar<'a>`.
    /// The lowered poly-trait-ref will track this binder explicitly, however.
    #[instrument(level = "debug", skip(self, span, constness, bounds))]
    pub(crate) fn lower_poly_trait_ref(
        &self,
        trait_ref: &hir::TraitRef<'tcx>,
        span: Span,
        constness: ty::BoundConstness,
        polarity: ty::PredicatePolarity,
        self_ty: Ty<'tcx>,
        bounds: &mut Bounds<'tcx>,
        only_self_bounds: OnlySelfBounds,
    ) -> GenericArgCountResult {
        let trait_def_id = trait_ref.trait_def_id().unwrap_or_else(|| FatalError.raise());
        let trait_segment = trait_ref.path.segments.last().unwrap();

        self.prohibit_generic_args(trait_ref.path.segments.split_last().unwrap().1.iter(), |_| {});
        self.complain_about_internal_fn_trait(span, trait_def_id, trait_segment, false);

        let (generic_args, arg_count) = self.lower_generic_args_of_path(
            trait_ref.path.span,
            trait_def_id,
            &[],
            trait_segment,
            Some(self_ty),
            constness,
        );

        let tcx = self.tcx();
        let bound_vars = tcx.late_bound_vars(trait_ref.hir_ref_id);
        debug!(?bound_vars);

        let poly_trait_ref = ty::Binder::bind_with_vars(
            ty::TraitRef::new(tcx, trait_def_id, generic_args),
            bound_vars,
        );

        debug!(?poly_trait_ref);
        bounds.push_trait_bound(tcx, poly_trait_ref, span, polarity);

        let mut dup_bindings = FxIndexMap::default();
        for binding in trait_segment.args().bindings {
            // Don't register additional associated type bounds for negative bounds,
            // since we should have emitten an error for them earlier, and they will
            // not be well-formed!
            if polarity != ty::PredicatePolarity::Positive {
                assert!(
                    self.tcx().dcx().has_errors().is_some(),
                    "negative trait bounds should not have bindings",
                );
                continue;
            }

            // Specify type to assert that error was already reported in `Err` case.
            let _: Result<_, ErrorGuaranteed> = self.lower_assoc_item_binding(
                trait_ref.hir_ref_id,
                poly_trait_ref,
                binding,
                bounds,
                &mut dup_bindings,
                binding.span,
                only_self_bounds,
            );
            // Okay to ignore `Err` because of `ErrorGuaranteed` (see above).
        }

        arg_count
    }

    /// Lower a monomorphic trait reference given a self type while prohibiting associated item bindings.
    ///
    /// *Monomorphic* in the sense that it doesn't bind any late-bound vars.
    fn lower_mono_trait_ref(
        &self,
        span: Span,
        trait_def_id: DefId,
        self_ty: Ty<'tcx>,
        trait_segment: &hir::PathSegment<'tcx>,
        is_impl: bool,
        // FIXME(effects): Move all host param things in HIR ty lowering to AST lowering.
        constness: ty::BoundConstness,
    ) -> ty::TraitRef<'tcx> {
        self.complain_about_internal_fn_trait(span, trait_def_id, trait_segment, is_impl);

        let (generic_args, _) = self.lower_generic_args_of_path(
            span,
            trait_def_id,
            &[],
            trait_segment,
            Some(self_ty),
            constness,
        );
        if let Some(b) = trait_segment.args().bindings.first() {
            prohibit_assoc_item_binding(self.tcx(), b.span, Some((trait_segment, span)));
        }
        ty::TraitRef::new(self.tcx(), trait_def_id, generic_args)
    }

    fn probe_trait_that_defines_assoc_item(
        &self,
        trait_def_id: DefId,
        assoc_kind: ty::AssocKind,
        assoc_name: Ident,
    ) -> bool {
        self.tcx()
            .associated_items(trait_def_id)
            .find_by_name_and_kind(self.tcx(), assoc_name, assoc_kind, trait_def_id)
            .is_some()
    }

    fn lower_path_segment(
        &self,
        span: Span,
        did: DefId,
        item_segment: &hir::PathSegment<'tcx>,
    ) -> Ty<'tcx> {
        let tcx = self.tcx();
        let args = self.lower_generic_args_of_path_segment(span, did, item_segment);

        if let DefKind::TyAlias = tcx.def_kind(did)
            && tcx.type_alias_is_lazy(did)
        {
            // Type aliases defined in crates that have the
            // feature `lazy_type_alias` enabled get encoded as a type alias that normalization will
            // then actually instantiate the where bounds of.
            let alias_ty = ty::AliasTy::new(tcx, did, args);
            Ty::new_alias(tcx, ty::Weak, alias_ty)
        } else {
            tcx.at(span).type_of(did).instantiate(tcx, args)
        }
    }

    /// Search for a trait bound on a type parameter whose trait defines the associated type given by `assoc_name`.
    ///
    /// This fails if there is no such bound in the list of candidates or if there are multiple
    /// candidates in which case it reports ambiguity.
    ///
    /// `ty_param_def_id` is the `LocalDefId` of the type parameter.
    #[instrument(level = "debug", skip_all, ret)]
    fn probe_single_ty_param_bound_for_assoc_ty(
        &self,
        ty_param_def_id: LocalDefId,
        assoc_name: Ident,
        span: Span,
    ) -> Result<ty::PolyTraitRef<'tcx>, ErrorGuaranteed> {
        debug!(?ty_param_def_id, ?assoc_name, ?span);
        let tcx = self.tcx();

        let predicates = &self.probe_ty_param_bounds(span, ty_param_def_id, assoc_name).predicates;
        debug!("predicates={:#?}", predicates);

        let param_name = tcx.hir().ty_param_name(ty_param_def_id);
        self.probe_single_bound_for_assoc_item(
            || {
                traits::transitive_bounds_that_define_assoc_item(
                    tcx,
                    predicates
                        .iter()
                        .filter_map(|(p, _)| Some(p.as_trait_clause()?.map_bound(|t| t.trait_ref))),
                    assoc_name,
                )
            },
            param_name,
            Some(ty_param_def_id),
            ty::AssocKind::Type,
            assoc_name,
            span,
            None,
        )
    }

    /// Search for a single trait bound whose trait defines the associated item given by `assoc_name`.
    ///
    /// This fails if there is no such bound in the list of candidates or if there are multiple
    /// candidates in which case it reports ambiguity.
    #[instrument(level = "debug", skip(self, all_candidates, ty_param_name, binding), ret)]
    fn probe_single_bound_for_assoc_item<I>(
        &self,
        all_candidates: impl Fn() -> I,
        ty_param_name: impl Display,
        ty_param_def_id: Option<LocalDefId>,
        assoc_kind: ty::AssocKind,
        assoc_name: Ident,
        span: Span,
        binding: Option<&hir::TypeBinding<'tcx>>,
    ) -> Result<ty::PolyTraitRef<'tcx>, ErrorGuaranteed>
    where
        I: Iterator<Item = ty::PolyTraitRef<'tcx>>,
    {
        let tcx = self.tcx();

        let mut matching_candidates = all_candidates().filter(|r| {
            self.probe_trait_that_defines_assoc_item(r.def_id(), assoc_kind, assoc_name)
        });

        let Some(bound) = matching_candidates.next() else {
            let reported = self.complain_about_assoc_item_not_found(
                all_candidates,
                &ty_param_name.to_string(),
                ty_param_def_id,
                assoc_kind,
                assoc_name,
                span,
                binding,
            );
            self.set_tainted_by_errors(reported);
            return Err(reported);
        };
        debug!(?bound);

        if let Some(bound2) = matching_candidates.next() {
            debug!(?bound2);

            let assoc_kind_str = assoc_kind_str(assoc_kind);
            let ty_param_name = &ty_param_name.to_string();
            let mut err = tcx.dcx().create_err(crate::errors::AmbiguousAssocItem {
                span,
                assoc_kind: assoc_kind_str,
                assoc_name,
                ty_param_name,
            });
            // Provide a more specific error code index entry for equality bindings.
            err.code(
                if let Some(binding) = binding
                    && let hir::TypeBindingKind::Equality { .. } = binding.kind
                {
                    E0222
                } else {
                    E0221
                },
            );

            // FIXME(#97583): Resugar equality bounds to type/const bindings.
            // FIXME: Turn this into a structured, translateable & more actionable suggestion.
            let mut where_bounds = vec![];
            for bound in [bound, bound2].into_iter().chain(matching_candidates) {
                let bound_id = bound.def_id();
                let bound_span = tcx
                    .associated_items(bound_id)
                    .find_by_name_and_kind(tcx, assoc_name, assoc_kind, bound_id)
                    .and_then(|item| tcx.hir().span_if_local(item.def_id));

                if let Some(bound_span) = bound_span {
                    err.span_label(
                        bound_span,
                        format!("ambiguous `{assoc_name}` from `{}`", bound.print_trait_sugared(),),
                    );
                    if let Some(binding) = binding {
                        match binding.kind {
                            hir::TypeBindingKind::Equality { term } => {
                                let term: ty::Term<'_> = match term {
                                    hir::Term::Ty(ty) => self.lower_ty(ty).into(),
                                    hir::Term::Const(ct) => {
                                        ty::Const::from_anon_const(tcx, ct.def_id).into()
                                    }
                                };
                                // FIXME(#97583): This isn't syntactically well-formed!
                                where_bounds.push(format!(
                                    "        T: {trait}::{assoc_name} = {term}",
                                    trait = bound.print_only_trait_path(),
                                ));
                            }
                            // FIXME: Provide a suggestion.
                            hir::TypeBindingKind::Constraint { bounds: _ } => {}
                        }
                    } else {
                        err.span_suggestion_verbose(
                            span.with_hi(assoc_name.span.lo()),
                            "use fully-qualified syntax to disambiguate",
                            format!("<{ty_param_name} as {}>::", bound.print_only_trait_path()),
                            Applicability::MaybeIncorrect,
                        );
                    }
                } else {
                    err.note(format!(
                        "associated {assoc_kind_str} `{assoc_name}` could derive from `{}`",
                        bound.print_only_trait_path(),
                    ));
                }
            }
            if !where_bounds.is_empty() {
                err.help(format!(
                    "consider introducing a new type parameter `T` and adding `where` constraints:\
                     \n    where\n        T: {ty_param_name},\n{}",
                    where_bounds.join(",\n"),
                ));
            }
            let reported = err.emit();
            self.set_tainted_by_errors(reported);
            if !where_bounds.is_empty() {
                return Err(reported);
            }
        }

        Ok(bound)
    }

    /// Lower a [type-relative] path referring to an associated type or to an enum variant.
    ///
    /// If the path refers to an enum variant and `permit_variants` holds,
    /// the returned type is simply the provided self type `qself_ty`.
    ///
    /// A path like `A::B::C::D` is understood as `<A::B::C>::D`. I.e.,
    /// `qself_ty` / `qself` is `A::B::C` and `assoc_segment` is `D`.
    /// We return the lowered type and the `DefId` for the whole path.
    ///
    /// We only support associated type paths whose self type is a type parameter or a `Self`
    /// type alias (in a trait impl) like `T::Ty` (where `T` is a ty param) or `Self::Ty`.
    /// We **don't** support paths whose self type is an arbitrary type like `Struct::Ty` where
    /// struct `Struct` impls an in-scope trait that defines an associated type called `Ty`.
    /// For the latter case, we report ambiguity.
    /// While desirable to support, the implemention would be non-trivial. Tracked in [#22519].
    ///
    /// At the time of writing, *inherent associated types* are also resolved here. This however
    /// is [problematic][iat]. A proper implementation would be as non-trivial as the one
    /// described in the previous paragraph and their modeling of projections would likely be
    /// very similar in nature.
    ///
    /// [type-relative]: hir::QPath::TypeRelative
    /// [#22519]: https://github.com/rust-lang/rust/issues/22519
    /// [iat]: https://github.com/rust-lang/rust/issues/8995#issuecomment-1569208403
    //
    // NOTE: When this function starts resolving `Trait::AssocTy` successfully
    // it should also start reporting the `BARE_TRAIT_OBJECTS` lint.
    #[instrument(level = "debug", skip_all, ret)]
    pub fn lower_assoc_path(
        &self,
        hir_ref_id: hir::HirId,
        span: Span,
        qself_ty: Ty<'tcx>,
        qself: &hir::Ty<'_>,
        assoc_segment: &hir::PathSegment<'tcx>,
        permit_variants: bool,
    ) -> Result<(Ty<'tcx>, DefKind, DefId), ErrorGuaranteed> {
        debug!(%qself_ty, ?assoc_segment.ident);
        let tcx = self.tcx();

        let assoc_ident = assoc_segment.ident;
        let qself_res = if let hir::TyKind::Path(hir::QPath::Resolved(_, path)) = &qself.kind {
            path.res
        } else {
            Res::Err
        };

        // Check if we have an enum variant or an inherent associated type.
        let mut variant_resolution = None;
        if let Some(adt_def) = self.probe_adt(span, qself_ty) {
            if adt_def.is_enum() {
                let variant_def = adt_def
                    .variants()
                    .iter()
                    .find(|vd| tcx.hygienic_eq(assoc_ident, vd.ident(tcx), adt_def.did()));
                if let Some(variant_def) = variant_def {
                    if permit_variants {
                        tcx.check_stability(variant_def.def_id, Some(hir_ref_id), span, None);
                        self.prohibit_generic_args(slice::from_ref(assoc_segment).iter(), |err| {
                            err.note("enum variants can't have type parameters");
                            let type_name = tcx.item_name(adt_def.did());
                            let msg = format!(
                                "you might have meant to specify type parameters on enum \
                                 `{type_name}`"
                            );
                            let Some(args) = assoc_segment.args else {
                                return;
                            };
                            // Get the span of the generics args *including* the leading `::`.
                            // We do so by stretching args.span_ext to the left by 2. Earlier
                            // it was done based on the end of assoc segment but that sometimes
                            // led to impossible spans and caused issues like #116473
                            let args_span = args.span_ext.with_lo(args.span_ext.lo() - BytePos(2));
                            if tcx.generics_of(adt_def.did()).count() == 0 {
                                // FIXME(estebank): we could also verify that the arguments being
                                // work for the `enum`, instead of just looking if it takes *any*.
                                err.span_suggestion_verbose(
                                    args_span,
                                    format!("{type_name} doesn't have generic parameters"),
                                    "",
                                    Applicability::MachineApplicable,
                                );
                                return;
                            }
                            let Ok(snippet) = tcx.sess.source_map().span_to_snippet(args_span)
                            else {
                                err.note(msg);
                                return;
                            };
                            let (qself_sugg_span, is_self) =
                                if let hir::TyKind::Path(hir::QPath::Resolved(_, path)) =
                                    &qself.kind
                                {
                                    // If the path segment already has type params, we want to overwrite
                                    // them.
                                    match &path.segments {
                                        // `segment` is the previous to last element on the path,
                                        // which would normally be the `enum` itself, while the last
                                        // `_` `PathSegment` corresponds to the variant.
                                        [
                                            ..,
                                            hir::PathSegment {
                                                ident,
                                                args,
                                                res: Res::Def(DefKind::Enum, _),
                                                ..
                                            },
                                            _,
                                        ] => (
                                            // We need to include the `::` in `Type::Variant::<Args>`
                                            // to point the span to `::<Args>`, not just `<Args>`.
                                            ident.span.shrink_to_hi().to(args
                                                .map_or(ident.span.shrink_to_hi(), |a| a.span_ext)),
                                            false,
                                        ),
                                        [segment] => (
                                            // We need to include the `::` in `Type::Variant::<Args>`
                                            // to point the span to `::<Args>`, not just `<Args>`.
                                            segment.ident.span.shrink_to_hi().to(segment
                                                .args
                                                .map_or(segment.ident.span.shrink_to_hi(), |a| {
                                                    a.span_ext
                                                })),
                                            kw::SelfUpper == segment.ident.name,
                                        ),
                                        _ => {
                                            err.note(msg);
                                            return;
                                        }
                                    }
                                } else {
                                    err.note(msg);
                                    return;
                                };
                            let suggestion = vec![
                                if is_self {
                                    // Account for people writing `Self::Variant::<Args>`, where
                                    // `Self` is the enum, and suggest replacing `Self` with the
                                    // appropriate type: `Type::<Args>::Variant`.
                                    (qself.span, format!("{type_name}{snippet}"))
                                } else {
                                    (qself_sugg_span, snippet)
                                },
                                (args_span, String::new()),
                            ];
                            err.multipart_suggestion_verbose(
                                msg,
                                suggestion,
                                Applicability::MaybeIncorrect,
                            );
                        });
                        return Ok((qself_ty, DefKind::Variant, variant_def.def_id));
                    } else {
                        variant_resolution = Some(variant_def.def_id);
                    }
                }
            }

            // FIXME(inherent_associated_types, #106719): Support self types other than ADTs.
            if let Some((ty, did)) = self.probe_inherent_assoc_ty(
                assoc_ident,
                assoc_segment,
                adt_def.did(),
                qself_ty,
                hir_ref_id,
                span,
            )? {
                return Ok((ty, DefKind::AssocTy, did));
            }
        }

        // Find the type of the associated item, and the trait where the associated
        // item is declared.
        let bound = match (&qself_ty.kind(), qself_res) {
            (_, Res::SelfTyAlias { alias_to: impl_def_id, is_trait_impl: true, .. }) => {
                // `Self` in an impl of a trait -- we have a concrete self type and a
                // trait reference.
                let Some(trait_ref) = tcx.impl_trait_ref(impl_def_id) else {
                    // A cycle error occurred, most likely.
                    tcx.dcx().span_bug(span, "expected cycle error");
                };

                self.probe_single_bound_for_assoc_item(
                    || {
                        traits::supertraits(
                            tcx,
                            ty::Binder::dummy(trait_ref.instantiate_identity()),
                        )
                    },
                    kw::SelfUpper,
                    None,
                    ty::AssocKind::Type,
                    assoc_ident,
                    span,
                    None,
                )?
            }
            (
                &ty::Param(_),
                Res::SelfTyParam { trait_: param_did } | Res::Def(DefKind::TyParam, param_did),
            ) => self.probe_single_ty_param_bound_for_assoc_ty(
                param_did.expect_local(),
                assoc_ident,
                span,
            )?,
            _ => {
                let reported = if variant_resolution.is_some() {
                    // Variant in type position
                    let msg = format!("expected type, found variant `{assoc_ident}`");
                    tcx.dcx().span_err(span, msg)
                } else if qself_ty.is_enum() {
                    let mut err = struct_span_code_err!(
                        tcx.dcx(),
                        assoc_ident.span,
                        E0599,
                        "no variant named `{}` found for enum `{}`",
                        assoc_ident,
                        qself_ty,
                    );

                    let adt_def = qself_ty.ty_adt_def().expect("enum is not an ADT");
                    if let Some(suggested_name) = find_best_match_for_name(
                        &adt_def
                            .variants()
                            .iter()
                            .map(|variant| variant.name)
                            .collect::<Vec<Symbol>>(),
                        assoc_ident.name,
                        None,
                    ) {
                        err.span_suggestion(
                            assoc_ident.span,
                            "there is a variant with a similar name",
                            suggested_name,
                            Applicability::MaybeIncorrect,
                        );
                    } else {
                        err.span_label(
                            assoc_ident.span,
                            format!("variant not found in `{qself_ty}`"),
                        );
                    }

                    if let Some(sp) = tcx.hir().span_if_local(adt_def.did()) {
                        err.span_label(sp, format!("variant `{assoc_ident}` not found here"));
                    }

                    err.emit()
                } else if let Err(reported) = qself_ty.error_reported() {
                    reported
                } else if let ty::Alias(ty::Opaque, alias_ty) = qself_ty.kind() {
                    // `<impl Trait as OtherTrait>::Assoc` makes no sense.
                    struct_span_code_err!(
                        tcx.dcx(),
                        tcx.def_span(alias_ty.def_id),
                        E0667,
                        "`impl Trait` is not allowed in path parameters"
                    )
                    .emit() // Already reported in an earlier stage.
                } else {
                    self.maybe_report_similar_assoc_fn(span, qself_ty, qself)?;

                    let traits: Vec<_> =
                        self.probe_traits_that_match_assoc_ty(qself_ty, assoc_ident);

                    // Don't print `ty::Error` to the user.
                    self.report_ambiguous_assoc_ty(
                        span,
                        &[qself_ty.to_string()],
                        &traits,
                        assoc_ident.name,
                    )
                };
                self.set_tainted_by_errors(reported);
                return Err(reported);
            }
        };

        let trait_did = bound.def_id();
        let assoc_ty_did = self.probe_assoc_ty(assoc_ident, hir_ref_id, span, trait_did).unwrap();
        let ty = self.lower_assoc_ty(span, assoc_ty_did, assoc_segment, bound);

        if let Some(variant_def_id) = variant_resolution {
            tcx.node_span_lint(
                AMBIGUOUS_ASSOCIATED_ITEMS,
                hir_ref_id,
                span,
                "ambiguous associated item",
                |lint| {
                    let mut could_refer_to = |kind: DefKind, def_id, also| {
                        let note_msg = format!(
                            "`{}` could{} refer to the {} defined here",
                            assoc_ident,
                            also,
                            tcx.def_kind_descr(kind, def_id)
                        );
                        lint.span_note(tcx.def_span(def_id), note_msg);
                    };

                    could_refer_to(DefKind::Variant, variant_def_id, "");
                    could_refer_to(DefKind::AssocTy, assoc_ty_did, " also");

                    lint.span_suggestion(
                        span,
                        "use fully-qualified syntax",
                        format!("<{} as {}>::{}", qself_ty, tcx.item_name(trait_did), assoc_ident),
                        Applicability::MachineApplicable,
                    );
                },
            );
        }
        Ok((ty, DefKind::AssocTy, assoc_ty_did))
    }

    fn probe_inherent_assoc_ty(
        &self,
        name: Ident,
        segment: &hir::PathSegment<'tcx>,
        adt_did: DefId,
        self_ty: Ty<'tcx>,
        block: hir::HirId,
        span: Span,
    ) -> Result<Option<(Ty<'tcx>, DefId)>, ErrorGuaranteed> {
        let tcx = self.tcx();

        // Don't attempt to look up inherent associated types when the feature is not enabled.
        // Theoretically it'd be fine to do so since we feature-gate their definition site.
        // However, due to current limitations of the implementation (caused by us performing
        // selection during HIR ty lowering instead of in the trait solver), IATs can lead to cycle
        // errors (#108491) which mask the feature-gate error, needlessly confusing users
        // who use IATs by accident (#113265).
        if !tcx.features().inherent_associated_types {
            return Ok(None);
        }

        let candidates: Vec<_> = tcx
            .inherent_impls(adt_did)?
            .iter()
            .filter_map(|&impl_| Some((impl_, self.probe_assoc_ty_unchecked(name, block, impl_)?)))
            .collect();

        if candidates.is_empty() {
            return Ok(None);
        }

        //
        // Select applicable inherent associated type candidates modulo regions.
        //

        // In contexts that have no inference context, just make a new one.
        // We do need a local variable to store it, though.
        let infcx_;
        let infcx = match self.infcx() {
            Some(infcx) => infcx,
            None => {
                assert!(!self_ty.has_infer());
                infcx_ = tcx.infer_ctxt().ignoring_regions().build();
                &infcx_
            }
        };

        // FIXME(inherent_associated_types): Acquiring the ParamEnv this early leads to cycle errors
        // when inside of an ADT (#108491) or where clause.
        let param_env = tcx.param_env(block.owner);

        let mut universes = if self_ty.has_escaping_bound_vars() {
            vec![None; self_ty.outer_exclusive_binder().as_usize()]
        } else {
            vec![]
        };

        let (impl_, (assoc_item, def_scope)) = crate::traits::with_replaced_escaping_bound_vars(
            infcx,
            &mut universes,
            self_ty,
            |self_ty| {
                self.select_inherent_assoc_type_candidates(
                    infcx, name, span, self_ty, param_env, candidates,
                )
            },
        )?;

        self.check_assoc_ty(assoc_item, name, def_scope, block, span);

        // FIXME(fmease): Currently creating throwaway `parent_args` to please
        // `lower_generic_args_of_assoc_item`. Modify the latter instead (or sth. similar) to
        // not require the parent args logic.
        let parent_args = ty::GenericArgs::identity_for_item(tcx, impl_);
        let args = self.lower_generic_args_of_assoc_item(span, assoc_item, segment, parent_args);
        let args = tcx.mk_args_from_iter(
            std::iter::once(ty::GenericArg::from(self_ty))
                .chain(args.into_iter().skip(parent_args.len())),
        );

        let ty = Ty::new_alias(tcx, ty::Inherent, ty::AliasTy::new(tcx, assoc_item, args));

        Ok(Some((ty, assoc_item)))
    }

    fn select_inherent_assoc_type_candidates(
        &self,
        infcx: &InferCtxt<'tcx>,
        name: Ident,
        span: Span,
        self_ty: Ty<'tcx>,
        param_env: ParamEnv<'tcx>,
        candidates: Vec<(DefId, (DefId, DefId))>,
    ) -> Result<(DefId, (DefId, DefId)), ErrorGuaranteed> {
        let tcx = self.tcx();
        let mut fulfillment_errors = Vec::new();

        let applicable_candidates: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|&(impl_, _)| {
                infcx.probe(|_| {
                    let ocx = ObligationCtxt::new(infcx);
                    let self_ty = ocx.normalize(&ObligationCause::dummy(), param_env, self_ty);

                    let impl_args = infcx.fresh_args_for_item(span, impl_);
                    let impl_ty = tcx.type_of(impl_).instantiate(tcx, impl_args);
                    let impl_ty = ocx.normalize(&ObligationCause::dummy(), param_env, impl_ty);

                    // Check that the self types can be related.
                    if ocx.eq(&ObligationCause::dummy(), param_env, impl_ty, self_ty).is_err() {
                        return false;
                    }

                    // Check whether the impl imposes obligations we have to worry about.
                    let impl_bounds = tcx.predicates_of(impl_).instantiate(tcx, impl_args);
                    let impl_bounds =
                        ocx.normalize(&ObligationCause::dummy(), param_env, impl_bounds);
                    let impl_obligations = traits::predicates_for_generics(
                        |_, _| ObligationCause::dummy(),
                        param_env,
                        impl_bounds,
                    );
                    ocx.register_obligations(impl_obligations);

                    let mut errors = ocx.select_where_possible();
                    if !errors.is_empty() {
                        fulfillment_errors.append(&mut errors);
                        return false;
                    }

                    true
                })
            })
            .collect();

        match &applicable_candidates[..] {
            &[] => Err(self.complain_about_inherent_assoc_ty_not_found(
                name,
                self_ty,
                candidates,
                fulfillment_errors,
                span,
            )),

            &[applicable_candidate] => Ok(applicable_candidate),

            &[_, ..] => Err(self.complain_about_ambiguous_inherent_assoc_ty(
                name,
                applicable_candidates.into_iter().map(|(_, (candidate, _))| candidate).collect(),
                span,
            )),
        }
    }

    fn probe_assoc_ty(
        &self,
        name: Ident,
        block: hir::HirId,
        span: Span,
        scope: DefId,
    ) -> Option<DefId> {
        let (item, def_scope) = self.probe_assoc_ty_unchecked(name, block, scope)?;
        self.check_assoc_ty(item, name, def_scope, block, span);
        Some(item)
    }

    fn probe_assoc_ty_unchecked(
        &self,
        name: Ident,
        block: hir::HirId,
        scope: DefId,
    ) -> Option<(DefId, DefId)> {
        let tcx = self.tcx();
        let (ident, def_scope) = tcx.adjust_ident_and_get_scope(name, scope, block);

        // We have already adjusted the item name above, so compare with `.normalize_to_macros_2_0()`
        // instead of calling `filter_by_name_and_kind` which would needlessly normalize the
        // `ident` again and again.
        let item = tcx.associated_items(scope).in_definition_order().find(|i| {
            i.kind.namespace() == Namespace::TypeNS
                && i.ident(tcx).normalize_to_macros_2_0() == ident
        })?;

        Some((item.def_id, def_scope))
    }

    fn check_assoc_ty(
        &self,
        item: DefId,
        name: Ident,
        def_scope: DefId,
        block: hir::HirId,
        span: Span,
    ) {
        let tcx = self.tcx();
        let kind = DefKind::AssocTy;

        if !tcx.visibility(item).is_accessible_from(def_scope, tcx) {
            let kind = tcx.def_kind_descr(kind, item);
            let msg = format!("{kind} `{name}` is private");
            let def_span = tcx.def_span(item);
            let reported = tcx
                .dcx()
                .struct_span_err(span, msg)
                .with_code(E0624)
                .with_span_label(span, format!("private {kind}"))
                .with_span_label(def_span, format!("{kind} defined here"))
                .emit();
            self.set_tainted_by_errors(reported);
        }
        tcx.check_stability(item, Some(block), span, None);
    }

    fn probe_traits_that_match_assoc_ty(
        &self,
        qself_ty: Ty<'tcx>,
        assoc_ident: Ident,
    ) -> Vec<String> {
        let tcx = self.tcx();

        // In contexts that have no inference context, just make a new one.
        // We do need a local variable to store it, though.
        let infcx_;
        let infcx = if let Some(infcx) = self.infcx() {
            infcx
        } else {
            assert!(!qself_ty.has_infer());
            infcx_ = tcx.infer_ctxt().build();
            &infcx_
        };

        tcx.all_traits()
            .filter(|trait_def_id| {
                // Consider only traits with the associated type
                tcx.associated_items(*trait_def_id)
                        .in_definition_order()
                        .any(|i| {
                            i.kind.namespace() == Namespace::TypeNS
                                && i.ident(tcx).normalize_to_macros_2_0() == assoc_ident
                                && matches!(i.kind, ty::AssocKind::Type)
                        })
                    // Consider only accessible traits
                    && tcx.visibility(*trait_def_id)
                        .is_accessible_from(self.item_def_id(), tcx)
                    && tcx.all_impls(*trait_def_id)
                        .any(|impl_def_id| {
                            let impl_header = tcx.impl_trait_header(impl_def_id);
                            impl_header.is_some_and(|header| {
                                let trait_ref = header.trait_ref.instantiate(
                                    tcx,
                                    infcx.fresh_args_for_item(DUMMY_SP, impl_def_id),
                                );

                                let value = tcx.fold_regions(qself_ty, |_, _| tcx.lifetimes.re_erased);
                                // FIXME: Don't bother dealing with non-lifetime binders here...
                                if value.has_escaping_bound_vars() {
                                    return false;
                                }
                                infcx
                                    .can_eq(
                                        ty::ParamEnv::empty(),
                                        trait_ref.self_ty(),
                                        value,
                                    ) && header.polarity != ty::ImplPolarity::Negative
                            })
                        })
            })
            .map(|trait_def_id| tcx.def_path_str(trait_def_id))
            .collect()
    }

    /// Lower a qualified path to a type.
    #[instrument(level = "debug", skip_all)]
    fn lower_qpath(
        &self,
        span: Span,
        opt_self_ty: Option<Ty<'tcx>>,
        item_def_id: DefId,
        trait_segment: &hir::PathSegment<'tcx>,
        item_segment: &hir::PathSegment<'tcx>,
        constness: ty::BoundConstness,
    ) -> Ty<'tcx> {
        let tcx = self.tcx();

        let trait_def_id = tcx.parent(item_def_id);
        debug!(?trait_def_id);

        let Some(self_ty) = opt_self_ty else {
            let path_str = tcx.def_path_str(trait_def_id);

            let def_id = self.item_def_id();
            debug!(item_def_id = ?def_id);

            let parent_def_id = def_id
                .as_local()
                .map(|def_id| tcx.local_def_id_to_hir_id(def_id))
                .map(|hir_id| tcx.hir().get_parent_item(hir_id).to_def_id());
            debug!(?parent_def_id);

            // If the trait in segment is the same as the trait defining the item,
            // use the `<Self as ..>` syntax in the error.
            let is_part_of_self_trait_constraints = def_id == trait_def_id;
            let is_part_of_fn_in_self_trait = parent_def_id == Some(trait_def_id);

            let type_names = if is_part_of_self_trait_constraints || is_part_of_fn_in_self_trait {
                vec!["Self".to_string()]
            } else {
                // Find all the types that have an `impl` for the trait.
                tcx.all_impls(trait_def_id)
                    .filter_map(|impl_def_id| tcx.impl_trait_header(impl_def_id))
                    .filter(|header| {
                        // Consider only accessible traits
                        tcx.visibility(trait_def_id).is_accessible_from(self.item_def_id(), tcx)
                            && header.polarity != ty::ImplPolarity::Negative
                    })
                    .map(|header| header.trait_ref.instantiate_identity().self_ty())
                    // We don't care about blanket impls.
                    .filter(|self_ty| !self_ty.has_non_region_param())
                    .map(|self_ty| tcx.erase_regions(self_ty).to_string())
                    .collect()
            };
            // FIXME: also look at `tcx.generics_of(self.item_def_id()).params` any that
            // references the trait. Relevant for the first case in
            // `src/test/ui/associated-types/associated-types-in-ambiguous-context.rs`
            let reported = self.report_ambiguous_assoc_ty(
                span,
                &type_names,
                &[path_str],
                item_segment.ident.name,
            );
            return Ty::new_error(tcx, reported);
        };
        debug!(?self_ty);

        let trait_ref =
            self.lower_mono_trait_ref(span, trait_def_id, self_ty, trait_segment, false, constness);
        debug!(?trait_ref);

        let item_args =
            self.lower_generic_args_of_assoc_item(span, item_def_id, item_segment, trait_ref.args);

        Ty::new_projection(tcx, item_def_id, item_args)
    }

    pub fn prohibit_generic_args<'a>(
        &self,
        segments: impl Iterator<Item = &'a hir::PathSegment<'a>> + Clone,
        extend: impl Fn(&mut Diag<'_>),
    ) -> bool {
        let args = segments.clone().flat_map(|segment| segment.args().args);

        let (lt, ty, ct, inf) =
            args.clone().fold((false, false, false, false), |(lt, ty, ct, inf), arg| match arg {
                hir::GenericArg::Lifetime(_) => (true, ty, ct, inf),
                hir::GenericArg::Type(_) => (lt, true, ct, inf),
                hir::GenericArg::Const(_) => (lt, ty, true, inf),
                hir::GenericArg::Infer(_) => (lt, ty, ct, true),
            });
        let mut emitted = false;
        if lt || ty || ct || inf {
            let types_and_spans: Vec<_> = segments
                .clone()
                .flat_map(|segment| {
                    if segment.args().args.is_empty() {
                        None
                    } else {
                        Some((
                            match segment.res {
                                Res::PrimTy(ty) => {
                                    format!("{} `{}`", segment.res.descr(), ty.name())
                                }
                                Res::Def(_, def_id)
                                    if let Some(name) = self.tcx().opt_item_name(def_id) =>
                                {
                                    format!("{} `{name}`", segment.res.descr())
                                }
                                Res::Err => "this type".to_string(),
                                _ => segment.res.descr().to_string(),
                            },
                            segment.ident.span,
                        ))
                    }
                })
                .collect();
            let this_type = match &types_and_spans[..] {
                [.., _, (last, _)] => format!(
                    "{} and {last}",
                    types_and_spans[..types_and_spans.len() - 1]
                        .iter()
                        .map(|(x, _)| x.as_str())
                        .intersperse(", ")
                        .collect::<String>()
                ),
                [(only, _)] => only.to_string(),
                [] => "this type".to_string(),
            };

            let arg_spans: Vec<Span> = args.map(|arg| arg.span()).collect();

            let mut kinds = Vec::with_capacity(4);
            if lt {
                kinds.push("lifetime");
            }
            if ty {
                kinds.push("type");
            }
            if ct {
                kinds.push("const");
            }
            if inf {
                kinds.push("generic");
            }
            let (kind, s) = match kinds[..] {
                [.., _, last] => (
                    format!(
                        "{} and {last}",
                        kinds[..kinds.len() - 1]
                            .iter()
                            .map(|&x| x)
                            .intersperse(", ")
                            .collect::<String>()
                    ),
                    "s",
                ),
                [only] => (only.to_string(), ""),
                [] => unreachable!("expected at least one generic to prohibit"),
            };
            let last_span = *arg_spans.last().unwrap();
            let span: MultiSpan = arg_spans.into();
            let mut err = struct_span_code_err!(
                self.tcx().dcx(),
                span,
                E0109,
                "{kind} arguments are not allowed on {this_type}",
            );
            err.span_label(last_span, format!("{kind} argument{s} not allowed"));
            for (what, span) in types_and_spans {
                err.span_label(span, format!("not allowed on {what}"));
            }
            extend(&mut err);
            self.set_tainted_by_errors(err.emit());
            emitted = true;
        }

        for segment in segments {
            // Only emit the first error to avoid overloading the user with error messages.
            if let Some(b) = segment.args().bindings.first() {
                prohibit_assoc_item_binding(self.tcx(), b.span, None);
                return true;
            }
        }
        emitted
    }

    /// Probe path segments that are semantically allowed to have generic arguments.
    ///
    /// ### Example
    ///
    /// ```ignore (illustrative)
    ///    Option::None::<()>
    /// //         ^^^^ permitted to have generic args
    ///
    /// // ==> [GenericPathSegment(Option_def_id, 1)]
    ///
    ///    Option::<()>::None
    /// // ^^^^^^        ^^^^ *not* permitted to have generic args
    /// // permitted to have generic args
    ///
    /// // ==> [GenericPathSegment(Option_def_id, 0)]
    /// ```
    // FIXME(eddyb, varkor) handle type paths here too, not just value ones.
    pub fn probe_generic_path_segments(
        &self,
        segments: &[hir::PathSegment<'_>],
        self_ty: Option<Ty<'tcx>>,
        kind: DefKind,
        def_id: DefId,
        span: Span,
    ) -> Vec<GenericPathSegment> {
        // We need to extract the generic arguments supplied by the user in
        // the path `path`. Due to the current setup, this is a bit of a
        // tricky process; the problem is that resolve only tells us the
        // end-point of the path resolution, and not the intermediate steps.
        // Luckily, we can (at least for now) deduce the intermediate steps
        // just from the end-point.
        //
        // There are basically five cases to consider:
        //
        // 1. Reference to a constructor of a struct:
        //
        //        struct Foo<T>(...)
        //
        //    In this case, the generic arguments are declared in the type space.
        //
        // 2. Reference to a constructor of an enum variant:
        //
        //        enum E<T> { Foo(...) }
        //
        //    In this case, the generic arguments are defined in the type space,
        //    but may be specified either on the type or the variant.
        //
        // 3. Reference to a free function or constant:
        //
        //        fn foo<T>() {}
        //
        //    In this case, the path will again always have the form
        //    `a::b::foo::<T>` where only the final segment should have generic
        //    arguments. However, in this case, those arguments are declared on
        //    a value, and hence are in the value space.
        //
        // 4. Reference to an associated function or constant:
        //
        //        impl<A> SomeStruct<A> {
        //            fn foo<B>(...) {}
        //        }
        //
        //    Here we can have a path like `a::b::SomeStruct::<A>::foo::<B>`,
        //    in which case generic arguments may appear in two places. The
        //    penultimate segment, `SomeStruct::<A>`, contains generic arguments
        //    in the type space, and the final segment, `foo::<B>` contains
        //    generic arguments in value space.
        //
        // The first step then is to categorize the segments appropriately.

        let tcx = self.tcx();

        assert!(!segments.is_empty());
        let last = segments.len() - 1;

        let mut generic_segments = vec![];

        match kind {
            // Case 1. Reference to a struct constructor.
            DefKind::Ctor(CtorOf::Struct, ..) => {
                // Everything but the final segment should have no
                // parameters at all.
                let generics = tcx.generics_of(def_id);
                // Variant and struct constructors use the
                // generics of their parent type definition.
                let generics_def_id = generics.parent.unwrap_or(def_id);
                generic_segments.push(GenericPathSegment(generics_def_id, last));
            }

            // Case 2. Reference to a variant constructor.
            DefKind::Ctor(CtorOf::Variant, ..) | DefKind::Variant => {
                let (generics_def_id, index) = if let Some(self_ty) = self_ty {
                    let adt_def = self.probe_adt(span, self_ty).unwrap();
                    debug_assert!(adt_def.is_enum());
                    (adt_def.did(), last)
                } else if last >= 1 && segments[last - 1].args.is_some() {
                    // Everything but the penultimate segment should have no
                    // parameters at all.
                    let mut def_id = def_id;

                    // `DefKind::Ctor` -> `DefKind::Variant`
                    if let DefKind::Ctor(..) = kind {
                        def_id = tcx.parent(def_id);
                    }

                    // `DefKind::Variant` -> `DefKind::Enum`
                    let enum_def_id = tcx.parent(def_id);
                    (enum_def_id, last - 1)
                } else {
                    // FIXME: lint here recommending `Enum::<...>::Variant` form
                    // instead of `Enum::Variant::<...>` form.

                    // Everything but the final segment should have no
                    // parameters at all.
                    let generics = tcx.generics_of(def_id);
                    // Variant and struct constructors use the
                    // generics of their parent type definition.
                    (generics.parent.unwrap_or(def_id), last)
                };
                generic_segments.push(GenericPathSegment(generics_def_id, index));
            }

            // Case 3. Reference to a top-level value.
            DefKind::Fn | DefKind::Const | DefKind::ConstParam | DefKind::Static { .. } => {
                generic_segments.push(GenericPathSegment(def_id, last));
            }

            // Case 4. Reference to a method or associated const.
            DefKind::AssocFn | DefKind::AssocConst => {
                if segments.len() >= 2 {
                    let generics = tcx.generics_of(def_id);
                    generic_segments.push(GenericPathSegment(generics.parent.unwrap(), last - 1));
                }
                generic_segments.push(GenericPathSegment(def_id, last));
            }

            kind => bug!("unexpected definition kind {:?} for {:?}", kind, def_id),
        }

        debug!(?generic_segments);

        generic_segments
    }

    /// Lower a type `Path` to a type.
    #[instrument(level = "debug", skip_all)]
    pub fn lower_path(
        &self,
        opt_self_ty: Option<Ty<'tcx>>,
        path: &hir::Path<'tcx>,
        hir_id: hir::HirId,
        permit_variants: bool,
    ) -> Ty<'tcx> {
        debug!(?path.res, ?opt_self_ty, ?path.segments);
        let tcx = self.tcx();

        let span = path.span;
        match path.res {
            Res::Def(DefKind::OpaqueTy, did) => {
                // Check for desugared `impl Trait`.
                assert!(tcx.is_type_alias_impl_trait(did));
                let item_segment = path.segments.split_last().unwrap();
                self.prohibit_generic_args(item_segment.1.iter(), |err| {
                    err.note("`impl Trait` types can't have type parameters");
                });
                let args = self.lower_generic_args_of_path_segment(span, did, item_segment.0);
                Ty::new_opaque(tcx, did, args)
            }
            Res::Def(
                DefKind::Enum
                | DefKind::TyAlias
                | DefKind::Struct
                | DefKind::Union
                | DefKind::ForeignTy,
                did,
            ) => {
                assert_eq!(opt_self_ty, None);
                self.prohibit_generic_args(path.segments.split_last().unwrap().1.iter(), |_| {});
                self.lower_path_segment(span, did, path.segments.last().unwrap())
            }
            Res::Def(kind @ DefKind::Variant, def_id) if permit_variants => {
                // Lower "variant type" as if it were a real type.
                // The resulting `Ty` is type of the variant's enum for now.
                assert_eq!(opt_self_ty, None);

                let generic_segments =
                    self.probe_generic_path_segments(path.segments, None, kind, def_id, span);
                let indices: FxHashSet<_> =
                    generic_segments.iter().map(|GenericPathSegment(_, index)| index).collect();
                self.prohibit_generic_args(
                    path.segments.iter().enumerate().filter_map(|(index, seg)| {
                        if !indices.contains(&index) { Some(seg) } else { None }
                    }),
                    |err| {
                        err.note("enum variants can't have type parameters");
                    },
                );

                let GenericPathSegment(def_id, index) = generic_segments.last().unwrap();
                self.lower_path_segment(span, *def_id, &path.segments[*index])
            }
            Res::Def(DefKind::TyParam, def_id) => {
                assert_eq!(opt_self_ty, None);
                self.prohibit_generic_args(path.segments.iter(), |err| {
                    if let Some(span) = tcx.def_ident_span(def_id) {
                        let name = tcx.item_name(def_id);
                        err.span_note(span, format!("type parameter `{name}` defined here"));
                    }
                });
                self.lower_ty_param(hir_id)
            }
            Res::SelfTyParam { .. } => {
                // `Self` in trait or type alias.
                assert_eq!(opt_self_ty, None);
                self.prohibit_generic_args(path.segments.iter(), |err| {
                    if let [hir::PathSegment { args: Some(args), ident, .. }] = &path.segments {
                        err.span_suggestion_verbose(
                            ident.span.shrink_to_hi().to(args.span_ext),
                            "the `Self` type doesn't accept type parameters",
                            "",
                            Applicability::MaybeIncorrect,
                        );
                    }
                });
                tcx.types.self_param
            }
            Res::SelfTyAlias { alias_to: def_id, forbid_generic, .. } => {
                // `Self` in impl (we know the concrete type).
                assert_eq!(opt_self_ty, None);
                // Try to evaluate any array length constants.
                let ty = tcx.at(span).type_of(def_id).instantiate_identity();
                let span_of_impl = tcx.span_of_impl(def_id);
                self.prohibit_generic_args(path.segments.iter(), |err| {
                    let def_id = match *ty.kind() {
                        ty::Adt(self_def, _) => self_def.did(),
                        _ => return,
                    };

                    let type_name = tcx.item_name(def_id);
                    let span_of_ty = tcx.def_ident_span(def_id);
                    let generics = tcx.generics_of(def_id).count();

                    let msg = format!("`Self` is of type `{ty}`");
                    if let (Ok(i_sp), Some(t_sp)) = (span_of_impl, span_of_ty) {
                        let mut span: MultiSpan = vec![t_sp].into();
                        span.push_span_label(
                            i_sp,
                            format!("`Self` is on type `{type_name}` in this `impl`"),
                        );
                        let mut postfix = "";
                        if generics == 0 {
                            postfix = ", which doesn't have generic parameters";
                        }
                        span.push_span_label(
                            t_sp,
                            format!("`Self` corresponds to this type{postfix}"),
                        );
                        err.span_note(span, msg);
                    } else {
                        err.note(msg);
                    }
                    for segment in path.segments {
                        if let Some(args) = segment.args
                            && segment.ident.name == kw::SelfUpper
                        {
                            if generics == 0 {
                                // FIXME(estebank): we could also verify that the arguments being
                                // work for the `enum`, instead of just looking if it takes *any*.
                                err.span_suggestion_verbose(
                                    segment.ident.span.shrink_to_hi().to(args.span_ext),
                                    "the `Self` type doesn't accept type parameters",
                                    "",
                                    Applicability::MachineApplicable,
                                );
                                return;
                            } else {
                                err.span_suggestion_verbose(
                                    segment.ident.span,
                                    format!(
                                        "the `Self` type doesn't accept type parameters, use the \
                                        concrete type's name `{type_name}` instead if you want to \
                                        specify its type parameters"
                                    ),
                                    type_name,
                                    Applicability::MaybeIncorrect,
                                );
                            }
                        }
                    }
                });
                // HACK(min_const_generics): Forbid generic `Self` types
                // here as we can't easily do that during nameres.
                //
                // We do this before normalization as we otherwise allow
                // ```rust
                // trait AlwaysApplicable { type Assoc; }
                // impl<T: ?Sized> AlwaysApplicable for T { type Assoc = usize; }
                //
                // trait BindsParam<T> {
                //     type ArrayTy;
                // }
                // impl<T> BindsParam<T> for <T as AlwaysApplicable>::Assoc {
                //    type ArrayTy = [u8; Self::MAX];
                // }
                // ```
                // Note that the normalization happens in the param env of
                // the anon const, which is empty. This is why the
                // `AlwaysApplicable` impl needs a `T: ?Sized` bound for
                // this to compile if we were to normalize here.
                if forbid_generic && ty.has_param() {
                    let mut err = tcx.dcx().struct_span_err(
                        path.span,
                        "generic `Self` types are currently not permitted in anonymous constants",
                    );
                    if let Some(hir::Node::Item(&hir::Item {
                        kind: hir::ItemKind::Impl(impl_),
                        ..
                    })) = tcx.hir().get_if_local(def_id)
                    {
                        err.span_note(impl_.self_ty.span, "not a concrete type");
                    }
                    let reported = err.emit();
                    self.set_tainted_by_errors(reported);
                    Ty::new_error(tcx, reported)
                } else {
                    ty
                }
            }
            Res::Def(DefKind::AssocTy, def_id) => {
                debug_assert!(path.segments.len() >= 2);
                self.prohibit_generic_args(path.segments[..path.segments.len() - 2].iter(), |_| {});
                // HACK: until we support `<Type as ~const Trait>`, assume all of them are.
                let constness = if tcx.has_attr(tcx.parent(def_id), sym::const_trait) {
                    ty::BoundConstness::ConstIfConst
                } else {
                    ty::BoundConstness::NotConst
                };
                self.lower_qpath(
                    span,
                    opt_self_ty,
                    def_id,
                    &path.segments[path.segments.len() - 2],
                    path.segments.last().unwrap(),
                    constness,
                )
            }
            Res::PrimTy(prim_ty) => {
                assert_eq!(opt_self_ty, None);
                self.prohibit_generic_args(path.segments.iter(), |err| {
                    let name = prim_ty.name_str();
                    for segment in path.segments {
                        if let Some(args) = segment.args {
                            err.span_suggestion_verbose(
                                segment.ident.span.shrink_to_hi().to(args.span_ext),
                                format!("primitive type `{name}` doesn't have generic parameters"),
                                "",
                                Applicability::MaybeIncorrect,
                            );
                        }
                    }
                });
                match prim_ty {
                    hir::PrimTy::Bool => tcx.types.bool,
                    hir::PrimTy::Char => tcx.types.char,
                    hir::PrimTy::Int(it) => Ty::new_int(tcx, ty::int_ty(it)),
                    hir::PrimTy::Uint(uit) => Ty::new_uint(tcx, ty::uint_ty(uit)),
                    hir::PrimTy::Float(ft) => Ty::new_float(tcx, ty::float_ty(ft)),
                    hir::PrimTy::Str => tcx.types.str_,
                }
            }
            Res::Err => {
                let e = self
                    .tcx()
                    .dcx()
                    .span_delayed_bug(path.span, "path with `Res::Err` but no error emitted");
                self.set_tainted_by_errors(e);
                Ty::new_error(self.tcx(), e)
            }
            _ => span_bug!(span, "unexpected resolution: {:?}", path.res),
        }
    }

    /// Lower a type parameter from the HIR to our internal notion of a type.
    ///
    /// Early-bound type parameters get lowered to [`ty::Param`]
    /// and late-bound ones to [`ty::Bound`].
    pub(crate) fn lower_ty_param(&self, hir_id: hir::HirId) -> Ty<'tcx> {
        let tcx = self.tcx();
        match tcx.named_bound_var(hir_id) {
            Some(rbv::ResolvedArg::LateBound(debruijn, index, def_id)) => {
                let name = tcx.item_name(def_id);
                let br = ty::BoundTy {
                    var: ty::BoundVar::from_u32(index),
                    kind: ty::BoundTyKind::Param(def_id, name),
                };
                Ty::new_bound(tcx, debruijn, br)
            }
            Some(rbv::ResolvedArg::EarlyBound(def_id)) => {
                let def_id = def_id.expect_local();
                let item_def_id = tcx.hir().ty_param_owner(def_id);
                let generics = tcx.generics_of(item_def_id);
                let index = generics.param_def_id_to_index[&def_id.to_def_id()];
                Ty::new_param(tcx, index, tcx.hir().ty_param_name(def_id))
            }
            Some(rbv::ResolvedArg::Error(guar)) => Ty::new_error(tcx, guar),
            arg => bug!("unexpected bound var resolution for {hir_id:?}: {arg:?}"),
        }
    }

    /// Lower a const parameter from the HIR to our internal notion of a constant.
    ///
    /// Early-bound const parameters get lowered to [`ty::ConstKind::Param`]
    /// and late-bound ones to [`ty::ConstKind::Bound`].
    pub(crate) fn lower_const_param(&self, hir_id: hir::HirId, param_ty: Ty<'tcx>) -> Const<'tcx> {
        let tcx = self.tcx();
        match tcx.named_bound_var(hir_id) {
            Some(rbv::ResolvedArg::EarlyBound(def_id)) => {
                // Find the name and index of the const parameter by indexing the generics of
                // the parent item and construct a `ParamConst`.
                let item_def_id = tcx.parent(def_id);
                let generics = tcx.generics_of(item_def_id);
                let index = generics.param_def_id_to_index[&def_id];
                let name = tcx.item_name(def_id);
                ty::Const::new_param(tcx, ty::ParamConst::new(index, name), param_ty)
            }
            Some(rbv::ResolvedArg::LateBound(debruijn, index, _)) => {
                ty::Const::new_bound(tcx, debruijn, ty::BoundVar::from_u32(index), param_ty)
            }
            Some(rbv::ResolvedArg::Error(guar)) => ty::Const::new_error(tcx, guar, param_ty),
            arg => bug!("unexpected bound var resolution for {:?}: {arg:?}", hir_id),
        }
    }

    /// Lower a type from the HIR to our internal notion of a type.
    pub fn lower_ty(&self, hir_ty: &hir::Ty<'tcx>) -> Ty<'tcx> {
        self.lower_ty_common(hir_ty, false, false)
    }

    /// Lower a type inside of a path from the HIR to our internal notion of a type.
    pub fn lower_ty_in_path(&self, hir_ty: &hir::Ty<'tcx>) -> Ty<'tcx> {
        self.lower_ty_common(hir_ty, false, true)
    }

    fn check_delegation_constraints(&self, sig_id: DefId, span: Span, emit: bool) -> bool {
        let mut error_occured = false;
        let sig_span = self.tcx().def_span(sig_id);
        let mut try_emit = |descr| {
            if emit {
                self.tcx().dcx().emit_err(crate::errors::NotSupportedDelegation {
                    span,
                    descr,
                    callee_span: sig_span,
                });
            }
            error_occured = true;
        };

        if let Some(node) = self.tcx().hir().get_if_local(sig_id)
            && let Some(decl) = node.fn_decl()
            && let hir::FnRetTy::Return(ty) = decl.output
            && let hir::TyKind::InferDelegation(_, _) = ty.kind
        {
            try_emit("recursive delegation");
        }

        let sig = self.tcx().fn_sig(sig_id).instantiate_identity();
        if sig.output().has_opaque_types() {
            try_emit("delegation to a function with opaque type");
        }

        let sig_generics = self.tcx().generics_of(sig_id);
        let parent = self.tcx().parent(self.item_def_id());
        let parent_generics = self.tcx().generics_of(parent);

        let parent_is_trait = (self.tcx().def_kind(parent) == DefKind::Trait) as usize;
        let sig_has_self = sig_generics.has_self as usize;

        if sig_generics.count() > sig_has_self || parent_generics.count() > parent_is_trait {
            try_emit("delegation with early bound generics");
        }

        if self.tcx().asyncness(sig_id) == ty::Asyncness::Yes {
            try_emit("delegation to async functions");
        }

        if self.tcx().constness(sig_id) == hir::Constness::Const {
            try_emit("delegation to const functions");
        }

        if sig.c_variadic() {
            try_emit("delegation to variadic functions");
            // variadic functions are also `unsafe` and `extern "C"`.
            // Do not emit same error multiple times.
            return error_occured;
        }

        if let hir::Unsafety::Unsafe = sig.unsafety() {
            try_emit("delegation to unsafe functions");
        }

        if abi::Abi::Rust != sig.abi() {
            try_emit("delegation to non Rust ABI functions");
        }

        error_occured
    }

    fn lower_delegation_ty(
        &self,
        sig_id: DefId,
        idx: hir::InferDelegationKind,
        span: Span,
    ) -> Ty<'tcx> {
        if self.check_delegation_constraints(sig_id, span, idx == hir::InferDelegationKind::Output)
        {
            let e = self.tcx().dcx().span_delayed_bug(span, "not supported delegation case");
            self.set_tainted_by_errors(e);
            return Ty::new_error(self.tcx(), e);
        };
        let sig = self.tcx().fn_sig(sig_id);
        let sig_generics = self.tcx().generics_of(sig_id);

        let parent = self.tcx().parent(self.item_def_id());
        let parent_def_kind = self.tcx().def_kind(parent);

        let sig = if let DefKind::Impl { .. } = parent_def_kind
            && sig_generics.has_self
        {
            // Generic params can't be here except the trait self type.
            // They are not supported yet.
            assert_eq!(sig_generics.count(), 1);
            assert_eq!(self.tcx().generics_of(parent).count(), 0);

            let self_ty = self.tcx().type_of(parent).instantiate_identity();
            let generic_self_ty = ty::GenericArg::from(self_ty);
            let args = self.tcx().mk_args_from_iter(std::iter::once(generic_self_ty));
            sig.instantiate(self.tcx(), args)
        } else {
            sig.instantiate_identity()
        };

        // Bound vars are also inherited from `sig_id`.
        // They will be rebound later in `lower_fn_ty`.
        let sig = sig.skip_binder();

        match idx {
            hir::InferDelegationKind::Input(id) => sig.inputs()[id],
            hir::InferDelegationKind::Output => sig.output(),
        }
    }

    /// Lower a type from the HIR to our internal notion of a type given some extra data for diagnostics.
    ///
    /// Extra diagnostic data:
    ///
    /// 1. `borrowed`: Whether trait object types are borrowed like in `&dyn Trait`.
    ///    Used to avoid emitting redundant errors.
    /// 2. `in_path`: Whether the type appears inside of a path.
    ///    Used to provide correct diagnostics for bare trait object types.
    #[instrument(level = "debug", skip(self), ret)]
    fn lower_ty_common(&self, hir_ty: &hir::Ty<'tcx>, borrowed: bool, in_path: bool) -> Ty<'tcx> {
        let tcx = self.tcx();

        let result_ty = match &hir_ty.kind {
            hir::TyKind::InferDelegation(sig_id, idx) => {
                self.lower_delegation_ty(*sig_id, *idx, hir_ty.span)
            }
            hir::TyKind::Slice(ty) => Ty::new_slice(tcx, self.lower_ty(ty)),
            hir::TyKind::Ptr(mt) => Ty::new_ptr(tcx, self.lower_ty(mt.ty), mt.mutbl),
            hir::TyKind::Ref(region, mt) => {
                let r = self.lower_lifetime(region, None);
                debug!(?r);
                let t = self.lower_ty_common(mt.ty, true, false);
                Ty::new_ref(tcx, r, t, mt.mutbl)
            }
            hir::TyKind::Never => tcx.types.never,
            hir::TyKind::Tup(fields) => {
                Ty::new_tup_from_iter(tcx, fields.iter().map(|t| self.lower_ty(t)))
            }
            hir::TyKind::AnonAdt(item_id) => {
                let _guard = debug_span!("AnonAdt");

                let did = item_id.owner_id.def_id;
                let adt_def = tcx.adt_def(did);

                let args = ty::GenericArgs::for_item(tcx, did.to_def_id(), |param, _| {
                    tcx.mk_param_from_def(param)
                });
                debug!(?args);

                Ty::new_adt(tcx, adt_def, tcx.mk_args(args))
            }
            hir::TyKind::BareFn(bf) => {
                require_c_abi_if_c_variadic(tcx, bf.decl, bf.abi, hir_ty.span);

                Ty::new_fn_ptr(
                    tcx,
                    self.lower_fn_ty(
                        hir_ty.hir_id,
                        bf.unsafety,
                        bf.abi,
                        bf.decl,
                        None,
                        Some(hir_ty),
                    ),
                )
            }
            hir::TyKind::TraitObject(bounds, lifetime, repr) => {
                self.maybe_lint_bare_trait(hir_ty, in_path);
                let repr = match repr {
                    TraitObjectSyntax::Dyn | TraitObjectSyntax::None => ty::Dyn,
                    TraitObjectSyntax::DynStar => ty::DynStar,
                };

                self.lower_trait_object_ty(
                    hir_ty.span,
                    hir_ty.hir_id,
                    bounds,
                    lifetime,
                    borrowed,
                    repr,
                )
            }
            hir::TyKind::Path(hir::QPath::Resolved(maybe_qself, path)) => {
                debug!(?maybe_qself, ?path);
                let opt_self_ty = maybe_qself.as_ref().map(|qself| self.lower_ty(qself));
                self.lower_path(opt_self_ty, path, hir_ty.hir_id, false)
            }
            &hir::TyKind::OpaqueDef(item_id, lifetimes, in_trait) => {
                let opaque_ty = tcx.hir().item(item_id);

                match opaque_ty.kind {
                    hir::ItemKind::OpaqueTy(&hir::OpaqueTy { .. }) => {
                        let local_def_id = item_id.owner_id.def_id;
                        // If this is an RPITIT and we are using the new RPITIT lowering scheme, we
                        // generate the def_id of an associated type for the trait and return as
                        // type a projection.
                        let def_id = if in_trait {
                            tcx.associated_type_for_impl_trait_in_trait(local_def_id).to_def_id()
                        } else {
                            local_def_id.to_def_id()
                        };
                        self.lower_opaque_ty(def_id, lifetimes, in_trait)
                    }
                    ref i => bug!("`impl Trait` pointed to non-opaque type?? {:#?}", i),
                }
            }
            hir::TyKind::Path(hir::QPath::TypeRelative(qself, segment)) => {
                debug!(?qself, ?segment);
                let ty = self.lower_ty_common(qself, false, true);
                self.lower_assoc_path(hir_ty.hir_id, hir_ty.span, ty, qself, segment, false)
                    .map(|(ty, _, _)| ty)
                    .unwrap_or_else(|guar| Ty::new_error(tcx, guar))
            }
            &hir::TyKind::Path(hir::QPath::LangItem(lang_item, span)) => {
                let def_id = tcx.require_lang_item(lang_item, Some(span));
                let (args, _) = self.lower_generic_args_of_path(
                    span,
                    def_id,
                    &[],
                    &hir::PathSegment::invalid(),
                    None,
                    ty::BoundConstness::NotConst,
                );
                tcx.at(span).type_of(def_id).instantiate(tcx, args)
            }
            hir::TyKind::Array(ty, length) => {
                let length = match length {
                    hir::ArrayLen::Infer(inf) => self.ct_infer(tcx.types.usize, None, inf.span),
                    hir::ArrayLen::Body(constant) => {
                        ty::Const::from_anon_const(tcx, constant.def_id)
                    }
                };

                Ty::new_array_with_const_len(tcx, self.lower_ty(ty), length)
            }
            hir::TyKind::Typeof(e) => tcx.type_of(e.def_id).instantiate_identity(),
            hir::TyKind::Infer => {
                // Infer also appears as the type of arguments or return
                // values in an ExprKind::Closure, or as
                // the type of local variables. Both of these cases are
                // handled specially and will not descend into this routine.
                self.ty_infer(None, hir_ty.span)
            }
            hir::TyKind::Err(guar) => Ty::new_error(tcx, *guar),
        };

        self.record_ty(hir_ty.hir_id, result_ty, hir_ty.span);
        result_ty
    }

    /// Lower an opaque type (i.e., an existential impl-Trait type) from the HIR.
    #[instrument(level = "debug", skip_all, ret)]
    fn lower_opaque_ty(
        &self,
        def_id: DefId,
        lifetimes: &[hir::GenericArg<'_>],
        in_trait: bool,
    ) -> Ty<'tcx> {
        debug!(?def_id, ?lifetimes);
        let tcx = self.tcx();

        let generics = tcx.generics_of(def_id);
        debug!(?generics);

        let args = ty::GenericArgs::for_item(tcx, def_id, |param, _| {
            // We use `generics.count() - lifetimes.len()` here instead of `generics.parent_count`
            // since return-position impl trait in trait squashes all of the generics from its source fn
            // into its own generics, so the opaque's "own" params isn't always just lifetimes.
            if let Some(i) = (param.index as usize).checked_sub(generics.count() - lifetimes.len())
            {
                // Resolve our own lifetime parameters.
                let GenericParamDefKind::Lifetime { .. } = param.kind else {
                    span_bug!(
                        tcx.def_span(param.def_id),
                        "only expected lifetime for opaque's own generics, got {:?}",
                        param.kind
                    );
                };
                let hir::GenericArg::Lifetime(lifetime) = &lifetimes[i] else {
                    bug!(
                        "expected lifetime argument for param {param:?}, found {:?}",
                        &lifetimes[i]
                    )
                };
                self.lower_lifetime(lifetime, None).into()
            } else {
                tcx.mk_param_from_def(param)
            }
        });
        debug!(?args);

        if in_trait {
            Ty::new_projection(tcx, def_id, args)
        } else {
            Ty::new_opaque(tcx, def_id, args)
        }
    }

    pub fn lower_arg_ty(&self, ty: &hir::Ty<'tcx>, expected_ty: Option<Ty<'tcx>>) -> Ty<'tcx> {
        match ty.kind {
            hir::TyKind::Infer if let Some(expected_ty) = expected_ty => {
                self.record_ty(ty.hir_id, expected_ty, ty.span);
                expected_ty
            }
            _ => self.lower_ty(ty),
        }
    }

    /// Lower a function type from the HIR to our internal notion of a function signature.
    #[instrument(level = "debug", skip(self, hir_id, unsafety, abi, decl, generics, hir_ty), ret)]
    pub fn lower_fn_ty(
        &self,
        hir_id: hir::HirId,
        unsafety: hir::Unsafety,
        abi: abi::Abi,
        decl: &hir::FnDecl<'tcx>,
        generics: Option<&hir::Generics<'_>>,
        hir_ty: Option<&hir::Ty<'_>>,
    ) -> ty::PolyFnSig<'tcx> {
        let tcx = self.tcx();
        let bound_vars = if let hir::FnRetTy::Return(ret_ty) = decl.output
            && let hir::TyKind::InferDelegation(sig_id, _) = ret_ty.kind
        {
            tcx.fn_sig(sig_id).skip_binder().bound_vars()
        } else {
            tcx.late_bound_vars(hir_id)
        };
        debug!(?bound_vars);

        // We proactively collect all the inferred type params to emit a single error per fn def.
        let mut visitor = HirPlaceholderCollector::default();
        let mut infer_replacements = vec![];

        if let Some(generics) = generics {
            walk_generics(&mut visitor, generics);
        }

        let input_tys: Vec<_> = decl
            .inputs
            .iter()
            .enumerate()
            .map(|(i, a)| {
                if let hir::TyKind::Infer = a.kind
                    && !self.allow_infer()
                {
                    if let Some(suggested_ty) =
                        self.suggest_trait_fn_ty_for_impl_fn_infer(hir_id, Some(i))
                    {
                        infer_replacements.push((a.span, suggested_ty.to_string()));
                        return Ty::new_error_with_message(
                            self.tcx(),
                            a.span,
                            suggested_ty.to_string(),
                        );
                    }
                }

                // Only visit the type looking for `_` if we didn't fix the type above
                visitor.visit_ty(a);
                self.lower_arg_ty(a, None)
            })
            .collect();

        let output_ty = match decl.output {
            hir::FnRetTy::Return(output) => {
                if let hir::TyKind::Infer = output.kind
                    && !self.allow_infer()
                    && let Some(suggested_ty) =
                        self.suggest_trait_fn_ty_for_impl_fn_infer(hir_id, None)
                {
                    infer_replacements.push((output.span, suggested_ty.to_string()));
                    Ty::new_error_with_message(self.tcx(), output.span, suggested_ty.to_string())
                } else {
                    visitor.visit_ty(output);
                    self.lower_ty(output)
                }
            }
            hir::FnRetTy::DefaultReturn(..) => Ty::new_unit(tcx),
        };

        debug!(?output_ty);

        let fn_ty = tcx.mk_fn_sig(input_tys, output_ty, decl.c_variadic, unsafety, abi);
        let bare_fn_ty = ty::Binder::bind_with_vars(fn_ty, bound_vars);

        if !self.allow_infer() && !(visitor.0.is_empty() && infer_replacements.is_empty()) {
            // We always collect the spans for placeholder types when evaluating `fn`s, but we
            // only want to emit an error complaining about them if infer types (`_`) are not
            // allowed. `allow_infer` gates this behavior. We check for the presence of
            // `ident_span` to not emit an error twice when we have `fn foo(_: fn() -> _)`.

            let mut diag = crate::collect::placeholder_type_error_diag(
                tcx,
                generics,
                visitor.0,
                infer_replacements.iter().map(|(s, _)| *s).collect(),
                true,
                hir_ty,
                "function",
            );

            if !infer_replacements.is_empty() {
                diag.multipart_suggestion(
                    format!(
                    "try replacing `_` with the type{} in the corresponding trait method signature",
                    rustc_errors::pluralize!(infer_replacements.len()),
                ),
                    infer_replacements,
                    Applicability::MachineApplicable,
                );
            }

            self.set_tainted_by_errors(diag.emit());
        }

        // Find any late-bound regions declared in return type that do
        // not appear in the arguments. These are not well-formed.
        //
        // Example:
        //     for<'a> fn() -> &'a str <-- 'a is bad
        //     for<'a> fn(&'a String) -> &'a str <-- 'a is ok
        let inputs = bare_fn_ty.inputs();
        let late_bound_in_args =
            tcx.collect_constrained_late_bound_regions(inputs.map_bound(|i| i.to_owned()));
        let output = bare_fn_ty.output();
        let late_bound_in_ret = tcx.collect_referenced_late_bound_regions(output);

        self.validate_late_bound_regions(late_bound_in_args, late_bound_in_ret, |br_name| {
            struct_span_code_err!(
                tcx.dcx(),
                decl.output.span(),
                E0581,
                "return type references {}, which is not constrained by the fn input types",
                br_name
            )
        });

        bare_fn_ty
    }

    /// Given a fn_hir_id for a impl function, suggest the type that is found on the
    /// corresponding function in the trait that the impl implements, if it exists.
    /// If arg_idx is Some, then it corresponds to an input type index, otherwise it
    /// corresponds to the return type.
    fn suggest_trait_fn_ty_for_impl_fn_infer(
        &self,
        fn_hir_id: hir::HirId,
        arg_idx: Option<usize>,
    ) -> Option<Ty<'tcx>> {
        let tcx = self.tcx();
        let hir::Node::ImplItem(hir::ImplItem { kind: hir::ImplItemKind::Fn(..), ident, .. }) =
            tcx.hir_node(fn_hir_id)
        else {
            return None;
        };
        let i = tcx.parent_hir_node(fn_hir_id).expect_item().expect_impl();

        let trait_ref = self.lower_impl_trait_ref(i.of_trait.as_ref()?, self.lower_ty(i.self_ty));

        let assoc = tcx.associated_items(trait_ref.def_id).find_by_name_and_kind(
            tcx,
            *ident,
            ty::AssocKind::Fn,
            trait_ref.def_id,
        )?;

        let fn_sig = tcx.fn_sig(assoc.def_id).instantiate(
            tcx,
            trait_ref.args.extend_to(tcx, assoc.def_id, |param, _| tcx.mk_param_from_def(param)),
        );
        let fn_sig = tcx.liberate_late_bound_regions(fn_hir_id.expect_owner().to_def_id(), fn_sig);

        Some(if let Some(arg_idx) = arg_idx {
            *fn_sig.inputs().get(arg_idx)?
        } else {
            fn_sig.output()
        })
    }

    #[instrument(level = "trace", skip(self, generate_err))]
    fn validate_late_bound_regions(
        &self,
        constrained_regions: FxHashSet<ty::BoundRegionKind>,
        referenced_regions: FxHashSet<ty::BoundRegionKind>,
        generate_err: impl Fn(&str) -> Diag<'tcx>,
    ) {
        for br in referenced_regions.difference(&constrained_regions) {
            let br_name = match *br {
                ty::BrNamed(_, kw::UnderscoreLifetime) | ty::BrAnon | ty::BrEnv => {
                    "an anonymous lifetime".to_string()
                }
                ty::BrNamed(_, name) => format!("lifetime `{name}`"),
            };

            let mut err = generate_err(&br_name);

            if let ty::BrNamed(_, kw::UnderscoreLifetime) | ty::BrAnon = *br {
                // The only way for an anonymous lifetime to wind up
                // in the return type but **also** be unconstrained is
                // if it only appears in "associated types" in the
                // input. See #47511 and #62200 for examples. In this case,
                // though we can easily give a hint that ought to be
                // relevant.
                err.note(
                    "lifetimes appearing in an associated or opaque type are not considered constrained",
                );
                err.note("consider introducing a named lifetime parameter");
            }

            self.set_tainted_by_errors(err.emit());
        }
    }

    /// Given the bounds on an object, determines what single region bound (if any) we can
    /// use to summarize this type.
    ///
    /// The basic idea is that we will use the bound the user
    /// provided, if they provided one, and otherwise search the supertypes of trait bounds
    /// for region bounds. It may be that we can derive no bound at all, in which case
    /// we return `None`.
    #[instrument(level = "debug", skip(self, span), ret)]
    fn compute_object_lifetime_bound(
        &self,
        span: Span,
        existential_predicates: &'tcx ty::List<ty::PolyExistentialPredicate<'tcx>>,
    ) -> Option<ty::Region<'tcx>> // if None, use the default
    {
        let tcx = self.tcx();

        // No explicit region bound specified. Therefore, examine trait
        // bounds and see if we can derive region bounds from those.
        let derived_region_bounds = object_region_bounds(tcx, existential_predicates);

        // If there are no derived region bounds, then report back that we
        // can find no region bound. The caller will use the default.
        if derived_region_bounds.is_empty() {
            return None;
        }

        // If any of the derived region bounds are 'static, that is always
        // the best choice.
        if derived_region_bounds.iter().any(|r| r.is_static()) {
            return Some(tcx.lifetimes.re_static);
        }

        // Determine whether there is exactly one unique region in the set
        // of derived region bounds. If so, use that. Otherwise, report an
        // error.
        let r = derived_region_bounds[0];
        if derived_region_bounds[1..].iter().any(|r1| r != *r1) {
            self.set_tainted_by_errors(tcx.dcx().emit_err(AmbiguousLifetimeBound { span }));
        }
        Some(r)
    }
}

fn assoc_kind_str(kind: ty::AssocKind) -> &'static str {
    match kind {
        ty::AssocKind::Fn => "function",
        ty::AssocKind::Const => "constant",
        ty::AssocKind::Type => "type",
    }
}
