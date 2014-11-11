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
use middle::ty::RegionVid;
use middle::ty;
use middle::typeck::infer::combine::*;
use middle::typeck::infer::equate::Equate;
use middle::typeck::infer::glb::Glb;
use middle::typeck::infer::lattice::*;
use middle::typeck::infer::sub::Sub;
use middle::typeck::infer::{cres, InferCtxt};
use middle::typeck::infer::fold_regions_in_sig;
use middle::typeck::infer::{TypeTrace, Subtype};
use middle::typeck::infer::region_inference::RegionMark;
use syntax::ast::{Many, Once, NodeId};
use syntax::ast::{NormalFn, UnsafeFn};
use syntax::ast::{Onceness, FnStyle};
use syntax::ast::{MutMutable, MutImmutable};
use util::nodemap::FnvHashMap;
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
        let tcx = self.fields.infcx.tcx;

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
               a.repr(self.fields.infcx.tcx),
               b.repr(self.fields.infcx.tcx));

        Ok(self.fields.infcx.region_vars.lub_regions(Subtype(self.trace()), a, b))
    }

    fn fn_sigs(&self, a: &ty::FnSig, b: &ty::FnSig) -> cres<ty::FnSig> {
        // Note: this is a subtle algorithm.  For a full explanation,
        // please see the large comment in `region_inference.rs`.

        // Make a mark so we can examine "all bindings that were
        // created as part of this type comparison".
        let mark = self.fields.infcx.region_vars.mark();

        // Instantiate each bound region with a fresh region variable.
        let (a_with_fresh, a_map) =
            self.fields.infcx.replace_late_bound_regions_with_fresh_regions(
                self.trace(), a);
        let (b_with_fresh, _) =
            self.fields.infcx.replace_late_bound_regions_with_fresh_regions(
                self.trace(), b);

        // Collect constraints.
        let sig0 = try!(super_fn_sigs(self, &a_with_fresh, &b_with_fresh));
        debug!("sig0 = {}", sig0.repr(self.fields.infcx.tcx));

        // Generalize the regions appearing in sig0 if possible
        let new_vars =
            self.fields.infcx.region_vars.vars_created_since_mark(mark);
        let sig1 =
            fold_regions_in_sig(
                self.fields.infcx.tcx,
                &sig0,
                |r| generalize_region(self, mark, new_vars.as_slice(),
                                      sig0.binder_id, &a_map, r));
        return Ok(sig1);

        fn generalize_region(this: &Lub,
                             mark: RegionMark,
                             new_vars: &[RegionVid],
                             new_scope: NodeId,
                             a_map: &FnvHashMap<ty::BoundRegion, ty::Region>,
                             r0: ty::Region)
                             -> ty::Region {
            // Regions that pre-dated the LUB computation stay as they are.
            if !is_var_in_set(new_vars, r0) {
                assert!(!r0.is_bound());
                debug!("generalize_region(r0={}): not new variable", r0);
                return r0;
            }

            let tainted = this.fields.infcx.region_vars.tainted(mark, r0);

            // Variables created during LUB computation which are
            // *related* to regions that pre-date the LUB computation
            // stay as they are.
            if !tainted.iter().all(|r| is_var_in_set(new_vars, *r)) {
                debug!("generalize_region(r0={}): \
                        non-new-variables found in {}",
                       r0, tainted);
                assert!(!r0.is_bound());
                return r0;
            }

            // Otherwise, the variable must be associated with at
            // least one of the variables representing bound regions
            // in both A and B.  Replace the variable with the "first"
            // bound region from A that we find it to be associated
            // with.
            for (a_br, a_r) in a_map.iter() {
                if tainted.iter().any(|x| x == a_r) {
                    debug!("generalize_region(r0={}): \
                            replacing with {}, tainted={}",
                           r0, *a_br, tainted);
                    return ty::ReLateBound(new_scope, *a_br);
                }
            }

            this.fields.infcx.tcx.sess.span_bug(
                this.fields.trace.origin.span(),
                format!("region {} is not associated with \
                         any bound region from A!",
                        r0).as_slice())
        }
    }

    fn tys(&self, a: ty::t, b: ty::t) -> cres<ty::t> {
        super_lattice_tys(self, a, b)
    }
}
