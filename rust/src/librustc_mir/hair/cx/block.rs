// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use hair::*;
use hair::cx::Cx;
use hair::cx::to_ref::ToRef;
use rustc::middle::region::{BlockRemainder, CodeExtent};
use rustc::hir;

impl<'tcx> Mirror<'tcx> for &'tcx hir::Block {
    type Output = Block<'tcx>;

    fn make_mirror<'a, 'gcx>(self, cx: &mut Cx<'a, 'gcx, 'tcx>) -> Block<'tcx> {
        // We have to eagerly translate the "spine" of the statements
        // in order to get the lexical scoping correctly.
        let stmts = mirror_stmts(cx, self.hir_id.local_id, &*self.stmts);
        let opt_destruction_extent = cx.region_maps.opt_destruction_extent(self.hir_id.local_id);
        Block {
            targeted_by_break: self.targeted_by_break,
            extent: CodeExtent::Misc(self.hir_id.local_id),
            opt_destruction_extent,
            span: self.span,
            stmts,
            expr: self.expr.to_ref(),
        }
    }
}

fn mirror_stmts<'a, 'gcx, 'tcx>(cx: &mut Cx<'a, 'gcx, 'tcx>,
                                block_id: hir::ItemLocalId,
                                stmts: &'tcx [hir::Stmt])
                                -> Vec<StmtRef<'tcx>> {
    let mut result = vec![];
    for (index, stmt) in stmts.iter().enumerate() {
        let hir_id = cx.tcx.hir.node_to_hir_id(stmt.node.id());
        let opt_dxn_ext = cx.region_maps.opt_destruction_extent(hir_id.local_id);
        match stmt.node {
            hir::StmtExpr(ref expr, _) |
            hir::StmtSemi(ref expr, _) => {
                result.push(StmtRef::Mirror(Box::new(Stmt {
                    kind: StmtKind::Expr {
                        scope: CodeExtent::Misc(hir_id.local_id),
                        expr: expr.to_ref(),
                    },
                    opt_destruction_extent: opt_dxn_ext,
                })))
            }
            hir::StmtDecl(ref decl, _) => {
                match decl.node {
                    hir::DeclItem(..) => {
                        // ignore for purposes of the MIR
                    }
                    hir::DeclLocal(ref local) => {
                        let remainder_extent = CodeExtent::Remainder(BlockRemainder {
                            block: block_id,
                            first_statement_index: index as u32,
                        });

                        let pattern = Pattern::from_hir(cx.tcx.global_tcx(),
                                                        cx.param_env.and(cx.identity_substs),
                                                        cx.tables(),
                                                        &local.pat);
                        result.push(StmtRef::Mirror(Box::new(Stmt {
                            kind: StmtKind::Let {
                                remainder_scope: remainder_extent,
                                init_scope: CodeExtent::Misc(hir_id.local_id),
                                pattern,
                                initializer: local.init.to_ref(),
                            },
                            opt_destruction_extent: opt_dxn_ext,
                        })));
                    }
                }
            }
        }
    }
    return result;
}

pub fn to_expr_ref<'a, 'gcx, 'tcx>(cx: &mut Cx<'a, 'gcx, 'tcx>,
                                   block: &'tcx hir::Block)
                                   -> ExprRef<'tcx> {
    let block_ty = cx.tables().node_id_to_type(block.hir_id);
    let temp_lifetime = cx.region_maps.temporary_scope(block.hir_id.local_id);
    let expr = Expr {
        ty: block_ty,
        temp_lifetime,
        span: block.span,
        kind: ExprKind::Block { body: block },
    };
    expr.to_ref()
}
