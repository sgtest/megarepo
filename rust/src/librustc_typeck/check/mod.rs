// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
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
  through the `demand` module.  The `infer` module is in charge
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
stored in `fcx.node_types` and `fcx.item_substs`.  These types
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

pub use self::LvaluePreference::*;
pub use self::Expectation::*;
use self::IsBinopAssignment::*;
use self::TupleArgumentsFlag::*;

use astconv::{self, ast_region_to_region, ast_ty_to_ty, AstConv};
use check::_match::pat_ctxt;
use middle::{const_eval, def};
use middle::infer;
use middle::lang_items::IteratorItem;
use middle::mem_categorization as mc;
use middle::mem_categorization::McResult;
use middle::pat_util::{self, pat_id_map};
use middle::region::CodeExtent;
use middle::subst::{self, Subst, Substs, VecPerParamSpace, ParamSpace};
use middle::traits;
use middle::ty::{FnSig, VariantInfo, TypeScheme};
use middle::ty::{Disr, ParamTy, ParameterEnvironment};
use middle::ty::{self, HasProjectionTypes, RegionEscape, Ty};
use middle::ty::liberate_late_bound_regions;
use middle::ty::{MethodCall, MethodCallee, MethodMap, ObjectCastMap};
use middle::ty_fold::{TypeFolder, TypeFoldable};
use rscope::RegionScope;
use session::Session;
use {CrateCtxt, lookup_def_ccx, no_params, require_same_types};
use TypeAndSubsts;
use middle::lang_items::TypeIdLangItem;
use lint;
use util::common::{block_query, indenter, loop_query};
use util::ppaux::{self, UserString, Repr};
use util::nodemap::{DefIdMap, FnvHashMap, NodeMap};

use std::cell::{Cell, Ref, RefCell};
use std::mem::replace;
use std::rc::Rc;
use std::iter::repeat;
use syntax::{self, abi, attr};
use syntax::ast::{self, ProvidedMethod, RequiredMethod, TypeTraitItem, DefId};
use syntax::ast_util::{self, local_def, PostExpansionMethod};
use syntax::codemap::{self, Span};
use syntax::owned_slice::OwnedSlice;
use syntax::parse::token;
use syntax::print::pprust;
use syntax::ptr::P;
use syntax::visit::{self, Visitor};

mod assoc;
pub mod _match;
pub mod vtable;
pub mod writeback;
pub mod regionmanip;
pub mod regionck;
pub mod demand;
pub mod method;
mod upvar;
pub mod wf;
mod closure;
mod callee;

/// Fields that are part of a `FnCtxt` which are inherited by
/// closures defined within the function.  For example:
///
///     fn foo() {
///         bar(move|| { ... })
///     }
///
/// Here, the function `foo()` and the closure passed to
/// `bar()` will each have their own `FnCtxt`, but they will
/// share the inherited fields.
pub struct Inherited<'a, 'tcx: 'a> {
    infcx: infer::InferCtxt<'a, 'tcx>,
    locals: RefCell<NodeMap<Ty<'tcx>>>,
    param_env: ty::ParameterEnvironment<'a, 'tcx>,

    // Temporary tables:
    node_types: RefCell<NodeMap<Ty<'tcx>>>,
    item_substs: RefCell<NodeMap<ty::ItemSubsts<'tcx>>>,
    adjustments: RefCell<NodeMap<ty::AutoAdjustment<'tcx>>>,
    method_map: MethodMap<'tcx>,
    upvar_borrow_map: RefCell<ty::UpvarBorrowMap>,
    unboxed_closures: RefCell<DefIdMap<ty::UnboxedClosure<'tcx>>>,
    object_cast_map: ObjectCastMap<'tcx>,

    // A mapping from each fn's id to its signature, with all bound
    // regions replaced with free ones. Unlike the other tables, this
    // one is never copied into the tcx: it is only used by regionck.
    fn_sig_map: RefCell<NodeMap<Vec<Ty<'tcx>>>>,

    // Tracks trait obligations incurred during this function body.
    fulfillment_cx: RefCell<traits::FulfillmentContext<'tcx>>,
}

/// When type-checking an expression, we propagate downward
/// whatever type hint we are able in the form of an `Expectation`.
#[derive(Copy)]
enum Expectation<'tcx> {
    /// We know nothing about what type this expression should have.
    NoExpectation,

    /// This expression should have the type given (or some subtype)
    ExpectHasType(Ty<'tcx>),

    /// This expression will be cast to the `Ty`
    ExpectCastableToType(Ty<'tcx>),

    /// This rvalue expression will be wrapped in `&` or `Box` and coerced
    /// to `&Ty` or `Box<Ty>`, respectively. `Ty` is `[A]` or `Trait`.
    ExpectRvalueLikeUnsized(Ty<'tcx>),
}

impl<'tcx> Expectation<'tcx> {
    // Disregard "castable to" expectations because they
    // can lead us astray. Consider for example `if cond
    // {22} else {c} as u8` -- if we propagate the
    // "castable to u8" constraint to 22, it will pick the
    // type 22u8, which is overly constrained (c might not
    // be a u8). In effect, the problem is that the
    // "castable to" expectation is not the tightest thing
    // we can say, so we want to drop it in this case.
    // The tightest thing we can say is "must unify with
    // else branch". Note that in the case of a "has type"
    // constraint, this limitation does not hold.

    // If the expected type is just a type variable, then don't use
    // an expected type. Otherwise, we might write parts of the type
    // when checking the 'then' block which are incompatible with the
    // 'else' branch.
    fn adjust_for_branches<'a>(&self, fcx: &FnCtxt<'a, 'tcx>) -> Expectation<'tcx> {
        match *self {
            ExpectHasType(ety) => {
                let ety = fcx.infcx().shallow_resolve(ety);
                if !ty::type_is_ty_var(ety) {
                    ExpectHasType(ety)
                } else {
                    NoExpectation
                }
            }
            ExpectRvalueLikeUnsized(ety) => {
                ExpectRvalueLikeUnsized(ety)
            }
            _ => NoExpectation
        }
    }
}

#[derive(Copy, Clone)]
pub struct UnsafetyState {
    pub def: ast::NodeId,
    pub unsafety: ast::Unsafety,
    from_fn: bool
}

impl UnsafetyState {
    pub fn function(unsafety: ast::Unsafety, def: ast::NodeId) -> UnsafetyState {
        UnsafetyState { def: def, unsafety: unsafety, from_fn: true }
    }

    pub fn recurse(&mut self, blk: &ast::Block) -> UnsafetyState {
        match self.unsafety {
            // If this unsafe, then if the outer function was already marked as
            // unsafe we shouldn't attribute the unsafe'ness to the block. This
            // way the block can be warned about instead of ignoring this
            // extraneous block (functions are never warned about).
            ast::Unsafety::Unsafe if self.from_fn => *self,

            unsafety => {
                let (unsafety, def) = match blk.rules {
                    ast::UnsafeBlock(..) => (ast::Unsafety::Unsafe, blk.id),
                    ast::DefaultBlock => (unsafety, self.def),
                };
                UnsafetyState{ def: def,
                             unsafety: unsafety,
                             from_fn: false }
            }
        }
    }
}

/// Whether `check_binop` is part of an assignment or not.
/// Used to know whether we allow user overloads and to print
/// better messages on error.
#[derive(PartialEq)]
enum IsBinopAssignment{
    SimpleBinop,
    BinopAssignment,
}

#[derive(Clone)]
pub struct FnCtxt<'a, 'tcx: 'a> {
    body_id: ast::NodeId,

    // This flag is set to true if, during the writeback phase, we encounter
    // a type error in this function.
    writeback_errors: Cell<bool>,

    // Number of errors that had been reported when we started
    // checking this function. On exit, if we find that *more* errors
    // have been reported, we will skip regionck and other work that
    // expects the types within the function to be consistent.
    err_count_on_creation: uint,

    ret_ty: ty::FnOutput<'tcx>,

    ps: RefCell<UnsafetyState>,

    inh: &'a Inherited<'a, 'tcx>,

    ccx: &'a CrateCtxt<'a, 'tcx>,
}

impl<'a, 'tcx> mc::Typer<'tcx> for FnCtxt<'a, 'tcx> {
    fn tcx(&self) -> &ty::ctxt<'tcx> {
        self.ccx.tcx
    }
    fn node_ty(&self, id: ast::NodeId) -> McResult<Ty<'tcx>> {
        let ty = self.node_ty(id);
        self.resolve_type_vars_or_error(&ty)
    }
    fn expr_ty_adjusted(&self, expr: &ast::Expr) -> McResult<Ty<'tcx>> {
        let ty = self.adjust_expr_ty(expr, self.inh.adjustments.borrow().get(&expr.id));
        self.resolve_type_vars_or_error(&ty)
    }
    fn type_moves_by_default(&self, span: Span, ty: Ty<'tcx>) -> bool {
        let ty = self.infcx().resolve_type_vars_if_possible(&ty);
        traits::type_known_to_meet_builtin_bound(self.infcx(), self, ty, ty::BoundCopy, span)
    }
    fn node_method_ty(&self, method_call: ty::MethodCall)
                      -> Option<Ty<'tcx>> {
        self.inh.method_map.borrow()
                           .get(&method_call)
                           .map(|method| method.ty)
                           .map(|ty| self.infcx().resolve_type_vars_if_possible(&ty))
    }
    fn node_method_origin(&self, method_call: ty::MethodCall)
                          -> Option<ty::MethodOrigin<'tcx>>
    {
        self.inh.method_map.borrow()
                           .get(&method_call)
                           .map(|method| method.origin.clone())
    }
    fn adjustments(&self) -> &RefCell<NodeMap<ty::AutoAdjustment<'tcx>>> {
        &self.inh.adjustments
    }
    fn is_method_call(&self, id: ast::NodeId) -> bool {
        self.inh.method_map.borrow().contains_key(&ty::MethodCall::expr(id))
    }
    fn temporary_scope(&self, rvalue_id: ast::NodeId) -> Option<CodeExtent> {
        self.param_env().temporary_scope(rvalue_id)
    }
    fn upvar_borrow(&self, upvar_id: ty::UpvarId) -> Option<ty::UpvarBorrow> {
        self.inh.upvar_borrow_map.borrow().get(&upvar_id).cloned()
    }
    fn capture_mode(&self, closure_expr_id: ast::NodeId)
                    -> ast::CaptureClause {
        self.ccx.tcx.capture_mode(closure_expr_id)
    }
}

impl<'a, 'tcx> ty::UnboxedClosureTyper<'tcx> for FnCtxt<'a, 'tcx> {
    fn param_env<'b>(&'b self) -> &'b ty::ParameterEnvironment<'b,'tcx> {
        &self.inh.param_env
    }

    fn unboxed_closure_kind(&self,
                            def_id: ast::DefId)
                            -> ty::UnboxedClosureKind
    {
        self.inh.unboxed_closures.borrow()[def_id].kind
    }

    fn unboxed_closure_type(&self,
                            def_id: ast::DefId,
                            substs: &subst::Substs<'tcx>)
                            -> ty::ClosureTy<'tcx>
    {
        self.inh.unboxed_closures.borrow()[def_id].closure_type.subst(self.tcx(), substs)
    }

    fn unboxed_closure_upvars(&self,
                              def_id: ast::DefId,
                              substs: &Substs<'tcx>)
                              -> Option<Vec<ty::UnboxedClosureUpvar<'tcx>>>
    {
        ty::unboxed_closure_upvars(self, def_id, substs)
    }
}

impl<'a, 'tcx> Inherited<'a, 'tcx> {
    fn new(tcx: &'a ty::ctxt<'tcx>,
           param_env: ty::ParameterEnvironment<'a, 'tcx>)
           -> Inherited<'a, 'tcx> {
        Inherited {
            infcx: infer::new_infer_ctxt(tcx),
            locals: RefCell::new(NodeMap::new()),
            param_env: param_env,
            node_types: RefCell::new(NodeMap::new()),
            item_substs: RefCell::new(NodeMap::new()),
            adjustments: RefCell::new(NodeMap::new()),
            method_map: RefCell::new(FnvHashMap::new()),
            object_cast_map: RefCell::new(NodeMap::new()),
            upvar_borrow_map: RefCell::new(FnvHashMap::new()),
            unboxed_closures: RefCell::new(DefIdMap::new()),
            fn_sig_map: RefCell::new(NodeMap::new()),
            fulfillment_cx: RefCell::new(traits::FulfillmentContext::new()),
        }
    }

    fn normalize_associated_types_in<T>(&self,
                                        typer: &ty::UnboxedClosureTyper<'tcx>,
                                        span: Span,
                                        body_id: ast::NodeId,
                                        value: &T)
                                        -> T
        where T : TypeFoldable<'tcx> + Clone + HasProjectionTypes + Repr<'tcx>
    {
        let mut fulfillment_cx = self.fulfillment_cx.borrow_mut();
        assoc::normalize_associated_types_in(&self.infcx,
                                             typer,
                                             &mut *fulfillment_cx, span,
                                             body_id,
                                             value)
    }

}

// Used by check_const and check_enum_variants
pub fn blank_fn_ctxt<'a, 'tcx>(ccx: &'a CrateCtxt<'a, 'tcx>,
                               inh: &'a Inherited<'a, 'tcx>,
                               rty: ty::FnOutput<'tcx>,
                               body_id: ast::NodeId)
                               -> FnCtxt<'a, 'tcx> {
    FnCtxt {
        body_id: body_id,
        writeback_errors: Cell::new(false),
        err_count_on_creation: ccx.tcx.sess.err_count(),
        ret_ty: rty,
        ps: RefCell::new(UnsafetyState::function(ast::Unsafety::Normal, 0)),
        inh: inh,
        ccx: ccx
    }
}

fn static_inherited_fields<'a, 'tcx>(ccx: &'a CrateCtxt<'a, 'tcx>)
                                    -> Inherited<'a, 'tcx> {
    // It's kind of a kludge to manufacture a fake function context
    // and statement context, but we might as well do write the code only once
    let param_env = ty::empty_parameter_environment(ccx.tcx);
    Inherited::new(ccx.tcx, param_env)
}

struct CheckItemTypesVisitor<'a, 'tcx: 'a> { ccx: &'a CrateCtxt<'a, 'tcx> }

impl<'a, 'tcx, 'v> Visitor<'v> for CheckItemTypesVisitor<'a, 'tcx> {
    fn visit_item(&mut self, i: &ast::Item) {
        check_item(self.ccx, i);
        visit::walk_item(self, i);
    }

    fn visit_ty(&mut self, t: &ast::Ty) {
        match t.node {
            ast::TyFixedLengthVec(_, ref expr) => {
                check_const_in_type(self.ccx, &**expr, self.ccx.tcx.types.uint);
            }
            _ => {}
        }

        visit::walk_ty(self, t);
    }
}

pub fn check_item_types(ccx: &CrateCtxt) {
    let krate = ccx.tcx.map.krate();
    let mut visit = wf::CheckTypeWellFormedVisitor::new(ccx);
    visit::walk_crate(&mut visit, krate);

    // If types are not well-formed, it leads to all manner of errors
    // downstream, so stop reporting errors at this point.
    ccx.tcx.sess.abort_if_errors();

    let mut visit = CheckItemTypesVisitor { ccx: ccx };
    visit::walk_crate(&mut visit, krate);

    ccx.tcx.sess.abort_if_errors();
}

fn check_bare_fn<'a, 'tcx>(ccx: &CrateCtxt<'a, 'tcx>,
                           decl: &ast::FnDecl,
                           body: &ast::Block,
                           id: ast::NodeId,
                           raw_fty: Ty<'tcx>,
                           param_env: ty::ParameterEnvironment<'a, 'tcx>) {
    match raw_fty.sty {
        ty::ty_bare_fn(_, ref fn_ty) => {
            let inh = Inherited::new(ccx.tcx, param_env);

            // Compute the fty from point of view of inside fn.
            let fn_sig =
                fn_ty.sig.subst(ccx.tcx, &inh.param_env.free_substs);
            let fn_sig =
                liberate_late_bound_regions(ccx.tcx, CodeExtent::from_node_id(body.id), &fn_sig);
            let fn_sig =
                inh.normalize_associated_types_in(&inh.param_env, body.span, body.id, &fn_sig);

            let fcx = check_fn(ccx, fn_ty.unsafety, id, &fn_sig,
                               decl, id, body, &inh);

            vtable::select_all_fcx_obligations_or_error(&fcx);
            upvar::closure_analyze_fn(&fcx, id, decl, body);
            regionck::regionck_fn(&fcx, id, decl, body);
            writeback::resolve_type_vars_in_fn(&fcx, decl, body);
        }
        _ => ccx.tcx.sess.impossible_case(body.span,
                                 "check_bare_fn: function type expected")
    }
}

struct GatherLocalsVisitor<'a, 'tcx: 'a> {
    fcx: &'a FnCtxt<'a, 'tcx>
}

impl<'a, 'tcx> GatherLocalsVisitor<'a, 'tcx> {
    fn assign(&mut self, _span: Span, nid: ast::NodeId, ty_opt: Option<Ty<'tcx>>) -> Ty<'tcx> {
        match ty_opt {
            None => {
                // infer the variable's type
                let var_ty = self.fcx.infcx().next_ty_var();
                self.fcx.inh.locals.borrow_mut().insert(nid, var_ty);
                var_ty
            }
            Some(typ) => {
                // take type that the user specified
                self.fcx.inh.locals.borrow_mut().insert(nid, typ);
                typ
            }
        }
    }
}

impl<'a, 'tcx, 'v> Visitor<'v> for GatherLocalsVisitor<'a, 'tcx> {
    // Add explicitly-declared locals.
    fn visit_local(&mut self, local: &ast::Local) {
        let o_ty = match local.ty {
            Some(ref ty) => Some(self.fcx.to_ty(&**ty)),
            None => None
        };
        self.assign(local.span, local.id, o_ty);
        debug!("Local variable {} is assigned type {}",
               self.fcx.pat_to_string(&*local.pat),
               self.fcx.infcx().ty_to_string(
                   self.fcx.inh.locals.borrow()[local.id].clone()));
        visit::walk_local(self, local);
    }

    // Add pattern bindings.
    fn visit_pat(&mut self, p: &ast::Pat) {
        if let ast::PatIdent(_, ref path1, _) = p.node {
            if pat_util::pat_is_binding(&self.fcx.ccx.tcx.def_map, p) {
                let var_ty = self.assign(p.span, p.id, None);

                self.fcx.require_type_is_sized(var_ty, p.span,
                                               traits::VariableType(p.id));

                debug!("Pattern binding {} is assigned to {} with type {}",
                       token::get_ident(path1.node),
                       self.fcx.infcx().ty_to_string(
                           self.fcx.inh.locals.borrow()[p.id].clone()),
                       var_ty.repr(self.fcx.tcx()));
            }
        }
        visit::walk_pat(self, p);
    }

    fn visit_block(&mut self, b: &ast::Block) {
        // non-obvious: the `blk` variable maps to region lb, so
        // we have to keep this up-to-date.  This
        // is... unfortunate.  It'd be nice to not need this.
        visit::walk_block(self, b);
    }

    // Since an expr occurs as part of the type fixed size arrays we
    // need to record the type for that node
    fn visit_ty(&mut self, t: &ast::Ty) {
        match t.node {
            ast::TyFixedLengthVec(ref ty, ref count_expr) => {
                self.visit_ty(&**ty);
                check_expr_with_hint(self.fcx, &**count_expr, self.fcx.tcx().types.uint);
            }
            _ => visit::walk_ty(self, t)
        }
    }

    // Don't descend into fns and items
    fn visit_fn(&mut self, _: visit::FnKind<'v>, _: &'v ast::FnDecl,
                _: &'v ast::Block, _: Span, _: ast::NodeId) { }
    fn visit_item(&mut self, _: &ast::Item) { }

}

/// Helper used by check_bare_fn and check_expr_fn. Does the grungy work of checking a function
/// body and returns the function context used for that purpose, since in the case of a fn item
/// there is still a bit more to do.
///
/// * ...
/// * inherited: other fields inherited from the enclosing fn (if any)
fn check_fn<'a, 'tcx>(ccx: &'a CrateCtxt<'a, 'tcx>,
                      unsafety: ast::Unsafety,
                      unsafety_id: ast::NodeId,
                      fn_sig: &ty::FnSig<'tcx>,
                      decl: &ast::FnDecl,
                      fn_id: ast::NodeId,
                      body: &ast::Block,
                      inherited: &'a Inherited<'a, 'tcx>)
                      -> FnCtxt<'a, 'tcx>
{
    let tcx = ccx.tcx;
    let err_count_on_creation = tcx.sess.err_count();

    let arg_tys = fn_sig.inputs[];
    let ret_ty = fn_sig.output;

    debug!("check_fn(arg_tys={}, ret_ty={}, fn_id={})",
           arg_tys.repr(tcx),
           ret_ty.repr(tcx),
           fn_id);

    // Create the function context.  This is either derived from scratch or,
    // in the case of function expressions, based on the outer context.
    let fcx = FnCtxt {
        body_id: body.id,
        writeback_errors: Cell::new(false),
        err_count_on_creation: err_count_on_creation,
        ret_ty: ret_ty,
        ps: RefCell::new(UnsafetyState::function(unsafety, unsafety_id)),
        inh: inherited,
        ccx: ccx
    };

    // Remember return type so that regionck can access it later.
    let mut fn_sig_tys: Vec<Ty> =
        arg_tys.iter()
        .map(|&ty| ty)
        .collect();

    if let ty::FnConverging(ret_ty) = ret_ty {
        fcx.require_type_is_sized(ret_ty, decl.output.span(), traits::ReturnType);
        fn_sig_tys.push(ret_ty);
    }

    debug!("fn-sig-map: fn_id={} fn_sig_tys={}",
           fn_id,
           fn_sig_tys.repr(tcx));

    inherited.fn_sig_map.borrow_mut().insert(fn_id, fn_sig_tys);

    {
        let mut visit = GatherLocalsVisitor { fcx: &fcx, };

        // Add formal parameters.
        for (arg_ty, input) in arg_tys.iter().zip(decl.inputs.iter()) {
            // Create type variables for each argument.
            pat_util::pat_bindings(
                &tcx.def_map,
                &*input.pat,
                |_bm, pat_id, sp, _path| {
                    let var_ty = visit.assign(sp, pat_id, None);
                    fcx.require_type_is_sized(var_ty, sp,
                                              traits::VariableType(pat_id));
                });

            // Check the pattern.
            let pcx = pat_ctxt {
                fcx: &fcx,
                map: pat_id_map(&tcx.def_map, &*input.pat),
            };
            _match::check_pat(&pcx, &*input.pat, *arg_ty);
        }

        visit.visit_block(body);
    }

    check_block_with_expected(&fcx, body, match ret_ty {
        ty::FnConverging(result_type) => ExpectHasType(result_type),
        ty::FnDiverging => NoExpectation
    });

    for (input, arg) in decl.inputs.iter().zip(arg_tys.iter()) {
        fcx.write_ty(input.id, *arg);
    }

    fcx
}

pub fn check_struct(ccx: &CrateCtxt, id: ast::NodeId, span: Span) {
    let tcx = ccx.tcx;

    check_representable(tcx, span, id, "struct");
    check_instantiable(tcx, span, id);

    if ty::lookup_simd(tcx, local_def(id)) {
        check_simd(tcx, span, id);
    }
}

pub fn check_item(ccx: &CrateCtxt, it: &ast::Item) {
    debug!("check_item(it.id={}, it.ident={})",
           it.id,
           ty::item_path_str(ccx.tcx, local_def(it.id)));
    let _indenter = indenter();

    match it.node {
      ast::ItemStatic(_, _, ref e) |
      ast::ItemConst(_, ref e) => check_const(ccx, it.span, &**e, it.id),
      ast::ItemEnum(ref enum_definition, _) => {
        check_enum_variants(ccx,
                            it.span,
                            enum_definition.variants[],
                            it.id);
      }
      ast::ItemFn(ref decl, _, _, _, ref body) => {
        let fn_pty = ty::lookup_item_type(ccx.tcx, ast_util::local_def(it.id));
        let param_env = ParameterEnvironment::for_item(ccx.tcx, it.id);
        check_bare_fn(ccx, &**decl, &**body, it.id, fn_pty.ty, param_env);
      }
      ast::ItemImpl(_, _, _, _, _, ref impl_items) => {
        debug!("ItemImpl {} with id {}", token::get_ident(it.ident), it.id);

        let impl_pty = ty::lookup_item_type(ccx.tcx, ast_util::local_def(it.id));

          match ty::impl_trait_ref(ccx.tcx, local_def(it.id)) {
              Some(impl_trait_ref) => {
                check_impl_items_against_trait(ccx,
                                               it.span,
                                               &*impl_trait_ref,
                                               impl_items.as_slice());
              }
              None => { }
          }

        for impl_item in impl_items.iter() {
            match *impl_item {
                ast::MethodImplItem(ref m) => {
                    check_method_body(ccx, &impl_pty.generics, &**m);
                }
                ast::TypeImplItem(_) => {
                    // Nothing to do here.
                }
            }
        }

      }
      ast::ItemTrait(_, _, _, ref trait_methods) => {
        let trait_def = ty::lookup_trait_def(ccx.tcx, local_def(it.id));
        for trait_method in trait_methods.iter() {
            match *trait_method {
                RequiredMethod(..) => {
                    // Nothing to do, since required methods don't have
                    // bodies to check.
                }
                ProvidedMethod(ref m) => {
                    check_method_body(ccx, &trait_def.generics, &**m);
                }
                TypeTraitItem(_) => {
                    // Nothing to do.
                }
            }
        }
      }
      ast::ItemStruct(..) => {
        check_struct(ccx, it.id, it.span);
      }
      ast::ItemTy(ref t, ref generics) => {
        let pty_ty = ty::node_id_to_type(ccx.tcx, it.id);
        check_bounds_are_used(ccx, t.span, &generics.ty_params, pty_ty);
      }
      ast::ItemForeignMod(ref m) => {
        if m.abi == abi::RustIntrinsic {
            for item in m.items.iter() {
                check_intrinsic_type(ccx, &**item);
            }
        } else {
            for item in m.items.iter() {
                let pty = ty::lookup_item_type(ccx.tcx, local_def(item.id));
                if !pty.generics.types.is_empty() {
                    span_err!(ccx.tcx.sess, item.span, E0044,
                        "foreign items may not have type parameters");
                }

                if let ast::ForeignItemFn(ref fn_decl, _) = item.node {
                    if fn_decl.variadic && m.abi != abi::C {
                        span_err!(ccx.tcx.sess, item.span, E0045,
                                  "variadic function must have C calling convention");
                    }
                }
            }
        }
      }
      _ => {/* nothing to do */ }
    }
}

/// Type checks a method body.
///
/// # Parameters
///
/// * `item_generics`: generics defined on the impl/trait that contains
///   the method
/// * `self_bound`: bound for the `Self` type parameter, if any
/// * `method`: the method definition
fn check_method_body<'a, 'tcx>(ccx: &CrateCtxt<'a, 'tcx>,
                               item_generics: &ty::Generics<'tcx>,
                               method: &ast::Method) {
    debug!("check_method_body(item_generics={}, method.id={})",
            item_generics.repr(ccx.tcx),
            method.id);
    let param_env = ParameterEnvironment::for_item(ccx.tcx, method.id);

    let fty = ty::node_id_to_type(ccx.tcx, method.id);
    debug!("check_method_body: fty={}", fty.repr(ccx.tcx));

    check_bare_fn(ccx,
                  &*method.pe_fn_decl(),
                  &*method.pe_body(),
                  method.id,
                  fty,
                  param_env);
}

fn check_impl_items_against_trait<'a, 'tcx>(ccx: &CrateCtxt<'a, 'tcx>,
                                            impl_span: Span,
                                            impl_trait_ref: &ty::TraitRef<'tcx>,
                                            impl_items: &[ast::ImplItem]) {
    // Locate trait methods
    let tcx = ccx.tcx;
    let trait_items = ty::trait_items(tcx, impl_trait_ref.def_id);

    // Check existing impl methods to see if they are both present in trait
    // and compatible with trait signature
    for impl_item in impl_items.iter() {
        match *impl_item {
            ast::MethodImplItem(ref impl_method) => {
                let impl_method_def_id = local_def(impl_method.id);
                let impl_item_ty = ty::impl_or_trait_item(ccx.tcx,
                                                          impl_method_def_id);

                // If this is an impl of a trait method, find the
                // corresponding method definition in the trait.
                let opt_trait_method_ty =
                    trait_items.iter()
                               .find(|ti| ti.name() == impl_item_ty.name());
                match opt_trait_method_ty {
                    Some(trait_method_ty) => {
                        match (trait_method_ty, &impl_item_ty) {
                            (&ty::MethodTraitItem(ref trait_method_ty),
                             &ty::MethodTraitItem(ref impl_method_ty)) => {
                                compare_impl_method(ccx.tcx,
                                                    &**impl_method_ty,
                                                    impl_method.span,
                                                    impl_method.pe_body().id,
                                                    &**trait_method_ty,
                                                    &*impl_trait_ref);
                            }
                            _ => {
                                // This is span_bug as it should have already been
                                // caught in resolve.
                                tcx.sess.span_bug(
                                    impl_method.span,
                                    format!("item `{}` is of a different kind from its trait `{}`",
                                            token::get_name(impl_item_ty.name()),
                                            impl_trait_ref.repr(tcx)).as_slice());
                            }
                        }
                    }
                    None => {
                        // This is span_bug as it should have already been
                        // caught in resolve.
                        tcx.sess.span_bug(
                            impl_method.span,
                            format!("method `{}` is not a member of trait `{}`",
                                    token::get_name(impl_item_ty.name()),
                                    impl_trait_ref.repr(tcx)).as_slice());
                    }
                }
            }
            ast::TypeImplItem(ref typedef) => {
                let typedef_def_id = local_def(typedef.id);
                let typedef_ty = ty::impl_or_trait_item(ccx.tcx,
                                                        typedef_def_id);

                // If this is an impl of an associated type, find the
                // corresponding type definition in the trait.
                let opt_associated_type =
                    trait_items.iter()
                               .find(|ti| ti.name() == typedef_ty.name());
                match opt_associated_type {
                    Some(associated_type) => {
                        match (associated_type, &typedef_ty) {
                            (&ty::TypeTraitItem(_), &ty::TypeTraitItem(_)) => {}
                            _ => {
                                // This is `span_bug` as it should have
                                // already been caught in resolve.
                                tcx.sess.span_bug(
                                    typedef.span,
                                    format!("item `{}` is of a different kind from its trait `{}`",
                                            token::get_name(typedef_ty.name()),
                                            impl_trait_ref.repr(tcx)).as_slice());
                            }
                        }
                    }
                    None => {
                        // This is `span_bug` as it should have already been
                        // caught in resolve.
                        tcx.sess.span_bug(
                            typedef.span,
                            format!(
                                "associated type `{}` is not a member of \
                                 trait `{}`",
                                token::get_name(typedef_ty.name()),
                                impl_trait_ref.repr(tcx)).as_slice());
                    }
                }
            }
        }
    }

    // Check for missing items from trait
    let provided_methods = ty::provided_trait_methods(tcx, impl_trait_ref.def_id);
    let mut missing_methods = Vec::new();
    for trait_item in trait_items.iter() {
        match *trait_item {
            ty::MethodTraitItem(ref trait_method) => {
                let is_implemented =
                    impl_items.iter().any(|ii| {
                        match *ii {
                            ast::MethodImplItem(ref m) => {
                                m.pe_ident().name == trait_method.name
                            }
                            ast::TypeImplItem(_) => false,
                        }
                    });
                let is_provided =
                    provided_methods.iter().any(|m| m.name == trait_method.name);
                if !is_implemented && !is_provided {
                    missing_methods.push(format!("`{}`", token::get_name(trait_method.name)));
                }
            }
            ty::TypeTraitItem(ref associated_type) => {
                let is_implemented = impl_items.iter().any(|ii| {
                    match *ii {
                        ast::TypeImplItem(ref typedef) => {
                            typedef.ident.name == associated_type.name
                        }
                        ast::MethodImplItem(_) => false,
                    }
                });
                if !is_implemented {
                    missing_methods.push(format!("`{}`", token::get_name(associated_type.name)));
                }
            }
        }
    }

    if !missing_methods.is_empty() {
        span_err!(tcx.sess, impl_span, E0046,
            "not all trait items implemented, missing: {}",
            missing_methods.connect(", "));
    }
}

/// Checks that a method from an impl conforms to the signature of
/// the same method as declared in the trait.
///
/// # Parameters
///
/// - impl_generics: the generics declared on the impl itself (not the method!)
/// - impl_m: type of the method we are checking
/// - impl_m_span: span to use for reporting errors
/// - impl_m_body_id: id of the method body
/// - trait_m: the method in the trait
/// - trait_to_impl_substs: the substitutions used on the type of the trait
fn compare_impl_method<'tcx>(tcx: &ty::ctxt<'tcx>,
                             impl_m: &ty::Method<'tcx>,
                             impl_m_span: Span,
                             impl_m_body_id: ast::NodeId,
                             trait_m: &ty::Method<'tcx>,
                             impl_trait_ref: &ty::TraitRef<'tcx>) {
    debug!("compare_impl_method(impl_trait_ref={})",
           impl_trait_ref.repr(tcx));

    debug!("impl_trait_ref (liberated) = {}",
           impl_trait_ref.repr(tcx));

    let infcx = infer::new_infer_ctxt(tcx);
    let mut fulfillment_cx = traits::FulfillmentContext::new();

    let trait_to_impl_substs = &impl_trait_ref.substs;

    // Try to give more informative error messages about self typing
    // mismatches.  Note that any mismatch will also be detected
    // below, where we construct a canonical function type that
    // includes the self parameter as a normal parameter.  It's just
    // that the error messages you get out of this code are a bit more
    // inscrutable, particularly for cases where one method has no
    // self.
    match (&trait_m.explicit_self, &impl_m.explicit_self) {
        (&ty::StaticExplicitSelfCategory,
         &ty::StaticExplicitSelfCategory) => {}
        (&ty::StaticExplicitSelfCategory, _) => {
            tcx.sess.span_err(
                impl_m_span,
                format!("method `{}` has a `{}` declaration in the impl, \
                        but not in the trait",
                        token::get_name(trait_m.name),
                        ppaux::explicit_self_category_to_str(
                            &impl_m.explicit_self))[]);
            return;
        }
        (_, &ty::StaticExplicitSelfCategory) => {
            tcx.sess.span_err(
                impl_m_span,
                format!("method `{}` has a `{}` declaration in the trait, \
                        but not in the impl",
                        token::get_name(trait_m.name),
                        ppaux::explicit_self_category_to_str(
                            &trait_m.explicit_self))[]);
            return;
        }
        _ => {
            // Let the type checker catch other errors below
        }
    }

    let num_impl_m_type_params = impl_m.generics.types.len(subst::FnSpace);
    let num_trait_m_type_params = trait_m.generics.types.len(subst::FnSpace);
    if num_impl_m_type_params != num_trait_m_type_params {
        span_err!(tcx.sess, impl_m_span, E0049,
            "method `{}` has {} type parameter{} \
             but its trait declaration has {} type parameter{}",
            token::get_name(trait_m.name),
            num_impl_m_type_params,
            if num_impl_m_type_params == 1 {""} else {"s"},
            num_trait_m_type_params,
            if num_trait_m_type_params == 1 {""} else {"s"});
        return;
    }

    if impl_m.fty.sig.0.inputs.len() != trait_m.fty.sig.0.inputs.len() {
        span_err!(tcx.sess, impl_m_span, E0050,
            "method `{}` has {} parameter{} \
             but the declaration in trait `{}` has {}",
            token::get_name(trait_m.name),
            impl_m.fty.sig.0.inputs.len(),
            if impl_m.fty.sig.0.inputs.len() == 1 {""} else {"s"},
            ty::item_path_str(tcx, trait_m.def_id),
            trait_m.fty.sig.0.inputs.len());
        return;
    }

    // This code is best explained by example. Consider a trait:
    //
    //     trait Trait<'t,T> {
    //          fn method<'a,M>(t: &'t T, m: &'a M) -> Self;
    //     }
    //
    // And an impl:
    //
    //     impl<'i, 'j, U> Trait<'j, &'i U> for Foo {
    //          fn method<'b,N>(t: &'j &'i U, m: &'b N) -> Foo;
    //     }
    //
    // We wish to decide if those two method types are compatible.
    //
    // We start out with trait_to_impl_substs, that maps the trait
    // type parameters to impl type parameters. This is taken from the
    // impl trait reference:
    //
    //     trait_to_impl_substs = {'t => 'j, T => &'i U, Self => Foo}
    //
    // We create a mapping `dummy_substs` that maps from the impl type
    // parameters to fresh types and regions. For type parameters,
    // this is the identity transform, but we could as well use any
    // skolemized types. For regions, we convert from bound to free
    // regions (Note: but only early-bound regions, i.e., those
    // declared on the impl or used in type parameter bounds).
    //
    //     impl_to_skol_substs = {'i => 'i0, U => U0, N => N0 }
    //
    // Now we can apply skol_substs to the type of the impl method
    // to yield a new function type in terms of our fresh, skolemized
    // types:
    //
    //     <'b> fn(t: &'i0 U0, m: &'b) -> Foo
    //
    // We now want to extract and substitute the type of the *trait*
    // method and compare it. To do so, we must create a compound
    // substitution by combining trait_to_impl_substs and
    // impl_to_skol_substs, and also adding a mapping for the method
    // type parameters. We extend the mapping to also include
    // the method parameters.
    //
    //     trait_to_skol_substs = { T => &'i0 U0, Self => Foo, M => N0 }
    //
    // Applying this to the trait method type yields:
    //
    //     <'a> fn(t: &'i0 U0, m: &'a) -> Foo
    //
    // This type is also the same but the name of the bound region ('a
    // vs 'b).  However, the normal subtyping rules on fn types handle
    // this kind of equivalency just fine.

    // Create mapping from impl to skolemized.
    let impl_param_env = ty::construct_parameter_environment(tcx, &impl_m.generics, impl_m_body_id);
    let impl_to_skol_substs = &impl_param_env.free_substs;

    // Create mapping from trait to skolemized.
    let trait_to_skol_substs =
        trait_to_impl_substs
        .subst(tcx, impl_to_skol_substs)
        .with_method(impl_to_skol_substs.types.get_slice(subst::FnSpace).to_vec(),
                     impl_to_skol_substs.regions().get_slice(subst::FnSpace).to_vec());

    // Check region bounds.
    if !check_region_bounds_on_impl_method(tcx,
                                           impl_m_span,
                                           impl_m,
                                           &trait_m.generics,
                                           &impl_m.generics,
                                           &trait_to_skol_substs,
                                           impl_to_skol_substs) {
        return;
    }

    // Check bounds. Note that the bounds from the impl may reference
    // late-bound regions declared on the impl, so liberate those.
    // This requires two artificial binding scopes -- one for the impl,
    // and one for the method.
    //
    // An example would be:
    //
    //     trait Foo<T> { fn method<U:Bound<T>>() { ... } }
    //
    //     impl<'a> Foo<&'a T> for &'a U {
    //         fn method<U:Bound<&'a T>>() { ... }
    //     }
    //
    // Here, the region parameter `'a` is late-bound, so in the bound
    // `Bound<&'a T>`, the lifetime `'a` will be late-bound with a
    // depth of 3 (it is nested within 3 binders: the impl, method,
    // and trait-ref itself). So when we do the liberation, we have
    // two introduce two `ty::Binder` scopes, one for the impl and one
    // the method.
    //
    // The only late-bounded regions that can possibly appear here are
    // from the impl, not the method. This is because region
    // parameters declared on the method which appear in a type bound
    // would be early bound. On the trait side, there can be no
    // late-bound lifetimes because trait definitions do not introduce
    // a late region binder.
    let trait_bounds =
        trait_m.generics.types.get_slice(subst::FnSpace).iter()
        .map(|trait_param_def| &trait_param_def.bounds);
    let impl_bounds =
        impl_m.generics.types.get_slice(subst::FnSpace).iter()
        .map(|impl_param_def| &impl_param_def.bounds);
    for (i, (trait_param_bounds, impl_param_bounds)) in
        trait_bounds.zip(impl_bounds).enumerate()
    {
        // Check that the impl does not require any builtin-bounds
        // that the trait does not guarantee:
        let extra_bounds =
            impl_param_bounds.builtin_bounds -
            trait_param_bounds.builtin_bounds;
        if !extra_bounds.is_empty() {
            span_err!(tcx.sess, impl_m_span, E0051,
                "in method `{}`, type parameter {} requires `{}`, \
                 which is not required by the corresponding type parameter \
                 in the trait declaration",
                token::get_name(trait_m.name),
                i,
                extra_bounds.user_string(tcx));
           return;
        }

        // Check that the trait bounds of the trait imply the bounds of its
        // implementation.
        //
        // FIXME(pcwalton): We could be laxer here regarding sub- and super-
        // traits, but I doubt that'll be wanted often, so meh.
        for impl_trait_bound in impl_param_bounds.trait_bounds.iter() {
            debug!("compare_impl_method(): impl-trait-bound subst");
            let impl_trait_bound =
                impl_trait_bound.subst(tcx, impl_to_skol_substs);

            // There may be late-bound regions from the impl in the
            // impl's bound, so "liberate" those. Note that the
            // trait_to_skol_substs is derived from the impl's
            // trait-ref, and the late-bound regions appearing there
            // have already been liberated, so the result should match
            // up.

            let found_match_in_trait =
                trait_param_bounds.trait_bounds.iter().any(|trait_bound| {
                    debug!("compare_impl_method(): trait-bound subst");
                    let trait_bound =
                        trait_bound.subst(tcx, &trait_to_skol_substs);
                    infer::mk_sub_poly_trait_refs(&infcx,
                                                  true,
                                                  infer::Misc(impl_m_span),
                                                  trait_bound,
                                                  impl_trait_bound.clone()).is_ok()
                });

            if !found_match_in_trait {
                span_err!(tcx.sess, impl_m_span, E0052,
                          "in method `{}`, type parameter {} requires bound `{}`, which is not \
                           required by the corresponding type parameter in the trait declaration",
                          token::get_name(trait_m.name),
                          i,
                          impl_trait_bound.user_string(tcx));
            }
        }
    }

    // We now need to check that the signature of the impl method is
    // compatible with that of the trait method. We do this by
    // checking that `impl_fty <: trait_fty`.
    //
    // FIXME. Unfortunately, this doesn't quite work right now because
    // associated type normalization is not integrated into subtype
    // checks. For the comparison to be valid, we need to
    // normalize the associated types in the impl/trait methods
    // first. However, because function types bind regions, just
    // calling `normalize_associated_types_in` would have no effect on
    // any associated types appearing in the fn arguments or return
    // type.


    // Compute skolemized form of impl and trait method tys.
    let impl_fty = ty::mk_bare_fn(tcx, None, tcx.mk_bare_fn(impl_m.fty.clone()));
    let impl_fty = impl_fty.subst(tcx, impl_to_skol_substs);
    let trait_fty = ty::mk_bare_fn(tcx, None, tcx.mk_bare_fn(trait_m.fty.clone()));
    let trait_fty = trait_fty.subst(tcx, &trait_to_skol_substs);

    let err = infcx.try(|snapshot| {
        let origin = infer::MethodCompatCheck(impl_m_span);

        let (impl_sig, _) =
            infcx.replace_late_bound_regions_with_fresh_var(impl_m_span,
                                                            infer::HigherRankedType,
                                                            &impl_m.fty.sig);
        let impl_sig =
            impl_sig.subst(tcx, impl_to_skol_substs);
        let impl_sig =
            assoc::normalize_associated_types_in(&infcx,
                                                 &impl_param_env,
                                                 &mut fulfillment_cx,
                                                 impl_m_span,
                                                 impl_m_body_id,
                                                 &impl_sig);
        let impl_fty =
            ty::mk_bare_fn(tcx,
                           None,
                           tcx.mk_bare_fn(ty::BareFnTy { unsafety: impl_m.fty.unsafety,
                                                         abi: impl_m.fty.abi,
                                                         sig: ty::Binder(impl_sig) }));
        debug!("compare_impl_method: impl_fty={}",
               impl_fty.repr(tcx));

        let (trait_sig, skol_map) =
            infcx.skolemize_late_bound_regions(&trait_m.fty.sig, snapshot);
        let trait_sig =
            trait_sig.subst(tcx, &trait_to_skol_substs);
        let trait_sig =
            assoc::normalize_associated_types_in(&infcx,
                                                 &impl_param_env,
                                                 &mut fulfillment_cx,
                                                 impl_m_span,
                                                 impl_m_body_id,
                                                 &trait_sig);
        let trait_fty =
            ty::mk_bare_fn(tcx,
                           None,
                           tcx.mk_bare_fn(ty::BareFnTy { unsafety: trait_m.fty.unsafety,
                                                         abi: trait_m.fty.abi,
                                                         sig: ty::Binder(trait_sig) }));

        debug!("compare_impl_method: trait_fty={}",
               trait_fty.repr(tcx));

        try!(infer::mk_subty(&infcx, false, origin, impl_fty, trait_fty));

        infcx.leak_check(&skol_map, snapshot)
    });

    match err {
        Ok(()) => { }
        Err(terr) => {
            debug!("checking trait method for compatibility: impl ty {}, trait ty {}",
                   impl_fty.repr(tcx),
                   trait_fty.repr(tcx));
            span_err!(tcx.sess, impl_m_span, E0053,
                      "method `{}` has an incompatible type for trait: {}",
                      token::get_name(trait_m.name),
                      ty::type_err_to_str(tcx, &terr));
            return;
        }
    }

    // Run the fulfillment context to completion to accommodate any
    // associated type normalizations that may have occurred.
    match fulfillment_cx.select_all_or_error(&infcx, &impl_param_env) {
        Ok(()) => { }
        Err(errors) => {
            traits::report_fulfillment_errors(&infcx, &errors);
        }
    }

    // Finally, resolve all regions. This catches wily misuses of lifetime
    // parameters.
    infcx.resolve_regions_and_report_errors(impl_m_body_id);

    /// Check that region bounds on impl method are the same as those on the trait. In principle,
    /// it could be ok for there to be fewer region bounds on the impl method, but this leads to an
    /// annoying corner case that is painful to handle (described below), so for now we can just
    /// forbid it.
    ///
    /// Example (see `src/test/compile-fail/regions-bound-missing-bound-in-impl.rs`):
    ///
    /// ```
    /// trait Foo<'a> {
    ///     fn method1<'b>();
    ///     fn method2<'b:'a>();
    /// }
    ///
    /// impl<'a> Foo<'a> for ... {
    ///     fn method1<'b:'a>() { .. case 1, definitely bad .. }
    ///     fn method2<'b>() { .. case 2, could be ok .. }
    /// }
    /// ```
    ///
    /// The "definitely bad" case is case #1. Here, the impl adds an extra constraint not present
    /// in the trait.
    ///
    /// The "maybe bad" case is case #2. Here, the impl adds an extra constraint not present in the
    /// trait. We could in principle allow this, but it interacts in a complex way with early/late
    /// bound resolution of lifetimes. Basically the presence or absence of a lifetime bound
    /// affects whether the lifetime is early/late bound, and right now the code breaks if the
    /// trait has an early bound lifetime parameter and the method does not.
    fn check_region_bounds_on_impl_method<'tcx>(tcx: &ty::ctxt<'tcx>,
                                                span: Span,
                                                impl_m: &ty::Method<'tcx>,
                                                trait_generics: &ty::Generics<'tcx>,
                                                impl_generics: &ty::Generics<'tcx>,
                                                trait_to_skol_substs: &Substs<'tcx>,
                                                impl_to_skol_substs: &Substs<'tcx>)
                                                -> bool
    {

        let trait_params = trait_generics.regions.get_slice(subst::FnSpace);
        let impl_params = impl_generics.regions.get_slice(subst::FnSpace);

        debug!("check_region_bounds_on_impl_method: \
               trait_generics={} \
               impl_generics={} \
               trait_to_skol_substs={} \
               impl_to_skol_substs={}",
               trait_generics.repr(tcx),
               impl_generics.repr(tcx),
               trait_to_skol_substs.repr(tcx),
               impl_to_skol_substs.repr(tcx));

        // Must have same number of early-bound lifetime parameters.
        // Unfortunately, if the user screws up the bounds, then this
        // will change classification between early and late.  E.g.,
        // if in trait we have `<'a,'b:'a>`, and in impl we just have
        // `<'a,'b>`, then we have 2 early-bound lifetime parameters
        // in trait but 0 in the impl. But if we report "expected 2
        // but found 0" it's confusing, because it looks like there
        // are zero. Since I don't quite know how to phrase things at
        // the moment, give a kind of vague error message.
        if trait_params.len() != impl_params.len() {
            tcx.sess.span_err(
                span,
                format!("lifetime parameters or bounds on method `{}` do \
                         not match the trait declaration",
                        token::get_name(impl_m.name))[]);
            return false;
        }

        // Each parameter `'a:'b+'c+'d` in trait should have the same
        // set of bounds in the impl, after subst.
        for (trait_param, impl_param) in
            trait_params.iter().zip(
                impl_params.iter())
        {
            let trait_bounds =
                trait_param.bounds.subst(tcx, trait_to_skol_substs);
            let impl_bounds =
                impl_param.bounds.subst(tcx, impl_to_skol_substs);

            debug!("check_region_bounds_on_impl_method: \
                   trait_param={} \
                   impl_param={} \
                   trait_bounds={} \
                   impl_bounds={}",
                   trait_param.repr(tcx),
                   impl_param.repr(tcx),
                   trait_bounds.repr(tcx),
                   impl_bounds.repr(tcx));

            // Collect the set of bounds present in trait but not in
            // impl.
            let missing: Vec<ty::Region> =
                trait_bounds.iter()
                .filter(|&b| !impl_bounds.contains(b))
                .map(|&b| b)
                .collect();

            // Collect set present in impl but not in trait.
            let extra: Vec<ty::Region> =
                impl_bounds.iter()
                .filter(|&b| !trait_bounds.contains(b))
                .map(|&b| b)
                .collect();

            debug!("missing={} extra={}",
                   missing.repr(tcx), extra.repr(tcx));

            let err = if missing.len() != 0 || extra.len() != 0 {
                tcx.sess.span_err(
                    span,
                    format!(
                        "the lifetime parameter `{}` declared in the impl \
                         has a distinct set of bounds \
                         from its counterpart `{}` \
                         declared in the trait",
                        impl_param.name.user_string(tcx),
                        trait_param.name.user_string(tcx))[]);
                true
            } else {
                false
            };

            if missing.len() != 0 {
                tcx.sess.span_note(
                    span,
                    format!("the impl is missing the following bounds: `{}`",
                            missing.user_string(tcx))[]);
            }

            if extra.len() != 0 {
                tcx.sess.span_note(
                    span,
                    format!("the impl has the following extra bounds: `{}`",
                            extra.user_string(tcx))[]);
            }

            if err {
                return false;
            }
        }

        return true;
    }
}

fn check_cast(fcx: &FnCtxt,
              cast_expr: &ast::Expr,
              e: &ast::Expr,
              t: &ast::Ty) {
    let id = cast_expr.id;
    let span = cast_expr.span;

    // Find the type of `e`. Supply hints based on the type we are casting to,
    // if appropriate.
    let t_1 = fcx.to_ty(t);
    let t_1 = structurally_resolved_type(fcx, span, t_1);

    check_expr_with_expectation(fcx, e, ExpectCastableToType(t_1));

    let t_e = fcx.expr_ty(e);

    debug!("t_1={}", fcx.infcx().ty_to_string(t_1));
    debug!("t_e={}", fcx.infcx().ty_to_string(t_e));

    if ty::type_is_error(t_e) {
        fcx.write_error(id);
        return
    }

    if !fcx.type_is_known_to_be_sized(t_1, cast_expr.span) {
        let tstr = fcx.infcx().ty_to_string(t_1);
        fcx.type_error_message(span, |actual| {
            format!("cast to unsized type: `{}` as `{}`", actual, tstr)
        }, t_e, None);
        match t_e.sty {
            ty::ty_rptr(_, ty::mt { mutbl: mt, .. }) => {
                let mtstr = match mt {
                    ast::MutMutable => "mut ",
                    ast::MutImmutable => ""
                };
                if ty::type_is_trait(t_1) {
                    span_help!(fcx.tcx().sess, t.span, "did you mean `&{}{}`?", mtstr, tstr);
                } else {
                    span_help!(fcx.tcx().sess, span,
                               "consider using an implicit coercion to `&{}{}` instead",
                               mtstr, tstr);
                }
            }
            ty::ty_uniq(..) => {
                span_help!(fcx.tcx().sess, t.span, "did you mean `Box<{}>`?", tstr);
            }
            _ => {
                span_help!(fcx.tcx().sess, e.span,
                           "consider using a box or reference as appropriate");
            }
        }
        fcx.write_error(id);
        return
    }

    if ty::type_is_trait(t_1) {
        // This will be looked up later on.
        vtable::check_object_cast(fcx, cast_expr, e, t_1);
        fcx.write_ty(id, t_1);
        return
    }

    let t_1 = structurally_resolved_type(fcx, span, t_1);
    let t_e = structurally_resolved_type(fcx, span, t_e);

    if ty::type_is_nil(t_e) {
        fcx.type_error_message(span, |actual| {
            format!("cast from nil: `{}` as `{}`",
                    actual,
                    fcx.infcx().ty_to_string(t_1))
        }, t_e, None);
    } else if ty::type_is_nil(t_1) {
        fcx.type_error_message(span, |actual| {
            format!("cast to nil: `{}` as `{}`",
                    actual,
                    fcx.infcx().ty_to_string(t_1))
        }, t_e, None);
    }

    let t_e_is_bare_fn_item = ty::type_is_bare_fn_item(t_e);

    let t_1_is_scalar = ty::type_is_scalar(t_1);
    let t_1_is_char = ty::type_is_char(t_1);
    let t_1_is_bare_fn = ty::type_is_bare_fn(t_1);
    let t_1_is_float = ty::type_is_floating_point(t_1);

    // casts to scalars other than `char` and `bare fn` are trivial
    let t_1_is_trivial = t_1_is_scalar && !t_1_is_char && !t_1_is_bare_fn;
    if t_e_is_bare_fn_item && t_1_is_bare_fn {
        demand::coerce(fcx, e.span, t_1, &*e);
    } else if ty::type_is_c_like_enum(fcx.tcx(), t_e) && t_1_is_trivial {
        if t_1_is_float || ty::type_is_unsafe_ptr(t_1) {
            fcx.type_error_message(span, |actual| {
                format!("illegal cast; cast through an \
                         integer first: `{}` as `{}`",
                        actual,
                        fcx.infcx().ty_to_string(t_1))
            }, t_e, None);
        }
        // casts from C-like enums are allowed
    } else if t_1_is_char {
        let t_e = fcx.infcx().shallow_resolve(t_e);
        if t_e.sty != ty::ty_uint(ast::TyU8) {
            fcx.type_error_message(span, |actual| {
                format!("only `u8` can be cast as \
                         `char`, not `{}`", actual)
            }, t_e, None);
        }
    } else if t_1.sty == ty::ty_bool {
        span_err!(fcx.tcx().sess, span, E0054,
            "cannot cast as `bool`, compare with zero instead");
    } else if ty::type_is_region_ptr(t_e) && ty::type_is_unsafe_ptr(t_1) {
        fn types_compatible<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>, sp: Span,
                                      t1: Ty<'tcx>, t2: Ty<'tcx>) -> bool {
            match t1.sty {
                ty::ty_vec(_, Some(_)) => {}
                _ => return false
            }
            if ty::type_needs_infer(t2) {
                // This prevents this special case from going off when casting
                // to a type that isn't fully specified; e.g. `as *_`. (Issue
                // #14893.)
                return false
            }

            let el = ty::sequence_element_type(fcx.tcx(), t1);
            infer::mk_eqty(fcx.infcx(),
                           false,
                           infer::Misc(sp),
                           el,
                           t2).is_ok()
        }

        // Due to the limitations of LLVM global constants,
        // region pointers end up pointing at copies of
        // vector elements instead of the original values.
        // To allow unsafe pointers to work correctly, we
        // need to special-case obtaining an unsafe pointer
        // from a region pointer to a vector.

        /* this cast is only allowed from &[T, ..n] to *T or
        &T to *T. */
        match (&t_e.sty, &t_1.sty) {
            (&ty::ty_rptr(_, ty::mt { ty: mt1, mutbl: ast::MutImmutable }),
             &ty::ty_ptr(ty::mt { ty: mt2, mutbl: ast::MutImmutable }))
            if types_compatible(fcx, e.span, mt1, mt2) => {
                /* this case is allowed */
            }
            _ => {
                demand::coerce(fcx, e.span, t_1, &*e);
            }
        }
    } else if !(ty::type_is_scalar(t_e) && t_1_is_trivial) {
        /*
        If more type combinations should be supported than are
        supported here, then file an enhancement issue and
        record the issue number in this comment.
        */
        fcx.type_error_message(span, |actual| {
            format!("non-scalar cast: `{}` as `{}`",
                    actual,
                    fcx.infcx().ty_to_string(t_1))
        }, t_e, None);
    } else if ty::type_is_unsafe_ptr(t_e) && t_1_is_float {
        fcx.type_error_message(span, |actual| {
            format!("cannot cast from pointer to float directly: `{}` as `{}`; cast through an \
                     integer first",
                    actual,
                    fcx.infcx().ty_to_string(t_1))
        }, t_e, None);
    }

    fcx.write_ty(id, t_1);
}

impl<'a, 'tcx> AstConv<'tcx> for FnCtxt<'a, 'tcx> {
    fn tcx(&self) -> &ty::ctxt<'tcx> { self.ccx.tcx }

    fn get_item_type_scheme(&self, id: ast::DefId) -> ty::TypeScheme<'tcx> {
        ty::lookup_item_type(self.tcx(), id)
    }

    fn get_trait_def(&self, id: ast::DefId) -> Rc<ty::TraitDef<'tcx>> {
        ty::lookup_trait_def(self.tcx(), id)
    }

    fn get_free_substs(&self) -> Option<&Substs<'tcx>> {
        Some(&self.inh.param_env.free_substs)
    }

    fn ty_infer(&self, _span: Span) -> Ty<'tcx> {
        self.infcx().next_ty_var()
    }

    fn projected_ty_from_poly_trait_ref(&self,
                                        span: Span,
                                        poly_trait_ref: ty::PolyTraitRef<'tcx>,
                                        item_name: ast::Name)
                                        -> Ty<'tcx>
    {
        let (trait_ref, _) =
            self.infcx().replace_late_bound_regions_with_fresh_var(
                span,
                infer::LateBoundRegionConversionTime::AssocTypeProjection(item_name),
                &poly_trait_ref);

        self.normalize_associated_type(span, trait_ref, item_name)
    }

    fn projected_ty(&self,
                    span: Span,
                    trait_ref: Rc<ty::TraitRef<'tcx>>,
                    item_name: ast::Name)
                    -> Ty<'tcx>
    {
        self.normalize_associated_type(span, trait_ref, item_name)
    }
}

impl<'a, 'tcx> FnCtxt<'a, 'tcx> {
    fn tcx(&self) -> &ty::ctxt<'tcx> { self.ccx.tcx }

    pub fn infcx(&self) -> &infer::InferCtxt<'a,'tcx> {
        &self.inh.infcx
    }

    pub fn param_env(&self) -> &ty::ParameterEnvironment<'a,'tcx> {
        &self.inh.param_env
    }

    pub fn sess(&self) -> &Session {
        &self.tcx().sess
    }

    pub fn err_count_since_creation(&self) -> uint {
        self.ccx.tcx.sess.err_count() - self.err_count_on_creation
    }

    /// Resolves all type variables in `t` and then, if any were left
    /// unresolved, substitutes an error type. This is used after the
    /// main checking when doing a second pass before writeback. The
    /// justification is that writeback will produce an error for
    /// these unconstrained type variables.
    fn resolve_type_vars_or_error(&self, t: &Ty<'tcx>) -> mc::McResult<Ty<'tcx>> {
        let t = self.infcx().resolve_type_vars_if_possible(t);
        if ty::type_has_ty_infer(t) || ty::type_is_error(t) { Err(()) } else { Ok(t) }
    }

    pub fn tag(&self) -> String {
        format!("{}", self as *const FnCtxt)
    }

    pub fn local_ty(&self, span: Span, nid: ast::NodeId) -> Ty<'tcx> {
        match self.inh.locals.borrow().get(&nid) {
            Some(&t) => t,
            None => {
                self.tcx().sess.span_bug(
                    span,
                    format!("no type for local variable {}",
                            nid)[]);
            }
        }
    }

    /// Apply "fallbacks" to some types
    /// ! gets replaced with (), unconstrained ints with i32, and unconstrained floats with f64.
    pub fn default_type_parameters(&self) {
        use middle::ty::UnconstrainedNumeric::{UnconstrainedInt, UnconstrainedFloat, Neither};
        for (_, &ref ty) in self.inh.node_types.borrow_mut().iter_mut() {
            let resolved = self.infcx().resolve_type_vars_if_possible(ty);
            if self.infcx().type_var_diverges(resolved) {
                demand::eqtype(self, codemap::DUMMY_SP, *ty, ty::mk_nil(self.tcx()));
            } else {
                match self.infcx().type_is_unconstrained_numeric(resolved) {
                    UnconstrainedInt => {
                        demand::eqtype(self, codemap::DUMMY_SP, *ty, self.tcx().types.i32)
                    },
                    UnconstrainedFloat => {
                        demand::eqtype(self, codemap::DUMMY_SP, *ty, self.tcx().types.f64)
                    }
                    Neither => { }
                }
            }
        }
    }

    #[inline]
    pub fn write_ty(&self, node_id: ast::NodeId, ty: Ty<'tcx>) {
        debug!("write_ty({}, {}) in fcx {}",
               node_id, ppaux::ty_to_string(self.tcx(), ty), self.tag());
        self.inh.node_types.borrow_mut().insert(node_id, ty);
    }

    pub fn write_object_cast(&self,
                             key: ast::NodeId,
                             trait_ref: ty::PolyTraitRef<'tcx>) {
        debug!("write_object_cast key={} trait_ref={}",
               key, trait_ref.repr(self.tcx()));
        self.inh.object_cast_map.borrow_mut().insert(key, trait_ref);
    }

    pub fn write_substs(&self, node_id: ast::NodeId, substs: ty::ItemSubsts<'tcx>) {
        if !substs.substs.is_noop() {
            debug!("write_substs({}, {}) in fcx {}",
                   node_id,
                   substs.repr(self.tcx()),
                   self.tag());

            self.inh.item_substs.borrow_mut().insert(node_id, substs);
        }
    }

    pub fn write_autoderef_adjustment(&self,
                                      node_id: ast::NodeId,
                                      span: Span,
                                      derefs: uint) {
        if derefs == 0 { return; }
        self.write_adjustment(
            node_id,
            span,
            ty::AdjustDerefRef(ty::AutoDerefRef {
                autoderefs: derefs,
                autoref: None })
        );
    }

    pub fn write_adjustment(&self,
                            node_id: ast::NodeId,
                            span: Span,
                            adj: ty::AutoAdjustment<'tcx>) {
        debug!("write_adjustment(node_id={}, adj={})", node_id, adj.repr(self.tcx()));

        if adj.is_identity() {
            return;
        }

        // Careful: adjustments can imply trait obligations if we are
        // casting from a concrete type to an object type. I think
        // it'd probably be nicer to move the logic that creates the
        // obligation into the code that creates the adjustment, but
        // that's a bit awkward, so instead we go digging and pull the
        // obligation out here.
        self.register_adjustment_obligations(span, &adj);
        self.inh.adjustments.borrow_mut().insert(node_id, adj);
    }

    /// Basically whenever we are converting from a type scheme into
    /// the fn body space, we always want to normalize associated
    /// types as well. This function combines the two.
    fn instantiate_type_scheme<T>(&self,
                                  span: Span,
                                  substs: &Substs<'tcx>,
                                  value: &T)
                                  -> T
        where T : TypeFoldable<'tcx> + Clone + HasProjectionTypes + Repr<'tcx>
    {
        let value = value.subst(self.tcx(), substs);
        let result = self.normalize_associated_types_in(span, &value);
        debug!("instantiate_type_scheme(value={}, substs={}) = {}",
               value.repr(self.tcx()),
               substs.repr(self.tcx()),
               result.repr(self.tcx()));
        result
    }

    /// As `instantiate_type_scheme`, but for the bounds found in a
    /// generic type scheme.
    fn instantiate_bounds(&self,
                          span: Span,
                          substs: &Substs<'tcx>,
                          generics: &ty::Generics<'tcx>)
                          -> ty::GenericBounds<'tcx>
    {
        ty::GenericBounds {
            predicates: self.instantiate_type_scheme(span, substs, &generics.predicates)
        }
    }


    fn normalize_associated_types_in<T>(&self, span: Span, value: &T) -> T
        where T : TypeFoldable<'tcx> + Clone + HasProjectionTypes + Repr<'tcx>
    {
        self.inh.normalize_associated_types_in(self, span, self.body_id, value)
    }

    fn normalize_associated_type(&self,
                                 span: Span,
                                 trait_ref: Rc<ty::TraitRef<'tcx>>,
                                 item_name: ast::Name)
                                 -> Ty<'tcx>
    {
        let cause = traits::ObligationCause::new(span,
                                                 self.body_id,
                                                 traits::ObligationCauseCode::MiscObligation);
        self.inh.fulfillment_cx
            .borrow_mut()
            .normalize_projection_type(self.infcx(),
                                       self,
                                       ty::ProjectionTy {
                                           trait_ref: trait_ref,
                                           item_name: item_name,
                                       },
                                       cause)
    }

    fn register_adjustment_obligations(&self,
                                       span: Span,
                                       adj: &ty::AutoAdjustment<'tcx>) {
        match *adj {
            ty::AdjustReifyFnPointer(..) => {
            }
            ty::AdjustDerefRef(ref d_r) => {
                match d_r.autoref {
                    Some(ref a_r) => {
                        self.register_autoref_obligations(span, a_r);
                    }
                    None => {}
                }
            }
        }
    }

    fn register_autoref_obligations(&self,
                                    span: Span,
                                    autoref: &ty::AutoRef<'tcx>) {
        match *autoref {
            ty::AutoUnsize(ref unsize) => {
                self.register_unsize_obligations(span, unsize);
            }
            ty::AutoPtr(_, _, None) |
            ty::AutoUnsafe(_, None) => {
            }
            ty::AutoPtr(_, _, Some(ref a_r)) |
            ty::AutoUnsafe(_, Some(ref a_r)) => {
                self.register_autoref_obligations(span, &**a_r)
            }
            ty::AutoUnsizeUniq(ref unsize) => {
                self.register_unsize_obligations(span, unsize);
            }
        }
    }

    fn register_unsize_obligations(&self,
                                   span: Span,
                                   unsize: &ty::UnsizeKind<'tcx>) {
        debug!("register_unsize_obligations: unsize={}", unsize);

        match *unsize {
            ty::UnsizeLength(..) => {}
            ty::UnsizeStruct(ref u, _) => {
                self.register_unsize_obligations(span, &**u)
            }
            ty::UnsizeVtable(ref ty_trait, self_ty) => {
                vtable::check_object_safety(self.tcx(), ty_trait, span);

                // If the type is `Foo+'a`, ensures that the type
                // being cast to `Foo+'a` implements `Foo`:
                vtable::register_object_cast_obligations(self,
                                                         span,
                                                         ty_trait,
                                                         self_ty);

                // If the type is `Foo+'a`, ensures that the type
                // being cast to `Foo+'a` outlives `'a`:
                let cause = traits::ObligationCause { span: span,
                                                      body_id: self.body_id,
                                                      code: traits::ObjectCastObligation(self_ty) };
                self.register_region_obligation(self_ty, ty_trait.bounds.region_bound, cause);
            }
        }
    }

    /// Returns the type of `def_id` with all generics replaced by by fresh type/region variables.
    /// Also returns the substitution from the type parameters on `def_id` to the fresh variables.
    /// Registers any trait obligations specified on `def_id` at the same time.
    ///
    /// Note that function is only intended to be used with types (notably, not fns). This is
    /// because it doesn't do any instantiation of late-bound regions.
    pub fn instantiate_type(&self,
                            span: Span,
                            def_id: ast::DefId)
                            -> TypeAndSubsts<'tcx>
    {
        let type_scheme =
            ty::lookup_item_type(self.tcx(), def_id);
        let substs =
            self.infcx().fresh_substs_for_generics(
                span,
                &type_scheme.generics);
        let bounds =
            self.instantiate_bounds(span, &substs, &type_scheme.generics);
        self.add_obligations_for_parameters(
            traits::ObligationCause::new(
                span,
                self.body_id,
                traits::ItemObligation(def_id)),
            &bounds);
        let monotype =
            self.instantiate_type_scheme(span, &substs, &type_scheme.ty);

        TypeAndSubsts {
            ty: monotype,
            substs: substs
        }
    }

    pub fn write_nil(&self, node_id: ast::NodeId) {
        self.write_ty(node_id, ty::mk_nil(self.tcx()));
    }
    pub fn write_error(&self, node_id: ast::NodeId) {
        self.write_ty(node_id, self.tcx().types.err);
    }

    pub fn require_type_meets(&self,
                              ty: Ty<'tcx>,
                              span: Span,
                              code: traits::ObligationCauseCode<'tcx>,
                              bound: ty::BuiltinBound)
    {
        self.register_builtin_bound(
            ty,
            bound,
            traits::ObligationCause::new(span, self.body_id, code));
    }

    pub fn require_type_is_sized(&self,
                                 ty: Ty<'tcx>,
                                 span: Span,
                                 code: traits::ObligationCauseCode<'tcx>)
    {
        self.require_type_meets(ty, span, code, ty::BoundSized);
    }

    pub fn require_expr_have_sized_type(&self,
                                        expr: &ast::Expr,
                                        code: traits::ObligationCauseCode<'tcx>)
    {
        self.require_type_is_sized(self.expr_ty(expr), expr.span, code);
    }

    pub fn type_is_known_to_be_sized(&self,
                                     ty: Ty<'tcx>,
                                     span: Span)
                                     -> bool
    {
        traits::type_known_to_meet_builtin_bound(self.infcx(),
                                                 self.param_env(),
                                                 ty,
                                                 ty::BoundSized,
                                                 span)
    }

    pub fn register_builtin_bound(&self,
                                  ty: Ty<'tcx>,
                                  builtin_bound: ty::BuiltinBound,
                                  cause: traits::ObligationCause<'tcx>)
    {
        self.inh.fulfillment_cx.borrow_mut()
            .register_builtin_bound(self.infcx(), ty, builtin_bound, cause);
    }

    pub fn register_predicate(&self,
                              obligation: traits::PredicateObligation<'tcx>)
    {
        debug!("register_predicate({})",
               obligation.repr(self.tcx()));

        self.inh.fulfillment_cx
            .borrow_mut()
            .register_predicate_obligation(self.infcx(), obligation);
    }

    pub fn to_ty(&self, ast_t: &ast::Ty) -> Ty<'tcx> {
        let t = ast_ty_to_ty(self, self, ast_t);

        let mut bounds_checker = wf::BoundsChecker::new(self,
                                                        ast_t.span,
                                                        CodeExtent::from_node_id(self.body_id),
                                                        None);
        bounds_checker.check_ty(t);

        t
    }

    pub fn pat_to_string(&self, pat: &ast::Pat) -> String {
        pat.repr(self.tcx())
    }

    pub fn expr_ty(&self, ex: &ast::Expr) -> Ty<'tcx> {
        match self.inh.node_types.borrow().get(&ex.id) {
            Some(&t) => t,
            None => {
                self.tcx().sess.bug(format!("no type for expr in fcx {}",
                                            self.tag())[]);
            }
        }
    }

    /// Apply `adjustment` to the type of `expr`
    pub fn adjust_expr_ty(&self,
                          expr: &ast::Expr,
                          adjustment: Option<&ty::AutoAdjustment<'tcx>>)
                          -> Ty<'tcx>
    {
        let raw_ty = self.expr_ty(expr);
        let raw_ty = self.infcx().shallow_resolve(raw_ty);
        ty::adjust_ty(self.tcx(),
                      expr.span,
                      expr.id,
                      raw_ty,
                      adjustment,
                      |method_call| self.inh.method_map.borrow()
                                                       .get(&method_call)
                                                       .map(|method| method.ty))
    }

    pub fn node_ty(&self, id: ast::NodeId) -> Ty<'tcx> {
        match self.inh.node_types.borrow().get(&id) {
            Some(&t) => t,
            None => {
                self.tcx().sess.bug(
                    format!("no type for node {}: {} in fcx {}",
                            id, self.tcx().map.node_to_string(id),
                            self.tag())[]);
            }
        }
    }

    pub fn item_substs(&self) -> Ref<NodeMap<ty::ItemSubsts<'tcx>>> {
        self.inh.item_substs.borrow()
    }

    pub fn opt_node_ty_substs<F>(&self,
                                 id: ast::NodeId,
                                 f: F) where
        F: FnOnce(&ty::ItemSubsts<'tcx>),
    {
        match self.inh.item_substs.borrow().get(&id) {
            Some(s) => { f(s) }
            None => { }
        }
    }

    pub fn mk_subty(&self,
                    a_is_expected: bool,
                    origin: infer::TypeOrigin,
                    sub: Ty<'tcx>,
                    sup: Ty<'tcx>)
                    -> Result<(), ty::type_err<'tcx>> {
        infer::mk_subty(self.infcx(), a_is_expected, origin, sub, sup)
    }

    pub fn mk_assignty(&self,
                       expr: &ast::Expr,
                       sub: Ty<'tcx>,
                       sup: Ty<'tcx>)
                       -> Result<(), ty::type_err<'tcx>> {
        match infer::mk_coercety(self.infcx(),
                                 false,
                                 infer::ExprAssignable(expr.span),
                                 sub,
                                 sup) {
            Ok(None) => Ok(()),
            Err(ref e) => Err((*e)),
            Ok(Some(adjustment)) => {
                self.write_adjustment(expr.id, expr.span, adjustment);
                Ok(())
            }
        }
    }

    pub fn mk_eqty(&self,
                   a_is_expected: bool,
                   origin: infer::TypeOrigin,
                   sub: Ty<'tcx>,
                   sup: Ty<'tcx>)
                   -> Result<(), ty::type_err<'tcx>> {
        infer::mk_eqty(self.infcx(), a_is_expected, origin, sub, sup)
    }

    pub fn mk_subr(&self,
                   origin: infer::SubregionOrigin<'tcx>,
                   sub: ty::Region,
                   sup: ty::Region) {
        infer::mk_subr(self.infcx(), origin, sub, sup)
    }

    pub fn type_error_message<M>(&self,
                                 sp: Span,
                                 mk_msg: M,
                                 actual_ty: Ty<'tcx>,
                                 err: Option<&ty::type_err<'tcx>>) where
        M: FnOnce(String) -> String,
    {
        self.infcx().type_error_message(sp, mk_msg, actual_ty, err);
    }

    pub fn report_mismatched_types(&self,
                                   sp: Span,
                                   e: Ty<'tcx>,
                                   a: Ty<'tcx>,
                                   err: &ty::type_err<'tcx>) {
        self.infcx().report_mismatched_types(sp, e, a, err)
    }

    /// Registers an obligation for checking later, during regionck, that the type `ty` must
    /// outlive the region `r`.
    pub fn register_region_obligation(&self,
                                      ty: Ty<'tcx>,
                                      region: ty::Region,
                                      cause: traits::ObligationCause<'tcx>)
    {
        let mut fulfillment_cx = self.inh.fulfillment_cx.borrow_mut();
        fulfillment_cx.register_region_obligation(self.infcx(), ty, region, cause);
    }

    pub fn add_default_region_param_bounds(&self,
                                           substs: &Substs<'tcx>,
                                           expr: &ast::Expr)
    {
        for &ty in substs.types.iter() {
            let default_bound = ty::ReScope(CodeExtent::from_node_id(expr.id));
            let cause = traits::ObligationCause::new(expr.span, self.body_id,
                                                     traits::MiscObligation);
            self.register_region_obligation(ty, default_bound, cause);
        }
    }

    /// Given a fully substituted set of bounds (`generic_bounds`), and the values with which each
    /// type/region parameter was instantiated (`substs`), creates and registers suitable
    /// trait/region obligations.
    ///
    /// For example, if there is a function:
    ///
    /// ```
    /// fn foo<'a,T:'a>(...)
    /// ```
    ///
    /// and a reference:
    ///
    /// ```
    /// let f = foo;
    /// ```
    ///
    /// Then we will create a fresh region variable `'$0` and a fresh type variable `$1` for `'a`
    /// and `T`. This routine will add a region obligation `$1:'$0` and register it locally.
    pub fn add_obligations_for_parameters(&self,
                                          cause: traits::ObligationCause<'tcx>,
                                          generic_bounds: &ty::GenericBounds<'tcx>)
    {
        assert!(!generic_bounds.has_escaping_regions());

        debug!("add_obligations_for_parameters(generic_bounds={})",
               generic_bounds.repr(self.tcx()));

        let obligations = traits::predicates_for_generics(self.tcx(),
                                                          cause,
                                                          generic_bounds);

        obligations.map_move(|o| self.register_predicate(o));
    }
}

impl<'a, 'tcx> RegionScope for FnCtxt<'a, 'tcx> {
    fn default_region_bound(&self, span: Span) -> Option<ty::Region> {
        Some(self.infcx().next_region_var(infer::MiscVariable(span)))
    }

    fn anon_regions(&self, span: Span, count: uint)
                    -> Result<Vec<ty::Region>, Option<Vec<(String, uint)>>> {
        Ok(range(0, count).map(|_| {
            self.infcx().next_region_var(infer::MiscVariable(span))
        }).collect())
    }
}

#[derive(Copy, Show, PartialEq, Eq)]
pub enum LvaluePreference {
    PreferMutLvalue,
    NoPreference
}

/// Executes an autoderef loop for the type `t`. At each step, invokes `should_stop` to decide
/// whether to terminate the loop. Returns the final type and number of derefs that it performed.
///
/// Note: this method does not modify the adjustments table. The caller is responsible for
/// inserting an AutoAdjustment record into the `fcx` using one of the suitable methods.
pub fn autoderef<'a, 'tcx, T, F>(fcx: &FnCtxt<'a, 'tcx>,
                                 sp: Span,
                                 base_ty: Ty<'tcx>,
                                 expr_id: Option<ast::NodeId>,
                                 mut lvalue_pref: LvaluePreference,
                                 mut should_stop: F)
                                 -> (Ty<'tcx>, uint, Option<T>) where
    F: FnMut(Ty<'tcx>, uint) -> Option<T>,
{
    let mut t = base_ty;
    for autoderefs in range(0, fcx.tcx().sess.recursion_limit.get()) {
        let resolved_t = structurally_resolved_type(fcx, sp, t);

        if ty::type_is_error(resolved_t) {
            return (resolved_t, autoderefs, None);
        }

        match should_stop(resolved_t, autoderefs) {
            Some(x) => return (resolved_t, autoderefs, Some(x)),
            None => {}
        }

        // Otherwise, deref if type is derefable:
        let mt = match ty::deref(resolved_t, false) {
            Some(mt) => Some(mt),
            None => {
                let method_call = expr_id.map(|id| MethodCall::autoderef(id, autoderefs));
                try_overloaded_deref(fcx, sp, method_call, None, resolved_t, lvalue_pref)
            }
        };
        match mt {
            Some(mt) => {
                t = mt.ty;
                if mt.mutbl == ast::MutImmutable {
                    lvalue_pref = NoPreference;
                }
            }
            None => return (resolved_t, autoderefs, None)
        }
    }

    // We've reached the recursion limit, error gracefully.
    span_err!(fcx.tcx().sess, sp, E0055,
        "reached the recursion limit while auto-dereferencing {}",
        base_ty.repr(fcx.tcx()));
    (fcx.tcx().types.err, 0, None)
}

fn try_overloaded_deref<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                  span: Span,
                                  method_call: Option<MethodCall>,
                                  base_expr: Option<&ast::Expr>,
                                  base_ty: Ty<'tcx>,
                                  lvalue_pref: LvaluePreference)
                                  -> Option<ty::mt<'tcx>>
{
    // Try DerefMut first, if preferred.
    let method = match (lvalue_pref, fcx.tcx().lang_items.deref_mut_trait()) {
        (PreferMutLvalue, Some(trait_did)) => {
            method::lookup_in_trait(fcx, span, base_expr.map(|x| &*x),
                                    token::intern("deref_mut"), trait_did,
                                    base_ty, None)
        }
        _ => None
    };

    // Otherwise, fall back to Deref.
    let method = match (method, fcx.tcx().lang_items.deref_trait()) {
        (None, Some(trait_did)) => {
            method::lookup_in_trait(fcx, span, base_expr.map(|x| &*x),
                                    token::intern("deref"), trait_did,
                                    base_ty, None)
        }
        (method, _) => method
    };

    make_overloaded_lvalue_return_type(fcx, method_call, method)
}

/// For the overloaded lvalue expressions (`*x`, `x[3]`), the trait returns a type of `&T`, but the
/// actual type we assign to the *expression* is `T`. So this function just peels off the return
/// type by one layer to yield `T`. It also inserts the `method-callee` into the method map.
fn make_overloaded_lvalue_return_type<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                                method_call: Option<MethodCall>,
                                                method: Option<MethodCallee<'tcx>>)
                                                -> Option<ty::mt<'tcx>>
{
    match method {
        Some(method) => {
            let ref_ty = ty::ty_fn_ret(method.ty);
            match method_call {
                Some(method_call) => {
                    fcx.inh.method_map.borrow_mut().insert(method_call,
                                                           method);
                }
                None => {}
            }
            match ref_ty {
                ty::FnConverging(ref_ty) => {
                    ty::deref(ref_ty, true)
                }
                ty::FnDiverging => {
                    fcx.tcx().sess.bug("index/deref traits do not define a `!` return")
                }
            }
        }
        None => None,
    }
}

fn autoderef_for_index<'a, 'tcx, T, F>(fcx: &FnCtxt<'a, 'tcx>,
                                       base_expr: &ast::Expr,
                                       base_ty: Ty<'tcx>,
                                       lvalue_pref: LvaluePreference,
                                       mut step: F)
                                       -> Option<T> where
    F: FnMut(Ty<'tcx>, ty::AutoDerefRef<'tcx>) -> Option<T>,
{
    // FIXME(#18741) -- this is almost but not quite the same as the
    // autoderef that normal method probing does. They could likely be
    // consolidated.

    let (ty, autoderefs, final_mt) =
        autoderef(fcx, base_expr.span, base_ty, Some(base_expr.id), lvalue_pref, |adj_ty, idx| {
            let autoderefref = ty::AutoDerefRef { autoderefs: idx, autoref: None };
            step(adj_ty, autoderefref)
        });

    if final_mt.is_some() {
        return final_mt;
    }

    // After we have fully autoderef'd, if the resulting type is [T, ..n], then
    // do a final unsized coercion to yield [T].
    match ty.sty {
        ty::ty_vec(element_ty, Some(n)) => {
            let adjusted_ty = ty::mk_vec(fcx.tcx(), element_ty, None);
            let autoderefref = ty::AutoDerefRef {
                autoderefs: autoderefs,
                autoref: Some(ty::AutoUnsize(ty::UnsizeLength(n)))
            };
            step(adjusted_ty, autoderefref)
        }
        _ => {
            None
        }
    }
}

/// Checks for a `Slice` (or `SliceMut`) impl at the relevant level of autoderef. If it finds one,
/// installs method info and returns type of method (else None).
fn try_overloaded_slice_step<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                       method_call: MethodCall,
                                       expr: &ast::Expr,
                                       base_expr: &ast::Expr,
                                       base_ty: Ty<'tcx>, // autoderef'd type
                                       autoderefref: ty::AutoDerefRef<'tcx>,
                                       lvalue_pref: LvaluePreference,
                                       start_expr: &Option<P<ast::Expr>>,
                                       end_expr: &Option<P<ast::Expr>>)
                                       -> Option<(Ty<'tcx>, /* index type */
                                                  Ty<'tcx>)> /* return type */
{
    let input_ty = fcx.infcx().next_ty_var();
    let return_ty = fcx.infcx().next_ty_var();

    let method = match lvalue_pref {
        PreferMutLvalue => {
            // Try `SliceMut` first, if preferred.
            match fcx.tcx().lang_items.slice_mut_trait() {
                Some(trait_did) => {
                    let method_name = match (start_expr, end_expr) {
                        (&Some(_), &Some(_)) => "slice_or_fail_mut",
                        (&Some(_), &None) => "slice_from_or_fail_mut",
                        (&None, &Some(_)) => "slice_to_or_fail_mut",
                        (&None, &None) => "as_mut_slice_",
                    };

                    method::lookup_in_trait_adjusted(fcx,
                                                     expr.span,
                                                     Some(&*base_expr),
                                                     token::intern(method_name),
                                                     trait_did,
                                                     autoderefref,
                                                     base_ty,
                                                     Some(vec![input_ty, return_ty]))
                }
                _ => None,
            }
        }
        NoPreference => {
            // Otherwise, fall back to `Slice`.
            match fcx.tcx().lang_items.slice_trait() {
                Some(trait_did) => {
                    let method_name = match (start_expr, end_expr) {
                        (&Some(_), &Some(_)) => "slice_or_fail",
                        (&Some(_), &None) => "slice_from_or_fail",
                        (&None, &Some(_)) => "slice_to_or_fail",
                        (&None, &None) => "as_slice_",
                    };

                    method::lookup_in_trait_adjusted(fcx,
                                                     expr.span,
                                                     Some(&*base_expr),
                                                     token::intern(method_name),
                                                     trait_did,
                                                     autoderefref,
                                                     base_ty,
                                                     Some(vec![input_ty, return_ty]))
                }
                _ => None,
            }
        }
    };

    // If some lookup succeeded, install method in table
    method.map(|method| {
        let method_ty = method.ty;
        make_overloaded_lvalue_return_type(fcx, Some(method_call), Some(method));

        let result_ty = ty::ty_fn_ret(method_ty);
        let result_ty = match result_ty {
            ty::FnConverging(result_ty) => result_ty,
            ty::FnDiverging => {
                fcx.tcx().sess.span_bug(expr.span,
                "slice trait does not define a `!` return")
            }
        };

        (input_ty, result_ty)
    })
}

/// To type-check `base_expr[index_expr]`, we progressively autoderef (and otherwise adjust)
/// `base_expr`, looking for a type which either supports builtin indexing or overloaded indexing.
/// This loop implements one step in that search; the autoderef loop is implemented by
/// `autoderef_for_index`.
fn try_index_step<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                            method_call: MethodCall,
                            expr: &ast::Expr,
                            base_expr: &ast::Expr,
                            adjusted_ty: Ty<'tcx>,
                            adjustment: ty::AutoDerefRef<'tcx>,
                            lvalue_pref: LvaluePreference)
                            -> Option<(/*index type*/ Ty<'tcx>, /*element type*/ Ty<'tcx>)>
{
    debug!("try_index_step(expr={}, base_expr.id={}, adjusted_ty={}, adjustment={})",
           expr.repr(fcx.tcx()),
           base_expr.repr(fcx.tcx()),
           adjusted_ty.repr(fcx.tcx()),
           adjustment);

    // Try built-in indexing first.
    match ty::index(adjusted_ty) {
        Some(ty) => {
            fcx.write_adjustment(base_expr.id, base_expr.span, ty::AdjustDerefRef(adjustment));
            return Some((fcx.tcx().types.uint, ty));
        }

        None => { }
    }

    let input_ty = fcx.infcx().next_ty_var();

    // Try `IndexMut` first, if preferred.
    let method = match (lvalue_pref, fcx.tcx().lang_items.index_mut_trait()) {
        (PreferMutLvalue, Some(trait_did)) => {
            method::lookup_in_trait_adjusted(fcx,
                                             expr.span,
                                             Some(&*base_expr),
                                             token::intern("index_mut"),
                                             trait_did,
                                             adjustment.clone(),
                                             adjusted_ty,
                                             Some(vec![input_ty]))
        }
        _ => None,
    };

    // Otherwise, fall back to `Index`.
    let method = match (method, fcx.tcx().lang_items.index_trait()) {
        (None, Some(trait_did)) => {
            method::lookup_in_trait_adjusted(fcx,
                                             expr.span,
                                             Some(&*base_expr),
                                             token::intern("index"),
                                             trait_did,
                                             adjustment,
                                             adjusted_ty,
                                             Some(vec![input_ty]))
        }
        (method, _) => method,
    };

    // If some lookup succeeds, write callee into table and extract index/element
    // type from the method signature.
    // If some lookup succeeded, install method in table
    method.and_then(|method| {
        make_overloaded_lvalue_return_type(fcx, Some(method_call), Some(method)).
            map(|ret| (input_ty, ret.ty))
    })
}

/// Given the head of a `for` expression, looks up the `next` method in the
/// `Iterator` trait. Panics if the expression does not implement `next`.
///
/// The return type of this function represents the concrete element type
/// `A` in the type `Iterator<A>` that the method returns.
fn lookup_method_for_for_loop<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                        iterator_expr: &ast::Expr,
                                        loop_id: ast::NodeId)
                                        -> Ty<'tcx> {
    let trait_did = match fcx.tcx().lang_items.require(IteratorItem) {
        Ok(trait_did) => trait_did,
        Err(ref err_string) => {
            fcx.tcx().sess.span_err(iterator_expr.span,
                                    err_string[]);
            return fcx.tcx().types.err
        }
    };

    let expr_type = fcx.expr_ty(&*iterator_expr);
    let method = method::lookup_in_trait(fcx,
                                         iterator_expr.span,
                                         Some(&*iterator_expr),
                                         token::intern("next"),
                                         trait_did,
                                         expr_type,
                                         None);

    // Regardless of whether the lookup succeeds, check the method arguments
    // so that we have *some* type for each argument.
    let method_type = match method {
        Some(ref method) => method.ty,
        None => {
            let true_expr_type = fcx.infcx().resolve_type_vars_if_possible(&expr_type);

            if !ty::type_is_error(true_expr_type) {
                let ty_string = fcx.infcx().ty_to_string(true_expr_type);
                fcx.tcx().sess.span_err(iterator_expr.span,
                                        format!("`for` loop expression has type `{}` which does \
                                                not implement the `Iterator` trait; \
                                                maybe try .iter()",
                                                ty_string)[]);
            }
            fcx.tcx().types.err
        }
    };
    let return_type = check_method_argument_types(fcx,
                                                  iterator_expr.span,
                                                  method_type,
                                                  iterator_expr,
                                                  &[],
                                                  AutorefArgs::No,
                                                  DontTupleArguments);

    match method {
        Some(method) => {
            fcx.inh.method_map.borrow_mut().insert(MethodCall::expr(loop_id),
                                                   method);

            // We expect the return type to be `Option` or something like it.
            // Grab the first parameter of its type substitution.
            let return_type = match return_type {
                ty::FnConverging(return_type) =>
                    structurally_resolved_type(fcx, iterator_expr.span, return_type),
                ty::FnDiverging => fcx.tcx().types.err
            };
            match return_type.sty {
                ty::ty_enum(_, ref substs)
                        if !substs.types.is_empty_in(subst::TypeSpace) => {
                    *substs.types.get(subst::TypeSpace, 0)
                }
                ty::ty_err => {
                    fcx.tcx().types.err
                }
                _ => {
                    fcx.tcx().sess.span_err(iterator_expr.span,
                                            format!("`next` method of the `Iterator` \
                                                    trait has an unexpected type `{}`",
                                                    fcx.infcx().ty_to_string(return_type))
                                            []);
                    fcx.tcx().types.err
                }
            }
        }
        None => fcx.tcx().types.err
    }
}

fn check_method_argument_types<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                         sp: Span,
                                         method_fn_ty: Ty<'tcx>,
                                         callee_expr: &ast::Expr,
                                         args_no_rcvr: &[&P<ast::Expr>],
                                         autoref_args: AutorefArgs,
                                         tuple_arguments: TupleArgumentsFlag)
                                         -> ty::FnOutput<'tcx> {
    if ty::type_is_error(method_fn_ty) {
        let err_inputs = err_args(fcx.tcx(), args_no_rcvr.len());

        let err_inputs = match tuple_arguments {
            DontTupleArguments => err_inputs,
            TupleArguments => vec![ty::mk_tup(fcx.tcx(), err_inputs)],
        };

        check_argument_types(fcx,
                             sp,
                             err_inputs[],
                             args_no_rcvr,
                             autoref_args,
                             false,
                             tuple_arguments);
        ty::FnConverging(fcx.tcx().types.err)
    } else {
        match method_fn_ty.sty {
            ty::ty_bare_fn(_, ref fty) => {
                // HACK(eddyb) ignore self in the definition (see above).
                check_argument_types(fcx,
                                     sp,
                                     fty.sig.0.inputs.slice_from(1),
                                     args_no_rcvr,
                                     autoref_args,
                                     fty.sig.0.variadic,
                                     tuple_arguments);
                fty.sig.0.output
            }
            _ => {
                fcx.tcx().sess.span_bug(callee_expr.span,
                                        "method without bare fn type");
            }
        }
    }
}

/// Generic function that factors out common logic from function calls, method calls and overloaded
/// operators.
fn check_argument_types<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                  sp: Span,
                                  fn_inputs: &[Ty<'tcx>],
                                  args: &[&P<ast::Expr>],
                                  autoref_args: AutorefArgs,
                                  variadic: bool,
                                  tuple_arguments: TupleArgumentsFlag) {
    let tcx = fcx.ccx.tcx;

    // Grab the argument types, supplying fresh type variables
    // if the wrong number of arguments were supplied
    let supplied_arg_count = if tuple_arguments == DontTupleArguments {
        args.len()
    } else {
        1
    };

    let expected_arg_count = fn_inputs.len();
    let formal_tys = if tuple_arguments == TupleArguments {
        let tuple_type = structurally_resolved_type(fcx, sp, fn_inputs[0]);
        match tuple_type.sty {
            ty::ty_tup(ref arg_types) => {
                if arg_types.len() != args.len() {
                    span_err!(tcx.sess, sp, E0057,
                        "this function takes {} parameter{} but {} parameter{} supplied",
                        arg_types.len(),
                        if arg_types.len() == 1 {""} else {"s"},
                        args.len(),
                        if args.len() == 1 {" was"} else {"s were"});
                    err_args(fcx.tcx(), args.len())
                } else {
                    (*arg_types).clone()
                }
            }
            _ => {
                span_err!(tcx.sess, sp, E0059,
                    "cannot use call notation; the first type parameter \
                     for the function trait is neither a tuple nor unit");
                err_args(fcx.tcx(), args.len())
            }
        }
    } else if expected_arg_count == supplied_arg_count {
        fn_inputs.iter().map(|a| *a).collect()
    } else if variadic {
        if supplied_arg_count >= expected_arg_count {
            fn_inputs.iter().map(|a| *a).collect()
        } else {
            span_err!(tcx.sess, sp, E0060,
                "this function takes at least {} parameter{} \
                 but {} parameter{} supplied",
                expected_arg_count,
                if expected_arg_count == 1 {""} else {"s"},
                supplied_arg_count,
                if supplied_arg_count == 1 {" was"} else {"s were"});
            err_args(fcx.tcx(), supplied_arg_count)
        }
    } else {
        span_err!(tcx.sess, sp, E0061,
            "this function takes {} parameter{} but {} parameter{} supplied",
            expected_arg_count,
            if expected_arg_count == 1 {""} else {"s"},
            supplied_arg_count,
            if supplied_arg_count == 1 {" was"} else {"s were"});
        err_args(fcx.tcx(), supplied_arg_count)
    };

    debug!("check_argument_types: formal_tys={}",
           formal_tys.iter().map(|t| fcx.infcx().ty_to_string(*t)).collect::<Vec<String>>());

    // Check the arguments.
    // We do this in a pretty awful way: first we typecheck any arguments
    // that are not anonymous functions, then we typecheck the anonymous
    // functions. This is so that we have more information about the types
    // of arguments when we typecheck the functions. This isn't really the
    // right way to do this.
    let xs = [false, true];
    for check_blocks in xs.iter() {
        let check_blocks = *check_blocks;
        debug!("check_blocks={}", check_blocks);

        // More awful hacks: before we check the blocks, try to do
        // an "opportunistic" vtable resolution of any trait
        // bounds on the call.
        if check_blocks {
            vtable::select_new_fcx_obligations(fcx);
        }

        // For variadic functions, we don't have a declared type for all of
        // the arguments hence we only do our usual type checking with
        // the arguments who's types we do know.
        let t = if variadic {
            expected_arg_count
        } else if tuple_arguments == TupleArguments {
            args.len()
        } else {
            supplied_arg_count
        };
        for (i, arg) in args.iter().take(t).enumerate() {
            let is_block = match arg.node {
                ast::ExprClosure(..) => true,
                _ => false
            };

            if is_block == check_blocks {
                debug!("checking the argument");
                let mut formal_ty = formal_tys[i];

                match autoref_args {
                    AutorefArgs::Yes => {
                        match formal_ty.sty {
                            ty::ty_rptr(_, mt) => formal_ty = mt.ty,
                            ty::ty_err => (),
                            _ => {
                                // So we hit this case when one implements the
                                // operator traits but leaves an argument as
                                // just T instead of &T. We'll catch it in the
                                // mismatch impl/trait method phase no need to
                                // ICE here.
                                // See: #11450
                                formal_ty = tcx.types.err;
                            }
                        }
                    }
                    AutorefArgs::No => {}
                }

                check_expr_coercable_to_type(fcx, &***arg, formal_ty);
            }
        }
    }

    // We also need to make sure we at least write the ty of the other
    // arguments which we skipped above.
    if variadic {
        for arg in args.iter().skip(expected_arg_count) {
            check_expr(fcx, &***arg);

            // There are a few types which get autopromoted when passed via varargs
            // in C but we just error out instead and require explicit casts.
            let arg_ty = structurally_resolved_type(fcx, arg.span,
                                                    fcx.expr_ty(&***arg));
            match arg_ty.sty {
                ty::ty_float(ast::TyF32) => {
                    fcx.type_error_message(arg.span,
                                           |t| {
                        format!("can't pass an {} to variadic \
                                 function, cast to c_double", t)
                    }, arg_ty, None);
                }
                ty::ty_int(ast::TyI8) | ty::ty_int(ast::TyI16) | ty::ty_bool => {
                    fcx.type_error_message(arg.span, |t| {
                        format!("can't pass {} to variadic \
                                 function, cast to c_int",
                                       t)
                    }, arg_ty, None);
                }
                ty::ty_uint(ast::TyU8) | ty::ty_uint(ast::TyU16) => {
                    fcx.type_error_message(arg.span, |t| {
                        format!("can't pass {} to variadic \
                                 function, cast to c_uint",
                                       t)
                    }, arg_ty, None);
                }
                _ => {}
            }
        }
    }
}

// FIXME(#17596) Ty<'tcx> is incorrectly invariant w.r.t 'tcx.
fn err_args<'tcx>(tcx: &ty::ctxt<'tcx>, len: uint) -> Vec<Ty<'tcx>> {
    range(0, len).map(|_| tcx.types.err).collect()
}

fn write_call<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                        call_expr: &ast::Expr,
                        output: ty::FnOutput<'tcx>) {
    fcx.write_ty(call_expr.id, match output {
        ty::FnConverging(output_ty) => output_ty,
        ty::FnDiverging => fcx.infcx().next_diverging_ty_var()
    });
}

// AST fragment checking
fn check_lit<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                       lit: &ast::Lit,
                       expected: Expectation<'tcx>)
                       -> Ty<'tcx>
{
    let tcx = fcx.ccx.tcx;

    match lit.node {
        ast::LitStr(..) => ty::mk_str_slice(tcx, tcx.mk_region(ty::ReStatic), ast::MutImmutable),
        ast::LitBinary(..) => {
            ty::mk_slice(tcx,
                         tcx.mk_region(ty::ReStatic),
                         ty::mt{ ty: tcx.types.u8, mutbl: ast::MutImmutable })
        }
        ast::LitByte(_) => tcx.types.u8,
        ast::LitChar(_) => tcx.types.char,
        ast::LitInt(_, ast::SignedIntLit(t, _)) => ty::mk_mach_int(tcx, t),
        ast::LitInt(_, ast::UnsignedIntLit(t)) => ty::mk_mach_uint(tcx, t),
        ast::LitInt(_, ast::UnsuffixedIntLit(_)) => {
            let opt_ty = expected.map_to_option(fcx, |ty| {
                match ty.sty {
                    ty::ty_int(_) | ty::ty_uint(_) => Some(ty),
                    ty::ty_char => Some(tcx.types.u8),
                    ty::ty_ptr(..) => Some(tcx.types.uint),
                    ty::ty_bare_fn(..) => Some(tcx.types.uint),
                    _ => None
                }
            });
            opt_ty.unwrap_or_else(
                || ty::mk_int_var(tcx, fcx.infcx().next_int_var_id()))
        }
        ast::LitFloat(_, t) => ty::mk_mach_float(tcx, t),
        ast::LitFloatUnsuffixed(_) => {
            let opt_ty = expected.map_to_option(fcx, |ty| {
                match ty.sty {
                    ty::ty_float(_) => Some(ty),
                    _ => None
                }
            });
            opt_ty.unwrap_or_else(
                || ty::mk_float_var(tcx, fcx.infcx().next_float_var_id()))
        }
        ast::LitBool(_) => tcx.types.bool
    }
}

pub fn valid_range_bounds(ccx: &CrateCtxt,
                          from: &ast::Expr,
                          to: &ast::Expr)
                       -> Option<bool> {
    match const_eval::compare_lit_exprs(ccx.tcx, from, to) {
        Some(val) => Some(val <= 0),
        None => None
    }
}

pub fn check_expr_has_type<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                     expr: &ast::Expr,
                                     expected: Ty<'tcx>) {
    check_expr_with_unifier(
        fcx, expr, ExpectHasType(expected), NoPreference,
        || demand::suptype(fcx, expr.span, expected, fcx.expr_ty(expr)));
}

fn check_expr_coercable_to_type<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                          expr: &ast::Expr,
                                          expected: Ty<'tcx>) {
    check_expr_with_unifier(
        fcx, expr, ExpectHasType(expected), NoPreference,
        || demand::coerce(fcx, expr.span, expected, expr));
}

fn check_expr_with_hint<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>, expr: &ast::Expr,
                                  expected: Ty<'tcx>) {
    check_expr_with_unifier(
        fcx, expr, ExpectHasType(expected), NoPreference,
        || ())
}

fn check_expr_with_expectation<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                         expr: &ast::Expr,
                                         expected: Expectation<'tcx>) {
    check_expr_with_unifier(
        fcx, expr, expected, NoPreference,
        || ())
}

fn check_expr_with_expectation_and_lvalue_pref<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                                         expr: &ast::Expr,
                                                         expected: Expectation<'tcx>,
                                                         lvalue_pref: LvaluePreference)
{
    check_expr_with_unifier(fcx, expr, expected, lvalue_pref, || ())
}

fn check_expr(fcx: &FnCtxt, expr: &ast::Expr)  {
    check_expr_with_unifier(fcx, expr, NoExpectation, NoPreference, || ())
}

fn check_expr_with_lvalue_pref(fcx: &FnCtxt, expr: &ast::Expr,
                               lvalue_pref: LvaluePreference)  {
    check_expr_with_unifier(fcx, expr, NoExpectation, lvalue_pref, || ())
}

// determine the `self` type, using fresh variables for all variables
// declared on the impl declaration e.g., `impl<A,B> for ~[(A,B)]`
// would return ($0, $1) where $0 and $1 are freshly instantiated type
// variables.
pub fn impl_self_ty<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                              span: Span, // (potential) receiver for this impl
                              did: ast::DefId)
                              -> TypeAndSubsts<'tcx> {
    let tcx = fcx.tcx();

    let ity = ty::lookup_item_type(tcx, did);
    let (n_tps, rps, raw_ty) =
        (ity.generics.types.len(subst::TypeSpace),
         ity.generics.regions.get_slice(subst::TypeSpace),
         ity.ty);

    let rps = fcx.inh.infcx.region_vars_for_defs(span, rps);
    let tps = fcx.inh.infcx.next_ty_vars(n_tps);
    let substs = subst::Substs::new_type(tps, rps);
    let substd_ty = fcx.instantiate_type_scheme(span, &substs, &raw_ty);

    TypeAndSubsts { substs: substs, ty: substd_ty }
}

// Only for fields! Returns <none> for methods>
// Indifferent to privacy flags
pub fn lookup_field_ty<'tcx>(tcx: &ty::ctxt<'tcx>,
                             class_id: ast::DefId,
                             items: &[ty::field_ty],
                             fieldname: ast::Name,
                             substs: &subst::Substs<'tcx>)
                             -> Option<Ty<'tcx>> {

    let o_field = items.iter().find(|f| f.name == fieldname);
    o_field.map(|f| ty::lookup_field_type(tcx, class_id, f.id, substs))
}

pub fn lookup_tup_field_ty<'tcx>(tcx: &ty::ctxt<'tcx>,
                                 class_id: ast::DefId,
                                 items: &[ty::field_ty],
                                 idx: uint,
                                 substs: &subst::Substs<'tcx>)
                                 -> Option<Ty<'tcx>> {

    let o_field = if idx < items.len() { Some(&items[idx]) } else { None };
    o_field.map(|f| ty::lookup_field_type(tcx, class_id, f.id, substs))
}

// Controls whether the arguments are automatically referenced. This is useful
// for overloaded binary and unary operators.
#[derive(Copy, PartialEq)]
pub enum AutorefArgs {
    Yes,
    No,
}

/// Controls whether the arguments are tupled. This is used for the call
/// operator.
///
/// Tupling means that all call-side arguments are packed into a tuple and
/// passed as a single parameter. For example, if tupling is enabled, this
/// function:
///
///     fn f(x: (int, int))
///
/// Can be called as:
///
///     f(1, 2);
///
/// Instead of:
///
///     f((1, 2));
#[derive(Clone, Eq, PartialEq)]
enum TupleArgumentsFlag {
    DontTupleArguments,
    TupleArguments,
}

/// Invariant:
/// If an expression has any sub-expressions that result in a type error,
/// inspecting that expression's type with `ty::type_is_error` will return
/// true. Likewise, if an expression is known to diverge, inspecting its
/// type with `ty::type_is_bot` will return true (n.b.: since Rust is
/// strict, _|_ can appear in the type of an expression that does not,
/// itself, diverge: for example, fn() -> _|_.)
/// Note that inspecting a type's structure *directly* may expose the fact
/// that there are actually multiple representations for `ty_err`, so avoid
/// that when err needs to be handled differently.
fn check_expr_with_unifier<'a, 'tcx, F>(fcx: &FnCtxt<'a, 'tcx>,
                                        expr: &ast::Expr,
                                        expected: Expectation<'tcx>,
                                        lvalue_pref: LvaluePreference,
                                        unifier: F) where
    F: FnOnce(),
{
    debug!(">> typechecking: expr={} expected={}",
           expr.repr(fcx.tcx()), expected.repr(fcx.tcx()));

    // Checks a method call.
    fn check_method_call(fcx: &FnCtxt,
                         expr: &ast::Expr,
                         method_name: ast::SpannedIdent,
                         args: &[P<ast::Expr>],
                         tps: &[P<ast::Ty>],
                         lvalue_pref: LvaluePreference) {
        let rcvr = &*args[0];
        check_expr_with_lvalue_pref(fcx, &*rcvr, lvalue_pref);

        // no need to check for bot/err -- callee does that
        let expr_t = structurally_resolved_type(fcx,
                                                expr.span,
                                                fcx.expr_ty(&*rcvr));

        let tps = tps.iter().map(|ast_ty| fcx.to_ty(&**ast_ty)).collect::<Vec<_>>();
        let fn_ty = match method::lookup(fcx,
                                         method_name.span,
                                         method_name.node.name,
                                         expr_t,
                                         tps,
                                         expr,
                                         rcvr) {
            Ok(method) => {
                let method_ty = method.ty;
                let method_call = MethodCall::expr(expr.id);
                fcx.inh.method_map.borrow_mut().insert(method_call, method);
                method_ty
            }
            Err(error) => {
                method::report_error(fcx, method_name.span, expr_t, method_name.node.name, error);
                fcx.write_error(expr.id);
                fcx.tcx().types.err
            }
        };

        // Call the generic checker.
        let args: Vec<_> = args[1..].iter().map(|x| x).collect();
        let ret_ty = check_method_argument_types(fcx,
                                                 method_name.span,
                                                 fn_ty,
                                                 expr,
                                                 args.as_slice(),
                                                 AutorefArgs::No,
                                                 DontTupleArguments);

        write_call(fcx, expr, ret_ty);
    }

    // A generic function for checking the then and else in an if
    // or if-else.
    fn check_then_else<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                 cond_expr: &ast::Expr,
                                 then_blk: &ast::Block,
                                 opt_else_expr: Option<&ast::Expr>,
                                 id: ast::NodeId,
                                 sp: Span,
                                 expected: Expectation<'tcx>) {
        check_expr_has_type(fcx, cond_expr, fcx.tcx().types.bool);

        let expected = expected.adjust_for_branches(fcx);
        check_block_with_expected(fcx, then_blk, expected);
        let then_ty = fcx.node_ty(then_blk.id);

        let branches_ty = match opt_else_expr {
            Some(ref else_expr) => {
                check_expr_with_expectation(fcx, &**else_expr, expected);
                let else_ty = fcx.expr_ty(&**else_expr);
                infer::common_supertype(fcx.infcx(),
                                        infer::IfExpression(sp),
                                        true,
                                        then_ty,
                                        else_ty)
            }
            None => {
                infer::common_supertype(fcx.infcx(),
                                        infer::IfExpressionWithNoElse(sp),
                                        false,
                                        then_ty,
                                        ty::mk_nil(fcx.tcx()))
            }
        };

        let cond_ty = fcx.expr_ty(cond_expr);
        let if_ty = if ty::type_is_error(cond_ty) {
            fcx.tcx().types.err
        } else {
            branches_ty
        };

        fcx.write_ty(id, if_ty);
    }

    fn lookup_op_method<'a, 'tcx, F>(fcx: &'a FnCtxt<'a, 'tcx>,
                                     op_ex: &ast::Expr,
                                     lhs_ty: Ty<'tcx>,
                                     opname: ast::Name,
                                     trait_did: Option<ast::DefId>,
                                     lhs: &'a ast::Expr,
                                     rhs: Option<&P<ast::Expr>>,
                                     unbound_method: F,
                                     autoref_args: AutorefArgs) -> Ty<'tcx> where
        F: FnOnce(),
    {
        let method = match trait_did {
            Some(trait_did) => {
                // We do eager coercions to make using operators
                // more ergonomic:
                //
                // - If the input is of type &'a T (resp. &'a mut T),
                //   then reborrow it to &'b T (resp. &'b mut T) where
                //   'b <= 'a.  This makes things like `x == y`, where
                //   `x` and `y` are both region pointers, work.  We
                //   could also solve this with variance or different
                //   traits that don't force left and right to have same
                //   type.
                let (adj_ty, adjustment) = match lhs_ty.sty {
                    ty::ty_rptr(r_in, mt) => {
                        let r_adj = fcx.infcx().next_region_var(infer::Autoref(lhs.span));
                        fcx.mk_subr(infer::Reborrow(lhs.span), r_adj, *r_in);
                        let adjusted_ty = ty::mk_rptr(fcx.tcx(), fcx.tcx().mk_region(r_adj), mt);
                        let autoptr = ty::AutoPtr(r_adj, mt.mutbl, None);
                        let adjustment = ty::AutoDerefRef { autoderefs: 1, autoref: Some(autoptr) };
                        (adjusted_ty, adjustment)
                    }
                    _ => {
                        (lhs_ty, ty::AutoDerefRef { autoderefs: 0, autoref: None })
                    }
                };

                debug!("adjusted_ty={} adjustment={}",
                       adj_ty.repr(fcx.tcx()),
                       adjustment);

                method::lookup_in_trait_adjusted(fcx, op_ex.span, Some(lhs), opname,
                                                 trait_did, adjustment, adj_ty, None)
            }
            None => None
        };
        let args = match rhs {
            Some(rhs) => vec![rhs],
            None => vec![]
        };
        match method {
            Some(method) => {
                let method_ty = method.ty;
                // HACK(eddyb) Fully qualified path to work around a resolve bug.
                let method_call = ::middle::ty::MethodCall::expr(op_ex.id);
                fcx.inh.method_map.borrow_mut().insert(method_call, method);
                match check_method_argument_types(fcx,
                                            op_ex.span,
                                            method_ty,
                                            op_ex,
                                            args.as_slice(),
                                            autoref_args,
                                            DontTupleArguments) {
                    ty::FnConverging(result_type) => result_type,
                    ty::FnDiverging => fcx.tcx().types.err
                }
            }
            None => {
                unbound_method();
                // Check the args anyway
                // so we get all the error messages
                let expected_ty = fcx.tcx().types.err;
                check_method_argument_types(fcx,
                                            op_ex.span,
                                            expected_ty,
                                            op_ex,
                                            args.as_slice(),
                                            autoref_args,
                                            DontTupleArguments);
                fcx.tcx().types.err
            }
        }
    }

    // could be either an expr_binop or an expr_assign_binop
    fn check_binop(fcx: &FnCtxt,
                   expr: &ast::Expr,
                   op: ast::BinOp,
                   lhs: &ast::Expr,
                   rhs: &P<ast::Expr>,
                   is_binop_assignment: IsBinopAssignment) {
        let tcx = fcx.ccx.tcx;

        let lvalue_pref = match is_binop_assignment {
            BinopAssignment => PreferMutLvalue,
            SimpleBinop => NoPreference
        };
        check_expr_with_lvalue_pref(fcx, &*lhs, lvalue_pref);

        // Callee does bot / err checking
        let lhs_t = structurally_resolved_type(fcx, lhs.span,
                                               fcx.expr_ty(&*lhs));

        if ty::type_is_integral(lhs_t) && ast_util::is_shift_binop(op) {
            // Shift is a special case: rhs must be uint, no matter what lhs is
            check_expr_has_type(fcx, &**rhs, fcx.tcx().types.uint);
            fcx.write_ty(expr.id, lhs_t);
            return;
        }

        if ty::is_binopable(tcx, lhs_t, op) {
            let tvar = fcx.infcx().next_ty_var();
            demand::suptype(fcx, expr.span, tvar, lhs_t);
            check_expr_has_type(fcx, &**rhs, tvar);

            let result_t = match op {
                ast::BiEq | ast::BiNe | ast::BiLt | ast::BiLe | ast::BiGe |
                ast::BiGt => {
                    if ty::type_is_simd(tcx, lhs_t) {
                        if ty::type_is_fp(ty::simd_type(tcx, lhs_t)) {
                            fcx.type_error_message(expr.span,
                                |actual| {
                                    format!("binary comparison \
                                             operation `{}` not \
                                             supported for floating \
                                             point SIMD vector `{}`",
                                            ast_util::binop_to_string(op),
                                            actual)
                                },
                                lhs_t,
                                None
                            );
                            fcx.tcx().types.err
                        } else {
                            lhs_t
                        }
                    } else {
                        fcx.tcx().types.bool
                    }
                },
                _ => lhs_t,
            };

            fcx.write_ty(expr.id, result_t);
            return;
        }

        if op == ast::BiOr || op == ast::BiAnd {
            // This is an error; one of the operands must have the wrong
            // type
            fcx.write_error(expr.id);
            fcx.write_error(rhs.id);
            fcx.type_error_message(expr.span,
                                   |actual| {
                    format!("binary operation `{}` cannot be applied \
                             to type `{}`",
                            ast_util::binop_to_string(op),
                            actual)
                },
                lhs_t,
                None)
        }

        // Check for overloaded operators if not an assignment.
        let result_t = if is_binop_assignment == SimpleBinop {
            check_user_binop(fcx, expr, lhs, lhs_t, op, rhs)
        } else {
            fcx.type_error_message(expr.span,
                                   |actual| {
                                        format!("binary assignment \
                                                 operation `{}=` \
                                                 cannot be applied to \
                                                 type `{}`",
                                                ast_util::binop_to_string(op),
                                                actual)
                                   },
                                   lhs_t,
                                   None);
            check_expr(fcx, &**rhs);
            fcx.tcx().types.err
        };

        fcx.write_ty(expr.id, result_t);
        if ty::type_is_error(result_t) {
            fcx.write_ty(rhs.id, result_t);
        }
    }

    fn check_user_binop<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                  ex: &ast::Expr,
                                  lhs_expr: &ast::Expr,
                                  lhs_resolved_t: Ty<'tcx>,
                                  op: ast::BinOp,
                                  rhs: &P<ast::Expr>) -> Ty<'tcx> {
        let tcx = fcx.ccx.tcx;
        let lang = &tcx.lang_items;
        let (name, trait_did) = match op {
            ast::BiAdd => ("add", lang.add_trait()),
            ast::BiSub => ("sub", lang.sub_trait()),
            ast::BiMul => ("mul", lang.mul_trait()),
            ast::BiDiv => ("div", lang.div_trait()),
            ast::BiRem => ("rem", lang.rem_trait()),
            ast::BiBitXor => ("bitxor", lang.bitxor_trait()),
            ast::BiBitAnd => ("bitand", lang.bitand_trait()),
            ast::BiBitOr => ("bitor", lang.bitor_trait()),
            ast::BiShl => ("shl", lang.shl_trait()),
            ast::BiShr => ("shr", lang.shr_trait()),
            ast::BiLt => ("lt", lang.ord_trait()),
            ast::BiLe => ("le", lang.ord_trait()),
            ast::BiGe => ("ge", lang.ord_trait()),
            ast::BiGt => ("gt", lang.ord_trait()),
            ast::BiEq => ("eq", lang.eq_trait()),
            ast::BiNe => ("ne", lang.eq_trait()),
            ast::BiAnd | ast::BiOr => {
                check_expr(fcx, &**rhs);
                return tcx.types.err;
            }
        };
        lookup_op_method(fcx, ex, lhs_resolved_t, token::intern(name),
                         trait_did, lhs_expr, Some(rhs), || {
            fcx.type_error_message(ex.span, |actual| {
                format!("binary operation `{}` cannot be applied to type `{}`",
                        ast_util::binop_to_string(op),
                        actual)
            }, lhs_resolved_t, None)
        }, if ast_util::is_by_value_binop(op) { AutorefArgs::No } else { AutorefArgs::Yes })
    }

    fn check_user_unop<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                 op_str: &str,
                                 mname: &str,
                                 trait_did: Option<ast::DefId>,
                                 ex: &ast::Expr,
                                 rhs_expr: &ast::Expr,
                                 rhs_t: Ty<'tcx>,
                                 op: ast::UnOp) -> Ty<'tcx> {
       lookup_op_method(fcx, ex, rhs_t, token::intern(mname),
                        trait_did, rhs_expr, None, || {
            fcx.type_error_message(ex.span, |actual| {
                format!("cannot apply unary operator `{}` to type `{}`",
                        op_str, actual)
            }, rhs_t, None);
        }, if ast_util::is_by_value_unop(op) { AutorefArgs::No } else { AutorefArgs::Yes })
    }

    // Check field access expressions
    fn check_field(fcx: &FnCtxt,
                   expr: &ast::Expr,
                   lvalue_pref: LvaluePreference,
                   base: &ast::Expr,
                   field: &ast::SpannedIdent) {
        let tcx = fcx.ccx.tcx;
        check_expr_with_lvalue_pref(fcx, base, lvalue_pref);
        let expr_t = structurally_resolved_type(fcx, expr.span,
                                                fcx.expr_ty(base));
        // FIXME(eddyb) #12808 Integrate privacy into this auto-deref loop.
        let (_, autoderefs, field_ty) =
            autoderef(fcx, expr.span, expr_t, Some(base.id), lvalue_pref, |base_t, _| {
                match base_t.sty {
                    ty::ty_struct(base_id, substs) => {
                        debug!("struct named {}", ppaux::ty_to_string(tcx, base_t));
                        let fields = ty::lookup_struct_fields(tcx, base_id);
                        lookup_field_ty(tcx, base_id, fields[],
                                        field.node.name, &(*substs))
                    }
                    _ => None
                }
            });
        match field_ty {
            Some(field_ty) => {
                fcx.write_ty(expr.id, field_ty);
                fcx.write_autoderef_adjustment(base.id, base.span, autoderefs);
                return;
            }
            None => {}
        }

        if method::exists(fcx, field.span, field.node.name, expr_t, expr.id) {
            fcx.type_error_message(
                field.span,
                |actual| {
                    format!("attempted to take value of method `{}` on type \
                            `{}`", token::get_ident(field.node), actual)
                },
                expr_t, None);

            tcx.sess.span_help(field.span,
                               "maybe a `()` to call it is missing? \
                               If not, try an anonymous function");
        } else {
            fcx.type_error_message(
                expr.span,
                |actual| {
                    format!("attempted access of field `{}` on \
                            type `{}`, but no field with that \
                            name was found",
                            token::get_ident(field.node),
                            actual)
                },
                expr_t, None);
        }

        fcx.write_error(expr.id);
    }

    // Check tuple index expressions
    fn check_tup_field(fcx: &FnCtxt,
                       expr: &ast::Expr,
                       lvalue_pref: LvaluePreference,
                       base: &ast::Expr,
                       idx: codemap::Spanned<uint>) {
        let tcx = fcx.ccx.tcx;
        check_expr_with_lvalue_pref(fcx, base, lvalue_pref);
        let expr_t = structurally_resolved_type(fcx, expr.span,
                                                fcx.expr_ty(base));
        let mut tuple_like = false;
        // FIXME(eddyb) #12808 Integrate privacy into this auto-deref loop.
        let (_, autoderefs, field_ty) =
            autoderef(fcx, expr.span, expr_t, Some(base.id), lvalue_pref, |base_t, _| {
                match base_t.sty {
                    ty::ty_struct(base_id, substs) => {
                        tuple_like = ty::is_tuple_struct(tcx, base_id);
                        if tuple_like {
                            debug!("tuple struct named {}", ppaux::ty_to_string(tcx, base_t));
                            let fields = ty::lookup_struct_fields(tcx, base_id);
                            lookup_tup_field_ty(tcx, base_id, fields[],
                                                idx.node, &(*substs))
                        } else {
                            None
                        }
                    }
                    ty::ty_tup(ref v) => {
                        tuple_like = true;
                        if idx.node < v.len() { Some(v[idx.node]) } else { None }
                    }
                    _ => None
                }
            });
        match field_ty {
            Some(field_ty) => {
                fcx.write_ty(expr.id, field_ty);
                fcx.write_autoderef_adjustment(base.id, base.span, autoderefs);
                return;
            }
            None => {}
        }
        fcx.type_error_message(
            expr.span,
            |actual| {
                if tuple_like {
                    format!("attempted out-of-bounds tuple index `{}` on \
                                    type `{}`",
                                   idx.node,
                                   actual)
                } else {
                    format!("attempted tuple index `{}` on type `{}`, but the \
                                     type was not a tuple or tuple struct",
                                    idx.node,
                                    actual)
                }
            },
            expr_t, None);

        fcx.write_error(expr.id);
    }

    fn check_struct_or_variant_fields<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                                struct_ty: Ty<'tcx>,
                                                span: Span,
                                                class_id: ast::DefId,
                                                node_id: ast::NodeId,
                                                substitutions: &'tcx subst::Substs<'tcx>,
                                                field_types: &[ty::field_ty],
                                                ast_fields: &[ast::Field],
                                                check_completeness: bool,
                                                enum_id_opt: Option<ast::DefId>)  {
        let tcx = fcx.ccx.tcx;

        let mut class_field_map = FnvHashMap::new();
        let mut fields_found = 0;
        for field in field_types.iter() {
            class_field_map.insert(field.name, (field.id, false));
        }

        let mut error_happened = false;

        // Typecheck each field.
        for field in ast_fields.iter() {
            let mut expected_field_type = tcx.types.err;

            let pair = class_field_map.get(&field.ident.node.name).map(|x| *x);
            match pair {
                None => {
                    fcx.type_error_message(
                        field.ident.span,
                        |actual| match enum_id_opt {
                            Some(enum_id) => {
                                let variant_type = ty::enum_variant_with_id(tcx,
                                                                            enum_id,
                                                                            class_id);
                                format!("struct variant `{}::{}` has no field named `{}`",
                                        actual, variant_type.name.as_str(),
                                        token::get_ident(field.ident.node))
                            }
                            None => {
                                format!("structure `{}` has no field named `{}`",
                                        actual,
                                        token::get_ident(field.ident.node))
                            }
                        },
                        struct_ty,
                        None);
                    error_happened = true;
                }
                Some((_, true)) => {
                    span_err!(fcx.tcx().sess, field.ident.span, E0062,
                        "field `{}` specified more than once",
                        token::get_ident(field.ident.node));
                    error_happened = true;
                }
                Some((field_id, false)) => {
                    expected_field_type =
                        ty::lookup_field_type(
                            tcx, class_id, field_id, substitutions);
                    class_field_map.insert(
                        field.ident.node.name, (field_id, true));
                    fields_found += 1;
                }
            }
            // Make sure to give a type to the field even if there's
            // an error, so we can continue typechecking
            check_expr_coercable_to_type(
                    fcx,
                    &*field.expr,
                    expected_field_type);
        }

        if error_happened {
            fcx.write_error(node_id);
        }

        if check_completeness && !error_happened {
            // Make sure the programmer specified all the fields.
            assert!(fields_found <= field_types.len());
            if fields_found < field_types.len() {
                let mut missing_fields = Vec::new();
                for class_field in field_types.iter() {
                    let name = class_field.name;
                    let (_, seen) = class_field_map[name];
                    if !seen {
                        missing_fields.push(
                            format!("`{}`", token::get_name(name).get()))
                    }
                }

                span_err!(tcx.sess, span, E0063,
                    "missing field{}: {}",
                    if missing_fields.len() == 1 {""} else {"s"},
                    missing_fields.connect(", "));
             }
        }

        if !error_happened {
            fcx.write_ty(node_id, ty::mk_struct(fcx.ccx.tcx,
                                class_id, substitutions));
        }
    }

    fn check_struct_constructor(fcx: &FnCtxt,
                                id: ast::NodeId,
                                span: codemap::Span,
                                class_id: ast::DefId,
                                fields: &[ast::Field],
                                base_expr: Option<&ast::Expr>) {
        let tcx = fcx.ccx.tcx;

        // Generate the struct type.
        let TypeAndSubsts {
            ty: mut struct_type,
            substs: struct_substs
        } = fcx.instantiate_type(span, class_id);

        // Look up and check the fields.
        let class_fields = ty::lookup_struct_fields(tcx, class_id);
        check_struct_or_variant_fields(fcx,
                                       struct_type,
                                       span,
                                       class_id,
                                       id,
                                       fcx.ccx.tcx.mk_substs(struct_substs),
                                       class_fields[],
                                       fields,
                                       base_expr.is_none(),
                                       None);
        if ty::type_is_error(fcx.node_ty(id)) {
            struct_type = tcx.types.err;
        }

        // Check the base expression if necessary.
        match base_expr {
            None => {}
            Some(base_expr) => {
                check_expr_has_type(fcx, &*base_expr, struct_type);
            }
        }

        // Write in the resulting type.
        fcx.write_ty(id, struct_type);
    }

    fn check_struct_enum_variant(fcx: &FnCtxt,
                                 id: ast::NodeId,
                                 span: codemap::Span,
                                 enum_id: ast::DefId,
                                 variant_id: ast::DefId,
                                 fields: &[ast::Field]) {
        let tcx = fcx.ccx.tcx;

        // Look up the number of type parameters and the raw type, and
        // determine whether the enum is region-parameterized.
        let TypeAndSubsts {
            ty: enum_type,
            substs: substitutions
        } = fcx.instantiate_type(span, enum_id);

        // Look up and check the enum variant fields.
        let variant_fields = ty::lookup_struct_fields(tcx, variant_id);
        check_struct_or_variant_fields(fcx,
                                       enum_type,
                                       span,
                                       variant_id,
                                       id,
                                       fcx.ccx.tcx.mk_substs(substitutions),
                                       variant_fields[],
                                       fields,
                                       true,
                                       Some(enum_id));
        fcx.write_ty(id, enum_type);
    }

    fn check_struct_fields_on_error(fcx: &FnCtxt,
                                    id: ast::NodeId,
                                    fields: &[ast::Field],
                                    base_expr: &Option<P<ast::Expr>>) {
        // Make sure to still write the types
        // otherwise we might ICE
        fcx.write_error(id);
        for field in fields.iter() {
            check_expr(fcx, &*field.expr);
        }
        match *base_expr {
            Some(ref base) => check_expr(fcx, &**base),
            None => {}
        }
    }

    type ExprCheckerWithTy = fn(&FnCtxt, &ast::Expr, Ty);

    let tcx = fcx.ccx.tcx;
    let id = expr.id;
    match expr.node {
      ast::ExprBox(ref opt_place, ref subexpr) => {
          opt_place.as_ref().map(|place|check_expr(fcx, &**place));
          check_expr(fcx, &**subexpr);

          let mut checked = false;
          opt_place.as_ref().map(|place| match place.node {
              ast::ExprPath(ref path) => {
                  // FIXME(pcwalton): For now we hardcode the two permissible
                  // places: the exchange heap and the managed heap.
                  let definition = lookup_def(fcx, path.span, place.id);
                  let def_id = definition.def_id();
                  let referent_ty = fcx.expr_ty(&**subexpr);
                  if tcx.lang_items.exchange_heap() == Some(def_id) {
                      fcx.write_ty(id, ty::mk_uniq(tcx, referent_ty));
                      checked = true
                  }
              }
              _ => {}
          });

          if !checked {
              span_err!(tcx.sess, expr.span, E0066,
                  "only the managed heap and exchange heap are currently supported");
              fcx.write_ty(id, tcx.types.err);
          }
      }

      ast::ExprLit(ref lit) => {
        let typ = check_lit(fcx, &**lit, expected);
        fcx.write_ty(id, typ);
      }
      ast::ExprBinary(op, ref lhs, ref rhs) => {
        check_binop(fcx, expr, op, &**lhs, rhs, SimpleBinop);

        let lhs_ty = fcx.expr_ty(&**lhs);
        let rhs_ty = fcx.expr_ty(&**rhs);
        if ty::type_is_error(lhs_ty) ||
            ty::type_is_error(rhs_ty) {
            fcx.write_error(id);
        }
      }
      ast::ExprAssignOp(op, ref lhs, ref rhs) => {
        check_binop(fcx, expr, op, &**lhs, rhs, BinopAssignment);

        let lhs_t = fcx.expr_ty(&**lhs);
        let result_t = fcx.expr_ty(expr);
        demand::suptype(fcx, expr.span, result_t, lhs_t);

        let tcx = fcx.tcx();
        if !ty::expr_is_lval(tcx, &**lhs) {
            span_err!(tcx.sess, lhs.span, E0067, "illegal left-hand side expression");
        }

        fcx.require_expr_have_sized_type(&**lhs, traits::AssignmentLhsSized);

        // Overwrite result of check_binop...this preserves existing behavior
        // but seems quite dubious with regard to user-defined methods
        // and so forth. - Niko
        if !ty::type_is_error(result_t) {
            fcx.write_nil(expr.id);
        }
      }
      ast::ExprUnary(unop, ref oprnd) => {
        let expected_inner = expected.map(fcx, |ty| {
            match unop {
                ast::UnUniq => match ty.sty {
                    ty::ty_uniq(ty) => {
                        Expectation::rvalue_hint(ty)
                    }
                    _ => {
                        NoExpectation
                    }
                },
                ast::UnNot | ast::UnNeg => {
                    expected
                }
                ast::UnDeref => {
                    NoExpectation
                }
            }
        });
        let lvalue_pref = match unop {
            ast::UnDeref => lvalue_pref,
            _ => NoPreference
        };
        check_expr_with_expectation_and_lvalue_pref(
            fcx, &**oprnd, expected_inner, lvalue_pref);
        let mut oprnd_t = fcx.expr_ty(&**oprnd);

        if !ty::type_is_error(oprnd_t) {
            match unop {
                ast::UnUniq => {
                    oprnd_t = ty::mk_uniq(tcx, oprnd_t);
                }
                ast::UnDeref => {
                    oprnd_t = structurally_resolved_type(fcx, expr.span, oprnd_t);
                    oprnd_t = match ty::deref(oprnd_t, true) {
                        Some(mt) => mt.ty,
                        None => match try_overloaded_deref(fcx, expr.span,
                                                           Some(MethodCall::expr(expr.id)),
                                                           Some(&**oprnd), oprnd_t, lvalue_pref) {
                            Some(mt) => mt.ty,
                            None => {
                                let is_newtype = match oprnd_t.sty {
                                    ty::ty_struct(did, substs) => {
                                        let fields = ty::struct_fields(fcx.tcx(), did, substs);
                                        fields.len() == 1
                                        && fields[0].name ==
                                        token::special_idents::unnamed_field.name
                                    }
                                    _ => false
                                };
                                if is_newtype {
                                    // This is an obsolete struct deref
                                    span_err!(tcx.sess, expr.span, E0068,
                                        "single-field tuple-structs can \
                                         no longer be dereferenced");
                                } else {
                                    fcx.type_error_message(expr.span, |actual| {
                                        format!("type `{}` cannot be \
                                                dereferenced", actual)
                                    }, oprnd_t, None);
                                }
                                tcx.types.err
                            }
                        }
                    };
                }
                ast::UnNot => {
                    oprnd_t = structurally_resolved_type(fcx, oprnd.span,
                                                         oprnd_t);
                    if !(ty::type_is_integral(oprnd_t) ||
                         oprnd_t.sty == ty::ty_bool) {
                        oprnd_t = check_user_unop(fcx, "!", "not",
                                                  tcx.lang_items.not_trait(),
                                                  expr, &**oprnd, oprnd_t, unop);
                    }
                }
                ast::UnNeg => {
                    oprnd_t = structurally_resolved_type(fcx, oprnd.span,
                                                         oprnd_t);
                    if !(ty::type_is_integral(oprnd_t) ||
                         ty::type_is_fp(oprnd_t)) {
                        oprnd_t = check_user_unop(fcx, "-", "neg",
                                                  tcx.lang_items.neg_trait(),
                                                  expr, &**oprnd, oprnd_t, unop);
                    }
                }
            }
        }
        fcx.write_ty(id, oprnd_t);
      }
      ast::ExprAddrOf(mutbl, ref oprnd) => {
        let expected = expected.only_has_type();
        let hint = expected.map(fcx, |ty| {
            match ty.sty {
                ty::ty_rptr(_, ref mt) | ty::ty_ptr(ref mt) => {
                    if ty::expr_is_lval(fcx.tcx(), &**oprnd) {
                        // Lvalues may legitimately have unsized types.
                        // For example, dereferences of a fat pointer and
                        // the last field of a struct can be unsized.
                        ExpectHasType(mt.ty)
                    } else {
                        Expectation::rvalue_hint(mt.ty)
                    }
                }
                _ => NoExpectation
            }
        });
        let lvalue_pref = match mutbl {
            ast::MutMutable => PreferMutLvalue,
            ast::MutImmutable => NoPreference
        };
        check_expr_with_expectation_and_lvalue_pref(fcx,
                                                    &**oprnd,
                                                    hint,
                                                    lvalue_pref);

        let tm = ty::mt { ty: fcx.expr_ty(&**oprnd), mutbl: mutbl };
        let oprnd_t = if ty::type_is_error(tm.ty) {
            tcx.types.err
        } else {
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
            match oprnd.node {
                // String literals are already, implicitly converted to slices.
                //ast::ExprLit(lit) if ast_util::lit_is_str(lit) => fcx.expr_ty(oprnd),
                // Empty slices live in static memory.
                ast::ExprVec(ref elements) if elements.len() == 0 => {
                    // Note: we do not assign a lifetime of
                    // static. This is because the resulting type
                    // `&'static [T]` would require that T outlives
                    // `'static`!
                    let region = fcx.infcx().next_region_var(
                        infer::AddrOfSlice(expr.span));
                    ty::mk_rptr(tcx, tcx.mk_region(region), tm)
                }
                _ => {
                    let region = fcx.infcx().next_region_var(infer::AddrOfRegion(expr.span));
                    ty::mk_rptr(tcx, tcx.mk_region(region), tm)
                }
            }
        };
        fcx.write_ty(id, oprnd_t);
      }
      ast::ExprPath(ref pth) => {
          let defn = lookup_def(fcx, pth.span, id);
          let pty = type_scheme_for_def(fcx, expr.span, defn);
          instantiate_path(fcx, pth, pty, defn, expr.span, expr.id);

          // We always require that the type provided as the value for
          // a type parameter outlives the moment of instantiation.
          constrain_path_type_parameters(fcx, expr);
      }
      ast::ExprInlineAsm(ref ia) => {
          for &(_, ref input) in ia.inputs.iter() {
              check_expr(fcx, &**input);
          }
          for &(_, ref out, _) in ia.outputs.iter() {
              check_expr(fcx, &**out);
          }
          fcx.write_nil(id);
      }
      ast::ExprMac(_) => tcx.sess.bug("unexpanded macro"),
      ast::ExprBreak(_) => { fcx.write_ty(id, fcx.infcx().next_diverging_ty_var()); }
      ast::ExprAgain(_) => { fcx.write_ty(id, fcx.infcx().next_diverging_ty_var()); }
      ast::ExprRet(ref expr_opt) => {
        match fcx.ret_ty {
            ty::FnConverging(result_type) => {
                match *expr_opt {
                    None =>
                        if let Err(_) = fcx.mk_eqty(false, infer::Misc(expr.span),
                                                    result_type, ty::mk_nil(fcx.tcx())) {
                            span_err!(tcx.sess, expr.span, E0069,
                                "`return;` in function returning non-nil");
                        },
                    Some(ref e) => {
                        check_expr_coercable_to_type(fcx, &**e, result_type);
                    }
                }
            }
            ty::FnDiverging => {
                if let Some(ref e) = *expr_opt {
                    check_expr(fcx, &**e);
                }
                span_err!(tcx.sess, expr.span, E0166,
                    "`return` in a function declared as diverging");
            }
        }
        fcx.write_ty(id, fcx.infcx().next_diverging_ty_var());
      }
      ast::ExprParen(ref a) => {
        check_expr_with_expectation_and_lvalue_pref(fcx,
                                                    &**a,
                                                    expected,
                                                    lvalue_pref);
        fcx.write_ty(id, fcx.expr_ty(&**a));
      }
      ast::ExprAssign(ref lhs, ref rhs) => {
        check_expr_with_lvalue_pref(fcx, &**lhs, PreferMutLvalue);

        let tcx = fcx.tcx();
        if !ty::expr_is_lval(tcx, &**lhs) {
            span_err!(tcx.sess, expr.span, E0070,
                "illegal left-hand side expression");
        }

        let lhs_ty = fcx.expr_ty(&**lhs);
        check_expr_coercable_to_type(fcx, &**rhs, lhs_ty);
        let rhs_ty = fcx.expr_ty(&**rhs);

        fcx.require_expr_have_sized_type(&**lhs, traits::AssignmentLhsSized);

        if ty::type_is_error(lhs_ty) || ty::type_is_error(rhs_ty) {
            fcx.write_error(id);
        } else {
            fcx.write_nil(id);
        }
      }
      ast::ExprIf(ref cond, ref then_blk, ref opt_else_expr) => {
        check_then_else(fcx, &**cond, &**then_blk, opt_else_expr.as_ref().map(|e| &**e),
                        id, expr.span, expected);
      }
      ast::ExprIfLet(..) => {
        tcx.sess.span_bug(expr.span, "non-desugared ExprIfLet");
      }
      ast::ExprWhile(ref cond, ref body, _) => {
        check_expr_has_type(fcx, &**cond, tcx.types.bool);
        check_block_no_value(fcx, &**body);
        let cond_ty = fcx.expr_ty(&**cond);
        let body_ty = fcx.node_ty(body.id);
        if ty::type_is_error(cond_ty) || ty::type_is_error(body_ty) {
            fcx.write_error(id);
        }
        else {
            fcx.write_nil(id);
        }
      }
      ast::ExprWhileLet(..) => {
        tcx.sess.span_bug(expr.span, "non-desugared ExprWhileLet");
      }
      ast::ExprForLoop(ref pat, ref head, ref block, _) => {
        check_expr(fcx, &**head);
        let typ = lookup_method_for_for_loop(fcx, &**head, expr.id);
        vtable::select_new_fcx_obligations(fcx);

        debug!("ExprForLoop each item has type {}",
               fcx.infcx().resolve_type_vars_if_possible(&typ).repr(fcx.tcx()));

        let pcx = pat_ctxt {
            fcx: fcx,
            map: pat_id_map(&tcx.def_map, &**pat),
        };
        _match::check_pat(&pcx, &**pat, typ);

        check_block_no_value(fcx, &**block);
        fcx.write_nil(id);
      }
      ast::ExprLoop(ref body, _) => {
        check_block_no_value(fcx, &**body);
        if !may_break(tcx, expr.id, &**body) {
            fcx.write_ty(id, fcx.infcx().next_diverging_ty_var());
        } else {
            fcx.write_nil(id);
        }
      }
      ast::ExprMatch(ref discrim, ref arms, match_src) => {
        _match::check_match(fcx, expr, &**discrim, arms.as_slice(), expected, match_src);
      }
      ast::ExprClosure(capture, opt_kind, ref decl, ref body) => {
          closure::check_expr_closure(fcx, expr, capture, opt_kind, &**decl, &**body, expected);
      }
      ast::ExprBlock(ref b) => {
        check_block_with_expected(fcx, &**b, expected);
        fcx.write_ty(id, fcx.node_ty(b.id));
      }
      ast::ExprCall(ref callee, ref args) => {
          callee::check_call(fcx, expr, &**callee, args.as_slice());
      }
      ast::ExprMethodCall(ident, ref tps, ref args) => {
        check_method_call(fcx, expr, ident, args[], tps[], lvalue_pref);
        let arg_tys = args.iter().map(|a| fcx.expr_ty(&**a));
        let  args_err = arg_tys.fold(false,
             |rest_err, a| {
              rest_err || ty::type_is_error(a)});
        if args_err {
            fcx.write_error(id);
        }
      }
      ast::ExprCast(ref e, ref t) => {
        if let ast::TyFixedLengthVec(_, ref count_expr) = t.node {
            check_expr_with_hint(fcx, &**count_expr, tcx.types.uint);
        }
        check_cast(fcx, expr, &**e, &**t);
      }
      ast::ExprVec(ref args) => {
        let uty = expected.map_to_option(fcx, |uty| {
            match uty.sty {
                ty::ty_vec(ty, _) => Some(ty),
                _ => None
            }
        });

        let typ = match uty {
            Some(uty) => {
                for e in args.iter() {
                    check_expr_coercable_to_type(fcx, &**e, uty);
                }
                uty
            }
            None => {
                let t: Ty = fcx.infcx().next_ty_var();
                for e in args.iter() {
                    check_expr_has_type(fcx, &**e, t);
                }
                t
            }
        };
        let typ = ty::mk_vec(tcx, typ, Some(args.len()));
        fcx.write_ty(id, typ);
      }
      ast::ExprRepeat(ref element, ref count_expr) => {
        check_expr_has_type(fcx, &**count_expr, tcx.types.uint);
        let count = ty::eval_repeat_count(fcx.tcx(), &**count_expr);

        let uty = match expected {
            ExpectHasType(uty) => {
                match uty.sty {
                    ty::ty_vec(ty, _) => Some(ty),
                    _ => None
                }
            }
            _ => None
        };

        let (element_ty, t) = match uty {
            Some(uty) => {
                check_expr_coercable_to_type(fcx, &**element, uty);
                (uty, uty)
            }
            None => {
                let t: Ty = fcx.infcx().next_ty_var();
                check_expr_has_type(fcx, &**element, t);
                (fcx.expr_ty(&**element), t)
            }
        };

        if count > 1 {
            // For [foo, ..n] where n > 1, `foo` must have
            // Copy type:
            fcx.require_type_meets(
                t,
                expr.span,
                traits::RepeatVec,
                ty::BoundCopy);
        }

        if ty::type_is_error(element_ty) {
            fcx.write_error(id);
        } else {
            let t = ty::mk_vec(tcx, t, Some(count));
            fcx.write_ty(id, t);
        }
      }
      ast::ExprTup(ref elts) => {
        let expected = expected.only_has_type();
        let flds = expected.map_to_option(fcx, |ty| {
            match ty.sty {
                ty::ty_tup(ref flds) => Some(flds[]),
                _ => None
            }
        });
        let mut err_field = false;

        let elt_ts = elts.iter().enumerate().map(|(i, e)| {
            let t = match flds {
                Some(ref fs) if i < fs.len() => {
                    let ety = fs[i];
                    check_expr_coercable_to_type(fcx, &**e, ety);
                    ety
                }
                _ => {
                    check_expr_with_expectation(fcx, &**e, NoExpectation);
                    fcx.expr_ty(&**e)
                }
            };
            err_field = err_field || ty::type_is_error(t);
            t
        }).collect();
        if err_field {
            fcx.write_error(id);
        } else {
            let typ = ty::mk_tup(tcx, elt_ts);
            fcx.write_ty(id, typ);
        }
      }
      ast::ExprStruct(ref path, ref fields, ref base_expr) => {
        // Resolve the path.
        let def = tcx.def_map.borrow().get(&id).map(|i| *i);
        let struct_id = match def {
            Some(def::DefVariant(enum_id, variant_id, true)) => {
                check_struct_enum_variant(fcx, id, expr.span, enum_id,
                                          variant_id, fields[]);
                enum_id
            }
            Some(def::DefTrait(def_id)) => {
                span_err!(tcx.sess, path.span, E0159,
                    "use of trait `{}` as a struct constructor",
                    pprust::path_to_string(path));
                check_struct_fields_on_error(fcx,
                                             id,
                                             fields[],
                                             base_expr);
                def_id
            },
            Some(def) => {
                // Verify that this was actually a struct.
                let typ = ty::lookup_item_type(fcx.ccx.tcx, def.def_id());
                match typ.ty.sty {
                    ty::ty_struct(struct_did, _) => {
                        check_struct_constructor(fcx,
                                                 id,
                                                 expr.span,
                                                 struct_did,
                                                 fields[],
                                                 base_expr.as_ref().map(|e| &**e));
                    }
                    _ => {
                        span_err!(tcx.sess, path.span, E0071,
                            "`{}` does not name a structure",
                            pprust::path_to_string(path));
                        check_struct_fields_on_error(fcx,
                                                     id,
                                                     fields[],
                                                     base_expr);
                    }
                }

                def.def_id()
            }
            _ => {
                tcx.sess.span_bug(path.span,
                                  "structure constructor wasn't resolved")
            }
        };

        // Turn the path into a type and verify that that type unifies with
        // the resulting structure type. This is needed to handle type
        // parameters correctly.
        let actual_structure_type = fcx.expr_ty(&*expr);
        if !ty::type_is_error(actual_structure_type) {
            let type_and_substs = astconv::ast_path_to_ty_relaxed(fcx,
                                                                  fcx,
                                                                  struct_id,
                                                                  path);
            match fcx.mk_subty(false,
                               infer::Misc(path.span),
                               actual_structure_type,
                               type_and_substs.ty) {
                Ok(()) => {}
                Err(type_error) => {
                    let type_error_description =
                        ty::type_err_to_str(tcx, &type_error);
                    fcx.tcx()
                       .sess
                       .span_err(path.span,
                                 format!("structure constructor specifies a \
                                         structure of type `{}`, but this \
                                         structure has type `{}`: {}",
                                         fcx.infcx()
                                            .ty_to_string(type_and_substs.ty),
                                         fcx.infcx()
                                            .ty_to_string(
                                                actual_structure_type),
                                         type_error_description)[]);
                    ty::note_and_explain_type_err(tcx, &type_error);
                }
            }
        }

        fcx.require_expr_have_sized_type(expr, traits::StructInitializerSized);
      }
      ast::ExprField(ref base, ref field) => {
        check_field(fcx, expr, lvalue_pref, &**base, field);
      }
      ast::ExprTupField(ref base, idx) => {
        check_tup_field(fcx, expr, lvalue_pref, &**base, idx);
      }
      ast::ExprIndex(ref base, ref idx) => {
          check_expr_with_lvalue_pref(fcx, &**base, lvalue_pref);
          let base_t = fcx.expr_ty(&**base);
          if ty::type_is_error(base_t) {
              fcx.write_ty(id, base_t);
          } else {
              match idx.node {
                ast::ExprRange(ref start, ref end) => {
                  // A slice, rather than an index. Special cased for now (KILLME).
                  let base_t = structurally_resolved_type(fcx, expr.span, base_t);

                  let result =
                      autoderef_for_index(fcx, &**base, base_t, lvalue_pref, |adj_ty, adj| {
                          try_overloaded_slice_step(fcx,
                                                    MethodCall::expr(expr.id),
                                                    expr,
                                                    &**base,
                                                    adj_ty,
                                                    adj,
                                                    lvalue_pref,
                                                    start,
                                                    end)
                      });

                  let mut args = vec![];
                  start.as_ref().map(|x| args.push(x));
                  end.as_ref().map(|x| args.push(x));

                  match result {
                      Some((index_ty, element_ty)) => {
                          for a in args.iter() {
                            check_expr_has_type(fcx, &***a, index_ty);
                          }
                          fcx.write_ty(idx.id, element_ty);
                          fcx.write_ty(id, element_ty)
                      }
                      _ => {
                          for a in args.iter() {
                            check_expr(fcx, &***a);
                          }
                          fcx.type_error_message(expr.span,
                             |actual| {
                                  format!("cannot take a slice of a value with type `{}`",
                                          actual)
                             },
                             base_t,
                             None);
                          fcx.write_ty(idx.id, fcx.tcx().types.err);
                          fcx.write_ty(id, fcx.tcx().types.err);
                      }
                  }
                }
                _ => {
                  check_expr(fcx, &**idx);
                  let idx_t = fcx.expr_ty(&**idx);
                  if ty::type_is_error(idx_t) {
                      fcx.write_ty(id, idx_t);
                  } else {
                      let base_t = structurally_resolved_type(fcx, expr.span, base_t);

                      let result =
                          autoderef_for_index(fcx, &**base, base_t, lvalue_pref, |adj_ty, adj| {
                              try_index_step(fcx,
                                             MethodCall::expr(expr.id),
                                             expr,
                                             &**base,
                                             adj_ty,
                                             adj,
                                             lvalue_pref)
                          });

                      match result {
                          Some((index_ty, element_ty)) => {
                              check_expr_has_type(fcx, &**idx, index_ty);
                              fcx.write_ty(id, element_ty);
                          }
                          _ => {
                              check_expr_has_type(fcx, &**idx, fcx.tcx().types.err);
                              fcx.type_error_message(
                                  expr.span,
                                  |actual| {
                                      format!("cannot index a value of type `{}`",
                                              actual)
                                  },
                                  base_t,
                                  None);
                              fcx.write_ty(id, fcx.tcx().types.err);
                          }
                      }
                  }
                }
              }
          }
       }
       ast::ExprRange(ref start, ref end) => {
          let t_start = start.as_ref().map(|e| {
            check_expr(fcx, &**e);
            fcx.expr_ty(&**e)
          });
          let t_end = end.as_ref().map(|e| {
            check_expr(fcx, &**e);
            fcx.expr_ty(&**e)
          });

          let idx_type = match (t_start, t_end) {
              (Some(ty), None) | (None, Some(ty)) => {
                  Some(ty)
              }
              (Some(t_start), Some(t_end)) if (ty::type_is_error(t_start) ||
                                               ty::type_is_error(t_end)) => {
                  Some(fcx.tcx().types.err)
              }
              (Some(t_start), Some(t_end)) => {
                  Some(infer::common_supertype(fcx.infcx(),
                                               infer::RangeExpression(expr.span),
                                               true,
                                               t_start,
                                               t_end))
              }
              _ => None
          };

          // Note that we don't check the type of start/end satisfy any
          // bounds because right the range structs do not have any. If we add
          // some bounds, then we'll need to check `t_start` against them here.

          let range_type = match idx_type {
            Some(idx_type) if ty::type_is_error(idx_type) => {
                fcx.tcx().types.err
            }
            Some(idx_type) => {
                // Find the did from the appropriate lang item.
                let did = match (start, end) {
                    (&Some(_), &Some(_)) => tcx.lang_items.range_struct(),
                    (&Some(_), &None) => tcx.lang_items.range_from_struct(),
                    (&None, &Some(_)) => tcx.lang_items.range_to_struct(),
                    (&None, &None) => {
                        tcx.sess.span_bug(expr.span, "full range should be dealt with above")
                    }
                };

                if let Some(did) = did {
                    let polytype = ty::lookup_item_type(tcx, did);
                    let substs = Substs::new_type(vec![idx_type], vec![]);
                    let bounds = fcx.instantiate_bounds(expr.span, &substs, &polytype.generics);
                    fcx.add_obligations_for_parameters(
                        traits::ObligationCause::new(expr.span,
                                                     fcx.body_id,
                                                     traits::ItemObligation(did)),
                        &bounds);

                    ty::mk_struct(tcx, did, tcx.mk_substs(substs))
                } else {
                    tcx.sess.span_err(expr.span, "No lang item for range syntax");
                    fcx.tcx().types.err
                }
            }
            None => {
                // Neither start nor end => FullRange
                if let Some(did) = tcx.lang_items.full_range_struct() {
                    let substs = Substs::new_type(vec![], vec![]);
                    ty::mk_struct(tcx, did, tcx.mk_substs(substs))
                } else {
                    tcx.sess.span_err(expr.span, "No lang item for range syntax");
                    fcx.tcx().types.err
                }
            }
          };

          fcx.write_ty(id, range_type);
       }

    }

    debug!("type of expr({}) {} is...", expr.id,
           syntax::print::pprust::expr_to_string(expr));
    debug!("... {}, expected is {}",
           ppaux::ty_to_string(tcx, fcx.expr_ty(expr)),
           expected.repr(tcx));

    unifier();
}

fn constrain_path_type_parameters(fcx: &FnCtxt,
                                  expr: &ast::Expr)
{
    fcx.opt_node_ty_substs(expr.id, |item_substs| {
        fcx.add_default_region_param_bounds(&item_substs.substs, expr);
    });
}

impl<'tcx> Expectation<'tcx> {
    /// Provide an expectation for an rvalue expression given an *optional*
    /// hint, which is not required for type safety (the resulting type might
    /// be checked higher up, as is the case with `&expr` and `box expr`), but
    /// is useful in determining the concrete type.
    ///
    /// The primary use case is where the expected type is a fat pointer,
    /// like `&[int]`. For example, consider the following statement:
    ///
    ///    let x: &[int] = &[1, 2, 3];
    ///
    /// In this case, the expected type for the `&[1, 2, 3]` expression is
    /// `&[int]`. If however we were to say that `[1, 2, 3]` has the
    /// expectation `ExpectHasType([int])`, that would be too strong --
    /// `[1, 2, 3]` does not have the type `[int]` but rather `[int; 3]`.
    /// It is only the `&[1, 2, 3]` expression as a whole that can be coerced
    /// to the type `&[int]`. Therefore, we propagate this more limited hint,
    /// which still is useful, because it informs integer literals and the like.
    /// See the test case `test/run-pass/coerce-expect-unsized.rs` and #20169
    /// for examples of where this comes up,.
    fn rvalue_hint(ty: Ty<'tcx>) -> Expectation<'tcx> {
        match ty.sty {
            ty::ty_vec(_, None) | ty::ty_trait(..) => {
                ExpectRvalueLikeUnsized(ty)
            }
            _ => ExpectHasType(ty)
        }
    }

    fn only_has_type(self) -> Expectation<'tcx> {
        match self {
            ExpectHasType(t) => ExpectHasType(t),
            _ => NoExpectation
        }
    }

    // Resolves `expected` by a single level if it is a variable. If
    // there is no expected type or resolution is not possible (e.g.,
    // no constraints yet present), just returns `None`.
    fn resolve<'a>(self, fcx: &FnCtxt<'a, 'tcx>) -> Expectation<'tcx> {
        match self {
            NoExpectation => {
                NoExpectation
            }
            ExpectCastableToType(t) => {
                ExpectCastableToType(
                    fcx.infcx().resolve_type_vars_if_possible(&t))
            }
            ExpectHasType(t) => {
                ExpectHasType(
                    fcx.infcx().resolve_type_vars_if_possible(&t))
            }
            ExpectRvalueLikeUnsized(t) => {
                ExpectRvalueLikeUnsized(
                    fcx.infcx().resolve_type_vars_if_possible(&t))
            }
        }
    }

    fn map<'a, F>(self, fcx: &FnCtxt<'a, 'tcx>, unpack: F) -> Expectation<'tcx> where
        F: FnOnce(Ty<'tcx>) -> Expectation<'tcx>
    {
        match self.resolve(fcx) {
            NoExpectation => NoExpectation,
            ExpectCastableToType(ty) |
            ExpectHasType(ty) |
            ExpectRvalueLikeUnsized(ty) => unpack(ty),
        }
    }

    fn map_to_option<'a, O, F>(self, fcx: &FnCtxt<'a, 'tcx>, unpack: F) -> Option<O> where
        F: FnOnce(Ty<'tcx>) -> Option<O>,
    {
        match self.resolve(fcx) {
            NoExpectation => None,
            ExpectCastableToType(ty) |
            ExpectHasType(ty) |
            ExpectRvalueLikeUnsized(ty) => unpack(ty),
        }
    }
}

impl<'tcx> Repr<'tcx> for Expectation<'tcx> {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        match *self {
            NoExpectation => format!("NoExpectation"),
            ExpectHasType(t) => format!("ExpectHasType({})",
                                        t.repr(tcx)),
            ExpectCastableToType(t) => format!("ExpectCastableToType({})",
                                               t.repr(tcx)),
            ExpectRvalueLikeUnsized(t) => format!("ExpectRvalueLikeUnsized({})",
                                                  t.repr(tcx)),
        }
    }
}

pub fn check_decl_initializer(fcx: &FnCtxt,
                              nid: ast::NodeId,
                              init: &ast::Expr)
{
    let local_ty = fcx.local_ty(init.span, nid);
    check_expr_coercable_to_type(fcx, init, local_ty)
}

pub fn check_decl_local(fcx: &FnCtxt, local: &ast::Local)  {
    let tcx = fcx.ccx.tcx;

    let t = fcx.local_ty(local.span, local.id);
    fcx.write_ty(local.id, t);

    if let Some(ref init) = local.init {
        check_decl_initializer(fcx, local.id, &**init);
        let init_ty = fcx.expr_ty(&**init);
        if ty::type_is_error(init_ty) {
            fcx.write_ty(local.id, init_ty);
        }
    }

    let pcx = pat_ctxt {
        fcx: fcx,
        map: pat_id_map(&tcx.def_map, &*local.pat),
    };
    _match::check_pat(&pcx, &*local.pat, t);
    let pat_ty = fcx.node_ty(local.pat.id);
    if ty::type_is_error(pat_ty) {
        fcx.write_ty(local.id, pat_ty);
    }
}

pub fn check_stmt(fcx: &FnCtxt, stmt: &ast::Stmt)  {
    let node_id;
    let mut saw_bot = false;
    let mut saw_err = false;
    match stmt.node {
      ast::StmtDecl(ref decl, id) => {
        node_id = id;
        match decl.node {
          ast::DeclLocal(ref l) => {
              check_decl_local(fcx, &**l);
              let l_t = fcx.node_ty(l.id);
              saw_bot = saw_bot || fcx.infcx().type_var_diverges(l_t);
              saw_err = saw_err || ty::type_is_error(l_t);
          }
          ast::DeclItem(_) => {/* ignore for now */ }
        }
      }
      ast::StmtExpr(ref expr, id) => {
        node_id = id;
        // Check with expected type of ()
        check_expr_has_type(fcx, &**expr, ty::mk_nil(fcx.tcx()));
        let expr_ty = fcx.expr_ty(&**expr);
        saw_bot = saw_bot || fcx.infcx().type_var_diverges(expr_ty);
        saw_err = saw_err || ty::type_is_error(expr_ty);
      }
      ast::StmtSemi(ref expr, id) => {
        node_id = id;
        check_expr(fcx, &**expr);
        let expr_ty = fcx.expr_ty(&**expr);
        saw_bot |= fcx.infcx().type_var_diverges(expr_ty);
        saw_err |= ty::type_is_error(expr_ty);
      }
      ast::StmtMac(..) => fcx.ccx.tcx.sess.bug("unexpanded macro")
    }
    if saw_bot {
        fcx.write_ty(node_id, fcx.infcx().next_diverging_ty_var());
    }
    else if saw_err {
        fcx.write_error(node_id);
    }
    else {
        fcx.write_nil(node_id)
    }
}

pub fn check_block_no_value(fcx: &FnCtxt, blk: &ast::Block)  {
    check_block_with_expected(fcx, blk, ExpectHasType(ty::mk_nil(fcx.tcx())));
    let blkty = fcx.node_ty(blk.id);
    if ty::type_is_error(blkty) {
        fcx.write_error(blk.id);
    } else {
        let nilty = ty::mk_nil(fcx.tcx());
        demand::suptype(fcx, blk.span, nilty, blkty);
    }
}

fn check_block_with_expected<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                       blk: &ast::Block,
                                       expected: Expectation<'tcx>) {
    let prev = {
        let mut fcx_ps = fcx.ps.borrow_mut();
        let unsafety_state = fcx_ps.recurse(blk);
        replace(&mut *fcx_ps, unsafety_state)
    };

    let mut warned = false;
    let mut any_diverges = false;
    let mut any_err = false;
    for s in blk.stmts.iter() {
        check_stmt(fcx, &**s);
        let s_id = ast_util::stmt_id(&**s);
        let s_ty = fcx.node_ty(s_id);
        if any_diverges && !warned && match s.node {
            ast::StmtDecl(ref decl, _) => {
                match decl.node {
                    ast::DeclLocal(_) => true,
                    _ => false,
                }
            }
            ast::StmtExpr(_, _) | ast::StmtSemi(_, _) => true,
            _ => false
        } {
            fcx.ccx
                .tcx
                .sess
                .add_lint(lint::builtin::UNREACHABLE_CODE,
                          s_id,
                          s.span,
                          "unreachable statement".to_string());
            warned = true;
        }
        any_diverges = any_diverges || fcx.infcx().type_var_diverges(s_ty);
        any_err = any_err || ty::type_is_error(s_ty);
    }
    match blk.expr {
        None => if any_err {
            fcx.write_error(blk.id);
        } else if any_diverges {
            fcx.write_ty(blk.id, fcx.infcx().next_diverging_ty_var());
        } else {
            fcx.write_nil(blk.id);
        },
        Some(ref e) => {
            if any_diverges && !warned {
                fcx.ccx
                    .tcx
                    .sess
                    .add_lint(lint::builtin::UNREACHABLE_CODE,
                              e.id,
                              e.span,
                              "unreachable expression".to_string());
            }
            let ety = match expected {
                ExpectHasType(ety) => {
                    check_expr_coercable_to_type(fcx, &**e, ety);
                    ety
                }
                _ => {
                    check_expr_with_expectation(fcx, &**e, expected);
                    fcx.expr_ty(&**e)
                }
            };

            if any_err {
                fcx.write_error(blk.id);
            } else if any_diverges {
                fcx.write_ty(blk.id, fcx.infcx().next_diverging_ty_var());
            } else {
                fcx.write_ty(blk.id, ety);
            }
        }
    };

    *fcx.ps.borrow_mut() = prev;
}

/// Checks a constant appearing in a type. At the moment this is just the
/// length expression in a fixed-length vector, but someday it might be
/// extended to type-level numeric literals.
fn check_const_in_type<'a,'tcx>(ccx: &'a CrateCtxt<'a,'tcx>,
                                expr: &ast::Expr,
                                expected_type: Ty<'tcx>) {
    let inh = static_inherited_fields(ccx);
    let fcx = blank_fn_ctxt(ccx, &inh, ty::FnConverging(expected_type), expr.id);
    check_const_with_ty(&fcx, expr.span, expr, expected_type);
}

fn check_const(ccx: &CrateCtxt,
               sp: Span,
               e: &ast::Expr,
               id: ast::NodeId) {
    let inh = static_inherited_fields(ccx);
    let rty = ty::node_id_to_type(ccx.tcx, id);
    let fcx = blank_fn_ctxt(ccx, &inh, ty::FnConverging(rty), e.id);
    let declty = (*fcx.ccx.tcx.tcache.borrow())[local_def(id)].ty;
    check_const_with_ty(&fcx, sp, e, declty);
}

fn check_const_with_ty<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                 _: Span,
                                 e: &ast::Expr,
                                 declty: Ty<'tcx>) {
    // Gather locals in statics (because of block expressions).
    // This is technically unnecessary because locals in static items are forbidden,
    // but prevents type checking from blowing up before const checking can properly
    // emit a error.
    GatherLocalsVisitor { fcx: fcx }.visit_expr(e);

    check_expr_with_hint(fcx, e, declty);
    demand::coerce(fcx, e.span, declty, e);
    vtable::select_all_fcx_obligations_or_error(fcx);
    regionck::regionck_expr(fcx, e);
    writeback::resolve_type_vars_in_expr(fcx, e);
}

/// Checks whether a type can be represented in memory. In particular, it
/// identifies types that contain themselves without indirection through a
/// pointer, which would mean their size is unbounded. This is different from
/// the question of whether a type can be instantiated. See the definition of
/// `check_instantiable`.
pub fn check_representable(tcx: &ty::ctxt,
                           sp: Span,
                           item_id: ast::NodeId,
                           designation: &str) -> bool {
    let rty = ty::node_id_to_type(tcx, item_id);

    // Check that it is possible to represent this type. This call identifies
    // (1) types that contain themselves and (2) types that contain a different
    // recursive type. It is only necessary to throw an error on those that
    // contain themselves. For case 2, there must be an inner type that will be
    // caught by case 1.
    match ty::is_type_representable(tcx, sp, rty) {
      ty::SelfRecursive => {
        span_err!(tcx.sess, sp, E0072,
            "illegal recursive {} type; \
             wrap the inner value in a box to make it representable",
            designation);
        return false
      }
      ty::Representable | ty::ContainsRecursive => (),
    }
    return true
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
pub fn check_instantiable(tcx: &ty::ctxt,
                          sp: Span,
                          item_id: ast::NodeId)
                          -> bool {
    let item_ty = ty::node_id_to_type(tcx, item_id);
    if !ty::is_instantiable(tcx, item_ty) {
        span_err!(tcx.sess, sp, E0073,
            "this type cannot be instantiated without an \
             instance of itself");
        span_help!(tcx.sess, sp, "consider using `Option<{}>`",
            ppaux::ty_to_string(tcx, item_ty));
        false
    } else {
        true
    }
}

pub fn check_simd(tcx: &ty::ctxt, sp: Span, id: ast::NodeId) {
    let t = ty::node_id_to_type(tcx, id);
    if ty::type_needs_subst(t) {
        span_err!(tcx.sess, sp, E0074, "SIMD vector cannot be generic");
        return;
    }
    match t.sty {
        ty::ty_struct(did, substs) => {
            let fields = ty::lookup_struct_fields(tcx, did);
            if fields.is_empty() {
                span_err!(tcx.sess, sp, E0075, "SIMD vector cannot be empty");
                return;
            }
            let e = ty::lookup_field_type(tcx, did, fields[0].id, substs);
            if !fields.iter().all(
                         |f| ty::lookup_field_type(tcx, did, f.id, substs) == e) {
                span_err!(tcx.sess, sp, E0076, "SIMD vector should be homogeneous");
                return;
            }
            if !ty::type_is_machine(e) {
                span_err!(tcx.sess, sp, E0077,
                    "SIMD vector element type should be machine type");
                return;
            }
        }
        _ => ()
    }
}

pub fn check_enum_variants(ccx: &CrateCtxt,
                           sp: Span,
                           vs: &[P<ast::Variant>],
                           id: ast::NodeId) {

    fn disr_in_range(ccx: &CrateCtxt,
                     ty: attr::IntType,
                     disr: ty::Disr) -> bool {
        fn uint_in_range(ccx: &CrateCtxt, ty: ast::UintTy, disr: ty::Disr) -> bool {
            match ty {
                ast::TyU8 => disr as u8 as Disr == disr,
                ast::TyU16 => disr as u16 as Disr == disr,
                ast::TyU32 => disr as u32 as Disr == disr,
                ast::TyU64 => disr as u64 as Disr == disr,
                ast::TyU => uint_in_range(ccx, ccx.tcx.sess.target.uint_type, disr)
            }
        }
        fn int_in_range(ccx: &CrateCtxt, ty: ast::IntTy, disr: ty::Disr) -> bool {
            match ty {
                ast::TyI8 => disr as i8 as Disr == disr,
                ast::TyI16 => disr as i16 as Disr == disr,
                ast::TyI32 => disr as i32 as Disr == disr,
                ast::TyI64 => disr as i64 as Disr == disr,
                ast::TyI => int_in_range(ccx, ccx.tcx.sess.target.int_type, disr)
            }
        }
        match ty {
            attr::UnsignedInt(ty) => uint_in_range(ccx, ty, disr),
            attr::SignedInt(ty) => int_in_range(ccx, ty, disr)
        }
    }

    fn do_check<'a, 'tcx>(ccx: &CrateCtxt<'a, 'tcx>,
                          vs: &[P<ast::Variant>],
                          id: ast::NodeId,
                          hint: attr::ReprAttr)
                          -> Vec<Rc<ty::VariantInfo<'tcx>>> {

        let rty = ty::node_id_to_type(ccx.tcx, id);
        let mut variants: Vec<Rc<ty::VariantInfo>> = Vec::new();
        let mut disr_vals: Vec<ty::Disr> = Vec::new();
        let mut prev_disr_val: Option<ty::Disr> = None;

        for v in vs.iter() {

            // If the discriminant value is specified explicitly in the enum check whether the
            // initialization expression is valid, otherwise use the last value plus one.
            let mut current_disr_val = match prev_disr_val {
                Some(prev_disr_val) => prev_disr_val + 1,
                None => ty::INITIAL_DISCRIMINANT_VALUE
            };

            match v.node.disr_expr {
                Some(ref e) => {
                    debug!("disr expr, checking {}", pprust::expr_to_string(&**e));

                    let inh = static_inherited_fields(ccx);
                    let fcx = blank_fn_ctxt(ccx, &inh, ty::FnConverging(rty), e.id);
                    let declty = match hint {
                        attr::ReprAny | attr::ReprPacked | attr::ReprExtern => fcx.tcx().types.int,
                        attr::ReprInt(_, attr::SignedInt(ity)) => {
                            ty::mk_mach_int(fcx.tcx(), ity)
                        }
                        attr::ReprInt(_, attr::UnsignedInt(ity)) => {
                            ty::mk_mach_uint(fcx.tcx(), ity)
                        },
                    };
                    check_const_with_ty(&fcx, e.span, &**e, declty);
                    // check_expr (from check_const pass) doesn't guarantee
                    // that the expression is in a form that eval_const_expr can
                    // handle, so we may still get an internal compiler error

                    match const_eval::eval_const_expr_partial(ccx.tcx, &**e) {
                        Ok(const_eval::const_int(val)) => current_disr_val = val as Disr,
                        Ok(const_eval::const_uint(val)) => current_disr_val = val as Disr,
                        Ok(_) => {
                            span_err!(ccx.tcx.sess, e.span, E0079,
                                "expected signed integer constant");
                        }
                        Err(ref err) => {
                            span_err!(ccx.tcx.sess, e.span, E0080,
                                "expected constant: {}", *err);
                        }
                    }
                },
                None => ()
            };

            // Check for duplicate discriminant values
            match disr_vals.iter().position(|&x| x == current_disr_val) {
                Some(i) => {
                    span_err!(ccx.tcx.sess, v.span, E0081,
                        "discriminant value `{}` already exists", disr_vals[i]);
                    span_note!(ccx.tcx.sess, ccx.tcx.map.span(variants[i].id.node),
                        "conflicting discriminant here")
                }
                None => {}
            }
            // Check for unrepresentable discriminant values
            match hint {
                attr::ReprAny | attr::ReprExtern => (),
                attr::ReprInt(sp, ity) => {
                    if !disr_in_range(ccx, ity, current_disr_val) {
                        span_err!(ccx.tcx.sess, v.span, E0082,
                            "discriminant value outside specified type");
                        span_note!(ccx.tcx.sess, sp,
                            "discriminant type specified here");
                    }
                }
                attr::ReprPacked => {
                    ccx.tcx.sess.bug("range_to_inttype: found ReprPacked on an enum");
                }
            }
            disr_vals.push(current_disr_val);

            let variant_info = Rc::new(VariantInfo::from_ast_variant(ccx.tcx, &**v,
                                                                     current_disr_val));
            prev_disr_val = Some(current_disr_val);

            variants.push(variant_info);
        }

        return variants;
    }

    let hint = *ty::lookup_repr_hints(ccx.tcx, ast::DefId { krate: ast::LOCAL_CRATE, node: id })
                    [].get(0).unwrap_or(&attr::ReprAny);

    if hint != attr::ReprAny && vs.len() <= 1 {
        if vs.len() == 1 {
            span_err!(ccx.tcx.sess, sp, E0083,
                "unsupported representation for univariant enum");
        } else {
            span_err!(ccx.tcx.sess, sp, E0084,
                "unsupported representation for zero-variant enum");
        };
    }

    let variants = do_check(ccx, vs, id, hint);

    // cache so that ty::enum_variants won't repeat this work
    ccx.tcx.enum_var_cache.borrow_mut().insert(local_def(id), Rc::new(variants));

    check_representable(ccx.tcx, sp, id, "enum");

    // Check that it is possible to instantiate this enum:
    //
    // This *sounds* like the same that as representable, but it's
    // not.  See def'n of `check_instantiable()` for details.
    check_instantiable(ccx.tcx, sp, id);
}

pub fn lookup_def(fcx: &FnCtxt, sp: Span, id: ast::NodeId) -> def::Def {
    lookup_def_ccx(fcx.ccx, sp, id)
}

// Returns the type parameter count and the type for the given definition.
pub fn type_scheme_for_def<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                     sp: Span,
                                     defn: def::Def)
                                     -> TypeScheme<'tcx> {
    match defn {
      def::DefLocal(nid) | def::DefUpvar(nid, _, _) => {
          let typ = fcx.local_ty(sp, nid);
          return no_params(typ);
      }
      def::DefFn(id, _) | def::DefStaticMethod(id, _) | def::DefMethod(id, _, _) |
      def::DefStatic(id, _) | def::DefVariant(_, id, _) |
      def::DefStruct(id) | def::DefConst(id) => {
        return ty::lookup_item_type(fcx.ccx.tcx, id);
      }
      def::DefTrait(_) |
      def::DefTy(..) |
      def::DefAssociatedTy(..) |
      def::DefAssociatedPath(..) |
      def::DefPrimTy(_) |
      def::DefTyParam(..) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value, found type");
      }
      def::DefMod(..) | def::DefForeignMod(..) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value, found module");
      }
      def::DefUse(..) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value, found use");
      }
      def::DefRegion(..) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value, found region");
      }
      def::DefTyParamBinder(..) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value, found type parameter");
      }
      def::DefLabel(..) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value, found label");
      }
      def::DefSelfTy(..) => {
        fcx.ccx.tcx.sess.span_bug(sp, "expected value, found self ty");
      }
    }
}

// Instantiates the given path, which must refer to an item with the given
// number of type parameters and type.
pub fn instantiate_path<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                                  path: &ast::Path,
                                  type_scheme: TypeScheme<'tcx>,
                                  def: def::Def,
                                  span: Span,
                                  node_id: ast::NodeId) {
    debug!("instantiate_path(path={}, def={}, node_id={}, type_scheme={})",
           path.repr(fcx.tcx()),
           def.repr(fcx.tcx()),
           node_id,
           type_scheme.repr(fcx.tcx()));

    // We need to extract the type parameters supplied by the user in
    // the path `path`. Due to the current setup, this is a bit of a
    // tricky-process; the problem is that resolve only tells us the
    // end-point of the path resolution, and not the intermediate steps.
    // Luckily, we can (at least for now) deduce the intermediate steps
    // just from the end-point.
    //
    // There are basically three cases to consider:
    //
    // 1. Reference to a *type*, such as a struct or enum:
    //
    //        mod a { struct Foo<T> { ... } }
    //
    //    Because we don't allow types to be declared within one
    //    another, a path that leads to a type will always look like
    //    `a::b::Foo<T>` where `a` and `b` are modules. This implies
    //    that only the final segment can have type parameters, and
    //    they are located in the TypeSpace.
    //
    //    *Note:* Generally speaking, references to types don't
    //    actually pass through this function, but rather the
    //    `ast_ty_to_ty` function in `astconv`. However, in the case
    //    of struct patterns (and maybe literals) we do invoke
    //    `instantiate_path` to get the general type of an instance of
    //    a struct. (In these cases, there are actually no type
    //    parameters permitted at present, but perhaps we will allow
    //    them in the future.)
    //
    // 1b. Reference to a enum variant or tuple-like struct:
    //
    //        struct foo<T>(...)
    //        enum E<T> { foo(...) }
    //
    //    In these cases, the parameters are declared in the type
    //    space.
    //
    // 2. Reference to a *fn item*:
    //
    //        fn foo<T>() { }
    //
    //    In this case, the path will again always have the form
    //    `a::b::foo::<T>` where only the final segment should have
    //    type parameters. However, in this case, those parameters are
    //    declared on a value, and hence are in the `FnSpace`.
    //
    // 3. Reference to a *method*:
    //
    //        impl<A> SomeStruct<A> {
    //            fn foo<B>(...)
    //        }
    //
    //    Here we can have a path like
    //    `a::b::SomeStruct::<A>::foo::<B>`, in which case parameters
    //    may appear in two places. The penultimate segment,
    //    `SomeStruct::<A>`, contains parameters in TypeSpace, and the
    //    final segment, `foo::<B>` contains parameters in fn space.
    //
    // The first step then is to categorize the segments appropriately.

    assert!(path.segments.len() >= 1);
    let mut segment_spaces: Vec<_>;
    match def {
        // Case 1 and 1b. Reference to a *type* or *enum variant*.
        def::DefSelfTy(..) |
        def::DefStruct(..) |
        def::DefVariant(..) |
        def::DefTyParamBinder(..) |
        def::DefTy(..) |
        def::DefAssociatedTy(..) |
        def::DefAssociatedPath(..) |
        def::DefTrait(..) |
        def::DefPrimTy(..) |
        def::DefTyParam(..) => {
            // Everything but the final segment should have no
            // parameters at all.
            segment_spaces = repeat(None).take(path.segments.len() - 1).collect();
            segment_spaces.push(Some(subst::TypeSpace));
        }

        // Case 2. Reference to a top-level value.
        def::DefFn(..) |
        def::DefConst(..) |
        def::DefStatic(..) => {
            segment_spaces = repeat(None).take(path.segments.len() - 1).collect();
            segment_spaces.push(Some(subst::FnSpace));
        }

        // Case 3. Reference to a method.
        def::DefStaticMethod(_, providence) |
        def::DefMethod(_, _, providence) => {
            assert!(path.segments.len() >= 2);

            match providence {
                def::FromTrait(trait_did) => {
                    callee::check_legal_trait_for_method_call(fcx.ccx, span, trait_did)
                }
                def::FromImpl(_) => {}
            }

            segment_spaces = repeat(None).take(path.segments.len() - 2).collect();
            segment_spaces.push(Some(subst::TypeSpace));
            segment_spaces.push(Some(subst::FnSpace));
        }

        // Other cases. Various nonsense that really shouldn't show up
        // here. If they do, an error will have been reported
        // elsewhere. (I hope)
        def::DefMod(..) |
        def::DefForeignMod(..) |
        def::DefLocal(..) |
        def::DefUse(..) |
        def::DefRegion(..) |
        def::DefLabel(..) |
        def::DefUpvar(..) => {
            segment_spaces = repeat(None).take(path.segments.len()).collect();
        }
    }
    assert_eq!(segment_spaces.len(), path.segments.len());

    debug!("segment_spaces={}", segment_spaces);

    // Next, examine the definition, and determine how many type
    // parameters we expect from each space.
    let type_defs = &type_scheme.generics.types;
    let region_defs = &type_scheme.generics.regions;

    // Now that we have categorized what space the parameters for each
    // segment belong to, let's sort out the parameters that the user
    // provided (if any) into their appropriate spaces. We'll also report
    // errors if type parameters are provided in an inappropriate place.
    let mut substs = Substs::empty();
    for (opt_space, segment) in segment_spaces.iter().zip(path.segments.iter()) {
        match *opt_space {
            None => {
                report_error_if_segment_contains_type_parameters(fcx, segment);
            }

            Some(space) => {
                push_explicit_parameters_from_segment_to_substs(fcx,
                                                                space,
                                                                path.span,
                                                                type_defs,
                                                                region_defs,
                                                                segment,
                                                                &mut substs);
            }
        }
    }

    // Now we have to compare the types that the user *actually*
    // provided against the types that were *expected*. If the user
    // did not provide any types, then we want to substitute inference
    // variables. If the user provided some types, we may still need
    // to add defaults. If the user provided *too many* types, that's
    // a problem.
    for &space in ParamSpace::all().iter() {
        adjust_type_parameters(fcx, span, space, type_defs, &mut substs);
        assert_eq!(substs.types.len(space), type_defs.len(space));

        adjust_region_parameters(fcx, span, space, region_defs, &mut substs);
        assert_eq!(substs.regions().len(space), region_defs.len(space));
    }

    // The things we are substituting into the type should not contain
    // escaping late-bound regions, and nor should the base type scheme.
    assert!(!substs.has_regions_escaping_depth(0));
    assert!(!type_scheme.has_escaping_regions());

    // Add all the obligations that are required, substituting and
    // normalized appropriately.
    let bounds = fcx.instantiate_bounds(span, &substs, &type_scheme.generics);
    fcx.add_obligations_for_parameters(
        traits::ObligationCause::new(span, fcx.body_id, traits::ItemObligation(def.def_id())),
        &bounds);

    // Substitute the values for the type parameters into the type of
    // the referenced item.
    let ty_substituted = fcx.instantiate_type_scheme(span, &substs, &type_scheme.ty);

    fcx.write_ty(node_id, ty_substituted);
    fcx.write_substs(node_id, ty::ItemSubsts { substs: substs });
    return;

    fn report_error_if_segment_contains_type_parameters(
        fcx: &FnCtxt,
        segment: &ast::PathSegment)
    {
        for typ in segment.parameters.types().iter() {
            span_err!(fcx.tcx().sess, typ.span, E0085,
                "type parameters may not appear here");
            break;
        }

        for lifetime in segment.parameters.lifetimes().iter() {
            span_err!(fcx.tcx().sess, lifetime.span, E0086,
                "lifetime parameters may not appear here");
            break;
        }
    }

    /// Finds the parameters that the user provided and adds them to `substs`. If too many
    /// parameters are provided, then reports an error and clears the output vector.
    ///
    /// We clear the output vector because that will cause the `adjust_XXX_parameters()` later to
    /// use inference variables. This seems less likely to lead to derived errors.
    ///
    /// Note that we *do not* check for *too few* parameters here. Due to the presence of defaults
    /// etc that is more complicated. I wanted however to do the reporting of *too many* parameters
    /// here because we can easily use the precise span of the N+1'th parameter.
    fn push_explicit_parameters_from_segment_to_substs<'a, 'tcx>(
        fcx: &FnCtxt<'a, 'tcx>,
        space: subst::ParamSpace,
        span: Span,
        type_defs: &VecPerParamSpace<ty::TypeParameterDef<'tcx>>,
        region_defs: &VecPerParamSpace<ty::RegionParameterDef>,
        segment: &ast::PathSegment,
        substs: &mut Substs<'tcx>)
    {
        match segment.parameters {
            ast::AngleBracketedParameters(ref data) => {
                push_explicit_angle_bracketed_parameters_from_segment_to_substs(
                    fcx, space, type_defs, region_defs, data, substs);
            }

            ast::ParenthesizedParameters(ref data) => {
                fcx.tcx().sess.span_err(
                    span,
                    "parenthesized parameters may only be used with a trait");
                push_explicit_parenthesized_parameters_from_segment_to_substs(
                    fcx, space, span, type_defs, data, substs);
            }
        }
    }

    fn push_explicit_angle_bracketed_parameters_from_segment_to_substs<'a, 'tcx>(
        fcx: &FnCtxt<'a, 'tcx>,
        space: subst::ParamSpace,
        type_defs: &VecPerParamSpace<ty::TypeParameterDef<'tcx>>,
        region_defs: &VecPerParamSpace<ty::RegionParameterDef>,
        data: &ast::AngleBracketedParameterData,
        substs: &mut Substs<'tcx>)
    {
        {
            let type_count = type_defs.len(space);
            assert_eq!(substs.types.len(space), 0);
            for (i, typ) in data.types.iter().enumerate() {
                let t = fcx.to_ty(&**typ);
                if i < type_count {
                    substs.types.push(space, t);
                } else if i == type_count {
                    span_err!(fcx.tcx().sess, typ.span, E0087,
                        "too many type parameters provided: \
                         expected at most {} parameter(s), \
                         found {} parameter(s)",
                         type_count, data.types.len());
                    substs.types.truncate(space, 0);
                    break;
                }
            }
        }

        if data.bindings.len() > 0 {
            span_err!(fcx.tcx().sess, data.bindings[0].span, E0182,
                      "unexpected binding of associated item in expression path \
                       (only allowed in type paths)");
        }

        {
            let region_count = region_defs.len(space);
            assert_eq!(substs.regions().len(space), 0);
            for (i, lifetime) in data.lifetimes.iter().enumerate() {
                let r = ast_region_to_region(fcx.tcx(), lifetime);
                if i < region_count {
                    substs.mut_regions().push(space, r);
                } else if i == region_count {
                    span_err!(fcx.tcx().sess, lifetime.span, E0088,
                        "too many lifetime parameters provided: \
                         expected {} parameter(s), found {} parameter(s)",
                        region_count,
                        data.lifetimes.len());
                    substs.mut_regions().truncate(space, 0);
                    break;
                }
            }
        }
    }

    /// As with
    /// `push_explicit_angle_bracketed_parameters_from_segment_to_substs`,
    /// but intended for `Foo(A,B) -> C` form. This expands to
    /// roughly the same thing as `Foo<(A,B),C>`. One important
    /// difference has to do with the treatment of anonymous
    /// regions, which are translated into bound regions (NYI).
    fn push_explicit_parenthesized_parameters_from_segment_to_substs<'a, 'tcx>(
        fcx: &FnCtxt<'a, 'tcx>,
        space: subst::ParamSpace,
        span: Span,
        type_defs: &VecPerParamSpace<ty::TypeParameterDef<'tcx>>,
        data: &ast::ParenthesizedParameterData,
        substs: &mut Substs<'tcx>)
    {
        let type_count = type_defs.len(space);
        if type_count < 2 {
            span_err!(fcx.tcx().sess, span, E0167,
                      "parenthesized form always supplies 2 type parameters, \
                      but only {} parameter(s) were expected",
                      type_count);
        }

        let input_tys: Vec<Ty> =
            data.inputs.iter().map(|ty| fcx.to_ty(&**ty)).collect();

        let tuple_ty =
            ty::mk_tup(fcx.tcx(), input_tys);

        if type_count >= 1 {
            substs.types.push(space, tuple_ty);
        }

        let output_ty: Option<Ty> =
            data.output.as_ref().map(|ty| fcx.to_ty(&**ty));

        let output_ty =
            output_ty.unwrap_or(ty::mk_nil(fcx.tcx()));

        if type_count >= 2 {
            substs.types.push(space, output_ty);
        }
    }

    fn adjust_type_parameters<'a, 'tcx>(
        fcx: &FnCtxt<'a, 'tcx>,
        span: Span,
        space: ParamSpace,
        defs: &VecPerParamSpace<ty::TypeParameterDef<'tcx>>,
        substs: &mut Substs<'tcx>)
    {
        let provided_len = substs.types.len(space);
        let desired = defs.get_slice(space);
        let required_len = desired.iter()
                              .take_while(|d| d.default.is_none())
                              .count();

        debug!("adjust_type_parameters(space={}, \
               provided_len={}, \
               desired_len={}, \
               required_len={})",
               space,
               provided_len,
               desired.len(),
               required_len);

        // Enforced by `push_explicit_parameters_from_segment_to_substs()`.
        assert!(provided_len <= desired.len());

        // Nothing specified at all: supply inference variables for
        // everything.
        if provided_len == 0 {
            substs.types.replace(space,
                                 fcx.infcx().next_ty_vars(desired.len()));
            return;
        }

        // Too few parameters specified: report an error and use Err
        // for everything.
        if provided_len < required_len {
            let qualifier =
                if desired.len() != required_len { "at least " } else { "" };
            span_err!(fcx.tcx().sess, span, E0089,
                "too few type parameters provided: expected {}{} parameter(s) \
                , found {} parameter(s)",
                qualifier, required_len, provided_len);
            substs.types.replace(space, repeat(fcx.tcx().types.err).take(desired.len()).collect());
            return;
        }

        // Otherwise, add in any optional parameters that the user
        // omitted. The case of *too many* parameters is handled
        // already by
        // push_explicit_parameters_from_segment_to_substs(). Note
        // that the *default* type are expressed in terms of all prior
        // parameters, so we have to substitute as we go with the
        // partial substitution that we have built up.
        for i in range(provided_len, desired.len()) {
            let default = desired[i].default.unwrap();
            let default = default.subst_spanned(fcx.tcx(), substs, Some(span));
            substs.types.push(space, default);
        }
        assert_eq!(substs.types.len(space), desired.len());

        debug!("Final substs: {}", substs.repr(fcx.tcx()));
    }

    fn adjust_region_parameters(
        fcx: &FnCtxt,
        span: Span,
        space: ParamSpace,
        defs: &VecPerParamSpace<ty::RegionParameterDef>,
        substs: &mut Substs)
    {
        let provided_len = substs.mut_regions().len(space);
        let desired = defs.get_slice(space);

        // Enforced by `push_explicit_parameters_from_segment_to_substs()`.
        assert!(provided_len <= desired.len());

        // If nothing was provided, just use inference variables.
        if provided_len == 0 {
            substs.mut_regions().replace(
                space,
                fcx.infcx().region_vars_for_defs(span, desired));
            return;
        }

        // If just the right number were provided, everybody is happy.
        if provided_len == desired.len() {
            return;
        }

        // Otherwise, too few were provided. Report an error and then
        // use inference variables.
        span_err!(fcx.tcx().sess, span, E0090,
            "too few lifetime parameters provided: expected {} parameter(s), \
             found {} parameter(s)",
            desired.len(), provided_len);

        substs.mut_regions().replace(
            space,
            fcx.infcx().region_vars_for_defs(span, desired));
    }
}

// Resolves `typ` by a single level if `typ` is a type variable.  If no
// resolution is possible, then an error is reported.
pub fn structurally_resolved_type<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>, sp: Span,
                                            mut ty: Ty<'tcx>) -> Ty<'tcx> {
    // If `ty` is a type variable, see whether we already know what it is.
    ty = fcx.infcx().shallow_resolve(ty);

    // If not, try resolve pending fcx obligations. Those can shed light.
    //
    // FIXME(#18391) -- This current strategy can lead to bad performance in
    // extreme cases.  We probably ought to smarter in general about
    // only resolving when we need help and only resolving obligations
    // will actually help.
    if ty::type_is_ty_var(ty) {
        vtable::select_fcx_obligations_where_possible(fcx);
        ty = fcx.infcx().shallow_resolve(ty);
    }

    // If not, error.
    if ty::type_is_ty_var(ty) {
        fcx.type_error_message(sp, |_actual| {
            "the type of this value must be known in this \
             context".to_string()
        }, ty, None);
        demand::suptype(fcx, sp, fcx.tcx().types.err, ty);
        ty = fcx.tcx().types.err;
    }

    ty
}

// Returns true if b contains a break that can exit from b
pub fn may_break(cx: &ty::ctxt, id: ast::NodeId, b: &ast::Block) -> bool {
    // First: is there an unlabeled break immediately
    // inside the loop?
    (loop_query(&*b, |e| {
        match *e {
            ast::ExprBreak(_) => true,
            _ => false
        }
    })) ||
   // Second: is there a labeled break with label
   // <id> nested anywhere inside the loop?
    (block_query(b, |e| {
        match e.node {
            ast::ExprBreak(Some(_)) => {
                match cx.def_map.borrow().get(&e.id) {
                    Some(&def::DefLabel(loop_id)) if id == loop_id => true,
                    _ => false,
                }
            }
            _ => false
        }}))
}

pub fn check_bounds_are_used<'a, 'tcx>(ccx: &CrateCtxt<'a, 'tcx>,
                                       span: Span,
                                       tps: &OwnedSlice<ast::TyParam>,
                                       ty: Ty<'tcx>) {
    debug!("check_bounds_are_used(n_tps={}, ty={})",
           tps.len(), ppaux::ty_to_string(ccx.tcx, ty));

    // make a vector of booleans initially false, set to true when used
    if tps.len() == 0u { return; }
    let mut tps_used: Vec<_> = repeat(false).take(tps.len()).collect();

    ty::walk_ty(ty, |t| {
            match t.sty {
                ty::ty_param(ParamTy {idx, ..}) => {
                    debug!("Found use of ty param num {}", idx);
                    tps_used[idx as uint] = true;
                }
                _ => ()
            }
        });

    for (i, b) in tps_used.iter().enumerate() {
        if !*b {
            span_err!(ccx.tcx.sess, span, E0091,
                "type parameter `{}` is unused",
                token::get_ident(tps[i].ident));
        }
    }
}

pub fn check_intrinsic_type(ccx: &CrateCtxt, it: &ast::ForeignItem) {
    fn param<'a, 'tcx>(ccx: &CrateCtxt<'a, 'tcx>, n: u32) -> Ty<'tcx> {
        let name = token::intern(format!("P{}", n).as_slice());
        ty::mk_param(ccx.tcx, subst::FnSpace, n, name)
    }

    let tcx = ccx.tcx;
    let name = token::get_ident(it.ident);
    let (n_tps, inputs, output) = if name.get().starts_with("atomic_") {
        let split : Vec<&str> = name.get().split('_').collect();
        assert!(split.len() >= 2, "Atomic intrinsic not correct format");

        //We only care about the operation here
        let (n_tps, inputs, output) = match split[1] {
            "cxchg" => (1, vec!(ty::mk_mut_ptr(tcx, param(ccx, 0)),
                                param(ccx, 0),
                                param(ccx, 0)),
                        param(ccx, 0)),
            "load" => (1, vec!(ty::mk_imm_ptr(tcx, param(ccx, 0))),
                       param(ccx, 0)),
            "store" => (1, vec!(ty::mk_mut_ptr(tcx, param(ccx, 0)), param(ccx, 0)),
                        ty::mk_nil(tcx)),

            "xchg" | "xadd" | "xsub" | "and"  | "nand" | "or" | "xor" | "max" |
            "min"  | "umax" | "umin" => {
                (1, vec!(ty::mk_mut_ptr(tcx, param(ccx, 0)), param(ccx, 0)),
                 param(ccx, 0))
            }
            "fence" => {
                (0, Vec::new(), ty::mk_nil(tcx))
            }
            op => {
                span_err!(tcx.sess, it.span, E0092,
                    "unrecognized atomic operation function: `{}`", op);
                return;
            }
        };
        (n_tps, inputs, ty::FnConverging(output))
    } else if name.get() == "abort" || name.get() == "unreachable" {
        (0, Vec::new(), ty::FnDiverging)
    } else {
        let (n_tps, inputs, output) = match name.get() {
            "breakpoint" => (0, Vec::new(), ty::mk_nil(tcx)),
            "size_of" |
            "pref_align_of" | "min_align_of" => (1u, Vec::new(), ccx.tcx.types.uint),
            "init" => (1u, Vec::new(), param(ccx, 0)),
            "uninit" => (1u, Vec::new(), param(ccx, 0)),
            "forget" => (1u, vec!( param(ccx, 0) ), ty::mk_nil(tcx)),
            "transmute" => (2, vec!( param(ccx, 0) ), param(ccx, 1)),
            "move_val_init" => {
                (1u,
                 vec!(
                    ty::mk_mut_rptr(tcx,
                                    tcx.mk_region(ty::ReLateBound(ty::DebruijnIndex::new(1),
                                                                  ty::BrAnon(0))),
                                    param(ccx, 0)),
                    param(ccx, 0)
                  ),
               ty::mk_nil(tcx))
            }
            "needs_drop" => (1u, Vec::new(), ccx.tcx.types.bool),
            "owns_managed" => (1u, Vec::new(), ccx.tcx.types.bool),

            "get_tydesc" => {
              let tydesc_ty = match ty::get_tydesc_ty(ccx.tcx) {
                  Ok(t) => t,
                  Err(s) => { tcx.sess.span_fatal(it.span, s[]); }
              };
              let td_ptr = ty::mk_ptr(ccx.tcx, ty::mt {
                  ty: tydesc_ty,
                  mutbl: ast::MutImmutable
              });
              (1u, Vec::new(), td_ptr)
            }
            "type_id" => {
                let langid = ccx.tcx.lang_items.require(TypeIdLangItem);
                match langid {
                    Ok(did) => (1u,
                                Vec::new(),
                                ty::mk_struct(ccx.tcx, did,
                                              ccx.tcx.mk_substs(subst::Substs::empty()))),
                    Err(msg) => {
                        tcx.sess.span_fatal(it.span, msg[]);
                    }
                }
            },
            "offset" => {
              (1,
               vec!(
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutImmutable
                  }),
                  ccx.tcx.types.int
               ),
               ty::mk_ptr(tcx, ty::mt {
                   ty: param(ccx, 0),
                   mutbl: ast::MutImmutable
               }))
            }
            "copy_memory" | "copy_nonoverlapping_memory" |
            "volatile_copy_memory" | "volatile_copy_nonoverlapping_memory" => {
              (1,
               vec!(
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutMutable
                  }),
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutImmutable
                  }),
                  tcx.types.uint,
               ),
               ty::mk_nil(tcx))
            }
            "set_memory" | "volatile_set_memory" => {
              (1,
               vec!(
                  ty::mk_ptr(tcx, ty::mt {
                      ty: param(ccx, 0),
                      mutbl: ast::MutMutable
                  }),
                  tcx.types.u8,
                  tcx.types.uint,
               ),
               ty::mk_nil(tcx))
            }
            "sqrtf32" => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "sqrtf64" => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "powif32" => {
               (0,
                vec!( tcx.types.f32, tcx.types.i32 ),
                tcx.types.f32)
            }
            "powif64" => {
               (0,
                vec!( tcx.types.f64, tcx.types.i32 ),
                tcx.types.f64)
            }
            "sinf32" => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "sinf64" => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "cosf32" => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "cosf64" => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "powf32" => {
               (0,
                vec!( tcx.types.f32, tcx.types.f32 ),
                tcx.types.f32)
            }
            "powf64" => {
               (0,
                vec!( tcx.types.f64, tcx.types.f64 ),
                tcx.types.f64)
            }
            "expf32"   => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "expf64"   => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "exp2f32"  => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "exp2f64"  => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "logf32"   => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "logf64"   => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "log10f32" => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "log10f64" => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "log2f32"  => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "log2f64"  => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "fmaf32" => {
                (0,
                 vec!( tcx.types.f32, tcx.types.f32, tcx.types.f32 ),
                 tcx.types.f32)
            }
            "fmaf64" => {
                (0,
                 vec!( tcx.types.f64, tcx.types.f64, tcx.types.f64 ),
                 tcx.types.f64)
            }
            "fabsf32"      => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "fabsf64"      => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "copysignf32"  => (0, vec!( tcx.types.f32, tcx.types.f32 ), tcx.types.f32),
            "copysignf64"  => (0, vec!( tcx.types.f64, tcx.types.f64 ), tcx.types.f64),
            "floorf32"     => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "floorf64"     => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "ceilf32"      => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "ceilf64"      => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "truncf32"     => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "truncf64"     => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "rintf32"      => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "rintf64"      => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "nearbyintf32" => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "nearbyintf64" => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "roundf32"     => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "roundf64"     => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "ctpop8"       => (0, vec!( tcx.types.u8  ), tcx.types.u8),
            "ctpop16"      => (0, vec!( tcx.types.u16 ), tcx.types.u16),
            "ctpop32"      => (0, vec!( tcx.types.u32 ), tcx.types.u32),
            "ctpop64"      => (0, vec!( tcx.types.u64 ), tcx.types.u64),
            "ctlz8"        => (0, vec!( tcx.types.u8  ), tcx.types.u8),
            "ctlz16"       => (0, vec!( tcx.types.u16 ), tcx.types.u16),
            "ctlz32"       => (0, vec!( tcx.types.u32 ), tcx.types.u32),
            "ctlz64"       => (0, vec!( tcx.types.u64 ), tcx.types.u64),
            "cttz8"        => (0, vec!( tcx.types.u8  ), tcx.types.u8),
            "cttz16"       => (0, vec!( tcx.types.u16 ), tcx.types.u16),
            "cttz32"       => (0, vec!( tcx.types.u32 ), tcx.types.u32),
            "cttz64"       => (0, vec!( tcx.types.u64 ), tcx.types.u64),
            "bswap16"      => (0, vec!( tcx.types.u16 ), tcx.types.u16),
            "bswap32"      => (0, vec!( tcx.types.u32 ), tcx.types.u32),
            "bswap64"      => (0, vec!( tcx.types.u64 ), tcx.types.u64),

            "volatile_load" =>
                (1, vec!( ty::mk_imm_ptr(tcx, param(ccx, 0)) ), param(ccx, 0)),
            "volatile_store" =>
                (1, vec!( ty::mk_mut_ptr(tcx, param(ccx, 0)), param(ccx, 0) ), ty::mk_nil(tcx)),

            "i8_add_with_overflow" | "i8_sub_with_overflow" | "i8_mul_with_overflow" =>
                (0, vec!(tcx.types.i8, tcx.types.i8),
                ty::mk_tup(tcx, vec!(tcx.types.i8, tcx.types.bool))),

            "i16_add_with_overflow" | "i16_sub_with_overflow" | "i16_mul_with_overflow" =>
                (0, vec!(tcx.types.i16, tcx.types.i16),
                ty::mk_tup(tcx, vec!(tcx.types.i16, tcx.types.bool))),

            "i32_add_with_overflow" | "i32_sub_with_overflow" | "i32_mul_with_overflow" =>
                (0, vec!(tcx.types.i32, tcx.types.i32),
                ty::mk_tup(tcx, vec!(tcx.types.i32, tcx.types.bool))),

            "i64_add_with_overflow" | "i64_sub_with_overflow" | "i64_mul_with_overflow" =>
                (0, vec!(tcx.types.i64, tcx.types.i64),
                ty::mk_tup(tcx, vec!(tcx.types.i64, tcx.types.bool))),

            "u8_add_with_overflow" | "u8_sub_with_overflow" | "u8_mul_with_overflow" =>
                (0, vec!(tcx.types.u8, tcx.types.u8),
                ty::mk_tup(tcx, vec!(tcx.types.u8, tcx.types.bool))),

            "u16_add_with_overflow" | "u16_sub_with_overflow" | "u16_mul_with_overflow" =>
                (0, vec!(tcx.types.u16, tcx.types.u16),
                ty::mk_tup(tcx, vec!(tcx.types.u16, tcx.types.bool))),

            "u32_add_with_overflow" | "u32_sub_with_overflow" | "u32_mul_with_overflow"=>
                (0, vec!(tcx.types.u32, tcx.types.u32),
                ty::mk_tup(tcx, vec!(tcx.types.u32, tcx.types.bool))),

            "u64_add_with_overflow" | "u64_sub_with_overflow"  | "u64_mul_with_overflow" =>
                (0, vec!(tcx.types.u64, tcx.types.u64),
                ty::mk_tup(tcx, vec!(tcx.types.u64, tcx.types.bool))),

            "return_address" => (0, vec![], ty::mk_imm_ptr(tcx, tcx.types.u8)),

            "assume" => (0, vec![tcx.types.bool], ty::mk_nil(tcx)),

            ref other => {
                span_err!(tcx.sess, it.span, E0093,
                    "unrecognized intrinsic function: `{}`", *other);
                return;
            }
        };
        (n_tps, inputs, ty::FnConverging(output))
    };
    let fty = ty::mk_bare_fn(tcx, None, tcx.mk_bare_fn(ty::BareFnTy {
        unsafety: ast::Unsafety::Unsafe,
        abi: abi::RustIntrinsic,
        sig: ty::Binder(FnSig {
            inputs: inputs,
            output: output,
            variadic: false,
        }),
    }));
    let i_ty = ty::lookup_item_type(ccx.tcx, local_def(it.id));
    let i_n_tps = i_ty.generics.types.len(subst::FnSpace);
    if i_n_tps != n_tps {
        span_err!(tcx.sess, it.span, E0094,
            "intrinsic has wrong number of type \
             parameters: found {}, expected {}",
             i_n_tps, n_tps);
    } else {
        require_same_types(tcx,
                           None,
                           false,
                           it.span,
                           i_ty.ty,
                           fty,
                           || {
                format!("intrinsic has wrong type: expected `{}`",
                        ppaux::ty_to_string(ccx.tcx, fty))
            });
    }
}
