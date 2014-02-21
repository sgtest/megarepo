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

typeck.rs, an introduction

The type checker is responsible for:

1. Determining the type of each expression
2. Resolving methods and traits
3. Guaranteeing that most type rules are met ("most?", you say, "why most?"
   Well, dear reader, read on)

The main entry point is `check_crate()`.  Type checking operates in
several major phases:

1. The collect phase first passes over all items and determines their
   type, without examining their "innards".

2. Variance inference then runs to compute the variance of each parameter

3. Coherence checks for overlapping or orphaned impls

4. Finally, the check phase then checks function bodies and so forth.
   Within the check phase, we check each function body one at a time
   (bodies of function expressions are checked as part of the
   containing function).  Inference is used to supply types wherever
   they are unknown. The actual checking of a function itself has
   several phases (check, regionck, writeback), as discussed in the
   documentation for the `check` module.

The type checker is defined into various submodules which are documented
independently:

- astconv: converts the AST representation of types
  into the `ty` representation

- collect: computes the types of each top-level item and enters them into
  the `cx.tcache` table for later use

- coherence: enforces coherence rules, builds some tables

- variance: variance inference

- check: walks over function bodies and type checks them, inferring types for
  local variables, type parameters, etc as necessary.

- infer: finds the types to use for each type variable such that
  all subtyping and assignment constraints are met.  In essence, the check
  module specifies the constraints, and the infer module solves them.

*/

#[allow(non_camel_case_types)];

use driver::session;

use middle::resolve;
use middle::ty;
use util::common::time;
use util::ppaux::Repr;
use util::ppaux;

use std::cell::RefCell;
use std::hashmap::HashMap;
use std::rc::Rc;
use collections::List;
use collections::list;
use syntax::codemap::Span;
use syntax::print::pprust::*;
use syntax::{ast, ast_map, abi};

pub mod check;
pub mod rscope;
pub mod astconv;
pub mod infer;
pub mod collect;
pub mod coherence;
pub mod variance;

#[deriving(Clone, Encodable, Decodable, Eq, Ord)]
pub enum param_index {
    param_numbered(uint),
    param_self
}

#[deriving(Clone, Encodable, Decodable)]
pub enum method_origin {
    // fully statically resolved method
    method_static(ast::DefId),

    // method invoked on a type parameter with a bounded trait
    method_param(method_param),

    // method invoked on a trait instance
    method_object(method_object),

}

// details for a method invoked with a receiver whose type is a type parameter
// with a bounded trait.
#[deriving(Clone, Encodable, Decodable)]
pub struct method_param {
    // the trait containing the method to be invoked
    trait_id: ast::DefId,

    // index of the method to be invoked amongst the trait's methods
    method_num: uint,

    // index of the type parameter (from those that are in scope) that is
    // the type of the receiver
    param_num: param_index,

    // index of the bound for this type parameter which specifies the trait
    bound_num: uint,
}

// details for a method invoked with a receiver whose type is an object
#[deriving(Clone, Encodable, Decodable)]
pub struct method_object {
    // the (super)trait containing the method to be invoked
    trait_id: ast::DefId,

    // the actual base trait id of the object
    object_trait_id: ast::DefId,

    // index of the method to be invoked amongst the trait's methods
    method_num: uint,

    // index into the actual runtime vtable.
    // the vtable is formed by concatenating together the method lists of
    // the base object trait and all supertraits;  this is the index into
    // that vtable
    real_index: uint,
}

// maps from an expression id that corresponds to a method call to the details
// of the method to be invoked
pub type method_map = @RefCell<HashMap<ast::NodeId, method_origin>>;

pub type vtable_param_res = @~[vtable_origin];
// Resolutions for bounds of all parameters, left to right, for a given path.
pub type vtable_res = @~[vtable_param_res];

#[deriving(Clone)]
pub enum vtable_origin {
    /*
      Statically known vtable. def_id gives the class or impl item
      from whence comes the vtable, and tys are the type substs.
      vtable_res is the vtable itself
     */
    vtable_static(ast::DefId, ~[ty::t], vtable_res),

    /*
      Dynamic vtable, comes from a parameter that has a bound on it:
      fn foo<T:quux,baz,bar>(a: T) -- a's vtable would have a
      vtable_param origin

      The first argument is the param index (identifying T in the example),
      and the second is the bound number (identifying baz)
     */
    vtable_param(param_index, uint),
}

impl Repr for vtable_origin {
    fn repr(&self, tcx: ty::ctxt) -> ~str {
        match *self {
            vtable_static(def_id, ref tys, ref vtable_res) => {
                format!("vtable_static({:?}:{}, {}, {})",
                     def_id,
                     ty::item_path_str(tcx, def_id),
                     tys.repr(tcx),
                     vtable_res.repr(tcx))
            }

            vtable_param(x, y) => {
                format!("vtable_param({:?}, {:?})", x, y)
            }
        }
    }
}

pub type vtable_map = @RefCell<HashMap<ast::NodeId, vtable_res>>;


// Information about the vtable resolutions for for a trait impl.
// Mostly the information is important for implementing default
// methods.
#[deriving(Clone)]
pub struct impl_res {
    // resolutions for any bounded params on the trait definition
    trait_vtables: vtable_res,
    // resolutions for the trait /itself/ (and for supertraits)
    self_vtables: vtable_param_res
}

impl Repr for impl_res {
    fn repr(&self, tcx: ty::ctxt) -> ~str {
        format!("impl_res \\{trait_vtables={}, self_vtables={}\\}",
             self.trait_vtables.repr(tcx),
             self.self_vtables.repr(tcx))
    }
}

pub type impl_vtable_map = RefCell<HashMap<ast::DefId, impl_res>>;

pub struct CrateCtxt {
    // A mapping from method call sites to traits that have that method.
    trait_map: resolve::TraitMap,
    method_map: method_map,
    vtable_map: vtable_map,
    tcx: ty::ctxt
}

// Functions that write types into the node type table
pub fn write_ty_to_tcx(tcx: ty::ctxt, node_id: ast::NodeId, ty: ty::t) {
    debug!("write_ty_to_tcx({}, {})", node_id, ppaux::ty_to_str(tcx, ty));
    assert!(!ty::type_needs_infer(ty));
    let mut node_types = tcx.node_types.borrow_mut();
    node_types.get().insert(node_id as uint, ty);
}
pub fn write_substs_to_tcx(tcx: ty::ctxt,
                           node_id: ast::NodeId,
                           substs: ~[ty::t]) {
    if substs.len() > 0u {
        debug!("write_substs_to_tcx({}, {:?})", node_id,
               substs.map(|t| ppaux::ty_to_str(tcx, *t)));
        assert!(substs.iter().all(|t| !ty::type_needs_infer(*t)));

        let mut node_type_substs = tcx.node_type_substs.borrow_mut();
        node_type_substs.get().insert(node_id, substs);
    }
}
pub fn write_tpt_to_tcx(tcx: ty::ctxt,
                        node_id: ast::NodeId,
                        tpt: &ty::ty_param_substs_and_ty) {
    write_ty_to_tcx(tcx, node_id, tpt.ty);
    if !tpt.substs.tps.is_empty() {
        write_substs_to_tcx(tcx, node_id, tpt.substs.tps.clone());
    }
}

pub fn lookup_def_tcx(tcx: ty::ctxt, sp: Span, id: ast::NodeId) -> ast::Def {
    let def_map = tcx.def_map.borrow();
    match def_map.get().find(&id) {
        Some(&x) => x,
        _ => {
            tcx.sess.span_fatal(sp, "internal error looking up a definition")
        }
    }
}

pub fn lookup_def_ccx(ccx: &CrateCtxt, sp: Span, id: ast::NodeId)
                   -> ast::Def {
    lookup_def_tcx(ccx.tcx, sp, id)
}

pub fn no_params(t: ty::t) -> ty::ty_param_bounds_and_ty {
    ty::ty_param_bounds_and_ty {
        generics: ty::Generics {type_param_defs: Rc::new(~[]),
                                region_param_defs: Rc::new(~[])},
        ty: t
    }
}

pub fn require_same_types(tcx: ty::ctxt,
                          maybe_infcx: Option<&infer::InferCtxt>,
                          t1_is_expected: bool,
                          span: Span,
                          t1: ty::t,
                          t2: ty::t,
                          msg: || -> ~str)
                          -> bool {
    let result = match maybe_infcx {
        None => {
            let infcx = infer::new_infer_ctxt(tcx);
            infer::mk_eqty(&infcx, t1_is_expected, infer::Misc(span), t1, t2)
        }
        Some(infcx) => {
            infer::mk_eqty(infcx, t1_is_expected, infer::Misc(span), t1, t2)
        }
    };

    match result {
        Ok(_) => true,
        Err(ref terr) => {
            tcx.sess.span_err(span, msg() + ": " +
                              ty::type_err_to_str(tcx, terr));
            ty::note_and_explain_type_err(tcx, terr);
            false
        }
    }
}

// a list of mapping from in-scope-region-names ("isr") to the
// corresponding ty::Region
pub type isr_alist = @List<(ty::BoundRegion, ty::Region)>;

trait get_and_find_region {
    fn get(&self, br: ty::BoundRegion) -> ty::Region;
    fn find(&self, br: ty::BoundRegion) -> Option<ty::Region>;
}

impl get_and_find_region for isr_alist {
    fn get(&self, br: ty::BoundRegion) -> ty::Region {
        self.find(br).unwrap()
    }

    fn find(&self, br: ty::BoundRegion) -> Option<ty::Region> {
        let mut ret = None;
        list::each(*self, |isr| {
            let (isr_br, isr_r) = *isr;
            if isr_br == br { ret = Some(isr_r); false } else { true }
        });
        ret
    }
}

fn check_main_fn_ty(ccx: &CrateCtxt,
                    main_id: ast::NodeId,
                    main_span: Span) {
    let tcx = ccx.tcx;
    let main_t = ty::node_id_to_type(tcx, main_id);
    match ty::get(main_t).sty {
        ty::ty_bare_fn(..) => {
            match tcx.map.find(main_id) {
                Some(ast_map::NodeItem(it)) => {
                    match it.node {
                        ast::ItemFn(_, _, _, ref ps, _)
                        if ps.is_parameterized() => {
                            tcx.sess.span_err(
                                main_span,
                                "main function is not allowed to have type parameters");
                            return;
                        }
                        _ => ()
                    }
                }
                _ => ()
            }
            let se_ty = ty::mk_bare_fn(tcx, ty::BareFnTy {
                purity: ast::ImpureFn,
                abis: abi::AbiSet::Rust(),
                sig: ty::FnSig {
                    binder_id: main_id,
                    inputs: ~[],
                    output: ty::mk_nil(),
                    variadic: false
                }
            });

            require_same_types(tcx, None, false, main_span, main_t, se_ty,
                || format!("main function expects type: `{}`",
                        ppaux::ty_to_str(ccx.tcx, se_ty)));
        }
        _ => {
            tcx.sess.span_bug(main_span,
                              format!("main has a non-function type: found `{}`",
                                   ppaux::ty_to_str(tcx, main_t)));
        }
    }
}

fn check_start_fn_ty(ccx: &CrateCtxt,
                     start_id: ast::NodeId,
                     start_span: Span) {
    let tcx = ccx.tcx;
    let start_t = ty::node_id_to_type(tcx, start_id);
    match ty::get(start_t).sty {
        ty::ty_bare_fn(_) => {
            match tcx.map.find(start_id) {
                Some(ast_map::NodeItem(it)) => {
                    match it.node {
                        ast::ItemFn(_,_,_,ref ps,_)
                        if ps.is_parameterized() => {
                            tcx.sess.span_err(
                                start_span,
                                "start function is not allowed to have type parameters");
                            return;
                        }
                        _ => ()
                    }
                }
                _ => ()
            }

            let se_ty = ty::mk_bare_fn(tcx, ty::BareFnTy {
                purity: ast::ImpureFn,
                abis: abi::AbiSet::Rust(),
                sig: ty::FnSig {
                    binder_id: start_id,
                    inputs: ~[
                        ty::mk_int(),
                        ty::mk_imm_ptr(tcx, ty::mk_imm_ptr(tcx, ty::mk_u8()))
                    ],
                    output: ty::mk_int(),
                    variadic: false
                }
            });

            require_same_types(tcx, None, false, start_span, start_t, se_ty,
                || format!("start function expects type: `{}`", ppaux::ty_to_str(ccx.tcx, se_ty)));

        }
        _ => {
            tcx.sess.span_bug(start_span,
                              format!("start has a non-function type: found `{}`",
                                   ppaux::ty_to_str(tcx, start_t)));
        }
    }
}

fn check_for_entry_fn(ccx: &CrateCtxt) {
    let tcx = ccx.tcx;
    if !tcx.sess.building_library.get() {
        match tcx.sess.entry_fn.get() {
          Some((id, sp)) => match tcx.sess.entry_type.get() {
              Some(session::EntryMain) => check_main_fn_ty(ccx, id, sp),
              Some(session::EntryStart) => check_start_fn_ty(ccx, id, sp),
              Some(session::EntryNone) => {}
              None => tcx.sess.bug("entry function without a type")
          },
          None => {}
        }
    }
}

pub fn check_crate(tcx: ty::ctxt,
                   trait_map: resolve::TraitMap,
                   krate: &ast::Crate)
                -> (method_map, vtable_map) {
    let time_passes = tcx.sess.time_passes();
    let ccx = @CrateCtxt {
        trait_map: trait_map,
        method_map: @RefCell::new(HashMap::new()),
        vtable_map: @RefCell::new(HashMap::new()),
        tcx: tcx
    };

    time(time_passes, "type collecting", (), |_|
        collect::collect_item_types(ccx, krate));

    // this ensures that later parts of type checking can assume that items
    // have valid types and not error
    tcx.sess.abort_if_errors();

    time(time_passes, "variance inference", (), |_|
         variance::infer_variance(tcx, krate));

    time(time_passes, "coherence checking", (), |_|
        coherence::check_coherence(ccx, krate));

    time(time_passes, "type checking", (), |_|
        check::check_item_types(ccx, krate));

    check_for_entry_fn(ccx);
    tcx.sess.abort_if_errors();
    (ccx.method_map, ccx.vtable_map)
}
