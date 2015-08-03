// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_camel_case_types)]

use self::ConstVal::*;
use self::ErrKind::*;
use self::EvalHint::*;

use ast_map;
use ast_map::blocks::FnLikeNode;
use metadata::csearch;
use middle::{astencode, def, infer, subst, traits};
use middle::pat_util::def_to_path;
use middle::ty::{self, Ty};
use middle::astconv_util::ast_ty_to_prim_ty;
use util::num::ToPrimitive;

use syntax::ast::{self, Expr};
use syntax::ast_util;
use syntax::codemap::Span;
use syntax::parse::token::InternedString;
use syntax::ptr::P;
use syntax::{codemap, visit};

use std::borrow::{Cow, IntoCow};
use std::num::wrapping::OverflowingOps;
use std::cmp::Ordering;
use std::collections::hash_map::Entry::Vacant;
use std::{i8, i16, i32, i64, u8, u16, u32, u64};
use std::rc::Rc;

fn lookup_const<'a>(tcx: &'a ty::ctxt, e: &Expr) -> Option<&'a Expr> {
    let opt_def = tcx.def_map.borrow().get(&e.id).map(|d| d.full_def());
    match opt_def {
        Some(def::DefConst(def_id)) |
        Some(def::DefAssociatedConst(def_id)) => {
            lookup_const_by_id(tcx, def_id, Some(e.id))
        }
        Some(def::DefVariant(enum_def, variant_def, _)) => {
            lookup_variant_by_id(tcx, enum_def, variant_def)
        }
        _ => None
    }
}

fn lookup_variant_by_id<'a>(tcx: &'a ty::ctxt,
                            enum_def: ast::DefId,
                            variant_def: ast::DefId)
                            -> Option<&'a Expr> {
    fn variant_expr<'a>(variants: &'a [P<ast::Variant>], id: ast::NodeId)
                        -> Option<&'a Expr> {
        for variant in variants {
            if variant.node.id == id {
                return variant.node.disr_expr.as_ref().map(|e| &**e);
            }
        }
        None
    }

    if ast_util::is_local(enum_def) {
        match tcx.map.find(enum_def.node) {
            None => None,
            Some(ast_map::NodeItem(it)) => match it.node {
                ast::ItemEnum(ast::EnumDef { ref variants }, _) => {
                    variant_expr(&variants[..], variant_def.node)
                }
                _ => None
            },
            Some(_) => None
        }
    } else {
        match tcx.extern_const_variants.borrow().get(&variant_def) {
            Some(&ast::DUMMY_NODE_ID) => return None,
            Some(&expr_id) => {
                return Some(tcx.map.expect_expr(expr_id));
            }
            None => {}
        }
        let expr_id = match csearch::maybe_get_item_ast(tcx, enum_def,
            Box::new(|a, b, c, d| astencode::decode_inlined_item(a, b, c, d))) {
            csearch::FoundAst::Found(&ast::IIItem(ref item)) => match item.node {
                ast::ItemEnum(ast::EnumDef { ref variants }, _) => {
                    // NOTE this doesn't do the right thing, it compares inlined
                    // NodeId's to the original variant_def's NodeId, but they
                    // come from different crates, so they will likely never match.
                    variant_expr(&variants[..], variant_def.node).map(|e| e.id)
                }
                _ => None
            },
            _ => None
        };
        tcx.extern_const_variants.borrow_mut().insert(variant_def,
                                                      expr_id.unwrap_or(ast::DUMMY_NODE_ID));
        expr_id.map(|id| tcx.map.expect_expr(id))
    }
}

pub fn lookup_const_by_id<'a, 'tcx: 'a>(tcx: &'a ty::ctxt<'tcx>,
                                        def_id: ast::DefId,
                                        maybe_ref_id: Option<ast::NodeId>)
                                        -> Option<&'tcx Expr> {
    if ast_util::is_local(def_id) {
        match tcx.map.find(def_id.node) {
            None => None,
            Some(ast_map::NodeItem(it)) => match it.node {
                ast::ItemConst(_, ref const_expr) => {
                    Some(&*const_expr)
                }
                _ => None
            },
            Some(ast_map::NodeTraitItem(ti)) => match ti.node {
                ast::ConstTraitItem(_, _) => {
                    match maybe_ref_id {
                        // If we have a trait item, and we know the expression
                        // that's the source of the obligation to resolve it,
                        // `resolve_trait_associated_const` will select an impl
                        // or the default.
                        Some(ref_id) => {
                            let trait_id = tcx.trait_of_item(def_id)
                                              .unwrap();
                            let substs = tcx.node_id_item_substs(ref_id)
                                            .substs;
                            resolve_trait_associated_const(tcx, ti, trait_id,
                                                           substs)
                        }
                        // Technically, without knowing anything about the
                        // expression that generates the obligation, we could
                        // still return the default if there is one. However,
                        // it's safer to return `None` than to return some value
                        // that may differ from what you would get from
                        // correctly selecting an impl.
                        None => None
                    }
                }
                _ => None
            },
            Some(ast_map::NodeImplItem(ii)) => match ii.node {
                ast::ConstImplItem(_, ref expr) => {
                    Some(&*expr)
                }
                _ => None
            },
            Some(_) => None
        }
    } else {
        match tcx.extern_const_statics.borrow().get(&def_id) {
            Some(&ast::DUMMY_NODE_ID) => return None,
            Some(&expr_id) => {
                return Some(tcx.map.expect_expr(expr_id));
            }
            None => {}
        }
        let mut used_ref_id = false;
        let expr_id = match csearch::maybe_get_item_ast(tcx, def_id,
            Box::new(|a, b, c, d| astencode::decode_inlined_item(a, b, c, d))) {
            csearch::FoundAst::Found(&ast::IIItem(ref item)) => match item.node {
                ast::ItemConst(_, ref const_expr) => Some(const_expr.id),
                _ => None
            },
            csearch::FoundAst::Found(&ast::IITraitItem(trait_id, ref ti)) => match ti.node {
                ast::ConstTraitItem(_, _) => {
                    used_ref_id = true;
                    match maybe_ref_id {
                        // As mentioned in the comments above for in-crate
                        // constants, we only try to find the expression for
                        // a trait-associated const if the caller gives us
                        // the expression that refers to it.
                        Some(ref_id) => {
                            let substs = tcx.node_id_item_substs(ref_id)
                                            .substs;
                            resolve_trait_associated_const(tcx, ti, trait_id,
                                                           substs).map(|e| e.id)
                        }
                        None => None
                    }
                }
                _ => None
            },
            csearch::FoundAst::Found(&ast::IIImplItem(_, ref ii)) => match ii.node {
                ast::ConstImplItem(_, ref expr) => Some(expr.id),
                _ => None
            },
            _ => None
        };
        // If we used the reference expression, particularly to choose an impl
        // of a trait-associated const, don't cache that, because the next
        // lookup with the same def_id may yield a different result.
        if !used_ref_id {
            tcx.extern_const_statics
               .borrow_mut().insert(def_id,
                                    expr_id.unwrap_or(ast::DUMMY_NODE_ID));
        }
        expr_id.map(|id| tcx.map.expect_expr(id))
    }
}

fn inline_const_fn_from_external_crate(tcx: &ty::ctxt, def_id: ast::DefId)
                                       -> Option<ast::NodeId> {
    match tcx.extern_const_fns.borrow().get(&def_id) {
        Some(&ast::DUMMY_NODE_ID) => return None,
        Some(&fn_id) => return Some(fn_id),
        None => {}
    }

    if !csearch::is_const_fn(&tcx.sess.cstore, def_id) {
        tcx.extern_const_fns.borrow_mut().insert(def_id, ast::DUMMY_NODE_ID);
        return None;
    }

    let fn_id = match csearch::maybe_get_item_ast(tcx, def_id,
        box |a, b, c, d| astencode::decode_inlined_item(a, b, c, d)) {
        csearch::FoundAst::Found(&ast::IIItem(ref item)) => Some(item.id),
        csearch::FoundAst::Found(&ast::IIImplItem(_, ref item)) => Some(item.id),
        _ => None
    };
    tcx.extern_const_fns.borrow_mut().insert(def_id,
                                             fn_id.unwrap_or(ast::DUMMY_NODE_ID));
    fn_id
}

pub fn lookup_const_fn_by_id<'tcx>(tcx: &ty::ctxt<'tcx>, def_id: ast::DefId)
                                   -> Option<FnLikeNode<'tcx>>
{
    let fn_id = if !ast_util::is_local(def_id) {
        if let Some(fn_id) = inline_const_fn_from_external_crate(tcx, def_id) {
            fn_id
        } else {
            return None;
        }
    } else {
        def_id.node
    };

    let fn_like = match FnLikeNode::from_node(tcx.map.get(fn_id)) {
        Some(fn_like) => fn_like,
        None => return None
    };

    match fn_like.kind() {
        visit::FkItemFn(_, _, _, ast::Constness::Const, _, _) => {
            Some(fn_like)
        }
        visit::FkMethod(_, m, _) => {
            if m.constness == ast::Constness::Const {
                Some(fn_like)
            } else {
                None
            }
        }
        _ => None
    }
}

#[derive(Clone, PartialEq)]
pub enum ConstVal {
    Float(f64),
    Int(i64),
    Uint(u64),
    Str(InternedString),
    Binary(Rc<Vec<u8>>),
    Bool(bool),
    Struct(ast::NodeId),
    Tuple(ast::NodeId),
}

impl ConstVal {
    pub fn description(&self) -> &'static str {
        match *self {
            Float(_) => "float",
            Int(i) if i < 0 => "negative integer",
            Int(_) => "positive integer",
            Uint(_) => "unsigned integer",
            Str(_) => "string literal",
            Binary(_) => "binary array",
            Bool(_) => "boolean",
            Struct(_) => "struct",
            Tuple(_) => "tuple",
        }
    }
}

pub fn const_expr_to_pat(tcx: &ty::ctxt, expr: &Expr, span: Span) -> P<ast::Pat> {
    let pat = match expr.node {
        ast::ExprTup(ref exprs) =>
            ast::PatTup(exprs.iter().map(|expr| const_expr_to_pat(tcx, &**expr, span)).collect()),

        ast::ExprCall(ref callee, ref args) => {
            let def = *tcx.def_map.borrow().get(&callee.id).unwrap();
            if let Vacant(entry) = tcx.def_map.borrow_mut().entry(expr.id) {
               entry.insert(def);
            }
            let path = match def.full_def() {
                def::DefStruct(def_id) => def_to_path(tcx, def_id),
                def::DefVariant(_, variant_did, _) => def_to_path(tcx, variant_did),
                _ => unreachable!()
            };
            let pats = args.iter().map(|expr| const_expr_to_pat(tcx, &**expr, span)).collect();
            ast::PatEnum(path, Some(pats))
        }

        ast::ExprStruct(ref path, ref fields, None) => {
            let field_pats = fields.iter().map(|field| codemap::Spanned {
                span: codemap::DUMMY_SP,
                node: ast::FieldPat {
                    ident: field.ident.node,
                    pat: const_expr_to_pat(tcx, &*field.expr, span),
                    is_shorthand: false,
                },
            }).collect();
            ast::PatStruct(path.clone(), field_pats, false)
        }

        ast::ExprVec(ref exprs) => {
            let pats = exprs.iter().map(|expr| const_expr_to_pat(tcx, &**expr, span)).collect();
            ast::PatVec(pats, None, vec![])
        }

        ast::ExprPath(_, ref path) => {
            let opt_def = tcx.def_map.borrow().get(&expr.id).map(|d| d.full_def());
            match opt_def {
                Some(def::DefStruct(..)) =>
                    ast::PatStruct(path.clone(), vec![], false),
                Some(def::DefVariant(..)) =>
                    ast::PatEnum(path.clone(), None),
                _ => {
                    match lookup_const(tcx, expr) {
                        Some(actual) => return const_expr_to_pat(tcx, actual, span),
                        _ => unreachable!()
                    }
                }
            }
        }

        _ => ast::PatLit(P(expr.clone()))
    };
    P(ast::Pat { id: expr.id, node: pat, span: span })
}

pub fn eval_const_expr(tcx: &ty::ctxt, e: &Expr) -> ConstVal {
    match eval_const_expr_partial(tcx, e, ExprTypeChecked) {
        Ok(r) => r,
        Err(s) => tcx.sess.span_fatal(s.span, &s.description())
    }
}


#[derive(Clone)]
pub struct ConstEvalErr {
    pub span: Span,
    pub kind: ErrKind,
}

#[derive(Clone)]
pub enum ErrKind {
    CannotCast,
    CannotCastTo(&'static str),
    InvalidOpForBools(ast::BinOp_),
    InvalidOpForFloats(ast::BinOp_),
    InvalidOpForIntUint(ast::BinOp_),
    InvalidOpForUintInt(ast::BinOp_),
    NegateOn(ConstVal),
    NotOn(ConstVal),

    NegateWithOverflow(i64),
    AddiWithOverflow(i64, i64),
    SubiWithOverflow(i64, i64),
    MuliWithOverflow(i64, i64),
    AdduWithOverflow(u64, u64),
    SubuWithOverflow(u64, u64),
    MuluWithOverflow(u64, u64),
    DivideByZero,
    DivideWithOverflow,
    ModuloByZero,
    ModuloWithOverflow,
    ShiftLeftWithOverflow,
    ShiftRightWithOverflow,
    MissingStructField,
    NonConstPath,
    ExpectedConstTuple,
    ExpectedConstStruct,
    TupleIndexOutOfBounds,

    MiscBinaryOp,
    MiscCatchAll,
}

impl ConstEvalErr {
    pub fn description(&self) -> Cow<str> {
        use self::ErrKind::*;

        match self.kind {
            CannotCast => "can't cast this type".into_cow(),
            CannotCastTo(s) => format!("can't cast this type to {}", s).into_cow(),
            InvalidOpForBools(_) =>  "can't do this op on bools".into_cow(),
            InvalidOpForFloats(_) => "can't do this op on floats".into_cow(),
            InvalidOpForIntUint(..) => "can't do this op on an isize and usize".into_cow(),
            InvalidOpForUintInt(..) => "can't do this op on a usize and isize".into_cow(),
            NegateOn(ref const_val) => format!("negate on {}", const_val.description()).into_cow(),
            NotOn(ref const_val) => format!("not on {}", const_val.description()).into_cow(),

            NegateWithOverflow(..) => "attempted to negate with overflow".into_cow(),
            AddiWithOverflow(..) => "attempted to add with overflow".into_cow(),
            SubiWithOverflow(..) => "attempted to sub with overflow".into_cow(),
            MuliWithOverflow(..) => "attempted to mul with overflow".into_cow(),
            AdduWithOverflow(..) => "attempted to add with overflow".into_cow(),
            SubuWithOverflow(..) => "attempted to sub with overflow".into_cow(),
            MuluWithOverflow(..) => "attempted to mul with overflow".into_cow(),
            DivideByZero         => "attempted to divide by zero".into_cow(),
            DivideWithOverflow   => "attempted to divide with overflow".into_cow(),
            ModuloByZero         => "attempted remainder with a divisor of zero".into_cow(),
            ModuloWithOverflow   => "attempted remainder with overflow".into_cow(),
            ShiftLeftWithOverflow => "attempted left shift with overflow".into_cow(),
            ShiftRightWithOverflow => "attempted right shift with overflow".into_cow(),
            MissingStructField  => "nonexistent struct field".into_cow(),
            NonConstPath        => "non-constant path in constant expr".into_cow(),
            ExpectedConstTuple => "expected constant tuple".into_cow(),
            ExpectedConstStruct => "expected constant struct".into_cow(),
            TupleIndexOutOfBounds => "tuple index out of bounds".into_cow(),

            MiscBinaryOp => "bad operands for binary".into_cow(),
            MiscCatchAll => "unsupported constant expr".into_cow(),
        }
    }
}

pub type EvalResult = Result<ConstVal, ConstEvalErr>;
pub type CastResult = Result<ConstVal, ErrKind>;

// FIXME: Long-term, this enum should go away: trying to evaluate
// an expression which hasn't been type-checked is a recipe for
// disaster.  That said, it's not clear how to fix ast_ty_to_ty
// to avoid the ordering issue.

/// Hint to determine how to evaluate constant expressions which
/// might not be type-checked.
#[derive(Copy, Clone, Debug)]
pub enum EvalHint<'tcx> {
    /// We have a type-checked expression.
    ExprTypeChecked,
    /// We have an expression which hasn't been type-checked, but we have
    /// an idea of what the type will be because of the context. For example,
    /// the length of an array is always `usize`. (This is referred to as
    /// a hint because it isn't guaranteed to be consistent with what
    /// type-checking would compute.)
    UncheckedExprHint(Ty<'tcx>),
    /// We have an expression which has not yet been type-checked, and
    /// and we have no clue what the type will be.
    UncheckedExprNoHint,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum IntTy { I8, I16, I32, I64 }
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UintTy { U8, U16, U32, U64 }

impl IntTy {
    pub fn from(tcx: &ty::ctxt, t: ast::IntTy) -> IntTy {
        let t = if let ast::TyIs = t {
            tcx.sess.target.int_type
        } else {
            t
        };
        match t {
            ast::TyIs => unreachable!(),
            ast::TyI8  => IntTy::I8,
            ast::TyI16 => IntTy::I16,
            ast::TyI32 => IntTy::I32,
            ast::TyI64 => IntTy::I64,
        }
    }
}

impl UintTy {
    pub fn from(tcx: &ty::ctxt, t: ast::UintTy) -> UintTy {
        let t = if let ast::TyUs = t {
            tcx.sess.target.uint_type
        } else {
            t
        };
        match t {
            ast::TyUs => unreachable!(),
            ast::TyU8  => UintTy::U8,
            ast::TyU16 => UintTy::U16,
            ast::TyU32 => UintTy::U32,
            ast::TyU64 => UintTy::U64,
        }
    }
}

macro_rules! signal {
    ($e:expr, $exn:expr) => {
        return Err(ConstEvalErr { span: $e.span, kind: $exn })
    }
}

// The const_{int,uint}_checked_{neg,add,sub,mul,div,shl,shr} family
// of functions catch and signal overflow errors during constant
// evaluation.
//
// They all take the operator's arguments (`a` and `b` if binary), the
// overall expression (`e`) and, if available, whole expression's
// concrete type (`opt_ety`).
//
// If the whole expression's concrete type is None, then this is a
// constant evaluation happening before type check (e.g. in the check
// to confirm that a pattern range's left-side is not greater than its
// right-side). We do not do arithmetic modulo the type's bitwidth in
// such a case; we just do 64-bit arithmetic and assume that later
// passes will do it again with the type information, and thus do the
// overflow checks then.

pub fn const_int_checked_neg<'a>(
    a: i64, e: &'a Expr, opt_ety: Option<IntTy>) -> EvalResult {

    let (min,max) = match opt_ety {
        // (-i8::MIN is itself not an i8, etc, but this is an easy way
        // to allow literals to pass the check. Of course that does
        // not work for i64::MIN.)
        Some(IntTy::I8) =>  (-(i8::MAX as i64), -(i8::MIN as i64)),
        Some(IntTy::I16) => (-(i16::MAX as i64), -(i16::MIN as i64)),
        Some(IntTy::I32) => (-(i32::MAX as i64), -(i32::MIN as i64)),
        None | Some(IntTy::I64) => (-i64::MAX, -(i64::MIN+1)),
    };

    let oflo = a < min || a > max;
    if oflo {
        signal!(e, NegateWithOverflow(a));
    } else {
        Ok(Int(-a))
    }
}

pub fn const_uint_checked_neg<'a>(
    a: u64, _e: &'a Expr, _opt_ety: Option<UintTy>) -> EvalResult {
    // This always succeeds, and by definition, returns `(!a)+1`.
    Ok(Uint((!a).wrapping_add(1)))
}

fn const_uint_not(a: u64, opt_ety: Option<UintTy>) -> ConstVal {
    let mask = match opt_ety {
        Some(UintTy::U8) => u8::MAX as u64,
        Some(UintTy::U16) => u16::MAX as u64,
        Some(UintTy::U32) => u32::MAX as u64,
        None | Some(UintTy::U64) => u64::MAX,
    };
    Uint(!a & mask)
}

macro_rules! overflow_checking_body {
    ($a:ident, $b:ident, $ety:ident, $overflowing_op:ident,
     lhs: $to_8_lhs:ident $to_16_lhs:ident $to_32_lhs:ident,
     rhs: $to_8_rhs:ident $to_16_rhs:ident $to_32_rhs:ident $to_64_rhs:ident,
     $EnumTy:ident $T8: ident $T16: ident $T32: ident $T64: ident,
     $result_type: ident) => { {
        let (a,b,opt_ety) = ($a,$b,$ety);
        match opt_ety {
            Some($EnumTy::$T8) => match (a.$to_8_lhs(), b.$to_8_rhs()) {
                (Some(a), Some(b)) => {
                    let (a, oflo) = a.$overflowing_op(b);
                    (a as $result_type, oflo)
                }
                (None, _) | (_, None) => (0, true)
            },
            Some($EnumTy::$T16) => match (a.$to_16_lhs(), b.$to_16_rhs()) {
                (Some(a), Some(b)) => {
                    let (a, oflo) = a.$overflowing_op(b);
                    (a as $result_type, oflo)
                }
                (None, _) | (_, None) => (0, true)
            },
            Some($EnumTy::$T32) => match (a.$to_32_lhs(), b.$to_32_rhs()) {
                (Some(a), Some(b)) => {
                    let (a, oflo) = a.$overflowing_op(b);
                    (a as $result_type, oflo)
                }
                (None, _) | (_, None) => (0, true)
            },
            None | Some($EnumTy::$T64) => match b.$to_64_rhs() {
                Some(b) => a.$overflowing_op(b),
                None => (0, true),
            }
        }
    } }
}

macro_rules! int_arith_body {
    ($a:ident, $b:ident, $ety:ident, $overflowing_op:ident) => {
        overflow_checking_body!(
            $a, $b, $ety, $overflowing_op,
            lhs: to_i8 to_i16 to_i32,
            rhs: to_i8 to_i16 to_i32 to_i64, IntTy I8 I16 I32 I64, i64)
    }
}

macro_rules! uint_arith_body {
    ($a:ident, $b:ident, $ety:ident, $overflowing_op:ident) => {
        overflow_checking_body!(
            $a, $b, $ety, $overflowing_op,
            lhs: to_u8 to_u16 to_u32,
            rhs: to_u8 to_u16 to_u32 to_u64, UintTy U8 U16 U32 U64, u64)
    }
}

macro_rules! int_shift_body {
    ($a:ident, $b:ident, $ety:ident, $overflowing_op:ident) => {
        overflow_checking_body!(
            $a, $b, $ety, $overflowing_op,
            lhs: to_i8 to_i16 to_i32,
            rhs: to_u32 to_u32 to_u32 to_u32, IntTy I8 I16 I32 I64, i64)
    }
}

macro_rules! uint_shift_body {
    ($a:ident, $b:ident, $ety:ident, $overflowing_op:ident) => {
        overflow_checking_body!(
            $a, $b, $ety, $overflowing_op,
            lhs: to_u8 to_u16 to_u32,
            rhs: to_u32 to_u32 to_u32 to_u32, UintTy U8 U16 U32 U64, u64)
    }
}

macro_rules! pub_fn_checked_op {
    {$fn_name:ident ($a:ident : $a_ty:ty, $b:ident : $b_ty:ty,.. $WhichTy:ident) {
        $ret_oflo_body:ident $overflowing_op:ident
            $const_ty:ident $signal_exn:expr
    }} => {
        pub fn $fn_name<'a>($a: $a_ty,
                            $b: $b_ty,
                            e: &'a Expr,
                            opt_ety: Option<$WhichTy>) -> EvalResult {
            let (ret, oflo) = $ret_oflo_body!($a, $b, opt_ety, $overflowing_op);
            if !oflo { Ok($const_ty(ret)) } else { signal!(e, $signal_exn) }
        }
    }
}

pub_fn_checked_op!{ const_int_checked_add(a: i64, b: i64,.. IntTy) {
           int_arith_body overflowing_add Int AddiWithOverflow(a, b)
}}

pub_fn_checked_op!{ const_int_checked_sub(a: i64, b: i64,.. IntTy) {
           int_arith_body overflowing_sub Int SubiWithOverflow(a, b)
}}

pub_fn_checked_op!{ const_int_checked_mul(a: i64, b: i64,.. IntTy) {
           int_arith_body overflowing_mul Int MuliWithOverflow(a, b)
}}

pub fn const_int_checked_div<'a>(
    a: i64, b: i64, e: &'a Expr, opt_ety: Option<IntTy>) -> EvalResult {
    if b == 0 { signal!(e, DivideByZero); }
    let (ret, oflo) = int_arith_body!(a, b, opt_ety, overflowing_div);
    if !oflo { Ok(Int(ret)) } else { signal!(e, DivideWithOverflow) }
}

pub fn const_int_checked_rem<'a>(
    a: i64, b: i64, e: &'a Expr, opt_ety: Option<IntTy>) -> EvalResult {
    if b == 0 { signal!(e, ModuloByZero); }
    let (ret, oflo) = int_arith_body!(a, b, opt_ety, overflowing_rem);
    if !oflo { Ok(Int(ret)) } else { signal!(e, ModuloWithOverflow) }
}

pub_fn_checked_op!{ const_int_checked_shl(a: i64, b: i64,.. IntTy) {
           int_shift_body overflowing_shl Int ShiftLeftWithOverflow
}}

pub_fn_checked_op!{ const_int_checked_shl_via_uint(a: i64, b: u64,.. IntTy) {
           int_shift_body overflowing_shl Int ShiftLeftWithOverflow
}}

pub_fn_checked_op!{ const_int_checked_shr(a: i64, b: i64,.. IntTy) {
           int_shift_body overflowing_shr Int ShiftRightWithOverflow
}}

pub_fn_checked_op!{ const_int_checked_shr_via_uint(a: i64, b: u64,.. IntTy) {
           int_shift_body overflowing_shr Int ShiftRightWithOverflow
}}

pub_fn_checked_op!{ const_uint_checked_add(a: u64, b: u64,.. UintTy) {
           uint_arith_body overflowing_add Uint AdduWithOverflow(a, b)
}}

pub_fn_checked_op!{ const_uint_checked_sub(a: u64, b: u64,.. UintTy) {
           uint_arith_body overflowing_sub Uint SubuWithOverflow(a, b)
}}

pub_fn_checked_op!{ const_uint_checked_mul(a: u64, b: u64,.. UintTy) {
           uint_arith_body overflowing_mul Uint MuluWithOverflow(a, b)
}}

pub fn const_uint_checked_div<'a>(
    a: u64, b: u64, e: &'a Expr, opt_ety: Option<UintTy>) -> EvalResult {
    if b == 0 { signal!(e, DivideByZero); }
    let (ret, oflo) = uint_arith_body!(a, b, opt_ety, overflowing_div);
    if !oflo { Ok(Uint(ret)) } else { signal!(e, DivideWithOverflow) }
}

pub fn const_uint_checked_rem<'a>(
    a: u64, b: u64, e: &'a Expr, opt_ety: Option<UintTy>) -> EvalResult {
    if b == 0 { signal!(e, ModuloByZero); }
    let (ret, oflo) = uint_arith_body!(a, b, opt_ety, overflowing_rem);
    if !oflo { Ok(Uint(ret)) } else { signal!(e, ModuloWithOverflow) }
}

pub_fn_checked_op!{ const_uint_checked_shl(a: u64, b: u64,.. UintTy) {
           uint_shift_body overflowing_shl Uint ShiftLeftWithOverflow
}}

pub_fn_checked_op!{ const_uint_checked_shl_via_int(a: u64, b: i64,.. UintTy) {
           uint_shift_body overflowing_shl Uint ShiftLeftWithOverflow
}}

pub_fn_checked_op!{ const_uint_checked_shr(a: u64, b: u64,.. UintTy) {
           uint_shift_body overflowing_shr Uint ShiftRightWithOverflow
}}

pub_fn_checked_op!{ const_uint_checked_shr_via_int(a: u64, b: i64,.. UintTy) {
           uint_shift_body overflowing_shr Uint ShiftRightWithOverflow
}}

/// Evaluate a constant expression in a context where the expression isn't
/// guaranteed to be evaluatable. `ty_hint` is usually ExprTypeChecked,
/// but a few places need to evaluate constants during type-checking, like
/// computing the length of an array. (See also the FIXME above EvalHint.)
pub fn eval_const_expr_partial<'tcx>(tcx: &ty::ctxt<'tcx>,
                                     e: &Expr,
                                     ty_hint: EvalHint<'tcx>) -> EvalResult {
    fn fromb(b: bool) -> ConstVal { Int(b as i64) }

    // Try to compute the type of the expression based on the EvalHint.
    // (See also the definition of EvalHint, and the FIXME above EvalHint.)
    let ety = match ty_hint {
        ExprTypeChecked => {
            // After type-checking, expr_ty is guaranteed to succeed.
            Some(tcx.expr_ty(e))
        }
        UncheckedExprHint(ty) => {
            // Use the type hint; it's not guaranteed to be right, but it's
            // usually good enough.
            Some(ty)
        }
        UncheckedExprNoHint => {
            // This expression might not be type-checked, and we have no hint.
            // Try to query the context for a type anyway; we might get lucky
            // (for example, if the expression was imported from another crate).
            tcx.expr_ty_opt(e)
        }
    };

    // If type of expression itself is int or uint, normalize in these
    // bindings so that isize/usize is mapped to a type with an
    // inherently known bitwidth.
    let expr_int_type = ety.and_then(|ty| {
        if let ty::TyInt(t) = ty.sty {
            Some(IntTy::from(tcx, t)) } else { None }
    });
    let expr_uint_type = ety.and_then(|ty| {
        if let ty::TyUint(t) = ty.sty {
            Some(UintTy::from(tcx, t)) } else { None }
    });

    let result = match e.node {
      ast::ExprUnary(ast::UnNeg, ref inner) => {
        match try!(eval_const_expr_partial(tcx, &**inner, ty_hint)) {
          Float(f) => Float(-f),
          Int(n) =>  try!(const_int_checked_neg(n, e, expr_int_type)),
          Uint(i) => {
              try!(const_uint_checked_neg(i, e, expr_uint_type))
          }
          const_val => signal!(e, NegateOn(const_val)),
        }
      }
      ast::ExprUnary(ast::UnNot, ref inner) => {
        match try!(eval_const_expr_partial(tcx, &**inner, ty_hint)) {
          Int(i) => Int(!i),
          Uint(i) => const_uint_not(i, expr_uint_type),
          Bool(b) => Bool(!b),
          const_val => signal!(e, NotOn(const_val)),
        }
      }
      ast::ExprBinary(op, ref a, ref b) => {
        let b_ty = match op.node {
            ast::BiShl | ast::BiShr => {
                if let ExprTypeChecked = ty_hint {
                    ExprTypeChecked
                } else {
                    UncheckedExprHint(tcx.types.usize)
                }
            }
            _ => ty_hint
        };
        match (try!(eval_const_expr_partial(tcx, &**a, ty_hint)),
               try!(eval_const_expr_partial(tcx, &**b, b_ty))) {
          (Float(a), Float(b)) => {
            match op.node {
              ast::BiAdd => Float(a + b),
              ast::BiSub => Float(a - b),
              ast::BiMul => Float(a * b),
              ast::BiDiv => Float(a / b),
              ast::BiRem => Float(a % b),
              ast::BiEq => fromb(a == b),
              ast::BiLt => fromb(a < b),
              ast::BiLe => fromb(a <= b),
              ast::BiNe => fromb(a != b),
              ast::BiGe => fromb(a >= b),
              ast::BiGt => fromb(a > b),
              _ => signal!(e, InvalidOpForFloats(op.node))
            }
          }
          (Int(a), Int(b)) => {
            match op.node {
              ast::BiAdd => try!(const_int_checked_add(a,b,e,expr_int_type)),
              ast::BiSub => try!(const_int_checked_sub(a,b,e,expr_int_type)),
              ast::BiMul => try!(const_int_checked_mul(a,b,e,expr_int_type)),
              ast::BiDiv => try!(const_int_checked_div(a,b,e,expr_int_type)),
              ast::BiRem => try!(const_int_checked_rem(a,b,e,expr_int_type)),
              ast::BiAnd | ast::BiBitAnd => Int(a & b),
              ast::BiOr | ast::BiBitOr => Int(a | b),
              ast::BiBitXor => Int(a ^ b),
              ast::BiShl => try!(const_int_checked_shl(a,b,e,expr_int_type)),
              ast::BiShr => try!(const_int_checked_shr(a,b,e,expr_int_type)),
              ast::BiEq => fromb(a == b),
              ast::BiLt => fromb(a < b),
              ast::BiLe => fromb(a <= b),
              ast::BiNe => fromb(a != b),
              ast::BiGe => fromb(a >= b),
              ast::BiGt => fromb(a > b)
            }
          }
          (Uint(a), Uint(b)) => {
            match op.node {
              ast::BiAdd => try!(const_uint_checked_add(a,b,e,expr_uint_type)),
              ast::BiSub => try!(const_uint_checked_sub(a,b,e,expr_uint_type)),
              ast::BiMul => try!(const_uint_checked_mul(a,b,e,expr_uint_type)),
              ast::BiDiv => try!(const_uint_checked_div(a,b,e,expr_uint_type)),
              ast::BiRem => try!(const_uint_checked_rem(a,b,e,expr_uint_type)),
              ast::BiAnd | ast::BiBitAnd => Uint(a & b),
              ast::BiOr | ast::BiBitOr => Uint(a | b),
              ast::BiBitXor => Uint(a ^ b),
              ast::BiShl => try!(const_uint_checked_shl(a,b,e,expr_uint_type)),
              ast::BiShr => try!(const_uint_checked_shr(a,b,e,expr_uint_type)),
              ast::BiEq => fromb(a == b),
              ast::BiLt => fromb(a < b),
              ast::BiLe => fromb(a <= b),
              ast::BiNe => fromb(a != b),
              ast::BiGe => fromb(a >= b),
              ast::BiGt => fromb(a > b),
            }
          }
          // shifts can have any integral type as their rhs
          (Int(a), Uint(b)) => {
            match op.node {
              ast::BiShl => try!(const_int_checked_shl_via_uint(a,b,e,expr_int_type)),
              ast::BiShr => try!(const_int_checked_shr_via_uint(a,b,e,expr_int_type)),
              _ => signal!(e, InvalidOpForIntUint(op.node)),
            }
          }
          (Uint(a), Int(b)) => {
            match op.node {
              ast::BiShl => try!(const_uint_checked_shl_via_int(a,b,e,expr_uint_type)),
              ast::BiShr => try!(const_uint_checked_shr_via_int(a,b,e,expr_uint_type)),
              _ => signal!(e, InvalidOpForUintInt(op.node)),
            }
          }
          (Bool(a), Bool(b)) => {
            Bool(match op.node {
              ast::BiAnd => a && b,
              ast::BiOr => a || b,
              ast::BiBitXor => a ^ b,
              ast::BiBitAnd => a & b,
              ast::BiBitOr => a | b,
              ast::BiEq => a == b,
              ast::BiNe => a != b,
              _ => signal!(e, InvalidOpForBools(op.node)),
             })
          }

          _ => signal!(e, MiscBinaryOp),
        }
      }
      ast::ExprCast(ref base, ref target_ty) => {
        let ety = ety.or_else(|| ast_ty_to_prim_ty(tcx, &**target_ty))
                .unwrap_or_else(|| {
                    tcx.sess.span_fatal(target_ty.span,
                                        "target type not found for const cast")
                });

        let base_hint = if let ExprTypeChecked = ty_hint {
            ExprTypeChecked
        } else {
            // FIXME (#23833): the type-hint can cause problems,
            // e.g. `(i8::MAX + 1_i8) as u32` feeds in `u32` as result
            // type to the sum, and thus no overflow is signaled.
            match tcx.expr_ty_opt(&base) {
                Some(t) => UncheckedExprHint(t),
                None => ty_hint
            }
        };

        let val = try!(eval_const_expr_partial(tcx, &**base, base_hint));
        match cast_const(tcx, val, ety) {
            Ok(val) => val,
            Err(kind) => return Err(ConstEvalErr { span: e.span, kind: kind }),
        }
      }
      ast::ExprPath(..) => {
          let opt_def = tcx.def_map.borrow().get(&e.id).map(|d| d.full_def());
          let (const_expr, const_ty) = match opt_def {
              Some(def::DefConst(def_id)) => {
                  if ast_util::is_local(def_id) {
                      match tcx.map.find(def_id.node) {
                          Some(ast_map::NodeItem(it)) => match it.node {
                              ast::ItemConst(ref ty, ref expr) => {
                                  (Some(&**expr), Some(&**ty))
                              }
                              _ => (None, None)
                          },
                          _ => (None, None)
                      }
                  } else {
                      (lookup_const_by_id(tcx, def_id, Some(e.id)), None)
                  }
              }
              Some(def::DefAssociatedConst(def_id)) => {
                  if ast_util::is_local(def_id) {
                      match tcx.impl_or_trait_item(def_id).container() {
                          ty::TraitContainer(trait_id) => match tcx.map.find(def_id.node) {
                              Some(ast_map::NodeTraitItem(ti)) => match ti.node {
                                  ast::ConstTraitItem(ref ty, _) => {
                                      if let ExprTypeChecked = ty_hint {
                                          let substs = tcx.node_id_item_substs(e.id).substs;
                                          (resolve_trait_associated_const(tcx,
                                                                          ti,
                                                                          trait_id,
                                                                          substs),
                                           Some(&**ty))
                                       } else {
                                           (None, None)
                                       }
                                  }
                                  _ => (None, None)
                              },
                              _ => (None, None)
                          },
                          ty::ImplContainer(_) => match tcx.map.find(def_id.node) {
                              Some(ast_map::NodeImplItem(ii)) => match ii.node {
                                  ast::ConstImplItem(ref ty, ref expr) => {
                                      (Some(&**expr), Some(&**ty))
                                  }
                                  _ => (None, None)
                              },
                              _ => (None, None)
                          },
                      }
                  } else {
                      (lookup_const_by_id(tcx, def_id, Some(e.id)), None)
                  }
              }
              Some(def::DefVariant(enum_def, variant_def, _)) => {
                  (lookup_variant_by_id(tcx, enum_def, variant_def), None)
              }
              Some(def::DefStruct(_)) => {
                  return Ok(ConstVal::Struct(e.id))
              }
              _ => (None, None)
          };
          let const_expr = match const_expr {
              Some(actual_e) => actual_e,
              None => signal!(e, NonConstPath)
          };
          let item_hint = if let UncheckedExprNoHint = ty_hint {
              match const_ty {
                  Some(ty) => match ast_ty_to_prim_ty(tcx, ty) {
                      Some(ty) => UncheckedExprHint(ty),
                      None => UncheckedExprNoHint
                  },
                  None => UncheckedExprNoHint
              }
          } else {
              ty_hint
          };
          try!(eval_const_expr_partial(tcx, const_expr, item_hint))
      }
      ast::ExprLit(ref lit) => {
          lit_to_const(&**lit, ety)
      }
      ast::ExprParen(ref e) => try!(eval_const_expr_partial(tcx, &**e, ty_hint)),
      ast::ExprBlock(ref block) => {
        match block.expr {
            Some(ref expr) => try!(eval_const_expr_partial(tcx, &**expr, ty_hint)),
            None => Int(0)
        }
      }
      ast::ExprTup(_) => Tuple(e.id),
      ast::ExprStruct(..) => Struct(e.id),
      ast::ExprTupField(ref base, index) => {
        let base_hint = if let ExprTypeChecked = ty_hint {
            ExprTypeChecked
        } else {
            UncheckedExprNoHint
        };
        if let Ok(c) = eval_const_expr_partial(tcx, base, base_hint) {
            if let Tuple(tup_id) = c {
                if let ast::ExprTup(ref fields) = tcx.map.expect_expr(tup_id).node {
                    if index.node < fields.len() {
                        return eval_const_expr_partial(tcx, &fields[index.node], base_hint)
                    } else {
                        signal!(e, TupleIndexOutOfBounds);
                    }
                } else {
                    unreachable!()
                }
            } else {
                signal!(base, ExpectedConstTuple);
            }
        } else {
            signal!(base, NonConstPath)
        }
      }
      ast::ExprField(ref base, field_name) => {
        // Get the base expression if it is a struct and it is constant
        let base_hint = if let ExprTypeChecked = ty_hint {
            ExprTypeChecked
        } else {
            UncheckedExprNoHint
        };
        if let Ok(c) = eval_const_expr_partial(tcx, base, base_hint) {
            if let Struct(struct_id) = c {
                if let ast::ExprStruct(_, ref fields, _) = tcx.map.expect_expr(struct_id).node {
                    // Check that the given field exists and evaluate it
                    // if the idents are compared run-pass/issue-19244 fails
                    if let Some(f) = fields.iter().find(|f| f.ident.node.name
                                                         == field_name.node.name) {
                        return eval_const_expr_partial(tcx, &*f.expr, base_hint)
                    } else {
                        signal!(e, MissingStructField);
                    }
                } else {
                    unreachable!()
                }
            } else {
                signal!(base, ExpectedConstStruct);
            }
        } else {
            signal!(base, NonConstPath);
        }
      }
      _ => signal!(e, MiscCatchAll)
    };

    Ok(result)
}

fn resolve_trait_associated_const<'a, 'tcx: 'a>(tcx: &'a ty::ctxt<'tcx>,
                                                ti: &'tcx ast::TraitItem,
                                                trait_id: ast::DefId,
                                                rcvr_substs: subst::Substs<'tcx>)
                                                -> Option<&'tcx Expr>
{
    let subst::SeparateVecsPerParamSpace {
        types: rcvr_type,
        selfs: rcvr_self,
        fns: _,
    } = rcvr_substs.types.split();
    let trait_substs =
        subst::Substs::erased(subst::VecPerParamSpace::new(rcvr_type,
                                                           rcvr_self,
                                                           Vec::new()));
    let trait_substs = tcx.mk_substs(trait_substs);
    debug!("resolve_trait_associated_const: trait_substs={:?}",
           trait_substs);
    let trait_ref = ty::Binder(ty::TraitRef { def_id: trait_id,
                                              substs: trait_substs });

    tcx.populate_implementations_for_trait_if_necessary(trait_ref.def_id());
    let infcx = infer::new_infer_ctxt(tcx, &tcx.tables, None, false);

    let mut selcx = traits::SelectionContext::new(&infcx);
    let obligation = traits::Obligation::new(traits::ObligationCause::dummy(),
                                             trait_ref.to_poly_trait_predicate());
    let selection = match selcx.select(&obligation) {
        Ok(Some(vtable)) => vtable,
        // Still ambiguous, so give up and let the caller decide whether this
        // expression is really needed yet. Some associated constant values
        // can't be evaluated until monomorphization is done in trans.
        Ok(None) => {
            return None
        }
        Err(e) => {
            tcx.sess.span_bug(ti.span,
                              &format!("Encountered error `{:?}` when trying \
                                        to select an implementation for \
                                        constant trait item reference.",
                                       e))
        }
    };

    match selection {
        traits::VtableImpl(ref impl_data) => {
            match tcx.associated_consts(impl_data.impl_def_id)
                     .iter().find(|ic| ic.name == ti.ident.name) {
                Some(ic) => lookup_const_by_id(tcx, ic.def_id, None),
                None => match ti.node {
                    ast::ConstTraitItem(_, Some(ref expr)) => Some(&*expr),
                    _ => None,
                },
            }
        }
        _ => {
            tcx.sess.span_bug(
                ti.span,
                &format!("resolve_trait_associated_const: unexpected vtable type"))
        }
    }
}

fn cast_const<'tcx>(tcx: &ty::ctxt<'tcx>, val: ConstVal, ty: Ty) -> CastResult {
    macro_rules! convert_val {
        ($intermediate_ty:ty, $const_type:ident, $target_ty:ty) => {
            match val {
                Bool(b) => Ok($const_type(b as u64 as $intermediate_ty as $target_ty)),
                Uint(u) => Ok($const_type(u as $intermediate_ty as $target_ty)),
                Int(i) => Ok($const_type(i as $intermediate_ty as $target_ty)),
                Float(f) => Ok($const_type(f as $intermediate_ty as $target_ty)),
                _ => Err(ErrKind::CannotCastTo(stringify!($const_type))),
            }
        }
    }

    // Issue #23890: If isize/usize, then dispatch to appropriate target representation type
    match (&ty.sty, tcx.sess.target.int_type, tcx.sess.target.uint_type) {
        (&ty::TyInt(ast::TyIs), ast::TyI32, _) => return convert_val!(i32, Int, i64),
        (&ty::TyInt(ast::TyIs), ast::TyI64, _) => return convert_val!(i64, Int, i64),
        (&ty::TyInt(ast::TyIs), _, _) => panic!("unexpected target.int_type"),

        (&ty::TyUint(ast::TyUs), _, ast::TyU32) => return convert_val!(u32, Uint, u64),
        (&ty::TyUint(ast::TyUs), _, ast::TyU64) => return convert_val!(u64, Uint, u64),
        (&ty::TyUint(ast::TyUs), _, _) => panic!("unexpected target.uint_type"),

        _ => {}
    }

    match ty.sty {
        ty::TyInt(ast::TyIs) => unreachable!(),
        ty::TyUint(ast::TyUs) => unreachable!(),

        ty::TyInt(ast::TyI8) => convert_val!(i8, Int, i64),
        ty::TyInt(ast::TyI16) => convert_val!(i16, Int, i64),
        ty::TyInt(ast::TyI32) => convert_val!(i32, Int, i64),
        ty::TyInt(ast::TyI64) => convert_val!(i64, Int, i64),

        ty::TyUint(ast::TyU8) => convert_val!(u8, Uint, u64),
        ty::TyUint(ast::TyU16) => convert_val!(u16, Uint, u64),
        ty::TyUint(ast::TyU32) => convert_val!(u32, Uint, u64),
        ty::TyUint(ast::TyU64) => convert_val!(u64, Uint, u64),

        ty::TyFloat(ast::TyF32) => convert_val!(f32, Float, f64),
        ty::TyFloat(ast::TyF64) => convert_val!(f64, Float, f64),
        _ => Err(ErrKind::CannotCast),
    }
}

fn lit_to_const(lit: &ast::Lit, ty_hint: Option<Ty>) -> ConstVal {
    match lit.node {
        ast::LitStr(ref s, _) => Str((*s).clone()),
        ast::LitBinary(ref data) => {
            Binary(data.clone())
        }
        ast::LitByte(n) => Uint(n as u64),
        ast::LitChar(n) => Uint(n as u64),
        ast::LitInt(n, ast::SignedIntLit(_, ast::Plus)) => Int(n as i64),
        ast::LitInt(n, ast::UnsuffixedIntLit(ast::Plus)) => {
            match ty_hint.map(|ty| &ty.sty) {
                Some(&ty::TyUint(_)) => Uint(n),
                _ => Int(n as i64)
            }
        }
        ast::LitInt(n, ast::SignedIntLit(_, ast::Minus)) |
        ast::LitInt(n, ast::UnsuffixedIntLit(ast::Minus)) => Int(-(n as i64)),
        ast::LitInt(n, ast::UnsignedIntLit(_)) => Uint(n),
        ast::LitFloat(ref n, _) |
        ast::LitFloatUnsuffixed(ref n) => {
            Float(n.parse::<f64>().unwrap() as f64)
        }
        ast::LitBool(b) => Bool(b)
    }
}

pub fn compare_const_vals(a: &ConstVal, b: &ConstVal) -> Option<Ordering> {
    Some(match (a, b) {
        (&Int(a), &Int(b)) => a.cmp(&b),
        (&Uint(a), &Uint(b)) => a.cmp(&b),
        (&Float(a), &Float(b)) => {
            // This is pretty bad but it is the existing behavior.
            if a == b {
                Ordering::Equal
            } else if a < b {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
        (&Str(ref a), &Str(ref b)) => a.cmp(b),
        (&Bool(a), &Bool(b)) => a.cmp(&b),
        (&Binary(ref a), &Binary(ref b)) => a.cmp(b),
        _ => return None
    })
}

pub fn compare_lit_exprs<'tcx>(tcx: &ty::ctxt<'tcx>,
                               a: &Expr,
                               b: &Expr) -> Option<Ordering> {
    let a = match eval_const_expr_partial(tcx, a, ExprTypeChecked) {
        Ok(a) => a,
        Err(e) => {
            tcx.sess.span_err(a.span, &e.description());
            return None;
        }
    };
    let b = match eval_const_expr_partial(tcx, b, ExprTypeChecked) {
        Ok(b) => b,
        Err(e) => {
            tcx.sess.span_err(b.span, &e.description());
            return None;
        }
    };
    compare_const_vals(&a, &b)
}
