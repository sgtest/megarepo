// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::BackendTypes;
use mir::operand::OperandRef;
use rustc::ty::Ty;
use rustc_target::abi::call::FnType;
use syntax_pos::Span;

pub trait IntrinsicCallMethods<'tcx>: BackendTypes {
    /// Remember to add all intrinsics here, in librustc_typeck/check/mod.rs,
    /// and in libcore/intrinsics.rs; if you need access to any llvm intrinsics,
    /// add them to librustc_codegen_llvm/context.rs
    fn codegen_intrinsic_call(
        &mut self,
        callee_ty: Ty<'tcx>,
        fn_ty: &FnType<'tcx, Ty<'tcx>>,
        args: &[OperandRef<'tcx, Self::Value>],
        llresult: Self::Value,
        span: Span,
    );

    fn abort(&mut self);
    fn assume(&mut self, val: Self::Value);
    fn expect(&mut self, cond: Self::Value, expected: bool) -> Self::Value;
}
