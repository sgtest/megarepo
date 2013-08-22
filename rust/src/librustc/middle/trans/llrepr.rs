// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use middle::trans::context::CrateContext;
use middle::trans::type_::Type;
use lib::llvm::ValueRef;

pub trait LlvmRepr {
    fn llrepr(&self, ccx: &CrateContext) -> ~str;
}

impl<'self, T:LlvmRepr> LlvmRepr for &'self [T] {
    fn llrepr(&self, ccx: &CrateContext) -> ~str {
        let reprs = self.map(|t| t.llrepr(ccx));
        fmt!("[%s]", reprs.connect(","))
    }
}

impl LlvmRepr for Type {
    fn llrepr(&self, ccx: &CrateContext) -> ~str {
        ccx.tn.type_to_str(*self)
    }
}

impl LlvmRepr for ValueRef {
    fn llrepr(&self, ccx: &CrateContext) -> ~str {
        ccx.tn.val_to_str(*self)
    }
}


