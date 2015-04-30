// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use self::Strategy::*;
use llvm::*;
use trans::cabi::{ArgType, FnType};
use trans::type_::Type;
use super::common::*;
use super::machine::*;

enum Strategy { RetValue(Type), RetPointer }
pub fn compute_abi_info(ccx: &CrateContext,
                        atys: &[Type],
                        rty: Type,
                        ret_def: bool) -> FnType {
    let mut arg_tys = Vec::new();

    let ret_ty;
    if !ret_def {
        ret_ty = ArgType::direct(Type::void(ccx), None, None, None);
    } else if rty.kind() == Struct {
        // Returning a structure. Most often, this will use
        // a hidden first argument. On some platforms, though,
        // small structs are returned as integers.
        //
        // Some links:
        // http://www.angelcode.com/dev/callconv/callconv.html
        // Clang's ABI handling is in lib/CodeGen/TargetInfo.cpp

        let t = &ccx.sess().target.target;
        let strategy = if t.options.is_like_osx || t.options.is_like_windows {
            match llsize_of_alloc(ccx, rty) {
                1 => RetValue(Type::i8(ccx)),
                2 => RetValue(Type::i16(ccx)),
                4 => RetValue(Type::i32(ccx)),
                8 => RetValue(Type::i64(ccx)),
                _ => RetPointer
            }
        } else {
            RetPointer
        };

        match strategy {
            RetValue(t) => {
                ret_ty = ArgType::direct(rty, Some(t), None, None);
            }
            RetPointer => {
                ret_ty = ArgType::indirect(rty, Some(Attribute::StructRetAttribute));
            }
        }
    } else {
        let attr = if rty == Type::i1(ccx) { Some(Attribute::ZExtAttribute) } else { None };
        ret_ty = ArgType::direct(rty, None, None, attr);
    }

    for &t in atys {
        let ty = match t.kind() {
            Struct => {
                let size = llsize_of_alloc(ccx, t);
                if size == 0 {
                    ArgType::ignore(t)
                } else {
                    ArgType::indirect(t, Some(Attribute::ByValAttribute))
                }
            }
            _ => {
                let attr = if t == Type::i1(ccx) { Some(Attribute::ZExtAttribute) } else { None };
                ArgType::direct(t, None, None, attr)
            }
        };
        arg_tys.push(ty);
    }

    return FnType {
        arg_tys: arg_tys,
        ret_ty: ret_ty,
    };
}
