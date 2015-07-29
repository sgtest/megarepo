// Copyright 2014-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use check::regionck::{self, Rcx};

use middle::infer;
use middle::region;
use middle::subst::{self, Subst};
use middle::ty::{self, Ty};
use util::nodemap::FnvHashSet;

use syntax::ast;
use syntax::codemap::{self, Span};

/// check_drop_impl confirms that the Drop implementation identfied by
/// `drop_impl_did` is not any more specialized than the type it is
/// attached to (Issue #8142).
///
/// This means:
///
/// 1. The self type must be nominal (this is already checked during
///    coherence),
///
/// 2. The generic region/type parameters of the impl's self-type must
///    all be parameters of the Drop impl itself (i.e. no
///    specialization like `impl Drop for Foo<i32>`), and,
///
/// 3. Any bounds on the generic parameters must be reflected in the
///    struct/enum definition for the nominal type itself (i.e.
///    cannot do `struct S<T>; impl<T:Clone> Drop for S<T> { ... }`).
///
pub fn check_drop_impl(tcx: &ty::ctxt, drop_impl_did: ast::DefId) -> Result<(), ()> {
    let ty::TypeScheme { generics: ref dtor_generics,
                         ty: dtor_self_type } = tcx.lookup_item_type(drop_impl_did);
    let dtor_predicates = tcx.lookup_predicates(drop_impl_did);
    match dtor_self_type.sty {
        ty::TyEnum(self_type_did, self_to_impl_substs) |
        ty::TyStruct(self_type_did, self_to_impl_substs) => {
            try!(ensure_drop_params_and_item_params_correspond(tcx,
                                                               drop_impl_did,
                                                               dtor_generics,
                                                               &dtor_self_type,
                                                               self_type_did));

            ensure_drop_predicates_are_implied_by_item_defn(tcx,
                                                            drop_impl_did,
                                                            &dtor_predicates,
                                                            self_type_did,
                                                            self_to_impl_substs)
        }
        _ => {
            // Destructors only work on nominal types.  This was
            // already checked by coherence, so we can panic here.
            let span = tcx.map.def_id_span(drop_impl_did, codemap::DUMMY_SP);
            tcx.sess.span_bug(
                span, &format!("should have been rejected by coherence check: {}",
                               dtor_self_type));
        }
    }
}

fn ensure_drop_params_and_item_params_correspond<'tcx>(
    tcx: &ty::ctxt<'tcx>,
    drop_impl_did: ast::DefId,
    drop_impl_generics: &ty::Generics<'tcx>,
    drop_impl_ty: &ty::Ty<'tcx>,
    self_type_did: ast::DefId) -> Result<(), ()>
{
    // New strategy based on review suggestion from nikomatsakis.
    //
    // (In the text and code below, "named" denotes "struct/enum", and
    // "generic params" denotes "type and region params")
    //
    // 1. Create fresh skolemized type/region "constants" for each of
    //    the named type's generic params.  Instantiate the named type
    //    with the fresh constants, yielding `named_skolem`.
    //
    // 2. Create unification variables for each of the Drop impl's
    //    generic params.  Instantiate the impl's Self's type with the
    //    unification-vars, yielding `drop_unifier`.
    //
    // 3. Attempt to unify Self_unif with Type_skolem.  If unification
    //    succeeds, continue (i.e. with the predicate checks).

    let ty::TypeScheme { generics: ref named_type_generics,
                         ty: named_type } =
        tcx.lookup_item_type(self_type_did);

    let infcx = infer::new_infer_ctxt(tcx, &tcx.tables, None, false);

    infcx.commit_if_ok(|snapshot| {
        let (named_type_to_skolem, skol_map) =
            infcx.construct_skolemized_subst(named_type_generics, snapshot);
        let named_type_skolem = named_type.subst(tcx, &named_type_to_skolem);

        let drop_impl_span = tcx.map.def_id_span(drop_impl_did, codemap::DUMMY_SP);
        let drop_to_unifier =
            infcx.fresh_substs_for_generics(drop_impl_span, drop_impl_generics);
        let drop_unifier = drop_impl_ty.subst(tcx, &drop_to_unifier);

        if let Ok(()) = infer::mk_eqty(&infcx, true, infer::TypeOrigin::Misc(drop_impl_span),
                                       named_type_skolem, drop_unifier) {
            // Even if we did manage to equate the types, the process
            // may have just gathered unsolvable region constraints
            // like `R == 'static` (represented as a pair of subregion
            // constraints) for some skolemization constant R.
            //
            // However, the leak_check method allows us to confirm
            // that no skolemized regions escaped (i.e. were related
            // to other regions in the constraint graph).
            if let Ok(()) = infcx.leak_check(&skol_map, snapshot) {
                return Ok(())
            }
        }

        span_err!(tcx.sess, drop_impl_span, E0366,
                  "Implementations of Drop cannot be specialized");
        let item_span = tcx.map.span(self_type_did.node);
        tcx.sess.span_note(item_span,
                           "Use same sequence of generic type and region \
                            parameters that is on the struct/enum definition");
        return Err(());
    })
}

/// Confirms that every predicate imposed by dtor_predicates is
/// implied by assuming the predicates attached to self_type_did.
fn ensure_drop_predicates_are_implied_by_item_defn<'tcx>(
    tcx: &ty::ctxt<'tcx>,
    drop_impl_did: ast::DefId,
    dtor_predicates: &ty::GenericPredicates<'tcx>,
    self_type_did: ast::DefId,
    self_to_impl_substs: &subst::Substs<'tcx>) -> Result<(), ()> {

    // Here is an example, analogous to that from
    // `compare_impl_method`.
    //
    // Consider a struct type:
    //
    //     struct Type<'c, 'b:'c, 'a> {
    //         x: &'a Contents            // (contents are irrelevant;
    //         y: &'c Cell<&'b Contents>, //  only the bounds matter for our purposes.)
    //     }
    //
    // and a Drop impl:
    //
    //     impl<'z, 'y:'z, 'x:'y> Drop for P<'z, 'y, 'x> {
    //         fn drop(&mut self) { self.y.set(self.x); } // (only legal if 'x: 'y)
    //     }
    //
    // We start out with self_to_impl_substs, that maps the generic
    // parameters of Type to that of the Drop impl.
    //
    //     self_to_impl_substs = {'c => 'z, 'b => 'y, 'a => 'x}
    //
    // Applying this to the predicates (i.e. assumptions) provided by the item
    // definition yields the instantiated assumptions:
    //
    //     ['y : 'z]
    //
    // We then check all of the predicates of the Drop impl:
    //
    //     ['y:'z, 'x:'y]
    //
    // and ensure each is in the list of instantiated
    // assumptions. Here, `'y:'z` is present, but `'x:'y` is
    // absent. So we report an error that the Drop impl injected a
    // predicate that is not present on the struct definition.

    assert_eq!(self_type_did.krate, ast::LOCAL_CRATE);

    let drop_impl_span = tcx.map.def_id_span(drop_impl_did, codemap::DUMMY_SP);

    // We can assume the predicates attached to struct/enum definition
    // hold.
    let generic_assumptions = tcx.lookup_predicates(self_type_did);

    let assumptions_in_impl_context = generic_assumptions.instantiate(tcx, &self_to_impl_substs);
    assert!(assumptions_in_impl_context.predicates.is_empty_in(subst::SelfSpace));
    assert!(assumptions_in_impl_context.predicates.is_empty_in(subst::FnSpace));
    let assumptions_in_impl_context =
        assumptions_in_impl_context.predicates.get_slice(subst::TypeSpace);

    // An earlier version of this code attempted to do this checking
    // via the traits::fulfill machinery. However, it ran into trouble
    // since the fulfill machinery merely turns outlives-predicates
    // 'a:'b and T:'b into region inference constraints. It is simpler
    // just to look for all the predicates directly.

    assert!(dtor_predicates.predicates.is_empty_in(subst::SelfSpace));
    assert!(dtor_predicates.predicates.is_empty_in(subst::FnSpace));
    let predicates = dtor_predicates.predicates.get_slice(subst::TypeSpace);
    for predicate in predicates {
        // (We do not need to worry about deep analysis of type
        // expressions etc because the Drop impls are already forced
        // to take on a structure that is roughly a alpha-renaming of
        // the generic parameters of the item definition.)

        // This path now just checks *all* predicates via the direct
        // lookup, rather than using fulfill machinery.
        //
        // However, it may be more efficient in the future to batch
        // the analysis together via the fulfill , rather than the
        // repeated `contains` calls.

        if !assumptions_in_impl_context.contains(&predicate) {
            let item_span = tcx.map.span(self_type_did.node);
            span_err!(tcx.sess, drop_impl_span, E0367,
                      "The requirement `{}` is added only by the Drop impl.", predicate);
            tcx.sess.span_note(item_span,
                               "The same requirement must be part of \
                                the struct/enum definition");
        }
    }

    if tcx.sess.has_errors() {
        return Err(());
    }
    Ok(())
}

/// check_safety_of_destructor_if_necessary confirms that the type
/// expression `typ` conforms to the "Drop Check Rule" from the Sound
/// Generic Drop (RFC 769).
///
/// ----
///
/// The Drop Check Rule is the following:
///
/// Let `v` be some value (either temporary or named) and 'a be some
/// lifetime (scope). If the type of `v` owns data of type `D`, where
///
/// * (1.) `D` has a lifetime- or type-parametric Drop implementation, and
/// * (2.) the structure of `D` can reach a reference of type `&'a _`, and
/// * (3.) either:
///   * (A.) the Drop impl for `D` instantiates `D` at 'a directly,
///          i.e. `D<'a>`, or,
///   * (B.) the Drop impl for `D` has some type parameter with a
///          trait bound `T` where `T` is a trait that has at least
///          one method,
///
/// then 'a must strictly outlive the scope of v.
///
/// ----
///
/// This function is meant to by applied to the type for every
/// expression in the program.
pub fn check_safety_of_destructor_if_necessary<'a, 'tcx>(rcx: &mut Rcx<'a, 'tcx>,
                                                     typ: ty::Ty<'tcx>,
                                                     span: Span,
                                                     scope: region::CodeExtent) {
    debug!("check_safety_of_destructor_if_necessary typ: {:?} scope: {:?}",
           typ, scope);

    let parent_scope = rcx.tcx().region_maps.opt_encl_scope(scope).unwrap_or_else(|| {
        rcx.tcx().sess.span_bug(
            span, &format!("no enclosing scope found for scope: {:?}", scope))
    });

    let result = iterate_over_potentially_unsafe_regions_in_type(
        &mut DropckContext {
            rcx: rcx,
            span: span,
            parent_scope: parent_scope,
            breadcrumbs: FnvHashSet()
        },
        TypeContext::Root,
        typ,
        0);
    match result {
        Ok(()) => {}
        Err(Error::Overflow(ref ctxt, ref detected_on_typ)) => {
            let tcx = rcx.tcx();
            span_err!(tcx.sess, span, E0320,
                      "overflow while adding drop-check rules for {}", typ);
            match *ctxt {
                TypeContext::Root => {
                    // no need for an additional note if the overflow
                    // was somehow on the root.
                }
                TypeContext::EnumVariant { def_id, variant, arg_index } => {
                    // FIXME (pnkfelix): eventually lookup arg_name
                    // for the given index on struct variants.
                    span_note!(
                        rcx.tcx().sess,
                        span,
                        "overflowed on enum {} variant {} argument {} type: {}",
                        tcx.item_path_str(def_id),
                        variant,
                        arg_index,
                        detected_on_typ);
                }
                TypeContext::Struct { def_id, field } => {
                    span_note!(
                        rcx.tcx().sess,
                        span,
                        "overflowed on struct {} field {} type: {}",
                        tcx.item_path_str(def_id),
                        field,
                        detected_on_typ);
                }
            }
        }
    }
}

enum Error<'tcx> {
    Overflow(TypeContext, ty::Ty<'tcx>),
}

#[derive(Copy, Clone)]
enum TypeContext {
    Root,
    EnumVariant {
        def_id: ast::DefId,
        variant: ast::Name,
        arg_index: usize,
    },
    Struct {
        def_id: ast::DefId,
        field: ast::Name,
    }
}

struct DropckContext<'a, 'b: 'a, 'tcx: 'b> {
    rcx: &'a mut Rcx<'b, 'tcx>,
    /// types that have already been traversed
    breadcrumbs: FnvHashSet<Ty<'tcx>>,
    /// span for error reporting
    span: Span,
    /// the scope reachable dtorck types must outlive
    parent_scope: region::CodeExtent
}

// `context` is used for reporting overflow errors
fn iterate_over_potentially_unsafe_regions_in_type<'a, 'b, 'tcx>(
    cx: &mut DropckContext<'a, 'b, 'tcx>,
    context: TypeContext,
    ty: Ty<'tcx>,
    depth: usize) -> Result<(), Error<'tcx>>
{
    let tcx = cx.rcx.tcx();
    // Issue #22443: Watch out for overflow. While we are careful to
    // handle regular types properly, non-regular ones cause problems.
    let recursion_limit = tcx.sess.recursion_limit.get();
    if depth / 4 >= recursion_limit {
        // This can get into rather deep recursion, especially in the
        // presence of things like Vec<T> -> Unique<T> -> PhantomData<T> -> T.
        // use a higher recursion limit to avoid errors.
        return Err(Error::Overflow(context, ty))
    }

    let opt_phantom_data_def_id = tcx.lang_items.phantom_data();

    if !cx.breadcrumbs.insert(ty) {
        debug!("iterate_over_potentially_unsafe_regions_in_type \
               {}ty: {} scope: {:?} - cached",
               (0..depth).map(|_| ' ').collect::<String>(),
               ty, cx.parent_scope);
        return Ok(()); // we already visited this type
    }
    debug!("iterate_over_potentially_unsafe_regions_in_type \
           {}ty: {} scope: {:?}",
           (0..depth).map(|_| ' ').collect::<String>(),
           ty, cx.parent_scope);

    // If `typ` has a destructor, then we must ensure that all
    // borrowed data reachable via `typ` must outlive the parent
    // of `scope`. This is handled below.
    //
    // However, there is an important special case: by
    // parametricity, any generic type parameters have *no* trait
    // bounds in the Drop impl can not be used in any way (apart
    // from being dropped), and thus we can treat data borrowed
    // via such type parameters remains unreachable.
    //
    // For example, consider `impl<T> Drop for Vec<T> { ... }`,
    // which does have to be able to drop instances of `T`, but
    // otherwise cannot read data from `T`.
    //
    // Of course, for the type expression passed in for any such
    // unbounded type parameter `T`, we must resume the recursive
    // analysis on `T` (since it would be ignored by
    // type_must_outlive).
    //
    // FIXME (pnkfelix): Long term, we could be smart and actually
    // feed which generic parameters can be ignored *into* `fn
    // type_must_outlive` (or some generalization thereof). But
    // for the short term, it probably covers most cases of
    // interest to just special case Drop impls where: (1.) there
    // are no generic lifetime parameters and (2.)  *all* generic
    // type parameters are unbounded.  If both conditions hold, we
    // simply skip the `type_must_outlive` call entirely (but
    // resume the recursive checking of the type-substructure).
    if has_dtor_of_interest(tcx, ty, cx.span) {
        debug!("iterate_over_potentially_unsafe_regions_in_type \
                {}ty: {} - is a dtorck type!",
               (0..depth).map(|_| ' ').collect::<String>(),
               ty);

        regionck::type_must_outlive(cx.rcx,
                                    infer::SubregionOrigin::SafeDestructor(cx.span),
                                    ty,
                                    ty::ReScope(cx.parent_scope));

        return Ok(());
    }

    debug!("iterate_over_potentially_unsafe_regions_in_type \
           {}ty: {} scope: {:?} - checking interior",
           (0..depth).map(|_| ' ').collect::<String>(),
           ty, cx.parent_scope);

    // We still need to ensure all referenced data is safe.
    match ty.sty {
        ty::TyBool | ty::TyChar | ty::TyInt(_) | ty::TyUint(_) |
        ty::TyFloat(_) | ty::TyStr => {
            // primitive - definitely safe
            Ok(())
        }

        ty::TyBox(ity) | ty::TyArray(ity, _) | ty::TySlice(ity) => {
            // single-element containers, behave like their element
            iterate_over_potentially_unsafe_regions_in_type(
                cx, context, ity, depth+1)
        }

        ty::TyStruct(did, substs) if Some(did) == opt_phantom_data_def_id => {
            // PhantomData<T> - behaves identically to T
            let ity = *substs.types.get(subst::TypeSpace, 0);
            iterate_over_potentially_unsafe_regions_in_type(
                cx, context, ity, depth+1)
        }

        ty::TyStruct(did, substs) => {
            let fields = tcx.lookup_struct_fields(did);
            for field in &fields {
                let fty = tcx.lookup_field_type(did, field.id, substs);
                let fty = cx.rcx.fcx.resolve_type_vars_if_possible(
                    cx.rcx.fcx.normalize_associated_types_in(cx.span, &fty));
                try!(iterate_over_potentially_unsafe_regions_in_type(
                    cx,
                    TypeContext::Struct {
                        def_id: did,
                        field: field.name,
                    },
                    fty,
                    depth+1))
            }
            Ok(())
        }

        ty::TyEnum(did, substs) => {
            let all_variant_info = tcx.substd_enum_variants(did, substs);
            for variant_info in &all_variant_info {
                for (i, fty) in variant_info.args.iter().enumerate() {
                    let fty = cx.rcx.fcx.resolve_type_vars_if_possible(
                        cx.rcx.fcx.normalize_associated_types_in(cx.span, &fty));
                    try!(iterate_over_potentially_unsafe_regions_in_type(
                        cx,
                        TypeContext::EnumVariant {
                            def_id: did,
                            variant: variant_info.name,
                            arg_index: i,
                        },
                        fty,
                        depth+1));
                }
            }
            Ok(())
        }

        ty::TyTuple(ref tys) |
        ty::TyClosure(_, box ty::ClosureSubsts { upvar_tys: ref tys, .. }) => {
            for ty in tys {
                try!(iterate_over_potentially_unsafe_regions_in_type(
                    cx, context, ty, depth+1))
            }
            Ok(())
        }

        ty::TyRawPtr(..) | ty::TyRef(..) | ty::TyParam(..) => {
            // these always come with a witness of liveness (references
            // explicitly, pointers implicitly, parameters by the
            // caller).
            Ok(())
        }

        ty::TyBareFn(..) => {
            // FIXME(#26656): this type is always destruction-safe, but
            // it implicitly witnesses Self: Fn, which can be false.
            Ok(())
        }

        ty::TyInfer(..) | ty::TyError => {
            tcx.sess.delay_span_bug(cx.span, "unresolved type in regionck");
            Ok(())
        }

        // these are always dtorck
        ty::TyTrait(..) | ty::TyProjection(_) => unreachable!(),
    }
}

fn has_dtor_of_interest<'tcx>(tcx: &ty::ctxt<'tcx>,
                              ty: ty::Ty<'tcx>,
                              span: Span) -> bool {
    match ty.sty {
        ty::TyEnum(def_id, _) | ty::TyStruct(def_id, _) => {
            let dtor_method_did = match tcx.destructor_for_type.borrow().get(&def_id) {
                Some(def_id) => *def_id,
                None => {
                    debug!("ty: {:?} has no dtor, and thus isn't a dropck type", ty);
                    return false;
                }
            };
            let impl_did = tcx.impl_of_method(dtor_method_did)
                .unwrap_or_else(|| {
                    tcx.sess.span_bug(
                        span, "no Drop impl found for drop method")
                });

            let dtor_typescheme = tcx.lookup_item_type(impl_did);
            let dtor_generics = dtor_typescheme.generics;

            let mut has_pred_of_interest = false;

            let mut seen_items = Vec::new();
            let mut items_to_inspect = vec![impl_did];
            'items: while let Some(item_def_id) = items_to_inspect.pop() {
                if seen_items.contains(&item_def_id) {
                    continue;
                }

                for pred in tcx.lookup_predicates(item_def_id).predicates {
                    let result = match pred {
                        ty::Predicate::Equate(..) |
                        ty::Predicate::RegionOutlives(..) |
                        ty::Predicate::TypeOutlives(..) |
                        ty::Predicate::Projection(..) => {
                            // For now, assume all these where-clauses
                            // may give drop implementation capabilty
                            // to access borrowed data.
                            true
                        }

                        ty::Predicate::Trait(ty::Binder(ref t_pred)) => {
                            let def_id = t_pred.trait_ref.def_id;
                            if tcx.trait_items(def_id).len() != 0 {
                                // If trait has items, assume it adds
                                // capability to access borrowed data.
                                true
                            } else {
                                // Trait without items is itself
                                // uninteresting from POV of dropck.
                                //
                                // However, may have parent w/ items;
                                // so schedule checking of predicates,
                                items_to_inspect.push(def_id);
                                // and say "no capability found" for now.
                                false
                            }
                        }
                    };

                    if result {
                        has_pred_of_interest = true;
                        debug!("ty: {:?} has interesting dtor due to generic preds, e.g. {:?}",
                               ty, pred);
                        break 'items;
                    }
                }

                seen_items.push(item_def_id);
            }

            // In `impl<'a> Drop ...`, we automatically assume
            // `'a` is meaningful and thus represents a bound
            // through which we could reach borrowed data.
            //
            // FIXME (pnkfelix): In the future it would be good to
            // extend the language to allow the user to express,
            // in the impl signature, that a lifetime is not
            // actually used (something like `where 'a: ?Live`).
            let has_region_param_of_interest =
                dtor_generics.has_region_params(subst::TypeSpace);

            let has_dtor_of_interest =
                has_region_param_of_interest ||
                has_pred_of_interest;

            if has_dtor_of_interest {
                debug!("ty: {:?} has interesting dtor, due to \
                        region params: {} or pred: {}",
                       ty,
                       has_region_param_of_interest,
                       has_pred_of_interest);
            } else {
                debug!("ty: {:?} has dtor, but it is uninteresting", ty);
            }
            has_dtor_of_interest
        }
        ty::TyTrait(..) | ty::TyProjection(..) => {
            debug!("ty: {:?} isn't known, and therefore is a dropck type", ty);
            true
        },
        _ => false
    }
}
