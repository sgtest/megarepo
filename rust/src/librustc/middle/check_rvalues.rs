// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Checks that all rvalues in a crate have statically known size. check_crate
// is the public starting point.

use middle::expr_use_visitor as euv;
use middle::mem_categorization as mc;
use middle::ty::ParameterEnvironment;
use middle::ty;
use util::ppaux::ty_to_string;

use syntax::ast;
use syntax::codemap::Span;
use syntax::visit;

pub fn check_crate(tcx: &ty::ctxt,
                   krate: &ast::Crate) {
    let mut rvcx = RvalueContext { tcx: tcx };
    visit::walk_crate(&mut rvcx, krate);
}

struct RvalueContext<'a, 'tcx: 'a> {
    tcx: &'a ty::ctxt<'tcx>,
}

impl<'a, 'tcx, 'v> visit::Visitor<'v> for RvalueContext<'a, 'tcx> {
    fn visit_fn(&mut self,
                fk: visit::FnKind<'v>,
                fd: &'v ast::FnDecl,
                b: &'v ast::Block,
                s: Span,
                fn_id: ast::NodeId) {
        {
            let param_env = ParameterEnvironment::for_item(self.tcx, fn_id);
            let mut delegate = RvalueContextDelegate { tcx: self.tcx, param_env: &param_env };
            let mut euv = euv::ExprUseVisitor::new(&mut delegate, self.tcx, &param_env);
            euv.walk_fn(fd, b);
        }
        visit::walk_fn(self, fk, fd, b, s)
    }
}

struct RvalueContextDelegate<'a, 'tcx: 'a> {
    tcx: &'a ty::ctxt<'tcx>,
    param_env: &'a ty::ParameterEnvironment<'tcx>,
}

impl<'a, 'tcx> euv::Delegate<'tcx> for RvalueContextDelegate<'a, 'tcx> {
    fn consume(&mut self,
               _: ast::NodeId,
               span: Span,
               cmt: mc::cmt<'tcx>,
               _: euv::ConsumeMode) {
        debug!("consume; cmt: {}; type: {}", *cmt, ty_to_string(self.tcx, cmt.ty));
        if !ty::type_is_sized(self.tcx, cmt.ty, self.param_env) {
            span_err!(self.tcx.sess, span, E0161,
                "cannot move a value of type {0}: the size of {0} cannot be statically determined",
                ty_to_string(self.tcx, cmt.ty));
        }
    }

    fn matched_pat(&mut self,
                   _matched_pat: &ast::Pat,
                   _cmt: mc::cmt,
                   _mode: euv::MatchMode) {}

    fn consume_pat(&mut self,
                   _consume_pat: &ast::Pat,
                   _cmt: mc::cmt,
                   _mode: euv::ConsumeMode) {
    }

    fn borrow(&mut self,
              _borrow_id: ast::NodeId,
              _borrow_span: Span,
              _cmt: mc::cmt,
              _loan_region: ty::Region,
              _bk: ty::BorrowKind,
              _loan_cause: euv::LoanCause) {
    }

    fn decl_without_init(&mut self,
                         _id: ast::NodeId,
                         _span: Span) {
    }

    fn mutate(&mut self,
              _assignment_id: ast::NodeId,
              _assignment_span: Span,
              _assignee_cmt: mc::cmt,
              _mode: euv::MutateMode) {
    }
}
