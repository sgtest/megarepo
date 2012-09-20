// The classification code for the x86_64 ABI is taken from the clay language
// https://github.com/jckarter/clay/blob/master/compiler/src/externals.cpp

use driver::session::{session, arch_x86_64};
use syntax::codemap::span;
use libc::c_uint;
use syntax::{attr, ast_map};
use lib::llvm::{ llvm, TypeRef, ValueRef, Integer, Pointer, Float, Double,
    Struct, Array, ModuleRef, CallConv, Attribute,
    StructRetAttribute, ByValAttribute,
    SequentiallyConsistent, Acquire, Release, Xchg };
use syntax::{ast, ast_util};
use back::{link, abi};
use common::*;
use build::*;
use base::*;
use type_of::*;
use std::map::HashMap;
use util::ppaux::ty_to_str;
use datum::*;
use callee::*;
use expr::{Dest, Ignore};
use ty::{FnTyBase, FnMeta, FnSig};

export link_name, trans_foreign_mod, register_foreign_fn, trans_foreign_fn,
       trans_intrinsic;

enum x86_64_reg_class {
    no_class,
    integer_class,
    sse_fs_class,
    sse_fv_class,
    sse_ds_class,
    sse_dv_class,
    sse_int_class,
    sseup_class,
    x87_class,
    x87up_class,
    complex_x87_class,
    memory_class
}

#[cfg(stage0)]
impl x86_64_reg_class: cmp::Eq {
    pure fn eq(&&other: x86_64_reg_class) -> bool {
        (self as uint) == (other as uint)
    }
    pure fn ne(&&other: x86_64_reg_class) -> bool { !self.eq(other) }
}
#[cfg(stage1)]
#[cfg(stage2)]
impl x86_64_reg_class : cmp::Eq {
    pure fn eq(other: &x86_64_reg_class) -> bool {
        (self as uint) == ((*other) as uint)
    }
    pure fn ne(other: &x86_64_reg_class) -> bool { !self.eq(other) }
}

fn is_sse(++c: x86_64_reg_class) -> bool {
    return match c {
        sse_fs_class | sse_fv_class |
        sse_ds_class | sse_dv_class => true,
        _ => false
    };
}

fn is_ymm(cls: ~[x86_64_reg_class]) -> bool {
    let len = vec::len(cls);
    return (len > 2u &&
         is_sse(cls[0]) &&
         cls[1] == sseup_class &&
         cls[2] == sseup_class) ||
        (len > 3u &&
         is_sse(cls[1]) &&
         cls[2] == sseup_class &&
         cls[3] == sseup_class);
}

fn classify_ty(ty: TypeRef) -> ~[x86_64_reg_class] {
    fn align(off: uint, ty: TypeRef) -> uint {
        let a = ty_align(ty);
        return (off + a - 1u) / a * a;
    }

    fn struct_tys(ty: TypeRef) -> ~[TypeRef] {
        let n = llvm::LLVMCountStructElementTypes(ty);
        let elts = vec::from_elem(n as uint, ptr::null());
        do vec::as_imm_buf(elts) |buf, _len| {
            llvm::LLVMGetStructElementTypes(ty, buf);
        }
        return elts;
    }

    fn ty_align(ty: TypeRef) -> uint {
        return match llvm::LLVMGetTypeKind(ty) {
            Integer => {
                ((llvm::LLVMGetIntTypeWidth(ty) as uint) + 7) / 8
            }
            Pointer => 8,
            Float => 4,
            Double => 8,
            Struct => {
              do vec::foldl(0, struct_tys(ty)) |a, t| {
                  uint::max(a, ty_align(t))
              }
            }
            Array => {
                let elt = llvm::LLVMGetElementType(ty);
                ty_align(elt)
            }
            _ => fail ~"ty_size: unhandled type"
        };
    }

    fn ty_size(ty: TypeRef) -> uint {
        return match llvm::LLVMGetTypeKind(ty) {
            Integer => {
                ((llvm::LLVMGetIntTypeWidth(ty) as uint) + 7) / 8
            }
            Pointer => 8,
            Float => 4,
            Double => 8,
            Struct => {
              do vec::foldl(0, struct_tys(ty)) |s, t| {
                    s + ty_size(t)
                }
            }
            Array => {
              let len = llvm::LLVMGetArrayLength(ty) as uint;
              let elt = llvm::LLVMGetElementType(ty);
              let eltsz = ty_size(elt);
              len * eltsz
            }
            _ => fail ~"ty_size: unhandled type"
        };
    }

    fn all_mem(cls: ~[mut x86_64_reg_class]) {
        for uint::range(0, cls.len()) |i| {
            cls[i] = memory_class;
        }
    }

    fn unify(cls: ~[mut x86_64_reg_class],
             i: uint,
             newv: x86_64_reg_class) {
        if cls[i] == newv {
            return;
        } else if cls[i] == no_class {
            cls[i] = newv;
        } else if newv == no_class {
            return;
        } else if cls[i] == memory_class || newv == memory_class {
            cls[i] = memory_class;
        } else if cls[i] == integer_class || newv == integer_class {
            cls[i] = integer_class;
        } else if cls[i] == x87_class ||
                  cls[i] == x87up_class ||
                  cls[i] == complex_x87_class ||
                  newv == x87_class ||
                  newv == x87up_class ||
                  newv == complex_x87_class {
            cls[i] = memory_class;
        } else {
            cls[i] = newv;
        }
    }

    fn classify_struct(tys: ~[TypeRef],
                       cls: ~[mut x86_64_reg_class], i: uint,
                       off: uint) {
        if vec::is_empty(tys) {
            classify(T_i64(), cls, i, off);
        } else {
            let mut field_off = off;
            for vec::each(tys) |ty| {
                field_off = align(field_off, *ty);
                classify(*ty, cls, i, field_off);
                field_off += ty_size(*ty);
            }
        }
    }

    fn classify(ty: TypeRef,
                cls: ~[mut x86_64_reg_class], ix: uint,
                off: uint) {
        let t_align = ty_align(ty);
        let t_size = ty_size(ty);

        let misalign = off % t_align;
        if misalign != 0u {
            let mut i = off / 8u;
            let e = (off + t_size + 7u) / 8u;
            while i < e {
                unify(cls, ix + i, memory_class);
                i += 1u;
            }
            return;
        }

        match llvm::LLVMGetTypeKind(ty) as int {
            8 /* integer */ |
            12 /* pointer */ => {
                unify(cls, ix + off / 8u, integer_class);
            }
            2 /* float */ => {
                if off % 8u == 4u {
                    unify(cls, ix + off / 8u, sse_fv_class);
                } else {
                    unify(cls, ix + off / 8u, sse_fs_class);
                }
            }
            3 /* double */ => {
                unify(cls, ix + off / 8u, sse_ds_class);
            }
            10 /* struct */ => {
                classify_struct(struct_tys(ty), cls, ix, off);
            }
            11 /* array */ => {
                let elt = llvm::LLVMGetElementType(ty);
                let eltsz = ty_size(elt);
                let len = llvm::LLVMGetArrayLength(ty) as uint;
                let mut i = 0u;
                while i < len {
                    classify(elt, cls, ix, off + i * eltsz);
                    i += 1u;
                }
            }
            _ => fail ~"classify: unhandled type"
        }
    }

    fn fixup(ty: TypeRef, cls: ~[mut x86_64_reg_class]) {
        let mut i = 0u;
        let llty = llvm::LLVMGetTypeKind(ty) as int;
        let e = vec::len(cls);
        if vec::len(cls) > 2u &&
           (llty == 10 /* struct */ ||
            llty == 11 /* array */) {
            if is_sse(cls[i]) {
                i += 1u;
                while i < e {
                    if cls[i] != sseup_class {
                        all_mem(cls);
                        return;
                    }
                    i += 1u;
                }
            } else {
                all_mem(cls);
                return
            }
        } else {
            while i < e {
                if cls[i] == memory_class {
                    all_mem(cls);
                    return;
                }
                if cls[i] == x87up_class {
                    // for darwin
                    // cls[i] = sse_ds_class;
                    all_mem(cls);
                    return;
                }
                if cls[i] == sseup_class {
                    cls[i] = sse_int_class;
                } else if is_sse(cls[i]) {
                    i += 1;
                    while cls[i] == sseup_class { i += 1u; }
                } else if cls[i] == x87_class {
                    i += 1;
                    while cls[i] == x87up_class { i += 1u; }
                } else {
                    i += 1;
                }
            }
        }
    }

    let words = (ty_size(ty) + 7) / 8;
    let cls = vec::to_mut(vec::from_elem(words, no_class));
    if words > 4 {
        all_mem(cls);
        return vec::from_mut(move cls);
    }
    classify(ty, cls, 0, 0);
    fixup(ty, cls);
    return vec::from_mut(move cls);
}

fn llreg_ty(cls: ~[x86_64_reg_class]) -> TypeRef {
    fn llvec_len(cls: ~[x86_64_reg_class]) -> uint {
        let mut len = 1u;
        for vec::each(cls) |c| {
            if *c != sseup_class {
                break;
            }
            len += 1u;
        }
        return len;
    }

    let mut tys = ~[];
    let mut i = 0u;
    let e = vec::len(cls);
    while i < e {
        match cls[i] {
            integer_class => {
                vec::push(tys, T_i64());
            }
            sse_fv_class => {
                let vec_len = llvec_len(vec::tailn(cls, i + 1u)) * 2u;
                let vec_ty = llvm::LLVMVectorType(T_f32(),
                                                  vec_len as c_uint);
                vec::push(tys, vec_ty);
                i += vec_len;
                loop;
            }
            sse_fs_class => {
                vec::push(tys, T_f32());
            }
            sse_ds_class => {
                vec::push(tys, T_f64());
            }
            _ => fail ~"llregtype: unhandled class"
        }
        i += 1u;
    }
    return T_struct(tys);
}

type x86_64_llty = {
    cast: bool,
    ty: TypeRef
};

type x86_64_tys = {
    arg_tys: ~[x86_64_llty],
    ret_ty: x86_64_llty,
    attrs: ~[Option<Attribute>],
    sret: bool
};

fn x86_64_tys(atys: ~[TypeRef],
              rty: TypeRef,
              ret_def: bool) -> x86_64_tys {
    fn is_reg_ty(ty: TypeRef) -> bool {
        return match llvm::LLVMGetTypeKind(ty) as int {
            8 /* integer */ |
            12 /* pointer */ |
            2 /* float */ |
            3 /* double */ => true,
            _ => false
        };
    }

    fn is_pass_byval(cls: ~[x86_64_reg_class]) -> bool {
        return cls[0] == memory_class ||
            cls[0] == x87_class ||
            cls[0] == complex_x87_class;
    }

    fn is_ret_bysret(cls: ~[x86_64_reg_class]) -> bool {
        return cls[0] == memory_class;
    }

    fn x86_64_ty(ty: TypeRef,
                 is_mem_cls: fn(cls: ~[x86_64_reg_class]) -> bool,
                 attr: Attribute) -> (x86_64_llty, Option<Attribute>) {
        let mut cast = false;
        let mut ty_attr = option::None;
        let mut llty = ty;
        if !is_reg_ty(ty) {
            let cls = classify_ty(ty);
            if is_mem_cls(cls) {
                llty = T_ptr(ty);
                ty_attr = option::Some(attr);
            } else {
                cast = true;
                llty = llreg_ty(cls);
            }
        }
        return ({ cast: cast, ty: llty }, ty_attr);
    }

    let mut arg_tys = ~[];
    let mut attrs = ~[];
    for vec::each(atys) |t| {
        let (ty, attr) = x86_64_ty(*t, is_pass_byval, ByValAttribute);
        vec::push(arg_tys, ty);
        vec::push(attrs, attr);
    }
    let mut (ret_ty, ret_attr) = x86_64_ty(rty, is_ret_bysret,
                                       StructRetAttribute);
    let sret = option::is_some(ret_attr);
    if sret {
        arg_tys = vec::append(~[ret_ty], arg_tys);
        ret_ty = { cast:  false,
                   ty: T_void()
                 };
        attrs = vec::append(~[ret_attr], attrs);
    } else if !ret_def {
        ret_ty = { cast: false,
                   ty: T_void()
                 };
    }
    return {
        arg_tys: arg_tys,
        ret_ty: ret_ty,
        attrs: attrs,
        sret: sret
    };
}

fn decl_x86_64_fn(tys: x86_64_tys,
                  decl: fn(fnty: TypeRef) -> ValueRef) -> ValueRef {
    let atys = vec::map(tys.arg_tys, |t| t.ty);
    let rty = tys.ret_ty.ty;
    let fnty = T_fn(atys, rty);
    let llfn = decl(fnty);

    for vec::eachi(tys.attrs) |i, a| {
        match a {
            option::Some(attr) => {
                let llarg = get_param(llfn, i);
                llvm::LLVMAddAttribute(llarg, attr as c_uint);
            }
            _ => ()
        }
    }
    return llfn;
}

fn link_name(ccx: @crate_ctxt, i: @ast::foreign_item) -> ~str {
    match attr::first_attr_value_str_by_name(i.attrs, ~"link_name") {
        None => ccx.sess.str_of(i.ident),
        option::Some(ln) => ln
    }
}

type c_stack_tys = {
    arg_tys: ~[TypeRef],
    ret_ty: TypeRef,
    ret_def: bool,
    bundle_ty: TypeRef,
    shim_fn_ty: TypeRef,
    x86_64_tys: Option<x86_64_tys>
};

fn c_arg_and_ret_lltys(ccx: @crate_ctxt,
                       id: ast::node_id) -> (~[TypeRef], TypeRef, ty::t) {
    match ty::get(ty::node_id_to_type(ccx.tcx, id)).sty {
        ty::ty_fn(ref fn_ty) => {
            let llargtys = type_of_explicit_args(ccx, fn_ty.sig.inputs);
            let llretty = type_of::type_of(ccx, fn_ty.sig.output);
            (llargtys, llretty, fn_ty.sig.output)
        }
        _ => ccx.sess.bug(~"c_arg_and_ret_lltys called on non-function type")
    }
}

fn c_stack_tys(ccx: @crate_ctxt,
               id: ast::node_id) -> @c_stack_tys {
    let (llargtys, llretty, ret_ty) = c_arg_and_ret_lltys(ccx, id);
    let bundle_ty = T_struct(vec::append_one(llargtys, T_ptr(llretty)));
    let ret_def = !ty::type_is_bot(ret_ty) && !ty::type_is_nil(ret_ty);
    let x86_64 = if ccx.sess.targ_cfg.arch == arch_x86_64 {
        option::Some(x86_64_tys(llargtys, llretty, ret_def))
    } else {
        option::None
    };
    return @{
        arg_tys: llargtys,
        ret_ty: llretty,
        ret_def: ret_def,
        bundle_ty: bundle_ty,
        shim_fn_ty: T_fn(~[T_ptr(bundle_ty)], T_void()),
        x86_64_tys: x86_64
    };
}

type shim_arg_builder = fn(bcx: block, tys: @c_stack_tys,
                           llargbundle: ValueRef) -> ~[ValueRef];

type shim_ret_builder = fn(bcx: block, tys: @c_stack_tys,
                           llargbundle: ValueRef, llretval: ValueRef);

fn build_shim_fn_(ccx: @crate_ctxt,
                  shim_name: ~str,
                  llbasefn: ValueRef,
                  tys: @c_stack_tys,
                  cc: lib::llvm::CallConv,
                  arg_builder: shim_arg_builder,
                  ret_builder: shim_ret_builder) -> ValueRef {

    let llshimfn = decl_internal_cdecl_fn(
        ccx.llmod, shim_name, tys.shim_fn_ty);

    // Declare the body of the shim function:
    let fcx = new_fn_ctxt(ccx, ~[], llshimfn, None);
    let bcx = top_scope_block(fcx, None);
    let lltop = bcx.llbb;
    let llargbundle = get_param(llshimfn, 0u);
    let llargvals = arg_builder(bcx, tys, llargbundle);

    // Create the call itself and store the return value:
    let llretval = CallWithConv(bcx, llbasefn,
                                llargvals, cc); // r

    ret_builder(bcx, tys, llargbundle, llretval);

    build_return(bcx);
    finish_fn(fcx, lltop);

    return llshimfn;
}

type wrap_arg_builder = fn(bcx: block, tys: @c_stack_tys,
                           llwrapfn: ValueRef,
                           llargbundle: ValueRef);

type wrap_ret_builder = fn(bcx: block, tys: @c_stack_tys,
                           llargbundle: ValueRef);

fn build_wrap_fn_(ccx: @crate_ctxt,
                  tys: @c_stack_tys,
                  llshimfn: ValueRef,
                  llwrapfn: ValueRef,
                  shim_upcall: ValueRef,
                  arg_builder: wrap_arg_builder,
                  ret_builder: wrap_ret_builder) {

    let _icx = ccx.insn_ctxt("foreign::build_wrap_fn_");
    let fcx = new_fn_ctxt(ccx, ~[], llwrapfn, None);
    let bcx = top_scope_block(fcx, None);
    let lltop = bcx.llbb;

    // Allocate the struct and write the arguments into it.
    let llargbundle = alloca(bcx, tys.bundle_ty);
    arg_builder(bcx, tys, llwrapfn, llargbundle);

    // Create call itself.
    let llshimfnptr = PointerCast(bcx, llshimfn, T_ptr(T_i8()));
    let llrawargbundle = PointerCast(bcx, llargbundle, T_ptr(T_i8()));
    Call(bcx, shim_upcall, ~[llrawargbundle, llshimfnptr]);
    ret_builder(bcx, tys, llargbundle);

    tie_up_header_blocks(fcx, lltop);

    // Make sure our standard return block (that we didn't use) is terminated
    let ret_cx = raw_block(fcx, false, fcx.llreturn);
    Unreachable(ret_cx);
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
// Note: on i386, the layout of the args struct is generally the same as the
// desired layout of the arguments on the C stack.  Therefore, we could use
// upcall_alloc_c_stack() to allocate the `args` structure and switch the
// stack pointer appropriately to avoid a round of copies.  (In fact, the shim
// function itself is unnecessary). We used to do this, in fact, and will
// perhaps do so in the future.
fn trans_foreign_mod(ccx: @crate_ctxt,
                    foreign_mod: ast::foreign_mod, abi: ast::foreign_abi) {

    let _icx = ccx.insn_ctxt("foreign::trans_foreign_mod");

    fn build_shim_fn(ccx: @crate_ctxt,
                     foreign_item: @ast::foreign_item,
                     tys: @c_stack_tys,
                     cc: lib::llvm::CallConv) -> ValueRef {

        let _icx = ccx.insn_ctxt("foreign::build_shim_fn");

        fn build_args(bcx: block, tys: @c_stack_tys,
                      llargbundle: ValueRef) -> ~[ValueRef] {
            let _icx = bcx.insn_ctxt("foreign::shim::build_args");
            let mut llargvals = ~[];
            let mut i = 0u;
            let n = vec::len(tys.arg_tys);

            match tys.x86_64_tys {
                Some(x86_64) => {
                    let mut atys = x86_64.arg_tys;
                    let mut attrs = x86_64.attrs;
                    if x86_64.sret {
                        let llretptr = GEPi(bcx, llargbundle, [0u, n]);
                        let llretloc = Load(bcx, llretptr);
                        llargvals = ~[llretloc];
                        atys = vec::tail(atys);
                        attrs = vec::tail(attrs);
                    }
                    while i < n {
                        let llargval = if atys[i].cast {
                            let arg_ptr = GEPi(bcx, llargbundle, [0u, i]);
                            let arg_ptr = BitCast(bcx, arg_ptr,
                                              T_ptr(atys[i].ty));
                            Load(bcx, arg_ptr)
                        } else if option::is_some(attrs[i]) {
                            GEPi(bcx, llargbundle, [0u, i])
                        } else {
                            load_inbounds(bcx, llargbundle, [0u, i])
                        };
                        vec::push(llargvals, llargval);
                        i += 1u;
                    }
                }
                _ => {
                    while i < n {
                        let llargval = load_inbounds(bcx, llargbundle,
                                                          [0u, i]);
                        vec::push(llargvals, llargval);
                        i += 1u;
                    }
                }
            }
            return llargvals;
        }

        fn build_ret(bcx: block, tys: @c_stack_tys,
                     llargbundle: ValueRef, llretval: ValueRef)  {
            let _icx = bcx.insn_ctxt("foreign::shim::build_ret");
            match tys.x86_64_tys {
                Some(x86_64) => {
                  for vec::eachi(x86_64.attrs) |i, a| {
                        match a {
                            Some(attr) => {
                                llvm::LLVMAddInstrAttribute(
                                    llretval, (i + 1u) as c_uint,
                                              attr as c_uint);
                            }
                            _ => ()
                        }
                    }
                    if x86_64.sret || !tys.ret_def {
                        return;
                    }
                    let n = vec::len(tys.arg_tys);
                    let llretptr = GEPi(bcx, llargbundle, [0u, n]);
                    let llretloc = Load(bcx, llretptr);
                    if x86_64.ret_ty.cast {
                        let tmp_ptr = BitCast(bcx, llretloc,
                                                   T_ptr(x86_64.ret_ty.ty));
                        Store(bcx, llretval, tmp_ptr);
                    } else {
                        Store(bcx, llretval, llretloc);
                    };
                }
                _ => {
                    if tys.ret_def {
                        let n = vec::len(tys.arg_tys);
                        // R** llretptr = &args->r;
                        let llretptr = GEPi(bcx, llargbundle, [0u, n]);
                        // R* llretloc = *llretptr; /* (args->r) */
                        let llretloc = Load(bcx, llretptr);
                        // *args->r = r;
                        Store(bcx, llretval, llretloc);
                    }
                }
            }
        }

        let lname = link_name(ccx, foreign_item);
        let llbasefn = base_fn(ccx, lname, tys, cc);
        // Name the shim function
        let shim_name = lname + ~"__c_stack_shim";
        return build_shim_fn_(ccx, shim_name, llbasefn, tys, cc,
                           build_args, build_ret);
    }

    fn base_fn(ccx: @crate_ctxt, lname: ~str, tys: @c_stack_tys,
               cc: lib::llvm::CallConv) -> ValueRef {
        // Declare the "prototype" for the base function F:
        match tys.x86_64_tys {
          Some(x86_64) => {
            do decl_x86_64_fn(x86_64) |fnty| {
                decl_fn(ccx.llmod, lname, cc, fnty)
            }
          }
          _ => {
            let llbasefnty = T_fn(tys.arg_tys, tys.ret_ty);
            decl_fn(ccx.llmod, lname, cc, llbasefnty)
          }
        }
    }

    // FIXME (#2535): this is very shaky and probably gets ABIs wrong all
    // over the place
    fn build_direct_fn(ccx: @crate_ctxt, decl: ValueRef,
                       item: @ast::foreign_item, tys: @c_stack_tys,
                       cc: lib::llvm::CallConv) {
        let fcx = new_fn_ctxt(ccx, ~[], decl, None);
        let bcx = top_scope_block(fcx, None), lltop = bcx.llbb;
        let llbasefn = base_fn(ccx, link_name(ccx, item), tys, cc);
        let ty = ty::lookup_item_type(ccx.tcx,
                                      ast_util::local_def(item.id)).ty;
        let args = vec::from_fn(ty::ty_fn_args(ty).len(), |i| {
            get_param(decl, i + first_real_arg)
        });
        let retval = Call(bcx, llbasefn, args);
        if !ty::type_is_nil(ty::ty_fn_ret(ty)) {
            Store(bcx, retval, fcx.llretptr);
        }
        build_return(bcx);
        finish_fn(fcx, lltop);
    }

    fn build_wrap_fn(ccx: @crate_ctxt,
                     tys: @c_stack_tys,
                     llshimfn: ValueRef,
                     llwrapfn: ValueRef) {

        let _icx = ccx.insn_ctxt("foreign::build_wrap_fn");

        fn build_args(bcx: block, tys: @c_stack_tys,
                      llwrapfn: ValueRef, llargbundle: ValueRef) {
            let _icx = bcx.insn_ctxt("foreign::wrap::build_args");
            let mut i = 0u;
            let n = vec::len(tys.arg_tys);
            let implicit_args = first_real_arg; // return + env
            while i < n {
                let llargval = get_param(llwrapfn, i + implicit_args);
                store_inbounds(bcx, llargval, llargbundle, ~[0u, i]);
                i += 1u;
            }
            let llretptr = get_param(llwrapfn, 0u);
            store_inbounds(bcx, llretptr, llargbundle, ~[0u, n]);
        }

        fn build_ret(bcx: block, _tys: @c_stack_tys,
                     _llargbundle: ValueRef) {
            let _icx = bcx.insn_ctxt("foreign::wrap::build_ret");
            RetVoid(bcx);
        }

        build_wrap_fn_(ccx, tys, llshimfn, llwrapfn,
                       ccx.upcalls.call_shim_on_c_stack,
                       build_args, build_ret);
    }

    let mut cc = match abi {
      ast::foreign_abi_rust_intrinsic |
      ast::foreign_abi_cdecl => lib::llvm::CCallConv,
      ast::foreign_abi_stdcall => lib::llvm::X86StdcallCallConv
    };

    for vec::each(foreign_mod.items) |foreign_item| {
      match foreign_item.node {
        ast::foreign_item_fn(*) => {
          let id = foreign_item.id;
          if abi != ast::foreign_abi_rust_intrinsic {
              let llwrapfn = get_item_val(ccx, id);
              let tys = c_stack_tys(ccx, id);
              if attr::attrs_contains_name(foreign_item.attrs,
                                           ~"rust_stack") {
                  build_direct_fn(ccx, llwrapfn, *foreign_item, tys, cc);
              } else {
                  let llshimfn = build_shim_fn(ccx, *foreign_item, tys, cc);
                  build_wrap_fn(ccx, tys, llshimfn, llwrapfn);
              }
          } else {
              // Intrinsics are emitted by monomorphic fn
          }
        }
        ast::foreign_item_const(*) => {
            let ident = ccx.sess.parse_sess.interner.get(foreign_item.ident);
            ccx.item_symbols.insert(foreign_item.id, copy *ident);
        }
      }
    }
}

fn trans_intrinsic(ccx: @crate_ctxt, decl: ValueRef, item: @ast::foreign_item,
                   path: ast_map::path, substs: param_substs,
                   ref_id: Option<ast::node_id>)
{
    debug!("trans_intrinsic(item.ident=%s)", ccx.sess.str_of(item.ident));

    let fcx = new_fn_ctxt_w_id(ccx, path, decl, item.id,
                               Some(substs), Some(item.span));
    let mut bcx = top_scope_block(fcx, None), lltop = bcx.llbb;
    match ccx.sess.str_of(item.ident) {
        ~"atomic_xchg" => {
            let old = AtomicRMW(bcx, Xchg,
                                get_param(decl, first_real_arg),
                                get_param(decl, first_real_arg + 1u),
                                SequentiallyConsistent);
            Store(bcx, old, fcx.llretptr);
        }
        ~"atomic_xchg_acq" => {
            let old = AtomicRMW(bcx, Xchg,
                                get_param(decl, first_real_arg),
                                get_param(decl, first_real_arg + 1u),
                                Acquire);
            Store(bcx, old, fcx.llretptr);
        }
        ~"atomic_xchg_rel" => {
            let old = AtomicRMW(bcx, Xchg,
                                get_param(decl, first_real_arg),
                                get_param(decl, first_real_arg + 1u),
                                Release);
            Store(bcx, old, fcx.llretptr);
        }
        ~"atomic_xadd" => {
            let old = AtomicRMW(bcx, lib::llvm::Add,
                                get_param(decl, first_real_arg),
                                get_param(decl, first_real_arg + 1u),
                                SequentiallyConsistent);
            Store(bcx, old, fcx.llretptr);
        }
        ~"atomic_xadd_acq" => {
            let old = AtomicRMW(bcx, lib::llvm::Add,
                                get_param(decl, first_real_arg),
                                get_param(decl, first_real_arg + 1u),
                                Acquire);
            Store(bcx, old, fcx.llretptr);
        }
        ~"atomic_xadd_rel" => {
            let old = AtomicRMW(bcx, lib::llvm::Add,
                                get_param(decl, first_real_arg),
                                get_param(decl, first_real_arg + 1u),
                                Release);
            Store(bcx, old, fcx.llretptr);
        }
        ~"atomic_xsub" => {
            let old = AtomicRMW(bcx, lib::llvm::Sub,
                                get_param(decl, first_real_arg),
                                get_param(decl, first_real_arg + 1u),
                                SequentiallyConsistent);
            Store(bcx, old, fcx.llretptr);
        }
        ~"atomic_xsub_acq" => {
            let old = AtomicRMW(bcx, lib::llvm::Sub,
                                get_param(decl, first_real_arg),
                                get_param(decl, first_real_arg + 1u),
                                Acquire);
            Store(bcx, old, fcx.llretptr);
        }
        ~"atomic_xsub_rel" => {
            let old = AtomicRMW(bcx, lib::llvm::Sub,
                                get_param(decl, first_real_arg),
                                get_param(decl, first_real_arg + 1u),
                                Release);
            Store(bcx, old, fcx.llretptr);
        }
        ~"size_of" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            Store(bcx, C_uint(ccx, shape::llsize_of_real(ccx, lltp_ty)),
                  fcx.llretptr);
        }
        ~"move_val" => {
            let tp_ty = substs.tys[0];
            let src = Datum {val: get_param(decl, first_real_arg + 1u),
                             ty: tp_ty, mode: ByRef, source: FromLvalue};
            bcx = src.move_to(bcx, DROP_EXISTING,
                              get_param(decl, first_real_arg));
        }
        ~"move_val_init" => {
            let tp_ty = substs.tys[0];
            let src = Datum {val: get_param(decl, first_real_arg + 1u),
                             ty: tp_ty, mode: ByRef, source: FromLvalue};
            bcx = src.move_to(bcx, INIT, get_param(decl, first_real_arg));
        }
        ~"min_align_of" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            Store(bcx, C_uint(ccx, shape::llalign_of_min(ccx, lltp_ty)),
                  fcx.llretptr);
        }
        ~"pref_align_of"=> {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            Store(bcx, C_uint(ccx, shape::llalign_of_pref(ccx, lltp_ty)),
                  fcx.llretptr);
        }
        ~"get_tydesc" => {
            let tp_ty = substs.tys[0];
            let static_ti = get_tydesc(ccx, tp_ty);
            glue::lazily_emit_all_tydesc_glue(ccx, static_ti);

            // FIXME (#2712): change this to T_ptr(ccx.tydesc_ty) when the
            // core::sys copy of the get_tydesc interface dies off.
            let td = PointerCast(bcx, static_ti.tydesc, T_ptr(T_nil()));
            Store(bcx, td, fcx.llretptr);
        }
        ~"init" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            if !ty::type_is_nil(tp_ty) {
                Store(bcx, C_null(lltp_ty), fcx.llretptr);
            }
        }
        ~"forget" => {}
        ~"reinterpret_cast" => {
            let tp_ty = substs.tys[0];
            let lltp_ty = type_of::type_of(ccx, tp_ty);
            let llout_ty = type_of::type_of(ccx, substs.tys[1]);
            let tp_sz = shape::llsize_of_real(ccx, lltp_ty),
            out_sz = shape::llsize_of_real(ccx, llout_ty);
          if tp_sz != out_sz {
              let sp = match ccx.tcx.items.get(option::get(ref_id)) {
                  ast_map::node_expr(e) => e.span,
                  _ => fail ~"reinterpret_cast or forget has non-expr arg"
              };
              ccx.sess.span_fatal(
                  sp, fmt!("reinterpret_cast called on types \
                            with different size: %s (%u) to %s (%u)",
                           ty_to_str(ccx.tcx, tp_ty), tp_sz,
                           ty_to_str(ccx.tcx, substs.tys[1]), out_sz));
          }
          if !ty::type_is_nil(substs.tys[1]) {
              // NB: Do not use a Load and Store here. This causes
              // massive code bloat when reinterpret_cast is used on
              // large structural types.
              let llretptr = PointerCast(bcx, fcx.llretptr, T_ptr(T_i8()));
              let llcast = get_param(decl, first_real_arg);
              let llcast = PointerCast(bcx, llcast, T_ptr(T_i8()));
              call_memmove(bcx, llretptr, llcast, llsize_of(ccx, lltp_ty));
          }
      }
        ~"addr_of" => {
            Store(bcx, get_param(decl, first_real_arg), fcx.llretptr);
        }
        ~"needs_drop" => {
            let tp_ty = substs.tys[0];
            Store(bcx, C_bool(ty::type_needs_drop(ccx.tcx, tp_ty)),
                  fcx.llretptr);
        }
        ~"visit_tydesc" => {
            let td = get_param(decl, first_real_arg);
            let visitor = get_param(decl, first_real_arg + 1u);
            let td = PointerCast(bcx, td, T_ptr(ccx.tydesc_type));
            glue::call_tydesc_glue_full(bcx, visitor, td,
                                        abi::tydesc_field_visit_glue, None);
        }
        ~"frame_address" => {
            let frameaddress = ccx.intrinsics.get(~"llvm.frameaddress");
            let frameaddress_val = Call(bcx, frameaddress, ~[C_i32(0i32)]);
            let star_u8 = ty::mk_imm_ptr(
                bcx.tcx(),
                ty::mk_mach_uint(bcx.tcx(), ast::ty_u8));
            let fty = ty::mk_fn(bcx.tcx(), FnTyBase {
                meta: FnMeta {purity: ast::impure_fn,
                              proto:
                                  ty::proto_vstore(ty::vstore_slice(
                                      ty::re_bound(ty::br_anon(0)))),
                              bounds: @~[],
                              ret_style: ast::return_val},
                sig: FnSig {inputs: ~[{mode: ast::expl(ast::by_val),
                                       ty: star_u8}],
                            output: ty::mk_nil(bcx.tcx())}
            });
            let datum = Datum {val: get_param(decl, first_real_arg),
                               mode: ByRef, ty: fty, source: FromLvalue};
            bcx = trans_call_inner(
                bcx, None, fty, ty::mk_nil(bcx.tcx()),
                |bcx| Callee {bcx: bcx, data: Closure(datum)},
                ArgVals(~[frameaddress_val]), Ignore, DontAutorefArg);
        }
        ~"morestack_addr" => {
            // XXX This is a hack to grab the address of this particular
            // native function. There should be a general in-language
            // way to do this
            let llfty = type_of_fn(bcx.ccx(), ~[], ty::mk_nil(bcx.tcx()));
            let morestack_addr = decl_cdecl_fn(
                bcx.ccx().llmod, ~"__morestack", llfty);
            let morestack_addr = PointerCast(bcx, morestack_addr,
                                             T_ptr(T_nil()));
            Store(bcx, morestack_addr, fcx.llretptr);
        }
        _ => {
            // Could we make this an enum rather than a string? does it get
            // checked earlier?
            ccx.sess.span_bug(item.span, ~"unknown intrinsic");
        }
    }
    build_return(bcx);
    finish_fn(fcx, lltop);
}

fn trans_foreign_fn(ccx: @crate_ctxt, path: ast_map::path, decl: ast::fn_decl,
                  body: ast::blk, llwrapfn: ValueRef, id: ast::node_id) {

    let _icx = ccx.insn_ctxt("foreign::build_foreign_fn");

    fn build_rust_fn(ccx: @crate_ctxt, path: ast_map::path,
                     decl: ast::fn_decl, body: ast::blk,
                     id: ast::node_id) -> ValueRef {
        let _icx = ccx.insn_ctxt("foreign::foreign::build_rust_fn");
        let t = ty::node_id_to_type(ccx.tcx, id);
        let ps = link::mangle_internal_name_by_path(
            ccx, vec::append_one(path, ast_map::path_name(
                syntax::parse::token::special_idents::clownshoe_abi
            )));
        let llty = type_of_fn_from_ty(ccx, t);
        let llfndecl = decl_internal_cdecl_fn(ccx.llmod, ps, llty);
        trans_fn(ccx, path, decl, body, llfndecl, no_self, None, id);
        return llfndecl;
    }

    fn build_shim_fn(ccx: @crate_ctxt, path: ast_map::path,
                     llrustfn: ValueRef, tys: @c_stack_tys) -> ValueRef {

        let _icx = ccx.insn_ctxt("foreign::foreign::build_shim_fn");

        fn build_args(bcx: block, tys: @c_stack_tys,
                      llargbundle: ValueRef) -> ~[ValueRef] {
            let _icx = bcx.insn_ctxt("foreign::extern::shim::build_args");
            let mut llargvals = ~[];
            let mut i = 0u;
            let n = vec::len(tys.arg_tys);
            let llretptr = load_inbounds(bcx, llargbundle, ~[0u, n]);
            vec::push(llargvals, llretptr);
            let llenvptr = C_null(T_opaque_box_ptr(bcx.ccx()));
            vec::push(llargvals, llenvptr);
            while i < n {
                let llargval = load_inbounds(bcx, llargbundle, ~[0u, i]);
                vec::push(llargvals, llargval);
                i += 1u;
            }
            return llargvals;
        }

        fn build_ret(_bcx: block, _tys: @c_stack_tys,
                     _llargbundle: ValueRef, _llretval: ValueRef)  {
            // Nop. The return pointer in the Rust ABI function
            // is wired directly into the return slot in the shim struct
        }

        let shim_name = link::mangle_internal_name_by_path(
            ccx, vec::append_one(path, ast_map::path_name(
                syntax::parse::token::special_idents::clownshoe_stack_shim
            )));
        return build_shim_fn_(ccx, shim_name, llrustfn, tys,
                           lib::llvm::CCallConv,
                           build_args, build_ret);
    }

    fn build_wrap_fn(ccx: @crate_ctxt, llshimfn: ValueRef,
                     llwrapfn: ValueRef, tys: @c_stack_tys) {

        let _icx = ccx.insn_ctxt("foreign::foreign::build_wrap_fn");

        fn build_args(bcx: block, tys: @c_stack_tys,
                      llwrapfn: ValueRef, llargbundle: ValueRef) {
            let _icx = bcx.insn_ctxt("foreign::foreign::wrap::build_args");
            match tys.x86_64_tys {
                option::Some(x86_64) => {
                    let mut atys = x86_64.arg_tys;
                    let mut attrs = x86_64.attrs;
                    let mut j = 0u;
                    let llretptr = if x86_64.sret {
                        atys = vec::tail(atys);
                        attrs = vec::tail(attrs);
                        j = 1u;
                        get_param(llwrapfn, 0u)
                    } else if x86_64.ret_ty.cast {
                        let retptr = alloca(bcx, x86_64.ret_ty.ty);
                        BitCast(bcx, retptr, T_ptr(tys.ret_ty))
                    } else {
                        alloca(bcx, tys.ret_ty)
                    };

                    let mut i = 0u;
                    let n = vec::len(atys);
                    while i < n {
                        let mut argval = get_param(llwrapfn, i + j);
                        if option::is_some(attrs[i]) {
                            argval = Load(bcx, argval);
                            store_inbounds(bcx, argval, llargbundle,
                                           [0u, i]);
                        } else if atys[i].cast {
                            let argptr = GEPi(bcx, llargbundle, [0u, i]);
                            let argptr = BitCast(bcx, argptr,
                                                 T_ptr(atys[i].ty));
                            Store(bcx, argval, argptr);
                        } else {
                            store_inbounds(bcx, argval, llargbundle,
                                           [0u, i]);
                        }
                        i += 1u;
                    }
                    store_inbounds(bcx, llretptr, llargbundle, [0u, n]);
                }
                _ => {
                    let llretptr = alloca(bcx, tys.ret_ty);
                    let n = vec::len(tys.arg_tys);
                  for uint::range(0u, n) |i| {
                        let llargval = get_param(llwrapfn, i);
                        store_inbounds(bcx, llargval, llargbundle,
                                                      [0u, i]);
                    };
                    store_inbounds(bcx, llretptr, llargbundle, [0u, n]);
                }
            }
        }

        fn build_ret(bcx: block, tys: @c_stack_tys,
                     llargbundle: ValueRef) {
            let _icx = bcx.insn_ctxt("foreign::foreign::wrap::build_ret");
            match tys.x86_64_tys {
                option::Some(x86_64) => {
                    if x86_64.sret || !tys.ret_def {
                        RetVoid(bcx);
                        return;
                    }
                    let n = vec::len(tys.arg_tys);
                    let llretval = load_inbounds(bcx, llargbundle, ~[0u, n]);
                    let llretval = if x86_64.ret_ty.cast {
                        let retptr = BitCast(bcx, llretval,
                                                  T_ptr(x86_64.ret_ty.ty));
                        Load(bcx, retptr)
                    } else {
                        Load(bcx, llretval)
                    };
                    Ret(bcx, llretval);
                }
                _ => {
                    let n = vec::len(tys.arg_tys);
                    let llretval = load_inbounds(bcx, llargbundle, ~[0u, n]);
                    let llretval = Load(bcx, llretval);
                    Ret(bcx, llretval);
                }
            }
        }

        build_wrap_fn_(ccx, tys, llshimfn, llwrapfn,
                       ccx.upcalls.call_shim_on_rust_stack,
                       build_args, build_ret);
    }

    let tys = c_stack_tys(ccx, id);
    // The internal Rust ABI function - runs on the Rust stack
    let llrustfn = build_rust_fn(ccx, path, decl, body, id);
    // The internal shim function - runs on the Rust stack
    let llshimfn = build_shim_fn(ccx, path, llrustfn, tys);
    // The foreign C function - runs on the C stack
    build_wrap_fn(ccx, llshimfn, llwrapfn, tys)
}

fn register_foreign_fn(ccx: @crate_ctxt, sp: span,
                     path: ast_map::path, node_id: ast::node_id)
    -> ValueRef {
    let _icx = ccx.insn_ctxt("foreign::register_foreign_fn");
    let t = ty::node_id_to_type(ccx.tcx, node_id);
    let (llargtys, llretty, ret_ty) = c_arg_and_ret_lltys(ccx, node_id);
    return if ccx.sess.targ_cfg.arch == arch_x86_64 {
        let ret_def = !ty::type_is_bot(ret_ty) && !ty::type_is_nil(ret_ty);
        let x86_64 = x86_64_tys(llargtys, llretty, ret_def);
        do decl_x86_64_fn(x86_64) |fnty| {
            register_fn_fuller(ccx, sp, path, node_id,
                               t, lib::llvm::CCallConv, fnty)
        }
    } else {
        let llfty = T_fn(llargtys, llretty);
        register_fn_fuller(ccx, sp, path, node_id,
                           t, lib::llvm::CCallConv, llfty)
    }
}

fn abi_of_foreign_fn(ccx: @crate_ctxt, i: @ast::foreign_item)
    -> ast::foreign_abi {
    match attr::first_attr_value_str_by_name(i.attrs, ~"abi") {
      None => match ccx.tcx.items.get(i.id) {
        ast_map::node_foreign_item(_, abi, _) => abi,
        // ??
        _ => fail ~"abi_of_foreign_fn: not foreign"
      },
      Some(_) => match attr::foreign_abi(i.attrs) {
        either::Right(abi) => abi,
        either::Left(msg) => ccx.sess.span_fatal(i.span, msg)
      }
    }
}
