// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Translate the completed AST to the LLVM IR.
//!
//! Some functions here, such as trans_block and trans_expr, return a value --
//! the result of the translation to LLVM -- while others, such as trans_fn
//! and trans_item, are called only for the side effect of adding a
//! particular definition to the LLVM IR output we're producing.
//!
//! Hopefully useful general knowledge about trans:
//!
//!   * There's no way to find out the Ty type of a ValueRef.  Doing so
//!     would be "trying to get the eggs out of an omelette" (credit:
//!     pcwalton).  You can, instead, find out its TypeRef by calling val_ty,
//!     but one TypeRef corresponds to many `Ty`s; for instance, tup(int, int,
//!     int) and rec(x=int, y=int, z=int) will have the same TypeRef.

use super::ModuleLlvm;
use super::ModuleSource;
use super::ModuleTranslation;
use super::ModuleKind;

use abi;
use assert_module_sources;
use back::link;
use back::symbol_export;
use back::write::{self, OngoingCrateTranslation, create_target_machine};
use llvm::{ContextRef, ModuleRef, ValueRef, Vector, get_param};
use llvm;
use metadata;
use rustc::hir::def_id::{CrateNum, DefId, LOCAL_CRATE};
use rustc::middle::lang_items::StartFnLangItem;
use rustc::mir::mono::{Linkage, Visibility, Stats};
use rustc::middle::cstore::{EncodedMetadata};
use rustc::ty::{self, Ty, TyCtxt};
use rustc::ty::layout::{self, Align, TyLayout, LayoutOf};
use rustc::ty::maps::Providers;
use rustc::dep_graph::{DepNode, DepConstructor};
use rustc::ty::subst::Kind;
use rustc::middle::cstore::{self, LinkMeta, LinkagePreference};
use rustc::util::common::{time, print_time_passes_entry};
use rustc::session::config::{self, NoDebugInfo};
use rustc::session::Session;
use rustc_incremental;
use allocator;
use mir::place::PlaceRef;
use attributes;
use builder::Builder;
use callee;
use common::{C_bool, C_bytes_in_context, C_i32, C_usize};
use rustc_mir::monomorphize::collector::{self, MonoItemCollectionMode};
use common::{self, C_struct_in_context, C_array, CrateContext, val_ty};
use consts;
use context::{self, LocalCrateContext, SharedCrateContext};
use debuginfo;
use declare;
use meth;
use mir;
use monomorphize::Instance;
use monomorphize::partitioning::{self, PartitioningStrategy, CodegenUnit, CodegenUnitExt};
use symbol_names_test;
use time_graph;
use trans_item::{MonoItem, BaseMonoItemExt, MonoItemExt, DefPathBasedNames};
use type_::Type;
use type_of::LayoutLlvmExt;
use rustc::util::nodemap::{NodeSet, FxHashMap, FxHashSet, DefIdSet};
use CrateInfo;

use std::any::Any;
use std::ffi::CString;
use std::str;
use std::sync::Arc;
use std::time::{Instant, Duration};
use std::i32;
use std::iter;
use std::sync::mpsc;
use syntax_pos::Span;
use syntax_pos::symbol::InternedString;
use syntax::attr;
use rustc::hir;
use syntax::ast;

use mir::operand::OperandValue;

pub use rustc_trans_utils::{find_exported_symbols, check_for_rustc_errors_attr};
pub use rustc_mir::monomorphize::item::linkage_by_name;

pub struct StatRecorder<'a, 'tcx: 'a> {
    ccx: &'a CrateContext<'a, 'tcx>,
    name: Option<String>,
    istart: usize,
}

impl<'a, 'tcx> StatRecorder<'a, 'tcx> {
    pub fn new(ccx: &'a CrateContext<'a, 'tcx>, name: String) -> StatRecorder<'a, 'tcx> {
        let istart = ccx.stats().borrow().n_llvm_insns;
        StatRecorder {
            ccx,
            name: Some(name),
            istart,
        }
    }
}

impl<'a, 'tcx> Drop for StatRecorder<'a, 'tcx> {
    fn drop(&mut self) {
        if self.ccx.sess().trans_stats() {
            let mut stats = self.ccx.stats().borrow_mut();
            let iend = stats.n_llvm_insns;
            stats.fn_stats.push((self.name.take().unwrap(), iend - self.istart));
            stats.n_fns += 1;
            // Reset LLVM insn count to avoid compound costs.
            stats.n_llvm_insns = self.istart;
        }
    }
}

pub fn bin_op_to_icmp_predicate(op: hir::BinOp_,
                                signed: bool)
                                -> llvm::IntPredicate {
    match op {
        hir::BiEq => llvm::IntEQ,
        hir::BiNe => llvm::IntNE,
        hir::BiLt => if signed { llvm::IntSLT } else { llvm::IntULT },
        hir::BiLe => if signed { llvm::IntSLE } else { llvm::IntULE },
        hir::BiGt => if signed { llvm::IntSGT } else { llvm::IntUGT },
        hir::BiGe => if signed { llvm::IntSGE } else { llvm::IntUGE },
        op => {
            bug!("comparison_op_to_icmp_predicate: expected comparison operator, \
                  found {:?}",
                 op)
        }
    }
}

pub fn bin_op_to_fcmp_predicate(op: hir::BinOp_) -> llvm::RealPredicate {
    match op {
        hir::BiEq => llvm::RealOEQ,
        hir::BiNe => llvm::RealUNE,
        hir::BiLt => llvm::RealOLT,
        hir::BiLe => llvm::RealOLE,
        hir::BiGt => llvm::RealOGT,
        hir::BiGe => llvm::RealOGE,
        op => {
            bug!("comparison_op_to_fcmp_predicate: expected comparison operator, \
                  found {:?}",
                 op);
        }
    }
}

pub fn compare_simd_types<'a, 'tcx>(
    bcx: &Builder<'a, 'tcx>,
    lhs: ValueRef,
    rhs: ValueRef,
    t: Ty<'tcx>,
    ret_ty: Type,
    op: hir::BinOp_
) -> ValueRef {
    let signed = match t.sty {
        ty::TyFloat(_) => {
            let cmp = bin_op_to_fcmp_predicate(op);
            return bcx.sext(bcx.fcmp(cmp, lhs, rhs), ret_ty);
        },
        ty::TyUint(_) => false,
        ty::TyInt(_) => true,
        _ => bug!("compare_simd_types: invalid SIMD type"),
    };

    let cmp = bin_op_to_icmp_predicate(op, signed);
    // LLVM outputs an `< size x i1 >`, so we need to perform a sign extension
    // to get the correctly sized type. This will compile to a single instruction
    // once the IR is converted to assembly if the SIMD instruction is supported
    // by the target architecture.
    bcx.sext(bcx.icmp(cmp, lhs, rhs), ret_ty)
}

/// Retrieve the information we are losing (making dynamic) in an unsizing
/// adjustment.
///
/// The `old_info` argument is a bit funny. It is intended for use
/// in an upcast, where the new vtable for an object will be derived
/// from the old one.
pub fn unsized_info<'ccx, 'tcx>(ccx: &CrateContext<'ccx, 'tcx>,
                                source: Ty<'tcx>,
                                target: Ty<'tcx>,
                                old_info: Option<ValueRef>)
                                -> ValueRef {
    let (source, target) = ccx.tcx().struct_lockstep_tails(source, target);
    match (&source.sty, &target.sty) {
        (&ty::TyArray(_, len), &ty::TySlice(_)) => {
            C_usize(ccx, len.val.to_const_int().unwrap().to_u64().unwrap())
        }
        (&ty::TyDynamic(..), &ty::TyDynamic(..)) => {
            // For now, upcasts are limited to changes in marker
            // traits, and hence never actually require an actual
            // change to the vtable.
            old_info.expect("unsized_info: missing old info for trait upcast")
        }
        (_, &ty::TyDynamic(ref data, ..)) => {
            let vtable_ptr = ccx.layout_of(ccx.tcx().mk_mut_ptr(target))
                .field(ccx, abi::FAT_PTR_EXTRA);
            consts::ptrcast(meth::get_vtable(ccx, source, data.principal()),
                            vtable_ptr.llvm_type(ccx))
        }
        _ => bug!("unsized_info: invalid unsizing {:?} -> {:?}",
                                     source,
                                     target),
    }
}

/// Coerce `src` to `dst_ty`. `src_ty` must be a thin pointer.
pub fn unsize_thin_ptr<'a, 'tcx>(
    bcx: &Builder<'a, 'tcx>,
    src: ValueRef,
    src_ty: Ty<'tcx>,
    dst_ty: Ty<'tcx>
) -> (ValueRef, ValueRef) {
    debug!("unsize_thin_ptr: {:?} => {:?}", src_ty, dst_ty);
    match (&src_ty.sty, &dst_ty.sty) {
        (&ty::TyRef(_, ty::TypeAndMut { ty: a, .. }),
         &ty::TyRef(_, ty::TypeAndMut { ty: b, .. })) |
        (&ty::TyRef(_, ty::TypeAndMut { ty: a, .. }),
         &ty::TyRawPtr(ty::TypeAndMut { ty: b, .. })) |
        (&ty::TyRawPtr(ty::TypeAndMut { ty: a, .. }),
         &ty::TyRawPtr(ty::TypeAndMut { ty: b, .. })) => {
            assert!(bcx.ccx.shared().type_is_sized(a));
            let ptr_ty = bcx.ccx.layout_of(b).llvm_type(bcx.ccx).ptr_to();
            (bcx.pointercast(src, ptr_ty), unsized_info(bcx.ccx, a, b, None))
        }
        (&ty::TyAdt(def_a, _), &ty::TyAdt(def_b, _)) if def_a.is_box() && def_b.is_box() => {
            let (a, b) = (src_ty.boxed_ty(), dst_ty.boxed_ty());
            assert!(bcx.ccx.shared().type_is_sized(a));
            let ptr_ty = bcx.ccx.layout_of(b).llvm_type(bcx.ccx).ptr_to();
            (bcx.pointercast(src, ptr_ty), unsized_info(bcx.ccx, a, b, None))
        }
        (&ty::TyAdt(def_a, _), &ty::TyAdt(def_b, _)) => {
            assert_eq!(def_a, def_b);

            let src_layout = bcx.ccx.layout_of(src_ty);
            let dst_layout = bcx.ccx.layout_of(dst_ty);
            let mut result = None;
            for i in 0..src_layout.fields.count() {
                let src_f = src_layout.field(bcx.ccx, i);
                assert_eq!(src_layout.fields.offset(i).bytes(), 0);
                assert_eq!(dst_layout.fields.offset(i).bytes(), 0);
                if src_f.is_zst() {
                    continue;
                }
                assert_eq!(src_layout.size, src_f.size);

                let dst_f = dst_layout.field(bcx.ccx, i);
                assert_ne!(src_f.ty, dst_f.ty);
                assert_eq!(result, None);
                result = Some(unsize_thin_ptr(bcx, src, src_f.ty, dst_f.ty));
            }
            let (lldata, llextra) = result.unwrap();
            // HACK(eddyb) have to bitcast pointers until LLVM removes pointee types.
            (bcx.bitcast(lldata, dst_layout.scalar_pair_element_llvm_type(bcx.ccx, 0)),
             bcx.bitcast(llextra, dst_layout.scalar_pair_element_llvm_type(bcx.ccx, 1)))
        }
        _ => bug!("unsize_thin_ptr: called on bad types"),
    }
}

/// Coerce `src`, which is a reference to a value of type `src_ty`,
/// to a value of type `dst_ty` and store the result in `dst`
pub fn coerce_unsized_into<'a, 'tcx>(bcx: &Builder<'a, 'tcx>,
                                     src: PlaceRef<'tcx>,
                                     dst: PlaceRef<'tcx>) {
    let src_ty = src.layout.ty;
    let dst_ty = dst.layout.ty;
    let coerce_ptr = || {
        let (base, info) = match src.load(bcx).val {
            OperandValue::Pair(base, info) => {
                // fat-ptr to fat-ptr unsize preserves the vtable
                // i.e. &'a fmt::Debug+Send => &'a fmt::Debug
                // So we need to pointercast the base to ensure
                // the types match up.
                let thin_ptr = dst.layout.field(bcx.ccx, abi::FAT_PTR_ADDR);
                (bcx.pointercast(base, thin_ptr.llvm_type(bcx.ccx)), info)
            }
            OperandValue::Immediate(base) => {
                unsize_thin_ptr(bcx, base, src_ty, dst_ty)
            }
            OperandValue::Ref(..) => bug!()
        };
        OperandValue::Pair(base, info).store(bcx, dst);
    };
    match (&src_ty.sty, &dst_ty.sty) {
        (&ty::TyRef(..), &ty::TyRef(..)) |
        (&ty::TyRef(..), &ty::TyRawPtr(..)) |
        (&ty::TyRawPtr(..), &ty::TyRawPtr(..)) => {
            coerce_ptr()
        }
        (&ty::TyAdt(def_a, _), &ty::TyAdt(def_b, _)) if def_a.is_box() && def_b.is_box() => {
            coerce_ptr()
        }

        (&ty::TyAdt(def_a, _), &ty::TyAdt(def_b, _)) => {
            assert_eq!(def_a, def_b);

            for i in 0..def_a.variants[0].fields.len() {
                let src_f = src.project_field(bcx, i);
                let dst_f = dst.project_field(bcx, i);

                if dst_f.layout.is_zst() {
                    continue;
                }

                if src_f.layout.ty == dst_f.layout.ty {
                    memcpy_ty(bcx, dst_f.llval, src_f.llval, src_f.layout,
                        src_f.align.min(dst_f.align));
                } else {
                    coerce_unsized_into(bcx, src_f, dst_f);
                }
            }
        }
        _ => bug!("coerce_unsized_into: invalid coercion {:?} -> {:?}",
                  src_ty,
                  dst_ty),
    }
}

pub fn cast_shift_expr_rhs(
    cx: &Builder, op: hir::BinOp_, lhs: ValueRef, rhs: ValueRef
) -> ValueRef {
    cast_shift_rhs(op, lhs, rhs, |a, b| cx.trunc(a, b), |a, b| cx.zext(a, b))
}

pub fn cast_shift_const_rhs(op: hir::BinOp_, lhs: ValueRef, rhs: ValueRef) -> ValueRef {
    cast_shift_rhs(op,
                   lhs,
                   rhs,
                   |a, b| unsafe { llvm::LLVMConstTrunc(a, b.to_ref()) },
                   |a, b| unsafe { llvm::LLVMConstZExt(a, b.to_ref()) })
}

fn cast_shift_rhs<F, G>(op: hir::BinOp_,
                        lhs: ValueRef,
                        rhs: ValueRef,
                        trunc: F,
                        zext: G)
                        -> ValueRef
    where F: FnOnce(ValueRef, Type) -> ValueRef,
          G: FnOnce(ValueRef, Type) -> ValueRef
{
    // Shifts may have any size int on the rhs
    if op.is_shift() {
        let mut rhs_llty = val_ty(rhs);
        let mut lhs_llty = val_ty(lhs);
        if rhs_llty.kind() == Vector {
            rhs_llty = rhs_llty.element_type()
        }
        if lhs_llty.kind() == Vector {
            lhs_llty = lhs_llty.element_type()
        }
        let rhs_sz = rhs_llty.int_width();
        let lhs_sz = lhs_llty.int_width();
        if lhs_sz < rhs_sz {
            trunc(rhs, lhs_llty)
        } else if lhs_sz > rhs_sz {
            // FIXME (#1877: If shifting by negative
            // values becomes not undefined then this is wrong.
            zext(rhs, lhs_llty)
        } else {
            rhs
        }
    } else {
        rhs
    }
}

/// Returns whether this session's target will use SEH-based unwinding.
///
/// This is only true for MSVC targets, and even then the 64-bit MSVC target
/// currently uses SEH-ish unwinding with DWARF info tables to the side (same as
/// 64-bit MinGW) instead of "full SEH".
pub fn wants_msvc_seh(sess: &Session) -> bool {
    sess.target.target.options.is_like_msvc
}

pub fn call_assume<'a, 'tcx>(b: &Builder<'a, 'tcx>, val: ValueRef) {
    let assume_intrinsic = b.ccx.get_intrinsic("llvm.assume");
    b.call(assume_intrinsic, &[val], None);
}

pub fn from_immediate(bcx: &Builder, val: ValueRef) -> ValueRef {
    if val_ty(val) == Type::i1(bcx.ccx) {
        bcx.zext(val, Type::i8(bcx.ccx))
    } else {
        val
    }
}

pub fn to_immediate(bcx: &Builder, val: ValueRef, layout: layout::TyLayout) -> ValueRef {
    if let layout::Abi::Scalar(ref scalar) = layout.abi {
        if scalar.is_bool() {
            return bcx.trunc(val, Type::i1(bcx.ccx));
        }
    }
    val
}

pub fn call_memcpy(b: &Builder,
                   dst: ValueRef,
                   src: ValueRef,
                   n_bytes: ValueRef,
                   align: Align) {
    let ccx = b.ccx;
    let ptr_width = &ccx.sess().target.target.target_pointer_width;
    let key = format!("llvm.memcpy.p0i8.p0i8.i{}", ptr_width);
    let memcpy = ccx.get_intrinsic(&key);
    let src_ptr = b.pointercast(src, Type::i8p(ccx));
    let dst_ptr = b.pointercast(dst, Type::i8p(ccx));
    let size = b.intcast(n_bytes, ccx.isize_ty(), false);
    let align = C_i32(ccx, align.abi() as i32);
    let volatile = C_bool(ccx, false);
    b.call(memcpy, &[dst_ptr, src_ptr, size, align, volatile], None);
}

pub fn memcpy_ty<'a, 'tcx>(
    bcx: &Builder<'a, 'tcx>,
    dst: ValueRef,
    src: ValueRef,
    layout: TyLayout<'tcx>,
    align: Align,
) {
    let size = layout.size.bytes();
    if size == 0 {
        return;
    }

    call_memcpy(bcx, dst, src, C_usize(bcx.ccx, size), align);
}

pub fn call_memset<'a, 'tcx>(b: &Builder<'a, 'tcx>,
                             ptr: ValueRef,
                             fill_byte: ValueRef,
                             size: ValueRef,
                             align: ValueRef,
                             volatile: bool) -> ValueRef {
    let ptr_width = &b.ccx.sess().target.target.target_pointer_width;
    let intrinsic_key = format!("llvm.memset.p0i8.i{}", ptr_width);
    let llintrinsicfn = b.ccx.get_intrinsic(&intrinsic_key);
    let volatile = C_bool(b.ccx, volatile);
    b.call(llintrinsicfn, &[ptr, fill_byte, size, align, volatile], None)
}

pub fn trans_instance<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>, instance: Instance<'tcx>) {
    let _s = if ccx.sess().trans_stats() {
        let mut instance_name = String::new();
        DefPathBasedNames::new(ccx.tcx(), true, true)
            .push_def_path(instance.def_id(), &mut instance_name);
        Some(StatRecorder::new(ccx, instance_name))
    } else {
        None
    };

    // this is an info! to allow collecting monomorphization statistics
    // and to allow finding the last function before LLVM aborts from
    // release builds.
    info!("trans_instance({})", instance);

    let fn_ty = instance.ty(ccx.tcx());
    let sig = common::ty_fn_sig(ccx, fn_ty);
    let sig = ccx.tcx().erase_late_bound_regions_and_normalize(&sig);

    let lldecl = match ccx.instances().borrow().get(&instance) {
        Some(&val) => val,
        None => bug!("Instance `{:?}` not already declared", instance)
    };

    ccx.stats().borrow_mut().n_closures += 1;

    // The `uwtable` attribute according to LLVM is:
    //
    //     This attribute indicates that the ABI being targeted requires that an
    //     unwind table entry be produced for this function even if we can show
    //     that no exceptions passes by it. This is normally the case for the
    //     ELF x86-64 abi, but it can be disabled for some compilation units.
    //
    // Typically when we're compiling with `-C panic=abort` (which implies this
    // `no_landing_pads` check) we don't need `uwtable` because we can't
    // generate any exceptions! On Windows, however, exceptions include other
    // events such as illegal instructions, segfaults, etc. This means that on
    // Windows we end up still needing the `uwtable` attribute even if the `-C
    // panic=abort` flag is passed.
    //
    // You can also find more info on why Windows is whitelisted here in:
    //      https://bugzilla.mozilla.org/show_bug.cgi?id=1302078
    if !ccx.sess().no_landing_pads() ||
       ccx.sess().target.target.options.is_like_windows {
        attributes::emit_uwtable(lldecl, true);
    }

    let mir = ccx.tcx().instance_mir(instance.def);
    mir::trans_mir(ccx, lldecl, &mir, instance, sig);
}

pub fn set_link_section(ccx: &CrateContext,
                        llval: ValueRef,
                        attrs: &[ast::Attribute]) {
    if let Some(sect) = attr::first_attr_value_str_by_name(attrs, "link_section") {
        if contains_null(&sect.as_str()) {
            ccx.sess().fatal(&format!("Illegal null byte in link_section value: `{}`", &sect));
        }
        unsafe {
            let buf = CString::new(sect.as_str().as_bytes()).unwrap();
            llvm::LLVMSetSection(llval, buf.as_ptr());
        }
    }
}

/// Create the `main` function which will initialize the rust runtime and call
/// users main function.
fn maybe_create_entry_wrapper(ccx: &CrateContext) {
    let (main_def_id, span) = match *ccx.sess().entry_fn.borrow() {
        Some((id, span)) => {
            (ccx.tcx().hir.local_def_id(id), span)
        }
        None => return,
    };

    let instance = Instance::mono(ccx.tcx(), main_def_id);

    if !ccx.codegen_unit().contains_item(&MonoItem::Fn(instance)) {
        // We want to create the wrapper in the same codegen unit as Rust's main
        // function.
        return;
    }

    let main_llfn = callee::get_fn(ccx, instance);

    let et = ccx.sess().entry_type.get().unwrap();
    match et {
        config::EntryMain => create_entry_fn(ccx, span, main_llfn, main_def_id, true),
        config::EntryStart => create_entry_fn(ccx, span, main_llfn, main_def_id, false),
        config::EntryNone => {}    // Do nothing.
    }

    fn create_entry_fn<'ccx>(ccx: &'ccx CrateContext,
                       sp: Span,
                       rust_main: ValueRef,
                       rust_main_def_id: DefId,
                       use_start_lang_item: bool) {
        let llfty = Type::func(&[Type::c_int(ccx), Type::i8p(ccx).ptr_to()], &Type::c_int(ccx));

        let main_ret_ty = ccx.tcx().fn_sig(rust_main_def_id).output();
        // Given that `main()` has no arguments,
        // then its return type cannot have
        // late-bound regions, since late-bound
        // regions must appear in the argument
        // listing.
        let main_ret_ty = main_ret_ty.no_late_bound_regions().unwrap();

        if declare::get_defined_value(ccx, "main").is_some() {
            // FIXME: We should be smart and show a better diagnostic here.
            ccx.sess().struct_span_err(sp, "entry symbol `main` defined multiple times")
                      .help("did you use #[no_mangle] on `fn main`? Use #[start] instead")
                      .emit();
            ccx.sess().abort_if_errors();
            bug!();
        }
        let llfn = declare::declare_cfn(ccx, "main", llfty);

        // `main` should respect same config for frame pointer elimination as rest of code
        attributes::set_frame_pointer_elimination(ccx, llfn);

        let bld = Builder::new_block(ccx, llfn, "top");

        debuginfo::gdb::insert_reference_to_gdb_debug_scripts_section_global(ccx, &bld);

        // Params from native main() used as args for rust start function
        let param_argc = get_param(llfn, 0);
        let param_argv = get_param(llfn, 1);
        let arg_argc = bld.intcast(param_argc, ccx.isize_ty(), true);
        let arg_argv = param_argv;

        let (start_fn, args) = if use_start_lang_item {
            let start_def_id = ccx.tcx().require_lang_item(StartFnLangItem);
            let start_fn = callee::resolve_and_get_fn(ccx, start_def_id, ccx.tcx().mk_substs(
                iter::once(Kind::from(main_ret_ty))));
            (start_fn, vec![bld.pointercast(rust_main, Type::i8p(ccx).ptr_to()),
                            arg_argc, arg_argv])
        } else {
            debug!("using user-defined start fn");
            (rust_main, vec![arg_argc, arg_argv])
        };

        let result = bld.call(start_fn, &args, None);
        bld.ret(bld.intcast(result, Type::c_int(ccx), true));
    }
}

fn contains_null(s: &str) -> bool {
    s.bytes().any(|b| b == 0)
}

fn write_metadata<'a, 'gcx>(tcx: TyCtxt<'a, 'gcx, 'gcx>,
                            llmod_id: &str,
                            link_meta: &LinkMeta,
                            exported_symbols: &NodeSet)
                            -> (ContextRef, ModuleRef, EncodedMetadata) {
    use std::io::Write;
    use flate2::Compression;
    use flate2::write::DeflateEncoder;

    let (metadata_llcx, metadata_llmod) = unsafe {
        context::create_context_and_module(tcx.sess, llmod_id)
    };

    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    enum MetadataKind {
        None,
        Uncompressed,
        Compressed
    }

    let kind = tcx.sess.crate_types.borrow().iter().map(|ty| {
        match *ty {
            config::CrateTypeExecutable |
            config::CrateTypeStaticlib |
            config::CrateTypeCdylib => MetadataKind::None,

            config::CrateTypeRlib => MetadataKind::Uncompressed,

            config::CrateTypeDylib |
            config::CrateTypeProcMacro => MetadataKind::Compressed,
        }
    }).max().unwrap();

    if kind == MetadataKind::None {
        return (metadata_llcx,
                metadata_llmod,
                EncodedMetadata::new());
    }

    let metadata = tcx.encode_metadata(link_meta, exported_symbols);
    if kind == MetadataKind::Uncompressed {
        return (metadata_llcx, metadata_llmod, metadata);
    }

    assert!(kind == MetadataKind::Compressed);
    let mut compressed = tcx.metadata_encoding_version();
    DeflateEncoder::new(&mut compressed, Compression::Fast)
        .write_all(&metadata.raw_data).unwrap();

    let llmeta = C_bytes_in_context(metadata_llcx, &compressed);
    let llconst = C_struct_in_context(metadata_llcx, &[llmeta], false);
    let name = symbol_export::metadata_symbol_name(tcx);
    let buf = CString::new(name).unwrap();
    let llglobal = unsafe {
        llvm::LLVMAddGlobal(metadata_llmod, val_ty(llconst).to_ref(), buf.as_ptr())
    };
    unsafe {
        llvm::LLVMSetInitializer(llglobal, llconst);
        let section_name = metadata::metadata_section_name(&tcx.sess.target.target);
        let name = CString::new(section_name).unwrap();
        llvm::LLVMSetSection(llglobal, name.as_ptr());

        // Also generate a .section directive to force no
        // flags, at least for ELF outputs, so that the
        // metadata doesn't get loaded into memory.
        let directive = format!(".section {}", section_name);
        let directive = CString::new(directive).unwrap();
        llvm::LLVMSetModuleInlineAsm(metadata_llmod, directive.as_ptr())
    }
    return (metadata_llcx, metadata_llmod, metadata);
}

pub struct ValueIter {
    cur: ValueRef,
    step: unsafe extern "C" fn(ValueRef) -> ValueRef,
}

impl Iterator for ValueIter {
    type Item = ValueRef;

    fn next(&mut self) -> Option<ValueRef> {
        let old = self.cur;
        if !old.is_null() {
            self.cur = unsafe { (self.step)(old) };
            Some(old)
        } else {
            None
        }
    }
}

pub fn iter_globals(llmod: llvm::ModuleRef) -> ValueIter {
    unsafe {
        ValueIter {
            cur: llvm::LLVMGetFirstGlobal(llmod),
            step: llvm::LLVMGetNextGlobal,
        }
    }
}

pub fn trans_crate<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                             rx: mpsc::Receiver<Box<Any + Send>>)
                             -> OngoingCrateTranslation {

    check_for_rustc_errors_attr(tcx);

    if let Some(true) = tcx.sess.opts.debugging_opts.thinlto {
        if unsafe { !llvm::LLVMRustThinLTOAvailable() } {
            tcx.sess.fatal("this compiler's LLVM does not support ThinLTO");
        }
    }

    let crate_hash = tcx.crate_hash(LOCAL_CRATE);
    let link_meta = link::build_link_meta(crate_hash);
    let exported_symbol_node_ids = find_exported_symbols(tcx);

    let shared_ccx = SharedCrateContext::new(tcx);
    // Translate the metadata.
    let llmod_id = "metadata";
    let (metadata_llcx, metadata_llmod, metadata) =
        time(tcx.sess.time_passes(), "write metadata", || {
            write_metadata(tcx, llmod_id, &link_meta, &exported_symbol_node_ids)
        });

    let metadata_module = ModuleTranslation {
        name: link::METADATA_MODULE_NAME.to_string(),
        llmod_id: llmod_id.to_string(),
        source: ModuleSource::Translated(ModuleLlvm {
            llcx: metadata_llcx,
            llmod: metadata_llmod,
            tm: create_target_machine(tcx.sess),
        }),
        kind: ModuleKind::Metadata,
    };

    let time_graph = if tcx.sess.opts.debugging_opts.trans_time_graph {
        Some(time_graph::TimeGraph::new())
    } else {
        None
    };

    // Skip crate items and just output metadata in -Z no-trans mode.
    if tcx.sess.opts.debugging_opts.no_trans ||
       !tcx.sess.opts.output_types.should_trans() {
        let ongoing_translation = write::start_async_translation(
            tcx,
            time_graph.clone(),
            link_meta,
            metadata,
            rx,
            1);

        ongoing_translation.submit_pre_translated_module_to_llvm(tcx, metadata_module);
        ongoing_translation.translation_finished(tcx);

        assert_and_save_dep_graph(tcx);

        ongoing_translation.check_for_errors(tcx.sess);

        return ongoing_translation;
    }

    // Run the translation item collector and partition the collected items into
    // codegen units.
    let codegen_units =
        shared_ccx.tcx().collect_and_partition_translation_items(LOCAL_CRATE).1;
    let codegen_units = (*codegen_units).clone();

    // Force all codegen_unit queries so they are already either red or green
    // when compile_codegen_unit accesses them. We are not able to re-execute
    // the codegen_unit query from just the DepNode, so an unknown color would
    // lead to having to re-execute compile_codegen_unit, possibly
    // unnecessarily.
    if tcx.dep_graph.is_fully_enabled() {
        for cgu in &codegen_units {
            tcx.codegen_unit(cgu.name().clone());
        }
    }

    let ongoing_translation = write::start_async_translation(
        tcx,
        time_graph.clone(),
        link_meta,
        metadata,
        rx,
        codegen_units.len());

    // Translate an allocator shim, if any
    let allocator_module = if let Some(kind) = tcx.sess.allocator_kind.get() {
        unsafe {
            let llmod_id = "allocator";
            let (llcx, llmod) =
                context::create_context_and_module(tcx.sess, llmod_id);
            let modules = ModuleLlvm {
                llmod,
                llcx,
                tm: create_target_machine(tcx.sess),
            };
            time(tcx.sess.time_passes(), "write allocator module", || {
                allocator::trans(tcx, &modules, kind)
            });

            Some(ModuleTranslation {
                name: link::ALLOCATOR_MODULE_NAME.to_string(),
                llmod_id: llmod_id.to_string(),
                source: ModuleSource::Translated(modules),
                kind: ModuleKind::Allocator,
            })
        }
    } else {
        None
    };

    if let Some(allocator_module) = allocator_module {
        ongoing_translation.submit_pre_translated_module_to_llvm(tcx, allocator_module);
    }

    ongoing_translation.submit_pre_translated_module_to_llvm(tcx, metadata_module);

    // We sort the codegen units by size. This way we can schedule work for LLVM
    // a bit more efficiently. Note that "size" is defined rather crudely at the
    // moment as it is just the number of TransItems in the CGU, not taking into
    // account the size of each TransItem.
    let codegen_units = {
        let mut codegen_units = codegen_units;
        codegen_units.sort_by_key(|cgu| -(cgu.items().len() as isize));
        codegen_units
    };

    let mut total_trans_time = Duration::new(0, 0);
    let mut all_stats = Stats::default();

    for cgu in codegen_units.into_iter() {
        ongoing_translation.wait_for_signal_to_translate_item();
        ongoing_translation.check_for_errors(tcx.sess);

        // First, if incremental compilation is enabled, we try to re-use the
        // codegen unit from the cache.
        if tcx.dep_graph.is_fully_enabled() {
            let cgu_id = cgu.work_product_id();

            // Check whether there is a previous work-product we can
            // re-use.  Not only must the file exist, and the inputs not
            // be dirty, but the hash of the symbols we will generate must
            // be the same.
            if let Some(buf) = tcx.dep_graph.previous_work_product(&cgu_id) {
                let dep_node = &DepNode::new(tcx,
                    DepConstructor::CompileCodegenUnit(cgu.name().clone()));

                // We try to mark the DepNode::CompileCodegenUnit green. If we
                // succeed it means that none of the dependencies has changed
                // and we can safely re-use.
                if let Some(dep_node_index) = tcx.dep_graph.try_mark_green(tcx, dep_node) {
                    // Append ".rs" to LLVM module identifier.
                    //
                    // LLVM code generator emits a ".file filename" directive
                    // for ELF backends. Value of the "filename" is set as the
                    // LLVM module identifier.  Due to a LLVM MC bug[1], LLVM
                    // crashes if the module identifier is same as other symbols
                    // such as a function name in the module.
                    // 1. http://llvm.org/bugs/show_bug.cgi?id=11479
                    let llmod_id = format!("{}.rs", cgu.name());

                    let module = ModuleTranslation {
                        name: cgu.name().to_string(),
                        source: ModuleSource::Preexisting(buf),
                        kind: ModuleKind::Regular,
                        llmod_id,
                    };
                    tcx.dep_graph.mark_loaded_from_cache(dep_node_index, true);
                    write::submit_translated_module_to_llvm(tcx, module, 0);
                    // Continue to next cgu, this one is done.
                    continue
                }
            } else {
                // This can happen if files were  deleted from the cache
                // directory for some reason. We just re-compile then.
            }
        }

        let _timing_guard = time_graph.as_ref().map(|time_graph| {
            time_graph.start(write::TRANS_WORKER_TIMELINE,
                             write::TRANS_WORK_PACKAGE_KIND,
                             &format!("codegen {}", cgu.name()))
        });
        let start_time = Instant::now();
        all_stats.extend(tcx.compile_codegen_unit(*cgu.name()));
        total_trans_time += start_time.elapsed();
        ongoing_translation.check_for_errors(tcx.sess);
    }

    ongoing_translation.translation_finished(tcx);

    // Since the main thread is sometimes blocked during trans, we keep track
    // -Ztime-passes output manually.
    print_time_passes_entry(tcx.sess.time_passes(),
                            "translate to LLVM IR",
                            total_trans_time);

    if tcx.sess.opts.incremental.is_some() {
        assert_module_sources::assert_module_sources(tcx);
    }

    symbol_names_test::report_symbol_names(tcx);

    if shared_ccx.sess().trans_stats() {
        println!("--- trans stats ---");
        println!("n_glues_created: {}", all_stats.n_glues_created);
        println!("n_null_glues: {}", all_stats.n_null_glues);
        println!("n_real_glues: {}", all_stats.n_real_glues);

        println!("n_fns: {}", all_stats.n_fns);
        println!("n_inlines: {}", all_stats.n_inlines);
        println!("n_closures: {}", all_stats.n_closures);
        println!("fn stats:");
        all_stats.fn_stats.sort_by_key(|&(_, insns)| insns);
        for &(ref name, insns) in all_stats.fn_stats.iter() {
            println!("{} insns, {}", insns, *name);
        }
    }

    if shared_ccx.sess().count_llvm_insns() {
        for (k, v) in all_stats.llvm_insns.iter() {
            println!("{:7} {}", *v, *k);
        }
    }

    ongoing_translation.check_for_errors(tcx.sess);

    assert_and_save_dep_graph(tcx);
    ongoing_translation
}

fn assert_and_save_dep_graph<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>) {
    time(tcx.sess.time_passes(),
         "assert dep graph",
         || rustc_incremental::assert_dep_graph(tcx));

    time(tcx.sess.time_passes(),
         "serialize dep graph",
         || rustc_incremental::save_dep_graph(tcx));
}

#[inline(never)] // give this a place in the profiler
fn assert_symbols_are_distinct<'a, 'tcx, I>(tcx: TyCtxt<'a, 'tcx, 'tcx>, trans_items: I)
    where I: Iterator<Item=&'a MonoItem<'tcx>>
{
    let mut symbols: Vec<_> = trans_items.map(|trans_item| {
        (trans_item, trans_item.symbol_name(tcx))
    }).collect();

    (&mut symbols[..]).sort_by(|&(_, ref sym1), &(_, ref sym2)|{
        sym1.cmp(sym2)
    });

    for pair in (&symbols[..]).windows(2) {
        let sym1 = &pair[0].1;
        let sym2 = &pair[1].1;

        if *sym1 == *sym2 {
            let trans_item1 = pair[0].0;
            let trans_item2 = pair[1].0;

            let span1 = trans_item1.local_span(tcx);
            let span2 = trans_item2.local_span(tcx);

            // Deterministically select one of the spans for error reporting
            let span = match (span1, span2) {
                (Some(span1), Some(span2)) => {
                    Some(if span1.lo().0 > span2.lo().0 {
                        span1
                    } else {
                        span2
                    })
                }
                (Some(span), None) |
                (None, Some(span)) => Some(span),
                _ => None
            };

            let error_message = format!("symbol `{}` is already defined", sym1);

            if let Some(span) = span {
                tcx.sess.span_fatal(span, &error_message)
            } else {
                tcx.sess.fatal(&error_message)
            }
        }
    }
}

fn collect_and_partition_translation_items<'a, 'tcx>(
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    cnum: CrateNum,
) -> (Arc<DefIdSet>, Arc<Vec<Arc<CodegenUnit<'tcx>>>>)
{
    assert_eq!(cnum, LOCAL_CRATE);
    let time_passes = tcx.sess.time_passes();

    let collection_mode = match tcx.sess.opts.debugging_opts.print_trans_items {
        Some(ref s) => {
            let mode_string = s.to_lowercase();
            let mode_string = mode_string.trim();
            if mode_string == "eager" {
                MonoItemCollectionMode::Eager
            } else {
                if mode_string != "lazy" {
                    let message = format!("Unknown codegen-item collection mode '{}'. \
                                           Falling back to 'lazy' mode.",
                                           mode_string);
                    tcx.sess.warn(&message);
                }

                MonoItemCollectionMode::Lazy
            }
        }
        None => MonoItemCollectionMode::Lazy
    };

    let (items, inlining_map) =
        time(time_passes, "translation item collection", || {
            collector::collect_crate_mono_items(tcx, collection_mode)
    });

    assert_symbols_are_distinct(tcx, items.iter());

    let strategy = if tcx.sess.opts.incremental.is_some() {
        PartitioningStrategy::PerModule
    } else {
        PartitioningStrategy::FixedUnitCount(tcx.sess.codegen_units())
    };

    let codegen_units = time(time_passes, "codegen unit partitioning", || {
        partitioning::partition(tcx,
                                items.iter().cloned(),
                                strategy,
                                &inlining_map)
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>()
    });

    let translation_items: DefIdSet = items.iter().filter_map(|trans_item| {
        match *trans_item {
            MonoItem::Fn(ref instance) => Some(instance.def_id()),
            _ => None,
        }
    }).collect();

    if tcx.sess.opts.debugging_opts.print_trans_items.is_some() {
        let mut item_to_cgus = FxHashMap();

        for cgu in &codegen_units {
            for (&trans_item, &linkage) in cgu.items() {
                item_to_cgus.entry(trans_item)
                            .or_insert(Vec::new())
                            .push((cgu.name().clone(), linkage));
            }
        }

        let mut item_keys: Vec<_> = items
            .iter()
            .map(|i| {
                let mut output = i.to_string(tcx);
                output.push_str(" @@");
                let mut empty = Vec::new();
                let cgus = item_to_cgus.get_mut(i).unwrap_or(&mut empty);
                cgus.as_mut_slice().sort_by_key(|&(ref name, _)| name.clone());
                cgus.dedup();
                for &(ref cgu_name, (linkage, _)) in cgus.iter() {
                    output.push_str(" ");
                    output.push_str(&cgu_name);

                    let linkage_abbrev = match linkage {
                        Linkage::External => "External",
                        Linkage::AvailableExternally => "Available",
                        Linkage::LinkOnceAny => "OnceAny",
                        Linkage::LinkOnceODR => "OnceODR",
                        Linkage::WeakAny => "WeakAny",
                        Linkage::WeakODR => "WeakODR",
                        Linkage::Appending => "Appending",
                        Linkage::Internal => "Internal",
                        Linkage::Private => "Private",
                        Linkage::ExternalWeak => "ExternalWeak",
                        Linkage::Common => "Common",
                    };

                    output.push_str("[");
                    output.push_str(linkage_abbrev);
                    output.push_str("]");
                }
                output
            })
            .collect();

        item_keys.sort();

        for item in item_keys {
            println!("TRANS_ITEM {}", item);
        }
    }

    (Arc::new(translation_items), Arc::new(codegen_units))
}

impl CrateInfo {
    pub fn new(tcx: TyCtxt) -> CrateInfo {
        let mut info = CrateInfo {
            panic_runtime: None,
            compiler_builtins: None,
            profiler_runtime: None,
            sanitizer_runtime: None,
            is_no_builtins: FxHashSet(),
            native_libraries: FxHashMap(),
            used_libraries: tcx.native_libraries(LOCAL_CRATE),
            link_args: tcx.link_args(LOCAL_CRATE),
            crate_name: FxHashMap(),
            used_crates_dynamic: cstore::used_crates(tcx, LinkagePreference::RequireDynamic),
            used_crates_static: cstore::used_crates(tcx, LinkagePreference::RequireStatic),
            used_crate_source: FxHashMap(),
        };

        for &cnum in tcx.crates().iter() {
            info.native_libraries.insert(cnum, tcx.native_libraries(cnum));
            info.crate_name.insert(cnum, tcx.crate_name(cnum).to_string());
            info.used_crate_source.insert(cnum, tcx.used_crate_source(cnum));
            if tcx.is_panic_runtime(cnum) {
                info.panic_runtime = Some(cnum);
            }
            if tcx.is_compiler_builtins(cnum) {
                info.compiler_builtins = Some(cnum);
            }
            if tcx.is_profiler_runtime(cnum) {
                info.profiler_runtime = Some(cnum);
            }
            if tcx.is_sanitizer_runtime(cnum) {
                info.sanitizer_runtime = Some(cnum);
            }
            if tcx.is_no_builtins(cnum) {
                info.is_no_builtins.insert(cnum);
            }
        }


        return info
    }
}

fn is_translated_function(tcx: TyCtxt, id: DefId) -> bool {
    let (all_trans_items, _) =
        tcx.collect_and_partition_translation_items(LOCAL_CRATE);
    all_trans_items.contains(&id)
}

fn compile_codegen_unit<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                  cgu: InternedString) -> Stats {
    let cgu = tcx.codegen_unit(cgu);

    let start_time = Instant::now();
    let (stats, module) = module_translation(tcx, cgu);
    let time_to_translate = start_time.elapsed();

    // We assume that the cost to run LLVM on a CGU is proportional to
    // the time we needed for translating it.
    let cost = time_to_translate.as_secs() * 1_000_000_000 +
               time_to_translate.subsec_nanos() as u64;

    write::submit_translated_module_to_llvm(tcx,
                                            module,
                                            cost);
    return stats;

    fn module_translation<'a, 'tcx>(
        tcx: TyCtxt<'a, 'tcx, 'tcx>,
        cgu: Arc<CodegenUnit<'tcx>>)
        -> (Stats, ModuleTranslation)
    {
        let cgu_name = cgu.name().to_string();

        // Append ".rs" to LLVM module identifier.
        //
        // LLVM code generator emits a ".file filename" directive
        // for ELF backends. Value of the "filename" is set as the
        // LLVM module identifier.  Due to a LLVM MC bug[1], LLVM
        // crashes if the module identifier is same as other symbols
        // such as a function name in the module.
        // 1. http://llvm.org/bugs/show_bug.cgi?id=11479
        let llmod_id = format!("{}-{}.rs",
                               cgu.name(),
                               tcx.crate_disambiguator(LOCAL_CRATE)
                                   .to_fingerprint().to_hex());

        // Instantiate translation items without filling out definitions yet...
        let scx = SharedCrateContext::new(tcx);
        let lcx = LocalCrateContext::new(&scx, cgu, &llmod_id);
        let module = {
            let ccx = CrateContext::new(&scx, &lcx);
            let trans_items = ccx.codegen_unit()
                                 .items_in_deterministic_order(ccx.tcx());
            for &(trans_item, (linkage, visibility)) in &trans_items {
                trans_item.predefine(&ccx, linkage, visibility);
            }

            // ... and now that we have everything pre-defined, fill out those definitions.
            for &(trans_item, _) in &trans_items {
                trans_item.define(&ccx);
            }

            // If this codegen unit contains the main function, also create the
            // wrapper here
            maybe_create_entry_wrapper(&ccx);

            // Run replace-all-uses-with for statics that need it
            for &(old_g, new_g) in ccx.statics_to_rauw().borrow().iter() {
                unsafe {
                    let bitcast = llvm::LLVMConstPointerCast(new_g, llvm::LLVMTypeOf(old_g));
                    llvm::LLVMReplaceAllUsesWith(old_g, bitcast);
                    llvm::LLVMDeleteGlobal(old_g);
                }
            }

            // Create the llvm.used variable
            // This variable has type [N x i8*] and is stored in the llvm.metadata section
            if !ccx.used_statics().borrow().is_empty() {
                let name = CString::new("llvm.used").unwrap();
                let section = CString::new("llvm.metadata").unwrap();
                let array = C_array(Type::i8(&ccx).ptr_to(), &*ccx.used_statics().borrow());

                unsafe {
                    let g = llvm::LLVMAddGlobal(ccx.llmod(),
                                                val_ty(array).to_ref(),
                                                name.as_ptr());
                    llvm::LLVMSetInitializer(g, array);
                    llvm::LLVMRustSetLinkage(g, llvm::Linkage::AppendingLinkage);
                    llvm::LLVMSetSection(g, section.as_ptr());
                }
            }

            // Finalize debuginfo
            if ccx.sess().opts.debuginfo != NoDebugInfo {
                debuginfo::finalize(&ccx);
            }

            let llvm_module = ModuleLlvm {
                llcx: ccx.llcx(),
                llmod: ccx.llmod(),
                tm: create_target_machine(ccx.sess()),
            };

            ModuleTranslation {
                name: cgu_name,
                source: ModuleSource::Translated(llvm_module),
                kind: ModuleKind::Regular,
                llmod_id,
            }
        };

        (lcx.into_stats(), module)
    }
}

pub fn provide(providers: &mut Providers) {
    providers.collect_and_partition_translation_items =
        collect_and_partition_translation_items;

    providers.is_translated_function = is_translated_function;

    providers.codegen_unit = |tcx, name| {
        let (_, all) = tcx.collect_and_partition_translation_items(LOCAL_CRATE);
        all.iter()
            .find(|cgu| *cgu.name() == name)
            .cloned()
            .expect(&format!("failed to find cgu with name {:?}", name))
    };
    providers.compile_codegen_unit = compile_codegen_unit;
}

pub fn linkage_to_llvm(linkage: Linkage) -> llvm::Linkage {
    match linkage {
        Linkage::External => llvm::Linkage::ExternalLinkage,
        Linkage::AvailableExternally => llvm::Linkage::AvailableExternallyLinkage,
        Linkage::LinkOnceAny => llvm::Linkage::LinkOnceAnyLinkage,
        Linkage::LinkOnceODR => llvm::Linkage::LinkOnceODRLinkage,
        Linkage::WeakAny => llvm::Linkage::WeakAnyLinkage,
        Linkage::WeakODR => llvm::Linkage::WeakODRLinkage,
        Linkage::Appending => llvm::Linkage::AppendingLinkage,
        Linkage::Internal => llvm::Linkage::InternalLinkage,
        Linkage::Private => llvm::Linkage::PrivateLinkage,
        Linkage::ExternalWeak => llvm::Linkage::ExternalWeakLinkage,
        Linkage::Common => llvm::Linkage::CommonLinkage,
    }
}

pub fn visibility_to_llvm(linkage: Visibility) -> llvm::Visibility {
    match linkage {
        Visibility::Default => llvm::Visibility::Default,
        Visibility::Hidden => llvm::Visibility::Hidden,
        Visibility::Protected => llvm::Visibility::Protected,
    }
}

// FIXME(mw): Anything that is produced via DepGraph::with_task() must implement
//            the HashStable trait. Normally DepGraph::with_task() calls are
//            hidden behind queries, but CGU creation is a special case in two
//            ways: (1) it's not a query and (2) CGU are output nodes, so their
//            Fingerprints are not actually needed. It remains to be clarified
//            how exactly this case will be handled in the red/green system but
//            for now we content ourselves with providing a no-op HashStable
//            implementation for CGUs.
mod temp_stable_hash_impls {
    use rustc_data_structures::stable_hasher::{StableHasherResult, StableHasher,
                                               HashStable};
    use ModuleTranslation;

    impl<HCX> HashStable<HCX> for ModuleTranslation {
        fn hash_stable<W: StableHasherResult>(&self,
                                              _: &mut HCX,
                                              _: &mut StableHasher<W>) {
            // do nothing
        }
    }
}
