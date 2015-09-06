// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! An experimental pass that scources for `#[rustc_mir]` attributes,
//! builds the resulting MIR, and dumps it out into a file for inspection.
//!
//! The attribute formats that are currently accepted are:
//!
//! - `#[rustc_mir(graphviz="file.gv")]`
//! - `#[rustc_mir(pretty="file.mir")]`

extern crate syntax;
extern crate rustc;
extern crate rustc_front;

use build;
use dot;
use repr::Mir;
use std::fs::File;
use tcx::{PatNode, Cx};

use self::rustc::middle::def_id::DefId;
use self::rustc::middle::infer;
use self::rustc::middle::region::CodeExtentData;
use self::rustc::middle::ty::{self, Ty};
use self::rustc::util::common::ErrorReported;
use self::rustc_front::hir;
use self::rustc_front::attr::{AttrMetaMethods};
use self::rustc_front::visit;
use self::syntax::ast;
use self::syntax::codemap::Span;

pub fn dump_crate(tcx: &ty::ctxt) {
    let mut dump = OuterDump { tcx: tcx };
    visit::walk_crate(&mut dump, tcx.map.krate());
}

///////////////////////////////////////////////////////////////////////////
// OuterDump -- walks a crate, looking for fn items and methods to build MIR from

struct OuterDump<'a,'tcx:'a> {
    tcx: &'a ty::ctxt<'tcx>,
}

impl<'a, 'tcx> OuterDump<'a, 'tcx> {
    fn visit_mir<OP>(&self, attributes: &'tcx [hir::Attribute], mut walk_op: OP)
        where OP: FnMut(&mut InnerDump<'a,'tcx>)
    {
        let mut built_mir = false;

        for attr in attributes {
            if attr.check_name("rustc_mir") {
                let mut closure_dump = InnerDump { tcx: self.tcx, attr: Some(attr) };
                walk_op(&mut closure_dump);
                built_mir = true;
            }
        }

        let always_build_mir = self.tcx.sess.opts.always_build_mir;
        if !built_mir && always_build_mir {
            let mut closure_dump = InnerDump { tcx: self.tcx, attr: None };
            walk_op(&mut closure_dump);
        }
    }
}


impl<'a, 'tcx> visit::Visitor<'tcx> for OuterDump<'a, 'tcx> {
    fn visit_item(&mut self, item: &'tcx hir::Item) {
        self.visit_mir(&item.attrs, |c| visit::walk_item(c, item));
        visit::walk_item(self, item);
    }

    fn visit_trait_item(&mut self, trait_item: &'tcx hir::TraitItem) {
        match trait_item.node {
            hir::MethodTraitItem(_, Some(_)) => {
                self.visit_mir(&trait_item.attrs, |c| visit::walk_trait_item(c, trait_item));
            }
            _ => { }
        }
        visit::walk_trait_item(self, trait_item);
    }
}

///////////////////////////////////////////////////////////////////////////
// InnerDump -- dumps MIR for a single fn and its contained closures

struct InnerDump<'a,'tcx:'a> {
    tcx: &'a ty::ctxt<'tcx>,
    attr: Option<&'a hir::Attribute>,
}

impl<'a, 'tcx> visit::Visitor<'tcx> for InnerDump<'a,'tcx> {
    fn visit_item(&mut self, _: &'tcx hir::Item) {
        // ignore nested items; they need their own graphviz annotation
    }

    fn visit_fn(&mut self,
                fk: visit::FnKind<'tcx>,
                decl: &'tcx hir::FnDecl,
                body: &'tcx hir::Block,
                span: Span,
                id: ast::NodeId) {
        let (prefix, implicit_arg_tys) = match fk {
            visit::FnKind::Closure =>
                (format!("{}-", id), vec![closure_self_ty(&self.tcx, id, body.id)]),
            _ =>
                (format!(""), vec![]),
        };

        let param_env =
            ty::ParameterEnvironment::for_item(self.tcx, id);

        let infcx =
            infer::new_infer_ctxt(self.tcx,
                                  &self.tcx.tables,
                                  Some(param_env),
                                  true);

        match build_mir(Cx::new(&infcx), implicit_arg_tys, id, span, decl, body) {
            Ok(mir) => {
                let meta_item_list =
                    self.attr.iter()
                             .flat_map(|a| a.meta_item_list())
                             .flat_map(|l| l.iter());
                for item in meta_item_list {
                    if item.check_name("graphviz") {
                        match item.value_str() {
                            Some(s) => {
                                match
                                    File::create(format!("{}{}", prefix, s))
                                    .and_then(|ref mut output| dot::render(&mir, output))
                                {
                                    Ok(()) => { }
                                    Err(e) => {
                                        self.tcx.sess.span_fatal(
                                            item.span,
                                            &format!("Error writing graphviz \
                                                      results to `{}`: {}",
                                                     s, e));
                                    }
                                }
                            }
                            None => {
                                self.tcx.sess.span_err(
                                    item.span,
                                    &format!("graphviz attribute requires a path"));
                            }
                        }
                    }
                }
            }
            Err(ErrorReported) => { }
        }

        visit::walk_fn(self, fk, decl, body, span);
    }
}

fn build_mir<'a,'tcx:'a>(cx: Cx<'a,'tcx>,
                         implicit_arg_tys: Vec<Ty<'tcx>>,
                         fn_id: ast::NodeId,
                         span: Span,
                         decl: &'tcx hir::FnDecl,
                         body: &'tcx hir::Block)
                         -> Result<Mir<Cx<'a,'tcx>>, ErrorReported> {
    let arguments =
        decl.inputs
            .iter()
            .map(|arg| {
                let ty = cx.tcx.node_id_to_type(arg.id);
                (ty, PatNode::irrefutable(&arg.pat))
            })
            .collect();

    let parameter_scope =
        cx.tcx.region_maps.lookup_code_extent(
            CodeExtentData::ParameterScope { fn_id: fn_id, body_id: body.id });
    Ok(build::construct(cx,
                        span,
                        implicit_arg_tys,
                        arguments,
                        parameter_scope,
                        body))
}

fn closure_self_ty<'a,'tcx>(tcx: &ty::ctxt<'tcx>,
                            closure_expr_id: ast::NodeId,
                            body_id: ast::NodeId)
                            -> Ty<'tcx>
{
    let closure_ty = tcx.node_id_to_type(closure_expr_id);

    // We're just hard-coding the idea that the signature will be
    // &self or &mut self and hence will have a bound region with
    // number 0, hokey.
    let region =
        ty::Region::ReFree(
            ty::FreeRegion {
                scope: tcx.region_maps.item_extent(body_id),
                bound_region: ty::BoundRegion::BrAnon(0)
            });
    let region =
        tcx.mk_region(region);

    match tcx.closure_kind(DefId::local(closure_expr_id)) {
        ty::ClosureKind::FnClosureKind =>
            tcx.mk_ref(region,
                       ty::TypeAndMut { ty: closure_ty,
                                        mutbl: hir::MutImmutable }),
        ty::ClosureKind::FnMutClosureKind =>
            tcx.mk_ref(region,
                       ty::TypeAndMut { ty: closure_ty,
                                        mutbl: hir::MutMutable }),
        ty::ClosureKind::FnOnceClosureKind =>
            closure_ty
    }
}
