import syntax::ast;
import lib::llvm::llvm::ValueRef;
import trans_common::*;
import trans_build::*;
import trans::{
    trans_shared_malloc,
    type_of_inner,
    size_of,
    node_id_type,
    INIT,
    trans_shared_free,
    drop_ty,
    new_sub_block_ctxt,
    load_if_immediate,
    dest
};

export trans_uniq, make_free_glue, type_is_unique_box, autoderef, duplicate,
       alloc_uniq;

pure fn type_is_unique_box(bcx: @block_ctxt, ty: ty::t) -> bool {
    ty::type_is_unique_box(bcx_tcx(bcx), ty)
}

fn trans_uniq(bcx: @block_ctxt, contents: @ast::expr,
              node_id: ast::node_id, dest: dest) -> @block_ctxt {
    let uniq_ty = node_id_type(bcx_ccx(bcx), node_id);
    check type_is_unique_box(bcx, uniq_ty);
    let {bcx, val: llptr} = alloc_uniq(bcx, uniq_ty);
    add_clean_free(bcx, llptr, true);
    bcx = trans::trans_expr_save_in(bcx, contents, llptr);
    revoke_clean(bcx, llptr);
    ret trans::store_in_dest(bcx, llptr, dest);
}

fn alloc_uniq(bcx: @block_ctxt, uniq_ty: ty::t)
    : type_is_unique_box(bcx, uniq_ty) -> result {
    let contents_ty = content_ty(bcx, uniq_ty);
    let {bcx, val: llsz} = size_of(bcx, contents_ty);
    let ccx = bcx_ccx(bcx);
    check non_ty_var(ccx, contents_ty);
    let llptrty = T_ptr(type_of_inner(ccx, bcx.sp, contents_ty));
    ret trans_shared_malloc(bcx, llptrty, llsz);
}

fn make_free_glue(cx: @block_ctxt, vptr: ValueRef, t: ty::t)
    : type_is_unique_box(cx, t) -> @block_ctxt {

    let bcx = cx;
    let free_cx = new_sub_block_ctxt(bcx, "uniq_free");
    let next_cx = new_sub_block_ctxt(bcx, "uniq_free_next");
    let null_test = IsNull(bcx, vptr);
    CondBr(bcx, null_test, next_cx.llbb, free_cx.llbb);

    let bcx = free_cx;
    let bcx = drop_ty(bcx, vptr, content_ty(cx, t));
    let bcx = trans_shared_free(bcx, vptr);
    Br(bcx, next_cx.llbb);
    next_cx
}

fn content_ty(bcx: @block_ctxt, t: ty::t)
    : type_is_unique_box(bcx, t) -> ty::t {

    alt ty::struct(bcx_tcx(bcx), t) {
      ty::ty_uniq({ty: ct, _}) { ct }
    }
}

fn autoderef(bcx: @block_ctxt, v: ValueRef, t: ty::t)
    : type_is_unique_box(bcx, t) -> {v: ValueRef, t: ty::t} {

    let content_ty = content_ty(bcx, t);
    ret {v: v, t: content_ty};
}

fn duplicate(bcx: @block_ctxt, v: ValueRef, t: ty::t)
    : type_is_unique_box(bcx, t) -> result {

    let content_ty = content_ty(bcx, t);
    let {bcx, val: llptr} = alloc_uniq(bcx, t);

    let src = load_if_immediate(bcx, v, content_ty);
    let dst = llptr;
    let bcx = trans::copy_val(bcx, INIT, dst, src, content_ty);
    ret rslt(bcx, dst);
}