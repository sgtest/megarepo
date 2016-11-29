// Copyright 2012-2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//#![allow(non_camel_case_types)]

use rustc::middle::const_val::ConstVal::*;
use rustc::middle::const_val::ConstVal;
use self::ErrKind::*;
use self::EvalHint::*;

use rustc::hir::map as ast_map;
use rustc::hir::map::blocks::FnLikeNode;
use rustc::middle::cstore::InlinedItem;
use rustc::traits;
use rustc::hir::def::{Def, CtorKind};
use rustc::hir::def_id::DefId;
use rustc::ty::{self, Ty, TyCtxt};
use rustc::ty::util::IntTypeExt;
use rustc::ty::subst::Substs;
use rustc::traits::Reveal;
use rustc::util::common::ErrorReported;
use rustc::util::nodemap::DefIdMap;
use rustc::lint;

use graphviz::IntoCow;
use syntax::ast;
use rustc::hir::{Expr, PatKind};
use rustc::hir;
use syntax::ptr::P;
use syntax::codemap;
use syntax::attr::IntType;
use syntax_pos::{self, Span};

use std::borrow::Cow;
use std::cmp::Ordering;

use rustc_const_math::*;
use rustc_errors::DiagnosticBuilder;

macro_rules! math {
    ($e:expr, $op:expr) => {
        match $op {
            Ok(val) => val,
            Err(e) => signal!($e, Math(e)),
        }
    }
}

fn lookup_variant_by_id<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                  variant_def: DefId)
                                  -> Option<&'tcx Expr> {
    fn variant_expr<'a>(variants: &'a [hir::Variant], id: ast::NodeId)
                        -> Option<&'a Expr> {
        for variant in variants {
            if variant.node.data.id() == id {
                return variant.node.disr_expr.as_ref().map(|e| &**e);
            }
        }
        None
    }

    if let Some(variant_node_id) = tcx.map.as_local_node_id(variant_def) {
        let enum_node_id = tcx.map.get_parent(variant_node_id);
        match tcx.map.find(enum_node_id) {
            None => None,
            Some(ast_map::NodeItem(it)) => match it.node {
                hir::ItemEnum(hir::EnumDef { ref variants }, _) => {
                    variant_expr(variants, variant_node_id)
                }
                _ => None
            },
            Some(_) => None
        }
    } else {
        None
    }
}

/// * `def_id` is the id of the constant.
/// * `substs` is the monomorphized substitutions for the expression.
///
/// `substs` is optional and is used for associated constants.
/// This generally happens in late/trans const evaluation.
pub fn lookup_const_by_id<'a, 'tcx: 'a>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                        def_id: DefId,
                                        substs: Option<&'tcx Substs<'tcx>>)
                                        -> Option<(&'tcx Expr, Option<ty::Ty<'tcx>>)> {
    if let Some(node_id) = tcx.map.as_local_node_id(def_id) {
        match tcx.map.find(node_id) {
            None => None,
            Some(ast_map::NodeItem(it)) => match it.node {
                hir::ItemConst(ref ty, ref const_expr) => {
                    Some((&const_expr, tcx.ast_ty_to_prim_ty(ty)))
                }
                _ => None
            },
            Some(ast_map::NodeTraitItem(ti)) => match ti.node {
                hir::ConstTraitItem(ref ty, ref expr_option) => {
                    if let Some(substs) = substs {
                        // If we have a trait item and the substitutions for it,
                        // `resolve_trait_associated_const` will select an impl
                        // or the default.
                        let trait_id = tcx.map.get_parent(node_id);
                        let trait_id = tcx.map.local_def_id(trait_id);
                        let default_value = expr_option.as_ref()
                            .map(|expr| (&**expr, tcx.ast_ty_to_prim_ty(ty)));
                        resolve_trait_associated_const(tcx, def_id, default_value, trait_id, substs)
                    } else {
                        // Technically, without knowing anything about the
                        // expression that generates the obligation, we could
                        // still return the default if there is one. However,
                        // it's safer to return `None` than to return some value
                        // that may differ from what you would get from
                        // correctly selecting an impl.
                        None
                    }
                }
                _ => None
            },
            Some(ast_map::NodeImplItem(ii)) => match ii.node {
                hir::ImplItemKind::Const(ref ty, ref expr) => {
                    Some((&expr, tcx.ast_ty_to_prim_ty(ty)))
                }
                _ => None
            },
            Some(_) => None
        }
    } else {
        match tcx.extern_const_statics.borrow().get(&def_id) {
            Some(&None) => return None,
            Some(&Some((expr_id, ty))) => {
                return Some((tcx.map.expect_expr(expr_id), ty));
            }
            None => {}
        }
        let mut used_substs = false;
        let expr_ty = match tcx.sess.cstore.maybe_get_item_ast(tcx, def_id) {
            Some((&InlinedItem { body: ref const_expr, .. }, _)) => {
                Some((&**const_expr, Some(tcx.sess.cstore.item_type(tcx, def_id))))
            }
            _ => None
        };
        let expr_ty = match tcx.sess.cstore.describe_def(def_id) {
            Some(Def::AssociatedConst(_)) => {
                let trait_id = tcx.sess.cstore.trait_of_item(def_id);
                // As mentioned in the comments above for in-crate
                // constants, we only try to find the expression for a
                // trait-associated const if the caller gives us the
                // substitutions for the reference to it.
                if let Some(trait_id) = trait_id {
                    used_substs = true;

                    if let Some(substs) = substs {
                        resolve_trait_associated_const(tcx, def_id, expr_ty, trait_id, substs)
                    } else {
                        None
                    }
                } else {
                    expr_ty
                }
            },
            Some(Def::Const(..)) => expr_ty,
            _ => None
        };
        // If we used the substitutions, particularly to choose an impl
        // of a trait-associated const, don't cache that, because the next
        // lookup with the same def_id may yield a different result.
        if !used_substs {
            tcx.extern_const_statics
               .borrow_mut()
               .insert(def_id, expr_ty.map(|(e, t)| (e.id, t)));
        }
        expr_ty
    }
}

fn inline_const_fn_from_external_crate<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                                 def_id: DefId)
                                                 -> Option<ast::NodeId> {
    match tcx.extern_const_fns.borrow().get(&def_id) {
        Some(&ast::DUMMY_NODE_ID) => return None,
        Some(&fn_id) => return Some(fn_id),
        None => {}
    }

    if !tcx.sess.cstore.is_const_fn(def_id) {
        tcx.extern_const_fns.borrow_mut().insert(def_id, ast::DUMMY_NODE_ID);
        return None;
    }

    let fn_id = tcx.sess.cstore.maybe_get_item_ast(tcx, def_id).map(|t| t.1);
    tcx.extern_const_fns.borrow_mut().insert(def_id,
                                             fn_id.unwrap_or(ast::DUMMY_NODE_ID));
    fn_id
}

pub enum ConstFnNode<'tcx> {
    Local(FnLikeNode<'tcx>),
    Inlined(&'tcx InlinedItem)
}

pub fn lookup_const_fn_by_id<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, def_id: DefId)
                                       -> Option<ConstFnNode<'tcx>>
{
    let fn_id = if let Some(node_id) = tcx.map.as_local_node_id(def_id) {
        node_id
    } else {
        if let Some(fn_id) = inline_const_fn_from_external_crate(tcx, def_id) {
            if let ast_map::NodeInlinedItem(ii) = tcx.map.get(fn_id) {
                return Some(ConstFnNode::Inlined(ii));
            } else {
                bug!("Got const fn from external crate, but it's not inlined")
            }
        } else {
            return None;
        }
    };

    let fn_like = match FnLikeNode::from_node(tcx.map.get(fn_id)) {
        Some(fn_like) => fn_like,
        None => return None
    };

    if fn_like.constness() == hir::Constness::Const {
        Some(ConstFnNode::Local(fn_like))
    } else {
        None
    }
}

pub fn const_expr_to_pat<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                   expr: &Expr,
                                   pat_id: ast::NodeId,
                                   span: Span)
                                   -> Result<P<hir::Pat>, DefId> {
    let pat_ty = tcx.tables().expr_ty(expr);
    debug!("expr={:?} pat_ty={:?} pat_id={}", expr, pat_ty, pat_id);
    match pat_ty.sty {
        ty::TyFloat(_) => {
            tcx.sess.add_lint(
                lint::builtin::ILLEGAL_FLOATING_POINT_CONSTANT_PATTERN,
                pat_id,
                span,
                format!("floating point constants cannot be used in patterns"));
        }
        ty::TyAdt(adt_def, _) if adt_def.is_union() => {
            // Matching on union fields is unsafe, we can't hide it in constants
            tcx.sess.span_err(span, "cannot use unions in constant patterns");
        }
        ty::TyAdt(adt_def, _) => {
            if !tcx.has_attr(adt_def.did, "structural_match") {
                tcx.sess.add_lint(
                    lint::builtin::ILLEGAL_STRUCT_OR_ENUM_CONSTANT_PATTERN,
                    pat_id,
                    span,
                    format!("to use a constant of type `{}` \
                             in a pattern, \
                             `{}` must be annotated with `#[derive(PartialEq, Eq)]`",
                            tcx.item_path_str(adt_def.did),
                            tcx.item_path_str(adt_def.did)));
            }
        }
        _ => { }
    }
    let pat = match expr.node {
        hir::ExprTup(ref exprs) =>
            PatKind::Tuple(exprs.iter()
                                .map(|expr| const_expr_to_pat(tcx, &expr, pat_id, span))
                                .collect::<Result<_, _>>()?, None),

        hir::ExprCall(ref callee, ref args) => {
            let qpath = match callee.node {
                hir::ExprPath(ref qpath) => qpath,
                _ => bug!()
            };
            let def = tcx.tables().qpath_def(qpath, callee.id);
            let ctor_path = if let hir::QPath::Resolved(_, ref path) = *qpath {
                match def {
                    Def::StructCtor(_, CtorKind::Fn) |
                    Def::VariantCtor(_, CtorKind::Fn) => {
                        Some(path.clone())
                    }
                    _ => None
                }
            } else {
                None
            };
            match (def, ctor_path) {
                (Def::Fn(..), None) | (Def::Method(..), None) => {
                    PatKind::Lit(P(expr.clone()))
                }
                (_, Some(ctor_path)) => {
                    let pats = args.iter()
                                   .map(|expr| const_expr_to_pat(tcx, expr, pat_id, span))
                                   .collect::<Result<_, _>>()?;
                    PatKind::TupleStruct(hir::QPath::Resolved(None, ctor_path), pats, None)
                }
                _ => bug!()
            }
        }

        hir::ExprStruct(ref qpath, ref fields, None) => {
            let field_pats =
                fields.iter()
                      .map(|field| Ok(codemap::Spanned {
                          span: syntax_pos::DUMMY_SP,
                          node: hir::FieldPat {
                              name: field.name.node,
                              pat: const_expr_to_pat(tcx, &field.expr, pat_id, span)?,
                              is_shorthand: false,
                          },
                      }))
                      .collect::<Result<_, _>>()?;
            PatKind::Struct(qpath.clone(), field_pats, false)
        }

        hir::ExprArray(ref exprs) => {
            let pats = exprs.iter()
                            .map(|expr| const_expr_to_pat(tcx, &expr, pat_id, span))
                            .collect::<Result<_, _>>()?;
            PatKind::Slice(pats, None, hir::HirVec::new())
        }

        hir::ExprPath(ref qpath) => {
            let def = tcx.tables().qpath_def(qpath, expr.id);
            match def {
                Def::StructCtor(_, CtorKind::Const) |
                Def::VariantCtor(_, CtorKind::Const) => {
                    match expr.node {
                        hir::ExprPath(hir::QPath::Resolved(_, ref path)) => {
                            PatKind::Path(hir::QPath::Resolved(None, path.clone()))
                        }
                        _ => bug!()
                    }
                }
                Def::Const(def_id) | Def::AssociatedConst(def_id) => {
                    let substs = Some(tcx.tables().node_id_item_substs(expr.id)
                        .unwrap_or_else(|| tcx.intern_substs(&[])));
                    let (expr, _ty) = lookup_const_by_id(tcx, def_id, substs).unwrap();
                    return const_expr_to_pat(tcx, expr, pat_id, span);
                },
                _ => bug!(),
            }
        }

        _ => PatKind::Lit(P(expr.clone()))
    };
    Ok(P(hir::Pat { id: expr.id, node: pat, span: span }))
}

pub fn report_const_eval_err<'a, 'tcx>(
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    err: &ConstEvalErr,
    primary_span: Span,
    primary_kind: &str)
    -> DiagnosticBuilder<'tcx>
{
    let mut err = err;
    while let &ConstEvalErr { kind: ErroneousReferencedConstant(box ref i_err), .. } = err {
        err = i_err;
    }

    let mut diag = struct_span_err!(tcx.sess, err.span, E0080, "constant evaluation error");
    note_const_eval_err(tcx, err, primary_span, primary_kind, &mut diag);
    diag
}

pub fn fatal_const_eval_err<'a, 'tcx>(
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    err: &ConstEvalErr,
    primary_span: Span,
    primary_kind: &str)
    -> !
{
    report_const_eval_err(tcx, err, primary_span, primary_kind).emit();
    tcx.sess.abort_if_errors();
    unreachable!()
}

pub fn note_const_eval_err<'a, 'tcx>(
    _tcx: TyCtxt<'a, 'tcx, 'tcx>,
    err: &ConstEvalErr,
    primary_span: Span,
    primary_kind: &str,
    diag: &mut DiagnosticBuilder)
{
    match err.description() {
        ConstEvalErrDescription::Simple(message) => {
            diag.span_label(err.span, &message);
        }
    }

    if !primary_span.contains(err.span) {
        diag.span_note(primary_span,
                       &format!("for {} here", primary_kind));
    }
}

pub fn eval_const_expr<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                 e: &Expr) -> ConstVal {
    match eval_const_expr_checked(tcx, e) {
        Ok(r) => r,
        // non-const path still needs to be a fatal error, because enums are funky
        Err(s) => {
            report_const_eval_err(tcx, &s, e.span, "expression").emit();
            match s.kind {
                NonConstPath |
                UnimplementedConstVal(_) => tcx.sess.abort_if_errors(),
                _ => {}
            }
            Dummy
        },
    }
}

pub fn eval_const_expr_checked<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                         e: &Expr) -> EvalResult
{
    eval_const_expr_partial(tcx, e, ExprTypeChecked, None)
}

pub type FnArgMap<'a> = Option<&'a DefIdMap<ConstVal>>;

#[derive(Clone, Debug)]
pub struct ConstEvalErr {
    pub span: Span,
    pub kind: ErrKind,
}

#[derive(Clone, Debug)]
pub enum ErrKind {
    CannotCast,
    CannotCastTo(&'static str),
    InvalidOpForInts(hir::BinOp_),
    InvalidOpForBools(hir::BinOp_),
    InvalidOpForFloats(hir::BinOp_),
    InvalidOpForIntUint(hir::BinOp_),
    InvalidOpForUintInt(hir::BinOp_),
    NegateOn(ConstVal),
    NotOn(ConstVal),
    CallOn(ConstVal),

    MissingStructField,
    NonConstPath,
    UnimplementedConstVal(&'static str),
    UnresolvedPath,
    ExpectedConstTuple,
    ExpectedConstStruct,
    TupleIndexOutOfBounds,
    IndexedNonVec,
    IndexNegative,
    IndexNotInt,
    IndexOutOfBounds { len: u64, index: u64 },
    RepeatCountNotNatural,
    RepeatCountNotInt,

    MiscBinaryOp,
    MiscCatchAll,

    IndexOpFeatureGated,
    Math(ConstMathErr),

    IntermediateUnsignedNegative,
    /// Expected, Got
    TypeMismatch(String, ConstInt),

    BadType(ConstVal),
    ErroneousReferencedConstant(Box<ConstEvalErr>),
    CharCast(ConstInt),
}

impl From<ConstMathErr> for ErrKind {
    fn from(err: ConstMathErr) -> ErrKind {
        Math(err)
    }
}

#[derive(Clone, Debug)]
pub enum ConstEvalErrDescription<'a> {
    Simple(Cow<'a, str>),
}

impl<'a> ConstEvalErrDescription<'a> {
    /// Return a one-line description of the error, for lints and such
    pub fn into_oneline(self) -> Cow<'a, str> {
        match self {
            ConstEvalErrDescription::Simple(simple) => simple,
        }
    }
}

impl ConstEvalErr {
    pub fn description(&self) -> ConstEvalErrDescription {
        use self::ErrKind::*;
        use self::ConstEvalErrDescription::*;

        macro_rules! simple {
            ($msg:expr) => ({ Simple($msg.into_cow()) });
            ($fmt:expr, $($arg:tt)+) => ({
                Simple(format!($fmt, $($arg)+).into_cow())
            })
        }

        match self.kind {
            CannotCast => simple!("can't cast this type"),
            CannotCastTo(s) => simple!("can't cast this type to {}", s),
            InvalidOpForInts(_) =>  simple!("can't do this op on integrals"),
            InvalidOpForBools(_) =>  simple!("can't do this op on bools"),
            InvalidOpForFloats(_) => simple!("can't do this op on floats"),
            InvalidOpForIntUint(..) => simple!("can't do this op on an isize and usize"),
            InvalidOpForUintInt(..) => simple!("can't do this op on a usize and isize"),
            NegateOn(ref const_val) => simple!("negate on {}", const_val.description()),
            NotOn(ref const_val) => simple!("not on {}", const_val.description()),
            CallOn(ref const_val) => simple!("call on {}", const_val.description()),

            MissingStructField  => simple!("nonexistent struct field"),
            NonConstPath        => simple!("non-constant path in constant expression"),
            UnimplementedConstVal(what) =>
                simple!("unimplemented constant expression: {}", what),
            UnresolvedPath => simple!("unresolved path in constant expression"),
            ExpectedConstTuple => simple!("expected constant tuple"),
            ExpectedConstStruct => simple!("expected constant struct"),
            TupleIndexOutOfBounds => simple!("tuple index out of bounds"),
            IndexedNonVec => simple!("indexing is only supported for arrays"),
            IndexNegative => simple!("indices must be non-negative integers"),
            IndexNotInt => simple!("indices must be integers"),
            IndexOutOfBounds { len, index } => {
                simple!("index out of bounds: the len is {} but the index is {}",
                        len, index)
            }
            RepeatCountNotNatural => simple!("repeat count must be a natural number"),
            RepeatCountNotInt => simple!("repeat count must be integers"),

            MiscBinaryOp => simple!("bad operands for binary"),
            MiscCatchAll => simple!("unsupported constant expr"),
            IndexOpFeatureGated => simple!("the index operation on const values is unstable"),
            Math(ref err) => Simple(err.description().into_cow()),

            IntermediateUnsignedNegative => simple!(
                "during the computation of an unsigned a negative \
                 number was encountered. This is most likely a bug in\
                 the constant evaluator"),

            TypeMismatch(ref expected, ref got) => {
                simple!("expected {}, found {}", expected, got.description())
            },
            BadType(ref i) => simple!("value of wrong type: {:?}", i),
            ErroneousReferencedConstant(_) => simple!("could not evaluate referenced constant"),
            CharCast(ref got) => {
                simple!("only `u8` can be cast as `char`, not `{}`", got.description())
            },
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

impl<'tcx> EvalHint<'tcx> {
    fn erase_hint(&self) -> EvalHint<'tcx> {
        match *self {
            ExprTypeChecked => ExprTypeChecked,
            UncheckedExprHint(_) | UncheckedExprNoHint => UncheckedExprNoHint,
        }
    }
    fn checked_or(&self, ty: Ty<'tcx>) -> EvalHint<'tcx> {
        match *self {
            ExprTypeChecked => ExprTypeChecked,
            _ => UncheckedExprHint(ty),
        }
    }
}

macro_rules! signal {
    ($e:expr, $exn:expr) => {
        return Err(ConstEvalErr { span: $e.span, kind: $exn })
    }
}

/// Evaluate a constant expression in a context where the expression isn't
/// guaranteed to be evaluatable. `ty_hint` is usually ExprTypeChecked,
/// but a few places need to evaluate constants during type-checking, like
/// computing the length of an array. (See also the FIXME above EvalHint.)
pub fn eval_const_expr_partial<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                         e: &Expr,
                                         ty_hint: EvalHint<'tcx>,
                                         fn_args: FnArgMap) -> EvalResult {
    // Try to compute the type of the expression based on the EvalHint.
    // (See also the definition of EvalHint, and the FIXME above EvalHint.)
    let ety = match ty_hint {
        ExprTypeChecked => {
            // After type-checking, expr_ty is guaranteed to succeed.
            Some(tcx.tables().expr_ty(e))
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
            tcx.tables().expr_ty_opt(e)
        }
    };
    let result = match e.node {
      hir::ExprUnary(hir::UnNeg, ref inner) => {
        // unary neg literals already got their sign during creation
        if let hir::ExprLit(ref lit) = inner.node {
            use syntax::ast::*;
            use syntax::ast::LitIntType::*;
            const I8_OVERFLOW: u64 = ::std::i8::MAX as u64 + 1;
            const I16_OVERFLOW: u64 = ::std::i16::MAX as u64 + 1;
            const I32_OVERFLOW: u64 = ::std::i32::MAX as u64 + 1;
            const I64_OVERFLOW: u64 = ::std::i64::MAX as u64 + 1;
            match (&lit.node, ety.map(|t| &t.sty)) {
                (&LitKind::Int(I8_OVERFLOW, Unsuffixed), Some(&ty::TyInt(IntTy::I8))) |
                (&LitKind::Int(I8_OVERFLOW, Signed(IntTy::I8)), _) => {
                    return Ok(Integral(I8(::std::i8::MIN)))
                },
                (&LitKind::Int(I16_OVERFLOW, Unsuffixed), Some(&ty::TyInt(IntTy::I16))) |
                (&LitKind::Int(I16_OVERFLOW, Signed(IntTy::I16)), _) => {
                    return Ok(Integral(I16(::std::i16::MIN)))
                },
                (&LitKind::Int(I32_OVERFLOW, Unsuffixed), Some(&ty::TyInt(IntTy::I32))) |
                (&LitKind::Int(I32_OVERFLOW, Signed(IntTy::I32)), _) => {
                    return Ok(Integral(I32(::std::i32::MIN)))
                },
                (&LitKind::Int(I64_OVERFLOW, Unsuffixed), Some(&ty::TyInt(IntTy::I64))) |
                (&LitKind::Int(I64_OVERFLOW, Signed(IntTy::I64)), _) => {
                    return Ok(Integral(I64(::std::i64::MIN)))
                },
                (&LitKind::Int(n, Unsuffixed), Some(&ty::TyInt(IntTy::Is))) |
                (&LitKind::Int(n, Signed(IntTy::Is)), _) => {
                    match tcx.sess.target.int_type {
                        IntTy::I16 => if n == I16_OVERFLOW {
                            return Ok(Integral(Isize(Is16(::std::i16::MIN))));
                        },
                        IntTy::I32 => if n == I32_OVERFLOW {
                            return Ok(Integral(Isize(Is32(::std::i32::MIN))));
                        },
                        IntTy::I64 => if n == I64_OVERFLOW {
                            return Ok(Integral(Isize(Is64(::std::i64::MIN))));
                        },
                        _ => bug!(),
                    }
                },
                _ => {},
            }
        }
        match eval_const_expr_partial(tcx, &inner, ty_hint, fn_args)? {
          Float(f) => Float(-f),
          Integral(i) => Integral(math!(e, -i)),
          const_val => signal!(e, NegateOn(const_val)),
        }
      }
      hir::ExprUnary(hir::UnNot, ref inner) => {
        match eval_const_expr_partial(tcx, &inner, ty_hint, fn_args)? {
          Integral(i) => Integral(math!(e, !i)),
          Bool(b) => Bool(!b),
          const_val => signal!(e, NotOn(const_val)),
        }
      }
      hir::ExprUnary(hir::UnDeref, _) => signal!(e, UnimplementedConstVal("deref operation")),
      hir::ExprBinary(op, ref a, ref b) => {
        let b_ty = match op.node {
            hir::BiShl | hir::BiShr => ty_hint.erase_hint(),
            _ => ty_hint
        };
        // technically, if we don't have type hints, but integral eval
        // gives us a type through a type-suffix, cast or const def type
        // we need to re-eval the other value of the BinOp if it was
        // not inferred
        match (eval_const_expr_partial(tcx, &a, ty_hint, fn_args)?,
               eval_const_expr_partial(tcx, &b, b_ty, fn_args)?) {
          (Float(a), Float(b)) => {
            use std::cmp::Ordering::*;
            match op.node {
              hir::BiAdd => Float(math!(e, a + b)),
              hir::BiSub => Float(math!(e, a - b)),
              hir::BiMul => Float(math!(e, a * b)),
              hir::BiDiv => Float(math!(e, a / b)),
              hir::BiRem => Float(math!(e, a % b)),
              hir::BiEq => Bool(math!(e, a.try_cmp(b)) == Equal),
              hir::BiLt => Bool(math!(e, a.try_cmp(b)) == Less),
              hir::BiLe => Bool(math!(e, a.try_cmp(b)) != Greater),
              hir::BiNe => Bool(math!(e, a.try_cmp(b)) != Equal),
              hir::BiGe => Bool(math!(e, a.try_cmp(b)) != Less),
              hir::BiGt => Bool(math!(e, a.try_cmp(b)) == Greater),
              _ => signal!(e, InvalidOpForFloats(op.node)),
            }
          }
          (Integral(a), Integral(b)) => {
            use std::cmp::Ordering::*;
            match op.node {
              hir::BiAdd => Integral(math!(e, a + b)),
              hir::BiSub => Integral(math!(e, a - b)),
              hir::BiMul => Integral(math!(e, a * b)),
              hir::BiDiv => Integral(math!(e, a / b)),
              hir::BiRem => Integral(math!(e, a % b)),
              hir::BiBitAnd => Integral(math!(e, a & b)),
              hir::BiBitOr => Integral(math!(e, a | b)),
              hir::BiBitXor => Integral(math!(e, a ^ b)),
              hir::BiShl => Integral(math!(e, a << b)),
              hir::BiShr => Integral(math!(e, a >> b)),
              hir::BiEq => Bool(math!(e, a.try_cmp(b)) == Equal),
              hir::BiLt => Bool(math!(e, a.try_cmp(b)) == Less),
              hir::BiLe => Bool(math!(e, a.try_cmp(b)) != Greater),
              hir::BiNe => Bool(math!(e, a.try_cmp(b)) != Equal),
              hir::BiGe => Bool(math!(e, a.try_cmp(b)) != Less),
              hir::BiGt => Bool(math!(e, a.try_cmp(b)) == Greater),
              _ => signal!(e, InvalidOpForInts(op.node)),
            }
          }
          (Bool(a), Bool(b)) => {
            Bool(match op.node {
              hir::BiAnd => a && b,
              hir::BiOr => a || b,
              hir::BiBitXor => a ^ b,
              hir::BiBitAnd => a & b,
              hir::BiBitOr => a | b,
              hir::BiEq => a == b,
              hir::BiNe => a != b,
              hir::BiLt => a < b,
              hir::BiLe => a <= b,
              hir::BiGe => a >= b,
              hir::BiGt => a > b,
              _ => signal!(e, InvalidOpForBools(op.node)),
             })
          }

          _ => signal!(e, MiscBinaryOp),
        }
      }
      hir::ExprCast(ref base, ref target_ty) => {
        let ety = tcx.ast_ty_to_prim_ty(&target_ty).or(ety)
                .unwrap_or_else(|| {
                    tcx.sess.span_fatal(target_ty.span,
                                        "target type not found for const cast")
                });

        let base_hint = if let ExprTypeChecked = ty_hint {
            ExprTypeChecked
        } else {
            match tcx.tables().expr_ty_opt(&base) {
                Some(t) => UncheckedExprHint(t),
                None => ty_hint
            }
        };

        let val = match eval_const_expr_partial(tcx, &base, base_hint, fn_args) {
            Ok(val) => val,
            Err(ConstEvalErr { kind: ErroneousReferencedConstant(
                box ConstEvalErr { kind: TypeMismatch(_, val), .. }), .. }) |
            Err(ConstEvalErr { kind: TypeMismatch(_, val), .. }) => {
                // Something like `5i8 as usize` doesn't need a type hint for the base
                // instead take the type hint from the inner value
                let hint = match val.int_type() {
                    Some(IntType::UnsignedInt(ty)) => ty_hint.checked_or(tcx.mk_mach_uint(ty)),
                    Some(IntType::SignedInt(ty)) => ty_hint.checked_or(tcx.mk_mach_int(ty)),
                    // we had a type hint, so we can't have an unknown type
                    None => bug!(),
                };
                eval_const_expr_partial(tcx, &base, hint, fn_args)?
            },
            Err(e) => return Err(e),
        };
        match cast_const(tcx, val, ety) {
            Ok(val) => val,
            Err(kind) => return Err(ConstEvalErr { span: e.span, kind: kind }),
        }
      }
      hir::ExprPath(ref qpath) => {
          let def = tcx.tables().qpath_def(qpath, e.id);
          match def {
              Def::Const(def_id) |
              Def::AssociatedConst(def_id) => {
                  let substs = if let ExprTypeChecked = ty_hint {
                      Some(tcx.tables().node_id_item_substs(e.id)
                        .unwrap_or_else(|| tcx.intern_substs(&[])))
                  } else {
                      None
                  };
                  if let Some((expr, ty)) = lookup_const_by_id(tcx, def_id, substs) {
                      let item_hint = match ty {
                          Some(ty) => ty_hint.checked_or(ty),
                          None => ty_hint,
                      };
                      match eval_const_expr_partial(tcx, expr, item_hint, None) {
                          Ok(val) => val,
                          Err(err) => {
                              debug!("bad reference: {:?}, {:?}", err.description(), err.span);
                              signal!(e, ErroneousReferencedConstant(box err))
                          },
                      }
                  } else {
                      signal!(e, NonConstPath);
                  }
              },
              Def::VariantCtor(variant_def, ..) => {
                  if let Some(const_expr) = lookup_variant_by_id(tcx, variant_def) {
                      match eval_const_expr_partial(tcx, const_expr, ty_hint, None) {
                          Ok(val) => val,
                          Err(err) => {
                              debug!("bad reference: {:?}, {:?}", err.description(), err.span);
                              signal!(e, ErroneousReferencedConstant(box err))
                          },
                      }
                  } else {
                      signal!(e, UnimplementedConstVal("enum variants"));
                  }
              }
              Def::StructCtor(..) => {
                  ConstVal::Struct(e.id)
              }
              Def::Local(def_id) => {
                  debug!("Def::Local({:?}): {:?}", def_id, fn_args);
                  if let Some(val) = fn_args.and_then(|args| args.get(&def_id)) {
                      val.clone()
                  } else {
                      signal!(e, NonConstPath);
                  }
              },
              Def::Method(id) | Def::Fn(id) => Function(id),
              Def::Err => signal!(e, UnresolvedPath),
              _ => signal!(e, NonConstPath),
          }
      }
      hir::ExprCall(ref callee, ref args) => {
          let sub_ty_hint = ty_hint.erase_hint();
          let callee_val = eval_const_expr_partial(tcx, callee, sub_ty_hint, fn_args)?;
          let did = match callee_val {
              Function(did) => did,
              Struct(_) => signal!(e, UnimplementedConstVal("tuple struct constructors")),
              callee => signal!(e, CallOn(callee)),
          };
          let (arg_defs, body_id) = match lookup_const_fn_by_id(tcx, did) {
              Some(ConstFnNode::Inlined(ii)) => (ii.const_fn_args.clone(), ii.body.expr_id()),
              Some(ConstFnNode::Local(fn_like)) =>
                  (fn_like.decl().inputs.iter()
                   .map(|arg| match arg.pat.node {
                       hir::PatKind::Binding(_, def_id, _, _) => Some(def_id),
                       _ => None
                   }).collect(),
                   fn_like.body()),
              None => signal!(e, NonConstPath),
          };
          let result = tcx.map.expr(body_id);
          assert_eq!(arg_defs.len(), args.len());

          let mut call_args = DefIdMap();
          for (arg, arg_expr) in arg_defs.into_iter().zip(args.iter()) {
              let arg_hint = ty_hint.erase_hint();
              let arg_val = eval_const_expr_partial(
                  tcx,
                  arg_expr,
                  arg_hint,
                  fn_args
              )?;
              debug!("const call arg: {:?}", arg);
              if let Some(def_id) = arg {
                assert!(call_args.insert(def_id, arg_val).is_none());
              }
          }
          debug!("const call({:?})", call_args);
          eval_const_expr_partial(tcx, &result, ty_hint, Some(&call_args))?
      },
      hir::ExprLit(ref lit) => match lit_to_const(&lit.node, tcx, ety) {
          Ok(val) => val,
          Err(err) => signal!(e, err),
      },
      hir::ExprBlock(ref block) => {
        match block.expr {
            Some(ref expr) => eval_const_expr_partial(tcx, &expr, ty_hint, fn_args)?,
            None => signal!(e, UnimplementedConstVal("empty block")),
        }
      }
      hir::ExprType(ref e, _) => eval_const_expr_partial(tcx, &e, ty_hint, fn_args)?,
      hir::ExprTup(_) => Tuple(e.id),
      hir::ExprStruct(..) => Struct(e.id),
      hir::ExprIndex(ref arr, ref idx) => {
        if !tcx.sess.features.borrow().const_indexing {
            signal!(e, IndexOpFeatureGated);
        }
        let arr_hint = ty_hint.erase_hint();
        let arr = eval_const_expr_partial(tcx, arr, arr_hint, fn_args)?;
        let idx_hint = ty_hint.checked_or(tcx.types.usize);
        let idx = match eval_const_expr_partial(tcx, idx, idx_hint, fn_args)? {
            Integral(Usize(i)) => i.as_u64(tcx.sess.target.uint_type),
            Integral(_) => bug!(),
            _ => signal!(idx, IndexNotInt),
        };
        assert_eq!(idx as usize as u64, idx);
        match arr {
            Array(_, n) if idx >= n => {
                signal!(e, IndexOutOfBounds { len: n, index: idx })
            }
            Array(v, n) => if let hir::ExprArray(ref v) = tcx.map.expect_expr(v).node {
                assert_eq!(n as usize as u64, n);
                eval_const_expr_partial(tcx, &v[idx as usize], ty_hint, fn_args)?
            } else {
                bug!()
            },

            Repeat(_, n) if idx >= n => {
                signal!(e, IndexOutOfBounds { len: n, index: idx })
            }
            Repeat(elem, _) => eval_const_expr_partial(
                tcx,
                &tcx.map.expect_expr(elem),
                ty_hint,
                fn_args,
            )?,

            ByteStr(ref data) if idx >= data.len() as u64 => {
                signal!(e, IndexOutOfBounds { len: data.len() as u64, index: idx })
            }
            ByteStr(data) => {
                Integral(U8(data[idx as usize]))
            },

            _ => signal!(e, IndexedNonVec),
        }
      }
      hir::ExprArray(ref v) => Array(e.id, v.len() as u64),
      hir::ExprRepeat(_, ref n) => {
          let len_hint = ty_hint.checked_or(tcx.types.usize);
          Repeat(
              e.id,
              match eval_const_expr_partial(tcx, &n, len_hint, fn_args)? {
                  Integral(Usize(i)) => i.as_u64(tcx.sess.target.uint_type),
                  Integral(_) => signal!(e, RepeatCountNotNatural),
                  _ => signal!(e, RepeatCountNotInt),
              },
          )
      },
      hir::ExprTupField(ref base, index) => {
        let base_hint = ty_hint.erase_hint();
        let c = eval_const_expr_partial(tcx, base, base_hint, fn_args)?;
        if let Tuple(tup_id) = c {
            if let hir::ExprTup(ref fields) = tcx.map.expect_expr(tup_id).node {
                if index.node < fields.len() {
                    eval_const_expr_partial(tcx, &fields[index.node], ty_hint, fn_args)?
                } else {
                    signal!(e, TupleIndexOutOfBounds);
                }
            } else {
                bug!()
            }
        } else {
            signal!(base, ExpectedConstTuple);
        }
      }
      hir::ExprField(ref base, field_name) => {
        let base_hint = ty_hint.erase_hint();
        // Get the base expression if it is a struct and it is constant
        let c = eval_const_expr_partial(tcx, base, base_hint, fn_args)?;
        if let Struct(struct_id) = c {
            if let hir::ExprStruct(_, ref fields, _) = tcx.map.expect_expr(struct_id).node {
                // Check that the given field exists and evaluate it
                // if the idents are compared run-pass/issue-19244 fails
                if let Some(f) = fields.iter().find(|f| f.name.node
                                                     == field_name.node) {
                    eval_const_expr_partial(tcx, &f.expr, ty_hint, fn_args)?
                } else {
                    signal!(e, MissingStructField);
                }
            } else {
                bug!()
            }
        } else {
            signal!(base, ExpectedConstStruct);
        }
      }
      hir::ExprAddrOf(..) => signal!(e, UnimplementedConstVal("address operator")),
      _ => signal!(e, MiscCatchAll)
    };

    match (ety.map(|t| &t.sty), result) {
        (Some(ref ty_hint), Integral(i)) => match infer(i, tcx, ty_hint) {
            Ok(inferred) => Ok(Integral(inferred)),
            Err(err) => signal!(e, err),
        },
        (_, result) => Ok(result),
    }
}

fn infer<'a, 'tcx>(i: ConstInt,
                   tcx: TyCtxt<'a, 'tcx, 'tcx>,
                   ty_hint: &ty::TypeVariants<'tcx>)
                   -> Result<ConstInt, ErrKind> {
    use syntax::ast::*;

    match (ty_hint, i) {
        (&ty::TyInt(IntTy::I8), result @ I8(_)) => Ok(result),
        (&ty::TyInt(IntTy::I16), result @ I16(_)) => Ok(result),
        (&ty::TyInt(IntTy::I32), result @ I32(_)) => Ok(result),
        (&ty::TyInt(IntTy::I64), result @ I64(_)) => Ok(result),
        (&ty::TyInt(IntTy::Is), result @ Isize(_)) => Ok(result),

        (&ty::TyUint(UintTy::U8), result @ U8(_)) => Ok(result),
        (&ty::TyUint(UintTy::U16), result @ U16(_)) => Ok(result),
        (&ty::TyUint(UintTy::U32), result @ U32(_)) => Ok(result),
        (&ty::TyUint(UintTy::U64), result @ U64(_)) => Ok(result),
        (&ty::TyUint(UintTy::Us), result @ Usize(_)) => Ok(result),

        (&ty::TyInt(IntTy::I8), Infer(i)) => Ok(I8(i as i64 as i8)),
        (&ty::TyInt(IntTy::I16), Infer(i)) => Ok(I16(i as i64 as i16)),
        (&ty::TyInt(IntTy::I32), Infer(i)) => Ok(I32(i as i64 as i32)),
        (&ty::TyInt(IntTy::I64), Infer(i)) => Ok(I64(i as i64)),
        (&ty::TyInt(IntTy::Is), Infer(i)) => {
            Ok(Isize(ConstIsize::new_truncating(i as i64, tcx.sess.target.int_type)))
        },

        (&ty::TyInt(IntTy::I8), InferSigned(i)) => Ok(I8(i as i8)),
        (&ty::TyInt(IntTy::I16), InferSigned(i)) => Ok(I16(i as i16)),
        (&ty::TyInt(IntTy::I32), InferSigned(i)) => Ok(I32(i as i32)),
        (&ty::TyInt(IntTy::I64), InferSigned(i)) => Ok(I64(i)),
        (&ty::TyInt(IntTy::Is), InferSigned(i)) => {
            Ok(Isize(ConstIsize::new_truncating(i, tcx.sess.target.int_type)))
        },

        (&ty::TyUint(UintTy::U8), Infer(i)) => Ok(U8(i as u8)),
        (&ty::TyUint(UintTy::U16), Infer(i)) => Ok(U16(i as u16)),
        (&ty::TyUint(UintTy::U32), Infer(i)) => Ok(U32(i as u32)),
        (&ty::TyUint(UintTy::U64), Infer(i)) => Ok(U64(i)),
        (&ty::TyUint(UintTy::Us), Infer(i)) => {
            Ok(Usize(ConstUsize::new_truncating(i, tcx.sess.target.uint_type)))
        },
        (&ty::TyUint(_), InferSigned(_)) => Err(IntermediateUnsignedNegative),

        (&ty::TyInt(ity), i) => Err(TypeMismatch(ity.to_string(), i)),
        (&ty::TyUint(ity), i) => Err(TypeMismatch(ity.to_string(), i)),

        (&ty::TyAdt(adt, _), i) if adt.is_enum() => {
            let hints = tcx.lookup_repr_hints(adt.did);
            let int_ty = tcx.enum_repr_type(hints.iter().next());
            infer(i, tcx, &int_ty.to_ty(tcx).sty)
        },
        (_, i) => Err(BadType(ConstVal::Integral(i))),
    }
}

fn resolve_trait_associated_const<'a, 'tcx: 'a>(
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    trait_item_id: DefId,
    default_value: Option<(&'tcx Expr, Option<ty::Ty<'tcx>>)>,
    trait_id: DefId,
    rcvr_substs: &'tcx Substs<'tcx>
) -> Option<(&'tcx Expr, Option<ty::Ty<'tcx>>)>
{
    let trait_ref = ty::Binder(ty::TraitRef::new(trait_id, rcvr_substs));
    debug!("resolve_trait_associated_const: trait_ref={:?}",
           trait_ref);

    tcx.populate_implementations_for_trait_if_necessary(trait_id);
    tcx.infer_ctxt(None, None, Reveal::NotSpecializable).enter(|infcx| {
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
            Err(_) => {
                return None
            }
        };

        // NOTE: this code does not currently account for specialization, but when
        // it does so, it should hook into the Reveal to determine when the
        // constant should resolve; this will also require plumbing through to this
        // function whether we are in "trans mode" to pick the right Reveal
        // when constructing the inference context above.
        match selection {
            traits::VtableImpl(ref impl_data) => {
                let name = tcx.associated_item(trait_item_id).name;
                let ac = tcx.associated_items(impl_data.impl_def_id)
                    .find(|item| item.kind == ty::AssociatedKind::Const && item.name == name);
                match ac {
                    Some(ic) => lookup_const_by_id(tcx, ic.def_id, None),
                    None => default_value,
                }
            }
            _ => {
                bug!("resolve_trait_associated_const: unexpected vtable type")
            }
        }
    })
}

fn cast_const_int<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, val: ConstInt, ty: ty::Ty) -> CastResult {
    let v = val.to_u64_unchecked();
    match ty.sty {
        ty::TyBool if v == 0 => Ok(Bool(false)),
        ty::TyBool if v == 1 => Ok(Bool(true)),
        ty::TyInt(ast::IntTy::I8) => Ok(Integral(I8(v as i64 as i8))),
        ty::TyInt(ast::IntTy::I16) => Ok(Integral(I16(v as i64 as i16))),
        ty::TyInt(ast::IntTy::I32) => Ok(Integral(I32(v as i64 as i32))),
        ty::TyInt(ast::IntTy::I64) => Ok(Integral(I64(v as i64))),
        ty::TyInt(ast::IntTy::Is) => {
            Ok(Integral(Isize(ConstIsize::new_truncating(v as i64, tcx.sess.target.int_type))))
        },
        ty::TyUint(ast::UintTy::U8) => Ok(Integral(U8(v as u8))),
        ty::TyUint(ast::UintTy::U16) => Ok(Integral(U16(v as u16))),
        ty::TyUint(ast::UintTy::U32) => Ok(Integral(U32(v as u32))),
        ty::TyUint(ast::UintTy::U64) => Ok(Integral(U64(v))),
        ty::TyUint(ast::UintTy::Us) => {
            Ok(Integral(Usize(ConstUsize::new_truncating(v, tcx.sess.target.uint_type))))
        },
        ty::TyFloat(ast::FloatTy::F64) => match val.erase_type() {
            Infer(u) => Ok(Float(F64(u as f64))),
            InferSigned(i) => Ok(Float(F64(i as f64))),
            _ => bug!("ConstInt::erase_type returned something other than Infer/InferSigned"),
        },
        ty::TyFloat(ast::FloatTy::F32) => match val.erase_type() {
            Infer(u) => Ok(Float(F32(u as f32))),
            InferSigned(i) => Ok(Float(F32(i as f32))),
            _ => bug!("ConstInt::erase_type returned something other than Infer/InferSigned"),
        },
        ty::TyRawPtr(_) => Err(ErrKind::UnimplementedConstVal("casting an address to a raw ptr")),
        ty::TyChar => match infer(val, tcx, &ty::TyUint(ast::UintTy::U8)) {
            Ok(U8(u)) => Ok(Char(u as char)),
            // can only occur before typeck, typeck blocks `T as char` for `T` != `u8`
            _ => Err(CharCast(val)),
        },
        _ => Err(CannotCast),
    }
}

fn cast_const_float<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                              val: ConstFloat,
                              ty: ty::Ty) -> CastResult {
    match ty.sty {
        ty::TyInt(_) | ty::TyUint(_) => {
            let i = match val {
                F32(f) if f >= 0.0 => Infer(f as u64),
                FInfer { f64: f, .. } |
                F64(f) if f >= 0.0 => Infer(f as u64),

                F32(f) => InferSigned(f as i64),
                FInfer { f64: f, .. } |
                F64(f) => InferSigned(f as i64)
            };

            if let (InferSigned(_), &ty::TyUint(_)) = (i, &ty.sty) {
                return Err(CannotCast);
            }

            cast_const_int(tcx, i, ty)
        }
        ty::TyFloat(ast::FloatTy::F64) => Ok(Float(F64(match val {
            F32(f) => f as f64,
            FInfer { f64: f, .. } | F64(f) => f
        }))),
        ty::TyFloat(ast::FloatTy::F32) => Ok(Float(F32(match val {
            F64(f) => f as f32,
            FInfer { f32: f, .. } | F32(f) => f
        }))),
        _ => Err(CannotCast),
    }
}

fn cast_const<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, val: ConstVal, ty: ty::Ty) -> CastResult {
    match val {
        Integral(i) => cast_const_int(tcx, i, ty),
        Bool(b) => cast_const_int(tcx, Infer(b as u64), ty),
        Float(f) => cast_const_float(tcx, f, ty),
        Char(c) => cast_const_int(tcx, Infer(c as u64), ty),
        Function(_) => Err(UnimplementedConstVal("casting fn pointers")),
        ByteStr(b) => match ty.sty {
            ty::TyRawPtr(_) => {
                Err(ErrKind::UnimplementedConstVal("casting a bytestr to a raw ptr"))
            },
            ty::TyRef(_, ty::TypeAndMut { ref ty, mutbl: hir::MutImmutable }) => match ty.sty {
                ty::TyArray(ty, n) if ty == tcx.types.u8 && n == b.len() => Ok(ByteStr(b)),
                ty::TySlice(_) => {
                    Err(ErrKind::UnimplementedConstVal("casting a bytestr to slice"))
                },
                _ => Err(CannotCast),
            },
            _ => Err(CannotCast),
        },
        Str(s) => match ty.sty {
            ty::TyRawPtr(_) => Err(ErrKind::UnimplementedConstVal("casting a str to a raw ptr")),
            ty::TyRef(_, ty::TypeAndMut { ref ty, mutbl: hir::MutImmutable }) => match ty.sty {
                ty::TyStr => Ok(Str(s)),
                _ => Err(CannotCast),
            },
            _ => Err(CannotCast),
        },
        _ => Err(CannotCast),
    }
}

fn lit_to_const<'a, 'tcx>(lit: &ast::LitKind,
                          tcx: TyCtxt<'a, 'tcx, 'tcx>,
                          ty_hint: Option<Ty<'tcx>>)
                          -> Result<ConstVal, ErrKind> {
    use syntax::ast::*;
    use syntax::ast::LitIntType::*;
    match *lit {
        LitKind::Str(ref s, _) => Ok(Str(s.as_str())),
        LitKind::ByteStr(ref data) => Ok(ByteStr(data.clone())),
        LitKind::Byte(n) => Ok(Integral(U8(n))),
        LitKind::Int(n, Signed(ity)) => {
            infer(InferSigned(n as i64), tcx, &ty::TyInt(ity)).map(Integral)
        },

        LitKind::Int(n, Unsuffixed) => {
            match ty_hint.map(|t| &t.sty) {
                Some(&ty::TyInt(ity)) => {
                    infer(InferSigned(n as i64), tcx, &ty::TyInt(ity)).map(Integral)
                },
                Some(&ty::TyUint(uty)) => {
                    infer(Infer(n), tcx, &ty::TyUint(uty)).map(Integral)
                },
                None => Ok(Integral(Infer(n))),
                Some(&ty::TyAdt(adt, _)) => {
                    let hints = tcx.lookup_repr_hints(adt.did);
                    let int_ty = tcx.enum_repr_type(hints.iter().next());
                    infer(Infer(n), tcx, &int_ty.to_ty(tcx).sty).map(Integral)
                },
                Some(ty_hint) => bug!("bad ty_hint: {:?}, {:?}", ty_hint, lit),
            }
        },
        LitKind::Int(n, Unsigned(ity)) => {
            infer(Infer(n), tcx, &ty::TyUint(ity)).map(Integral)
        },

        LitKind::Float(n, fty) => {
            parse_float(&n.as_str(), Some(fty)).map(Float)
        }
        LitKind::FloatUnsuffixed(n) => {
            let fty_hint = match ty_hint.map(|t| &t.sty) {
                Some(&ty::TyFloat(fty)) => Some(fty),
                _ => None
            };
            parse_float(&n.as_str(), fty_hint).map(Float)
        }
        LitKind::Bool(b) => Ok(Bool(b)),
        LitKind::Char(c) => Ok(Char(c)),
    }
}

fn parse_float(num: &str, fty_hint: Option<ast::FloatTy>)
               -> Result<ConstFloat, ErrKind> {
    let val = match fty_hint {
        Some(ast::FloatTy::F32) => num.parse::<f32>().map(F32),
        Some(ast::FloatTy::F64) => num.parse::<f64>().map(F64),
        None => {
            num.parse::<f32>().and_then(|f32| {
                num.parse::<f64>().map(|f64| {
                    FInfer { f32: f32, f64: f64 }
                })
            })
        }
    };
    val.map_err(|_| {
        // FIXME(#31407) this is only necessary because float parsing is buggy
        UnimplementedConstVal("could not evaluate float literal (see issue #31407)")
    })
}

pub fn compare_const_vals(tcx: TyCtxt, span: Span, a: &ConstVal, b: &ConstVal)
                          -> Result<Ordering, ErrorReported>
{
    let result = match (a, b) {
        (&Integral(a), &Integral(b)) => a.try_cmp(b).ok(),
        (&Float(a), &Float(b)) => a.try_cmp(b).ok(),
        (&Str(ref a), &Str(ref b)) => Some(a.cmp(b)),
        (&Bool(a), &Bool(b)) => Some(a.cmp(&b)),
        (&ByteStr(ref a), &ByteStr(ref b)) => Some(a.cmp(b)),
        (&Char(a), &Char(ref b)) => Some(a.cmp(b)),
        _ => None,
    };

    match result {
        Some(result) => Ok(result),
        None => {
            // FIXME: can this ever be reached?
            span_err!(tcx.sess, span, E0298,
                      "type mismatch comparing {} and {}",
                      a.description(),
                      b.description());
            Err(ErrorReported)
        }
    }
}

pub fn compare_lit_exprs<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                   span: Span,
                                   a: &Expr,
                                   b: &Expr) -> Result<Ordering, ErrorReported> {
    let a = match eval_const_expr_partial(tcx, a, ExprTypeChecked, None) {
        Ok(a) => a,
        Err(e) => {
            report_const_eval_err(tcx, &e, a.span, "expression").emit();
            return Err(ErrorReported);
        }
    };
    let b = match eval_const_expr_partial(tcx, b, ExprTypeChecked, None) {
        Ok(b) => b,
        Err(e) => {
            report_const_eval_err(tcx, &e, b.span, "expression").emit();
            return Err(ErrorReported);
        }
    };
    compare_const_vals(tcx, span, &a, &b)
}


/// Returns the value of the length-valued expression
pub fn eval_length<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                             count_expr: &hir::Expr,
                             reason: &str)
                             -> Result<usize, ErrorReported>
{
    let hint = UncheckedExprHint(tcx.types.usize);
    match eval_const_expr_partial(tcx, count_expr, hint, None) {
        Ok(Integral(Usize(count))) => {
            let val = count.as_u64(tcx.sess.target.uint_type);
            assert_eq!(val as usize as u64, val);
            Ok(val as usize)
        },
        Ok(const_val) => {
            struct_span_err!(tcx.sess, count_expr.span, E0306,
                             "expected `usize` for {}, found {}",
                             reason,
                             const_val.description())
                .span_label(count_expr.span, &format!("expected `usize`"))
                .emit();

            Err(ErrorReported)
        }
        Err(err) => {
            let mut diag = report_const_eval_err(
                tcx, &err, count_expr.span, reason);

            if let hir::ExprPath(hir::QPath::Resolved(None, ref path)) = count_expr.node {
                if let Def::Local(..) = path.def {
                    diag.note(&format!("`{}` is a variable", path));
                }
            }

            diag.emit();
            Err(ErrorReported)
        }
    }
}
