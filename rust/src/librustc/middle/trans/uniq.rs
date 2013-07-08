// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use lib::llvm::ValueRef;
use middle::trans::base::*;
use middle::trans::build::*;
use middle::trans::common::*;
use middle::trans::datum::immediate_rvalue;
use middle::trans::datum;
use middle::trans::glue;
use middle::ty;
use middle::trans::machine::llsize_of;
use middle::trans::type_of;
use middle::trans::type_of::*;

pub fn make_free_glue(bcx: block, vptrptr: ValueRef, box_ty: ty::t)
    -> block {
    let _icx = push_ctxt("uniq::make_free_glue");
    let box_datum = immediate_rvalue(Load(bcx, vptrptr), box_ty);

    let not_null = IsNotNull(bcx, box_datum.val);
    do with_cond(bcx, not_null) |bcx| {
        let body_datum = box_datum.box_body(bcx);
        let bcx = glue::drop_ty(bcx, body_datum.to_ref_llval(bcx),
                                body_datum.ty);
        if ty::type_contents(bcx.tcx(), box_ty).contains_managed() {
            glue::trans_free(bcx, box_datum.val)
        } else {
            glue::trans_exchange_free(bcx, box_datum.val)
        }
    }
}

pub fn duplicate(bcx: block, src_box: ValueRef, src_ty: ty::t) -> Result {
    let _icx = push_ctxt("uniq::duplicate");

    // Load the body of the source (*src)
    let src_datum = immediate_rvalue(src_box, src_ty);
    let body_datum = src_datum.box_body(bcx);

    // Malloc space in exchange heap and copy src into it
    if ty::type_contents(bcx.tcx(), src_ty).contains_managed() {
        let MallocResult {
            bcx: bcx,
            box: dst_box,
            body: dst_body
        } = malloc_general(bcx, body_datum.ty, heap_managed_unique);
        body_datum.copy_to(bcx, datum::INIT, dst_body);

        rslt(bcx, dst_box)
    } else {
        let body_datum = body_datum.to_value_datum(bcx);
        let llty = type_of(bcx.ccx(), body_datum.ty);
        let size = llsize_of(bcx.ccx(), llty);
        let Result { bcx: bcx, val: val } = malloc_raw_dyn(bcx, body_datum.ty, heap_exchange, size);
        body_datum.copy_to(bcx, datum::INIT, val);
        Result { bcx: bcx, val: val }
    }
}
