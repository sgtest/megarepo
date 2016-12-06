// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use dep_graph::DepNode;
use hir::def::Def;
use hir::def_id::DefId;
use infer::InferCtxt;
use traits::Reveal;
use ty::{self, Ty, TyCtxt};
use ty::layout::{LayoutError, Pointer, SizeSkeleton};

use syntax::abi::Abi::RustIntrinsic;
use syntax::ast;
use syntax_pos::Span;
use hir::intravisit::{self, Visitor, FnKind, NestedVisitorMap};
use hir;

pub fn check_crate<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>) {
    let mut visitor = ItemVisitor {
        tcx: tcx
    };
    tcx.visit_all_item_likes_in_krate(DepNode::IntrinsicCheck, &mut visitor.as_deep_visitor());
}

struct ItemVisitor<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>
}

impl<'a, 'tcx> ItemVisitor<'a, 'tcx> {
    fn visit_const(&mut self, item_id: ast::NodeId, expr: &'tcx hir::Expr) {
        let param_env = ty::ParameterEnvironment::for_item(self.tcx, item_id);
        self.tcx.infer_ctxt(None, Some(param_env), Reveal::All).enter(|infcx| {
            let mut visitor = ExprVisitor {
                infcx: &infcx
            };
            visitor.visit_expr(expr);
        });
    }
}

struct ExprVisitor<'a, 'gcx: 'a+'tcx, 'tcx: 'a> {
    infcx: &'a InferCtxt<'a, 'gcx, 'tcx>
}

impl<'a, 'gcx, 'tcx> ExprVisitor<'a, 'gcx, 'tcx> {
    fn def_id_is_transmute(&self, def_id: DefId) -> bool {
        let intrinsic = match self.infcx.tcx.item_type(def_id).sty {
            ty::TyFnDef(.., ref bfty) => bfty.abi == RustIntrinsic,
            _ => return false
        };
        intrinsic && self.infcx.tcx.item_name(def_id) == "transmute"
    }

    fn check_transmute(&self, span: Span, from: Ty<'gcx>, to: Ty<'gcx>, id: ast::NodeId) {
        let sk_from = SizeSkeleton::compute(from, self.infcx);
        let sk_to = SizeSkeleton::compute(to, self.infcx);

        // Check for same size using the skeletons.
        if let (Ok(sk_from), Ok(sk_to)) = (sk_from, sk_to) {
            if sk_from.same_size(sk_to) {
                return;
            }

            match (&from.sty, sk_to) {
                (&ty::TyFnDef(..), SizeSkeleton::Known(size_to))
                        if size_to == Pointer.size(&self.infcx.tcx.data_layout) => {
                    // FIXME #19925 Remove this warning after a release cycle.
                    let msg = format!("`{}` is now zero-sized and has to be cast \
                                       to a pointer before transmuting to `{}`",
                                      from, to);
                    self.infcx.tcx.sess.add_lint(
                        ::lint::builtin::TRANSMUTE_FROM_FN_ITEM_TYPES, id, span, msg);
                    return;
                }
                _ => {}
            }
        }

        // Try to display a sensible error with as much information as possible.
        let skeleton_string = |ty: Ty<'gcx>, sk| {
            match sk {
                Ok(SizeSkeleton::Known(size)) => {
                    format!("{} bits", size.bits())
                }
                Ok(SizeSkeleton::Pointer { tail, .. }) => {
                    format!("pointer to {}", tail)
                }
                Err(LayoutError::Unknown(bad)) => {
                    if bad == ty {
                        format!("size can vary")
                    } else {
                        format!("size can vary because of {}", bad)
                    }
                }
                Err(err) => err.to_string()
            }
        };

        struct_span_err!(self.infcx.tcx.sess, span, E0512,
                  "transmute called with differently sized types: \
                   {} ({}) to {} ({})",
                  from, skeleton_string(from, sk_from),
                  to, skeleton_string(to, sk_to))
            .span_label(span,
                &format!("transmuting between {} and {}",
                    skeleton_string(from, sk_from),
                    skeleton_string(to, sk_to)))
            .emit();
    }
}

impl<'a, 'tcx> Visitor<'tcx> for ItemVisitor<'a, 'tcx> {
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'tcx> {
        NestedVisitorMap::OnlyBodies(&self.tcx.map)
    }

    // const, static and N in [T; N].
    fn visit_expr(&mut self, expr: &'tcx hir::Expr) {
        self.tcx.infer_ctxt(None, None, Reveal::All).enter(|infcx| {
            let mut visitor = ExprVisitor {
                infcx: &infcx
            };
            visitor.visit_expr(expr);
        });
    }

    fn visit_trait_item(&mut self, item: &'tcx hir::TraitItem) {
        if let hir::ConstTraitItem(_, Some(ref expr)) = item.node {
            self.visit_const(item.id, expr);
        } else {
            intravisit::walk_trait_item(self, item);
        }
    }

    fn visit_impl_item(&mut self, item: &'tcx hir::ImplItem) {
        if let hir::ImplItemKind::Const(_, ref expr) = item.node {
            self.visit_const(item.id, expr);
        } else {
            intravisit::walk_impl_item(self, item);
        }
    }

    fn visit_fn(&mut self, fk: FnKind<'tcx>, fd: &'tcx hir::FnDecl,
                b: hir::ExprId, s: Span, id: ast::NodeId) {
        if let FnKind::Closure(..) = fk {
            span_bug!(s, "intrinsicck: closure outside of function")
        }
        let param_env = ty::ParameterEnvironment::for_item(self.tcx, id);
        self.tcx.infer_ctxt(None, Some(param_env), Reveal::All).enter(|infcx| {
            let mut visitor = ExprVisitor {
                infcx: &infcx
            };
            visitor.visit_fn(fk, fd, b, s, id);
        });
    }
}

impl<'a, 'gcx, 'tcx> Visitor<'gcx> for ExprVisitor<'a, 'gcx, 'tcx> {
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'gcx> {
        NestedVisitorMap::OnlyBodies(&self.infcx.tcx.map)
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr) {
        let def = if let hir::ExprPath(ref qpath) = expr.node {
            self.infcx.tcx.tables().qpath_def(qpath, expr.id)
        } else {
            Def::Err
        };
        match def {
            Def::Fn(did) if self.def_id_is_transmute(did) => {
                let typ = self.infcx.tcx.tables().node_id_to_type(expr.id);
                match typ.sty {
                    ty::TyFnDef(.., ref bare_fn_ty) if bare_fn_ty.abi == RustIntrinsic => {
                        let from = bare_fn_ty.sig.skip_binder().inputs()[0];
                        let to = bare_fn_ty.sig.skip_binder().output();
                        self.check_transmute(expr.span, from, to, expr.id);
                    }
                    _ => {
                        span_bug!(expr.span, "transmute wasn't a bare fn?!");
                    }
                }
            }
            _ => {}
        }

        intravisit::walk_expr(self, expr);
    }
}
