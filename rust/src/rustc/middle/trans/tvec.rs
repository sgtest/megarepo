import syntax::ast;
import driver::session::session;
import lib::llvm::{ValueRef, TypeRef};
import back::abi;
import base::{call_memmove, trans_shared_malloc,
               INIT, copy_val, load_if_immediate, get_tydesc,
               sub_block, do_spill_noroot,
               dest};
import shape::{llsize_of, size_of};
import build::*;
import common::*;

fn get_fill(bcx: block, vptr: ValueRef) -> ValueRef {
    Load(bcx, GEPi(bcx, vptr, [0, abi::vec_elt_fill]))
}
fn get_dataptr(bcx: block, vptr: ValueRef, unit_ty: TypeRef)
    -> ValueRef {
    let ptr = GEPi(bcx, vptr, [0, abi::vec_elt_elems]);
    PointerCast(bcx, ptr, T_ptr(unit_ty))
}

fn pointer_add(bcx: block, ptr: ValueRef, bytes: ValueRef) -> ValueRef {
    let old_ty = val_ty(ptr);
    let bptr = PointerCast(bcx, ptr, T_ptr(T_i8()));
    ret PointerCast(bcx, InBoundsGEP(bcx, bptr, [bytes]), old_ty);
}

fn alloc_raw(bcx: block, fill: ValueRef, alloc: ValueRef) -> result {
    let ccx = bcx.ccx();
    let llvecty = ccx.opaque_vec_type;
    let vecsize = Add(bcx, alloc, llsize_of(ccx, llvecty));
    let {bcx: bcx, val: vecptr} =
        trans_shared_malloc(bcx, T_ptr(llvecty), vecsize);
    Store(bcx, fill, GEPi(bcx, vecptr, [0, abi::vec_elt_fill]));
    Store(bcx, alloc, GEPi(bcx, vecptr, [0, abi::vec_elt_alloc]));
    ret {bcx: bcx, val: vecptr};
}

type alloc_result =
    {bcx: block,
     val: ValueRef,
     unit_ty: ty::t,
     llunitsz: ValueRef,
     llunitty: TypeRef};

fn alloc(bcx: block, vec_ty: ty::t, elts: uint) -> alloc_result {
    let ccx = bcx.ccx();
    let unit_ty = ty::sequence_element_type(bcx.tcx(), vec_ty);
    let llunitty = type_of::type_of_or_i8(ccx, unit_ty);
    let llvecty = T_vec(ccx, llunitty);
    let {bcx: bcx, val: unit_sz} = size_of(bcx, unit_ty);

    let fill = Mul(bcx, C_uint(ccx, elts), unit_sz);
    let alloc = if elts < 4u {
                    Mul(bcx, C_int(ccx, 4), unit_sz)
                } else {
                    fill
                };
    let {bcx: bcx, val: vptr} = alloc_raw(bcx, fill, alloc);
    let vptr = PointerCast(bcx, vptr, T_ptr(llvecty));

    ret {bcx: bcx,
         val: vptr,
         unit_ty: unit_ty,
         llunitsz: unit_sz,
         llunitty: llunitty};
}

fn duplicate(bcx: block, vptr: ValueRef, vec_ty: ty::t) -> result {
    let ccx = bcx.ccx();
    let fill = get_fill(bcx, vptr);
    let size = Add(bcx, fill, llsize_of(ccx, ccx.opaque_vec_type));
    let {bcx: bcx, val: newptr} =
        trans_shared_malloc(bcx, val_ty(vptr), size);
    let bcx = call_memmove(bcx, newptr, vptr, size).bcx;
    let unit_ty = ty::sequence_element_type(bcx.tcx(), vec_ty);
    Store(bcx, fill, GEPi(bcx, newptr, [0, abi::vec_elt_alloc]));
    if ty::type_needs_drop(bcx.tcx(), unit_ty) {
        bcx = iter_vec(bcx, newptr, vec_ty, base::take_ty);
    }
    ret rslt(bcx, newptr);
}
fn make_free_glue(bcx: block, vptr: ValueRef, vec_ty: ty::t) ->
   block {
    let tcx = bcx.tcx(), unit_ty = ty::sequence_element_type(tcx, vec_ty);
    base::with_cond(bcx, IsNotNull(bcx, vptr)) {|bcx|
        let bcx = if ty::type_needs_drop(tcx, unit_ty) {
            iter_vec(bcx, vptr, vec_ty, base::drop_ty)
        } else { bcx };
        base::trans_shared_free(bcx, vptr)
    }
}

fn trans_vec(bcx: block, args: [@ast::expr], id: ast::node_id,
             dest: dest) -> block {
    let ccx = bcx.ccx(), bcx = bcx;
    if dest == base::ignore {
        for arg in args {
            bcx = base::trans_expr(bcx, arg, base::ignore);
        }
        ret bcx;
    }
    let vec_ty = node_id_type(bcx, id);
    let {bcx: bcx,
         val: vptr,
         llunitsz: llunitsz,
         unit_ty: unit_ty,
         llunitty: llunitty} =
        alloc(bcx, vec_ty, args.len());

    add_clean_free(bcx, vptr, true);
    // Store the individual elements.
    let dataptr = get_dataptr(bcx, vptr, llunitty);
    let i = 0u, temp_cleanups = [vptr];
    for e in args {
        let lleltptr = if ty::type_has_dynamic_size(bcx.tcx(), unit_ty) {
            InBoundsGEP(bcx, dataptr, [Mul(bcx, C_uint(ccx, i), llunitsz)])
        } else { InBoundsGEP(bcx, dataptr, [C_uint(ccx, i)]) };
        bcx = base::trans_expr_save_in(bcx, e, lleltptr);
        add_clean_temp_mem(bcx, lleltptr, unit_ty);
        temp_cleanups += [lleltptr];
        i += 1u;
    }
    for cln in temp_cleanups { revoke_clean(bcx, cln); }
    ret base::store_in_dest(bcx, vptr, dest);
}

fn trans_str(bcx: block, s: str, dest: dest) -> block {
    let veclen = str::len(s) + 1u; // +1 for \0
    let {bcx: bcx, val: sptr, _} =
        alloc(bcx, ty::mk_str(bcx.tcx()), veclen);

    let ccx = bcx.ccx();
    let llcstr = C_cstr(ccx, s);
    let bcx = call_memmove(bcx, get_dataptr(bcx, sptr, T_i8()), llcstr,
                           C_uint(ccx, veclen)).bcx;
    ret base::store_in_dest(bcx, sptr, dest);
}

fn trans_append(cx: block, vec_ty: ty::t, lhsptr: ValueRef,
                rhs: ValueRef) -> block {
    // Cast to opaque interior vector types if necessary.
    let ccx = cx.ccx();
    let unit_ty = ty::sequence_element_type(cx.tcx(), vec_ty);
    let dynamic = ty::type_has_dynamic_size(cx.tcx(), unit_ty);
    let (lhsptr, rhs) =
        if !dynamic {
            (lhsptr, rhs)
        } else {
            (PointerCast(cx, lhsptr, T_ptr(T_ptr(ccx.opaque_vec_type))),
             PointerCast(cx, rhs, T_ptr(ccx.opaque_vec_type)))
        };
    let strings = alt check ty::get(vec_ty).struct {
      ty::ty_str { true }
      ty::ty_vec(_) { false }
    };

    let {bcx: bcx, val: unit_sz} = size_of(cx, unit_ty);
    let llunitty = type_of::type_of_or_i8(ccx, unit_ty);

    let lhs = Load(bcx, lhsptr);
    let self_append = ICmp(bcx, lib::llvm::IntEQ, lhs, rhs);
    let lfill = get_fill(bcx, lhs);
    let rfill = get_fill(bcx, rhs);
    let new_fill = Add(bcx, lfill, rfill);
    if strings { new_fill = Sub(bcx, new_fill, C_int(ccx, 1)); }
    let opaque_lhs = PointerCast(bcx, lhsptr,
                                 T_ptr(T_ptr(ccx.opaque_vec_type)));
    Call(bcx, cx.ccx().upcalls.vec_grow,
         [opaque_lhs, new_fill]);
    // Was overwritten if we resized
    let lhs = Load(bcx, lhsptr);
    rhs = Select(bcx, self_append, lhs, rhs);

    let lhs_data = get_dataptr(bcx, lhs, llunitty);
    let lhs_off = lfill;
    if strings { lhs_off = Sub(bcx, lhs_off, C_int(ccx, 1)); }
    let write_ptr = pointer_add(bcx, lhs_data, lhs_off);
    let write_ptr_ptr = do_spill_noroot(bcx, write_ptr);
    let bcx = iter_vec_raw(bcx, rhs, vec_ty, rfill,
                     // We have to increment by the dynamically-computed size.
                     {|bcx, addr, _ty|
                         let write_ptr = Load(bcx, write_ptr_ptr);
                         let bcx =
                             copy_val(bcx, INIT, write_ptr,
                                      load_if_immediate(bcx, addr, unit_ty),
                                      unit_ty);
                         let incr = if dynamic {
                                        unit_sz
                                    } else {
                                        C_int(ccx, 1)
                                    };
                         Store(bcx, InBoundsGEP(bcx, write_ptr, [incr]),
                               write_ptr_ptr);
                         ret bcx;
                     });
    ret bcx;
}

fn trans_append_literal(bcx: block, vptrptr: ValueRef, vec_ty: ty::t,
                        vals: [@ast::expr]) -> block {
    let ccx = bcx.ccx();
    let elt_ty = ty::sequence_element_type(bcx.tcx(), vec_ty);
    let ti = none;
    let {bcx: bcx, val: td} = get_tydesc(bcx, elt_ty, ti);
    base::lazily_emit_tydesc_glue(ccx, abi::tydesc_field_take_glue, ti);
    let opaque_v = PointerCast(bcx, vptrptr,
                               T_ptr(T_ptr(ccx.opaque_vec_type)));
    for val in vals {
        let {bcx: e_bcx, val: elt} = base::trans_temp_expr(bcx, val);
        bcx = e_bcx;
        let r = base::spill_if_immediate(bcx, elt, elt_ty);
        let spilled = r.val;
        bcx = r.bcx;
        Call(bcx, bcx.ccx().upcalls.vec_push,
             [opaque_v, td, PointerCast(bcx, spilled, T_ptr(T_i8()))]);
    }
    ret bcx;
}

fn trans_add(bcx: block, vec_ty: ty::t, lhs: ValueRef,
             rhs: ValueRef, dest: dest) -> block {
    let ccx = bcx.ccx();
    let strings = alt ty::get(vec_ty).struct {
      ty::ty_str { true }
      _ { false }
    };
    let unit_ty = ty::sequence_element_type(bcx.tcx(), vec_ty);
    let llunitty = type_of::type_of_or_i8(ccx, unit_ty);
    let {bcx: bcx, val: llunitsz} = size_of(bcx, unit_ty);

    let lhs_fill = get_fill(bcx, lhs);
    if strings { lhs_fill = Sub(bcx, lhs_fill, C_int(ccx, 1)); }
    let rhs_fill = get_fill(bcx, rhs);
    let new_fill = Add(bcx, lhs_fill, rhs_fill);
    let {bcx: bcx, val: new_vec_ptr} = alloc_raw(bcx, new_fill, new_fill);
    new_vec_ptr = PointerCast(bcx, new_vec_ptr, T_ptr(T_vec(ccx, llunitty)));

    let write_ptr_ptr = do_spill_noroot
        (bcx, get_dataptr(bcx, new_vec_ptr, llunitty));
    let copy_fn = fn@(bcx: block, addr: ValueRef,
                      _ty: ty::t) -> block {
        let ccx = bcx.ccx();
        let write_ptr = Load(bcx, write_ptr_ptr);
        let bcx = copy_val(bcx, INIT, write_ptr,
                           load_if_immediate(bcx, addr, unit_ty), unit_ty);
        let incr =
            if ty::type_has_dynamic_size(bcx.tcx(), unit_ty) {
                llunitsz
            } else {
                C_int(ccx, 1)
            };
        Store(bcx, InBoundsGEP(bcx, write_ptr, [incr]),
              write_ptr_ptr);
        ret bcx;
    };

    let bcx = iter_vec_raw(bcx, lhs, vec_ty, lhs_fill, copy_fn);
    bcx = iter_vec_raw(bcx, rhs, vec_ty, rhs_fill, copy_fn);
    ret base::store_in_dest(bcx, new_vec_ptr, dest);
}

type val_and_ty_fn = fn@(block, ValueRef, ty::t) -> result;

type iter_vec_block = fn(block, ValueRef, ty::t) -> block;

fn iter_vec_raw(bcx: block, vptr: ValueRef, vec_ty: ty::t,
                fill: ValueRef, f: iter_vec_block) -> block {
    let ccx = bcx.ccx();
    let unit_ty = ty::sequence_element_type(bcx.tcx(), vec_ty);
    let llunitty = type_of::type_of_or_i8(ccx, unit_ty);
    let {bcx: bcx, val: unit_sz} = size_of(bcx, unit_ty);
    let vptr = PointerCast(bcx, vptr, T_ptr(T_vec(ccx, llunitty)));
    let data_ptr = get_dataptr(bcx, vptr, llunitty);

    // Calculate the last pointer address we want to handle.
    // FIXME: Optimize this when the size of the unit type is statically
    // known to not use pointer casts, which tend to confuse LLVM.
    let data_end_ptr = pointer_add(bcx, data_ptr, fill);

    // Now perform the iteration.
    let header_cx = sub_block(bcx, "iter_vec_loop_header");
    Br(bcx, header_cx.llbb);
    let data_ptr = Phi(header_cx, val_ty(data_ptr), [data_ptr], [bcx.llbb]);
    let not_yet_at_end =
        ICmp(header_cx, lib::llvm::IntULT, data_ptr, data_end_ptr);
    let body_cx = sub_block(header_cx, "iter_vec_loop_body");
    let next_cx = sub_block(header_cx, "iter_vec_next");
    CondBr(header_cx, not_yet_at_end, body_cx.llbb, next_cx.llbb);
    body_cx = f(body_cx, data_ptr, unit_ty);
    let increment =
        if ty::type_has_dynamic_size(bcx.tcx(), unit_ty) {
            unit_sz
        } else { C_int(ccx, 1) };
    AddIncomingToPhi(data_ptr, InBoundsGEP(body_cx, data_ptr, [increment]),
                     body_cx.llbb);
    Br(body_cx, header_cx.llbb);
    ret next_cx;
}

fn iter_vec(bcx: block, vptr: ValueRef, vec_ty: ty::t,
            f: iter_vec_block) -> block {
    let vptr = PointerCast(bcx, vptr, T_ptr(bcx.ccx().opaque_vec_type));
    ret iter_vec_raw(bcx, vptr, vec_ty, get_fill(bcx, vptr), f);
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
