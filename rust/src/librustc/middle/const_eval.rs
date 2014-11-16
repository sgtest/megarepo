// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_camel_case_types)]
#![allow(unsigned_negation)]

use metadata::csearch;
use middle::astencode;
use middle::def;
use middle::pat_util::def_to_path;
use middle::ty;
use middle::typeck::astconv;
use middle::typeck::check;
use util::nodemap::{DefIdMap};

use syntax::ast::*;
use syntax::parse::token::InternedString;
use syntax::ptr::P;
use syntax::visit::Visitor;
use syntax::visit;
use syntax::{ast, ast_map, ast_util, codemap};

use std::rc::Rc;
use std::collections::hash_map::Vacant;

//
// This pass classifies expressions by their constant-ness.
//
// Constant-ness comes in 3 flavours:
//
//   - Integer-constants: can be evaluated by the frontend all the way down
//     to their actual value. They are used in a few places (enum
//     discriminants, switch arms) and are a subset of
//     general-constants. They cover all the integer and integer-ish
//     literals (nil, bool, int, uint, char, iNN, uNN) and all integer
//     operators and copies applied to them.
//
//   - General-constants: can be evaluated by LLVM but not necessarily by
//     the frontend; usually due to reliance on target-specific stuff such
//     as "where in memory the value goes" or "what floating point mode the
//     target uses". This _includes_ integer-constants, plus the following
//     constructors:
//
//        fixed-size vectors and strings: [] and ""/_
//        vector and string slices: &[] and &""
//        tuples: (,)
//        enums: foo(...)
//        floating point literals and operators
//        & and * pointers
//        copies of general constants
//
//        (in theory, probably not at first: if/match on integer-const
//         conditions / discriminants)
//
//   - Non-constants: everything else.
//

pub enum constness {
    integral_const,
    general_const,
    non_const
}

type constness_cache = DefIdMap<constness>;

pub fn join(a: constness, b: constness) -> constness {
    match (a, b) {
      (integral_const, integral_const) => integral_const,
      (integral_const, general_const)
      | (general_const, integral_const)
      | (general_const, general_const) => general_const,
      _ => non_const
    }
}

pub fn join_all<It: Iterator<constness>>(mut cs: It) -> constness {
    cs.fold(integral_const, |a, b| join(a, b))
}

fn lookup_const<'a>(tcx: &'a ty::ctxt, e: &Expr) -> Option<&'a Expr> {
    let opt_def = tcx.def_map.borrow().find_copy(&e.id);
    match opt_def {
        Some(def::DefConst(def_id)) => {
            lookup_const_by_id(tcx, def_id)
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
        for variant in variants.iter() {
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
                ItemEnum(ast::EnumDef { ref variants }, _) => {
                    variant_expr(variants.as_slice(), variant_def.node)
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
            |a, b, c, d| astencode::decode_inlined_item(a, b, c, d)) {
            csearch::found(&ast::IIItem(ref item)) => match item.node {
                ItemEnum(ast::EnumDef { ref variants }, _) => {
                    // NOTE this doesn't do the right thing, it compares inlined
                    // NodeId's to the original variant_def's NodeId, but they
                    // come from different crates, so they will likely never match.
                    variant_expr(variants.as_slice(), variant_def.node).map(|e| e.id)
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

pub fn lookup_const_by_id<'a>(tcx: &'a ty::ctxt, def_id: ast::DefId)
                          -> Option<&'a Expr> {
    if ast_util::is_local(def_id) {
        match tcx.map.find(def_id.node) {
            None => None,
            Some(ast_map::NodeItem(it)) => match it.node {
                ItemConst(_, ref const_expr) => {
                    Some(&**const_expr)
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
        let expr_id = match csearch::maybe_get_item_ast(tcx, def_id,
            |a, b, c, d| astencode::decode_inlined_item(a, b, c, d)) {
            csearch::found(&ast::IIItem(ref item)) => match item.node {
                ItemConst(_, ref const_expr) => Some(const_expr.id),
                _ => None
            },
            _ => None
        };
        tcx.extern_const_statics.borrow_mut().insert(def_id,
                                                     expr_id.unwrap_or(ast::DUMMY_NODE_ID));
        expr_id.map(|id| tcx.map.expect_expr(id))
    }
}

struct ConstEvalVisitor<'a, 'tcx: 'a> {
    tcx: &'a ty::ctxt<'tcx>,
    ccache: constness_cache,
}

impl<'a, 'tcx> ConstEvalVisitor<'a, 'tcx> {
    fn classify(&mut self, e: &Expr) -> constness {
        let did = ast_util::local_def(e.id);
        match self.ccache.get(&did) {
            Some(&x) => return x,
            None => {}
        }
        let cn = match e.node {
            ast::ExprLit(ref lit) => {
                match lit.node {
                    ast::LitStr(..) | ast::LitFloat(..) => general_const,
                    _ => integral_const
                }
            }

            ast::ExprUnary(_, ref inner) | ast::ExprParen(ref inner) =>
                self.classify(&**inner),

            ast::ExprBinary(_, ref a, ref b) =>
                join(self.classify(&**a), self.classify(&**b)),

            ast::ExprTup(ref es) |
            ast::ExprVec(ref es) =>
                join_all(es.iter().map(|e| self.classify(&**e))),

            ast::ExprStruct(_, ref fs, None) => {
                let cs = fs.iter().map(|f| self.classify(&*f.expr));
                join_all(cs)
            }

            ast::ExprCast(ref base, _) => {
                let ty = ty::expr_ty(self.tcx, e);
                let base = self.classify(&**base);
                if ty::type_is_integral(ty) {
                    join(integral_const, base)
                } else if ty::type_is_fp(ty) {
                    join(general_const, base)
                } else {
                    non_const
                }
            }

            ast::ExprField(ref base, _, _) => self.classify(&**base),

            ast::ExprTupField(ref base, _, _) => self.classify(&**base),

            ast::ExprIndex(ref base, ref idx) =>
                join(self.classify(&**base), self.classify(&**idx)),

            ast::ExprAddrOf(ast::MutImmutable, ref base) =>
                self.classify(&**base),

            // FIXME: (#3728) we can probably do something CCI-ish
            // surrounding nonlocal constants. But we don't yet.
            ast::ExprPath(_) => self.lookup_constness(e),

            ast::ExprRepeat(..) => general_const,

            ast::ExprBlock(ref block) => {
                match block.expr {
                    Some(ref e) => self.classify(&**e),
                    None => integral_const
                }
            }

            _ => non_const
        };
        self.ccache.insert(did, cn);
        cn
    }

    fn lookup_constness(&self, e: &Expr) -> constness {
        match lookup_const(self.tcx, e) {
            Some(rhs) => {
                let ty = ty::expr_ty(self.tcx, &*rhs);
                if ty::type_is_integral(ty) {
                    integral_const
                } else {
                    general_const
                }
            }
            None => non_const
        }
    }

}

impl<'a, 'tcx, 'v> Visitor<'v> for ConstEvalVisitor<'a, 'tcx> {
    fn visit_ty(&mut self, t: &Ty) {
        match t.node {
            TyFixedLengthVec(_, ref expr) => {
                check::check_const_in_type(self.tcx, &**expr, ty::mk_uint());
            }
            _ => {}
        }

        visit::walk_ty(self, t);
    }

    fn visit_expr_post(&mut self, e: &Expr) {
        self.classify(e);
    }
}

pub fn process_crate(tcx: &ty::ctxt) {
    visit::walk_crate(&mut ConstEvalVisitor {
        tcx: tcx,
        ccache: DefIdMap::new(),
    }, tcx.map.krate());
    tcx.sess.abort_if_errors();
}


// FIXME (#33): this doesn't handle big integer/float literals correctly
// (nor does the rest of our literal handling).
#[deriving(Clone, PartialEq)]
pub enum const_val {
    const_float(f64),
    const_int(i64),
    const_uint(u64),
    const_str(InternedString),
    const_binary(Rc<Vec<u8> >),
    const_bool(bool)
}

pub fn const_expr_to_pat(tcx: &ty::ctxt, expr: &Expr) -> P<Pat> {
    let pat = match expr.node {
        ExprTup(ref exprs) =>
            PatTup(exprs.iter().map(|expr| const_expr_to_pat(tcx, &**expr)).collect()),

        ExprCall(ref callee, ref args) => {
            let def = tcx.def_map.borrow().get_copy(&callee.id);
            match tcx.def_map.borrow_mut().entry(expr.id) {
              Vacant(entry) => { entry.set(def); }
              _ => {}
            };
            let path = match def {
                def::DefStruct(def_id) => def_to_path(tcx, def_id),
                def::DefVariant(_, variant_did, _) => def_to_path(tcx, variant_did),
                _ => unreachable!()
            };
            let pats = args.iter().map(|expr| const_expr_to_pat(tcx, &**expr)).collect();
            PatEnum(path, Some(pats))
        }

        ExprStruct(ref path, ref fields, None) => {
            let field_pats = fields.iter().map(|field| codemap::Spanned {
                span: codemap::DUMMY_SP,
                node: FieldPat {
                    ident: field.ident.node,
                    pat: const_expr_to_pat(tcx, &*field.expr),
                    is_shorthand: false,
                },
            }).collect();
            PatStruct(path.clone(), field_pats, false)
        }

        ExprVec(ref exprs) => {
            let pats = exprs.iter().map(|expr| const_expr_to_pat(tcx, &**expr)).collect();
            PatVec(pats, None, vec![])
        }

        ExprPath(ref path) => {
            let opt_def = tcx.def_map.borrow().find_copy(&expr.id);
            match opt_def {
                Some(def::DefStruct(..)) =>
                    PatStruct(path.clone(), vec![], false),
                Some(def::DefVariant(..)) =>
                    PatEnum(path.clone(), None),
                _ => {
                    match lookup_const(tcx, expr) {
                        Some(actual) => return const_expr_to_pat(tcx, actual),
                        _ => unreachable!()
                    }
                }
            }
        }

        _ => PatLit(P(expr.clone()))
    };
    P(Pat { id: expr.id, node: pat, span: expr.span })
}

pub fn eval_const_expr(tcx: &ty::ctxt, e: &Expr) -> const_val {
    match eval_const_expr_partial(tcx, e) {
        Ok(r) => r,
        Err(s) => tcx.sess.span_fatal(e.span, s.as_slice())
    }
}

pub fn eval_const_expr_partial(tcx: &ty::ctxt, e: &Expr) -> Result<const_val, String> {
    fn fromb(b: bool) -> Result<const_val, String> { Ok(const_int(b as i64)) }
    match e.node {
      ExprUnary(UnNeg, ref inner) => {
        match eval_const_expr_partial(tcx, &**inner) {
          Ok(const_float(f)) => Ok(const_float(-f)),
          Ok(const_int(i)) => Ok(const_int(-i)),
          Ok(const_uint(i)) => Ok(const_uint(-i)),
          Ok(const_str(_)) => Err("negate on string".to_string()),
          Ok(const_bool(_)) => Err("negate on boolean".to_string()),
          ref err => ((*err).clone())
        }
      }
      ExprUnary(UnNot, ref inner) => {
        match eval_const_expr_partial(tcx, &**inner) {
          Ok(const_int(i)) => Ok(const_int(!i)),
          Ok(const_uint(i)) => Ok(const_uint(!i)),
          Ok(const_bool(b)) => Ok(const_bool(!b)),
          _ => Err("not on float or string".to_string())
        }
      }
      ExprBinary(op, ref a, ref b) => {
        match (eval_const_expr_partial(tcx, &**a),
               eval_const_expr_partial(tcx, &**b)) {
          (Ok(const_float(a)), Ok(const_float(b))) => {
            match op {
              BiAdd => Ok(const_float(a + b)),
              BiSub => Ok(const_float(a - b)),
              BiMul => Ok(const_float(a * b)),
              BiDiv => Ok(const_float(a / b)),
              BiRem => Ok(const_float(a % b)),
              BiEq => fromb(a == b),
              BiLt => fromb(a < b),
              BiLe => fromb(a <= b),
              BiNe => fromb(a != b),
              BiGe => fromb(a >= b),
              BiGt => fromb(a > b),
              _ => Err("can't do this op on floats".to_string())
            }
          }
          (Ok(const_int(a)), Ok(const_int(b))) => {
            match op {
              BiAdd => Ok(const_int(a + b)),
              BiSub => Ok(const_int(a - b)),
              BiMul => Ok(const_int(a * b)),
              BiDiv if b == 0 => {
                  Err("attempted to divide by zero".to_string())
              }
              BiDiv => Ok(const_int(a / b)),
              BiRem if b == 0 => {
                  Err("attempted remainder with a divisor of \
                       zero".to_string())
              }
              BiRem => Ok(const_int(a % b)),
              BiAnd | BiBitAnd => Ok(const_int(a & b)),
              BiOr | BiBitOr => Ok(const_int(a | b)),
              BiBitXor => Ok(const_int(a ^ b)),
              BiShl => Ok(const_int(a << b as uint)),
              BiShr => Ok(const_int(a >> b as uint)),
              BiEq => fromb(a == b),
              BiLt => fromb(a < b),
              BiLe => fromb(a <= b),
              BiNe => fromb(a != b),
              BiGe => fromb(a >= b),
              BiGt => fromb(a > b)
            }
          }
          (Ok(const_uint(a)), Ok(const_uint(b))) => {
            match op {
              BiAdd => Ok(const_uint(a + b)),
              BiSub => Ok(const_uint(a - b)),
              BiMul => Ok(const_uint(a * b)),
              BiDiv if b == 0 => {
                  Err("attempted to divide by zero".to_string())
              }
              BiDiv => Ok(const_uint(a / b)),
              BiRem if b == 0 => {
                  Err("attempted remainder with a divisor of \
                       zero".to_string())
              }
              BiRem => Ok(const_uint(a % b)),
              BiAnd | BiBitAnd => Ok(const_uint(a & b)),
              BiOr | BiBitOr => Ok(const_uint(a | b)),
              BiBitXor => Ok(const_uint(a ^ b)),
              BiShl => Ok(const_uint(a << b as uint)),
              BiShr => Ok(const_uint(a >> b as uint)),
              BiEq => fromb(a == b),
              BiLt => fromb(a < b),
              BiLe => fromb(a <= b),
              BiNe => fromb(a != b),
              BiGe => fromb(a >= b),
              BiGt => fromb(a > b),
            }
          }
          // shifts can have any integral type as their rhs
          (Ok(const_int(a)), Ok(const_uint(b))) => {
            match op {
              BiShl => Ok(const_int(a << b as uint)),
              BiShr => Ok(const_int(a >> b as uint)),
              _ => Err("can't do this op on an int and uint".to_string())
            }
          }
          (Ok(const_uint(a)), Ok(const_int(b))) => {
            match op {
              BiShl => Ok(const_uint(a << b as uint)),
              BiShr => Ok(const_uint(a >> b as uint)),
              _ => Err("can't do this op on a uint and int".to_string())
            }
          }
          (Ok(const_bool(a)), Ok(const_bool(b))) => {
            Ok(const_bool(match op {
              BiAnd => a && b,
              BiOr => a || b,
              BiBitXor => a ^ b,
              BiBitAnd => a & b,
              BiBitOr => a | b,
              BiEq => a == b,
              BiNe => a != b,
              _ => return Err("can't do this op on bools".to_string())
             }))
          }
          _ => Err("bad operands for binary".to_string())
        }
      }
      ExprCast(ref base, ref target_ty) => {
        // This tends to get called w/o the type actually having been
        // populated in the ctxt, which was causing things to blow up
        // (#5900). Fall back to doing a limited lookup to get past it.
        let ety = ty::expr_ty_opt(tcx, e)
                .or_else(|| astconv::ast_ty_to_prim_ty(tcx, &**target_ty))
                .unwrap_or_else(|| {
                    tcx.sess.span_fatal(target_ty.span,
                                        "target type not found for const cast")
                });

        macro_rules! define_casts(
            ($val:ident, {
                $($ty_pat:pat => (
                    $intermediate_ty:ty,
                    $const_type:ident,
                    $target_ty:ty
                )),*
            }) => (match ty::get(ety).sty {
                $($ty_pat => {
                    match $val {
                        const_bool(b) => Ok($const_type(b as $intermediate_ty as $target_ty)),
                        const_uint(u) => Ok($const_type(u as $intermediate_ty as $target_ty)),
                        const_int(i) => Ok($const_type(i as $intermediate_ty as $target_ty)),
                        const_float(f) => Ok($const_type(f as $intermediate_ty as $target_ty)),
                        _ => Err(concat!(
                            "can't cast this type to ", stringify!($const_type)
                        ).to_string())
                    }
                },)*
                _ => Err("can't cast this type".to_string())
            })
        )

        eval_const_expr_partial(tcx, &**base)
            .and_then(|val| define_casts!(val, {
                ty::ty_int(ast::TyI) => (int, const_int, i64),
                ty::ty_int(ast::TyI8) => (i8, const_int, i64),
                ty::ty_int(ast::TyI16) => (i16, const_int, i64),
                ty::ty_int(ast::TyI32) => (i32, const_int, i64),
                ty::ty_int(ast::TyI64) => (i64, const_int, i64),
                ty::ty_uint(ast::TyU) => (uint, const_uint, u64),
                ty::ty_uint(ast::TyU8) => (u8, const_uint, u64),
                ty::ty_uint(ast::TyU16) => (u16, const_uint, u64),
                ty::ty_uint(ast::TyU32) => (u32, const_uint, u64),
                ty::ty_uint(ast::TyU64) => (u64, const_uint, u64),
                ty::ty_float(ast::TyF32) => (f32, const_float, f64),
                ty::ty_float(ast::TyF64) => (f64, const_float, f64)
            }))
      }
      ExprPath(_) => {
          match lookup_const(tcx, e) {
              Some(actual_e) => eval_const_expr_partial(tcx, &*actual_e),
              None => Err("non-constant path in constant expr".to_string())
          }
      }
      ExprLit(ref lit) => Ok(lit_to_const(&**lit)),
      ExprParen(ref e)     => eval_const_expr_partial(tcx, &**e),
      ExprBlock(ref block) => {
        match block.expr {
            Some(ref expr) => eval_const_expr_partial(tcx, &**expr),
            None => Ok(const_int(0i64))
        }
      }
      _ => Err("unsupported constant expr".to_string())
    }
}

pub fn lit_to_const(lit: &Lit) -> const_val {
    match lit.node {
        LitStr(ref s, _) => const_str((*s).clone()),
        LitBinary(ref data) => {
            const_binary(Rc::new(data.iter().map(|x| *x).collect()))
        }
        LitByte(n) => const_uint(n as u64),
        LitChar(n) => const_uint(n as u64),
        LitInt(n, ast::SignedIntLit(_, ast::Plus)) |
        LitInt(n, ast::UnsuffixedIntLit(ast::Plus)) => const_int(n as i64),
        LitInt(n, ast::SignedIntLit(_, ast::Minus)) |
        LitInt(n, ast::UnsuffixedIntLit(ast::Minus)) => const_int(-(n as i64)),
        LitInt(n, ast::UnsignedIntLit(_)) => const_uint(n),
        LitFloat(ref n, _) |
        LitFloatUnsuffixed(ref n) => {
            const_float(from_str::<f64>(n.get()).unwrap() as f64)
        }
        LitBool(b) => const_bool(b)
    }
}

fn compare_vals<T: PartialOrd>(a: T, b: T) -> Option<int> {
    Some(if a == b { 0 } else if a < b { -1 } else { 1 })
}
pub fn compare_const_vals(a: &const_val, b: &const_val) -> Option<int> {
    match (a, b) {
        (&const_int(a), &const_int(b)) => compare_vals(a, b),
        (&const_uint(a), &const_uint(b)) => compare_vals(a, b),
        (&const_float(a), &const_float(b)) => compare_vals(a, b),
        (&const_str(ref a), &const_str(ref b)) => compare_vals(a, b),
        (&const_bool(a), &const_bool(b)) => compare_vals(a, b),
        (&const_binary(ref a), &const_binary(ref b)) => compare_vals(a, b),
        _ => None
    }
}

pub fn compare_lit_exprs(tcx: &ty::ctxt, a: &Expr, b: &Expr) -> Option<int> {
    compare_const_vals(&eval_const_expr(tcx, a), &eval_const_expr(tcx, b))
}
