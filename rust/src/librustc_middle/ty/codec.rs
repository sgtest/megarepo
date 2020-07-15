// This module contains some shared code for encoding and decoding various
// things from the `ty` module, and in particular implements support for
// "shorthands" which allow to have pointers back into the already encoded
// stream instead of re-encoding the same thing twice.
//
// The functionality in here is shared between persisting to crate metadata and
// persisting to incr. comp. caches.

use crate::arena::ArenaAllocatable;
use crate::infer::canonical::{CanonicalVarInfo, CanonicalVarInfos};
use crate::mir::{self, interpret::Allocation};
use crate::ty::subst::SubstsRef;
use crate::ty::{self, List, Ty, TyCtxt};
use rustc_data_structures::fx::FxHashMap;
use rustc_hir::def_id::{CrateNum, DefId};
use rustc_serialize::{opaque, Decodable, Decoder, Encodable, Encoder};
use rustc_span::Span;
use std::convert::{TryFrom, TryInto};
use std::hash::Hash;
use std::intrinsics;
use std::marker::DiscriminantKind;

/// The shorthand encoding uses an enum's variant index `usize`
/// and is offset by this value so it never matches a real variant.
/// This offset is also chosen so that the first byte is never < 0x80.
pub const SHORTHAND_OFFSET: usize = 0x80;

pub trait EncodableWithShorthand: Clone + Eq + Hash {
    type Variant: Encodable;
    fn variant(&self) -> &Self::Variant;
}

#[allow(rustc::usage_of_ty_tykind)]
impl<'tcx> EncodableWithShorthand for Ty<'tcx> {
    type Variant = ty::TyKind<'tcx>;
    fn variant(&self) -> &Self::Variant {
        &self.kind
    }
}

impl<'tcx> EncodableWithShorthand for ty::Predicate<'tcx> {
    type Variant = ty::PredicateKind<'tcx>;
    fn variant(&self) -> &Self::Variant {
        self.kind()
    }
}

pub trait TyEncoder: Encoder {
    fn position(&self) -> usize;
}

impl TyEncoder for opaque::Encoder {
    #[inline]
    fn position(&self) -> usize {
        self.position()
    }
}

/// Encode the given value or a previously cached shorthand.
pub fn encode_with_shorthand<E, T, M>(encoder: &mut E, value: &T, cache: M) -> Result<(), E::Error>
where
    E: TyEncoder,
    M: for<'b> Fn(&'b mut E) -> &'b mut FxHashMap<T, usize>,
    T: EncodableWithShorthand,
    <T::Variant as DiscriminantKind>::Discriminant: Ord + TryFrom<usize>,
{
    let existing_shorthand = cache(encoder).get(value).cloned();
    if let Some(shorthand) = existing_shorthand {
        return encoder.emit_usize(shorthand);
    }

    let variant = value.variant();

    let start = encoder.position();
    variant.encode(encoder)?;
    let len = encoder.position() - start;

    // The shorthand encoding uses the same usize as the
    // discriminant, with an offset so they can't conflict.
    let discriminant = intrinsics::discriminant_value(variant);
    assert!(discriminant < SHORTHAND_OFFSET.try_into().ok().unwrap());

    let shorthand = start + SHORTHAND_OFFSET;

    // Get the number of bits that leb128 could fit
    // in the same space as the fully encoded type.
    let leb128_bits = len * 7;

    // Check that the shorthand is a not longer than the
    // full encoding itself, i.e., it's an obvious win.
    if leb128_bits >= 64 || (shorthand as u64) < (1 << leb128_bits) {
        cache(encoder).insert(value.clone(), shorthand);
    }

    Ok(())
}

pub trait TyDecoder<'tcx>: Decoder {
    fn tcx(&self) -> TyCtxt<'tcx>;

    fn peek_byte(&self) -> u8;

    fn position(&self) -> usize;

    fn cached_ty_for_shorthand<F>(
        &mut self,
        shorthand: usize,
        or_insert_with: F,
    ) -> Result<Ty<'tcx>, Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<Ty<'tcx>, Self::Error>;

    fn cached_predicate_for_shorthand<F>(
        &mut self,
        shorthand: usize,
        or_insert_with: F,
    ) -> Result<ty::Predicate<'tcx>, Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<ty::Predicate<'tcx>, Self::Error>;

    fn with_position<F, R>(&mut self, pos: usize, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R;

    fn map_encoded_cnum_to_current(&self, cnum: CrateNum) -> CrateNum;

    fn positioned_at_shorthand(&self) -> bool {
        (self.peek_byte() & (SHORTHAND_OFFSET as u8)) != 0
    }
}

#[inline]
pub fn decode_arena_allocable<'tcx, D, T: ArenaAllocatable<'tcx> + Decodable>(
    decoder: &mut D,
) -> Result<&'tcx T, D::Error>
where
    D: TyDecoder<'tcx>,
{
    Ok(decoder.tcx().arena.alloc(Decodable::decode(decoder)?))
}

#[inline]
pub fn decode_arena_allocable_slice<'tcx, D, T: ArenaAllocatable<'tcx> + Decodable>(
    decoder: &mut D,
) -> Result<&'tcx [T], D::Error>
where
    D: TyDecoder<'tcx>,
{
    Ok(decoder.tcx().arena.alloc_from_iter(<Vec<T> as Decodable>::decode(decoder)?))
}

#[inline]
pub fn decode_cnum<D>(decoder: &mut D) -> Result<CrateNum, D::Error>
where
    D: TyDecoder<'tcx>,
{
    let cnum = CrateNum::from_u32(u32::decode(decoder)?);
    Ok(decoder.map_encoded_cnum_to_current(cnum))
}

#[allow(rustc::usage_of_ty_tykind)]
#[inline]
pub fn decode_ty<D>(decoder: &mut D) -> Result<Ty<'tcx>, D::Error>
where
    D: TyDecoder<'tcx>,
{
    // Handle shorthands first, if we have an usize > 0x80.
    if decoder.positioned_at_shorthand() {
        let pos = decoder.read_usize()?;
        assert!(pos >= SHORTHAND_OFFSET);
        let shorthand = pos - SHORTHAND_OFFSET;

        decoder.cached_ty_for_shorthand(shorthand, |decoder| {
            decoder.with_position(shorthand, Ty::decode)
        })
    } else {
        let tcx = decoder.tcx();
        Ok(tcx.mk_ty(ty::TyKind::decode(decoder)?))
    }
}

#[inline]
pub fn decode_predicate<D>(decoder: &mut D) -> Result<ty::Predicate<'tcx>, D::Error>
where
    D: TyDecoder<'tcx>,
{
    // Handle shorthands first, if we have an usize > 0x80.
    if decoder.positioned_at_shorthand() {
        let pos = decoder.read_usize()?;
        assert!(pos >= SHORTHAND_OFFSET);
        let shorthand = pos - SHORTHAND_OFFSET;

        decoder.cached_predicate_for_shorthand(shorthand, |decoder| {
            decoder.with_position(shorthand, ty::Predicate::decode)
        })
    } else {
        let tcx = decoder.tcx();
        Ok(tcx.mk_predicate(ty::PredicateKind::decode(decoder)?))
    }
}

#[inline]
pub fn decode_spanned_predicates<D>(
    decoder: &mut D,
) -> Result<&'tcx [(ty::Predicate<'tcx>, Span)], D::Error>
where
    D: TyDecoder<'tcx>,
{
    let tcx = decoder.tcx();
    Ok(tcx.arena.alloc_from_iter(
        (0..decoder.read_usize()?)
            .map(|_| Decodable::decode(decoder))
            .collect::<Result<Vec<_>, _>>()?,
    ))
}

#[inline]
pub fn decode_substs<D>(decoder: &mut D) -> Result<SubstsRef<'tcx>, D::Error>
where
    D: TyDecoder<'tcx>,
{
    let len = decoder.read_usize()?;
    let tcx = decoder.tcx();
    Ok(tcx.mk_substs((0..len).map(|_| Decodable::decode(decoder)))?)
}

#[inline]
pub fn decode_place<D>(decoder: &mut D) -> Result<mir::Place<'tcx>, D::Error>
where
    D: TyDecoder<'tcx>,
{
    let local: mir::Local = Decodable::decode(decoder)?;
    let len = decoder.read_usize()?;
    let projection: &'tcx List<mir::PlaceElem<'tcx>> =
        decoder.tcx().mk_place_elems((0..len).map(|_| Decodable::decode(decoder)))?;
    Ok(mir::Place { local, projection })
}

#[inline]
pub fn decode_region<D>(decoder: &mut D) -> Result<ty::Region<'tcx>, D::Error>
where
    D: TyDecoder<'tcx>,
{
    Ok(decoder.tcx().mk_region(Decodable::decode(decoder)?))
}

#[inline]
pub fn decode_ty_slice<D>(decoder: &mut D) -> Result<&'tcx ty::List<Ty<'tcx>>, D::Error>
where
    D: TyDecoder<'tcx>,
{
    let len = decoder.read_usize()?;
    Ok(decoder.tcx().mk_type_list((0..len).map(|_| Decodable::decode(decoder)))?)
}

#[inline]
pub fn decode_adt_def<D>(decoder: &mut D) -> Result<&'tcx ty::AdtDef, D::Error>
where
    D: TyDecoder<'tcx>,
{
    let def_id = DefId::decode(decoder)?;
    Ok(decoder.tcx().adt_def(def_id))
}

#[inline]
pub fn decode_symbol_name<D>(decoder: &mut D) -> Result<ty::SymbolName<'tcx>, D::Error>
where
    D: TyDecoder<'tcx>,
{
    Ok(ty::SymbolName::new(decoder.tcx(), &decoder.read_str()?))
}

#[inline]
pub fn decode_existential_predicate_slice<D>(
    decoder: &mut D,
) -> Result<&'tcx ty::List<ty::ExistentialPredicate<'tcx>>, D::Error>
where
    D: TyDecoder<'tcx>,
{
    let len = decoder.read_usize()?;
    Ok(decoder.tcx().mk_existential_predicates((0..len).map(|_| Decodable::decode(decoder)))?)
}

#[inline]
pub fn decode_canonical_var_infos<D>(decoder: &mut D) -> Result<CanonicalVarInfos<'tcx>, D::Error>
where
    D: TyDecoder<'tcx>,
{
    let len = decoder.read_usize()?;
    let interned: Result<Vec<CanonicalVarInfo>, _> =
        (0..len).map(|_| Decodable::decode(decoder)).collect();
    Ok(decoder.tcx().intern_canonical_var_infos(interned?.as_slice()))
}

#[inline]
pub fn decode_const<D>(decoder: &mut D) -> Result<&'tcx ty::Const<'tcx>, D::Error>
where
    D: TyDecoder<'tcx>,
{
    Ok(decoder.tcx().mk_const(Decodable::decode(decoder)?))
}

#[inline]
pub fn decode_allocation<D>(decoder: &mut D) -> Result<&'tcx Allocation, D::Error>
where
    D: TyDecoder<'tcx>,
{
    Ok(decoder.tcx().intern_const_alloc(Decodable::decode(decoder)?))
}

#[macro_export]
macro_rules! __impl_decoder_methods {
    ($($name:ident -> $ty:ty;)*) => {
        $(
            #[inline]
            fn $name(&mut self) -> Result<$ty, Self::Error> {
                self.opaque.$name()
            }
        )*
    }
}

#[macro_export]
macro_rules! impl_arena_allocatable_decoder {
    ([]$args:tt) => {};
    ([decode $(, $attrs:ident)*]
     [[$DecoderName:ident [$($typaram:tt),*]], [$name:ident: $ty:ty, $gen_ty:ty], $tcx:lifetime]) => {
         // FIXME(#36588): These impls are horribly unsound as they allow
         // the caller to pick any lifetime for `'tcx`, including `'static`.
        #[allow(unused_lifetimes)]
        impl<'_x, '_y, '_z, '_w, '_a, $($typaram),*> SpecializedDecoder<&'_a $gen_ty>
        for $DecoderName<$($typaram),*>
        where &'_a $gen_ty: UseSpecializedDecodable
        {
            #[inline]
            fn specialized_decode(&mut self) -> Result<&'_a $gen_ty, Self::Error> {
                unsafe {
                    std::mem::transmute::<
                        Result<&$tcx $ty, Self::Error>,
                        Result<&'_a $gen_ty, Self::Error>,
                    >(decode_arena_allocable(self))
                }
            }
        }

        #[allow(unused_lifetimes)]
        impl<'_x, '_y, '_z, '_w, '_a, $($typaram),*> SpecializedDecoder<&'_a [$gen_ty]>
        for $DecoderName<$($typaram),*>
        where &'_a [$gen_ty]: UseSpecializedDecodable
        {
            #[inline]
            fn specialized_decode(&mut self) -> Result<&'_a [$gen_ty], Self::Error> {
                unsafe {
                    std::mem::transmute::<
                        Result<&$tcx [$ty], Self::Error>,
                        Result<&'_a [$gen_ty], Self::Error>,
                    >(decode_arena_allocable_slice(self))
                }
            }
        }
    };
    ([$ignore:ident $(, $attrs:ident)*]$args:tt) => {
        impl_arena_allocatable_decoder!([$($attrs),*]$args);
    };
}

#[macro_export]
macro_rules! impl_arena_allocatable_decoders {
    ($args:tt, [$($a:tt $name:ident: $ty:ty, $gen_ty:ty;)*], $tcx:lifetime) => {
        $(
            impl_arena_allocatable_decoder!($a [$args, [$name: $ty, $gen_ty], $tcx]);
        )*
    }
}

#[macro_export]
macro_rules! implement_ty_decoder {
    ($DecoderName:ident <$($typaram:tt),*>) => {
        mod __ty_decoder_impl {
            use std::borrow::Cow;
            use std::mem::transmute;

            use rustc_serialize::{Decoder, SpecializedDecoder, UseSpecializedDecodable};

            use $crate::infer::canonical::CanonicalVarInfos;
            use $crate::ty;
            use $crate::ty::codec::*;
            use $crate::ty::subst::InternalSubsts;
            use rustc_hir::def_id::CrateNum;

            use rustc_span::Span;

            use super::$DecoderName;

            impl<$($typaram ),*> Decoder for $DecoderName<$($typaram),*> {
                type Error = String;

                __impl_decoder_methods! {
                    read_nil -> ();

                    read_u128 -> u128;
                    read_u64 -> u64;
                    read_u32 -> u32;
                    read_u16 -> u16;
                    read_u8 -> u8;
                    read_usize -> usize;

                    read_i128 -> i128;
                    read_i64 -> i64;
                    read_i32 -> i32;
                    read_i16 -> i16;
                    read_i8 -> i8;
                    read_isize -> isize;

                    read_bool -> bool;
                    read_f64 -> f64;
                    read_f32 -> f32;
                    read_char -> char;
                    read_str -> Cow<'_, str>;
                }

                fn error(&mut self, err: &str) -> Self::Error {
                    self.opaque.error(err)
                }
            }

            // FIXME(#36588): These impls are horribly unsound as they allow
            // the caller to pick any lifetime for `'tcx`, including `'static`.

            arena_types!(impl_arena_allocatable_decoders, [$DecoderName [$($typaram),*]], 'tcx);

            impl<$($typaram),*> SpecializedDecoder<CrateNum>
            for $DecoderName<$($typaram),*> {
                fn specialized_decode(&mut self) -> Result<CrateNum, Self::Error> {
                    decode_cnum(self)
                }
            }

            impl<'_x, '_y, $($typaram),*> SpecializedDecoder<&'_x ty::TyS<'_y>>
            for $DecoderName<$($typaram),*>
            where &'_x ty::TyS<'_y>: UseSpecializedDecodable
            {
                fn specialized_decode(&mut self) -> Result<&'_x ty::TyS<'_y>, Self::Error> {
                    unsafe {
                        transmute::<
                            Result<ty::Ty<'tcx>, Self::Error>,
                            Result<&'_x ty::TyS<'_y>, Self::Error>,
                        >(decode_ty(self))
                    }
                }
            }

            impl<'_x, $($typaram),*> SpecializedDecoder<ty::Predicate<'_x>>
            for $DecoderName<$($typaram),*> {
                fn specialized_decode(&mut self) -> Result<ty::Predicate<'_x>, Self::Error> {
                    unsafe {
                        transmute::<
                            Result<ty::Predicate<'tcx>, Self::Error>,
                            Result<ty::Predicate<'_x>, Self::Error>,
                        >(decode_predicate(self))
                    }
                }
            }

            impl<'_x, '_y, $($typaram),*> SpecializedDecoder<&'_x [(ty::Predicate<'_y>, Span)]>
            for $DecoderName<$($typaram),*>
            where &'_x [(ty::Predicate<'_y>, Span)]: UseSpecializedDecodable {
                fn specialized_decode(&mut self)
                                      -> Result<&'_x [(ty::Predicate<'_y>, Span)], Self::Error>
                {
                    unsafe { transmute(decode_spanned_predicates(self)) }
                }
            }

            impl<'_x, '_y, $($typaram),*> SpecializedDecoder<&'_x InternalSubsts<'_y>>
            for $DecoderName<$($typaram),*>
            where &'_x InternalSubsts<'_y>: UseSpecializedDecodable {
                fn specialized_decode(&mut self) -> Result<&'_x InternalSubsts<'_y>, Self::Error> {
                    unsafe { transmute(decode_substs(self)) }
                }
            }

            impl<'_x, $($typaram),*> SpecializedDecoder<$crate::mir::Place<'_x>>
            for $DecoderName<$($typaram),*> {
                fn specialized_decode(
                    &mut self
                ) -> Result<$crate::mir::Place<'_x>, Self::Error> {
                    unsafe { transmute(decode_place(self)) }
                }
            }

            impl<'_x, $($typaram),*> SpecializedDecoder<ty::Region<'_x>>
            for $DecoderName<$($typaram),*> {
                fn specialized_decode(&mut self) -> Result<ty::Region<'_x>, Self::Error> {
                    unsafe { transmute(decode_region(self)) }
                }
            }

            impl<'_x, '_y, '_z, $($typaram),*> SpecializedDecoder<&'_x ty::List<&'_y ty::TyS<'_z>>>
            for $DecoderName<$($typaram),*>
            where &'_x ty::List<&'_y ty::TyS<'_z>>: UseSpecializedDecodable {
                fn specialized_decode(&mut self)
                                      -> Result<&'_x ty::List<&'_y ty::TyS<'_z>>, Self::Error> {
                    unsafe { transmute(decode_ty_slice(self)) }
                }
            }

            impl<'_x, $($typaram),*> SpecializedDecoder<&'_x ty::AdtDef>
            for $DecoderName<$($typaram),*> {
                fn specialized_decode(&mut self) -> Result<&'_x ty::AdtDef, Self::Error> {
                    unsafe { transmute(decode_adt_def(self)) }
                }
            }

            impl<'_x, $($typaram),*> SpecializedDecoder<ty::SymbolName<'_x>>
            for $DecoderName<$($typaram),*> {
                fn specialized_decode(&mut self) -> Result<ty::SymbolName<'_x>, Self::Error> {
                    unsafe { transmute(decode_symbol_name(self)) }
                }
            }

            impl<'_x, '_y, $($typaram),*> SpecializedDecoder<&'_x ty::List<ty::ExistentialPredicate<'_y>>>
            for $DecoderName<$($typaram),*>
            where &'_x ty::List<ty::ExistentialPredicate<'_y>>: UseSpecializedDecodable {
                fn specialized_decode(&mut self)
                    -> Result<&'_x ty::List<ty::ExistentialPredicate<'_y>>, Self::Error> {
                        unsafe { transmute(decode_existential_predicate_slice(self)) }
                }
            }

            impl<'_x, $($typaram),*> SpecializedDecoder<CanonicalVarInfos<'_x>>
                for $DecoderName<$($typaram),*> {
                fn specialized_decode(&mut self)
                    -> Result<CanonicalVarInfos<'_x>, Self::Error> {
                        unsafe { transmute(decode_canonical_var_infos(self)) }
                }
            }

            impl<'_x, '_y, $($typaram),*> SpecializedDecoder<&'_x $crate::ty::Const<'_y>>
            for $DecoderName<$($typaram),*>
            where &'_x $crate::ty::Const<'_y>: UseSpecializedDecodable {
                fn specialized_decode(&mut self) -> Result<&'_x ty::Const<'_y>, Self::Error> {
                    unsafe { transmute(decode_const(self)) }
                }
            }

            impl<'_x, $($typaram),*> SpecializedDecoder<&'_x $crate::mir::interpret::Allocation>
            for $DecoderName<$($typaram),*> {
                fn specialized_decode(
                    &mut self
                ) -> Result<&'_x $crate::mir::interpret::Allocation, Self::Error> {
                    unsafe { transmute(decode_allocation(self)) }
                }
            }
        }
    };
}
