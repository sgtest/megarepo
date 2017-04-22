// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use abi::FnType;
use adt;
use common::*;
use machine;
use rustc::ty::{self, Ty, TypeFoldable};
use rustc::ty::layout::LayoutTyper;
use trans_item::DefPathBasedNames;
use type_::Type;

use syntax::ast;

pub fn fat_ptr_base_ty<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>, ty: Ty<'tcx>) -> Type {
    match ty.sty {
        ty::TyRef(_, ty::TypeAndMut { ty: t, .. }) |
        ty::TyRawPtr(ty::TypeAndMut { ty: t, .. }) if !ccx.shared().type_is_sized(t) => {
            in_memory_type_of(ccx, t).ptr_to()
        }
        ty::TyAdt(def, _) if def.is_box() => {
            in_memory_type_of(ccx, ty.boxed_ty()).ptr_to()
        }
        _ => bug!("expected fat ptr ty but got {:?}", ty)
    }
}

pub fn unsized_info_ty<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>, ty: Ty<'tcx>) -> Type {
    let unsized_part = ccx.tcx().struct_tail(ty);
    match unsized_part.sty {
        ty::TyStr | ty::TyArray(..) | ty::TySlice(_) => {
            Type::uint_from_ty(ccx, ast::UintTy::Us)
        }
        ty::TyDynamic(..) => Type::vtable_ptr(ccx),
        _ => bug!("Unexpected tail in unsized_info_ty: {:?} for ty={:?}",
                          unsized_part, ty)
    }
}

pub fn immediate_type_of<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, t: Ty<'tcx>) -> Type {
    if t.is_bool() {
        Type::i1(cx)
    } else {
        type_of(cx, t)
    }
}

/// Get the LLVM type corresponding to a Rust type, i.e. `rustc::ty::Ty`.
/// This is the right LLVM type for an alloca containing a value of that type,
/// and the pointee of an Lvalue Datum (which is always a LLVM pointer).
/// For unsized types, the returned type is a fat pointer, thus the resulting
/// LLVM type for a `Trait` Lvalue is `{ i8*, void(i8*)** }*`, which is a double
/// indirection to the actual data, unlike a `i8` Lvalue, which is just `i8*`.
/// This is needed due to the treatment of immediate values, as a fat pointer
/// is too large for it to be placed in SSA value (by our rules).
/// For the raw type without far pointer indirection, see `in_memory_type_of`.
pub fn type_of<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, ty: Ty<'tcx>) -> Type {
    let ty = if !cx.shared().type_is_sized(ty) {
        cx.tcx().mk_imm_ptr(ty)
    } else {
        ty
    };
    in_memory_type_of(cx, ty)
}

/// Get the LLVM type corresponding to a Rust type, i.e. `rustc::ty::Ty`.
/// This is the right LLVM type for a field/array element of that type,
/// and is the same as `type_of` for all Sized types.
/// Unsized types, however, are represented by a "minimal unit", e.g.
/// `[T]` becomes `T`, while `str` and `Trait` turn into `i8` - this
/// is useful for indexing slices, as `&[T]`'s data pointer is `T*`.
/// If the type is an unsized struct, the regular layout is generated,
/// with the inner-most trailing unsized field using the "minimal unit"
/// of that field's type - this is useful for taking the address of
/// that field and ensuring the struct has the right alignment.
/// For the LLVM type of a value as a whole, see `type_of`.
pub fn in_memory_type_of<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, t: Ty<'tcx>) -> Type {
    // Check the cache.
    if let Some(&llty) = cx.lltypes().borrow().get(&t) {
        return llty;
    }

    debug!("type_of {:?}", t);

    assert!(!t.has_escaping_regions(), "{:?} has escaping regions", t);

    // Replace any typedef'd types with their equivalent non-typedef
    // type. This ensures that all LLVM nominal types that contain
    // Rust types are defined as the same LLVM types.  If we don't do
    // this then, e.g. `Option<{myfield: bool}>` would be a different
    // type than `Option<myrec>`.
    let t_norm = cx.tcx().erase_regions(&t);

    if t != t_norm {
        let llty = in_memory_type_of(cx, t_norm);
        debug!("--> normalized {:?} to {:?} llty={:?}", t, t_norm, llty);
        cx.lltypes().borrow_mut().insert(t, llty);
        return llty;
    }

    let ptr_ty = |ty: Ty<'tcx>| {
        if !cx.shared().type_is_sized(ty) {
            if let ty::TyStr = ty.sty {
                // This means we get a nicer name in the output (str is always
                // unsized).
                cx.str_slice_type()
            } else {
                let ptr_ty = in_memory_type_of(cx, ty).ptr_to();
                let info_ty = unsized_info_ty(cx, ty);
                Type::struct_(cx, &[ptr_ty, info_ty], false)
            }
        } else {
            in_memory_type_of(cx, ty).ptr_to()
        }
    };

    let mut llty = match t.sty {
      ty::TyBool => Type::bool(cx),
      ty::TyChar => Type::char(cx),
      ty::TyInt(t) => Type::int_from_ty(cx, t),
      ty::TyUint(t) => Type::uint_from_ty(cx, t),
      ty::TyFloat(t) => Type::float_from_ty(cx, t),
      ty::TyNever => Type::nil(cx),
      ty::TyClosure(..) => {
          // Only create the named struct, but don't fill it in. We
          // fill it in *after* placing it into the type cache.
          adt::incomplete_type_of(cx, t, "closure")
      }

      ty::TyRef(_, ty::TypeAndMut{ty, ..}) |
      ty::TyRawPtr(ty::TypeAndMut{ty, ..}) => {
          ptr_ty(ty)
      }
      ty::TyAdt(def, _) if def.is_box() => {
          ptr_ty(t.boxed_ty())
      }

      ty::TyArray(ty, size) => {
          let size = size as u64;
          let llty = in_memory_type_of(cx, ty);
          Type::array(&llty, size)
      }

      // Unsized slice types (and str) have the type of their element, and
      // traits have the type of u8. This is so that the data pointer inside
      // fat pointers is of the right type (e.g. for array accesses), even
      // when taking the address of an unsized field in a struct.
      ty::TySlice(ty) => in_memory_type_of(cx, ty),
      ty::TyStr | ty::TyDynamic(..) => Type::i8(cx),

      ty::TyFnDef(..) => Type::nil(cx),
      ty::TyFnPtr(sig) => {
        let sig = cx.tcx().erase_late_bound_regions_and_normalize(&sig);
        FnType::new(cx, sig, &[]).llvm_type(cx).ptr_to()
      }
      ty::TyTuple(ref tys, _) if tys.is_empty() => Type::nil(cx),
      ty::TyTuple(..) => {
          adt::type_of(cx, t)
      }
      ty::TyAdt(..) if t.is_simd() => {
          let e = t.simd_type(cx.tcx());
          if !e.is_machine() {
              cx.sess().fatal(&format!("monomorphising SIMD type `{}` with \
                                        a non-machine element type `{}`",
                                       t, e))
          }
          let llet = in_memory_type_of(cx, e);
          let n = t.simd_size(cx.tcx()) as u64;
          Type::vector(&llet, n)
      }
      ty::TyAdt(..) => {
          // Only create the named struct, but don't fill it in. We
          // fill it in *after* placing it into the type cache. This
          // avoids creating more than one copy of the enum when one
          // of the enum's variants refers to the enum itself.
          let name = llvm_type_name(cx, t);
          adt::incomplete_type_of(cx, t, &name[..])
      }

      ty::TyInfer(..) |
      ty::TyProjection(..) |
      ty::TyParam(..) |
      ty::TyAnon(..) |
      ty::TyError => bug!("type_of with {:?}", t),
    };

    debug!("--> mapped t={:?} to llty={:?}", t, llty);

    cx.lltypes().borrow_mut().insert(t, llty);

    // If this was an enum or struct, fill in the type now.
    match t.sty {
        ty::TyAdt(..) | ty::TyClosure(..) if !t.is_simd() && !t.is_box() => {
            adt::finish_type_of(cx, t, &mut llty);
        }
        _ => ()
    }

    llty
}

impl<'a, 'tcx> CrateContext<'a, 'tcx> {
    pub fn align_of(&self, ty: Ty<'tcx>) -> machine::llalign {
        self.layout_of(ty).align(self).abi() as machine::llalign
    }

    pub fn size_of(&self, ty: Ty<'tcx>) -> machine::llsize {
        self.layout_of(ty).size(self).bytes() as machine::llsize
    }

    pub fn over_align_of(&self, t: Ty<'tcx>)
                              -> Option<machine::llalign> {
        let layout = self.layout_of(t);
        if let Some(align) = layout.over_align(&self.tcx().data_layout) {
            Some(align as machine::llalign)
        } else {
            None
        }
    }
}

fn llvm_type_name<'a, 'tcx>(cx: &CrateContext<'a, 'tcx>, ty: Ty<'tcx>) -> String {
    let mut name = String::with_capacity(32);
    let printer = DefPathBasedNames::new(cx.tcx(), true, true);
    printer.push_type_name(ty, &mut name);
    name
}
