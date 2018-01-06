// Copyright 2012-2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rustc::middle::const_val::ConstVal::*;
use rustc::middle::const_val::ConstAggregate::*;
use rustc::middle::const_val::ErrKind::*;
use rustc::middle::const_val::{ByteArray, ConstVal, ConstEvalErr, EvalResult, ErrKind};

use rustc::hir::map::blocks::FnLikeNode;
use rustc::hir::def::{Def, CtorKind};
use rustc::hir::def_id::DefId;
use rustc::ty::{self, Ty, TyCtxt};
use rustc::ty::layout::LayoutOf;
use rustc::ty::util::IntTypeExt;
use rustc::ty::subst::{Substs, Subst};
use rustc::util::common::ErrorReported;
use rustc::util::nodemap::NodeMap;

use syntax::abi::Abi;
use syntax::ast;
use syntax::attr;
use rustc::hir::{self, Expr};
use syntax_pos::Span;

use std::cmp::Ordering;

use rustc_const_math::*;
macro_rules! signal {
    ($e:expr, $exn:expr) => {
        return Err(ConstEvalErr { span: $e.span, kind: $exn })
    }
}

macro_rules! math {
    ($e:expr, $op:expr) => {
        match $op {
            Ok(val) => val,
            Err(e) => signal!($e, ErrKind::from(e)),
        }
    }
}

/// * `DefId` is the id of the constant.
/// * `Substs` is the monomorphized substitutions for the expression.
pub fn lookup_const_by_id<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                    key: ty::ParamEnvAnd<'tcx, (DefId, &'tcx Substs<'tcx>)>)
                                    -> Option<(DefId, &'tcx Substs<'tcx>)> {
    ty::Instance::resolve(
        tcx,
        key.param_env,
        key.value.0,
        key.value.1,
    ).map(|instance| (instance.def_id(), instance.substs))
}

pub struct ConstContext<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    tables: &'a ty::TypeckTables<'tcx>,
    param_env: ty::ParamEnv<'tcx>,
    substs: &'tcx Substs<'tcx>,
    fn_args: Option<NodeMap<&'tcx ty::Const<'tcx>>>
}

impl<'a, 'tcx> ConstContext<'a, 'tcx> {
    pub fn new(tcx: TyCtxt<'a, 'tcx, 'tcx>,
               param_env_and_substs: ty::ParamEnvAnd<'tcx, &'tcx Substs<'tcx>>,
               tables: &'a ty::TypeckTables<'tcx>)
               -> Self {
        ConstContext {
            tcx,
            param_env: param_env_and_substs.param_env,
            tables,
            substs: param_env_and_substs.value,
            fn_args: None
        }
    }

    /// Evaluate a constant expression in a context where the expression isn't
    /// guaranteed to be evaluable.
    pub fn eval(&self, e: &'tcx Expr) -> EvalResult<'tcx> {
        if self.tables.tainted_by_errors {
            signal!(e, TypeckError);
        }
        eval_const_expr_partial(self, e)
    }
}

type CastResult<'tcx> = Result<ConstVal<'tcx>, ErrKind<'tcx>>;

fn eval_const_expr_partial<'a, 'tcx>(cx: &ConstContext<'a, 'tcx>,
                                     e: &'tcx Expr) -> EvalResult<'tcx> {
    trace!("eval_const_expr_partial: {:?}", e);
    let tcx = cx.tcx;
    let ty = cx.tables.expr_ty(e).subst(tcx, cx.substs);
    let mk_const = |val| tcx.mk_const(ty::Const { val, ty });

    let result = match e.node {
      hir::ExprUnary(hir::UnNeg, ref inner) => {
        // unary neg literals already got their sign during creation
        if let hir::ExprLit(ref lit) = inner.node {
            use syntax::ast::*;
            use syntax::ast::LitIntType::*;
            const I8_OVERFLOW: u128 = i8::min_value() as u8 as u128;
            const I16_OVERFLOW: u128 = i16::min_value() as u16 as u128;
            const I32_OVERFLOW: u128 = i32::min_value() as u32 as u128;
            const I64_OVERFLOW: u128 = i64::min_value() as u64 as u128;
            const I128_OVERFLOW: u128 = i128::min_value() as u128;
            let negated = match (&lit.node, &ty.sty) {
                (&LitKind::Int(I8_OVERFLOW, _), &ty::TyInt(IntTy::I8)) |
                (&LitKind::Int(I8_OVERFLOW, Signed(IntTy::I8)), _) => {
                    Some(I8(i8::min_value()))
                },
                (&LitKind::Int(I16_OVERFLOW, _), &ty::TyInt(IntTy::I16)) |
                (&LitKind::Int(I16_OVERFLOW, Signed(IntTy::I16)), _) => {
                    Some(I16(i16::min_value()))
                },
                (&LitKind::Int(I32_OVERFLOW, _), &ty::TyInt(IntTy::I32)) |
                (&LitKind::Int(I32_OVERFLOW, Signed(IntTy::I32)), _) => {
                    Some(I32(i32::min_value()))
                },
                (&LitKind::Int(I64_OVERFLOW, _), &ty::TyInt(IntTy::I64)) |
                (&LitKind::Int(I64_OVERFLOW, Signed(IntTy::I64)), _) => {
                    Some(I64(i64::min_value()))
                },
                (&LitKind::Int(I128_OVERFLOW, _), &ty::TyInt(IntTy::I128)) |
                (&LitKind::Int(I128_OVERFLOW, Signed(IntTy::I128)), _) => {
                    Some(I128(i128::min_value()))
                },
                (&LitKind::Int(n, _), &ty::TyInt(IntTy::Isize)) |
                (&LitKind::Int(n, Signed(IntTy::Isize)), _) => {
                    match tcx.sess.target.isize_ty {
                        IntTy::I16 => if n == I16_OVERFLOW {
                            Some(Isize(Is16(i16::min_value())))
                        } else {
                            None
                        },
                        IntTy::I32 => if n == I32_OVERFLOW {
                            Some(Isize(Is32(i32::min_value())))
                        } else {
                            None
                        },
                        IntTy::I64 => if n == I64_OVERFLOW {
                            Some(Isize(Is64(i64::min_value())))
                        } else {
                            None
                        },
                        _ => span_bug!(e.span, "typeck error")
                    }
                },
                _ => None
            };
            if let Some(i) = negated {
                return Ok(mk_const(Integral(i)));
            }
        }
        mk_const(match cx.eval(inner)?.val {
          Float(f) => Float(-f),
          Integral(i) => Integral(math!(e, -i)),
          _ => signal!(e, TypeckError)
        })
      }
      hir::ExprUnary(hir::UnNot, ref inner) => {
        mk_const(match cx.eval(inner)?.val {
          Integral(i) => Integral(math!(e, !i)),
          Bool(b) => Bool(!b),
          _ => signal!(e, TypeckError)
        })
      }
      hir::ExprUnary(hir::UnDeref, _) => signal!(e, UnimplementedConstVal("deref operation")),
      hir::ExprBinary(op, ref a, ref b) => {
        // technically, if we don't have type hints, but integral eval
        // gives us a type through a type-suffix, cast or const def type
        // we need to re-eval the other value of the BinOp if it was
        // not inferred
        mk_const(match (cx.eval(a)?.val, cx.eval(b)?.val) {
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
              _ => span_bug!(e.span, "typeck error"),
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
              _ => span_bug!(e.span, "typeck error"),
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
              _ => span_bug!(e.span, "typeck error"),
             })
          }
          (Char(a), Char(b)) => {
            Bool(match op.node {
              hir::BiEq => a == b,
              hir::BiNe => a != b,
              hir::BiLt => a < b,
              hir::BiLe => a <= b,
              hir::BiGe => a >= b,
              hir::BiGt => a > b,
              _ => span_bug!(e.span, "typeck error"),
             })
          }

          _ => signal!(e, MiscBinaryOp),
        })
      }
      hir::ExprCast(ref base, _) => {
        let base_val = cx.eval(base)?;
        let base_ty = cx.tables.expr_ty(base).subst(tcx, cx.substs);
        if ty == base_ty {
            base_val
        } else {
            match cast_const(tcx, base_val.val, ty) {
                Ok(val) => mk_const(val),
                Err(kind) => signal!(e, kind),
            }
        }
      }
      hir::ExprPath(ref qpath) => {
        let substs = cx.tables.node_substs(e.hir_id).subst(tcx, cx.substs);
          match cx.tables.qpath_def(qpath, e.hir_id) {
              Def::Const(def_id) |
              Def::AssociatedConst(def_id) => {
                    let substs = tcx.normalize_associated_type_in_env(&substs, cx.param_env);
                    match tcx.at(e.span).const_eval(cx.param_env.and((def_id, substs))) {
                        Ok(val) => val,
                        Err(ConstEvalErr { kind: TypeckError, .. }) => {
                            signal!(e, TypeckError);
                        }
                        Err(err) => {
                            debug!("bad reference: {:?}, {:?}", err.description(), err.span);
                            signal!(e, ErroneousReferencedConstant(box err))
                        },
                    }
              },
              Def::VariantCtor(variant_def, CtorKind::Const) => {
                mk_const(Variant(variant_def))
              }
              Def::VariantCtor(_, CtorKind::Fn) => {
                  signal!(e, UnimplementedConstVal("enum variants"));
              }
              Def::StructCtor(_, CtorKind::Const) => {
                  mk_const(Aggregate(Struct(&[])))
              }
              Def::StructCtor(_, CtorKind::Fn) => {
                  signal!(e, UnimplementedConstVal("tuple struct constructors"))
              }
              Def::Local(id) => {
                  debug!("Def::Local({:?}): {:?}", id, cx.fn_args);
                  if let Some(&val) = cx.fn_args.as_ref().and_then(|args| args.get(&id)) {
                      val
                  } else {
                      signal!(e, NonConstPath);
                  }
              },
              Def::Method(id) | Def::Fn(id) => mk_const(Function(id, substs)),
              Def::Err => span_bug!(e.span, "typeck error"),
              _ => signal!(e, NonConstPath),
          }
      }
      hir::ExprCall(ref callee, ref args) => {
          let (def_id, substs) = match cx.eval(callee)?.val {
              Function(def_id, substs) => (def_id, substs),
              _ => signal!(e, TypeckError),
          };

          if tcx.fn_sig(def_id).abi() == Abi::RustIntrinsic {
            let layout_of = |ty: Ty<'tcx>| {
                let ty = tcx.erase_regions(&ty);
                (tcx.at(e.span), cx.param_env).layout_of(ty).map_err(|err| {
                    ConstEvalErr { span: e.span, kind: LayoutError(err) }
                })
            };
            match &tcx.item_name(def_id)[..] {
                "size_of" => {
                    let size = layout_of(substs.type_at(0))?.size.bytes();
                    return Ok(mk_const(Integral(Usize(ConstUsize::new(size,
                        tcx.sess.target.usize_ty).unwrap()))));
                }
                "min_align_of" => {
                    let align = layout_of(substs.type_at(0))?.align.abi();
                    return Ok(mk_const(Integral(Usize(ConstUsize::new(align,
                        tcx.sess.target.usize_ty).unwrap()))));
                }
                _ => signal!(e, TypeckError)
            }
          }

          let body = if let Some(node_id) = tcx.hir.as_local_node_id(def_id) {
            if let Some(fn_like) = FnLikeNode::from_node(tcx.hir.get(node_id)) {
                if fn_like.constness() == hir::Constness::Const {
                    tcx.hir.body(fn_like.body())
                } else {
                    signal!(e, TypeckError)
                }
            } else {
                signal!(e, TypeckError)
            }
          } else {
            if tcx.is_const_fn(def_id) {
                tcx.extern_const_body(def_id).body
            } else {
                signal!(e, TypeckError)
            }
          };

          let arg_ids = body.arguments.iter().map(|arg| match arg.pat.node {
               hir::PatKind::Binding(_, canonical_id, _, _) => Some(canonical_id),
               _ => None
           }).collect::<Vec<_>>();
          assert_eq!(arg_ids.len(), args.len());

          let mut call_args = NodeMap();
          for (arg, arg_expr) in arg_ids.into_iter().zip(args.iter()) {
              let arg_val = cx.eval(arg_expr)?;
              debug!("const call arg: {:?}", arg);
              if let Some(id) = arg {
                assert!(call_args.insert(id, arg_val).is_none());
              }
          }
          debug!("const call({:?})", call_args);
          let callee_cx = ConstContext {
            tcx,
            param_env: cx.param_env,
            tables: tcx.typeck_tables_of(def_id),
            substs,
            fn_args: Some(call_args)
          };
          callee_cx.eval(&body.value)?
      },
      hir::ExprLit(ref lit) => match lit_to_const(&lit.node, tcx, ty) {
          Ok(val) => mk_const(val),
          Err(err) => signal!(e, err),
      },
      hir::ExprBlock(ref block) => {
        match block.expr {
            Some(ref expr) => cx.eval(expr)?,
            None => mk_const(Aggregate(Tuple(&[]))),
        }
      }
      hir::ExprType(ref e, _) => cx.eval(e)?,
      hir::ExprTup(ref fields) => {
        let values = fields.iter().map(|e| cx.eval(e)).collect::<Result<Vec<_>, _>>()?;
        mk_const(Aggregate(Tuple(tcx.alloc_const_slice(&values))))
      }
      hir::ExprStruct(_, ref fields, _) => {
        mk_const(Aggregate(Struct(tcx.alloc_name_const_slice(&fields.iter().map(|f| {
            cx.eval(&f.expr).map(|v| (f.name.node, v))
        }).collect::<Result<Vec<_>, _>>()?))))
      }
      hir::ExprIndex(ref arr, ref idx) => {
        if !tcx.sess.features.borrow().const_indexing {
            signal!(e, IndexOpFeatureGated);
        }
        let arr = cx.eval(arr)?;
        let idx = match cx.eval(idx)?.val {
            Integral(Usize(i)) => i.as_u64(),
            _ => signal!(idx, IndexNotUsize),
        };
        assert_eq!(idx as usize as u64, idx);
        match arr.val {
            Aggregate(Array(v)) => {
                if let Some(&elem) = v.get(idx as usize) {
                    elem
                } else {
                    let n = v.len() as u64;
                    signal!(e, IndexOutOfBounds { len: n, index: idx })
                }
            }

            Aggregate(Repeat(.., n)) if idx >= n => {
                signal!(e, IndexOutOfBounds { len: n, index: idx })
            }
            Aggregate(Repeat(elem, _)) => elem,

            ByteStr(b) if idx >= b.data.len() as u64 => {
                signal!(e, IndexOutOfBounds { len: b.data.len() as u64, index: idx })
            }
            ByteStr(b) => {
                mk_const(Integral(U8(b.data[idx as usize])))
            },

            _ => signal!(e, IndexedNonVec),
        }
      }
      hir::ExprArray(ref v) => {
        let values = v.iter().map(|e| cx.eval(e)).collect::<Result<Vec<_>, _>>()?;
        mk_const(Aggregate(Array(tcx.alloc_const_slice(&values))))
      }
      hir::ExprRepeat(ref elem, _) => {
          let n = match ty.sty {
            ty::TyArray(_, n) => n.val.to_const_int().unwrap().to_u64().unwrap(),
            _ => span_bug!(e.span, "typeck error")
          };
          mk_const(Aggregate(Repeat(cx.eval(elem)?, n)))
      },
      hir::ExprTupField(ref base, index) => {
        if let Aggregate(Tuple(fields)) = cx.eval(base)?.val {
            fields[index.node]
        } else {
            signal!(base, ExpectedConstTuple);
        }
      }
      hir::ExprField(ref base, field_name) => {
        if let Aggregate(Struct(fields)) = cx.eval(base)?.val {
            if let Some(&(_, f)) = fields.iter().find(|&&(name, _)| name == field_name.node) {
                f
            } else {
                signal!(e, MissingStructField);
            }
        } else {
            signal!(base, ExpectedConstStruct);
        }
      }
      hir::ExprAddrOf(..) => signal!(e, UnimplementedConstVal("address operator")),
      _ => signal!(e, MiscCatchAll)
    };

    Ok(result)
}

fn cast_const_int<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                            val: ConstInt,
                            ty: Ty<'tcx>)
                            -> CastResult<'tcx> {
    let v = val.to_u128_unchecked();
    match ty.sty {
        ty::TyBool if v == 0 => Ok(Bool(false)),
        ty::TyBool if v == 1 => Ok(Bool(true)),
        ty::TyInt(ast::IntTy::I8) => Ok(Integral(I8(v as i128 as i8))),
        ty::TyInt(ast::IntTy::I16) => Ok(Integral(I16(v as i128 as i16))),
        ty::TyInt(ast::IntTy::I32) => Ok(Integral(I32(v as i128 as i32))),
        ty::TyInt(ast::IntTy::I64) => Ok(Integral(I64(v as i128 as i64))),
        ty::TyInt(ast::IntTy::I128) => Ok(Integral(I128(v as i128))),
        ty::TyInt(ast::IntTy::Isize) => {
            Ok(Integral(Isize(ConstIsize::new_truncating(v as i128, tcx.sess.target.isize_ty))))
        },
        ty::TyUint(ast::UintTy::U8) => Ok(Integral(U8(v as u8))),
        ty::TyUint(ast::UintTy::U16) => Ok(Integral(U16(v as u16))),
        ty::TyUint(ast::UintTy::U32) => Ok(Integral(U32(v as u32))),
        ty::TyUint(ast::UintTy::U64) => Ok(Integral(U64(v as u64))),
        ty::TyUint(ast::UintTy::U128) => Ok(Integral(U128(v as u128))),
        ty::TyUint(ast::UintTy::Usize) => {
            Ok(Integral(Usize(ConstUsize::new_truncating(v, tcx.sess.target.usize_ty))))
        },
        ty::TyFloat(fty) => {
            if let Some(i) = val.to_u128() {
                Ok(Float(ConstFloat::from_u128(i, fty)))
            } else {
                // The value must be negative, go through signed integers.
                let i = val.to_u128_unchecked() as i128;
                Ok(Float(ConstFloat::from_i128(i, fty)))
            }
        }
        ty::TyRawPtr(_) => Err(ErrKind::UnimplementedConstVal("casting an address to a raw ptr")),
        ty::TyChar => match val {
            U8(u) => Ok(Char(u as char)),
            _ => bug!(),
        },
        _ => Err(CannotCast),
    }
}

fn cast_const_float<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                              val: ConstFloat,
                              ty: Ty<'tcx>) -> CastResult<'tcx> {
    let int_width = |ty| {
        ty::layout::Integer::from_attr(tcx, ty).size().bits() as usize
    };
    match ty.sty {
        ty::TyInt(ity) => {
            if let Some(i) = val.to_i128(int_width(attr::SignedInt(ity))) {
                cast_const_int(tcx, I128(i), ty)
            } else {
                Err(CannotCast)
            }
        }
        ty::TyUint(uty) => {
            if let Some(i) = val.to_u128(int_width(attr::UnsignedInt(uty))) {
                cast_const_int(tcx, U128(i), ty)
            } else {
                Err(CannotCast)
            }
        }
        ty::TyFloat(fty) => Ok(Float(val.convert(fty))),
        _ => Err(CannotCast),
    }
}

fn cast_const<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                        val: ConstVal<'tcx>,
                        ty: Ty<'tcx>)
                        -> CastResult<'tcx> {
    match val {
        Integral(i) => cast_const_int(tcx, i, ty),
        Bool(b) => cast_const_int(tcx, U8(b as u8), ty),
        Float(f) => cast_const_float(tcx, f, ty),
        Char(c) => cast_const_int(tcx, U32(c as u32), ty),
        Variant(v) => {
            let adt = tcx.adt_def(tcx.parent_def_id(v).unwrap());
            let idx = adt.variant_index_with_id(v);
            cast_const_int(tcx, adt.discriminant_for_variant(tcx, idx), ty)
        }
        Function(..) => Err(UnimplementedConstVal("casting fn pointers")),
        ByteStr(b) => match ty.sty {
            ty::TyRawPtr(_) => {
                Err(ErrKind::UnimplementedConstVal("casting a bytestr to a raw ptr"))
            },
            ty::TyRef(_, ty::TypeAndMut { ref ty, mutbl: hir::MutImmutable }) => match ty.sty {
                ty::TyArray(ty, n) => {
                    let n = n.val.to_const_int().unwrap().to_u64().unwrap();
                    if ty == tcx.types.u8 && n == b.data.len() as u64 {
                        Ok(val)
                    } else {
                        Err(CannotCast)
                    }
                }
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

fn lit_to_const<'a, 'tcx>(lit: &'tcx ast::LitKind,
                          tcx: TyCtxt<'a, 'tcx, 'tcx>,
                          mut ty: Ty<'tcx>)
                          -> Result<ConstVal<'tcx>, ErrKind<'tcx>> {
    use syntax::ast::*;
    use syntax::ast::LitIntType::*;

    if let ty::TyAdt(adt, _) = ty.sty {
        if adt.is_enum() {
            ty = adt.repr.discr_type().to_ty(tcx)
        }
    }

    match *lit {
        LitKind::Str(ref s, _) => Ok(Str(s.as_str())),
        LitKind::ByteStr(ref data) => Ok(ByteStr(ByteArray { data })),
        LitKind::Byte(n) => Ok(Integral(U8(n))),
        LitKind::Int(n, hint) => {
            match (&ty.sty, hint) {
                (&ty::TyInt(ity), _) |
                (_, Signed(ity)) => {
                    Ok(Integral(ConstInt::new_signed_truncating(n as i128,
                        ity, tcx.sess.target.isize_ty)))
                }
                (&ty::TyUint(uty), _) |
                (_, Unsigned(uty)) => {
                    Ok(Integral(ConstInt::new_unsigned_truncating(n as u128,
                        uty, tcx.sess.target.usize_ty)))
                }
                _ => bug!()
            }
        }
        LitKind::Float(n, fty) => {
            parse_float(&n.as_str(), fty).map(Float)
        }
        LitKind::FloatUnsuffixed(n) => {
            let fty = match ty.sty {
                ty::TyFloat(fty) => fty,
                _ => bug!()
            };
            parse_float(&n.as_str(), fty).map(Float)
        }
        LitKind::Bool(b) => Ok(Bool(b)),
        LitKind::Char(c) => Ok(Char(c)),
    }
}

fn parse_float<'tcx>(num: &str, fty: ast::FloatTy)
                     -> Result<ConstFloat, ErrKind<'tcx>> {
    ConstFloat::from_str(num, fty).map_err(|_| {
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
        (&ByteStr(a), &ByteStr(b)) => Some(a.data.cmp(b.data)),
        (&Char(a), &Char(b)) => Some(a.cmp(&b)),
        _ => None,
    };

    match result {
        Some(result) => Ok(result),
        None => {
            // FIXME: can this ever be reached?
            tcx.sess.delay_span_bug(span,
                &format!("type mismatch comparing {:?} and {:?}", a, b));
            Err(ErrorReported)
        }
    }
}

impl<'a, 'tcx> ConstContext<'a, 'tcx> {
    pub fn compare_lit_exprs(&self,
                             span: Span,
                             a: &'tcx Expr,
                             b: &'tcx Expr) -> Result<Ordering, ErrorReported> {
        let tcx = self.tcx;
        let a = match self.eval(a) {
            Ok(a) => a,
            Err(e) => {
                e.report(tcx, a.span, "expression");
                return Err(ErrorReported);
            }
        };
        let b = match self.eval(b) {
            Ok(b) => b,
            Err(e) => {
                e.report(tcx, b.span, "expression");
                return Err(ErrorReported);
            }
        };
        compare_const_vals(tcx, span, &a.val, &b.val)
    }
}
