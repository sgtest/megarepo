// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

# Moves Computation

The goal of this file is to compute which
expressions/patterns/captures correspond to *moves*.  This is
generally a function of the context in which the expression appears as
well as the expression's type.

## Examples

We will use the following fragment of code to explain the various
considerations.  Note that in this code `x` is used after it has been
moved here.  This is not relevant to this pass, though the information
we compute would later be used to detect this error (see the section
Enforcement of Moves, below).

    struct Foo { a: int, b: ~int }
    let x: Foo = ...;
    let w = (x {Read}).a;      // Read
    let y = (x {Move}).b;      // Move
    let z = copy (x {Read}).b; // Read

Let's look at these examples one by one.  In the first case, `w`, the
expression being assigned is `x.a`, which has `int` type.  In that
case, the value is read, and the container (`x`) is also read.

In the second case, `y`, `x.b` is being assigned which has type
`~int`.  Because this type moves by default, that will be a move
reference.  Whenever we move from a compound expression like `x.b` (or
`x[b]` or `*x` or `{x}[b].c`, etc), this invalidates all containing
expressions since we do not currently permit "incomplete" variables
where part of them has been moved and part has not.  In this case,
this means that the reference to `x` is also a move.  We'll see later,
though, that these kind of "partial moves", where part of the
expression has been moved, are classified and stored somewhat
differently.

The final example (`z`) is `copy x.b`: in this case, although the
expression being assigned has type `~int`, there are no moves
involved.

### Patterns

For each binding in a match or let pattern, we also compute a read
or move designation.  A move binding means that the value will be
moved from the value being matched.  As a result, the expression
being matched (aka, the 'discriminant') is either moved or read
depending on whether the bindings move the value they bind to out of
the discriminant.

For examples, consider this match expression:

    match x {Move} {
      Foo { a: a {Read}, b: b {Move} } => {...}
    }

Here, the binding `b` is value (not ref) mode, and `b` has type
`~int`, and therefore the discriminant expression `x` would be
incomplete so it also considered moved.

In the following two examples, in contrast, the mode of `b` is either
`copy` or `ref` and hence the overall result is a read:

    match x {Read} {
      Foo { a: a {Read}, b: copy b {Read} } => {...}
    }

    match x {Read} {
      Foo { a: a {Read}, b: ref b {Read} } => {...}
    }

Similar reasoning can be applied to `let` expressions:

    let Foo { a: a {Read}, b: b {Move} } = x {Move};
    let Foo { a: a {Read}, b: copy b {Read} } = x {Read};
    let Foo { a: a {Read}, b: ref b  {Read} } = x {Read};

## Output

The pass results in the struct `MoveMaps` which contains several
maps:

`moves_map` is a set containing the id of every *outermost expression* or
*binding* that causes a move.  Note that `moves_map` only contains the *outermost
expressions* that are moved.  Therefore, if you have a use of `x.b`,
as in the example `y` above, the expression `x.b` would be in the
`moves_map` but not `x`.  The reason for this is that, for most
purposes, it's only the outermost expression that is needed.  The
borrow checker and trans, for example, only care about the outermost
expressions that are moved.  It is more efficient therefore just to
store those entries.

Sometimes though we want to know the variables that are moved (in
particular in the borrow checker). For these cases, the set
`moved_variables_set` just collects the ids of variables that are
moved.

Finally, the `capture_map` maps from the node_id of a closure
expression to an array of `CaptureVar` structs detailing which
variables are captured and how (by ref, by copy, by move).

## Enforcement of Moves

The enforcement of moves is done by the borrow checker.  Please see
the section "Moves and initialization" in `middle/borrowck/doc.rs`.

## Distributive property

Copies are "distributive" over parenthesization, but blocks are
considered rvalues.  What this means is that, for example, neither
`a.clone()` nor `(a).clone()` will move `a` (presuming that `a` has a
linear type and `clone()` takes its self by reference), but
`{a}.clone()` will move `a`, as would `(if cond {a} else {b}).clone()`
and so on.

*/


use middle::pat_util::{pat_bindings};
use middle::freevars;
use middle::ty;
use middle::typeck::method_map;
use util::ppaux;
use util::ppaux::Repr;
use util::common::indenter;
use util::ppaux::UserString;

use std::cell::RefCell;
use std::hashmap::{HashSet, HashMap};
use std::rc::Rc;
use syntax::ast::*;
use syntax::ast_util;
use syntax::visit;
use syntax::visit::Visitor;
use syntax::codemap::Span;

#[deriving(Eq, Encodable, Decodable)]
pub enum CaptureMode {
    CapCopy, // Copy the value into the closure.
    CapMove, // Move the value into the closure.
    CapRef,  // Reference directly from parent stack frame (used by `||`).
}

#[deriving(Encodable, Decodable)]
pub struct CaptureVar {
    def: Def,         // Variable being accessed free
    span: Span,       // Location of an access to this variable
    mode: CaptureMode // How variable is being accessed
}

pub type CaptureMap = @RefCell<HashMap<NodeId, Rc<~[CaptureVar]>>>;

pub type MovesMap = @RefCell<HashSet<NodeId>>;

/**
 * Set of variable node-ids that are moved.
 *
 * Note: The `VariableMovesMap` stores expression ids that
 * are moves, whereas this set stores the ids of the variables
 * that are moved at some point */
pub type MovedVariablesSet = @RefCell<HashSet<NodeId>>;

/** See the section Output on the module comment for explanation. */
#[deriving(Clone)]
pub struct MoveMaps {
    moves_map: MovesMap,
    moved_variables_set: MovedVariablesSet,
    capture_map: CaptureMap
}

#[deriving(Clone)]
struct VisitContext {
    tcx: ty::ctxt,
    method_map: method_map,
    move_maps: MoveMaps
}

#[deriving(Eq)]
enum UseMode {
    Move,        // This value or something owned by it is moved.
    Read         // Read no matter what the type.
}

impl visit::Visitor<()> for VisitContext {
    fn visit_fn(&mut self, fk: &visit::FnKind, fd: &FnDecl,
                b: &Block, s: Span, n: NodeId, _: ()) {
        compute_modes_for_fn(self, fk, fd, b, s, n);
    }
    fn visit_expr(&mut self, ex: &Expr, _: ()) {
        compute_modes_for_expr(self, ex);
    }
    fn visit_local(&mut self, l: &Local, _: ()) {
        compute_modes_for_local(self, l);
    }
    // FIXME(#10894) should continue recursing
    fn visit_ty(&mut self, _t: &Ty, _: ()) {}
}

pub fn compute_moves(tcx: ty::ctxt,
                     method_map: method_map,
                     krate: &Crate) -> MoveMaps
{
    let mut visit_cx = VisitContext {
        tcx: tcx,
        method_map: method_map,
        move_maps: MoveMaps {
            moves_map: @RefCell::new(HashSet::new()),
            capture_map: @RefCell::new(HashMap::new()),
            moved_variables_set: @RefCell::new(HashSet::new())
        }
    };
    let visit_cx = &mut visit_cx;
    visit::walk_crate(visit_cx, krate, ());
    return visit_cx.move_maps;
}

pub fn moved_variable_node_id_from_def(def: Def) -> Option<NodeId> {
    match def {
        DefBinding(nid, _) |
        DefArg(nid, _) |
        DefLocal(nid, _) => Some(nid),

      _ => None
    }
}

///////////////////////////////////////////////////////////////////////////
// Expressions

fn compute_modes_for_local<'a>(cx: &mut VisitContext,
                               local: &Local) {
    cx.use_pat(local.pat);
    for &init in local.init.iter() {
        cx.use_expr(init, Read);
    }
}

fn compute_modes_for_fn(cx: &mut VisitContext,
                        fk: &visit::FnKind,
                        decl: &FnDecl,
                        body: &Block,
                        span: Span,
                        id: NodeId) {
    for a in decl.inputs.iter() {
        cx.use_pat(a.pat);
    }
    visit::walk_fn(cx, fk, decl, body, span, id, ());
}

fn compute_modes_for_expr(cx: &mut VisitContext,
                          expr: &Expr)
{
    cx.consume_expr(expr);
}

impl VisitContext {
    pub fn consume_exprs(&mut self, exprs: &[@Expr]) {
        for expr in exprs.iter() {
            self.consume_expr(*expr);
        }
    }

    pub fn consume_expr(&mut self, expr: &Expr) {
        /*!
         * Indicates that the value of `expr` will be consumed,
         * meaning either copied or moved depending on its type.
         */

        debug!("consume_expr(expr={})",
               expr.repr(self.tcx));

        let expr_ty = ty::expr_ty_adjusted(self.tcx, expr);
        if ty::type_moves_by_default(self.tcx, expr_ty) {
            {
                let mut moves_map = self.move_maps.moves_map.borrow_mut();
                moves_map.get().insert(expr.id);
            }
            self.use_expr(expr, Move);
        } else {
            self.use_expr(expr, Read);
        };
    }

    pub fn consume_block(&mut self, blk: &Block) {
        /*!
         * Indicates that the value of `blk` will be consumed,
         * meaning either copied or moved depending on its type.
         */

        debug!("consume_block(blk.id={:?})", blk.id);

        for stmt in blk.stmts.iter() {
            self.visit_stmt(*stmt, ());
        }

        for tail_expr in blk.expr.iter() {
            self.consume_expr(*tail_expr);
        }
    }

    pub fn use_expr(&mut self,
                    expr: &Expr,
                    expr_mode: UseMode) {
        /*!
         * Indicates that `expr` is used with a given mode.  This will
         * in turn trigger calls to the subcomponents of `expr`.
         */

        debug!("use_expr(expr={}, mode={:?})",
               expr.repr(self.tcx),
               expr_mode);

        // `expr_mode` refers to the post-adjustment value.  If one of
        // those adjustments is to take a reference, then it's only
        // reading the underlying expression, not moving it.
        let comp_mode = {
            let adjustments = self.tcx.adjustments.borrow();
            match adjustments.get().find(&expr.id) {
                Some(adjustment) => {
                    match **adjustment {
                        ty::AutoDerefRef(ty::AutoDerefRef {
                            autoref: Some(_),
                            ..
                        }) => Read,
                        _ => expr_mode,
                    }
                }
                _ => expr_mode,
            }
        };

        debug!("comp_mode = {:?}", comp_mode);

        match expr.node {
            ExprPath(..) => {
                match comp_mode {
                    Move => {
                        let def_map = self.tcx.def_map.borrow();
                        let def = def_map.get().get_copy(&expr.id);
                        let r = moved_variable_node_id_from_def(def);
                        for &id in r.iter() {
                            let mut moved_variables_set =
                                self.move_maps
                                    .moved_variables_set
                                    .borrow_mut();
                            moved_variables_set.get().insert(id);
                        }
                    }
                    Read => {}
                }
            }

            ExprUnary(_, UnDeref, base) => {       // *base
                if !self.use_overloaded_operator(expr, base, [])
                {
                    // Moving out of *base moves out of base.
                    self.use_expr(base, comp_mode);
                }
            }

            ExprField(base, _, _) => {        // base.f
                // Moving out of base.f moves out of base.
                self.use_expr(base, comp_mode);
            }

            ExprIndex(_, lhs, rhs) => {          // lhs[rhs]
                if !self.use_overloaded_operator(expr, lhs, [rhs])
                {
                    self.use_expr(lhs, comp_mode);
                    self.consume_expr(rhs);
                }
            }

            ExprCall(callee, ref args) => {    // callee(args)
                // Figure out whether the called function is consumed.
                let mode = match ty::get(ty::expr_ty(self.tcx, callee)).sty {
                    ty::ty_closure(ref cty) => {
                        match cty.onceness {
                        Once => Move,
                        Many => Read,
                        }
                    },
                    ty::ty_bare_fn(..) => Read,
                    ref x =>
                        self.tcx.sess.span_bug(callee.span,
                            format!("non-function type in moves for expr_call: {:?}", x)),
                };
                // Note we're not using consume_expr, which uses type_moves_by_default
                // to determine the mode, for this. The reason is that while stack
                // closures should be noncopyable, they shouldn't move by default;
                // calling a closure should only consume it if it's once.
                if mode == Move {
                    {
                        let mut moves_map = self.move_maps
                                                .moves_map
                                                .borrow_mut();
                        moves_map.get().insert(callee.id);
                    }
                }
                self.use_expr(callee, mode);
                self.use_fn_args(callee.id, *args);
            }

            ExprMethodCall(callee_id, _, _, ref args) => { // callee.m(args)
                self.use_fn_args(callee_id, *args);
            }

            ExprStruct(_, ref fields, opt_with) => {
                for field in fields.iter() {
                    self.consume_expr(field.expr);
                }

                for with_expr in opt_with.iter() {
                    // If there are any fields whose type is move-by-default,
                    // then `with` is consumed, otherwise it is only read
                    let with_ty = ty::expr_ty(self.tcx, *with_expr);
                    let with_fields = match ty::get(with_ty).sty {
                        ty::ty_struct(did, ref substs) => {
                            ty::struct_fields(self.tcx, did, substs)
                        }
                        ref r => {
                           self.tcx.sess.span_bug(
                                with_expr.span,
                                format!("bad base expr type in record: {:?}", r))
                        }
                    };

                    // The `with` expr must be consumed if it contains
                    // any fields which (1) were not explicitly
                    // specified and (2) have a type that
                    // moves-by-default:
                    let consume_with = with_fields.iter().any(|tf| {
                        !fields.iter().any(|f| f.ident.node.name == tf.ident.name) &&
                            ty::type_moves_by_default(self.tcx, tf.mt.ty)
                    });

                    fn has_dtor(tcx: ty::ctxt, ty: ty::t) -> bool {
                        use middle::ty::{get,ty_struct,ty_enum};
                        match get(ty).sty {
                            ty_struct(did, _) | ty_enum(did, _) => ty::has_dtor(tcx, did),
                            _ => false,
                        }
                    }

                    if consume_with {
                        if has_dtor(self.tcx, with_ty) {
                            self.tcx.sess.span_err(with_expr.span,
                                                   format!("cannot move out of type `{}`, \
                                                         which defines the `Drop` trait",
                                                        with_ty.user_string(self.tcx)));
                        }
                        self.consume_expr(*with_expr);
                    } else {
                        self.use_expr(*with_expr, Read);
                    }
                }
            }

            ExprTup(ref exprs) => {
                self.consume_exprs(*exprs);
            }

            ExprIf(cond_expr, then_blk, opt_else_expr) => {
                self.consume_expr(cond_expr);
                self.consume_block(then_blk);
                for else_expr in opt_else_expr.iter() {
                    self.consume_expr(*else_expr);
                }
            }

            ExprMatch(discr, ref arms) => {
                for arm in arms.iter() {
                    self.consume_arm(arm);
                }

                // The discriminant may, in fact, be partially moved
                // if there are by-move bindings, but borrowck deals
                // with that itself.
                self.use_expr(discr, Read);
            }

            ExprParen(base) => {
                // Note: base is not considered a *component* here, so
                // use `expr_mode` not `comp_mode`.
                self.use_expr(base, expr_mode);
            }

            ExprVec(ref exprs, _) => {
                self.consume_exprs(*exprs);
            }

            ExprAddrOf(_, base) => {   // &base
                self.use_expr(base, Read);
            }

            ExprLogLevel |
            ExprInlineAsm(..) |
            ExprBreak(..) |
            ExprAgain(..) |
            ExprLit(..) => {}

            ExprLoop(blk, _) => {
                self.consume_block(blk);
            }

            ExprWhile(cond_expr, blk) => {
                self.consume_expr(cond_expr);
                self.consume_block(blk);
            }

            ExprForLoop(..) => fail!("non-desugared expr_for_loop"),

            ExprUnary(_, _, lhs) => {
                if !self.use_overloaded_operator(expr, lhs, [])
                {
                    self.consume_expr(lhs);
                }
            }

            ExprBinary(_, _, lhs, rhs) => {
                if !self.use_overloaded_operator(expr, lhs, [rhs])
                {
                    self.consume_expr(lhs);
                    self.consume_expr(rhs);
                }
            }

            ExprBlock(blk) => {
                self.consume_block(blk);
            }

            ExprRet(ref opt_expr) => {
                for expr in opt_expr.iter() {
                    self.consume_expr(*expr);
                }
            }

            ExprAssign(lhs, rhs) => {
                self.use_expr(lhs, Read);
                self.consume_expr(rhs);
            }

            ExprCast(base, _) => {
                self.consume_expr(base);
            }

            ExprAssignOp(_, _, lhs, rhs) => {
                // FIXME(#4712) --- Overloaded operators?
                //
                // if !self.use_overloaded_operator(expr, DoDerefArgs, lhs, [rhs])
                // {
                self.consume_expr(lhs);
                self.consume_expr(rhs);
                // }
            }

            ExprRepeat(base, count, _) => {
                self.consume_expr(base);
                self.consume_expr(count);
            }

            ExprFnBlock(ref decl, body) |
            ExprProc(ref decl, body) => {
                for a in decl.inputs.iter() {
                    self.use_pat(a.pat);
                }
                let cap_vars = self.compute_captures(expr.id);
                {
                    let mut capture_map = self.move_maps
                                              .capture_map
                                              .borrow_mut();
                    capture_map.get().insert(expr.id, cap_vars);
                }
                self.consume_block(body);
            }

            ExprVstore(base, _) => {
                self.use_expr(base, comp_mode);
            }

            ExprBox(place, base) => {
                self.use_expr(place, comp_mode);
                self.use_expr(base, comp_mode);
            }

            ExprMac(..) => {
                self.tcx.sess.span_bug(
                    expr.span,
                    "macro expression remains after expansion");
            }
        }
    }

    pub fn use_overloaded_operator(&mut self,
                                   expr: &Expr,
                                   receiver_expr: @Expr,
                                   arg_exprs: &[@Expr])
                                   -> bool {
        let method_map = self.method_map.borrow();
        if !method_map.get().contains_key(&expr.id) {
            return false;
        }

        self.use_fn_arg(receiver_expr);

        // for overloaded operatrs, we are always passing in a
        // reference, so it's always read mode:
        for arg_expr in arg_exprs.iter() {
            self.use_expr(*arg_expr, Read);
        }

        return true;
    }

    pub fn consume_arm(&mut self, arm: &Arm) {
        for pat in arm.pats.iter() {
            self.use_pat(*pat);
        }

        for guard in arm.guard.iter() {
            self.consume_expr(*guard);
        }

        self.consume_block(arm.body);
    }

    pub fn use_pat(&mut self, pat: @Pat) {
        /*!
         *
         * Decides whether each binding in a pattern moves the value
         * into itself or not based on its type and annotation.
         */

        pat_bindings(self.tcx.def_map, pat, |bm, id, _span, path| {
            let binding_moves = match bm {
                BindByRef(_) => false,
                BindByValue(_) => {
                    let pat_ty = ty::node_id_to_type(self.tcx, id);
                    debug!("pattern {:?} {} type is {}",
                           id,
                           ast_util::path_to_ident(path).repr(self.tcx),
                           pat_ty.repr(self.tcx));
                    ty::type_moves_by_default(self.tcx, pat_ty)
                }
            };

            debug!("pattern binding {:?}: bm={:?}, binding_moves={}",
                   id, bm, binding_moves);

            if binding_moves {
                {
                    let mut moves_map = self.move_maps.moves_map.borrow_mut();
                    moves_map.get().insert(id);
                }
            }
        })
    }

    pub fn use_fn_args(&mut self,
                       _: NodeId,
                       arg_exprs: &[@Expr]) {
        //! Uses the argument expressions.
        for arg_expr in arg_exprs.iter() {
            self.use_fn_arg(*arg_expr);
        }
    }

    pub fn use_fn_arg(&mut self, arg_expr: @Expr) {
        //! Uses the argument.
        self.consume_expr(arg_expr)
    }

    pub fn compute_captures(&mut self, fn_expr_id: NodeId) -> Rc<~[CaptureVar]> {
        debug!("compute_capture_vars(fn_expr_id={:?})", fn_expr_id);
        let _indenter = indenter();

        let fn_ty = ty::node_id_to_type(self.tcx, fn_expr_id);
        let sigil = ty::ty_closure_sigil(fn_ty);
        let freevars = freevars::get_freevars(self.tcx, fn_expr_id);
        let v = if sigil == BorrowedSigil {
            // || captures everything by ref
            freevars.iter()
                    .map(|fvar| CaptureVar {def: fvar.def, span: fvar.span, mode: CapRef})
                    .collect()
        } else {
            // @fn() and ~fn() capture by copy or by move depending on type
            freevars.iter()
                    .map(|fvar| {
                let fvar_def_id = ast_util::def_id_of_def(fvar.def).node;
                let fvar_ty = ty::node_id_to_type(self.tcx, fvar_def_id);
                debug!("fvar_def_id={:?} fvar_ty={}",
                       fvar_def_id, ppaux::ty_to_str(self.tcx, fvar_ty));
                let mode = if ty::type_moves_by_default(self.tcx, fvar_ty) {
                    CapMove
                } else {
                    CapCopy
                };
                CaptureVar {def: fvar.def, span: fvar.span, mode:mode}

                }).collect()
        };
        Rc::new(v)
    }
}
