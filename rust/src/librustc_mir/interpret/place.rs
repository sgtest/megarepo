// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Computations on places -- field projections, going from mir::Place, and writing
//! into a place.
//! All high-level functions to write to memory work on places as destinations.

use std::convert::TryFrom;

use rustc::mir;
use rustc::ty::{self, Ty};
use rustc::ty::layout::{self, Size, Align, LayoutOf, TyLayout, HasDataLayout};
use rustc_data_structures::indexed_vec::Idx;

use rustc::mir::interpret::{
    GlobalId, Scalar, EvalResult, Pointer, ScalarMaybeUndef
};
use super::{EvalContext, Machine, Value, ValTy, Operand, OpTy, MemoryKind};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct MemPlace {
    /// A place may have an integral pointer for ZSTs, and since it might
    /// be turned back into a reference before ever being dereferenced.
    /// However, it may never be undef.
    pub ptr: Scalar,
    pub align: Align,
    /// Metadata for unsized places.  Interpretation is up to the type.
    /// Must not be present for sized types, but can be missing for unsized types
    /// (e.g. `extern type`).
    pub extra: Option<Scalar>,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum Place {
    /// A place referring to a value allocated in the `Memory` system.
    Ptr(MemPlace),

    /// To support alloc-free locals, we are able to write directly to a local.
    /// (Without that optimization, we'd just always be a `MemPlace`.)
    Local {
        frame: usize,
        local: mir::Local,
    },
}

#[derive(Copy, Clone, Debug)]
pub struct PlaceTy<'tcx> {
    place: Place,
    pub layout: TyLayout<'tcx>,
}

impl<'tcx> ::std::ops::Deref for PlaceTy<'tcx> {
    type Target = Place;
    #[inline(always)]
    fn deref(&self) -> &Place {
        &self.place
    }
}

/// A MemPlace with its layout. Constructing it is only possible in this module.
#[derive(Copy, Clone, Debug)]
pub struct MPlaceTy<'tcx> {
    mplace: MemPlace,
    pub layout: TyLayout<'tcx>,
}

impl<'tcx> ::std::ops::Deref for MPlaceTy<'tcx> {
    type Target = MemPlace;
    #[inline(always)]
    fn deref(&self) -> &MemPlace {
        &self.mplace
    }
}

impl<'tcx> From<MPlaceTy<'tcx>> for PlaceTy<'tcx> {
    #[inline(always)]
    fn from(mplace: MPlaceTy<'tcx>) -> Self {
        PlaceTy {
            place: Place::Ptr(mplace.mplace),
            layout: mplace.layout
        }
    }
}

impl MemPlace {
    #[inline(always)]
    pub fn from_scalar_ptr(ptr: Scalar, align: Align) -> Self {
        MemPlace {
            ptr,
            align,
            extra: None,
        }
    }

    #[inline(always)]
    pub fn from_ptr(ptr: Pointer, align: Align) -> Self {
        Self::from_scalar_ptr(ptr.into(), align)
    }

    #[inline(always)]
    pub fn to_scalar_ptr_align(self) -> (Scalar, Align) {
        assert_eq!(self.extra, None);
        (self.ptr, self.align)
    }

    /// Extract the ptr part of the mplace
    #[inline(always)]
    pub fn to_ptr(self) -> EvalResult<'tcx, Pointer> {
        // At this point, we forget about the alignment information --
        // the place has been turned into a reference, and no matter where it came from,
        // it now must be aligned.
        self.to_scalar_ptr_align().0.to_ptr()
    }

    /// Turn a mplace into a (thin or fat) pointer, as a reference, pointing to the same space.
    /// This is the inverse of `ref_to_mplace`.
    pub fn to_ref(self) -> Value {
        // We ignore the alignment of the place here -- special handling for packed structs ends
        // at the `&` operator.
        match self.extra {
            None => Value::Scalar(self.ptr.into()),
            Some(extra) => Value::ScalarPair(self.ptr.into(), extra.into()),
        }
    }
}

impl<'tcx> MPlaceTy<'tcx> {
    #[inline]
    fn from_aligned_ptr(ptr: Pointer, layout: TyLayout<'tcx>) -> Self {
        MPlaceTy { mplace: MemPlace::from_ptr(ptr, layout.align), layout }
    }

    #[inline]
    pub(super) fn len(self, cx: impl HasDataLayout) -> EvalResult<'tcx, u64> {
        if self.layout.is_unsized() {
            // We need to consult `extra` metadata
            match self.layout.ty.sty {
                ty::Slice(..) | ty::Str =>
                    return self.extra.unwrap().to_usize(cx),
                _ => bug!("len not supported on unsized type {:?}", self.layout.ty),
            }
        } else {
            // Go through the layout.  There are lots of types that support a length,
            // e.g. SIMD types.
            match self.layout.fields {
                layout::FieldPlacement::Array { count, .. } => Ok(count),
                _ => bug!("len not supported on sized type {:?}", self.layout.ty),
            }
        }
    }

    #[inline]
    pub(super) fn vtable(self) -> EvalResult<'tcx, Pointer> {
        match self.layout.ty.sty {
            ty::Dynamic(..) => self.extra.unwrap().to_ptr(),
            _ => bug!("vtable not supported on type {:?}", self.layout.ty),
        }
    }
}

impl<'tcx> OpTy<'tcx> {
    #[inline(always)]
    pub fn try_as_mplace(self) -> Result<MPlaceTy<'tcx>, Value> {
        match *self {
            Operand::Indirect(mplace) => Ok(MPlaceTy { mplace, layout: self.layout }),
            Operand::Immediate(value) => Err(value),
        }
    }

    #[inline(always)]
    pub fn to_mem_place(self) -> MPlaceTy<'tcx> {
        self.try_as_mplace().unwrap()
    }
}

impl<'tcx> Place {
    /// Produces a Place that will error if attempted to be read from or written to
    #[inline]
    pub fn null(cx: impl HasDataLayout) -> Self {
        Self::from_scalar_ptr(Scalar::ptr_null(cx), Align::from_bytes(1, 1).unwrap())
    }

    #[inline]
    pub fn from_scalar_ptr(ptr: Scalar, align: Align) -> Self {
        Place::Ptr(MemPlace::from_scalar_ptr(ptr, align))
    }

    #[inline]
    pub fn from_ptr(ptr: Pointer, align: Align) -> Self {
        Place::Ptr(MemPlace::from_ptr(ptr, align))
    }

    #[inline]
    pub fn to_mem_place(self) -> MemPlace {
        match self {
            Place::Ptr(mplace) => mplace,
            _ => bug!("to_mem_place: expected Place::Ptr, got {:?}", self),

        }
    }

    #[inline]
    pub fn to_scalar_ptr_align(self) -> (Scalar, Align) {
        self.to_mem_place().to_scalar_ptr_align()
    }

    #[inline]
    pub fn to_ptr(self) -> EvalResult<'tcx, Pointer> {
        self.to_mem_place().to_ptr()
    }
}

impl<'tcx> PlaceTy<'tcx> {
    /// Produces a Place that will error if attempted to be read from or written to
    #[inline]
    pub fn null(cx: impl HasDataLayout, layout: TyLayout<'tcx>) -> Self {
        PlaceTy { place: Place::from_scalar_ptr(Scalar::ptr_null(cx), layout.align), layout }
    }

    #[inline]
    pub fn to_mem_place(self) -> MPlaceTy<'tcx> {
        MPlaceTy { mplace: self.place.to_mem_place(), layout: self.layout }
    }
}

impl<'a, 'mir, 'tcx, M: Machine<'mir, 'tcx>> EvalContext<'a, 'mir, 'tcx, M> {
    /// Take a value, which represents a (thin or fat) reference, and make it a place.
    /// Alignment is just based on the type.  This is the inverse of `MemPlace::to_ref`.
    pub fn ref_to_mplace(
        &self, val: ValTy<'tcx>
    ) -> EvalResult<'tcx, MPlaceTy<'tcx>> {
        let pointee_type = val.layout.ty.builtin_deref(true).unwrap().ty;
        let layout = self.layout_of(pointee_type)?;
        let align = layout.align;
        let mplace = match *val {
            Value::Scalar(ptr) =>
                MemPlace { ptr: ptr.not_undef()?, align, extra: None },
            Value::ScalarPair(ptr, extra) =>
                MemPlace { ptr: ptr.not_undef()?, align, extra: Some(extra.not_undef()?) },
        };
        Ok(MPlaceTy { mplace, layout })
    }

    /// Offset a pointer to project to a field. Unlike place_field, this is always
    /// possible without allocating, so it can take &self. Also return the field's layout.
    /// This supports both struct and array fields.
    #[inline(always)]
    pub fn mplace_field(
        &self,
        base: MPlaceTy<'tcx>,
        field: u64,
    ) -> EvalResult<'tcx, MPlaceTy<'tcx>> {
        // Not using the layout method because we want to compute on u64
        let offset = match base.layout.fields {
            layout::FieldPlacement::Arbitrary { ref offsets, .. } =>
                offsets[usize::try_from(field).unwrap()],
            layout::FieldPlacement::Array { stride, .. } => {
                let len = base.len(self)?;
                assert!(field < len, "Tried to access element {} of array/slice with length {}",
                    field, len);
                stride * field
            }
            layout::FieldPlacement::Union(count) => {
                assert!(field < count as u64,
                        "Tried to access field {} of union with {} fields", field, count);
                // Offset is always 0
                Size::from_bytes(0)
            }
        };
        // the only way conversion can fail if is this is an array (otherwise we already panicked
        // above). In that case, all fields are equal.
        let field_layout = base.layout.field(self, usize::try_from(field).unwrap_or(0))?;

        // Offset may need adjustment for unsized fields
        let (extra, offset) = if field_layout.is_unsized() {
            // re-use parent metadata to determine dynamic field layout
            let (_, align) = self.size_and_align_of(base.extra, field_layout)?;
            (base.extra, offset.abi_align(align))

        } else {
            // base.extra could be present; we might be accessing a sized field of an unsized
            // struct.
            (None, offset)
        };

        let ptr = base.ptr.ptr_offset(offset, self)?;
        let align = base.align.min(field_layout.align); // only use static information

        Ok(MPlaceTy { mplace: MemPlace { ptr, align, extra }, layout: field_layout })
    }

    // Iterates over all fields of an array. Much more efficient than doing the
    // same by repeatedly calling `mplace_array`.
    pub fn mplace_array_fields(
        &self,
        base: MPlaceTy<'tcx>,
    ) -> EvalResult<'tcx, impl Iterator<Item=EvalResult<'tcx, MPlaceTy<'tcx>>> + 'a> {
        let len = base.len(self)?; // also asserts that we have a type where this makes sense
        let stride = match base.layout.fields {
            layout::FieldPlacement::Array { stride, .. } => stride,
            _ => bug!("mplace_array_fields: expected an array layout"),
        };
        let layout = base.layout.field(self, 0)?;
        let dl = &self.tcx.data_layout;
        Ok((0..len).map(move |i| {
            let ptr = base.ptr.ptr_offset(i * stride, dl)?;
            Ok(MPlaceTy {
                mplace: MemPlace { ptr, align: base.align, extra: None },
                layout
            })
        }))
    }

    pub fn mplace_subslice(
        &self,
        base: MPlaceTy<'tcx>,
        from: u64,
        to: u64,
    ) -> EvalResult<'tcx, MPlaceTy<'tcx>> {
        let len = base.len(self)?; // also asserts that we have a type where this makes sense
        assert!(from <= len - to);

        // Not using layout method because that works with usize, and does not work with slices
        // (that have count 0 in their layout).
        let from_offset = match base.layout.fields {
            layout::FieldPlacement::Array { stride, .. } =>
                stride * from,
            _ => bug!("Unexpected layout of index access: {:#?}", base.layout),
        };
        let ptr = base.ptr.ptr_offset(from_offset, self)?;

        // Compute extra and new layout
        let inner_len = len - to - from;
        let (extra, ty) = match base.layout.ty.sty {
            // It is not nice to match on the type, but that seems to be the only way to
            // implement this.
            ty::Array(inner, _) =>
                (None, self.tcx.mk_array(inner, inner_len)),
            ty::Slice(..) => {
                let len = Scalar::Bits {
                    bits: inner_len.into(),
                    size: self.memory.pointer_size().bytes() as u8
                };
                (Some(len), base.layout.ty)
            }
            _ =>
                bug!("cannot subslice non-array type: `{:?}`", base.layout.ty),
        };
        let layout = self.layout_of(ty)?;

        Ok(MPlaceTy {
            mplace: MemPlace { ptr, align: base.align, extra },
            layout
        })
    }

    pub fn mplace_downcast(
        &self,
        base: MPlaceTy<'tcx>,
        variant: usize,
    ) -> EvalResult<'tcx, MPlaceTy<'tcx>> {
        // Downcasts only change the layout
        assert_eq!(base.extra, None);
        Ok(MPlaceTy { layout: base.layout.for_variant(self, variant), ..base })
    }

    /// Project into an mplace
    pub fn mplace_projection(
        &self,
        base: MPlaceTy<'tcx>,
        proj_elem: &mir::PlaceElem<'tcx>,
    ) -> EvalResult<'tcx, MPlaceTy<'tcx>> {
        use rustc::mir::ProjectionElem::*;
        Ok(match *proj_elem {
            Field(field, _) => self.mplace_field(base, field.index() as u64)?,
            Downcast(_, variant) => self.mplace_downcast(base, variant)?,
            Deref => self.deref_operand(base.into())?,

            Index(local) => {
                let n = *self.frame().locals[local].access()?;
                let n_layout = self.layout_of(self.tcx.types.usize)?;
                let n = self.read_scalar(OpTy { op: n, layout: n_layout })?;
                let n = n.to_bits(self.tcx.data_layout.pointer_size)?;
                self.mplace_field(base, u64::try_from(n).unwrap())?
            }

            ConstantIndex {
                offset,
                min_length,
                from_end,
            } => {
                let n = base.len(self)?;
                assert!(n >= min_length as u64);

                let index = if from_end {
                    n - u64::from(offset)
                } else {
                    u64::from(offset)
                };

                self.mplace_field(base, index)?
            }

            Subslice { from, to } =>
                self.mplace_subslice(base, u64::from(from), u64::from(to))?,
        })
    }

    /// Get the place of a field inside the place, and also the field's type.
    /// Just a convenience function, but used quite a bit.
    pub fn place_field(
        &mut self,
        base: PlaceTy<'tcx>,
        field: u64,
    ) -> EvalResult<'tcx, PlaceTy<'tcx>> {
        // FIXME: We could try to be smarter and avoid allocation for fields that span the
        // entire place.
        let mplace = self.force_allocation(base)?;
        Ok(self.mplace_field(mplace, field)?.into())
    }

    pub fn place_downcast(
        &mut self,
        base: PlaceTy<'tcx>,
        variant: usize,
    ) -> EvalResult<'tcx, PlaceTy<'tcx>> {
        // Downcast just changes the layout
        Ok(match base.place {
            Place::Ptr(mplace) =>
                self.mplace_downcast(MPlaceTy { mplace, layout: base.layout }, variant)?.into(),
            Place::Local { .. } => {
                let layout = base.layout.for_variant(&self, variant);
                PlaceTy { layout, ..base }
            }
        })
    }

    /// Project into a place
    pub fn place_projection(
        &mut self,
        base: PlaceTy<'tcx>,
        proj_elem: &mir::ProjectionElem<'tcx, mir::Local, Ty<'tcx>>,
    ) -> EvalResult<'tcx, PlaceTy<'tcx>> {
        use rustc::mir::ProjectionElem::*;
        Ok(match *proj_elem {
            Field(field, _) =>  self.place_field(base, field.index() as u64)?,
            Downcast(_, variant) => self.place_downcast(base, variant)?,
            Deref => self.deref_operand(self.place_to_op(base)?)?.into(),
            // For the other variants, we have to force an allocation.
            // This matches `operand_projection`.
            Subslice { .. } | ConstantIndex { .. } | Index(_) => {
                let mplace = self.force_allocation(base)?;
                self.mplace_projection(mplace, proj_elem)?.into()
            }
        })
    }

    /// Evaluate statics and promoteds to an `MPlace`.  Used to share some code between
    /// `eval_place` and `eval_place_to_op`.
    pub(super) fn eval_place_to_mplace(
        &self,
        mir_place: &mir::Place<'tcx>
    ) -> EvalResult<'tcx, MPlaceTy<'tcx>> {
        use rustc::mir::Place::*;
        Ok(match *mir_place {
            Promoted(ref promoted) => {
                let instance = self.frame().instance;
                let op = self.global_to_op(GlobalId {
                    instance,
                    promoted: Some(promoted.0),
                })?;
                let mplace = op.to_mem_place(); // these are always in memory
                let ty = self.monomorphize(promoted.1, self.substs());
                MPlaceTy {
                    mplace,
                    layout: self.layout_of(ty)?,
                }
            }

            Static(ref static_) => {
                let ty = self.monomorphize(static_.ty, self.substs());
                let layout = self.layout_of(ty)?;
                let instance = ty::Instance::mono(*self.tcx, static_.def_id);
                let cid = GlobalId {
                    instance,
                    promoted: None
                };
                // Just create a lazy reference, so we can support recursive statics.
                // tcx takes are of assigning every static one and only one unique AllocId.
                // When the data here is ever actually used, memory will notice,
                // and it knows how to deal with alloc_id that are present in the
                // global table but not in its local memory: It calls back into tcx through
                // a query, triggering the CTFE machinery to actually turn this lazy reference
                // into a bunch of bytes.  IOW, statics are evaluated with CTFE even when
                // this EvalContext uses another Machine (e.g., in miri).  This is what we
                // want!  This way, computing statics works concistently between codegen
                // and miri: They use the same query to eventually obtain a `ty::Const`
                // and use that for further computation.
                let alloc = self.tcx.alloc_map.lock().intern_static(cid.instance.def_id());
                MPlaceTy::from_aligned_ptr(alloc.into(), layout)
            }

            _ => bug!("eval_place_to_mplace called on {:?}", mir_place),
        })
    }

    /// Compute a place.  You should only use this if you intend to write into this
    /// place; for reading, a more efficient alternative is `eval_place_for_read`.
    pub fn eval_place(&mut self, mir_place: &mir::Place<'tcx>) -> EvalResult<'tcx, PlaceTy<'tcx>> {
        use rustc::mir::Place::*;
        let place = match *mir_place {
            Local(mir::RETURN_PLACE) => PlaceTy {
                place: self.frame().return_place,
                layout: self.layout_of_local(self.cur_frame(), mir::RETURN_PLACE)?,
            },
            Local(local) => PlaceTy {
                place: Place::Local {
                    frame: self.cur_frame(),
                    local,
                },
                layout: self.layout_of_local(self.cur_frame(), local)?,
            },

            Projection(ref proj) => {
                let place = self.eval_place(&proj.base)?;
                self.place_projection(place, &proj.elem)?
            }

            _ => self.eval_place_to_mplace(mir_place)?.into(),
        };

        self.dump_place(place.place);
        Ok(place)
    }

    /// Write a scalar to a place
    pub fn write_scalar(
        &mut self,
        val: impl Into<ScalarMaybeUndef>,
        dest: PlaceTy<'tcx>,
    ) -> EvalResult<'tcx> {
        self.write_value(Value::Scalar(val.into()), dest)
    }

    /// Write a value to a place
    pub fn write_value(
        &mut self,
        src_val: Value,
        dest: PlaceTy<'tcx>,
    ) -> EvalResult<'tcx> {
        trace!("write_value: {:?} <- {:?}", *dest, src_val);
        // See if we can avoid an allocation. This is the counterpart to `try_read_value`,
        // but not factored as a separate function.
        let mplace = match dest.place {
            Place::Local { frame, local } => {
                match *self.stack[frame].locals[local].access_mut()? {
                    Operand::Immediate(ref mut dest_val) => {
                        // Yay, we can just change the local directly.
                        *dest_val = src_val;
                        return Ok(());
                    },
                    Operand::Indirect(mplace) => mplace, // already in memory
                }
            },
            Place::Ptr(mplace) => mplace, // already in memory
        };

        // This is already in memory, write there.
        let dest = MPlaceTy { mplace, layout: dest.layout };
        self.write_value_to_mplace(src_val, dest)
    }

    /// Write a value to memory
    fn write_value_to_mplace(
        &mut self,
        value: Value,
        dest: MPlaceTy<'tcx>,
    ) -> EvalResult<'tcx> {
        let (ptr, ptr_align) = dest.to_scalar_ptr_align();
        // Note that it is really important that the type here is the right one, and matches the
        // type things are read at. In case `src_val` is a `ScalarPair`, we don't do any magic here
        // to handle padding properly, which is only correct if we never look at this data with the
        // wrong type.

        // Nothing to do for ZSTs, other than checking alignment
        if dest.layout.size.bytes() == 0 {
            self.memory.check_align(ptr, ptr_align)?;
            return Ok(());
        }

        let ptr = ptr.to_ptr()?;
        match value {
            Value::Scalar(scalar) => {
                self.memory.write_scalar(
                    ptr, ptr_align.min(dest.layout.align), scalar, dest.layout.size
                )
            }
            Value::ScalarPair(a_val, b_val) => {
                let (a, b) = match dest.layout.abi {
                    layout::Abi::ScalarPair(ref a, ref b) => (&a.value, &b.value),
                    _ => bug!("write_value_to_mplace: invalid ScalarPair layout: {:#?}",
                              dest.layout)
                };
                let (a_size, b_size) = (a.size(&self), b.size(&self));
                let (a_align, b_align) = (a.align(&self), b.align(&self));
                let b_offset = a_size.abi_align(b_align);
                let b_ptr = ptr.offset(b_offset, &self)?.into();

                self.memory.write_scalar(ptr, ptr_align.min(a_align), a_val, a_size)?;
                self.memory.write_scalar(b_ptr, ptr_align.min(b_align), b_val, b_size)
            }
        }
    }

    /// Copy the data from an operand to a place
    pub fn copy_op(
        &mut self,
        src: OpTy<'tcx>,
        dest: PlaceTy<'tcx>,
    ) -> EvalResult<'tcx> {
        assert_eq!(src.layout.size, dest.layout.size,
            "Size mismatch when copying!\nsrc: {:#?}\ndest: {:#?}", src, dest);

        // Let us see if the layout is simple so we take a shortcut, avoid force_allocation.
        let (src_ptr, src_align) = match self.try_read_value(src)? {
            Ok(src_val) =>
                // Yay, we got a value that we can write directly.  We write with the
                // *source layout*, because that was used to load, and if they do not match
                // this is a transmute we want to support.
                return self.write_value(src_val, PlaceTy { place: *dest, layout: src.layout }),
            Err(mplace) => mplace.to_scalar_ptr_align(),
        };
        // Slow path, this does not fit into an immediate. Just memcpy.
        trace!("copy_op: {:?} <- {:?}", *dest, *src);
        let (dest_ptr, dest_align) = self.force_allocation(dest)?.to_scalar_ptr_align();
        self.memory.copy(
            src_ptr, src_align,
            dest_ptr, dest_align,
            src.layout.size, false
        )
    }

    /// Make sure that a place is in memory, and return where it is.
    /// This is essentially `force_to_memplace`.
    pub fn force_allocation(
        &mut self,
        place: PlaceTy<'tcx>,
    ) -> EvalResult<'tcx, MPlaceTy<'tcx>> {
        let mplace = match place.place {
            Place::Local { frame, local } => {
                match *self.stack[frame].locals[local].access()? {
                    Operand::Indirect(mplace) => mplace,
                    Operand::Immediate(value) => {
                        // We need to make an allocation.
                        // FIXME: Consider not doing anything for a ZST, and just returning
                        // a fake pointer?  Are we even called for ZST?

                        // We need the layout of the local.  We can NOT use the layout we got,
                        // that might e.g. be an inner field of a struct with `Scalar` layout,
                        // that has different alignment than the outer field.
                        let local_layout = self.layout_of_local(frame, local)?;
                        let ptr = self.allocate(local_layout, MemoryKind::Stack)?;
                        self.write_value_to_mplace(value, ptr)?;
                        let mplace = ptr.mplace;
                        // Update the local
                        *self.stack[frame].locals[local].access_mut()? =
                            Operand::Indirect(mplace);
                        mplace
                    }
                }
            }
            Place::Ptr(mplace) => mplace
        };
        // Return with the original layout, so that the caller can go on
        Ok(MPlaceTy { mplace, layout: place.layout })
    }

    pub fn allocate(
        &mut self,
        layout: TyLayout<'tcx>,
        kind: MemoryKind<M::MemoryKinds>,
    ) -> EvalResult<'tcx, MPlaceTy<'tcx>> {
        assert!(!layout.is_unsized(), "cannot alloc memory for unsized type");
        let ptr = self.memory.allocate(layout.size, layout.align, kind)?;
        Ok(MPlaceTy::from_aligned_ptr(ptr, layout))
    }

    pub fn write_discriminant_index(
        &mut self,
        variant_index: usize,
        dest: PlaceTy<'tcx>,
    ) -> EvalResult<'tcx> {
        match dest.layout.variants {
            layout::Variants::Single { index } => {
                assert_eq!(index, variant_index);
            }
            layout::Variants::Tagged { ref tag, .. } => {
                let adt_def = dest.layout.ty.ty_adt_def().unwrap();
                assert!(variant_index < adt_def.variants.len());
                let discr_val = adt_def
                    .discriminant_for_variant(*self.tcx, variant_index)
                    .val;

                // raw discriminants for enums are isize or bigger during
                // their computation, but the in-memory tag is the smallest possible
                // representation
                let size = tag.value.size(self.tcx.tcx);
                let shift = 128 - size.bits();
                let discr_val = (discr_val << shift) >> shift;

                let discr_dest = self.place_field(dest, 0)?;
                self.write_scalar(Scalar::Bits {
                    bits: discr_val,
                    size: size.bytes() as u8,
                }, discr_dest)?;
            }
            layout::Variants::NicheFilling {
                dataful_variant,
                ref niche_variants,
                niche_start,
                ..
            } => {
                assert!(variant_index < dest.layout.ty.ty_adt_def().unwrap().variants.len());
                if variant_index != dataful_variant {
                    let niche_dest =
                        self.place_field(dest, 0)?;
                    let niche_value = ((variant_index - niche_variants.start()) as u128)
                        .wrapping_add(niche_start);
                    self.write_scalar(Scalar::Bits {
                        bits: niche_value,
                        size: niche_dest.layout.size.bytes() as u8,
                    }, niche_dest)?;
                }
            }
        }

        Ok(())
    }

    /// Every place can be read from, so we can turm them into an operand
    #[inline(always)]
    pub fn place_to_op(&self, place: PlaceTy<'tcx>) -> EvalResult<'tcx, OpTy<'tcx>> {
        let op = match place.place {
            Place::Ptr(mplace) => {
                Operand::Indirect(mplace)
            }
            Place::Local { frame, local } =>
                *self.stack[frame].locals[local].access()?
        };
        Ok(OpTy { op, layout: place.layout })
    }

    /// Turn a place with a `dyn Trait` type into a place with the actual dynamic type.
    /// Also return some more information so drop doesn't have to run the same code twice.
    pub(super) fn unpack_dyn_trait(&self, mplace: MPlaceTy<'tcx>)
    -> EvalResult<'tcx, (ty::Instance<'tcx>, MPlaceTy<'tcx>)> {
        let vtable = mplace.vtable()?; // also sanity checks the type
        let (instance, ty) = self.read_drop_type_from_vtable(vtable)?;
        let layout = self.layout_of(ty)?;

        // More sanity checks
        let (size, align) = self.read_size_and_align_from_vtable(vtable)?;
        assert_eq!(size, layout.size);
        assert_eq!(align.abi(), layout.align.abi()); // only ABI alignment is preserved
        // FIXME: More checks for the vtable? We could make sure it is exactly
        // the one one would expect for this type.

        let mplace = MPlaceTy {
            mplace: MemPlace { extra: None, ..*mplace },
            layout
        };
        Ok((instance, mplace))
    }
}
