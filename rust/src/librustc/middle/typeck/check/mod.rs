// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*

# check.rs

Within the check phase of type check, we check each item one at a time
(bodies of function expressions are checked as part of the containing
function).  Inference is used to supply types wherever they are
unknown.

By far the most complex case is checking the body of a function. This
can be broken down into several distinct phases:

- gather: creates type variables to represent the type of each local
  variable and pattern binding.

- main: the main pass does the lion's share of the work: it
  determines the types of all expressions, resolves
  methods, checks for most invalid conditions, and so forth.  In
  some cases, where a type is unknown, it may create a type or region
  variable and use that as the type of an expression.

  In the process of checking, various constraints will be placed on
  these type variables through the subtyping relationships requested
  through the `demand` module.  The `typeck::infer` module is in charge
  of resolving those constraints.

- regionck: after main is complete, the regionck pass goes over all
  types looking for regions and making sure that they did not escape
  into places they are not in scope.  This may also influence the
  final assignments of the various region variables if there is some
  flexibility.

- vtable: find and records the impls to use for each trait bound that
  appears on a type parameter.

- writeback: writes the final types within a function body, replacing
  type variables with their final inferred types.  These final types
  are written into the `tcx.node_types` table, which should *never* contain
  any reference to a type variable.

## Intermediate types

While type checking a function, the intermediate types for the
expressions, blocks, and so forth contained within the function are
stored in `fcx.node_types` and `fcx.node_type_substs`.  These types
may contain unresolved type variables.  After type checking is
complete, the functions in the writeback module are used to take the
types from this table, resolve them, and then write them into their
permanent home in the type context `ccx.tcx`.

This means that during inferencing you should use `fcx.write_ty()`
and `fcx.expr_ty()` / `fcx.node_ty()` to write/obtain the types of
nodes within the function.

The types of top-level items, which never contain unbound type
variables, are stored directly into the `tcx` tables.

n.b.: A type variable is not the same thing as a type parameter.  A
type variable is rather an "instance" of a type parameter: that is,
given a generic function `fn foo<T>(t: T)`: while checking the
function `foo`, the type `ty_param(0)` refers to the type `T`, which
is treated in abstract.  When `foo()` is called, however, `T` will be
substituted for a fresh type variable `N`.  This variable will
eventually be resolved to some concrete type (which might itself be
type parameter).

*/

use core::prelude::*;

use middle::capture;
use middle::const_eval;
use middle::pat_util::pat_id_map;
use middle::pat_util;
use middle::ty::{TyVid, Vid, FnTyBase, FnMeta, FnSig, VariantInfo_, field};
use middle::ty::{ty_param_bounds_and_ty, ty_param_substs_and_ty};
use middle::ty::{re_bound, br_cap_avoid};
use middle::ty;
use middle::typeck::astconv::{ast_conv, ast_path_to_ty};
use middle::typeck::astconv::{ast_region_to_region, ast_ty_to_ty};
use middle::typeck::astconv;
use middle::typeck::check::_match::pat_ctxt;
use middle::typeck::check::method::TransformTypeNormally;
use middle::typeck::check::regionmanip::replace_bound_regions_in_fn_ty;
use middle::typeck::check::regionmanip::replace_bound_regions_in_fn_sig;
use middle::typeck::check::vtable::{LocationInfo, VtableContext};
use middle::typeck::crate_ctxt;
use middle::typeck::infer::{resolve_type, force_tvar};
use middle::typeck::infer;
use middle::typeck::rscope::{anon_rscope, binding_rscope, bound_self_region};
use middle::typeck::rscope::{empty_rscope, in_anon_rscope};
use middle::typeck::rscope::{in_binding_rscope, region_scope, type_rscope};
use middle::typeck::rscope;
use middle::typeck::{isr_alist, lookup_def_ccx, method_map_entry};
use middle::typeck::{method_origin, method_self, method_trait, no_params};
use middle::typeck::{require_same_types};
use util::common::{block_query, indenter, loop_query};
use util::ppaux::{bound_region_to_str, expr_repr};
use util::ppaux;

use core::either;
use core::option;
use core::ptr;
use core::result::{Result, Ok, Err};
use core::result;
use core::str;
use core::vec;
use std::list::Nil;
use std::map::HashMap;
use std::map;
use syntax::ast::{provided, required, spanned, ty_i};
use syntax::ast;
use syntax::ast_map;
use syntax::ast_util::{Private, Public, is_local, local_def, respan};
use syntax::ast_util::{visibility_to_privacy};
use syntax::ast_util;
use syntax::codemap::span;
use syntax::codemap;
use syntax::parse::token::special_idents;
use syntax::print::pprust::{expr_to_str, pat_to_str};
use syntax::print::pprust;
use syntax::visit;
use syntax;

export _match;
export vtable;
export writeback;
export regionmanip;
export regionck;
export demand;
export method;
export fn_ctxt;
export lookup_local;
export impl_self_ty;
export DerefArgs;
export DontDerefArgs;
export DoDerefArgs;
export check_item_types;
export check_block;
export check_expr_has_type;
export fn_ctxt;
export lookup_def;
export structure_of;
export self_info;
export structurally_resolved_type;
export instantiate_path;
export valid_range_bounds;

#[legacy_exports]
pub mod _match;
#[legacy_exports]
pub mod vtable;
#[legacy_exports]
pub mod writeback;
#[legacy_exports]
pub mod regionmanip;
#[legacy_exports]
pub mod regionck;
#[legacy_exports]
pub mod demand;
#[legacy_exports]
pub mod method;

type self_info = {
    self_ty: ty::t,
    self_id: ast::node_id,
    def_id: ast::def_id,
    explicit_self: ast::self_ty
};

/// Fields that are part of a `fn_ctxt` which are inherited by
/// closures defined within the function.  For example:
///
///     fn foo() {
///         do bar() { ... }
///     }
///
/// Here, the function `foo()` and the closure passed to
/// `bar()` will each have their own `fn_ctxt`, but they will
/// share the inherited fields.
struct inherited {
    infcx: @infer::InferCtxt,
    locals: HashMap<ast::node_id, TyVid>,
    node_types: HashMap<ast::node_id, ty::t>,
    node_type_substs: HashMap<ast::node_id, ty::substs>,
    adjustments: HashMap<ast::node_id, @ty::AutoAdjustment>
}

enum FnKind { ForLoop, DoBlock, Vanilla }

pub struct fn_ctxt {
    // var_bindings, locals and next_var_id are shared
    // with any nested functions that capture the environment
    // (and with any functions whose environment is being captured).

    // Refers to whichever `self` is in scope, even this fn_ctxt is
    // for a nested closure that captures `self`
    self_info: Option<self_info>,
    ret_ty: ty::t,
    // Used by loop bodies that return from the outer function
    indirect_ret_ty: Option<ty::t>,
    purity: ast::purity,

    // Sometimes we generate region pointers where the precise region
    // to use is not known. For example, an expression like `&x.f`
    // where `x` is of type `@T`: in this case, we will be rooting
    // `x` onto the stack frame, and we could choose to root it until
    // the end of (almost) any enclosing block or expression.  We
    // want to pick the narrowest block that encompasses all uses.
    //
    // What we do in such cases is to generate a region variable with
    // `region_lb` as a lower bound.  The regionck pass then adds
    // other constriants based on how the variable is used and region
    // inference selects the ultimate value.  Finally, borrowck is
    // charged with guaranteeing that the value whose address was taken
    // can actually be made to live as long as it needs to live.
    mut region_lb: ast::node_id,

    // Says whether we're inside a for loop, in a do block
    // or neither. Helps with error messages involving the
    // function return type.
    fn_kind: FnKind,

    in_scope_regions: isr_alist,

    inh: @inherited,

    ccx: @crate_ctxt,
}

fn blank_inherited(ccx: @crate_ctxt) -> @inherited {
    @inherited {
        infcx: infer::new_infer_ctxt(ccx.tcx),
        locals: HashMap(),
        node_types: map::HashMap(),
        node_type_substs: map::HashMap(),
        adjustments: map::HashMap()
    }
}

// Used by check_const and check_enum_variants
fn blank_fn_ctxt(ccx: @crate_ctxt, rty: ty::t,
                 region_bnd: ast::node_id) -> @fn_ctxt {
// It's kind of a kludge to manufacture a fake function context
// and statement context, but we might as well do write the code only once
    @fn_ctxt {
        self_info: None,
        ret_ty: rty,
        indirect_ret_ty: None,
        purity: ast::pure_fn,
        mut region_lb: region_bnd,
        in_scope_regions: @Nil,
        fn_kind: Vanilla,
        inh: blank_inherited(ccx),
        ccx: ccx
    }
}

fn check_item_types(ccx: @crate_ctxt, crate: @ast::crate) {
    let visit = visit::mk_simple_visitor(@visit::SimpleVisitor {
        visit_item: |a| check_item(ccx, a),
        .. *visit::default_simple_visitor()
    });
    visit::visit_crate(*crate, (), visit);
}

fn check_bare_fn(ccx: @crate_ctxt,
                 decl: &ast::fn_decl,
                 body: ast::blk,
                 id: ast::node_id,
                 self_info: Option<self_info>) {
    let fty = ty::node_id_to_type(ccx.tcx, id);
    match ty::get(fty).sty {
        ty::ty_fn(ref fn_ty) => {
            check_fn(ccx, self_info, fn_ty, decl, body, Vanilla, None)
        }
        _ => ccx.tcx.sess.impossible_case(body.span,
                                 "check_bare_fn: function type expected")
    }
}

fn check_fn(ccx: @crate_ctxt,
            self_info: Option<self_info>,
            fn_ty: &ty::FnTy,
            decl: &ast::fn_decl,
            body: ast::blk,
            fn_kind: FnKind,
            old_fcx: Option<@fn_ctxt>) {

    let tcx = ccx.tcx;
    let indirect_ret = match fn_kind {
        ForLoop => true, _ => false
    };

    // ______________________________________________________________________
    // First, we have to replace any bound regions in the fn and self
    // types with free ones.  The free region references will be bound
    // the node_id of the body block.

    let {isr, self_info, fn_sig} = {
        let old_isr = option::map_default(&old_fcx, @Nil,
                                          |fcx| fcx.in_scope_regions);
        replace_bound_regions_in_fn_sig(tcx, old_isr, self_info, &fn_ty.sig,
                                        |br| ty::re_free(body.node.id, br))
    };

    let arg_tys = fn_sig.inputs.map(|a| a.ty);
    let ret_ty = fn_sig.output;

    debug!("check_fn(arg_tys=%?, ret_ty=%?, self_info.self_ty=%?)",
           arg_tys.map(|a| ppaux::ty_to_str(tcx, *a)),
           ppaux::ty_to_str(tcx, ret_ty),
           option::map(&self_info, |s| ppaux::ty_to_str(tcx, s.self_ty)));

    // ______________________________________________________________________
    // Create the function context.  This is either derived from scratch or,
    // in the case of function expressions, based on the outer context.
    let fcx: @fn_ctxt = {
        let (purity, inherited) = match old_fcx {
            None => {
                (fn_ty.meta.purity,
                 blank_inherited(ccx))
            }
            Some(fcx) => {
                (ty::determine_inherited_purity(fcx.purity, fn_ty.meta.purity,
                                                fn_ty.meta.proto),
                 fcx.inh)
            }
        };

        let indirect_ret_ty = if indirect_ret {
            let ofcx = old_fcx.get();
            match ofcx.indirect_ret_ty {
              Some(t) => Some(t),
              None => Some(ofcx.ret_ty)
            }
        } else { None };

        @fn_ctxt {
            self_info: self_info,
            ret_ty: ret_ty,
            indirect_ret_ty: indirect_ret_ty,
            purity: purity,
            mut region_lb: body.node.id,
            in_scope_regions: isr,
            fn_kind: fn_kind,
            inh: inherited,
            ccx: ccx
        }
    };

    // Update the self_info to contain an accurate self type (taking
    // into account explicit self).
    let self_info = do self_info.chain_ref |self_info| {
        // If the self type is sty_static, we don't have a self ty.
        if self_info.explicit_self.node == ast::sty_static {
            None
        } else  {
            let self_region = fcx.in_scope_regions.find(ty::br_self);
            let ty = method::transform_self_type_for_method(
                fcx.tcx(),
                self_region,
                self_info.self_ty,
                self_info.explicit_self.node,
                TransformTypeNormally);
            Some({self_ty: ty,.. *self_info})
        }
    };

    // XXX: Bad copy.
    gather_locals(fcx, decl, body, copy arg_tys, self_info);
    check_block(fcx, body);

    // We unify the tail expr's type with the
    // function result type, if there is a tail expr.
    match body.node.expr {
      Some(tail_expr) => {
        let tail_expr_ty = fcx.expr_ty(tail_expr);
        // Special case: we print a special error if there appears
        // to be do-block/for-loop confusion
        demand::suptype_with_fn(fcx, tail_expr.span, fcx.ret_ty, tail_expr_ty,
            |sp, e, a, s| {
                fcx.report_mismatched_return_types(sp, e, a, s) });
      }
      None => ()
    }

    for self_info.each |self_info| {
        fcx.write_ty(self_info.self_id, self_info.self_ty);
    }
    for vec::each2(decl.inputs, arg_tys) |input, arg| {
        fcx.write_ty(input.id, *arg);
    }

    // If we don't have any enclosing function scope, it is time to
    // force any remaining type vars to be resolved.
    // If we have an enclosing function scope, our type variables will be
    // resolved when the enclosing scope finishes up.
    if old_fcx.is_none() {
        vtable::resolve_in_block(fcx, body);
        regionck::regionck_fn(fcx, body);
        writeback::resolve_type_vars_in_fn(fcx, decl, body, self_info);
    }

    fn gather_locals(fcx: @fn_ctxt,
                     decl: &ast::fn_decl,
                     body: ast::blk,
                     arg_tys: ~[ty::t],
                     self_info: Option<self_info>) {
        let tcx = fcx.ccx.tcx;

        let assign = fn@(span: span, nid: ast::node_id,
                         ty_opt: Option<ty::t>) {
            let var_id = fcx.infcx().next_ty_var_id();
            fcx.inh.locals.insert(nid, var_id);
            match ty_opt {
                None => {/* nothing to do */ }
                Some(typ) => {
                    infer::mk_eqty(fcx.infcx(), false, span,
                                   ty::mk_var(tcx, var_id), typ);
                }
            }
        };

        // Add the self parameter
        for self_info.each |self_info| {
            assign(self_info.explicit_self.span,
                   self_info.self_id,
                   Some(self_info.self_ty));
            debug!("self is assigned to %s",
                   fcx.inh.locals.get(self_info.self_id).to_str());
        }

        // Add formal parameters.
        for vec::each2(arg_tys, decl.inputs) |arg_ty, input| {
            // Create type variables for each argument.
            do pat_util::pat_bindings(tcx.def_map, input.pat)
                    |_bm, pat_id, _sp, _path| {
                assign(input.ty.span, pat_id, None);
            }

            // Check the pattern.
            let region = fcx.block_region();
            let pcx = pat_ctxt {
                fcx: fcx,
                map: pat_id_map(tcx.def_map, input.pat),
                match_region: region,
                block_region: region,
            };
            _match::check_pat(pcx, input.pat, *arg_ty);
        }

        // Add explicitly-declared locals.
        let visit_local = fn@(local: @ast::local,
                              &&e: (), v: visit::vt<()>) {
            let o_ty = match local.node.ty.node {
              ast::ty_infer => None,
              _ => Some(fcx.to_ty(local.node.ty))
            };
            assign(local.span, local.node.id, o_ty);
            debug!("Local variable %s is assigned to %s",
                   pat_to_str(local.node.pat, tcx.sess.intr()),
                   fcx.inh.locals.get(local.node.id).to_str());
            visit::visit_local(local, e, v);
        };

        // Add pattern bindings.
        let visit_pat = fn@(p: @ast::pat, &&e: (), v: visit::vt<()>) {
            match p.node {
              ast::pat_ident(_, path, _)
                  if pat_util::pat_is_binding(fcx.ccx.tcx.def_map, p) => {
                assign(p.span, p.id, None);
                debug!("Pattern binding %s is assigned to %s",
                       tcx.sess.str_of(path.idents[0]),
                       fcx.inh.locals.get(p.id).to_str());
              }
              _ => {}
            }
            visit::visit_pat(p, e, v);
        };

        let visit_block = fn@(b: ast::blk, &&e: (), v: visit::vt<()>) {
            // non-obvious: the `blk` variable maps to region lb, so
            // we have to keep this up-to-date.  This
            // is... unfortunate.  It'd be nice to not need this.
            do fcx.with_region_lb(b.node.id) {
                visit::visit_block(b, e, v);
            }
        };

        // Don't descend into fns and items
        fn visit_fn(_fk: visit::fn_kind, _decl: ast::fn_decl,
                    _body: ast::blk, _sp: span,
                    _id: ast::node_id, &&_t: (), _v: visit::vt<()>) {
        }
        fn visit_item(_i: @ast::item, &&_e: (), _v: visit::vt<()>) { }

        let visit = visit::mk_vt(
            @visit::Visitor {visit_local: visit_local,
                             visit_pat: visit_pat,
                             visit_fn: visit_fn,
                             visit_item: visit_item,
                             visit_block: visit_block,
                             ..*visit::default_visitor()});

        (visit.visit_block)(body, (), visit);
    }
}

fn check_method(ccx: @crate_ctxt, method: @ast::method,
                self_ty: ty::t, self_impl_def_id: ast::def_id) {
    let self_info = {self_ty: self_ty,
                     self_id: method.self_id,
                     def_id: self_impl_def_id,
                     explicit_self: method.self_ty };
    check_bare_fn(ccx, &method.decl, method.body, method.id, Some(self_info));
}

fn check_no_duplicate_fields(tcx: ty::ctxt, fields:
                             ~[(ast::ident, span)]) {
    let field_names = HashMap();

    for fields.each |p| {
        let (id, sp) = *p;
        match field_names.find(id) {
          Some(orig_sp) => {
            tcx.sess.span_err(sp, fmt!("Duplicate field \
                                   name %s in record type declaration",
                                        tcx.sess.str_of(id)));
            tcx.sess.span_note(orig_sp, ~"First declaration of \
                                          this field occurred here");
            break;
          }
          None => {
            field_names.insert(id, sp);
          }
        }
    }

}

fn check_struct(ccx: @crate_ctxt, struct_def: @ast::struct_def,
                id: ast::node_id, span: span) {
    let tcx = ccx.tcx;
    let self_ty = ty::node_id_to_type(tcx, id);

    do struct_def.dtor.iter() |dtor| {
        let class_t = { self_ty: self_ty,
                        self_id: dtor.node.self_id,
                        def_id: local_def(id),
                        explicit_self:
                            spanned { node: ast::sty_by_ref,
                                      span: ast_util::dummy_sp() } };
        // typecheck the dtor
        let dtor_dec = ast_util::dtor_dec();
        check_bare_fn(ccx, &dtor_dec,
                      dtor.node.body, dtor.node.id,
                      Some(class_t));
    };

    // Check that the class is instantiable
    check_instantiable(ccx.tcx, span, id);
}

fn check_item(ccx: @crate_ctxt, it: @ast::item) {
    debug!("check_item(it.id=%d, it.ident=%s)",
           it.id,
           ty::item_path_str(ccx.tcx, local_def(it.id)));
    let _indenter = indenter();

    match /*bad*/copy it.node {
      ast::item_const(_, e) => check_const(ccx, it.span, e, it.id),
      ast::item_enum(ref enum_definition, _) => {
        check_enum_variants(ccx,
                            it.span,
                            /*bad*/copy (*enum_definition).variants,
                            it.id);
      }
      ast::item_fn(ref decl, _, _, ref body) => {
        check_bare_fn(ccx, decl, (*body), it.id, None);
      }
      ast::item_impl(_, _, ty, ms) => {
        let rp = ccx.tcx.region_paramd_items.find(it.id);
        debug!("item_impl %s with id %d rp %?",
               ccx.tcx.sess.str_of(it.ident), it.id, rp);
        let self_ty = ccx.to_ty(rscope::type_rscope(rp), ty);
        for ms.each |m| {
            check_method(ccx, *m, self_ty, local_def(it.id));
        }
      }
      ast::item_trait(_, _, ref trait_methods) => {
        for (*trait_methods).each |trait_method| {
            match *trait_method {
              required(*) => {
                // Nothing to do, since required methods don't have
                // bodies to check.
              }
              provided(m) => {
                check_method(ccx, m, ty::mk_self(ccx.tcx), local_def(it.id));
              }
            }
        }
      }
      ast::item_struct(struct_def, _) => {
        check_struct(ccx, struct_def, it.id, it.span);
      }
      ast::item_ty(t, tps) => {
        let tpt_ty = ty::node_id_to_type(ccx.tcx, it.id);
        check_bounds_are_used(ccx, t.span, tps, tpt_ty);
        // If this is a record ty, check for duplicate fields
        match t.node {
            ast::ty_rec(ref fields) => {
              check_no_duplicate_fields(ccx.tcx, (*fields).map(|f|
                                              (f.node.ident, f.span)));
            }
            _ => ()
        }
      }
      ast::item_foreign_mod(m) => {
        if syntax::attr::foreign_abi(it.attrs) ==
            either::Right(ast::foreign_abi_rust_intrinsic) {
            for m.items.each |item| {
                check_intrinsic_type(ccx, *item);
            }
        } else {
            for m.items.each |item| {
                let tpt = ty::lookup_item_type(ccx.tcx, local_def(item.id));
                if (*tpt.bounds).is_not_empty() {
                    ccx.tcx.sess.span_err(
                        item.span,
                        fmt!("foreign items may not have type parameters"));
                }
            }
        }
      }
      _ => {/* nothing to do */ }
    }
}

impl @fn_ctxt: ast_conv {
    fn tcx() -> ty::ctxt { self.ccx.tcx }
    fn ccx() -> @crate_ctxt { self.ccx }

    fn get_item_ty(id: ast::def_id) -> ty::ty_param_bounds_and_ty {
        ty::lookup_item_type(self.tcx(), id)
    }

    fn ty_infer(_span: span) -> ty::t {
        self.infcx().next_ty_var()
    }
}

impl @fn_ctxt {
    fn infcx() -> @infer::InferCtxt { self.inh.infcx }
    fn search_in_scope_regions(br: ty::bound_region)
        -> Result<ty::Region, ~str>
    {
        match self.in_scope_regions.find(br) {
            Some(r) => result::Ok(r),
            None => {
                let blk_br = ty::br_named(special_idents::blk);
                if br == blk_br {
                    result::Ok(self.block_region())
                } else {
                    result::Err(fmt!("named region `%s` not in scope here",
                                     bound_region_to_str(self.tcx(), br)))
                }
            }
        }
    }
}

impl @fn_ctxt: region_scope {
    fn anon_region(span: span) -> Result<ty::Region, ~str> {
        result::Ok(self.infcx().next_region_var_nb(span))
    }
    fn self_region(_span: span) -> Result<ty::Region, ~str> {
        self.search_in_scope_regions(ty::br_self)
    }
    fn named_region(_span: span, id: ast::ident) -> Result<ty::Region, ~str> {
        self.search_in_scope_regions(ty::br_named(id))
    }
}

impl @fn_ctxt {
    fn tag() -> ~str { fmt!("%x", ptr::addr_of(&(*self)) as uint) }

    fn expr_to_str(expr: @ast::expr) -> ~str {
        fmt!("expr(%?:%s)", expr.id,
             pprust::expr_to_str(expr, self.tcx().sess.intr()))
    }

    fn block_region() -> ty::Region {
        ty::re_scope(self.region_lb)
    }

    #[inline(always)]
    fn write_ty(node_id: ast::node_id, ty: ty::t) {
        debug!("write_ty(%d, %s) in fcx %s",
               node_id, ppaux::ty_to_str(self.tcx(), ty), self.tag());
        self.inh.node_types.insert(node_id, ty);
    }

    fn write_substs(node_id: ast::node_id, +substs: ty::substs) {
        if !ty::substs_is_noop(&substs) {
            debug!("write_substs(%d, %s) in fcx %s",
                   node_id,
                   ty::substs_to_str(self.tcx(), &substs),
                   self.tag());
            self.inh.node_type_substs.insert(node_id, substs);
        }
    }

    fn write_ty_substs(node_id: ast::node_id, ty: ty::t,
                       +substs: ty::substs) {
        let ty = ty::subst(self.tcx(), &substs, ty);
        self.write_ty(node_id, ty);
        self.write_substs(node_id, substs);
    }

    fn write_autoderef_adjustment(node_id: ast::node_id, derefs: uint) {
        if derefs == 0 { return; }
        self.write_adjustment(
            node_id,
            @ty::AutoAdjustment { autoderefs: derefs, autoref: None }
        );
    }

    fn write_adjustment(node_id: ast::node_id, adj: @ty::AutoAdjustment) {
        debug!("write_adjustment(node_id=%?, adj=%?)", node_id, adj);
        self.inh.adjustments.insert(node_id, adj);
    }

    fn write_nil(node_id: ast::node_id) {
        self.write_ty(node_id, ty::mk_nil(self.tcx()));
    }
    fn write_bot(node_id: ast::node_id) {
        self.write_ty(node_id, ty::mk_bot(self.tcx()));
    }

    fn to_ty(ast_t: @ast::Ty) -> ty::t {
        ast_ty_to_ty(self, self, ast_t)
    }

    fn expr_to_str(expr: @ast::expr) -> ~str {
        expr_repr(self.tcx(), expr)
    }

    fn expr_ty(ex: @ast::expr) -> ty::t {
        match self.inh.node_types.find(ex.id) {
            Some(t) => t,
            None => {
                self.tcx().sess.bug(
                    fmt!("no type for %s in fcx %s",
                         self.expr_to_str(ex), self.tag()));
            }
        }
    }
    fn node_ty(id: ast::node_id) -> ty::t {
        match self.inh.node_types.find(id) {
            Some(t) => t,
            None => {
                self.tcx().sess.bug(
                    fmt!("no type for node %d: %s in fcx %s",
                         id, ast_map::node_id_to_str(
                             self.tcx().items, id,
                             self.tcx().sess.parse_sess.interner),
                         self.tag()));
            }
        }
    }
    fn node_ty_substs(id: ast::node_id) -> ty::substs {
        match self.inh.node_type_substs.find(id) {
            Some(ref ts) => (/*bad*/copy *ts),
            None => {
                self.tcx().sess.bug(
                    fmt!("no type substs for node %d: %s in fcx %s",
                         id, ast_map::node_id_to_str(
                             self.tcx().items, id,
                             self.tcx().sess.parse_sess.interner),
                         self.tag()));
            }
        }
    }
    fn opt_node_ty_substs(id: ast::node_id) -> Option<ty::substs> {
        self.inh.node_type_substs.find(id)
    }


    fn mk_subty(a_is_expected: bool, span: span,
                sub: ty::t, sup: ty::t) -> Result<(), ty::type_err> {
        infer::mk_subty(self.infcx(), a_is_expected, span, sub, sup)
    }

    fn can_mk_subty(sub: ty::t, sup: ty::t) -> Result<(), ty::type_err> {
        infer::can_mk_subty(self.infcx(), sub, sup)
    }

    fn mk_assignty(expr: @ast::expr, sub: ty::t, sup: ty::t)
        -> Result<(), ty::type_err>
    {
        match infer::mk_assignty(self.infcx(), false, expr.span, sub, sup) {
            Ok(None) => result::Ok(()),
            Err(ref e) => result::Err((*e)),
            Ok(Some(adjustment)) => {
                self.write_adjustment(expr.id, adjustment);
                Ok(())
            }
        }
    }

    fn can_mk_assignty(sub: ty::t, sup: ty::t) -> Result<(), ty::type_err> {
        infer::can_mk_assignty(self.infcx(), sub, sup)
    }

    fn mk_eqty(a_is_expected: bool, span: span,
               sub: ty::t, sup: ty::t) -> Result<(), ty::type_err> {
        infer::mk_eqty(self.infcx(), a_is_expected, span, sub, sup)
    }

    fn mk_subr(a_is_expected: bool, span: span,
               sub: ty::Region, sup: ty::Region) -> Result<(), ty::type_err> {
        infer::mk_subr(self.infcx(), a_is_expected, span, sub, sup)
    }

    fn require_unsafe(sp: span, op: ~str) {
        match self.purity {
          ast::unsafe_fn => {/*ok*/}
          _ => {
            self.ccx.tcx.sess.span_err(
                sp,
                fmt!("%s requires unsafe function or block", op));
          }
        }
    }
    fn with_region_lb<R>(lb: ast::node_id, f: fn() -> R) -> R {
        let old_region_lb = self.region_lb;
        self.region_lb = lb;
        let v = f();
        self.region_lb = old_region_lb;
        move v
    }

    fn region_var_if_parameterized(rp: Option<ty::region_variance>,
                                   span: span,
                                   lower_bound: ty::Region)
        -> Option<ty::Region>
    {
        rp.map(
            |_rp| self.infcx().next_region_var_with_lb(span, lower_bound))
    }

    fn type_error_message(sp: span, mk_msg: fn(~str) -> ~str,
                          actual_ty: ty::t, err: Option<&ty::type_err>) {
        self.infcx().type_error_message(sp, mk_msg, actual_ty, err);
    }

    fn report_mismatched_return_types(sp: span, e: ty::t, a: ty::t,
                               err: &ty::type_err) {
        match self.fn_kind {
            ForLoop if !ty::type_is_bool(e) && !ty::type_is_nil(a) =>
                    self.tcx().sess.span_err(sp, fmt!("A for-loop body must \
                        return (), but it returns %s here. \
                        Perhaps you meant to write a `do`-block?",
                                            ppaux::ty_to_str(self.tcx(), a))),
            DoBlock if ty::type_is_bool(e) && ty::type_is_nil(a) =>
                // If we expected bool and got ()...
                    self.tcx().sess.span_err(sp, fmt!("Do-block body must \
                        return %s, but returns () here. Perhaps you meant \
                        to write a `for`-loop?",
                        ppaux::ty_to_str(self.tcx(), e))),
            _ => self.infcx().report_mismatched_types(sp, e, a, err)
        }
    }

    fn report_mismatched_types(sp: span, e: ty::t, a: ty::t,
                               err: &ty::type_err) {
            self.infcx().report_mismatched_types(sp, e, a, err)
    }
}

fn do_autoderef(fcx: @fn_ctxt, sp: span, t: ty::t) -> (ty::t, uint) {
    /*!
     *
     * Autoderefs the type `t` as many times as possible, returning
     * a new type and a counter for how many times the type was
     * deref'd.  If the counter is non-zero, the receiver is responsible
     * for inserting an AutoAdjustment record into `tcx.adjustments`
     * so that trans/borrowck/etc know about this autoderef. */

    let mut t1 = t;
    let mut enum_dids = ~[];
    let mut autoderefs = 0;
    loop {
        let sty = structure_of(fcx, sp, t1);

        // Some extra checks to detect weird cycles and so forth:
        match sty {
            ty::ty_box(inner) | ty::ty_uniq(inner) |
            ty::ty_rptr(_, inner) => {
                match ty::get(t1).sty {
                    ty::ty_infer(ty::TyVar(v1)) => {
                        ty::occurs_check(fcx.ccx.tcx, sp, v1,
                                         ty::mk_box(fcx.ccx.tcx, inner));
                    }
                    _ => ()
                }
            }
            ty::ty_enum(ref did, _) => {
                // Watch out for a type like `enum t = @t`.  Such a
                // type would otherwise infinitely auto-deref.  Only
                // autoderef loops during typeck (basically, this one
                // and the loops in typeck::check::method) need to be
                // concerned with this, as an error will be reported
                // on the enum definition as well because the enum is
                // not instantiable.
                if vec::contains(enum_dids, did) {
                    return (t1, autoderefs);
                }
                enum_dids.push(*did);
            }
            _ => { /*ok*/ }
        }

        // Otherwise, deref if type is derefable:
        match ty::deref_sty(fcx.ccx.tcx, &sty, false) {
            None => {
                return (t1, autoderefs);
            }
            Some(mt) => {
                autoderefs += 1;
                t1 = mt.ty
            }
        }
    };
}

// AST fragment checking
fn check_lit(fcx: @fn_ctxt, lit: @ast::lit) -> ty::t {
    let tcx = fcx.ccx.tcx;

    match lit.node {
      ast::lit_str(*) => ty::mk_estr(tcx, ty::vstore_slice(ty::re_static)),
      ast::lit_int(_, t) => ty::mk_mach_int(tcx, t),
      ast::lit_uint(_, t) => ty::mk_mach_uint(tcx, t),
      ast::lit_int_unsuffixed(_) => {
        // An unsuffixed integer literal could have any integral type,
        // so we create an integral type variable for it.
        ty::mk_int_var(tcx, fcx.infcx().next_int_var_id())
      }
      ast::lit_float(_, t) => ty::mk_mach_float(tcx, t),
      ast::lit_float_unsuffixed(_) => {
        // An unsuffixed floating point literal could have any floating point
        // type, so we create a floating point type variable for it.
        ty::mk_float_var(tcx, fcx.infcx().next_float_var_id())
      }
      ast::lit_nil => ty::mk_nil(tcx),
      ast::lit_bool(_) => ty::mk_bool(tcx)
    }
}

fn valid_range_bounds(ccx: @crate_ctxt, from: @ast::expr, to: @ast::expr)
    -> bool {
    const_eval::compare_lit_exprs(ccx.tcx, from, to) <= 0
}

fn check_expr_has_type(
    fcx: @fn_ctxt, expr: @ast::expr,
    expected: ty::t) -> bool
{
    do check_expr_with_unifier(fcx, expr, Some(expected)) {
        demand::suptype(fcx, expr.span, expected, fcx.expr_ty(expr));
    }
}

fn check_expr_assignable_to_type(
    fcx: @fn_ctxt, expr: @ast::expr,
    expected: ty::t) -> bool
{
    do check_expr_with_unifier(fcx, expr, Some(expected)) {
        demand::assign(fcx, expr.span, expected, expr)
    }
}

fn check_expr_with_hint(
    fcx: @fn_ctxt, expr: @ast::expr,
    expected: ty::t) -> bool
{
    check_expr_with_unifier(fcx, expr, Some(expected), || ())
}

fn check_expr_with_opt_hint(
    fcx: @fn_ctxt, expr: @ast::expr,
    expected: Option<ty::t>) -> bool
{
    check_expr_with_unifier(fcx, expr, expected, || ())
}

fn check_expr(fcx: @fn_ctxt, expr: @ast::expr) -> bool {
    check_expr_with_unifier(fcx, expr, None, || ())
}

// determine the `self` type, using fresh variables for all variables
// declared on the impl declaration e.g., `impl<A,B> for ~[(A,B)]`
// would return ($0, $1) where $0 and $1 are freshly instantiated type
// variables.
pub fn impl_self_ty(vcx: &VtableContext,
                    location_info: &LocationInfo, // (potential) receiver for
                                                  // this impl
                    did: ast::def_id)
                 -> ty_param_substs_and_ty {
    let tcx = vcx.tcx();

    let {n_tps, region_param, raw_ty} = if did.crate == ast::local_crate {
        let region_param = tcx.region_paramd_items.find(did.node);
        match tcx.items.find(did.node) {
          Some(ast_map::node_item(@ast::item {
                  node: ast::item_impl(ref ts, _, st, _),
                  _
              }, _)) => {
            {n_tps: ts.len(),
             region_param: region_param,
             raw_ty: vcx.ccx.to_ty(rscope::type_rscope(region_param), st)}
          }
          Some(ast_map::node_item(@ast::item {
                  node: ast::item_struct(_, ref ts),
                  id: class_id,
                  _
              },_)) => {
              /* If the impl is a class, the self ty is just the class ty
                 (doing a no-op subst for the ty params; in the next step,
                 we substitute in fresh vars for them)
               */
              {n_tps: ts.len(),
               region_param: region_param,
               raw_ty: ty::mk_struct(tcx, local_def(class_id),
                      {self_r: rscope::bound_self_region(region_param),
                       self_ty: None,
                       tps: ty::ty_params_to_tys(tcx, /*bad*/copy *ts)})}
          }
          _ => { tcx.sess.bug(~"impl_self_ty: unbound item or item that \
               doesn't have a self_ty"); }
        }
    } else {
        let ity = ty::lookup_item_type(tcx, did);
        {n_tps: vec::len(*ity.bounds),
         region_param: ity.region_param,
         raw_ty: ity.ty}
    };

    let self_r = if region_param.is_some() {
        Some(vcx.infcx.next_region_var(location_info.span,
                                         location_info.id))
    } else {
        None
    };
    let tps = vcx.infcx.next_ty_vars(n_tps);

    let substs = {self_r: self_r, self_ty: None, tps: tps};
    let substd_ty = ty::subst(tcx, &substs, raw_ty);
    {substs: substs, ty: substd_ty}
}

// Only for fields! Returns <none> for methods>
// Indifferent to privacy flags
fn lookup_field_ty(tcx: ty::ctxt,
                   class_id: ast::def_id,
                   items: &[ty::field_ty],
                   fieldname: ast::ident,
                   substs: &ty::substs) -> Option<ty::t> {

    let o_field = vec::find(items, |f| f.ident == fieldname);
    do o_field.map() |f| {
        ty::lookup_field_type(tcx, class_id, f.id, substs)
    }
}

// Controls whether the arguments are automatically referenced. This is useful
// for overloaded binary and unary operators.
pub enum DerefArgs {
    DontDerefArgs,
    DoDerefArgs
}

fn break_here() {
    debug!("break here!");
}

fn check_expr_with_unifier(fcx: @fn_ctxt,
                           expr: @ast::expr,
                           expected: Option<ty::t>,
                           unifier: fn()) -> bool
{
    debug!(">> typechecking %s", fcx.expr_to_str(expr));

    // A generic function to factor out common logic from call and
    // overloaded operations
    fn check_call_inner(
        fcx: @fn_ctxt,
        sp: span,
        call_expr_id: ast::node_id,
        in_fty: ty::t,
        callee_expr: @ast::expr,
        args: ~[@ast::expr],
        deref_args: DerefArgs) -> {fty: ty::t, bot: bool} {

        let tcx = fcx.ccx.tcx;
        let mut bot = false;

        // Replace all region parameters in the arguments and return
        // type with fresh region variables.

        debug!("check_call_inner: before universal quant., in_fty=%s",
               fcx.infcx().ty_to_str(in_fty));

        let mut formal_tys;

        // This is subtle: we expect `fty` to be a function type, which
        // normally introduce a level of binding.  In this case, we want to
        // process the types bound by the function but not by any nested
        // functions.  Therefore, we match one level of structure.
        let fty =
            match structure_of(fcx, sp, in_fty) {
              ty::ty_fn(ref fn_ty) => {
                  let fn_ty =
                    /*bad*/copy replace_bound_regions_in_fn_ty(tcx, @Nil,
                      None, fn_ty, |_br| fcx.infcx().next_region_var(sp,
                                                      call_expr_id)).fn_ty;

                  let supplied_arg_count = args.len();

                  bot |= ty::type_is_bot(fn_ty.sig.output);

                  // Grab the argument types, supplying fresh type variables
                  // if the wrong number of arguments were supplied
                  let expected_arg_count = fn_ty.sig.inputs.len();
                  formal_tys = if expected_arg_count == supplied_arg_count {
                      fn_ty.sig.inputs.map(|a| a.ty)
                  } else {
                      tcx.sess.span_err(
                          sp, fmt!("this function takes %u parameter%s but \
                                    %u parameter%s supplied",
                                   expected_arg_count,
                                   if expected_arg_count == 1 {
                                       ~""
                                   } else {
                                       ~"s"
                                   },
                                   supplied_arg_count,
                                   if supplied_arg_count == 1 {
                                       ~" was"
                                   } else {
                                       ~"s were"
                                   }));
                      fcx.infcx().next_ty_vars(supplied_arg_count)
                  };
                  ty::mk_fn(tcx, fn_ty)
              }
              _ => {
                  fcx.type_error_message(sp, |actual| {
                      fmt!("expected function or foreign function but \
                            found `%s`", actual) }, in_fty, None);
                  // check each arg against "error", in order to set up
                  // all the node type bindings
                  formal_tys = args.map(|_x| ty::mk_err(tcx));
                  ty::mk_err(tcx)
              }
            };

        debug!("check_call_inner: after universal quant., fty=%s",
               fcx.infcx().ty_to_str(fty));

        // Check the arguments.
        // We do this in a pretty awful way: first we typecheck any arguments
        // that are not anonymous functions, then we typecheck the anonymous
        // functions. This is so that we have more information about the types
        // of arguments when we typecheck the functions. This isn't really the
        // right way to do this.
        for [false, true].each |check_blocks| {
            let check_blocks = *check_blocks;
            debug!("check_blocks=%b", check_blocks);

            // More awful hacks: before we check the blocks, try to do
            // an "opportunistic" vtable resolution of any trait
            // bounds on the call.
            if check_blocks {
                vtable::early_resolve_expr(callee_expr, fcx, true);
            }

            for args.eachi |i, arg| {
                let is_block = match arg.node {
                    ast::expr_fn_block(*) | ast::expr_loop_body(*) |
                    ast::expr_do_body(*) => true,
                    _ => false
                };

                if is_block == check_blocks {
                    debug!("checking the argument");
                    let mut formal_ty = formal_tys[i];

                    match deref_args {
                        DoDerefArgs => {
                            match ty::get(formal_ty).sty {
                                ty::ty_rptr(_, mt) => formal_ty = mt.ty,
                                _ => {
                                    fcx.ccx.tcx.sess.span_bug(arg.span,
                                                              ~"no ref");
                                }
                            }
                        }
                        DontDerefArgs => {}
                    }

                    // mismatch error happens in here
                    bot |= check_expr_assignable_to_type(
                        fcx, *arg, formal_ty);

                }
            }
        }

        {fty: fty, bot: bot}
    }

    // A generic function for checking assignment expressions
    fn check_assignment(fcx: @fn_ctxt,
                        lhs: @ast::expr,
                        rhs: @ast::expr,
                        id: ast::node_id)
                     -> bool {
        let mut bot = check_expr(fcx, lhs);
        let lhs_type = fcx.expr_ty(lhs);
        bot |= check_expr_assignable_to_type(fcx, rhs, lhs_type);
        fcx.write_ty(id, ty::mk_nil(fcx.ccx.tcx));
        return bot;
    }

    // A generic function for doing all of the checking for call or
    // method expressions
    fn check_call_or_method(fcx: @fn_ctxt,
                            sp: span,
                            call_expr_id: ast::node_id,
                            fn_ty: ty::t,
                            expr: @ast::expr,
                            +args: ~[@ast::expr],
                            bot: bool)
                         -> bool {
        let mut bot = bot;

        // Call the generic checker.
        let fty = {
            let r = check_call_inner(fcx, sp, call_expr_id,
                                     fn_ty, expr, args, DontDerefArgs);
            bot |= r.bot;
            r.fty
        };

        // Pull the return type out of the type of the function.
        match structure_of(fcx, sp, fty) {
          ty::ty_fn(ref f) => {
              fcx.write_ty(call_expr_id, f.sig.output);
              return bot;
          }
          _ => {
              fcx.write_ty(call_expr_id, ty::mk_err(fcx.ccx.tcx));
              fcx.type_error_message(sp, |_actual| {
                  ~"expected function"}, fty, None);
              return bot;
          }
        }
    }

    // A generic function for doing all of the checking for call expressions
    fn check_call(fcx: @fn_ctxt, sp: span, call_expr_id: ast::node_id,
                  f: @ast::expr, +args: ~[@ast::expr]) -> bool {
        // Index expressions need to be handled separately, to inform them
        // that they appear in call position.
        let mut bot = match /*bad*/copy f.node {
            ast::expr_field(base, field, tys) => {
                check_field(fcx, f, true, base, field, tys)
            }
            _ => check_expr(fcx, f)
        };

        check_call_or_method(fcx,
                             sp,
                             call_expr_id,
                             fcx.expr_ty(f),
                             f,
                             args,
                             bot)
    }

    // Checks a method call.
    fn check_method_call(fcx: @fn_ctxt,
                         expr: @ast::expr,
                         rcvr: @ast::expr,
                         method_name: ast::ident,
                         +args: ~[@ast::expr],
                         tps: ~[@ast::Ty])
                      -> bool {
        let bot = check_expr(fcx, rcvr);
        let expr_t = structurally_resolved_type(fcx,
                                                expr.span,
                                                fcx.expr_ty(rcvr));

        let tps = tps.map(|ast_ty| fcx.to_ty(*ast_ty));
        match method::lookup(fcx,
                             expr,
                             rcvr,
                             expr.callee_id,
                             method_name,
                             expr_t,
                             tps,
                             DontDerefArgs) {
            Some(ref entry) => {
                fcx.ccx.method_map.insert(expr.id, (*entry));
            }
            None => {
                fcx.type_error_message(expr.span,
                  |actual| {
                      fmt!("type `%s` does not implement any method in scope \
                            named `%s`",
                           actual,
                           fcx.ccx.tcx.sess.str_of(method_name))
                  },
                  expr_t,
                  None);

                // Add error type for the result
                fcx.write_ty(expr.id, ty::mk_err(fcx.ccx.tcx));
                fcx.write_ty(expr.callee_id, ty::mk_err(fcx.ccx.tcx));
            }
        }

        check_call_or_method(fcx,
                             expr.span,
                             expr.id,
                             fcx.node_ty(expr.callee_id),
                             expr,
                             args,
                             bot)
    }

    // A generic function for checking for or for-each loops
    fn check_for(fcx: @fn_ctxt, local: @ast::local,
                 element_ty: ty::t, body: ast::blk,
                 node_id: ast::node_id) -> bool {
        let locid = lookup_local(fcx, local.span, local.node.id);
        demand::suptype(fcx, local.span,
                       ty::mk_var(fcx.ccx.tcx, locid),
                       element_ty);
        let bot = check_decl_local(fcx, local);
        check_block_no_value(fcx, body);
        fcx.write_nil(node_id);
        return bot;
    }

    // A generic function for checking the then and else in an if
    // or if-check
    fn check_then_else(fcx: @fn_ctxt, thn: ast::blk,
                       elsopt: Option<@ast::expr>, id: ast::node_id,
                       _sp: span) -> bool {
        let (if_t, if_bot) =
            match elsopt {
                Some(els) => {
                    let if_t = fcx.infcx().next_ty_var();
                    let thn_bot = check_block(fcx, thn);
                    let thn_t = fcx.node_ty(thn.node.id);
                    demand::suptype(fcx, thn.span, if_t, thn_t);
                    let els_bot = check_expr_has_type(fcx, els, if_t);
                    (if_t, thn_bot & els_bot)
                }
                None => {
                    check_block_no_value(fcx, thn);
                    (ty::mk_nil(fcx.ccx.tcx), false)
                }
            };
        fcx.write_ty(id, if_t);
        return if_bot;
    }

    fn lookup_op_method(fcx: @fn_ctxt, op_ex: @ast::expr,
                        self_ex: @ast::expr, self_t: ty::t,
                        opname: ast::ident, +args: ~[@ast::expr],
                        +deref_args: DerefArgs)
        -> Option<(ty::t, bool)>
    {
        match method::lookup(fcx, op_ex, self_ex,
                             op_ex.callee_id, opname, self_t, ~[],
                             deref_args) {
          Some(ref origin) => {
            let {fty: method_ty, bot: bot} = {
                let method_ty = fcx.node_ty(op_ex.callee_id);
                check_call_inner(fcx, op_ex.span, op_ex.id,
                                 method_ty, op_ex, args, deref_args)
            };
            fcx.ccx.method_map.insert(op_ex.id, (*origin));
            Some((ty::ty_fn_ret(method_ty), bot))
          }
          _ => None
        }
    }

    // could be either a expr_binop or an expr_assign_binop
    fn check_binop(fcx: @fn_ctxt, expr: @ast::expr,
                   op: ast::binop,
                   lhs: @ast::expr,
                   rhs: @ast::expr) -> bool {
        let tcx = fcx.ccx.tcx;

        let lhs_bot = check_expr(fcx, lhs);
        let lhs_t = fcx.expr_ty(lhs);
        let lhs_t = structurally_resolved_type(fcx, lhs.span, lhs_t);

        if ty::type_is_integral(lhs_t) && ast_util::is_shift_binop(op) {
            // Shift is a special case: rhs can be any integral type
            let rhs_bot = check_expr(fcx, rhs);
            let rhs_t = fcx.expr_ty(rhs);
            require_integral(fcx, rhs.span, rhs_t);
            fcx.write_ty(expr.id, lhs_t);
            return lhs_bot | rhs_bot;
        }

        if ty::is_binopable(tcx, lhs_t, op) {
            let tvar = fcx.infcx().next_ty_var();
            demand::suptype(fcx, expr.span, tvar, lhs_t);
            let rhs_bot = check_expr_has_type(fcx, rhs, tvar);

            let result_t = match op {
                ast::eq | ast::ne | ast::lt | ast::le | ast::ge |
                ast::gt => {
                    ty::mk_bool(tcx)
                }
                _ => {
                    lhs_t
                }
            };

            fcx.write_ty(expr.id, result_t);
            return {
                if !ast_util::lazy_binop(op) { lhs_bot | rhs_bot }
                else { lhs_bot }
            };
        }

        // A hack, but this prevents multiple errors for the same code
        // (since check_user_binop calls structurally_resolve_type)
        let (result, rhs_bot) =
           match ty::deref(fcx.tcx(), lhs_t, false).map(
                      |tt| structurally_resolved_type(fcx,
                                                      expr.span, tt.ty)) {
                Some(t) if ty::get(t).sty == ty::ty_err => (t, false),
                _ => check_user_binop(fcx, expr, lhs, lhs_t, op, rhs)
           };
        fcx.write_ty(expr.id, result);
        return lhs_bot | rhs_bot;
    }

    fn check_user_binop(fcx: @fn_ctxt, ex: @ast::expr,
                        lhs_expr: @ast::expr, lhs_resolved_t: ty::t,
                        op: ast::binop, rhs: @ast::expr) -> (ty::t, bool)
    {
        let tcx = fcx.ccx.tcx;
        match ast_util::binop_to_method_name(op) {
          Some(ref name) => {
            match lookup_op_method(fcx, ex, lhs_expr, lhs_resolved_t,
                                   fcx.tcx().sess.ident_of((*name)),
                                   ~[rhs], DoDerefArgs) {
              Some(pair) => return pair,
              _ => ()
            }
          }
          _ => ()
        }
        check_expr(fcx, rhs);
        fcx.type_error_message(ex.span,
           |actual| {
               fmt!("binary operation %s cannot be applied to type `%s`",
                    ast_util::binop_to_str(op), actual)
           },
           lhs_resolved_t, None);

        // If the or operator is used it might be that the user forgot to
        // supply the do keyword.  Let's be more helpful in that situation.
        if op == ast::or {
          match ty::get(lhs_resolved_t).sty {
            ty::ty_fn(_) => {
              tcx.sess.span_note(
                  ex.span, ~"did you forget the 'do' keyword for the call?");
            }
            _ => ()
          }
        }

        (lhs_resolved_t, false)
    }

    fn check_user_unop(fcx: @fn_ctxt, op_str: ~str, mname: ~str,
                       ex: @ast::expr,
                       rhs_expr: @ast::expr, rhs_t: ty::t) -> ty::t {
        match lookup_op_method(fcx, ex, rhs_expr, rhs_t,
                               fcx.tcx().sess.ident_of(mname), ~[],
                               DontDerefArgs) {
          Some((ret_ty, _)) => ret_ty,
          _ => {
              fcx.type_error_message(ex.span, |actual| {
                  fmt!("cannot apply unary operator `%s` to type `%s`",
                              op_str, actual)
              }, rhs_t, None);
              rhs_t
          }
        }
    }

    // Resolves `expected` by a single level if it is a variable and passes it
    // through the `unpack` function.  It there is no expected type or
    // resolution is not possible (e.g., no constraints yet present), just
    // returns `none`.
    fn unpack_expected<O: Copy>(fcx: @fn_ctxt, expected: Option<ty::t>,
                                unpack: fn(&ty::sty) -> Option<O>)
        -> Option<O> {
        match expected {
            Some(t) => {
                match resolve_type(fcx.infcx(), t, force_tvar) {
                    Ok(t) => unpack(&ty::get(t).sty),
                    _ => None
                }
            }
            _ => None
        }
    }

    fn check_expr_fn(fcx: @fn_ctxt,
                     expr: @ast::expr,
                     ast_proto_opt: Option<ast::Proto>,
                     decl: &ast::fn_decl,
                     body: ast::blk,
                     fn_kind: FnKind,
                     expected: Option<ty::t>) {
        let tcx = fcx.ccx.tcx;

        // Find the expected input/output types (if any).  Careful to
        // avoid capture of bound regions in the expected type.  See
        // def'n of br_cap_avoid() for a more lengthy explanation of
        // what's going on here.
        // Also try to pick up inferred purity and proto, defaulting
        // to impure and block. Note that we only will use those for
        // block syntax lambdas; that is, lambdas without explicit
        // protos.
        let expected_sty = unpack_expected(fcx, expected, |x| Some(copy *x));
        let (expected_tys,
             expected_purity,
             expected_proto,
             expected_onceness) = {
            match expected_sty {
                Some(ty::ty_fn(ref fn_ty)) => {
                    let id = expr.id;
                    let {fn_ty: fn_ty, _} =
                        replace_bound_regions_in_fn_ty(
                            tcx, @Nil, None, fn_ty,
                            |br| ty::re_bound(ty::br_cap_avoid(id, @br)));
                    (Some({inputs: /*bad*/copy fn_ty.sig.inputs,
                           output: fn_ty.sig.output}),
                     fn_ty.meta.purity,
                     fn_ty.meta.proto,
                     fn_ty.meta.onceness)
                }
                _ => {
                    (None, ast::impure_fn, ast::ProtoBorrowed, ast::Many)
                }
            }
        };

        // If the proto is specified, use that, otherwise select a
        // proto based on inference.
        let (proto, purity) = match ast_proto_opt {
            Some(p) => (p, ast::impure_fn),
            None => (expected_proto, expected_purity)
        };

        // construct the function type
        let mut fn_ty = astconv::ty_of_fn_decl(
            fcx, fcx,
            proto, purity, expected_onceness,
            /*bounds:*/ @~[], /*opt_region:*/ None,
            *decl, expected_tys, expr.span);

        // XXX: Bad copy.
        let fty = ty::mk_fn(tcx, copy fn_ty);

        debug!("check_expr_fn_with_unifier %s fty=%s",
               expr_to_str(expr, tcx.sess.intr()),
               fcx.infcx().ty_to_str(fty));

        fcx.write_ty(expr.id, fty);

        // We inherit the same self info as the enclosing scope,
        // since the function we're checking might capture `self`
        check_fn(fcx.ccx, fcx.self_info, &fn_ty, decl, body,
                 fn_kind, Some(fcx));
    }


    // Check field access expressions
    fn check_field(fcx: @fn_ctxt, expr: @ast::expr, is_callee: bool,
                   base: @ast::expr, field: ast::ident, tys: ~[@ast::Ty])
        -> bool
    {
        let tcx = fcx.ccx.tcx;
        let bot = check_expr(fcx, base);
        let expr_t = structurally_resolved_type(fcx, expr.span,
                                                fcx.expr_ty(base));
        let (base_t, derefs) = do_autoderef(fcx, expr.span, expr_t);
        let n_tys = tys.len();

        match structure_of(fcx, expr.span, base_t) {
            ty::ty_rec(fields) => {
                match ty::field_idx(field, fields) {
                    Some(ix) => {
                        if n_tys > 0 {
                            tcx.sess.span_err(
                                expr.span,
                                ~"can't provide type parameters \
                                  to a field access");
                        }
                        fcx.write_ty(expr.id, fields[ix].mt.ty);
                        fcx.write_autoderef_adjustment(base.id, derefs);
                        return bot;
                    }
                    _ => ()
                }
            }
            ty::ty_struct(base_id, ref substs) => {
                // This is just for fields -- the same code handles
                // methods in both classes and traits

                // (1) verify that the class id actually has a field called
                // field
                debug!("class named %s", ppaux::ty_to_str(tcx, base_t));
                let cls_items = ty::lookup_struct_fields(tcx, base_id);
                match lookup_field_ty(tcx, base_id, cls_items,
                                      field, &(*substs)) {
                    Some(field_ty) => {
                        // (2) look up what field's type is, and return it
                        fcx.write_ty(expr.id, field_ty);
                        fcx.write_autoderef_adjustment(base.id, derefs);
                        return bot;
                    }
                    None => ()
                }
            }
            _ => ()
        }

        let tps = vec::map(tys, |ty| fcx.to_ty(*ty));

        match method::lookup(fcx, expr, base, expr.id,
                             field, expr_t, tps, DontDerefArgs) {
            Some(ref entry) => {
                fcx.ccx.method_map.insert(expr.id, (*entry));

                // If we have resolved to a method but this is not in
                // a callee position, error
                if !is_callee {
                    tcx.sess.span_err(
                        expr.span,
                        ~"attempted to take value of method \
                          (try writing an anonymous function)");
                }
            }
            None => {
                fcx.type_error_message(expr.span,
                  |actual| {
                      fmt!("attempted access of field `%s` on type `%s`, but \
                            no field or method with that name was found",
                           tcx.sess.str_of(field), actual)
                  },
                  expr_t, None);
                // Add error type for the result
                fcx.write_ty(expr.id, ty::mk_err(tcx));
            }
        }

        return bot;
    }

    fn check_struct_or_variant_fields(fcx: @fn_ctxt,
                                      span: span,
                                      class_id: ast::def_id,
                                      substitutions: &ty::substs,
                                      field_types: ~[ty::field_ty],
                                      ast_fields: ~[ast::field],
                                      check_completeness: bool) -> bool {
        let tcx = fcx.ccx.tcx;
        let mut bot = false;

        debug!("%? %?", ast_fields.len(), field_types.len());

        let class_field_map = HashMap();
        let mut fields_found = 0;
        for field_types.each |field| {
            // XXX: Check visibility here.
            class_field_map.insert(field.ident, (field.id, false));
        }

        // Typecheck each field.
        for ast_fields.each |field| {
            match class_field_map.find(field.node.ident) {
                None => {
                    tcx.sess.span_err(
                        field.span,
                        fmt!("structure has no field named `%s`",
                             tcx.sess.str_of(field.node.ident)));
                }
                Some((_, true)) => {
                    tcx.sess.span_err(
                        field.span,
                        fmt!("field `%s` specified more than once",
                             tcx.sess.str_of(field.node.ident)));
                }
                Some((field_id, false)) => {
                    let expected_field_type =
                        ty::lookup_field_type(
                            tcx, class_id, field_id, substitutions);
                    bot |=
                        check_expr_assignable_to_type(
                            fcx,
                            field.node.expr,
                            expected_field_type);
                    class_field_map.insert(
                        field.node.ident, (field_id, true));
                    fields_found += 1;
                }
            }
        }

        if check_completeness {
            // Make sure the programmer specified all the fields.
            assert fields_found <= field_types.len();
            if fields_found < field_types.len() {
                let mut missing_fields = ~[];
                for field_types.each |class_field| {
                    let name = class_field.ident;
                    let (_, seen) = class_field_map.get(name);
                    if !seen {
                        missing_fields.push(
                            ~"`" + tcx.sess.str_of(name) + ~"`");
                    }
                }

                tcx.sess.span_err(span,
                                  fmt!("missing field%s: %s",
                                       if missing_fields.len() == 1 {
                                           ~""
                                       } else {
                                           ~"s"
                                       },
                                       str::connect(missing_fields, ~", ")));
            }
        }

        return bot;
    }

    fn check_struct_constructor(fcx: @fn_ctxt,
                                id: ast::node_id,
                                span: codemap::span,
                                class_id: ast::def_id,
                                fields: ~[ast::field],
                                base_expr: Option<@ast::expr>) -> bool {
        let mut bot = false;
        let tcx = fcx.ccx.tcx;

        // Look up the number of type parameters and the raw type, and
        // determine whether the class is region-parameterized.
        let type_parameter_count, region_parameterized, raw_type;
        if class_id.crate == ast::local_crate {
            region_parameterized =
                tcx.region_paramd_items.find(class_id.node);
            match tcx.items.find(class_id.node) {
                Some(ast_map::node_item(@ast::item {
                        node: ast::item_struct(_, ref type_parameters),
                        _
                    }, _)) => {

                    type_parameter_count = type_parameters.len();

                    let self_region =
                        bound_self_region(region_parameterized);

                    raw_type = ty::mk_struct(tcx, class_id, {
                        self_r: self_region,
                        self_ty: None,
                        tps: ty::ty_params_to_tys(
                            tcx,
                            /*bad*/copy *type_parameters)
                    });
                }
                _ => {
                    tcx.sess.span_bug(span,
                                      ~"resolve didn't map this to a class");
                }
            }
        } else {
            let item_type = ty::lookup_item_type(tcx, class_id);
            type_parameter_count = (*item_type.bounds).len();
            region_parameterized = item_type.region_param;
            raw_type = item_type.ty;
        }

        // Generate the struct type.
        let self_region =
            fcx.region_var_if_parameterized(region_parameterized,
                                            span,
                                            ty::re_scope(id));
        let type_parameters = fcx.infcx().next_ty_vars(type_parameter_count);
        let substitutions = {
            self_r: self_region,
            self_ty: None,
            tps: type_parameters
        };

        let struct_type = ty::subst(tcx, &substitutions, raw_type);

        // Look up and check the fields.
        let class_fields = ty::lookup_struct_fields(tcx, class_id);
        bot = check_struct_or_variant_fields(fcx,
                                             span,
                                             class_id,
                                             &substitutions,
                                             class_fields,
                                             fields,
                                             base_expr.is_none()) || bot;

        // Check the base expression if necessary.
        match base_expr {
            None => {}
            Some(base_expr) => {
                bot = check_expr_has_type(fcx, base_expr, struct_type) || bot
            }
        }

        // Write in the resulting type.
        fcx.write_ty(id, struct_type);
        return bot;
    }

    fn check_struct_enum_variant(fcx: @fn_ctxt,
                                 id: ast::node_id,
                                 span: codemap::span,
                                 enum_id: ast::def_id,
                                 variant_id: ast::def_id,
                                 fields: ~[ast::field]) -> bool {
        let mut bot = false;
        let tcx = fcx.ccx.tcx;

        // Look up the number of type parameters and the raw type, and
        // determine whether the enum is region-parameterized.
        let type_parameter_count, region_parameterized, raw_type;
        if enum_id.crate == ast::local_crate {
            region_parameterized =
                tcx.region_paramd_items.find(enum_id.node);
            match tcx.items.find(enum_id.node) {
                Some(ast_map::node_item(@ast::item {
                        node: ast::item_enum(_, ref type_parameters),
                        _
                    }, _)) => {

                    type_parameter_count = type_parameters.len();

                    let self_region =
                        bound_self_region(region_parameterized);

                    raw_type = ty::mk_enum(tcx, enum_id, {
                        self_r: self_region,
                        self_ty: None,
                        tps: ty::ty_params_to_tys(
                            tcx,
                            /*bad*/copy *type_parameters)
                    });
                }
                _ => {
                    tcx.sess.span_bug(span,
                                      ~"resolve didn't map this to an enum");
                }
            }
        } else {
            let item_type = ty::lookup_item_type(tcx, enum_id);
            type_parameter_count = (*item_type.bounds).len();
            region_parameterized = item_type.region_param;
            raw_type = item_type.ty;
        }

        // Generate the enum type.
        let self_region =
            fcx.region_var_if_parameterized(region_parameterized,
                                            span,
                                            ty::re_scope(id));
        let type_parameters = fcx.infcx().next_ty_vars(type_parameter_count);
        let substitutions = {
            self_r: self_region,
            self_ty: None,
            tps: type_parameters
        };

        let enum_type = ty::subst(tcx, &substitutions, raw_type);

        // Look up and check the enum variant fields.
        let variant_fields = ty::lookup_struct_fields(tcx, variant_id);
        bot = check_struct_or_variant_fields(fcx,
                                             span,
                                             variant_id,
                                             &substitutions,
                                             variant_fields,
                                             fields,
                                             true) || bot;

        // Write in the resulting type.
        fcx.write_ty(id, enum_type);
        return bot;
    }

    let tcx = fcx.ccx.tcx;
    let id = expr.id;
    let mut bot = false;
    match /*bad*/copy expr.node {
      ast::expr_vstore(ev, vst) => {
        let typ = match /*bad*/copy ev.node {
          ast::expr_lit(@ast::spanned { node: ast::lit_str(s), _ }) => {
            let tt = ast_expr_vstore_to_vstore(fcx, ev, str::len(*s), vst);
            ty::mk_estr(tcx, tt)
          }
          ast::expr_vec(args, mutbl) => {
            let tt = ast_expr_vstore_to_vstore(fcx, ev, args.len(), vst);
            let mutability;
            match vst {
                ast::expr_vstore_mut_box | ast::expr_vstore_mut_slice => {
                    mutability = ast::m_mutbl
                }
                _ => mutability = mutbl
            }
            let t: ty::t = fcx.infcx().next_ty_var();
            for args.each |e| { bot |= check_expr_has_type(fcx, *e, t); }
            ty::mk_evec(tcx, ty::mt {ty: t, mutbl: mutability}, tt)
          }
          ast::expr_repeat(element, count_expr, mutbl) => {
            let count = ty::eval_repeat_count(tcx, count_expr, expr.span);
            fcx.write_ty(count_expr.id, ty::mk_uint(tcx));
            let tt = ast_expr_vstore_to_vstore(fcx, ev, count, vst);
            let t: ty::t = fcx.infcx().next_ty_var();
            bot |= check_expr_has_type(fcx, element, t);
            ty::mk_evec(tcx, ty::mt {ty: t, mutbl: mutbl}, tt)
          }
          _ =>
            tcx.sess.span_bug(expr.span, ~"vstore modifier on non-sequence")
        };
        fcx.write_ty(ev.id, typ);
        fcx.write_ty(id, typ);
      }

      ast::expr_lit(lit) => {
        let typ = check_lit(fcx, lit);
        fcx.write_ty(id, typ);
      }
      ast::expr_binary(op, lhs, rhs) => {
        bot |= check_binop(fcx, expr, op, lhs, rhs);
      }
      ast::expr_assign_op(op, lhs, rhs) => {
        bot |= check_binop(fcx, expr, op, lhs, rhs);
        let lhs_t = fcx.expr_ty(lhs);
        let result_t = fcx.expr_ty(expr);
        demand::suptype(fcx, expr.span, result_t, lhs_t);

        // Overwrite result of check_binop...this preserves existing behavior
        // but seems quite dubious with regard to user-defined methods
        // and so forth. - Niko
        fcx.write_nil(expr.id);
      }
      ast::expr_unary(unop, oprnd) => {
        let exp_inner = do unpack_expected(fcx, expected) |sty| {
            match unop {
              ast::box(_) | ast::uniq(_) => match *sty {
                ty::ty_box(ref mt) | ty::ty_uniq(ref mt) => Some(mt.ty),
                _ => None
              },
              ast::not | ast::neg => expected,
              ast::deref => None
            }
        };
        bot = check_expr_with_opt_hint(fcx, oprnd, exp_inner);
        let mut oprnd_t = fcx.expr_ty(oprnd);
        match unop {
          ast::box(mutbl) => {
            oprnd_t = ty::mk_box(tcx, ty::mt {ty: oprnd_t, mutbl: mutbl});
          }
          ast::uniq(mutbl) => {
            oprnd_t = ty::mk_uniq(tcx, ty::mt {ty: oprnd_t, mutbl: mutbl});
          }
          ast::deref => {
            let sty = structure_of(fcx, expr.span, oprnd_t);

            match sty {
              // deref'ing an unsafe pointer requires that we be in an unsafe
              // context
              ty::ty_ptr(*) => {
                fcx.require_unsafe(
                    expr.span,
                    ~"dereference of unsafe pointer");
              }
              _ => { /*ok*/ }
            }

            let operand_ty = ty::deref_sty(tcx, &sty, true);

            match operand_ty {
              Some(mt) => {
                  oprnd_t = mt.ty
              }
              None => {
                match sty {
                  ty::ty_enum(*) => {
                    tcx.sess.span_err(
                        expr.span,
                        ~"can only dereference enums \
                         with a single variant which has a \
                         single argument");
                  }
                  ty::ty_struct(*) => {
                    tcx.sess.span_err(
                        expr.span,
                        ~"can only dereference structs with one anonymous \
                          field");
                  }
                  _ => {
                      fcx.type_error_message(expr.span, |actual| {
                          fmt!("type %s cannot be dereferenced", actual)
                      }, oprnd_t, None);
                  }
                }
              }
            }
          }
          ast::not => {
            oprnd_t = structurally_resolved_type(fcx, oprnd.span, oprnd_t);
            if !(ty::type_is_integral(oprnd_t) ||
                 ty::get(oprnd_t).sty == ty::ty_bool) {
                oprnd_t = check_user_unop(fcx, ~"!", ~"not", expr,
                                         oprnd, oprnd_t);
            }
          }
          ast::neg => {
            oprnd_t = structurally_resolved_type(fcx, oprnd.span, oprnd_t);
            if !(ty::type_is_integral(oprnd_t) ||
                 ty::type_is_fp(oprnd_t)) {
                oprnd_t = check_user_unop(fcx, ~"-", ~"neg", expr,
                                         oprnd, oprnd_t);
            }
          }
        }
        fcx.write_ty(id, oprnd_t);
      }
      ast::expr_addr_of(mutbl, oprnd) => {
          let hint = unpack_expected(
              fcx, expected,
              |sty| match *sty { ty::ty_rptr(_, ref mt) => Some(mt.ty),
                                 _ => None });
        bot = check_expr_with_opt_hint(fcx, oprnd, hint);

        // Note: at this point, we cannot say what the best lifetime
        // is to use for resulting pointer.  We want to use the
        // shortest lifetime possible so as to avoid spurious borrowck
        // errors.  Moreover, the longest lifetime will depend on the
        // precise details of the value whose address is being taken
        // (and how long it is valid), which we don't know yet until type
        // inference is complete.
        //
        // Therefore, here we simply generate a region variable with
        // the current expression as a lower bound.  The region
        // inferencer will then select the ultimate value.  Finally,
        // borrowck is charged with guaranteeing that the value whose
        // address was taken can actually be made to live as long as
        // it needs to live.
        let region = fcx.infcx().next_region_var(expr.span, expr.id);

        let tm = ty::mt { ty: fcx.expr_ty(oprnd), mutbl: mutbl };
        let oprnd_t = ty::mk_rptr(tcx, region, tm);
        fcx.write_ty(id, oprnd_t);
      }
      ast::expr_path(pth) => {
        let defn = lookup_def(fcx, pth.span, id);

        let tpt = ty_param_bounds_and_ty_for_def(fcx, expr.span, defn);
        let region_lb = ty::re_scope(expr.id);
        instantiate_path(fcx, pth, tpt, expr.span, expr.id, region_lb);
      }
      ast::expr_mac(_) => tcx.sess.bug(~"unexpanded macro"),
      ast::expr_fail(expr_opt) => {
        bot = true;
        match expr_opt {
          None => {/* do nothing */ }
          Some(e) => {
            check_expr_has_type(
                fcx, e, ty::mk_estr(tcx, ty::vstore_uniq));
          }
        }
        fcx.write_bot(id);
      }
      ast::expr_break(_) => { fcx.write_bot(id); bot = true; }
      ast::expr_again(_) => { fcx.write_bot(id); bot = true; }
      ast::expr_ret(expr_opt) => {
        bot = true;
        let ret_ty = match fcx.indirect_ret_ty {
          Some(t) =>  t, None => fcx.ret_ty
        };
        match expr_opt {
          None => match fcx.mk_eqty(false, expr.span,
                                    ret_ty, ty::mk_nil(tcx)) {
            result::Ok(_) => { /* fall through */ }
            result::Err(_) => {
                tcx.sess.span_err(
                    expr.span,
                    ~"`return;` in function returning non-nil");
            }
          },
          Some(e) => {
              check_expr_has_type(fcx, e, ret_ty);
          }
        }
        fcx.write_bot(id);
      }
      ast::expr_log(_, lv, e) => {
        bot = check_expr_has_type(fcx, lv,
                                  ty::mk_mach_uint(tcx, ast::ty_u32));

        // Note: this does not always execute, so do not propagate bot:
        check_expr(fcx, e);
        fcx.write_nil(id);
      }
      ast::expr_assert(e) => {
        bot = check_expr_has_type(fcx, e, ty::mk_bool(tcx));
        fcx.write_nil(id);
      }
      ast::expr_copy(a) | ast::expr_unary_move(a) => {
        bot = check_expr_with_opt_hint(fcx, a, expected);
        fcx.write_ty(id, fcx.expr_ty(a));
      }
      ast::expr_paren(a) => {
        bot = check_expr_with_opt_hint(fcx, a, expected);
        fcx.write_ty(id, fcx.expr_ty(a));
      }
      ast::expr_assign(lhs, rhs) => {
        bot = check_assignment(fcx, lhs, rhs, id);
      }
      ast::expr_swap(lhs, rhs) => {
        bot = check_assignment(fcx, lhs, rhs, id);
      }
      ast::expr_if(cond, ref thn, elsopt) => {
        bot = check_expr_has_type(fcx, cond, ty::mk_bool(tcx));
        bot |= check_then_else(fcx, *thn, elsopt, id, expr.span);
      }
      ast::expr_while(cond, ref body) => {
        bot = check_expr_has_type(fcx, cond, ty::mk_bool(tcx));
        check_block_no_value(fcx, (*body));
        fcx.write_ty(id, ty::mk_nil(tcx));
      }
      ast::expr_loop(ref body, _) => {
        check_block_no_value(fcx, (*body));
        fcx.write_ty(id, ty::mk_nil(tcx));
        bot = !may_break(tcx, expr.id, (*body));
      }
      ast::expr_match(discrim, ref arms) => {
        bot = _match::check_match(fcx, expr, discrim, (/*bad*/copy *arms));
      }
      ast::expr_fn(proto, ref decl, ref body, cap_clause) => {
        check_expr_fn(fcx, expr, Some(proto),
                      decl, (*body), Vanilla, expected);
        capture::check_capture_clause(tcx, expr.id, cap_clause);
      }
      ast::expr_fn_block(ref decl, ref body, cap_clause) => {
        check_expr_fn(fcx, expr, None,
                      decl, (*body), Vanilla, expected);
        capture::check_capture_clause(tcx, expr.id, cap_clause);
      }
      ast::expr_loop_body(b) => {
        // a loop body is the special argument to a `for` loop.  We know that
        // there will be an expected type in this context because it can only
        // appear in the context of a call, so we get the expected type of the
        // parameter. The catch here is that we need to validate two things:
        // 1. a closure that returns a bool is expected
        // 2. the closure that was given returns unit
        let mut err_happened = false;
        let expected_sty = unpack_expected(fcx, expected, |x| Some(copy *x));
        let inner_ty = match expected_sty {
          Some(ty::ty_fn(ref fty)) => {
            match fcx.mk_subty(false, expr.span,
                               (*fty).sig.output, ty::mk_bool(tcx)) {
              result::Ok(_) => (),
              result::Err(_) => {
                   fcx.type_error_message(expr.span,
                      |actual| {
                          fmt!("a `loop` function's last argument \
                                should return `bool`, not `%s`", actual)
                      },
                      (*fty).sig.output, None);
                err_happened = true;
                fcx.write_ty(id, ty::mk_err(tcx));
              }
            }
            ty::mk_fn(tcx, FnTyBase {
                meta: (*fty).meta,
                sig: FnSig {output: ty::mk_nil(tcx),
                            ../*bad*/copy (*fty).sig}
            })
          }
          _ =>
              match expected {
                  Some(expected_t) => {
                      fcx.type_error_message(expr.span, |actual| {
                          fmt!("a `loop` function's last \
                                argument should be of function \
                                type, not `%s`",
                               actual)
                      },
                                             expected_t, None);
                      let err_ty = ty::mk_err(tcx);
                      fcx.write_ty(id, err_ty);
                      err_happened = true;
                      err_ty
                  }
                  None => fcx.tcx().sess.impossible_case(expr.span,
                            ~"loop body must have an expected type")
              }
        };
        match b.node {
          ast::expr_fn_block(ref decl, ref body, cap_clause) => {
            check_expr_fn(fcx, b, None,
                          decl, *body, ForLoop, Some(inner_ty));
            demand::suptype(fcx, b.span, inner_ty, fcx.expr_ty(b));
            capture::check_capture_clause(tcx, b.id, cap_clause);
          }
          // argh
          _ => fail ~"expr_fn_block"
        }
        let block_ty = structurally_resolved_type(
            fcx, expr.span, fcx.node_ty(b.id));
        if err_happened {
            fcx.write_ty(expr.id, ty::mk_err(fcx.tcx()));
        } else {
            let loop_body_ty = ty::replace_fn_return_type(tcx, block_ty,
                                                          ty::mk_bool(tcx));
            fcx.write_ty(expr.id, loop_body_ty);
        }
      }
      ast::expr_do_body(b) => {
        let expected_sty = unpack_expected(fcx, expected, |x| Some(copy *x));
        let inner_ty = match expected_sty {
          Some(ty::ty_fn(ref fty)) => {
              ty::mk_fn(tcx, (/*bad*/copy *fty))
          }
          _ => match expected {
                  Some(expected_t) => {
                      fcx.type_error_message(expr.span, |_actual| {
                          ~"Non-function passed to a `do` \
                            function as its last argument, or wrong number \
                            of arguments passed to a `do` function"
                      }, expected_t, None);
                      let err_ty = ty::mk_err(tcx);
                      fcx.write_ty(id, err_ty);
                      err_ty
                  }
                  None => fcx.tcx().sess.impossible_case(expr.span,
                              ~"do body must have expected type")
              }
        };
        match b.node {
          ast::expr_fn_block(ref decl, ref body, cap_clause) => {
            check_expr_fn(fcx, b, None,
                          decl, *body, DoBlock, Some(inner_ty));
            demand::suptype(fcx, b.span, inner_ty, fcx.expr_ty(b));
            capture::check_capture_clause(tcx, b.id, cap_clause);
          }
          // argh
          _ => fail ~"expected fn ty"
        }
        fcx.write_ty(expr.id, fcx.node_ty(b.id));
      }
      ast::expr_block(ref b) => {
        // If this is an unchecked block, turn off purity-checking
        bot = check_block_with_expected(fcx, *b, expected);
        let typ =
            match b.node.expr {
              Some(expr) => fcx.expr_ty(expr),
              None => ty::mk_nil(tcx)
            };
        fcx.write_ty(id, typ);
      }
      ast::expr_call(f, args, _) => {
        bot = check_call(fcx, expr.span, expr.id, f, args);
      }
      ast::expr_method_call(rcvr, ident, tps, args, _) => {
        bot = check_method_call(fcx, expr, rcvr, ident, args, tps);
      }
      ast::expr_cast(e, t) => {
        bot = check_expr(fcx, e);
        let t_1 = fcx.to_ty(t);
        let t_e = fcx.expr_ty(e);

        debug!("t_1=%s", fcx.infcx().ty_to_str(t_1));
        debug!("t_e=%s", fcx.infcx().ty_to_str(t_e));

        match ty::get(t_1).sty {
          // This will be looked up later on
          ty::ty_trait(*) => (),

          _ => {
            if ty::type_is_nil(t_e) {
                fcx.type_error_message(expr.span, |actual| {
                    fmt!("cast from nil: `%s` as `%s`", actual,
                         fcx.infcx().ty_to_str(t_1))
                }, t_e, None);
            } else if ty::type_is_nil(t_1) {
                fcx.type_error_message(expr.span, |actual| {
                    fmt!("cast to nil: `%s` as `%s`", actual,
                         fcx.infcx().ty_to_str(t_1))
                }, t_e, None);
            }

            let t_1_is_scalar = type_is_scalar(fcx, expr.span, t_1);
            if type_is_c_like_enum(fcx,expr.span,t_e) && t_1_is_scalar {
                /* this case is allowed */
            } else if !(type_is_scalar(fcx,expr.span,t_e) && t_1_is_scalar) {
                /*
                If more type combinations should be supported than are
                supported here, then file an enhancement issue and record the
                issue number in this comment.
                */
                fcx.type_error_message(expr.span, |actual| {
                    fmt!("non-scalar cast: `%s` as `%s`", actual,
                         fcx.infcx().ty_to_str(t_1))
                }, t_e, None);
            }
          }
        }
        fcx.write_ty(id, t_1);
      }
      ast::expr_vec(args, mutbl) => {
        let t: ty::t = fcx.infcx().next_ty_var();
        for args.each |e| { bot |= check_expr_has_type(fcx, *e, t); }
        let typ = ty::mk_evec(tcx, ty::mt {ty: t, mutbl: mutbl},
                              ty::vstore_fixed(args.len()));
        fcx.write_ty(id, typ);
      }
      ast::expr_repeat(element, count_expr, mutbl) => {
        let count = ty::eval_repeat_count(tcx, count_expr, expr.span);
        fcx.write_ty(count_expr.id, ty::mk_uint(tcx));
        let t: ty::t = fcx.infcx().next_ty_var();
        bot |= check_expr_has_type(fcx, element, t);
        let t = ty::mk_evec(tcx, ty::mt {ty: t, mutbl: mutbl},
                            ty::vstore_fixed(count));
        fcx.write_ty(id, t);
      }
      ast::expr_tup(elts) => {
        let flds = unpack_expected(fcx, expected, |sty| {
            match *sty { ty::ty_tup(ref flds) => Some(copy *flds), _ => None }
        });
        let elt_ts = do elts.mapi |i, e| {
            check_expr_with_opt_hint(fcx, *e, flds.map(|fs| fs[i]));
            fcx.expr_ty(*e)
        };
        let typ = ty::mk_tup(tcx, elt_ts);
        fcx.write_ty(id, typ);
      }
      ast::expr_rec(ref fields, base) => {
        for base.each |b| { check_expr_with_opt_hint(fcx, *b, expected); }
        let expected = option::or(expected,
                                  base.map(|b| fcx.expr_ty(*b)));
        let flds = unpack_expected(fcx, expected, |sty|
            match *sty {
                ty::ty_rec(ref flds) => Some(copy *flds),
                _ => None
            }
        );
        let fields_t = fields.map(|f| {
            let hint_mt =
                flds.chain_ref(
                    |flds| vec::find(*flds, |tf| tf.ident == f.node.ident));
            let hint = hint_mt.map(|tf| tf.mt.ty);
            bot |= check_expr_with_opt_hint(fcx, f.node.expr, hint);
            let expr_t = fcx.expr_ty(f.node.expr);
            let expr_mt = ty::mt {ty: expr_t, mutbl: f.node.mutbl};
            // for the most precise error message,
            // should be f.node.expr.span, not f.span
            respan(f.node.expr.span, {ident: f.node.ident, mt: expr_mt})
        });
        match base {
          None => {
            fn get_node(f: &spanned<field>) -> field { f.node }
            let typ = ty::mk_rec(tcx, vec::map(fields_t, get_node));
            fcx.write_ty(id, typ);
            /* Check for duplicate fields */
            /* Only do this check if there's no base expr -- the reason is
               that we're extending a record we know has no dup fields, and
               it would be ill-typed anyway if we duplicated one of its
               fields */
            check_no_duplicate_fields(tcx, (*fields).map(|f|
                                                    (f.node.ident, f.span)));
          }
          Some(bexpr) => {
            let bexpr_t = fcx.expr_ty(bexpr);
            let base_fields =  match structure_of(fcx, expr.span, bexpr_t) {
              ty::ty_rec(flds) => flds,
              _ => {
                  fcx.type_error_message(expr.span, |_actual| {
                      ~"record update has non-record base"
                  }, bexpr_t, None);
                fcx.write_ty(id, ty::mk_err(tcx));
                return true;
              }
            };
            fcx.write_ty(id, bexpr_t);
            for fields_t.each |f| {
                let mut found = false;
                for base_fields.each |bf| {
                    if f.node.ident == bf.ident {
                        demand::suptype(fcx, f.span, bf.mt.ty, f.node.mt.ty);
                        found = true;
                    }
                }
                if !found {
                    tcx.sess.span_err(f.span,
                                        ~"unknown field in record update: " +
                                        tcx.sess.str_of(f.node.ident));
                    fcx.write_ty(id, ty::mk_err(tcx));
                    return true;
                }
            }
          }
        }
      }
      ast::expr_struct(path, ref fields, base_expr) => {
        // Resolve the path.
        match tcx.def_map.find(id) {
            Some(ast::def_struct(type_def_id)) => {
                check_struct_constructor(fcx, id, expr.span, type_def_id,
                                         (/*bad*/copy *fields), base_expr);
            }
            Some(ast::def_variant(enum_id, variant_id)) => {
                check_struct_enum_variant(fcx, id, expr.span, enum_id,
                                          variant_id, (/*bad*/copy *fields));
            }
            _ => {
                tcx.sess.span_bug(path.span, ~"structure constructor does \
                                               not name a structure type");
            }
        }
      }
      ast::expr_field(base, field, tys) => {
        bot = check_field(fcx, expr, false, base, field, tys);
      }
      ast::expr_index(base, idx) => {
          bot |= check_expr(fcx, base);
          let raw_base_t = fcx.expr_ty(base);
          let (base_t, derefs) = do_autoderef(fcx, expr.span, raw_base_t);
          bot |= check_expr(fcx, idx);
          let idx_t = fcx.expr_ty(idx);
          let base_sty = structure_of(fcx, expr.span, base_t);
          match ty::index_sty(tcx, &base_sty) {
              Some(mt) => {
                  require_integral(fcx, idx.span, idx_t);
                  fcx.write_ty(id, mt.ty);
                  fcx.write_autoderef_adjustment(base.id, derefs);
              }
              None => {
                  let resolved = structurally_resolved_type(fcx, expr.span,
                                                            raw_base_t);
                  match lookup_op_method(fcx, expr, base, resolved,
                                         tcx.sess.ident_of(~"index"),
                                         ~[idx], DontDerefArgs) {
                      Some((ret_ty, _)) => fcx.write_ty(id, ret_ty),
                      _ => {
                          fcx.type_error_message(expr.span, |actual|
                              fmt!("cannot index a value of type `%s`",
                                   actual), base_t, None);
                          fcx.write_ty(id, ty::mk_err(tcx));
                          return true;
                      }
                  }
              }
          }
      }
    }
    if bot { fcx.write_bot(expr.id); }

    debug!("type of expr %s is...",
           syntax::print::pprust::expr_to_str(expr, tcx.sess.intr()));
    debug!("... %s, expected is %s",
           ppaux::ty_to_str(tcx, fcx.expr_ty(expr)),
           match expected {
               Some(t) => ppaux::ty_to_str(tcx, t),
               _ => ~"empty"
           });

    unifier();

    debug!("<< bot=%b", bot);
    return bot;
}

fn require_integral(fcx: @fn_ctxt, sp: span, t: ty::t) {
    if !type_is_integral(fcx, sp, t) {
        fcx.type_error_message(sp, |actual| {
            fmt!("mismatched types: expected integral type but found `%s`",
                 actual)
        }, t, None);
    }
}

fn check_decl_initializer(fcx: @fn_ctxt, nid: ast::node_id,
                          init: @ast::expr) -> bool {
    let lty = ty::mk_var(fcx.ccx.tcx, lookup_local(fcx, init.span, nid));
    return check_expr_assignable_to_type(fcx, init, lty);
}

fn check_decl_local(fcx: @fn_ctxt, local: @ast::local) -> bool {
    let mut bot = false;
    let tcx = fcx.ccx.tcx;

    let t = ty::mk_var(tcx, fcx.inh.locals.get(local.node.id));
    fcx.write_ty(local.node.id, t);

    match local.node.init {
        Some(init) => {
            bot = check_decl_initializer(fcx, local.node.id, init);
        }
        _ => {}
    }

    let region =
        ty::re_scope(tcx.region_map.get(local.node.id));
    let pcx = pat_ctxt {
        fcx: fcx,
        map: pat_id_map(tcx.def_map, local.node.pat),
        match_region: region,
        block_region: region,
    };
    _match::check_pat(pcx, local.node.pat, t);
    return bot;
}

fn check_stmt(fcx: @fn_ctxt, stmt: @ast::stmt) -> bool {
    let mut node_id;
    let mut bot = false;
    match stmt.node {
      ast::stmt_decl(decl, id) => {
        node_id = id;
        match /*bad*/copy decl.node {
          ast::decl_local(ls) => for ls.each |l| {
            bot |= check_decl_local(fcx, *l);
          },
          ast::decl_item(_) => {/* ignore for now */ }
        }
      }
      ast::stmt_expr(expr, id) => {
        node_id = id;
        bot = check_expr_has_type(fcx, expr, ty::mk_nil(fcx.ccx.tcx));
      }
      ast::stmt_semi(expr, id) => {
        node_id = id;
        bot = check_expr(fcx, expr);
      }
      ast::stmt_mac(*) => fcx.ccx.tcx.sess.bug(~"unexpanded macro")
    }
    fcx.write_nil(node_id);
    return bot;
}

fn check_block_no_value(fcx: @fn_ctxt, blk: ast::blk) -> bool {
    let bot = check_block(fcx, blk);
    if !bot {
        let blkty = fcx.node_ty(blk.node.id);
        let nilty = ty::mk_nil(fcx.ccx.tcx);
        demand::suptype(fcx, blk.span, nilty, blkty);
    }
    return bot;
}

fn check_block(fcx0: @fn_ctxt, blk: ast::blk) -> bool {
    check_block_with_expected(fcx0, blk, None)
}

fn check_block_with_expected(fcx0: @fn_ctxt,
                             blk: ast::blk,
                             expected: Option<ty::t>) -> bool {
    let fcx = match blk.node.rules {
      ast::unsafe_blk => @fn_ctxt {purity: ast::unsafe_fn,.. copy *fcx0},
      ast::default_blk => fcx0
    };
    do fcx.with_region_lb(blk.node.id) {
        let mut bot = false;
        let mut warned = false;
        for blk.node.stmts.each |s| {
            if bot && !warned &&
                match s.node {
                  ast::stmt_decl(@ast::spanned { node: ast::decl_local(_),
                                                 _}, _) |
                  ast::stmt_expr(_, _) | ast::stmt_semi(_, _) => {
                    true
                  }
                  _ => false
                } {
                fcx.ccx.tcx.sess.span_warn(s.span, ~"unreachable statement");
                warned = true;
            }
            bot |= check_stmt(fcx, *s);
        }
        match blk.node.expr {
          None => fcx.write_nil(blk.node.id),
          Some(e) => {
            if bot && !warned {
                fcx.ccx.tcx.sess.span_warn(e.span, ~"unreachable expression");
            }
            bot |= check_expr_with_opt_hint(fcx, e, expected);
            let ety = fcx.expr_ty(e);
            fcx.write_ty(blk.node.id, ety);
          }
        }
        if bot {
            fcx.write_bot(blk.node.id);
        }
        bot
    }
}

fn check_const(ccx: @crate_ctxt, _sp: span, e: @ast::expr, id: ast::node_id) {
    let rty = ty::node_id_to_type(ccx.tcx, id);
    let fcx = blank_fn_ctxt(ccx, rty, e.id);
    let declty = fcx.ccx.tcx.tcache.get(local_def(id)).ty;
    check_const_with_ty(fcx, _sp, e, declty);
}

fn check_const_with_ty(fcx: @fn_ctxt, _sp: span, e: @ast::expr,
                       declty: ty::t) {
    check_expr(fcx, e);
    let cty = fcx.expr_ty(e);
    demand::suptype(fcx, e.span, declty, cty);
    regionck::regionck_expr(fcx, e);
    writeback::resolve_type_vars_in_expr(fcx, e);
}

/// Checks whether a type can be created without an instance of itself.
/// This is similar but different from the question of whether a type
/// can be represented.  For example, the following type:
///
///     enum foo { None, Some(foo) }
///
/// is instantiable but is not representable.  Similarly, the type
///
///     enum foo { Some(@foo) }
///
/// is representable, but not instantiable.
fn check_instantiable(tcx: ty::ctxt,
                      sp: span,
                      item_id: ast::node_id) {
    let item_ty = ty::node_id_to_type(tcx, item_id);
    if !ty::is_instantiable(tcx, item_ty) {
        tcx.sess.span_err(sp, fmt!("this type cannot be instantiated \
                  without an instance of itself; \
                  consider using `option<%s>`",
                                   ppaux::ty_to_str(tcx, item_ty)));
    }
}

fn check_enum_variants(ccx: @crate_ctxt,
                       sp: span,
                       +vs: ~[ast::variant],
                       id: ast::node_id) {
    fn do_check(ccx: @crate_ctxt, sp: span, vs: ~[ast::variant],
                id: ast::node_id, disr_vals: &mut ~[int], disr_val: &mut int,
                variants: &mut ~[ty::VariantInfo]) {
        let rty = ty::node_id_to_type(ccx.tcx, id);
        for vs.each |v| {
            do v.node.disr_expr.iter |e_ref| {
                let e = *e_ref;
                debug!("disr expr, checking %s",
                       expr_to_str(e, ccx.tcx.sess.intr()));
                let declty = ty::mk_int(ccx.tcx);
                let fcx = blank_fn_ctxt(ccx, rty, e.id);
                check_const_with_ty(fcx, e.span, e, declty);
                // check_expr (from check_const pass) doesn't guarantee
                // that the expression is in an form that eval_const_expr can
                // handle, so we may still get an internal compiler error

                match const_eval::eval_const_expr_partial(ccx.tcx, e) {
                  Ok(const_eval::const_int(val)) => {
                    *disr_val = val as int;
                  }
                  Ok(_) => {
                    ccx.tcx.sess.span_err(e.span, ~"expected signed integer \
                                                    constant");
                  }
                  Err(ref err) => {
                    ccx.tcx.sess.span_err(e.span,
                     fmt!("expected constant: %s", (*err)));

                  }
                }
            }
            if vec::contains(*disr_vals, &*disr_val) {
                ccx.tcx.sess.span_err(v.span,
                                      ~"discriminator value already exists");
            }
            disr_vals.push(*disr_val);
            let ctor_ty = ty::node_id_to_type(ccx.tcx, v.node.id);
            let arg_tys;

            let this_disr_val = *disr_val;
            *disr_val += 1;

            match v.node.kind {
                ast::tuple_variant_kind(ref args) if args.len() > 0u => {
                    arg_tys = Some(ty::ty_fn_args(ctor_ty).map(|a| a.ty));
                }
                ast::tuple_variant_kind(_) => {
                    arg_tys = Some(~[]);
                }
                ast::struct_variant_kind(_) => {
                    arg_tys = Some(ty::lookup_struct_fields(
                        ccx.tcx, local_def(v.node.id)).map(|cf|
                            ty::node_id_to_type(ccx.tcx, cf.id.node)));
                }
                ast::enum_variant_kind(_) => {
                    arg_tys = None;
                    do_check(ccx,
                             sp,
                             /*bad*/copy vs,
                             id,
                             &mut *disr_vals,
                             &mut *disr_val,
                             &mut *variants);
                }
            }

            match arg_tys {
                None => {}
                Some(arg_tys) => {
                    variants.push(
                        @VariantInfo_{args: arg_tys, ctor_ty: ctor_ty,
                          name: v.node.name, id: local_def(v.node.id),
                          disr_val: this_disr_val, vis: v.node.vis});
                }
            }
        }
    }

    let rty = ty::node_id_to_type(ccx.tcx, id);
    let mut disr_vals: ~[int] = ~[];
    let mut disr_val = 0;
    let mut variants = ~[];

    do_check(ccx, sp, vs, id, &mut disr_vals, &mut disr_val, &mut variants);

    // cache so that ty::enum_variants won't repeat this work
    ccx.tcx.enum_var_cache.insert(local_def(id), @variants);

    // Check that it is possible to represent this enum:
    let mut outer = true, did = local_def(id);
    if ty::type_structurally_contains(ccx.tcx, rty, |sty| {
        match *sty {
          ty::ty_enum(id, _) if id == did => {
            if outer { outer = false; false }
            else { true }
          }
          _ => false
        }
    }) {
        ccx.tcx.sess.span_err(sp, ~"illegal recursive enum type; \
                                 wrap the inner value in a box to \
                                 make it representable");
    }

    // Check that it is possible to instantiate this enum:
    //
    // This *sounds* like the same that as representable, but it's
    // not.  See def'n of `check_instantiable()` for details.
    check_instantiable(ccx.tcx, sp, id);
}

pub fn lookup_local(fcx: @fn_ctxt, sp: span, id: ast::node_id) -> TyVid {
    match fcx.inh.locals.find(id) {
        Some(x) => x,
        _ => {
            fcx.ccx.tcx.sess.span_fatal(
                sp,
                ~"internal error looking up a local var")
        }
    }
}

fn lookup_def(fcx: @fn_ctxt, sp: span, id: ast::node_id) -> ast::def {
    lookup_def_ccx(fcx.ccx, sp, id)
}

// Returns the type parameter count and the type for the given definition.
fn ty_param_bounds_and_ty_for_def(fcx: @fn_ctxt, sp: span, defn: ast::def) ->
    ty_param_bounds_and_ty {

    match defn {
      ast::def_arg(nid, _) | ast::def_local(nid, _) |
      ast::def_self(nid, _) | ast::def_binding(nid, _) => {
        assert (fcx.inh.locals.contains_key(nid));
        let typ = ty::mk_var(fcx.ccx.tcx, lookup_local(fcx, sp, nid));
        return no_params(typ);
      }
      ast::def_fn(_, ast::extern_fn) => {
        // extern functions are just u8 pointers
        return {
            bounds: @~[],
            region_param: None,
            ty: ty::mk_ptr(
                fcx.ccx.tcx,
                ty::mt {
                    ty: ty::mk_mach_uint(fcx.ccx.tcx, ast::ty_u8),
                    mutbl: ast::m_imm
                })
        };
      }

      ast::def_fn(id, ast::unsafe_fn) |
      ast::def_static_method(id, _, ast::unsafe_fn) => {
        // Unsafe functions can only be touched in an unsafe context
        fcx.require_unsafe(sp, ~"access to unsafe function");
        return ty::lookup_item_type(fcx.ccx.tcx, id);
      }

      ast::def_fn(id, _) | ast::def_static_method(id, _, _) |
      ast::def_const(id) | ast::def_variant(_, id) |
      ast::def_struct(id) => {
        return ty::lookup_item_type(fcx.ccx.tcx, id);
      }
      ast::def_upvar(_, inner, _, _) => {
        return ty_param_bounds_and_ty_for_def(fcx, sp, *inner);
      }
      ast::def_ty(_) | ast::def_prim_ty(_) | ast::def_ty_param(*)=> {
        fcx.ccx.tcx.sess.span_bug(sp, ~"expected value but found type");
      }
      ast::def_mod(*) | ast::def_foreign_mod(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, ~"expected value but found module");
      }
      ast::def_use(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, ~"expected value but found use");
      }
      ast::def_region(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, ~"expected value but found region");
      }
      ast::def_typaram_binder(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, ~"expected value but found type \
                                          parameter");
      }
      ast::def_label(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, ~"expected value but found label");
      }
      ast::def_self_ty(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, ~"expected value but found self ty");
      }
    }
}

// Instantiates the given path, which must refer to an item with the given
// number of type parameters and type.
fn instantiate_path(fcx: @fn_ctxt,
                    pth: @ast::path,
                    tpt: ty_param_bounds_and_ty,
                    span: span,
                    node_id: ast::node_id,
                    region_lb: ty::Region) {
    debug!(">>> instantiate_path");

    let ty_param_count = vec::len(*tpt.bounds);
    let ty_substs_len = vec::len(pth.types);

    // determine the region bound, using the value given by the user
    // (if any) and otherwise using a fresh region variable
    let self_r = match pth.rp {
      Some(r) => {
        match tpt.region_param {
          None => {
            fcx.ccx.tcx.sess.span_err
                (span, ~"this item is not region-parameterized");
            None
          }
          Some(_) => {
            Some(ast_region_to_region(fcx, fcx, span, r))
          }
        }
      }
      None => {
        fcx.region_var_if_parameterized(
            tpt.region_param, span, region_lb)
      }
    };

    // determine values for type parameters, using the values given by
    // the user (if any) and otherwise using fresh type variables
    let tps = if ty_substs_len == 0 {
        fcx.infcx().next_ty_vars(ty_param_count)
    } else if ty_param_count == 0 {
        fcx.ccx.tcx.sess.span_err
            (span, ~"this item does not take type parameters");
        fcx.infcx().next_ty_vars(ty_param_count)
    } else if ty_substs_len > ty_param_count {
        fcx.ccx.tcx.sess.span_err
            (span, ~"too many type parameters provided for this item");
        fcx.infcx().next_ty_vars(ty_param_count)
    } else if ty_substs_len < ty_param_count {
        fcx.ccx.tcx.sess.span_err
            (span, ~"not enough type parameters provided for this item");
        fcx.infcx().next_ty_vars(ty_param_count)
    } else {
        pth.types.map(|aty| fcx.to_ty(*aty))
    };

    let substs = {self_r: self_r, self_ty: None, tps: tps};
    fcx.write_ty_substs(node_id, tpt.ty, substs);

    debug!("<<<");
}

// Resolves `typ` by a single level if `typ` is a type variable.  If no
// resolution is possible, then an error is reported.
fn structurally_resolved_type(fcx: @fn_ctxt, sp: span, tp: ty::t) -> ty::t {
    match infer::resolve_type(fcx.infcx(), tp, force_tvar) {
        Ok(t_s) if !ty::type_is_ty_var(t_s) => return t_s,
        _ => {
            fcx.type_error_message(sp, |_actual| {
                ~"the type of this value must be known in this context"
            }, tp, None);
            return ty::mk_err(fcx.tcx());
        }
    }
}

// Returns the one-level-deep structure of the given type.
fn structure_of(fcx: @fn_ctxt, sp: span, typ: ty::t) -> ty::sty {
    /*bad*/copy ty::get(structurally_resolved_type(fcx, sp, typ)).sty
}

fn type_is_integral(fcx: @fn_ctxt, sp: span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    return ty::type_is_integral(typ_s);
}

fn type_is_scalar(fcx: @fn_ctxt, sp: span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    return ty::type_is_scalar(typ_s);
}

fn type_is_c_like_enum(fcx: @fn_ctxt, sp: span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    return ty::type_is_c_like_enum(fcx.ccx.tcx, typ_s);
}

fn ast_expr_vstore_to_vstore(fcx: @fn_ctxt, e: @ast::expr, n: uint,
                             v: ast::expr_vstore) -> ty::vstore {
    match v {
        ast::expr_vstore_fixed(None) => ty::vstore_fixed(n),
        ast::expr_vstore_fixed(Some(u)) => {
            if n != u {
                let s = fmt!("fixed-size sequence mismatch: %u vs. %u",u, n);
                fcx.ccx.tcx.sess.span_err(e.span,s);
            }
            ty::vstore_fixed(u)
        }
        ast::expr_vstore_uniq => ty::vstore_uniq,
        ast::expr_vstore_box | ast::expr_vstore_mut_box => ty::vstore_box,
        ast::expr_vstore_slice | ast::expr_vstore_mut_slice => {
            let r = fcx.infcx().next_region_var(e.span, e.id);
            ty::vstore_slice(r)
        }
    }
}

// Returns true if b contains a break that can exit from b
fn may_break(cx: ty::ctxt, id: ast::node_id, b: ast::blk) -> bool {
    // First: is there an unlabeled break immediately
    // inside the loop?
    (loop_query(b, |e| {
        match e {
            ast::expr_break(_) => true,
            _ => false
        }
    })) ||
   // Second: is there a labeled break with label
   // <id> nested anywhere inside the loop?
    (block_query(b, |e| {
        match e.node {
            ast::expr_break(Some(_)) =>
                match cx.def_map.find(e.id) {
                    Some(ast::def_label(loop_id)) if id == loop_id => true,
                    _ => false,
                },
            _ => false
        }}))
}

fn check_bounds_are_used(ccx: @crate_ctxt,
                         span: span,
                         tps: ~[ast::ty_param],
                         ty: ty::t) {
    debug!("check_bounds_are_used(n_tps=%u, ty=%s)",
           tps.len(), ppaux::ty_to_str(ccx.tcx, ty));

    // make a vector of booleans initially false, set to true when used
    if tps.len() == 0u { return; }
    let tps_used = vec::to_mut(vec::from_elem(tps.len(), false));

    ty::walk_regions_and_ty(
        ccx.tcx, ty,
        |_r| {},
        |t| {
            match ty::get(t).sty {
              ty::ty_param({idx, _}) => {
                  debug!("Found use of ty param #%u", idx);
                  tps_used[idx] = true;
              }
              _ => ()
            }
            true
        });

    for tps_used.eachi |i, b| {
        if !*b {
            ccx.tcx.sess.span_err(
                span, fmt!("type parameter `%s` is unused",
                           ccx.tcx.sess.str_of(tps[i].ident)));
        }
    }
}

fn check_intrinsic_type(ccx: @crate_ctxt, it: @ast::foreign_item) {
    fn param(ccx: @crate_ctxt, n: uint) -> ty::t {
        ty::mk_param(ccx.tcx, n, local_def(0))
    }
    fn arg(m: ast::rmode, ty: ty::t) -> ty::arg {
        {mode: ast::expl(m), ty: ty}
    }
    let tcx = ccx.tcx;
    let (n_tps, inputs, output) = match ccx.tcx.sess.str_of(it.ident) {
      ~"size_of" |
      ~"pref_align_of" | ~"min_align_of" => (1u, ~[], ty::mk_uint(ccx.tcx)),
      ~"init" => (1u, ~[], param(ccx, 0u)),
      ~"forget" => (1u, ~[arg(ast::by_move, param(ccx, 0u))],
                    ty::mk_nil(tcx)),
      ~"reinterpret_cast" => (2u, ~[arg(ast::by_ref, param(ccx, 0u))],
                              param(ccx, 1u)),
      ~"addr_of" => (1u, ~[arg(ast::by_ref, param(ccx, 0u))],
                      ty::mk_imm_ptr(tcx, param(ccx, 0u))),
      ~"move_val" | ~"move_val_init" => {
          (1u, ~[arg(ast::by_copy,
                     ty::mk_mut_rptr(tcx, ty::re_bound(ty::br_anon(0)),
                                     param(ccx, 0u))),
               arg(ast::by_move, param(ccx, 0u))],
         ty::mk_nil(tcx))
      }
      ~"needs_drop" => (1u, ~[], ty::mk_bool(tcx)),

      ~"atomic_cxchg"    | ~"atomic_cxchg_acq"| ~"atomic_cxchg_rel" => {
        (0u, ~[arg(ast::by_copy,
                   ty::mk_mut_rptr(tcx, ty::re_bound(ty::br_anon(0)),
                                   ty::mk_int(tcx))),
               arg(ast::by_copy, ty::mk_int(tcx)),
               arg(ast::by_copy, ty::mk_int(tcx))],
         ty::mk_int(tcx))
      }
      ~"atomic_xchg"     | ~"atomic_xadd"     | ~"atomic_xsub"     |
      ~"atomic_xchg_acq" | ~"atomic_xadd_acq" | ~"atomic_xsub_acq" |
      ~"atomic_xchg_rel" | ~"atomic_xadd_rel" | ~"atomic_xsub_rel" => {
        (0u, ~[arg(ast::by_copy,
                   ty::mk_mut_rptr(tcx, ty::re_bound(ty::br_anon(0)),
                                   ty::mk_int(tcx))),
               arg(ast::by_copy, ty::mk_int(tcx))],
         ty::mk_int(tcx))
      }

      ~"get_tydesc" => {
        // FIXME (#3730): return *intrinsic::tydesc, not *()
        (1u, ~[], ty::mk_nil_ptr(tcx))
      }
      ~"visit_tydesc" => {
          let tydesc_name = special_idents::tydesc;
          let ty_visitor_name = tcx.sess.ident_of(~"TyVisitor");
          assert tcx.intrinsic_defs.contains_key(tydesc_name);
          assert ccx.tcx.intrinsic_defs.contains_key(ty_visitor_name);
          let (_, tydesc_ty) = tcx.intrinsic_defs.get(tydesc_name);
          let (_, visitor_trait) = tcx.intrinsic_defs.get(ty_visitor_name);
          let td_ptr = ty::mk_ptr(ccx.tcx, ty::mt {ty: tydesc_ty,
                                                   mutbl: ast::m_imm});
          (0u, ~[arg(ast::by_val, td_ptr),
                 arg(ast::by_ref, visitor_trait)], ty::mk_nil(tcx))
      }
      ~"frame_address" => {
        let fty = ty::mk_fn(ccx.tcx, FnTyBase {
            meta: FnMeta {purity: ast::impure_fn,
                          proto: ast::ProtoBorrowed,
                          onceness: ast::Once,
                          region: ty::re_bound(ty::br_anon(0)),
                          bounds: @~[]},
            sig: FnSig {inputs: ~[{mode: ast::expl(ast::by_val),
                                   ty: ty::mk_imm_ptr(
                                       ccx.tcx,
                                       ty::mk_mach_uint(ccx.tcx, ast::ty_u8))
                                  }],
                        output: ty::mk_nil(ccx.tcx)}
        });
        (0u, ~[arg(ast::by_ref, fty)], ty::mk_nil(tcx))
      }
      ~"morestack_addr" => {
        (0u, ~[], ty::mk_nil_ptr(tcx))
      }
     ~"sqrtf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"sqrtf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"powif32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx)),
               arg(ast::by_copy, ty::mk_i32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"powif64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx)),
               arg(ast::by_copy, ty::mk_i32(tcx))],
         ty::mk_f64(tcx))
     }
     ~"sinf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"sinf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"cosf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"cosf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"powf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx)),
               arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"powf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx)),
               arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"expf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"expf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"exp2f32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"exp2f64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"logf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"logf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"log10f32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"log10f64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"log2f32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"log2f64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"fmaf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx)),
               arg(ast::by_copy, ty::mk_f32(tcx)),
               arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"fmaf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx)),
               arg(ast::by_copy, ty::mk_f64(tcx)),
               arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"fabsf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"fabsf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"floorf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"floorf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"ceilf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"ceilf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"truncf32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f32(tcx))],
         ty::mk_f32(tcx))
     }
     ~"truncf64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_f64(tcx))],
         ty::mk_f64(tcx))
     }
     ~"ctpop8" => {
        (0u, ~[arg(ast::by_copy, ty::mk_i8(tcx))],
         ty::mk_i8(tcx))
     }
     ~"ctpop16" => {
        (0u, ~[arg(ast::by_copy, ty::mk_i16(tcx))],
         ty::mk_i16(tcx))
     }
     ~"ctpop32" => {
        (0u, ~[arg(ast::by_copy, ty::mk_i32(tcx))],
         ty::mk_i32(tcx))
     }
     ~"ctpop64" => {
        (0u, ~[arg(ast::by_copy, ty::mk_i64(tcx))],
         ty::mk_i64(tcx))
     }
     ~"ctlz8" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i8(tcx))],
         ty::mk_i8(tcx))
     }
     ~"ctlz16" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i16(tcx))],
         ty::mk_i16(tcx))
     }
     ~"ctlz32" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i32(tcx))],
         ty::mk_i32(tcx))
     }
     ~"ctlz64" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i64(tcx))],
         ty::mk_i64(tcx))
     }
     ~"cttz8" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i8(tcx))],
         ty::mk_i8(tcx))
     }
     ~"cttz16" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i16(tcx))],
         ty::mk_i16(tcx))
     }
     ~"cttz32" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i32(tcx))],
         ty::mk_i32(tcx))
     }
     ~"cttz64" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i64(tcx))],
         ty::mk_i64(tcx))
     }
     ~"bswap16" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i16(tcx))],
         ty::mk_i16(tcx))
     }
     ~"bswap32" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i32(tcx))],
         ty::mk_i32(tcx))
     }
     ~"bswap64" => {
         (0u, ~[arg(ast::by_copy, ty::mk_i64(tcx))],
         ty::mk_i64(tcx))
     }
     ref other => {
        tcx.sess.span_err(it.span, ~"unrecognized intrinsic function: `" +
                          (*other) + ~"`");
        return;
      }
    };
    let fty = ty::mk_fn(tcx, FnTyBase {
        meta: FnMeta {purity: ast::impure_fn,
                      proto: ast::ProtoBare,
                      onceness: ast::Many,
                      region: ty::re_static,
                      bounds: @~[]},
        sig: FnSig {inputs: inputs,
                    output: output}
    });
    let i_ty = ty::lookup_item_type(ccx.tcx, local_def(it.id));
    let i_n_tps = (*i_ty.bounds).len();
    if i_n_tps != n_tps {
        tcx.sess.span_err(it.span, fmt!("intrinsic has wrong number \
                                         of type parameters: found %u, \
                                         expected %u", i_n_tps, n_tps));
    } else {
        require_same_types(
            tcx, None, false, it.span, i_ty.ty, fty,
            || fmt!("intrinsic has wrong type: \
                      expected `%s`",
                     ppaux::ty_to_str(ccx.tcx, fty)));
    }
}
