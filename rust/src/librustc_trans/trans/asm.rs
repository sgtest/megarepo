// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! # Translation of inline assembly.

use llvm;
use trans::build::*;
use trans::callee;
use trans::common::*;
use trans::cleanup;
use trans::cleanup::CleanupMethods;
use trans::expr;
use trans::type_of;
use trans::type_::Type;

use syntax::ast;
use std::ffi::CString;
use libc::{c_uint, c_char};

// Take an inline assembly expression and splat it out via LLVM
pub fn trans_inline_asm<'blk, 'tcx>(bcx: Block<'blk, 'tcx>, ia: &ast::InlineAsm)
                                    -> Block<'blk, 'tcx> {
    let fcx = bcx.fcx;
    let mut bcx = bcx;
    let mut constraints = Vec::new();
    let mut output_types = Vec::new();

    let temp_scope = fcx.push_custom_cleanup_scope();

    let mut ext_inputs = Vec::new();
    let mut ext_constraints = Vec::new();

    // Prepare the output operands
    let outputs = ia.outputs.iter().enumerate().map(|(i, &(ref c, ref out, is_rw))| {
        constraints.push((*c).clone());

        let out_datum = unpack_datum!(bcx, expr::trans(bcx, &**out));
        output_types.push(type_of::type_of(bcx.ccx(), out_datum.ty));
        let val = out_datum.val;
        if is_rw {
            ext_inputs.push(unpack_result!(bcx, {
                callee::trans_arg_datum(bcx,
                                       expr_ty(bcx, &**out),
                                       out_datum,
                                       cleanup::CustomScope(temp_scope),
                                       callee::DontAutorefArg)
            }));
            ext_constraints.push(i.to_string());
        }
        val

    }).collect::<Vec<_>>();

    // Now the input operands
    let mut inputs = ia.inputs.iter().map(|&(ref c, ref input)| {
        constraints.push((*c).clone());

        let in_datum = unpack_datum!(bcx, expr::trans(bcx, &**input));
        unpack_result!(bcx, {
            callee::trans_arg_datum(bcx,
                                    expr_ty(bcx, &**input),
                                    in_datum,
                                    cleanup::CustomScope(temp_scope),
                                    callee::DontAutorefArg)
        })
    }).collect::<Vec<_>>();
    inputs.push_all(&ext_inputs[..]);

    // no failure occurred preparing operands, no need to cleanup
    fcx.pop_custom_cleanup_scope(temp_scope);

    let clobbers = ia.clobbers.iter()
                              .map(|s| format!("~{{{}}}", &s));

    // Default per-arch clobbers
    // Basically what clang does
    let arch_clobbers = match &bcx.sess().target.target.arch[..] {
        "x86" | "x86_64" => vec!("~{dirflag}", "~{fpsr}", "~{flags}"),
        _                => Vec::new()
    };

    let all_constraints= constraints.iter()
                                    .map(|s| s.to_string())
                                    .chain(ext_constraints.into_iter())
                                    .chain(clobbers)
                                    .chain(arch_clobbers.iter()
                                               .map(|s| s.to_string()))
                                    .collect::<Vec<String>>()
                                    .connect(",");

    debug!("Asm Constraints: {}", &all_constraints[..]);

    // Depending on how many outputs we have, the return type is different
    let num_outputs = outputs.len();
    let output_type = match num_outputs {
        0 => Type::void(bcx.ccx()),
        1 => output_types[0],
        _ => Type::struct_(bcx.ccx(), &output_types[..], false)
    };

    let dialect = match ia.dialect {
        ast::AsmAtt   => llvm::AD_ATT,
        ast::AsmIntel => llvm::AD_Intel
    };

    let asm = CString::new(ia.asm.as_bytes()).unwrap();
    let constraint_cstr = CString::new(all_constraints).unwrap();
    let r = InlineAsmCall(bcx,
                          asm.as_ptr(),
                          constraint_cstr.as_ptr(),
                          &inputs,
                          output_type,
                          ia.volatile,
                          ia.alignstack,
                          dialect);

    // Again, based on how many outputs we have
    if num_outputs == 1 {
        Store(bcx, r, outputs[0]);
    } else {
        for (i, o) in outputs.iter().enumerate() {
            let v = ExtractValue(bcx, r, i);
            Store(bcx, v, *o);
        }
    }

    // Store expn_id in a metadata node so we can map LLVM errors
    // back to source locations.  See #17552.
    unsafe {
        let key = "srcloc";
        let kind = llvm::LLVMGetMDKindIDInContext(bcx.ccx().llcx(),
            key.as_ptr() as *const c_char, key.len() as c_uint);

        let val: llvm::ValueRef = C_i32(bcx.ccx(), ia.expn_id.into_u32() as i32);

        llvm::LLVMSetMetadata(r, kind,
            llvm::LLVMMDNodeInContext(bcx.ccx().llcx(), &val, 1));
    }

    return bcx;

}

