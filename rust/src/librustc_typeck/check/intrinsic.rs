// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Type-checking for the rust-intrinsic and platform-intrinsic
//! intrinsics that the compiler exposes.

use astconv::AstConv;
use intrinsics;
use middle::def_id::DefId;
use middle::subst;
use middle::ty::FnSig;
use middle::ty::{self, Ty};
use middle::ty_fold::TypeFolder;
use {CrateCtxt, require_same_types};

use std::collections::{HashMap};
use syntax::abi;
use syntax::attr::AttrMetaMethods;
use syntax::codemap::Span;
use syntax::parse::token;

use rustc_front::hir;

fn equate_intrinsic_type<'a, 'tcx>(tcx: &ty::ctxt<'tcx>, it: &hir::ForeignItem,
                                   n_tps: usize,
                                   abi: abi::Abi,
                                   inputs: Vec<ty::Ty<'tcx>>,
                                   output: ty::FnOutput<'tcx>) {
    let fty = tcx.mk_fn(None, tcx.mk_bare_fn(ty::BareFnTy {
        unsafety: hir::Unsafety::Unsafe,
        abi: abi,
        sig: ty::Binder(FnSig {
            inputs: inputs,
            output: output,
            variadic: false,
        }),
    }));
    let i_ty = tcx.lookup_item_type(DefId::local(it.id));
    let i_n_tps = i_ty.generics.types.len(subst::FnSpace);
    if i_n_tps != n_tps {
        span_err!(tcx.sess, it.span, E0094,
            "intrinsic has wrong number of type \
             parameters: found {}, expected {}",
             i_n_tps, n_tps);
    } else {
        require_same_types(tcx,
                           None,
                           false,
                           it.span,
                           i_ty.ty,
                           fty,
                           || {
                format!("intrinsic has wrong type: expected `{}`",
                         fty)
            });
    }
}

/// Remember to add all intrinsics here, in librustc_trans/trans/intrinsic.rs,
/// and in libcore/intrinsics.rs
pub fn check_intrinsic_type(ccx: &CrateCtxt, it: &hir::ForeignItem) {
    fn param<'a, 'tcx>(ccx: &CrateCtxt<'a, 'tcx>, n: u32) -> Ty<'tcx> {
        let name = token::intern(&format!("P{}", n));
        ccx.tcx.mk_param(subst::FnSpace, n, name)
    }

    let tcx = ccx.tcx;
    let name = it.ident.name.as_str();
    let (n_tps, inputs, output) = if name.starts_with("atomic_") {
        let split : Vec<&str> = name.split('_').collect();
        assert!(split.len() >= 2, "Atomic intrinsic not correct format");

        //We only care about the operation here
        let (n_tps, inputs, output) = match split[1] {
            "cxchg" => (1, vec!(tcx.mk_mut_ptr(param(ccx, 0)),
                                param(ccx, 0),
                                param(ccx, 0)),
                        param(ccx, 0)),
            "load" => (1, vec!(tcx.mk_imm_ptr(param(ccx, 0))),
                       param(ccx, 0)),
            "store" => (1, vec!(tcx.mk_mut_ptr(param(ccx, 0)), param(ccx, 0)),
                        tcx.mk_nil()),

            "xchg" | "xadd" | "xsub" | "and"  | "nand" | "or" | "xor" | "max" |
            "min"  | "umax" | "umin" => {
                (1, vec!(tcx.mk_mut_ptr(param(ccx, 0)), param(ccx, 0)),
                 param(ccx, 0))
            }
            "fence" | "singlethreadfence" => {
                (0, Vec::new(), tcx.mk_nil())
            }
            op => {
                span_err!(tcx.sess, it.span, E0092,
                    "unrecognized atomic operation function: `{}`", op);
                return;
            }
        };
        (n_tps, inputs, ty::FnConverging(output))
    } else if &name[..] == "abort" || &name[..] == "unreachable" {
        (0, Vec::new(), ty::FnDiverging)
    } else {
        let (n_tps, inputs, output) = match &name[..] {
            "breakpoint" => (0, Vec::new(), tcx.mk_nil()),
            "size_of" |
            "pref_align_of" | "min_align_of" => (1, Vec::new(), ccx.tcx.types.usize),
            "size_of_val" |  "min_align_of_val" => {
                (1, vec![
                    tcx.mk_imm_ref(tcx.mk_region(ty::ReLateBound(ty::DebruijnIndex::new(1),
                                                                  ty::BrAnon(0))),
                                    param(ccx, 0))
                 ], ccx.tcx.types.usize)
            }
            "init" | "init_dropped" => (1, Vec::new(), param(ccx, 0)),
            "uninit" => (1, Vec::new(), param(ccx, 0)),
            "forget" => (1, vec!( param(ccx, 0) ), tcx.mk_nil()),
            "transmute" => (2, vec!( param(ccx, 0) ), param(ccx, 1)),
            "move_val_init" => {
                (1,
                 vec!(
                    tcx.mk_mut_ptr(param(ccx, 0)),
                    param(ccx, 0)
                  ),
               tcx.mk_nil())
            }
            "drop_in_place" => {
                (1, vec![tcx.mk_mut_ptr(param(ccx, 0))], tcx.mk_nil())
            }
            "needs_drop" => (1, Vec::new(), ccx.tcx.types.bool),

            "type_name" => (1, Vec::new(), tcx.mk_static_str()),
            "type_id" => (1, Vec::new(), ccx.tcx.types.u64),
            "offset" | "arith_offset" => {
              (1,
               vec!(
                  tcx.mk_ptr(ty::TypeAndMut {
                      ty: param(ccx, 0),
                      mutbl: hir::MutImmutable
                  }),
                  ccx.tcx.types.isize
               ),
               tcx.mk_ptr(ty::TypeAndMut {
                   ty: param(ccx, 0),
                   mutbl: hir::MutImmutable
               }))
            }
            "copy" | "copy_nonoverlapping" => {
              (1,
               vec!(
                  tcx.mk_ptr(ty::TypeAndMut {
                      ty: param(ccx, 0),
                      mutbl: hir::MutImmutable
                  }),
                  tcx.mk_ptr(ty::TypeAndMut {
                      ty: param(ccx, 0),
                      mutbl: hir::MutMutable
                  }),
                  tcx.types.usize,
               ),
               tcx.mk_nil())
            }
            "volatile_copy_memory" | "volatile_copy_nonoverlapping_memory" => {
              (1,
               vec!(
                  tcx.mk_ptr(ty::TypeAndMut {
                      ty: param(ccx, 0),
                      mutbl: hir::MutMutable
                  }),
                  tcx.mk_ptr(ty::TypeAndMut {
                      ty: param(ccx, 0),
                      mutbl: hir::MutImmutable
                  }),
                  tcx.types.usize,
               ),
               tcx.mk_nil())
            }
            "write_bytes" | "volatile_set_memory" => {
              (1,
               vec!(
                  tcx.mk_ptr(ty::TypeAndMut {
                      ty: param(ccx, 0),
                      mutbl: hir::MutMutable
                  }),
                  tcx.types.u8,
                  tcx.types.usize,
               ),
               tcx.mk_nil())
            }
            "sqrtf32" => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "sqrtf64" => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "powif32" => {
               (0,
                vec!( tcx.types.f32, tcx.types.i32 ),
                tcx.types.f32)
            }
            "powif64" => {
               (0,
                vec!( tcx.types.f64, tcx.types.i32 ),
                tcx.types.f64)
            }
            "sinf32" => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "sinf64" => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "cosf32" => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "cosf64" => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "powf32" => {
               (0,
                vec!( tcx.types.f32, tcx.types.f32 ),
                tcx.types.f32)
            }
            "powf64" => {
               (0,
                vec!( tcx.types.f64, tcx.types.f64 ),
                tcx.types.f64)
            }
            "expf32"   => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "expf64"   => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "exp2f32"  => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "exp2f64"  => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "logf32"   => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "logf64"   => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "log10f32" => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "log10f64" => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "log2f32"  => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "log2f64"  => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "fmaf32" => {
                (0,
                 vec!( tcx.types.f32, tcx.types.f32, tcx.types.f32 ),
                 tcx.types.f32)
            }
            "fmaf64" => {
                (0,
                 vec!( tcx.types.f64, tcx.types.f64, tcx.types.f64 ),
                 tcx.types.f64)
            }
            "fabsf32"      => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "fabsf64"      => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "copysignf32"  => (0, vec!( tcx.types.f32, tcx.types.f32 ), tcx.types.f32),
            "copysignf64"  => (0, vec!( tcx.types.f64, tcx.types.f64 ), tcx.types.f64),
            "floorf32"     => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "floorf64"     => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "ceilf32"      => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "ceilf64"      => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "truncf32"     => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "truncf64"     => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "rintf32"      => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "rintf64"      => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "nearbyintf32" => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "nearbyintf64" => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "roundf32"     => (0, vec!( tcx.types.f32 ), tcx.types.f32),
            "roundf64"     => (0, vec!( tcx.types.f64 ), tcx.types.f64),
            "ctpop8"       => (0, vec!( tcx.types.u8  ), tcx.types.u8),
            "ctpop16"      => (0, vec!( tcx.types.u16 ), tcx.types.u16),
            "ctpop32"      => (0, vec!( tcx.types.u32 ), tcx.types.u32),
            "ctpop64"      => (0, vec!( tcx.types.u64 ), tcx.types.u64),
            "ctlz8"        => (0, vec!( tcx.types.u8  ), tcx.types.u8),
            "ctlz16"       => (0, vec!( tcx.types.u16 ), tcx.types.u16),
            "ctlz32"       => (0, vec!( tcx.types.u32 ), tcx.types.u32),
            "ctlz64"       => (0, vec!( tcx.types.u64 ), tcx.types.u64),
            "cttz8"        => (0, vec!( tcx.types.u8  ), tcx.types.u8),
            "cttz16"       => (0, vec!( tcx.types.u16 ), tcx.types.u16),
            "cttz32"       => (0, vec!( tcx.types.u32 ), tcx.types.u32),
            "cttz64"       => (0, vec!( tcx.types.u64 ), tcx.types.u64),
            "bswap16"      => (0, vec!( tcx.types.u16 ), tcx.types.u16),
            "bswap32"      => (0, vec!( tcx.types.u32 ), tcx.types.u32),
            "bswap64"      => (0, vec!( tcx.types.u64 ), tcx.types.u64),

            "volatile_load" =>
                (1, vec!( tcx.mk_imm_ptr(param(ccx, 0)) ), param(ccx, 0)),
            "volatile_store" =>
                (1, vec!( tcx.mk_mut_ptr(param(ccx, 0)), param(ccx, 0) ), tcx.mk_nil()),

            "i8_add_with_overflow" | "i8_sub_with_overflow" | "i8_mul_with_overflow" =>
                (0, vec!(tcx.types.i8, tcx.types.i8),
                tcx.mk_tup(vec!(tcx.types.i8, tcx.types.bool))),

            "i16_add_with_overflow" | "i16_sub_with_overflow" | "i16_mul_with_overflow" =>
                (0, vec!(tcx.types.i16, tcx.types.i16),
                tcx.mk_tup(vec!(tcx.types.i16, tcx.types.bool))),

            "i32_add_with_overflow" | "i32_sub_with_overflow" | "i32_mul_with_overflow" =>
                (0, vec!(tcx.types.i32, tcx.types.i32),
                tcx.mk_tup(vec!(tcx.types.i32, tcx.types.bool))),

            "i64_add_with_overflow" | "i64_sub_with_overflow" | "i64_mul_with_overflow" =>
                (0, vec!(tcx.types.i64, tcx.types.i64),
                tcx.mk_tup(vec!(tcx.types.i64, tcx.types.bool))),

            "u8_add_with_overflow" | "u8_sub_with_overflow" | "u8_mul_with_overflow" =>
                (0, vec!(tcx.types.u8, tcx.types.u8),
                tcx.mk_tup(vec!(tcx.types.u8, tcx.types.bool))),

            "u16_add_with_overflow" | "u16_sub_with_overflow" | "u16_mul_with_overflow" =>
                (0, vec!(tcx.types.u16, tcx.types.u16),
                tcx.mk_tup(vec!(tcx.types.u16, tcx.types.bool))),

            "u32_add_with_overflow" | "u32_sub_with_overflow" | "u32_mul_with_overflow"=>
                (0, vec!(tcx.types.u32, tcx.types.u32),
                tcx.mk_tup(vec!(tcx.types.u32, tcx.types.bool))),

            "u64_add_with_overflow" | "u64_sub_with_overflow"  | "u64_mul_with_overflow" =>
                (0, vec!(tcx.types.u64, tcx.types.u64),
                tcx.mk_tup(vec!(tcx.types.u64, tcx.types.bool))),

            "unchecked_udiv" | "unchecked_sdiv" | "unchecked_urem" | "unchecked_srem" =>
                (1, vec![param(ccx, 0), param(ccx, 0)], param(ccx, 0)),

            "overflowing_add" | "overflowing_sub" | "overflowing_mul" =>
                (1, vec![param(ccx, 0), param(ccx, 0)], param(ccx, 0)),

            "return_address" => (0, vec![], tcx.mk_imm_ptr(tcx.types.u8)),

            "assume" => (0, vec![tcx.types.bool], tcx.mk_nil()),

            "discriminant_value" => (1, vec![
                    tcx.mk_imm_ref(tcx.mk_region(ty::ReLateBound(ty::DebruijnIndex::new(1),
                                                                  ty::BrAnon(0))),
                                   param(ccx, 0))], tcx.types.u64),

            "try" => {
                let mut_u8 = tcx.mk_mut_ptr(tcx.types.u8);
                let fn_ty = ty::BareFnTy {
                    unsafety: hir::Unsafety::Normal,
                    abi: abi::Rust,
                    sig: ty::Binder(FnSig {
                        inputs: vec![mut_u8],
                        output: ty::FnOutput::FnConverging(tcx.mk_nil()),
                        variadic: false,
                    }),
                };
                let fn_ty = tcx.mk_bare_fn(fn_ty);
                (0, vec![tcx.mk_fn(None, fn_ty), mut_u8], mut_u8)
            }

            ref other => {
                span_err!(tcx.sess, it.span, E0093,
                          "unrecognized intrinsic function: `{}`", *other);
                return;
            }
        };
        (n_tps, inputs, ty::FnConverging(output))
    };
    equate_intrinsic_type(
        tcx,
        it,
        n_tps,
        abi::RustIntrinsic,
        inputs,
        output
        )
}

/// Type-check `extern "platform-intrinsic" { ... }` functions.
pub fn check_platform_intrinsic_type(ccx: &CrateCtxt,
                                     it: &hir::ForeignItem) {
    let param = |n| {
        let name = token::intern(&format!("P{}", n));
        ccx.tcx.mk_param(subst::FnSpace, n, name)
    };

    let tcx = ccx.tcx;
    let i_ty = tcx.lookup_item_type(DefId::local(it.id));
    let i_n_tps = i_ty.generics.types.len(subst::FnSpace);
    let name = it.ident.name.as_str();

    let (n_tps, inputs, output) = match &*name {
        "simd_eq" | "simd_ne" | "simd_lt" | "simd_le" | "simd_gt" | "simd_ge" => {
            (2, vec![param(0), param(0)], param(1))
        }
        "simd_add" | "simd_sub" | "simd_mul" |
        "simd_div" | "simd_shl" | "simd_shr" |
        "simd_and" | "simd_or" | "simd_xor" => {
            (1, vec![param(0), param(0)], param(0))
        }
        "simd_insert" => (2, vec![param(0), tcx.types.u32, param(1)], param(0)),
        "simd_extract" => (2, vec![param(0), tcx.types.u32], param(1)),
        "simd_cast" => (2, vec![param(0)], param(1)),
        name if name.starts_with("simd_shuffle") => {
            match name["simd_shuffle".len()..].parse() {
                Ok(n) => {
                    let params = vec![param(0), param(0),
                                      tcx.mk_ty(ty::TyArray(tcx.types.u32, n))];
                    (2, params, param(1))
                }
                Err(_) => {
                    span_err!(tcx.sess, it.span, E0439,
                              "invalid `simd_shuffle`, needs length: `{}`", name);
                    return
                }
            }
        }
        _ => {
            match intrinsics::Intrinsic::find(tcx, &name) {
                Some(intr) => {
                    // this function is a platform specific intrinsic
                    if i_n_tps != 0 {
                        span_err!(tcx.sess, it.span, E0440,
                                  "platform-specific intrinsic has wrong number of type \
                                   parameters: found {}, expected 0",
                                  i_n_tps);
                        return
                    }

                    let mut structural_to_nomimal = HashMap::new();

                    let sig = tcx.no_late_bound_regions(i_ty.ty.fn_sig()).unwrap();
                    if intr.inputs.len() != sig.inputs.len() {
                        span_err!(tcx.sess, it.span, E0444,
                                  "platform-specific intrinsic has invalid number of \
                                   arguments: found {}, expected {}",
                                  intr.inputs.len(), sig.inputs.len());
                        return
                    }
                    let input_pairs = intr.inputs.iter().zip(&sig.inputs);
                    for (i, (expected_arg, arg)) in input_pairs.enumerate() {
                        match_intrinsic_type_to_type(tcx, &format!("argument {}", i + 1), it.span,
                                                     &mut structural_to_nomimal, expected_arg, arg);
                    }
                    match_intrinsic_type_to_type(tcx, "return value", it.span,
                                                 &mut structural_to_nomimal,
                                                 &intr.output, sig.output.unwrap());
                    return
                }
                None => {
                    span_err!(tcx.sess, it.span, E0441,
                              "unrecognized platform-specific intrinsic function: `{}`", name);
                    return;
                }
            }
        }
    };

    equate_intrinsic_type(
        tcx,
        it,
        n_tps,
        abi::PlatformIntrinsic,
        inputs,
        ty::FnConverging(output)
        )
}

// walk the expected type and the actual type in lock step, checking they're
// the same, in a kinda-structural way, i.e. `Vector`s have to be simd structs with
// exactly the right element type
fn match_intrinsic_type_to_type<'tcx, 'a>(
        tcx: &ty::ctxt<'tcx>,
        position: &str,
        span: Span,
        structural_to_nominal: &mut HashMap<&'a intrinsics::Type, ty::Ty<'tcx>>,
        expected: &'a intrinsics::Type, t: ty::Ty<'tcx>)
{
    use intrinsics::Type::*;

    let simple_error = |real: &str, expected: &str| {
        span_err!(tcx.sess, span, E0442,
                  "intrinsic {} has wrong type: found {}, expected {}",
                  position, real, expected)
    };

    match *expected {
        Void => match t.sty {
            ty::TyTuple(ref v) if v.is_empty() => {},
            _ => simple_error(&format!("`{}`", t), "()"),
        },
        // (The width we pass to LLVM doesn't concern the type checker.)
        Integer(signed, bits, _llvm_width) => match (signed, bits, &t.sty) {
            (true,  8,  &ty::TyInt(hir::IntTy::TyI8)) |
            (false, 8,  &ty::TyUint(hir::UintTy::TyU8)) |
            (true,  16, &ty::TyInt(hir::IntTy::TyI16)) |
            (false, 16, &ty::TyUint(hir::UintTy::TyU16)) |
            (true,  32, &ty::TyInt(hir::IntTy::TyI32)) |
            (false, 32, &ty::TyUint(hir::UintTy::TyU32)) |
            (true,  64, &ty::TyInt(hir::IntTy::TyI64)) |
            (false, 64, &ty::TyUint(hir::UintTy::TyU64)) => {},
            _ => simple_error(&format!("`{}`", t),
                              &format!("`{}{n}`",
                                       if signed {"i"} else {"u"},
                                       n = bits)),
        },
        Float(bits) => match (bits, &t.sty) {
            (32, &ty::TyFloat(hir::FloatTy::TyF32)) |
            (64, &ty::TyFloat(hir::FloatTy::TyF64)) => {},
            _ => simple_error(&format!("`{}`", t),
                              &format!("`f{n}`", n = bits)),
        },
        Pointer(ref inner_expected, ref _llvm_type, const_) => {
            match t.sty {
                ty::TyRawPtr(ty::TypeAndMut { ty, mutbl }) => {
                    if (mutbl == hir::MutImmutable) != const_ {
                        simple_error(&format!("`{}`", t),
                                     if const_ {"const pointer"} else {"mut pointer"})
                    }
                    match_intrinsic_type_to_type(tcx, position, span, structural_to_nominal,
                                                 inner_expected, ty)
                }
                _ => simple_error(&format!("`{}`", t),
                                  &format!("raw pointer")),
            }
        }
        Vector(ref inner_expected, ref _llvm_type, len) => {
            if !t.is_simd() {
                simple_error(&format!("non-simd type `{}`", t),
                             "simd type");
                return;
            }
            let t_len = t.simd_size(tcx);
            if len as usize != t_len {
                simple_error(&format!("vector with length {}", t_len),
                             &format!("length {}", len));
                return;
            }
            let t_ty = t.simd_type(tcx);
            {
                // check that a given structural type always has the same an intrinsic definition
                let previous = structural_to_nominal.entry(expected).or_insert(t);
                if *previous != t {
                    // this gets its own error code because it is non-trivial
                    span_err!(tcx.sess, span, E0443,
                              "intrinsic {} has wrong type: found `{}`, expected `{}` which \
                               was used for this vector type previously in this signature",
                              position,
                              t,
                              *previous);
                    return;
                }
            }
            match_intrinsic_type_to_type(tcx,
                                         position,
                                         span,
                                         structural_to_nominal,
                                         inner_expected,
                                         t_ty)
        }
        Aggregate(_flatten, ref expected_contents) => {
            match t.sty {
                ty::TyTuple(ref contents) => {
                    if contents.len() != expected_contents.len() {
                        simple_error(&format!("tuple with length {}", contents.len()),
                                     &format!("tuple with length {}", expected_contents.len()));
                        return
                    }
                    for (e, c) in expected_contents.iter().zip(contents) {
                        match_intrinsic_type_to_type(tcx, position, span, structural_to_nominal,
                                                     e, c)
                    }
                }
                _ => simple_error(&format!("`{}`", t),
                                  &format!("tuple")),
            }
        }
    }
}
