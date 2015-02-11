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
use middle::subst;
use middle::ty::{self, Ty};
use util::ppaux::{Repr};

use syntax::codemap::Span;

pub fn check_safety_of_destructor_if_necessary<'a, 'tcx>(rcx: &mut Rcx<'a, 'tcx>,
                                                     typ: ty::Ty<'tcx>,
                                                     span: Span,
                                                     scope: region::CodeExtent) {
    debug!("check_safety_of_destructor_if_necessary typ: {} scope: {:?}",
           typ.repr(rcx.tcx()), scope);

    // types that have been traversed so far by `traverse_type_if_unseen`
    let mut breadcrumbs: Vec<Ty<'tcx>> = Vec::new();

    iterate_over_potentially_unsafe_regions_in_type(
        rcx,
        &mut breadcrumbs,
        typ,
        span,
        scope,
        0);
}

fn iterate_over_potentially_unsafe_regions_in_type<'a, 'tcx>(
    rcx: &mut Rcx<'a, 'tcx>,
    breadcrumbs: &mut Vec<Ty<'tcx>>,
    ty_root: ty::Ty<'tcx>,
    span: Span,
    scope: region::CodeExtent,
    depth: uint)
{
    let origin = |&:| infer::SubregionOrigin::SafeDestructor(span);
    let mut walker = ty_root.walk();
    let opt_phantom_data_def_id = rcx.tcx().lang_items.phantom_data();

    let destructor_for_type = rcx.tcx().destructor_for_type.borrow();

    while let Some(typ) = walker.next() {
        // Avoid recursing forever.
        if breadcrumbs.contains(&typ) {
            continue;
        }
        breadcrumbs.push(typ);

        // If we encounter `PhantomData<T>`, then we should replace it
        // with `T`, the type it represents as owned by the
        // surrounding context, before doing further analysis.
        let typ = if let ty::ty_struct(struct_did, substs) = typ.sty {
            if opt_phantom_data_def_id == Some(struct_did) {
                let item_type = ty::lookup_item_type(rcx.tcx(), struct_did);
                let tp_def = item_type.generics.types
                    .opt_get(subst::TypeSpace, 0).unwrap();
                let new_typ = substs.type_for_def(tp_def);
                debug!("replacing phantom {} with {}",
                       typ.repr(rcx.tcx()), new_typ.repr(rcx.tcx()));
                new_typ
            } else {
                typ
            }
        } else {
            typ
        };

        let opt_type_did = match typ.sty {
            ty::ty_struct(struct_did, _) => Some(struct_did),
            ty::ty_enum(enum_did, _) => Some(enum_did),
            _ => None,
        };

        let opt_dtor =
            opt_type_did.and_then(|did| destructor_for_type.get(&did));

        debug!("iterate_over_potentially_unsafe_regions_in_type \
                {}typ: {} scope: {:?} opt_dtor: {:?}",
               (0..depth).map(|_| ' ').collect::<String>(),
               typ.repr(rcx.tcx()), scope, opt_dtor);

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

        let has_dtor_of_interest;

        if let Some(&dtor_method_did) = opt_dtor {
            let impl_did = ty::impl_of_method(rcx.tcx(), dtor_method_did)
                .unwrap_or_else(|| {
                    rcx.tcx().sess.span_bug(
                        span, "no Drop impl found for drop method")
                });

            let dtor_typescheme = ty::lookup_item_type(rcx.tcx(), impl_did);
            let dtor_generics = dtor_typescheme.generics;

            let has_pred_of_interest = dtor_generics.predicates.iter().any(|pred| {
                // In `impl<T> Drop where ...`, we automatically
                // assume some predicate will be meaningful and thus
                // represents a type through which we could reach
                // borrowed data. However, there can be implicit
                // predicates (namely for Sized), and so we still need
                // to walk through and filter out those cases.

                let result = match *pred {
                    ty::Predicate::Trait(ty::Binder(ref t_pred)) => {
                        let def_id = t_pred.trait_ref.def_id;
                        match rcx.tcx().lang_items.to_builtin_kind(def_id) {
                            Some(ty::BoundSend) |
                            Some(ty::BoundSized) |
                            Some(ty::BoundCopy) |
                            Some(ty::BoundSync) => false,
                            _ => true,
                        }
                    }
                    ty::Predicate::Equate(..) |
                    ty::Predicate::RegionOutlives(..) |
                    ty::Predicate::TypeOutlives(..) |
                    ty::Predicate::Projection(..) => {
                        // we assume all of these where-clauses may
                        // give the drop implementation the capabilty
                        // to access borrowed data.
                        true
                    }
                };

                if result {
                    debug!("typ: {} has interesting dtor due to generic preds, e.g. {}",
                           typ.repr(rcx.tcx()), pred.repr(rcx.tcx()));
                }

                result
            });

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

            has_dtor_of_interest =
                has_region_param_of_interest ||
                has_pred_of_interest;

            if has_dtor_of_interest {
                debug!("typ: {} has interesting dtor, due to \
                        region params: {} or pred: {}",
                       typ.repr(rcx.tcx()),
                       has_region_param_of_interest,
                       has_pred_of_interest);
            } else {
                debug!("typ: {} has dtor, but it is uninteresting",
                       typ.repr(rcx.tcx()));
            }

        } else {
            debug!("typ: {} has no dtor, and thus is uninteresting",
                   typ.repr(rcx.tcx()));
            has_dtor_of_interest = false;
        }

        if has_dtor_of_interest {
            // If `typ` has a destructor, then we must ensure that all
            // borrowed data reachable via `typ` must outlive the
            // parent of `scope`. (It does not suffice for it to
            // outlive `scope` because that could imply that the
            // borrowed data is torn down in between the end of
            // `scope` and when the destructor itself actually runs.)

            let parent_region =
                match rcx.tcx().region_maps.opt_encl_scope(scope) {
                    Some(parent_scope) => ty::ReScope(parent_scope),
                    None => rcx.tcx().sess.span_bug(
                        span, format!("no enclosing scope found for scope: {:?}",
                                      scope).as_slice()),
                };

            regionck::type_must_outlive(rcx, origin(), typ, parent_region);

        } else {
            // Okay, `typ` itself is itself not reachable by a
            // destructor; but it may contain substructure that has a
            // destructor.

            match typ.sty {
                ty::ty_struct(struct_did, substs) => {
                    // Don't recurse; we extract type's substructure,
                    // so do not process subparts of type expression.
                    walker.skip_current_subtree();

                    let fields =
                        ty::lookup_struct_fields(rcx.tcx(), struct_did);
                    for field in fields.iter() {
                        let field_type =
                            ty::lookup_field_type(rcx.tcx(),
                                                  struct_did,
                                                  field.id,
                                                  substs);
                        iterate_over_potentially_unsafe_regions_in_type(
                            rcx,
                            breadcrumbs,
                            field_type,
                            span,
                            scope,
                            depth+1)
                    }
                }

                ty::ty_enum(enum_did, substs) => {
                    // Don't recurse; we extract type's substructure,
                    // so do not process subparts of type expression.
                    walker.skip_current_subtree();

                    let all_variant_info =
                        ty::substd_enum_variants(rcx.tcx(),
                                                 enum_did,
                                                 substs);
                    for variant_info in all_variant_info.iter() {
                        for argument_type in variant_info.args.iter() {
                            iterate_over_potentially_unsafe_regions_in_type(
                                rcx,
                                breadcrumbs,
                                *argument_type,
                                span,
                                scope,
                                depth+1)
                        }
                    }
                }

                ty::ty_rptr(..) | ty::ty_ptr(_) | ty::ty_bare_fn(..) => {
                    // Don't recurse, since references, pointers,
                    // boxes, and bare functions don't own instances
                    // of the types appearing within them.
                    walker.skip_current_subtree();
                }
                _ => {}
            };

            // You might be tempted to pop breadcrumbs here after
            // processing type's internals above, but then you hit
            // exponential time blowup e.g. on
            // compile-fail/huge-struct.rs. Instead, we do not remove
            // anything from the breadcrumbs vector during any particular
            // traversal, and instead clear it after the whole traversal
            // is done.
        }
    }
}
