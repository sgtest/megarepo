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
use rustc::hir::def_id::DefId;
use rustc::ty::layout::Align;

pub trait StaticMethods: BackendTypes {
    fn static_addr_of(&self, cv: Self::Value, align: Align, kind: Option<&str>) -> Self::Value;
    fn codegen_static(&self, def_id: DefId, is_mutable: bool);
}

pub trait StaticBuilderMethods<'tcx>: BackendTypes {
    fn get_static(&self, def_id: DefId) -> Self::Value;
}
