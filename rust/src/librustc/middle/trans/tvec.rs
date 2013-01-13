// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use back::abi;
use lib::llvm::{ValueRef, TypeRef};
use middle::trans::build::*;
use middle::trans::common::*;
use middle::trans::datum::*;
use middle::trans::expr::{Dest, Ignore, SaveIn};
use middle::trans::expr;
use middle::trans::glue;
use middle::trans::shape::{llsize_of, nonzero_llsize_of};
use middle::trans::type_of;
use middle::ty;
use util::common::indenter;
use util::ppaux::ty_to_str;

use syntax::ast;
use syntax::codemap::span;
use syntax::print::pprust::{expr_to_str};

// Boxed vector types are in some sense currently a "shorthand" for a box
// containing an unboxed vector. This expands a boxed vector type into such an
// expanded type. It doesn't respect mutability, but that doesn't matter at
// this point.
fn expand_boxed_vec_ty(tcx: ty::ctxt, t: ty::t) -> ty::t {
    let unit_ty = ty::sequence_element_type(tcx, t);
    let unboxed_vec_ty = ty::mk_mut_unboxed_vec(tcx, unit_ty);
    match ty::get(t).sty {
      ty::ty_estr(ty::vstore_uniq) | ty::ty_evec(_, ty::vstore_uniq) => {
        ty::mk_imm_uniq(tcx, unboxed_vec_ty)
      }
      ty::ty_estr(ty::vstore_box) | ty::ty_evec(_, ty::vstore_box) => {
        ty::mk_imm_box(tcx, unboxed_vec_ty)
      }
      _ => tcx.sess.bug(~"non boxed-vec type \
                          in tvec::expand_boxed_vec_ty")
    }
}

fn get_fill(bcx: block, vptr: ValueRef) -> ValueRef {
    let _icx = bcx.insn_ctxt("tvec::get_fill");
    Load(bcx, GEPi(bcx, vptr, [0u, abi::vec_elt_fill]))
}
fn set_fill(bcx: block, vptr: ValueRef, fill: ValueRef) {
    Store(bcx, fill, GEPi(bcx, vptr, [0u, abi::vec_elt_fill]));
}
fn get_alloc(bcx: block, vptr: ValueRef) -> ValueRef {
    Load(bcx, GEPi(bcx, vptr, [0u, abi::vec_elt_alloc]))
}

fn get_bodyptr(bcx: block, vptr: ValueRef) -> ValueRef {
    base::non_gc_box_cast(bcx, GEPi(bcx, vptr, [0u, abi::box_field_body]))
}

fn get_dataptr(bcx: block, vptr: ValueRef) -> ValueRef {
    let _icx = bcx.insn_ctxt("tvec::get_dataptr");
    GEPi(bcx, vptr, [0u, abi::vec_elt_elems, 0u])
}

fn pointer_add(bcx: block, ptr: ValueRef, bytes: ValueRef) -> ValueRef {
    let _icx = bcx.insn_ctxt("tvec::pointer_add");
    let old_ty = val_ty(ptr);
    let bptr = PointerCast(bcx, ptr, T_ptr(T_i8()));
    return PointerCast(bcx, InBoundsGEP(bcx, bptr, ~[bytes]), old_ty);
}

fn alloc_raw(bcx: block, unit_ty: ty::t,
              fill: ValueRef, alloc: ValueRef, heap: heap) -> Result {
    let _icx = bcx.insn_ctxt("tvec::alloc_uniq");
    let ccx = bcx.ccx();

    let vecbodyty = ty::mk_mut_unboxed_vec(bcx.tcx(), unit_ty);
    let vecsize = Add(bcx, alloc, llsize_of(ccx, ccx.opaque_vec_type));

    let {bcx, box, body} =
        base::malloc_general_dyn(bcx, vecbodyty, heap, vecsize);
    Store(bcx, fill, GEPi(bcx, body, [0u, abi::vec_elt_fill]));
    Store(bcx, alloc, GEPi(bcx, body, [0u, abi::vec_elt_alloc]));
    return rslt(bcx, box);
}
fn alloc_uniq_raw(bcx: block, unit_ty: ty::t,
                  fill: ValueRef, alloc: ValueRef) -> Result {
    alloc_raw(bcx, unit_ty, fill, alloc, heap_exchange)
}

fn alloc_vec(bcx: block, unit_ty: ty::t, elts: uint, heap: heap) -> Result {
    let _icx = bcx.insn_ctxt("tvec::alloc_uniq");
    let ccx = bcx.ccx();
    let llunitty = type_of::type_of(ccx, unit_ty);
    let unit_sz = nonzero_llsize_of(ccx, llunitty);

    let fill = Mul(bcx, C_uint(ccx, elts), unit_sz);
    let alloc = if elts < 4u { Mul(bcx, C_int(ccx, 4), unit_sz) }
                else { fill };
    let Result {bcx: bcx, val: vptr} =
        alloc_raw(bcx, unit_ty, fill, alloc, heap);
    return rslt(bcx, vptr);
}

fn duplicate_uniq(bcx: block, vptr: ValueRef, vec_ty: ty::t) -> Result {
    let _icx = bcx.insn_ctxt("tvec::duplicate_uniq");

    let fill = get_fill(bcx, get_bodyptr(bcx, vptr));
    let unit_ty = ty::sequence_element_type(bcx.tcx(), vec_ty);
    let Result {bcx, val: newptr} = alloc_uniq_raw(bcx, unit_ty, fill, fill);

    let data_ptr = get_dataptr(bcx, get_bodyptr(bcx, vptr));
    let new_data_ptr = get_dataptr(bcx, get_bodyptr(bcx, newptr));
    base::call_memcpy(bcx, new_data_ptr, data_ptr, fill);

    let bcx = if ty::type_needs_drop(bcx.tcx(), unit_ty) {
        iter_vec_raw(bcx, new_data_ptr, vec_ty, fill, glue::take_ty)
    } else { bcx };
    return rslt(bcx, newptr);
}

fn make_drop_glue_unboxed(bcx: block, vptr: ValueRef, vec_ty: ty::t) ->
   block {
    let _icx = bcx.insn_ctxt("tvec::make_drop_glue_unboxed");
    let tcx = bcx.tcx(), unit_ty = ty::sequence_element_type(tcx, vec_ty);
    if ty::type_needs_drop(tcx, unit_ty) {
        iter_vec_unboxed(bcx, vptr, vec_ty, glue::drop_ty)
    } else { bcx }
}

struct VecTypes {
    vec_ty: ty::t,
    unit_ty: ty::t,
    llunit_ty: TypeRef,
    llunit_size: ValueRef
}

impl VecTypes {
    fn to_str(ccx: @crate_ctxt) -> ~str {
        fmt!("VecTypes {vec_ty=%s, unit_ty=%s, llunit_ty=%s, llunit_size=%s}",
             ty_to_str(ccx.tcx, self.vec_ty),
             ty_to_str(ccx.tcx, self.unit_ty),
             ty_str(ccx.tn, self.llunit_ty),
             val_str(ccx.tn, self.llunit_size))
    }
}

fn trans_fixed_vstore(bcx: block,
                      vstore_expr: @ast::expr,
                      content_expr: @ast::expr,
                      dest: expr::Dest) -> block
{
    //!
    //
    // [...] allocates a fixed-size array and moves it around "by value".
    // In this case, it means that the caller has already given us a location
    // to store the array of the suitable size, so all we have to do is
    // generate the content.

    debug!("trans_fixed_vstore(vstore_expr=%s, dest=%?)",
           bcx.expr_to_str(vstore_expr), dest.to_str(bcx.ccx()));
    let _indenter = indenter();

    let vt = vec_types_from_expr(bcx, vstore_expr);

    return match dest {
        Ignore => write_content(bcx, &vt, vstore_expr, content_expr, dest),
        SaveIn(lldest) => {
            // lldest will have type *[T x N], but we want the type *T,
            // so use GEP to convert:
            let lldest = GEPi(bcx, lldest, [0, 0]);
            write_content(bcx, &vt, vstore_expr, content_expr, SaveIn(lldest))
        }
    };
}

fn trans_slice_vstore(bcx: block,
                      vstore_expr: @ast::expr,
                      content_expr: @ast::expr,
                      dest: expr::Dest) -> block
{
    //!
    //
    // &[...] allocates memory on the stack and writes the values into it,
    // returning a slice (pair of ptr, len).  &"..." is similar except that
    // the memory can be statically allocated.

    let ccx = bcx.ccx();

    debug!("trans_slice_vstore(vstore_expr=%s, dest=%s)",
           bcx.expr_to_str(vstore_expr), dest.to_str(ccx));
    let _indenter = indenter();

    // Handle the &"..." case:
    match content_expr.node {
        ast::expr_lit(@ast::spanned {node: ast::lit_str(s), span: _}) => {
            return trans_lit_str(bcx, content_expr, s, dest);
        }
        _ => {}
    }

    // Handle the &[...] case:
    let vt = vec_types_from_expr(bcx, vstore_expr);
    let count = elements_required(bcx, content_expr);
    debug!("vt=%s, count=%?", vt.to_str(ccx), count);

    // Make a fixed-length backing array and allocate it on the stack.
    let llcount = C_uint(ccx, count);
    let llfixed = base::arrayalloca(bcx, vt.llunit_ty, llcount);

    // Arrange for the backing array to be cleaned up.
    let fixed_ty = ty::mk_evec(bcx.tcx(),
                               {ty: vt.unit_ty, mutbl: ast::m_mutbl},
                               ty::vstore_fixed(count));
    let llfixed_ty = T_ptr(type_of::type_of(bcx.ccx(), fixed_ty));
    let llfixed_casted = BitCast(bcx, llfixed, llfixed_ty);
    add_clean(bcx, llfixed_casted, fixed_ty);

    // Generate the content into the backing array.
    let bcx = write_content(bcx, &vt, vstore_expr,
                            content_expr, SaveIn(llfixed));

    // Finally, create the slice pair itself.
    match dest {
        Ignore => {}
        SaveIn(lldest) => {
            Store(bcx, llfixed, GEPi(bcx, lldest, [0u, abi::slice_elt_base]));
            let lllen = Mul(bcx, llcount, vt.llunit_size);
            Store(bcx, lllen, GEPi(bcx, lldest, [0u, abi::slice_elt_len]));
        }
    }

    return bcx;
}

fn trans_lit_str(bcx: block,
                 lit_expr: @ast::expr,
                 lit_str: @~str,
                 dest: Dest) -> block
{
    //!
    //
    // Literal strings translate to slices into static memory.  This is
    // different from trans_slice_vstore() above because it does need to copy
    // the content anywhere.

    debug!("trans_lit_str(lit_expr=%s, dest=%s)",
           bcx.expr_to_str(lit_expr),
           dest.to_str(bcx.ccx()));
    let _indenter = indenter();

    match dest {
        Ignore => bcx,
        SaveIn(lldest) => {
            unsafe {
                let bytes = lit_str.len() + 1; // count null-terminator too
                let llbytes = C_uint(bcx.ccx(), bytes);
                let llcstr = C_cstr(bcx.ccx(), /*bad*/copy *lit_str);
                let llcstr = llvm::LLVMConstPointerCast(llcstr,
                                                        T_ptr(T_i8()));
                Store(bcx,
                      llcstr,
                      GEPi(bcx, lldest, [0u, abi::slice_elt_base]));
                Store(bcx,
                      llbytes,
                      GEPi(bcx, lldest, [0u, abi::slice_elt_len]));
                bcx
            }
        }
    }
}


fn trans_uniq_or_managed_vstore(bcx: block,
                            heap: heap,
                            vstore_expr: @ast::expr,
                            content_expr: @ast::expr) -> DatumBlock
{
    //!
    //
    // @[...] or ~[...] (also @"..." or ~"...") allocate boxes in the
    // appropriate heap and write the array elements into them.

    debug!("trans_uniq_or_managed_vstore(vstore_expr=%s, heap=%?)",
           bcx.expr_to_str(vstore_expr), heap);
    let _indenter = indenter();

    let vt = vec_types_from_expr(bcx, vstore_expr);
    let count = elements_required(bcx, content_expr);

    let Result {bcx, val} = alloc_vec(bcx, vt.unit_ty, count, heap);
    add_clean_free(bcx, val, heap);
    let dataptr = get_dataptr(bcx, get_bodyptr(bcx, val));

    debug!("alloc_vec() returned val=%s, dataptr=%s",
           bcx.val_str(val), bcx.val_str(dataptr));

    let bcx = write_content(bcx, &vt, vstore_expr,
                            content_expr, SaveIn(dataptr));

    revoke_clean(bcx, val);

    return immediate_rvalue_bcx(bcx, val, vt.vec_ty);
}

fn write_content(bcx: block,
                 vt: &VecTypes,
                 vstore_expr: @ast::expr,
                 content_expr: @ast::expr,
                 dest: Dest) -> block
{
    let _icx = bcx.insn_ctxt("tvec::write_content");
    let mut bcx = bcx;

    debug!("write_content(vt=%s, dest=%s, vstore_expr=%?)",
           vt.to_str(bcx.ccx()),
           dest.to_str(bcx.ccx()),
           bcx.expr_to_str(vstore_expr));
    let _indenter = indenter();

    match /*bad*/copy content_expr.node {
        ast::expr_lit(@ast::spanned { node: ast::lit_str(s), _ }) => {
            match dest {
                Ignore => {
                    return bcx;
                }
                SaveIn(lldest) => {
                    let bytes = s.len() + 1; // copy null-terminator too
                    let llbytes = C_uint(bcx.ccx(), bytes);
                    let llcstr = C_cstr(bcx.ccx(), /*bad*/copy *s);
                    base::call_memcpy(bcx, lldest, llcstr, llbytes);
                    return bcx;
                }
            }
        }
        ast::expr_vec(elements, _) => {
            match dest {
                Ignore => {
                    for elements.each |element| {
                        bcx = expr::trans_into(bcx, *element, Ignore);
                    }
                }

                SaveIn(lldest) => {
                    let mut temp_cleanups = ~[];
                    for elements.eachi |i, element| {
                        let lleltptr = GEPi(bcx, lldest, [i]);
                        debug!("writing index %? with lleltptr=%?",
                               i, bcx.val_str(lleltptr));
                        bcx = expr::trans_into(bcx, *element,
                                               SaveIn(lleltptr));
                        add_clean_temp_mem(bcx, lleltptr, vt.unit_ty);
                        temp_cleanups.push(lleltptr);
                    }
                    for vec::each(temp_cleanups) |cleanup| {
                        revoke_clean(bcx, *cleanup);
                    }
                }
            }
            return bcx;
        }
        ast::expr_repeat(element, count_expr, _) => {
            match dest {
                Ignore => {
                    return expr::trans_into(bcx, element, Ignore);
                }
                SaveIn(lldest) => {
                    let count = ty::eval_repeat_count(bcx.tcx(), count_expr,
                                                      count_expr.span);
                    if count == 0 {
                        return bcx;
                    }

                    let tmpdatum = unpack_datum!(bcx, {
                        expr::trans_to_datum(bcx, element)
                    });

                    let mut temp_cleanups = ~[];

                    for uint::range(0, count) |i| {
                        let lleltptr = GEPi(bcx, lldest, [i]);
                        if i < count - 1 {
                            // Copy all but the last one in.
                            bcx = tmpdatum.copy_to(bcx, INIT, lleltptr);
                        } else {
                            // Move the last one in.
                            bcx = tmpdatum.move_to(bcx, INIT, lleltptr);
                        }
                        add_clean_temp_mem(bcx, lleltptr, vt.unit_ty);
                        temp_cleanups.push(lleltptr);
                    }

                    for vec::each(temp_cleanups) |cleanup| {
                        revoke_clean(bcx, *cleanup);
                    }

                    return bcx;
                }
            }
        }
        _ => {
            bcx.tcx().sess.span_bug(content_expr.span,
                                    ~"Unexpected evec content");
        }
    }
}

fn vec_types_from_expr(bcx: block, vec_expr: @ast::expr) -> VecTypes {
    let vec_ty = node_id_type(bcx, vec_expr.id);
    vec_types(bcx, vec_ty)
}

fn vec_types(bcx: block, vec_ty: ty::t) -> VecTypes {
    let ccx = bcx.ccx();
    let unit_ty = ty::sequence_element_type(bcx.tcx(), vec_ty);
    let llunit_ty = type_of::type_of(ccx, unit_ty);
    let llunit_size = nonzero_llsize_of(ccx, llunit_ty);

    VecTypes {vec_ty: vec_ty,
              unit_ty: unit_ty,
              llunit_ty: llunit_ty,
              llunit_size: llunit_size}
}

fn elements_required(bcx: block, content_expr: @ast::expr) -> uint {
    //! Figure out the number of elements we need to store this content

    match /*bad*/copy content_expr.node {
        ast::expr_lit(@ast::spanned { node: ast::lit_str(s), _ }) => {
            s.len() + 1
        },
        ast::expr_vec(es, _) => es.len(),
        ast::expr_repeat(_, count_expr, _) => {
            ty::eval_repeat_count(bcx.tcx(), count_expr, content_expr.span)
        }
        _ => bcx.tcx().sess.span_bug(content_expr.span,
                                     ~"Unexpected evec content")
    }
}

fn get_base_and_len(bcx: block,
                    llval: ValueRef,
                    vec_ty: ty::t) -> (ValueRef, ValueRef) {
    //!
    //
    // Converts a vector into the slice pair.  The vector should be stored in
    // `llval` which should be either immediate or by-ref as appropriate for
    // the vector type.  If you have a datum, you would probably prefer to
    // call `Datum::get_base_and_len()` which will handle any conversions for
    // you.

    let ccx = bcx.ccx();
    let vt = vec_types(bcx, vec_ty);

    let vstore = match ty::get(vt.vec_ty).sty {
      ty::ty_estr(vst) | ty::ty_evec(_, vst) => vst,
      _ => ty::vstore_uniq
    };

    match vstore {
        ty::vstore_fixed(n) => {
            let base = GEPi(bcx, llval, [0u, 0u]);
            let n = if ty::type_is_str(vec_ty) { n + 1u } else { n };
            let len = Mul(bcx, C_uint(ccx, n), vt.llunit_size);
            (base, len)
        }
        ty::vstore_slice(_) => {
            let base = Load(bcx, GEPi(bcx, llval, [0u, abi::slice_elt_base]));
            let len = Load(bcx, GEPi(bcx, llval, [0u, abi::slice_elt_len]));
            (base, len)
        }
        ty::vstore_uniq | ty::vstore_box => {
            let body = tvec::get_bodyptr(bcx, llval);
            (tvec::get_dataptr(bcx, body), tvec::get_fill(bcx, body))
        }
    }
}

type val_and_ty_fn = fn@(block, ValueRef, ty::t) -> Result;

type iter_vec_block = fn(block, ValueRef, ty::t) -> block;

fn iter_vec_raw(bcx: block, data_ptr: ValueRef, vec_ty: ty::t,
                fill: ValueRef, f: iter_vec_block) -> block {
    let _icx = bcx.insn_ctxt("tvec::iter_vec_raw");

    let unit_ty = ty::sequence_element_type(bcx.tcx(), vec_ty);

    // Calculate the last pointer address we want to handle.
    // FIXME (#3729): Optimize this when the size of the unit type is
    // statically known to not use pointer casts, which tend to confuse
    // LLVM.
    let data_end_ptr = pointer_add(bcx, data_ptr, fill);

    // Now perform the iteration.
    let header_bcx = base::sub_block(bcx, ~"iter_vec_loop_header");
    Br(bcx, header_bcx.llbb);
    let data_ptr =
        Phi(header_bcx, val_ty(data_ptr), ~[data_ptr], ~[bcx.llbb]);
    let not_yet_at_end =
        ICmp(header_bcx, lib::llvm::IntULT, data_ptr, data_end_ptr);
    let body_bcx = base::sub_block(header_bcx, ~"iter_vec_loop_body");
    let next_bcx = base::sub_block(header_bcx, ~"iter_vec_next");
    CondBr(header_bcx, not_yet_at_end, body_bcx.llbb, next_bcx.llbb);
    let body_bcx = f(body_bcx, data_ptr, unit_ty);
    AddIncomingToPhi(data_ptr, InBoundsGEP(body_bcx, data_ptr,
                                           ~[C_int(bcx.ccx(), 1)]),
                     body_bcx.llbb);
    Br(body_bcx, header_bcx.llbb);
    return next_bcx;

}

fn iter_vec_uniq(bcx: block, vptr: ValueRef, vec_ty: ty::t,
                 fill: ValueRef, f: iter_vec_block) -> block {
    let _icx = bcx.insn_ctxt("tvec::iter_vec_uniq");
    let data_ptr = get_dataptr(bcx, get_bodyptr(bcx, vptr));
    iter_vec_raw(bcx, data_ptr, vec_ty, fill, f)
}

fn iter_vec_unboxed(bcx: block, body_ptr: ValueRef, vec_ty: ty::t,
                    f: iter_vec_block) -> block {
    let _icx = bcx.insn_ctxt("tvec::iter_vec_unboxed");
    let fill = get_fill(bcx, body_ptr);
    let dataptr = get_dataptr(bcx, body_ptr);
    return iter_vec_raw(bcx, dataptr, vec_ty, fill, f);
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
