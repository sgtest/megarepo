// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! See `doc.rs` for high-level documentation

use super::SelectionContext;
use super::Obligation;
use super::util;

use middle::subst;
use middle::subst::Subst;
use middle::ty::{mod, Ty};
use middle::typeck::infer::{mod, InferCtxt};
use syntax::ast;
use syntax::codemap::DUMMY_SP;
use util::ppaux::Repr;

pub fn impl_can_satisfy(infcx: &InferCtxt,
                        impl1_def_id: ast::DefId,
                        impl2_def_id: ast::DefId)
                        -> bool
{
    debug!("impl_can_satisfy(\
           impl1_def_id={}, \
           impl2_def_id={})",
           impl1_def_id.repr(infcx.tcx),
           impl2_def_id.repr(infcx.tcx));

    // `impl1` provides an implementation of `Foo<X,Y> for Z`.
    let impl1_substs =
        util::fresh_substs_for_impl(infcx, DUMMY_SP, impl1_def_id);
    let impl1_trait_ref =
        ty::impl_trait_ref(infcx.tcx, impl1_def_id).unwrap()
                                                   .subst(infcx.tcx, &impl1_substs);
    let impl1_trait_ref =
        infcx.replace_late_bound_regions_with_fresh_var(DUMMY_SP,
                                                        infer::FnCall,
                                                        &impl1_trait_ref).0;

    // Determine whether `impl2` can provide an implementation for those
    // same types.
    let param_env = ty::empty_parameter_environment();
    let mut selcx = SelectionContext::intercrate(infcx, &param_env, infcx.tcx);
    let obligation = Obligation::misc(DUMMY_SP, impl1_trait_ref);
    debug!("impl_can_satisfy(obligation={})", obligation.repr(infcx.tcx));
    selcx.evaluate_impl(impl2_def_id, &obligation)
}

pub fn impl_is_local(tcx: &ty::ctxt,
                     impl_def_id: ast::DefId)
                     -> bool
{
    debug!("impl_is_local({})", impl_def_id.repr(tcx));

    // We only except this routine to be invoked on implementations
    // of a trait, not inherent implementations.
    let trait_ref = ty::impl_trait_ref(tcx, impl_def_id).unwrap();
    debug!("trait_ref={}", trait_ref.repr(tcx));

    // If the trait is local to the crate, ok.
    if trait_ref.def_id.krate == ast::LOCAL_CRATE {
        debug!("trait {} is local to current crate",
               trait_ref.def_id.repr(tcx));
        return true;
    }

    // Otherwise, at least one of the input types must be local to the
    // crate.
    trait_ref.input_types().iter().any(|&t| ty_is_local(tcx, t))
}

pub fn ty_is_local<'tcx>(tcx: &ty::ctxt<'tcx>, ty: Ty<'tcx>) -> bool {
    debug!("ty_is_local({})", ty.repr(tcx));

    match ty.sty {
        ty::ty_bool |
        ty::ty_char |
        ty::ty_int(..) |
        ty::ty_uint(..) |
        ty::ty_float(..) |
        ty::ty_str(..) => {
            false
        }

        ty::ty_unboxed_closure(..) => {
            // This routine is invoked on types specified by users as
            // part of an impl and hence an unboxed closure type
            // cannot appear.
            tcx.sess.bug("ty_is_local applied to unboxed closure type")
        }

        ty::ty_bare_fn(..) |
        ty::ty_closure(..) => {
            false
        }

        ty::ty_uniq(t) => {
            let krate = tcx.lang_items.owned_box().map(|d| d.krate);
            krate == Some(ast::LOCAL_CRATE) || ty_is_local(tcx, t)
        }

        ty::ty_vec(t, _) |
        ty::ty_ptr(ty::mt { ty: t, .. }) |
        ty::ty_rptr(_, ty::mt { ty: t, .. }) => {
            ty_is_local(tcx, t)
        }

        ty::ty_tup(ref ts) => {
            ts.iter().any(|&t| ty_is_local(tcx, t))
        }

        ty::ty_enum(def_id, ref substs) |
        ty::ty_struct(def_id, ref substs) => {
            def_id.krate == ast::LOCAL_CRATE || {
                let variances = ty::item_variances(tcx, def_id);
                subst::ParamSpace::all().iter().any(|&space| {
                    substs.types.get_slice(space).iter().enumerate().any(
                        |(i, &t)| {
                            match *variances.types.get(space, i) {
                                ty::Bivariant => {
                                    // If Foo<T> is bivariant with respect to
                                    // T, then it doesn't matter whether T is
                                    // local or not, because `Foo<U>` for any
                                    // U will be a subtype of T.
                                    false
                                }
                                ty::Contravariant |
                                ty::Covariant |
                                ty::Invariant => {
                                    ty_is_local(tcx, t)
                                }
                            }
                        })
                })
            }
        }

        ty::ty_trait(ref tt) => {
            tt.principal.def_id.krate == ast::LOCAL_CRATE
        }

        // Type parameters may be bound to types that are not local to
        // the crate.
        ty::ty_param(..) => {
            false
        }

        ty::ty_infer(..) |
        ty::ty_open(..) |
        ty::ty_err => {
            tcx.sess.bug(
                format!("ty_is_local invoked on unexpected type: {}",
                        ty.repr(tcx)).as_slice())
        }
    }
}
