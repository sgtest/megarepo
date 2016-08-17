// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

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

# Note

This API is completely unstable and subject to change.

*/

#![crate_name = "rustc_typeck"]
#![unstable(feature = "rustc_private", issue = "27812")]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
      html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
      html_root_url = "https://doc.rust-lang.org/nightly/")]
#![cfg_attr(not(stage0), deny(warnings))]

#![allow(non_camel_case_types)]

#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(quote)]
#![feature(rustc_diagnostic_macros)]
#![feature(rustc_private)]
#![feature(staged_api)]
#![feature(question_mark)]

#[macro_use] extern crate log;
#[macro_use] extern crate syntax;
extern crate syntax_pos;

extern crate arena;
extern crate fmt_macros;
#[macro_use] extern crate rustc;
extern crate rustc_platform_intrinsics as intrinsics;
extern crate rustc_back;
extern crate rustc_const_math;
extern crate rustc_const_eval;
extern crate rustc_errors as errors;

pub use rustc::dep_graph;
pub use rustc::hir;
pub use rustc::lint;
pub use rustc::middle;
pub use rustc::session;
pub use rustc::util;

use dep_graph::DepNode;
use hir::map as hir_map;
use rustc::infer::TypeOrigin;
use rustc::ty::subst::Substs;
use rustc::ty::{self, Ty, TyCtxt, TypeFoldable};
use rustc::traits::{self, Reveal};
use session::{config, CompileResult};
use util::common::time;

use syntax::ast;
use syntax::abi::Abi;
use syntax_pos::Span;

use std::cell::RefCell;
use util::nodemap::NodeMap;

// NB: This module needs to be declared first so diagnostics are
// registered before they are used.
pub mod diagnostics;

pub mod check;
pub mod check_unused;
mod rscope;
mod astconv;
pub mod collect;
mod constrained_type_params;
pub mod coherence;
pub mod variance;

pub struct TypeAndSubsts<'tcx> {
    pub substs: &'tcx Substs<'tcx>,
    pub ty: Ty<'tcx>,
}

pub struct CrateCtxt<'a, 'tcx: 'a> {
    ast_ty_to_ty_cache: RefCell<NodeMap<Ty<'tcx>>>,

    /// A vector of every trait accessible in the whole crate
    /// (i.e. including those from subcrates). This is used only for
    /// error reporting, and so is lazily initialised and generally
    /// shouldn't taint the common path (hence the RefCell).
    pub all_traits: RefCell<Option<check::method::AllTraitsVec>>,

    /// This stack is used to identify cycles in the user's source.
    /// Note that these cycles can cross multiple items.
    pub stack: RefCell<Vec<collect::AstConvRequest>>,

    pub tcx: TyCtxt<'a, 'tcx, 'tcx>,

    /// Obligations which will have to be checked at the end of
    /// type-checking, after all functions have been inferred.
    /// The key is the NodeId of the item the obligations were from.
    pub deferred_obligations: RefCell<NodeMap<Vec<traits::DeferredObligation<'tcx>>>>,
}

// Functions that write types into the node type table
fn write_ty_to_tcx<'a, 'tcx>(ccx: &CrateCtxt<'a, 'tcx>, node_id: ast::NodeId, ty: Ty<'tcx>) {
    debug!("write_ty_to_tcx({}, {:?})", node_id,  ty);
    assert!(!ty.needs_infer());
    ccx.tcx.node_type_insert(node_id, ty);
}

fn write_substs_to_tcx<'a, 'tcx>(ccx: &CrateCtxt<'a, 'tcx>,
                                 node_id: ast::NodeId,
                                 item_substs: ty::ItemSubsts<'tcx>) {
    if !item_substs.is_noop() {
        debug!("write_substs_to_tcx({}, {:?})",
               node_id,
               item_substs);

        assert!(!item_substs.substs.types.needs_infer());

        ccx.tcx.tables.borrow_mut().item_substs.insert(node_id, item_substs);
    }
}

fn require_c_abi_if_variadic(tcx: TyCtxt,
                             decl: &hir::FnDecl,
                             abi: Abi,
                             span: Span) {
    if decl.variadic && abi != Abi::C {
        let mut err = struct_span_err!(tcx.sess, span, E0045,
                  "variadic function must have C calling convention");
        err.span_label(span, &("variadics require C calling conventions").to_string())
            .emit();
    }
}

fn require_same_types<'a, 'tcx>(ccx: &CrateCtxt<'a, 'tcx>,
                                origin: TypeOrigin,
                                t1: Ty<'tcx>,
                                t2: Ty<'tcx>)
                                -> bool {
    ccx.tcx.infer_ctxt(None, None, Reveal::NotSpecializable).enter(|infcx| {
        if let Err(err) = infcx.eq_types(false, origin.clone(), t1, t2) {
            infcx.report_mismatched_types(origin, t1, t2, err);
            false
        } else {
            true
        }
    })
}

fn check_main_fn_ty(ccx: &CrateCtxt,
                    main_id: ast::NodeId,
                    main_span: Span) {
    let tcx = ccx.tcx;
    let main_t = tcx.node_id_to_type(main_id);
    match main_t.sty {
        ty::TyFnDef(..) => {
            match tcx.map.find(main_id) {
                Some(hir_map::NodeItem(it)) => {
                    match it.node {
                        hir::ItemFn(_, _, _, _, ref generics, _) => {
                            if let Some(gen_span) = generics.span() {
                                struct_span_err!(ccx.tcx.sess, gen_span, E0131,
                                         "main function is not allowed to have type parameters")
                                    .span_label(gen_span,
                                                &format!("main cannot have type parameters"))
                                    .emit();
                                return;
                            }
                        }
                        _ => ()
                    }
                }
                _ => ()
            }
            let main_def_id = tcx.map.local_def_id(main_id);
            let substs = Substs::empty(tcx);
            let se_ty = tcx.mk_fn_def(main_def_id, substs,
                                      tcx.mk_bare_fn(ty::BareFnTy {
                unsafety: hir::Unsafety::Normal,
                abi: Abi::Rust,
                sig: ty::Binder(ty::FnSig {
                    inputs: Vec::new(),
                    output: tcx.mk_nil(),
                    variadic: false
                })
            }));

            require_same_types(
                ccx,
                TypeOrigin::MainFunctionType(main_span),
                main_t,
                se_ty);
        }
        _ => {
            span_bug!(main_span,
                      "main has a non-function type: found `{}`",
                      main_t);
        }
    }
}

fn check_start_fn_ty(ccx: &CrateCtxt,
                     start_id: ast::NodeId,
                     start_span: Span) {
    let tcx = ccx.tcx;
    let start_t = tcx.node_id_to_type(start_id);
    match start_t.sty {
        ty::TyFnDef(..) => {
            match tcx.map.find(start_id) {
                Some(hir_map::NodeItem(it)) => {
                    match it.node {
                        hir::ItemFn(_,_,_,_,ref ps,_)
                        if ps.is_parameterized() => {
                            let sp = if let Some(sp) = ps.span() { sp } else { start_span };
                            struct_span_err!(tcx.sess, sp, E0132,
                                "start function is not allowed to have type parameters")
                                .span_label(sp,
                                            &format!("start function cannot have type parameters"))
                                .emit();
                            return;
                        }
                        _ => ()
                    }
                }
                _ => ()
            }

            let start_def_id = ccx.tcx.map.local_def_id(start_id);
            let substs = Substs::empty(tcx);
            let se_ty = tcx.mk_fn_def(start_def_id, substs,
                                      tcx.mk_bare_fn(ty::BareFnTy {
                unsafety: hir::Unsafety::Normal,
                abi: Abi::Rust,
                sig: ty::Binder(ty::FnSig {
                    inputs: vec!(
                        tcx.types.isize,
                        tcx.mk_imm_ptr(tcx.mk_imm_ptr(tcx.types.u8))
                    ),
                    output: tcx.types.isize,
                    variadic: false,
                }),
            }));

            require_same_types(
                ccx,
                TypeOrigin::StartFunctionType(start_span),
                start_t,
                se_ty);
        }
        _ => {
            span_bug!(start_span,
                      "start has a non-function type: found `{}`",
                      start_t);
        }
    }
}

fn check_for_entry_fn(ccx: &CrateCtxt) {
    let tcx = ccx.tcx;
    let _task = tcx.dep_graph.in_task(DepNode::CheckEntryFn);
    if let Some((id, sp)) = *tcx.sess.entry_fn.borrow() {
        match tcx.sess.entry_type.get() {
            Some(config::EntryMain) => check_main_fn_ty(ccx, id, sp),
            Some(config::EntryStart) => check_start_fn_ty(ccx, id, sp),
            Some(config::EntryNone) => {}
            None => bug!("entry function without a type")
        }
    }
}

pub fn check_crate<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>)
                             -> CompileResult {
    let time_passes = tcx.sess.time_passes();
    let ccx = CrateCtxt {
        ast_ty_to_ty_cache: RefCell::new(NodeMap()),
        all_traits: RefCell::new(None),
        stack: RefCell::new(Vec::new()),
        tcx: tcx,
        deferred_obligations: RefCell::new(NodeMap()),
    };

    // this ensures that later parts of type checking can assume that items
    // have valid types and not error
    tcx.sess.track_errors(|| {
        time(time_passes, "type collecting", ||
             collect::collect_item_types(&ccx));

    })?;

    time(time_passes, "variance inference", ||
         variance::infer_variance(tcx));

    tcx.sess.track_errors(|| {
      time(time_passes, "coherence checking", ||
          coherence::check_coherence(&ccx));
    })?;

    time(time_passes, "wf checking", || check::check_wf_new(&ccx))?;

    time(time_passes, "item-types checking", || check::check_item_types(&ccx))?;

    time(time_passes, "item-bodies checking", || check::check_item_bodies(&ccx))?;

    time(time_passes, "drop-impl checking", || check::check_drop_impls(&ccx))?;

    check_unused::check_crate(tcx);
    check_for_entry_fn(&ccx);

    let err_count = tcx.sess.err_count();
    if err_count == 0 {
        Ok(())
    } else {
        Err(err_count)
    }
}

__build_diagnostic_array! { librustc_typeck, DIAGNOSTICS }
