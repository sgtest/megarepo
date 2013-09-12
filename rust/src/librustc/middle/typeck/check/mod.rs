// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
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


use middle::const_eval;
use middle::pat_util::pat_id_map;
use middle::pat_util;
use middle::lint::unreachable_code;
use middle::ty::{FnSig, VariantInfo};
use middle::ty::{ty_param_bounds_and_ty, ty_param_substs_and_ty};
use middle::ty::{substs, param_ty, Disr, ExprTyProvider};
use middle::ty;
use middle::typeck::astconv::AstConv;
use middle::typeck::astconv::{ast_region_to_region, ast_ty_to_ty};
use middle::typeck::astconv;
use middle::typeck::check::_match::pat_ctxt;
use middle::typeck::check::method::{AutoderefReceiver};
use middle::typeck::check::method::{AutoderefReceiverFlag};
use middle::typeck::check::method::{CheckTraitsAndInherentMethods};
use middle::typeck::check::method::{CheckTraitsOnly, DontAutoderefReceiver};
use middle::typeck::check::regionmanip::replace_bound_regions_in_fn_sig;
use middle::typeck::check::regionmanip::relate_free_regions;
use middle::typeck::check::vtable::{LocationInfo, VtableContext};
use middle::typeck::CrateCtxt;
use middle::typeck::infer::{resolve_type, force_tvar};
use middle::typeck::infer;
use middle::typeck::rscope::bound_self_region;
use middle::typeck::rscope::{RegionError};
use middle::typeck::rscope::RegionScope;
use middle::typeck::{isr_alist, lookup_def_ccx};
use middle::typeck::no_params;
use middle::typeck::{require_same_types, method_map, vtable_map};
use util::common::{block_query, indenter, loop_query};
use util::ppaux::{bound_region_ptr_to_str};
use util::ppaux;


use std::cast::transmute;
use std::hashmap::HashMap;
use std::result;
use std::util::replace;
use std::vec;
use extra::list::Nil;
use syntax::abi::AbiSet;
use syntax::ast::{provided, required};
use syntax::ast;
use syntax::ast_map;
use syntax::ast_util::local_def;
use syntax::ast_util;
use syntax::codemap::Span;
use syntax::codemap;
use syntax::opt_vec::OptVec;
use syntax::opt_vec;
use syntax::parse::token;
use syntax::parse::token::special_idents;
use syntax::print::pprust;
use syntax::visit;
use syntax::visit::Visitor;
use syntax;

pub mod _match;
pub mod vtable;
pub mod writeback;
pub mod regionmanip;
pub mod regionck;
pub mod demand;
pub mod method;

pub struct SelfInfo {
    self_ty: ty::t,
    self_id: ast::NodeId,
    span: Span
}

/// Fields that are part of a `FnCtxt` which are inherited by
/// closures defined within the function.  For example:
///
///     fn foo() {
///         do bar() { ... }
///     }
///
/// Here, the function `foo()` and the closure passed to
/// `bar()` will each have their own `FnCtxt`, but they will
/// share the inherited fields.
pub struct inherited {
    infcx: @mut infer::InferCtxt,
    locals: @mut HashMap<ast::NodeId, ty::t>,

    // Temporary tables:
    node_types: @mut HashMap<ast::NodeId, ty::t>,
    node_type_substs: @mut HashMap<ast::NodeId, ty::substs>,
    adjustments: @mut HashMap<ast::NodeId, @ty::AutoAdjustment>,
    method_map: method_map,
    vtable_map: vtable_map,
}

#[deriving(Clone)]
pub enum FnKind {
    // A do-closure.
    DoBlock,

    // A normal closure or fn item.
    Vanilla
}

#[deriving(Clone)]
pub struct PurityState {
    def: ast::NodeId,
    purity: ast::purity,
    priv from_fn: bool
}

impl PurityState {
    pub fn function(purity: ast::purity, def: ast::NodeId) -> PurityState {
        PurityState { def: def, purity: purity, from_fn: true }
    }

    pub fn recurse(&mut self, blk: &ast::Block) -> PurityState {
        match self.purity {
            // If this unsafe, then if the outer function was already marked as
            // unsafe we shouldn't attribute the unsafe'ness to the block. This
            // way the block can be warned about instead of ignoring this
            // extraneous block (functions are never warned about).
            ast::unsafe_fn if self.from_fn => *self,

            purity => {
                let (purity, def) = match blk.rules {
                    ast::UnsafeBlock(*) => (ast::unsafe_fn, blk.id),
                    ast::DefaultBlock => (purity, self.def),
                };
                PurityState{ def: def,
                             purity: purity,
                             from_fn: false }
            }
        }
    }
}

/// Whether `check_binop` allows overloaded operators to be invoked.
#[deriving(Eq)]
enum AllowOverloadedOperatorsFlag {
    AllowOverloadedOperators,
    DontAllowOverloadedOperators,
}

#[deriving(Clone)]
pub struct FnCtxt {
    // Number of errors that had been reported when we started
    // checking this function. On exit, if we find that *more* errors
    // have been reported, we will skip regionck and other work that
    // expects the types within the function to be consistent.
    err_count_on_creation: uint,

    ret_ty: ty::t,
    ps: PurityState,

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
    region_lb: ast::NodeId,

    // Says whether we're inside a for loop, in a do block
    // or neither. Helps with error messages involving the
    // function return type.
    fn_kind: FnKind,

    in_scope_regions: isr_alist,

    inh: @inherited,

    ccx: @mut CrateCtxt,
}

pub fn blank_inherited(ccx: @mut CrateCtxt) -> @inherited {
    @inherited {
        infcx: infer::new_infer_ctxt(ccx.tcx),
        locals: @mut HashMap::new(),
        node_types: @mut HashMap::new(),
        node_type_substs: @mut HashMap::new(),
        adjustments: @mut HashMap::new(),
        method_map: @mut HashMap::new(),
        vtable_map: @mut HashMap::new(),
    }
}

// Used by check_const and check_enum_variants
pub fn blank_fn_ctxt(ccx: @mut CrateCtxt,
                     rty: ty::t,
                     region_bnd: ast::NodeId)
                  -> @mut FnCtxt {
// It's kind of a kludge to manufacture a fake function context
// and statement context, but we might as well do write the code only once
    @mut FnCtxt {
        err_count_on_creation: ccx.tcx.sess.err_count(),
        ret_ty: rty,
        ps: PurityState::function(ast::impure_fn, 0),
        region_lb: region_bnd,
        in_scope_regions: @Nil,
        fn_kind: Vanilla,
        inh: blank_inherited(ccx),
        ccx: ccx
    }
}

impl ExprTyProvider for FnCtxt {
    fn expr_ty(&self, ex: &ast::Expr) -> ty::t {
        self.expr_ty(ex)
    }

    fn ty_ctxt(&self) -> ty::ctxt {
        self.ccx.tcx
    }
}

struct CheckItemTypesVisitor { ccx: @mut CrateCtxt }

impl Visitor<()> for CheckItemTypesVisitor {
    fn visit_item(&mut self, i:@ast::item, _:()) {
        check_item(self.ccx, i);
        visit::walk_item(self, i, ());
    }
}

pub fn check_item_types(ccx: @mut CrateCtxt, crate: &ast::Crate) {
    let mut visit = CheckItemTypesVisitor { ccx: ccx };
    visit::walk_crate(&mut visit, crate, ());
}

pub fn check_bare_fn(ccx: @mut CrateCtxt,
                     decl: &ast::fn_decl,
                     body: &ast::Block,
                     id: ast::NodeId,
                     self_info: Option<SelfInfo>) {
    let fty = ty::node_id_to_type(ccx.tcx, id);
    match ty::get(fty).sty {
        ty::ty_bare_fn(ref fn_ty) => {
            let fcx =
                check_fn(ccx, self_info, fn_ty.purity,
                         &fn_ty.sig, decl, id, body, Vanilla,
                         @Nil, blank_inherited(ccx));;

            vtable::resolve_in_block(fcx, body);
            regionck::regionck_fn(fcx, body);
            writeback::resolve_type_vars_in_fn(fcx, decl, body, self_info);
        }
        _ => ccx.tcx.sess.impossible_case(body.span,
                                 "check_bare_fn: function type expected")
    }
}

struct GatherLocalsVisitor {
                     fcx: @mut FnCtxt,
                     tcx: ty::ctxt,
}

impl GatherLocalsVisitor {
    fn assign(&mut self, nid: ast::NodeId, ty_opt: Option<ty::t>) {
            match ty_opt {
                None => {
                    // infer the variable's type
                    let var_id = self.fcx.infcx().next_ty_var_id();
                    let var_ty = ty::mk_var(self.fcx.tcx(), var_id);
                    self.fcx.inh.locals.insert(nid, var_ty);
                }
                Some(typ) => {
                    // take type that the user specified
                    self.fcx.inh.locals.insert(nid, typ);
                }
            }
    }
}

impl Visitor<()> for GatherLocalsVisitor {
        // Add explicitly-declared locals.
    fn visit_local(&mut self, local:@ast::Local, _:()) {
            let o_ty = match local.ty.node {
              ast::ty_infer => None,
              _ => Some(self.fcx.to_ty(&local.ty))
            };
            self.assign(local.id, o_ty);
            debug!("Local variable %s is assigned type %s",
                   self.fcx.pat_to_str(local.pat),
                   self.fcx.infcx().ty_to_str(
                       self.fcx.inh.locals.get_copy(&local.id)));
            visit::walk_local(self, local, ());

    }
        // Add pattern bindings.
    fn visit_pat(&mut self, p:@ast::Pat, _:()) {
            match p.node {
              ast::PatIdent(_, ref path, _)
                  if pat_util::pat_is_binding(self.fcx.ccx.tcx.def_map, p) => {
                self.assign(p.id, None);
                debug!("Pattern binding %s is assigned to %s",
                       self.tcx.sess.str_of(path.segments[0].identifier),
                       self.fcx.infcx().ty_to_str(
                           self.fcx.inh.locals.get_copy(&p.id)));
              }
              _ => {}
            }
            visit::walk_pat(self, p, ());

    }

    fn visit_block(&mut self, b:&ast::Block, _:()) {
            // non-obvious: the `blk` variable maps to region lb, so
            // we have to keep this up-to-date.  This
            // is... unfortunate.  It'd be nice to not need this.
            do self.fcx.with_region_lb(b.id) {
                visit::walk_block(self, b, ());
            }
    }

        // Don't descend into fns and items
    fn visit_fn(&mut self, _:&visit::fn_kind, _:&ast::fn_decl,
                _:&ast::Block, _:Span, _:ast::NodeId, _:()) { }
    fn visit_item(&mut self, _:@ast::item, _:()) { }

}

pub fn check_fn(ccx: @mut CrateCtxt,
                opt_self_info: Option<SelfInfo>,
                purity: ast::purity,
                fn_sig: &ty::FnSig,
                decl: &ast::fn_decl,
                id: ast::NodeId,
                body: &ast::Block,
                fn_kind: FnKind,
                inherited_isr: isr_alist,
                inherited: @inherited) -> @mut FnCtxt
{
    /*!
     *
     * Helper used by check_bare_fn and check_expr_fn.  Does the
     * grungy work of checking a function body and returns the
     * function context used for that purpose, since in the case of a
     * fn item there is still a bit more to do.
     *
     * - ...
     * - inherited_isr: regions in scope from the enclosing fn (if any)
     * - inherited: other fields inherited from the enclosing fn (if any)
     */

    let tcx = ccx.tcx;
    let err_count_on_creation = tcx.sess.err_count();

    // ______________________________________________________________________
    // First, we have to replace any bound regions in the fn and self
    // types with free ones.  The free region references will be bound
    // the node_id of the body block.
    let (isr, opt_self_info, fn_sig) = {
        let opt_self_ty = opt_self_info.map(|i| i.self_ty);
        let (isr, opt_self_ty, fn_sig) =
            replace_bound_regions_in_fn_sig(
                tcx, inherited_isr, opt_self_ty, fn_sig,
                |br| ty::re_free(ty::FreeRegion {scope_id: body.id,
                                                 bound_region: br}));
        let opt_self_info =
            opt_self_info.map_move(
                |si| SelfInfo {self_ty: opt_self_ty.unwrap(), .. si});
        (isr, opt_self_info, fn_sig)
    };

    relate_free_regions(tcx, opt_self_info.map(|s| s.self_ty), &fn_sig);

    let arg_tys = fn_sig.inputs.map(|a| *a);
    let ret_ty = fn_sig.output;

    debug!("check_fn(arg_tys=%?, ret_ty=%?, opt_self_ty=%?)",
           arg_tys.map(|&a| ppaux::ty_to_str(tcx, a)),
           ppaux::ty_to_str(tcx, ret_ty),
           opt_self_info.map(|si| ppaux::ty_to_str(tcx, si.self_ty)));

    // ______________________________________________________________________
    // Create the function context.  This is either derived from scratch or,
    // in the case of function expressions, based on the outer context.
    let fcx: @mut FnCtxt = {
        @mut FnCtxt {
            err_count_on_creation: err_count_on_creation,
            ret_ty: ret_ty,
            ps: PurityState::function(purity, id),
            region_lb: body.id,
            in_scope_regions: isr,
            fn_kind: fn_kind,
            inh: inherited,
            ccx: ccx
        }
    };

    gather_locals(fcx, decl, body, arg_tys, opt_self_info);
    check_block_with_expected(fcx, body, Some(ret_ty));

    // We unify the tail expr's type with the
    // function result type, if there is a tail expr.
    match body.expr {
      Some(tail_expr) => {
        let tail_expr_ty = fcx.expr_ty(tail_expr);
        // Special case: we print a special error if there appears
        // to be do-block/for-loop confusion
        demand::suptype_with_fn(fcx, tail_expr.span, false,
            fcx.ret_ty, tail_expr_ty,
            |sp, e, a, s| {
                fcx.report_mismatched_return_types(sp, e, a, s) });
      }
      None => ()
    }

    for self_info in opt_self_info.iter() {
        fcx.write_ty(self_info.self_id, self_info.self_ty);
    }
    for (input, arg) in decl.inputs.iter().zip(arg_tys.iter()) {
        fcx.write_ty(input.id, *arg);
    }

    return fcx;

    fn gather_locals(fcx: @mut FnCtxt,
                     decl: &ast::fn_decl,
                     body: &ast::Block,
                     arg_tys: &[ty::t],
                     opt_self_info: Option<SelfInfo>) {
        let tcx = fcx.ccx.tcx;

        let mut visit = GatherLocalsVisitor { fcx: fcx, tcx: tcx, };

        // Add the self parameter
        for self_info in opt_self_info.iter() {
            visit.assign(self_info.self_id, Some(self_info.self_ty));
            debug!("self is assigned to %s",
                   fcx.infcx().ty_to_str(
                       fcx.inh.locals.get_copy(&self_info.self_id)));
        }

        // Add formal parameters.
        for (arg_ty, input) in arg_tys.iter().zip(decl.inputs.iter()) {
            // Create type variables for each argument.
            do pat_util::pat_bindings(tcx.def_map, input.pat)
                    |_bm, pat_id, _sp, _path| {
                visit.assign(pat_id, None);
            }

            // Check the pattern.
            let pcx = pat_ctxt {
                fcx: fcx,
                map: pat_id_map(tcx.def_map, input.pat),
            };
            _match::check_pat(&pcx, input.pat, *arg_ty);
        }

        visit.visit_block(body, ());
    }
}

pub fn check_method(ccx: @mut CrateCtxt,
                    method: @ast::method)
{
    let method_def_id = local_def(method.id);
    let method_ty = ty::method(ccx.tcx, method_def_id);
    let opt_self_info = method_ty.transformed_self_ty.map_move(|ty| {
        SelfInfo {self_ty: ty,
                  self_id: method.self_id,
                  span: method.explicit_self.span}
    });

    check_bare_fn(
        ccx,
        &method.decl,
        &method.body,
        method.id,
        opt_self_info
    );
}

pub fn check_no_duplicate_fields(tcx: ty::ctxt,
                                 fields: ~[(ast::Ident, Span)]) {
    let mut field_names = HashMap::new();

    for p in fields.iter() {
        let (id, sp) = *p;
        let orig_sp = field_names.find(&id).map_move(|x| *x);
        match orig_sp {
            Some(orig_sp) => {
                tcx.sess.span_err(sp, fmt!("Duplicate field name %s in record type declaration",
                                           tcx.sess.str_of(id)));
                tcx.sess.span_note(orig_sp, "First declaration of this field occurred here");
                break;
            }
            None => {
                field_names.insert(id, sp);
            }
        }
    }
}

pub fn check_struct(ccx: @mut CrateCtxt, id: ast::NodeId, span: Span) {
    let tcx = ccx.tcx;

    // Check that the class is instantiable
    check_instantiable(tcx, span, id);

    if ty::lookup_simd(tcx, local_def(id)) {
        check_simd(tcx, span, id);
    }
}

pub fn check_item(ccx: @mut CrateCtxt, it: @ast::item) {
    debug!("check_item(it.id=%d, it.ident=%s)",
           it.id,
           ty::item_path_str(ccx.tcx, local_def(it.id)));
    let _indenter = indenter();

    match it.node {
      ast::item_static(_, _, e) => check_const(ccx, it.span, e, it.id),
      ast::item_enum(ref enum_definition, _) => {
        check_enum_variants(ccx,
                            it.span,
                            enum_definition.variants,
                            it.id);
      }
      ast::item_fn(ref decl, _, _, _, ref body) => {
        check_bare_fn(ccx, decl, body, it.id, None);
      }
      ast::item_impl(_, _, _, ref ms) => {
        let rp = ccx.tcx.region_paramd_items.find(&it.id).map_move(|x| *x);
        debug!("item_impl %s with id %d rp %?",
               ccx.tcx.sess.str_of(it.ident), it.id, rp);
        for m in ms.iter() {
            check_method(ccx, *m);
        }
        vtable::resolve_impl(ccx, it);
      }
      ast::item_trait(_, _, ref trait_methods) => {
        for trait_method in (*trait_methods).iter() {
            match *trait_method {
              required(*) => {
                // Nothing to do, since required methods don't have
                // bodies to check.
              }
              provided(m) => {
                check_method(ccx, m);
              }
            }
        }
      }
      ast::item_struct(*) => {
        check_struct(ccx, it.id, it.span);
      }
      ast::item_ty(ref t, ref generics) => {
        let tpt_ty = ty::node_id_to_type(ccx.tcx, it.id);
        check_bounds_are_used(ccx, t.span, &generics.ty_params, tpt_ty);
      }
      ast::item_foreign_mod(ref m) => {
        if m.abis.is_intrinsic() {
            for item in m.items.iter() {
                check_intrinsic_type(ccx, *item);
            }
        } else {
            for item in m.items.iter() {
                let tpt = ty::lookup_item_type(ccx.tcx, local_def(item.id));
                if tpt.generics.has_type_params() {
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

impl AstConv for FnCtxt {
    fn tcx(&self) -> ty::ctxt { self.ccx.tcx }

    fn get_item_ty(&self, id: ast::DefId) -> ty::ty_param_bounds_and_ty {
        ty::lookup_item_type(self.tcx(), id)
    }

    fn get_trait_def(&self, id: ast::DefId) -> @ty::TraitDef {
        ty::lookup_trait_def(self.tcx(), id)
    }

    fn ty_infer(&self, _span: Span) -> ty::t {
        self.infcx().next_ty_var()
    }
}

impl FnCtxt {
    pub fn infcx(&self) -> @mut infer::InferCtxt {
        self.inh.infcx
    }
    pub fn err_count_since_creation(&self) -> uint {
        self.ccx.tcx.sess.err_count() - self.err_count_on_creation
    }
    pub fn search_in_scope_regions(&self,
                                   span: Span,
                                   br: ty::bound_region)
                                   -> Result<ty::Region, RegionError> {
        let in_scope_regions = self.in_scope_regions;
        match in_scope_regions.find(br) {
            Some(r) => result::Ok(r),
            None => {
                let blk_br = ty::br_named(special_idents::blk);
                if br == blk_br {
                    result::Ok(self.block_region())
                } else {
                    result::Err(RegionError {
                        msg: {
                            fmt!("named region `%s` not in scope here",
                                 bound_region_ptr_to_str(self.tcx(), br))
                        },
                        replacement: {
                            self.infcx().next_region_var(
                                infer::BoundRegionError(span))
                        }
                    })
                }
            }
        }
    }
}

impl RegionScope for FnCtxt {
    fn anon_region(&self, span: Span) -> Result<ty::Region, RegionError> {
        result::Ok(self.infcx().next_region_var(infer::MiscVariable(span)))
    }
    fn self_region(&self, span: Span) -> Result<ty::Region, RegionError> {
        self.search_in_scope_regions(span, ty::br_self)
    }
    fn named_region(&self,
                    span: Span,
                    id: ast::Ident) -> Result<ty::Region, RegionError> {
        self.search_in_scope_regions(span, ty::br_named(id))
    }
}

impl FnCtxt {
    pub fn tag(&self) -> ~str {
        unsafe {
            fmt!("%x", transmute(self))
        }
    }

    pub fn local_ty(&self, span: Span, nid: ast::NodeId) -> ty::t {
        match self.inh.locals.find(&nid) {
            Some(&t) => t,
            None => {
                self.tcx().sess.span_bug(
                    span,
                    fmt!("No type for local variable %?", nid));
            }
        }
    }

    pub fn block_region(&self) -> ty::Region {
        ty::re_scope(self.region_lb)
    }

    #[inline]
    pub fn write_ty(&self, node_id: ast::NodeId, ty: ty::t) {
        debug!("write_ty(%d, %s) in fcx %s",
               node_id, ppaux::ty_to_str(self.tcx(), ty), self.tag());
        self.inh.node_types.insert(node_id, ty);
    }

    pub fn write_substs(&self, node_id: ast::NodeId, substs: ty::substs) {
        if !ty::substs_is_noop(&substs) {
            debug!("write_substs(%d, %s) in fcx %s",
                   node_id,
                   ty::substs_to_str(self.tcx(), &substs),
                   self.tag());
            self.inh.node_type_substs.insert(node_id, substs);
        }
    }

    pub fn write_ty_substs(&self,
                           node_id: ast::NodeId,
                           ty: ty::t,
                           substs: ty::substs) {
        let ty = ty::subst(self.tcx(), &substs, ty);
        self.write_ty(node_id, ty);
        self.write_substs(node_id, substs);
    }

    pub fn write_autoderef_adjustment(&self,
                                      node_id: ast::NodeId,
                                      derefs: uint) {
        if derefs == 0 { return; }
        self.write_adjustment(
            node_id,
            @ty::AutoDerefRef(ty::AutoDerefRef {
                autoderefs: derefs,
                autoref: None })
        );
    }

    pub fn write_adjustment(&self,
                            node_id: ast::NodeId,
                            adj: @ty::AutoAdjustment) {
        debug!("write_adjustment(node_id=%?, adj=%?)", node_id, adj);
        self.inh.adjustments.insert(node_id, adj);
    }

    pub fn write_nil(&self, node_id: ast::NodeId) {
        self.write_ty(node_id, ty::mk_nil());
    }
    pub fn write_bot(&self, node_id: ast::NodeId) {
        self.write_ty(node_id, ty::mk_bot());
    }
    pub fn write_error(@mut self, node_id: ast::NodeId) {
        self.write_ty(node_id, ty::mk_err());
    }

    pub fn to_ty(&self, ast_t: &ast::Ty) -> ty::t {
        ast_ty_to_ty(self, self, ast_t)
    }

    pub fn pat_to_str(&self, pat: @ast::Pat) -> ~str {
        pat.repr(self.tcx())
    }

    pub fn expr_ty(&self, ex: &ast::Expr) -> ty::t {
        match self.inh.node_types.find(&ex.id) {
            Some(&t) => t,
            None => {
                self.tcx().sess.bug(fmt!("no type for expr in fcx %s",
                                         self.tag()));
            }
        }
    }

    pub fn node_ty(&self, id: ast::NodeId) -> ty::t {
        match self.inh.node_types.find(&id) {
            Some(&t) => t,
            None => {
                self.tcx().sess.bug(
                    fmt!("no type for node %d: %s in fcx %s",
                         id, ast_map::node_id_to_str(
                             self.tcx().items, id,
                             token::get_ident_interner()),
                         self.tag()));
            }
        }
    }

    pub fn node_ty_substs(&self, id: ast::NodeId) -> ty::substs {
        match self.inh.node_type_substs.find(&id) {
            Some(ts) => (*ts).clone(),
            None => {
                self.tcx().sess.bug(
                    fmt!("no type substs for node %d: %s in fcx %s",
                         id, ast_map::node_id_to_str(
                             self.tcx().items, id,
                             token::get_ident_interner()),
                         self.tag()));
            }
        }
    }

    pub fn opt_node_ty_substs(&self,
                              id: ast::NodeId,
                              f: &fn(&ty::substs) -> bool)
                              -> bool {
        match self.inh.node_type_substs.find(&id) {
            Some(s) => f(s),
            None => true
        }
    }

    pub fn mk_subty(&self,
                    a_is_expected: bool,
                    origin: infer::TypeOrigin,
                    sub: ty::t,
                    sup: ty::t)
                    -> Result<(), ty::type_err> {
        infer::mk_subty(self.infcx(), a_is_expected, origin, sub, sup)
    }

    pub fn can_mk_subty(&self, sub: ty::t, sup: ty::t)
                        -> Result<(), ty::type_err> {
        infer::can_mk_subty(self.infcx(), sub, sup)
    }

    pub fn mk_assignty(&self,
                       expr: @ast::Expr,
                       sub: ty::t,
                       sup: ty::t)
                       -> Result<(), ty::type_err> {
        match infer::mk_coercety(self.infcx(),
                                 false,
                                 infer::ExprAssignable(expr),
                                 sub,
                                 sup) {
            Ok(None) => result::Ok(()),
            Err(ref e) => result::Err((*e)),
            Ok(Some(adjustment)) => {
                self.write_adjustment(expr.id, adjustment);
                Ok(())
            }
        }
    }

    pub fn can_mk_assignty(&self, sub: ty::t, sup: ty::t)
                           -> Result<(), ty::type_err> {
        infer::can_mk_coercety(self.infcx(), sub, sup)
    }

    pub fn mk_eqty(&self,
                   a_is_expected: bool,
                   origin: infer::TypeOrigin,
                   sub: ty::t,
                   sup: ty::t)
                   -> Result<(), ty::type_err> {
        infer::mk_eqty(self.infcx(), a_is_expected, origin, sub, sup)
    }

    pub fn mk_subr(&self,
                   a_is_expected: bool,
                   origin: infer::SubregionOrigin,
                   sub: ty::Region,
                   sup: ty::Region) {
        infer::mk_subr(self.infcx(), a_is_expected, origin, sub, sup)
    }

    pub fn with_region_lb<R>(@mut self, lb: ast::NodeId, f: &fn() -> R)
                             -> R {
        let old_region_lb = self.region_lb;
        self.region_lb = lb;
        let v = f();
        self.region_lb = old_region_lb;
        v
    }

    pub fn region_var_if_parameterized(&self,
                                       rp: Option<ty::region_variance>,
                                       span: Span)
                                       -> OptVec<ty::Region> {
        match rp {
            None => opt_vec::Empty,
            Some(_) => {
                opt_vec::with(
                    self.infcx().next_region_var(
                        infer::BoundRegionInTypeOrImpl(span)))
            }
        }
    }

    pub fn type_error_message(&self,
                              sp: Span,
                              mk_msg: &fn(~str) -> ~str,
                              actual_ty: ty::t,
                              err: Option<&ty::type_err>) {
        self.infcx().type_error_message(sp, mk_msg, actual_ty, err);
    }

    pub fn report_mismatched_return_types(&self,
                                          sp: Span,
                                          e: ty::t,
                                          a: ty::t,
                                          err: &ty::type_err) {
        // Derived error
        if ty::type_is_error(e) || ty::type_is_error(a) {
            return;
        }
        self.infcx().report_mismatched_types(sp, e, a, err)
    }

    pub fn report_mismatched_types(&self,
                                   sp: Span,
                                   e: ty::t,
                                   a: ty::t,
                                   err: &ty::type_err) {
        self.infcx().report_mismatched_types(sp, e, a, err)
    }
}

pub fn do_autoderef(fcx: @mut FnCtxt, sp: Span, t: ty::t) -> (ty::t, uint) {
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
        match *sty {
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
                if enum_dids.contains(did) {
                    return (t1, autoderefs);
                }
                enum_dids.push(*did);
            }
            _ => { /*ok*/ }
        }

        // Otherwise, deref if type is derefable:
        match ty::deref_sty(fcx.ccx.tcx, sty, false) {
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
pub fn check_lit(fcx: @mut FnCtxt, lit: @ast::lit) -> ty::t {
    let tcx = fcx.ccx.tcx;

    match lit.node {
      ast::lit_str(*) => ty::mk_estr(tcx, ty::vstore_slice(ty::re_static)),
      ast::lit_char(_) => ty::mk_char(),
      ast::lit_int(_, t) => ty::mk_mach_int(t),
      ast::lit_uint(_, t) => ty::mk_mach_uint(t),
      ast::lit_int_unsuffixed(_) => {
        // An unsuffixed integer literal could have any integral type,
        // so we create an integral type variable for it.
        ty::mk_int_var(tcx, fcx.infcx().next_int_var_id())
      }
      ast::lit_float(_, t) => ty::mk_mach_float(t),
      ast::lit_float_unsuffixed(_) => {
        // An unsuffixed floating point literal could have any floating point
        // type, so we create a floating point type variable for it.
        ty::mk_float_var(tcx, fcx.infcx().next_float_var_id())
      }
      ast::lit_nil => ty::mk_nil(),
      ast::lit_bool(_) => ty::mk_bool()
    }
}

pub fn valid_range_bounds(ccx: @mut CrateCtxt,
                          from: @ast::Expr,
                          to: @ast::Expr)
                       -> Option<bool> {
    match const_eval::compare_lit_exprs(ccx.tcx, from, to) {
        Some(val) => Some(val <= 0),
        None => None
    }
}

pub fn check_expr_has_type(
    fcx: @mut FnCtxt, expr: @ast::Expr,
    expected: ty::t) {
    do check_expr_with_unifier(fcx, expr, Some(expected)) {
        demand::suptype(fcx, expr.span, expected, fcx.expr_ty(expr));
    }
}

pub fn check_expr_coercable_to_type(
    fcx: @mut FnCtxt, expr: @ast::Expr,
    expected: ty::t) {
    do check_expr_with_unifier(fcx, expr, Some(expected)) {
        demand::coerce(fcx, expr.span, expected, expr)
    }
}

pub fn check_expr_with_hint(
    fcx: @mut FnCtxt, expr: @ast::Expr,
    expected: ty::t) {
    check_expr_with_unifier(fcx, expr, Some(expected), || ())
}

pub fn check_expr_with_opt_hint(
    fcx: @mut FnCtxt, expr: @ast::Expr,
    expected: Option<ty::t>)  {
    check_expr_with_unifier(fcx, expr, expected, || ())
}

pub fn check_expr(fcx: @mut FnCtxt, expr: @ast::Expr)  {
    check_expr_with_unifier(fcx, expr, None, || ())
}

// determine the `self` type, using fresh variables for all variables
// declared on the impl declaration e.g., `impl<A,B> for ~[(A,B)]`
// would return ($0, $1) where $0 and $1 are freshly instantiated type
// variables.
pub fn impl_self_ty(vcx: &VtableContext,
                    location_info: &LocationInfo, // (potential) receiver for
                                                  // this impl
                    did: ast::DefId)
                 -> ty_param_substs_and_ty {
    let tcx = vcx.tcx();

    let (n_tps, region_param, raw_ty) = {
        let ity = ty::lookup_item_type(tcx, did);
        (ity.generics.type_param_defs.len(), ity.generics.region_param, ity.ty)
    };

    let regions = ty::NonerasedRegions(if region_param.is_some() {
        opt_vec::with(vcx.infcx.next_region_var(
            infer::BoundRegionInTypeOrImpl(location_info.span)))
    } else {
        opt_vec::Empty
    });
    let tps = vcx.infcx.next_ty_vars(n_tps);

    let substs = substs {regions: regions, self_ty: None, tps: tps};
    let substd_ty = ty::subst(tcx, &substs, raw_ty);

    ty_param_substs_and_ty { substs: substs, ty: substd_ty }
}

// Only for fields! Returns <none> for methods>
// Indifferent to privacy flags
pub fn lookup_field_ty(tcx: ty::ctxt,
                       class_id: ast::DefId,
                       items: &[ty::field_ty],
                       fieldname: ast::Name,
                       substs: &ty::substs) -> Option<ty::t> {

    let o_field = items.iter().find(|f| f.name == fieldname);
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

// Given the provenance of a static method, returns the generics of the static
// method's container.
fn generics_of_static_method_container(type_context: ty::ctxt,
                                       provenance: ast::MethodProvenance)
                                       -> ty::Generics {
    match provenance {
        ast::FromTrait(trait_def_id) => {
            ty::lookup_trait_def(type_context, trait_def_id).generics
        }
        ast::FromImpl(impl_def_id) => {
            ty::lookup_item_type(type_context, impl_def_id).generics
        }
    }
}

// Verifies that type parameters supplied in paths are in the right
// locations.
fn check_type_parameter_positions_in_path(function_context: @mut FnCtxt,
                                          path: &ast::Path,
                                          def: ast::Def) {
    // We only care about checking the case in which the path has two or
    // more segments.
    if path.segments.len() < 2 {
        return
    }

    // Verify that no lifetimes or type parameters are present anywhere
    // except the final two elements of the path.
    for i in range(0, path.segments.len() - 2) {
        match path.segments[i].lifetime {
            None => {}
            Some(lifetime) => {
                function_context.tcx()
                                .sess
                                .span_err(lifetime.span,
                                          "lifetime parameters may not \
                                           appear here")
            }
        }

        for typ in path.segments[i].types.iter() {
            function_context.tcx()
                            .sess
                            .span_err(typ.span,
                                      "type parameters may not appear here")
        }
    }

    // If there are no parameters at all, there is nothing more to do; the
    // rest of typechecking will (attempt to) infer everything.
    if path.segments
           .iter()
           .all(|s| s.lifetime.is_none() && s.types.is_empty()) {
        return
    }

    match def {
        // If this is a static method of a trait or implementation, then
        // ensure that the segment of the path which names the trait or
        // implementation (the penultimate segment) is annotated with the
        // right number of type parameters.
        ast::DefStaticMethod(_, provenance, _) => {
            let generics =
                generics_of_static_method_container(function_context.ccx.tcx,
                                                    provenance);
            let name = match provenance {
                ast::FromTrait(_) => "trait",
                ast::FromImpl(_) => "impl",
            };

            let trait_segment = &path.segments[path.segments.len() - 2];

            // Make sure lifetime parameterization agrees with the trait or
            // implementation type.
            match (generics.region_param, trait_segment.lifetime) {
                (Some(_), None) => {
                    function_context.tcx()
                                    .sess
                                    .span_err(path.span,
                                              fmt!("this %s has a lifetime \
                                                    parameter but no \
                                                    lifetime was specified",
                                                   name))
                }
                (None, Some(_)) => {
                    function_context.tcx()
                                    .sess
                                    .span_err(path.span,
                                              fmt!("this %s has no lifetime \
                                                    parameter but a lifetime \
                                                    was specified",
                                                   name))
                }
                (Some(_), Some(_)) | (None, None) => {}
            }

            // Make sure the number of type parameters supplied on the trait
            // or implementation segment equals the number of type parameters
            // on the trait or implementation definition.
            let trait_type_parameter_count = generics.type_param_defs.len();
            let supplied_type_parameter_count = trait_segment.types.len();
            if trait_type_parameter_count != supplied_type_parameter_count {
                let trait_count_suffix = if trait_type_parameter_count == 1 {
                    ""
                } else {
                    "s"
                };
                let supplied_count_suffix =
                    if supplied_type_parameter_count == 1 {
                        ""
                    } else {
                        "s"
                    };
                function_context.tcx()
                                .sess
                                .span_err(path.span,
                                          fmt!("the %s referenced by this \
                                                path has %u type \
                                                parameter%s, but %u type \
                                                parameter%s were supplied",
                                               name,
                                               trait_type_parameter_count,
                                               trait_count_suffix,
                                               supplied_type_parameter_count,
                                               supplied_count_suffix))
            }
        }
        _ => {
            // Verify that no lifetimes or type parameters are present on
            // the penultimate segment of the path.
            let segment = &path.segments[path.segments.len() - 2];
            match segment.lifetime {
                None => {}
                Some(lifetime) => {
                    function_context.tcx()
                                    .sess
                                    .span_err(lifetime.span,
                                              "lifetime parameters may not
                                               appear here")
                }
            }
            for typ in segment.types.iter() {
                function_context.tcx()
                                .sess
                                .span_err(typ.span,
                                          "type parameters may not appear \
                                           here");
                function_context.tcx()
                                .sess
                                .span_note(typ.span,
                                           fmt!("this is a %?", def));
            }
        }
    }
}

/// Invariant:
/// If an expression has any sub-expressions that result in a type error,
/// inspecting that expression's type with `ty::type_is_error` will return
/// true. Likewise, if an expression is known to diverge, inspecting its
/// type with `ty::type_is_bot` will return true (n.b.: since Rust is
/// strict, _|_ can appear in the type of an expression that does not,
/// itself, diverge: for example, fn() -> _|_.)
/// Note that inspecting a type's structure *directly* may expose the fact
/// that there are actually multiple representations for both `ty_err` and
/// `ty_bot`, so avoid that when err and bot need to be handled differently.
pub fn check_expr_with_unifier(fcx: @mut FnCtxt,
                               expr: @ast::Expr,
                               expected: Option<ty::t>,
                               unifier: &fn()) {
    debug!(">> typechecking");

    fn check_method_argument_types(
        fcx: @mut FnCtxt,
        sp: Span,
        method_fn_ty: ty::t,
        callee_expr: @ast::Expr,
        args: &[@ast::Expr],
        sugar: ast::CallSugar,
        deref_args: DerefArgs) -> ty::t
    {
        if ty::type_is_error(method_fn_ty) {
            let err_inputs = err_args(args.len());
            check_argument_types(fcx, sp, err_inputs, callee_expr,
                                 args, sugar, deref_args);
            method_fn_ty
        } else {
            match ty::get(method_fn_ty).sty {
                ty::ty_bare_fn(ref fty) => {
                    check_argument_types(fcx, sp, fty.sig.inputs, callee_expr,
                                         args, sugar, deref_args);
                    fty.sig.output
                }
                _ => {
                    fcx.tcx().sess.span_bug(
                        sp,
                        fmt!("Method without bare fn type"));
                }
            }
        }
    }

    fn check_argument_types(
        fcx: @mut FnCtxt,
        sp: Span,
        fn_inputs: &[ty::t],
        callee_expr: @ast::Expr,
        args: &[@ast::Expr],
        sugar: ast::CallSugar,
        deref_args: DerefArgs)
    {
        /*!
         *
         * Generic function that factors out common logic from
         * function calls, method calls and overloaded operators.
         */

        let tcx = fcx.ccx.tcx;

        // Grab the argument types, supplying fresh type variables
        // if the wrong number of arguments were supplied
        let supplied_arg_count = args.len();
        let expected_arg_count = fn_inputs.len();
        let formal_tys = if expected_arg_count == supplied_arg_count {
            fn_inputs.map(|a| *a)
        } else {
            let suffix = match sugar {
                ast::NoSugar => "",
                ast::DoSugar => " (including the closure passed by \
                                 the `do` keyword)",
                ast::ForSugar => " (including the closure passed by \
                                  the `for` keyword)"
            };
            let msg = fmt!("this function takes %u parameter%s but \
                            %u parameter%s supplied%s",
                           expected_arg_count,
                           if expected_arg_count == 1 {""}
                           else {"s"},
                           supplied_arg_count,
                           if supplied_arg_count == 1 {" was"}
                           else {"s were"},
                           suffix);

            tcx.sess.span_err(sp, msg);

            vec::from_elem(supplied_arg_count, ty::mk_err())
        };

        debug!("check_argument_types: formal_tys=%?",
               formal_tys.map(|t| fcx.infcx().ty_to_str(*t)));

        // Check the arguments.
        // We do this in a pretty awful way: first we typecheck any arguments
        // that are not anonymous functions, then we typecheck the anonymous
        // functions. This is so that we have more information about the types
        // of arguments when we typecheck the functions. This isn't really the
        // right way to do this.
        let xs = [false, true];
        for check_blocks in xs.iter() {
            let check_blocks = *check_blocks;
            debug!("check_blocks=%b", check_blocks);

            // More awful hacks: before we check the blocks, try to do
            // an "opportunistic" vtable resolution of any trait
            // bounds on the call.
            if check_blocks {
                vtable::early_resolve_expr(callee_expr, fcx, true);
            }

            for (i, arg) in args.iter().enumerate() {
                let is_block = match arg.node {
                    ast::ExprFnBlock(*) |
                    ast::ExprDoBody(*) => true,
                    _ => false
                };

                if is_block == check_blocks {
                    debug!("checking the argument");
                    let mut formal_ty = formal_tys[i];

                    match deref_args {
                        DoDerefArgs => {
                            match ty::get(formal_ty).sty {
                                ty::ty_rptr(_, mt) => formal_ty = mt.ty,
                                ty::ty_err => (),
                                _ => {
                                    fcx.ccx.tcx.sess.span_bug(arg.span, "no ref");
                                }
                            }
                        }
                        DontDerefArgs => {}
                    }

                    check_expr_coercable_to_type(
                        fcx, *arg, formal_ty);

                }
            }
        }
    }

    fn err_args(len: uint) -> ~[ty::t] {
        vec::from_fn(len, |_| ty::mk_err())
    }

    // A generic function for checking assignment expressions
    fn check_assignment(fcx: @mut FnCtxt,
                        lhs: @ast::Expr,
                        rhs: @ast::Expr,
                        id: ast::NodeId) {
        check_expr(fcx, lhs);
        let lhs_type = fcx.expr_ty(lhs);
        check_expr_has_type(fcx, rhs, lhs_type);
        fcx.write_ty(id, ty::mk_nil());
        // The callee checks for bot / err, we don't need to
    }

    fn write_call(fcx: @mut FnCtxt,
                  call_expr: @ast::Expr,
                  output: ty::t,
                  sugar: ast::CallSugar) {
        let ret_ty = match sugar {
            ast::ForSugar => {
                match ty::get(output).sty {
                    ty::ty_bool => {}
                    _ => fcx.type_error_message(call_expr.span, |actual| {
                            fmt!("expected `for` closure to return `bool`, \
                                  but found `%s`", actual) },
                            output, None)
                }
                ty::mk_nil()
            }
            _ => output
        };
        fcx.write_ty(call_expr.id, ret_ty);
    }

    // A generic function for doing all of the checking for call expressions
    fn check_call(fcx: @mut FnCtxt,
                  callee_id: ast::NodeId,
                  call_expr: @ast::Expr,
                  f: @ast::Expr,
                  args: &[@ast::Expr],
                  sugar: ast::CallSugar) {
        // Index expressions need to be handled separately, to inform them
        // that they appear in call position.
        check_expr(fcx, f);

        // Store the type of `f` as the type of the callee
        let fn_ty = fcx.expr_ty(f);

        // FIXME(#6273) should write callee type AFTER regions have
        // been subst'd.  However, it is awkward to deal with this
        // now. Best thing would I think be to just have a separate
        // "callee table" that contains the FnSig and not a general
        // purpose ty::t
        fcx.write_ty(callee_id, fn_ty);

        // Extract the function signature from `in_fty`.
        let fn_sty = structure_of(fcx, f.span, fn_ty);

        // This is the "default" function signature, used in case of error.
        // In that case, we check each argument against "error" in order to
        // set up all the node type bindings.
        let error_fn_sig = FnSig {
            bound_lifetime_names: opt_vec::Empty,
            inputs: err_args(args.len()),
            output: ty::mk_err()
        };

        let fn_sig = match *fn_sty {
            ty::ty_bare_fn(ty::BareFnTy {sig: ref sig, _}) |
            ty::ty_closure(ty::ClosureTy {sig: ref sig, _}) => sig,
            _ => {
                fcx.type_error_message(call_expr.span, |actual| {
                    fmt!("expected function but \
                          found `%s`", actual) }, fn_ty, None);
                &error_fn_sig
            }
        };

        // Replace any bound regions that appear in the function
        // signature with region variables
        let (_, _, fn_sig) =
            replace_bound_regions_in_fn_sig(fcx.tcx(),
                                            @Nil,
                                            None,
                                            fn_sig,
                                            |br| fcx.infcx()
                                                    .next_region_var(
                    infer::BoundRegionInFnCall(call_expr.span, br)));

        // Call the generic checker.
        check_argument_types(fcx, call_expr.span, fn_sig.inputs, f,
                             args, sugar, DontDerefArgs);

        write_call(fcx, call_expr, fn_sig.output, sugar);
    }

    // Checks a method call.
    fn check_method_call(fcx: @mut FnCtxt,
                         callee_id: ast::NodeId,
                         expr: @ast::Expr,
                         rcvr: @ast::Expr,
                         method_name: ast::Ident,
                         args: &[@ast::Expr],
                         tps: &[ast::Ty],
                         sugar: ast::CallSugar) {
        check_expr(fcx, rcvr);

        // no need to check for bot/err -- callee does that
        let expr_t = structurally_resolved_type(fcx,
                                                expr.span,
                                                fcx.expr_ty(rcvr));

        let tps = tps.map(|ast_ty| fcx.to_ty(ast_ty));
        match method::lookup(fcx,
                             expr,
                             rcvr,
                             callee_id,
                             method_name.name,
                             expr_t,
                             tps,
                             DontDerefArgs,
                             CheckTraitsAndInherentMethods,
                             AutoderefReceiver) {
            Some(ref entry) => {
                let method_map = fcx.inh.method_map;
                method_map.insert(expr.id, (*entry));
            }
            None => {
                debug!("(checking method call) failing expr is %d", expr.id);

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
                fcx.write_error(expr.id);
                fcx.write_error(callee_id);
            }
        }

        // Call the generic checker.
        let fn_ty = fcx.node_ty(callee_id);
        let ret_ty = check_method_argument_types(fcx, expr.span,
                                                 fn_ty, expr, args, sugar,
                                                 DontDerefArgs);

        write_call(fcx, expr, ret_ty, sugar);
    }

    // A generic function for checking the then and else in an if
    // or if-check
    fn check_then_else(fcx: @mut FnCtxt,
                       cond_expr: @ast::Expr,
                       then_blk: &ast::Block,
                       opt_else_expr: Option<@ast::Expr>,
                       id: ast::NodeId,
                       sp: Span,
                       expected: Option<ty::t>) {
        check_expr_has_type(fcx, cond_expr, ty::mk_bool());

        let branches_ty = match opt_else_expr {
            Some(else_expr) => {
                check_block_with_expected(fcx, then_blk, expected);
                let then_ty = fcx.node_ty(then_blk.id);
                check_expr_with_opt_hint(fcx, else_expr, expected);
                let else_ty = fcx.expr_ty(else_expr);
                infer::common_supertype(fcx.infcx(),
                                        infer::IfExpression(sp),
                                        true,
                                        then_ty,
                                        else_ty)
            }
            None => {
                check_block_no_value(fcx, then_blk);
                ty::mk_nil()
            }
        };

        let cond_ty = fcx.expr_ty(cond_expr);
        let if_ty = if ty::type_is_error(cond_ty) {
            ty::mk_err()
        } else if ty::type_is_bot(cond_ty) {
            ty::mk_bot()
        } else {
            branches_ty
        };

        fcx.write_ty(id, if_ty);
    }

    fn lookup_op_method(fcx: @mut FnCtxt,
                        callee_id: ast::NodeId,
                        op_ex: @ast::Expr,
                        self_ex: @ast::Expr,
                        self_t: ty::t,
                        opname: ast::Name,
                        args: ~[@ast::Expr],
                        deref_args: DerefArgs,
                        autoderef_receiver: AutoderefReceiverFlag,
                        unbound_method: &fn(),
                        _expected_result: Option<ty::t>
                       )
                     -> ty::t {
        match method::lookup(fcx, op_ex, self_ex,
                             callee_id, opname, self_t, [],
                             deref_args, CheckTraitsOnly, autoderef_receiver) {
            Some(ref origin) => {
                let method_ty = fcx.node_ty(callee_id);
                let method_map = fcx.inh.method_map;
                method_map.insert(op_ex.id, *origin);
                check_method_argument_types(fcx, op_ex.span,
                                            method_ty, op_ex, args,
                                            ast::NoSugar, deref_args)
            }
            _ => {
                unbound_method();
                // Check the args anyway
                // so we get all the error messages
                let expected_ty = ty::mk_err();
                check_method_argument_types(fcx, op_ex.span,
                                            expected_ty, op_ex, args,
                                            ast::NoSugar, deref_args);
                ty::mk_err()
            }
        }
    }

    // could be either a expr_binop or an expr_assign_binop
    fn check_binop(fcx: @mut FnCtxt,
                   callee_id: ast::NodeId,
                   expr: @ast::Expr,
                   op: ast::BinOp,
                   lhs: @ast::Expr,
                   rhs: @ast::Expr,
                   // Used only in the error case
                   expected_result: Option<ty::t>,
                   allow_overloaded_operators: AllowOverloadedOperatorsFlag
                  ) {
        let tcx = fcx.ccx.tcx;

        check_expr(fcx, lhs);
        // Callee does bot / err checking
        let lhs_t = structurally_resolved_type(fcx, lhs.span,
                                               fcx.expr_ty(lhs));

        if ty::type_is_integral(lhs_t) && ast_util::is_shift_binop(op) {
            // Shift is a special case: rhs can be any integral type
            check_expr(fcx, rhs);
            let rhs_t = fcx.expr_ty(rhs);
            require_integral(fcx, rhs.span, rhs_t);
            fcx.write_ty(expr.id, lhs_t);
            return;
        }

        if ty::is_binopable(tcx, lhs_t, op) {
            let tvar = fcx.infcx().next_ty_var();
            demand::suptype(fcx, expr.span, tvar, lhs_t);
            check_expr_has_type(fcx, rhs, tvar);

            let result_t = match op {
                ast::BiEq | ast::BiNe | ast::BiLt | ast::BiLe | ast::BiGe |
                ast::BiGt => {
                    ty::mk_bool()
                }
                _ => {
                    lhs_t
                }
            };

            fcx.write_ty(expr.id, result_t);
            return;
        }

        if op == ast::BiOr || op == ast::BiAnd {
            // This is an error; one of the operands must have the wrong
            // type
            fcx.write_error(expr.id);
            fcx.write_error(rhs.id);
            fcx.type_error_message(expr.span, |actual| {
                fmt!("binary operation %s cannot be applied \
                      to type `%s`",
                     ast_util::binop_to_str(op), actual)},
                                   lhs_t, None)

        }

        // Check for overloaded operators if allowed.
        let result_t;
        if allow_overloaded_operators == AllowOverloadedOperators {
            result_t = check_user_binop(fcx,
                                        callee_id,
                                        expr,
                                        lhs,
                                        lhs_t,
                                        op,
                                        rhs,
                                        expected_result);
        } else {
            fcx.type_error_message(expr.span,
                                   |actual| {
                                        fmt!("binary operation %s cannot be \
                                              applied to type `%s`",
                                             ast_util::binop_to_str(op),
                                             actual)
                                   },
                                   lhs_t,
                                   None);
            result_t = ty::mk_err();
        }

        fcx.write_ty(expr.id, result_t);
        if ty::type_is_error(result_t) {
            fcx.write_ty(rhs.id, result_t);
        }
    }

    fn check_user_binop(fcx: @mut FnCtxt,
                        callee_id: ast::NodeId,
                        ex: @ast::Expr,
                        lhs_expr: @ast::Expr,
                        lhs_resolved_t: ty::t,
                        op: ast::BinOp,
                        rhs: @ast::Expr,
                       expected_result: Option<ty::t>) -> ty::t {
        let tcx = fcx.ccx.tcx;
        match ast_util::binop_to_method_name(op) {
            Some(ref name) => {
                let if_op_unbound = || {
                    fcx.type_error_message(ex.span, |actual| {
                        fmt!("binary operation %s cannot be applied \
                              to type `%s`",
                             ast_util::binop_to_str(op), actual)},
                            lhs_resolved_t, None)
                };
                return lookup_op_method(fcx, callee_id, ex, lhs_expr, lhs_resolved_t,
                                       token::intern(*name),
                                       ~[rhs], DoDerefArgs, DontAutoderefReceiver, if_op_unbound,
                                       expected_result);
            }
            None => ()
        };
        check_expr(fcx, rhs);

        // If the or operator is used it might be that the user forgot to
        // supply the do keyword.  Let's be more helpful in that situation.
        if op == ast::BiOr {
            match ty::get(lhs_resolved_t).sty {
                ty::ty_bare_fn(_) | ty::ty_closure(_) => {
                    tcx.sess.span_note(
                        ex.span, "did you forget the `do` keyword for the call?");
                }
                _ => ()
            }
        }

        ty::mk_err()
    }

    fn check_user_unop(fcx: @mut FnCtxt,
                       callee_id: ast::NodeId,
                       op_str: &str,
                       mname: &str,
                       ex: @ast::Expr,
                       rhs_expr: @ast::Expr,
                       rhs_t: ty::t,
                       expected_t: Option<ty::t>)
                    -> ty::t {
       lookup_op_method(
            fcx, callee_id, ex, rhs_expr, rhs_t,
            token::intern(mname), ~[],
            DoDerefArgs, DontAutoderefReceiver,
            || {
                fcx.type_error_message(ex.span, |actual| {
                    fmt!("cannot apply unary operator `%s` to type `%s`",
                         op_str, actual)
                }, rhs_t, None);
            }, expected_t)
    }

    // Resolves `expected` by a single level if it is a variable and passes it
    // through the `unpack` function.  It there is no expected type or
    // resolution is not possible (e.g., no constraints yet present), just
    // returns `none`.
    fn unpack_expected<O>(fcx: @mut FnCtxt,
                          expected: Option<ty::t>,
                          unpack: &fn(&ty::sty) -> Option<O>)
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

    fn check_expr_fn(fcx: @mut FnCtxt,
                     expr: @ast::Expr,
                     ast_sigil_opt: Option<ast::Sigil>,
                     decl: &ast::fn_decl,
                     body: &ast::Block,
                     fn_kind: FnKind,
                     expected: Option<ty::t>) {
        let tcx = fcx.ccx.tcx;

        // Find the expected input/output types (if any).  Careful to
        // avoid capture of bound regions in the expected type.  See
        // def'n of br_cap_avoid() for a more lengthy explanation of
        // what's going on here.
        // Also try to pick up inferred purity and sigil, defaulting
        // to impure and block. Note that we only will use those for
        // block syntax lambdas; that is, lambdas without explicit
        // sigils.
        let expected_sty = unpack_expected(fcx,
                                           expected,
                                           |x| Some((*x).clone()));
        let error_happened = false;
        let (expected_sig,
             expected_purity,
             expected_sigil,
             expected_onceness,
             expected_bounds) = {
            match expected_sty {
                Some(ty::ty_closure(ref cenv)) => {
                    let id = expr.id;
                    let (_, _, sig) =
                        replace_bound_regions_in_fn_sig(
                            tcx, @Nil, None, &cenv.sig,
                            |br| ty::re_bound(ty::br_cap_avoid(id, @br)));
                    (Some(sig), cenv.purity, cenv.sigil,
                     cenv.onceness, cenv.bounds)
                }
                _ => {
                    // Not an error! Means we're inferring the closure type
                    (None, ast::impure_fn, ast::BorrowedSigil,
                     ast::Many, ty::EmptyBuiltinBounds())
                }
            }
        };

        // If the proto is specified, use that, otherwise select a
        // proto based on inference.
        let (sigil, purity) = match ast_sigil_opt {
            Some(p) => (p, ast::impure_fn),
            None => (expected_sigil, expected_purity)
        };

        // construct the function type
        let fn_ty = astconv::ty_of_closure(fcx,
                                           fcx,
                                           sigil,
                                           purity,
                                           expected_onceness,
                                           expected_bounds,
                                           &None,
                                           decl,
                                           expected_sig,
                                           &opt_vec::Empty,
                                           expr.span);

        let fty_sig;
        let fty = if error_happened {
            fty_sig = FnSig {
                bound_lifetime_names: opt_vec::Empty,
                inputs: fn_ty.sig.inputs.map(|_| ty::mk_err()),
                output: ty::mk_err()
            };
            ty::mk_err()
        } else {
            let fn_ty_copy = fn_ty.clone();
            fty_sig = fn_ty.sig.clone();
            ty::mk_closure(tcx, fn_ty_copy)
        };

        debug!("check_expr_fn_with_unifier fty=%s",
               fcx.infcx().ty_to_str(fty));

        fcx.write_ty(expr.id, fty);

        let (inherited_purity, id) =
            ty::determine_inherited_purity((fcx.ps.purity, fcx.ps.def),
                                           (purity, expr.id),
                                           sigil);

        check_fn(fcx.ccx, None, inherited_purity, &fty_sig,
                 decl, id, body, fn_kind, fcx.in_scope_regions, fcx.inh);
    }


    // Check field access expressions
    fn check_field(fcx: @mut FnCtxt,
                   expr: @ast::Expr,
                   base: @ast::Expr,
                   field: ast::Name,
                   tys: &[ast::Ty]) {
        let tcx = fcx.ccx.tcx;
        let bot = check_expr(fcx, base);
        let expr_t = structurally_resolved_type(fcx, expr.span,
                                                fcx.expr_ty(base));
        let (base_t, derefs) = do_autoderef(fcx, expr.span, expr_t);

        match *structure_of(fcx, expr.span, base_t) {
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

        let tps : ~[ty::t] = tys.iter().map(|ty| fcx.to_ty(ty)).collect();
        match method::lookup(fcx,
                             expr,
                             base,
                             expr.id,
                             field,
                             expr_t,
                             tps,
                             DontDerefArgs,
                             CheckTraitsAndInherentMethods,
                             AutoderefReceiver) {
            Some(_) => {
                fcx.type_error_message(
                    expr.span,
                    |actual| {
                        fmt!("attempted to take value of method `%s` on type `%s` \
                              (try writing an anonymous function)",
                             token::interner_get(field), actual)
                    },
                    expr_t, None);
            }

            None => {
                fcx.type_error_message(
                    expr.span,
                    |actual| {
                        fmt!("attempted access of field `%s` on type `%s`, \
                              but no field with that name was found",
                             token::interner_get(field), actual)
                    },
                    expr_t, None);
            }
        }

        fcx.write_error(expr.id);
    }

    fn check_struct_or_variant_fields(fcx: @mut FnCtxt,
                                      span: Span,
                                      class_id: ast::DefId,
                                      node_id: ast::NodeId,
                                      substitutions: ty::substs,
                                      field_types: &[ty::field_ty],
                                      ast_fields: &[ast::Field],
                                      check_completeness: bool)  {
        let tcx = fcx.ccx.tcx;

        let mut class_field_map = HashMap::new();
        let mut fields_found = 0;
        for field in field_types.iter() {
            class_field_map.insert(field.name, (field.id, false));
        }

        let mut error_happened = false;

        // Typecheck each field.
        for field in ast_fields.iter() {
            let mut expected_field_type = ty::mk_err();

            let pair = class_field_map.find(&field.ident.name).map_move(|x| *x);
            match pair {
                None => {
                    tcx.sess.span_err(
                        field.span,
                        fmt!("structure has no field named `%s`",
                             tcx.sess.str_of(field.ident)));
                    error_happened = true;
                }
                Some((_, true)) => {
                    tcx.sess.span_err(
                        field.span,
                        fmt!("field `%s` specified more than once",
                             tcx.sess.str_of(field.ident)));
                    error_happened = true;
                }
                Some((field_id, false)) => {
                    expected_field_type =
                        ty::lookup_field_type(
                            tcx, class_id, field_id, &substitutions);
                    class_field_map.insert(
                        field.ident.name, (field_id, true));
                    fields_found += 1;
                }
            }
            // Make sure to give a type to the field even if there's
            // an error, so we can continue typechecking
            check_expr_coercable_to_type(
                    fcx,
                    field.expr,
                    expected_field_type);
        }

        if error_happened {
            fcx.write_error(node_id);
        }

        if check_completeness && !error_happened {
            // Make sure the programmer specified all the fields.
            assert!(fields_found <= field_types.len());
            if fields_found < field_types.len() {
                let mut missing_fields = ~[];
                for class_field in field_types.iter() {
                    let name = class_field.name;
                    let (_, seen) = *class_field_map.get(&name);
                    if !seen {
                        missing_fields.push(
                            ~"`" + token::interner_get(name) + "`");
                    }
                }

                tcx.sess.span_err(span,
                                  fmt!("missing field%s: %s",
                                       if missing_fields.len() == 1 {
                                           ""
                                       } else {
                                           "s"
                                       },
                                       missing_fields.connect(", ")));
             }
        }

        if !error_happened {
            fcx.write_ty(node_id, ty::mk_struct(fcx.ccx.tcx,
                                class_id, substitutions));
        }
    }

    fn check_struct_constructor(fcx: @mut FnCtxt,
                                id: ast::NodeId,
                                span: codemap::Span,
                                class_id: ast::DefId,
                                fields: &[ast::Field],
                                base_expr: Option<@ast::Expr>) {
        let tcx = fcx.ccx.tcx;

        // Look up the number of type parameters and the raw type, and
        // determine whether the class is region-parameterized.
        let type_parameter_count;
        let region_parameterized;
        let raw_type;
        if class_id.crate == ast::LOCAL_CRATE {
            region_parameterized =
                tcx.region_paramd_items.find(&class_id.node).
                    map_move(|x| *x);
            match tcx.items.find(&class_id.node) {
                Some(&ast_map::node_item(@ast::item {
                        node: ast::item_struct(_, ref generics),
                        _
                    }, _)) => {

                    type_parameter_count = generics.ty_params.len();

                    let self_region =
                        bound_self_region(region_parameterized);

                    raw_type = ty::mk_struct(tcx, class_id, substs {
                        regions: ty::NonerasedRegions(self_region),
                        self_ty: None,
                        tps: ty::ty_params_to_tys(
                            tcx,
                            generics)
                    });
                }
                _ => {
                    tcx.sess.span_bug(span,
                                      "resolve didn't map this to a class");
                }
            }
        } else {
            let item_type = ty::lookup_item_type(tcx, class_id);
            type_parameter_count = item_type.generics.type_param_defs.len();
            region_parameterized = item_type.generics.region_param;
            raw_type = item_type.ty;
        }

        // Generate the struct type.
        let regions =
            fcx.region_var_if_parameterized(region_parameterized, span);
        let type_parameters = fcx.infcx().next_ty_vars(type_parameter_count);
        let substitutions = substs {
            regions: ty::NonerasedRegions(regions),
            self_ty: None,
            tps: type_parameters
        };

        let mut struct_type = ty::subst(tcx, &substitutions, raw_type);

        // Look up and check the fields.
        let class_fields = ty::lookup_struct_fields(tcx, class_id);
        check_struct_or_variant_fields(fcx,
                                           span,
                                           class_id,
                                           id,
                                           substitutions,
                                           class_fields,
                                           fields,
                                           base_expr.is_none());
        if ty::type_is_error(fcx.node_ty(id)) {
            struct_type = ty::mk_err();
        }

        // Check the base expression if necessary.
        match base_expr {
            None => {}
            Some(base_expr) => {
                check_expr_has_type(fcx, base_expr, struct_type);
                if ty::type_is_bot(fcx.node_ty(base_expr.id)) {
                    struct_type = ty::mk_bot();
                }
            }
        }

        // Write in the resulting type.
        fcx.write_ty(id, struct_type);
    }

    fn check_struct_enum_variant(fcx: @mut FnCtxt,
                                 id: ast::NodeId,
                                 span: codemap::Span,
                                 enum_id: ast::DefId,
                                 variant_id: ast::DefId,
                                 fields: &[ast::Field]) {
        let tcx = fcx.ccx.tcx;

        // Look up the number of type parameters and the raw type, and
        // determine whether the enum is region-parameterized.
        let type_parameter_count;
        let region_parameterized;
        let raw_type;
        if enum_id.crate == ast::LOCAL_CRATE {
            region_parameterized =
                tcx.region_paramd_items.find(&enum_id.node).map_move(|x| *x);
            match tcx.items.find(&enum_id.node) {
                Some(&ast_map::node_item(@ast::item {
                        node: ast::item_enum(_, ref generics),
                        _
                    }, _)) => {

                    type_parameter_count = generics.ty_params.len();

                    let regions = bound_self_region(region_parameterized);

                    raw_type = ty::mk_enum(tcx, enum_id, substs {
                        regions: ty::NonerasedRegions(regions),
                        self_ty: None,
                        tps: ty::ty_params_to_tys(
                            tcx,
                            generics)
                    });
                }
                _ => {
                    tcx.sess.span_bug(span,
                                      "resolve didn't map this to an enum");
                }
            }
        } else {
            let item_type = ty::lookup_item_type(tcx, enum_id);
            type_parameter_count = item_type.generics.type_param_defs.len();
            region_parameterized = item_type.generics.region_param;
            raw_type = item_type.ty;
        }

        // Generate the enum type.
        let regions =
            fcx.region_var_if_parameterized(region_parameterized, span);
        let type_parameters = fcx.infcx().next_ty_vars(type_parameter_count);
        let substitutions = substs {
            regions: ty::NonerasedRegions(regions),
            self_ty: None,
            tps: type_parameters
        };

        let enum_type = ty::subst(tcx, &substitutions, raw_type);

        // Look up and check the enum variant fields.
        let variant_fields = ty::lookup_struct_fields(tcx, variant_id);
        check_struct_or_variant_fields(fcx,
                                       span,
                                       variant_id,
                                       id,
                                       substitutions,
                                       variant_fields,
                                       fields,
                                       true);
        fcx.write_ty(id, enum_type);
    }

    let tcx = fcx.ccx.tcx;
    let id = expr.id;
    match expr.node {
      ast::ExprVstore(ev, vst) => {
        let typ = match ev.node {
          ast::ExprLit(@codemap::Spanned { node: ast::lit_str(_), _ }) => {
            let tt = ast_expr_vstore_to_vstore(fcx, ev, vst);
            ty::mk_estr(tcx, tt)
          }
          ast::ExprVec(ref args, mutbl) => {
            let tt = ast_expr_vstore_to_vstore(fcx, ev, vst);
            let mutability;
            let mut any_error = false;
            let mut any_bot = false;
            match vst {
                ast::ExprVstoreMutBox | ast::ExprVstoreMutSlice => {
                    mutability = ast::MutMutable
                }
                _ => mutability = mutbl
            }
            let t: ty::t = fcx.infcx().next_ty_var();
            for e in args.iter() {
                check_expr_has_type(fcx, *e, t);
                let arg_t = fcx.expr_ty(*e);
                if ty::type_is_error(arg_t) {
                    any_error = true;
                }
                else if ty::type_is_bot(arg_t) {
                    any_bot = true;
                }
            }
            if any_error {
                ty::mk_err()
            }
            else if any_bot {
                ty::mk_bot()
            }
            else {
                ty::mk_evec(tcx, ty::mt {ty: t, mutbl: mutability}, tt)
            }
          }
          ast::ExprRepeat(element, count_expr, mutbl) => {
            check_expr_with_hint(fcx, count_expr, ty::mk_uint());
            let _ = ty::eval_repeat_count(fcx, count_expr);
            let tt = ast_expr_vstore_to_vstore(fcx, ev, vst);
            let mutability = match vst {
                ast::ExprVstoreMutBox | ast::ExprVstoreMutSlice => {
                    ast::MutMutable
                }
                _ => mutbl
            };
            let t: ty::t = fcx.infcx().next_ty_var();
            check_expr_has_type(fcx, element, t);
            let arg_t = fcx.expr_ty(element);
            if ty::type_is_error(arg_t) {
                ty::mk_err()
            } else if ty::type_is_bot(arg_t) {
                ty::mk_bot()
            } else {
                ty::mk_evec(tcx, ty::mt {ty: t, mutbl: mutability}, tt)
            }
          }
          _ =>
            tcx.sess.span_bug(expr.span, "vstore modifier on non-sequence")
        };
        fcx.write_ty(ev.id, typ);
        fcx.write_ty(id, typ);
      }

      ast::ExprLit(lit) => {
        let typ = check_lit(fcx, lit);
        fcx.write_ty(id, typ);
      }
      ast::ExprBinary(callee_id, op, lhs, rhs) => {
        check_binop(fcx,
                    callee_id,
                    expr,
                    op,
                    lhs,
                    rhs,
                    expected,
                    AllowOverloadedOperators);

        let lhs_ty = fcx.expr_ty(lhs);
        let rhs_ty = fcx.expr_ty(rhs);
        if ty::type_is_error(lhs_ty) ||
            ty::type_is_error(rhs_ty) {
            fcx.write_error(id);
        }
        else if ty::type_is_bot(lhs_ty) ||
          (ty::type_is_bot(rhs_ty) && !ast_util::lazy_binop(op)) {
            fcx.write_bot(id);
        }
      }
      ast::ExprAssignOp(callee_id, op, lhs, rhs) => {
        check_binop(fcx,
                    callee_id,
                    expr,
                    op,
                    lhs,
                    rhs,
                    expected,
                    DontAllowOverloadedOperators);

        let lhs_t = fcx.expr_ty(lhs);
        let result_t = fcx.expr_ty(expr);
        demand::suptype(fcx, expr.span, result_t, lhs_t);

        // Overwrite result of check_binop...this preserves existing behavior
        // but seems quite dubious with regard to user-defined methods
        // and so forth. - Niko
        if !ty::type_is_error(result_t)
            && !ty::type_is_bot(result_t) {
            fcx.write_nil(expr.id);
        }
      }
      ast::ExprUnary(callee_id, unop, oprnd) => {
        let exp_inner = do unpack_expected(fcx, expected) |sty| {
            match unop {
              ast::UnBox(_) | ast::UnUniq => match *sty {
                ty::ty_box(ref mt) | ty::ty_uniq(ref mt) => Some(mt.ty),
                _ => None
              },
              ast::UnNot | ast::UnNeg => expected,
              ast::UnDeref => None
            }
        };
        check_expr_with_opt_hint(fcx, oprnd, exp_inner);
        let mut oprnd_t = fcx.expr_ty(oprnd);
        if !ty::type_is_error(oprnd_t) &&
              !ty::type_is_bot(oprnd_t) {
            match unop {
                ast::UnBox(mutbl) => {
                    oprnd_t = ty::mk_box(tcx,
                                         ty::mt {ty: oprnd_t, mutbl: mutbl});
                }
                ast::UnUniq => {
                    oprnd_t = ty::mk_uniq(tcx,
                                          ty::mt {ty: oprnd_t,
                                                  mutbl: ast::MutImmutable});
                }
                ast::UnDeref => {
                    let sty = structure_of(fcx, expr.span, oprnd_t);
                    let operand_ty = ty::deref_sty(tcx, sty, true);
                    match operand_ty {
                        Some(mt) => {
                            oprnd_t = mt.ty
                        }
                        None => {
                            match *sty {
                                ty::ty_enum(*) => {
                                    tcx.sess.span_err(
                                        expr.span,
                                        "can only dereference enums with a single variant which \
                                         has a single argument");
                                }
                                ty::ty_struct(*) => {
                                    tcx.sess.span_err(
                                        expr.span,
                                        "can only dereference structs with one anonymous field");
                                }
                                _ => {
                                    fcx.type_error_message(expr.span,
                                        |actual| {
                                            fmt!("type %s cannot be dereferenced", actual)
                                    }, oprnd_t, None);
                                }
                            }
                        }
                    }
                }
                ast::UnNot => {
                    oprnd_t = structurally_resolved_type(fcx, oprnd.span,
                                                         oprnd_t);
                    if !(ty::type_is_integral(oprnd_t) ||
                         ty::get(oprnd_t).sty == ty::ty_bool) {
                        oprnd_t = check_user_unop(fcx, callee_id,
                            "!", "not", expr, oprnd, oprnd_t,
                                                  expected);
                    }
                }
                ast::UnNeg => {
                    oprnd_t = structurally_resolved_type(fcx, oprnd.span,
                                                         oprnd_t);
                    if !(ty::type_is_integral(oprnd_t) ||
                         ty::type_is_fp(oprnd_t)) {
                        oprnd_t = check_user_unop(fcx, callee_id,
                            "-", "neg", expr, oprnd, oprnd_t, expected);
                    }
                }
            }
        }
        fcx.write_ty(id, oprnd_t);
      }
      ast::ExprAddrOf(mutbl, oprnd) => {
          let hint = unpack_expected(
              fcx, expected,
              |sty| match *sty { ty::ty_rptr(_, ref mt) => Some(mt.ty),
                                 _ => None });
        check_expr_with_opt_hint(fcx, oprnd, hint);

        // Note: at this point, we cannot say what the best lifetime
        // is to use for resulting pointer.  We want to use the
        // shortest lifetime possible so as to avoid spurious borrowck
        // errors.  Moreover, the longest lifetime will depend on the
        // precise details of the value whose address is being taken
        // (and how long it is valid), which we don't know yet until type
        // inference is complete.
        //
        // Therefore, here we simply generate a region variable.  The
        // region inferencer will then select the ultimate value.
        // Finally, borrowck is charged with guaranteeing that the
        // value whose address was taken can actually be made to live
        // as long as it needs to live.
        let region = fcx.infcx().next_region_var(
            infer::AddrOfRegion(expr.span));

        let tm = ty::mt { ty: fcx.expr_ty(oprnd), mutbl: mutbl };
        let oprnd_t = if ty::type_is_error(tm.ty) {
            ty::mk_err()
        } else if ty::type_is_bot(tm.ty) {
            ty::mk_bot()
        }
        else {
            ty::mk_rptr(tcx, region, tm)
        };
        fcx.write_ty(id, oprnd_t);
      }
      ast::ExprPath(ref pth) => {
        let defn = lookup_def(fcx, pth.span, id);

        check_type_parameter_positions_in_path(fcx, pth, defn);
        let tpt = ty_param_bounds_and_ty_for_def(fcx, expr.span, defn);
        instantiate_path(fcx, pth, tpt, defn, expr.span, expr.id);
      }
      ast::ExprSelf => {
        let definition = lookup_def(fcx, expr.span, id);
        let ty_param_bounds_and_ty =
            ty_param_bounds_and_ty_for_def(fcx, expr.span, definition);
        fcx.write_ty(id, ty_param_bounds_and_ty.ty);
      }
      ast::ExprInlineAsm(ref ia) => {
          for &(_, input) in ia.inputs.iter() {
              check_expr(fcx, input);
          }
          for &(_, out) in ia.outputs.iter() {
              check_expr(fcx, out);
          }
          fcx.write_nil(id);
      }
      ast::ExprMac(_) => tcx.sess.bug("unexpanded macro"),
      ast::ExprBreak(_) => { fcx.write_bot(id); }
      ast::ExprAgain(_) => { fcx.write_bot(id); }
      ast::ExprRet(expr_opt) => {
        let ret_ty = fcx.ret_ty;
        match expr_opt {
          None => match fcx.mk_eqty(false, infer::Misc(expr.span),
                                    ret_ty, ty::mk_nil()) {
            result::Ok(_) => { /* fall through */ }
            result::Err(_) => {
                tcx.sess.span_err(
                    expr.span,
                    "`return;` in function returning non-nil");
            }
          },
          Some(e) => {
              check_expr_has_type(fcx, e, ret_ty);
          }
        }
        fcx.write_bot(id);
      }
      ast::ExprLogLevel => {
        fcx.write_ty(id, ty::mk_u32())
      }
      ast::ExprParen(a) => {
        check_expr_with_opt_hint(fcx, a, expected);
        fcx.write_ty(id, fcx.expr_ty(a));
      }
      ast::ExprAssign(lhs, rhs) => {
        check_assignment(fcx, lhs, rhs, id);
        let lhs_ty = fcx.expr_ty(lhs);
        let rhs_ty = fcx.expr_ty(rhs);
        if ty::type_is_error(lhs_ty) || ty::type_is_error(rhs_ty) {
            fcx.write_error(id);
        }
        else if ty::type_is_bot(lhs_ty) || ty::type_is_bot(rhs_ty) {
            fcx.write_bot(id);
        }
        else {
            fcx.write_nil(id);
        }
      }
      ast::ExprIf(cond, ref then_blk, opt_else_expr) => {
        check_then_else(fcx, cond, then_blk, opt_else_expr,
                        id, expr.span, expected);
      }
      ast::ExprWhile(cond, ref body) => {
        check_expr_has_type(fcx, cond, ty::mk_bool());
        check_block_no_value(fcx, body);
        let cond_ty = fcx.expr_ty(cond);
        let body_ty = fcx.node_ty(body.id);
        if ty::type_is_error(cond_ty) || ty::type_is_error(body_ty) {
            fcx.write_error(id);
        }
        else if ty::type_is_bot(cond_ty) {
            fcx.write_bot(id);
        }
        else {
            fcx.write_nil(id);
        }
      }
      ast::ExprForLoop(*) =>
          fail!("non-desugared expr_for_loop"),
      ast::ExprLoop(ref body, _) => {
        check_block_no_value(fcx, (body));
        if !may_break(tcx, expr.id, body) {
            fcx.write_bot(id);
        }
        else {
            fcx.write_nil(id);
        }
      }
      ast::ExprMatch(discrim, ref arms) => {
        _match::check_match(fcx, expr, discrim, *arms);
      }
      ast::ExprFnBlock(ref decl, ref body) => {
        check_expr_fn(fcx, expr, None,
                      decl, body, Vanilla, expected);
      }
      ast::ExprDoBody(b) => {
        let expected_sty = unpack_expected(fcx,
                                           expected,
                                           |x| Some((*x).clone()));
        let inner_ty = match expected_sty {
            Some(ty::ty_closure(_)) => expected.unwrap(),
            _ => match expected {
                Some(expected_t) => {
                    fcx.type_error_message(expr.span, |actual| {
                        fmt!("last argument in `do` call \
                              has non-closure type: %s",
                             actual)
                    }, expected_t, None);
                    let err_ty = ty::mk_err();
                    fcx.write_ty(id, err_ty);
                    err_ty
                }
                None => {
                    fcx.tcx().sess.impossible_case(
                        expr.span,
                        "do body must have expected type")
                }
            }
        };
        match b.node {
          ast::ExprFnBlock(ref decl, ref body) => {
            check_expr_fn(fcx, b, None,
                          decl, body, DoBlock, Some(inner_ty));
            demand::suptype(fcx, b.span, inner_ty, fcx.expr_ty(b));
          }
          // argh
          _ => fail!("expected fn ty")
        }
        fcx.write_ty(expr.id, fcx.node_ty(b.id));
      }
      ast::ExprBlock(ref b) => {
        check_block_with_expected(fcx, b, expected);
        fcx.write_ty(id, fcx.node_ty(b.id));
      }
      ast::ExprCall(f, ref args, sugar) => {
          check_call(fcx, expr.id, expr, f, *args, sugar);
          let f_ty = fcx.expr_ty(f);
          let (args_bot, args_err) = args.iter().fold((false, false),
             |(rest_bot, rest_err), a| {
                 // is this not working?
                 let a_ty = fcx.expr_ty(*a);
                 (rest_bot || ty::type_is_bot(a_ty),
                  rest_err || ty::type_is_error(a_ty))});
          if ty::type_is_error(f_ty) || args_err {
              fcx.write_error(id);
          }
          else if ty::type_is_bot(f_ty) || args_bot {
              fcx.write_bot(id);
          }
      }
      ast::ExprMethodCall(callee_id, rcvr, ident, ref tps, ref args, sugar) => {
        check_method_call(fcx, callee_id, expr, rcvr, ident, *args, *tps, sugar);
        let f_ty = fcx.expr_ty(rcvr);
        let arg_tys = args.map(|a| fcx.expr_ty(*a));
        let (args_bot, args_err) = arg_tys.iter().fold((false, false),
             |(rest_bot, rest_err), a| {
              (rest_bot || ty::type_is_bot(*a),
               rest_err || ty::type_is_error(*a))});
        if ty::type_is_error(f_ty) || args_err {
            fcx.write_error(id);
        }
        else if ty::type_is_bot(f_ty) || args_bot {
            fcx.write_bot(id);
        }
      }
      ast::ExprCast(e, ref t) => {
        check_expr(fcx, e);
        let t_1 = fcx.to_ty(t);
        let t_e = fcx.expr_ty(e);

        debug!("t_1=%s", fcx.infcx().ty_to_str(t_1));
        debug!("t_e=%s", fcx.infcx().ty_to_str(t_e));

        if ty::type_is_error(t_e) {
            fcx.write_error(id);
        }
        else if ty::type_is_bot(t_e) {
            fcx.write_bot(id);
        }
        else {
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

                    let t1 = structurally_resolved_type(fcx, e.span, t_1);
                    let te = structurally_resolved_type(fcx, e.span, t_e);
                    let t_1_is_char = type_is_char(fcx, expr.span, t_1);

                    // casts to scalars other than `char` are allowed
                    let t_1_is_trivial = type_is_scalar(fcx, expr.span, t_1) && !t_1_is_char;

                    if type_is_c_like_enum(fcx, expr.span, t_e) && t_1_is_trivial {
                        // casts from C-like enums are allowed
                    } else if t_1_is_char {
                        if ty::get(te).sty != ty::ty_uint(ast::ty_u8) {
                            fcx.type_error_message(expr.span, |actual| {
                                fmt!("only `u8` can be cast as `char`, not `%s`", actual)
                            }, t_e, None);
                        }
                    } else if ty::get(t1).sty == ty::ty_bool {
                        fcx.tcx().sess.span_err(expr.span,
                                                "cannot cast as `bool`, compare with zero instead");
                    } else if type_is_region_ptr(fcx, expr.span, t_e) &&
                        type_is_unsafe_ptr(fcx, expr.span, t_1) {

                        fn is_vec(t: ty::t) -> bool {
                            match ty::get(t).sty {
                                ty::ty_evec(_,_) => true,
                                _ => false
                            }
                        }
                        fn types_compatible(fcx: @mut FnCtxt, sp: Span,
                                            t1: ty::t, t2: ty::t) -> bool {
                            if !is_vec(t1) {
                                false
                            } else {
                                let el = ty::sequence_element_type(fcx.tcx(),
                                                                   t1);
                                infer::mk_eqty(fcx.infcx(), false,
                                               infer::Misc(sp), el, t2).is_ok()
                            }
                        }

                        // Due to the limitations of LLVM global constants,
                        // region pointers end up pointing at copies of
                        // vector elements instead of the original values.
                        // To allow unsafe pointers to work correctly, we
                        // need to special-case obtaining an unsafe pointer
                        // from a region pointer to a vector.

                        /* this cast is only allowed from &[T] to *T or
                        &T to *T. */
                        match (&ty::get(te).sty, &ty::get(t_1).sty) {
                            (&ty::ty_rptr(_, mt1), &ty::ty_ptr(mt2))
                            if types_compatible(fcx, e.span,
                                                mt1.ty, mt2.ty) => {
                                /* this case is allowed */
                            }
                            _ => {
                                demand::coerce(fcx, e.span, t_1, e);
                            }
                        }
                    } else if !(type_is_scalar(fcx,expr.span,t_e)
                                && t_1_is_trivial) {
                        /*
                        If more type combinations should be supported than are
                        supported here, then file an enhancement issue and
                        record the issue number in this comment.
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
      }
      ast::ExprVec(ref args, mutbl) => {
        let t: ty::t = fcx.infcx().next_ty_var();
        for e in args.iter() {
            check_expr_has_type(fcx, *e, t);
        }
        let typ = ty::mk_evec(tcx, ty::mt {ty: t, mutbl: mutbl},
                              ty::vstore_fixed(args.len()));
        fcx.write_ty(id, typ);
      }
      ast::ExprRepeat(element, count_expr, mutbl) => {
        check_expr_with_hint(fcx, count_expr, ty::mk_uint());
        let count = ty::eval_repeat_count(fcx, count_expr);
        let t: ty::t = fcx.infcx().next_ty_var();
        check_expr_has_type(fcx, element, t);
        let element_ty = fcx.expr_ty(element);
        if ty::type_is_error(element_ty) {
            fcx.write_error(id);
        }
        else if ty::type_is_bot(element_ty) {
            fcx.write_bot(id);
        }
        else {
            let t = ty::mk_evec(tcx, ty::mt {ty: t, mutbl: mutbl},
                                ty::vstore_fixed(count));
            fcx.write_ty(id, t);
        }
      }
      ast::ExprTup(ref elts) => {
        let flds = unpack_expected(fcx, expected, |sty| {
            match *sty {
                ty::ty_tup(ref flds) => Some((*flds).clone()),
                _ => None
            }
        });
        let mut bot_field = false;
        let mut err_field = false;

        let elt_ts = do elts.iter().enumerate().map |(i, e)| {
            let opt_hint = match flds {
                Some(ref fs) if i < fs.len() => Some(fs[i]),
                _ => None
            };
            check_expr_with_opt_hint(fcx, *e, opt_hint);
            let t = fcx.expr_ty(*e);
            err_field = err_field || ty::type_is_error(t);
            bot_field = bot_field || ty::type_is_bot(t);
            t
        }.collect();
        if bot_field {
            fcx.write_bot(id);
        } else if err_field {
            fcx.write_error(id);
        } else {
            let typ = ty::mk_tup(tcx, elt_ts);
            fcx.write_ty(id, typ);
        }
      }
      ast::ExprStruct(ref path, ref fields, base_expr) => {
        // Resolve the path.
        match tcx.def_map.find(&id) {
            Some(&ast::DefStruct(type_def_id)) => {
                check_struct_constructor(fcx, id, expr.span, type_def_id,
                                         *fields, base_expr);
            }
            Some(&ast::DefVariant(enum_id, variant_id, _)) => {
                check_struct_enum_variant(fcx, id, expr.span, enum_id,
                                          variant_id, *fields);
            }
            _ => {
                tcx.sess.span_bug(path.span,
                                  "structure constructor does not name a structure type");
            }
        }
      }
      ast::ExprField(base, field, ref tys) => {
        check_field(fcx, expr, base, field.name, *tys);
      }
      ast::ExprIndex(callee_id, base, idx) => {
          check_expr(fcx, base);
          check_expr(fcx, idx);
          let raw_base_t = fcx.expr_ty(base);
          let idx_t = fcx.expr_ty(idx);
          if ty::type_is_error(raw_base_t) || ty::type_is_bot(raw_base_t) {
              fcx.write_ty(id, raw_base_t);
          } else if ty::type_is_error(idx_t) || ty::type_is_bot(idx_t) {
              fcx.write_ty(id, idx_t);
          } else {
              let (base_t, derefs) = do_autoderef(fcx, expr.span, raw_base_t);
              let base_sty = structure_of(fcx, expr.span, base_t);
              match ty::index_sty(base_sty) {
                  Some(mt) => {
                      require_integral(fcx, idx.span, idx_t);
                      fcx.write_ty(id, mt.ty);
                      fcx.write_autoderef_adjustment(base.id, derefs);
                  }
                  None => {
                      let resolved = structurally_resolved_type(fcx,
                                                                expr.span,
                                                                raw_base_t);
                      let index_ident = tcx.sess.ident_of("index");
                      let error_message = || {
                        fcx.type_error_message(expr.span,
                                               |actual| {
                                                fmt!("cannot index a value \
                                                      of type `%s`",
                                                     actual)
                                               },
                                               base_t,
                                               None);
                      };
                      let ret_ty = lookup_op_method(fcx,
                                                    callee_id,
                                                    expr,
                                                    base,
                                                    resolved,
                                                    index_ident.name,
                                                    ~[idx],
                                                    DoDerefArgs,
                                                    AutoderefReceiver,
                                                    error_message,
                                                    expected);
                      fcx.write_ty(id, ret_ty);
                  }
              }
          }
       }
    }

    debug!("type of expr(%d) %s is...", expr.id,
           syntax::print::pprust::expr_to_str(expr, tcx.sess.intr()));
    debug!("... %s, expected is %s",
           ppaux::ty_to_str(tcx, fcx.expr_ty(expr)),
           match expected {
               Some(t) => ppaux::ty_to_str(tcx, t),
               _ => ~"empty"
           });

    unifier();
}

pub fn require_integral(fcx: @mut FnCtxt, sp: Span, t: ty::t) {
    if !type_is_integral(fcx, sp, t) {
        fcx.type_error_message(sp, |actual| {
            fmt!("mismatched types: expected integral type but found `%s`",
                 actual)
        }, t, None);
    }
}

pub fn check_decl_initializer(fcx: @mut FnCtxt,
                              nid: ast::NodeId,
                              init: @ast::Expr)
                            {
    let local_ty = fcx.local_ty(init.span, nid);
    check_expr_coercable_to_type(fcx, init, local_ty)
}

pub fn check_decl_local(fcx: @mut FnCtxt, local: @ast::Local)  {
    let tcx = fcx.ccx.tcx;

    let t = fcx.local_ty(local.span, local.id);
    fcx.write_ty(local.id, t);

    match local.init {
        Some(init) => {
            check_decl_initializer(fcx, local.id, init);
            let init_ty = fcx.expr_ty(init);
            if ty::type_is_error(init_ty) || ty::type_is_bot(init_ty) {
                fcx.write_ty(local.id, init_ty);
            }
        }
        _ => {}
    }

    let pcx = pat_ctxt {
        fcx: fcx,
        map: pat_id_map(tcx.def_map, local.pat),
    };
    _match::check_pat(&pcx, local.pat, t);
    let pat_ty = fcx.node_ty(local.pat.id);
    if ty::type_is_error(pat_ty) || ty::type_is_bot(pat_ty) {
        fcx.write_ty(local.id, pat_ty);
    }
}

pub fn check_stmt(fcx: @mut FnCtxt, stmt: @ast::Stmt)  {
    let node_id;
    let mut saw_bot = false;
    let mut saw_err = false;
    match stmt.node {
      ast::StmtDecl(decl, id) => {
        node_id = id;
        match decl.node {
          ast::DeclLocal(ref l) => {
              check_decl_local(fcx, *l);
              let l_t = fcx.node_ty(l.id);
              saw_bot = saw_bot || ty::type_is_bot(l_t);
              saw_err = saw_err || ty::type_is_error(l_t);
          }
          ast::DeclItem(_) => {/* ignore for now */ }
        }
      }
      ast::StmtExpr(expr, id) => {
        node_id = id;
        // Check with expected type of ()
        check_expr_has_type(fcx, expr, ty::mk_nil());
        let expr_ty = fcx.expr_ty(expr);
        saw_bot = saw_bot || ty::type_is_bot(expr_ty);
        saw_err = saw_err || ty::type_is_error(expr_ty);
      }
      ast::StmtSemi(expr, id) => {
        node_id = id;
        check_expr(fcx, expr);
        let expr_ty = fcx.expr_ty(expr);
        saw_bot |= ty::type_is_bot(expr_ty);
        saw_err |= ty::type_is_error(expr_ty);
      }
      ast::StmtMac(*) => fcx.ccx.tcx.sess.bug("unexpanded macro")
    }
    if saw_bot {
        fcx.write_bot(node_id);
    }
    else if saw_err {
        fcx.write_error(node_id);
    }
    else {
        fcx.write_nil(node_id)
    }
}

pub fn check_block_no_value(fcx: @mut FnCtxt, blk: &ast::Block)  {
    check_block_with_expected(fcx, blk, Some(ty::mk_nil()));
    let blkty = fcx.node_ty(blk.id);
    if ty::type_is_error(blkty) {
        fcx.write_error(blk.id);
    }
    else if ty::type_is_bot(blkty) {
        fcx.write_bot(blk.id);
    }
    else {
        let nilty = ty::mk_nil();
        demand::suptype(fcx, blk.span, nilty, blkty);
    }
}

pub fn check_block(fcx0: @mut FnCtxt, blk: &ast::Block)  {
    check_block_with_expected(fcx0, blk, None)
}

pub fn check_block_with_expected(fcx: @mut FnCtxt,
                                 blk: &ast::Block,
                                 expected: Option<ty::t>) {
    let purity_state = fcx.ps.recurse(blk);
    let prev = replace(&mut fcx.ps, purity_state);

    do fcx.with_region_lb(blk.id) {
        let mut warned = false;
        let mut last_was_bot = false;
        let mut any_bot = false;
        let mut any_err = false;
        for s in blk.stmts.iter() {
            check_stmt(fcx, *s);
            let s_id = ast_util::stmt_id(*s);
            let s_ty = fcx.node_ty(s_id);
            if last_was_bot && !warned && match s.node {
                  ast::StmtDecl(@codemap::Spanned { node: ast::DeclLocal(_),
                                                 _}, _) |
                  ast::StmtExpr(_, _) | ast::StmtSemi(_, _) => {
                    true
                  }
                  _ => false
                } {
                fcx.ccx.tcx.sess.add_lint(unreachable_code, s_id, s.span,
                                          ~"unreachable statement");
                warned = true;
            }
            if ty::type_is_bot(s_ty) {
                last_was_bot = true;
            }
            any_bot = any_bot || ty::type_is_bot(s_ty);
            any_err = any_err || ty::type_is_error(s_ty);
        }
        match blk.expr {
            None => if any_err {
                fcx.write_error(blk.id);
            }
            else if any_bot {
                fcx.write_bot(blk.id);
            }
            else  {
                fcx.write_nil(blk.id);
            },
          Some(e) => {
            if any_bot && !warned {
                fcx.ccx.tcx.sess.span_warn(e.span, "unreachable expression");
            }
            check_expr_with_opt_hint(fcx, e, expected);
              let ety = fcx.expr_ty(e);
              fcx.write_ty(blk.id, ety);
              if any_err {
                  fcx.write_error(blk.id);
              }
              else if any_bot {
                  fcx.write_bot(blk.id);
              }
          }
        };
    }

    fcx.ps = prev;
}

pub fn check_const(ccx: @mut CrateCtxt,
                   sp: Span,
                   e: @ast::Expr,
                   id: ast::NodeId) {
    let rty = ty::node_id_to_type(ccx.tcx, id);
    let fcx = blank_fn_ctxt(ccx, rty, e.id);
    let declty = fcx.ccx.tcx.tcache.get(&local_def(id)).ty;
    check_const_with_ty(fcx, sp, e, declty);
}

pub fn check_const_with_ty(fcx: @mut FnCtxt,
                           _: Span,
                           e: @ast::Expr,
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
pub fn check_instantiable(tcx: ty::ctxt,
                          sp: Span,
                          item_id: ast::NodeId) {
    let item_ty = ty::node_id_to_type(tcx, item_id);
    if !ty::is_instantiable(tcx, item_ty) {
        tcx.sess.span_err(sp, fmt!("this type cannot be instantiated \
                  without an instance of itself; \
                  consider using `Option<%s>`",
                                   ppaux::ty_to_str(tcx, item_ty)));
    }
}

pub fn check_simd(tcx: ty::ctxt, sp: Span, id: ast::NodeId) {
    let t = ty::node_id_to_type(tcx, id);
    if ty::type_needs_subst(t) {
        tcx.sess.span_err(sp, "SIMD vector cannot be generic");
        return;
    }
    match ty::get(t).sty {
        ty::ty_struct(did, ref substs) => {
            let fields = ty::lookup_struct_fields(tcx, did);
            if fields.is_empty() {
                tcx.sess.span_err(sp, "SIMD vector cannot be empty");
                return;
            }
            let e = ty::lookup_field_type(tcx, did, fields[0].id, substs);
            if !fields.iter().all(
                         |f| ty::lookup_field_type(tcx, did, f.id, substs) == e) {
                tcx.sess.span_err(sp, "SIMD vector should be homogeneous");
                return;
            }
            if !ty::type_is_machine(e) {
                tcx.sess.span_err(sp, "SIMD vector element type should be \
                                       machine type");
                return;
            }
        }
        _ => ()
    }
}

pub fn check_enum_variants(ccx: @mut CrateCtxt,
                           sp: Span,
                           vs: &[ast::variant],
                           id: ast::NodeId) {
    fn do_check(ccx: @mut CrateCtxt,
                vs: &[ast::variant],
                id: ast::NodeId)
                -> ~[@ty::VariantInfo] {

        let rty = ty::node_id_to_type(ccx.tcx, id);
        let mut variants: ~[@ty::VariantInfo] = ~[];
        let mut disr_vals: ~[ty::Disr] = ~[];
        let mut prev_disr_val: Option<ty::Disr> = None;

        for v in vs.iter() {

            // If the discriminant value is specified explicitly in the enum check whether the
            // initialization expression is valid, otherwise use the last value plus one.
            let mut current_disr_val = match prev_disr_val {
                Some(prev_disr_val) => prev_disr_val + 1,
                None => ty::INITIAL_DISCRIMINANT_VALUE
            };

            match v.node.disr_expr {
                Some(e) => {
                    debug!("disr expr, checking %s", pprust::expr_to_str(e, ccx.tcx.sess.intr()));

                    let fcx = blank_fn_ctxt(ccx, rty, e.id);
                    let declty = ty::mk_int_var(ccx.tcx, fcx.infcx().next_int_var_id());
                    check_const_with_ty(fcx, e.span, e, declty);
                    // check_expr (from check_const pass) doesn't guarantee
                    // that the expression is in an form that eval_const_expr can
                    // handle, so we may still get an internal compiler error

                    match const_eval::eval_const_expr_partial(&ccx.tcx, e) {
                        Ok(const_eval::const_int(val)) => current_disr_val = val as Disr,
                        Ok(const_eval::const_uint(val)) => current_disr_val = val as Disr,
                        Ok(_) => {
                            ccx.tcx.sess.span_err(e.span, "expected signed integer constant");
                        }
                        Err(ref err) => {
                            ccx.tcx.sess.span_err(e.span, fmt!("expected constant: %s", (*err)));
                        }
                    }
                },
                None => ()
            };

            // Check for duplicate discriminator values
            if disr_vals.contains(&current_disr_val) {
                ccx.tcx.sess.span_err(v.span, "discriminator value already exists");
            }
            disr_vals.push(current_disr_val);

            let variant_info = @VariantInfo::from_ast_variant(ccx.tcx, v, current_disr_val);
            prev_disr_val = Some(current_disr_val);

            variants.push(variant_info);
        }

        return variants;
    }

    let rty = ty::node_id_to_type(ccx.tcx, id);

    let variants = do_check(ccx, vs, id);

    // cache so that ty::enum_variants won't repeat this work
    ccx.tcx.enum_var_cache.insert(local_def(id), @variants);

    // Check that it is possible to represent this enum:
    let mut outer = true;
    let did = local_def(id);
    if ty::type_structurally_contains(ccx.tcx, rty, |sty| {
        match *sty {
          ty::ty_enum(id, _) if id == did => {
            if outer { outer = false; false }
            else { true }
          }
          _ => false
        }
    }) {
        ccx.tcx.sess.span_err(sp,
                              "illegal recursive enum type; \
                               wrap the inner value in a box to make it representable");
    }

    // Check that it is possible to instantiate this enum:
    //
    // This *sounds* like the same that as representable, but it's
    // not.  See def'n of `check_instantiable()` for details.
    check_instantiable(ccx.tcx, sp, id);
}

pub fn lookup_def(fcx: @mut FnCtxt, sp: Span, id: ast::NodeId) -> ast::Def {
    lookup_def_ccx(fcx.ccx, sp, id)
}

// Returns the type parameter count and the type for the given definition.
pub fn ty_param_bounds_and_ty_for_def(fcx: @mut FnCtxt,
                                      sp: Span,
                                      defn: ast::Def)
                                   -> ty_param_bounds_and_ty {
    match defn {
      ast::DefArg(nid, _) | ast::DefLocal(nid, _) | ast::DefSelf(nid) |
      ast::DefBinding(nid, _) => {
          let typ = fcx.local_ty(sp, nid);
          return no_params(typ);
      }
      ast::DefFn(id, _) | ast::DefStaticMethod(id, _, _) |
      ast::DefStatic(id, _) | ast::DefVariant(_, id, _) |
      ast::DefStruct(id) => {
        return ty::lookup_item_type(fcx.ccx.tcx, id);
      }
      ast::DefUpvar(_, inner, _, _) => {
        return ty_param_bounds_and_ty_for_def(fcx, sp, *inner);
      }
      ast::DefTrait(_) |
      ast::DefTy(_) |
      ast::DefPrimTy(_) |
      ast::DefTyParam(*)=> {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value but found type");
      }
      ast::DefMod(*) | ast::DefForeignMod(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value but found module");
      }
      ast::DefUse(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value but found use");
      }
      ast::DefRegion(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value but found region");
      }
      ast::DefTyParamBinder(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value but found type parameter");
      }
      ast::DefLabel(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value but found label");
      }
      ast::DefSelfTy(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value but found self ty");
      }
      ast::DefMethod(*) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value but found method");
      }
    }
}

// Instantiates the given path, which must refer to an item with the given
// number of type parameters and type.
pub fn instantiate_path(fcx: @mut FnCtxt,
                        pth: &ast::Path,
                        tpt: ty_param_bounds_and_ty,
                        def: ast::Def,
                        span: Span,
                        node_id: ast::NodeId) {
    debug!(">>> instantiate_path");

    let ty_param_count = tpt.generics.type_param_defs.len();
    let mut ty_substs_len = 0;
    for segment in pth.segments.iter() {
        ty_substs_len += segment.types.len()
    }

    debug!("tpt=%s ty_param_count=%? ty_substs_len=%?",
           tpt.repr(fcx.tcx()),
           ty_param_count,
           ty_substs_len);

    // determine the region bound, using the value given by the user
    // (if any) and otherwise using a fresh region variable
    let regions = match pth.segments.last().lifetime {
        Some(_) => { // user supplied a lifetime parameter...
            match tpt.generics.region_param {
                None => { // ...but the type is not lifetime parameterized!
                    fcx.ccx.tcx.sess.span_err
                        (span, "this item is not region-parameterized");
                    opt_vec::Empty
                }
                Some(_) => { // ...and the type is lifetime parameterized, ok.
                    opt_vec::with(
                        ast_region_to_region(fcx,
                                             fcx,
                                             span,
                                             &pth.segments.last().lifetime))
                }
            }
        }
        None => { // no lifetime parameter supplied, insert default
            fcx.region_var_if_parameterized(tpt.generics.region_param, span)
        }
    };

    // Special case: If there is a self parameter, omit it from the list of
    // type parameters.
    //
    // Here we calculate the "user type parameter count", which is the number
    // of type parameters actually manifest in the AST. This will differ from
    // the internal type parameter count when there are self types involved.
    let (user_type_parameter_count, self_parameter_index) = match def {
        ast::DefStaticMethod(_, provenance @ ast::FromTrait(_), _) => {
            let generics = generics_of_static_method_container(fcx.ccx.tcx,
                                                               provenance);
            (ty_param_count - 1, Some(generics.type_param_defs.len()))
        }
        _ => (ty_param_count, None),
    };

    // determine values for type parameters, using the values given by
    // the user (if any) and otherwise using fresh type variables
    let tps = if ty_substs_len == 0 {
        fcx.infcx().next_ty_vars(ty_param_count)
    } else if ty_param_count == 0 {
        fcx.ccx.tcx.sess.span_err
            (span, "this item does not take type parameters");
        fcx.infcx().next_ty_vars(ty_param_count)
    } else if ty_substs_len > user_type_parameter_count {
        fcx.ccx.tcx.sess.span_err
            (span,
             fmt!("too many type parameters provided: expected %u, found %u",
                  user_type_parameter_count, ty_substs_len));
        fcx.infcx().next_ty_vars(ty_param_count)
    } else if ty_substs_len < user_type_parameter_count {
        fcx.ccx.tcx.sess.span_err
            (span,
             fmt!("not enough type parameters provided: expected %u, found %u",
                  user_type_parameter_count, ty_substs_len));
        fcx.infcx().next_ty_vars(ty_param_count)
    } else {
        // Build up the list of type parameters, inserting the self parameter
        // at the appropriate position.
        let mut result = ~[];
        let mut pushed = false;
        for (i, ast_type) in pth.segments
                                .iter()
                                .flat_map(|segment| segment.types.iter())
                                .enumerate() {
            match self_parameter_index {
                Some(index) if index == i => {
                    result.push(fcx.infcx().next_ty_vars(1)[0]);
                    pushed = true;
                }
                _ => {}
            }
            result.push(fcx.to_ty(ast_type))
        }

        // If the self parameter goes at the end, insert it there.
        if !pushed && self_parameter_index.is_some() {
            result.push(fcx.infcx().next_ty_vars(1)[0])
        }

        assert_eq!(result.len(), ty_param_count)
        result
    };

    let substs = substs {
        regions: ty::NonerasedRegions(regions),
        self_ty: None,
        tps: tps
    };
    fcx.write_ty_substs(node_id, tpt.ty, substs);

    debug!("<<<");
}

// Resolves `typ` by a single level if `typ` is a type variable.  If no
// resolution is possible, then an error is reported.
pub fn structurally_resolved_type(fcx: @mut FnCtxt, sp: Span, tp: ty::t)
                               -> ty::t {
    match infer::resolve_type(fcx.infcx(), tp, force_tvar) {
        Ok(t_s) if !ty::type_is_ty_var(t_s) => t_s,
        _ => {
            fcx.type_error_message(sp, |_actual| {
                ~"the type of this value must be known in this context"
            }, tp, None);
            demand::suptype(fcx, sp, ty::mk_err(), tp);
            tp
        }
    }
}

// Returns the one-level-deep structure of the given type.
pub fn structure_of<'a>(fcx: @mut FnCtxt, sp: Span, typ: ty::t)
                        -> &'a ty::sty {
    &ty::get(structurally_resolved_type(fcx, sp, typ)).sty
}

pub fn type_is_integral(fcx: @mut FnCtxt, sp: Span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    return ty::type_is_integral(typ_s);
}

pub fn type_is_scalar(fcx: @mut FnCtxt, sp: Span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    return ty::type_is_scalar(typ_s);
}

pub fn type_is_char(fcx: @mut FnCtxt, sp: Span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    return ty::type_is_char(typ_s);
}

pub fn type_is_unsafe_ptr(fcx: @mut FnCtxt, sp: Span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    return ty::type_is_unsafe_ptr(typ_s);
}

pub fn type_is_region_ptr(fcx: @mut FnCtxt, sp: Span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    return ty::type_is_region_ptr(typ_s);
}

pub fn type_is_c_like_enum(fcx: @mut FnCtxt, sp: Span, typ: ty::t) -> bool {
    let typ_s = structurally_resolved_type(fcx, sp, typ);
    return ty::type_is_c_like_enum(fcx.ccx.tcx, typ_s);
}

pub fn ast_expr_vstore_to_vstore(fcx: @mut FnCtxt,
                                 e: @ast::Expr,
                                 v: ast::ExprVstore)
                              -> ty::vstore {
    match v {
        ast::ExprVstoreUniq => ty::vstore_uniq,
        ast::ExprVstoreBox | ast::ExprVstoreMutBox => ty::vstore_box,
        ast::ExprVstoreSlice | ast::ExprVstoreMutSlice => {
            let r = fcx.infcx().next_region_var(infer::AddrOfSlice(e.span));
            ty::vstore_slice(r)
        }
    }
}

// Returns true if b contains a break that can exit from b
pub fn may_break(cx: ty::ctxt, id: ast::NodeId, b: &ast::Block) -> bool {
    // First: is there an unlabeled break immediately
    // inside the loop?
    (loop_query(b, |e| {
        match *e {
            ast::ExprBreak(_) => true,
            _ => false
        }
    })) ||
   // Second: is there a labeled break with label
   // <id> nested anywhere inside the loop?
    (block_query(b, |e| {
        match e.node {
            ast::ExprBreak(Some(_)) =>
                match cx.def_map.find(&e.id) {
                    Some(&ast::DefLabel(loop_id)) if id == loop_id => true,
                    _ => false,
                },
            _ => false
        }}))
}

pub fn check_bounds_are_used(ccx: @mut CrateCtxt,
                             span: Span,
                             tps: &OptVec<ast::TyParam>,
                             ty: ty::t) {
    debug!("check_bounds_are_used(n_tps=%u, ty=%s)",
           tps.len(), ppaux::ty_to_str(ccx.tcx, ty));

    // make a vector of booleans initially false, set to true when used
    if tps.len() == 0u { return; }
    let mut tps_used = vec::from_elem(tps.len(), false);

    ty::walk_regions_and_ty(
        ccx.tcx, ty,
        |_r| {},
        |t| {
            match ty::get(t).sty {
              ty::ty_param(param_ty {idx, _}) => {
                  debug!("Found use of ty param #%u", idx);
                  tps_used[idx] = true;
              }
              _ => ()
            }
            true
        });

    for (i, b) in tps_used.iter().enumerate() {
        if !*b {
            ccx.tcx.sess.span_err(
                span, fmt!("type parameter `%s` is unused",
                           ccx.tcx.sess.str_of(tps.get(i).ident)));
        }
    }
}

pub fn check_intrinsic_type(ccx: @mut CrateCtxt, it: @ast::foreign_item) {
    fn param(ccx: @mut CrateCtxt, n: uint) -> ty::t {
        ty::mk_param(ccx.tcx, n, local_def(0))
    }

    let tcx = ccx.tcx;
    let nm = ccx.tcx.sess.str_of(it.ident);
    let name = nm.as_slice();
    let (n_tps, inputs, output) = if name.starts_with("atomic_") {
        let split : ~[&str] = name.split_iter('_').collect();
        assert!(split.len() >= 2, "Atomic intrinsic not correct format");

        //We only care about the operation here
        match split[1] {
            "cxchg" => (0, ~[ty::mk_mut_rptr(tcx,
                                             ty::re_bound(ty::br_anon(0)),
                                             ty::mk_int()),
                        ty::mk_int(),
                        ty::mk_int()
                        ], ty::mk_int()),
            "load" => (0,
               ~[
                  ty::mk_imm_rptr(tcx, ty::re_bound(ty::br_anon(0)), ty::mk_int())
               ],
              ty::mk_int()),
            "store" => (0,
               ~[
                  ty::mk_mut_rptr(tcx, ty::re_bound(ty::br_anon(0)), ty::mk_int()),
                  ty::mk_int()
               ],
               ty::mk_nil()),

            "xchg" | "xadd" | "xsub" | "and"  | "nand" | "or"   | "xor"  | "max"  |
            "min"  | "umax" | "umin" => {
                (0, ~[ty::mk_mut_rptr(tcx,
                                      ty::re_bound(ty::br_anon(0)),
                                      ty::mk_int()), ty::mk_int() ], ty::mk_int())
            }
            "fence" => {
                (0, ~[], ty::mk_nil())
            }
            op => {
                tcx.sess.span_err(it.span,
                                  fmt!("unrecognized atomic operation function: `%s`",
                                       op));
                return;
            }
        }

    } else {
        match name {
            "size_of" |
            "pref_align_of" | "min_align_of" => (1u, ~[], ty::mk_uint()),
            "init" => (1u, ~[], param(ccx, 0u)),
            "uninit" => (1u, ~[], param(ccx, 0u)),
            "forget" => (1u, ~[ param(ccx, 0) ], ty::mk_nil()),
            "transmute" => (2, ~[ param(ccx, 0) ], param(ccx, 1)),
            "move_val" | "move_val_init" => {
                (1u,
                 ~[
                    ty::mk_mut_rptr(tcx, ty::re_bound(ty::br_anon(0)), param(ccx, 0)),
                    param(ccx, 0u)
                  ],
               ty::mk_nil())
            }
            "needs_drop" => (1u, ~[], ty::mk_bool()),
            "contains_managed" => (1u, ~[], ty::mk_bool()),
            "atomic_xchg"     | "atomic_xadd"     | "atomic_xsub"     |
            "atomic_xchg_acq" | "atomic_xadd_acq" | "atomic_xsub_acq" |
            "atomic_xchg_rel" | "atomic_xadd_rel" | "atomic_xsub_rel" => {
              (0,
               ~[
                  ty::mk_mut_rptr(tcx, ty::re_bound(ty::br_anon(0)), ty::mk_int()),
                  ty::mk_int()
               ],
               ty::mk_int())
            }

            "get_tydesc" => {
              let tydesc_ty = match ty::get_tydesc_ty(ccx.tcx) {
                  Ok(t) => t,
                  Err(s) => { tcx.sess.span_fatal(it.span, s); }
              };
              let td_ptr = ty::mk_ptr(ccx.tcx, ty::mt {
                  ty: tydesc_ty,
                  mutbl: ast::MutImmutable
              });
              (1u, ~[], td_ptr)
            }
            "visit_tydesc" => {
              let tydesc_ty = match ty::get_tydesc_ty(ccx.tcx) {
                  Ok(t) => t,
                  Err(s) => { tcx.sess.span_fatal(it.span, s); }
              };
              let region = ty::re_bound(ty::br_anon(0));
              let visitor_object_ty = match ty::visitor_object_ty(tcx, region) {
                  Ok((_, vot)) => vot,
                  Err(s) => { tcx.sess.span_fatal(it.span, s); }
              };

              let td_ptr = ty::mk_ptr(ccx.tcx, ty::mt {
                  ty: tydesc_ty,
                  mutbl: ast::MutImmutable
              });
              (0, ~[ td_ptr, visitor_object_ty ], ty::mk_nil())
            }
            "frame_address" => {
              let fty = ty::mk_closure(ccx.tcx, ty::ClosureTy {
                  purity: ast::impure_fn,
                  sigil: ast::BorrowedSigil,
                  onceness: ast::Once,
                  region: ty::re_bound(ty::br_anon(0)),
                  bounds: ty::EmptyBuiltinBounds(),
                  sig: ty::FnSig {
                      bound_lifetime_names: opt_vec::Empty,
                      inputs: ~[ty::mk_imm_ptr(ccx.tcx, ty::mk_mach_uint(ast::ty_u8))],
                      output: ty::mk_nil()
                  }
              });
              (0u, ~[fty], ty::mk_nil())
            }
            "morestack_addr" => {
              (0u, ~[], ty::mk_nil_ptr(ccx.tcx))
            }
            "offset" => {
              (1,
               ~[
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutImmutable
                  }),
                  ty::mk_int()
               ],
               ty::mk_ptr(tcx, ty::mt {
                   ty: param(ccx, 0),
                   mutbl: ast::MutImmutable
               }))
            }
            "memcpy32" => {
              (1,
               ~[
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutMutable
                  }),
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutImmutable
                  }),
                  ty::mk_u32()
               ],
               ty::mk_nil())
            }
            "memcpy64" => {
              (1,
               ~[
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutMutable
                  }),
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutImmutable
                  }),
                  ty::mk_u64()
               ],
               ty::mk_nil())
            }
            "memmove32" => {
              (1,
               ~[
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutMutable
                  }),
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutImmutable
                  }),
                  ty::mk_u32()
               ],
               ty::mk_nil())
            }
            "memmove64" => {
              (1,
               ~[
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutMutable
                  }),
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutImmutable
                  }),
                  ty::mk_u64()
               ],
               ty::mk_nil())
            }
            "memset32" => {
              (1,
               ~[
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutMutable
                  }),
                  ty::mk_u8(),
                  ty::mk_u32()
               ],
               ty::mk_nil())
            }
            "memset64" => {
              (1,
               ~[
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutMutable
                  }),
                  ty::mk_u8(),
                  ty::mk_u64()
               ],
               ty::mk_nil())
            }
            "sqrtf32" => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "sqrtf64" => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "powif32" => {
               (0,
                ~[ ty::mk_f32(), ty::mk_i32() ],
                ty::mk_f32())
            }
            "powif64" => {
               (0,
                ~[ ty::mk_f64(), ty::mk_i32() ],
                ty::mk_f64())
            }
            "sinf32" => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "sinf64" => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "cosf32" => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "cosf64" => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "powf32" => {
               (0,
                ~[ ty::mk_f32(), ty::mk_f32() ],
                ty::mk_f32())
            }
            "powf64" => {
               (0,
                ~[ ty::mk_f64(), ty::mk_f64() ],
                ty::mk_f64())
            }
            "expf32"   => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "expf64"   => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "exp2f32"  => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "exp2f64"  => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "logf32"   => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "logf64"   => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "log10f32" => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "log10f64" => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "log2f32"  => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "log2f64"  => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "fmaf32" => {
                (0,
                 ~[ ty::mk_f32(), ty::mk_f32(), ty::mk_f32() ],
                 ty::mk_f32())
            }
            "fmaf64" => {
                (0,
                 ~[ ty::mk_f64(), ty::mk_f64(), ty::mk_f64() ],
                 ty::mk_f64())
            }
            "fabsf32"  => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "fabsf64"  => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "floorf32" => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "floorf64" => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "ceilf32"  => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "ceilf64"  => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "truncf32" => (0, ~[ ty::mk_f32() ], ty::mk_f32()),
            "truncf64" => (0, ~[ ty::mk_f64() ], ty::mk_f64()),
            "ctpop8"   => (0, ~[ ty::mk_i8()  ], ty::mk_i8()),
            "ctpop16"  => (0, ~[ ty::mk_i16() ], ty::mk_i16()),
            "ctpop32"  => (0, ~[ ty::mk_i32() ], ty::mk_i32()),
            "ctpop64"  => (0, ~[ ty::mk_i64() ], ty::mk_i64()),
            "ctlz8"    => (0, ~[ ty::mk_i8()  ], ty::mk_i8()),
            "ctlz16"   => (0, ~[ ty::mk_i16() ], ty::mk_i16()),
            "ctlz32"   => (0, ~[ ty::mk_i32() ], ty::mk_i32()),
            "ctlz64"   => (0, ~[ ty::mk_i64() ], ty::mk_i64()),
            "cttz8"    => (0, ~[ ty::mk_i8()  ], ty::mk_i8()),
            "cttz16"   => (0, ~[ ty::mk_i16() ], ty::mk_i16()),
            "cttz32"   => (0, ~[ ty::mk_i32() ], ty::mk_i32()),
            "cttz64"   => (0, ~[ ty::mk_i64() ], ty::mk_i64()),
            "bswap16"  => (0, ~[ ty::mk_i16() ], ty::mk_i16()),
            "bswap32"  => (0, ~[ ty::mk_i32() ], ty::mk_i32()),
            "bswap64"  => (0, ~[ ty::mk_i64() ], ty::mk_i64()),

            "i8_add_with_overflow" | "i8_sub_with_overflow" | "i8_mul_with_overflow" =>
                (0, ~[ty::mk_i8(), ty::mk_i8()],
                ty::mk_tup(tcx, ~[ty::mk_i8(), ty::mk_bool()])),

            "i16_add_with_overflow" | "i16_sub_with_overflow" | "i16_mul_with_overflow" =>
                (0, ~[ty::mk_i16(), ty::mk_i16()],
                ty::mk_tup(tcx, ~[ty::mk_i16(), ty::mk_bool()])),

            "i32_add_with_overflow" | "i32_sub_with_overflow" | "i32_mul_with_overflow" =>
                (0, ~[ty::mk_i32(), ty::mk_i32()],
                ty::mk_tup(tcx, ~[ty::mk_i32(), ty::mk_bool()])),

            "i64_add_with_overflow" | "i64_sub_with_overflow" | "i64_mul_with_overflow" =>
                (0, ~[ty::mk_i64(), ty::mk_i64()],
                ty::mk_tup(tcx, ~[ty::mk_i64(), ty::mk_bool()])),

            "u8_add_with_overflow" | "u8_sub_with_overflow" | "u8_mul_with_overflow" =>
                (0, ~[ty::mk_u8(), ty::mk_u8()],
                ty::mk_tup(tcx, ~[ty::mk_u8(), ty::mk_bool()])),

            "u16_add_with_overflow" | "u16_sub_with_overflow" | "u16_mul_with_overflow" =>
                (0, ~[ty::mk_u16(), ty::mk_u16()],
                ty::mk_tup(tcx, ~[ty::mk_u16(), ty::mk_bool()])),

            "u32_add_with_overflow" | "u32_sub_with_overflow" | "u32_mul_with_overflow"=>
                (0, ~[ty::mk_u32(), ty::mk_u32()],
                ty::mk_tup(tcx, ~[ty::mk_u32(), ty::mk_bool()])),

            "u64_add_with_overflow" | "u64_sub_with_overflow"  | "u64_mul_with_overflow" =>
                (0, ~[ty::mk_u64(), ty::mk_u64()],
                ty::mk_tup(tcx, ~[ty::mk_u64(), ty::mk_bool()])),

            ref other => {
                tcx.sess.span_err(it.span,
                                  fmt!("unrecognized intrinsic function: `%s`",
                                       *other));
                return;
            }
        }
    };
    let fty = ty::mk_bare_fn(tcx, ty::BareFnTy {
        purity: ast::unsafe_fn,
        abis: AbiSet::Intrinsic(),
        sig: FnSig {bound_lifetime_names: opt_vec::Empty,
                    inputs: inputs,
                    output: output}
    });
    let i_ty = ty::lookup_item_type(ccx.tcx, local_def(it.id));
    let i_n_tps = i_ty.generics.type_param_defs.len();
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
