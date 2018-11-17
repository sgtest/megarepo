// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::HasCodegen;
use rustc::ty::{FnSig, Instance, Ty};
use rustc_target::abi::call::FnType;

pub trait AbiMethods<'tcx> {
    fn new_fn_type(&self, sig: FnSig<'tcx>, extra_args: &[Ty<'tcx>]) -> FnType<'tcx, Ty<'tcx>>;
    fn new_vtable(&self, sig: FnSig<'tcx>, extra_args: &[Ty<'tcx>]) -> FnType<'tcx, Ty<'tcx>>;
    fn fn_type_of_instance(&self, instance: &Instance<'tcx>) -> FnType<'tcx, Ty<'tcx>>;
}

pub trait AbiBuilderMethods<'tcx>: HasCodegen<'tcx> {
    fn apply_attrs_callsite(&mut self, ty: &FnType<'tcx, Ty<'tcx>>, callsite: Self::Value);
}
