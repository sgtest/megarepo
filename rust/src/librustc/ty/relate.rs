// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Generalized type relating mechanism. A type relation R relates a
//! pair of values (A, B). A and B are usually types or regions but
//! can be other things. Examples of type relations are subtyping,
//! type equality, etc.

use hir::def_id::DefId;
use ty::subst::Substs;
use ty::{self, Ty, TyCtxt, TypeFoldable};
use ty::error::{ExpectedFound, TypeError};
use std::rc::Rc;
use syntax::abi;
use hir as ast;

pub type RelateResult<'tcx, T> = Result<T, TypeError<'tcx>>;

#[derive(Clone, Debug)]
pub enum Cause {
    ExistentialRegionBound, // relating an existential region bound
}

pub trait TypeRelation<'a, 'gcx: 'a+'tcx, 'tcx: 'a> : Sized {
    fn tcx(&self) -> TyCtxt<'a, 'gcx, 'tcx>;

    /// Returns a static string we can use for printouts.
    fn tag(&self) -> &'static str;

    /// Returns true if the value `a` is the "expected" type in the
    /// relation. Just affects error messages.
    fn a_is_expected(&self) -> bool;

    fn with_cause<F,R>(&mut self, _cause: Cause, f: F) -> R
        where F: FnOnce(&mut Self) -> R
    {
        f(self)
    }

    /// Generic relation routine suitable for most anything.
    fn relate<T: Relate<'tcx>>(&mut self, a: &T, b: &T) -> RelateResult<'tcx, T> {
        Relate::relate(self, a, b)
    }

    /// Relete elements of two slices pairwise.
    fn relate_zip<T: Relate<'tcx>>(&mut self, a: &[T], b: &[T]) -> RelateResult<'tcx, Vec<T>> {
        assert_eq!(a.len(), b.len());
        a.iter().zip(b).map(|(a, b)| self.relate(a, b)).collect()
    }

    /// Switch variance for the purpose of relating `a` and `b`.
    fn relate_with_variance<T: Relate<'tcx>>(&mut self,
                                             variance: ty::Variance,
                                             a: &T,
                                             b: &T)
                                             -> RelateResult<'tcx, T>;

    // Overrideable relations. You shouldn't typically call these
    // directly, instead call `relate()`, which in turn calls
    // these. This is both more uniform but also allows us to add
    // additional hooks for other types in the future if needed
    // without making older code, which called `relate`, obsolete.

    fn tys(&mut self, a: Ty<'tcx>, b: Ty<'tcx>)
           -> RelateResult<'tcx, Ty<'tcx>>;

    fn regions(&mut self, a: ty::Region, b: ty::Region)
               -> RelateResult<'tcx, ty::Region>;

    fn binders<T>(&mut self, a: &ty::Binder<T>, b: &ty::Binder<T>)
                  -> RelateResult<'tcx, ty::Binder<T>>
        where T: Relate<'tcx>;
}

pub trait Relate<'tcx>: TypeFoldable<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R, a: &Self, b: &Self)
                           -> RelateResult<'tcx, Self>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a;
}

///////////////////////////////////////////////////////////////////////////
// Relate impls

impl<'tcx> Relate<'tcx> for ty::TypeAndMut<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ty::TypeAndMut<'tcx>,
                           b: &ty::TypeAndMut<'tcx>)
                           -> RelateResult<'tcx, ty::TypeAndMut<'tcx>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        debug!("{}.mts({:?}, {:?})",
               relation.tag(),
               a,
               b);
        if a.mutbl != b.mutbl {
            Err(TypeError::Mutability)
        } else {
            let mutbl = a.mutbl;
            let variance = match mutbl {
                ast::Mutability::MutImmutable => ty::Covariant,
                ast::Mutability::MutMutable => ty::Invariant,
            };
            let ty = relation.relate_with_variance(variance, &a.ty, &b.ty)?;
            Ok(ty::TypeAndMut {ty: ty, mutbl: mutbl})
        }
    }
}

// substitutions are not themselves relatable without more context,
// but they is an important subroutine for things that ARE relatable,
// like traits etc.
fn relate_item_substs<'a, 'gcx, 'tcx, R>(relation: &mut R,
                                         item_def_id: DefId,
                                         a_subst: &'tcx Substs<'tcx>,
                                         b_subst: &'tcx Substs<'tcx>)
                                         -> RelateResult<'tcx, &'tcx Substs<'tcx>>
    where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
{
    debug!("substs: item_def_id={:?} a_subst={:?} b_subst={:?}",
           item_def_id,
           a_subst,
           b_subst);

    let variances;
    let opt_variances = if relation.tcx().variance_computed.get() {
        variances = relation.tcx().item_variances(item_def_id);
        Some(&*variances)
    } else {
        None
    };
    relate_substs(relation, opt_variances, a_subst, b_subst)
}

pub fn relate_substs<'a, 'gcx, 'tcx, R>(relation: &mut R,
                                        variances: Option<&ty::ItemVariances>,
                                        a_subst: &'tcx Substs<'tcx>,
                                        b_subst: &'tcx Substs<'tcx>)
                                        -> RelateResult<'tcx, &'tcx Substs<'tcx>>
    where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
{
    let tcx = relation.tcx();

    let types = a_subst.types.iter().enumerate().map(|(i, a_ty)| {
        let b_ty = &b_subst.types[i];
        let variance = variances.map_or(ty::Invariant, |v| v.types[i]);
        relation.relate_with_variance(variance, a_ty, b_ty)
    }).collect()?;

    let regions = a_subst.regions.iter().enumerate().map(|(i, a_r)| {
        let b_r = &b_subst.regions[i];
        let variance = variances.map_or(ty::Invariant, |v| v.regions[i]);
        relation.relate_with_variance(variance, a_r, b_r)
    }).collect()?;

    Ok(Substs::new(tcx, types, regions))
}

impl<'tcx> Relate<'tcx> for &'tcx ty::BareFnTy<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &&'tcx ty::BareFnTy<'tcx>,
                           b: &&'tcx ty::BareFnTy<'tcx>)
                           -> RelateResult<'tcx, &'tcx ty::BareFnTy<'tcx>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        let unsafety = relation.relate(&a.unsafety, &b.unsafety)?;
        let abi = relation.relate(&a.abi, &b.abi)?;
        let sig = relation.relate(&a.sig, &b.sig)?;
        Ok(relation.tcx().mk_bare_fn(ty::BareFnTy {
            unsafety: unsafety,
            abi: abi,
            sig: sig
        }))
    }
}

impl<'tcx> Relate<'tcx> for ty::FnSig<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ty::FnSig<'tcx>,
                           b: &ty::FnSig<'tcx>)
                           -> RelateResult<'tcx, ty::FnSig<'tcx>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        if a.variadic != b.variadic {
            return Err(TypeError::VariadicMismatch(
                expected_found(relation, &a.variadic, &b.variadic)));
        }

        let inputs = relate_arg_vecs(relation,
                                     &a.inputs,
                                     &b.inputs)?;
        let output = relation.relate(&a.output, &b.output)?;

        Ok(ty::FnSig {inputs: inputs,
                      output: output,
                      variadic: a.variadic})
    }
}

fn relate_arg_vecs<'a, 'gcx, 'tcx, R>(relation: &mut R,
                                      a_args: &[Ty<'tcx>],
                                      b_args: &[Ty<'tcx>])
                                      -> RelateResult<'tcx, Vec<Ty<'tcx>>>
    where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
{
    if a_args.len() != b_args.len() {
        return Err(TypeError::ArgCount);
    }

    a_args.iter().zip(b_args)
          .map(|(a, b)| relation.relate_with_variance(ty::Contravariant, a, b))
          .collect()
}

impl<'tcx> Relate<'tcx> for ast::Unsafety {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ast::Unsafety,
                           b: &ast::Unsafety)
                           -> RelateResult<'tcx, ast::Unsafety>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        if a != b {
            Err(TypeError::UnsafetyMismatch(expected_found(relation, a, b)))
        } else {
            Ok(*a)
        }
    }
}

impl<'tcx> Relate<'tcx> for abi::Abi {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &abi::Abi,
                           b: &abi::Abi)
                           -> RelateResult<'tcx, abi::Abi>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        if a == b {
            Ok(*a)
        } else {
            Err(TypeError::AbiMismatch(expected_found(relation, a, b)))
        }
    }
}

impl<'tcx> Relate<'tcx> for ty::ProjectionTy<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ty::ProjectionTy<'tcx>,
                           b: &ty::ProjectionTy<'tcx>)
                           -> RelateResult<'tcx, ty::ProjectionTy<'tcx>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        if a.item_name != b.item_name {
            Err(TypeError::ProjectionNameMismatched(
                expected_found(relation, &a.item_name, &b.item_name)))
        } else {
            let trait_ref = relation.relate(&a.trait_ref, &b.trait_ref)?;
            Ok(ty::ProjectionTy { trait_ref: trait_ref, item_name: a.item_name })
        }
    }
}

impl<'tcx> Relate<'tcx> for ty::ExistentialProjection<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ty::ExistentialProjection<'tcx>,
                           b: &ty::ExistentialProjection<'tcx>)
                           -> RelateResult<'tcx, ty::ExistentialProjection<'tcx>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        if a.item_name != b.item_name {
            Err(TypeError::ProjectionNameMismatched(
                expected_found(relation, &a.item_name, &b.item_name)))
        } else {
            let trait_ref = relation.relate(&a.trait_ref, &b.trait_ref)?;
            let ty = relation.relate(&a.ty, &b.ty)?;
            Ok(ty::ExistentialProjection {
                trait_ref: trait_ref,
                item_name: a.item_name,
                ty: ty
            })
        }
    }
}

impl<'tcx> Relate<'tcx> for Vec<ty::PolyExistentialProjection<'tcx>> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &Vec<ty::PolyExistentialProjection<'tcx>>,
                           b: &Vec<ty::PolyExistentialProjection<'tcx>>)
                           -> RelateResult<'tcx, Vec<ty::PolyExistentialProjection<'tcx>>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        // To be compatible, `a` and `b` must be for precisely the
        // same set of traits and item names. We always require that
        // projection bounds lists are sorted by trait-def-id and item-name,
        // so we can just iterate through the lists pairwise, so long as they are the
        // same length.
        if a.len() != b.len() {
            Err(TypeError::ProjectionBoundsLength(expected_found(relation, &a.len(), &b.len())))
        } else {
            a.iter().zip(b)
                .map(|(a, b)| relation.relate(a, b))
                .collect()
        }
    }
}

impl<'tcx> Relate<'tcx> for ty::BuiltinBounds {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ty::BuiltinBounds,
                           b: &ty::BuiltinBounds)
                           -> RelateResult<'tcx, ty::BuiltinBounds>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        // Two sets of builtin bounds are only relatable if they are
        // precisely the same (but see the coercion code).
        if a != b {
            Err(TypeError::BuiltinBoundsMismatch(expected_found(relation, a, b)))
        } else {
            Ok(*a)
        }
    }
}

impl<'tcx> Relate<'tcx> for ty::TraitRef<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ty::TraitRef<'tcx>,
                           b: &ty::TraitRef<'tcx>)
                           -> RelateResult<'tcx, ty::TraitRef<'tcx>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        // Different traits cannot be related
        if a.def_id != b.def_id {
            Err(TypeError::Traits(expected_found(relation, &a.def_id, &b.def_id)))
        } else {
            let substs = relate_item_substs(relation, a.def_id, a.substs, b.substs)?;
            Ok(ty::TraitRef { def_id: a.def_id, substs: substs })
        }
    }
}

impl<'tcx> Relate<'tcx> for ty::ExistentialTraitRef<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ty::ExistentialTraitRef<'tcx>,
                           b: &ty::ExistentialTraitRef<'tcx>)
                           -> RelateResult<'tcx, ty::ExistentialTraitRef<'tcx>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        // Different traits cannot be related
        if a.def_id != b.def_id {
            Err(TypeError::Traits(expected_found(relation, &a.def_id, &b.def_id)))
        } else {
            let substs = relate_item_substs(relation, a.def_id, a.substs, b.substs)?;
            Ok(ty::ExistentialTraitRef { def_id: a.def_id, substs: substs })
        }
    }
}

impl<'tcx> Relate<'tcx> for Ty<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &Ty<'tcx>,
                           b: &Ty<'tcx>)
                           -> RelateResult<'tcx, Ty<'tcx>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        relation.tys(a, b)
    }
}

/// The main "type relation" routine. Note that this does not handle
/// inference artifacts, so you should filter those out before calling
/// it.
pub fn super_relate_tys<'a, 'gcx, 'tcx, R>(relation: &mut R,
                                           a: Ty<'tcx>,
                                           b: Ty<'tcx>)
                                           -> RelateResult<'tcx, Ty<'tcx>>
    where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
{
    let tcx = relation.tcx();
    let a_sty = &a.sty;
    let b_sty = &b.sty;
    debug!("super_tys: a_sty={:?} b_sty={:?}", a_sty, b_sty);
    match (a_sty, b_sty) {
        (&ty::TyInfer(_), _) |
        (_, &ty::TyInfer(_)) =>
        {
            // The caller should handle these cases!
            bug!("var types encountered in super_relate_tys")
        }

        (&ty::TyError, _) | (_, &ty::TyError) =>
        {
            Ok(tcx.types.err)
        }

        (&ty::TyNever, _) |
        (&ty::TyChar, _) |
        (&ty::TyBool, _) |
        (&ty::TyInt(_), _) |
        (&ty::TyUint(_), _) |
        (&ty::TyFloat(_), _) |
        (&ty::TyStr, _)
            if a == b =>
        {
            Ok(a)
        }

        (&ty::TyParam(ref a_p), &ty::TyParam(ref b_p))
            if a_p.idx == b_p.idx =>
        {
            Ok(a)
        }

        (&ty::TyEnum(a_def, a_substs), &ty::TyEnum(b_def, b_substs))
            if a_def == b_def =>
        {
            let substs = relate_item_substs(relation, a_def.did, a_substs, b_substs)?;
            Ok(tcx.mk_enum(a_def, substs))
        }

        (&ty::TyTrait(ref a_obj), &ty::TyTrait(ref b_obj)) =>
        {
            let principal = relation.relate(&a_obj.principal, &b_obj.principal)?;
            let r =
                relation.with_cause(
                    Cause::ExistentialRegionBound,
                    |relation| relation.relate_with_variance(ty::Contravariant,
                                                             &a_obj.region_bound,
                                                             &b_obj.region_bound))?;
            let nb = relation.relate(&a_obj.builtin_bounds, &b_obj.builtin_bounds)?;
            let pb = relation.relate(&a_obj.projection_bounds, &b_obj.projection_bounds)?;
            Ok(tcx.mk_trait(ty::TraitObject {
                principal: principal,
                region_bound: r,
                builtin_bounds: nb,
                projection_bounds: pb
            }))
        }

        (&ty::TyStruct(a_def, a_substs), &ty::TyStruct(b_def, b_substs))
            if a_def == b_def =>
        {
            let substs = relate_item_substs(relation, a_def.did, a_substs, b_substs)?;
            Ok(tcx.mk_struct(a_def, substs))
        }

        (&ty::TyClosure(a_id, a_substs),
         &ty::TyClosure(b_id, b_substs))
            if a_id == b_id =>
        {
            // All TyClosure types with the same id represent
            // the (anonymous) type of the same closure expression. So
            // all of their regions should be equated.
            let substs = relation.relate(&a_substs, &b_substs)?;
            Ok(tcx.mk_closure_from_closure_substs(a_id, substs))
        }

        (&ty::TyBox(a_inner), &ty::TyBox(b_inner)) =>
        {
            let typ = relation.relate(&a_inner, &b_inner)?;
            Ok(tcx.mk_box(typ))
        }

        (&ty::TyRawPtr(ref a_mt), &ty::TyRawPtr(ref b_mt)) =>
        {
            let mt = relation.relate(a_mt, b_mt)?;
            Ok(tcx.mk_ptr(mt))
        }

        (&ty::TyRef(a_r, ref a_mt), &ty::TyRef(b_r, ref b_mt)) =>
        {
            let r = relation.relate_with_variance(ty::Contravariant, a_r, b_r)?;
            let mt = relation.relate(a_mt, b_mt)?;
            Ok(tcx.mk_ref(tcx.mk_region(r), mt))
        }

        (&ty::TyArray(a_t, sz_a), &ty::TyArray(b_t, sz_b)) =>
        {
            let t = relation.relate(&a_t, &b_t)?;
            if sz_a == sz_b {
                Ok(tcx.mk_array(t, sz_a))
            } else {
                Err(TypeError::FixedArraySize(expected_found(relation, &sz_a, &sz_b)))
            }
        }

        (&ty::TySlice(a_t), &ty::TySlice(b_t)) =>
        {
            let t = relation.relate(&a_t, &b_t)?;
            Ok(tcx.mk_slice(t))
        }

        (&ty::TyTuple(as_), &ty::TyTuple(bs)) =>
        {
            if as_.len() == bs.len() {
                let ts = as_.iter().zip(bs)
                            .map(|(a, b)| relation.relate(a, b))
                            .collect::<Result<_, _>>()?;
                Ok(tcx.mk_tup(ts))
            } else if !(as_.is_empty() || bs.is_empty()) {
                Err(TypeError::TupleSize(
                    expected_found(relation, &as_.len(), &bs.len())))
            } else {
                Err(TypeError::Sorts(expected_found(relation, &a, &b)))
            }
        }

        (&ty::TyFnDef(a_def_id, a_substs, a_fty),
         &ty::TyFnDef(b_def_id, b_substs, b_fty))
            if a_def_id == b_def_id =>
        {
            let substs = relate_substs(relation, None, a_substs, b_substs)?;
            let fty = relation.relate(&a_fty, &b_fty)?;
            Ok(tcx.mk_fn_def(a_def_id, substs, fty))
        }

        (&ty::TyFnPtr(a_fty), &ty::TyFnPtr(b_fty)) =>
        {
            let fty = relation.relate(&a_fty, &b_fty)?;
            Ok(tcx.mk_fn_ptr(fty))
        }

        (&ty::TyProjection(ref a_data), &ty::TyProjection(ref b_data)) =>
        {
            let projection_ty = relation.relate(a_data, b_data)?;
            Ok(tcx.mk_projection(projection_ty.trait_ref, projection_ty.item_name))
        }

        (&ty::TyAnon(a_def_id, a_substs), &ty::TyAnon(b_def_id, b_substs))
            if a_def_id == b_def_id =>
        {
            let substs = relate_substs(relation, None, a_substs, b_substs)?;
            Ok(tcx.mk_anon(a_def_id, substs))
        }

        _ =>
        {
            Err(TypeError::Sorts(expected_found(relation, &a, &b)))
        }
    }
}

impl<'tcx> Relate<'tcx> for ty::ClosureSubsts<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ty::ClosureSubsts<'tcx>,
                           b: &ty::ClosureSubsts<'tcx>)
                           -> RelateResult<'tcx, ty::ClosureSubsts<'tcx>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        let substs = relate_substs(relation, None, a.func_substs, b.func_substs)?;
        let upvar_tys = relation.relate_zip(&a.upvar_tys, &b.upvar_tys)?;
        Ok(ty::ClosureSubsts {
            func_substs: substs,
            upvar_tys: relation.tcx().mk_type_list(upvar_tys)
        })
    }
}

impl<'tcx> Relate<'tcx> for &'tcx Substs<'tcx> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &&'tcx Substs<'tcx>,
                           b: &&'tcx Substs<'tcx>)
                           -> RelateResult<'tcx, &'tcx Substs<'tcx>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        relate_substs(relation, None, a, b)
    }
}

impl<'tcx> Relate<'tcx> for ty::Region {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ty::Region,
                           b: &ty::Region)
                           -> RelateResult<'tcx, ty::Region>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        relation.regions(*a, *b)
    }
}

impl<'tcx, T: Relate<'tcx>> Relate<'tcx> for ty::Binder<T> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &ty::Binder<T>,
                           b: &ty::Binder<T>)
                           -> RelateResult<'tcx, ty::Binder<T>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        relation.binders(a, b)
    }
}

impl<'tcx, T: Relate<'tcx>> Relate<'tcx> for Rc<T> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &Rc<T>,
                           b: &Rc<T>)
                           -> RelateResult<'tcx, Rc<T>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        let a: &T = a;
        let b: &T = b;
        Ok(Rc::new(relation.relate(a, b)?))
    }
}

impl<'tcx, T: Relate<'tcx>> Relate<'tcx> for Box<T> {
    fn relate<'a, 'gcx, R>(relation: &mut R,
                           a: &Box<T>,
                           b: &Box<T>)
                           -> RelateResult<'tcx, Box<T>>
        where R: TypeRelation<'a, 'gcx, 'tcx>, 'gcx: 'a+'tcx, 'tcx: 'a
    {
        let a: &T = a;
        let b: &T = b;
        Ok(Box::new(relation.relate(a, b)?))
    }
}

///////////////////////////////////////////////////////////////////////////
// Error handling

pub fn expected_found<'a, 'gcx, 'tcx, R, T>(relation: &mut R,
                                            a: &T,
                                            b: &T)
                                            -> ExpectedFound<T>
    where R: TypeRelation<'a, 'gcx, 'tcx>, T: Clone, 'gcx: 'a+'tcx, 'tcx: 'a
{
    expected_found_bool(relation.a_is_expected(), a, b)
}

pub fn expected_found_bool<T>(a_is_expected: bool,
                              a: &T,
                              b: &T)
                              -> ExpectedFound<T>
    where T: Clone
{
    let a = a.clone();
    let b = b.clone();
    if a_is_expected {
        ExpectedFound {expected: a, found: b}
    } else {
        ExpectedFound {expected: b, found: a}
    }
}
