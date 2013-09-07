// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! See doc.rs for a thorough explanation of the borrow checker */


use mc = middle::mem_categorization;
use middle::ty;
use middle::typeck;
use middle::moves;
use middle::dataflow::DataFlowContext;
use middle::dataflow::DataFlowOperator;
use util::common::stmt_set;
use util::ppaux::{note_and_explain_region, Repr, UserString};

use std::hashmap::{HashSet, HashMap};
use std::io;
use std::ops::{BitOr, BitAnd};
use std::result::{Result};
use syntax::ast;
use syntax::ast_map;
use syntax::codemap::Span;
use syntax::parse::token;
use syntax::visit;
use syntax::visit::{Visitor,fn_kind};
use syntax::ast::{fn_decl,Block,NodeId};

macro_rules! if_ok(
    ($inp: expr) => (
        match $inp {
            Ok(v) => { v }
            Err(e) => { return Err(e); }
        }
    )
)

pub mod doc;

pub mod check_loans;

pub mod gather_loans;

pub mod move_data;

pub struct LoanDataFlowOperator;

/// XXX(pcwalton): Should just be #[deriving(Clone)], but that doesn't work
/// yet on unit structs.
impl Clone for LoanDataFlowOperator {
    fn clone(&self) -> LoanDataFlowOperator {
        LoanDataFlowOperator
    }
}

pub type LoanDataFlow = DataFlowContext<LoanDataFlowOperator>;

struct BorrowckVisitor;

impl Visitor<@BorrowckCtxt> for BorrowckVisitor {
    fn visit_fn(&mut self, fk:&fn_kind, fd:&fn_decl,
                b:&Block, s:Span, n:NodeId, e:@BorrowckCtxt) {
        borrowck_fn(self, fk, fd, b, s, n, e);
    }
}

pub fn check_crate(
    tcx: ty::ctxt,
    method_map: typeck::method_map,
    moves_map: moves::MovesMap,
    moved_variables_set: moves::MovedVariablesSet,
    capture_map: moves::CaptureMap,
    crate: &ast::Crate) -> (root_map, write_guard_map)
{
    let bccx = @BorrowckCtxt {
        tcx: tcx,
        method_map: method_map,
        moves_map: moves_map,
        moved_variables_set: moved_variables_set,
        capture_map: capture_map,
        root_map: root_map(),
        loan_map: @mut HashMap::new(),
        write_guard_map: @mut HashSet::new(),
        stmt_map: @mut HashSet::new(),
        stats: @mut BorrowStats {
            loaned_paths_same: 0,
            loaned_paths_imm: 0,
            stable_paths: 0,
            req_pure_paths: 0,
            guaranteed_paths: 0,
        }
    };

    let mut v = BorrowckVisitor;
    visit::walk_crate(&mut v, crate, bccx);

    if tcx.sess.borrowck_stats() {
        io::println("--- borrowck stats ---");
        printfln!("paths requiring guarantees: %u",
                  bccx.stats.guaranteed_paths);
        printfln!("paths requiring loans     : %s",
                  make_stat(bccx, bccx.stats.loaned_paths_same));
        printfln!("paths requiring imm loans : %s",
                  make_stat(bccx, bccx.stats.loaned_paths_imm));
        printfln!("stable paths              : %s",
                  make_stat(bccx, bccx.stats.stable_paths));
        printfln!("paths requiring purity    : %s",
                  make_stat(bccx, bccx.stats.req_pure_paths));
    }

    return (bccx.root_map, bccx.write_guard_map);

    fn make_stat(bccx: &BorrowckCtxt, stat: uint) -> ~str {
        let stat_f = stat as float;
        let total = bccx.stats.guaranteed_paths as float;
        fmt!("%u (%.0f%%)", stat  , stat_f * 100f / total)
    }
}

fn borrowck_fn(v: &mut BorrowckVisitor,
               fk: &visit::fn_kind,
               decl: &ast::fn_decl,
               body: &ast::Block,
               sp: Span,
               id: ast::NodeId,
               this: @BorrowckCtxt) {
    match fk {
        &visit::fk_anon(*) |
        &visit::fk_fn_block(*) => {
            // Closures are checked as part of their containing fn item.
        }

        &visit::fk_item_fn(*) |
        &visit::fk_method(*) => {
            debug!("borrowck_fn(id=%?)", id);

            // Check the body of fn items.
            let (id_range, all_loans, move_data) =
                gather_loans::gather_loans(this, decl, body);
            let mut loan_dfcx =
                DataFlowContext::new(this.tcx,
                                     this.method_map,
                                     LoanDataFlowOperator,
                                     id_range,
                                     all_loans.len());
            for (loan_idx, loan) in all_loans.iter().enumerate() {
                loan_dfcx.add_gen(loan.gen_scope, loan_idx);
                loan_dfcx.add_kill(loan.kill_scope, loan_idx);
            }
            loan_dfcx.propagate(body);

            let flowed_moves = move_data::FlowedMoveData::new(move_data,
                                                              this.tcx,
                                                              this.method_map,
                                                              id_range,
                                                              body);

            check_loans::check_loans(this, &loan_dfcx, flowed_moves,
                                     *all_loans, body);
        }
    }

    visit::walk_fn(v, fk, decl, body, sp, id, this);
}

// ----------------------------------------------------------------------
// Type definitions

pub struct BorrowckCtxt {
    tcx: ty::ctxt,
    method_map: typeck::method_map,
    moves_map: moves::MovesMap,
    moved_variables_set: moves::MovedVariablesSet,
    capture_map: moves::CaptureMap,
    root_map: root_map,
    loan_map: LoanMap,
    write_guard_map: write_guard_map,
    stmt_map: stmt_set,

    // Statistics:
    stats: @mut BorrowStats
}

pub struct BorrowStats {
    loaned_paths_same: uint,
    loaned_paths_imm: uint,
    stable_paths: uint,
    req_pure_paths: uint,
    guaranteed_paths: uint
}

pub type LoanMap = @mut HashMap<ast::NodeId, @Loan>;

// The keys to the root map combine the `id` of the deref expression
// with the number of types that it is *autodereferenced*. So, for
// example, imagine I have a variable `x: @@@T` and an expression
// `(*x).f`.  This will have 3 derefs, one explicit and then two
// autoderefs. These are the relevant `root_map_key` values that could
// appear:
//
//    {id:*x, derefs:0} --> roots `x` (type: @@@T, due to explicit deref)
//    {id:*x, derefs:1} --> roots `*x` (type: @@T, due to autoderef #1)
//    {id:*x, derefs:2} --> roots `**x` (type: @T, due to autoderef #2)
//
// Note that there is no entry with derefs:3---the type of that expression
// is T, which is not a box.
//
// Note that implicit dereferences also occur with indexing of `@[]`,
// `@str`, etc.  The same rules apply. So, for example, given a
// variable `x` of type `@[@[...]]`, if I have an instance of the
// expression `x[0]` which is then auto-slice'd, there would be two
// potential entries in the root map, both with the id of the `x[0]`
// expression. The entry with `derefs==0` refers to the deref of `x`
// used as part of evaluating `x[0]`. The entry with `derefs==1`
// refers to the deref of the `x[0]` that occurs as part of the
// auto-slice.
#[deriving(Eq, IterBytes)]
pub struct root_map_key {
    id: ast::NodeId,
    derefs: uint
}

// A set containing IDs of expressions of gc'd type that need to have a write
// guard.
pub type write_guard_map = @mut HashSet<root_map_key>;

pub type BckResult<T> = Result<T, BckError>;

#[deriving(Eq)]
pub enum PartialTotal {
    Partial,   // Loan affects some portion
    Total      // Loan affects entire path
}

///////////////////////////////////////////////////////////////////////////
// Loans and loan paths

#[deriving(Clone, Eq)]
pub enum LoanMutability {
    ImmutableMutability,
    ConstMutability,
    MutableMutability,
}

impl LoanMutability {
    pub fn from_ast_mutability(ast_mutability: ast::Mutability)
                               -> LoanMutability {
        match ast_mutability {
            ast::MutImmutable => ImmutableMutability,
            ast::MutMutable => MutableMutability,
        }
    }
}

impl ToStr for LoanMutability {
    fn to_str(&self) -> ~str {
        match *self {
            ImmutableMutability => ~"immutable",
            ConstMutability => ~"read-only",
            MutableMutability => ~"mutable",
        }
    }
}

/// Record of a loan that was issued.
pub struct Loan {
    index: uint,
    loan_path: @LoanPath,
    cmt: mc::cmt,
    mutbl: LoanMutability,
    restrictions: ~[Restriction],
    gen_scope: ast::NodeId,
    kill_scope: ast::NodeId,
    span: Span,
}

#[deriving(Eq, IterBytes)]
pub enum LoanPath {
    LpVar(ast::NodeId),               // `x` in doc.rs
    LpExtend(@LoanPath, mc::MutabilityCategory, LoanPathElem)
}

#[deriving(Eq, IterBytes)]
pub enum LoanPathElem {
    LpDeref(mc::PointerKind),    // `*LV` in doc.rs
    LpInterior(mc::InteriorKind) // `LV.f` in doc.rs
}

impl LoanPath {
    pub fn node_id(&self) -> ast::NodeId {
        match *self {
            LpVar(local_id) => local_id,
            LpExtend(base, _, _) => base.node_id()
        }
    }
}

pub fn opt_loan_path(cmt: mc::cmt) -> Option<@LoanPath> {
    //! Computes the `LoanPath` (if any) for a `cmt`.
    //! Note that this logic is somewhat duplicated in
    //! the method `compute()` found in `gather_loans::restrictions`,
    //! which allows it to share common loan path pieces as it
    //! traverses the CMT.

    match cmt.cat {
        mc::cat_rvalue(*) |
        mc::cat_static_item |
        mc::cat_copied_upvar(_) => {
            None
        }

        mc::cat_local(id) |
        mc::cat_arg(id) |
        mc::cat_self(id) => {
            Some(@LpVar(id))
        }

        mc::cat_deref(cmt_base, _, pk) => {
            do opt_loan_path(cmt_base).map_move |lp| {
                @LpExtend(lp, cmt.mutbl, LpDeref(pk))
            }
        }

        mc::cat_interior(cmt_base, ik) => {
            do opt_loan_path(cmt_base).map_move |lp| {
                @LpExtend(lp, cmt.mutbl, LpInterior(ik))
            }
        }

        mc::cat_downcast(cmt_base) |
        mc::cat_stack_upvar(cmt_base) |
        mc::cat_discr(cmt_base, _) => {
            opt_loan_path(cmt_base)
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Restrictions
//
// Borrowing an lvalue often results in *restrictions* that limit what
// can be done with this lvalue during the scope of the loan:
//
// - `RESTR_MUTATE`: The lvalue may not be modified.
// - `RESTR_CLAIM`: `&mut` borrows of the lvalue are forbidden.
// - `RESTR_FREEZE`: `&` borrows of the lvalue are forbidden.
// - `RESTR_ALIAS`: All borrows of the lvalue are forbidden.
//
// In addition, no value which is restricted may be moved. Therefore,
// restrictions are meaningful even if the RestrictionSet is empty,
// because the restriction against moves is implied.

pub struct Restriction {
    loan_path: @LoanPath,
    set: RestrictionSet
}

#[deriving(Eq)]
pub struct RestrictionSet {
    bits: u32
}

pub static RESTR_EMPTY: RestrictionSet  = RestrictionSet {bits: 0b0000};
pub static RESTR_MUTATE: RestrictionSet = RestrictionSet {bits: 0b0001};
pub static RESTR_CLAIM: RestrictionSet  = RestrictionSet {bits: 0b0010};
pub static RESTR_FREEZE: RestrictionSet = RestrictionSet {bits: 0b0100};
pub static RESTR_ALIAS: RestrictionSet  = RestrictionSet {bits: 0b1000};

impl RestrictionSet {
    pub fn intersects(&self, restr: RestrictionSet) -> bool {
        (self.bits & restr.bits) != 0
    }

    pub fn contains_all(&self, restr: RestrictionSet) -> bool {
        (self.bits & restr.bits) == restr.bits
    }
}

impl BitOr<RestrictionSet,RestrictionSet> for RestrictionSet {
    fn bitor(&self, rhs: &RestrictionSet) -> RestrictionSet {
        RestrictionSet {bits: self.bits | rhs.bits}
    }
}

impl BitAnd<RestrictionSet,RestrictionSet> for RestrictionSet {
    fn bitand(&self, rhs: &RestrictionSet) -> RestrictionSet {
        RestrictionSet {bits: self.bits & rhs.bits}
    }
}

///////////////////////////////////////////////////////////////////////////
// Rooting of managed boxes
//
// When we borrow the interior of a managed box, it is sometimes
// necessary to *root* the box, meaning to stash a copy of the box
// somewhere that the garbage collector will find it. This ensures
// that the box is not collected for the lifetime of the borrow.
//
// As part of this rooting, we sometimes also freeze the box at
// runtime, meaning that we dynamically detect when the box is
// borrowed in incompatible ways.
//
// Both of these actions are driven through the `root_map`, which maps
// from a node to the dynamic rooting action that should be taken when
// that node executes. The node is identified through a
// `root_map_key`, which pairs a node-id and a deref count---the
// problem is that sometimes the box that needs to be rooted is only
// uncovered after a certain number of auto-derefs.

pub struct RootInfo {
    scope: ast::NodeId,
    freeze: Option<DynaFreezeKind> // Some() if we should freeze box at runtime
}

pub type root_map = @mut HashMap<root_map_key, RootInfo>;

pub fn root_map() -> root_map {
    return @mut HashMap::new();
}

pub enum DynaFreezeKind {
    DynaImm,
    DynaMut
}

impl ToStr for DynaFreezeKind {
    fn to_str(&self) -> ~str {
        match *self {
            DynaMut => ~"mutable",
            DynaImm => ~"immutable"
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Errors

// Errors that can occur
#[deriving(Eq)]
pub enum bckerr_code {
    err_mutbl(LoanMutability),
    err_out_of_root_scope(ty::Region, ty::Region), // superscope, subscope
    err_out_of_scope(ty::Region, ty::Region), // superscope, subscope
    err_freeze_aliasable_const
}

// Combination of an error code and the categorization of the expression
// that caused it
#[deriving(Eq)]
pub struct BckError {
    span: Span,
    cmt: mc::cmt,
    code: bckerr_code
}

pub enum AliasableViolationKind {
    MutabilityViolation,
    BorrowViolation
}

pub enum MovedValueUseKind {
    MovedInUse,
    MovedInCapture,
}

///////////////////////////////////////////////////////////////////////////
// Misc

impl BorrowckCtxt {
    pub fn is_subregion_of(&self, r_sub: ty::Region, r_sup: ty::Region)
                           -> bool {
        self.tcx.region_maps.is_subregion_of(r_sub, r_sup)
    }

    pub fn is_subscope_of(&self, r_sub: ast::NodeId, r_sup: ast::NodeId)
                          -> bool {
        self.tcx.region_maps.is_subscope_of(r_sub, r_sup)
    }

    pub fn is_move(&self, id: ast::NodeId) -> bool {
        self.moves_map.contains(&id)
    }

    pub fn cat_expr(&self, expr: @ast::Expr) -> mc::cmt {
        mc::cat_expr(self.tcx, self.method_map, expr)
    }

    pub fn cat_expr_unadjusted(&self, expr: @ast::Expr) -> mc::cmt {
        mc::cat_expr_unadjusted(self.tcx, self.method_map, expr)
    }

    pub fn cat_expr_autoderefd(&self,
                               expr: @ast::Expr,
                               adj: @ty::AutoAdjustment)
                               -> mc::cmt {
        match *adj {
            ty::AutoAddEnv(*) => {
                // no autoderefs
                mc::cat_expr_unadjusted(self.tcx, self.method_map, expr)
            }

            ty::AutoDerefRef(
                ty::AutoDerefRef {
                    autoderefs: autoderefs, _}) => {
                mc::cat_expr_autoderefd(self.tcx, self.method_map, expr,
                                        autoderefs)
            }
        }
    }

    pub fn cat_def(&self,
                   id: ast::NodeId,
                   span: Span,
                   ty: ty::t,
                   def: ast::Def)
                   -> mc::cmt {
        mc::cat_def(self.tcx, self.method_map, id, span, ty, def)
    }

    pub fn cat_discr(&self, cmt: mc::cmt, match_id: ast::NodeId) -> mc::cmt {
        @mc::cmt_ {cat:mc::cat_discr(cmt, match_id),
                   mutbl:cmt.mutbl.inherit(),
                   ..*cmt}
    }

    pub fn mc_ctxt(&self) -> mc::mem_categorization_ctxt {
        mc::mem_categorization_ctxt {tcx: self.tcx,
                                     method_map: self.method_map}
    }

    pub fn cat_pattern(&self,
                       cmt: mc::cmt,
                       pat: @ast::Pat,
                       op: &fn(mc::cmt, @ast::Pat)) {
        let mc = self.mc_ctxt();
        mc.cat_pattern(cmt, pat, op);
    }

    pub fn report(&self, err: BckError) {
        self.span_err(
            err.span,
            self.bckerr_to_str(err));
        self.note_and_explain_bckerr(err);
    }

    pub fn report_use_of_moved_value(&self,
                                     use_span: Span,
                                     use_kind: MovedValueUseKind,
                                     lp: &LoanPath,
                                     move: &move_data::Move,
                                     moved_lp: @LoanPath) {
        let verb = match use_kind {
            MovedInUse => "use",
            MovedInCapture => "capture",
        };

        match move.kind {
            move_data::Declared => {
                self.tcx.sess.span_err(
                    use_span,
                    fmt!("%s of possibly uninitialized value: `%s`",
                         verb,
                         self.loan_path_to_str(lp)));
            }
            _ => {
                let partially = if lp == moved_lp {""} else {"partially "};
                self.tcx.sess.span_err(
                    use_span,
                    fmt!("%s of %smoved value: `%s`",
                         verb,
                         partially,
                         self.loan_path_to_str(lp)));
            }
        }

        match move.kind {
            move_data::Declared => {}

            move_data::MoveExpr(expr) => {
                let expr_ty = ty::expr_ty_adjusted(self.tcx, expr);
                let suggestion = move_suggestion(self.tcx, expr_ty,
                        "moved by default (use `copy` to override)");
                self.tcx.sess.span_note(
                    expr.span,
                    fmt!("`%s` moved here because it has type `%s`, which is %s",
                         self.loan_path_to_str(moved_lp),
                         expr_ty.user_string(self.tcx), suggestion));
            }

            move_data::MovePat(pat) => {
                let pat_ty = ty::node_id_to_type(self.tcx, pat.id);
                self.tcx.sess.span_note(
                    pat.span,
                    fmt!("`%s` moved here because it has type `%s`, \
                          which is moved by default (use `ref` to override)",
                         self.loan_path_to_str(moved_lp),
                         pat_ty.user_string(self.tcx)));
            }

            move_data::Captured(expr) => {
                let expr_ty = ty::expr_ty_adjusted(self.tcx, expr);
                let suggestion = move_suggestion(self.tcx, expr_ty,
                        "moved by default (make a copy and \
                         capture that instead to override)");
                self.tcx.sess.span_note(
                    expr.span,
                    fmt!("`%s` moved into closure environment here because it \
                          has type `%s`, which is %s",
                         self.loan_path_to_str(moved_lp),
                         expr_ty.user_string(self.tcx), suggestion));
            }
        }

        fn move_suggestion(tcx: ty::ctxt, ty: ty::t, default_msg: &'static str)
                          -> &'static str {
            match ty::get(ty).sty {
                ty::ty_closure(ref cty) if cty.sigil == ast::BorrowedSigil =>
                    "a non-copyable stack closure (capture it in a new closure, \
                     e.g. `|x| f(x)`, to override)",
                _ if ty::type_moves_by_default(tcx, ty) =>
                    "non-copyable (perhaps you meant to use clone()?)",
                _ => default_msg,
            }
        }
    }

    pub fn report_reassigned_immutable_variable(&self,
                                                span: Span,
                                                lp: &LoanPath,
                                                assign:
                                                &move_data::Assignment) {
        self.tcx.sess.span_err(
            span,
            fmt!("re-assignment of immutable variable `%s`",
                 self.loan_path_to_str(lp)));
        self.tcx.sess.span_note(
            assign.span,
            fmt!("prior assignment occurs here"));
    }

    pub fn span_err(&self, s: Span, m: &str) {
        self.tcx.sess.span_err(s, m);
    }

    pub fn span_note(&self, s: Span, m: &str) {
        self.tcx.sess.span_note(s, m);
    }

    pub fn bckerr_to_str(&self, err: BckError) -> ~str {
        match err.code {
            err_mutbl(lk) => {
                fmt!("cannot borrow %s %s as %s",
                     err.cmt.mutbl.to_user_str(),
                     self.cmt_to_str(err.cmt),
                     self.mut_to_str(lk))
            }
            err_out_of_root_scope(*) => {
                fmt!("cannot root managed value long enough")
            }
            err_out_of_scope(*) => {
                fmt!("borrowed value does not live long enough")
            }
            err_freeze_aliasable_const => {
                // Means that the user borrowed a ~T or enum value
                // residing in &const or @const pointer.  Terrible
                // error message, but then &const and @const are
                // supposed to be going away.
                fmt!("unsafe borrow of aliasable, const value")
            }
        }
    }

    pub fn report_aliasability_violation(&self,
                                         span: Span,
                                         kind: AliasableViolationKind,
                                         cause: mc::AliasableReason) {
        let prefix = match kind {
            MutabilityViolation => "cannot assign to an `&mut`",
            BorrowViolation => "cannot borrow an `&mut`"
        };

        match cause {
            mc::AliasableOther => {
                self.tcx.sess.span_err(
                    span,
                    fmt!("%s in an aliasable location", prefix));
            }
            mc::AliasableManaged(ast::MutMutable) => {
                // FIXME(#6269) reborrow @mut to &mut
                self.tcx.sess.span_err(
                    span,
                    fmt!("%s in a `@mut` pointer; \
                          try borrowing as `&mut` first", prefix));
            }
            mc::AliasableManaged(m) => {
                self.tcx.sess.span_err(
                    span,
                    fmt!("%s in a `@%s` pointer; \
                          try an `@mut` instead",
                         prefix,
                         self.mut_to_keyword(m)));
            }
            mc::AliasableBorrowed(m) => {
                self.tcx.sess.span_err(
                    span,
                    fmt!("%s in a `&%s` pointer; \
                          try an `&mut` instead",
                         prefix,
                         self.mut_to_keyword(m)));
            }
        }
    }

    pub fn note_and_explain_bckerr(&self, err: BckError) {
        let code = err.code;
        match code {
            err_mutbl(*) | err_freeze_aliasable_const(*) => {}

            err_out_of_root_scope(super_scope, sub_scope) => {
                note_and_explain_region(
                    self.tcx,
                    "managed value would have to be rooted for ",
                    sub_scope,
                    "...");
                note_and_explain_region(
                    self.tcx,
                    "...but can only be rooted for ",
                    super_scope,
                    "");
            }

            err_out_of_scope(super_scope, sub_scope) => {
                note_and_explain_region(
                    self.tcx,
                    "borrowed pointer must be valid for ",
                    sub_scope,
                    "...");
                note_and_explain_region(
                    self.tcx,
                    "...but borrowed value is only valid for ",
                    super_scope,
                    "");
          }
        }
    }

    pub fn append_loan_path_to_str_from_interior(&self,
                                                 loan_path: &LoanPath,
                                                 out: &mut ~str) {
        match *loan_path {
            LpExtend(_, _, LpDeref(_)) => {
                out.push_char('(');
                self.append_loan_path_to_str(loan_path, out);
                out.push_char(')');
            }
            LpExtend(_, _, LpInterior(_)) |
            LpVar(_) => {
                self.append_loan_path_to_str(loan_path, out);
            }
        }
    }

    pub fn append_loan_path_to_str(&self,
                                   loan_path: &LoanPath,
                                   out: &mut ~str) {
        match *loan_path {
            LpVar(id) => {
                match self.tcx.items.find(&id) {
                    Some(&ast_map::node_local(ref ident)) => {
                        out.push_str(token::ident_to_str(ident));
                    }
                    r => {
                        self.tcx.sess.bug(
                            fmt!("Loan path LpVar(%?) maps to %?, not local",
                                 id, r));
                    }
                }
            }

            LpExtend(lp_base, _, LpInterior(mc::InteriorField(fname))) => {
                self.append_loan_path_to_str_from_interior(lp_base, out);
                match fname {
                    mc::NamedField(ref fname) => {
                        out.push_char('.');
                        out.push_str(token::interner_get(*fname));
                    }
                    mc::PositionalField(idx) => {
                        out.push_char('#'); // invent a notation here
                        out.push_str(idx.to_str());
                    }
                }
            }

            LpExtend(lp_base, _, LpInterior(mc::InteriorElement(_))) => {
                self.append_loan_path_to_str_from_interior(lp_base, out);
                out.push_str("[]");
            }

            LpExtend(lp_base, _, LpDeref(_)) => {
                out.push_char('*');
                self.append_loan_path_to_str(lp_base, out);
            }
        }
    }

    pub fn loan_path_to_str(&self, loan_path: &LoanPath) -> ~str {
        let mut result = ~"";
        self.append_loan_path_to_str(loan_path, &mut result);
        result
    }

    pub fn cmt_to_str(&self, cmt: mc::cmt) -> ~str {
        let mc = &mc::mem_categorization_ctxt {tcx: self.tcx,
                                               method_map: self.method_map};
        mc.cmt_to_str(cmt)
    }

    pub fn mut_to_str(&self, mutbl: LoanMutability) -> ~str {
        mutbl.to_str()
    }

    pub fn mut_to_keyword(&self, mutbl: ast::Mutability) -> &'static str {
        match mutbl {
            ast::MutImmutable => "",
            ast::MutMutable => "mut",
        }
    }
}

impl DataFlowOperator for LoanDataFlowOperator {
    #[inline]
    fn initial_value(&self) -> bool {
        false // no loans in scope by default
    }

    #[inline]
    fn join(&self, succ: uint, pred: uint) -> uint {
        succ | pred // loans from both preds are in scope
    }

    #[inline]
    fn walk_closures(&self) -> bool {
        true
    }
}

impl Repr for Loan {
    fn repr(&self, tcx: ty::ctxt) -> ~str {
        fmt!("Loan_%?(%s, %?, %?-%?, %s)",
             self.index,
             self.loan_path.repr(tcx),
             self.mutbl,
             self.gen_scope,
             self.kill_scope,
             self.restrictions.repr(tcx))
    }
}

impl Repr for Restriction {
    fn repr(&self, tcx: ty::ctxt) -> ~str {
        fmt!("Restriction(%s, %x)",
             self.loan_path.repr(tcx),
             self.set.bits as uint)
    }
}

impl Repr for LoanPath {
    fn repr(&self, tcx: ty::ctxt) -> ~str {
        match self {
            &LpVar(id) => {
                fmt!("$(%?)", id)
            }

            &LpExtend(lp, _, LpDeref(_)) => {
                fmt!("%s.*", lp.repr(tcx))
            }

            &LpExtend(lp, _, LpInterior(ref interior)) => {
                fmt!("%s.%s", lp.repr(tcx), interior.repr(tcx))
            }
        }
    }
}
