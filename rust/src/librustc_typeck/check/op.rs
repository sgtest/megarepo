// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Code related to processing overloaded binary and unary operators.

use super::FnCtxt;
use super::method::MethodCallee;
use rustc::ty::{self, Ty, TypeFoldable, PreferMutLvalue, TypeVariants};
use rustc::ty::TypeVariants::{TyStr, TyRef};
use rustc::ty::adjustment::{Adjustment, Adjust, AutoBorrow};
use rustc::infer::type_variable::TypeVariableOrigin;
use errors;
use syntax_pos::Span;
use syntax::symbol::Symbol;
use rustc::hir;

impl<'a, 'gcx, 'tcx> FnCtxt<'a, 'gcx, 'tcx> {
    /// Check a `a <op>= b`
    pub fn check_binop_assign(&self,
                              expr: &'gcx hir::Expr,
                              op: hir::BinOp,
                              lhs_expr: &'gcx hir::Expr,
                              rhs_expr: &'gcx hir::Expr) -> Ty<'tcx>
    {
        let lhs_ty = self.check_expr_with_lvalue_pref(lhs_expr, PreferMutLvalue);

        let lhs_ty = self.resolve_type_vars_with_obligations(lhs_ty);
        let (rhs_ty, return_ty) =
            self.check_overloaded_binop(expr, lhs_expr, lhs_ty, rhs_expr, op, IsAssign::Yes);
        let rhs_ty = self.resolve_type_vars_with_obligations(rhs_ty);

        let ty = if !lhs_ty.is_ty_var() && !rhs_ty.is_ty_var()
                    && is_builtin_binop(lhs_ty, rhs_ty, op) {
            self.enforce_builtin_binop_types(lhs_expr, lhs_ty, rhs_expr, rhs_ty, op);
            self.tcx.mk_nil()
        } else {
            return_ty
        };

        let tcx = self.tcx;
        if !tcx.expr_is_lval(lhs_expr) {
            struct_span_err!(
                tcx.sess, lhs_expr.span,
                E0067, "invalid left-hand side expression")
            .span_label(
                lhs_expr.span,
                "invalid expression for left-hand side")
            .emit();
        }
        ty
    }

    /// Check a potentially overloaded binary operator.
    pub fn check_binop(&self,
                       expr: &'gcx hir::Expr,
                       op: hir::BinOp,
                       lhs_expr: &'gcx hir::Expr,
                       rhs_expr: &'gcx hir::Expr) -> Ty<'tcx>
    {
        let tcx = self.tcx;

        debug!("check_binop(expr.id={}, expr={:?}, op={:?}, lhs_expr={:?}, rhs_expr={:?})",
               expr.id,
               expr,
               op,
               lhs_expr,
               rhs_expr);

        let lhs_ty = self.check_expr(lhs_expr);
        let lhs_ty = self.resolve_type_vars_with_obligations(lhs_ty);

        match BinOpCategory::from(op) {
            BinOpCategory::Shortcircuit => {
                // && and || are a simple case.
                let lhs_diverges = self.diverges.get();
                self.demand_suptype(lhs_expr.span, tcx.mk_bool(), lhs_ty);
                self.check_expr_coercable_to_type(rhs_expr, tcx.mk_bool());

                // Depending on the LHS' value, the RHS can never execute.
                self.diverges.set(lhs_diverges);

                tcx.mk_bool()
            }
            _ => {
                // Otherwise, we always treat operators as if they are
                // overloaded. This is the way to be most flexible w/r/t
                // types that get inferred.
                let (rhs_ty, return_ty) =
                    self.check_overloaded_binop(expr, lhs_expr, lhs_ty,
                                                rhs_expr, op, IsAssign::No);

                // Supply type inference hints if relevant. Probably these
                // hints should be enforced during select as part of the
                // `consider_unification_despite_ambiguity` routine, but this
                // more convenient for now.
                //
                // The basic idea is to help type inference by taking
                // advantage of things we know about how the impls for
                // scalar types are arranged. This is important in a
                // scenario like `1_u32 << 2`, because it lets us quickly
                // deduce that the result type should be `u32`, even
                // though we don't know yet what type 2 has and hence
                // can't pin this down to a specific impl.
                let rhs_ty = self.resolve_type_vars_with_obligations(rhs_ty);
                if
                    !lhs_ty.is_ty_var() && !rhs_ty.is_ty_var() &&
                    is_builtin_binop(lhs_ty, rhs_ty, op)
                {
                    let builtin_return_ty =
                        self.enforce_builtin_binop_types(lhs_expr, lhs_ty, rhs_expr, rhs_ty, op);
                    self.demand_suptype(expr.span, builtin_return_ty, return_ty);
                }

                return_ty
            }
        }
    }

    fn enforce_builtin_binop_types(&self,
                                   lhs_expr: &'gcx hir::Expr,
                                   lhs_ty: Ty<'tcx>,
                                   rhs_expr: &'gcx hir::Expr,
                                   rhs_ty: Ty<'tcx>,
                                   op: hir::BinOp)
                                   -> Ty<'tcx>
    {
        debug_assert!(is_builtin_binop(lhs_ty, rhs_ty, op));

        let tcx = self.tcx;
        match BinOpCategory::from(op) {
            BinOpCategory::Shortcircuit => {
                self.demand_suptype(lhs_expr.span, tcx.mk_bool(), lhs_ty);
                self.demand_suptype(rhs_expr.span, tcx.mk_bool(), rhs_ty);
                tcx.mk_bool()
            }

            BinOpCategory::Shift => {
                // result type is same as LHS always
                lhs_ty
            }

            BinOpCategory::Math |
            BinOpCategory::Bitwise => {
                // both LHS and RHS and result will have the same type
                self.demand_suptype(rhs_expr.span, lhs_ty, rhs_ty);
                lhs_ty
            }

            BinOpCategory::Comparison => {
                // both LHS and RHS and result will have the same type
                self.demand_suptype(rhs_expr.span, lhs_ty, rhs_ty);
                tcx.mk_bool()
            }
        }
    }

    fn check_overloaded_binop(&self,
                              expr: &'gcx hir::Expr,
                              lhs_expr: &'gcx hir::Expr,
                              lhs_ty: Ty<'tcx>,
                              rhs_expr: &'gcx hir::Expr,
                              op: hir::BinOp,
                              is_assign: IsAssign)
                              -> (Ty<'tcx>, Ty<'tcx>)
    {
        debug!("check_overloaded_binop(expr.id={}, lhs_ty={:?}, is_assign={:?})",
               expr.id,
               lhs_ty,
               is_assign);

        // NB: As we have not yet type-checked the RHS, we don't have the
        // type at hand. Make a variable to represent it. The whole reason
        // for this indirection is so that, below, we can check the expr
        // using this variable as the expected type, which sometimes lets
        // us do better coercions than we would be able to do otherwise,
        // particularly for things like `String + &String`.
        let rhs_ty_var = self.next_ty_var(TypeVariableOrigin::MiscVariable(rhs_expr.span));

        let result = self.lookup_op_method(lhs_ty, &[rhs_ty_var], Op::Binary(op, is_assign));

        // see `NB` above
        let rhs_ty = self.check_expr_coercable_to_type(rhs_expr, rhs_ty_var);

        let return_ty = match result {
            Ok(method) => {
                let by_ref_binop = !op.node.is_by_value();
                if is_assign == IsAssign::Yes || by_ref_binop {
                    if let ty::TyRef(region, mt) = method.sig.inputs()[0].sty {
                        let autoref = Adjustment {
                            kind: Adjust::Borrow(AutoBorrow::Ref(region, mt.mutbl)),
                            target: method.sig.inputs()[0]
                        };
                        self.apply_adjustments(lhs_expr, vec![autoref]);
                    }
                }
                if by_ref_binop {
                    if let ty::TyRef(region, mt) = method.sig.inputs()[1].sty {
                        let autoref = Adjustment {
                            kind: Adjust::Borrow(AutoBorrow::Ref(region, mt.mutbl)),
                            target: method.sig.inputs()[1]
                        };
                        // HACK(eddyb) Bypass checks due to reborrows being in
                        // some cases applied on the RHS, on top of which we need
                        // to autoref, which is not allowed by apply_adjustments.
                        // self.apply_adjustments(rhs_expr, vec![autoref]);
                        self.tables
                            .borrow_mut()
                            .adjustments_mut()
                            .entry(rhs_expr.hir_id)
                            .or_insert(vec![])
                            .push(autoref);
                    }
                }
                self.write_method_call(expr.hir_id, method);

                method.sig.output()
            }
            Err(()) => {
                // error types are considered "builtin"
                if !lhs_ty.references_error() {
                    if let IsAssign::Yes = is_assign {
                        struct_span_err!(self.tcx.sess, expr.span, E0368,
                                         "binary assignment operation `{}=` \
                                          cannot be applied to type `{}`",
                                         op.node.as_str(),
                                         lhs_ty)
                            .span_label(lhs_expr.span,
                                        format!("cannot use `{}=` on type `{}`",
                                        op.node.as_str(), lhs_ty))
                            .emit();
                    } else {
                        let mut err = struct_span_err!(self.tcx.sess, expr.span, E0369,
                            "binary operation `{}` cannot be applied to type `{}`",
                            op.node.as_str(),
                            lhs_ty);

                        if let TypeVariants::TyRef(_, ref ty_mut) = lhs_ty.sty {
                            if {
                                !self.infcx.type_moves_by_default(self.param_env,
                                                                  ty_mut.ty,
                                                                  lhs_expr.span) &&
                                    self.lookup_op_method(ty_mut.ty,
                                                          &[rhs_ty],
                                                          Op::Binary(op, is_assign))
                                        .is_ok()
                            } {
                                err.note(
                                    &format!(
                                        "this is a reference to a type that `{}` can be applied \
                                        to; you need to dereference this variable once for this \
                                        operation to work",
                                    op.node.as_str()));
                            }
                        }

                        let missing_trait = match op.node {
                            hir::BiAdd    => Some("std::ops::Add"),
                            hir::BiSub    => Some("std::ops::Sub"),
                            hir::BiMul    => Some("std::ops::Mul"),
                            hir::BiDiv    => Some("std::ops::Div"),
                            hir::BiRem    => Some("std::ops::Rem"),
                            hir::BiBitAnd => Some("std::ops::BitAnd"),
                            hir::BiBitOr  => Some("std::ops::BitOr"),
                            hir::BiShl    => Some("std::ops::Shl"),
                            hir::BiShr    => Some("std::ops::Shr"),
                            hir::BiEq | hir::BiNe => Some("std::cmp::PartialEq"),
                            hir::BiLt | hir::BiLe | hir::BiGt | hir::BiGe =>
                                Some("std::cmp::PartialOrd"),
                            _             => None
                        };

                        if let Some(missing_trait) = missing_trait {
                            if missing_trait == "std::ops::Add" &&
                                self.check_str_addition(expr, lhs_expr, lhs_ty,
                                                        rhs_ty, &mut err) {
                                // This has nothing here because it means we did string
                                // concatenation (e.g. "Hello " + "World!"). This means
                                // we don't want the note in the else clause to be emitted
                            } else {
                                err.note(
                                    &format!("an implementation of `{}` might be missing for `{}`",
                                             missing_trait, lhs_ty));
                            }
                        }
                        err.emit();
                    }
                }
                self.tcx.types.err
            }
        };

        (rhs_ty_var, return_ty)
    }

    fn check_str_addition(&self,
                          expr: &'gcx hir::Expr,
                          lhs_expr: &'gcx hir::Expr,
                          lhs_ty: Ty<'tcx>,
                          rhs_ty: Ty<'tcx>,
                          err: &mut errors::DiagnosticBuilder) -> bool {
        // If this function returns true it means a note was printed, so we don't need
        // to print the normal "implementation of `std::ops::Add` might be missing" note
        let mut is_string_addition = false;
        if let TyRef(_, l_ty) = lhs_ty.sty {
            if let TyRef(_, r_ty) = rhs_ty.sty {
                if l_ty.ty.sty == TyStr && r_ty.ty.sty == TyStr {
                    err.span_label(expr.span,
                        "`+` can't be used to concatenate two `&str` strings");
                    let codemap = self.tcx.sess.codemap();
                    let suggestion =
                        match codemap.span_to_snippet(lhs_expr.span) {
                            Ok(lstring) => format!("{}.to_owned()", lstring),
                            _ => format!("<expression>")
                        };
                    err.span_suggestion(lhs_expr.span,
                        &format!("`to_owned()` can be used to create an owned `String` \
                                  from a string reference. String concatenation \
                                  appends the string on the right to the string \
                                  on the left and may require reallocation. This \
                                  requires ownership of the string on the left"), suggestion);
                    is_string_addition = true;
                }

            }

        }

        is_string_addition
    }

    pub fn check_user_unop(&self,
                           ex: &'gcx hir::Expr,
                           operand_ty: Ty<'tcx>,
                           op: hir::UnOp)
                           -> Ty<'tcx>
    {
        assert!(op.is_by_value());
        match self.lookup_op_method(operand_ty, &[], Op::Unary(op, ex.span)) {
            Ok(method) => {
                self.write_method_call(ex.hir_id, method);
                method.sig.output()
            }
            Err(()) => {
                let actual = self.resolve_type_vars_if_possible(&operand_ty);
                if !actual.references_error() {
                    struct_span_err!(self.tcx.sess, ex.span, E0600,
                                     "cannot apply unary operator `{}` to type `{}`",
                                     op.as_str(), actual).emit();
                }
                self.tcx.types.err
            }
        }
    }

    fn lookup_op_method(&self, lhs_ty: Ty<'tcx>, other_tys: &[Ty<'tcx>], op: Op)
                        -> Result<MethodCallee<'tcx>, ()>
    {
        let lang = self.tcx.lang_items();

        let span = match op {
            Op::Binary(op, _) => op.span,
            Op::Unary(_, span) => span
        };
        let (opname, trait_did) = if let Op::Binary(op, IsAssign::Yes) = op {
            match op.node {
                hir::BiAdd => ("add_assign", lang.add_assign_trait()),
                hir::BiSub => ("sub_assign", lang.sub_assign_trait()),
                hir::BiMul => ("mul_assign", lang.mul_assign_trait()),
                hir::BiDiv => ("div_assign", lang.div_assign_trait()),
                hir::BiRem => ("rem_assign", lang.rem_assign_trait()),
                hir::BiBitXor => ("bitxor_assign", lang.bitxor_assign_trait()),
                hir::BiBitAnd => ("bitand_assign", lang.bitand_assign_trait()),
                hir::BiBitOr => ("bitor_assign", lang.bitor_assign_trait()),
                hir::BiShl => ("shl_assign", lang.shl_assign_trait()),
                hir::BiShr => ("shr_assign", lang.shr_assign_trait()),
                hir::BiLt | hir::BiLe |
                hir::BiGe | hir::BiGt |
                hir::BiEq | hir::BiNe |
                hir::BiAnd | hir::BiOr => {
                    span_bug!(span,
                              "impossible assignment operation: {}=",
                              op.node.as_str())
                }
            }
        } else if let Op::Binary(op, IsAssign::No) = op {
            match op.node {
                hir::BiAdd => ("add", lang.add_trait()),
                hir::BiSub => ("sub", lang.sub_trait()),
                hir::BiMul => ("mul", lang.mul_trait()),
                hir::BiDiv => ("div", lang.div_trait()),
                hir::BiRem => ("rem", lang.rem_trait()),
                hir::BiBitXor => ("bitxor", lang.bitxor_trait()),
                hir::BiBitAnd => ("bitand", lang.bitand_trait()),
                hir::BiBitOr => ("bitor", lang.bitor_trait()),
                hir::BiShl => ("shl", lang.shl_trait()),
                hir::BiShr => ("shr", lang.shr_trait()),
                hir::BiLt => ("lt", lang.ord_trait()),
                hir::BiLe => ("le", lang.ord_trait()),
                hir::BiGe => ("ge", lang.ord_trait()),
                hir::BiGt => ("gt", lang.ord_trait()),
                hir::BiEq => ("eq", lang.eq_trait()),
                hir::BiNe => ("ne", lang.eq_trait()),
                hir::BiAnd | hir::BiOr => {
                    span_bug!(span, "&& and || are not overloadable")
                }
            }
        } else if let Op::Unary(hir::UnNot, _) = op {
            ("not", lang.not_trait())
        } else if let Op::Unary(hir::UnNeg, _) = op {
            ("neg", lang.neg_trait())
        } else {
            bug!("lookup_op_method: op not supported: {:?}", op)
        };

        debug!("lookup_op_method(lhs_ty={:?}, op={:?}, opname={:?}, trait_did={:?})",
               lhs_ty,
               op,
               opname,
               trait_did);

        let method = trait_did.and_then(|trait_did| {
            let opname = Symbol::intern(opname);
            self.lookup_method_in_trait(span, opname, trait_did, lhs_ty, Some(other_tys))
        });

        match method {
            Some(ok) => {
                let method = self.register_infer_ok_obligations(ok);
                self.select_obligations_where_possible();

                Ok(method)
            }
            None => {
                Err(())
            }
        }
    }
}

// Binary operator categories. These categories summarize the behavior
// with respect to the builtin operationrs supported.
enum BinOpCategory {
    /// &&, || -- cannot be overridden
    Shortcircuit,

    /// <<, >> -- when shifting a single integer, rhs can be any
    /// integer type. For simd, types must match.
    Shift,

    /// +, -, etc -- takes equal types, produces same type as input,
    /// applicable to ints/floats/simd
    Math,

    /// &, |, ^ -- takes equal types, produces same type as input,
    /// applicable to ints/floats/simd/bool
    Bitwise,

    /// ==, !=, etc -- takes equal types, produces bools, except for simd,
    /// which produce the input type
    Comparison,
}

impl BinOpCategory {
    fn from(op: hir::BinOp) -> BinOpCategory {
        match op.node {
            hir::BiShl | hir::BiShr =>
                BinOpCategory::Shift,

            hir::BiAdd |
            hir::BiSub |
            hir::BiMul |
            hir::BiDiv |
            hir::BiRem =>
                BinOpCategory::Math,

            hir::BiBitXor |
            hir::BiBitAnd |
            hir::BiBitOr =>
                BinOpCategory::Bitwise,

            hir::BiEq |
            hir::BiNe |
            hir::BiLt |
            hir::BiLe |
            hir::BiGe |
            hir::BiGt =>
                BinOpCategory::Comparison,

            hir::BiAnd |
            hir::BiOr =>
                BinOpCategory::Shortcircuit,
        }
    }
}

/// Whether the binary operation is an assignment (`a += b`), or not (`a + b`)
#[derive(Clone, Copy, Debug, PartialEq)]
enum IsAssign {
    No,
    Yes,
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Binary(hir::BinOp, IsAssign),
    Unary(hir::UnOp, Span),
}

/// Returns true if this is a built-in arithmetic operation (e.g. u32
/// + u32, i16x4 == i16x4) and false if these types would have to be
/// overloaded to be legal. There are two reasons that we distinguish
/// builtin operations from overloaded ones (vs trying to drive
/// everything uniformly through the trait system and intrinsics or
/// something like that):
///
/// 1. Builtin operations can trivially be evaluated in constants.
/// 2. For comparison operators applied to SIMD types the result is
///    not of type `bool`. For example, `i16x4==i16x4` yields a
///    type like `i16x4`. This means that the overloaded trait
///    `PartialEq` is not applicable.
///
/// Reason #2 is the killer. I tried for a while to always use
/// overloaded logic and just check the types in constants/trans after
/// the fact, and it worked fine, except for SIMD types. -nmatsakis
fn is_builtin_binop(lhs: Ty, rhs: Ty, op: hir::BinOp) -> bool {
    match BinOpCategory::from(op) {
        BinOpCategory::Shortcircuit => {
            true
        }

        BinOpCategory::Shift => {
            lhs.references_error() || rhs.references_error() ||
                lhs.is_integral() && rhs.is_integral()
        }

        BinOpCategory::Math => {
            lhs.references_error() || rhs.references_error() ||
                lhs.is_integral() && rhs.is_integral() ||
                lhs.is_floating_point() && rhs.is_floating_point()
        }

        BinOpCategory::Bitwise => {
            lhs.references_error() || rhs.references_error() ||
                lhs.is_integral() && rhs.is_integral() ||
                lhs.is_floating_point() && rhs.is_floating_point() ||
                lhs.is_bool() && rhs.is_bool()
        }

        BinOpCategory::Comparison => {
            lhs.references_error() || rhs.references_error() ||
                lhs.is_scalar() && rhs.is_scalar()
        }
    }
}
