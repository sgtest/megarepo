// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! misc. type-system utilities too small to deserve their own file

use hir::def_id::{DefId, LOCAL_CRATE};
use hir::map::DefPathData;
use infer::InferCtxt;
use ich::{StableHashingContext, NodeIdHashingMode};
use traits::{self, Reveal};
use ty::{self, Ty, TyCtxt, TypeAndMut, TypeFlags, TypeFoldable};
use ty::ParameterEnvironment;
use ty::fold::TypeVisitor;
use ty::layout::{Layout, LayoutError};
use ty::subst::{Subst, Kind};
use ty::TypeVariants::*;
use util::common::ErrorReported;
use util::nodemap::{FxHashMap, FxHashSet};
use middle::lang_items;

use rustc_const_math::{ConstInt, ConstIsize, ConstUsize};
use rustc_data_structures::stable_hasher::{StableHasher, StableHasherResult,
                                           HashStable};
use std::cell::RefCell;
use std::cmp;
use std::hash::Hash;
use std::intrinsics;
use syntax::ast::{self, Name};
use syntax::attr::{self, SignedInt, UnsignedInt};
use syntax_pos::{Span, DUMMY_SP};

use hir;

type Disr = ConstInt;

pub trait IntTypeExt {
    fn to_ty<'a, 'gcx, 'tcx>(&self, tcx: TyCtxt<'a, 'gcx, 'tcx>) -> Ty<'tcx>;
    fn disr_incr<'a, 'tcx>(&self, tcx: TyCtxt<'a, 'tcx, 'tcx>, val: Option<Disr>)
                           -> Option<Disr>;
    fn assert_ty_matches(&self, val: Disr);
    fn initial_discriminant<'a, 'tcx>(&self, tcx: TyCtxt<'a, 'tcx, 'tcx>) -> Disr;
}


macro_rules! typed_literal {
    ($tcx:expr, $ty:expr, $lit:expr) => {
        match $ty {
            SignedInt(ast::IntTy::I8)    => ConstInt::I8($lit),
            SignedInt(ast::IntTy::I16)   => ConstInt::I16($lit),
            SignedInt(ast::IntTy::I32)   => ConstInt::I32($lit),
            SignedInt(ast::IntTy::I64)   => ConstInt::I64($lit),
            SignedInt(ast::IntTy::I128)   => ConstInt::I128($lit),
            SignedInt(ast::IntTy::Is) => match $tcx.sess.target.int_type {
                ast::IntTy::I16 => ConstInt::Isize(ConstIsize::Is16($lit)),
                ast::IntTy::I32 => ConstInt::Isize(ConstIsize::Is32($lit)),
                ast::IntTy::I64 => ConstInt::Isize(ConstIsize::Is64($lit)),
                _ => bug!(),
            },
            UnsignedInt(ast::UintTy::U8)  => ConstInt::U8($lit),
            UnsignedInt(ast::UintTy::U16) => ConstInt::U16($lit),
            UnsignedInt(ast::UintTy::U32) => ConstInt::U32($lit),
            UnsignedInt(ast::UintTy::U64) => ConstInt::U64($lit),
            UnsignedInt(ast::UintTy::U128) => ConstInt::U128($lit),
            UnsignedInt(ast::UintTy::Us) => match $tcx.sess.target.uint_type {
                ast::UintTy::U16 => ConstInt::Usize(ConstUsize::Us16($lit)),
                ast::UintTy::U32 => ConstInt::Usize(ConstUsize::Us32($lit)),
                ast::UintTy::U64 => ConstInt::Usize(ConstUsize::Us64($lit)),
                _ => bug!(),
            },
        }
    }
}

impl IntTypeExt for attr::IntType {
    fn to_ty<'a, 'gcx, 'tcx>(&self, tcx: TyCtxt<'a, 'gcx, 'tcx>) -> Ty<'tcx> {
        match *self {
            SignedInt(ast::IntTy::I8)      => tcx.types.i8,
            SignedInt(ast::IntTy::I16)     => tcx.types.i16,
            SignedInt(ast::IntTy::I32)     => tcx.types.i32,
            SignedInt(ast::IntTy::I64)     => tcx.types.i64,
            SignedInt(ast::IntTy::I128)     => tcx.types.i128,
            SignedInt(ast::IntTy::Is)   => tcx.types.isize,
            UnsignedInt(ast::UintTy::U8)    => tcx.types.u8,
            UnsignedInt(ast::UintTy::U16)   => tcx.types.u16,
            UnsignedInt(ast::UintTy::U32)   => tcx.types.u32,
            UnsignedInt(ast::UintTy::U64)   => tcx.types.u64,
            UnsignedInt(ast::UintTy::U128)   => tcx.types.u128,
            UnsignedInt(ast::UintTy::Us) => tcx.types.usize,
        }
    }

    fn initial_discriminant<'a, 'tcx>(&self, tcx: TyCtxt<'a, 'tcx, 'tcx>) -> Disr {
        typed_literal!(tcx, *self, 0)
    }

    fn assert_ty_matches(&self, val: Disr) {
        match (*self, val) {
            (SignedInt(ast::IntTy::I8), ConstInt::I8(_)) => {},
            (SignedInt(ast::IntTy::I16), ConstInt::I16(_)) => {},
            (SignedInt(ast::IntTy::I32), ConstInt::I32(_)) => {},
            (SignedInt(ast::IntTy::I64), ConstInt::I64(_)) => {},
            (SignedInt(ast::IntTy::I128), ConstInt::I128(_)) => {},
            (SignedInt(ast::IntTy::Is), ConstInt::Isize(_)) => {},
            (UnsignedInt(ast::UintTy::U8), ConstInt::U8(_)) => {},
            (UnsignedInt(ast::UintTy::U16), ConstInt::U16(_)) => {},
            (UnsignedInt(ast::UintTy::U32), ConstInt::U32(_)) => {},
            (UnsignedInt(ast::UintTy::U64), ConstInt::U64(_)) => {},
            (UnsignedInt(ast::UintTy::U128), ConstInt::U128(_)) => {},
            (UnsignedInt(ast::UintTy::Us), ConstInt::Usize(_)) => {},
            _ => bug!("disr type mismatch: {:?} vs {:?}", self, val),
        }
    }

    fn disr_incr<'a, 'tcx>(&self, tcx: TyCtxt<'a, 'tcx, 'tcx>, val: Option<Disr>)
                           -> Option<Disr> {
        if let Some(val) = val {
            self.assert_ty_matches(val);
            (val + typed_literal!(tcx, *self, 1)).ok()
        } else {
            Some(self.initial_discriminant(tcx))
        }
    }
}


#[derive(Copy, Clone)]
pub enum CopyImplementationError<'tcx> {
    InfrigingField(&'tcx ty::FieldDef),
    NotAnAdt,
    HasDestructor,
}

/// Describes whether a type is representable. For types that are not
/// representable, 'SelfRecursive' and 'ContainsRecursive' are used to
/// distinguish between types that are recursive with themselves and types that
/// contain a different recursive type. These cases can therefore be treated
/// differently when reporting errors.
///
/// The ordering of the cases is significant. They are sorted so that cmp::max
/// will keep the "more erroneous" of two values.
#[derive(Copy, Clone, PartialOrd, Ord, Eq, PartialEq, Debug)]
pub enum Representability {
    Representable,
    ContainsRecursive,
    SelfRecursive,
}

impl<'tcx> ParameterEnvironment<'tcx> {
    pub fn can_type_implement_copy<'a>(&self, tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                       self_type: Ty<'tcx>, span: Span)
                                       -> Result<(), CopyImplementationError> {
        // FIXME: (@jroesch) float this code up
        tcx.infer_ctxt(self.clone(), Reveal::UserFacing).enter(|infcx| {
            let (adt, substs) = match self_type.sty {
                ty::TyAdt(adt, substs) => (adt, substs),
                _ => return Err(CopyImplementationError::NotAnAdt),
            };

            let field_implements_copy = |field: &ty::FieldDef| {
                let cause = traits::ObligationCause::dummy();
                match traits::fully_normalize(&infcx, cause, &field.ty(tcx, substs)) {
                    Ok(ty) => !infcx.type_moves_by_default(ty, span),
                    Err(..) => false,
                }
            };

            for variant in &adt.variants {
                for field in &variant.fields {
                    if !field_implements_copy(field) {
                        return Err(CopyImplementationError::InfrigingField(field));
                    }
                }
            }

            if adt.has_dtor(tcx) {
                return Err(CopyImplementationError::HasDestructor);
            }

            Ok(())
        })
    }
}

impl<'a, 'tcx> TyCtxt<'a, 'tcx, 'tcx> {
    /// Creates a hash of the type `Ty` which will be the same no matter what crate
    /// context it's calculated within. This is used by the `type_id` intrinsic.
    pub fn type_id_hash(self, ty: Ty<'tcx>) -> u64 {
        let mut hasher = StableHasher::new();
        let mut hcx = StableHashingContext::new(self);

        hcx.while_hashing_spans(false, |hcx| {
            hcx.with_node_id_hashing_mode(NodeIdHashingMode::HashDefPath, |hcx| {
                ty.hash_stable(hcx, &mut hasher);
            });
        });
        hasher.finish()
    }
}

impl<'a, 'gcx, 'tcx> TyCtxt<'a, 'gcx, 'tcx> {
    pub fn has_error_field(self, ty: Ty<'tcx>) -> bool {
        match ty.sty {
            ty::TyAdt(def, substs) => {
                for field in def.all_fields() {
                    let field_ty = field.ty(self, substs);
                    if let TyError = field_ty.sty {
                        return true;
                    }
                }
            }
            _ => (),
        }
        false
    }

    /// Returns the type of element at index `i` in tuple or tuple-like type `t`.
    /// For an enum `t`, `variant` is None only if `t` is a univariant enum.
    pub fn positional_element_ty(self,
                                 ty: Ty<'tcx>,
                                 i: usize,
                                 variant: Option<DefId>) -> Option<Ty<'tcx>> {
        match (&ty.sty, variant) {
            (&TyAdt(adt, substs), Some(vid)) => {
                adt.variant_with_id(vid).fields.get(i).map(|f| f.ty(self, substs))
            }
            (&TyAdt(adt, substs), None) => {
                // Don't use `struct_variant`, this may be a univariant enum.
                adt.variants[0].fields.get(i).map(|f| f.ty(self, substs))
            }
            (&TyTuple(ref v, _), None) => v.get(i).cloned(),
            _ => None,
        }
    }

    /// Returns the type of element at field `n` in struct or struct-like type `t`.
    /// For an enum `t`, `variant` must be some def id.
    pub fn named_element_ty(self,
                            ty: Ty<'tcx>,
                            n: Name,
                            variant: Option<DefId>) -> Option<Ty<'tcx>> {
        match (&ty.sty, variant) {
            (&TyAdt(adt, substs), Some(vid)) => {
                adt.variant_with_id(vid).find_field_named(n).map(|f| f.ty(self, substs))
            }
            (&TyAdt(adt, substs), None) => {
                adt.struct_variant().find_field_named(n).map(|f| f.ty(self, substs))
            }
            _ => return None
        }
    }

    /// Returns the deeply last field of nested structures, or the same type,
    /// if not a structure at all. Corresponds to the only possible unsized
    /// field, and its type can be used to determine unsizing strategy.
    pub fn struct_tail(self, mut ty: Ty<'tcx>) -> Ty<'tcx> {
        while let TyAdt(def, substs) = ty.sty {
            if !def.is_struct() {
                break;
            }
            match def.struct_variant().fields.last() {
                Some(f) => ty = f.ty(self, substs),
                None => break,
            }
        }
        ty
    }

    /// Same as applying struct_tail on `source` and `target`, but only
    /// keeps going as long as the two types are instances of the same
    /// structure definitions.
    /// For `(Foo<Foo<T>>, Foo<Trait>)`, the result will be `(Foo<T>, Trait)`,
    /// whereas struct_tail produces `T`, and `Trait`, respectively.
    pub fn struct_lockstep_tails(self,
                                 source: Ty<'tcx>,
                                 target: Ty<'tcx>)
                                 -> (Ty<'tcx>, Ty<'tcx>) {
        let (mut a, mut b) = (source, target);
        while let (&TyAdt(a_def, a_substs), &TyAdt(b_def, b_substs)) = (&a.sty, &b.sty) {
            if a_def != b_def || !a_def.is_struct() {
                break;
            }
            match a_def.struct_variant().fields.last() {
                Some(f) => {
                    a = f.ty(self, a_substs);
                    b = f.ty(self, b_substs);
                }
                _ => break,
            }
        }
        (a, b)
    }

    /// Given a set of predicates that apply to an object type, returns
    /// the region bounds that the (erased) `Self` type must
    /// outlive. Precisely *because* the `Self` type is erased, the
    /// parameter `erased_self_ty` must be supplied to indicate what type
    /// has been used to represent `Self` in the predicates
    /// themselves. This should really be a unique type; `FreshTy(0)` is a
    /// popular choice.
    ///
    /// NB: in some cases, particularly around higher-ranked bounds,
    /// this function returns a kind of conservative approximation.
    /// That is, all regions returned by this function are definitely
    /// required, but there may be other region bounds that are not
    /// returned, as well as requirements like `for<'a> T: 'a`.
    ///
    /// Requires that trait definitions have been processed so that we can
    /// elaborate predicates and walk supertraits.
    pub fn required_region_bounds(self,
                                  erased_self_ty: Ty<'tcx>,
                                  predicates: Vec<ty::Predicate<'tcx>>)
                                  -> Vec<&'tcx ty::Region>    {
        debug!("required_region_bounds(erased_self_ty={:?}, predicates={:?})",
               erased_self_ty,
               predicates);

        assert!(!erased_self_ty.has_escaping_regions());

        traits::elaborate_predicates(self, predicates)
            .filter_map(|predicate| {
                match predicate {
                    ty::Predicate::Projection(..) |
                    ty::Predicate::Trait(..) |
                    ty::Predicate::Equate(..) |
                    ty::Predicate::Subtype(..) |
                    ty::Predicate::WellFormed(..) |
                    ty::Predicate::ObjectSafe(..) |
                    ty::Predicate::ClosureKind(..) |
                    ty::Predicate::RegionOutlives(..) => {
                        None
                    }
                    ty::Predicate::TypeOutlives(ty::Binder(ty::OutlivesPredicate(t, r))) => {
                        // Search for a bound of the form `erased_self_ty
                        // : 'a`, but be wary of something like `for<'a>
                        // erased_self_ty : 'a` (we interpret a
                        // higher-ranked bound like that as 'static,
                        // though at present the code in `fulfill.rs`
                        // considers such bounds to be unsatisfiable, so
                        // it's kind of a moot point since you could never
                        // construct such an object, but this seems
                        // correct even if that code changes).
                        if t == erased_self_ty && !r.has_escaping_regions() {
                            Some(r)
                        } else {
                            None
                        }
                    }
                }
            })
            .collect()
    }

    /// Calculate the destructor of a given type.
    pub fn calculate_dtor(
        self,
        adt_did: DefId,
        validate: &mut FnMut(Self, DefId) -> Result<(), ErrorReported>
    ) -> Option<ty::Destructor> {
        let drop_trait = if let Some(def_id) = self.lang_items.drop_trait() {
            def_id
        } else {
            return None;
        };

        self.coherent_trait((LOCAL_CRATE, drop_trait));

        let mut dtor_did = None;
        let ty = self.type_of(adt_did);
        self.trait_def(drop_trait).for_each_relevant_impl(self, ty, |impl_did| {
            if let Some(item) = self.associated_items(impl_did).next() {
                if let Ok(()) = validate(self, impl_did) {
                    dtor_did = Some(item.def_id);
                }
            }
        });

        let dtor_did = match dtor_did {
            Some(dtor) => dtor,
            None => return None,
        };

        Some(ty::Destructor { did: dtor_did })
    }

    /// Return the set of types that are required to be alive in
    /// order to run the destructor of `def` (see RFCs 769 and
    /// 1238).
    ///
    /// Note that this returns only the constraints for the
    /// destructor of `def` itself. For the destructors of the
    /// contents, you need `adt_dtorck_constraint`.
    pub fn destructor_constraints(self, def: &'tcx ty::AdtDef)
                                  -> Vec<ty::subst::Kind<'tcx>>
    {
        let dtor = match def.destructor(self) {
            None => {
                debug!("destructor_constraints({:?}) - no dtor", def.did);
                return vec![]
            }
            Some(dtor) => dtor.did
        };

        // RFC 1238: if the destructor method is tagged with the
        // attribute `unsafe_destructor_blind_to_params`, then the
        // compiler is being instructed to *assume* that the
        // destructor will not access borrowed data,
        // even if such data is otherwise reachable.
        //
        // Such access can be in plain sight (e.g. dereferencing
        // `*foo.0` of `Foo<'a>(&'a u32)`) or indirectly hidden
        // (e.g. calling `foo.0.clone()` of `Foo<T:Clone>`).
        if self.has_attr(dtor, "unsafe_destructor_blind_to_params") {
            debug!("destructor_constraint({:?}) - blind", def.did);
            return vec![];
        }

        let impl_def_id = self.associated_item(dtor).container.id();
        let impl_generics = self.generics_of(impl_def_id);

        // We have a destructor - all the parameters that are not
        // pure_wrt_drop (i.e, don't have a #[may_dangle] attribute)
        // must be live.

        // We need to return the list of parameters from the ADTs
        // generics/substs that correspond to impure parameters on the
        // impl's generics. This is a bit ugly, but conceptually simple:
        //
        // Suppose our ADT looks like the following
        //
        //     struct S<X, Y, Z>(X, Y, Z);
        //
        // and the impl is
        //
        //     impl<#[may_dangle] P0, P1, P2> Drop for S<P1, P2, P0>
        //
        // We want to return the parameters (X, Y). For that, we match
        // up the item-substs <X, Y, Z> with the substs on the impl ADT,
        // <P1, P2, P0>, and then look up which of the impl substs refer to
        // parameters marked as pure.

        let impl_substs = match self.type_of(impl_def_id).sty {
            ty::TyAdt(def_, substs) if def_ == def => substs,
            _ => bug!()
        };

        let item_substs = match self.type_of(def.did).sty {
            ty::TyAdt(def_, substs) if def_ == def => substs,
            _ => bug!()
        };

        let result = item_substs.iter().zip(impl_substs.iter())
            .filter(|&(_, &k)| {
                if let Some(&ty::Region::ReEarlyBound(ref ebr)) = k.as_region() {
                    !impl_generics.region_param(ebr).pure_wrt_drop
                } else if let Some(&ty::TyS {
                    sty: ty::TypeVariants::TyParam(ref pt), ..
                }) = k.as_type() {
                    !impl_generics.type_param(pt).pure_wrt_drop
                } else {
                    // not a type or region param - this should be reported
                    // as an error.
                    false
                }
            }).map(|(&item_param, _)| item_param).collect();
        debug!("destructor_constraint({:?}) = {:?}", def.did, result);
        result
    }

    /// Return a set of constraints that needs to be satisfied in
    /// order for `ty` to be valid for destruction.
    pub fn dtorck_constraint_for_ty(self,
                                    span: Span,
                                    for_ty: Ty<'tcx>,
                                    depth: usize,
                                    ty: Ty<'tcx>)
                                    -> Result<ty::DtorckConstraint<'tcx>, ErrorReported>
    {
        debug!("dtorck_constraint_for_ty({:?}, {:?}, {:?}, {:?})",
               span, for_ty, depth, ty);

        if depth >= self.sess.recursion_limit.get() {
            let mut err = struct_span_err!(
                self.sess, span, E0320,
                "overflow while adding drop-check rules for {}", for_ty);
            err.note(&format!("overflowed on {}", ty));
            err.emit();
            return Err(ErrorReported);
        }

        let result = match ty.sty {
            ty::TyBool | ty::TyChar | ty::TyInt(_) | ty::TyUint(_) |
            ty::TyFloat(_) | ty::TyStr | ty::TyNever |
            ty::TyRawPtr(..) | ty::TyRef(..) | ty::TyFnDef(..) | ty::TyFnPtr(_) => {
                // these types never have a destructor
                Ok(ty::DtorckConstraint::empty())
            }

            ty::TyArray(ety, _) | ty::TySlice(ety) => {
                // single-element containers, behave like their element
                self.dtorck_constraint_for_ty(span, for_ty, depth+1, ety)
            }

            ty::TyTuple(tys, _) => {
                tys.iter().map(|ty| {
                    self.dtorck_constraint_for_ty(span, for_ty, depth+1, ty)
                }).collect()
            }

            ty::TyClosure(def_id, substs) => {
                substs.upvar_tys(def_id, self).map(|ty| {
                    self.dtorck_constraint_for_ty(span, for_ty, depth+1, ty)
                }).collect()
            }

            ty::TyAdt(def, substs) => {
                let ty::DtorckConstraint {
                    dtorck_types, outlives
                } = self.at(span).adt_dtorck_constraint(def.did);
                Ok(ty::DtorckConstraint {
                    // FIXME: we can try to recursively `dtorck_constraint_on_ty`
                    // there, but that needs some way to handle cycles.
                    dtorck_types: dtorck_types.subst(self, substs),
                    outlives: outlives.subst(self, substs)
                })
            }

            // Objects must be alive in order for their destructor
            // to be called.
            ty::TyDynamic(..) => Ok(ty::DtorckConstraint {
                outlives: vec![Kind::from(ty)],
                dtorck_types: vec![],
            }),

            // Types that can't be resolved. Pass them forward.
            ty::TyProjection(..) | ty::TyAnon(..) | ty::TyParam(..) => {
                Ok(ty::DtorckConstraint {
                    outlives: vec![],
                    dtorck_types: vec![ty],
                })
            }

            ty::TyInfer(..) | ty::TyError => {
                self.sess.delay_span_bug(span, "unresolved type in dtorck");
                Err(ErrorReported)
            }
        };

        debug!("dtorck_constraint_for_ty({:?}) = {:?}", ty, result);
        result
    }

    pub fn closure_base_def_id(self, def_id: DefId) -> DefId {
        let mut def_id = def_id;
        while self.def_key(def_id).disambiguated_data.data == DefPathData::ClosureExpr {
            def_id = self.parent_def_id(def_id).unwrap_or_else(|| {
                bug!("closure {:?} has no parent", def_id);
            });
        }
        def_id
    }

    /// Given the def-id of some item that has no type parameters, make
    /// a suitable "empty substs" for it.
    pub fn empty_substs_for_def_id(self, item_def_id: DefId) -> &'tcx ty::Substs<'tcx> {
        ty::Substs::for_item(self, item_def_id,
                             |_, _| self.types.re_erased,
                             |_, _| {
            bug!("empty_substs_for_def_id: {:?} has type parameters", item_def_id)
        })
    }
}

pub struct TypeIdHasher<'a, 'gcx: 'a+'tcx, 'tcx: 'a, W> {
    tcx: TyCtxt<'a, 'gcx, 'tcx>,
    state: StableHasher<W>,
}

impl<'a, 'gcx, 'tcx, W> TypeIdHasher<'a, 'gcx, 'tcx, W>
    where W: StableHasherResult
{
    pub fn new(tcx: TyCtxt<'a, 'gcx, 'tcx>) -> Self {
        TypeIdHasher { tcx: tcx, state: StableHasher::new() }
    }

    pub fn finish(self) -> W {
        self.state.finish()
    }

    pub fn hash<T: Hash>(&mut self, x: T) {
        x.hash(&mut self.state);
    }

    fn hash_discriminant_u8<T>(&mut self, x: &T) {
        let v = unsafe {
            intrinsics::discriminant_value(x)
        };
        let b = v as u8;
        assert_eq!(v, b as u64);
        self.hash(b)
    }

    fn def_id(&mut self, did: DefId) {
        // Hash the DefPath corresponding to the DefId, which is independent
        // of compiler internal state. We already have a stable hash value of
        // all DefPaths available via tcx.def_path_hash(), so we just feed that
        // into the hasher.
        let hash = self.tcx.def_path_hash(did);
        self.hash(hash);
    }
}

impl<'a, 'gcx, 'tcx, W> TypeVisitor<'tcx> for TypeIdHasher<'a, 'gcx, 'tcx, W>
    where W: StableHasherResult
{
    fn visit_ty(&mut self, ty: Ty<'tcx>) -> bool {
        // Distinguish between the Ty variants uniformly.
        self.hash_discriminant_u8(&ty.sty);

        match ty.sty {
            TyInt(i) => self.hash(i),
            TyUint(u) => self.hash(u),
            TyFloat(f) => self.hash(f),
            TyArray(_, n) => self.hash(n),
            TyRawPtr(m) |
            TyRef(_, m) => self.hash(m.mutbl),
            TyClosure(def_id, _) |
            TyAnon(def_id, _) |
            TyFnDef(def_id, ..) => self.def_id(def_id),
            TyAdt(d, _) => self.def_id(d.did),
            TyFnPtr(f) => {
                self.hash(f.unsafety());
                self.hash(f.abi());
                self.hash(f.variadic());
                self.hash(f.inputs().skip_binder().len());
            }
            TyDynamic(ref data, ..) => {
                if let Some(p) = data.principal() {
                    self.def_id(p.def_id());
                }
                for d in data.auto_traits() {
                    self.def_id(d);
                }
            }
            TyTuple(tys, defaulted) => {
                self.hash(tys.len());
                self.hash(defaulted);
            }
            TyParam(p) => {
                self.hash(p.idx);
                self.hash(p.name.as_str());
            }
            TyProjection(ref data) => {
                self.def_id(data.trait_ref.def_id);
                self.hash(data.item_name.as_str());
            }
            TyNever |
            TyBool |
            TyChar |
            TyStr |
            TySlice(_) => {}

            TyError |
            TyInfer(_) => bug!("TypeIdHasher: unexpected type {}", ty)
        }

        ty.super_visit_with(self)
    }

    fn visit_region(&mut self, r: &'tcx ty::Region) -> bool {
        self.hash_discriminant_u8(r);
        match *r {
            ty::ReErased |
            ty::ReStatic |
            ty::ReEmpty => {
                // No variant fields to hash for these ...
            }
            ty::ReLateBound(db, ty::BrAnon(i)) => {
                self.hash(db.depth);
                self.hash(i);
            }
            ty::ReEarlyBound(ty::EarlyBoundRegion { index, name }) => {
                self.hash(index);
                self.hash(name.as_str());
            }
            ty::ReLateBound(..) |
            ty::ReFree(..) |
            ty::ReScope(..) |
            ty::ReVar(..) |
            ty::ReSkolemized(..) => {
                bug!("TypeIdHasher: unexpected region {:?}", r)
            }
        }
        false
    }

    fn visit_binder<T: TypeFoldable<'tcx>>(&mut self, x: &ty::Binder<T>) -> bool {
        // Anonymize late-bound regions so that, for example:
        // `for<'a, b> fn(&'a &'b T)` and `for<'a, b> fn(&'b &'a T)`
        // result in the same TypeId (the two types are equivalent).
        self.tcx.anonymize_late_bound_regions(x).super_visit_with(self)
    }
}

impl<'a, 'tcx> ty::TyS<'tcx> {
    fn impls_bound(&'tcx self, tcx: TyCtxt<'a, 'tcx, 'tcx>,
                   param_env: &ParameterEnvironment<'tcx>,
                   def_id: DefId,
                   cache: &RefCell<FxHashMap<Ty<'tcx>, bool>>,
                   span: Span) -> bool
    {
        if self.has_param_types() || self.has_self_ty() {
            if let Some(result) = cache.borrow().get(self) {
                return *result;
            }
        }
        let result =
            tcx.infer_ctxt(param_env.clone(), Reveal::UserFacing)
            .enter(|infcx| {
                traits::type_known_to_meet_bound(&infcx, self, def_id, span)
            });
        if self.has_param_types() || self.has_self_ty() {
            cache.borrow_mut().insert(self, result);
        }
        return result;
    }

    // FIXME (@jroesch): I made this public to use it, not sure if should be private
    pub fn moves_by_default(&'tcx self, tcx: TyCtxt<'a, 'tcx, 'tcx>,
                            param_env: &ParameterEnvironment<'tcx>,
                            span: Span) -> bool {
        if self.flags.get().intersects(TypeFlags::MOVENESS_CACHED) {
            return self.flags.get().intersects(TypeFlags::MOVES_BY_DEFAULT);
        }

        assert!(!self.needs_infer());

        // Fast-path for primitive types
        let result = match self.sty {
            TyBool | TyChar | TyInt(..) | TyUint(..) | TyFloat(..) | TyNever |
            TyRawPtr(..) | TyFnDef(..) | TyFnPtr(_) | TyRef(_, TypeAndMut {
                mutbl: hir::MutImmutable, ..
            }) => Some(false),

            TyStr | TyRef(_, TypeAndMut {
                mutbl: hir::MutMutable, ..
            }) => Some(true),

            TyArray(..) | TySlice(..) | TyDynamic(..) | TyTuple(..) |
            TyClosure(..) | TyAdt(..) | TyAnon(..) |
            TyProjection(..) | TyParam(..) | TyInfer(..) | TyError => None
        }.unwrap_or_else(|| {
            !self.impls_bound(tcx, param_env,
                              tcx.require_lang_item(lang_items::CopyTraitLangItem),
                              &param_env.is_copy_cache, span) });

        if !self.has_param_types() && !self.has_self_ty() {
            self.flags.set(self.flags.get() | if result {
                TypeFlags::MOVENESS_CACHED | TypeFlags::MOVES_BY_DEFAULT
            } else {
                TypeFlags::MOVENESS_CACHED
            });
        }

        result
    }

    #[inline]
    pub fn is_sized(&'tcx self, tcx: TyCtxt<'a, 'tcx, 'tcx>,
                    param_env: &ParameterEnvironment<'tcx>,
                    span: Span) -> bool
    {
        if self.flags.get().intersects(TypeFlags::SIZEDNESS_CACHED) {
            return self.flags.get().intersects(TypeFlags::IS_SIZED);
        }

        self.is_sized_uncached(tcx, param_env, span)
    }

    fn is_sized_uncached(&'tcx self, tcx: TyCtxt<'a, 'tcx, 'tcx>,
                         param_env: &ParameterEnvironment<'tcx>,
                         span: Span) -> bool {
        assert!(!self.needs_infer());

        // Fast-path for primitive types
        let result = match self.sty {
            TyBool | TyChar | TyInt(..) | TyUint(..) | TyFloat(..) |
            TyRawPtr(..) | TyRef(..) | TyFnDef(..) | TyFnPtr(_) |
            TyArray(..) | TyTuple(..) | TyClosure(..) | TyNever => Some(true),

            TyStr | TyDynamic(..) | TySlice(_) => Some(false),

            TyAdt(..) | TyProjection(..) | TyParam(..) |
            TyInfer(..) | TyAnon(..) | TyError => None
        }.unwrap_or_else(|| {
            self.impls_bound(tcx, param_env, tcx.require_lang_item(lang_items::SizedTraitLangItem),
                              &param_env.is_sized_cache, span) });

        if !self.has_param_types() && !self.has_self_ty() {
            self.flags.set(self.flags.get() | if result {
                TypeFlags::SIZEDNESS_CACHED | TypeFlags::IS_SIZED
            } else {
                TypeFlags::SIZEDNESS_CACHED
            });
        }

        result
    }

    /// Returns `true` if and only if there are no `UnsafeCell`s
    /// nested within the type (ignoring `PhantomData` or pointers).
    #[inline]
    pub fn is_freeze(&'tcx self, tcx: TyCtxt<'a, 'tcx, 'tcx>,
                     param_env: &ParameterEnvironment<'tcx>,
                     span: Span) -> bool
    {
        if self.flags.get().intersects(TypeFlags::FREEZENESS_CACHED) {
            return self.flags.get().intersects(TypeFlags::IS_FREEZE);
        }

        self.is_freeze_uncached(tcx, param_env, span)
    }

    fn is_freeze_uncached(&'tcx self, tcx: TyCtxt<'a, 'tcx, 'tcx>,
                          param_env: &ParameterEnvironment<'tcx>,
                          span: Span) -> bool {
        assert!(!self.needs_infer());

        // Fast-path for primitive types
        let result = match self.sty {
            TyBool | TyChar | TyInt(..) | TyUint(..) | TyFloat(..) |
            TyRawPtr(..) | TyRef(..) | TyFnDef(..) | TyFnPtr(_) |
            TyStr | TyNever => Some(true),

            TyArray(..) | TySlice(_) |
            TyTuple(..) | TyClosure(..) | TyAdt(..) |
            TyDynamic(..) | TyProjection(..) | TyParam(..) |
            TyInfer(..) | TyAnon(..) | TyError => None
        }.unwrap_or_else(|| {
            self.impls_bound(tcx, param_env, tcx.require_lang_item(lang_items::FreezeTraitLangItem),
                              &param_env.is_freeze_cache, span) });

        if !self.has_param_types() && !self.has_self_ty() {
            self.flags.set(self.flags.get() | if result {
                TypeFlags::FREEZENESS_CACHED | TypeFlags::IS_FREEZE
            } else {
                TypeFlags::FREEZENESS_CACHED
            });
        }

        result
    }

    /// If `ty.needs_drop(...)` returns `true`, then `ty` is definitely
    /// non-copy and *might* have a destructor attached; if it returns
    /// `false`, then `ty` definitely has no destructor (i.e. no drop glue).
    ///
    /// (Note that this implies that if `ty` has a destructor attached,
    /// then `needs_drop` will definitely return `true` for `ty`.)
    #[inline]
    pub fn needs_drop(&'tcx self, tcx: TyCtxt<'a, 'tcx, 'tcx>,
                    param_env: &ty::ParameterEnvironment<'tcx>) -> bool {
        if self.flags.get().intersects(TypeFlags::NEEDS_DROP_CACHED) {
            return self.flags.get().intersects(TypeFlags::NEEDS_DROP);
        }

        self.needs_drop_uncached(tcx, param_env, &mut FxHashSet())
    }

    fn needs_drop_inner(&'tcx self,
                        tcx: TyCtxt<'a, 'tcx, 'tcx>,
                        param_env: &ty::ParameterEnvironment<'tcx>,
                        stack: &mut FxHashSet<Ty<'tcx>>)
                        -> bool {
        if self.flags.get().intersects(TypeFlags::NEEDS_DROP_CACHED) {
            return self.flags.get().intersects(TypeFlags::NEEDS_DROP);
        }

        // This should be reported as an error by `check_representable`.
        //
        // Consider the type as not needing drop in the meanwhile to avoid
        // further errors.
        if let Some(_) = stack.replace(self) {
            return false;
        }

        let needs_drop = self.needs_drop_uncached(tcx, param_env, stack);

        // "Pop" the cycle detection "stack".
        stack.remove(self);

        needs_drop
    }

    fn needs_drop_uncached(&'tcx self,
                           tcx: TyCtxt<'a, 'tcx, 'tcx>,
                           param_env: &ty::ParameterEnvironment<'tcx>,
                           stack: &mut FxHashSet<Ty<'tcx>>)
                           -> bool {
        assert!(!self.needs_infer());

        let result = match self.sty {
            // Fast-path for primitive types
            ty::TyInfer(ty::FreshIntTy(_)) | ty::TyInfer(ty::FreshFloatTy(_)) |
            ty::TyBool | ty::TyInt(_) | ty::TyUint(_) | ty::TyFloat(_) | ty::TyNever |
            ty::TyFnDef(..) | ty::TyFnPtr(_) | ty::TyChar |
            ty::TyRawPtr(_) | ty::TyRef(..) | ty::TyStr => false,

            // Issue #22536: We first query type_moves_by_default.  It sees a
            // normalized version of the type, and therefore will definitely
            // know whether the type implements Copy (and thus needs no
            // cleanup/drop/zeroing) ...
            _ if !self.moves_by_default(tcx, param_env, DUMMY_SP) => false,

            // ... (issue #22536 continued) but as an optimization, still use
            // prior logic of asking for the structural "may drop".

            // FIXME(#22815): Note that this is a conservative heuristic;
            // it may report that the type "may drop" when actual type does
            // not actually have a destructor associated with it. But since
            // the type absolutely did not have the `Copy` bound attached
            // (see above), it is sound to treat it as having a destructor.

            // User destructors are the only way to have concrete drop types.
            ty::TyAdt(def, _) if def.has_dtor(tcx) => true,

            // Can refer to a type which may drop.
            // FIXME(eddyb) check this against a ParameterEnvironment.
            ty::TyDynamic(..) | ty::TyProjection(..) | ty::TyParam(_) |
            ty::TyAnon(..) | ty::TyInfer(_) | ty::TyError => true,

            // Structural recursion.
            ty::TyArray(ty, _) | ty::TySlice(ty) => {
                ty.needs_drop_inner(tcx, param_env, stack)
            }

            ty::TyClosure(def_id, ref substs) => {
                substs.upvar_tys(def_id, tcx)
                    .any(|ty| ty.needs_drop_inner(tcx, param_env, stack))
            }

            ty::TyTuple(ref tys, _) => {
                tys.iter().any(|ty| ty.needs_drop_inner(tcx, param_env, stack))
            }

            // unions don't have destructors regardless of the child types
            ty::TyAdt(def, _) if def.is_union() => false,

            ty::TyAdt(def, substs) => {
                def.variants.iter().any(|v| {
                    v.fields.iter().any(|f| {
                        f.ty(tcx, substs).needs_drop_inner(tcx, param_env, stack)
                    })
                })
            }
        };

        if !self.has_param_types() && !self.has_self_ty() {
            self.flags.set(self.flags.get() | if result {
                TypeFlags::NEEDS_DROP_CACHED | TypeFlags::NEEDS_DROP
            } else {
                TypeFlags::NEEDS_DROP_CACHED
            });
        }

        result
    }

    #[inline]
    pub fn layout<'lcx>(&'tcx self, infcx: &InferCtxt<'a, 'tcx, 'lcx>)
                        -> Result<&'tcx Layout, LayoutError<'tcx>> {
        let tcx = infcx.tcx.global_tcx();
        let can_cache = !self.has_param_types() && !self.has_self_ty();
        if can_cache {
            if let Some(&cached) = tcx.layout_cache.borrow().get(&self) {
                return Ok(cached);
            }
        }

        let rec_limit = tcx.sess.recursion_limit.get();
        let depth = tcx.layout_depth.get();
        if depth > rec_limit {
            tcx.sess.fatal(
                &format!("overflow representing the type `{}`", self));
        }

        tcx.layout_depth.set(depth+1);
        let layout = Layout::compute_uncached(self, infcx);
        tcx.layout_depth.set(depth);
        let layout = layout?;
        if can_cache {
            tcx.layout_cache.borrow_mut().insert(self, layout);
        }
        Ok(layout)
    }


    /// Check whether a type is representable. This means it cannot contain unboxed
    /// structural recursion. This check is needed for structs and enums.
    pub fn is_representable(&'tcx self, tcx: TyCtxt<'a, 'tcx, 'tcx>, sp: Span)
                            -> Representability {

        // Iterate until something non-representable is found
        fn find_nonrepresentable<'a, 'tcx, It>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                               sp: Span,
                                               seen: &mut Vec<Ty<'tcx>>,
                                               iter: It)
                                               -> Representability
        where It: Iterator<Item=Ty<'tcx>> {
            iter.fold(Representability::Representable,
                      |r, ty| cmp::max(r, is_type_structurally_recursive(tcx, sp, seen, ty)))
        }

        fn are_inner_types_recursive<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>, sp: Span,
                                               seen: &mut Vec<Ty<'tcx>>, ty: Ty<'tcx>)
                                               -> Representability {
            match ty.sty {
                TyTuple(ref ts, _) => {
                    find_nonrepresentable(tcx, sp, seen, ts.iter().cloned())
                }
                // Fixed-length vectors.
                // FIXME(#11924) Behavior undecided for zero-length vectors.
                TyArray(ty, _) => {
                    is_type_structurally_recursive(tcx, sp, seen, ty)
                }
                TyAdt(def, substs) => {
                    find_nonrepresentable(tcx,
                                          sp,
                                          seen,
                                          def.all_fields().map(|f| f.ty(tcx, substs)))
                }
                TyClosure(..) => {
                    // this check is run on type definitions, so we don't expect
                    // to see closure types
                    bug!("requires check invoked on inapplicable type: {:?}", ty)
                }
                _ => Representability::Representable,
            }
        }

        fn same_struct_or_enum<'tcx>(ty: Ty<'tcx>, def: &'tcx ty::AdtDef) -> bool {
            match ty.sty {
                TyAdt(ty_def, _) => {
                     ty_def == def
                }
                _ => false
            }
        }

        fn same_type<'tcx>(a: Ty<'tcx>, b: Ty<'tcx>) -> bool {
            match (&a.sty, &b.sty) {
                (&TyAdt(did_a, substs_a), &TyAdt(did_b, substs_b)) => {
                    if did_a != did_b {
                        return false;
                    }

                    substs_a.types().zip(substs_b.types()).all(|(a, b)| same_type(a, b))
                }
                _ => a == b,
            }
        }

        // Does the type `ty` directly (without indirection through a pointer)
        // contain any types on stack `seen`?
        fn is_type_structurally_recursive<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                                    sp: Span,
                                                    seen: &mut Vec<Ty<'tcx>>,
                                                    ty: Ty<'tcx>) -> Representability {
            debug!("is_type_structurally_recursive: {:?}", ty);

            match ty.sty {
                TyAdt(def, _) => {
                    {
                        // Iterate through stack of previously seen types.
                        let mut iter = seen.iter();

                        // The first item in `seen` is the type we are actually curious about.
                        // We want to return SelfRecursive if this type contains itself.
                        // It is important that we DON'T take generic parameters into account
                        // for this check, so that Bar<T> in this example counts as SelfRecursive:
                        //
                        // struct Foo;
                        // struct Bar<T> { x: Bar<Foo> }

                        if let Some(&seen_type) = iter.next() {
                            if same_struct_or_enum(seen_type, def) {
                                debug!("SelfRecursive: {:?} contains {:?}",
                                       seen_type,
                                       ty);
                                return Representability::SelfRecursive;
                            }
                        }

                        // We also need to know whether the first item contains other types
                        // that are structurally recursive. If we don't catch this case, we
                        // will recurse infinitely for some inputs.
                        //
                        // It is important that we DO take generic parameters into account
                        // here, so that code like this is considered SelfRecursive, not
                        // ContainsRecursive:
                        //
                        // struct Foo { Option<Option<Foo>> }

                        for &seen_type in iter {
                            if same_type(ty, seen_type) {
                                debug!("ContainsRecursive: {:?} contains {:?}",
                                       seen_type,
                                       ty);
                                return Representability::ContainsRecursive;
                            }
                        }
                    }

                    // For structs and enums, track all previously seen types by pushing them
                    // onto the 'seen' stack.
                    seen.push(ty);
                    let out = are_inner_types_recursive(tcx, sp, seen, ty);
                    seen.pop();
                    out
                }
                _ => {
                    // No need to push in other cases.
                    are_inner_types_recursive(tcx, sp, seen, ty)
                }
            }
        }

        debug!("is_type_representable: {:?}", self);

        // To avoid a stack overflow when checking an enum variant or struct that
        // contains a different, structurally recursive type, maintain a stack
        // of seen types and check recursion for each of them (issues #3008, #3779).
        let mut seen: Vec<Ty> = Vec::new();
        let r = is_type_structurally_recursive(tcx, sp, &mut seen, self);
        debug!("is_type_representable: {:?} is {:?}", self, r);
        r
    }
}
