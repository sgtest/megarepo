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

# Translation of expressions.

## User's guide

If you wish to translate an expression, there are two basic modes:

1. `trans_into(block, expr, Dest) -> block`
2. `trans_to_datum(block, expr) -> DatumBlock`

`trans_into()` is the preferred form to use whenever possible.  It
evaluates the expression and stores its result into `Dest`, which
must either be the special flag ignore (throw the result away) or
be a pointer to memory of the same type/size as the expression.

Sometimes, though, you just want to evaluate the expression into
some memory location so you can go and inspect it (e.g., a `match`
expression).  In that case, `trans_to_datum()` is your friend.  It
will evaluate the expression and return a `Datum` describing where
the result is to be found.  This function tries to return its
result in the most efficient way possible, without introducing
extra copies or sacrificing information.  Therefore, for lvalue
expressions, you always get a by-ref `Datum` in return that points
at the memory for this lvalue (almost, see [1]).  For rvalue
expressions, we will return a by-value `Datum` whenever possible,
but it is often necessary to allocate a stack slot, store the
result of the rvalue in there, and then return a pointer to the
slot (see the discussion later on about the different kinds of
rvalues).

## More specific functions

The two functions above are the most general and can handle any
situation, but there are a few other functions that are useful
in specific scenarios:

- `trans_lvalue()` is exactly like `trans_to_datum()` but it only
  works on lvalues.  This is mostly used as an assertion for those
  places where only an lvalue is expected.  It also guarantees that
  you will get a by-ref Datum back (almost, see [1]).
- `trans_local_var()` can be used to trans a ref to a local variable
  that is not an expression.

## Ownership and cleanups

The current system for cleanups associates required cleanups with
block contexts.  Block contexts are structured into a tree that
resembles the code itself.  Not every block context has cleanups
associated with it, only those blocks that have a kind of
`block_scope`.  See `common::block_kind` for more details.

If you invoke `trans_into()`, no cleanup is scheduled for you.  The
value is written into the given destination and is assumed to be owned
by that destination.

When you invoke `trans_to_datum()` on an rvalue, the resulting
datum/value will have an appropriate cleanup scheduled for the
innermost cleanup scope.  If you later use `move_to()` or
`drop_val()`, this cleanup will be canceled.

During the evaluation of an expression, temporary cleanups are created
and later canceled.  These represent intermediate or partial results
which must be cleaned up in the event of task failure.

## Implementation details

We divide expressions into three categories, based on how they are most
naturally implemented:

1. Lvalues
2. Datum rvalues
3. DPS rvalues
4. Statement rvalues

Lvalues always refer to user-assignable memory locations.
Translating those always results in a by-ref datum; this introduces
no inefficiencies into the generated code, because all lvalues are
naturally addressable.

Datum rvalues are rvalues that always generate datums as a result.
These are generally scalar results, such as `a+b` where `a` and `b`
are integers.

DPS rvalues are rvalues that, when translated, must be given a
memory location to write into (or the Ignore flag).  These are
generally expressions that produce structural results that are
larger than one word (e.g., a struct literal), but also expressions
(like `if`) that involve control flow (otherwise we'd have to
generate phi nodes).

Finally, statement rvalues are rvalues that always produce a nil
return type, such as `while` loops or assignments (`a = b`).

## Caveats

[1] Actually, some lvalues are only stored by value and not by
reference.  An example (as of this writing) would be immutable
arguments or pattern bindings of immediate type.  However, mutable
lvalues are *never* stored by value.

*/

use core::prelude::*;

use lib::llvm::ValueRef;
use middle::resolve;
use middle::trans::base::*;
use middle::trans::callee::{AutorefArg, DoAutorefArg, DontAutorefArg};
use middle::trans::callee;
use middle::trans::closure;
use middle::trans::common::*;
use middle::trans::consts;
use middle::trans::controlflow;
use middle::trans::datum::*;
use middle::trans::machine;
use middle::trans::meth;
use middle::trans::tvec;
use middle::ty::MoveValue;
use middle::ty::struct_mutable_fields;
use middle::ty::{AutoPtr, AutoBorrowVec, AutoBorrowVecRef, AutoBorrowFn};
use util::common::indenter;
use util::ppaux::ty_to_str;

use syntax::print::pprust::{expr_to_str};
use syntax::ast;
use syntax::ast::spanned;

// The primary two functions for translating expressions:
export trans_to_datum, trans_into;

// More specific variants than trans_to_datum/trans_into that are useful
// in some scenarios:
export trans_local_var;

// Other helpers, types, and so forth:
export with_field_tys;
export Dest, SaveIn, Ignore;
export cast_type_kind;
export cast_kind, cast_pointer, cast_integral, cast_float;
export cast_enum, cast_other;

// Destinations

// These are passed around by the code generating functions to track the
// destination of a computation's value.

fn macros() { include!("macros.rs"); } // FIXME(#3114): Macro import/export.

enum Dest {
    SaveIn(ValueRef),
    Ignore,
}

impl Dest {
    fn to_str(ccx: @crate_ctxt) -> ~str {
        match self {
            SaveIn(v) => fmt!("SaveIn(%s)", val_str(ccx.tn, v)),
            Ignore => ~"Ignore"
        }
    }
}

impl Dest : cmp::Eq {
    pure fn eq(&self, other: &Dest) -> bool {
        match ((*self), (*other)) {
            (SaveIn(e0a), SaveIn(e0b)) => e0a == e0b,
            (Ignore, Ignore) => true,
            (SaveIn(*), _) => false,
            (Ignore, _) => false,
        }
    }
    pure fn ne(&self, other: &Dest) -> bool { !(*self).eq(other) }
}

fn drop_and_cancel_clean(bcx: block, dat: Datum) -> block {
    let bcx = dat.drop_val(bcx);
    dat.cancel_clean(bcx);
    return bcx;
}

fn trans_to_datum(bcx: block, expr: @ast::expr) -> DatumBlock {
    debug!("trans_to_datum(expr=%s)", bcx.expr_to_str(expr));
    return match bcx.tcx().adjustments.find(expr.id) {
        None => {
            trans_to_datum_unadjusted(bcx, expr)
        }
        Some(adj) => {
            let mut bcx = bcx;
            let mut datum = unpack_datum!(bcx, {
                trans_to_datum_unadjusted(bcx, expr)
            });

            if adj.autoderefs > 0 {
                datum = datum.autoderef(bcx, expr.id, adj.autoderefs);
            }

            datum = match adj.autoref {
                None => datum,
                Some(ref autoref) => {
                    match autoref.kind {
                        AutoPtr => {
                            unpack_datum!(bcx, auto_ref(bcx, datum))
                        }
                        AutoBorrowVec => {
                            unpack_datum!(bcx, auto_slice(bcx, datum))
                        }
                        AutoBorrowVecRef => {
                            unpack_datum!(bcx, auto_slice_and_ref(bcx, datum))
                        }
                        AutoBorrowFn => {
                            // currently, all closure types are
                            // represented precisely the same, so no
                            // runtime adjustment is required:
                            datum
                        }
                    }
                }
            };

            debug!("after adjustments, datum=%s", datum.to_str(bcx.ccx()));

            return DatumBlock {bcx: bcx, datum: datum};
        }
    };

    fn auto_ref(bcx: block, datum: Datum) -> DatumBlock {
        DatumBlock {bcx: bcx, datum: datum.to_rptr(bcx)}
    }

    fn auto_slice(bcx: block, datum: Datum) -> DatumBlock {
        // This is not the most efficient thing possible; since slices
        // are two words it'd be better if this were compiled in
        // 'dest' mode, but I can't find a nice way to structure the
        // code and keep it DRY that accommodates that use case at the
        // moment.

        let tcx = bcx.tcx();
        let unit_ty = ty::sequence_element_type(tcx, datum.ty);
        let (base, len) = datum.get_base_and_len(bcx);

        // this type may have a different region/mutability than the
        // real one, but it will have the same runtime representation
        let slice_ty = ty::mk_evec(tcx,
                                   ty::mt { ty: unit_ty, mutbl: ast::m_imm },
                                   ty::vstore_slice(ty::re_static));

        let scratch = scratch_datum(bcx, slice_ty, false);
        Store(bcx, base, GEPi(bcx, scratch.val, [0u, abi::slice_elt_base]));
        Store(bcx, len, GEPi(bcx, scratch.val, [0u, abi::slice_elt_len]));
        DatumBlock {bcx: bcx, datum: scratch}
    }

    fn auto_slice_and_ref(bcx: block, datum: Datum) -> DatumBlock {
        let DatumBlock { bcx, datum } = auto_slice(bcx, datum);
        auto_ref(bcx, datum)
    }
}

fn trans_into(bcx: block, expr: @ast::expr, dest: Dest) -> block {
    return match bcx.tcx().adjustments.find(expr.id) {
        None => trans_into_unadjusted(bcx, expr, dest),
        Some(_) => {
            // use trans_to_datum, which is mildly less efficient but
            // which will perform the adjustments:
            let datumblock = trans_to_datum(bcx, expr);
            match dest {
                Ignore => datumblock.bcx,
                SaveIn(lldest) => datumblock.store_to(INIT, lldest)
            }
        }
    }
}

fn trans_lvalue(bcx: block, expr: @ast::expr) -> DatumBlock {
    return match bcx.tcx().adjustments.find(expr.id) {
        None => trans_lvalue_unadjusted(bcx, expr),
        Some(_) => {
            bcx.sess().span_bug(
                expr.span,
                fmt!("trans_lvalue() called on an expression \
                      with adjustments"));
        }
    };
}

fn trans_to_datum_unadjusted(bcx: block, expr: @ast::expr) -> DatumBlock {
    /*!
     *
     * Translates an expression into a datum.  If this expression
     * is an rvalue, this will result in a temporary value being
     * created.  If you already know where the result should be stored,
     * you should use `trans_into()` instead. */

    let mut bcx = bcx;

    debug!("trans_to_datum_unadjusted(expr=%s)", bcx.expr_to_str(expr));
    let _indenter = indenter();

    debuginfo::update_source_pos(bcx, expr.span);

    match ty::expr_kind(bcx.tcx(), bcx.ccx().maps.method_map, expr) {
        ty::LvalueExpr => {
            return trans_lvalue_unadjusted(bcx, expr);
        }

        ty::RvalueDatumExpr => {
            let datum = unpack_datum!(bcx, {
                trans_rvalue_datum_unadjusted(bcx, expr)
            });
            datum.add_clean(bcx);
            return DatumBlock {bcx: bcx, datum: datum};
        }

        ty::RvalueStmtExpr => {
            bcx = trans_rvalue_stmt_unadjusted(bcx, expr);
            return nil(bcx, expr_ty(bcx, expr));
        }

        ty::RvalueDpsExpr => {
            let ty = expr_ty(bcx, expr);
            if ty::type_is_nil(ty) || ty::type_is_bot(ty) {
                bcx = trans_rvalue_dps_unadjusted(bcx, expr, Ignore);
                return nil(bcx, ty);
            } else {
                let scratch = scratch_datum(bcx, ty, false);
                bcx = trans_rvalue_dps_unadjusted(
                    bcx, expr, SaveIn(scratch.val));

                // Note: this is not obviously a good idea.  It causes
                // immediate values to be loaded immediately after a
                // return from a call or other similar expression,
                // which in turn leads to alloca's having shorter
                // lifetimes and hence larger stack frames.  However,
                // in turn it can lead to more register pressure.
                // Still, in practice it seems to increase
                // performance, since we have fewer problems with
                // morestack churn.
                let scratch = scratch.to_appropriate_datum(bcx);

                scratch.add_clean(bcx);
                return DatumBlock {bcx: bcx, datum: scratch};
            }
        }
    }

    fn nil(bcx: block, ty: ty::t) -> DatumBlock {
        let datum = immediate_rvalue(C_nil(), ty);
        DatumBlock {bcx: bcx, datum: datum}
    }
}

fn trans_into_unadjusted(bcx: block, expr: @ast::expr, dest: Dest) -> block {
    let ty = expr_ty(bcx, expr);

    debug!("trans_into_unadjusted(expr=%s, dest=%s)",
           bcx.expr_to_str(expr),
           dest.to_str(bcx.ccx()));
    let _indenter = indenter();

    debuginfo::update_source_pos(bcx, expr.span);

    let dest = {
        if ty::type_is_nil(ty) || ty::type_is_bot(ty) {
            Ignore
        } else {
            dest
        }
    };

    let kind = bcx.expr_kind(expr);
    debug!("expr kind = %?", kind);
    match kind {
        ty::LvalueExpr => {
            let datumblock = trans_lvalue_unadjusted(bcx, expr);
            match dest {
                Ignore => datumblock.bcx,
                SaveIn(lldest) => datumblock.store_to(INIT, lldest)
            }
        }
        ty::RvalueDatumExpr => {
            let datumblock = trans_rvalue_datum_unadjusted(bcx, expr);
            match dest {
                Ignore => datumblock.drop_val(),
                SaveIn(lldest) => datumblock.store_to(INIT, lldest)
            }
        }
        ty::RvalueDpsExpr => {
            return trans_rvalue_dps_unadjusted(bcx, expr, dest);
        }
        ty::RvalueStmtExpr => {
            return trans_rvalue_stmt_unadjusted(bcx, expr);
        }
    }
}

fn trans_rvalue_datum_unadjusted(bcx: block, expr: @ast::expr) -> DatumBlock {
    let _icx = bcx.insn_ctxt("trans_rvalue_datum_unadjusted");

    trace_span!(bcx, expr.span, shorten(bcx.expr_to_str(expr)));

    match expr.node {
        ast::expr_vstore(contents, ast::expr_vstore_box) |
        ast::expr_vstore(contents, ast::expr_vstore_mut_box) => {
            return tvec::trans_uniq_or_managed_vstore(bcx, heap_shared,
                                                      expr, contents);
        }
        ast::expr_vstore(contents, ast::expr_vstore_uniq) => {
            return tvec::trans_uniq_or_managed_vstore(bcx, heap_exchange,
                                                      expr, contents);
        }
        ast::expr_lit(lit) => {
            return trans_immediate_lit(bcx, expr, *lit);
        }
        ast::expr_binary(op, lhs, rhs) => {
            // if overloaded, would be RvalueDpsExpr
            assert !bcx.ccx().maps.method_map.contains_key(expr.id);

            return trans_binary(bcx, expr, op, lhs, rhs);
        }
        ast::expr_unary(op, x) => {
            return trans_unary_datum(bcx, expr, op, x);
        }
        ast::expr_addr_of(_, x) => {
            return trans_addr_of(bcx, expr, x);
        }
        ast::expr_cast(val, _) => {
            return trans_imm_cast(bcx, val, expr.id);
        }
        ast::expr_paren(e) => {
            return trans_rvalue_datum_unadjusted(bcx, e);
        }
        _ => {
            bcx.tcx().sess.span_bug(
                expr.span,
                fmt!("trans_rvalue_datum_unadjusted reached \
                      fall-through case: %?",
                     expr.node));
        }
    }
}

fn trans_rvalue_stmt_unadjusted(bcx: block, expr: @ast::expr) -> block {
    let mut bcx = bcx;
    let _icx = bcx.insn_ctxt("trans_rvalue_stmt");

    trace_span!(bcx, expr.span, shorten(bcx.expr_to_str(expr)));

    match expr.node {
        ast::expr_break(label_opt) => {
            return controlflow::trans_break(bcx, label_opt);
        }
        ast::expr_again(label_opt) => {
            return controlflow::trans_cont(bcx, label_opt);
        }
        ast::expr_ret(ex) => {
            return controlflow::trans_ret(bcx, ex);
        }
        ast::expr_fail(why) => {
            return controlflow::trans_fail_expr(bcx, Some(expr.span), why);
        }
        ast::expr_log(_, lvl, a) => {
            return controlflow::trans_log(expr, lvl, bcx, a);
        }
        ast::expr_assert(a) => {
            return controlflow::trans_check_expr(bcx, expr, a, ~"Assertion");
        }
        ast::expr_while(cond, ref body) => {
            return controlflow::trans_while(bcx, cond, (*body));
        }
        ast::expr_loop(ref body, opt_label) => {
            return controlflow::trans_loop(bcx, (*body), opt_label);
        }
        ast::expr_assign(dst, src) => {
            let src_datum = unpack_datum!(bcx, trans_to_datum(bcx, src));
            let dst_datum = unpack_datum!(bcx, trans_lvalue(bcx, dst));
            return src_datum.store_to_datum(bcx, DROP_EXISTING, dst_datum);
        }
        ast::expr_swap(dst, src) => {
            let dst_datum = unpack_datum!(bcx, trans_lvalue(bcx, dst));
            let src_datum = unpack_datum!(bcx, trans_lvalue(bcx, src));
            let scratch = scratch_datum(bcx, dst_datum.ty, false);

            let bcx = dst_datum.move_to_datum(bcx, INIT, scratch);
            let bcx = src_datum.move_to_datum(bcx, INIT, dst_datum);
            return scratch.move_to_datum(bcx, INIT, src_datum);
        }
        ast::expr_assign_op(op, dst, src) => {
            return trans_assign_op(bcx, expr, op, dst, src);
        }
        ast::expr_paren(a) => {
            return trans_rvalue_stmt_unadjusted(bcx, a);
        }
        _ => {
            bcx.tcx().sess.span_bug(
                expr.span,
                fmt!("trans_rvalue_stmt_unadjusted reached \
                      fall-through case: %?",
                     expr.node));
        }
    };
}

fn trans_rvalue_dps_unadjusted(bcx: block, expr: @ast::expr,
                               dest: Dest) -> block {
    let mut bcx = bcx;
    let _icx = bcx.insn_ctxt("trans_rvalue_dps_unadjusted");
    let tcx = bcx.tcx();

    trace_span!(bcx, expr.span, shorten(bcx.expr_to_str(expr)));

    // XXX: This copy is really bad.
    match /*bad*/copy expr.node {
        ast::expr_paren(e) => {
            return trans_rvalue_dps_unadjusted(bcx, e, dest);
        }
        ast::expr_path(_) => {
            return trans_def_dps_unadjusted(bcx, expr,
                                            bcx.def(expr.id), dest);
        }
        ast::expr_if(cond, ref thn, els) => {
            return controlflow::trans_if(bcx, cond, (*thn), els, dest);
        }
        ast::expr_match(discr, ref arms) => {
            return _match::trans_match(bcx, expr, discr, /*bad*/copy *arms,
                                       dest);
        }
        ast::expr_block(ref blk) => {
            return do base::with_scope(bcx, (*blk).info(),
                                       ~"block-expr body") |bcx| {
                controlflow::trans_block(bcx, (*blk), dest)
            };
        }
        ast::expr_rec(ref fields, base) |
        ast::expr_struct(_, ref fields, base) => {
            return trans_rec_or_struct(bcx, (*fields), base, expr.id, dest);
        }
        ast::expr_tup(args) => {
            return trans_tup(bcx, args, dest);
        }
        ast::expr_lit(@ast::spanned {node: ast::lit_str(s), _}) => {
            return tvec::trans_lit_str(bcx, expr, s, dest);
        }
        ast::expr_vstore(contents, ast::expr_vstore_slice) |
        ast::expr_vstore(contents, ast::expr_vstore_mut_slice) => {
            return tvec::trans_slice_vstore(bcx, expr, contents, dest);
        }
        ast::expr_vstore(contents, ast::expr_vstore_fixed(_)) => {
            return tvec::trans_fixed_vstore(bcx, expr, contents, dest);
        }
        ast::expr_vec(*) | ast::expr_repeat(*) => {
            return tvec::trans_fixed_vstore(bcx, expr, expr, dest);
        }
        // XXX: Bad copy.
        ast::expr_fn(proto, copy decl, ref body, cap_clause) => {
            // Don't use this function for anything real. Use the one in
            // astconv instead.
            return closure::trans_expr_fn(bcx, proto, decl,
                                          /*bad*/copy *body,
                                          expr.id, expr.id,
                                          cap_clause, None, dest);
        }
        ast::expr_fn_block(ref decl, ref body, cap_clause) => {
            let expr_ty = expr_ty(bcx, expr);
            match ty::get(expr_ty).sty {
                ty::ty_fn(ref fn_ty) => {
                    debug!("translating fn_block %s with type %s",
                           expr_to_str(expr, tcx.sess.intr()),
                           ty_to_str(tcx, expr_ty));
                    return closure::trans_expr_fn(
                        bcx, fn_ty.meta.proto, /*bad*/copy *decl,
                        /*bad*/copy *body, expr.id, expr.id,
                        cap_clause, None, dest);
                }
                _ => {
                    bcx.sess().impossible_case(
                        expr.span, "fn_block has body with a non-fn type");
                }
            }
        }
        ast::expr_loop_body(blk) => {
            match ty::get(expr_ty(bcx, expr)).sty {
                ty::ty_fn(ref fn_ty) => {
                    match blk.node {
                        ast::expr_fn_block(copy decl, ref body, cap) => {
                            return closure::trans_expr_fn(
                                bcx,
                                fn_ty.meta.proto,
                                decl,
                                /*bad*/copy *body,
                                expr.id,
                                blk.id,
                                cap,
                                Some(None),
                                dest);
                        }
                        _ => {
                            bcx.sess().impossible_case(
                                expr.span,
                                "loop_body has the wrong kind of contents")
                        }
                    }
                }
                _ => {
                    bcx.sess().impossible_case(
                        expr.span, "loop_body has body with a non-fn type")
                }
            }
        }
        ast::expr_do_body(blk) => {
            return trans_into(bcx, blk, dest);
        }
        ast::expr_copy(a) => {
            return trans_into(bcx, a, dest);
        }
        ast::expr_unary_move(a) => {
            if bcx.expr_is_lval(a) {
                let datum = unpack_datum!(bcx, trans_to_datum(bcx, a));
                return match dest {
                    Ignore => drop_and_cancel_clean(bcx, datum),
                    SaveIn(addr) => datum.move_to(bcx, INIT, addr)
                };
            } else {
                return trans_into(bcx, a, dest);
            }
        }
        ast::expr_call(f, args, _) => {
            return callee::trans_call(
                bcx, expr, f, callee::ArgExprs(args), expr.id, dest);
        }
        ast::expr_method_call(rcvr, _, _, args, _) => {
            return callee::trans_method_call(bcx,
                                             expr,
                                             rcvr,
                                             callee::ArgExprs(args),
                                             dest);
        }
        ast::expr_binary(_, lhs, rhs) => {
            // if not overloaded, would be RvalueDatumExpr
            return trans_overloaded_op(bcx, expr, lhs, ~[rhs], dest,
                                       DoAutorefArg);
        }
        ast::expr_unary(_, subexpr) => {
            // if not overloaded, would be RvalueDatumExpr
            return trans_overloaded_op(bcx, expr, subexpr, ~[], dest,
                                       DontAutorefArg);
        }
        ast::expr_index(base, idx) => {
            // if not overloaded, would be RvalueDatumExpr
            return trans_overloaded_op(bcx, expr, base, ~[idx], dest,
                                       DontAutorefArg);
        }
        ast::expr_cast(val, _) => {
            match ty::get(node_id_type(bcx, expr.id)).sty {
                ty::ty_trait(_, _, vstore) => {
                    return meth::trans_trait_cast(bcx, val, expr.id, dest,
                                                  vstore);
                }
                _ => {
                    bcx.tcx().sess.span_bug(expr.span,
                                            ~"expr_cast of non-trait");
                }
            }
        }
        ast::expr_assign_op(op, dst, src) => {
            return trans_assign_op(bcx, expr, op, dst, src);
        }
        _ => {
            bcx.tcx().sess.span_bug(
                expr.span,
                fmt!("trans_rvalue_dps_unadjusted reached \
                      fall-through case: %?",
                     expr.node));
        }
    }
}

fn trans_def_dps_unadjusted(bcx: block, ref_expr: @ast::expr,
                            def: ast::def, dest: Dest) -> block {
    let _icx = bcx.insn_ctxt("trans_def_dps_unadjusted");
    let ccx = bcx.ccx();

    let lldest = match dest {
        SaveIn(lldest) => lldest,
        Ignore => { return bcx; }
    };

    match def {
        ast::def_fn(did, _) | ast::def_static_method(did, None, _) => {
            let fn_data = callee::trans_fn_ref(bcx, did, ref_expr.id);
            return fn_data_to_datum(bcx, did, fn_data, lldest);
        }
        ast::def_static_method(impl_did, Some(trait_did), _) => {
            let fn_data = meth::trans_static_method_callee(bcx, impl_did,
                                                           trait_did,
                                                           ref_expr.id);
            return fn_data_to_datum(bcx, impl_did, fn_data, lldest);
        }
        ast::def_variant(tid, vid) => {
            if ty::enum_variant_with_id(ccx.tcx, tid, vid).args.len() > 0u {
                // N-ary variant.
                let fn_data = callee::trans_fn_ref(bcx, vid, ref_expr.id);
                return fn_data_to_datum(bcx, vid, fn_data, lldest);
            } else {
                // Nullary variant.
                let lldiscrimptr = GEPi(bcx, lldest, [0u, 0u]);
                let lldiscrim_gv = base::lookup_discriminant(ccx, vid);
                let lldiscrim = Load(bcx, lldiscrim_gv);
                Store(bcx, lldiscrim, lldiscrimptr);
                return bcx;
            }
        }
        ast::def_struct(*) => {
            // Nothing to do here.
            // XXX: May not be true in the case of classes with destructors.
            return bcx;
        }
        _ => {
            bcx.tcx().sess.span_bug(ref_expr.span, fmt!(
                "Non-DPS def %? referened by %s",
                def, bcx.node_id_to_str(ref_expr.id)));
        }
    }
}

fn trans_lvalue_unadjusted(bcx: block, expr: @ast::expr) -> DatumBlock {
    /*!
     *
     * Translates an lvalue expression, always yielding a by-ref
     * datum.  Generally speaking you should call trans_to_datum()
     * instead, but sometimes we call trans_lvalue() directly as a
     * means of asserting that a particular expression is an lvalue. */

    let _icx = bcx.insn_ctxt("trans_lval");
    let mut bcx = bcx;

    debug!("trans_lvalue(expr=%s)", bcx.expr_to_str(expr));
    let _indenter = indenter();

    trace_span!(bcx, expr.span, shorten(bcx.expr_to_str(expr)));

    let unrooted_datum = unpack_datum!(bcx, unrooted(bcx, expr));

    // If the lvalue must remain rooted, create a scratch datum, copy
    // the lvalue in there, and then arrange for it to be cleaned up
    // at the end of the scope with id `scope_id`:
    let root_key = {id:expr.id, derefs:0u};
    for bcx.ccx().maps.root_map.find(root_key).each |scope_id| {
        unrooted_datum.root(bcx, *scope_id);
    }

    return DatumBlock {bcx: bcx, datum: unrooted_datum};

    fn unrooted(bcx: block, expr: @ast::expr) -> DatumBlock {
        let mut bcx = bcx;

        match expr.node {
            ast::expr_paren(e) => {
                return unrooted(bcx, e);
            }
            ast::expr_path(_) => {
                return trans_def_lvalue(bcx, expr, bcx.def(expr.id));
            }
            ast::expr_field(base, ident, _) => {
                return trans_rec_field(bcx, base, ident, expr.id);
            }
            ast::expr_index(base, idx) => {
                return trans_index(bcx, expr, base, idx);
            }
            ast::expr_unary(ast::deref, base) => {
                let basedatum = unpack_datum!(bcx, trans_to_datum(bcx, base));
                let derefdatum = basedatum.deref(bcx, base, 0);
                return DatumBlock {bcx: bcx, datum: derefdatum};
            }
            _ => {
                bcx.tcx().sess.span_bug(
                    expr.span,
                    fmt!("trans_lvalue reached fall-through case: %?",
                         expr.node));
            }
        }
    }
}

fn trans_def_lvalue(bcx: block,
                    ref_expr: @ast::expr,
                    def: ast::def)
                 -> DatumBlock {
    let _icx = bcx.insn_ctxt("trans_def_lvalue");
    let ccx = bcx.ccx();
    match def {
        ast::def_const(did) => {
            let const_ty = expr_ty(bcx, ref_expr);
            let val = if did.crate == ast::local_crate {
                // The LLVM global has the type of its initializer,
                // which may not be equal to the enum's type for
                // non-C-like enums.
                PointerCast(bcx, base::get_item_val(ccx, did.node),
                            T_ptr(type_of(bcx.ccx(), const_ty)))
            } else {
                base::trans_external_path(ccx, did, const_ty)
            };
            DatumBlock {
                bcx: bcx,
                datum: Datum {val: val,
                              ty: const_ty,
                              mode: ByRef,
                              source: FromLvalue}
            }
        }
        _ => {
            DatumBlock {
                bcx: bcx,
                datum: trans_local_var(bcx, def, Some(ref_expr.id))
            }
        }
    }
}

fn trans_local_var(bcx: block,
                   def: ast::def,
                   expr_id_opt: Option<ast::node_id>)
                -> Datum {
    let _icx = bcx.insn_ctxt("trans_local_var");

    return match def {
        ast::def_upvar(nid, _, _, _) => {
            // Can't move upvars, so this is never a FromLvalueLastUse.
            let local_ty = node_id_type(bcx, nid);
            match bcx.fcx.llupvars.find(nid) {
                Some(val) => {
                    Datum {
                        val: val,
                        ty: local_ty,
                        mode: ByRef,
                        source: FromLvalue
                    }
                }
                None => {
                    bcx.sess().bug(fmt!(
                        "trans_local_var: no llval for upvar %? found", nid));
                }
            }
        }
        ast::def_arg(nid, _) => {
            take_local(bcx, bcx.fcx.llargs, nid, expr_id_opt)
        }
        ast::def_local(nid, _) | ast::def_binding(nid, _) => {
            take_local(bcx, bcx.fcx.lllocals, nid, expr_id_opt)
        }
        ast::def_self(nid, _) => {
            let self_info: ValSelfData = match bcx.fcx.llself {
                Some(ref self_info) => *self_info,
                None => {
                    bcx.sess().bug(fmt!(
                        "trans_local_var: reference to self \
                         out of context with id %?", nid));
                }
            };

            // This cast should not be necessary. We should cast self *once*,
            // but right now this conflicts with default methods.
            let real_self_ty = monomorphize_type(bcx, self_info.t);
            let llselfty = T_ptr(type_of::type_of(bcx.ccx(), real_self_ty));

            let casted_val = PointerCast(bcx, self_info.v, llselfty);
            Datum {
                val: casted_val,
                ty: self_info.t,
                mode: ByRef,
                source: source_from_opt_lvalue_type(bcx.tcx(), expr_id_opt)
            }
        }
        _ => {
            bcx.sess().unimpl(fmt!(
                "unsupported def type in trans_local_var: %?", def));
        }
    };

    fn take_local(bcx: block,
                  table: HashMap<ast::node_id, local_val>,
                  nid: ast::node_id,
                  expr_id_opt: Option<ast::node_id>) -> Datum {
        let (v, mode) = match table.find(nid) {
            Some(local_mem(v)) => (v, ByRef),
            Some(local_imm(v)) => (v, ByValue),
            None => {
                bcx.sess().bug(fmt!(
                    "trans_local_var: no llval for local/arg %? found", nid));
            }
        };
        let ty = node_id_type(bcx, nid);

        debug!("take_local(nid=%?, v=%s, mode=%?, ty=%s)",
               nid, bcx.val_str(v), mode, bcx.ty_to_str(ty));

        Datum {
            val: v,
            ty: ty,
            mode: mode,
            source: source_from_opt_lvalue_type(bcx.tcx(), expr_id_opt)
        }
    }
}

fn fn_data_to_datum(bcx: block,
                    def_id: ast::def_id,
                    fn_data: callee::FnData,
                    lldest: ValueRef) -> block {
    //!
    //
    // Translates a reference to a top-level fn item into a rust
    // value.  This is generally a Rust closure pair: (fn ptr, env)
    // where the environment is NULL.  However, extern functions for
    // interfacing with C are represted as just the fn ptr with type
    // *u8.
    //
    // Strictly speaking, references to extern fns ought to be
    // RvalueDatumExprs, but it's not worth the complexity to avoid the
    // extra stack slot that LLVM probably optimizes away anyhow.

    let fn_tpt = ty::lookup_item_type(bcx.tcx(), def_id);
    if ty::ty_fn_purity(fn_tpt.ty) == ast::extern_fn {
        let val = PointerCast(bcx, fn_data.llfn, T_ptr(T_i8()));
        Store(bcx, val, lldest);
        return bcx;
    }

    let llfn = GEPi(bcx, lldest, [0u, abi::fn_field_code]);
    Store(bcx, fn_data.llfn, llfn);
    let llenv = GEPi(bcx, lldest, [0u, abi::fn_field_box]);
    Store(bcx, base::null_env_ptr(bcx), llenv);
    return bcx;
}

// The optional node ID here is the node ID of the path identifying the enum
// variant in use. If none, this cannot possibly an enum variant (so, if it
// is and `node_id_opt` is none, this function fails).
fn with_field_tys<R>(tcx: ty::ctxt,
                     ty: ty::t,
                     node_id_opt: Option<ast::node_id>,
                     op: fn(bool, (&[ty::field])) -> R) -> R {
    match ty::get(ty).sty {
        ty::ty_rec(ref fields) => {
            op(false, *fields)
        }

        ty::ty_struct(did, ref substs) => {
            let has_dtor = ty::ty_dtor(tcx, did).is_present();
            op(has_dtor, struct_mutable_fields(tcx, did, substs))
        }

        ty::ty_enum(_, ref substs) => {
            // We want the *variant* ID here, not the enum ID.
            match node_id_opt {
                None => {
                    tcx.sess.bug(fmt!(
                        "cannot get field types from the enum type %s \
                         without a node ID",
                        ty_to_str(tcx, ty)));
                }
                Some(node_id) => {
                    match tcx.def_map.get(node_id) {
                        ast::def_variant(_, variant_id) => {
                            op(false, struct_mutable_fields(
                                tcx, variant_id, substs))
                        }
                        _ => {
                            tcx.sess.bug(~"resolve didn't map this expr to a \
                                           variant ID")
                        }
                    }
                }
            }
        }

        _ => {
            tcx.sess.bug(fmt!(
                "cannot get field types from the type %s",
                ty_to_str(tcx, ty)));
        }
    }
}

fn trans_rec_field(bcx: block,
                   base: @ast::expr,
                   field: ast::ident,
                   expr_id: ast::node_id) -> DatumBlock {
    let mut bcx = bcx;
    let _icx = bcx.insn_ctxt("trans_rec_field");

    let base_datum = unpack_datum!(bcx, trans_to_datum(bcx, base));
    do with_field_tys(bcx.tcx(), base_datum.ty, None) |_dtor, field_tys| {
        let ix = ty::field_idx_strict(bcx.tcx(), field, field_tys);
        DatumBlock {
            datum: base_datum.GEPi(bcx,
                                   [0u, 0u, ix],
                                   field_tys[ix].mt.ty,
                                   source_from_opt_lvalue_type(
                                        bcx.tcx(), Some(expr_id))),
            bcx: bcx
        }
    }
}

fn source_from_opt_lvalue_type(tcx: ty::ctxt,
                               expr_id_opt: Option<ast::node_id>)
                            -> DatumSource {
    match expr_id_opt {
        None => FromLvalue,
        Some(expr_id) => {
            match tcx.value_modes.find(expr_id) {
                Some(MoveValue) => FromLastUseLvalue,
                Some(_) | None => FromLvalue,
            }
        }
    }
}

fn trans_index(bcx: block,
               index_expr: @ast::expr,
               base: @ast::expr,
               idx: @ast::expr) -> DatumBlock {
    let _icx = bcx.insn_ctxt("trans_index");
    let ccx = bcx.ccx();
    let base_ty = expr_ty(bcx, base);
    let mut bcx = bcx;

    let base_datum = unpack_datum!(bcx, trans_to_datum(bcx, base));

    // Translate index expression and cast to a suitable LLVM integer.
    // Rust is less strict than LLVM in this regard.
    let Result {bcx, val: ix_val} = trans_to_datum(bcx, idx).to_result();
    let ix_size = machine::llbitsize_of_real(bcx.ccx(), val_ty(ix_val));
    let int_size = machine::llbitsize_of_real(bcx.ccx(), ccx.int_type);
    let ix_val = {
        if ix_size < int_size {
            if ty::type_is_signed(expr_ty(bcx, idx)) {
                SExt(bcx, ix_val, ccx.int_type)
            } else { ZExt(bcx, ix_val, ccx.int_type) }
        } else if ix_size > int_size {
            Trunc(bcx, ix_val, ccx.int_type)
        } else {
            ix_val
        }
    };

    let vt = tvec::vec_types(bcx, base_datum.ty);
    base::maybe_name_value(bcx.ccx(), vt.llunit_size, ~"unit_sz");
    let scaled_ix = Mul(bcx, ix_val, vt.llunit_size);
    base::maybe_name_value(bcx.ccx(), scaled_ix, ~"scaled_ix");

    let mut (base, len) = base_datum.get_base_and_len(bcx);

    if ty::type_is_str(base_ty) {
        // acccount for null terminator in the case of string
        len = Sub(bcx, len, C_uint(bcx.ccx(), 1u));
    }

    debug!("trans_index: base %s", val_str(bcx.ccx().tn, base));
    debug!("trans_index: len %s", val_str(bcx.ccx().tn, len));

    let bounds_check = ICmp(bcx, lib::llvm::IntUGE, scaled_ix, len);
    let bcx = do with_cond(bcx, bounds_check) |bcx| {
        let unscaled_len = UDiv(bcx, len, vt.llunit_size);
        controlflow::trans_fail_bounds_check(bcx, index_expr.span,
                                             ix_val, unscaled_len)
    };
    let elt = InBoundsGEP(bcx, base, ~[ix_val]);
    let elt = PointerCast(bcx, elt, T_ptr(vt.llunit_ty));
    return DatumBlock {
        bcx: bcx,
        datum: Datum {val: elt,
                      ty: vt.unit_ty,
                      mode: ByRef,
                      source: source_from_opt_lvalue_type(
                            bcx.tcx(), Some(index_expr.id))}
    };
}

fn trans_rec_or_struct(bcx: block,
                       fields: &[ast::field],
                       base: Option<@ast::expr>,
                       id: ast::node_id,
                       dest: Dest) -> block
{
    let _icx = bcx.insn_ctxt("trans_rec");
    let mut bcx = bcx;

    // Handle the case where the result is ignored.
    let addr;
    match dest {
        SaveIn(p) => {
            addr = p;
        }
        Ignore => {
            // just evaluate the values for each field and drop them
            // on the floor
            for vec::each(fields) |fld| {
                bcx = trans_into(bcx, fld.node.expr, Ignore);
            }
            return bcx;
        }
    }

    // If this is a struct-like variant, write in the discriminant if
    // necessary, position the address at the right location, and cast the
    // address.
    let ty = node_id_type(bcx, id);
    let tcx = bcx.tcx();
    let addr = match ty::get(ty).sty {
        ty::ty_enum(_, ref substs) => {
            match tcx.def_map.get(id) {
                ast::def_variant(enum_id, variant_id) => {
                    let variant_info = ty::enum_variant_with_id(
                        tcx, enum_id, variant_id);
                    let addr = if ty::enum_is_univariant(tcx, enum_id) {
                        addr
                    } else {
                        Store(bcx,
                              C_int(bcx.ccx(), variant_info.disr_val),
                              GEPi(bcx, addr, [0, 0]));
                        GEPi(bcx, addr, [0, 1])
                    };
                    let fields = ty::struct_mutable_fields(
                        tcx, variant_id, substs);
                    let field_lltys = do fields.map |field| {
                        type_of(bcx.ccx(),
                                ty::subst_tps(
                                    tcx, substs.tps, None, field.mt.ty))
                    };
                    PointerCast(bcx, addr,
                                T_ptr(T_struct(~[T_struct(field_lltys)])))
                }
                _ => {
                    tcx.sess.bug(~"resolve didn't write the right def in for \
                                   this struct-like variant")
                }
            }
        }
        _ => addr
    };

    do with_field_tys(tcx, ty, Some(id)) |has_dtor, field_tys| {
        // evaluate each of the fields and store them into their
        // correct locations
        let mut temp_cleanups = ~[];
        for fields.each |field| {
            let ix = ty::field_idx_strict(tcx, field.node.ident, field_tys);
            let dest = GEPi(bcx, addr, struct_field(ix));
            bcx = trans_into(bcx, field.node.expr, SaveIn(dest));
            add_clean_temp_mem(bcx, dest, field_tys[ix].mt.ty);
            temp_cleanups.push(dest);
        }

        // copy over any remaining fields from the base (for
        // functional record update)
        for base.each |base_expr| {
            let base_datum = unpack_datum!(
                bcx, trans_to_datum(bcx, *base_expr));

            // Copy over inherited fields
            for field_tys.eachi |i, field_ty| {
                if !fields.any(|f| f.node.ident == field_ty.ident) {
                    let dest = GEPi(bcx, addr, struct_field(i));
                    let base_field =
                        base_datum.GEPi(bcx,
                                        struct_field(i),
                                        field_ty.mt.ty,
                                        FromLvalue);
                    bcx = base_field.store_to(bcx, INIT, dest);
                }
            }
        }

        // Add the drop flag if necessary.
        if has_dtor {
            let dest = GEPi(bcx, addr, struct_dtor());
            Store(bcx, C_u8(1), dest);
        }

        // Now revoke the cleanups as we pass responsibility for the data
        // structure on to the caller
        for temp_cleanups.each |cleanup| {
            revoke_clean(bcx, *cleanup);
        }
        bcx
    }
}

fn trans_tup(bcx: block, elts: ~[@ast::expr], dest: Dest) -> block {
    let _icx = bcx.insn_ctxt("trans_tup");
    let mut bcx = bcx;
    let addr = match dest {
        Ignore => {
            for vec::each(elts) |ex| {
                bcx = trans_into(bcx, *ex, Ignore);
            }
            return bcx;
        }
        SaveIn(pos) => pos,
    };
    let mut temp_cleanups = ~[];
    for vec::eachi(elts) |i, e| {
        let dest = GEPi(bcx, addr, [0u, i]);
        let e_ty = expr_ty(bcx, *e);
        bcx = trans_into(bcx, *e, SaveIn(dest));
        add_clean_temp_mem(bcx, dest, e_ty);
        temp_cleanups.push(dest);
    }
    for vec::each(temp_cleanups) |cleanup| {
        revoke_clean(bcx, *cleanup);
    }
    return bcx;
}

fn trans_immediate_lit(bcx: block, expr: @ast::expr,
                       lit: ast::lit) -> DatumBlock {
    // must not be a string constant, that is a RvalueDpsExpr
    let _icx = bcx.insn_ctxt("trans_immediate_lit");
    let ty = expr_ty(bcx, expr);
    immediate_rvalue_bcx(bcx, consts::const_lit(bcx.ccx(), expr, lit), ty)
}

fn trans_unary_datum(bcx: block,
                     un_expr: @ast::expr,
                     op: ast::unop,
                     sub_expr: @ast::expr) -> DatumBlock {

    let _icx = bcx.insn_ctxt("trans_unary_datum");

    // if deref, would be LvalueExpr
    assert op != ast::deref;

    // if overloaded, would be RvalueDpsExpr
    assert !bcx.ccx().maps.method_map.contains_key(un_expr.id);

    let un_ty = expr_ty(bcx, un_expr);
    let sub_ty = expr_ty(bcx, sub_expr);

    return match op {
        ast::not => {
            let Result {bcx, val} = trans_to_datum(bcx, sub_expr).to_result();
            immediate_rvalue_bcx(bcx, Not(bcx, val), un_ty)
        }
        ast::neg => {
            let Result {bcx, val} = trans_to_datum(bcx, sub_expr).to_result();
            let llneg = {
                if ty::type_is_fp(un_ty) {
                    FNeg(bcx, val)
                } else {
                    Neg(bcx, val)
                }
            };
            immediate_rvalue_bcx(bcx, llneg, un_ty)
        }
        ast::box(_) => {
            trans_boxed_expr(bcx, un_ty, sub_expr, sub_ty, heap_shared)
        }
        ast::uniq(_) => {
            trans_boxed_expr(bcx, un_ty, sub_expr, sub_ty, heap_exchange)
        }
        ast::deref => {
            bcx.sess().bug(~"deref expressions should have been \
                             translated using trans_lvalue(), not \
                             trans_unary_datum()")
        }
    };

    fn trans_boxed_expr(bcx: block,
                        box_ty: ty::t,
                        contents: @ast::expr,
                        contents_ty: ty::t,
                        heap: heap) -> DatumBlock {
        let _icx = bcx.insn_ctxt("trans_boxed_expr");
        let {bcx, box, body} =
            base::malloc_general(bcx, contents_ty, heap);
        add_clean_free(bcx, box, heap);
        let bcx = trans_into(bcx, contents, SaveIn(body));
        revoke_clean(bcx, box);
        return immediate_rvalue_bcx(bcx, box, box_ty);
    }
}

fn trans_addr_of(bcx: block, expr: @ast::expr,
                 subexpr: @ast::expr) -> DatumBlock {
    let _icx = bcx.insn_ctxt("trans_addr_of");
    let mut bcx = bcx;
    let sub_datum = unpack_datum!(bcx, trans_to_datum(bcx, subexpr));
    let llval = sub_datum.to_ref_llval(bcx);
    return immediate_rvalue_bcx(bcx, llval, expr_ty(bcx, expr));
}

// Important to get types for both lhs and rhs, because one might be _|_
// and the other not.
fn trans_eager_binop(bcx: block,
                     binop_expr: @ast::expr,
                     binop_ty: ty::t,
                     op: ast::binop,
                     lhs_datum: &Datum,
                     rhs_datum: &Datum) -> DatumBlock
{
    let mut bcx = bcx;
    let _icx = bcx.insn_ctxt("trans_eager_binop");

    let lhs = lhs_datum.to_appropriate_llval(bcx);
    let lhs_t = lhs_datum.ty;

    let rhs = rhs_datum.to_appropriate_llval(bcx);
    let rhs_t = rhs_datum.ty;

    let intype = {
        if ty::type_is_bot(lhs_t) { rhs_t }
        else { lhs_t }
    };
    let is_float = ty::type_is_fp(intype);

    let rhs = base::cast_shift_expr_rhs(bcx, op, lhs, rhs);

    let mut bcx = bcx;
    let val = match op {
      ast::add => {
        if is_float { FAdd(bcx, lhs, rhs) }
        else { Add(bcx, lhs, rhs) }
      }
      ast::subtract => {
        if is_float { FSub(bcx, lhs, rhs) }
        else { Sub(bcx, lhs, rhs) }
      }
      ast::mul => {
        if is_float { FMul(bcx, lhs, rhs) }
        else { Mul(bcx, lhs, rhs) }
      }
      ast::div => {
        if is_float {
            FDiv(bcx, lhs, rhs)
        } else {
            // Only zero-check integers; fp /0 is NaN
            bcx = base::fail_if_zero(bcx, binop_expr.span,
                                     op, rhs, rhs_t);
            if ty::type_is_signed(intype) {
                SDiv(bcx, lhs, rhs)
            } else {
                UDiv(bcx, lhs, rhs)
            }
        }
      }
      ast::rem => {
        if is_float {
            FRem(bcx, lhs, rhs)
        } else {
            // Only zero-check integers; fp %0 is NaN
            bcx = base::fail_if_zero(bcx, binop_expr.span,
                                     op, rhs, rhs_t);
            if ty::type_is_signed(intype) {
                SRem(bcx, lhs, rhs)
            } else {
                URem(bcx, lhs, rhs)
            }
        }
      }
      ast::bitor => Or(bcx, lhs, rhs),
      ast::bitand => And(bcx, lhs, rhs),
      ast::bitxor => Xor(bcx, lhs, rhs),
      ast::shl => Shl(bcx, lhs, rhs),
      ast::shr => {
        if ty::type_is_signed(intype) {
            AShr(bcx, lhs, rhs)
        } else { LShr(bcx, lhs, rhs) }
      }
      ast::eq | ast::ne | ast::lt | ast::ge | ast::le | ast::gt => {
        if ty::type_is_bot(rhs_t) {
            C_bool(false)
        } else {
            if !ty::type_is_scalar(rhs_t) {
                bcx.tcx().sess.span_bug(binop_expr.span,
                                        ~"non-scalar comparison");
            }
            let cmpr = base::compare_scalar_types(bcx, lhs, rhs, rhs_t, op);
            bcx = cmpr.bcx;
            cmpr.val
        }
      }
      _ => {
        bcx.tcx().sess.span_bug(binop_expr.span, ~"unexpected binop");
      }
    };

    return immediate_rvalue_bcx(bcx, val, binop_ty);
}

// refinement types would obviate the need for this
enum lazy_binop_ty { lazy_and, lazy_or }

fn trans_lazy_binop(bcx: block,
                    binop_expr: @ast::expr,
                    op: lazy_binop_ty,
                    a: @ast::expr,
                    b: @ast::expr) -> DatumBlock
{
    let _icx = bcx.insn_ctxt("trans_lazy_binop");
    let binop_ty = expr_ty(bcx, binop_expr);
    let mut bcx = bcx;

    let Result {bcx: past_lhs, val: lhs} = {
        do base::with_scope_result(bcx, a.info(), ~"lhs") |bcx| {
            trans_to_datum(bcx, a).to_result()
        }
    };

    if past_lhs.unreachable {
        return immediate_rvalue_bcx(past_lhs, lhs, binop_ty);
    }

    let join = base::sub_block(bcx, ~"join");
    let before_rhs = base::sub_block(bcx, ~"rhs");

    match op {
      lazy_and => CondBr(past_lhs, lhs, before_rhs.llbb, join.llbb),
      lazy_or => CondBr(past_lhs, lhs, join.llbb, before_rhs.llbb)
    }
    let Result {bcx: past_rhs, val: rhs} = {
        do base::with_scope_result(before_rhs, b.info(), ~"rhs") |bcx| {
            trans_to_datum(bcx, b).to_result()
        }
    };

    if past_rhs.unreachable {
        return immediate_rvalue_bcx(join, lhs, binop_ty);
    }

    Br(past_rhs, join.llbb);
    let phi = Phi(join, T_bool(), ~[lhs, rhs], ~[past_lhs.llbb,
                                                 past_rhs.llbb]);

    return immediate_rvalue_bcx(join, phi, binop_ty);
}

fn trans_binary(bcx: block,
                binop_expr: @ast::expr,
                op: ast::binop,
                lhs: @ast::expr,
                rhs: @ast::expr) -> DatumBlock
{
    let _icx = bcx.insn_ctxt("trans_binary");

    match op {
        ast::and => {
            trans_lazy_binop(bcx, binop_expr, lazy_and, lhs, rhs)
        }
        ast::or => {
            trans_lazy_binop(bcx, binop_expr, lazy_or, lhs, rhs)
        }
        _ => {
            let mut bcx = bcx;
            let lhs_datum = unpack_datum!(bcx, trans_to_datum(bcx, lhs));
            let rhs_datum = unpack_datum!(bcx, trans_to_datum(bcx, rhs));
            let binop_ty = expr_ty(bcx, binop_expr);
            trans_eager_binop(bcx, binop_expr, binop_ty, op,
                              &lhs_datum, &rhs_datum)
        }
    }
}

fn trans_overloaded_op(bcx: block,
                       expr: @ast::expr,
                       rcvr: @ast::expr,
                       +args: ~[@ast::expr],
                       dest: Dest,
                       +autoref_arg: AutorefArg) -> block
{
    let origin = bcx.ccx().maps.method_map.get(expr.id);
    let fty = node_id_type(bcx, expr.callee_id);
    return callee::trans_call_inner(
        bcx, expr.info(), fty,
        expr_ty(bcx, expr),
        |bcx| meth::trans_method_callee(bcx, expr.callee_id, rcvr, origin),
        callee::ArgExprs(args), dest, autoref_arg);
}

fn int_cast(bcx: block, lldsttype: TypeRef, llsrctype: TypeRef,
            llsrc: ValueRef, signed: bool) -> ValueRef {
    let _icx = bcx.insn_ctxt("int_cast");
    unsafe {
        let srcsz = llvm::LLVMGetIntTypeWidth(llsrctype);
        let dstsz = llvm::LLVMGetIntTypeWidth(lldsttype);
        return if dstsz == srcsz {
            BitCast(bcx, llsrc, lldsttype)
        } else if srcsz > dstsz {
            TruncOrBitCast(bcx, llsrc, lldsttype)
        } else if signed {
            SExtOrBitCast(bcx, llsrc, lldsttype)
        } else {
            ZExtOrBitCast(bcx, llsrc, lldsttype)
        };
    }
}

fn float_cast(bcx: block, lldsttype: TypeRef, llsrctype: TypeRef,
              llsrc: ValueRef) -> ValueRef {
    let _icx = bcx.insn_ctxt("float_cast");
    let srcsz = lib::llvm::float_width(llsrctype);
    let dstsz = lib::llvm::float_width(lldsttype);
    return if dstsz > srcsz {
        FPExt(bcx, llsrc, lldsttype)
    } else if srcsz > dstsz {
        FPTrunc(bcx, llsrc, lldsttype)
    } else { llsrc };
}

enum cast_kind {
    cast_pointer,
    cast_integral,
    cast_float,
    cast_enum,
    cast_other,
}

impl cast_kind : cmp::Eq {
    pure fn eq(&self, other: &cast_kind) -> bool {
        match ((*self), (*other)) {
            (cast_pointer, cast_pointer) => true,
            (cast_integral, cast_integral) => true,
            (cast_float, cast_float) => true,
            (cast_enum, cast_enum) => true,
            (cast_other, cast_other) => true,
            (cast_pointer, _) => false,
            (cast_integral, _) => false,
            (cast_float, _) => false,
            (cast_enum, _) => false,
            (cast_other, _) => false,
        }
    }
    pure fn ne(&self, other: &cast_kind) -> bool { !(*self).eq(other) }
}

fn cast_type_kind(t: ty::t) -> cast_kind {
    match ty::get(t).sty {
        ty::ty_float(*)   => cast_float,
        ty::ty_ptr(*)     => cast_pointer,
        ty::ty_rptr(*)    => cast_pointer,
        ty::ty_int(*)     => cast_integral,
        ty::ty_uint(*)    => cast_integral,
        ty::ty_bool       => cast_integral,
        ty::ty_enum(*)    => cast_enum,
        _                 => cast_other
    }
}

fn trans_imm_cast(bcx: block, expr: @ast::expr,
                  id: ast::node_id) -> DatumBlock {
    let _icx = bcx.insn_ctxt("trans_cast");
    let ccx = bcx.ccx();

    let t_out = node_id_type(bcx, id);

    let mut bcx = bcx;
    let llexpr = unpack_result!(bcx, trans_to_datum(bcx, expr).to_result());
    let ll_t_in = val_ty(llexpr);
    let t_in = expr_ty(bcx, expr);
    let ll_t_out = type_of::type_of(ccx, t_out);

    let k_in = cast_type_kind(t_in);
    let k_out = cast_type_kind(t_out);
    let s_in = k_in == cast_integral && ty::type_is_signed(t_in);

    let newval =
        match {in: k_in, out: k_out} {
            {in: cast_integral, out: cast_integral} => {
                int_cast(bcx, ll_t_out, ll_t_in, llexpr, s_in)
            }
            {in: cast_float, out: cast_float} => {
                float_cast(bcx, ll_t_out, ll_t_in, llexpr)
            }
            {in: cast_integral, out: cast_float} => {
                if s_in {
                    SIToFP(bcx, llexpr, ll_t_out)
                } else { UIToFP(bcx, llexpr, ll_t_out) }
            }
            {in: cast_float, out: cast_integral} => {
                if ty::type_is_signed(t_out) {
                    FPToSI(bcx, llexpr, ll_t_out)
                } else { FPToUI(bcx, llexpr, ll_t_out) }
            }
            {in: cast_integral, out: cast_pointer} => {
                IntToPtr(bcx, llexpr, ll_t_out)
            }
            {in: cast_pointer, out: cast_integral} => {
                PtrToInt(bcx, llexpr, ll_t_out)
            }
            {in: cast_pointer, out: cast_pointer} => {
                PointerCast(bcx, llexpr, ll_t_out)
            }
            {in: cast_enum, out: cast_integral} |
            {in: cast_enum, out: cast_float} => {
                let bcx = bcx;
                let llenumty = T_opaque_enum_ptr(ccx);
                let av_enum = PointerCast(bcx, llexpr, llenumty);
                let lldiscrim_a_ptr = GEPi(bcx, av_enum, [0u, 0u]);
                let lldiscrim_a = Load(bcx, lldiscrim_a_ptr);
                match k_out {
                    cast_integral => int_cast(bcx, ll_t_out,
                                              val_ty(lldiscrim_a),
                                              lldiscrim_a, true),
                    cast_float => SIToFP(bcx, lldiscrim_a, ll_t_out),
                    _ => ccx.sess.bug(~"translating unsupported cast.")
                }
            }
            _ => ccx.sess.bug(~"translating unsupported cast.")
        };
    return immediate_rvalue_bcx(bcx, newval, t_out);
}

fn trans_assign_op(bcx: block,
                   expr: @ast::expr,
                   op: ast::binop,
                   dst: @ast::expr,
                   src: @ast::expr) -> block
{
    let _icx = bcx.insn_ctxt("trans_assign_op");
    let mut bcx = bcx;

    debug!("trans_assign_op(expr=%s)", bcx.expr_to_str(expr));

    // Evaluate LHS (destination), which should be an lvalue
    let dst_datum = unpack_datum!(bcx, trans_lvalue_unadjusted(bcx, dst));

    // A user-defined operator method
    if bcx.ccx().maps.method_map.find(expr.id).is_some() {
        // FIXME(#2528) evaluates the receiver twice!!
        let scratch = scratch_datum(bcx, dst_datum.ty, false);
        let bcx = trans_overloaded_op(bcx, expr, dst, ~[src],
                                      SaveIn(scratch.val), DoAutorefArg);
        return scratch.move_to_datum(bcx, DROP_EXISTING, dst_datum);
    }

    // Evaluate RHS (source)
    let src_datum = unpack_datum!(bcx, trans_to_datum(bcx, src));

    // Perform computation and store the result
    let result_datum =
        unpack_datum!(bcx,
                      trans_eager_binop(
                          bcx, expr, dst_datum.ty, op,
                          &dst_datum, &src_datum));
    return result_datum.store_to_datum(bcx, DROP_EXISTING, dst_datum);
}

fn shorten(+x: ~str) -> ~str {
    if x.len() > 60 { x.substr(0, 60) } else { x }
}
