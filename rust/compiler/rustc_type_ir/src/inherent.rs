//! Set of traits which are used to emulate the inherent impls that are present in `rustc_middle`.
//! It is customary to glob-import `rustc_type_ir::inherent::*` to bring all of these traits into
//! scope when programming in interner-agnostic settings, and to avoid importing any of these
//! directly elsewhere (i.e. specify the full path for an implementation downstream).

use std::fmt::Debug;
use std::hash::Hash;
use std::ops::Deref;

use crate::fold::TypeSuperFoldable;
use crate::visit::{Flags, TypeSuperVisitable};
use crate::{
    AliasTy, AliasTyKind, BoundVar, ConstKind, DebruijnIndex, DebugWithInfcx, Interner, RegionKind,
    TyKind, UnevaluatedConst, UniverseIndex,
};

pub trait Ty<I: Interner<Ty = Self>>:
    Copy
    + DebugWithInfcx<I>
    + Hash
    + Eq
    + Into<I::GenericArg>
    + Into<I::Term>
    + IntoKind<Kind = TyKind<I>>
    + TypeSuperVisitable<I>
    + TypeSuperFoldable<I>
    + Flags
{
    fn new_bool(interner: I) -> Self;

    fn new_anon_bound(interner: I, debruijn: DebruijnIndex, var: BoundVar) -> Self;

    fn new_alias(interner: I, kind: AliasTyKind, alias_ty: AliasTy<I>) -> Self;
}

pub trait Tys<I: Interner<Tys = Self>>:
    Copy + Debug + Hash + Eq + IntoIterator<Item = I::Ty> + Deref<Target: Deref<Target = [I::Ty]>>
{
    fn split_inputs_and_output(self) -> (I::FnInputTys, I::Ty);
}

pub trait Abi<I: Interner<Abi = Self>>: Copy + Debug + Hash + Eq {
    /// Whether this ABI is `extern "Rust"`.
    fn is_rust(self) -> bool;
}

pub trait Safety<I: Interner<Safety = Self>>: Copy + Debug + Hash + Eq {
    fn prefix_str(self) -> &'static str;
}

pub trait Region<I: Interner<Region = Self>>:
    Copy + DebugWithInfcx<I> + Hash + Eq + Into<I::GenericArg> + IntoKind<Kind = RegionKind<I>> + Flags
{
    fn new_anon_bound(interner: I, debruijn: DebruijnIndex, var: BoundVar) -> Self;

    fn new_static(interner: I) -> Self;
}

pub trait Const<I: Interner<Const = Self>>:
    Copy
    + DebugWithInfcx<I>
    + Hash
    + Eq
    + Into<I::GenericArg>
    + Into<I::Term>
    + IntoKind<Kind = ConstKind<I>>
    + TypeSuperVisitable<I>
    + TypeSuperFoldable<I>
    + Flags
{
    fn new_anon_bound(interner: I, debruijn: DebruijnIndex, var: BoundVar, ty: I::Ty) -> Self;

    fn new_unevaluated(interner: I, uv: UnevaluatedConst<I>, ty: I::Ty) -> Self;

    fn ty(self) -> I::Ty;
}

pub trait GenericsOf<I: Interner<GenericsOf = Self>> {
    fn count(&self) -> usize;
}

pub trait GenericArgs<I: Interner<GenericArgs = Self>>:
    Copy
    + DebugWithInfcx<I>
    + Hash
    + Eq
    + IntoIterator<Item = I::GenericArg>
    + Deref<Target: Deref<Target = [I::GenericArg]>>
    + Default
{
    fn type_at(self, i: usize) -> I::Ty;

    fn identity_for_item(interner: I, def_id: I::DefId) -> I::GenericArgs;
}

pub trait Predicate<I: Interner<Predicate = Self>>:
    Copy + Debug + Hash + Eq + TypeSuperVisitable<I> + TypeSuperFoldable<I> + Flags
{
}

/// Common capabilities of placeholder kinds
pub trait PlaceholderLike: Copy + Debug + Hash + Eq {
    fn universe(self) -> UniverseIndex;
    fn var(self) -> BoundVar;

    fn with_updated_universe(self, ui: UniverseIndex) -> Self;

    fn new(ui: UniverseIndex, var: BoundVar) -> Self;
}

pub trait IntoKind {
    type Kind;

    fn kind(self) -> Self::Kind;
}

pub trait BoundVars<I: Interner> {
    fn bound_vars(&self) -> I::BoundVars;

    fn has_no_bound_vars(&self) -> bool;
}

pub trait BoundVarLike<I: Interner> {
    fn var(self) -> BoundVar;
}
