// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use middle::freevars::freevar_entry;
use middle::freevars;
use middle::ty;
use middle::typeck;
use util::ppaux::{Repr, ty_to_str};
use util::ppaux::UserString;

use syntax::ast::*;
use syntax::attr::attrs_contains_name;
use syntax::codemap::span;
use syntax::print::pprust::expr_to_str;
use syntax::{visit, ast_util};

// Kind analysis pass.
//
// There are several kinds defined by various operations. The most restrictive
// kind is noncopyable. The noncopyable kind can be extended with any number
// of the following attributes.
//
//  send: Things that can be sent on channels or included in spawned closures.
//  freeze: Things thare are deeply immutable. They are guaranteed never to
//    change, and can be safely shared without copying between tasks.
//  'static: Things that do not contain borrowed pointers.
//
// Send includes scalar types as well as classes and unique types containing
// only sendable types.
//
// Freeze include scalar types, things without non-const fields, and pointers
// to freezable things.
//
// This pass ensures that type parameters are only instantiated with types
// whose kinds are equal or less general than the way the type parameter was
// annotated (with the `Send` or `Freeze` bound).
//
// It also verifies that noncopyable kinds are not copied. Sendability is not
// applied, since none of our language primitives send. Instead, the sending
// primitives in the stdlib are explicitly annotated to only take sendable
// types.

pub static try_adding: &'static str = "Try adding a move";

#[deriving(Clone)]
pub struct Context {
    tcx: ty::ctxt,
    method_map: typeck::method_map,
    current_item: node_id
}

pub fn check_crate(tcx: ty::ctxt,
                   method_map: typeck::method_map,
                   crate: &crate) {
    let ctx = Context {
        tcx: tcx,
        method_map: method_map,
        current_item: -1
    };
    let visit = visit::mk_vt(@visit::Visitor {
        visit_expr: check_expr,
        visit_fn: check_fn,
        visit_ty: check_ty,
        visit_item: check_item,
        visit_block: check_block,
        .. *visit::default_visitor()
    });
    visit::visit_crate(crate, (ctx, visit));
    tcx.sess.abort_if_errors();
}

fn check_struct_safe_for_destructor(cx: Context,
                                    span: span,
                                    struct_did: def_id) {
    let struct_tpt = ty::lookup_item_type(cx.tcx, struct_did);
    if !struct_tpt.generics.has_type_params() {
        let struct_ty = ty::mk_struct(cx.tcx, struct_did, ty::substs {
            self_r: None,
            self_ty: None,
            tps: ~[]
        });
        if !ty::type_is_sendable(cx.tcx, struct_ty) {
            cx.tcx.sess.span_err(span,
                                 "cannot implement a destructor on a \
                                  structure that does not satisfy Send");
            cx.tcx.sess.span_note(span,
                                  "use \"#[unsafe_destructor]\" on the \
                                   implementation to force the compiler to \
                                   allow this");
        }
    } else {
        cx.tcx.sess.span_err(span,
                             "cannot implement a destructor on a structure \
                              with type parameters");
        cx.tcx.sess.span_note(span,
                              "use \"#[unsafe_destructor]\" on the \
                               implementation to force the compiler to \
                               allow this");
    }
}

fn check_block(block: &blk, (cx, visitor): (Context, visit::vt<Context>)) {
    visit::visit_block(block, (cx, visitor));
}

fn check_item(item: @item, (cx, visitor): (Context, visit::vt<Context>)) {
    // If this is a destructor, check kinds.
    if !attrs_contains_name(item.attrs, "unsafe_destructor") {
        match item.node {
            item_impl(_, Some(ref trait_ref), ref self_type, _) => {
                match cx.tcx.def_map.find(&trait_ref.ref_id) {
                    None => cx.tcx.sess.bug("trait ref not in def map!"),
                    Some(&trait_def) => {
                        let trait_def_id = ast_util::def_id_of_def(trait_def);
                        if cx.tcx.lang_items.drop_trait() == Some(trait_def_id) {
                            // Yes, it's a destructor.
                            match self_type.node {
                                ty_path(_, ref bounds, path_node_id) => {
                                    assert!(bounds.is_none());
                                    let struct_def = cx.tcx.def_map.get_copy(
                                        &path_node_id);
                                    let struct_did =
                                        ast_util::def_id_of_def(struct_def);
                                    check_struct_safe_for_destructor(
                                        cx,
                                        self_type.span,
                                        struct_did);
                                }
                                _ => {
                                    cx.tcx.sess.span_bug(self_type.span,
                                                         "the self type for \
                                                          the Drop trait \
                                                          impl is not a \
                                                          path");
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let cx = Context { current_item: item.id, ..cx };
    visit::visit_item(item, (cx, visitor));
}

// Yields the appropriate function to check the kind of closed over
// variables. `id` is the node_id for some expression that creates the
// closure.
fn with_appropriate_checker(cx: Context, id: node_id,
                            b: &fn(checker: &fn(Context, @freevar_entry))) {
    fn check_for_uniq(cx: Context, fv: &freevar_entry, bounds: ty::BuiltinBounds) {
        // all captured data must be owned, regardless of whether it is
        // moved in or copied in.
        let id = ast_util::def_id_of_def(fv.def).node;
        let var_t = ty::node_id_to_type(cx.tcx, id);

        // check that only immutable variables are implicitly copied in
        check_imm_free_var(cx, fv.def, fv.span);

        check_freevar_bounds(cx, fv.span, var_t, bounds, None);
    }

    fn check_for_box(cx: Context, fv: &freevar_entry, bounds: ty::BuiltinBounds) {
        // all captured data must be owned
        let id = ast_util::def_id_of_def(fv.def).node;
        let var_t = ty::node_id_to_type(cx.tcx, id);

        // check that only immutable variables are implicitly copied in
        check_imm_free_var(cx, fv.def, fv.span);

        check_freevar_bounds(cx, fv.span, var_t, bounds, None);
    }

    fn check_for_block(cx: Context, fv: &freevar_entry,
                       bounds: ty::BuiltinBounds, region: ty::Region) {
        let id = ast_util::def_id_of_def(fv.def).node;
        let var_t = ty::node_id_to_type(cx.tcx, id);
        // FIXME(#3569): Figure out whether the implicit borrow is actually
        // mutable. Currently we assume all upvars are referenced mutably.
        let implicit_borrowed_type = ty::mk_mut_rptr(cx.tcx, region, var_t);
        check_freevar_bounds(cx, fv.span, implicit_borrowed_type,
                             bounds, Some(var_t));
    }

    fn check_for_bare(cx: Context, fv: @freevar_entry) {
        cx.tcx.sess.span_err(
            fv.span,
            "can't capture dynamic environment in a fn item; \
            use the || { ... } closure form instead");
    } // same check is done in resolve.rs, but shouldn't be done

    let fty = ty::node_id_to_type(cx.tcx, id);
    match ty::get(fty).sty {
        ty::ty_closure(ty::ClosureTy {sigil: OwnedSigil, bounds: bounds, _}) => {
            b(|cx, fv| check_for_uniq(cx, fv, bounds))
        }
        ty::ty_closure(ty::ClosureTy {sigil: ManagedSigil, bounds: bounds, _}) => {
            b(|cx, fv| check_for_box(cx, fv, bounds))
        }
        ty::ty_closure(ty::ClosureTy {sigil: BorrowedSigil, bounds: bounds,
                                      region: region, _}) => {
            b(|cx, fv| check_for_block(cx, fv, bounds, region))
        }
        ty::ty_bare_fn(_) => {
            b(check_for_bare)
        }
        ref s => {
            cx.tcx.sess.bug(
                fmt!("expect fn type in kind checker, not %?", s));
        }
    }
}

// Check that the free variables used in a shared/sendable closure conform
// to the copy/move kind bounds. Then recursively check the function body.
fn check_fn(
    fk: &visit::fn_kind,
    decl: &fn_decl,
    body: &blk,
    sp: span,
    fn_id: node_id,
    (cx, v): (Context,
              visit::vt<Context>)) {

    // Check kinds on free variables:
    do with_appropriate_checker(cx, fn_id) |chk| {
        let r = freevars::get_freevars(cx.tcx, fn_id);
        for r.iter().advance |fv| {
            chk(cx, *fv);
        }
    }

    visit::visit_fn(fk, decl, body, sp, fn_id, (cx, v));
}

pub fn check_expr(e: @expr, (cx, v): (Context, visit::vt<Context>)) {
    debug!("kind::check_expr(%s)", expr_to_str(e, cx.tcx.sess.intr()));

    // Handle any kind bounds on type parameters
    let type_parameter_id = match e.get_callee_id() {
        Some(callee_id) => callee_id,
        None => e.id,
    };
    {
        let r = cx.tcx.node_type_substs.find(&type_parameter_id);
        for r.iter().advance |ts| {
            let type_param_defs = match e.node {
              expr_path(_) => {
                let did = ast_util::def_id_of_def(cx.tcx.def_map.get_copy(&e.id));
                ty::lookup_item_type(cx.tcx, did).generics.type_param_defs
              }
              _ => {
                // Type substitutions should only occur on paths and
                // method calls, so this needs to be a method call.

                // Even though the callee_id may have been the id with
                // node_type_substs, e.id is correct here.
                ty::method_call_type_param_defs(cx.tcx, cx.method_map, e.id).expect(
                    "non path/method call expr has type substs??")
              }
            };
            if ts.len() != type_param_defs.len() {
                // Fail earlier to make debugging easier
                fail!("internal error: in kind::check_expr, length \
                      mismatch between actual and declared bounds: actual = \
                      %s, declared = %s",
                      ts.repr(cx.tcx),
                      type_param_defs.repr(cx.tcx));
            }
            for ts.iter().zip(type_param_defs.iter()).advance |(&ty, type_param_def)| {
                check_typaram_bounds(cx, type_parameter_id, e.span, ty, type_param_def)
            }
        }
    }

    match e.node {
        expr_unary(_, box(_), interior) => {
            let interior_type = ty::expr_ty(cx.tcx, interior);
            let _ = check_durable(cx.tcx, interior_type, interior.span);
        }
        expr_cast(source, _) => {
            check_cast_for_escaping_regions(cx, source, e);
            match ty::get(ty::expr_ty(cx.tcx, e)).sty {
                ty::ty_trait(_, _, _, _, bounds) => {
                    let source_ty = ty::expr_ty(cx.tcx, source);
                    check_trait_cast_bounds(cx, e.span, source_ty, bounds)
                }
                _ => { }
            }
        }
        expr_repeat(element, count_expr, _) => {
            let count = ty::eval_repeat_count(&cx.tcx, count_expr);
            if count > 1 {
                let element_ty = ty::expr_ty(cx.tcx, element);
                check_copy(cx, element_ty, element.span,
                           "repeated element will be copied");
            }
        }
        _ => {}
    }
    visit::visit_expr(e, (cx, v));
}

fn check_ty(aty: &Ty, (cx, v): (Context, visit::vt<Context>)) {
    match aty.node {
      ty_path(_, _, id) => {
          let r = cx.tcx.node_type_substs.find(&id);
          for r.iter().advance |ts| {
              let did = ast_util::def_id_of_def(cx.tcx.def_map.get_copy(&id));
              let type_param_defs =
                  ty::lookup_item_type(cx.tcx, did).generics.type_param_defs;
              for ts.iter().zip(type_param_defs.iter()).advance |(&ty, type_param_def)| {
                  check_typaram_bounds(cx, aty.id, aty.span, ty, type_param_def)
              }
          }
      }
      _ => {}
    }
    visit::visit_ty(aty, (cx, v));
}

// Calls "any_missing" if any bounds were missing.
pub fn check_builtin_bounds(cx: Context, ty: ty::t, bounds: ty::BuiltinBounds,
                            any_missing: &fn(ty::BuiltinBounds))
{
    let kind = ty::type_contents(cx.tcx, ty);
    let mut missing = ty::EmptyBuiltinBounds();
    for bounds.each |bound| {
        if !kind.meets_bound(cx.tcx, bound) {
            missing.add(bound);
        }
    }
    if !missing.is_empty() {
        any_missing(missing);
    }
}

pub fn check_typaram_bounds(cx: Context,
                    _type_parameter_id: node_id,
                    sp: span,
                    ty: ty::t,
                    type_param_def: &ty::TypeParameterDef)
{
    do check_builtin_bounds(cx, ty, type_param_def.bounds.builtin_bounds) |missing| {
        cx.tcx.sess.span_err(
            sp,
            fmt!("instantiating a type parameter with an incompatible type \
                  `%s`, which does not fulfill `%s`",
                 ty_to_str(cx.tcx, ty),
                 missing.user_string(cx.tcx)));
    }
}

pub fn check_freevar_bounds(cx: Context, sp: span, ty: ty::t,
                            bounds: ty::BuiltinBounds, referenced_ty: Option<ty::t>)
{
    do check_builtin_bounds(cx, ty, bounds) |missing| {
        // Will be Some if the freevar is implicitly borrowed (stack closure).
        // Emit a less mysterious error message in this case.
        match referenced_ty {
            Some(rty) => cx.tcx.sess.span_err(sp,
                fmt!("cannot implicitly borrow variable of type `%s` in a bounded \
                      stack closure (implicit reference does not fulfill `%s`)",
                     ty_to_str(cx.tcx, rty), missing.user_string(cx.tcx))),
            None => cx.tcx.sess.span_err(sp,
                fmt!("cannot capture variable of type `%s`, which does \
                      not fulfill `%s`, in a bounded closure",
                     ty_to_str(cx.tcx, ty), missing.user_string(cx.tcx))),
        }
        cx.tcx.sess.span_note(
            sp,
            fmt!("this closure's environment must satisfy `%s`",
                 bounds.user_string(cx.tcx)));
    }
}

pub fn check_trait_cast_bounds(cx: Context, sp: span, ty: ty::t,
                               bounds: ty::BuiltinBounds) {
    do check_builtin_bounds(cx, ty, bounds) |missing| {
        cx.tcx.sess.span_err(sp,
            fmt!("cannot pack type `%s`, which does not fulfill \
                  `%s`, as a trait bounded by %s",
                 ty_to_str(cx.tcx, ty), missing.user_string(cx.tcx),
                 bounds.user_string(cx.tcx)));
    }
}

fn is_nullary_variant(cx: Context, ex: @expr) -> bool {
    match ex.node {
      expr_path(_) => {
        match cx.tcx.def_map.get_copy(&ex.id) {
          def_variant(edid, vdid) => {
              ty::enum_variant_with_id(cx.tcx, edid, vdid).args.is_empty()
          }
          _ => false
        }
      }
      _ => false
    }
}

fn check_imm_free_var(cx: Context, def: def, sp: span) {
    match def {
        def_local(_, is_mutbl) => {
            if is_mutbl {
                cx.tcx.sess.span_err(
                    sp,
                    "mutable variables cannot be implicitly captured");
            }
        }
        def_arg(*) => { /* ok */ }
        def_upvar(_, def1, _, _) => { check_imm_free_var(cx, *def1, sp); }
        def_binding(*) | def_self(*) => { /*ok*/ }
        _ => {
            cx.tcx.sess.span_bug(
                sp,
                fmt!("unknown def for free variable: %?", def));
        }
    }
}

fn check_copy(cx: Context, ty: ty::t, sp: span, reason: &str) {
    debug!("type_contents(%s)=%s",
           ty_to_str(cx.tcx, ty),
           ty::type_contents(cx.tcx, ty).to_str());
    if ty::type_moves_by_default(cx.tcx, ty) {
        cx.tcx.sess.span_err(
            sp, fmt!("copying a value of non-copyable type `%s`",
                     ty_to_str(cx.tcx, ty)));
        cx.tcx.sess.span_note(sp, fmt!("%s", reason));
    }
}

pub fn check_send(cx: Context, ty: ty::t, sp: span) -> bool {
    if !ty::type_is_sendable(cx.tcx, ty) {
        cx.tcx.sess.span_err(
            sp, fmt!("value has non-sendable type `%s`",
                     ty_to_str(cx.tcx, ty)));
        false
    } else {
        true
    }
}

// note: also used from middle::typeck::regionck!
pub fn check_durable(tcx: ty::ctxt, ty: ty::t, sp: span) -> bool {
    if !ty::type_is_static(tcx, ty) {
        match ty::get(ty).sty {
          ty::ty_param(*) => {
            tcx.sess.span_err(sp, "value may contain borrowed \
                                   pointers; add `'static` bound");
          }
          _ => {
            tcx.sess.span_err(sp, "value may contain borrowed \
                                   pointers");
          }
        }
        false
    } else {
        true
    }
}

/// This is rather subtle.  When we are casting a value to a instantiated
/// trait like `a as trait<'r>`, regionck already ensures that any borrowed
/// pointers that appear in the type of `a` are bounded by `'r` (ed.: rem
/// FIXME(#5723)).  However, it is possible that there are *type parameters*
/// in the type of `a`, and those *type parameters* may have borrowed pointers
/// within them.  We have to guarantee that the regions which appear in those
/// type parameters are not obscured.
///
/// Therefore, we ensure that one of three conditions holds:
///
/// (1) The trait instance cannot escape the current fn.  This is
/// guaranteed if the region bound `&r` is some scope within the fn
/// itself.  This case is safe because whatever borrowed pointers are
/// found within the type parameter, they must enclose the fn body
/// itself.
///
/// (2) The type parameter appears in the type of the trait.  For
/// example, if the type parameter is `T` and the trait type is
/// `deque<T>`, then whatever borrowed ptrs may appear in `T` also
/// appear in `deque<T>`.
///
/// (3) The type parameter is sendable (and therefore does not contain
/// borrowed ptrs).
///
/// FIXME(#5723)---This code should probably move into regionck.
pub fn check_cast_for_escaping_regions(
    cx: Context,
    source: &expr,
    target: &expr)
{
    // Determine what type we are casting to; if it is not an trait, then no
    // worries.
    let target_ty = ty::expr_ty(cx.tcx, target);
    match ty::get(target_ty).sty {
        ty::ty_trait(*) => {}
        _ => { return; }
    }

    // Collect up the regions that appear in the target type.  We want to
    // ensure that these lifetimes are shorter than all lifetimes that are in
    // the source type.  See test `src/test/compile-fail/regions-trait-2.rs`
    let mut target_regions = ~[];
    ty::walk_regions_and_ty(
        cx.tcx,
        target_ty,
        |r| {
            if !r.is_bound() {
                target_regions.push(r);
            }
        },
        |_| true);

    // Check, based on the region associated with the trait, whether it can
    // possibly escape the enclosing fn item (note that all type parameters
    // must have been declared on the enclosing fn item).
    if target_regions.iter().any(|r| is_re_scope(*r)) {
        return; /* case (1) */
    }

    // Assuming the trait instance can escape, then ensure that each parameter
    // either appears in the trait type or is sendable.
    let target_params = ty::param_tys_in_type(target_ty);
    let source_ty = ty::expr_ty(cx.tcx, source);
    ty::walk_regions_and_ty(
        cx.tcx,
        source_ty,

        |_r| {
            // FIXME(#5723) --- turn this check on once &Objects are usable
            //
            // if !target_regions.iter().any(|t_r| is_subregion_of(cx, *t_r, r)) {
            //     cx.tcx.sess.span_err(
            //         source.span,
            //         fmt!("source contains borrowed pointer with lifetime \
            //               not found in the target type `%s`",
            //              ty_to_str(cx.tcx, target_ty)));
            //     note_and_explain_region(
            //         cx.tcx, "source data is only valid for ", r, "");
            // }
        },

        |ty| {
            match ty::get(ty).sty {
                ty::ty_param(source_param) => {
                    if target_params.iter().any(|x| x == &source_param) {
                        /* case (2) */
                    } else {
                        check_durable(cx.tcx, ty, source.span); /* case (3) */
                    }
                }
                _ => {}
            }
            true
        });

    fn is_re_scope(r: ty::Region) -> bool {
        match r {
            ty::re_scope(*) => true,
            _ => false
        }
    }

    fn is_subregion_of(cx: Context, r_sub: ty::Region, r_sup: ty::Region) -> bool {
        cx.tcx.region_maps.is_subregion_of(r_sub, r_sup)
    }
}

