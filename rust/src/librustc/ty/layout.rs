// Copyright 2016 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub use self::Integer::*;
pub use self::Layout::*;
pub use self::Primitive::*;

use infer::InferCtxt;
use session::Session;
use traits;
use ty::{self, Ty, TyCtxt, TypeFoldable};

use util::common::slice_pat;

use syntax::ast::{FloatTy, IntTy, UintTy};
use syntax::attr;
use syntax_pos::DUMMY_SP;

use std::cmp;
use std::fmt;
use std::i64;

/// Parsed [Data layout](http://llvm.org/docs/LangRef.html#data-layout)
/// for a target, which contains everything needed to compute layouts.
pub struct TargetDataLayout {
    pub endian: Endian,
    pub i1_align: Align,
    pub i8_align: Align,
    pub i16_align: Align,
    pub i32_align: Align,
    pub i64_align: Align,
    pub f32_align: Align,
    pub f64_align: Align,
    pub pointer_size: Size,
    pub pointer_align: Align,
    pub aggregate_align: Align,

    /// Alignments for vector types.
    pub vector_align: Vec<(Size, Align)>
}

impl Default for TargetDataLayout {
    fn default() -> TargetDataLayout {
        TargetDataLayout {
            endian: Endian::Big,
            i1_align: Align::from_bits(8, 8).unwrap(),
            i8_align: Align::from_bits(8, 8).unwrap(),
            i16_align: Align::from_bits(16, 16).unwrap(),
            i32_align: Align::from_bits(32, 32).unwrap(),
            i64_align: Align::from_bits(32, 64).unwrap(),
            f32_align: Align::from_bits(32, 32).unwrap(),
            f64_align: Align::from_bits(64, 64).unwrap(),
            pointer_size: Size::from_bits(64),
            pointer_align: Align::from_bits(64, 64).unwrap(),
            aggregate_align: Align::from_bits(0, 64).unwrap(),
            vector_align: vec![
                (Size::from_bits(64), Align::from_bits(64, 64).unwrap()),
                (Size::from_bits(128), Align::from_bits(128, 128).unwrap())
            ]
        }
    }
}

impl TargetDataLayout {
    pub fn parse(sess: &Session) -> TargetDataLayout {
        // Parse a bit count from a string.
        let parse_bits = |s: &str, kind: &str, cause: &str| {
            s.parse::<u64>().unwrap_or_else(|err| {
                sess.err(&format!("invalid {} `{}` for `{}` in \"data-layout\": {}",
                                  kind, s, cause, err));
                0
            })
        };

        // Parse a size string.
        let size = |s: &str, cause: &str| {
            Size::from_bits(parse_bits(s, "size", cause))
        };

        // Parse an alignment string.
        let align = |s: &[&str], cause: &str| {
            if s.is_empty() {
                sess.err(&format!("missing alignment for `{}` in \"data-layout\"", cause));
            }
            let abi = parse_bits(s[0], "alignment", cause);
            let pref = s.get(1).map_or(abi, |pref| parse_bits(pref, "alignment", cause));
            Align::from_bits(abi, pref).unwrap_or_else(|err| {
                sess.err(&format!("invalid alignment for `{}` in \"data-layout\": {}",
                                  cause, err));
                Align::from_bits(8, 8).unwrap()
            })
        };

        let mut dl = TargetDataLayout::default();
        for spec in sess.target.target.data_layout.split("-") {
            match slice_pat(&&spec.split(":").collect::<Vec<_>>()[..]) {
                &["e"] => dl.endian = Endian::Little,
                &["E"] => dl.endian = Endian::Big,
                &["a", ref a..] => dl.aggregate_align = align(a, "a"),
                &["f32", ref a..] => dl.f32_align = align(a, "f32"),
                &["f64", ref a..] => dl.f64_align = align(a, "f64"),
                &[p @ "p", s, ref a..] | &[p @ "p0", s, ref a..] => {
                    dl.pointer_size = size(s, p);
                    dl.pointer_align = align(a, p);
                }
                &[s, ref a..] if s.starts_with("i") => {
                    let ty_align = match s[1..].parse::<u64>() {
                        Ok(1) => &mut dl.i8_align,
                        Ok(8) => &mut dl.i8_align,
                        Ok(16) => &mut dl.i16_align,
                        Ok(32) => &mut dl.i32_align,
                        Ok(64) => &mut dl.i64_align,
                        Ok(_) => continue,
                        Err(_) => {
                            size(&s[1..], "i"); // For the user error.
                            continue;
                        }
                    };
                    *ty_align = align(a, s);
                }
                &[s, ref a..] if s.starts_with("v") => {
                    let v_size = size(&s[1..], "v");
                    let a = align(a, s);
                    if let Some(v) = dl.vector_align.iter_mut().find(|v| v.0 == v_size) {
                        v.1 = a;
                        continue;
                    }
                    // No existing entry, add a new one.
                    dl.vector_align.push((v_size, a));
                }
                _ => {} // Ignore everything else.
            }
        }

        // Perform consistency checks against the Target information.
        let endian_str = match dl.endian {
            Endian::Little => "little",
            Endian::Big => "big"
        };
        if endian_str != sess.target.target.target_endian {
            sess.err(&format!("inconsistent target specification: \"data-layout\" claims \
                               architecture is {}-endian, while \"target-endian\" is `{}`",
                              endian_str, sess.target.target.target_endian));
        }

        if dl.pointer_size.bits().to_string() != sess.target.target.target_pointer_width {
            sess.err(&format!("inconsistent target specification: \"data-layout\" claims \
                               pointers are {}-bit, while \"target-pointer-width\" is `{}`",
                              dl.pointer_size.bits(), sess.target.target.target_pointer_width));
        }

        dl
    }

    /// Return exclusive upper bound on object size.
    ///
    /// The theoretical maximum object size is defined as the maximum positive `isize` value.
    /// This ensures that the `offset` semantics remain well-defined by allowing it to correctly
    /// index every address within an object along with one byte past the end, along with allowing
    /// `isize` to store the difference between any two pointers into an object.
    ///
    /// The upper bound on 64-bit currently needs to be lower because LLVM uses a 64-bit integer
    /// to represent object size in bits. It would need to be 1 << 61 to account for this, but is
    /// currently conservatively bounded to 1 << 47 as that is enough to cover the current usable
    /// address space on 64-bit ARMv8 and x86_64.
    pub fn obj_size_bound(&self) -> u64 {
        match self.pointer_size.bits() {
            16 => 1 << 15,
            32 => 1 << 31,
            64 => 1 << 47,
            bits => bug!("obj_size_bound: unknown pointer bit size {}", bits)
        }
    }

    pub fn ptr_sized_integer(&self) -> Integer {
        match self.pointer_size.bits() {
            16 => I16,
            32 => I32,
            64 => I64,
            bits => bug!("ptr_sized_integer: unknown pointer bit size {}", bits)
        }
    }
}

/// Endianness of the target, which must match cfg(target-endian).
#[derive(Copy, Clone)]
pub enum Endian {
    Little,
    Big
}

/// Size of a type in bytes.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Size {
    raw: u64
}

impl Size {
    pub fn from_bits(bits: u64) -> Size {
        Size::from_bytes((bits + 7) / 8)
    }

    pub fn from_bytes(bytes: u64) -> Size {
        if bytes >= (1 << 61) {
            bug!("Size::from_bytes: {} bytes in bits doesn't fit in u64", bytes)
        }
        Size {
            raw: bytes
        }
    }

    pub fn bytes(self) -> u64 {
        self.raw
    }

    pub fn bits(self) -> u64 {
        self.bytes() * 8
    }

    pub fn abi_align(self, align: Align) -> Size {
        let mask = align.abi() - 1;
        Size::from_bytes((self.bytes() + mask) & !mask)
    }

    pub fn checked_add(self, offset: Size, dl: &TargetDataLayout) -> Option<Size> {
        // Each Size is less than dl.obj_size_bound(), so the sum is
        // also less than 1 << 62 (and therefore can't overflow).
        let bytes = self.bytes() + offset.bytes();

        if bytes < dl.obj_size_bound() {
            Some(Size::from_bytes(bytes))
        } else {
            None
        }
    }

    pub fn checked_mul(self, count: u64, dl: &TargetDataLayout) -> Option<Size> {
        // Each Size is less than dl.obj_size_bound(), so the sum is
        // also less than 1 << 62 (and therefore can't overflow).
        match self.bytes().checked_mul(count) {
            Some(bytes) if bytes < dl.obj_size_bound() => {
                Some(Size::from_bytes(bytes))
            }
            _ => None
        }
    }
}

/// Alignment of a type in bytes, both ABI-mandated and preferred.
/// Since alignments are always powers of 2, we can pack both in one byte,
/// giving each a nibble (4 bits) for a maximum alignment of 2^15 = 32768.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Align {
    raw: u8
}

impl Align {
    pub fn from_bits(abi: u64, pref: u64) -> Result<Align, String> {
        Align::from_bytes((abi + 7) / 8, (pref + 7) / 8)
    }

    pub fn from_bytes(abi: u64, pref: u64) -> Result<Align, String> {
        let pack = |align: u64| {
            // Treat an alignment of 0 bytes like 1-byte alignment.
            if align == 0 {
                return Ok(0);
            }

            let mut bytes = align;
            let mut pow: u8 = 0;
            while (bytes & 1) == 0 {
                pow += 1;
                bytes >>= 1;
            }
            if bytes != 1 {
                Err(format!("`{}` is not a power of 2", align))
            } else if pow > 0x0f {
                Err(format!("`{}` is too large", align))
            } else {
                Ok(pow)
            }
        };

        Ok(Align {
            raw: pack(abi)? | (pack(pref)? << 4)
        })
    }

    pub fn abi(self) -> u64 {
        1 << (self.raw & 0xf)
    }

    pub fn pref(self) -> u64 {
        1 << (self.raw >> 4)
    }

    pub fn min(self, other: Align) -> Align {
        let abi = cmp::min(self.raw & 0x0f, other.raw & 0x0f);
        let pref = cmp::min(self.raw & 0xf0, other.raw & 0xf0);
        Align {
            raw: abi | pref
        }
    }

    pub fn max(self, other: Align) -> Align {
        let abi = cmp::max(self.raw & 0x0f, other.raw & 0x0f);
        let pref = cmp::max(self.raw & 0xf0, other.raw & 0xf0);
        Align {
            raw: abi | pref
        }
    }
}

/// Integers, also used for enum discriminants.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Integer {
    I1,
    I8,
    I16,
    I32,
    I64
}

impl Integer {
    /// Find the smallest Integer type which can represent the signed value.
    pub fn fit_signed(x: i64) -> Integer {
        match x {
            -0x0000_0001...0x0000_0000 => I1,
            -0x0000_0080...0x0000_007f => I8,
            -0x0000_8000...0x0000_7fff => I16,
            -0x8000_0000...0x7fff_ffff => I32,
            _ => I64
        }
    }

    /// Find the smallest Integer type which can represent the unsigned value.
    pub fn fit_unsigned(x: u64) -> Integer {
        match x {
            0...0x0000_0001 => I1,
            0...0x0000_00ff => I8,
            0...0x0000_ffff => I16,
            0...0xffff_ffff => I32,
            _ => I64
        }
    }

    /// Get the Integer type from an attr::IntType.
    pub fn from_attr(dl: &TargetDataLayout, ity: attr::IntType) -> Integer {
        match ity {
            attr::SignedInt(IntTy::I8) | attr::UnsignedInt(UintTy::U8) => I8,
            attr::SignedInt(IntTy::I16) | attr::UnsignedInt(UintTy::U16) => I16,
            attr::SignedInt(IntTy::I32) | attr::UnsignedInt(UintTy::U32) => I32,
            attr::SignedInt(IntTy::I64) | attr::UnsignedInt(UintTy::U64) => I64,
            attr::SignedInt(IntTy::Is) | attr::UnsignedInt(UintTy::Us) => {
                dl.ptr_sized_integer()
            }
        }
    }

    /// Find the appropriate Integer type and signedness for the given
    /// signed discriminant range and #[repr] attribute.
    /// N.B.: u64 values above i64::MAX will be treated as signed, but
    /// that shouldn't affect anything, other than maybe debuginfo.
    pub fn repr_discr(tcx: TyCtxt, hint: attr::ReprAttr, min: i64, max: i64)
                      -> (Integer, bool) {
        // Theoretically, negative values could be larger in unsigned representation
        // than the unsigned representation of the signed minimum. However, if there
        // are any negative values, the only valid unsigned representation is u64
        // which can fit all i64 values, so the result remains unaffected.
        let unsigned_fit = Integer::fit_unsigned(cmp::max(min as u64, max as u64));
        let signed_fit = cmp::max(Integer::fit_signed(min), Integer::fit_signed(max));

        let at_least = match hint {
            attr::ReprInt(span, ity) => {
                let discr = Integer::from_attr(&tcx.data_layout, ity);
                let fit = if ity.is_signed() { signed_fit } else { unsigned_fit };
                if discr < fit {
                    span_bug!(span, "representation hint insufficient for discriminant range")
                }
                return (discr, ity.is_signed());
            }
            attr::ReprExtern => {
                match &tcx.sess.target.target.arch[..] {
                    // WARNING: the ARM EABI has two variants; the one corresponding
                    // to `at_least == I32` appears to be used on Linux and NetBSD,
                    // but some systems may use the variant corresponding to no
                    // lower bound.  However, we don't run on those yet...?
                    "arm" => I32,
                    _ => I32,
                }
            }
            attr::ReprAny => I8,
            attr::ReprPacked => {
                bug!("Integer::repr_discr: found #[repr(packed)] on an enum");
            }
            attr::ReprSimd => {
                bug!("Integer::repr_discr: found #[repr(simd)] on an enum");
            }
        };

        // If there are no negative values, we can use the unsigned fit.
        if min >= 0 {
            (cmp::max(unsigned_fit, at_least), false)
        } else {
            (cmp::max(signed_fit, at_least), true)
        }
    }
}

/// Fundamental unit of memory access and layout.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Primitive {
    Int(Integer),
    F32,
    F64,
    Pointer
}

impl Primitive {
    pub fn size(self, dl: &TargetDataLayout) -> Size {
        match self {
            Int(I1) | Int(I8) => Size::from_bits(8),
            Int(I16) => Size::from_bits(16),
            Int(I32) | F32 => Size::from_bits(32),
            Int(I64) | F64 => Size::from_bits(64),
            Pointer => dl.pointer_size
        }
    }

    pub fn align(self, dl: &TargetDataLayout) -> Align {
        match self {
            Int(I1) => dl.i1_align,
            Int(I8) => dl.i8_align,
            Int(I16) => dl.i16_align,
            Int(I32) => dl.i32_align,
            Int(I64) => dl.i64_align,
            F32 => dl.f32_align,
            F64 => dl.f64_align,
            Pointer => dl.pointer_align
        }
    }
}

/// Path through fields of nested structures.
// FIXME(eddyb) use small vector optimization for the common case.
pub type FieldPath = Vec<u32>;

/// A structure, a product type in ADT terms.
#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Struct {
    pub align: Align,

    /// If true, no alignment padding is used.
    pub packed: bool,

    /// If true, the size is exact, otherwise it's only a lower bound.
    pub sized: bool,

    /// Offsets for the first byte after each field.
    /// That is, field_offset(i) = offset_after_field[i - 1] and the
    /// whole structure's size is the last offset, excluding padding.
    // FIXME(eddyb) use small vector optimization for the common case.
    pub offset_after_field: Vec<Size>
}

impl<'a, 'gcx, 'tcx> Struct {
    pub fn new(dl: &TargetDataLayout, packed: bool) -> Struct {
        Struct {
            align: if packed { dl.i8_align } else { dl.aggregate_align },
            packed: packed,
            sized: true,
            offset_after_field: vec![]
        }
    }

    /// Extend the Struct with more fields.
    pub fn extend<I>(&mut self, dl: &TargetDataLayout,
                     fields: I,
                     scapegoat: Ty<'gcx>)
                     -> Result<(), LayoutError<'gcx>>
    where I: Iterator<Item=Result<&'a Layout, LayoutError<'gcx>>> {
        self.offset_after_field.reserve(fields.size_hint().0);

        for field in fields {
            if !self.sized {
                bug!("Struct::compute: field #{} of `{}` comes after unsized field",
                     self.offset_after_field.len(), scapegoat);
            }

            let field = field?;
            if field.is_unsized() {
                self.sized = false;
            }

            // Invariant: offset < dl.obj_size_bound() <= 1<<61
            let mut offset = if !self.packed {
                let align = field.align(dl);
                self.align = self.align.max(align);
                self.offset_after_field.last_mut().map_or(Size::from_bytes(0), |last| {
                    *last = last.abi_align(align);
                    *last
                })
            } else {
                self.offset_after_field.last().map_or(Size::from_bytes(0), |&last| last)
            };

            offset = offset.checked_add(field.size(dl), dl)
                           .map_or(Err(LayoutError::SizeOverflow(scapegoat)), Ok)?;

            self.offset_after_field.push(offset);
        }

        Ok(())
    }

    /// Get the size without trailing alignment padding.
    pub fn min_size(&self) -> Size {
        self.offset_after_field.last().map_or(Size::from_bytes(0), |&last| last)
    }

    /// Get the size with trailing aligment padding.
    pub fn stride(&self) -> Size {
        self.min_size().abi_align(self.align)
    }

    /// Determine whether a structure would be zero-sized, given its fields.
    pub fn would_be_zero_sized<I>(dl: &TargetDataLayout, fields: I)
                                  -> Result<bool, LayoutError<'gcx>>
    where I: Iterator<Item=Result<&'a Layout, LayoutError<'gcx>>> {
        for field in fields {
            let field = field?;
            if field.is_unsized() || field.size(dl).bytes() > 0 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Find the path leading to a non-zero leaf field, starting from
    /// the given type and recursing through aggregates.
    // FIXME(eddyb) track value ranges and traverse already optimized enums.
    pub fn non_zero_field_in_type(infcx: &InferCtxt<'a, 'gcx, 'tcx>,
                                  ty: Ty<'gcx>)
                                  -> Result<Option<FieldPath>, LayoutError<'gcx>> {
        let tcx = infcx.tcx.global_tcx();
        match (ty.layout(infcx)?, &ty.sty) {
            (&Scalar { non_zero: true, .. }, _) => Ok(Some(vec![])),
            (&FatPointer { non_zero: true, .. }, _) => {
                Ok(Some(vec![FAT_PTR_ADDR as u32]))
            }

            // Is this the NonZero lang item wrapping a pointer or integer type?
            (&Univariant { non_zero: true, .. }, &ty::TyStruct(def, substs)) => {
                let fields = &def.struct_variant().fields;
                assert_eq!(fields.len(), 1);
                let ty = normalize_associated_type(infcx, fields[0].ty(tcx, substs));
                match *ty.layout(infcx)? {
                    // FIXME(eddyb) also allow floating-point types here.
                    Scalar { value: Int(_), non_zero: false } |
                    Scalar { value: Pointer, non_zero: false } => {
                        Ok(Some(vec![0]))
                    }
                    FatPointer { non_zero: false, .. } => {
                        Ok(Some(vec![FAT_PTR_ADDR as u32, 0]))
                    }
                    _ => Ok(None)
                }
            }

            // Perhaps one of the fields of this struct is non-zero
            // let's recurse and find out
            (_, &ty::TyStruct(def, substs)) => {
                Struct::non_zero_field_path(infcx, def.struct_variant().fields
                                                      .iter().map(|field| {
                    normalize_associated_type(infcx, field.ty(tcx, substs))
                }))
            }

            // Perhaps one of the upvars of this closure is non-zero
            // Let's recurse and find out!
            (_, &ty::TyClosure(_, ty::ClosureSubsts { upvar_tys: tys, .. })) |
            // Can we use one of the fields in this tuple?
            (_, &ty::TyTuple(tys)) => {
                Struct::non_zero_field_path(infcx, tys.iter().cloned())
            }

            // Is this a fixed-size array of something non-zero
            // with at least one element?
            (_, &ty::TyArray(ety, d)) if d > 0 => {
                Struct::non_zero_field_path(infcx, Some(ety).into_iter())
            }

            // Anything else is not a non-zero type.
            _ => Ok(None)
        }
    }

    /// Find the path leading to a non-zero leaf field, starting from
    /// the given set of fields and recursing through aggregates.
    pub fn non_zero_field_path<I>(infcx: &InferCtxt<'a, 'gcx, 'tcx>,
                                  fields: I)
                                  -> Result<Option<FieldPath>, LayoutError<'gcx>>
    where I: Iterator<Item=Ty<'gcx>> {
        for (i, ty) in fields.enumerate() {
            if let Some(mut path) = Struct::non_zero_field_in_type(infcx, ty)? {
                path.push(i as u32);
                return Ok(Some(path));
            }
        }
        Ok(None)
    }
}

/// The first half of a fat pointer.
/// - For a trait object, this is the address of the box.
/// - For a slice, this is the base address.
pub const FAT_PTR_ADDR: usize = 0;

/// The second half of a fat pointer.
/// - For a trait object, this is the address of the vtable.
/// - For a slice, this is the length.
pub const FAT_PTR_EXTRA: usize = 1;

/// Type layout, from which size and alignment can be cheaply computed.
/// For ADTs, it also includes field placement and enum optimizations.
/// NOTE: Because Layout is interned, redundant information should be
/// kept to a minimum, e.g. it includes no sub-component Ty or Layout.
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Layout {
    /// TyBool, TyChar, TyInt, TyUint, TyFloat, TyRawPtr, TyRef or TyFnPtr.
    Scalar {
        value: Primitive,
        // If true, the value cannot represent a bit pattern of all zeroes.
        non_zero: bool
    },

    /// SIMD vectors, from TyStruct marked with #[repr(simd)].
    Vector {
        element: Primitive,
        count: u64
    },

    /// TyArray, TySlice or TyStr.
    Array {
        /// If true, the size is exact, otherwise it's only a lower bound.
        sized: bool,
        align: Align,
        size: Size
    },

    /// TyRawPtr or TyRef with a !Sized pointee.
    FatPointer {
        metadata: Primitive,
        // If true, the pointer cannot be null.
        non_zero: bool
    },

    // Remaining variants are all ADTs such as TyStruct, TyEnum or TyTuple.

    /// C-like enums; basically an integer.
    CEnum {
        discr: Integer,
        signed: bool,
        // Inclusive discriminant range.
        // If min > max, it represents min...u64::MAX followed by 0...max.
        // FIXME(eddyb) always use the shortest range, e.g. by finding
        // the largest space between two consecutive discriminants and
        // taking everything else as the (shortest) discriminant range.
        min: u64,
        max: u64
    },

    /// Single-case enums, and structs/tuples.
    Univariant {
        variant: Struct,
        // If true, the structure is NonZero.
        // FIXME(eddyb) use a newtype Layout kind for this.
        non_zero: bool
    },

    /// General-case enums: for each case there is a struct, and they
    /// all start with a field for the discriminant.
    General {
        discr: Integer,
        variants: Vec<Struct>,
        size: Size,
        align: Align
    },

    /// Two cases distinguished by a nullable pointer: the case with discriminant
    /// `nndiscr` must have single field which is known to be nonnull due to its type.
    /// The other case is known to be zero sized. Hence we represent the enum
    /// as simply a nullable pointer: if not null it indicates the `nndiscr` variant,
    /// otherwise it indicates the other case.
    ///
    /// For example, `std::option::Option` instantiated at a safe pointer type
    /// is represented such that `None` is a null pointer and `Some` is the
    /// identity function.
    RawNullablePointer {
        nndiscr: u64,
        value: Primitive
    },

    /// Two cases distinguished by a nullable pointer: the case with discriminant
    /// `nndiscr` is represented by the struct `nonnull`, where the `discrfield`th
    /// field is known to be nonnull due to its type; if that field is null, then
    /// it represents the other case, which is known to be zero sized.
    StructWrappedNullablePointer {
        nndiscr: u64,
        nonnull: Struct,
        // N.B. There is a 0 at the start, for LLVM GEP through a pointer.
        discrfield: FieldPath
    }
}

#[derive(Copy, Clone, Debug)]
pub enum LayoutError<'tcx> {
    Unknown(Ty<'tcx>),
    SizeOverflow(Ty<'tcx>)
}

impl<'tcx> fmt::Display for LayoutError<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            LayoutError::Unknown(ty) => {
                write!(f, "the type `{:?}` has an unknown layout", ty)
            }
            LayoutError::SizeOverflow(ty) => {
                write!(f, "the type `{:?}` is too big for the current architecture", ty)
            }
        }
    }
}

/// Helper function for normalizing associated types in an inference context.
fn normalize_associated_type<'a, 'gcx, 'tcx>(infcx: &InferCtxt<'a, 'gcx, 'tcx>,
                                             ty: Ty<'gcx>)
                                             -> Ty<'gcx> {
    if !ty.has_projection_types() {
        return ty;
    }

    let mut selcx = traits::SelectionContext::new(infcx);
    let cause = traits::ObligationCause::dummy();
    let traits::Normalized { value: result, obligations } =
        traits::normalize(&mut selcx, cause, &ty);

    let mut fulfill_cx = traits::FulfillmentContext::new();

    for obligation in obligations {
        fulfill_cx.register_predicate_obligation(infcx, obligation);
    }

    infcx.drain_fulfillment_cx_or_panic(DUMMY_SP, &mut fulfill_cx, &result)
}

impl<'a, 'gcx, 'tcx> Layout {
    pub fn compute_uncached(ty: Ty<'gcx>,
                            infcx: &InferCtxt<'a, 'gcx, 'tcx>)
                            -> Result<Layout, LayoutError<'gcx>> {
        let tcx = infcx.tcx.global_tcx();
        let dl = &tcx.data_layout;
        assert!(!ty.has_infer_types());

        let layout = match ty.sty {
            // Basic scalars.
            ty::TyBool => Scalar { value: Int(I1), non_zero: false },
            ty::TyChar => Scalar { value: Int(I32), non_zero: false },
            ty::TyInt(ity) => {
                Scalar {
                    value: Int(Integer::from_attr(dl, attr::SignedInt(ity))),
                    non_zero: false
                }
            }
            ty::TyUint(ity) => {
                Scalar {
                    value: Int(Integer::from_attr(dl, attr::UnsignedInt(ity))),
                    non_zero: false
                }
            }
            ty::TyFloat(FloatTy::F32) => Scalar { value: F32, non_zero: false },
            ty::TyFloat(FloatTy::F64) => Scalar { value: F64, non_zero: false },
            ty::TyFnPtr(_) => Scalar { value: Pointer, non_zero: true },

            // Potentially-fat pointers.
            ty::TyBox(pointee) |
            ty::TyRef(_, ty::TypeAndMut { ty: pointee, .. }) |
            ty::TyRawPtr(ty::TypeAndMut { ty: pointee, .. }) => {
                let non_zero = !ty.is_unsafe_ptr();
                if pointee.is_sized(tcx, &infcx.parameter_environment, DUMMY_SP) {
                    Scalar { value: Pointer, non_zero: non_zero }
                } else {
                    let unsized_part = tcx.struct_tail(pointee);
                    let meta = match unsized_part.sty {
                        ty::TySlice(_) | ty::TyStr => {
                            Int(dl.ptr_sized_integer())
                        }
                        ty::TyTrait(_) => Pointer,
                        _ => return Err(LayoutError::Unknown(unsized_part))
                    };
                    FatPointer { metadata: meta, non_zero: non_zero }
                }
            }

            // Arrays and slices.
            ty::TyArray(element, count) => {
                let element = element.layout(infcx)?;
                Array {
                    sized: true,
                    align: element.align(dl),
                    size: element.size(dl).checked_mul(count as u64, dl)
                                 .map_or(Err(LayoutError::SizeOverflow(ty)), Ok)?
                }
            }
            ty::TySlice(element) => {
                Array {
                    sized: false,
                    align: element.layout(infcx)?.align(dl),
                    size: Size::from_bytes(0)
                }
            }
            ty::TyStr => {
                Array {
                    sized: false,
                    align: dl.i8_align,
                    size: Size::from_bytes(0)
                }
            }

            // Odd unit types.
            ty::TyFnDef(..) => {
                Univariant {
                    variant: Struct::new(dl, false),
                    non_zero: false
                }
            }
            ty::TyTrait(_) => {
                let mut unit = Struct::new(dl, false);
                unit.sized = false;
                Univariant { variant: unit, non_zero: false }
            }

            // Tuples.
            ty::TyClosure(_, ty::ClosureSubsts { upvar_tys: tys, .. }) |
            ty::TyTuple(tys) => {
                let mut st = Struct::new(dl, false);
                st.extend(dl, tys.iter().map(|ty| ty.layout(infcx)), ty)?;
                Univariant { variant: st, non_zero: false }
            }

            // ADTs.
            ty::TyStruct(def, substs) => {
                if ty.is_simd() {
                    // SIMD vector types.
                    let element = ty.simd_type(tcx);
                    match *element.layout(infcx)? {
                        Scalar { value, .. } => {
                            return Ok(Vector {
                                element: value,
                                count: ty.simd_size(tcx) as u64
                            });
                        }
                        _ => {
                            tcx.sess.fatal(&format!("monomorphising SIMD type `{}` with \
                                                     a non-machine element type `{}`",
                                                    ty, element));
                        }
                    }
                }
                let fields = def.struct_variant().fields.iter().map(|field| {
                    normalize_associated_type(infcx, field.ty(tcx, substs))
                        .layout(infcx)
                });
                let packed = tcx.lookup_packed(def.did);
                let mut st = Struct::new(dl, packed);
                st.extend(dl, fields, ty)?;

                // FIXME(16758) don't add a drop flag to unsized structs, as it
                // won't actually be in the location we say it is because it'll be after
                // the unsized field. Several other pieces of code assume that the unsized
                // field is definitely the last one.
                if def.dtor_kind().has_drop_flag() &&
                   ty.is_sized(tcx, &infcx.parameter_environment, DUMMY_SP) {
                    st.extend(dl, Some(Ok(&Scalar {
                        value: Int(I8),
                        non_zero: false
                    })).into_iter(), ty)?;
                }
                Univariant {
                    variant: st,
                    non_zero: Some(def.did) == tcx.lang_items.non_zero()
                }
            }
            ty::TyEnum(def, substs) => {
                let hint = *tcx.lookup_repr_hints(def.did).get(0)
                    .unwrap_or(&attr::ReprAny);

                let dtor = def.dtor_kind().has_drop_flag();
                let drop_flag = if dtor {
                    Some(Scalar { value: Int(I8), non_zero: false })
                } else {
                    None
                };

                if def.variants.is_empty() {
                    // Uninhabitable; represent as unit
                    // (Typechecking will reject discriminant-sizing attrs.)
                    assert_eq!(hint, attr::ReprAny);

                    let mut st = Struct::new(dl, false);
                    st.extend(dl, drop_flag.iter().map(Ok), ty)?;
                    return Ok(Univariant { variant: st, non_zero: false });
                }

                if !dtor && def.variants.iter().all(|v| v.fields.is_empty()) {
                    // All bodies empty -> intlike
                    let (mut min, mut max) = (i64::MAX, i64::MIN);
                    for v in &def.variants {
                        let x = v.disr_val.to_u64_unchecked() as i64;
                        if x < min { min = x; }
                        if x > max { max = x; }
                    }

                    let (discr, signed) = Integer::repr_discr(tcx, hint, min, max);
                    return Ok(CEnum {
                        discr: discr,
                        signed: signed,
                        min: min as u64,
                        max: max as u64
                    });
                }

                // Since there's at least one
                // non-empty body, explicit discriminants should have
                // been rejected by a checker before this point.
                for (i, v) in def.variants.iter().enumerate() {
                    if i as u64 != v.disr_val.to_u64_unchecked() {
                        bug!("non-C-like enum {} with specified discriminants",
                             tcx.item_path_str(def.did));
                    }
                }

                if def.variants.len() == 1 {
                    // Equivalent to a struct/tuple/newtype.
                    // (Typechecking will reject discriminant-sizing attrs.)
                    assert_eq!(hint, attr::ReprAny);
                    let fields = def.variants[0].fields.iter().map(|field| {
                        normalize_associated_type(infcx, field.ty(tcx, substs))
                            .layout(infcx)
                    });
                    let mut st = Struct::new(dl, false);
                    st.extend(dl, fields.chain(drop_flag.iter().map(Ok)), ty)?;
                    return Ok(Univariant { variant: st, non_zero: false });
                }

                // Cache the substituted and normalized variant field types.
                let variants = def.variants.iter().map(|v| {
                    v.fields.iter().map(|field| {
                        normalize_associated_type(infcx, field.ty(tcx, substs))
                    }).collect::<Vec<_>>()
                }).collect::<Vec<_>>();

                if !dtor && variants.len() == 2 && hint == attr::ReprAny {
                    // Nullable pointer optimization
                    for discr in 0..2 {
                        let other_fields = variants[1 - discr].iter().map(|ty| {
                            ty.layout(infcx)
                        });
                        if !Struct::would_be_zero_sized(dl, other_fields)? {
                            continue;
                        }
                        let path = Struct::non_zero_field_path(infcx,
                            variants[discr].iter().cloned())?;
                        let mut path = if let Some(p) = path { p } else { continue };

                        // FIXME(eddyb) should take advantage of a newtype.
                        if path == &[0] && variants[discr].len() == 1 {
                            match *variants[discr][0].layout(infcx)? {
                                Scalar { value, .. } => {
                                    return Ok(RawNullablePointer {
                                        nndiscr: discr as u64,
                                        value: value
                                    });
                                }
                                _ => {
                                    bug!("Layout::compute: `{}`'s non-zero \
                                          `{}` field not scalar?!",
                                         ty, variants[discr][0])
                                }
                            }
                        }

                        path.push(0); // For GEP through a pointer.
                        path.reverse();
                        let mut st = Struct::new(dl, false);
                        st.extend(dl, variants[discr].iter().map(|ty| {
                            ty.layout(infcx)
                        }), ty)?;
                        return Ok(StructWrappedNullablePointer {
                            nndiscr: discr as u64,
                            nonnull: st,
                            discrfield: path
                        });
                    }
                }

                // The general case.
                let discr_max = (variants.len() - 1) as i64;
                assert!(discr_max >= 0);
                let (min_ity, _) = Integer::repr_discr(tcx, hint, 0, discr_max);

                let mut align = dl.aggregate_align;
                let mut size = Size::from_bytes(0);

                // We're interested in the smallest alignment, so start large.
                let mut start_align = Align::from_bytes(256, 256).unwrap();

                // Create the set of structs that represent each variant
                // Use the minimum integer type we figured out above
                let discr = Some(Scalar { value: Int(min_ity), non_zero: false });
                let mut variants = variants.into_iter().map(|fields| {
                    let mut found_start = false;
                    let fields = fields.into_iter().map(|field| {
                        let field = field.layout(infcx)?;
                        if !found_start {
                            // Find the first field we can't move later
                            // to make room for a larger discriminant.
                            let field_align = field.align(dl);
                            if field.size(dl).bytes() != 0 || field_align.abi() != 1 {
                                start_align = start_align.min(field_align);
                                found_start = true;
                            }
                        }
                        Ok(field)
                    });
                    let mut st = Struct::new(dl, false);
                    st.extend(dl, discr.iter().map(Ok).chain(fields)
                                              .chain(drop_flag.iter().map(Ok)), ty)?;
                    size = cmp::max(size, st.min_size());
                    align = align.max(st.align);
                    Ok(st)
                }).collect::<Result<Vec<_>, _>>()?;

                // Align the maximum variant size to the largest alignment.
                size = size.abi_align(align);

                if size.bytes() >= dl.obj_size_bound() {
                    return Err(LayoutError::SizeOverflow(ty));
                }

                // Check to see if we should use a different type for the
                // discriminant. We can safely use a type with the same size
                // as the alignment of the first field of each variant.
                // We increase the size of the discriminant to avoid LLVM copying
                // padding when it doesn't need to. This normally causes unaligned
                // load/stores and excessive memcpy/memset operations. By using a
                // bigger integer size, LLVM can be sure about it's contents and
                // won't be so conservative.

                // Use the initial field alignment
                let wanted = start_align.abi();
                let mut ity = min_ity;
                for &candidate in &[I16, I32, I64] {
                    let ty = Int(candidate);
                    if wanted == ty.align(dl).abi() && wanted == ty.size(dl).bytes() {
                        ity = candidate;
                        break;
                    }
                }

                // FIXME(eddyb) conservative only to avoid diverging from trans::adt.
                if align.abi() != start_align.abi() {
                    ity = min_ity;
                }

                // If the alignment is not larger than the chosen discriminant size,
                // don't use the alignment as the final size.
                if ity <= min_ity {
                    ity = min_ity;
                } else {
                    // Patch up the variants' first few fields.
                    let old_ity_size = Int(min_ity).size(dl);
                    let new_ity_size = Int(ity).size(dl);
                    for variant in &mut variants {
                        for offset in &mut variant.offset_after_field {
                            if *offset > old_ity_size {
                                break;
                            }
                            *offset = new_ity_size;
                        }
                    }
                }

                General {
                    discr: ity,
                    variants: variants,
                    size: size,
                    align: align
                }
            }

            // Types with no meaningful known layout.
            ty::TyProjection(_) | ty::TyParam(_) => {
                return Err(LayoutError::Unknown(ty));
            }
            ty::TyInfer(_) | ty::TyError => {
                bug!("Layout::compute: unexpected type `{}`", ty)
            }
        };

        Ok(layout)
    }

    /// Returns true if the layout corresponds to an unsized type.
    pub fn is_unsized(&self) -> bool {
        match *self {
            Scalar {..} | Vector {..} | FatPointer {..} |
            CEnum {..} | General {..} |
            RawNullablePointer {..} |
            StructWrappedNullablePointer {..} => false,

            Array { sized, .. } |
            Univariant { variant: Struct { sized, .. }, .. } => !sized
        }
    }

    pub fn size(&self, dl: &TargetDataLayout) -> Size {
        match *self {
            Scalar { value, .. } | RawNullablePointer { value, .. } => {
                value.size(dl)
            }

            Vector { element, count } => {
                let elem_size = element.size(dl);
                let vec_size = match elem_size.checked_mul(count, dl) {
                    Some(size) => size,
                    None => bug!("Layout::size({:?}): {} * {} overflowed",
                                 self, elem_size.bytes(), count)
                };
                vec_size.abi_align(self.align(dl))
            }

            FatPointer { metadata, .. } => {
                // Effectively a (ptr, meta) tuple.
                Pointer.size(dl).abi_align(metadata.align(dl))
                       .checked_add(metadata.size(dl), dl).unwrap()
                       .abi_align(self.align(dl))
            }

            CEnum { discr, .. } => Int(discr).size(dl),
            Array { size, .. } | General { size, .. } => size,

            Univariant { ref variant, .. } |
            StructWrappedNullablePointer { nonnull: ref variant, .. } => {
                variant.stride()
            }
        }
    }

    pub fn align(&self, dl: &TargetDataLayout) -> Align {
        match *self {
            Scalar { value, .. } | RawNullablePointer { value, .. } => {
                value.align(dl)
            }

            Vector { element, count } => {
                let elem_size = element.size(dl);
                let vec_size = match elem_size.checked_mul(count, dl) {
                    Some(size) => size,
                    None => bug!("Layout::align({:?}): {} * {} overflowed",
                                 self, elem_size.bytes(), count)
                };
                for &(size, align) in &dl.vector_align {
                    if size == vec_size {
                        return align;
                    }
                }
                // Default to natural alignment, which is what LLVM does.
                // That is, use the size, rounded up to a power of 2.
                let align = vec_size.bytes().next_power_of_two();
                Align::from_bytes(align, align).unwrap()
            }

            FatPointer { metadata, .. } => {
                // Effectively a (ptr, meta) tuple.
                Pointer.align(dl).max(metadata.align(dl))
            }

            CEnum { discr, .. } => Int(discr).align(dl),
            Array { align, .. } | General { align, .. } => align,

            Univariant { ref variant, .. } |
            StructWrappedNullablePointer { nonnull: ref variant, .. } => {
                variant.align
            }
        }
    }
}

/// Type size "skeleton", i.e. the only information determining a type's size.
/// While this is conservative, (aside from constant sizes, only pointers,
/// newtypes thereof and null pointer optimized enums are allowed), it is
/// enough to statically check common usecases of transmute.
#[derive(Copy, Clone, Debug)]
pub enum SizeSkeleton<'tcx> {
    /// Any statically computable Layout.
    Known(Size),

    /// A potentially-fat pointer.
    Pointer {
        // If true, this pointer is never null.
        non_zero: bool,
        // The type which determines the unsized metadata, if any,
        // of this pointer. Either a type parameter or a projection
        // depending on one, with regions erased.
        tail: Ty<'tcx>
    }
}

impl<'a, 'gcx, 'tcx> SizeSkeleton<'gcx> {
    pub fn compute(ty: Ty<'gcx>, infcx: &InferCtxt<'a, 'gcx, 'tcx>)
                   -> Result<SizeSkeleton<'gcx>, LayoutError<'gcx>> {
        let tcx = infcx.tcx.global_tcx();
        assert!(!ty.has_infer_types());

        // First try computing a static layout.
        let err = match ty.layout(infcx) {
            Ok(layout) => {
                return Ok(SizeSkeleton::Known(layout.size(&tcx.data_layout)));
            }
            Err(err) => err
        };

        match ty.sty {
            ty::TyBox(pointee) |
            ty::TyRef(_, ty::TypeAndMut { ty: pointee, .. }) |
            ty::TyRawPtr(ty::TypeAndMut { ty: pointee, .. }) => {
                let non_zero = !ty.is_unsafe_ptr();
                let tail = tcx.struct_tail(pointee);
                match tail.sty {
                    ty::TyParam(_) | ty::TyProjection(_) => {
                        assert!(tail.has_param_types() || tail.has_self_ty());
                        Ok(SizeSkeleton::Pointer {
                            non_zero: non_zero,
                            tail: tcx.erase_regions(&tail)
                        })
                    }
                    _ => {
                        bug!("SizeSkeleton::compute({}): layout errored ({}), yet \
                              tail `{}` is not a type parameter or a projection",
                             ty, err, tail)
                    }
                }
            }

            ty::TyStruct(def, substs) | ty::TyEnum(def, substs) => {
                // Only newtypes and enums w/ nullable pointer optimization.
                if def.variants.is_empty() || def.variants.len() > 2 {
                    return Err(err);
                }

                // If there's a drop flag, it can't be just a pointer.
                if def.dtor_kind().has_drop_flag() {
                    return Err(err);
                }

                // Get a zero-sized variant or a pointer newtype.
                let zero_or_ptr_variant = |i: usize| {
                    let fields = def.variants[i].fields.iter().map(|field| {
                        let ty = normalize_associated_type(infcx, &field.ty(tcx, substs));
                        SizeSkeleton::compute(ty, infcx)
                    });
                    let mut ptr = None;
                    for field in fields {
                        let field = field?;
                        match field {
                            SizeSkeleton::Known(size) => {
                                if size.bytes() > 0 {
                                    return Err(err);
                                }
                            }
                            SizeSkeleton::Pointer {..} => {
                                if ptr.is_some() {
                                    return Err(err);
                                }
                                ptr = Some(field);
                            }
                        }
                    }
                    Ok(ptr)
                };

                let v0 = zero_or_ptr_variant(0)?;
                // Newtype.
                if def.variants.len() == 1 {
                    if let Some(SizeSkeleton::Pointer { non_zero, tail }) = v0 {
                        return Ok(SizeSkeleton::Pointer {
                            non_zero: non_zero ||
                                Some(def.did) == tcx.lang_items.non_zero(),
                            tail: tail
                        });
                    } else {
                        return Err(err);
                    }
                }

                let v1 = zero_or_ptr_variant(1)?;
                // Nullable pointer enum optimization.
                match (v0, v1) {
                    (Some(SizeSkeleton::Pointer { non_zero: true, tail }), None) |
                    (None, Some(SizeSkeleton::Pointer { non_zero: true, tail })) => {
                        Ok(SizeSkeleton::Pointer {
                            non_zero: false,
                            tail: tail
                        })
                    }
                    _ => Err(err)
                }
            }

            _ => Err(err)
        }
    }

    pub fn same_size(self, other: SizeSkeleton) -> bool {
        match (self, other) {
            (SizeSkeleton::Known(a), SizeSkeleton::Known(b)) => a == b,
            (SizeSkeleton::Pointer { tail: a, .. },
             SizeSkeleton::Pointer { tail: b, .. }) => a == b,
            _ => false
        }
    }
}
