// Copyright 2018 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Functions concerning immediate values and operands, and reading from operands.
//! All high-level functions to read from memory work on operands as sources.

use std::convert::TryInto;

use rustc::{mir, ty};
use rustc::ty::layout::{self, Size, LayoutOf, TyLayout, HasDataLayout, IntegerExt};

use rustc::mir::interpret::{
    GlobalId, AllocId,
    ConstValue, Pointer, Scalar,
    EvalResult, EvalErrorKind
};
use super::{EvalContext, Machine, MemPlace, MPlaceTy, MemoryKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, RustcEncodable, RustcDecodable, Hash)]
pub enum ScalarMaybeUndef<Tag=(), Id=AllocId> {
    Scalar(Scalar<Tag, Id>),
    Undef,
}

impl<Tag> From<Scalar<Tag>> for ScalarMaybeUndef<Tag> {
    #[inline(always)]
    fn from(s: Scalar<Tag>) -> Self {
        ScalarMaybeUndef::Scalar(s)
    }
}

impl<'tcx> ScalarMaybeUndef<()> {
    #[inline]
    pub fn with_default_tag<Tag>(self) -> ScalarMaybeUndef<Tag>
        where Tag: Default
    {
        match self {
            ScalarMaybeUndef::Scalar(s) => ScalarMaybeUndef::Scalar(s.with_default_tag()),
            ScalarMaybeUndef::Undef => ScalarMaybeUndef::Undef,
        }
    }
}

impl<'tcx, Tag> ScalarMaybeUndef<Tag> {
    #[inline]
    pub fn erase_tag(self) -> ScalarMaybeUndef
    {
        match self {
            ScalarMaybeUndef::Scalar(s) => ScalarMaybeUndef::Scalar(s.erase_tag()),
            ScalarMaybeUndef::Undef => ScalarMaybeUndef::Undef,
        }
    }

    #[inline]
    pub fn not_undef(self) -> EvalResult<'static, Scalar<Tag>> {
        match self {
            ScalarMaybeUndef::Scalar(scalar) => Ok(scalar),
            ScalarMaybeUndef::Undef => err!(ReadUndefBytes(Size::from_bytes(0))),
        }
    }

    #[inline(always)]
    pub fn to_ptr(self) -> EvalResult<'tcx, Pointer<Tag>> {
        self.not_undef()?.to_ptr()
    }

    #[inline(always)]
    pub fn to_bits(self, target_size: Size) -> EvalResult<'tcx, u128> {
        self.not_undef()?.to_bits(target_size)
    }

    #[inline(always)]
    pub fn to_bool(self) -> EvalResult<'tcx, bool> {
        self.not_undef()?.to_bool()
    }

    #[inline(always)]
    pub fn to_char(self) -> EvalResult<'tcx, char> {
        self.not_undef()?.to_char()
    }

    #[inline(always)]
    pub fn to_f32(self) -> EvalResult<'tcx, f32> {
        self.not_undef()?.to_f32()
    }

    #[inline(always)]
    pub fn to_f64(self) -> EvalResult<'tcx, f64> {
        self.not_undef()?.to_f64()
    }

    #[inline(always)]
    pub fn to_u8(self) -> EvalResult<'tcx, u8> {
        self.not_undef()?.to_u8()
    }

    #[inline(always)]
    pub fn to_u32(self) -> EvalResult<'tcx, u32> {
        self.not_undef()?.to_u32()
    }

    #[inline(always)]
    pub fn to_u64(self) -> EvalResult<'tcx, u64> {
        self.not_undef()?.to_u64()
    }

    #[inline(always)]
    pub fn to_usize(self, cx: impl HasDataLayout) -> EvalResult<'tcx, u64> {
        self.not_undef()?.to_usize(cx)
    }

    #[inline(always)]
    pub fn to_i8(self) -> EvalResult<'tcx, i8> {
        self.not_undef()?.to_i8()
    }

    #[inline(always)]
    pub fn to_i32(self) -> EvalResult<'tcx, i32> {
        self.not_undef()?.to_i32()
    }

    #[inline(always)]
    pub fn to_i64(self) -> EvalResult<'tcx, i64> {
        self.not_undef()?.to_i64()
    }

    #[inline(always)]
    pub fn to_isize(self, cx: impl HasDataLayout) -> EvalResult<'tcx, i64> {
        self.not_undef()?.to_isize(cx)
    }
}


/// A `Value` represents a single immediate self-contained Rust value.
///
/// For optimization of a few very common cases, there is also a representation for a pair of
/// primitive values (`ScalarPair`). It allows Miri to avoid making allocations for checked binary
/// operations and fat pointers. This idea was taken from rustc's codegen.
/// In particular, thanks to `ScalarPair`, arithmetic operations and casts can be entirely
/// defined on `Value`, and do not have to work with a `Place`.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum Value<Tag=(), Id=AllocId> {
    Scalar(ScalarMaybeUndef<Tag, Id>),
    ScalarPair(ScalarMaybeUndef<Tag, Id>, ScalarMaybeUndef<Tag, Id>),
}

impl Value {
    #[inline]
    pub fn with_default_tag<Tag>(self) -> Value<Tag>
        where Tag: Default
    {
        match self {
            Value::Scalar(x) => Value::Scalar(x.with_default_tag()),
            Value::ScalarPair(x, y) =>
                Value::ScalarPair(x.with_default_tag(), y.with_default_tag()),
        }
    }
}

impl<'tcx, Tag> Value<Tag> {
    #[inline]
    pub fn erase_tag(self) -> Value
    {
        match self {
            Value::Scalar(x) => Value::Scalar(x.erase_tag()),
            Value::ScalarPair(x, y) =>
                Value::ScalarPair(x.erase_tag(), y.erase_tag()),
        }
    }

    pub fn new_slice(
        val: Scalar<Tag>,
        len: u64,
        cx: impl HasDataLayout
    ) -> Self {
        Value::ScalarPair(val.into(), Scalar::from_uint(len, cx.data_layout().pointer_size).into())
    }

    pub fn new_dyn_trait(val: Scalar<Tag>, vtable: Pointer<Tag>) -> Self {
        Value::ScalarPair(val.into(), Scalar::Ptr(vtable).into())
    }

    #[inline]
    pub fn to_scalar_or_undef(self) -> ScalarMaybeUndef<Tag> {
        match self {
            Value::Scalar(val) => val,
            Value::ScalarPair(..) => bug!("Got a fat pointer where a scalar was expected"),
        }
    }

    #[inline]
    pub fn to_scalar(self) -> EvalResult<'tcx, Scalar<Tag>> {
        self.to_scalar_or_undef().not_undef()
    }

    #[inline]
    pub fn to_scalar_pair(self) -> EvalResult<'tcx, (Scalar<Tag>, Scalar<Tag>)> {
        match self {
            Value::Scalar(..) => bug!("Got a thin pointer where a scalar pair was expected"),
            Value::ScalarPair(a, b) => Ok((a.not_undef()?, b.not_undef()?))
        }
    }

    /// Convert the value into a pointer (or a pointer-sized integer).
    /// Throws away the second half of a ScalarPair!
    #[inline]
    pub fn to_scalar_ptr(self) -> EvalResult<'tcx, Scalar<Tag>> {
        match self {
            Value::Scalar(ptr) |
            Value::ScalarPair(ptr, _) => ptr.not_undef(),
        }
    }

    /// Convert the value into its metadata.
    /// Throws away the first half of a ScalarPair!
    #[inline]
    pub fn to_meta(self) -> EvalResult<'tcx, Option<Scalar<Tag>>> {
        Ok(match self {
            Value::Scalar(_) => None,
            Value::ScalarPair(_, meta) => Some(meta.not_undef()?),
        })
    }
}

// ScalarPair needs a type to interpret, so we often have a value and a type together
// as input for binary and cast operations.
#[derive(Copy, Clone, Debug)]
pub struct ValTy<'tcx, Tag=()> {
    value: Value<Tag>,
    pub layout: TyLayout<'tcx>,
}

impl<'tcx, Tag> ::std::ops::Deref for ValTy<'tcx, Tag> {
    type Target = Value<Tag>;
    #[inline(always)]
    fn deref(&self) -> &Value<Tag> {
        &self.value
    }
}

/// An `Operand` is the result of computing a `mir::Operand`. It can be immediate,
/// or still in memory.  The latter is an optimization, to delay reading that chunk of
/// memory and to avoid having to store arbitrary-sized data here.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum Operand<Tag=(), Id=AllocId> {
    Immediate(Value<Tag, Id>),
    Indirect(MemPlace<Tag, Id>),
}

impl Operand {
    #[inline]
    pub fn with_default_tag<Tag>(self) -> Operand<Tag>
        where Tag: Default
    {
        match self {
            Operand::Immediate(x) => Operand::Immediate(x.with_default_tag()),
            Operand::Indirect(x) => Operand::Indirect(x.with_default_tag()),
        }
    }
}

impl<Tag> Operand<Tag> {
    #[inline]
    pub fn erase_tag(self) -> Operand
    {
        match self {
            Operand::Immediate(x) => Operand::Immediate(x.erase_tag()),
            Operand::Indirect(x) => Operand::Indirect(x.erase_tag()),
        }
    }

    #[inline]
    pub fn to_mem_place(self) -> MemPlace<Tag>
        where Tag: ::std::fmt::Debug
    {
        match self {
            Operand::Indirect(mplace) => mplace,
            _ => bug!("to_mem_place: expected Operand::Indirect, got {:?}", self),

        }
    }

    #[inline]
    pub fn to_immediate(self) -> Value<Tag>
        where Tag: ::std::fmt::Debug
    {
        match self {
            Operand::Immediate(val) => val,
            _ => bug!("to_immediate: expected Operand::Immediate, got {:?}", self),

        }
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub struct OpTy<'tcx, Tag=()> {
    crate op: Operand<Tag>, // ideally we'd make this private, but const_prop needs this
    pub layout: TyLayout<'tcx>,
}

impl<'tcx, Tag> ::std::ops::Deref for OpTy<'tcx, Tag> {
    type Target = Operand<Tag>;
    #[inline(always)]
    fn deref(&self) -> &Operand<Tag> {
        &self.op
    }
}

impl<'tcx, Tag: Copy> From<MPlaceTy<'tcx, Tag>> for OpTy<'tcx, Tag> {
    #[inline(always)]
    fn from(mplace: MPlaceTy<'tcx, Tag>) -> Self {
        OpTy {
            op: Operand::Indirect(*mplace),
            layout: mplace.layout
        }
    }
}

impl<'tcx, Tag> From<ValTy<'tcx, Tag>> for OpTy<'tcx, Tag> {
    #[inline(always)]
    fn from(val: ValTy<'tcx, Tag>) -> Self {
        OpTy {
            op: Operand::Immediate(val.value),
            layout: val.layout
        }
    }
}

impl<'tcx, Tag> OpTy<'tcx, Tag>
{
    #[inline]
    pub fn erase_tag(self) -> OpTy<'tcx>
    {
        OpTy {
            op: self.op.erase_tag(),
            layout: self.layout,
        }
    }
}

// Use the existing layout if given (but sanity check in debug mode),
// or compute the layout.
#[inline(always)]
fn from_known_layout<'tcx>(
    layout: Option<TyLayout<'tcx>>,
    compute: impl FnOnce() -> EvalResult<'tcx, TyLayout<'tcx>>
) -> EvalResult<'tcx, TyLayout<'tcx>> {
    match layout {
        None => compute(),
        Some(layout) => {
            if cfg!(debug_assertions) {
                let layout2 = compute()?;
                assert_eq!(layout.details, layout2.details,
                    "Mismatch in layout of supposedly equal-layout types {:?} and {:?}",
                    layout.ty, layout2.ty);
            }
            Ok(layout)
        }
    }
}

impl<'a, 'mir, 'tcx, M: Machine<'a, 'mir, 'tcx>> EvalContext<'a, 'mir, 'tcx, M> {
    /// Try reading a value in memory; this is interesting particularly for ScalarPair.
    /// Return None if the layout does not permit loading this as a value.
    pub(super) fn try_read_value_from_mplace(
        &self,
        mplace: MPlaceTy<'tcx, M::PointerTag>,
    ) -> EvalResult<'tcx, Option<Value<M::PointerTag>>> {
        if mplace.layout.is_unsized() {
            // Don't touch unsized
            return Ok(None);
        }
        let (ptr, ptr_align) = mplace.to_scalar_ptr_align();

        if mplace.layout.is_zst() {
            // Not all ZSTs have a layout we would handle below, so just short-circuit them
            // all here.
            self.memory.check_align(ptr, ptr_align)?;
            return Ok(Some(Value::Scalar(Scalar::zst().into())));
        }

        let ptr = ptr.to_ptr()?;
        match mplace.layout.abi {
            layout::Abi::Scalar(..) => {
                let scalar = self.memory.read_scalar(ptr, ptr_align, mplace.layout.size)?;
                Ok(Some(Value::Scalar(scalar)))
            }
            layout::Abi::ScalarPair(ref a, ref b) => {
                let (a, b) = (&a.value, &b.value);
                let (a_size, b_size) = (a.size(self), b.size(self));
                let a_ptr = ptr;
                let b_offset = a_size.abi_align(b.align(self));
                assert!(b_offset.bytes() > 0); // we later use the offset to test which field to use
                let b_ptr = ptr.offset(b_offset, self)?.into();
                let a_val = self.memory.read_scalar(a_ptr, ptr_align, a_size)?;
                let b_val = self.memory.read_scalar(b_ptr, ptr_align, b_size)?;
                Ok(Some(Value::ScalarPair(a_val, b_val)))
            }
            _ => Ok(None),
        }
    }

    /// Try returning an immediate value for the operand.
    /// If the layout does not permit loading this as a value, return where in memory
    /// we can find the data.
    /// Note that for a given layout, this operation will either always fail or always
    /// succeed!  Whether it succeeds depends on whether the layout can be represented
    /// in a `Value`, not on which data is stored there currently.
    pub(crate) fn try_read_value(
        &self,
        src: OpTy<'tcx, M::PointerTag>,
    ) -> EvalResult<'tcx, Result<Value<M::PointerTag>, MemPlace<M::PointerTag>>> {
        Ok(match src.try_as_mplace() {
            Ok(mplace) => {
                if let Some(val) = self.try_read_value_from_mplace(mplace)? {
                    Ok(val)
                } else {
                    Err(*mplace)
                }
            },
            Err(val) => Ok(val),
        })
    }

    /// Read a value from a place, asserting that that is possible with the given layout.
    #[inline(always)]
    pub fn read_value(
        &self,
        op: OpTy<'tcx, M::PointerTag>
    ) -> EvalResult<'tcx, ValTy<'tcx, M::PointerTag>> {
        if let Ok(value) = self.try_read_value(op)? {
            Ok(ValTy { value, layout: op.layout })
        } else {
            bug!("primitive read failed for type: {:?}", op.layout.ty);
        }
    }

    /// Read a scalar from a place
    pub fn read_scalar(
        &self,
        op: OpTy<'tcx, M::PointerTag>
    ) -> EvalResult<'tcx, ScalarMaybeUndef<M::PointerTag>> {
        match *self.read_value(op)? {
            Value::ScalarPair(..) => bug!("got ScalarPair for type: {:?}", op.layout.ty),
            Value::Scalar(val) => Ok(val),
        }
    }

    // Turn the MPlace into a string (must already be dereferenced!)
    pub fn read_str(
        &self,
        mplace: MPlaceTy<'tcx, M::PointerTag>,
    ) -> EvalResult<'tcx, &str> {
        let len = mplace.len(self)?;
        let bytes = self.memory.read_bytes(mplace.ptr, Size::from_bytes(len as u64))?;
        let str = ::std::str::from_utf8(bytes)
            .map_err(|err| EvalErrorKind::ValidationFailure(err.to_string()))?;
        Ok(str)
    }

    pub fn uninit_operand(
        &mut self,
        layout: TyLayout<'tcx>
    ) -> EvalResult<'tcx, Operand<M::PointerTag>> {
        // This decides which types we will use the Immediate optimization for, and hence should
        // match what `try_read_value` and `eval_place_to_op` support.
        if layout.is_zst() {
            return Ok(Operand::Immediate(Value::Scalar(Scalar::zst().into())));
        }

        Ok(match layout.abi {
            layout::Abi::Scalar(..) =>
                Operand::Immediate(Value::Scalar(ScalarMaybeUndef::Undef)),
            layout::Abi::ScalarPair(..) =>
                Operand::Immediate(Value::ScalarPair(
                    ScalarMaybeUndef::Undef,
                    ScalarMaybeUndef::Undef,
                )),
            _ => {
                trace!("Forcing allocation for local of type {:?}", layout.ty);
                Operand::Indirect(
                    *self.allocate(layout, MemoryKind::Stack)?
                )
            }
        })
    }

    /// Projection functions
    pub fn operand_field(
        &self,
        op: OpTy<'tcx, M::PointerTag>,
        field: u64,
    ) -> EvalResult<'tcx, OpTy<'tcx, M::PointerTag>> {
        let base = match op.try_as_mplace() {
            Ok(mplace) => {
                // The easy case
                let field = self.mplace_field(mplace, field)?;
                return Ok(field.into());
            },
            Err(value) => value
        };

        let field = field.try_into().unwrap();
        let field_layout = op.layout.field(self, field)?;
        if field_layout.is_zst() {
            let val = Value::Scalar(Scalar::zst().into());
            return Ok(OpTy { op: Operand::Immediate(val), layout: field_layout });
        }
        let offset = op.layout.fields.offset(field);
        let value = match base {
            // the field covers the entire type
            _ if offset.bytes() == 0 && field_layout.size == op.layout.size => base,
            // extract fields from types with `ScalarPair` ABI
            Value::ScalarPair(a, b) => {
                let val = if offset.bytes() == 0 { a } else { b };
                Value::Scalar(val)
            },
            Value::Scalar(val) =>
                bug!("field access on non aggregate {:#?}, {:#?}", val, op.layout),
        };
        Ok(OpTy { op: Operand::Immediate(value), layout: field_layout })
    }

    pub fn operand_downcast(
        &self,
        op: OpTy<'tcx, M::PointerTag>,
        variant: usize,
    ) -> EvalResult<'tcx, OpTy<'tcx, M::PointerTag>> {
        // Downcasts only change the layout
        Ok(match op.try_as_mplace() {
            Ok(mplace) => {
                self.mplace_downcast(mplace, variant)?.into()
            },
            Err(..) => {
                let layout = op.layout.for_variant(self, variant);
                OpTy { layout, ..op }
            }
        })
    }

    // Take an operand, representing a pointer, and dereference it to a place -- that
    // will always be a MemPlace.
    pub(super) fn deref_operand(
        &self,
        src: OpTy<'tcx, M::PointerTag>,
    ) -> EvalResult<'tcx, MPlaceTy<'tcx, M::PointerTag>> {
        let val = self.read_value(src)?;
        trace!("deref to {} on {:?}", val.layout.ty, *val);
        Ok(self.ref_to_mplace(val)?)
    }

    pub fn operand_projection(
        &self,
        base: OpTy<'tcx, M::PointerTag>,
        proj_elem: &mir::PlaceElem<'tcx>,
    ) -> EvalResult<'tcx, OpTy<'tcx, M::PointerTag>> {
        use rustc::mir::ProjectionElem::*;
        Ok(match *proj_elem {
            Field(field, _) => self.operand_field(base, field.index() as u64)?,
            Downcast(_, variant) => self.operand_downcast(base, variant)?,
            Deref => self.deref_operand(base)?.into(),
            Subslice { .. } | ConstantIndex { .. } | Index(_) => if base.layout.is_zst() {
                OpTy {
                    op: Operand::Immediate(Value::Scalar(Scalar::zst().into())),
                    // the actual index doesn't matter, so we just pick a convenient one like 0
                    layout: base.layout.field(self, 0)?,
                }
            } else {
                // The rest should only occur as mplace, we do not use Immediates for types
                // allowing such operations.  This matches place_projection forcing an allocation.
                let mplace = base.to_mem_place();
                self.mplace_projection(mplace, proj_elem)?.into()
            }
        })
    }

    /// This is used by [priroda](https://github.com/oli-obk/priroda) to get an OpTy from a local
    ///
    /// When you know the layout of the local in advance, you can pass it as last argument
    pub fn access_local(
        &self,
        frame: &super::Frame<'mir, 'tcx, M::PointerTag>,
        local: mir::Local,
        layout: Option<TyLayout<'tcx>>,
    ) -> EvalResult<'tcx, OpTy<'tcx, M::PointerTag>> {
        assert_ne!(local, mir::RETURN_PLACE);
        let op = *frame.locals[local].access()?;
        let layout = from_known_layout(layout,
                    || self.layout_of_local(frame, local))?;
        Ok(OpTy { op, layout })
    }

    // Evaluate a place with the goal of reading from it.  This lets us sometimes
    // avoid allocations.  If you already know the layout, you can pass it in
    // to avoid looking it up again.
    fn eval_place_to_op(
        &self,
        mir_place: &mir::Place<'tcx>,
        layout: Option<TyLayout<'tcx>>,
    ) -> EvalResult<'tcx, OpTy<'tcx, M::PointerTag>> {
        use rustc::mir::Place::*;
        let op = match *mir_place {
            Local(mir::RETURN_PLACE) => return err!(ReadFromReturnPointer),
            Local(local) => self.access_local(self.frame(), local, layout)?,

            Projection(ref proj) => {
                let op = self.eval_place_to_op(&proj.base, None)?;
                self.operand_projection(op, &proj.elem)?
            }

            _ => self.eval_place_to_mplace(mir_place)?.into(),
        };

        trace!("eval_place_to_op: got {:?}", *op);
        Ok(op)
    }

    /// Evaluate the operand, returning a place where you can then find the data.
    /// if you already know the layout, you can save two some table lookups
    /// by passing it in here.
    pub fn eval_operand(
        &self,
        mir_op: &mir::Operand<'tcx>,
        layout: Option<TyLayout<'tcx>>,
    ) -> EvalResult<'tcx, OpTy<'tcx, M::PointerTag>> {
        use rustc::mir::Operand::*;
        let op = match *mir_op {
            // FIXME: do some more logic on `move` to invalidate the old location
            Copy(ref place) |
            Move(ref place) =>
                self.eval_place_to_op(place, layout)?,

            Constant(ref constant) => {
                let layout = from_known_layout(layout, || {
                    let ty = self.monomorphize(mir_op.ty(self.mir(), *self.tcx), self.substs());
                    self.layout_of(ty)
                })?;
                let op = self.const_value_to_op(constant.literal.val)?;
                OpTy { op, layout }
            }
        };
        trace!("{:?}: {:?}", mir_op, *op);
        Ok(op)
    }

    /// Evaluate a bunch of operands at once
    pub(super) fn eval_operands(
        &self,
        ops: &[mir::Operand<'tcx>],
    ) -> EvalResult<'tcx, Vec<OpTy<'tcx, M::PointerTag>>> {
        ops.into_iter()
            .map(|op| self.eval_operand(op, None))
            .collect()
    }

    // Also used e.g. when miri runs into a constant.
    pub(super) fn const_value_to_op(
        &self,
        val: ConstValue<'tcx>,
    ) -> EvalResult<'tcx, Operand<M::PointerTag>> {
        trace!("const_value_to_op: {:?}", val);
        match val {
            ConstValue::Unevaluated(def_id, substs) => {
                let instance = self.resolve(def_id, substs)?;
                self.global_to_op(GlobalId {
                    instance,
                    promoted: None,
                })
            }
            ConstValue::ByRef(id, alloc, offset) => {
                // We rely on mutability being set correctly in that allocation to prevent writes
                // where none should happen -- and for `static mut`, we copy on demand anyway.
                Ok(Operand::Indirect(
                    MemPlace::from_ptr(Pointer::new(id, offset), alloc.align)
                ).with_default_tag())
            },
            ConstValue::ScalarPair(a, b) =>
                Ok(Operand::Immediate(Value::ScalarPair(a.into(), b.into())).with_default_tag()),
            ConstValue::Scalar(x) =>
                Ok(Operand::Immediate(Value::Scalar(x.into())).with_default_tag()),
        }
    }
    pub fn const_to_op(
        &self,
        cnst: &ty::Const<'tcx>,
    ) -> EvalResult<'tcx, OpTy<'tcx, M::PointerTag>> {
        let op = self.const_value_to_op(cnst.val)?;
        Ok(OpTy { op, layout: self.layout_of(cnst.ty)? })
    }

    pub(super) fn global_to_op(
        &self,
        gid: GlobalId<'tcx>
    ) -> EvalResult<'tcx, Operand<M::PointerTag>> {
        let cv = self.const_eval(gid)?;
        self.const_value_to_op(cv.val)
    }

    /// Read discriminant, return the runtime value as well as the variant index.
    pub fn read_discriminant(
        &self,
        rval: OpTy<'tcx, M::PointerTag>,
    ) -> EvalResult<'tcx, (u128, usize)> {
        trace!("read_discriminant_value {:#?}", rval.layout);

        match rval.layout.variants {
            layout::Variants::Single { index } => {
                let discr_val = rval.layout.ty.ty_adt_def().map_or(
                    index as u128,
                    |def| def.discriminant_for_variant(*self.tcx, index).val);
                return Ok((discr_val, index));
            }
            layout::Variants::Tagged { .. } |
            layout::Variants::NicheFilling { .. } => {},
        }
        // read raw discriminant value
        let discr_op = self.operand_field(rval, 0)?;
        let discr_val = self.read_value(discr_op)?;
        let raw_discr = discr_val.to_scalar()?;
        trace!("discr value: {:?}", raw_discr);
        // post-process
        Ok(match rval.layout.variants {
            layout::Variants::Single { .. } => bug!(),
            layout::Variants::Tagged { .. } => {
                let real_discr = if discr_val.layout.ty.is_signed() {
                    let i = raw_discr.to_bits(discr_val.layout.size)? as i128;
                    // going from layout tag type to typeck discriminant type
                    // requires first sign extending with the layout discriminant
                    let shift = 128 - discr_val.layout.size.bits();
                    let sexted = (i << shift) >> shift;
                    // and then zeroing with the typeck discriminant type
                    let discr_ty = rval.layout.ty
                        .ty_adt_def().expect("tagged layout corresponds to adt")
                        .repr
                        .discr_type();
                    let discr_ty = layout::Integer::from_attr(self.tcx.tcx, discr_ty);
                    let shift = 128 - discr_ty.size().bits();
                    let truncatee = sexted as u128;
                    (truncatee << shift) >> shift
                } else {
                    raw_discr.to_bits(discr_val.layout.size)?
                };
                // Make sure we catch invalid discriminants
                let index = rval.layout.ty
                    .ty_adt_def()
                    .expect("tagged layout for non adt")
                    .discriminants(self.tcx.tcx)
                    .position(|var| var.val == real_discr)
                    .ok_or_else(|| EvalErrorKind::InvalidDiscriminant(real_discr))?;
                (real_discr, index)
            },
            layout::Variants::NicheFilling {
                dataful_variant,
                ref niche_variants,
                niche_start,
                ..
            } => {
                let variants_start = *niche_variants.start() as u128;
                let variants_end = *niche_variants.end() as u128;
                let real_discr = match raw_discr {
                    Scalar::Ptr(_) => {
                        // The niche must be just 0 (which a pointer value never is)
                        assert!(niche_start == 0);
                        assert!(variants_start == variants_end);
                        dataful_variant as u128
                    },
                    Scalar::Bits { bits: raw_discr, size } => {
                        assert_eq!(size as u64, discr_val.layout.size.bytes());
                        let discr = raw_discr.wrapping_sub(niche_start)
                            .wrapping_add(variants_start);
                        if variants_start <= discr && discr <= variants_end {
                            discr
                        } else {
                            dataful_variant as u128
                        }
                    },
                };
                let index = real_discr as usize;
                assert_eq!(index as u128, real_discr);
                assert!(index < rval.layout.ty
                    .ty_adt_def()
                    .expect("tagged layout for non adt")
                    .variants.len());
                (real_discr, index)
            }
        })
    }

}
