use std::fmt;
use rustc_macros::HashStable;

use crate::ty::{Ty, InferConst, ParamConst, layout::{HasDataLayout, Size}, subst::SubstsRef};
use crate::ty::PlaceholderConst;
use crate::hir::def_id::DefId;

use super::{EvalResult, Pointer, PointerArithmetic, Allocation, AllocId, sign_extend, truncate};

/// Represents the result of a raw const operation, pre-validation.
#[derive(Copy, Clone, Debug, Eq, PartialEq, RustcEncodable, RustcDecodable, Hash, HashStable)]
pub struct RawConst<'tcx> {
    // the value lives here, at offset 0, and that allocation definitely is a `AllocKind::Memory`
    // (so you can use `AllocMap::unwrap_memory`).
    pub alloc_id: AllocId,
    pub ty: Ty<'tcx>,
}

/// Represents a constant value in Rust. `Scalar` and `ScalarPair` are optimizations that
/// match the `LocalState` optimizations for easy conversions between `Value` and `ConstValue`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord,
         RustcEncodable, RustcDecodable, Hash, HashStable)]
pub enum ConstValue<'tcx> {
    /// A const generic parameter.
    Param(ParamConst),

    /// Infer the value of the const.
    Infer(InferConst<'tcx>),

    /// A placeholder const - universally quantified higher-ranked const.
    Placeholder(PlaceholderConst),

    /// Used only for types with `layout::abi::Scalar` ABI and ZSTs.
    ///
    /// Not using the enum `Value` to encode that this must not be `Undef`.
    Scalar(Scalar),

    /// Used only for `&[u8]` and `&str`
    Slice {
        data: &'tcx Allocation,
        start: usize,
        end: usize,
    },

    /// An allocation together with a pointer into the allocation.
    /// Invariant: the pointer's `AllocId` resolves to the allocation.
    ByRef(Pointer, &'tcx Allocation),

    /// Used in the HIR by using `Unevaluated` everywhere and later normalizing to one of the other
    /// variants when the code is monomorphic enough for that.
    Unevaluated(DefId, SubstsRef<'tcx>),
}

#[cfg(target_arch = "x86_64")]
static_assert_size!(ConstValue<'_>, 32);

impl<'tcx> ConstValue<'tcx> {
    #[inline]
    pub fn try_to_scalar(&self) -> Option<Scalar> {
        match *self {
            ConstValue::Param(_) |
            ConstValue::Infer(_) |
            ConstValue::Placeholder(_) |
            ConstValue::ByRef(..) |
            ConstValue::Unevaluated(..) |
            ConstValue::Slice { .. } => None,
            ConstValue::Scalar(val) => Some(val),
        }
    }

    #[inline]
    pub fn try_to_bits(&self, size: Size) -> Option<u128> {
        self.try_to_scalar()?.to_bits(size).ok()
    }

    #[inline]
    pub fn try_to_ptr(&self) -> Option<Pointer> {
        self.try_to_scalar()?.to_ptr().ok()
    }
}

/// A `Scalar` represents an immediate, primitive value existing outside of a
/// `memory::Allocation`. It is in many ways like a small chunk of a `Allocation`, up to 8 bytes in
/// size. Like a range of bytes in an `Allocation`, a `Scalar` can either represent the raw bytes
/// of a simple value or a pointer into another `Allocation`
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd,
         RustcEncodable, RustcDecodable, Hash, HashStable)]
pub enum Scalar<Tag=(), Id=AllocId> {
    /// The raw bytes of a simple value.
    Raw {
        /// The first `size` bytes of `data` are the value.
        /// Do not try to read less or more bytes than that. The remaining bytes must be 0.
        data: u128,
        size: u8,
    },

    /// A pointer into an `Allocation`. An `Allocation` in the `memory` module has a list of
    /// relocations, but a `Scalar` is only large enough to contain one, so we just represent the
    /// relocation and its associated offset together as a `Pointer` here.
    Ptr(Pointer<Tag, Id>),
}

#[cfg(target_arch = "x86_64")]
static_assert_size!(Scalar, 24);

impl<Tag: fmt::Debug, Id: fmt::Debug> fmt::Debug for Scalar<Tag, Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scalar::Ptr(ptr) =>
                write!(f, "{:?}", ptr),
            &Scalar::Raw { data, size } => {
                Scalar::check_data(data, size);
                if size == 0 {
                    write!(f, "<ZST>")
                } else {
                    // Format as hex number wide enough to fit any value of the given `size`.
                    // So data=20, size=1 will be "0x14", but with size=4 it'll be "0x00000014".
                    write!(f, "0x{:>0width$x}", data, width=(size*2) as usize)
                }
            }
        }
    }
}

impl<Tag> fmt::Display for Scalar<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scalar::Ptr(_) => write!(f, "a pointer"),
            Scalar::Raw { data, .. } => write!(f, "{}", data),
        }
    }
}

impl<'tcx> Scalar<()> {
    #[inline(always)]
    fn check_data(data: u128, size: u8) {
        debug_assert_eq!(truncate(data, Size::from_bytes(size as u64)), data,
                         "Scalar value {:#x} exceeds size of {} bytes", data, size);
    }

    #[inline]
    pub fn with_tag<Tag>(self, new_tag: Tag) -> Scalar<Tag> {
        match self {
            Scalar::Ptr(ptr) => Scalar::Ptr(ptr.with_tag(new_tag)),
            Scalar::Raw { data, size } => Scalar::Raw { data, size },
        }
    }

    #[inline(always)]
    pub fn with_default_tag<Tag>(self) -> Scalar<Tag>
        where Tag: Default
    {
        self.with_tag(Tag::default())
    }
}

impl<'tcx, Tag> Scalar<Tag> {
    #[inline]
    pub fn erase_tag(self) -> Scalar {
        match self {
            Scalar::Ptr(ptr) => Scalar::Ptr(ptr.erase_tag()),
            Scalar::Raw { data, size } => Scalar::Raw { data, size },
        }
    }

    #[inline]
    pub fn ptr_null(cx: &impl HasDataLayout) -> Self {
        Scalar::Raw {
            data: 0,
            size: cx.data_layout().pointer_size.bytes() as u8,
        }
    }

    #[inline]
    pub fn zst() -> Self {
        Scalar::Raw { data: 0, size: 0 }
    }

    #[inline]
    pub fn ptr_offset(self, i: Size, cx: &impl HasDataLayout) -> EvalResult<'tcx, Self> {
        let dl = cx.data_layout();
        match self {
            Scalar::Raw { data, size } => {
                assert_eq!(size as u64, dl.pointer_size.bytes());
                Ok(Scalar::Raw {
                    data: dl.offset(data as u64, i.bytes())? as u128,
                    size,
                })
            }
            Scalar::Ptr(ptr) => ptr.offset(i, dl).map(Scalar::Ptr),
        }
    }

    #[inline]
    pub fn ptr_wrapping_offset(self, i: Size, cx: &impl HasDataLayout) -> Self {
        let dl = cx.data_layout();
        match self {
            Scalar::Raw { data, size } => {
                assert_eq!(size as u64, dl.pointer_size.bytes());
                Scalar::Raw {
                    data: dl.overflowing_offset(data as u64, i.bytes()).0 as u128,
                    size,
                }
            }
            Scalar::Ptr(ptr) => Scalar::Ptr(ptr.wrapping_offset(i, dl)),
        }
    }

    #[inline]
    pub fn ptr_signed_offset(self, i: i64, cx: &impl HasDataLayout) -> EvalResult<'tcx, Self> {
        let dl = cx.data_layout();
        match self {
            Scalar::Raw { data, size } => {
                assert_eq!(size as u64, dl.pointer_size().bytes());
                Ok(Scalar::Raw {
                    data: dl.signed_offset(data as u64, i)? as u128,
                    size,
                })
            }
            Scalar::Ptr(ptr) => ptr.signed_offset(i, dl).map(Scalar::Ptr),
        }
    }

    #[inline]
    pub fn ptr_wrapping_signed_offset(self, i: i64, cx: &impl HasDataLayout) -> Self {
        let dl = cx.data_layout();
        match self {
            Scalar::Raw { data, size } => {
                assert_eq!(size as u64, dl.pointer_size.bytes());
                Scalar::Raw {
                    data: dl.overflowing_signed_offset(data as u64, i128::from(i)).0 as u128,
                    size,
                }
            }
            Scalar::Ptr(ptr) => Scalar::Ptr(ptr.wrapping_signed_offset(i, dl)),
        }
    }

    /// Returns this pointer's offset from the allocation base, or from NULL (for
    /// integer pointers).
    #[inline]
    pub fn get_ptr_offset(self, cx: &impl HasDataLayout) -> Size {
        match self {
            Scalar::Raw { data, size } => {
                assert_eq!(size as u64, cx.pointer_size().bytes());
                Size::from_bytes(data as u64)
            }
            Scalar::Ptr(ptr) => ptr.offset,
        }
    }

    #[inline]
    pub fn is_null_ptr(self, cx: &impl HasDataLayout) -> bool {
        match self {
            Scalar::Raw { data, size } => {
                assert_eq!(size as u64, cx.data_layout().pointer_size.bytes());
                data == 0
            },
            Scalar::Ptr(_) => false,
        }
    }

    #[inline]
    pub fn from_bool(b: bool) -> Self {
        Scalar::Raw { data: b as u128, size: 1 }
    }

    #[inline]
    pub fn from_char(c: char) -> Self {
        Scalar::Raw { data: c as u128, size: 4 }
    }

    #[inline]
    pub fn from_uint(i: impl Into<u128>, size: Size) -> Self {
        let i = i.into();
        assert_eq!(
            truncate(i, size), i,
            "Unsigned value {:#x} does not fit in {} bits", i, size.bits()
        );
        Scalar::Raw { data: i, size: size.bytes() as u8 }
    }

    #[inline]
    pub fn from_int(i: impl Into<i128>, size: Size) -> Self {
        let i = i.into();
        // `into` performed sign extension, we have to truncate
        let truncated = truncate(i as u128, size);
        assert_eq!(
            sign_extend(truncated, size) as i128, i,
            "Signed value {:#x} does not fit in {} bits", i, size.bits()
        );
        Scalar::Raw { data: truncated, size: size.bytes() as u8 }
    }

    #[inline]
    pub fn from_f32(f: f32) -> Self {
        Scalar::Raw { data: f.to_bits() as u128, size: 4 }
    }

    #[inline]
    pub fn from_f64(f: f64) -> Self {
        Scalar::Raw { data: f.to_bits() as u128, size: 8 }
    }

    #[inline]
    pub fn to_bits_or_ptr(
        self,
        target_size: Size,
        cx: &impl HasDataLayout,
    ) -> Result<u128, Pointer<Tag>> {
        match self {
            Scalar::Raw { data, size } => {
                assert_eq!(target_size.bytes(), size as u64);
                assert_ne!(size, 0, "you should never look at the bits of a ZST");
                Scalar::check_data(data, size);
                Ok(data)
            }
            Scalar::Ptr(ptr) => {
                assert_eq!(target_size, cx.data_layout().pointer_size);
                Err(ptr)
            }
        }
    }

    #[inline]
    pub fn to_bits(self, target_size: Size) -> EvalResult<'tcx, u128> {
        match self {
            Scalar::Raw { data, size } => {
                assert_eq!(target_size.bytes(), size as u64);
                assert_ne!(size, 0, "you should never look at the bits of a ZST");
                Scalar::check_data(data, size);
                Ok(data)
            }
            Scalar::Ptr(_) => err!(ReadPointerAsBytes),
        }
    }

    #[inline]
    pub fn to_ptr(self) -> EvalResult<'tcx, Pointer<Tag>> {
        match self {
            Scalar::Raw { data: 0, .. } => err!(InvalidNullPointerUsage),
            Scalar::Raw { .. } => err!(ReadBytesAsPointer),
            Scalar::Ptr(p) => Ok(p),
        }
    }

    #[inline]
    pub fn is_bits(self) -> bool {
        match self {
            Scalar::Raw { .. } => true,
            _ => false,
        }
    }

    #[inline]
    pub fn is_ptr(self) -> bool {
        match self {
            Scalar::Ptr(_) => true,
            _ => false,
        }
    }

    pub fn to_bool(self) -> EvalResult<'tcx, bool> {
        match self {
            Scalar::Raw { data: 0, size: 1 } => Ok(false),
            Scalar::Raw { data: 1, size: 1 } => Ok(true),
            _ => err!(InvalidBool),
        }
    }

    pub fn to_char(self) -> EvalResult<'tcx, char> {
        let val = self.to_u32()?;
        match ::std::char::from_u32(val) {
            Some(c) => Ok(c),
            None => err!(InvalidChar(val as u128)),
        }
    }

    pub fn to_u8(self) -> EvalResult<'static, u8> {
        let sz = Size::from_bits(8);
        let b = self.to_bits(sz)?;
        Ok(b as u8)
    }

    pub fn to_u32(self) -> EvalResult<'static, u32> {
        let sz = Size::from_bits(32);
        let b = self.to_bits(sz)?;
        Ok(b as u32)
    }

    pub fn to_u64(self) -> EvalResult<'static, u64> {
        let sz = Size::from_bits(64);
        let b = self.to_bits(sz)?;
        Ok(b as u64)
    }

    pub fn to_usize(self, cx: &impl HasDataLayout) -> EvalResult<'static, u64> {
        let b = self.to_bits(cx.data_layout().pointer_size)?;
        Ok(b as u64)
    }

    pub fn to_i8(self) -> EvalResult<'static, i8> {
        let sz = Size::from_bits(8);
        let b = self.to_bits(sz)?;
        let b = sign_extend(b, sz) as i128;
        Ok(b as i8)
    }

    pub fn to_i32(self) -> EvalResult<'static, i32> {
        let sz = Size::from_bits(32);
        let b = self.to_bits(sz)?;
        let b = sign_extend(b, sz) as i128;
        Ok(b as i32)
    }

    pub fn to_i64(self) -> EvalResult<'static, i64> {
        let sz = Size::from_bits(64);
        let b = self.to_bits(sz)?;
        let b = sign_extend(b, sz) as i128;
        Ok(b as i64)
    }

    pub fn to_isize(self, cx: &impl HasDataLayout) -> EvalResult<'static, i64> {
        let sz = cx.data_layout().pointer_size;
        let b = self.to_bits(sz)?;
        let b = sign_extend(b, sz) as i128;
        Ok(b as i64)
    }

    #[inline]
    pub fn to_f32(self) -> EvalResult<'static, f32> {
        Ok(f32::from_bits(self.to_u32()?))
    }

    #[inline]
    pub fn to_f64(self) -> EvalResult<'static, f64> {
        Ok(f64::from_bits(self.to_u64()?))
    }
}

impl<Tag> From<Pointer<Tag>> for Scalar<Tag> {
    #[inline(always)]
    fn from(ptr: Pointer<Tag>) -> Self {
        Scalar::Ptr(ptr)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, RustcEncodable, RustcDecodable, Hash)]
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

impl<Tag: fmt::Debug, Id: fmt::Debug> fmt::Debug for ScalarMaybeUndef<Tag, Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScalarMaybeUndef::Undef => write!(f, "Undef"),
            ScalarMaybeUndef::Scalar(s) => write!(f, "{:?}", s),
        }
    }
}

impl<Tag> fmt::Display for ScalarMaybeUndef<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScalarMaybeUndef::Undef => write!(f, "uninitialized bytes"),
            ScalarMaybeUndef::Scalar(s) => write!(f, "{}", s),
        }
    }
}

impl<'tcx> ScalarMaybeUndef<()> {
    #[inline]
    pub fn with_tag<Tag>(self, new_tag: Tag) -> ScalarMaybeUndef<Tag> {
        match self {
            ScalarMaybeUndef::Scalar(s) => ScalarMaybeUndef::Scalar(s.with_tag(new_tag)),
            ScalarMaybeUndef::Undef => ScalarMaybeUndef::Undef,
        }
    }

    #[inline(always)]
    pub fn with_default_tag<Tag>(self) -> ScalarMaybeUndef<Tag>
        where Tag: Default
    {
        self.with_tag(Tag::default())
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
    pub fn to_usize(self, cx: &impl HasDataLayout) -> EvalResult<'tcx, u64> {
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
    pub fn to_isize(self, cx: &impl HasDataLayout) -> EvalResult<'tcx, i64> {
        self.not_undef()?.to_isize(cx)
    }
}

impl_stable_hash_for!(enum crate::mir::interpret::ScalarMaybeUndef {
    Scalar(v),
    Undef
});
