import core::ctypes::c_uint;
import syntax::ast;
import syntax::ast_util;
import lib::llvm::llvm;
import lib::llvm::{ValueRef, TypeRef};
import common::*;
import build::*;
import base::*;
import middle::freevars::{get_freevars, freevar_info};
import option::{some, none};
import back::abi;
import syntax::codemap::span;
import syntax::print::pprust::expr_to_str;
import back::link::{
    mangle_internal_name_by_path,
    mangle_internal_name_by_path_and_seq};
import util::ppaux::ty_to_str;
import shape::{size_of};
import ast_map::{path, path_mod, path_name};

// ___Good to know (tm)__________________________________________________
//
// The layout of a closure environment in memory is
// roughly as follows:
//
// struct rust_opaque_box {         // see rust_internal.h
//   unsigned ref_count;            // only used for fn@()
//   type_desc *tydesc;             // describes closure_data struct
//   rust_opaque_box *prev;         // (used internally by memory alloc)
//   rust_opaque_box *next;         // (used internally by memory alloc)
//   struct closure_data {
//       type_desc *bound_tdescs[]; // bound descriptors
//       struct {
//         upvar1_t upvar1;
//         ...
//         upvarN_t upvarN;
//       } bound_data;
//    }
// };
//
// Note that the closure is itself a rust_opaque_box.  This is true
// even for fn~ and fn&, because we wish to keep binary compatibility
// between all kinds of closures.  The allocation strategy for this
// closure depends on the closure type.  For a sendfn, the closure
// (and the referenced type descriptors) will be allocated in the
// exchange heap.  For a fn, the closure is allocated in the task heap
// and is reference counted.  For a block, the closure is allocated on
// the stack.
//
// ## Opaque closures and the embedded type descriptor ##
//
// One interesting part of closures is that they encapsulate the data
// that they close over.  So when I have a ptr to a closure, I do not
// know how many type descriptors it contains nor what upvars are
// captured within.  That means I do not know precisely how big it is
// nor where its fields are located.  This is called an "opaque
// closure".
//
// Typically an opaque closure suffices because we only manipulate it
// by ptr.  The routine common::T_opaque_box_ptr() returns an
// appropriate type for such an opaque closure; it allows access to
// the box fields, but not the closure_data itself.
//
// But sometimes, such as when cloning or freeing a closure, we need
// to know the full information.  That is where the type descriptor
// that defines the closure comes in handy.  We can use its take and
// drop glue functions to allocate/free data as needed.
//
// ## Subtleties concerning alignment ##
//
// It is important that we be able to locate the closure data *without
// knowing the kind of data that is being bound*.  This can be tricky
// because the alignment requirements of the bound data affects the
// alignment requires of the closure_data struct as a whole.  However,
// right now this is a non-issue in any case, because the size of the
// rust_opaque_box header is always a mutiple of 16-bytes, which is
// the maximum alignment requirement we ever have to worry about.
//
// The only reason alignment matters is that, in order to learn what data
// is bound, we would normally first load the type descriptors: but their
// location is ultimately depend on their content!  There is, however, a
// workaround.  We can load the tydesc from the rust_opaque_box, which
// describes the closure_data struct and has self-contained derived type
// descriptors, and read the alignment from there.   It's just annoying to
// do.  Hopefully should this ever become an issue we'll have monomorphized
// and type descriptors will all be a bad dream.
//
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

enum environment_value {
    // Evaluate expr and store result in env (used for bind).
    env_expr(@ast::expr, ty::t),

    // Copy the value from this llvm ValueRef into the environment.
    env_copy(ValueRef, ty::t, lval_kind),

    // Move the value from this llvm ValueRef into the environment.
    env_move(ValueRef, ty::t, lval_kind),

    // Access by reference (used for blocks).
    env_ref(ValueRef, ty::t, lval_kind),
}

fn ev_to_str(ccx: @crate_ctxt, ev: environment_value) -> str {
    alt ev {
      env_expr(ex, _) { expr_to_str(ex) }
      env_copy(v, t, lk) { #fmt("copy(%s,%s)", val_str(ccx.tn, v),
                                ty_to_str(ccx.tcx, t)) }
      env_move(v, t, lk) { #fmt("move(%s,%s)", val_str(ccx.tn, v),
                                ty_to_str(ccx.tcx, t)) }
      env_ref(v, t, lk) { #fmt("ref(%s,%s)", val_str(ccx.tn, v),
                                ty_to_str(ccx.tcx, t)) }
    }
}

fn mk_tydesc_ty(tcx: ty::ctxt, ck: ty::closure_kind) -> ty::t {
    ret alt ck {
      ty::ck_block | ty::ck_box { ty::mk_type(tcx) }
      ty::ck_uniq { ty::mk_send_type(tcx) }
    };
}

fn mk_tuplified_uniq_cbox_ty(tcx: ty::ctxt, cdata_ty: ty::t) -> ty::t {
    let tydesc_ty = mk_tydesc_ty(tcx, ty::ck_uniq);
    let cbox_ty = tuplify_cbox_ty(tcx, cdata_ty, tydesc_ty);
    ret ty::mk_imm_uniq(tcx, cbox_ty);
}

// Given a closure ty, emits a corresponding tuple ty
fn mk_closure_tys(tcx: ty::ctxt,
                  ck: ty::closure_kind,
                  ty_params: [fn_ty_param],
                  bound_values: [environment_value])
    -> (ty::t, [ty::t]) {
    let bound_tys = [];

    let tydesc_ty = mk_tydesc_ty(tcx, ck);

    // Compute the closed over tydescs
    let param_ptrs = [];
    for tp in ty_params {
        param_ptrs += [tydesc_ty];
        option::may(tp.dicts) {|dicts|
            for dict in dicts { param_ptrs += [tydesc_ty]; }
        }
    }

    // Compute the closed over data
    for bv in bound_values {
        bound_tys += [alt bv {
            env_copy(_, t, _) { t }
            env_move(_, t, _) { t }
            env_ref(_, t, _) { t }
            env_expr(_, t) { t }
        }];
    }
    let bound_data_ty = ty::mk_tup(tcx, bound_tys);

    let cdata_ty = ty::mk_tup(tcx, [ty::mk_tup(tcx, param_ptrs),
                                    bound_data_ty]);
    #debug["cdata_ty=%s", ty_to_str(tcx, cdata_ty)];
    ret (cdata_ty, bound_tys);
}

fn allocate_cbox(bcx: @block_ctxt,
                 ck: ty::closure_kind,
                 cdata_ty: ty::t)
    -> (@block_ctxt, ValueRef, [ValueRef]) {

    // let ccx = bcx_ccx(bcx);
    let tcx = bcx_tcx(bcx);

    fn nuke_ref_count(bcx: @block_ctxt, box: ValueRef) {
        // Initialize ref count to arbitrary value for debugging:
        let ccx = bcx_ccx(bcx);
        let box = PointerCast(bcx, box, T_opaque_box_ptr(ccx));
        let ref_cnt = GEPi(bcx, box, [0, abi::box_field_refcnt]);
        let rc = C_int(ccx, 0x12345678);
        Store(bcx, rc, ref_cnt);
    }

    fn store_uniq_tydesc(bcx: @block_ctxt,
                         cdata_ty: ty::t,
                         box: ValueRef,
                         &ti: option::t<@tydesc_info>) -> @block_ctxt {
        let ccx = bcx_ccx(bcx);
        let bound_tydesc = GEPi(bcx, box, [0, abi::box_field_tydesc]);
        let {bcx, val: td} =
            base::get_tydesc(bcx, cdata_ty, true, ti).result;
        let td = Call(bcx, ccx.upcalls.create_shared_type_desc, [td]);
        Store(bcx, td, bound_tydesc);
        bcx
    }

    // Allocate and initialize the box:
    let ti = none;
    let temp_cleanups = [];
    let (bcx, box) = alt ck {
      ty::ck_box {
        let {bcx, val: box} = trans_malloc_boxed_raw(bcx, cdata_ty, ti);
        (bcx, box)
      }
      ty::ck_uniq {
        let uniq_cbox_ty = mk_tuplified_uniq_cbox_ty(tcx, cdata_ty);
        check uniq::type_is_unique_box(bcx, uniq_cbox_ty);
        let {bcx, val: box} = uniq::alloc_uniq(bcx, uniq_cbox_ty);
        nuke_ref_count(bcx, box);
        let bcx = store_uniq_tydesc(bcx, cdata_ty, box, ti);
        (bcx, box)
      }
      ty::ck_block {
        let cbox_ty = tuplify_box_ty(tcx, cdata_ty);
        let {bcx, val: box} = base::alloc_ty(bcx, cbox_ty);
        nuke_ref_count(bcx, box);
        (bcx, box)
      }
    };

    base::lazily_emit_tydesc_glue(bcx, abi::tydesc_field_take_glue, ti);
    base::lazily_emit_tydesc_glue(bcx, abi::tydesc_field_drop_glue, ti);
    base::lazily_emit_tydesc_glue(bcx, abi::tydesc_field_free_glue, ti);

    ret (bcx, box, temp_cleanups);
}

type closure_result = {
    llbox: ValueRef,     // llvalue of ptr to closure
    cdata_ty: ty::t,      // type of the closure data
    bcx: @block_ctxt     // final bcx
};

fn cast_if_we_can(bcx: @block_ctxt, llbox: ValueRef, t: ty::t) -> ValueRef {
    let ccx = bcx_ccx(bcx);
    if check type_has_static_size(ccx, t) {
        let llty = type_of(ccx, t);
        ret PointerCast(bcx, llbox, llty);
    } else {
        ret llbox;
    }
}

// Given a block context and a list of tydescs and values to bind
// construct a closure out of them. If copying is true, it is a
// heap allocated closure that copies the upvars into environment.
// Otherwise, it is stack allocated and copies pointers to the upvars.
fn store_environment(
    bcx: @block_ctxt, lltyparams: [fn_ty_param],
    bound_values: [environment_value],
    ck: ty::closure_kind)
    -> closure_result {

    fn maybe_clone_tydesc(bcx: @block_ctxt,
                          ck: ty::closure_kind,
                          td: ValueRef) -> ValueRef {
        ret alt ck {
          ty::ck_block | ty::ck_box {
            td
          }
          ty::ck_uniq {
            Call(bcx, bcx_ccx(bcx).upcalls.create_shared_type_desc, [td])
          }
        };
    }

    let ccx = bcx_ccx(bcx);
    let tcx = bcx_tcx(bcx);

    // compute the shape of the closure
    let (cdata_ty, bound_tys) =
        mk_closure_tys(tcx, ck, lltyparams, bound_values);

    // allocate closure in the heap
    let (bcx, llbox, temp_cleanups) =
        allocate_cbox(bcx, ck, cdata_ty);

    // cbox_ty has the form of a tuple: (a, b, c) we want a ptr to a
    // tuple.  This could be a ptr in uniq or a box or on stack,
    // whatever.
    let cbox_ty = tuplify_box_ty(tcx, cdata_ty);
    let cboxptr_ty = ty::mk_ptr(tcx, {ty:cbox_ty, mut:ast::imm});
    let llbox = cast_if_we_can(bcx, llbox, cboxptr_ty);
    check type_is_tup_like(bcx, cbox_ty);

    // If necessary, copy tydescs describing type parameters into the
    // appropriate slot in the closure.
    let {bcx:bcx, val:ty_params_slot} =
        GEP_tup_like(bcx, cbox_ty, llbox,
                     [0, abi::box_field_body, abi::closure_body_ty_params]);
    let off = 0;
    for tp in lltyparams {
        let cloned_td = maybe_clone_tydesc(bcx, ck, tp.desc);
        Store(bcx, cloned_td, GEPi(bcx, ty_params_slot, [0, off]));
        off += 1;
        option::may(tp.dicts, {|dicts|
            for dict in dicts {
                let cast = PointerCast(bcx, dict, val_ty(cloned_td));
                Store(bcx, cast, GEPi(bcx, ty_params_slot, [0, off]));
                off += 1;
            }
        });
    }

    // Copy expr values into boxed bindings.
    // Silly check
    vec::iteri(bound_values) { |i, bv|
        if (!ccx.sess.opts.no_asm_comments) {
            add_comment(bcx, #fmt("Copy %s into closure",
                                  ev_to_str(ccx, bv)));
        }

        let bound_data = GEP_tup_like_1(bcx, cbox_ty, llbox,
                                        [0,
                                         abi::box_field_body,
                                         abi::closure_body_bindings,
                                         i as int]);
        bcx = bound_data.bcx;
        let bound_data = bound_data.val;
        alt bv {
          env_expr(e, _) {
            bcx = base::trans_expr_save_in(bcx, e, bound_data);
            add_clean_temp_mem(bcx, bound_data, bound_tys[i]);
            temp_cleanups += [bound_data];
          }
          env_copy(val, ty, owned) {
            let val1 = load_if_immediate(bcx, val, ty);
            bcx = base::copy_val(bcx, INIT, bound_data, val1, ty);
          }
          env_copy(val, ty, owned_imm) {
            bcx = base::copy_val(bcx, INIT, bound_data, val, ty);
          }
          env_copy(_, _, temporary) {
            fail "Cannot capture temporary upvar";
          }
          env_move(val, ty, kind) {
            let src = {bcx:bcx, val:val, kind:kind};
            bcx = move_val(bcx, INIT, bound_data, src, ty);
          }
          env_ref(val, ty, owned) {
            Store(bcx, val, bound_data);
          }
          env_ref(val, ty, owned_imm) {
            let addr = do_spill_noroot(bcx, val);
            Store(bcx, addr, bound_data);
          }
          env_ref(_, _, temporary) {
            fail "Cannot capture temporary upvar";
          }
        }
    }
    for cleanup in temp_cleanups { revoke_clean(bcx, cleanup); }

    ret {llbox: llbox, cdata_ty: cdata_ty, bcx: bcx};
}

// Given a context and a list of upvars, build a closure. This just
// collects the upvars and packages them up for store_environment.
fn build_closure(bcx0: @block_ctxt,
                 cap_vars: [capture::capture_var],
                 ck: ty::closure_kind)
    -> closure_result {
    // If we need to, package up the iterator body to call
    let env_vals = [];
    let bcx = bcx0;
    let tcx = bcx_tcx(bcx);

    // Package up the captured upvars
    vec::iter(cap_vars) { |cap_var|
        let lv = trans_local_var(bcx, cap_var.def);
        let nid = ast_util::def_id_of_def(cap_var.def).node;
        let ty = ty::node_id_to_type(tcx, nid);
        alt cap_var.mode {
          capture::cap_ref {
            assert ck == ty::ck_block;
            ty = ty::mk_mut_ptr(tcx, ty);
            env_vals += [env_ref(lv.val, ty, lv.kind)];
          }
          capture::cap_copy {
            env_vals += [env_copy(lv.val, ty, lv.kind)];
          }
          capture::cap_move {
            env_vals += [env_move(lv.val, ty, lv.kind)];
          }
          capture::cap_drop {
            bcx = drop_ty(bcx, lv.val, ty);
          }
        }
    }
    ret store_environment(bcx, copy bcx.fcx.lltyparams, env_vals, ck);
}

// Given an enclosing block context, a new function context, a closure type,
// and a list of upvars, generate code to load and populate the environment
// with the upvars and type descriptors.
fn load_environment(enclosing_cx: @block_ctxt,
                    fcx: @fn_ctxt,
                    cdata_ty: ty::t,
                    cap_vars: [capture::capture_var],
                    ck: ty::closure_kind) {
    let bcx = new_raw_block_ctxt(fcx, fcx.llloadenv);

    // Load a pointer to the closure data, skipping over the box header:
    let llcdata = base::opaque_box_body(bcx, cdata_ty, fcx.llenv);

    // Populate the type parameters from the environment. We need to
    // do this first because the tydescs are needed to index into
    // the bindings if they are dynamically sized.
    check type_is_tup_like(bcx, cdata_ty);
    let {bcx, val: lltydescs} = GEP_tup_like(bcx, cdata_ty, llcdata,
                                            [0, abi::closure_body_ty_params]);
    let off = 0;
    for tp in copy enclosing_cx.fcx.lltyparams {
        let tydesc = Load(bcx, GEPi(bcx, lltydescs, [0, off]));
        off += 1;
        let dicts = option::map(tp.dicts, {|dicts|
            let rslt = [];
            for dict in dicts {
                let dict = Load(bcx, GEPi(bcx, lltydescs, [0, off]));
                rslt += [PointerCast(bcx, dict, T_ptr(T_dict()))];
                off += 1;
            }
            rslt
        });
        fcx.lltyparams += [{desc: tydesc, dicts: dicts}];
    }

    // Populate the upvars from the environment.
    let i = 0u;
    vec::iter(cap_vars) { |cap_var|
        alt cap_var.mode {
          capture::cap_drop { /* ignore */ }
          _ {
            check type_is_tup_like(bcx, cdata_ty);
            let upvarptr =
                GEP_tup_like(bcx, cdata_ty, llcdata,
                             [0, abi::closure_body_bindings, i as int]);
            bcx = upvarptr.bcx;
            let llupvarptr = upvarptr.val;
            alt ck {
              ty::ck_block { llupvarptr = Load(bcx, llupvarptr); }
              ty::ck_uniq | ty::ck_box { }
            }
            let def_id = ast_util::def_id_of_def(cap_var.def);
            fcx.llupvars.insert(def_id.node, llupvarptr);
            i += 1u;
          }
        }
    }
}

fn trans_expr_fn(bcx: @block_ctxt,
                 proto: ast::proto,
                 decl: ast::fn_decl,
                 body: ast::blk,
                 sp: span,
                 id: ast::node_id,
                 cap_clause: ast::capture_clause,
                 dest: dest) -> @block_ctxt {
    if dest == ignore { ret bcx; }
    let ccx = bcx_ccx(bcx), bcx = bcx;
    let fty = node_id_type(bcx, id);
    let llfnty = type_of_fn_from_ty(ccx, fty, []);
    let sub_path = bcx.fcx.path + [path_name("anon")];
    let s = mangle_internal_name_by_path(ccx, sub_path);
    let llfn = decl_internal_cdecl_fn(ccx.llmod, s, llfnty);
    register_fn(ccx, sp, sub_path, "anon fn", [], id);

    let trans_closure_env = fn@(ck: ty::closure_kind) -> ValueRef {
        let cap_vars = capture::compute_capture_vars(
            ccx.tcx, id, proto, cap_clause);
        let {llbox, cdata_ty, bcx} = build_closure(bcx, cap_vars, ck);
        trans_closure(ccx, sub_path, decl, body, llfn, no_self, [],
                      bcx.fcx.param_substs, id, {|fcx|
            load_environment(bcx, fcx, cdata_ty, cap_vars, ck);
        });
        llbox
    };

    let closure = alt proto {
      ast::proto_any | ast::proto_block { trans_closure_env(ty::ck_block) }
      ast::proto_box { trans_closure_env(ty::ck_box) }
      ast::proto_uniq { trans_closure_env(ty::ck_uniq) }
      ast::proto_bare {
        trans_closure(ccx, sub_path, decl, body, llfn, no_self, [], none,
                      id, {|_fcx|});
        C_null(T_opaque_box_ptr(ccx))
      }
    };
    fill_fn_pair(bcx, get_dest_addr(dest), llfn, closure);
    ret bcx;
}

fn trans_bind(cx: @block_ctxt, f: @ast::expr, args: [option<@ast::expr>],
              id: ast::node_id, dest: dest) -> @block_ctxt {
    let f_res = trans_callee(cx, f);
    ret trans_bind_1(cx, expr_ty(cx, f), f_res, args,
                     ty::node_id_to_type(bcx_tcx(cx), id), dest);
}

fn trans_bind_1(cx: @block_ctxt, outgoing_fty: ty::t,
                f_res: lval_maybe_callee,
                args: [option<@ast::expr>], pair_ty: ty::t,
                dest: dest) -> @block_ctxt {
    let bound: [@ast::expr] = [];
    for argopt: option<@ast::expr> in args {
        alt argopt { none { } some(e) { bound += [e]; } }
    }
    let bcx = f_res.bcx;
    if dest == ignore {
        for ex in bound { bcx = trans_expr(bcx, ex, ignore); }
        ret bcx;
    }

    // Figure out which tydescs we need to pass, if any.
    let (outgoing_fty_real, lltydescs, param_bounds) = alt f_res.generic {
      none { (outgoing_fty, [], @[]) }
      some(ginfo) {
        let tds = [], orig = 0u;
        vec::iter2(ginfo.tydescs, *ginfo.param_bounds) {|td, bounds|
            tds += [td];
            for bound in *bounds {
                alt bound {
                  ty::bound_iface(_) {
                    let dict = impl::get_dict(
                        bcx, option::get(ginfo.origins)[orig]);
                    tds += [PointerCast(bcx, dict.val, val_ty(td))];
                    orig += 1u;
                    bcx = dict.bcx;
                  }
                  _ {}
                }
            }
        }
        lazily_emit_all_generic_info_tydesc_glues(cx, ginfo);
        (ginfo.item_type, tds, ginfo.param_bounds)
      }
    };

    if vec::len(bound) == 0u && vec::len(lltydescs) == 0u {
        // Trivial 'binding': just return the closure
        let lv = lval_maybe_callee_to_lval(f_res, pair_ty);
        bcx = lv.bcx;
        ret memmove_ty(bcx, get_dest_addr(dest), lv.val, pair_ty);
    }
    let closure = alt f_res.env {
      null_env { none }
      _ { let (_, cl) = maybe_add_env(cx, f_res); some(cl) }
    };

    // FIXME: should follow from a precondition on trans_bind_1
    let ccx = bcx_ccx(cx);
    check (type_has_static_size(ccx, outgoing_fty));

    // Arrange for the bound function to live in the first binding spot
    // if the function is not statically known.
    let (env_vals, target_res) = alt closure {
      some(cl) {
        // Cast the function we are binding to be the type that the
        // closure will expect it to have. The type the closure knows
        // about has the type parameters substituted with the real types.
        let llclosurety = T_ptr(type_of(ccx, outgoing_fty));
        let src_loc = PointerCast(bcx, cl, llclosurety);
        ([env_copy(src_loc, pair_ty, owned)], none)
      }
      none { ([], some(f_res.val)) }
    };

    // Actually construct the closure
    let {llbox, cdata_ty, bcx} = store_environment(
        bcx, vec::map(lltydescs, {|d| {desc: d, dicts: none}}),
        env_vals + vec::map(bound, {|x| env_expr(x, expr_ty(bcx, x))}),
        ty::ck_box);

    // Make thunk
    let llthunk = trans_bind_thunk(
        cx.fcx.ccx, cx.fcx.path, pair_ty, outgoing_fty_real, args,
        cdata_ty, *param_bounds, target_res);

    // Fill the function pair
    fill_fn_pair(bcx, get_dest_addr(dest), llthunk.val, llbox);
    ret bcx;
}

fn make_null_test(
    in_bcx: @block_ctxt,
    ptr: ValueRef,
    blk: fn(@block_ctxt) -> @block_ctxt)
    -> @block_ctxt {
    let not_null_bcx = new_sub_block_ctxt(in_bcx, "not null");
    let next_bcx = new_sub_block_ctxt(in_bcx, "next");
    let null_test = IsNull(in_bcx, ptr);
    CondBr(in_bcx, null_test, next_bcx.llbb, not_null_bcx.llbb);
    let not_null_bcx = blk(not_null_bcx);
    Br(not_null_bcx, next_bcx.llbb);
    ret next_bcx;
}

fn make_fn_glue(
    cx: @block_ctxt,
    v: ValueRef,
    t: ty::t,
    glue_fn: fn@(@block_ctxt, v: ValueRef, t: ty::t) -> @block_ctxt)
    -> @block_ctxt {
    let bcx = cx;
    let tcx = bcx_tcx(cx);

    let fn_env = fn@(ck: ty::closure_kind) -> @block_ctxt {
        let box_cell_v = GEPi(cx, v, [0, abi::fn_field_box]);
        let box_ptr_v = Load(cx, box_cell_v);
        make_null_test(cx, box_ptr_v) {|bcx|
            let closure_ty = ty::mk_opaque_closure_ptr(tcx, ck);
            glue_fn(bcx, box_cell_v, closure_ty)
        }
    };

    ret alt ty::struct(tcx, t) {
      ty::ty_fn({proto: ast::proto_bare, _}) |
      ty::ty_fn({proto: ast::proto_block, _}) |
      ty::ty_fn({proto: ast::proto_any, _}) { bcx }
      ty::ty_fn({proto: ast::proto_uniq, _}) { fn_env(ty::ck_uniq) }
      ty::ty_fn({proto: ast::proto_box, _}) { fn_env(ty::ck_box) }
      _ { fail "make_fn_glue invoked on non-function type" }
    };
}

fn make_opaque_cbox_take_glue(
    bcx: @block_ctxt,
    ck: ty::closure_kind,
    cboxptr: ValueRef)     // ptr to ptr to the opaque closure
    -> @block_ctxt {
    // Easy cases:
    alt ck {
      ty::ck_block { ret bcx; }
      ty::ck_box { ret incr_refcnt_of_boxed(bcx, Load(bcx, cboxptr)); }
      ty::ck_uniq { /* hard case: */ }
    }

    // Hard case, a deep copy:
    let ccx = bcx_ccx(bcx);
    let tcx = bcx_tcx(bcx);
    let llopaquecboxty = T_opaque_box_ptr(ccx);
    let cbox_in = Load(bcx, cboxptr);
    make_null_test(bcx, cbox_in) {|bcx|
        // Load the size from the type descr found in the cbox
        let cbox_in = PointerCast(bcx, cbox_in, llopaquecboxty);
        let tydescptr = GEPi(bcx, cbox_in, [0, abi::box_field_tydesc]);
        let tydesc = Load(bcx, tydescptr);
        let tydesc = PointerCast(bcx, tydesc, T_ptr(ccx.tydesc_type));
        let sz = Load(bcx, GEPi(bcx, tydesc, [0, abi::tydesc_field_size]));

        // Adjust sz to account for the rust_opaque_box header fields
        let sz = Add(bcx, sz, base::llsize_of(ccx, T_box_header(ccx)));

        // Allocate memory, update original ptr, and copy existing data
        let malloc = ccx.upcalls.shared_malloc;
        let cbox_out = Call(bcx, malloc, [sz, tydesc]);
        let cbox_out = PointerCast(bcx, cbox_out, llopaquecboxty);
        let {bcx, val: _} = call_memmove(bcx, cbox_out, cbox_in, sz);
        Store(bcx, cbox_out, cboxptr);

        // Take the (deeply cloned) type descriptor
        let tydesc_out = GEPi(bcx, cbox_out, [0, abi::box_field_tydesc]);
        let bcx = take_ty(bcx, tydesc_out, mk_tydesc_ty(tcx, ty::ck_uniq));

        // Take the data in the tuple
        let ti = none;
        let cdata_out = GEPi(bcx, cbox_out, [0, abi::box_field_body]);
        call_tydesc_glue_full(bcx, cdata_out, tydesc,
                              abi::tydesc_field_take_glue, ti);
        bcx
    }
}

fn make_opaque_cbox_drop_glue(
    bcx: @block_ctxt,
    ck: ty::closure_kind,
    cboxptr: ValueRef)     // ptr to the opaque closure
    -> @block_ctxt {
    alt ck {
      ty::ck_block { bcx }
      ty::ck_box {
        decr_refcnt_maybe_free(bcx, Load(bcx, cboxptr),
                               ty::mk_opaque_closure_ptr(bcx_tcx(bcx), ck))
      }
      ty::ck_uniq {
        free_ty(bcx, Load(bcx, cboxptr),
                ty::mk_opaque_closure_ptr(bcx_tcx(bcx), ck))
      }
    }
}

fn make_opaque_cbox_free_glue(
    bcx: @block_ctxt,
    ck: ty::closure_kind,
    cbox: ValueRef)     // ptr to the opaque closure
    -> @block_ctxt {
    alt ck {
      ty::ck_block { ret bcx; }
      ty::ck_box | ty::ck_uniq { /* hard cases: */ }
    }

    let ccx = bcx_ccx(bcx);
    let tcx = bcx_tcx(bcx);
    make_null_test(bcx, cbox) {|bcx|
        // Load the type descr found in the cbox
        let lltydescty = T_ptr(ccx.tydesc_type);
        let cbox = PointerCast(bcx, cbox, T_opaque_cbox_ptr(ccx));
        let tydescptr = GEPi(bcx, cbox, [0, abi::box_field_tydesc]);
        let tydesc = Load(bcx, tydescptr);
        let tydesc = PointerCast(bcx, tydesc, lltydescty);

        // Drop the tuple data then free the descriptor
        let ti = none;
        let cdata = GEPi(bcx, cbox, [0, abi::box_field_body]);
        call_tydesc_glue_full(bcx, cdata, tydesc,
                              abi::tydesc_field_drop_glue, ti);

        // Free the ty descr (if necc) and the box itself
        alt ck {
          ty::ck_block { fail "Impossible"; }
          ty::ck_box {
            trans_free(bcx, cbox)
          }
          ty::ck_uniq {
            let bcx = free_ty(bcx, tydesc, mk_tydesc_ty(tcx, ck));
            trans_shared_free(bcx, cbox)
          }
        }
    }
}

// pth is cx.path
fn trans_bind_thunk(ccx: @crate_ctxt,
                    path: path,
                    incoming_fty: ty::t,
                    outgoing_fty: ty::t,
                    args: [option<@ast::expr>],
                    cdata_ty: ty::t,
                    param_bounds: [ty::param_bounds],
                    target_fn: option<ValueRef>)
    -> {val: ValueRef, ty: TypeRef} {

    // If we supported constraints on record fields, we could make the
    // constraints for this function:
    /*
    : returns_non_ty_var(ccx, outgoing_fty),
      type_has_static_size(ccx, incoming_fty) ->
    */
    // but since we don't, we have to do the checks at the beginning.
    let tcx = ccx.tcx;
    check type_has_static_size(ccx, incoming_fty);

    #debug["trans_bind_thunk[incoming_fty=%s,outgoing_fty=%s,\
            cdata_ty=%s,param_bounds=%?]",
           ty_to_str(tcx, incoming_fty),
           ty_to_str(tcx, outgoing_fty),
           ty_to_str(tcx, cdata_ty),
           param_bounds];

    // Here we're not necessarily constructing a thunk in the sense of
    // "function with no arguments".  The result of compiling 'bind f(foo,
    // bar, baz)' would be a thunk that, when called, applies f to those
    // arguments and returns the result.  But we're stretching the meaning of
    // the word "thunk" here to also mean the result of compiling, say, 'bind
    // f(foo, _, baz)', or any other bind expression that binds f and leaves
    // some (or all) of the arguments unbound.

    // Here, 'incoming_fty' is the type of the entire bind expression, while
    // 'outgoing_fty' is the type of the function that is having some of its
    // arguments bound.  If f is a function that takes three arguments of type
    // int and returns int, and we're translating, say, 'bind f(3, _, 5)',
    // then outgoing_fty is the type of f, which is (int, int, int) -> int,
    // and incoming_fty is the type of 'bind f(3, _, 5)', which is int -> int.

    // Once translated, the entire bind expression will be the call f(foo,
    // bar, baz) wrapped in a (so-called) thunk that takes 'bar' as its
    // argument and that has bindings of 'foo' to 3 and 'baz' to 5 and a
    // pointer to 'f' all saved in its environment.  So, our job is to
    // construct and return that thunk.

    // Give the thunk a name, type, and value.
    let s = mangle_internal_name_by_path_and_seq(ccx, path, "thunk");
    let llthunk_ty = get_pair_fn_ty(type_of(ccx, incoming_fty));
    let llthunk = decl_internal_cdecl_fn(ccx.llmod, s, llthunk_ty);

    // Create a new function context and block context for the thunk, and hold
    // onto a pointer to the first block in the function for later use.
    let fcx = new_fn_ctxt(ccx, path, llthunk, none);
    let bcx = new_top_block_ctxt(fcx, none);
    let lltop = bcx.llbb;
    // Since we might need to construct derived tydescs that depend on
    // our bound tydescs, we need to load tydescs out of the environment
    // before derived tydescs are constructed. To do this, we load them
    // in the load_env block.
    let l_bcx = new_raw_block_ctxt(fcx, fcx.llloadenv);

    // The 'llenv' that will arrive in the thunk we're creating is an
    // environment that will contain the values of its arguments and a
    // pointer to the original function.  This environment is always
    // stored like an opaque box (see big comment at the header of the
    // file), so we load the body body, which contains the type descr
    // and cached data.
    let llcdata = base::opaque_box_body(l_bcx, cdata_ty, fcx.llenv);

    // "target", in this context, means the function that's having some of its
    // arguments bound and that will be called inside the thunk we're
    // creating.  (In our running example, target is the function f.)  Pick
    // out the pointer to the target function from the environment. The
    // target function lives in the first binding spot.
    let (lltargetfn, lltargetenv, starting_idx) = alt target_fn {
      some(fptr) {
        (fptr, llvm::LLVMGetUndef(T_opaque_cbox_ptr(ccx)), 0)
      }
      none {
        // Silly check
        check type_is_tup_like(bcx, cdata_ty);
        let {bcx: cx, val: pair} =
            GEP_tup_like(bcx, cdata_ty, llcdata,
                         [0, abi::closure_body_bindings, 0]);
        let lltargetenv =
            Load(cx, GEPi(cx, pair, [0, abi::fn_field_box]));
        let lltargetfn = Load
            (cx, GEPi(cx, pair, [0, abi::fn_field_code]));
        bcx = cx;
        (lltargetfn, lltargetenv, 1)
      }
    };

    // And then, pick out the target function's own environment.  That's what
    // we'll use as the environment the thunk gets.

    // Get f's return type, which will also be the return type of the entire
    // bind expression.
    let outgoing_ret_ty = ty::ty_fn_ret(ccx.tcx, outgoing_fty);

    // Get the types of the arguments to f.
    let outgoing_args = ty::ty_fn_args(ccx.tcx, outgoing_fty);

    // The 'llretptr' that will arrive in the thunk we're creating also needs
    // to be the correct type.  Cast it to f's return type, if necessary.
    let llretptr = fcx.llretptr;
    if ty::type_contains_params(ccx.tcx, outgoing_ret_ty) {
        check non_ty_var(ccx, outgoing_ret_ty);
        let llretty = type_of_inner(ccx, outgoing_ret_ty);
        llretptr = PointerCast(bcx, llretptr, T_ptr(llretty));
    }

    // Set up the three implicit arguments to the thunk.
    let llargs: [ValueRef] = [llretptr, lltargetenv];

    // Copy in the type parameters.
    check type_is_tup_like(l_bcx, cdata_ty);
    let {bcx: l_bcx, val: param_record} =
        GEP_tup_like(l_bcx, cdata_ty, llcdata,
                     [0, abi::closure_body_ty_params]);
    let off = 0;
    for param in param_bounds {
        let dsc = Load(l_bcx, GEPi(l_bcx, param_record, [0, off])),
            dicts = none;
        llargs += [dsc];
        off += 1;
        for bound in *param {
            alt bound {
              ty::bound_iface(_) {
                let dict = Load(l_bcx, GEPi(l_bcx, param_record, [0, off]));
                dict = PointerCast(l_bcx, dict, T_ptr(T_dict()));
                llargs += [dict];
                off += 1;
                dicts = some(alt dicts {
                  none { [dict] }
                  some(ds) { ds + [dict] }
                });
              }
              _ {}
            }
        }
        fcx.lltyparams += [{desc: dsc, dicts: dicts}];
    }

    let a: uint = 2u; // retptr, env come first
    let b: int = starting_idx;
    let outgoing_arg_index: uint = 0u;
    let llout_arg_tys: [TypeRef] =
        type_of_explicit_args(ccx, outgoing_args);
    for arg: option<@ast::expr> in args {
        let out_arg = outgoing_args[outgoing_arg_index];
        let llout_arg_ty = llout_arg_tys[outgoing_arg_index];
        alt arg {
          // Arg provided at binding time; thunk copies it from
          // closure.
          some(e) {
            // Silly check
            check type_is_tup_like(bcx, cdata_ty);
            let bound_arg =
                GEP_tup_like(bcx, cdata_ty, llcdata,
                             [0, abi::closure_body_bindings, b]);
            bcx = bound_arg.bcx;
            let val = bound_arg.val;

            alt ty::resolved_mode(tcx, out_arg.mode) {
              ast::by_val {
                val = Load(bcx, val);
              }
              ast::by_copy {
                let {bcx: cx, val: alloc} = alloc_ty(bcx, out_arg.ty);
                bcx = memmove_ty(cx, alloc, val, out_arg.ty);
                bcx = take_ty(bcx, alloc, out_arg.ty);
                val = alloc;
              }
              ast::by_ref | ast::by_mut_ref | ast::by_move { }
            }

            // If the type is parameterized, then we need to cast the
            // type we actually have to the parameterized out type.
            if ty::type_contains_params(ccx.tcx, out_arg.ty) {
                val = PointerCast(bcx, val, llout_arg_ty);
            }
            llargs += [val];
            b += 1;
          }

          // Arg will be provided when the thunk is invoked.
          none {
            let arg: ValueRef = llvm::LLVMGetParam(llthunk, a as c_uint);
            if ty::type_contains_params(ccx.tcx, out_arg.ty) {
                arg = PointerCast(bcx, arg, llout_arg_ty);
            }
            llargs += [arg];
            a += 1u;
          }
        }
        outgoing_arg_index += 1u;
    }

    // Cast the outgoing function to the appropriate type.
    // This is necessary because the type of the function that we have
    // in the closure does not know how many type descriptors the function
    // needs to take.
    let lltargetty =
        type_of_fn_from_ty(ccx, outgoing_fty, param_bounds);
    lltargetfn = PointerCast(bcx, lltargetfn, T_ptr(lltargetty));
    Call(bcx, lltargetfn, llargs);
    build_return(bcx);
    finish_fn(fcx, lltop);
    ret {val: llthunk, ty: llthunk_ty};
}
