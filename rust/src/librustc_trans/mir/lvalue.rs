// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use llvm::ValueRef;
use rustc::ty::{self, Ty, TypeFoldable};
use rustc::ty::layout::{self, LayoutTyper};
use rustc::mir;
use rustc::mir::tcx::LvalueTy;
use rustc_data_structures::indexed_vec::Idx;
use adt;
use builder::Builder;
use common::{self, CrateContext, C_uint};
use consts;
use machine;
use type_of;
use type_::Type;
use value::Value;
use glue;

use std::ptr;
use std::ops;

use super::{MirContext, LocalRef};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Alignment {
    Packed,
    AbiAligned,
}

impl ops::BitOr for Alignment {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        match (self, rhs) {
            (Alignment::Packed, _) => Alignment::Packed,
            (Alignment::AbiAligned, a) => a,
        }
    }
}

impl Alignment {
    pub fn from_packed(packed: bool) -> Self {
        if packed {
            Alignment::Packed
        } else {
            Alignment::AbiAligned
        }
    }

    pub fn to_align(self) -> Option<u32> {
        match self {
            Alignment::Packed => Some(1),
            Alignment::AbiAligned => None,
        }
    }

    pub fn min_with(self, align: u32) -> Option<u32> {
        match self {
            Alignment::Packed => Some(1),
            Alignment::AbiAligned => Some(align),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct LvalueRef<'tcx> {
    /// Pointer to the contents of the lvalue
    pub llval: ValueRef,

    /// This lvalue's extra data if it is unsized, or null
    pub llextra: ValueRef,

    /// Monomorphized type of this lvalue, including variant information
    pub ty: LvalueTy<'tcx>,

    /// Whether this lvalue is known to be aligned according to its layout
    pub alignment: Alignment,
}

impl<'a, 'tcx> LvalueRef<'tcx> {
    pub fn new_sized(llval: ValueRef, lvalue_ty: LvalueTy<'tcx>,
                     alignment: Alignment) -> LvalueRef<'tcx> {
        LvalueRef { llval: llval, llextra: ptr::null_mut(), ty: lvalue_ty, alignment: alignment }
    }

    pub fn new_sized_ty(llval: ValueRef, ty: Ty<'tcx>, alignment: Alignment) -> LvalueRef<'tcx> {
        LvalueRef::new_sized(llval, LvalueTy::from_ty(ty), alignment)
    }

    pub fn alloca(bcx: &Builder<'a, 'tcx>, ty: Ty<'tcx>, name: &str) -> LvalueRef<'tcx> {
        debug!("alloca({:?}: {:?})", name, ty);
        let tmp = bcx.alloca(
            type_of::type_of(bcx.ccx, ty), name, bcx.ccx.over_align_of(ty));
        assert!(!ty.has_param_types());
        Self::new_sized_ty(tmp, ty, Alignment::AbiAligned)
    }

    pub fn len(&self, ccx: &CrateContext<'a, 'tcx>) -> ValueRef {
        let ty = self.ty.to_ty(ccx.tcx());
        match ty.sty {
            ty::TyArray(_, n) => common::C_uint(ccx, n),
            ty::TySlice(_) | ty::TyStr => {
                assert!(self.llextra != ptr::null_mut());
                self.llextra
            }
            _ => bug!("unexpected type `{}` in LvalueRef::len", ty)
        }
    }

    pub fn has_extra(&self) -> bool {
        !self.llextra.is_null()
    }

    fn struct_field_ptr(
        self,
        bcx: &Builder<'a, 'tcx>,
        st: &layout::Struct,
        fields: &Vec<Ty<'tcx>>,
        ix: usize,
        needs_cast: bool
    ) -> (ValueRef, Alignment) {
        let fty = fields[ix];
        let ccx = bcx.ccx;

        let alignment = self.alignment | Alignment::from_packed(st.packed);

        let llfields = adt::struct_llfields(ccx, fields, st);
        let ptr_val = if needs_cast {
            let real_ty = Type::struct_(ccx, &llfields[..], st.packed);
            bcx.pointercast(self.llval, real_ty.ptr_to())
        } else {
            self.llval
        };

        // Simple case - we can just GEP the field
        //   * First field - Always aligned properly
        //   * Packed struct - There is no alignment padding
        //   * Field is sized - pointer is properly aligned already
        if st.offsets[ix] == layout::Size::from_bytes(0) || st.packed ||
            bcx.ccx.shared().type_is_sized(fty) {
                return (bcx.struct_gep(
                        ptr_val, adt::struct_llfields_index(st, ix)), alignment);
            }

        // If the type of the last field is [T] or str, then we don't need to do
        // any adjusments
        match fty.sty {
            ty::TySlice(..) | ty::TyStr => {
                return (bcx.struct_gep(
                        ptr_val, adt::struct_llfields_index(st, ix)), alignment);
            }
            _ => ()
        }

        // There's no metadata available, log the case and just do the GEP.
        if !self.has_extra() {
            debug!("Unsized field `{}`, of `{:?}` has no metadata for adjustment",
                ix, Value(ptr_val));
            return (bcx.struct_gep(ptr_val, adt::struct_llfields_index(st, ix)), alignment);
        }

        // We need to get the pointer manually now.
        // We do this by casting to a *i8, then offsetting it by the appropriate amount.
        // We do this instead of, say, simply adjusting the pointer from the result of a GEP
        // because the field may have an arbitrary alignment in the LLVM representation
        // anyway.
        //
        // To demonstrate:
        //   struct Foo<T: ?Sized> {
        //      x: u16,
        //      y: T
        //   }
        //
        // The type Foo<Foo<Trait>> is represented in LLVM as { u16, { u16, u8 }}, meaning that
        // the `y` field has 16-bit alignment.

        let meta = self.llextra;


        let offset = st.offsets[ix].bytes();
        let unaligned_offset = C_uint(bcx.ccx, offset);

        // Get the alignment of the field
        let (_, align) = glue::size_and_align_of_dst(bcx, fty, meta);

        // Bump the unaligned offset up to the appropriate alignment using the
        // following expression:
        //
        //   (unaligned offset + (align - 1)) & -align

        // Calculate offset
        let align_sub_1 = bcx.sub(align, C_uint(bcx.ccx, 1u64));
        let offset = bcx.and(bcx.add(unaligned_offset, align_sub_1),
        bcx.neg(align));

        debug!("struct_field_ptr: DST field offset: {:?}", Value(offset));

        // Cast and adjust pointer
        let byte_ptr = bcx.pointercast(ptr_val, Type::i8p(bcx.ccx));
        let byte_ptr = bcx.gep(byte_ptr, &[offset]);

        // Finally, cast back to the type expected
        let ll_fty = type_of::in_memory_type_of(bcx.ccx, fty);
        debug!("struct_field_ptr: Field type is {:?}", ll_fty);
        (bcx.pointercast(byte_ptr, ll_fty.ptr_to()), alignment)
    }

    /// Access a field, at a point when the value's case is known.
    pub fn trans_field_ptr(self, bcx: &Builder<'a, 'tcx>, ix: usize) -> (ValueRef, Alignment) {
        let discr = match self.ty {
            LvalueTy::Ty { .. } => 0,
            LvalueTy::Downcast { variant_index, .. } => variant_index,
        };
        let t = self.ty.to_ty(bcx.tcx());
        let l = bcx.ccx.layout_of(t);
        // Note: if this ever needs to generate conditionals (e.g., if we
        // decide to do some kind of cdr-coding-like non-unique repr
        // someday), it will need to return a possibly-new bcx as well.
        match *l {
            layout::Univariant { ref variant, .. } => {
                assert_eq!(discr, 0);
                self.struct_field_ptr(bcx, &variant,
                    &adt::compute_fields(bcx.ccx, t, 0, false), ix, false)
            }
            layout::Vector { count, .. } => {
                assert_eq!(discr, 0);
                assert!((ix as u64) < count);
                (bcx.struct_gep(self.llval, ix), self.alignment)
            }
            layout::General { discr: d, ref variants, .. } => {
                let mut fields = adt::compute_fields(bcx.ccx, t, discr, false);
                fields.insert(0, d.to_ty(&bcx.tcx(), false));
                self.struct_field_ptr(bcx, &variants[discr], &fields, ix + 1, true)
            }
            layout::UntaggedUnion { ref variants } => {
                let fields = adt::compute_fields(bcx.ccx, t, 0, false);
                let ty = type_of::in_memory_type_of(bcx.ccx, fields[ix]);
                (bcx.pointercast(self.llval, ty.ptr_to()),
                 self.alignment | Alignment::from_packed(variants.packed))
            }
            layout::RawNullablePointer { nndiscr, .. } |
            layout::StructWrappedNullablePointer { nndiscr,  .. } if discr as u64 != nndiscr => {
                let nullfields = adt::compute_fields(bcx.ccx, t, (1-nndiscr) as usize, false);
                // The unit-like case might have a nonzero number of unit-like fields.
                // (e.d., Result of Either with (), as one side.)
                let ty = type_of::type_of(bcx.ccx, nullfields[ix]);
                assert_eq!(machine::llsize_of_alloc(bcx.ccx, ty), 0);
                (bcx.pointercast(self.llval, ty.ptr_to()), Alignment::Packed)
            }
            layout::RawNullablePointer { nndiscr, .. } => {
                let nnty = adt::compute_fields(bcx.ccx, t, nndiscr as usize, false)[0];
                assert_eq!(ix, 0);
                assert_eq!(discr as u64, nndiscr);
                let ty = type_of::type_of(bcx.ccx, nnty);
                (bcx.pointercast(self.llval, ty.ptr_to()), self.alignment)
            }
            layout::StructWrappedNullablePointer { ref nonnull, nndiscr, .. } => {
                assert_eq!(discr as u64, nndiscr);
                self.struct_field_ptr(bcx, &nonnull,
                     &adt::compute_fields(bcx.ccx, t, discr, false), ix, false)
            }
            _ => bug!("element access in type without elements: {} represented as {:#?}", t, l)
        }
    }

    pub fn project_index(&self, bcx: &Builder<'a, 'tcx>, llindex: ValueRef) -> ValueRef {
        if let ty::TySlice(_) = self.ty.to_ty(bcx.tcx()).sty {
            // Slices already point to the array element type.
            bcx.inbounds_gep(self.llval, &[llindex])
        } else {
            let zero = common::C_uint(bcx.ccx, 0u64);
            bcx.inbounds_gep(self.llval, &[zero, llindex])
        }
    }
}

impl<'a, 'tcx> MirContext<'a, 'tcx> {
    pub fn trans_lvalue(&mut self,
                        bcx: &Builder<'a, 'tcx>,
                        lvalue: &mir::Lvalue<'tcx>)
                        -> LvalueRef<'tcx> {
        debug!("trans_lvalue(lvalue={:?})", lvalue);

        let ccx = bcx.ccx;
        let tcx = ccx.tcx();

        if let mir::Lvalue::Local(index) = *lvalue {
            match self.locals[index] {
                LocalRef::Lvalue(lvalue) => {
                    return lvalue;
                }
                LocalRef::Operand(..) => {
                    bug!("using operand local {:?} as lvalue", lvalue);
                }
            }
        }

        let result = match *lvalue {
            mir::Lvalue::Local(_) => bug!(), // handled above
            mir::Lvalue::Static(box mir::Static { def_id, ty }) => {
                LvalueRef::new_sized(consts::get_static(ccx, def_id),
                                     LvalueTy::from_ty(self.monomorphize(&ty)),
                                     Alignment::AbiAligned)
            },
            mir::Lvalue::Projection(box mir::Projection {
                ref base,
                elem: mir::ProjectionElem::Deref
            }) => {
                // Load the pointer from its location.
                self.trans_consume(bcx, base).deref()
            }
            mir::Lvalue::Projection(ref projection) => {
                let tr_base = self.trans_lvalue(bcx, &projection.base);
                let projected_ty = tr_base.ty.projection_ty(tcx, &projection.elem);
                let projected_ty = self.monomorphize(&projected_ty);
                let align = tr_base.alignment;

                let ((llprojected, align), llextra) = match projection.elem {
                    mir::ProjectionElem::Deref => bug!(),
                    mir::ProjectionElem::Field(ref field, _) => {
                        let llextra = if self.ccx.shared().type_is_sized(projected_ty.to_ty(tcx)) {
                            ptr::null_mut()
                        } else {
                            tr_base.llextra
                        };
                        (tr_base.trans_field_ptr(bcx, field.index()), llextra)
                    }
                    mir::ProjectionElem::Index(index) => {
                        let index = &mir::Operand::Consume(mir::Lvalue::Local(index));
                        let index = self.trans_operand(bcx, index);
                        let llindex = self.prepare_index(bcx, index.immediate());
                        ((tr_base.project_index(bcx, llindex), align), ptr::null_mut())
                    }
                    mir::ProjectionElem::ConstantIndex { offset,
                                                         from_end: false,
                                                         min_length: _ } => {
                        let lloffset = C_uint(bcx.ccx, offset);
                        ((tr_base.project_index(bcx, lloffset), align), ptr::null_mut())
                    }
                    mir::ProjectionElem::ConstantIndex { offset,
                                                         from_end: true,
                                                         min_length: _ } => {
                        let lloffset = C_uint(bcx.ccx, offset);
                        let lllen = tr_base.len(bcx.ccx);
                        let llindex = bcx.sub(lllen, lloffset);
                        ((tr_base.project_index(bcx, llindex), align), ptr::null_mut())
                    }
                    mir::ProjectionElem::Subslice { from, to } => {
                        let llbase = tr_base.project_index(bcx, C_uint(bcx.ccx, from));

                        let base_ty = tr_base.ty.to_ty(bcx.tcx());
                        match base_ty.sty {
                            ty::TyArray(..) => {
                                // must cast the lvalue pointer type to the new
                                // array type (*[%_; new_len]).
                                let base_ty = self.monomorphized_lvalue_ty(lvalue);
                                let llbasety = type_of::type_of(bcx.ccx, base_ty).ptr_to();
                                let llbase = bcx.pointercast(llbase, llbasety);
                                ((llbase, align), ptr::null_mut())
                            }
                            ty::TySlice(..) => {
                                assert!(tr_base.llextra != ptr::null_mut());
                                let lllen = bcx.sub(tr_base.llextra,
                                                    C_uint(bcx.ccx, from+to));
                                ((llbase, align), lllen)
                            }
                            _ => bug!("unexpected type {:?} in Subslice", base_ty)
                        }
                    }
                    mir::ProjectionElem::Downcast(..) => {
                        ((tr_base.llval, align), tr_base.llextra)
                    }
                };
                LvalueRef {
                    llval: llprojected,
                    llextra,
                    ty: projected_ty,
                    alignment: align,
                }
            }
        };
        debug!("trans_lvalue(lvalue={:?}) => {:?}", lvalue, result);
        result
    }

    /// Adjust the bitwidth of an index since LLVM is less forgiving
    /// than we are.
    ///
    /// nmatsakis: is this still necessary? Not sure.
    fn prepare_index(&mut self, bcx: &Builder<'a, 'tcx>, llindex: ValueRef) -> ValueRef {
        let index_size = machine::llbitsize_of_real(bcx.ccx, common::val_ty(llindex));
        let int_size = machine::llbitsize_of_real(bcx.ccx, bcx.ccx.int_type());
        if index_size < int_size {
            bcx.zext(llindex, bcx.ccx.int_type())
        } else if index_size > int_size {
            bcx.trunc(llindex, bcx.ccx.int_type())
        } else {
            llindex
        }
    }

    pub fn monomorphized_lvalue_ty(&self, lvalue: &mir::Lvalue<'tcx>) -> Ty<'tcx> {
        let tcx = self.ccx.tcx();
        let lvalue_ty = lvalue.ty(self.mir, tcx);
        self.monomorphize(&lvalue_ty.to_ty(tcx))
    }
}
