// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use back::{link, abi};
use lib::llvm::{ValueRef};
use lib;
use middle::trans::base::*;
use middle::trans::cabi;
use middle::trans::cabi_x86;
use middle::trans::cabi_x86_64;
use middle::trans::cabi_arm;
use middle::trans::cabi_mips;
use middle::trans::build::*;
use middle::trans::callee::*;
use middle::trans::common::*;
use middle::trans::datum::*;
use middle::trans::expr::Ignore;
use middle::trans::machine::llsize_of;
use middle::trans::glue;
use middle::trans::machine;
use middle::trans::type_of::*;
use middle::trans::type_of;
use middle::ty;
use middle::ty::FnSig;
use util::ppaux::ty_to_str;

use core::uint;
use core::vec;
use syntax::codemap::span;
use syntax::{ast, ast_util};
use syntax::{attr, ast_map};
use syntax::opt_vec;
use syntax::parse::token::special_idents;
use syntax::parse::token;
use syntax::abi::{X86, X86_64, Arm, Mips};
use syntax::abi::{RustIntrinsic, Rust, Stdcall, Fastcall,
                  Cdecl, Aapcs, C};
use middle::trans::type_::Type;

fn abi_info(ccx: @mut CrateContext) -> @cabi::ABIInfo {
    return match ccx.sess.targ_cfg.arch {
        X86 => cabi_x86::abi_info(ccx),
        X86_64 => cabi_x86_64::abi_info(),
        Arm => cabi_arm::abi_info(),
        Mips => cabi_mips::abi_info(),
    }
}

pub fn link_name(ccx: &CrateContext, i: @ast::foreign_item) -> @str {
     match attr::first_attr_value_str_by_name(i.attrs, "link_name") {
        None => ccx.sess.str_of(i.ident),
        Some(ln) => ln,
    }
}

struct ShimTypes {
    fn_sig: ty::FnSig,

    /// LLVM types that will appear on the foreign function
    llsig: LlvmSignature,

    /// True if there is a return value (not bottom, not unit)
    ret_def: bool,

    /// Type of the struct we will use to shuttle values back and forth.
    /// This is always derived from the llsig.
    bundle_ty: Type,

    /// Type of the shim function itself.
    shim_fn_ty: Type,

    /// Adapter object for handling native ABI rules (trust me, you
    /// don't want to know).
    fn_ty: cabi::FnType
}

struct LlvmSignature {
    llarg_tys: ~[Type],
    llret_ty: Type,
    sret: bool,
}

fn foreign_signature(ccx: @mut CrateContext, fn_sig: &ty::FnSig)
                     -> LlvmSignature {
    /*!
     * The ForeignSignature is the LLVM types of the arguments/return type
     * of a function.  Note that these LLVM types are not quite the same
     * as the LLVM types would be for a native Rust function because foreign
     * functions just plain ignore modes.  They also don't pass aggregate
     * values by pointer like we do.
     */

    let llarg_tys = fn_sig.inputs.map(|arg_ty| type_of(ccx, *arg_ty));
    let llret_ty = type_of::type_of(ccx, fn_sig.output);
    LlvmSignature {
        llarg_tys: llarg_tys,
        llret_ty: llret_ty,
        sret: !ty::type_is_immediate(fn_sig.output),
    }
}

fn shim_types(ccx: @mut CrateContext, id: ast::node_id) -> ShimTypes {
    let fn_sig = match ty::get(ty::node_id_to_type(ccx.tcx, id)).sty {
        ty::ty_bare_fn(ref fn_ty) => copy fn_ty.sig,
        _ => ccx.sess.bug("c_arg_and_ret_lltys called on non-function type")
    };
    let llsig = foreign_signature(ccx, &fn_sig);
    let bundle_ty = Type::struct_(llsig.llarg_tys + [llsig.llret_ty.ptr_to()], false);
    let ret_def = !ty::type_is_bot(fn_sig.output) &&
                  !ty::type_is_nil(fn_sig.output);
    let fn_ty = abi_info(ccx).compute_info(llsig.llarg_tys, llsig.llret_ty, ret_def);
    ShimTypes {
        fn_sig: fn_sig,
        llsig: llsig,
        ret_def: ret_def,
        bundle_ty: bundle_ty,
        shim_fn_ty: Type::func([bundle_ty.ptr_to()], &Type::void()),
        fn_ty: fn_ty
    }
}

type shim_arg_builder<'self> =
    &'self fn(bcx: block, tys: &ShimTypes,
              llargbundle: ValueRef) -> ~[ValueRef];

type shim_ret_builder<'self> =
    &'self fn(bcx: block, tys: &ShimTypes,
              llargbundle: ValueRef,
              llretval: ValueRef);

fn build_shim_fn_(ccx: @mut CrateContext,
                  shim_name: ~str,
                  llbasefn: ValueRef,
                  tys: &ShimTypes,
                  cc: lib::llvm::CallConv,
                  arg_builder: shim_arg_builder,
                  ret_builder: shim_ret_builder)
               -> ValueRef {
    let llshimfn = decl_internal_cdecl_fn(
        ccx.llmod, shim_name, tys.shim_fn_ty);

    // Declare the body of the shim function:
    let fcx = new_fn_ctxt(ccx, ~[], llshimfn, tys.fn_sig.output, None);
    let bcx = top_scope_block(fcx, None);
    let lltop = bcx.llbb;

    let llargbundle = get_param(llshimfn, 0u);
    let llargvals = arg_builder(bcx, tys, llargbundle);

    // Create the call itself and store the return value:
    let llretval = CallWithConv(bcx, llbasefn, llargvals, cc);

    ret_builder(bcx, tys, llargbundle, llretval);

    // Don't finish up the function in the usual way, because this doesn't
    // follow the normal Rust calling conventions.
    tie_up_header_blocks(fcx, lltop);

    let ret_cx = raw_block(fcx, false, fcx.llreturn);
    RetVoid(ret_cx);

    return llshimfn;
}

type wrap_arg_builder<'self> = &'self fn(bcx: block,
                                         tys: &ShimTypes,
                                         llwrapfn: ValueRef,
                                         llargbundle: ValueRef);

type wrap_ret_builder<'self> = &'self fn(bcx: block,
                                         tys: &ShimTypes,
                                         llargbundle: ValueRef);

fn build_wrap_fn_(ccx: @mut CrateContext,
                  tys: &ShimTypes,
                  llshimfn: ValueRef,
                  llwrapfn: ValueRef,
                  shim_upcall: ValueRef,
                  needs_c_return: bool,
                  arg_builder: wrap_arg_builder,
                  ret_builder: wrap_ret_builder) {
    let _icx = push_ctxt("foreign::build_wrap_fn_");
    let fcx = new_fn_ctxt(ccx, ~[], llwrapfn, tys.fn_sig.output, None);

    // Patch up the return type if it's not immediate and we're returning via
    // the C ABI.
    if needs_c_return && !ty::type_is_immediate(tys.fn_sig.output) {
        let lloutputtype = type_of::type_of(fcx.ccx, tys.fn_sig.output);
        fcx.llretptr = Some(alloca(raw_block(fcx, false, fcx.llstaticallocas),
                                   lloutputtype));
    }

    let bcx = top_scope_block(fcx, None);
    let lltop = bcx.llbb;

    // Allocate the struct and write the arguments into it.
    let llargbundle = alloca(bcx, tys.bundle_ty);
    arg_builder(bcx, tys, llwrapfn, llargbundle);

    // Create call itself.
    let llshimfnptr = PointerCast(bcx, llshimfn, Type::i8p());
    let llrawargbundle = PointerCast(bcx, llargbundle, Type::i8p());
    Call(bcx, shim_upcall, [llrawargbundle, llshimfnptr]);
    ret_builder(bcx, tys, llargbundle);

    // Perform a custom version of `finish_fn`. First, tie up the header
    // blocks.
    tie_up_header_blocks(fcx, lltop);

    // Then return according to the C ABI.
    let return_context = raw_block(fcx, false, fcx.llreturn);

    let llfunctiontype = val_ty(llwrapfn);
    let llfunctiontype = llfunctiontype.element_type();
    let return_type = llfunctiontype.return_type();
    if return_type.kind() == ::lib::llvm::Void {
        // XXX: This might be wrong if there are any functions for which
        // the C ABI specifies a void output pointer and the Rust ABI
        // does not.
        RetVoid(return_context);
    } else {
        // Cast if we have to...
        // XXX: This is ugly.
        let llretptr = BitCast(return_context, fcx.llretptr.get(), return_type.ptr_to());
        Ret(return_context, Load(return_context, llretptr));
    }
}

// For each foreign function F, we generate a wrapper function W and a shim
// function S that all work together.  The wrapper function W is the function
// that other rust code actually invokes.  Its job is to marshall the
// arguments into a struct.  It then uses a small bit of assembly to switch
// over to the C stack and invoke the shim function.  The shim function S then
// unpacks the arguments from the struct and invokes the actual function F
// according to its specified calling convention.
//
// Example: Given a foreign c-stack function F(x: X, y: Y) -> Z,
// we generate a wrapper function W that looks like:
//
//    void W(Z* dest, void *env, X x, Y y) {
//        struct { X x; Y y; Z *z; } args = { x, y, z };
//        call_on_c_stack_shim(S, &args);
//    }
//
// The shim function S then looks something like:
//
//     void S(struct { X x; Y y; Z *z; } *args) {
//         *args->z = F(args->x, args->y);
//     }
//
// However, if the return type of F is dynamically sized or of aggregate type,
// the shim function looks like:
//
//     void S(struct { X x; Y y; Z *z; } *args) {
//         F(args->z, args->x, args->y);
//     }
//
// Note: on i386, the layout of the args struct is generally the same
// as the desired layout of the arguments on the C stack.  Therefore,
// we could use upcall_alloc_c_stack() to allocate the `args`
// structure and switch the stack pointer appropriately to avoid a
// round of copies.  (In fact, the shim function itself is
// unnecessary). We used to do this, in fact, and will perhaps do so
// in the future.
pub fn trans_foreign_mod(ccx: @mut CrateContext,
                         path: &ast_map::path,
                         foreign_mod: &ast::foreign_mod) {
    let _icx = push_ctxt("foreign::trans_foreign_mod");

    let arch = ccx.sess.targ_cfg.arch;
    let abi = match foreign_mod.abis.for_arch(arch) {
        None => {
            ccx.sess.fatal(
                fmt!("No suitable ABI for target architecture \
                      in module %s",
                     ast_map::path_to_str(*path,
                                          ccx.sess.intr())));
        }

        Some(abi) => abi,
    };

    for foreign_mod.items.each |&foreign_item| {
        match foreign_item.node {
            ast::foreign_item_fn(*) => {
                let id = foreign_item.id;
                match abi {
                    RustIntrinsic => {
                        // Intrinsics are emitted by monomorphic fn
                    }

                    Rust => {
                        // FIXME(#3678) Implement linking to foreign fns with Rust ABI
                        ccx.sess.unimpl(
                            fmt!("Foreign functions with Rust ABI"));
                    }

                    Stdcall => {
                        build_foreign_fn(ccx, id, foreign_item,
                                         lib::llvm::X86StdcallCallConv);
                    }

                    Fastcall => {
                        build_foreign_fn(ccx, id, foreign_item,
                                         lib::llvm::X86FastcallCallConv);
                    }

                    Cdecl => {
                        // FIXME(#3678) should really be more specific
                        build_foreign_fn(ccx, id, foreign_item,
                                         lib::llvm::CCallConv);
                    }

                    Aapcs => {
                        // FIXME(#3678) should really be more specific
                        build_foreign_fn(ccx, id, foreign_item,
                                         lib::llvm::CCallConv);
                    }

                    C => {
                        build_foreign_fn(ccx, id, foreign_item,
                                         lib::llvm::CCallConv);
                    }
                }
            }
            ast::foreign_item_const(*) => {
                let ident = token::ident_to_str(&foreign_item.ident);
                ccx.item_symbols.insert(foreign_item.id, /* bad */ident.to_owned());
            }
        }
    }

    fn build_foreign_fn(ccx: @mut CrateContext,
                        id: ast::node_id,
                        foreign_item: @ast::foreign_item,
                        cc: lib::llvm::CallConv) {
        let llwrapfn = get_item_val(ccx, id);
        let tys = shim_types(ccx, id);
        if attr::attrs_contains_name(foreign_item.attrs, "rust_stack") {
            build_direct_fn(ccx, llwrapfn, foreign_item,
                            &tys, cc);
        } else if attr::attrs_contains_name(foreign_item.attrs, "fast_ffi") {
            build_fast_ffi_fn(ccx, llwrapfn, foreign_item, &tys, cc);
        } else {
            let llshimfn = build_shim_fn(ccx, foreign_item, &tys, cc);
            build_wrap_fn(ccx, &tys, llshimfn, llwrapfn);
        }
    }

    fn build_shim_fn(ccx: @mut CrateContext,
                     foreign_item: @ast::foreign_item,
                     tys: &ShimTypes,
                     cc: lib::llvm::CallConv)
                  -> ValueRef {
        /*!
         *
         * Build S, from comment above:
         *
         *     void S(struct { X x; Y y; Z *z; } *args) {
         *         F(args->z, args->x, args->y);
         *     }
         */

        let _icx = push_ctxt("foreign::build_shim_fn");

        fn build_args(bcx: block, tys: &ShimTypes, llargbundle: ValueRef)
                   -> ~[ValueRef] {
            let _icx = push_ctxt("foreign::shim::build_args");
            tys.fn_ty.build_shim_args(bcx, tys.llsig.llarg_tys, llargbundle)
        }

        fn build_ret(bcx: block,
                     tys: &ShimTypes,
                     llargbundle: ValueRef,
                     llretval: ValueRef) {
            let _icx = push_ctxt("foreign::shim::build_ret");
            tys.fn_ty.build_shim_ret(bcx,
                                     tys.llsig.llarg_tys,
                                     tys.ret_def,
                                     llargbundle,
                                     llretval);
            build_return(bcx);
        }

        let lname = link_name(ccx, foreign_item);
        let llbasefn = base_fn(ccx, lname, tys, cc);
        // Name the shim function
        let shim_name = fmt!("%s__c_stack_shim", lname);
        build_shim_fn_(ccx,
                       shim_name,
                       llbasefn,
                       tys,
                       cc,
                       build_args,
                       build_ret)
    }

    fn base_fn(ccx: &CrateContext,
               lname: &str,
               tys: &ShimTypes,
               cc: lib::llvm::CallConv)
               -> ValueRef {
        // Declare the "prototype" for the base function F:
        do tys.fn_ty.decl_fn |fnty| {
            decl_fn(ccx.llmod, lname, cc, fnty)
        }
    }

    // FIXME (#2535): this is very shaky and probably gets ABIs wrong all
    // over the place
    fn build_direct_fn(ccx: @mut CrateContext,
                       decl: ValueRef,
                       item: @ast::foreign_item,
                       tys: &ShimTypes,
                       cc: lib::llvm::CallConv) {
        debug!("build_direct_fn(%s)", link_name(ccx, item));

        let fcx = new_fn_ctxt(ccx, ~[], decl, tys.fn_sig.output, None);
        let bcx = top_scope_block(fcx, None);
        let lltop = bcx.llbb;
        let llbasefn = base_fn(ccx, link_name(ccx, item), tys, cc);
        let ty = ty::lookup_item_type(ccx.tcx,
                                      ast_util::local_def(item.id)).ty;
        let ret_ty = ty::ty_fn_ret(ty);
        let args = vec::from_fn(ty::ty_fn_args(ty).len(), |i| {
            get_param(decl, fcx.arg_pos(i))
        });
        let retval = Call(bcx, llbasefn, args);
        if !ty::type_is_nil(ret_ty) && !ty::type_is_bot(ret_ty) {
            Store(bcx, retval, fcx.llretptr.get());
        }
        build_return(bcx);
        finish_fn(fcx, lltop);
    }

    // FIXME (#2535): this is very shaky and probably gets ABIs wrong all
    // over the place
    fn build_fast_ffi_fn(ccx: @mut CrateContext,
                         decl: ValueRef,
                         item: @ast::foreign_item,
                         tys: &ShimTypes,
                         cc: lib::llvm::CallConv) {
        debug!("build_fast_ffi_fn(%s)", link_name(ccx, item));

        let fcx = new_fn_ctxt(ccx, ~[], decl, tys.fn_sig.output, None);
        let bcx = top_scope_block(fcx, None);
        let lltop = bcx.llbb;
        let llbasefn = base_fn(ccx, link_name(ccx, item), tys, cc);
        set_no_inline(fcx.llfn);
        set_fixed_stack_segment(fcx.llfn);
        let ty = ty::lookup_item_type(ccx.tcx,
                                      ast_util::local_def(item.id)).ty;
        let ret_ty = ty::ty_fn_ret(ty);
        let args = vec::from_fn(ty::ty_fn_args(ty).len(), |i| {
            get_param(decl, fcx.arg_pos(i))
        });
        let retval = Call(bcx, llbasefn, args);
        if !ty::type_is_nil(ret_ty) && !ty::type_is_bot(ret_ty) {
            Store(bcx, retval, fcx.llretptr.get());
        }
        build_return(bcx);
        finish_fn(fcx, lltop);
    }

    fn build_wrap_fn(ccx: @mut CrateContext,
                     tys: &ShimTypes,
                     llshimfn: ValueRef,
                     llwrapfn: ValueRef) {
        /*!
         *
         * Build W, from comment above:
         *
         *     void W(Z* dest, void *env, X x, Y y) {
         *         struct { X x; Y y; Z *z; } args = { x, y, z };
         *         call_on_c_stack_shim(S, &args);
         *     }
         *
         * One thing we have to be very careful of is to
         * account for the Rust modes.
         */

        let _icx = push_ctxt("foreign::build_wrap_fn");

        build_wrap_fn_(ccx,
                       tys,
                       llshimfn,
                       llwrapfn,
                       ccx.upcalls.call_shim_on_c_stack,
                       false,
                       build_args,
                       build_ret);

        fn build_args(bcx: block,
                      tys: &ShimTypes,
                      llwrapfn: ValueRef,
                      llargbundle: ValueRef) {
            let _icx = push_ctxt("foreign::wrap::build_args");
            let ccx = bcx.ccx();
            let n = tys.llsig.llarg_tys.len();
            for uint::range(0, n) |i| {
                let arg_i = bcx.fcx.arg_pos(i);
                let mut llargval = get_param(llwrapfn, arg_i);

                // In some cases, Rust will pass a pointer which the
                // native C type doesn't have.  In that case, just
                // load the value from the pointer.
                if type_of::arg_is_indirect(ccx, &tys.fn_sig.inputs[i]) {
                    llargval = Load(bcx, llargval);
                }

                store_inbounds(bcx, llargval, llargbundle, [0u, i]);
            }

            for bcx.fcx.llretptr.iter().advance |&retptr| {
                store_inbounds(bcx, retptr, llargbundle, [0u, n]);
            }
        }

        fn build_ret(bcx: block,
                     shim_types: &ShimTypes,
                     llargbundle: ValueRef) {
            let _icx = push_ctxt("foreign::wrap::build_ret");
            let arg_count = shim_types.fn_sig.inputs.len();
            for bcx.fcx.llretptr.iter().advance |&retptr| {
                let llretptr = load_inbounds(bcx, llargbundle, [0, arg_count]);
                Store(bcx, Load(bcx, llretptr), retptr);
            }
            build_return(bcx);
        }
    }
}

pub fn trans_intrinsic(ccx: @mut CrateContext,
                       decl: ValueRef,
                       item: @ast::foreign_item,
                       path: ast_map::path,
                       substs: @param_substs,
                       attributes: &[ast::attribute],
                       ref_id: Option<ast::node_id>) {
    debug!("trans_intrinsic(item.ident=%s)", ccx.sess.str_of(item.ident));

    let output_type = ty::ty_fn_ret(ty::node_id_to_type(ccx.tcx, item.id));

    let fcx = new_fn_ctxt_w_id(ccx,
                               path,
                               decl,
                               item.id,
                               output_type,
                               None,
                               Some(substs),
                               Some(item.span));

    // Set the fixed stack segment flag if necessary.
    if attr::attrs_contains_name(attributes, "fixed_stack_segment") {
        set_fixed_stack_segment(fcx.llfn);
    }

    let mut bcx = top_scope_block(fcx, None);
    let lltop = bcx.llbb;
    let first_real_arg = fcx.arg_pos(0u);

    let nm = ccx.sess.str_of(item.ident);
    let name = nm.as_slice();

    // This requires that atomic intrinsics follow a specific naming pattern:
    // "atomic_<operation>[_<ordering>], and no ordering means SeqCst
    if name.starts_with("atomic_") {
        let split : ~[&str] = name.split_iter('_').collect();
        assert!(split.len() >= 2, "Atomic intrinsic not correct format");
        let order = if split.len() == 2 {
            lib::llvm::SequentiallyConsistent
        } else {
            match split[2] {
                "relaxed" => lib::llvm::Monotonic,
                "acq"     => lib::llvm::Acquire,
                "rel"     => lib::llvm::Release,
                "acqrel"  => lib::llvm::AcquireRelease,
                _ => ccx.sess.fatal("Unknown ordering in atomic intrinsic")
            }
        };

        match split[1] {
            "cxchg" => {
                let old = AtomicCmpXchg(bcx, get_param(decl, first_real_arg),
                                        get_param(decl, first_real_arg + 1u),
                                        get_param(decl, first_real_arg + 2u),
                                        order);
                Store(bcx, old, fcx.llretptr.get());
            }
            "load" => {
                let old = AtomicLoad(bcx, get_param(decl, first_real_arg),
                                     order);
                Store(bcx, old, fcx.llretptr.get());
            }
            "store" => {
                AtomicStore(bcx, get_param(decl, first_real_arg + 1u),
                            get_param(decl, first_real_arg),
                            order);
            }
            op => {
                // These are all AtomicRMW ops
                let atom_op = match op {
                    "xchg"  => lib::llvm::Xchg,
                    "xadd"  => lib::llvm::Add,
                    "xsub"  => lib::llvm::Sub,
                    "and"   => lib::llvm::And,
                    "nand"  => lib::llvm::Nand,
                    "or"    => lib::llvm::Or,
                    "xor"   => lib::llvm::Xor,
                    "max"   => lib::llvm::Max,
                    "min"   => lib::llvm::Min,
                    "umax"  => lib::llvm::UMax,
                    "umin"  => lib::llvm::UMin,
                    _ => ccx.sess.fatal("Unknown atomic operation")
                };

                let old = AtomicRMW(bcx, atom_op, get_param(decl, first_real_arg),
                                    get_param(decl, first_real_arg + 1u),
                                    order);
                Store(bcx, old, fcx.llretptr.get());
            }
        }

        build_return(bcx);
        finish_fn(fcx, lltop);

        return;
    }

    match name {
        "size_of" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            Store(bcx, C_uint(ccx, machine::llsize_of_real(ccx, lltp_ty)),
                  fcx.llretptr.get());
        }
        "move_val" => {
            // Create a datum reflecting the value being moved.
            // Use `appropriate_mode` so that the datum is by ref
            // if the value is non-immediate. Note that, with
            // intrinsics, there are no argument cleanups to
            // concern ourselves with.
            let tp_ty = substs.tys[0];
            let mode = appropriate_mode(tp_ty);
            let src = Datum {val: get_param(decl, first_real_arg + 1u),
                             ty: tp_ty, mode: mode};
            bcx = src.move_to(bcx, DROP_EXISTING,
                              get_param(decl, first_real_arg));
        }
        "move_val_init" => {
            // See comments for `"move_val"`.
            let tp_ty = substs.tys[0];
            let mode = appropriate_mode(tp_ty);
            let src = Datum {val: get_param(decl, first_real_arg + 1u),
                             ty: tp_ty, mode: mode};
            bcx = src.move_to(bcx, INIT, get_param(decl, first_real_arg));
        }
        "min_align_of" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            Store(bcx, C_uint(ccx, machine::llalign_of_min(ccx, lltp_ty)),
                  fcx.llretptr.get());
        }
        "pref_align_of"=> {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            Store(bcx, C_uint(ccx, machine::llalign_of_pref(ccx, lltp_ty)),
                  fcx.llretptr.get());
        }
        "get_tydesc" => {
            let tp_ty = substs.tys[0];
            let static_ti = get_tydesc(ccx, tp_ty);
            glue::lazily_emit_all_tydesc_glue(ccx, static_ti);

            // FIXME (#3727): change this to ccx.tydesc_ty.ptr_to() when the
            // core::sys copy of the get_tydesc interface dies off.
            let td = PointerCast(bcx, static_ti.tydesc, Type::nil().ptr_to());
            Store(bcx, td, fcx.llretptr.get());
        }
        "init" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            if !ty::type_is_nil(tp_ty) {
                Store(bcx, C_null(lltp_ty), fcx.llretptr.get());
            }
        }
        "uninit" => {
            // Do nothing, this is effectively a no-op
        }
        "forget" => {}
        "transmute" => {
            let (in_type, out_type) = (substs.tys[0], substs.tys[1]);
            let llintype = type_of::type_of(ccx, in_type);
            let llouttype = type_of::type_of(ccx, out_type);

            let in_type_size = machine::llbitsize_of_real(ccx, llintype);
            let out_type_size = machine::llbitsize_of_real(ccx, llouttype);
            if in_type_size != out_type_size {
                let sp = match ccx.tcx.items.get_copy(&ref_id.get()) {
                    ast_map::node_expr(e) => e.span,
                    _ => fail!("transmute has non-expr arg"),
                };
                let pluralize = |n| if 1u == n { "" } else { "s" };
                ccx.sess.span_fatal(sp,
                                    fmt!("transmute called on types with \
                                          different sizes: %s (%u bit%s) to \
                                          %s (%u bit%s)",
                                         ty_to_str(ccx.tcx, in_type),
                                         in_type_size,
                                         pluralize(in_type_size),
                                         ty_to_str(ccx.tcx, out_type),
                                         out_type_size,
                                         pluralize(out_type_size)));
            }

            if !ty::type_is_nil(out_type) {
                // NB: Do not use a Load and Store here. This causes massive
                // code bloat when `transmute` is used on large structural
                // types.
                let lldestptr = fcx.llretptr.get();
                let lldestptr = PointerCast(bcx, lldestptr, Type::i8p());

                let llsrcval = get_param(decl, first_real_arg);
                let llsrcptr = if ty::type_is_immediate(in_type) {
                    let llsrcptr = alloca(bcx, llintype);
                    Store(bcx, llsrcval, llsrcptr);
                    llsrcptr
                } else {
                    llsrcval
                };
                let llsrcptr = PointerCast(bcx, llsrcptr, Type::i8p());

                let llsize = llsize_of(ccx, llintype);
                call_memcpy(bcx, lldestptr, llsrcptr, llsize, 1);
            }
        }
        "needs_drop" => {
            let tp_ty = substs.tys[0];
            Store(bcx,
                  C_bool(ty::type_needs_drop(ccx.tcx, tp_ty)),
                  fcx.llretptr.get());
        }
        "visit_tydesc" => {
            let td = get_param(decl, first_real_arg);
            let visitor = get_param(decl, first_real_arg + 1u);
            //let llvisitorptr = alloca(bcx, val_ty(visitor));
            //Store(bcx, visitor, llvisitorptr);
            let td = PointerCast(bcx, td, ccx.tydesc_type.ptr_to());
            glue::call_tydesc_glue_full(bcx, visitor, td,
                                        abi::tydesc_field_visit_glue, None);
        }
        "frame_address" => {
            let frameaddress = ccx.intrinsics.get_copy(& &"llvm.frameaddress");
            let frameaddress_val = Call(bcx, frameaddress, [C_i32(0i32)]);
            let star_u8 = ty::mk_imm_ptr(
                bcx.tcx(),
                ty::mk_mach_uint(ast::ty_u8));
            let fty = ty::mk_closure(bcx.tcx(), ty::ClosureTy {
                purity: ast::impure_fn,
                sigil: ast::BorrowedSigil,
                onceness: ast::Many,
                region: ty::re_bound(ty::br_anon(0)),
                bounds: ty::EmptyBuiltinBounds(),
                sig: FnSig {
                    bound_lifetime_names: opt_vec::Empty,
                    inputs: ~[ star_u8 ],
                    output: ty::mk_nil()
                }
            });
            let datum = Datum {val: get_param(decl, first_real_arg),
                               mode: ByRef(ZeroMem), ty: fty};
            let arg_vals = ~[frameaddress_val];
            bcx = trans_call_inner(
                bcx, None, fty, ty::mk_nil(),
                |bcx| Callee {bcx: bcx, data: Closure(datum)},
                ArgVals(arg_vals), Ignore, DontAutorefArg);
        }
        "morestack_addr" => {
            // XXX This is a hack to grab the address of this particular
            // native function. There should be a general in-language
            // way to do this
            let llfty = type_of_fn(bcx.ccx(), [], ty::mk_nil());
            let morestack_addr = decl_cdecl_fn(
                bcx.ccx().llmod, "__morestack", llfty);
            let morestack_addr = PointerCast(bcx, morestack_addr, Type::nil().ptr_to());
            Store(bcx, morestack_addr, fcx.llretptr.get());
        }
        "memcpy32" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            let align = C_i32(machine::llalign_of_min(ccx, lltp_ty) as i32);
            let size = C_i32(machine::llsize_of_real(ccx, lltp_ty) as i32);

            let dst_ptr = PointerCast(bcx, get_param(decl, first_real_arg), Type::i8p());
            let src_ptr = PointerCast(bcx, get_param(decl, first_real_arg + 1), Type::i8p());
            let count = get_param(decl, first_real_arg + 2);
            let volatile = C_i1(false);
            let llfn = bcx.ccx().intrinsics.get_copy(& &"llvm.memcpy.p0i8.p0i8.i32");
            Call(bcx, llfn, [dst_ptr, src_ptr, Mul(bcx, size, count), align, volatile]);
        }
        "memcpy64" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            let align = C_i32(machine::llalign_of_min(ccx, lltp_ty) as i32);
            let size = C_i64(machine::llsize_of_real(ccx, lltp_ty) as i64);

            let dst_ptr = PointerCast(bcx, get_param(decl, first_real_arg), Type::i8p());
            let src_ptr = PointerCast(bcx, get_param(decl, first_real_arg + 1), Type::i8p());
            let count = get_param(decl, first_real_arg + 2);
            let volatile = C_i1(false);
            let llfn = bcx.ccx().intrinsics.get_copy(& &"llvm.memcpy.p0i8.p0i8.i64");
            Call(bcx, llfn, [dst_ptr, src_ptr, Mul(bcx, size, count), align, volatile]);
        }
        "memmove32" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            let align = C_i32(machine::llalign_of_min(ccx, lltp_ty) as i32);
            let size = C_i32(machine::llsize_of_real(ccx, lltp_ty) as i32);

            let dst_ptr = PointerCast(bcx, get_param(decl, first_real_arg), Type::i8p());
            let src_ptr = PointerCast(bcx, get_param(decl, first_real_arg + 1), Type::i8p());
            let count = get_param(decl, first_real_arg + 2);
            let volatile = C_i1(false);
            let llfn = bcx.ccx().intrinsics.get_copy(& &"llvm.memmove.p0i8.p0i8.i32");
            Call(bcx, llfn, [dst_ptr, src_ptr, Mul(bcx, size, count), align, volatile]);
        }
        "memmove64" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            let align = C_i32(machine::llalign_of_min(ccx, lltp_ty) as i32);
            let size = C_i64(machine::llsize_of_real(ccx, lltp_ty) as i64);

            let dst_ptr = PointerCast(bcx, get_param(decl, first_real_arg), Type::i8p());
            let src_ptr = PointerCast(bcx, get_param(decl, first_real_arg + 1), Type::i8p());
            let count = get_param(decl, first_real_arg + 2);
            let volatile = C_i1(false);
            let llfn = bcx.ccx().intrinsics.get_copy(& &"llvm.memmove.p0i8.p0i8.i64");
            Call(bcx, llfn, [dst_ptr, src_ptr, Mul(bcx, size, count), align, volatile]);
        }
        "memset32" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            let align = C_i32(machine::llalign_of_min(ccx, lltp_ty) as i32);
            let size = C_i32(machine::llsize_of_real(ccx, lltp_ty) as i32);

            let dst_ptr = PointerCast(bcx, get_param(decl, first_real_arg), Type::i8p());
            let val = get_param(decl, first_real_arg + 1);
            let count = get_param(decl, first_real_arg + 2);
            let volatile = C_i1(false);
            let llfn = bcx.ccx().intrinsics.get_copy(& &"llvm.memset.p0i8.i32");
            Call(bcx, llfn, [dst_ptr, val, Mul(bcx, size, count), align, volatile]);
        }
        "memset64" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            let align = C_i32(machine::llalign_of_min(ccx, lltp_ty) as i32);
            let size = C_i64(machine::llsize_of_real(ccx, lltp_ty) as i64);

            let dst_ptr = PointerCast(bcx, get_param(decl, first_real_arg), Type::i8p());
            let val = get_param(decl, first_real_arg + 1);
            let count = get_param(decl, first_real_arg + 2);
            let volatile = C_i1(false);
            let llfn = bcx.ccx().intrinsics.get_copy(& &"llvm.memset.p0i8.i64");
            Call(bcx, llfn, [dst_ptr, val, Mul(bcx, size, count), align, volatile]);
        }
        "sqrtf32" => {
            let x = get_param(decl, first_real_arg);
            let sqrtf = ccx.intrinsics.get_copy(& &"llvm.sqrt.f32");
            Store(bcx, Call(bcx, sqrtf, [x]), fcx.llretptr.get());
        }
        "sqrtf64" => {
            let x = get_param(decl, first_real_arg);
            let sqrtf = ccx.intrinsics.get_copy(& &"llvm.sqrt.f64");
            Store(bcx, Call(bcx, sqrtf, [x]), fcx.llretptr.get());
        }
        "powif32" => {
            let a = get_param(decl, first_real_arg);
            let x = get_param(decl, first_real_arg + 1u);
            let powif = ccx.intrinsics.get_copy(& &"llvm.powi.f32");
            Store(bcx, Call(bcx, powif, [a, x]), fcx.llretptr.get());
        }
        "powif64" => {
            let a = get_param(decl, first_real_arg);
            let x = get_param(decl, first_real_arg + 1u);
            let powif = ccx.intrinsics.get_copy(& &"llvm.powi.f64");
            Store(bcx, Call(bcx, powif, [a, x]), fcx.llretptr.get());
        }
        "sinf32" => {
            let x = get_param(decl, first_real_arg);
            let sinf = ccx.intrinsics.get_copy(& &"llvm.sin.f32");
            Store(bcx, Call(bcx, sinf, [x]), fcx.llretptr.get());
        }
        "sinf64" => {
            let x = get_param(decl, first_real_arg);
            let sinf = ccx.intrinsics.get_copy(& &"llvm.sin.f64");
            Store(bcx, Call(bcx, sinf, [x]), fcx.llretptr.get());
        }
        "cosf32" => {
            let x = get_param(decl, first_real_arg);
            let cosf = ccx.intrinsics.get_copy(& &"llvm.cos.f32");
            Store(bcx, Call(bcx, cosf, [x]), fcx.llretptr.get());
        }
        "cosf64" => {
            let x = get_param(decl, first_real_arg);
            let cosf = ccx.intrinsics.get_copy(& &"llvm.cos.f64");
            Store(bcx, Call(bcx, cosf, [x]), fcx.llretptr.get());
        }
        "powf32" => {
            let a = get_param(decl, first_real_arg);
            let x = get_param(decl, first_real_arg + 1u);
            let powf = ccx.intrinsics.get_copy(& &"llvm.pow.f32");
            Store(bcx, Call(bcx, powf, [a, x]), fcx.llretptr.get());
        }
        "powf64" => {
            let a = get_param(decl, first_real_arg);
            let x = get_param(decl, first_real_arg + 1u);
            let powf = ccx.intrinsics.get_copy(& &"llvm.pow.f64");
            Store(bcx, Call(bcx, powf, [a, x]), fcx.llretptr.get());
        }
        "expf32" => {
            let x = get_param(decl, first_real_arg);
            let expf = ccx.intrinsics.get_copy(& &"llvm.exp.f32");
            Store(bcx, Call(bcx, expf, [x]), fcx.llretptr.get());
        }
        "expf64" => {
            let x = get_param(decl, first_real_arg);
            let expf = ccx.intrinsics.get_copy(& &"llvm.exp.f64");
            Store(bcx, Call(bcx, expf, [x]), fcx.llretptr.get());
        }
        "exp2f32" => {
            let x = get_param(decl, first_real_arg);
            let exp2f = ccx.intrinsics.get_copy(& &"llvm.exp2.f32");
            Store(bcx, Call(bcx, exp2f, [x]), fcx.llretptr.get());
        }
        "exp2f64" => {
            let x = get_param(decl, first_real_arg);
            let exp2f = ccx.intrinsics.get_copy(& &"llvm.exp2.f64");
            Store(bcx, Call(bcx, exp2f, [x]), fcx.llretptr.get());
        }
        "logf32" => {
            let x = get_param(decl, first_real_arg);
            let logf = ccx.intrinsics.get_copy(& &"llvm.log.f32");
            Store(bcx, Call(bcx, logf, [x]), fcx.llretptr.get());
        }
        "logf64" => {
            let x = get_param(decl, first_real_arg);
            let logf = ccx.intrinsics.get_copy(& &"llvm.log.f64");
            Store(bcx, Call(bcx, logf, [x]), fcx.llretptr.get());
        }
        "log10f32" => {
            let x = get_param(decl, first_real_arg);
            let log10f = ccx.intrinsics.get_copy(& &"llvm.log10.f32");
            Store(bcx, Call(bcx, log10f, [x]), fcx.llretptr.get());
        }
        "log10f64" => {
            let x = get_param(decl, first_real_arg);
            let log10f = ccx.intrinsics.get_copy(& &"llvm.log10.f64");
            Store(bcx, Call(bcx, log10f, [x]), fcx.llretptr.get());
        }
        "log2f32" => {
            let x = get_param(decl, first_real_arg);
            let log2f = ccx.intrinsics.get_copy(& &"llvm.log2.f32");
            Store(bcx, Call(bcx, log2f, [x]), fcx.llretptr.get());
        }
        "log2f64" => {
            let x = get_param(decl, first_real_arg);
            let log2f = ccx.intrinsics.get_copy(& &"llvm.log2.f64");
            Store(bcx, Call(bcx, log2f, [x]), fcx.llretptr.get());
        }
        "fmaf32" => {
            let a = get_param(decl, first_real_arg);
            let b = get_param(decl, first_real_arg + 1u);
            let c = get_param(decl, first_real_arg + 2u);
            let fmaf = ccx.intrinsics.get_copy(& &"llvm.fma.f32");
            Store(bcx, Call(bcx, fmaf, [a, b, c]), fcx.llretptr.get());
        }
        "fmaf64" => {
            let a = get_param(decl, first_real_arg);
            let b = get_param(decl, first_real_arg + 1u);
            let c = get_param(decl, first_real_arg + 2u);
            let fmaf = ccx.intrinsics.get_copy(& &"llvm.fma.f64");
            Store(bcx, Call(bcx, fmaf, [a, b, c]), fcx.llretptr.get());
        }
        "fabsf32" => {
            let x = get_param(decl, first_real_arg);
            let fabsf = ccx.intrinsics.get_copy(& &"llvm.fabs.f32");
            Store(bcx, Call(bcx, fabsf, [x]), fcx.llretptr.get());
        }
        "fabsf64" => {
            let x = get_param(decl, first_real_arg);
            let fabsf = ccx.intrinsics.get_copy(& &"llvm.fabs.f64");
            Store(bcx, Call(bcx, fabsf, [x]), fcx.llretptr.get());
        }
        "floorf32" => {
            let x = get_param(decl, first_real_arg);
            let floorf = ccx.intrinsics.get_copy(& &"llvm.floor.f32");
            Store(bcx, Call(bcx, floorf, [x]), fcx.llretptr.get());
        }
        "floorf64" => {
            let x = get_param(decl, first_real_arg);
            let floorf = ccx.intrinsics.get_copy(& &"llvm.floor.f64");
            Store(bcx, Call(bcx, floorf, [x]), fcx.llretptr.get());
        }
        "ceilf32" => {
            let x = get_param(decl, first_real_arg);
            let ceilf = ccx.intrinsics.get_copy(& &"llvm.ceil.f32");
            Store(bcx, Call(bcx, ceilf, [x]), fcx.llretptr.get());
        }
        "ceilf64" => {
            let x = get_param(decl, first_real_arg);
            let ceilf = ccx.intrinsics.get_copy(& &"llvm.ceil.f64");
            Store(bcx, Call(bcx, ceilf, [x]), fcx.llretptr.get());
        }
        "truncf32" => {
            let x = get_param(decl, first_real_arg);
            let truncf = ccx.intrinsics.get_copy(& &"llvm.trunc.f32");
            Store(bcx, Call(bcx, truncf, [x]), fcx.llretptr.get());
        }
        "truncf64" => {
            let x = get_param(decl, first_real_arg);
            let truncf = ccx.intrinsics.get_copy(& &"llvm.trunc.f64");
            Store(bcx, Call(bcx, truncf, [x]), fcx.llretptr.get());
        }
        "ctpop8" => {
            let x = get_param(decl, first_real_arg);
            let ctpop = ccx.intrinsics.get_copy(& &"llvm.ctpop.i8");
            Store(bcx, Call(bcx, ctpop, [x]), fcx.llretptr.get())
        }
        "ctpop16" => {
            let x = get_param(decl, first_real_arg);
            let ctpop = ccx.intrinsics.get_copy(& &"llvm.ctpop.i16");
            Store(bcx, Call(bcx, ctpop, [x]), fcx.llretptr.get())
        }
        "ctpop32" => {
            let x = get_param(decl, first_real_arg);
            let ctpop = ccx.intrinsics.get_copy(& &"llvm.ctpop.i32");
            Store(bcx, Call(bcx, ctpop, [x]), fcx.llretptr.get())
        }
        "ctpop64" => {
            let x = get_param(decl, first_real_arg);
            let ctpop = ccx.intrinsics.get_copy(& &"llvm.ctpop.i64");
            Store(bcx, Call(bcx, ctpop, [x]), fcx.llretptr.get())
        }
        "ctlz8" => {
            let x = get_param(decl, first_real_arg);
            let y = C_i1(false);
            let ctlz = ccx.intrinsics.get_copy(& &"llvm.ctlz.i8");
            Store(bcx, Call(bcx, ctlz, [x, y]), fcx.llretptr.get())
        }
        "ctlz16" => {
            let x = get_param(decl, first_real_arg);
            let y = C_i1(false);
            let ctlz = ccx.intrinsics.get_copy(& &"llvm.ctlz.i16");
            Store(bcx, Call(bcx, ctlz, [x, y]), fcx.llretptr.get())
        }
        "ctlz32" => {
            let x = get_param(decl, first_real_arg);
            let y = C_i1(false);
            let ctlz = ccx.intrinsics.get_copy(& &"llvm.ctlz.i32");
            Store(bcx, Call(bcx, ctlz, [x, y]), fcx.llretptr.get())
        }
        "ctlz64" => {
            let x = get_param(decl, first_real_arg);
            let y = C_i1(false);
            let ctlz = ccx.intrinsics.get_copy(& &"llvm.ctlz.i64");
            Store(bcx, Call(bcx, ctlz, [x, y]), fcx.llretptr.get())
        }
        "cttz8" => {
            let x = get_param(decl, first_real_arg);
            let y = C_i1(false);
            let cttz = ccx.intrinsics.get_copy(& &"llvm.cttz.i8");
            Store(bcx, Call(bcx, cttz, [x, y]), fcx.llretptr.get())
        }
        "cttz16" => {
            let x = get_param(decl, first_real_arg);
            let y = C_i1(false);
            let cttz = ccx.intrinsics.get_copy(& &"llvm.cttz.i16");
            Store(bcx, Call(bcx, cttz, [x, y]), fcx.llretptr.get())
        }
        "cttz32" => {
            let x = get_param(decl, first_real_arg);
            let y = C_i1(false);
            let cttz = ccx.intrinsics.get_copy(& &"llvm.cttz.i32");
            Store(bcx, Call(bcx, cttz, [x, y]), fcx.llretptr.get())
        }
        "cttz64" => {
            let x = get_param(decl, first_real_arg);
            let y = C_i1(false);
            let cttz = ccx.intrinsics.get_copy(& &"llvm.cttz.i64");
            Store(bcx, Call(bcx, cttz, [x, y]), fcx.llretptr.get())
        }
        "bswap16" => {
            let x = get_param(decl, first_real_arg);
            let cttz = ccx.intrinsics.get_copy(& &"llvm.bswap.i16");
            Store(bcx, Call(bcx, cttz, [x]), fcx.llretptr.get())
        }
        "bswap32" => {
            let x = get_param(decl, first_real_arg);
            let cttz = ccx.intrinsics.get_copy(& &"llvm.bswap.i32");
            Store(bcx, Call(bcx, cttz, [x]), fcx.llretptr.get())
        }
        "bswap64" => {
            let x = get_param(decl, first_real_arg);
            let cttz = ccx.intrinsics.get_copy(& &"llvm.bswap.i64");
            Store(bcx, Call(bcx, cttz, [x]), fcx.llretptr.get())
        }
        _ => {
            // Could we make this an enum rather than a string? does it get
            // checked earlier?
            ccx.sess.span_bug(item.span, "unknown intrinsic");
        }
    }
    build_return(bcx);
    finish_fn(fcx, lltop);
}

/**
 * Translates a "crust" fn, meaning a Rust fn that can be called
 * from C code.  In this case, we have to perform some adaptation
 * to (1) switch back to the Rust stack and (2) adapt the C calling
 * convention to our own.
 *
 * Example: Given a crust fn F(x: X, y: Y) -> Z, we generate a
 * Rust function R as normal:
 *
 *    void R(Z* dest, void *env, X x, Y y) {...}
 *
 * and then we generate a wrapper function W that looks like:
 *
 *    Z W(X x, Y y) {
 *        struct { X x; Y y; Z *z; } args = { x, y, z };
 *        call_on_c_stack_shim(S, &args);
 *    }
 *
 * Note that the wrapper follows the foreign (typically "C") ABI.
 * The wrapper is the actual "value" of the foreign fn.  Finally,
 * we generate a shim function S that looks like:
 *
 *     void S(struct { X x; Y y; Z *z; } *args) {
 *         R(args->z, NULL, args->x, args->y);
 *     }
 */
pub fn trans_foreign_fn(ccx: @mut CrateContext,
                        path: ast_map::path,
                        decl: &ast::fn_decl,
                        body: &ast::blk,
                        llwrapfn: ValueRef,
                        id: ast::node_id) {
    let _icx = push_ctxt("foreign::build_foreign_fn");

    fn build_rust_fn(ccx: @mut CrateContext,
                     path: ast_map::path,
                     decl: &ast::fn_decl,
                     body: &ast::blk,
                     id: ast::node_id)
                  -> ValueRef {
        let _icx = push_ctxt("foreign::foreign::build_rust_fn");
        let t = ty::node_id_to_type(ccx.tcx, id);
        // XXX: Bad copy.
        let ps = link::mangle_internal_name_by_path(
            ccx, vec::append_one(copy path, ast_map::path_name(
                special_idents::clownshoe_abi
            )));
        let llty = type_of_fn_from_ty(ccx, t);
        let llfndecl = decl_internal_cdecl_fn(ccx.llmod, ps, llty);
        trans_fn(ccx,
                 path,
                 decl,
                 body,
                 llfndecl,
                 no_self,
                 None,
                 id,
                 None,
                 []);
        return llfndecl;
    }

    fn build_shim_fn(ccx: @mut CrateContext,
                     path: ast_map::path,
                     llrustfn: ValueRef,
                     tys: &ShimTypes)
                     -> ValueRef {
        /*!
         *
         * Generate the shim S:
         *
         *     void S(struct { X x; Y y; Z *z; } *args) {
         *         R(args->z, NULL, &args->x, args->y);
         *     }
         *
         * One complication is that we must adapt to the Rust
         * calling convention, which introduces indirection
         * in some cases.  To demonstrate this, I wrote one of the
         * entries above as `&args->x`, because presumably `X` is
         * one of those types that is passed by pointer in Rust.
         */

        let _icx = push_ctxt("foreign::foreign::build_shim_fn");

        fn build_args(bcx: block, tys: &ShimTypes, llargbundle: ValueRef)
                      -> ~[ValueRef] {
            let _icx = push_ctxt("foreign::extern::shim::build_args");
            let ccx = bcx.ccx();
            let mut llargvals = ~[];
            let mut i = 0u;
            let n = tys.fn_sig.inputs.len();

            if !ty::type_is_immediate(tys.fn_sig.output) {
                let llretptr = load_inbounds(bcx, llargbundle, [0u, n]);
                llargvals.push(llretptr);
            }

            let llenvptr = C_null(Type::opaque_box(bcx.ccx()).ptr_to());
            llargvals.push(llenvptr);
            while i < n {
                // Get a pointer to the argument:
                let mut llargval = GEPi(bcx, llargbundle, [0u, i]);

                if !type_of::arg_is_indirect(ccx, &tys.fn_sig.inputs[i]) {
                    // If Rust would pass this by value, load the value.
                    llargval = Load(bcx, llargval);
                }

                llargvals.push(llargval);
                i += 1u;
            }
            return llargvals;
        }

        fn build_ret(bcx: block,
                     shim_types: &ShimTypes,
                     llargbundle: ValueRef,
                     llretval: ValueRef) {
            if bcx.fcx.llretptr.is_some() && ty::type_is_immediate(shim_types.fn_sig.output) {
                // Write the value into the argument bundle.
                let arg_count = shim_types.fn_sig.inputs.len();
                let llretptr = load_inbounds(bcx,
                                             llargbundle,
                                             [0, arg_count]);
                Store(bcx, llretval, llretptr);
            } else {
                // NB: The return pointer in the Rust ABI function is wired
                // directly into the return slot in the shim struct.
            }

            build_return(bcx);
        }

        let shim_name = link::mangle_internal_name_by_path(
            ccx,
            vec::append_one(path, ast_map::path_name(
                special_idents::clownshoe_stack_shim
            )));
        build_shim_fn_(ccx,
                       shim_name,
                       llrustfn,
                       tys,
                       lib::llvm::CCallConv,
                       build_args,
                       build_ret)
    }

    fn build_wrap_fn(ccx: @mut CrateContext,
                     llshimfn: ValueRef,
                     llwrapfn: ValueRef,
                     tys: &ShimTypes) {
        /*!
         *
         * Generate the wrapper W:
         *
         *    Z W(X x, Y y) {
         *        struct { X x; Y y; Z *z; } args = { x, y, z };
         *        call_on_c_stack_shim(S, &args);
         *    }
         */

        let _icx = push_ctxt("foreign::foreign::build_wrap_fn");

        build_wrap_fn_(ccx,
                       tys,
                       llshimfn,
                       llwrapfn,
                       ccx.upcalls.call_shim_on_rust_stack,
                       true,
                       build_args,
                       build_ret);

        fn build_args(bcx: block,
                      tys: &ShimTypes,
                      llwrapfn: ValueRef,
                      llargbundle: ValueRef) {
            let _icx = push_ctxt("foreign::foreign::wrap::build_args");
            tys.fn_ty.build_wrap_args(bcx,
                                      tys.llsig.llret_ty,
                                      llwrapfn,
                                      llargbundle);
        }

        fn build_ret(bcx: block, tys: &ShimTypes, llargbundle: ValueRef) {
            let _icx = push_ctxt("foreign::foreign::wrap::build_ret");
            tys.fn_ty.build_wrap_ret(bcx, tys.llsig.llarg_tys, llargbundle);
            build_return(bcx);
        }
    }

    let tys = shim_types(ccx, id);
    // The internal Rust ABI function - runs on the Rust stack
    // XXX: Bad copy.
    let llrustfn = build_rust_fn(ccx, copy path, decl, body, id);
    // The internal shim function - runs on the Rust stack
    let llshimfn = build_shim_fn(ccx, path, llrustfn, &tys);
    // The foreign C function - runs on the C stack
    build_wrap_fn(ccx, llshimfn, llwrapfn, &tys)
}

pub fn register_foreign_fn(ccx: @mut CrateContext,
                           sp: span,
                           path: ast_map::path,
                           node_id: ast::node_id,
                           attrs: &[ast::attribute])
                           -> ValueRef {
    let _icx = push_ctxt("foreign::register_foreign_fn");

    let t = ty::node_id_to_type(ccx.tcx, node_id);

    let tys = shim_types(ccx, node_id);
    do tys.fn_ty.decl_fn |fnty| {
        register_fn_fuller(ccx,
                           sp,
                           /*bad*/copy path,
                           node_id,
                           attrs,
                           t,
                           lib::llvm::CCallConv,
                           fnty)
    }
}
