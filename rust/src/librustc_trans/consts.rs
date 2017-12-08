// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use llvm;
use llvm::{SetUnnamedAddr};
use llvm::{ValueRef, True};
use rustc::hir::def_id::DefId;
use rustc::hir::map as hir_map;
use rustc::middle::const_val::ConstEvalErr;
use debuginfo;
use base;
use trans_item::{TransItem, TransItemExt};
use common::{self, CrateContext, val_ty};
use declare;
use monomorphize::Instance;
use type_::Type;
use type_of::LayoutLlvmExt;
use rustc::ty;
use rustc::ty::layout::{Align, LayoutOf};

use rustc::hir;

use std::ffi::{CStr, CString};
use syntax::ast;
use syntax::attr;

pub fn ptrcast(val: ValueRef, ty: Type) -> ValueRef {
    unsafe {
        llvm::LLVMConstPointerCast(val, ty.to_ref())
    }
}

pub fn bitcast(val: ValueRef, ty: Type) -> ValueRef {
    unsafe {
        llvm::LLVMConstBitCast(val, ty.to_ref())
    }
}

fn set_global_alignment(ccx: &CrateContext,
                        gv: ValueRef,
                        mut align: Align) {
    // The target may require greater alignment for globals than the type does.
    // Note: GCC and Clang also allow `__attribute__((aligned))` on variables,
    // which can force it to be smaller.  Rust doesn't support this yet.
    if let Some(min) = ccx.sess().target.target.options.min_global_align {
        match ty::layout::Align::from_bits(min, min) {
            Ok(min) => align = align.max(min),
            Err(err) => {
                ccx.sess().err(&format!("invalid minimum global alignment: {}", err));
            }
        }
    }
    unsafe {
        llvm::LLVMSetAlignment(gv, align.abi() as u32);
    }
}

pub fn addr_of_mut(ccx: &CrateContext,
                   cv: ValueRef,
                   align: Align,
                   kind: &str)
                    -> ValueRef {
    unsafe {
        let name = ccx.generate_local_symbol_name(kind);
        let gv = declare::define_global(ccx, &name[..], val_ty(cv)).unwrap_or_else(||{
            bug!("symbol `{}` is already defined", name);
        });
        llvm::LLVMSetInitializer(gv, cv);
        set_global_alignment(ccx, gv, align);
        llvm::LLVMRustSetLinkage(gv, llvm::Linkage::PrivateLinkage);
        SetUnnamedAddr(gv, true);
        gv
    }
}

pub fn addr_of(ccx: &CrateContext,
               cv: ValueRef,
               align: Align,
               kind: &str)
               -> ValueRef {
    if let Some(&gv) = ccx.const_globals().borrow().get(&cv) {
        unsafe {
            // Upgrade the alignment in cases where the same constant is used with different
            // alignment requirements
            let llalign = align.abi() as u32;
            if llalign > llvm::LLVMGetAlignment(gv) {
                llvm::LLVMSetAlignment(gv, llalign);
            }
        }
        return gv;
    }
    let gv = addr_of_mut(ccx, cv, align, kind);
    unsafe {
        llvm::LLVMSetGlobalConstant(gv, True);
    }
    ccx.const_globals().borrow_mut().insert(cv, gv);
    gv
}

pub fn get_static(ccx: &CrateContext, def_id: DefId) -> ValueRef {
    let instance = Instance::mono(ccx.tcx(), def_id);
    if let Some(&g) = ccx.instances().borrow().get(&instance) {
        return g;
    }

    let ty = common::instance_ty(ccx.tcx(), &instance);
    let g = if let Some(id) = ccx.tcx().hir.as_local_node_id(def_id) {

        let llty = ccx.layout_of(ty).llvm_type(ccx);
        let (g, attrs) = match ccx.tcx().hir.get(id) {
            hir_map::NodeItem(&hir::Item {
                ref attrs, span, node: hir::ItemStatic(..), ..
            }) => {
                let sym = TransItem::Static(id).symbol_name(ccx.tcx());

                let defined_in_current_codegen_unit = ccx.codegen_unit()
                                                         .items()
                                                         .contains_key(&TransItem::Static(id));
                assert!(!defined_in_current_codegen_unit);

                if declare::get_declared_value(ccx, &sym[..]).is_some() {
                    span_bug!(span, "trans: Conflicting symbol names for static?");
                }

                let g = declare::define_global(ccx, &sym[..], llty).unwrap();

                if !ccx.tcx().is_exported_symbol(def_id) {
                    unsafe {
                        llvm::LLVMRustSetVisibility(g, llvm::Visibility::Hidden);
                    }
                }

                (g, attrs)
            }

            hir_map::NodeForeignItem(&hir::ForeignItem {
                ref attrs, span, node: hir::ForeignItemStatic(..), ..
            }) => {
                let sym = ccx.tcx().symbol_name(instance);
                let g = if let Some(name) =
                        attr::first_attr_value_str_by_name(&attrs, "linkage") {
                    // If this is a static with a linkage specified, then we need to handle
                    // it a little specially. The typesystem prevents things like &T and
                    // extern "C" fn() from being non-null, so we can't just declare a
                    // static and call it a day. Some linkages (like weak) will make it such
                    // that the static actually has a null value.
                    let linkage = match base::linkage_by_name(&name.as_str()) {
                        Some(linkage) => linkage,
                        None => {
                            ccx.sess().span_fatal(span, "invalid linkage specified");
                        }
                    };
                    let llty2 = match ty.sty {
                        ty::TyRawPtr(ref mt) => ccx.layout_of(mt.ty).llvm_type(ccx),
                        _ => {
                            ccx.sess().span_fatal(span, "must have type `*const T` or `*mut T`");
                        }
                    };
                    unsafe {
                        // Declare a symbol `foo` with the desired linkage.
                        let g1 = declare::declare_global(ccx, &sym, llty2);
                        llvm::LLVMRustSetLinkage(g1, base::linkage_to_llvm(linkage));

                        // Declare an internal global `extern_with_linkage_foo` which
                        // is initialized with the address of `foo`.  If `foo` is
                        // discarded during linking (for example, if `foo` has weak
                        // linkage and there are no definitions), then
                        // `extern_with_linkage_foo` will instead be initialized to
                        // zero.
                        let mut real_name = "_rust_extern_with_linkage_".to_string();
                        real_name.push_str(&sym);
                        let g2 = declare::define_global(ccx, &real_name, llty).unwrap_or_else(||{
                            ccx.sess().span_fatal(span,
                                &format!("symbol `{}` is already defined", &sym))
                        });
                        llvm::LLVMRustSetLinkage(g2, llvm::Linkage::InternalLinkage);
                        llvm::LLVMSetInitializer(g2, g1);
                        g2
                    }
                } else {
                    // Generate an external declaration.
                    declare::declare_global(ccx, &sym, llty)
                };

                (g, attrs)
            }

            item => bug!("get_static: expected static, found {:?}", item)
        };

        for attr in attrs {
            if attr.check_name("thread_local") {
                llvm::set_thread_local_mode(g, ccx.tls_model());
            }
        }

        g
    } else {
        let sym = ccx.tcx().symbol_name(instance);

        // FIXME(nagisa): perhaps the map of externs could be offloaded to llvm somehow?
        // FIXME(nagisa): investigate whether it can be changed into define_global
        let g = declare::declare_global(ccx, &sym, ccx.layout_of(ty).llvm_type(ccx));
        // Thread-local statics in some other crate need to *always* be linked
        // against in a thread-local fashion, so we need to be sure to apply the
        // thread-local attribute locally if it was present remotely. If we
        // don't do this then linker errors can be generated where the linker
        // complains that one object files has a thread local version of the
        // symbol and another one doesn't.
        for attr in ccx.tcx().get_attrs(def_id).iter() {
            if attr.check_name("thread_local") {
                llvm::set_thread_local_mode(g, ccx.tls_model());
            }
        }
        if ccx.use_dll_storage_attrs() && !ccx.tcx().is_foreign_item(def_id) {
            // This item is external but not foreign, i.e. it originates from an external Rust
            // crate. Since we don't know whether this crate will be linked dynamically or
            // statically in the final application, we always mark such symbols as 'dllimport'.
            // If final linkage happens to be static, we rely on compiler-emitted __imp_ stubs to
            // make things work.
            unsafe {
                llvm::LLVMSetDLLStorageClass(g, llvm::DLLStorageClass::DllImport);
            }
        }
        g
    };

    if ccx.use_dll_storage_attrs() && ccx.tcx().is_dllimport_foreign_item(def_id) {
        // For foreign (native) libs we know the exact storage type to use.
        unsafe {
            llvm::LLVMSetDLLStorageClass(g, llvm::DLLStorageClass::DllImport);
        }
    }

    ccx.instances().borrow_mut().insert(instance, g);
    ccx.statics().borrow_mut().insert(g, def_id);
    g
}

pub fn trans_static<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                              m: hir::Mutability,
                              id: ast::NodeId,
                              attrs: &[ast::Attribute])
                              -> Result<ValueRef, ConstEvalErr<'tcx>> {
    unsafe {
        let def_id = ccx.tcx().hir.local_def_id(id);
        let g = get_static(ccx, def_id);

        let v = ::mir::trans_static_initializer(ccx, def_id)?;

        // boolean SSA values are i1, but they have to be stored in i8 slots,
        // otherwise some LLVM optimization passes don't work as expected
        let mut val_llty = val_ty(v);
        let v = if val_llty == Type::i1(ccx) {
            val_llty = Type::i8(ccx);
            llvm::LLVMConstZExt(v, val_llty.to_ref())
        } else {
            v
        };

        let instance = Instance::mono(ccx.tcx(), def_id);
        let ty = common::instance_ty(ccx.tcx(), &instance);
        let llty = ccx.layout_of(ty).llvm_type(ccx);
        let g = if val_llty == llty {
            g
        } else {
            // If we created the global with the wrong type,
            // correct the type.
            let empty_string = CString::new("").unwrap();
            let name_str_ref = CStr::from_ptr(llvm::LLVMGetValueName(g));
            let name_string = CString::new(name_str_ref.to_bytes()).unwrap();
            llvm::LLVMSetValueName(g, empty_string.as_ptr());

            let linkage = llvm::LLVMRustGetLinkage(g);
            let visibility = llvm::LLVMRustGetVisibility(g);

            let new_g = llvm::LLVMRustGetOrInsertGlobal(
                ccx.llmod(), name_string.as_ptr(), val_llty.to_ref());

            llvm::LLVMRustSetLinkage(new_g, linkage);
            llvm::LLVMRustSetVisibility(new_g, visibility);

            // To avoid breaking any invariants, we leave around the old
            // global for the moment; we'll replace all references to it
            // with the new global later. (See base::trans_crate.)
            ccx.statics_to_rauw().borrow_mut().push((g, new_g));
            new_g
        };
        set_global_alignment(ccx, g, ccx.align_of(ty));
        llvm::LLVMSetInitializer(g, v);

        // As an optimization, all shared statics which do not have interior
        // mutability are placed into read-only memory.
        if m != hir::MutMutable {
            if ccx.shared().type_is_freeze(ty) {
                llvm::LLVMSetGlobalConstant(g, llvm::True);
            }
        }

        debuginfo::create_global_var_metadata(ccx, id, g);

        if attr::contains_name(attrs, "thread_local") {
            llvm::set_thread_local_mode(g, ccx.tls_model());
        }

        base::set_link_section(ccx, g, attrs);

        if attr::contains_name(attrs, "used") {
            // This static will be stored in the llvm.used variable which is an array of i8*
            let cast = llvm::LLVMConstPointerCast(g, Type::i8p(ccx).to_ref());
            ccx.used_statics().borrow_mut().push(cast);
        }

        Ok(g)
    }
}
