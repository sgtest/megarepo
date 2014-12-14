// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use back::abi;
use llvm;
use llvm::{ConstFCmp, ConstICmp, SetLinkage, PrivateLinkage, ValueRef, Bool, True, False};
use llvm::{IntEQ, IntNE, IntUGT, IntUGE, IntULT, IntULE, IntSGT, IntSGE, IntSLT, IntSLE,
           RealOEQ, RealOGT, RealOGE, RealOLT, RealOLE, RealONE};
use metadata::csearch;
use middle::{const_eval, def};
use trans::{adt, closure, consts, debuginfo, expr, inline, machine};
use trans::base::{mod, push_ctxt};
use trans::common::*;
use trans::type_::Type;
use trans::type_of;
use middle::ty::{mod, Ty};
use util::ppaux::{Repr, ty_to_string};

use std::c_str::ToCStr;
use libc::c_uint;
use syntax::{ast, ast_util};
use syntax::ptr::P;

pub fn const_lit(cx: &CrateContext, e: &ast::Expr, lit: &ast::Lit)
    -> ValueRef {
    let _icx = push_ctxt("trans_lit");
    debug!("const_lit: {}", lit);
    match lit.node {
        ast::LitByte(b) => C_integral(Type::uint_from_ty(cx, ast::TyU8), b as u64, false),
        ast::LitChar(i) => C_integral(Type::char(cx), i as u64, false),
        ast::LitInt(i, ast::SignedIntLit(t, _)) => {
            C_integral(Type::int_from_ty(cx, t), i, true)
        }
        ast::LitInt(u, ast::UnsignedIntLit(t)) => {
            C_integral(Type::uint_from_ty(cx, t), u, false)
        }
        ast::LitInt(i, ast::UnsuffixedIntLit(_)) => {
            let lit_int_ty = ty::node_id_to_type(cx.tcx(), e.id);
            match lit_int_ty.sty {
                ty::ty_int(t) => {
                    C_integral(Type::int_from_ty(cx, t), i as u64, true)
                }
                ty::ty_uint(t) => {
                    C_integral(Type::uint_from_ty(cx, t), i as u64, false)
                }
                _ => cx.sess().span_bug(lit.span,
                        format!("integer literal has type {} (expected int \
                                 or uint)",
                                ty_to_string(cx.tcx(), lit_int_ty)).as_slice())
            }
        }
        ast::LitFloat(ref fs, t) => {
            C_floating(fs.get(), Type::float_from_ty(cx, t))
        }
        ast::LitFloatUnsuffixed(ref fs) => {
            let lit_float_ty = ty::node_id_to_type(cx.tcx(), e.id);
            match lit_float_ty.sty {
                ty::ty_float(t) => {
                    C_floating(fs.get(), Type::float_from_ty(cx, t))
                }
                _ => {
                    cx.sess().span_bug(lit.span,
                        "floating point literal doesn't have the right type");
                }
            }
        }
        ast::LitBool(b) => C_bool(cx, b),
        ast::LitStr(ref s, _) => C_str_slice(cx, (*s).clone()),
        ast::LitBinary(ref data) => C_binary_slice(cx, data.as_slice()),
    }
}

pub fn const_ptrcast(cx: &CrateContext, a: ValueRef, t: Type) -> ValueRef {
    unsafe {
        let b = llvm::LLVMConstPointerCast(a, t.ptr_to().to_ref());
        assert!(cx.const_globals().borrow_mut().insert(b as int, a).is_none());
        b
    }
}

fn const_vec(cx: &CrateContext, e: &ast::Expr,
             es: &[P<ast::Expr>]) -> (ValueRef, Type) {
    let vec_ty = ty::expr_ty(cx.tcx(), e);
    let unit_ty = ty::sequence_element_type(cx.tcx(), vec_ty);
    let llunitty = type_of::type_of(cx, unit_ty);
    let vs = es.iter().map(|e| const_expr(cx, &**e).0)
                      .collect::<Vec<_>>();
    // If the vector contains enums, an LLVM array won't work.
    let v = if vs.iter().any(|vi| val_ty(*vi) != llunitty) {
        C_struct(cx, vs.as_slice(), false)
    } else {
        C_array(llunitty, vs.as_slice())
    };
    (v, llunitty)
}

pub fn const_addr_of(cx: &CrateContext, cv: ValueRef, mutbl: ast::Mutability) -> ValueRef {
    unsafe {
        let gv = "const".with_c_str(|name| {
            llvm::LLVMAddGlobal(cx.llmod(), val_ty(cv).to_ref(), name)
        });
        llvm::LLVMSetInitializer(gv, cv);
        llvm::LLVMSetGlobalConstant(gv,
                                    if mutbl == ast::MutImmutable {True} else {False});
        SetLinkage(gv, PrivateLinkage);
        gv
    }
}

fn const_deref_ptr(cx: &CrateContext, v: ValueRef) -> ValueRef {
    let v = match cx.const_globals().borrow().get(&(v as int)) {
        Some(&v) => v,
        None => v
    };
    unsafe {
        llvm::LLVMGetInitializer(v)
    }
}

fn const_deref_newtype<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, v: ValueRef, t: Ty<'tcx>)
    -> ValueRef {
    let repr = adt::represent_type(cx, t);
    adt::const_get_field(cx, &*repr, v, 0, 0)
}

fn const_deref<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, v: ValueRef,
                         t: Ty<'tcx>, explicit: bool)
                         -> (ValueRef, Ty<'tcx>) {
    match ty::deref(t, explicit) {
        Some(ref mt) => {
            match t.sty {
                ty::ty_ptr(mt) | ty::ty_rptr(_, mt) => {
                    if ty::type_is_sized(cx.tcx(), mt.ty) {
                        (const_deref_ptr(cx, v), mt.ty)
                    } else {
                        // Derefing a fat pointer does not change the representation,
                        // just the type to ty_open.
                        (v, ty::mk_open(cx.tcx(), mt.ty))
                    }
                }
                ty::ty_enum(..) | ty::ty_struct(..) => {
                    assert!(mt.mutbl != ast::MutMutable);
                    (const_deref_newtype(cx, v, t), mt.ty)
                }
                _ => {
                    cx.sess().bug(format!("unexpected dereferenceable type {}",
                                          ty_to_string(cx.tcx(), t)).as_slice())
                }
            }
        }
        None => {
            cx.sess().bug(format!("cannot dereference const of type {}",
                                  ty_to_string(cx.tcx(), t)).as_slice())
        }
    }
}

pub fn get_const_val(cx: &CrateContext,
                     mut def_id: ast::DefId) -> ValueRef {
    let contains_key = cx.const_values().borrow().contains_key(&def_id.node);
    if !ast_util::is_local(def_id) || !contains_key {
        if !ast_util::is_local(def_id) {
            def_id = inline::maybe_instantiate_inline(cx, def_id);
        }

        if let ast::ItemConst(..) = cx.tcx().map.expect_item(def_id.node).node {
            base::get_item_val(cx, def_id.node);
        }
    }

    cx.const_values().borrow()[def_id.node].clone()
}

pub fn const_expr<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, e: &ast::Expr)
                            -> (ValueRef, Ty<'tcx>) {
    let llconst = const_expr_unadjusted(cx, e);
    let mut llconst = llconst;
    let ety = ty::expr_ty(cx.tcx(), e);
    let mut ety_adjusted = ty::expr_ty_adjusted(cx.tcx(), e);
    let opt_adj = cx.tcx().adjustments.borrow().get(&e.id).cloned();
    match opt_adj {
        None => { }
        Some(adj) => {
            match adj {
                ty::AdjustAddEnv(ty::RegionTraitStore(ty::ReStatic, _)) => {
                    let def = ty::resolve_expr(cx.tcx(), e);
                    let wrapper = closure::get_wrapper_for_bare_fn(cx,
                                                                   ety_adjusted,
                                                                   def,
                                                                   llconst,
                                                                   true);
                    llconst = C_struct(cx, &[wrapper, C_null(Type::i8p(cx))], false)
                }
                ty::AdjustAddEnv(store) => {
                    cx.sess()
                      .span_bug(e.span,
                                format!("unexpected static function: {}",
                                        store).as_slice())
                }
                ty::AdjustDerefRef(ref adj) => {
                    let mut ty = ety;
                    // Save the last autoderef in case we can avoid it.
                    if adj.autoderefs > 0 {
                        for _ in range(0, adj.autoderefs-1) {
                            let (dv, dt) = const_deref(cx, llconst, ty, false);
                            llconst = dv;
                            ty = dt;
                        }
                    }

                    match adj.autoref {
                        None => {
                            let (dv, dt) = const_deref(cx, llconst, ty, false);
                            llconst = dv;

                            // If we derefed a fat pointer then we will have an
                            // open type here. So we need to update the type with
                            // the one returned from const_deref.
                            ety_adjusted = dt;
                        }
                        Some(ref autoref) => {
                            match *autoref {
                                ty::AutoUnsafe(_, None) |
                                ty::AutoPtr(ty::ReStatic, _, None) => {
                                    // Don't copy data to do a deref+ref
                                    // (i.e., skip the last auto-deref).
                                    if adj.autoderefs == 0 {
                                        llconst = const_addr_of(cx, llconst, ast::MutImmutable);
                                    }
                                }
                                ty::AutoPtr(ty::ReStatic, _, Some(box ty::AutoUnsize(..))) => {
                                    if adj.autoderefs > 0 {
                                        // Seeing as we are deref'ing here and take a reference
                                        // again to make the pointer part of the far pointer below,
                                        // we just skip the whole thing. We still need the type
                                        // though. This works even if we don't need to deref
                                        // because of byref semantics. Note that this is not just
                                        // an optimisation, it is necessary for mutable vectors to
                                        // work properly.
                                        let (_, dt) = const_deref(cx, llconst, ty, false);
                                        ty = dt;
                                    } else {
                                        llconst = const_addr_of(cx, llconst, ast::MutImmutable)
                                    }

                                    match ty.sty {
                                        ty::ty_vec(unit_ty, Some(len)) => {
                                            let llunitty = type_of::type_of(cx, unit_ty);
                                            let llptr = const_ptrcast(cx, llconst, llunitty);
                                            assert_eq!(abi::FAT_PTR_ADDR, 0);
                                            assert_eq!(abi::FAT_PTR_EXTRA, 1);
                                            llconst = C_struct(cx, &[
                                                llptr,
                                                C_uint(cx, len)
                                            ], false);
                                        }
                                        _ => cx.sess().span_bug(e.span,
                                            format!("unimplemented type in const unsize: {}",
                                                    ty_to_string(cx.tcx(), ty)).as_slice())
                                    }
                                }
                                _ => {
                                    cx.sess()
                                      .span_bug(e.span,
                                                format!("unimplemented const \
                                                         autoref {}",
                                                        autoref).as_slice())
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let llty = type_of::sizing_type_of(cx, ety_adjusted);
    let csize = machine::llsize_of_alloc(cx, val_ty(llconst));
    let tsize = machine::llsize_of_alloc(cx, llty);
    if csize != tsize {
        unsafe {
            // FIXME these values could use some context
            llvm::LLVMDumpValue(llconst);
            llvm::LLVMDumpValue(C_undef(llty));
        }
        cx.sess().bug(format!("const {} of type {} has size {} instead of {}",
                         e.repr(cx.tcx()), ty_to_string(cx.tcx(), ety),
                         csize, tsize).as_slice());
    }
    (llconst, ety_adjusted)
}

// the bool returned is whether this expression can be inlined into other crates
// if it's assigned to a static.
fn const_expr_unadjusted(cx: &CrateContext, e: &ast::Expr) -> ValueRef {
    let map_list = |exprs: &[P<ast::Expr>]| {
        exprs.iter().map(|e| const_expr(cx, &**e).0)
             .fold(Vec::new(), |mut l, val| { l.push(val); l })
    };
    unsafe {
        let _icx = push_ctxt("const_expr");
        return match e.node {
          ast::ExprLit(ref lit) => {
              consts::const_lit(cx, e, &**lit)
          }
          ast::ExprBinary(b, ref e1, ref e2) => {
            let (te1, _) = const_expr(cx, &**e1);
            let (te2, _) = const_expr(cx, &**e2);

            let te2 = base::cast_shift_const_rhs(b, te1, te2);

            /* Neither type is bottom, and we expect them to be unified
             * already, so the following is safe. */
            let ty = ty::expr_ty(cx.tcx(), &**e1);
            let is_float = ty::type_is_fp(ty);
            let signed = ty::type_is_signed(ty);
            return match b {
              ast::BiAdd   => {
                if is_float { llvm::LLVMConstFAdd(te1, te2) }
                else        { llvm::LLVMConstAdd(te1, te2) }
              }
              ast::BiSub => {
                if is_float { llvm::LLVMConstFSub(te1, te2) }
                else        { llvm::LLVMConstSub(te1, te2) }
              }
              ast::BiMul    => {
                if is_float { llvm::LLVMConstFMul(te1, te2) }
                else        { llvm::LLVMConstMul(te1, te2) }
              }
              ast::BiDiv    => {
                if is_float    { llvm::LLVMConstFDiv(te1, te2) }
                else if signed { llvm::LLVMConstSDiv(te1, te2) }
                else           { llvm::LLVMConstUDiv(te1, te2) }
              }
              ast::BiRem    => {
                if is_float    { llvm::LLVMConstFRem(te1, te2) }
                else if signed { llvm::LLVMConstSRem(te1, te2) }
                else           { llvm::LLVMConstURem(te1, te2) }
              }
              ast::BiAnd    => llvm::LLVMConstAnd(te1, te2),
              ast::BiOr     => llvm::LLVMConstOr(te1, te2),
              ast::BiBitXor => llvm::LLVMConstXor(te1, te2),
              ast::BiBitAnd => llvm::LLVMConstAnd(te1, te2),
              ast::BiBitOr  => llvm::LLVMConstOr(te1, te2),
              ast::BiShl    => llvm::LLVMConstShl(te1, te2),
              ast::BiShr    => {
                if signed { llvm::LLVMConstAShr(te1, te2) }
                else      { llvm::LLVMConstLShr(te1, te2) }
              }
              ast::BiEq     => {
                  if is_float { ConstFCmp(RealOEQ, te1, te2) }
                  else        { ConstICmp(IntEQ, te1, te2)   }
              },
              ast::BiLt     => {
                  if is_float { ConstFCmp(RealOLT, te1, te2) }
                  else        {
                      if signed { ConstICmp(IntSLT, te1, te2) }
                      else      { ConstICmp(IntULT, te1, te2) }
                  }
              },
              ast::BiLe     => {
                  if is_float { ConstFCmp(RealOLE, te1, te2) }
                  else        {
                      if signed { ConstICmp(IntSLE, te1, te2) }
                      else      { ConstICmp(IntULE, te1, te2) }
                  }
              },
              ast::BiNe     => {
                  if is_float { ConstFCmp(RealONE, te1, te2) }
                  else        { ConstICmp(IntNE, te1, te2) }
              },
              ast::BiGe     => {
                  if is_float { ConstFCmp(RealOGE, te1, te2) }
                  else        {
                      if signed { ConstICmp(IntSGE, te1, te2) }
                      else      { ConstICmp(IntUGE, te1, te2) }
                  }
              },
              ast::BiGt     => {
                  if is_float { ConstFCmp(RealOGT, te1, te2) }
                  else        {
                      if signed { ConstICmp(IntSGT, te1, te2) }
                      else      { ConstICmp(IntUGT, te1, te2) }
                  }
              },
            }
          },
          ast::ExprUnary(u, ref e) => {
            let (te, _) = const_expr(cx, &**e);
            let ty = ty::expr_ty(cx.tcx(), &**e);
            let is_float = ty::type_is_fp(ty);
            return match u {
              ast::UnUniq | ast::UnDeref => {
                let (dv, _dt) = const_deref(cx, te, ty, true);
                dv
              }
              ast::UnNot    => llvm::LLVMConstNot(te),
              ast::UnNeg    => {
                if is_float { llvm::LLVMConstFNeg(te) }
                else        { llvm::LLVMConstNeg(te) }
              }
            }
          }
          ast::ExprField(ref base, field) => {
              let (bv, bt) = const_expr(cx, &**base);
              let brepr = adt::represent_type(cx, bt);
              expr::with_field_tys(cx.tcx(), bt, None, |discr, field_tys| {
                  let ix = ty::field_idx_strict(cx.tcx(), field.node.name, field_tys);
                  adt::const_get_field(cx, &*brepr, bv, discr, ix)
              })
          }
          ast::ExprTupField(ref base, idx) => {
              let (bv, bt) = const_expr(cx, &**base);
              let brepr = adt::represent_type(cx, bt);
              expr::with_field_tys(cx.tcx(), bt, None, |discr, _| {
                  adt::const_get_field(cx, &*brepr, bv, discr, idx.node)
              })
          }

          ast::ExprIndex(ref base, ref index) => {
              let (bv, bt) = const_expr(cx, &**base);
              let iv = match const_eval::eval_const_expr(cx.tcx(), &**index) {
                  const_eval::const_int(i) => i as u64,
                  const_eval::const_uint(u) => u,
                  _ => cx.sess().span_bug(index.span,
                                          "index is not an integer-constant expression")
              };
              let (arr, len) = match bt.sty {
                  ty::ty_vec(_, Some(u)) => (bv, C_uint(cx, u)),
                  ty::ty_open(ty) => match ty.sty {
                      ty::ty_vec(_, None) | ty::ty_str => {
                          let e1 = const_get_elt(cx, bv, &[0]);
                          (const_deref_ptr(cx, e1), const_get_elt(cx, bv, &[1]))
                      },
                      _ => cx.sess().span_bug(base.span,
                                              format!("index-expr base must be a vector \
                                                       or string type, found {}",
                                                      ty_to_string(cx.tcx(), bt)).as_slice())
                  },
                  ty::ty_rptr(_, mt) => match mt.ty.sty {
                      ty::ty_vec(_, Some(u)) => {
                          (const_deref_ptr(cx, bv), C_uint(cx, u))
                      },
                      _ => cx.sess().span_bug(base.span,
                                              format!("index-expr base must be a vector \
                                                       or string type, found {}",
                                                      ty_to_string(cx.tcx(), bt)).as_slice())
                  },
                  _ => cx.sess().span_bug(base.span,
                                          format!("index-expr base must be a vector \
                                                   or string type, found {}",
                                                  ty_to_string(cx.tcx(), bt)).as_slice())
              };

              let len = llvm::LLVMConstIntGetZExtValue(len) as u64;
              let len = match bt.sty {
                  ty::ty_uniq(ty) | ty::ty_rptr(_, ty::mt{ty, ..}) => match ty.sty {
                      ty::ty_str => {
                          assert!(len > 0);
                          len - 1
                      }
                      _ => len
                  },
                  _ => len
              };
              if iv >= len {
                  // FIXME #3170: report this earlier on in the const-eval
                  // pass. Reporting here is a bit late.
                  cx.sess().span_err(e.span,
                                     "const index-expr is out of bounds");
              }
              const_get_elt(cx, arr, &[iv as c_uint])
          }
          ast::ExprCast(ref base, _) => {
            let ety = ty::expr_ty(cx.tcx(), e);
            let llty = type_of::type_of(cx, ety);
            let (v, basety) = const_expr(cx, &**base);
            return match (expr::cast_type_kind(cx.tcx(), basety),
                           expr::cast_type_kind(cx.tcx(), ety)) {

              (expr::cast_integral, expr::cast_integral) => {
                let s = ty::type_is_signed(basety) as Bool;
                llvm::LLVMConstIntCast(v, llty.to_ref(), s)
              }
              (expr::cast_integral, expr::cast_float) => {
                if ty::type_is_signed(basety) {
                    llvm::LLVMConstSIToFP(v, llty.to_ref())
                } else {
                    llvm::LLVMConstUIToFP(v, llty.to_ref())
                }
              }
              (expr::cast_float, expr::cast_float) => {
                llvm::LLVMConstFPCast(v, llty.to_ref())
              }
              (expr::cast_float, expr::cast_integral) => {
                if ty::type_is_signed(ety) { llvm::LLVMConstFPToSI(v, llty.to_ref()) }
                else { llvm::LLVMConstFPToUI(v, llty.to_ref()) }
              }
              (expr::cast_enum, expr::cast_integral) => {
                let repr = adt::represent_type(cx, basety);
                let discr = adt::const_get_discrim(cx, &*repr, v);
                let iv = C_integral(cx.int_type(), discr, false);
                let ety_cast = expr::cast_type_kind(cx.tcx(), ety);
                match ety_cast {
                    expr::cast_integral => {
                        let s = ty::type_is_signed(ety) as Bool;
                        llvm::LLVMConstIntCast(iv, llty.to_ref(), s)
                    }
                    _ => cx.sess().bug("enum cast destination is not \
                                        integral")
                }
              }
              (expr::cast_pointer, expr::cast_pointer) => {
                llvm::LLVMConstPointerCast(v, llty.to_ref())
              }
              (expr::cast_integral, expr::cast_pointer) => {
                llvm::LLVMConstIntToPtr(v, llty.to_ref())
              }
              (expr::cast_pointer, expr::cast_integral) => {
                llvm::LLVMConstPtrToInt(v, llty.to_ref())
              }
              _ => {
                cx.sess().impossible_case(e.span,
                                          "bad combination of types for cast")
              }
            }
          }
          ast::ExprAddrOf(mutbl, ref sub) => {
              // If this is the address of some static, then we need to return
              // the actual address of the static itself (short circuit the rest
              // of const eval).
              let mut cur = sub;
              loop {
                  match cur.node {
                      ast::ExprParen(ref sub) => cur = sub,
                      _ => break,
                  }
              }
              let opt_def = cx.tcx().def_map.borrow().get(&cur.id).cloned();
              if let Some(def::DefStatic(def_id, _)) = opt_def {
                  let ty = ty::expr_ty(cx.tcx(), e);
                  return get_static_val(cx, def_id, ty);
              }

              // If this isn't the address of a static, then keep going through
              // normal constant evaluation.
              let (e, _) = const_expr(cx, &**sub);
              const_addr_of(cx, e, mutbl)
          }
          ast::ExprTup(ref es) => {
              let ety = ty::expr_ty(cx.tcx(), e);
              let repr = adt::represent_type(cx, ety);
              let vals = map_list(es.as_slice());
              adt::trans_const(cx, &*repr, 0, vals.as_slice())
          }
          ast::ExprStruct(_, ref fs, ref base_opt) => {
              let ety = ty::expr_ty(cx.tcx(), e);
              let repr = adt::represent_type(cx, ety);
              let tcx = cx.tcx();

              let base_val = match *base_opt {
                Some(ref base) => Some(const_expr(cx, &**base)),
                None => None
              };

              expr::with_field_tys(tcx, ety, Some(e.id), |discr, field_tys| {
                  let cs = field_tys.iter().enumerate()
                                    .map(|(ix, &field_ty)| {
                      match fs.iter().find(|f| field_ty.name == f.ident.node.name) {
                          Some(ref f) => const_expr(cx, &*f.expr).0,
                          None => {
                              match base_val {
                                  Some((bv, _)) => {
                                      adt::const_get_field(cx, &*repr, bv,
                                                           discr, ix)
                                  }
                                  None => {
                                      cx.sess().span_bug(e.span,
                                                         "missing struct field")
                                  }
                              }
                          }
                      }
                  }).collect::<Vec<_>>();
                  adt::trans_const(cx, &*repr, discr, cs.as_slice())
              })
          }
          ast::ExprVec(ref es) => {
            const_vec(cx, e, es.as_slice()).0
          }
          ast::ExprRepeat(ref elem, ref count) => {
            let vec_ty = ty::expr_ty(cx.tcx(), e);
            let unit_ty = ty::sequence_element_type(cx.tcx(), vec_ty);
            let llunitty = type_of::type_of(cx, unit_ty);
            let n = match const_eval::eval_const_expr(cx.tcx(), &**count) {
                const_eval::const_int(i)  => i as uint,
                const_eval::const_uint(i) => i as uint,
                _ => cx.sess().span_bug(count.span, "count must be integral const expression.")
            };
            let vs = Vec::from_elem(n, const_expr(cx, &**elem).0);
            if vs.iter().any(|vi| val_ty(*vi) != llunitty) {
                C_struct(cx, vs.as_slice(), false)
            } else {
                C_array(llunitty, vs.as_slice())
            }
          }
          ast::ExprPath(ref pth) => {
            // Assert that there are no type parameters in this path.
            assert!(pth.segments.iter().all(|seg| !seg.parameters.has_types()));

            let opt_def = cx.tcx().def_map.borrow().get(&e.id).cloned();
            match opt_def {
                Some(def::DefFn(def_id, _)) => {
                    if !ast_util::is_local(def_id) {
                        let ty = csearch::get_type(cx.tcx(), def_id).ty;
                        base::trans_external_path(cx, def_id, ty)
                    } else {
                        assert!(ast_util::is_local(def_id));
                        base::get_item_val(cx, def_id.node)
                    }
                }
                Some(def::DefConst(def_id)) => {
                    get_const_val(cx, def_id)
                }
                Some(def::DefVariant(enum_did, variant_did, _)) => {
                    let ety = ty::expr_ty(cx.tcx(), e);
                    let repr = adt::represent_type(cx, ety);
                    let vinfo = ty::enum_variant_with_id(cx.tcx(),
                                                         enum_did,
                                                         variant_did);
                    adt::trans_const(cx, &*repr, vinfo.disr_val, &[])
                }
                Some(def::DefStruct(_)) => {
                    let ety = ty::expr_ty(cx.tcx(), e);
                    let llty = type_of::type_of(cx, ety);
                    C_null(llty)
                }
                _ => {
                    cx.sess().span_bug(e.span, "expected a const, fn, struct, \
                                                or variant def")
                }
            }
          }
          ast::ExprCall(ref callee, ref args) => {
              let opt_def = cx.tcx().def_map.borrow().get(&callee.id).cloned();
              match opt_def {
                  Some(def::DefStruct(_)) => {
                      let ety = ty::expr_ty(cx.tcx(), e);
                      let repr = adt::represent_type(cx, ety);
                      let arg_vals = map_list(args.as_slice());
                      adt::trans_const(cx, &*repr, 0, arg_vals.as_slice())
                  }
                  Some(def::DefVariant(enum_did, variant_did, _)) => {
                      let ety = ty::expr_ty(cx.tcx(), e);
                      let repr = adt::represent_type(cx, ety);
                      let vinfo = ty::enum_variant_with_id(cx.tcx(),
                                                           enum_did,
                                                           variant_did);
                      let arg_vals = map_list(args.as_slice());
                      adt::trans_const(cx,
                                       &*repr,
                                       vinfo.disr_val,
                                       arg_vals.as_slice())
                  }
                  _ => cx.sess().span_bug(e.span, "expected a struct or variant def")
              }
          }
          ast::ExprParen(ref e) => const_expr(cx, &**e).0,
          ast::ExprBlock(ref block) => {
            match block.expr {
                Some(ref expr) => const_expr(cx, &**expr).0,
                None => C_nil(cx)
            }
          }
          _ => cx.sess().span_bug(e.span,
                  "bad constant expression type in consts::const_expr")
        };
    }
}

pub fn trans_static(ccx: &CrateContext, m: ast::Mutability, id: ast::NodeId) {
    unsafe {
        let _icx = push_ctxt("trans_static");
        let g = base::get_item_val(ccx, id);
        // At this point, get_item_val has already translated the
        // constant's initializer to determine its LLVM type.
        let v = ccx.static_values().borrow()[id].clone();
        // boolean SSA values are i1, but they have to be stored in i8 slots,
        // otherwise some LLVM optimization passes don't work as expected
        let v = if llvm::LLVMTypeOf(v) == Type::i1(ccx).to_ref() {
            llvm::LLVMConstZExt(v, Type::i8(ccx).to_ref())
        } else {
            v
        };
        llvm::LLVMSetInitializer(g, v);

        // As an optimization, all shared statics which do not have interior
        // mutability are placed into read-only memory.
        if m != ast::MutMutable {
            let node_ty = ty::node_id_to_type(ccx.tcx(), id);
            let tcontents = ty::type_contents(ccx.tcx(), node_ty);
            if !tcontents.interior_unsafe() {
                llvm::LLVMSetGlobalConstant(g, True);
            }
        }
        debuginfo::create_global_var_metadata(ccx, id, g);
    }
}

fn get_static_val<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>, did: ast::DefId,
                            ty: Ty<'tcx>) -> ValueRef {
    if ast_util::is_local(did) { return base::get_item_val(ccx, did.node) }
    base::trans_external_path(ccx, did, ty)
}
