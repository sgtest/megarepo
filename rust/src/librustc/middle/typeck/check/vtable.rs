// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use middle::subst::{SelfSpace, FnSpace};
use middle::traits;
use middle::traits::{SelectionError, OutputTypeParameterMismatch, Overflow, Unimplemented};
use middle::traits::{Obligation, obligation_for_builtin_bound};
use middle::traits::{FulfillmentError, CodeSelectionError, CodeAmbiguity};
use middle::traits::{ObligationCause};
use middle::ty;
use middle::typeck::check::{FnCtxt,
                            structurally_resolved_type};
use middle::typeck::infer;
use std::rc::Rc;
use syntax::ast;
use syntax::codemap::Span;
use util::ppaux::{UserString, Repr, ty_to_string};

pub fn check_object_cast(fcx: &FnCtxt,
                         cast_expr: &ast::Expr,
                         source_expr: &ast::Expr,
                         target_object_ty: ty::t)
{
    debug!("check_object_cast(cast_expr={}, target_object_ty={})",
           cast_expr.repr(fcx.tcx()),
           target_object_ty.repr(fcx.tcx()));

    // Look up vtables for the type we're casting to,
    // passing in the source and target type.  The source
    // must be a pointer type suitable to the object sigil,
    // e.g.: `&x as &Trait` or `box x as Box<Trait>`
    let source_ty = fcx.expr_ty(source_expr);
    let source_ty = structurally_resolved_type(fcx, source_expr.span, source_ty);
    debug!("source_ty={}", source_ty.repr(fcx.tcx()));
    match (&ty::get(source_ty).sty, &ty::get(target_object_ty).sty) {
        (&ty::ty_uniq(referent_ty), &ty::ty_uniq(object_trait_ty)) => {
            let object_trait = object_trait(&object_trait_ty);

            // Ensure that if ~T is cast to ~Trait, then T : Trait
            push_cast_obligation(fcx, cast_expr, object_trait, referent_ty);
            check_object_safety(fcx.tcx(), object_trait, source_expr.span);
        }

        (&ty::ty_rptr(referent_region, ty::mt { ty: referent_ty,
                                                mutbl: referent_mutbl }),
         &ty::ty_rptr(target_region, ty::mt { ty: object_trait_ty,
                                              mutbl: target_mutbl })) =>
        {
            let object_trait = object_trait(&object_trait_ty);
            if !mutability_allowed(referent_mutbl, target_mutbl) {
                fcx.tcx().sess.span_err(source_expr.span,
                                        "types differ in mutability");
            } else {
                // Ensure that if &'a T is cast to &'b Trait, then T : Trait
                push_cast_obligation(fcx, cast_expr,
                                     object_trait,
                                     referent_ty);

                // Ensure that if &'a T is cast to &'b Trait, then 'b <= 'a
                infer::mk_subr(fcx.infcx(),
                               infer::RelateObjectBound(source_expr.span),
                               target_region,
                               referent_region);

                check_object_safety(fcx.tcx(), object_trait, source_expr.span);
            }
        }

        (_, &ty::ty_uniq(..)) => {
            fcx.ccx.tcx.sess.span_err(
                source_expr.span,
                format!("can only cast an boxed pointer \
                         to a boxed object, not a {}",
                        ty::ty_sort_string(fcx.tcx(), source_ty)).as_slice());
        }

        (_, &ty::ty_rptr(..)) => {
            fcx.ccx.tcx.sess.span_err(
                source_expr.span,
                format!("can only cast a &-pointer \
                         to an &-object, not a {}",
                        ty::ty_sort_string(fcx.tcx(), source_ty)).as_slice());
        }

        _ => {
            fcx.tcx().sess.span_bug(
                source_expr.span,
                "expected object type");
        }
    }

    // Because we currently give unsound lifetimes to the "t_box", I
    // could have written &'static ty::TyTrait here, but it seems
    // gratuitously unsafe.
    fn object_trait<'a>(t: &'a ty::t) -> &'a ty::TyTrait {
        match ty::get(*t).sty {
            ty::ty_trait(ref ty_trait) => &**ty_trait,
            _ => panic!("expected ty_trait")
        }
    }

    fn mutability_allowed(a_mutbl: ast::Mutability,
                          b_mutbl: ast::Mutability)
                          -> bool {
        a_mutbl == b_mutbl ||
            (a_mutbl == ast::MutMutable && b_mutbl == ast::MutImmutable)
    }

    fn push_cast_obligation(fcx: &FnCtxt,
                            cast_expr: &ast::Expr,
                            object_trait: &ty::TyTrait,
                            referent_ty: ty::t) {
        let object_trait_ref =
            register_object_cast_obligations(fcx,
                                             cast_expr.span,
                                             object_trait,
                                             referent_ty);

        // Finally record the object_trait_ref for use during trans
        // (it would prob be better not to do this, but it's just kind
        // of a pain to have to reconstruct it).
        fcx.write_object_cast(cast_expr.id, object_trait_ref);
    }
}

// Check that a trait is 'object-safe'. This should be checked whenever a trait object
// is created (by casting or coercion, etc.). A trait is object-safe if all its
// methods are object-safe. A trait method is object-safe if it does not take
// self by value, has no type parameters and does not use the `Self` type, except
// in self position.
pub fn check_object_safety(tcx: &ty::ctxt, object_trait: &ty::TyTrait, span: Span) {
    // Skip the fn_once lang item trait since only the compiler should call
    // `call_once` which is the method which takes self by value. What could go
    // wrong?
    match tcx.lang_items.fn_once_trait() {
        Some(def_id) if def_id == object_trait.def_id => return,
        _ => {}
    }

    let trait_items = ty::trait_items(tcx, object_trait.def_id);

    let mut errors = Vec::new();
    for item in trait_items.iter() {
        match *item {
            ty::MethodTraitItem(ref m) => {
                errors.push(check_object_safety_of_method(tcx, &**m))
            }
            ty::TypeTraitItem(_) => {}
        }
    }

    let mut errors = errors.iter().flat_map(|x| x.iter()).peekable();
    if errors.peek().is_some() {
        let trait_name = ty::item_path_str(tcx, object_trait.def_id);
        span_err!(tcx.sess, span, E0038,
            "cannot convert to a trait object because trait `{}` is not object-safe",
            trait_name);

        for msg in errors {
            tcx.sess.note(msg.as_slice());
        }
    }

    // Returns a vec of error messages. If hte vec is empty - no errors!
    fn check_object_safety_of_method(tcx: &ty::ctxt, method: &ty::Method) -> Vec<String> {
        /*!
         * There are some limitations to calling functions through an
         * object, because (a) the self type is not known
         * (that's the whole point of a trait instance, after all, to
         * obscure the self type) and (b) the call must go through a
         * vtable and hence cannot be monomorphized.
         */
        let mut msgs = Vec::new();

        let method_name = method.name.repr(tcx);

        match method.explicit_self {
            ty::ByValueExplicitSelfCategory => { // reason (a) above
                msgs.push(format!("cannot call a method (`{}`) with a by-value \
                                   receiver through a trait object", method_name))
            }

            ty::StaticExplicitSelfCategory |
            ty::ByReferenceExplicitSelfCategory(..) |
            ty::ByBoxExplicitSelfCategory => {}
        }

        // reason (a) above
        let check_for_self_ty = |ty| {
            if ty::type_has_self(ty) {
                Some(format!(
                    "cannot call a method (`{}`) whose type contains \
                     a self-type (`{}`) through a trait object",
                    method_name, ty_to_string(tcx, ty)))
            } else {
                None
            }
        };
        let ref sig = method.fty.sig;
        for &input_ty in sig.inputs[1..].iter() {
            match check_for_self_ty(input_ty) {
                Some(msg) => msgs.push(msg),
                _ => {}
            }
        }
        if let ty::FnConverging(result_type) = sig.output {
            match check_for_self_ty(result_type) {
                Some(msg) => msgs.push(msg),
                _ => {}
            }
        }

        if method.generics.has_type_params(FnSpace) {
            // reason (b) above
            msgs.push(format!("cannot call a generic method (`{}`) through a trait object",
                              method_name));
        }

        msgs
    }
}

pub fn register_object_cast_obligations(fcx: &FnCtxt,
                                        span: Span,
                                        object_trait: &ty::TyTrait,
                                        referent_ty: ty::t)
                                        -> Rc<ty::TraitRef>
{
    // This is just for better error reporting. Kinda goofy. The object type stuff
    // needs some refactoring so there is a more convenient type to pass around.
    let object_trait_ty =
        ty::mk_trait(fcx.tcx(),
                     object_trait.def_id,
                     object_trait.substs.clone(),
                     object_trait.bounds);

    debug!("register_object_cast_obligations: referent_ty={} object_trait_ty={}",
           referent_ty.repr(fcx.tcx()),
           object_trait_ty.repr(fcx.tcx()));

    // Take the type parameters from the object type, but set
    // the Self type (which is unknown, for the object type)
    // to be the type we are casting from.
    let mut object_substs = object_trait.substs.clone();
    assert!(object_substs.self_ty().is_none());
    object_substs.types.push(SelfSpace, referent_ty);

    // Create the obligation for casting from T to Trait.
    let object_trait_ref =
        Rc::new(ty::TraitRef { def_id: object_trait.def_id,
                               substs: object_substs });
    let object_obligation =
        Obligation::new(
            ObligationCause::new(span,
                                 traits::ObjectCastObligation(object_trait_ty)),
            object_trait_ref.clone());
    fcx.register_obligation(object_obligation);

    // Create additional obligations for all the various builtin
    // bounds attached to the object cast. (In other words, if the
    // object type is Foo+Send, this would create an obligation
    // for the Send check.)
    for builtin_bound in object_trait.bounds.builtin_bounds.iter() {
            let obligation = obligation_for_builtin_bound(
                fcx.tcx(),
                ObligationCause::new(span,
                                     traits::ObjectCastObligation(object_trait_ty)),
                referent_ty,
                builtin_bound);
            match obligation {
                Ok(obligation) => fcx.register_obligation(obligation),
                _ => {}
            }
    }

    object_trait_ref
}

pub fn select_all_fcx_obligations_or_error(fcx: &FnCtxt) {
    debug!("select_all_fcx_obligations_or_error");

    let mut fulfillment_cx = fcx.inh.fulfillment_cx.borrow_mut();
    let r = fulfillment_cx.select_all_or_error(fcx.infcx(),
                                               &fcx.inh.param_env,
                                               fcx);
    match r {
        Ok(()) => { }
        Err(errors) => { report_fulfillment_errors(fcx, &errors); }
    }
}

fn resolve_trait_ref(fcx: &FnCtxt, obligation: &Obligation)
                     -> (ty::TraitRef, ty::t)
{
    let trait_ref =
        fcx.infcx().resolve_type_vars_in_trait_ref_if_possible(
            &*obligation.trait_ref);
    let self_ty =
        trait_ref.substs.self_ty().unwrap();
    (trait_ref, self_ty)
}

pub fn report_fulfillment_errors(fcx: &FnCtxt,
                                 errors: &Vec<FulfillmentError>) {
    for error in errors.iter() {
        report_fulfillment_error(fcx, error);
    }
}

pub fn report_fulfillment_error(fcx: &FnCtxt,
                                error: &FulfillmentError) {
    match error.code {
        CodeSelectionError(ref e) => {
            report_selection_error(fcx, &error.obligation, e);
        }
        CodeAmbiguity => {
            maybe_report_ambiguity(fcx, &error.obligation);
        }
    }
}

pub fn report_selection_error(fcx: &FnCtxt,
                              obligation: &Obligation,
                              error: &SelectionError)
{
    match *error {
        Overflow => {
            let (trait_ref, self_ty) = resolve_trait_ref(fcx, obligation);
            fcx.tcx().sess.span_err(
                obligation.cause.span,
                format!(
                    "overflow evaluating the trait `{}` for the type `{}`",
                    trait_ref.user_string(fcx.tcx()),
                    self_ty.user_string(fcx.tcx())).as_slice());
            note_obligation_cause(fcx, obligation);
        }
        Unimplemented => {
            let (trait_ref, self_ty) = resolve_trait_ref(fcx, obligation);
            if !ty::type_is_error(self_ty) {
                fcx.tcx().sess.span_err(
                    obligation.cause.span,
                    format!(
                        "the trait `{}` is not implemented for the type `{}`",
                        trait_ref.user_string(fcx.tcx()),
                        self_ty.user_string(fcx.tcx())).as_slice());
                note_obligation_cause(fcx, obligation);
            }
        }
        OutputTypeParameterMismatch(ref expected_trait_ref, ref e) => {
            let expected_trait_ref =
                fcx.infcx().resolve_type_vars_in_trait_ref_if_possible(
                    &**expected_trait_ref);
            let (trait_ref, self_ty) = resolve_trait_ref(fcx, obligation);
            if !ty::type_is_error(self_ty) {
                fcx.tcx().sess.span_err(
                    obligation.cause.span,
                    format!(
                        "type mismatch: the type `{}` implements the trait `{}`, \
                         but the trait `{}` is required ({})",
                        self_ty.user_string(fcx.tcx()),
                        expected_trait_ref.user_string(fcx.tcx()),
                        trait_ref.user_string(fcx.tcx()),
                        ty::type_err_to_str(fcx.tcx(), e)).as_slice());
                note_obligation_cause(fcx, obligation);
            }
        }
    }
}

pub fn maybe_report_ambiguity(fcx: &FnCtxt, obligation: &Obligation) {
    // Unable to successfully determine, probably means
    // insufficient type information, but could mean
    // ambiguous impls. The latter *ought* to be a
    // coherence violation, so we don't report it here.
    let (trait_ref, self_ty) = resolve_trait_ref(fcx, obligation);
    debug!("maybe_report_ambiguity(trait_ref={}, self_ty={}, obligation={})",
           trait_ref.repr(fcx.tcx()),
           self_ty.repr(fcx.tcx()),
           obligation.repr(fcx.tcx()));
    let all_types = &trait_ref.substs.types;
    if all_types.iter().any(|&t| ty::type_is_error(t)) {
    } else if all_types.iter().any(|&t| ty::type_needs_infer(t)) {
        // This is kind of a hack: it frequently happens that some earlier
        // error prevents types from being fully inferred, and then we get
        // a bunch of uninteresting errors saying something like "<generic
        // #0> doesn't implement Sized".  It may even be true that we
        // could just skip over all checks where the self-ty is an
        // inference variable, but I was afraid that there might be an
        // inference variable created, registered as an obligation, and
        // then never forced by writeback, and hence by skipping here we'd
        // be ignoring the fact that we don't KNOW the type works
        // out. Though even that would probably be harmless, given that
        // we're only talking about builtin traits, which are known to be
        // inhabited. But in any case I just threw in this check for
        // has_errors() to be sure that compilation isn't happening
        // anyway. In that case, why inundate the user.
        if !fcx.tcx().sess.has_errors() {
            if fcx.ccx.tcx.lang_items.sized_trait()
                  .map_or(false, |sized_id| sized_id == trait_ref.def_id) {
                fcx.tcx().sess.span_err(
                    obligation.cause.span,
                    format!(
                        "unable to infer enough type information about `{}`; type annotations \
                         required",
                        self_ty.user_string(fcx.tcx())).as_slice());
            } else {
                fcx.tcx().sess.span_err(
                    obligation.cause.span,
                    format!(
                        "unable to infer enough type information to \
                         locate the impl of the trait `{}` for \
                         the type `{}`; type annotations required",
                        trait_ref.user_string(fcx.tcx()),
                        self_ty.user_string(fcx.tcx())).as_slice());
                note_obligation_cause(fcx, obligation);
            }
        }
    } else if !fcx.tcx().sess.has_errors() {
         // Ambiguity. Coherence should have reported an error.
        fcx.tcx().sess.span_bug(
            obligation.cause.span,
            format!(
                "coherence failed to report ambiguity: \
                 cannot locate the impl of the trait `{}` for \
                 the type `{}`",
                trait_ref.user_string(fcx.tcx()),
                self_ty.user_string(fcx.tcx())).as_slice());
    }
}

pub fn select_fcx_obligations_where_possible(fcx: &FnCtxt) {
    /*! Select as many obligations as we can at present. */

    match
        fcx.inh.fulfillment_cx
        .borrow_mut()
        .select_where_possible(fcx.infcx(), &fcx.inh.param_env, fcx)
    {
        Ok(()) => { }
        Err(errors) => { report_fulfillment_errors(fcx, &errors); }
    }
}

pub fn select_new_fcx_obligations(fcx: &FnCtxt) {
    /*!
     * Try to select any fcx obligation that we haven't tried yet,
     * in an effort to improve inference. You could just call
     * `select_fcx_obligations_where_possible` except that it leads
     * to repeated work.
     */

    match
        fcx.inh.fulfillment_cx
        .borrow_mut()
        .select_new_obligations(fcx.infcx(), &fcx.inh.param_env, fcx)
    {
        Ok(()) => { }
        Err(errors) => { report_fulfillment_errors(fcx, &errors); }
    }
}

fn note_obligation_cause(fcx: &FnCtxt,
                         obligation: &Obligation) {
    let tcx = fcx.tcx();
    let trait_name = ty::item_path_str(tcx, obligation.trait_ref.def_id);
    match obligation.cause.code {
        traits::MiscObligation => { }
        traits::ItemObligation(item_def_id) => {
            let item_name = ty::item_path_str(tcx, item_def_id);
            tcx.sess.span_note(
                obligation.cause.span,
                format!(
                    "the trait `{}` must be implemented because it is required by `{}`",
                    trait_name,
                    item_name).as_slice());
        }
        traits::ObjectCastObligation(object_ty) => {
            tcx.sess.span_note(
                obligation.cause.span,
                format!(
                    "the trait `{}` must be implemented for the cast \
                     to the object type `{}`",
                    trait_name,
                    fcx.infcx().ty_to_string(object_ty)).as_slice());
        }
        traits::RepeatVec => {
            tcx.sess.span_note(
                obligation.cause.span,
                "the `Copy` trait is required because the \
                 repeated element will be copied");
        }
        traits::VariableType(_) => {
            tcx.sess.span_note(
                obligation.cause.span,
                "all local variables must have a statically known size");
        }
        traits::ReturnType => {
            tcx.sess.span_note(
                obligation.cause.span,
                "the return type of a function must have a \
                 statically known size");
        }
        traits::AssignmentLhsSized => {
            tcx.sess.span_note(
                obligation.cause.span,
                "the left-hand-side of an assignment must have a statically known size");
        }
        traits::StructInitializerSized => {
            tcx.sess.span_note(
                obligation.cause.span,
                "structs must have a statically known size to be initialized");
        }
        traits::DropTrait => {
            span_note!(tcx.sess, obligation.cause.span,
                      "cannot implement a destructor on a \
                      structure or enumeration that does not satisfy Send");
            span_note!(tcx.sess, obligation.cause.span,
                       "use \"#[unsafe_destructor]\" on the implementation \
                       to force the compiler to allow this");
        }
        traits::ClosureCapture(var_id, closure_span) => {
            let name = ty::local_var_name_str(tcx, var_id);
            span_note!(tcx.sess, closure_span,
                       "the closure that captures `{}` requires that all captured variables \"
                       implement the trait `{}`",
                       name,
                       trait_name);
        }
        traits::FieldSized => {
            span_note!(tcx.sess, obligation.cause.span,
                       "only the last field of a struct or enum variant \
                       may have a dynamically sized type")
        }
    }
}
