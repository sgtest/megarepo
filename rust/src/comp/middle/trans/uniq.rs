import syntax::ast;
import lib::llvm::ValueRef;
import common::*;
import build::*;
import base::*;
import shape::size_of;

export trans_uniq, make_free_glue, autoderef, duplicate, alloc_uniq;

fn trans_uniq(bcx: block, contents: @ast::expr,
              node_id: ast::node_id, dest: dest) -> block {
    let uniq_ty = node_id_type(bcx, node_id);
    let {bcx, val: llptr} = alloc_uniq(bcx, uniq_ty);
    add_clean_free(bcx, llptr, true);
    bcx = trans_expr_save_in(bcx, contents, llptr);
    revoke_clean(bcx, llptr);
    ret store_in_dest(bcx, llptr, dest);
}

fn alloc_uniq(cx: block, uniq_ty: ty::t) -> result {
    let bcx = cx;
    let contents_ty = content_ty(uniq_ty);
    let r = size_of(bcx, contents_ty);
    bcx = r.bcx;
    let llsz = r.val;

    let llptrty = T_ptr(type_of(bcx_ccx(bcx), contents_ty));

    r = trans_shared_malloc(bcx, llptrty, llsz);
    bcx = r.bcx;
    let llptr = r.val;

    ret rslt(bcx, llptr);
}

fn make_free_glue(bcx: block, vptr: ValueRef, t: ty::t)
    -> block {
    with_cond(bcx, IsNotNull(bcx, vptr)) {|bcx|
        let bcx = drop_ty(bcx, vptr, content_ty(t));
        trans_shared_free(bcx, vptr)
    }
}

fn content_ty(t: ty::t) -> ty::t {
    alt ty::get(t).struct {
      ty::ty_uniq({ty: ct, _}) { ct }
      _ { std::util::unreachable(); }
    }
}

fn autoderef(v: ValueRef, t: ty::t) -> {v: ValueRef, t: ty::t} {
    let content_ty = content_ty(t);
    ret {v: v, t: content_ty};
}

fn duplicate(bcx: block, v: ValueRef, t: ty::t) -> result {
    let content_ty = content_ty(t);
    let {bcx, val: llptr} = alloc_uniq(bcx, t);

    let src = load_if_immediate(bcx, v, content_ty);
    let dst = llptr;
    let bcx = copy_val(bcx, INIT, dst, src, content_ty);
    ret rslt(bcx, dst);
}