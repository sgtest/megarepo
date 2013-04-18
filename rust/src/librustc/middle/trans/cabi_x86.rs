// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use driver::session::os_win32;
use core::option::*;
use lib::llvm::*;
use lib::llvm::llvm::*;
use super::cabi::*;
use super::common::*;
use super::machine::*;

struct X86_ABIInfo {
    ccx: @CrateContext
}

impl ABIInfo for X86_ABIInfo {
    fn compute_info(&self,
                    atys: &[TypeRef],
                    rty: TypeRef,
                    ret_def: bool) -> FnType {
        let mut arg_tys = do atys.map |a| {
            LLVMType { cast: false, ty: *a }
        };
        let mut ret_ty = LLVMType {
            cast: false,
            ty: rty
        };
        let mut attrs = do atys.map |_| {
            None
        };

        // Rules for returning structs taken from
        // http://www.angelcode.com/dev/callconv/callconv.html
        let sret = {
            let returning_a_struct = unsafe { LLVMGetTypeKind(rty) == Struct && ret_def };
            let big_struct = if self.ccx.sess.targ_cfg.os != os_win32 {
                true
            } else {
                llsize_of_alloc(self.ccx, rty) > 8
            };
            returning_a_struct && big_struct
        };

        if sret {
            let ret_ptr_ty = LLVMType {
                cast: false,
                ty: T_ptr(ret_ty.ty)
            };
            arg_tys = ~[ret_ptr_ty] + arg_tys;
            attrs = ~[Some(StructRetAttribute)] + attrs;
            ret_ty = LLVMType {
                cast: false,
                ty: T_void(),
            };
        }

        return FnType {
            arg_tys: arg_tys,
            ret_ty: ret_ty,
            attrs: attrs,
            sret: sret
        };
    }
}

pub fn abi_info(ccx: @CrateContext) -> @ABIInfo {
    return @X86_ABIInfo {
        ccx: ccx
    } as @ABIInfo;
}
