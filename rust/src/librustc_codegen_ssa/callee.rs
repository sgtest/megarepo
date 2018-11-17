// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use traits::*;
use rustc::ty;
use rustc::ty::subst::Substs;
use rustc::hir::def_id::DefId;

pub fn resolve_and_get_fn<'tcx, Cx: CodegenMethods<'tcx>>(
    cx: &Cx,
    def_id: DefId,
    substs: &'tcx Substs<'tcx>,
) -> Cx::Value {
    cx.get_fn(
        ty::Instance::resolve(
            cx.tcx(),
            ty::ParamEnv::reveal_all(),
            def_id,
            substs
        ).unwrap()
    )
}

pub fn resolve_and_get_fn_for_vtable<'tcx,
    Cx: Backend<'tcx> + MiscMethods<'tcx> + TypeMethods<'tcx>
>(
    cx: &Cx,
    def_id: DefId,
    substs: &'tcx Substs<'tcx>,
) -> Cx::Value {
    cx.get_fn(
        ty::Instance::resolve_for_vtable(
            cx.tcx(),
            ty::ParamEnv::reveal_all(),
            def_id,
            substs
        ).unwrap()
    )
}
