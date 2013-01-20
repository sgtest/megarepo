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

typeck.rs, an introduction

The type checker is responsible for:

1. Determining the type of each expression
2. Resolving methods and traits
3. Guaranteeing that most type rules are met ("most?", you say, "why most?"
   Well, dear reader, read on)

The main entry point is `check_crate()`.  Type checking operates in two major
phases: collect and check.  The collect phase passes over all items and
determines their type, without examining their "innards".  The check phase
then checks function bodies and so forth.

Within the check phase, we check each function body one at a time (bodies of
function expressions are checked as part of the containing function).
Inference is used to supply types wherever they are unknown. The actual
checking of a function itself has several phases (check, regionck, writeback),
as discussed in the documentation for the `check` module.

The type checker is defined into various submodules which are documented
independently:

- astconv: converts the AST representation of types
  into the `ty` representation

- collect: computes the types of each top-level item and enters them into
  the `cx.tcache` table for later use

- check: walks over function bodies and type checks them, inferring types for
  local variables, type parameters, etc as necessary.

- infer: finds the types to use for each type variable such that
  all subtyping and assignment constraints are met.  In essence, the check
  module specifies the constraints, and the infer module solves them.

*/

#[legacy_exports];

use core::prelude::*;

use metadata::csearch;
use middle::pat_util::{pat_id_map, PatIdMap};
use middle::resolve;
use middle::ty::{arg, field, node_type_table, mk_nil, ty_param_bounds_and_ty};
use middle::ty::{ty_param_substs_and_ty, vstore_uniq};
use middle::ty;
use util::common::{block_query, indent, indenter, loop_query};
use util::ppaux::{bound_region_to_str, vstore_to_str, expr_repr};
use util::ppaux::{ty_to_str, tys_to_str, region_to_str};
use util::ppaux;

use core::dvec::DVec;
use core::result::Result;
use core::result;
use core::vec;
use std::list::{List, Nil, Cons};
use std::list;
use std::map::HashMap;
use std::map;
use std::smallintmap;
use syntax::ast::{provided, required, spanned};
use syntax::ast_map::node_id_to_str;
use syntax::ast_util::{has_legacy_export_attr};
use syntax::ast_util::{local_def, respan, split_trait_methods};
use syntax::codemap::span;
use syntax::print::pprust::*;
use syntax::visit;
use syntax::{ast, ast_util, ast_map};

export check;
export check_crate;
export infer;
export method_map;
export method_origin;
export method_map_entry;
export vtable_map;
export vtable_res;
export vtable_origin;
export method_static, method_param, method_trait, method_self;
export vtable_static, vtable_param, vtable_trait;
export provided_methods_map;
export coherence;
export check;
export rscope;
export astconv;
export infer;
export collect;
export coherence;
export deriving;
export crate_ctxt;
export write_ty_to_tcx, write_substs_to_tcx;
export no_params;
export isr_alist;
export require_same_types;
export lookup_def_ccx, lookup_def_tcx;

#[legacy_exports]
#[path = "check/mod.rs"]
pub mod check;
#[legacy_exports]
pub mod rscope;
#[legacy_exports]
pub mod astconv;
#[path = "infer/mod.rs"]
pub mod infer;
#[legacy_exports]
pub mod collect;
#[legacy_exports]
pub mod coherence;

#[auto_encode]
#[auto_decode]
pub enum method_origin {
    // fully statically resolved method
    method_static(ast::def_id),

    // method invoked on a type parameter with a bounded trait
    method_param(method_param),

    // method invoked on a trait instance
    method_trait(ast::def_id, uint, ty::vstore),

    // method invoked on "self" inside a default method
    method_self(ast::def_id, uint),
}

// details for a method invoked with a receiver whose type is a type parameter
// with a bounded trait.
#[auto_encode]
#[auto_decode]
struct method_param {
    // the trait containing the method to be invoked
    trait_id: ast::def_id,

    // index of the method to be invoked amongst the trait's methods
    method_num: uint,

    // index of the type parameter (from those that are in scope) that is
    // the type of the receiver
    param_num: uint,

    // index of the bound for this type parameter which specifies the trait
    bound_num: uint,
}

pub struct method_map_entry {
    // the type and mode of the self parameter, which is not reflected
    // in the fn type (FIXME #3446)
    self_arg: ty::arg,

    // the type of explicit self on the method
    explicit_self: ast::self_ty_,

    // method details being invoked
    origin: method_origin,
}

// maps from an expression id that corresponds to a method call to the details
// of the method to be invoked
pub type method_map = HashMap<ast::node_id, method_map_entry>;

// Resolutions for bounds of all parameters, left to right, for a given path.
pub type vtable_res = @~[vtable_origin];

pub enum vtable_origin {
    /*
      Statically known vtable. def_id gives the class or impl item
      from whence comes the vtable, and tys are the type substs.
      vtable_res is the vtable itself
     */
    vtable_static(ast::def_id, ~[ty::t], vtable_res),
    /*
      Dynamic vtable, comes from a parameter that has a bound on it:
      fn foo<T: quux, baz, bar>(a: T) -- a's vtable would have a
      vtable_param origin

      The first uint is the param number (identifying T in the example),
      and the second is the bound number (identifying baz)
     */
    vtable_param(uint, uint),
    /*
      Dynamic vtable, comes from something known to have a trait
      type. def_id refers to the trait item, tys are the substs
     */
    vtable_trait(ast::def_id, ~[ty::t]),
}

impl vtable_origin {
    fn to_str(tcx: ty::ctxt) -> ~str {
        match self {
            vtable_static(def_id, ref tys, ref vtable_res) => {
                fmt!("vtable_static(%?:%s, %?, %?)",
                     def_id, ty::item_path_str(tcx, def_id),
                     tys,
                     vtable_res.map(|o| o.to_str(tcx)))
            }

            vtable_param(x, y) => {
                fmt!("vtable_param(%?, %?)", x, y)
            }

            vtable_trait(def_id, ref tys) => {
                fmt!("vtable_trait(%?:%s, %?)",
                     def_id, ty::item_path_str(tcx, def_id),
                     tys.map(|t| ppaux::ty_to_str(tcx, *t)))
            }
        }
    }
}

type vtable_map = HashMap<ast::node_id, vtable_res>;

struct crate_ctxt__ {
    // A mapping from method call sites to traits that have that method.
    trait_map: resolve::TraitMap,
    method_map: method_map,
    vtable_map: vtable_map,
    coherence_info: @coherence::CoherenceInfo,
    tcx: ty::ctxt
}

pub enum crate_ctxt {
    crate_ctxt_(crate_ctxt__)
}

// Functions that write types into the node type table
fn write_ty_to_tcx(tcx: ty::ctxt, node_id: ast::node_id, ty: ty::t) {
    debug!("write_ty_to_tcx(%d, %s)", node_id, ppaux::ty_to_str(tcx, ty));
    smallintmap::insert(*tcx.node_types, node_id as uint, ty);
}
fn write_substs_to_tcx(tcx: ty::ctxt,
                       node_id: ast::node_id,
                       +substs: ~[ty::t]) {
    if substs.len() > 0u {
        debug!("write_substs_to_tcx(%d, %?)", node_id,
               substs.map(|t| ppaux::ty_to_str(tcx, *t)));
        tcx.node_type_substs.insert(node_id, substs);
    }
}

fn lookup_def_tcx(tcx: ty::ctxt, sp: span, id: ast::node_id) -> ast::def {
    match tcx.def_map.find(id) {
      Some(x) => x,
      _ => {
        tcx.sess.span_fatal(sp, ~"internal error looking up a definition")
      }
    }
}

fn lookup_def_ccx(ccx: @crate_ctxt, sp: span, id: ast::node_id) -> ast::def {
    lookup_def_tcx(ccx.tcx, sp, id)
}

fn no_params(t: ty::t) -> ty::ty_param_bounds_and_ty {
    {bounds: @~[], region_param: None, ty: t}
}

fn require_same_types(
    tcx: ty::ctxt,
    maybe_infcx: Option<@infer::InferCtxt>,
    t1_is_expected: bool,
    span: span,
    t1: ty::t,
    t2: ty::t,
    msg: fn() -> ~str) -> bool {

    let l_tcx, l_infcx;
    match maybe_infcx {
      None => {
        l_tcx = tcx;
        l_infcx = infer::new_infer_ctxt(tcx);
      }
      Some(i) => {
        l_tcx = i.tcx;
        l_infcx = i;
      }
    }

    match infer::mk_eqty(l_infcx, t1_is_expected, span, t1, t2) {
        result::Ok(()) => true,
        result::Err(ref terr) => {
            l_tcx.sess.span_err(span, msg() + ~": " +
                                ty::type_err_to_str(l_tcx, terr));
            ty::note_and_explain_type_err(l_tcx, terr);
            false
        }
    }
}

// a list of mapping from in-scope-region-names ("isr") to the
// corresponding ty::Region
type isr_alist = @List<(ty::bound_region, ty::Region)>;

trait get_and_find_region {
    fn get(br: ty::bound_region) -> ty::Region;
    fn find(br: ty::bound_region) -> Option<ty::Region>;
}

impl isr_alist: get_and_find_region {
    fn get(br: ty::bound_region) -> ty::Region {
        self.find(br).get()
    }

    fn find(br: ty::bound_region) -> Option<ty::Region> {
        for list::each(self) |isr| {
            let (isr_br, isr_r) = *isr;
            if isr_br == br { return Some(isr_r); }
        }
        return None;
    }
}

fn arg_is_argv_ty(tcx: ty::ctxt, a: ty::arg) -> bool {
    match ty::resolved_mode(tcx, a.mode) {
        ast::by_val => { /*ok*/ }
        _ => {
            return false;
        }
    }

    match ty::get(a.ty).sty {
      ty::ty_evec(mt, vstore_uniq) => {
        if mt.mutbl != ast::m_imm { return false; }
        match ty::get(mt.ty).sty {
          ty::ty_estr(vstore_uniq) => return true,
          _ => return false
        }
      }
      _ => return false
    }
}

fn check_main_fn_ty(ccx: @crate_ctxt,
                    main_id: ast::node_id,
                    main_span: span) {

    let tcx = ccx.tcx;
    let main_t = ty::node_id_to_type(tcx, main_id);
    match ty::get(main_t).sty {
        ty::ty_fn(ref fn_ty) => {
            match tcx.items.find(main_id) {
                Some(ast_map::node_item(it,_)) => {
                    match it.node {
                        ast::item_fn(_, _, ref ps, _)
                        if vec::is_not_empty(*ps) => {
                            tcx.sess.span_err(
                                main_span,
                                ~"main function is not allowed \
                                  to have type parameters");
                            return;
                        }
                        _ => ()
                    }
                }
                _ => ()
            }
            let mut ok = ty::type_is_nil((*fn_ty).sig.output);
            let num_args = vec::len((*fn_ty).sig.inputs);
            ok &= num_args == 0u;
            if !ok {
                tcx.sess.span_err(
                    main_span,
                    fmt!("Wrong type in main function: found `%s`, \
                          expected `fn() -> ()`",
                         ppaux::ty_to_str(tcx, main_t)));
            }
        }
        _ => {
            tcx.sess.span_bug(main_span,
                              ~"main has a non-function type: found `" +
                              ppaux::ty_to_str(tcx, main_t) + ~"`");
        }
    }
}

fn check_for_main_fn(ccx: @crate_ctxt) {
    let tcx = ccx.tcx;
    if !tcx.sess.building_library {
        match copy tcx.sess.main_fn {
          Some((id, sp)) => check_main_fn_ty(ccx, id, sp),
          None => tcx.sess.err(~"main function not found")
        }
    }
}

fn check_crate(tcx: ty::ctxt,
               trait_map: resolve::TraitMap,
               crate: @ast::crate)
    -> (method_map, vtable_map) {

    let ccx = @crate_ctxt_(crate_ctxt__ {
        trait_map: trait_map,
        method_map: map::HashMap(),
        vtable_map: map::HashMap(),
        coherence_info: @coherence::CoherenceInfo(),
        tcx: tcx
    });
    collect::collect_item_types(ccx, crate);
    coherence::check_coherence(ccx, crate);

    check::check_item_types(ccx, crate);
    check_for_main_fn(ccx);
    tcx.sess.abort_if_errors();
    (ccx.method_map, ccx.vtable_map)
}
//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
