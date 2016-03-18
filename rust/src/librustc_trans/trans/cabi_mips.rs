// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_upper_case_globals)]

use libc::c_uint;
use std::cmp;
use llvm;
use llvm::{Integer, Pointer, Float, Double, Struct, Array, Vector};
use trans::abi::{ArgType, FnType};
use trans::context::CrateContext;
use trans::type_::Type;

fn align_up_to(off: usize, a: usize) -> usize {
    return (off + a - 1) / a * a;
}

fn align(off: usize, ty: Type) -> usize {
    let a = ty_align(ty);
    return align_up_to(off, a);
}

fn ty_align(ty: Type) -> usize {
    match ty.kind() {
        Integer => ((ty.int_width() as usize) + 7) / 8,
        Pointer => 4,
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

fn ty_size(ty: Type) -> usize {
    match ty.kind() {
        Integer => ((ty.int_width() as usize) + 7) / 8,
        Pointer => 4,
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

fn classify_arg_ty(ccx: &CrateContext, arg: &mut ArgType, offset: &mut usize) {
    let orig_offset = *offset;
    let size = ty_size(arg.ty) * 8;
    let mut align = ty_align(arg.ty);

    align = cmp::min(cmp::max(align, 4), 8);
    *offset = align_up_to(*offset, align);
    *offset += align_up_to(size, align * 8) / 8;

    if !is_reg_ty(arg.ty) {
        arg.cast = Some(struct_ty(ccx, arg.ty));
        arg.pad = padding_ty(ccx, align, orig_offset);
    }
}

fn is_reg_ty(ty: Type) -> bool {
    return match ty.kind() {
        Integer
        | Pointer
        | Float
        | Double
        | Vector => true,
        _ => false
    };
}

fn padding_ty(ccx: &CrateContext, align: usize, offset: usize) -> Option<Type> {
    if ((align - 1 ) & offset) > 0 {
        Some(Type::i32(ccx))
    } else {
        None
    }
}

fn coerce_to_int(ccx: &CrateContext, size: usize) -> Vec<Type> {
    let int_ty = Type::i32(ccx);
    let mut args = Vec::new();

    let mut n = size / 32;
    while n > 0 {
        args.push(int_ty);
        n -= 1;
    }

    let r = size % 32;
    if r > 0 {
        unsafe {
            args.push(Type::from_ref(llvm::LLVMIntTypeInContext(ccx.llcx(), r as c_uint)));
        }
    }

    args
}

fn struct_ty(ccx: &CrateContext, ty: Type) -> Type {
    let size = ty_size(ty) * 8;
    Type::struct_(ccx, &coerce_to_int(ccx, size), false)
}

pub fn compute_abi_info(ccx: &CrateContext, fty: &mut FnType) {
    if !fty.ret.is_ignore() && !is_reg_ty(fty.ret.ty) {
        fty.ret.make_indirect(ccx);
    }

    let mut offset = if fty.ret.is_indirect() { 4 } else { 0 };
    for arg in &mut fty.args {
        if arg.is_ignore() { continue; }
        classify_arg_ty(ccx, arg, &mut offset);
    }
}
