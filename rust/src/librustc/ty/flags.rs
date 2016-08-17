// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use ty::subst::Substs;
use ty::{self, Ty, TypeFlags, TypeFoldable};

pub struct FlagComputation {
    pub flags: TypeFlags,

    // maximum depth of any bound region that we have seen thus far
    pub depth: u32,
}

impl FlagComputation {
    fn new() -> FlagComputation {
        FlagComputation { flags: TypeFlags::empty(), depth: 0 }
    }

    pub fn for_sty(st: &ty::TypeVariants) -> FlagComputation {
        let mut result = FlagComputation::new();
        result.add_sty(st);
        result
    }

    fn add_flags(&mut self, flags: TypeFlags) {
        self.flags = self.flags | (flags & TypeFlags::NOMINAL_FLAGS);
    }

    fn add_depth(&mut self, depth: u32) {
        if depth > self.depth {
            self.depth = depth;
        }
    }

    /// Adds the flags/depth from a set of types that appear within the current type, but within a
    /// region binder.
    fn add_bound_computation(&mut self, computation: &FlagComputation) {
        self.add_flags(computation.flags);

        // The types that contributed to `computation` occurred within
        // a region binder, so subtract one from the region depth
        // within when adding the depth to `self`.
        let depth = computation.depth;
        if depth > 0 {
            self.add_depth(depth - 1);
        }
    }

    fn add_sty(&mut self, st: &ty::TypeVariants) {
        match st {
            &ty::TyBool |
            &ty::TyChar |
            &ty::TyInt(_) |
            &ty::TyFloat(_) |
            &ty::TyUint(_) |
            &ty::TyNever |
            &ty::TyStr => {
            }

            // You might think that we could just return TyError for
            // any type containing TyError as a component, and get
            // rid of the TypeFlags::HAS_TY_ERR flag -- likewise for ty_bot (with
            // the exception of function types that return bot).
            // But doing so caused sporadic memory corruption, and
            // neither I (tjc) nor nmatsakis could figure out why,
            // so we're doing it this way.
            &ty::TyError => {
                self.add_flags(TypeFlags::HAS_TY_ERR)
            }

            &ty::TyParam(ref p) => {
                self.add_flags(TypeFlags::HAS_LOCAL_NAMES);
                if p.is_self() {
                    self.add_flags(TypeFlags::HAS_SELF);
                } else {
                    self.add_flags(TypeFlags::HAS_PARAMS);
                }
            }

            &ty::TyClosure(_, ref substs) => {
                self.add_flags(TypeFlags::HAS_TY_CLOSURE);
                self.add_flags(TypeFlags::HAS_LOCAL_NAMES);
                self.add_substs(&substs.func_substs);
                self.add_tys(&substs.upvar_tys);
            }

            &ty::TyInfer(infer) => {
                self.add_flags(TypeFlags::HAS_LOCAL_NAMES); // it might, right?
                self.add_flags(TypeFlags::HAS_TY_INFER);
                match infer {
                    ty::FreshTy(_) |
                    ty::FreshIntTy(_) |
                    ty::FreshFloatTy(_) => {}
                    _ => self.add_flags(TypeFlags::KEEP_IN_LOCAL_TCX)
                }
            }

            &ty::TyEnum(_, substs) | &ty::TyStruct(_, substs) => {
                self.add_substs(substs);
            }

            &ty::TyProjection(ref data) => {
                self.add_flags(TypeFlags::HAS_PROJECTION);
                self.add_projection_ty(data);
            }

            &ty::TyAnon(_, substs) => {
                self.add_flags(TypeFlags::HAS_PROJECTION);
                self.add_substs(substs);
            }

            &ty::TyTrait(ref obj) => {
                let mut computation = FlagComputation::new();
                computation.add_substs(obj.principal.skip_binder().substs);
                for projection_bound in &obj.projection_bounds {
                    let mut proj_computation = FlagComputation::new();
                    proj_computation.add_existential_projection(&projection_bound.0);
                    self.add_bound_computation(&proj_computation);
                }
                self.add_bound_computation(&computation);
                self.add_region(obj.region_bound);
            }

            &ty::TyBox(tt) | &ty::TyArray(tt, _) | &ty::TySlice(tt) => {
                self.add_ty(tt)
            }

            &ty::TyRawPtr(ref m) => {
                self.add_ty(m.ty);
            }

            &ty::TyRef(r, ref m) => {
                self.add_region(*r);
                self.add_ty(m.ty);
            }

            &ty::TyTuple(ref ts) => {
                self.add_tys(&ts[..]);
            }

            &ty::TyFnDef(_, substs, ref f) => {
                self.add_substs(substs);
                self.add_fn_sig(&f.sig);
            }

            &ty::TyFnPtr(ref f) => {
                self.add_fn_sig(&f.sig);
            }
        }
    }

    fn add_ty(&mut self, ty: Ty) {
        self.add_flags(ty.flags.get());
        self.add_depth(ty.region_depth);
    }

    fn add_tys(&mut self, tys: &[Ty]) {
        for &ty in tys {
            self.add_ty(ty);
        }
    }

    fn add_fn_sig(&mut self, fn_sig: &ty::PolyFnSig) {
        let mut computation = FlagComputation::new();

        computation.add_tys(&fn_sig.0.inputs);
        computation.add_ty(fn_sig.0.output);

        self.add_bound_computation(&computation);
    }

    fn add_region(&mut self, r: ty::Region) {
        match r {
            ty::ReVar(..) => {
                self.add_flags(TypeFlags::HAS_RE_INFER);
                self.add_flags(TypeFlags::KEEP_IN_LOCAL_TCX);
            }
            ty::ReSkolemized(..) => {
                self.add_flags(TypeFlags::HAS_RE_INFER);
                self.add_flags(TypeFlags::HAS_RE_SKOL);
                self.add_flags(TypeFlags::KEEP_IN_LOCAL_TCX);
            }
            ty::ReLateBound(debruijn, _) => { self.add_depth(debruijn.depth); }
            ty::ReEarlyBound(..) => { self.add_flags(TypeFlags::HAS_RE_EARLY_BOUND); }
            ty::ReStatic | ty::ReErased => {}
            _ => { self.add_flags(TypeFlags::HAS_FREE_REGIONS); }
        }

        if !r.is_global() {
            self.add_flags(TypeFlags::HAS_LOCAL_NAMES);
        }
    }

    fn add_existential_projection(&mut self, projection: &ty::ExistentialProjection) {
        self.add_substs(projection.trait_ref.substs);
        self.add_ty(projection.ty);
    }

    fn add_projection_ty(&mut self, projection_ty: &ty::ProjectionTy) {
        self.add_substs(projection_ty.trait_ref.substs);
    }

    fn add_substs(&mut self, substs: &Substs) {
        self.add_tys(&substs.types);
        for &r in &substs.regions {
            self.add_region(r);
        }
    }
}
