// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! **Canonicalization** is the key to constructing a query in the
//! middle of type inference. Ordinarily, it is not possible to store
//! types from type inference in query keys, because they contain
//! references to inference variables whose lifetimes are too short
//! and so forth. Canonicalizing a value T1 using `canonicalize_query`
//! produces two things:
//!
//! - a value T2 where each unbound inference variable has been
//!   replaced with a **canonical variable**;
//! - a map M (of type `CanonicalVarValues`) from those canonical
//!   variables back to the original.
//!
//! We can then do queries using T2. These will give back constriants
//! on the canonical variables which can be translated, using the map
//! M, into constraints in our source context. This process of
//! translating the results back is done by the
//! `instantiate_query_result` method.
//!
//! For a more detailed look at what is happening here, check
//! out the [chapter in the rustc guide][c].
//!
//! [c]: https://rust-lang-nursery.github.io/rustc-guide/traits/canonicalization.html

use infer::{InferCtxt, RegionVariableOrigin, TypeVariableOrigin};
use rustc_data_structures::indexed_vec::IndexVec;
use smallvec::SmallVec;
use rustc_data_structures::sync::Lrc;
use serialize::UseSpecializedDecodable;
use std::ops::Index;
use syntax::source_map::Span;
use ty::fold::TypeFoldable;
use ty::subst::Kind;
use ty::{self, CanonicalVar, Lift, Region, List, TyCtxt};

mod canonicalizer;

pub mod query_response;

mod substitute;

/// A "canonicalized" type `V` is one where all free inference
/// variables have been rewritten to "canonical vars". These are
/// numbered starting from 0 in order of first appearance.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, RustcDecodable, RustcEncodable)]
pub struct Canonical<'gcx, V> {
    pub variables: CanonicalVarInfos<'gcx>,
    pub value: V,
}

pub type CanonicalVarInfos<'gcx> = &'gcx List<CanonicalVarInfo>;

impl<'gcx> UseSpecializedDecodable for CanonicalVarInfos<'gcx> {}

/// A set of values corresponding to the canonical variables from some
/// `Canonical`. You can give these values to
/// `canonical_value.substitute` to substitute them into the canonical
/// value at the right places.
///
/// When you canonicalize a value `V`, you get back one of these
/// vectors with the original values that were replaced by canonical
/// variables. You will need to supply it later to instantiate the
/// canonicalized query response.
#[derive(Clone, Debug, PartialEq, Eq, Hash, RustcDecodable, RustcEncodable)]
pub struct CanonicalVarValues<'tcx> {
    pub var_values: IndexVec<CanonicalVar, Kind<'tcx>>,
}

/// When we canonicalize a value to form a query, we wind up replacing
/// various parts of it with canonical variables. This struct stores
/// those replaced bits to remember for when we process the query
/// result.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, RustcDecodable, RustcEncodable)]
pub struct OriginalQueryValues<'tcx> {
    /// This is equivalent to `CanonicalVarValues`, but using a
    /// `SmallVec` yields a significant performance win.
    pub var_values: SmallVec<[Kind<'tcx>; 8]>,
}

/// Information about a canonical variable that is included with the
/// canonical value. This is sufficient information for code to create
/// a copy of the canonical value in some other inference context,
/// with fresh inference variables replacing the canonical values.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, RustcDecodable, RustcEncodable)]
pub struct CanonicalVarInfo {
    pub kind: CanonicalVarKind,
}

/// Describes the "kind" of the canonical variable. This is a "kind"
/// in the type-theory sense of the term -- i.e., a "meta" type system
/// that analyzes type-like values.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, RustcDecodable, RustcEncodable)]
pub enum CanonicalVarKind {
    /// Some kind of type inference variable.
    Ty(CanonicalTyVarKind),

    /// Region variable `'?R`.
    Region,
}

/// Rust actually has more than one category of type variables;
/// notably, the type variables we create for literals (e.g., 22 or
/// 22.) can only be instantiated with integral/float types (e.g.,
/// usize or f32). In order to faithfully reproduce a type, we need to
/// know what set of types a given type variable can be unified with.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, RustcDecodable, RustcEncodable)]
pub enum CanonicalTyVarKind {
    /// General type variable `?T` that can be unified with arbitrary types.
    General,

    /// Integral type variable `?I` (that can only be unified with integral types).
    Int,

    /// Floating-point type variable `?F` (that can only be unified with float types).
    Float,
}

/// After we execute a query with a canonicalized key, we get back a
/// `Canonical<QueryResponse<..>>`. You can use
/// `instantiate_query_result` to access the data in this result.
#[derive(Clone, Debug)]
pub struct QueryResponse<'tcx, R> {
    pub var_values: CanonicalVarValues<'tcx>,
    pub region_constraints: Vec<QueryRegionConstraint<'tcx>>,
    pub certainty: Certainty,
    pub value: R,
}

pub type Canonicalized<'gcx, V> = Canonical<'gcx, <V as Lift<'gcx>>::Lifted>;

pub type CanonicalizedQueryResponse<'gcx, T> =
    Lrc<Canonical<'gcx, QueryResponse<'gcx, <T as Lift<'gcx>>::Lifted>>>;

/// Indicates whether or not we were able to prove the query to be
/// true.
#[derive(Copy, Clone, Debug)]
pub enum Certainty {
    /// The query is known to be true, presuming that you apply the
    /// given `var_values` and the region-constraints are satisfied.
    Proven,

    /// The query is not known to be true, but also not known to be
    /// false. The `var_values` represent *either* values that must
    /// hold in order for the query to be true, or helpful tips that
    /// *might* make it true. Currently rustc's trait solver cannot
    /// distinguish the two (e.g., due to our preference for where
    /// clauses over impls).
    ///
    /// After some unifiations and things have been done, it makes
    /// sense to try and prove again -- of course, at that point, the
    /// canonical form will be different, making this a distinct
    /// query.
    Ambiguous,
}

impl Certainty {
    pub fn is_proven(&self) -> bool {
        match self {
            Certainty::Proven => true,
            Certainty::Ambiguous => false,
        }
    }

    pub fn is_ambiguous(&self) -> bool {
        !self.is_proven()
    }
}

impl<'tcx, R> QueryResponse<'tcx, R> {
    pub fn is_proven(&self) -> bool {
        self.certainty.is_proven()
    }

    pub fn is_ambiguous(&self) -> bool {
        !self.is_proven()
    }
}

impl<'tcx, R> Canonical<'tcx, QueryResponse<'tcx, R>> {
    pub fn is_proven(&self) -> bool {
        self.value.is_proven()
    }

    pub fn is_ambiguous(&self) -> bool {
        !self.is_proven()
    }
}

impl<'gcx, V> Canonical<'gcx, V> {
    /// Allows you to map the `value` of a canonical while keeping the
    /// same set of bound variables.
    ///
    /// **WARNING:** This function is very easy to mis-use, hence the
    /// name!  In particular, the new value `W` must use all **the
    /// same type/region variables** in **precisely the same order**
    /// as the original! (The ordering is defined by the
    /// `TypeFoldable` implementation of the type in question.)
    ///
    /// An example of a **correct** use of this:
    ///
    /// ```rust,ignore (not real code)
    /// let a: Canonical<'_, T> = ...;
    /// let b: Canonical<'_, (T,)> = a.unchecked_map(|v| (v, ));
    /// ```
    ///
    /// An example of an **incorrect** use of this:
    ///
    /// ```rust,ignore (not real code)
    /// let a: Canonical<'tcx, T> = ...;
    /// let ty: Ty<'tcx> = ...;
    /// let b: Canonical<'tcx, (T, Ty<'tcx>)> = a.unchecked_map(|v| (v, ty));
    /// ```
    pub fn unchecked_map<W>(self, map_op: impl FnOnce(V) -> W) -> Canonical<'gcx, W> {
        let Canonical { variables, value } = self;
        Canonical { variables, value: map_op(value) }
    }
}

pub type QueryRegionConstraint<'tcx> = ty::Binder<ty::OutlivesPredicate<Kind<'tcx>, Region<'tcx>>>;

impl<'cx, 'gcx, 'tcx> InferCtxt<'cx, 'gcx, 'tcx> {
    /// Creates a substitution S for the canonical value with fresh
    /// inference variables and applies it to the canonical value.
    /// Returns both the instantiated result *and* the substitution S.
    ///
    /// This is only meant to be invoked as part of constructing an
    /// inference context at the start of a query (see
    /// `InferCtxtBuilder::enter_with_canonical`).  It basically
    /// brings the canonical value "into scope" within your new infcx.
    ///
    /// At the end of processing, the substitution S (once
    /// canonicalized) then represents the values that you computed
    /// for each of the canonical inputs to your query.

    pub(in infer) fn instantiate_canonical_with_fresh_inference_vars<T>(
        &self,
        span: Span,
        canonical: &Canonical<'tcx, T>,
    ) -> (T, CanonicalVarValues<'tcx>)
    where
        T: TypeFoldable<'tcx>,
    {
        assert_eq!(self.universe(), ty::UniverseIndex::ROOT, "infcx not newly created");
        assert_eq!(self.type_variables.borrow().num_vars(), 0, "infcx not newly created");

        let canonical_inference_vars =
            self.fresh_inference_vars_for_canonical_vars(span, canonical.variables);
        let result = canonical.substitute(self.tcx, &canonical_inference_vars);
        (result, canonical_inference_vars)
    }

    /// Given the "infos" about the canonical variables from some
    /// canonical, creates fresh inference variables with the same
    /// characteristics. You can then use `substitute` to instantiate
    /// the canonical variable with these inference variables.
    fn fresh_inference_vars_for_canonical_vars(
        &self,
        span: Span,
        variables: &List<CanonicalVarInfo>,
    ) -> CanonicalVarValues<'tcx> {
        let var_values: IndexVec<CanonicalVar, Kind<'tcx>> = variables
            .iter()
            .map(|info| self.fresh_inference_var_for_canonical_var(span, *info))
            .collect();

        CanonicalVarValues { var_values }
    }

    /// Given the "info" about a canonical variable, creates a fresh
    /// inference variable with the same characteristics.
    fn fresh_inference_var_for_canonical_var(
        &self,
        span: Span,
        cv_info: CanonicalVarInfo,
    ) -> Kind<'tcx> {
        match cv_info.kind {
            CanonicalVarKind::Ty(ty_kind) => {
                let ty = match ty_kind {
                    CanonicalTyVarKind::General => {
                        self.next_ty_var(TypeVariableOrigin::MiscVariable(span))
                    }

                    CanonicalTyVarKind::Int => self.tcx.mk_int_var(self.next_int_var_id()),

                    CanonicalTyVarKind::Float => self.tcx.mk_float_var(self.next_float_var_id()),
                };
                ty.into()
            }

            CanonicalVarKind::Region => self
                .next_region_var(RegionVariableOrigin::MiscVariable(span))
                .into(),
        }
    }
}

CloneTypeFoldableAndLiftImpls! {
    ::infer::canonical::Certainty,
    ::infer::canonical::CanonicalVarInfo,
    ::infer::canonical::CanonicalVarKind,
}

CloneTypeFoldableImpls! {
    for <'tcx> {
        ::infer::canonical::CanonicalVarInfos<'tcx>,
    }
}

BraceStructTypeFoldableImpl! {
    impl<'tcx, C> TypeFoldable<'tcx> for Canonical<'tcx, C> {
        variables,
        value,
    } where C: TypeFoldable<'tcx>
}

BraceStructLiftImpl! {
    impl<'a, 'tcx, T> Lift<'tcx> for Canonical<'a, T> {
        type Lifted = Canonical<'tcx, T::Lifted>;
        variables, value
    } where T: Lift<'tcx>
}

impl<'tcx> CanonicalVarValues<'tcx> {
    fn len(&self) -> usize {
        self.var_values.len()
    }
}

impl<'a, 'tcx> IntoIterator for &'a CanonicalVarValues<'tcx> {
    type Item = Kind<'tcx>;
    type IntoIter = ::std::iter::Cloned<::std::slice::Iter<'a, Kind<'tcx>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.var_values.iter().cloned()
    }
}

BraceStructLiftImpl! {
    impl<'a, 'tcx> Lift<'tcx> for CanonicalVarValues<'a> {
        type Lifted = CanonicalVarValues<'tcx>;
        var_values,
    }
}

BraceStructTypeFoldableImpl! {
    impl<'tcx> TypeFoldable<'tcx> for CanonicalVarValues<'tcx> {
        var_values,
    }
}

BraceStructTypeFoldableImpl! {
    impl<'tcx, R> TypeFoldable<'tcx> for QueryResponse<'tcx, R> {
        var_values, region_constraints, certainty, value
    } where R: TypeFoldable<'tcx>,
}

BraceStructLiftImpl! {
    impl<'a, 'tcx, R> Lift<'tcx> for QueryResponse<'a, R> {
        type Lifted = QueryResponse<'tcx, R::Lifted>;
        var_values, region_constraints, certainty, value
    } where R: Lift<'tcx>
}

impl<'tcx> Index<CanonicalVar> for CanonicalVarValues<'tcx> {
    type Output = Kind<'tcx>;

    fn index(&self, value: CanonicalVar) -> &Kind<'tcx> {
        &self.var_values[value]
    }
}
