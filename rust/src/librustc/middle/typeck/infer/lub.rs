// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use middle::ty::{BuiltinBounds};
use middle::ty;
use middle::typeck::infer::combine::*;
use middle::typeck::infer::equate::Equate;
use middle::typeck::infer::glb::Glb;
use middle::typeck::infer::higher_ranked::HigherRankedRelations;
use middle::typeck::infer::lattice::*;
use middle::typeck::infer::sub::Sub;
use middle::typeck::infer::{cres, InferCtxt};
use middle::typeck::infer::{TypeTrace, Subtype};
use syntax::ast::{Many, Once};
use syntax::ast::{NormalFn, UnsafeFn};
use syntax::ast::{Onceness, FnStyle};
use syntax::ast::{MutMutable, MutImmutable};
use util::ppaux::mt_to_string;
use util::ppaux::Repr;

/// "Least upper bound" (common supertype)
pub struct Lub<'f, 'tcx: 'f> {
    fields: CombineFields<'f, 'tcx>
}

#[allow(non_snake_case)]
pub fn Lub<'f, 'tcx>(cf: CombineFields<'f, 'tcx>) -> Lub<'f, 'tcx> {
    Lub { fields: cf }
}

impl<'f, 'tcx> Combine<'tcx> for Lub<'f, 'tcx> {
    fn infcx<'a>(&'a self) -> &'a InferCtxt<'a, 'tcx> { self.fields.infcx }
    fn tag(&self) -> String { "lub".to_string() }
    fn a_is_expected(&self) -> bool { self.fields.a_is_expected }
    fn trace(&self) -> TypeTrace { self.fields.trace.clone() }

    fn equate<'a>(&'a self) -> Equate<'a, 'tcx> { Equate(self.fields.clone()) }
    fn sub<'a>(&'a self) -> Sub<'a, 'tcx> { Sub(self.fields.clone()) }
    fn lub<'a>(&'a self) -> Lub<'a, 'tcx> { Lub(self.fields.clone()) }
    fn glb<'a>(&'a self) -> Glb<'a, 'tcx> { Glb(self.fields.clone()) }

    fn mts(&self, a: &ty::mt, b: &ty::mt) -> cres<ty::mt> {
        let tcx = self.tcx();

        debug!("{}.mts({}, {})",
               self.tag(),
               mt_to_string(tcx, a),
               mt_to_string(tcx, b));

        if a.mutbl != b.mutbl {
            return Err(ty::terr_mutability)
        }

        let m = a.mutbl;
        match m {
            MutImmutable => {
                let t = try!(self.tys(a.ty, b.ty));
                Ok(ty::mt {ty: t, mutbl: m})
            }

            MutMutable => {
                let t = try!(self.equate().tys(a.ty, b.ty));
                Ok(ty::mt {ty: t, mutbl: m})
            }
        }
    }

    fn contratys(&self, a: ty::t, b: ty::t) -> cres<ty::t> {
        self.glb().tys(a, b)
    }

    fn fn_styles(&self, a: FnStyle, b: FnStyle) -> cres<FnStyle> {
        match (a, b) {
          (UnsafeFn, _) | (_, UnsafeFn) => Ok(UnsafeFn),
          (NormalFn, NormalFn) => Ok(NormalFn),
        }
    }

    fn oncenesses(&self, a: Onceness, b: Onceness) -> cres<Onceness> {
        match (a, b) {
            (Once, _) | (_, Once) => Ok(Once),
            (Many, Many) => Ok(Many)
        }
    }

    fn builtin_bounds(&self,
                      a: ty::BuiltinBounds,
                      b: ty::BuiltinBounds)
                      -> cres<ty::BuiltinBounds> {
        // More bounds is a subtype of fewer bounds, so
        // the LUB (mutual supertype) is the intersection.
        Ok(a.intersection(b))
    }

    fn contraregions(&self, a: ty::Region, b: ty::Region)
                    -> cres<ty::Region> {
        self.glb().regions(a, b)
    }

    fn regions(&self, a: ty::Region, b: ty::Region) -> cres<ty::Region> {
        debug!("{}.regions({}, {})",
               self.tag(),
               a.repr(self.tcx()),
               b.repr(self.tcx()));

        Ok(self.infcx().region_vars.lub_regions(Subtype(self.trace()), a, b))
    }

    fn fn_sigs(&self, a: &ty::FnSig, b: &ty::FnSig) -> cres<ty::FnSig> {
        self.higher_ranked_lub(a, b)
    }

    fn tys(&self, a: ty::t, b: ty::t) -> cres<ty::t> {
        super_lattice_tys(self, a, b)
    }

    fn trait_refs(&self, a: &ty::TraitRef, b: &ty::TraitRef) -> cres<ty::TraitRef> {
        self.higher_ranked_lub(a, b)
    }
}
