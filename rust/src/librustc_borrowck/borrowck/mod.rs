// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! See The Book chapter on the borrow checker for more details.

#![allow(non_camel_case_types)]

pub use self::LoanPathKind::*;
pub use self::LoanPathElem::*;
pub use self::bckerr_code::*;
pub use self::AliasableViolationKind::*;
pub use self::MovedValueUseKind::*;

pub use self::mir::elaborate_drops::ElaborateDrops;

use self::InteriorKind::*;

use rustc::dep_graph::DepNode;
use rustc::hir::map as hir_map;
use rustc::hir::map::blocks::FnLikeNode;
use rustc::cfg;
use rustc::middle::dataflow::DataFlowContext;
use rustc::middle::dataflow::BitwiseOperator;
use rustc::middle::dataflow::DataFlowOperator;
use rustc::middle::dataflow::KillFrom;
use rustc::hir::def_id::DefId;
use rustc::middle::expr_use_visitor as euv;
use rustc::middle::mem_categorization as mc;
use rustc::middle::mem_categorization::Categorization;
use rustc::middle::region;
use rustc::ty::{self, TyCtxt};

use std::fmt;
use std::rc::Rc;
use std::hash::{Hash, Hasher};
use syntax::ast;
use syntax_pos::{MultiSpan, Span};
use errors::DiagnosticBuilder;

use rustc::hir;
use rustc::hir::intravisit::{self, Visitor};

pub mod check_loans;

pub mod gather_loans;

pub mod move_data;

mod mir;

#[derive(Clone, Copy)]
pub struct LoanDataFlowOperator;

pub type LoanDataFlow<'a, 'tcx> = DataFlowContext<'a, 'tcx, LoanDataFlowOperator>;

pub fn check_crate<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>) {
    tcx.dep_graph.with_task(DepNode::BorrowCheckKrate, || {
        tcx.visit_all_bodies_in_krate(|body_owner_def_id, body_id| {
            tcx.dep_graph.with_task(DepNode::BorrowCheck(body_owner_def_id), || {
                borrowck_fn(tcx, body_id);
            });
        });
    });
}

/// Collection of conclusions determined via borrow checker analyses.
pub struct AnalysisData<'a, 'tcx: 'a> {
    pub all_loans: Vec<Loan<'tcx>>,
    pub loans: DataFlowContext<'a, 'tcx, LoanDataFlowOperator>,
    pub move_data: move_data::FlowedMoveData<'a, 'tcx>,
}

fn borrowck_fn<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, body_id: hir::BodyId) {
    debug!("borrowck_fn(body_id={:?})", body_id);

    let owner_id = tcx.hir.body_owner(body_id);
    let owner_def_id = tcx.hir.local_def_id(owner_id);
    let attributes = tcx.get_attrs(owner_def_id);
    let tables = tcx.item_tables(owner_def_id);

    let mut bccx = &mut BorrowckCtxt {
        tcx: tcx,
        tables: tables,
    };

    let body = bccx.tcx.hir.body(body_id);

    if bccx.tcx.has_attr(owner_def_id, "rustc_mir_borrowck") {
        mir::borrowck_mir(bccx, owner_id, &attributes);
    }

    let cfg = cfg::CFG::new(bccx.tcx, &body);
    let AnalysisData { all_loans,
                       loans: loan_dfcx,
                       move_data: flowed_moves } =
        build_borrowck_dataflow_data(bccx, &cfg, body_id);

    move_data::fragments::instrument_move_fragments(&flowed_moves.move_data,
                                                    bccx.tcx,
                                                    owner_id);
    move_data::fragments::build_unfragmented_map(bccx,
                                                 &flowed_moves.move_data,
                                                 owner_id);

    check_loans::check_loans(bccx, &loan_dfcx, &flowed_moves, &all_loans[..], body);
}

fn build_borrowck_dataflow_data<'a, 'tcx>(this: &mut BorrowckCtxt<'a, 'tcx>,
                                          cfg: &cfg::CFG,
                                          body_id: hir::BodyId)
                                          -> AnalysisData<'a, 'tcx>
{
    // Check the body of fn items.
    let tcx = this.tcx;
    let body = tcx.hir.body(body_id);
    let id_range = {
        let mut visitor = intravisit::IdRangeComputingVisitor::new(&tcx.hir);
        visitor.visit_body(body);
        visitor.result()
    };
    let (all_loans, move_data) =
        gather_loans::gather_loans_in_fn(this, body_id);

    let mut loan_dfcx =
        DataFlowContext::new(this.tcx,
                             "borrowck",
                             Some(body),
                             cfg,
                             LoanDataFlowOperator,
                             id_range,
                             all_loans.len());
    for (loan_idx, loan) in all_loans.iter().enumerate() {
        loan_dfcx.add_gen(loan.gen_scope.node_id(&tcx.region_maps), loan_idx);
        loan_dfcx.add_kill(KillFrom::ScopeEnd,
                           loan.kill_scope.node_id(&tcx.region_maps), loan_idx);
    }
    loan_dfcx.add_kills_from_flow_exits(cfg);
    loan_dfcx.propagate(cfg, body);

    let flowed_moves = move_data::FlowedMoveData::new(move_data,
                                                      this.tcx,
                                                      cfg,
                                                      id_range,
                                                      body);

    AnalysisData { all_loans: all_loans,
                   loans: loan_dfcx,
                   move_data:flowed_moves }
}

/// Accessor for introspective clients inspecting `AnalysisData` and
/// the `BorrowckCtxt` itself , e.g. the flowgraph visualizer.
pub fn build_borrowck_dataflow_data_for_fn<'a, 'tcx>(
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    body_id: hir::BodyId,
    cfg: &cfg::CFG)
    -> (BorrowckCtxt<'a, 'tcx>, AnalysisData<'a, 'tcx>)
{
    let owner_id = tcx.hir.body_owner(body_id);
    let owner_def_id = tcx.hir.local_def_id(owner_id);
    let tables = tcx.item_tables(owner_def_id);

    let mut bccx = BorrowckCtxt {
        tcx: tcx,
        tables: tables,
    };

    let dataflow_data = build_borrowck_dataflow_data(&mut bccx, cfg, body_id);
    (bccx, dataflow_data)
}

// ----------------------------------------------------------------------
// Type definitions

pub struct BorrowckCtxt<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,

    // tables for the current thing we are checking; set to
    // Some in `borrowck_fn` and cleared later
    tables: &'a ty::TypeckTables<'tcx>,
}

///////////////////////////////////////////////////////////////////////////
// Loans and loan paths

/// Record of a loan that was issued.
pub struct Loan<'tcx> {
    index: usize,
    loan_path: Rc<LoanPath<'tcx>>,
    kind: ty::BorrowKind,
    restricted_paths: Vec<Rc<LoanPath<'tcx>>>,

    /// gen_scope indicates where loan is introduced. Typically the
    /// loan is introduced at the point of the borrow, but in some
    /// cases, notably method arguments, the loan may be introduced
    /// only later, once it comes into scope.  See also
    /// `GatherLoanCtxt::compute_gen_scope`.
    gen_scope: region::CodeExtent,

    /// kill_scope indicates when the loan goes out of scope.  This is
    /// either when the lifetime expires or when the local variable
    /// which roots the loan-path goes out of scope, whichever happens
    /// faster. See also `GatherLoanCtxt::compute_kill_scope`.
    kill_scope: region::CodeExtent,
    span: Span,
    cause: euv::LoanCause,
}

impl<'tcx> Loan<'tcx> {
    pub fn loan_path(&self) -> Rc<LoanPath<'tcx>> {
        self.loan_path.clone()
    }
}

#[derive(Eq)]
pub struct LoanPath<'tcx> {
    kind: LoanPathKind<'tcx>,
    ty: ty::Ty<'tcx>,
}

impl<'tcx> PartialEq for LoanPath<'tcx> {
    fn eq(&self, that: &LoanPath<'tcx>) -> bool {
        self.kind == that.kind
    }
}

impl<'tcx> Hash for LoanPath<'tcx> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
    }
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub enum LoanPathKind<'tcx> {
    LpVar(ast::NodeId),                         // `x` in README.md
    LpUpvar(ty::UpvarId),                       // `x` captured by-value into closure
    LpDowncast(Rc<LoanPath<'tcx>>, DefId), // `x` downcast to particular enum variant
    LpExtend(Rc<LoanPath<'tcx>>, mc::MutabilityCategory, LoanPathElem<'tcx>)
}

impl<'tcx> LoanPath<'tcx> {
    fn new(kind: LoanPathKind<'tcx>, ty: ty::Ty<'tcx>) -> LoanPath<'tcx> {
        LoanPath { kind: kind, ty: ty }
    }

    fn to_type(&self) -> ty::Ty<'tcx> { self.ty }
}

// FIXME (pnkfelix): See discussion here
// https://github.com/pnkfelix/rust/commit/
//     b2b39e8700e37ad32b486b9a8409b50a8a53aa51#commitcomment-7892003
const DOWNCAST_PRINTED_OPERATOR: &'static str = " as ";

// A local, "cleaned" version of `mc::InteriorKind` that drops
// information that is not relevant to loan-path analysis. (In
// particular, the distinction between how precisely an array-element
// is tracked is irrelevant here.)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteriorKind {
    InteriorField(mc::FieldName),
    InteriorElement(mc::ElementKind),
}

trait ToInteriorKind { fn cleaned(self) -> InteriorKind; }
impl ToInteriorKind for mc::InteriorKind {
    fn cleaned(self) -> InteriorKind {
        match self {
            mc::InteriorField(name) => InteriorField(name),
            mc::InteriorElement(_, elem_kind) => InteriorElement(elem_kind),
        }
    }
}

// This can be:
// - a pointer dereference (`*LV` in README.md)
// - a field reference, with an optional definition of the containing
//   enum variant (`LV.f` in README.md)
// `DefId` is present when the field is part of struct that is in
// a variant of an enum. For instance in:
// `enum E { X { foo: u32 }, Y { foo: u32 }}`
// each `foo` is qualified by the definitition id of the variant (`X` or `Y`).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum LoanPathElem<'tcx> {
    LpDeref(mc::PointerKind<'tcx>),
    LpInterior(Option<DefId>, InteriorKind),
}

pub fn closure_to_block(closure_id: ast::NodeId,
                        tcx: TyCtxt) -> ast::NodeId {
    match tcx.hir.get(closure_id) {
        hir_map::NodeExpr(expr) => match expr.node {
            hir::ExprClosure(.., body_id, _) => {
                body_id.node_id
            }
            _ => {
                bug!("encountered non-closure id: {}", closure_id)
            }
        },
        _ => bug!("encountered non-expr id: {}", closure_id)
    }
}

impl<'a, 'tcx> LoanPath<'tcx> {
    pub fn kill_scope(&self, tcx: TyCtxt<'a, 'tcx, 'tcx>) -> region::CodeExtent {
        match self.kind {
            LpVar(local_id) => tcx.region_maps.var_scope(local_id),
            LpUpvar(upvar_id) => {
                let block_id = closure_to_block(upvar_id.closure_expr_id, tcx);
                tcx.region_maps.node_extent(block_id)
            }
            LpDowncast(ref base, _) |
            LpExtend(ref base, ..) => base.kill_scope(tcx),
        }
    }

    fn has_fork(&self, other: &LoanPath<'tcx>) -> bool {
        match (&self.kind, &other.kind) {
            (&LpExtend(ref base, _, LpInterior(opt_variant_id, id)),
             &LpExtend(ref base2, _, LpInterior(opt_variant_id2, id2))) =>
                if id == id2 && opt_variant_id == opt_variant_id2 {
                    base.has_fork(&base2)
                } else {
                    true
                },
            (&LpExtend(ref base, _, LpDeref(_)), _) => base.has_fork(other),
            (_, &LpExtend(ref base, _, LpDeref(_))) => self.has_fork(&base),
            _ => false,
        }
    }

    fn depth(&self) -> usize {
        match self.kind {
            LpExtend(ref base, _, LpDeref(_)) => base.depth(),
            LpExtend(ref base, _, LpInterior(..)) => base.depth() + 1,
            _ => 0,
        }
    }

    fn common(&self, other: &LoanPath<'tcx>) -> Option<LoanPath<'tcx>> {
        match (&self.kind, &other.kind) {
            (&LpExtend(ref base, a, LpInterior(opt_variant_id, id)),
             &LpExtend(ref base2, _, LpInterior(opt_variant_id2, id2))) => {
                if id == id2 && opt_variant_id == opt_variant_id2 {
                    base.common(&base2).map(|x| {
                        let xd = x.depth();
                        if base.depth() == xd && base2.depth() == xd {
                            LoanPath {
                                kind: LpExtend(Rc::new(x), a, LpInterior(opt_variant_id, id)),
                                ty: self.ty,
                            }
                        } else {
                            x
                        }
                    })
                } else {
                    base.common(&base2)
                }
            }
            (&LpExtend(ref base, _, LpDeref(_)), _) => base.common(other),
            (_, &LpExtend(ref other, _, LpDeref(_))) => self.common(&other),
            (&LpVar(id), &LpVar(id2)) => {
                if id == id2 {
                    Some(LoanPath { kind: LpVar(id), ty: self.ty })
                } else {
                    None
                }
            }
            (&LpUpvar(id), &LpUpvar(id2)) => {
                if id == id2 {
                    Some(LoanPath { kind: LpUpvar(id), ty: self.ty })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

pub fn opt_loan_path<'tcx>(cmt: &mc::cmt<'tcx>) -> Option<Rc<LoanPath<'tcx>>> {
    //! Computes the `LoanPath` (if any) for a `cmt`.
    //! Note that this logic is somewhat duplicated in
    //! the method `compute()` found in `gather_loans::restrictions`,
    //! which allows it to share common loan path pieces as it
    //! traverses the CMT.

    let new_lp = |v: LoanPathKind<'tcx>| Rc::new(LoanPath::new(v, cmt.ty));

    match cmt.cat {
        Categorization::Rvalue(..) |
        Categorization::StaticItem => {
            None
        }

        Categorization::Local(id) => {
            Some(new_lp(LpVar(id)))
        }

        Categorization::Upvar(mc::Upvar { id, .. }) => {
            Some(new_lp(LpUpvar(id)))
        }

        Categorization::Deref(ref cmt_base, _, pk) => {
            opt_loan_path(cmt_base).map(|lp| {
                new_lp(LpExtend(lp, cmt.mutbl, LpDeref(pk)))
            })
        }

        Categorization::Interior(ref cmt_base, ik) => {
            opt_loan_path(cmt_base).map(|lp| {
                let opt_variant_id = match cmt_base.cat {
                    Categorization::Downcast(_, did) =>  Some(did),
                    _ => None
                };
                new_lp(LpExtend(lp, cmt.mutbl, LpInterior(opt_variant_id, ik.cleaned())))
            })
        }

        Categorization::Downcast(ref cmt_base, variant_def_id) =>
            opt_loan_path(cmt_base)
            .map(|lp| {
                new_lp(LpDowncast(lp, variant_def_id))
            }),

    }
}

///////////////////////////////////////////////////////////////////////////
// Errors

// Errors that can occur
#[derive(Debug, PartialEq)]
pub enum bckerr_code<'tcx> {
    err_mutbl,
    /// superscope, subscope, loan cause
    err_out_of_scope(&'tcx ty::Region, &'tcx ty::Region, euv::LoanCause),
    err_borrowed_pointer_too_short(&'tcx ty::Region, &'tcx ty::Region), // loan, ptr
}

// Combination of an error code and the categorization of the expression
// that caused it
#[derive(Debug, PartialEq)]
pub struct BckError<'tcx> {
    span: Span,
    cause: AliasableViolationKind,
    cmt: mc::cmt<'tcx>,
    code: bckerr_code<'tcx>
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum AliasableViolationKind {
    MutabilityViolation,
    BorrowViolation(euv::LoanCause)
}

#[derive(Copy, Clone, Debug)]
pub enum MovedValueUseKind {
    MovedInUse,
    MovedInCapture,
}

///////////////////////////////////////////////////////////////////////////
// Misc

impl<'a, 'tcx> BorrowckCtxt<'a, 'tcx> {
    pub fn is_subregion_of(&self,
                           r_sub: &'tcx ty::Region,
                           r_sup: &'tcx ty::Region)
                           -> bool
    {
        self.tables.free_region_map.is_subregion_of(self.tcx, r_sub, r_sup)
    }

    pub fn report(&self, err: BckError<'tcx>) {
        // Catch and handle some particular cases.
        match (&err.code, &err.cause) {
            (&err_out_of_scope(&ty::ReScope(_), &ty::ReStatic, _),
             &BorrowViolation(euv::ClosureCapture(span))) |
            (&err_out_of_scope(&ty::ReScope(_), &ty::ReFree(..), _),
             &BorrowViolation(euv::ClosureCapture(span))) => {
                return self.report_out_of_scope_escaping_closure_capture(&err, span);
            }
            _ => { }
        }

        let mut db = self.bckerr_to_diag(&err);
        self.note_and_explain_bckerr(&mut db, err);
        db.emit();
    }

    pub fn report_use_of_moved_value(&self,
                                     use_span: Span,
                                     use_kind: MovedValueUseKind,
                                     lp: &LoanPath<'tcx>,
                                     the_move: &move_data::Move,
                                     moved_lp: &LoanPath<'tcx>,
                                     _param_env: &ty::ParameterEnvironment<'tcx>) {
        let (verb, verb_participle) = match use_kind {
            MovedInUse => ("use", "used"),
            MovedInCapture => ("capture", "captured"),
        };

        let (_ol, _moved_lp_msg, mut err) = match the_move.kind {
            move_data::Declared => {
                // If this is an uninitialized variable, just emit a simple warning
                // and return.
                struct_span_err!(
                    self.tcx.sess, use_span, E0381,
                    "{} of possibly uninitialized variable: `{}`",
                    verb,
                    self.loan_path_to_string(lp))
                .span_label(use_span, &format!("use of possibly uninitialized `{}`",
                    self.loan_path_to_string(lp)))
                .emit();
                return;
            }
            _ => {
                // If moved_lp is something like `x.a`, and lp is something like `x.b`, we would
                // normally generate a rather confusing message:
                //
                //     error: use of moved value: `x.b`
                //     note: `x.a` moved here...
                //
                // What we want to do instead is get the 'common ancestor' of the two moves and
                // use that for most of the message instead, giving is something like this:
                //
                //     error: use of moved value: `x`
                //     note: `x` moved here (through moving `x.a`)...

                let common = moved_lp.common(lp);
                let has_common = common.is_some();
                let has_fork = moved_lp.has_fork(lp);
                let (nl, ol, moved_lp_msg) =
                    if has_fork && has_common {
                        let nl = self.loan_path_to_string(&common.unwrap());
                        let ol = nl.clone();
                        let moved_lp_msg = format!(" (through moving `{}`)",
                                                   self.loan_path_to_string(moved_lp));
                        (nl, ol, moved_lp_msg)
                    } else {
                        (self.loan_path_to_string(lp),
                         self.loan_path_to_string(moved_lp),
                         String::new())
                    };

                let partial = moved_lp.depth() > lp.depth();
                let msg = if !has_fork && partial { "partially " }
                          else if has_fork && !has_common { "collaterally "}
                          else { "" };
                let err = struct_span_err!(
                    self.tcx.sess, use_span, E0382,
                    "{} of {}moved value: `{}`",
                    verb, msg, nl);
                (ol, moved_lp_msg, err)}
        };

        // Get type of value and span where it was previously
        // moved.
        let (move_span, move_note) = match the_move.kind {
            move_data::Declared => {
                unreachable!();
            }

            move_data::MoveExpr |
            move_data::MovePat =>
                (self.tcx.hir.span(the_move.id), ""),

            move_data::Captured =>
                (match self.tcx.hir.expect_expr(the_move.id).node {
                    hir::ExprClosure(.., fn_decl_span) => fn_decl_span,
                    ref r => bug!("Captured({}) maps to non-closure: {:?}",
                                  the_move.id, r),
                }, " (into closure)"),
        };

        // Annotate the use and the move in the span. Watch out for
        // the case where the use and the move are the same. This
        // means the use is in a loop.
        err = if use_span == move_span {
            err.span_label(
                use_span,
                &format!("value moved{} here in previous iteration of loop",
                         move_note));
            err
        } else {
            err.span_label(use_span, &format!("value {} here after move", verb_participle))
               .span_label(move_span, &format!("value moved{} here", move_note));
            err
        };

        err.note(&format!("move occurs because `{}` has type `{}`, \
                           which does not implement the `Copy` trait",
                          self.loan_path_to_string(moved_lp),
                          moved_lp.ty));

        // Note: we used to suggest adding a `ref binding` or calling
        // `clone` but those suggestions have been removed because
        // they are often not what you actually want to do, and were
        // not considered particularly helpful.

        err.emit();
    }

    pub fn report_partial_reinitialization_of_uninitialized_structure(
            &self,
            span: Span,
            lp: &LoanPath<'tcx>) {
        span_err!(
            self.tcx.sess, span, E0383,
            "partial reinitialization of uninitialized structure `{}`",
            self.loan_path_to_string(lp));
    }

    pub fn report_reassigned_immutable_variable(&self,
                                                span: Span,
                                                lp: &LoanPath<'tcx>,
                                                assign:
                                                &move_data::Assignment) {
        let mut err = struct_span_err!(
            self.tcx.sess, span, E0384,
            "re-assignment of immutable variable `{}`",
            self.loan_path_to_string(lp));
        err.span_label(span, &format!("re-assignment of immutable variable"));
        if span != assign.span {
            err.span_label(assign.span, &format!("first assignment to `{}`",
                                              self.loan_path_to_string(lp)));
        }
        err.emit();
    }

    pub fn span_err(&self, s: Span, m: &str) {
        self.tcx.sess.span_err(s, m);
    }

    pub fn struct_span_err<S: Into<MultiSpan>>(&self, s: S, m: &str)
                                              -> DiagnosticBuilder<'a> {
        self.tcx.sess.struct_span_err(s, m)
    }

    pub fn struct_span_err_with_code<S: Into<MultiSpan>>(&self,
                                                         s: S,
                                                         msg: &str,
                                                         code: &str)
                                                         -> DiagnosticBuilder<'a> {
        self.tcx.sess.struct_span_err_with_code(s, msg, code)
    }

    pub fn span_err_with_code<S: Into<MultiSpan>>(&self, s: S, msg: &str, code: &str) {
        self.tcx.sess.span_err_with_code(s, msg, code);
    }

    pub fn bckerr_to_diag(&self, err: &BckError<'tcx>) -> DiagnosticBuilder<'a> {
        let span = err.span.clone();
        let mut immutable_field = None;

        let msg = &match err.code {
            err_mutbl => {
                let descr = match err.cmt.note {
                    mc::NoteClosureEnv(_) | mc::NoteUpvarRef(_) => {
                        self.cmt_to_string(&err.cmt)
                    }
                    _ => match opt_loan_path(&err.cmt) {
                        None => {
                            format!("{} {}",
                                    err.cmt.mutbl.to_user_str(),
                                    self.cmt_to_string(&err.cmt))

                        }
                        Some(lp) => {
                            format!("{} {} `{}`",
                                    err.cmt.mutbl.to_user_str(),
                                    self.cmt_to_string(&err.cmt),
                                    self.loan_path_to_string(&lp))
                        }
                    }
                };

                match err.cause {
                    MutabilityViolation => {
                        format!("cannot assign to {}", descr)
                    }
                    BorrowViolation(euv::ClosureCapture(_)) => {
                        format!("closure cannot assign to {}", descr)
                    }
                    BorrowViolation(euv::OverloadedOperator) |
                    BorrowViolation(euv::AddrOf) |
                    BorrowViolation(euv::RefBinding) |
                    BorrowViolation(euv::AutoRef) |
                    BorrowViolation(euv::AutoUnsafe) |
                    BorrowViolation(euv::ForLoop) |
                    BorrowViolation(euv::MatchDiscriminant) => {
                        // Check for this field's definition to see if it is an immutable reference
                        // and suggest making it mutable if that is the case.
                        immutable_field = err.cmt.get_field_name()
                            .and_then(|name| err.cmt.get_field(name))
                            .and_then(|did| self.tcx.hir.as_local_node_id(did))
                            .and_then(|nid| {
                                if let hir_map::Node::NodeField(ref field) = self.tcx.hir.get(nid) {
                                    return self.suggest_mut_for_immutable(&field.ty)
                                        .map(|msg| (self.tcx.hir.span(nid), msg));
                                }
                                None
                            });

                        format!("cannot borrow {} as mutable", descr)
                    }
                    BorrowViolation(euv::ClosureInvocation) => {
                        span_bug!(err.span,
                            "err_mutbl with a closure invocation");
                    }
                }
            }
            err_out_of_scope(..) => {
                let msg = match opt_loan_path(&err.cmt) {
                    None => "borrowed value".to_string(),
                    Some(lp) => {
                        format!("`{}`", self.loan_path_to_string(&lp))
                    }
                };
                format!("{} does not live long enough", msg)
            }
            err_borrowed_pointer_too_short(..) => {
                let descr = self.cmt_to_path_or_string(&err.cmt);
                format!("lifetime of {} is too short to guarantee \
                         its contents can be safely reborrowed",
                        descr)
            }
        };

        let mut db = self.struct_span_err(span, msg);
        if let Some((span, msg)) = immutable_field {
            db.span_label(span, &msg);
        }
        db
    }

    pub fn report_aliasability_violation(&self,
                                         span: Span,
                                         kind: AliasableViolationKind,
                                         cause: mc::AliasableReason,
                                         cmt: mc::cmt<'tcx>) {
        let mut is_closure = false;
        let prefix = match kind {
            MutabilityViolation => {
                "cannot assign to data"
            }
            BorrowViolation(euv::ClosureCapture(_)) |
            BorrowViolation(euv::OverloadedOperator) |
            BorrowViolation(euv::AddrOf) |
            BorrowViolation(euv::AutoRef) |
            BorrowViolation(euv::AutoUnsafe) |
            BorrowViolation(euv::RefBinding) |
            BorrowViolation(euv::MatchDiscriminant) => {
                "cannot borrow data mutably"
            }

            BorrowViolation(euv::ClosureInvocation) => {
                is_closure = true;
                "closure invocation"
            }

            BorrowViolation(euv::ForLoop) => {
                "`for` loop"
            }
        };

        let mut err = match cause {
            mc::AliasableOther => {
                struct_span_err!(
                    self.tcx.sess, span, E0385,
                    "{} in an aliasable location", prefix)
            }
            mc::AliasableReason::UnaliasableImmutable => {
                struct_span_err!(
                    self.tcx.sess, span, E0386,
                    "{} in an immutable container", prefix)
            }
            mc::AliasableClosure(id) => {
                let mut err = struct_span_err!(
                    self.tcx.sess, span, E0387,
                    "{} in a captured outer variable in an `Fn` closure", prefix);
                if let BorrowViolation(euv::ClosureCapture(_)) = kind {
                    // The aliasability violation with closure captures can
                    // happen for nested closures, so we know the enclosing
                    // closure incorrectly accepts an `Fn` while it needs to
                    // be `FnMut`.
                    span_help!(&mut err, self.tcx.hir.span(id),
                           "consider changing this to accept closures that implement `FnMut`");
                } else {
                    span_help!(&mut err, self.tcx.hir.span(id),
                           "consider changing this closure to take self by mutable reference");
                }
                err
            }
            mc::AliasableStatic |
            mc::AliasableStaticMut => {
                // This path cannot occur. It happens when we have an
                // `&mut` or assignment to a static. But in the case
                // of `static X`, we get a mutability violation first,
                // and never get here. In the case of `static mut X`,
                // that is unsafe and hence the aliasability error is
                // ignored.
                span_bug!(span, "aliasability violation for static `{}`", prefix)
            }
            mc::AliasableBorrowed => {
                let mut e = struct_span_err!(
                    self.tcx.sess, span, E0389,
                    "{} in a `&` reference", prefix);
                e.span_label(span, &"assignment into an immutable reference");
                if let Some(nid) = cmt.get_arg_if_immutable(&self.tcx.hir) {
                    self.immutable_argument_should_be_mut(nid, &mut e);
                }
                e
            }
        };

        if is_closure {
            err.help("closures behind references must be called via `&mut`");
        }
        err.emit();
    }

    /// Given a type, if it is an immutable reference, return a suggestion to make it mutable
    fn suggest_mut_for_immutable(&self, pty: &hir::Ty) -> Option<String> {
        // Check wether the argument is an immutable reference
        if let hir::TyRptr(lifetime, hir::MutTy {
            mutbl: hir::Mutability::MutImmutable,
            ref ty
        }) = pty.node {
            // Account for existing lifetimes when generating the message
            if !lifetime.is_elided() {
                if let Ok(snippet) = self.tcx.sess.codemap().span_to_snippet(ty.span) {
                    if let Ok(lifetime_snippet) = self.tcx.sess.codemap()
                        .span_to_snippet(lifetime.span) {
                            return Some(format!("use `&{} mut {}` here to make mutable",
                                                lifetime_snippet,
                                                snippet));
                    }
                }
            } else if let Ok(snippet) = self.tcx.sess.codemap().span_to_snippet(pty.span) {
                if snippet.starts_with("&") {
                    return Some(format!("use `{}` here to make mutable",
                                        snippet.replace("&", "&mut ")));
                }
            } else {
                bug!("couldn't find a snippet for span: {:?}", pty.span);
            }
        }
        None
    }

    fn immutable_argument_should_be_mut(&self, nid: ast::NodeId, db: &mut DiagnosticBuilder) {
        let parent = self.tcx.hir.get_parent_node(nid);
        let parent_node = self.tcx.hir.get(parent);

        // The parent node is like a fn
        if let Some(fn_like) = FnLikeNode::from_node(parent_node) {
            // `nid`'s parent's `Body`
            let fn_body = self.tcx.hir.body(fn_like.body());
            // Get the position of `nid` in the arguments list
            let arg_pos = fn_body.arguments.iter().position(|arg| arg.pat.id == nid);
            if let Some(i) = arg_pos {
                // The argument's `Ty`
                let arg_ty = &fn_like.decl().inputs[i];
                if let Some(msg) = self.suggest_mut_for_immutable(&arg_ty) {
                    db.span_label(arg_ty.span, &msg);
                }
            }
        }
    }

    fn report_out_of_scope_escaping_closure_capture(&self,
                                                    err: &BckError<'tcx>,
                                                    capture_span: Span)
    {
        let cmt_path_or_string = self.cmt_to_path_or_string(&err.cmt);

        let suggestion =
            match self.tcx.sess.codemap().span_to_snippet(err.span) {
                Ok(string) => format!("move {}", string),
                Err(_) => format!("move |<args>| <body>")
            };

        struct_span_err!(self.tcx.sess, err.span, E0373,
                         "closure may outlive the current function, \
                          but it borrows {}, \
                          which is owned by the current function",
                         cmt_path_or_string)
            .span_label(capture_span,
                       &format!("{} is borrowed here",
                                cmt_path_or_string))
            .span_label(err.span,
                       &format!("may outlive borrowed value {}",
                                cmt_path_or_string))
            .span_suggestion(err.span,
                             &format!("to force the closure to take ownership of {} \
                                       (and any other referenced variables), \
                                       use the `move` keyword, as shown:",
                                       cmt_path_or_string),
                             suggestion)
            .emit();
    }

    fn region_end_span(&self, region: &'tcx ty::Region) -> Option<Span> {
        match *region {
            ty::ReScope(scope) => {
                match scope.span(&self.tcx.region_maps, &self.tcx.hir) {
                    Some(s) => {
                        Some(s.end_point())
                    }
                    None => {
                        None
                    }
                }
            }
            _ => None
        }
    }

    pub fn note_and_explain_bckerr(&self, db: &mut DiagnosticBuilder, err: BckError<'tcx>) {
        let error_span = err.span.clone();
        match err.code {
            err_mutbl => self.note_and_explain_mutbl_error(db, &err, &error_span),
            err_out_of_scope(super_scope, sub_scope, cause) => {
                let (value_kind, value_msg) = match err.cmt.cat {
                    mc::Categorization::Rvalue(..) =>
                        ("temporary value", "temporary value created here"),
                    _ =>
                        ("borrowed value", "borrow occurs here")
                };

                let is_closure = match cause {
                    euv::ClosureCapture(s) => {
                        // The primary span starts out as the closure creation point.
                        // Change the primary span here to highlight the use of the variable
                        // in the closure, because it seems more natural. Highlight
                        // closure creation point as a secondary span.
                        match db.span.primary_span() {
                            Some(primary) => {
                                db.span = MultiSpan::from_span(s);
                                db.span_label(primary, &format!("capture occurs here"));
                                db.span_label(s, &"does not live long enough");
                                true
                            }
                            None => false
                        }
                    }
                    _ => {
                        db.span_label(error_span, &"does not live long enough");
                        false
                    }
                };

                let sub_span = self.region_end_span(sub_scope);
                let super_span = self.region_end_span(super_scope);

                match (sub_span, super_span) {
                    (Some(s1), Some(s2)) if s1 == s2 => {
                        if !is_closure {
                            db.span = MultiSpan::from_span(s1);
                            db.span_label(error_span, &value_msg);
                            let msg = match opt_loan_path(&err.cmt) {
                                None => value_kind.to_string(),
                                Some(lp) => {
                                    format!("`{}`", self.loan_path_to_string(&lp))
                                }
                            };
                            db.span_label(s1,
                                          &format!("{} dropped here while still borrowed", msg));
                        } else {
                            db.span_label(s1, &format!("{} dropped before borrower", value_kind));
                        }
                        db.note("values in a scope are dropped in the opposite order \
                                they are created");
                    }
                    (Some(s1), Some(s2)) if !is_closure => {
                        db.span = MultiSpan::from_span(s2);
                        db.span_label(error_span, &value_msg);
                        let msg = match opt_loan_path(&err.cmt) {
                            None => value_kind.to_string(),
                            Some(lp) => {
                                format!("`{}`", self.loan_path_to_string(&lp))
                            }
                        };
                        db.span_label(s2, &format!("{} dropped here while still borrowed", msg));
                        db.span_label(s1, &format!("{} needs to live until here", value_kind));
                    }
                    _ => {
                        match sub_span {
                            Some(s) => {
                                db.span_label(s, &format!("{} needs to live until here",
                                                          value_kind));
                            }
                            None => {
                                self.tcx.note_and_explain_region(
                                    db,
                                    "borrowed value must be valid for ",
                                    sub_scope,
                                    "...");
                            }
                        }
                        match super_span {
                            Some(s) => {
                                db.span_label(s, &format!("{} only lives until here", value_kind));
                            }
                            None => {
                                self.tcx.note_and_explain_region(
                                    db,
                                    "...but borrowed value is only valid for ",
                                    super_scope,
                                    "");
                            }
                        }
                    }
                }

                if let Some(_) = statement_scope_span(self.tcx, super_scope) {
                    db.note("consider using a `let` binding to increase its lifetime");
                }



                match err.cmt.cat {
                    mc::Categorization::Rvalue(r, or) if r != or => {
                        db.note("\
before rustc 1.16, this temporary lived longer - see issue #39283 \
(https://github.com/rust-lang/rust/issues/39283)");
                    }
                    _ => {}
                }
            }

            err_borrowed_pointer_too_short(loan_scope, ptr_scope) => {
                let descr = match opt_loan_path(&err.cmt) {
                    Some(lp) => {
                        format!("`{}`", self.loan_path_to_string(&lp))
                    }
                    None => self.cmt_to_string(&err.cmt),
                };
                self.tcx.note_and_explain_region(
                    db,
                    &format!("{} would have to be valid for ",
                            descr),
                    loan_scope,
                    "...");
                self.tcx.note_and_explain_region(
                    db,
                    &format!("...but {} is only valid for ", descr),
                    ptr_scope,
                    "");
            }
        }
    }

    fn note_and_explain_mutbl_error(&self, db: &mut DiagnosticBuilder, err: &BckError<'tcx>,
                                    error_span: &Span) {
        match err.cmt.note {
            mc::NoteClosureEnv(upvar_id) | mc::NoteUpvarRef(upvar_id) => {
                // If this is an `Fn` closure, it simply can't mutate upvars.
                // If it's an `FnMut` closure, the original variable was declared immutable.
                // We need to determine which is the case here.
                let kind = match err.cmt.upvar().unwrap().cat {
                    Categorization::Upvar(mc::Upvar { kind, .. }) => kind,
                    _ => bug!()
                };
                if kind == ty::ClosureKind::Fn {
                    db.span_help(self.tcx.hir.span(upvar_id.closure_expr_id),
                                 "consider changing this closure to take \
                                 self by mutable reference");
                }
            }
            _ => {
                if let Categorization::Deref(..) = err.cmt.cat {
                    db.span_label(*error_span, &"cannot borrow as mutable");
                    if let Some(local_id) = err.cmt.get_arg_if_immutable(&self.tcx.hir) {
                        self.immutable_argument_should_be_mut(local_id, db);
                    } else if let Categorization::Deref(ref inner_cmt, ..) = err.cmt.cat {
                        if let Categorization::Local(local_id) = inner_cmt.cat {
                            self.immutable_argument_should_be_mut(local_id, db);
                        }
                    }
                } else if let Categorization::Local(local_id) = err.cmt.cat {
                    let span = self.tcx.hir.span(local_id);
                    if let Ok(snippet) = self.tcx.sess.codemap().span_to_snippet(span) {
                        if snippet.starts_with("ref mut ") || snippet.starts_with("&mut ") {
                            db.span_label(*error_span, &format!("cannot reborrow mutably"));
                            db.span_label(*error_span, &format!("try removing `&mut` here"));
                        } else {
                            if snippet.starts_with("ref ") {
                                db.span_label(span, &format!("use `{}` here to make mutable",
                                                             snippet.replace("ref ", "ref mut ")));
                            } else if snippet != "self" {
                                db.span_label(span,
                                              &format!("use `mut {}` here to make mutable",
                                                       snippet));
                            }
                            db.span_label(*error_span, &format!("cannot borrow mutably"));
                        }
                    } else {
                        db.span_label(*error_span, &format!("cannot borrow mutably"));
                    }
                }
            }
        }
    }
    pub fn append_loan_path_to_string(&self,
                                      loan_path: &LoanPath<'tcx>,
                                      out: &mut String) {
        match loan_path.kind {
            LpUpvar(ty::UpvarId{ var_id: id, closure_expr_id: _ }) |
            LpVar(id) => {
                out.push_str(&self.tcx.local_var_name_str(id));
            }

            LpDowncast(ref lp_base, variant_def_id) => {
                out.push('(');
                self.append_loan_path_to_string(&lp_base, out);
                out.push_str(DOWNCAST_PRINTED_OPERATOR);
                out.push_str(&self.tcx.item_path_str(variant_def_id));
                out.push(')');
            }

            LpExtend(ref lp_base, _, LpInterior(_, InteriorField(fname))) => {
                self.append_autoderefd_loan_path_to_string(&lp_base, out);
                match fname {
                    mc::NamedField(fname) => {
                        out.push('.');
                        out.push_str(&fname.as_str());
                    }
                    mc::PositionalField(idx) => {
                        out.push('.');
                        out.push_str(&idx.to_string());
                    }
                }
            }

            LpExtend(ref lp_base, _, LpInterior(_, InteriorElement(..))) => {
                self.append_autoderefd_loan_path_to_string(&lp_base, out);
                out.push_str("[..]");
            }

            LpExtend(ref lp_base, _, LpDeref(_)) => {
                out.push('*');
                self.append_loan_path_to_string(&lp_base, out);
            }
        }
    }

    pub fn append_autoderefd_loan_path_to_string(&self,
                                                 loan_path: &LoanPath<'tcx>,
                                                 out: &mut String) {
        match loan_path.kind {
            LpExtend(ref lp_base, _, LpDeref(_)) => {
                // For a path like `(*x).f` or `(*x)[3]`, autoderef
                // rules would normally allow users to omit the `*x`.
                // So just serialize such paths to `x.f` or x[3]` respectively.
                self.append_autoderefd_loan_path_to_string(&lp_base, out)
            }

            LpDowncast(ref lp_base, variant_def_id) => {
                out.push('(');
                self.append_autoderefd_loan_path_to_string(&lp_base, out);
                out.push(':');
                out.push_str(&self.tcx.item_path_str(variant_def_id));
                out.push(')');
            }

            LpVar(..) | LpUpvar(..) | LpExtend(.., LpInterior(..)) => {
                self.append_loan_path_to_string(loan_path, out)
            }
        }
    }

    pub fn loan_path_to_string(&self, loan_path: &LoanPath<'tcx>) -> String {
        let mut result = String::new();
        self.append_loan_path_to_string(loan_path, &mut result);
        result
    }

    pub fn cmt_to_string(&self, cmt: &mc::cmt_<'tcx>) -> String {
        cmt.descriptive_string(self.tcx)
    }

    pub fn cmt_to_path_or_string(&self, cmt: &mc::cmt<'tcx>) -> String {
        match opt_loan_path(cmt) {
            Some(lp) => format!("`{}`", self.loan_path_to_string(&lp)),
            None => self.cmt_to_string(cmt),
        }
    }
}

fn statement_scope_span(tcx: TyCtxt, region: &ty::Region) -> Option<Span> {
    match *region {
        ty::ReScope(scope) => {
            match tcx.hir.find(scope.node_id(&tcx.region_maps)) {
                Some(hir_map::NodeStmt(stmt)) => Some(stmt.span),
                _ => None
            }
        }
        _ => None
    }
}

impl BitwiseOperator for LoanDataFlowOperator {
    #[inline]
    fn join(&self, succ: usize, pred: usize) -> usize {
        succ | pred // loans from both preds are in scope
    }
}

impl DataFlowOperator for LoanDataFlowOperator {
    #[inline]
    fn initial_value(&self) -> bool {
        false // no loans in scope by default
    }
}

impl<'tcx> fmt::Debug for InteriorKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            InteriorField(mc::NamedField(fld)) => write!(f, "{}", fld),
            InteriorField(mc::PositionalField(i)) => write!(f, "#{}", i),
            InteriorElement(..) => write!(f, "[]"),
        }
    }
}

impl<'tcx> fmt::Debug for Loan<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Loan_{}({:?}, {:?}, {:?}-{:?}, {:?})",
               self.index,
               self.loan_path,
               self.kind,
               self.gen_scope,
               self.kill_scope,
               self.restricted_paths)
    }
}

impl<'tcx> fmt::Debug for LoanPath<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            LpVar(id) => {
                write!(f, "$({})", ty::tls::with(|tcx| tcx.hir.node_to_string(id)))
            }

            LpUpvar(ty::UpvarId{ var_id, closure_expr_id }) => {
                let s = ty::tls::with(|tcx| tcx.hir.node_to_string(var_id));
                write!(f, "$({} captured by id={})", s, closure_expr_id)
            }

            LpDowncast(ref lp, variant_def_id) => {
                let variant_str = if variant_def_id.is_local() {
                    ty::tls::with(|tcx| tcx.item_path_str(variant_def_id))
                } else {
                    format!("{:?}", variant_def_id)
                };
                write!(f, "({:?}{}{})", lp, DOWNCAST_PRINTED_OPERATOR, variant_str)
            }

            LpExtend(ref lp, _, LpDeref(_)) => {
                write!(f, "{:?}.*", lp)
            }

            LpExtend(ref lp, _, LpInterior(_, ref interior)) => {
                write!(f, "{:?}.{:?}", lp, interior)
            }
        }
    }
}

impl<'tcx> fmt::Display for LoanPath<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind {
            LpVar(id) => {
                write!(f, "$({})", ty::tls::with(|tcx| tcx.hir.node_to_user_string(id)))
            }

            LpUpvar(ty::UpvarId{ var_id, closure_expr_id: _ }) => {
                let s = ty::tls::with(|tcx| tcx.hir.node_to_user_string(var_id));
                write!(f, "$({} captured by closure)", s)
            }

            LpDowncast(ref lp, variant_def_id) => {
                let variant_str = if variant_def_id.is_local() {
                    ty::tls::with(|tcx| tcx.item_path_str(variant_def_id))
                } else {
                    format!("{:?}", variant_def_id)
                };
                write!(f, "({}{}{})", lp, DOWNCAST_PRINTED_OPERATOR, variant_str)
            }

            LpExtend(ref lp, _, LpDeref(_)) => {
                write!(f, "{}.*", lp)
            }

            LpExtend(ref lp, _, LpInterior(_, ref interior)) => {
                write!(f, "{}.{:?}", lp, interior)
            }
        }
    }
}
