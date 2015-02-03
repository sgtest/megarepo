// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_upper_case_globals)]

use llvm::{Integer, Pointer, Float, Double, Struct, Array, Vector};
use llvm::{StructRetAttribute, ZExtAttribute};
use trans::cabi::{FnType, ArgType};
use trans::context::CrateContext;
use trans::type_::Type;

use std::cmp;

fn align_up_to(off: uint, a: uint) -> uint {
    return (off + a - 1) / a * a;
}

fn align(off: uint, ty: Type) -> uint {
    let a = ty_align(ty);
    return align_up_to(off, a);
}

fn ty_align(ty: Type) -> uint {
    match ty.kind() {
        Integer => ((ty.int_width() as uint) + 7) / 8,
        Pointer => 8,
        Float => 4,
        Double => 8,
        Struct => {
            if ty.is_packed() {
                1
            } else {
                let str_tys = ty.field_types();
                str_tys.iter().fold(1, |a, t| cmp::max(a, ty_align(*t)))
            }
        }
        Array => {
            let elt = ty.element_type();
            ty_align(elt)
        }
        Vector => {
            let len = ty.vector_length();
            let elt = ty.element_type();
            ty_align(elt) * len
        }
        _ => panic!("ty_align: unhandled type")
    }
}

fn ty_size(ty: Type) -> uint {
    match ty.kind() {
        Integer => ((ty.int_width() as uint) + 7) / 8,
        Pointer => 8,
        Float => 4,
        Double => 8,
        Struct => {
            if ty.is_packed() {
                let str_tys = ty.field_types();
                str_tys.iter().fold(0, |s, t| s + ty_size(*t))
            } else {
                let str_tys = ty.field_types();
                let size = str_tys.iter().fold(0, |s, t| align(s, *t) + ty_size(*t));
                align(size, ty)
            }
        }
        Array => {
            let len = ty.array_length();
            let elt = ty.element_type();
            let eltsz = ty_size(elt);
            len * eltsz
        }
        Vector => {
            let len = ty.vector_length();
            let elt = ty.element_type();
            let eltsz = ty_size(elt);
            len * eltsz
        }
        _ => panic!("ty_size: unhandled type")
    }
}

fn classify_ret_ty(ccx: &CrateContext, ty: Type) -> ArgType {
    if is_reg_ty(ty) {
        let attr = if ty == Type::i1(ccx) { Some(ZExtAttribute) } else { None };
        return ArgType::direct(ty, None, None, attr);
    }
    let size = ty_size(ty);
    if size <= 16 {
        let llty = if size <= 1 {
            Type::i8(ccx)
        } else if size <= 2 {
            Type::i16(ccx)
        } else if size <= 4 {
            Type::i32(ccx)
        } else if size <= 8 {
            Type::i64(ccx)
        } else {
            Type::array(&Type::i64(ccx), ((size + 7 ) / 8 ) as u64)
        };
        return ArgType::direct(ty, Some(llty), None, None);
    }
    ArgType::indirect(ty, Some(StructRetAttribute))
}

fn classify_arg_ty(ccx: &CrateContext, ty: Type) -> ArgType {
    if is_reg_ty(ty) {
        let attr = if ty == Type::i1(ccx) { Some(ZExtAttribute) } else { None };
        return ArgType::direct(ty, None, None, attr);
    }
    let size = ty_size(ty);
    if size <= 16 {
        let llty = if size == 0 {
            Type::array(&Type::i64(ccx), 0u64)
        } else if size == 1 {
            Type::i8(ccx)
        } else if size == 2 {
            Type::i16(ccx)
        } else if size <= 4 {
            Type::i32(ccx)
        } else if size <= 8 {
            Type::i64(ccx)
        } else {
            Type::array(&Type::i64(ccx), ((size + 7 ) / 8 ) as u64)
        };
        return ArgType::direct(ty, Some(llty), None, None);
    }
    ArgType::indirect(ty, None)
}

fn is_reg_ty(ty: Type) -> bool {
    match ty.kind() {
        Integer
        | Pointer
        | Float
        | Double
        | Vector => true,
        _ => false
    }
}

pub fn compute_abi_info(ccx: &CrateContext,
                        atys: &[Type],
                        rty: Type,
                        ret_def: bool) -> FnType {
    let mut arg_tys = Vec::new();
    for &aty in atys {
        let ty = classify_arg_ty(ccx, aty);
        arg_tys.push(ty);
    }

    let ret_ty = if ret_def {
        classify_ret_ty(ccx, rty)
    } else {
        ArgType::direct(Type::void(ccx), None, None, None)
    };

    return FnType {
        arg_tys: arg_tys,
        ret_ty: ret_ty,
    };
}
