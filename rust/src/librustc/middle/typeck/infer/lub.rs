// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use middle::ty::RegionVid;
use middle::ty;
use middle::typeck::infer::combine::*;
use middle::typeck::infer::glb::Glb;
use middle::typeck::infer::lattice::*;
use middle::typeck::infer::sub::Sub;
use middle::typeck::infer::to_str::InferStr;
use middle::typeck::isr_alist;
use util::ppaux::mt_to_str;

use std::list;
use syntax::ast::{Many, Once, extern_fn, m_const, impure_fn, noreturn};
use syntax::ast::{pure_fn, ret_style, return_val, unsafe_fn};

fn macros() { include!("macros.rs"); } // FIXME(#3114): Macro import/export.

enum Lub = CombineFields;  // least-upper-bound: common supertype

impl Lub {
    fn bot_ty(b: ty::t) -> cres<ty::t> { Ok(b) }
    fn ty_bot(b: ty::t) -> cres<ty::t> { self.bot_ty(b) } // commutative
}

impl Lub: Combine {
    fn infcx() -> @InferCtxt { self.infcx }
    fn tag() -> ~str { ~"lub" }
    fn a_is_expected() -> bool { self.a_is_expected }
    fn span() -> span { self.span }

    fn sub() -> Sub { Sub(*self) }
    fn lub() -> Lub { Lub(*self) }
    fn glb() -> Glb { Glb(*self) }

    fn mts(a: ty::mt, b: ty::mt) -> cres<ty::mt> {
        let tcx = self.infcx.tcx;

        debug!("%s.mts(%s, %s)",
               self.tag(),
               mt_to_str(tcx, a),
               mt_to_str(tcx, b));

        let m = if a.mutbl == b.mutbl {
            a.mutbl
        } else {
            m_const
        };

        match m {
          m_imm | m_const => {
            self.tys(a.ty, b.ty).chain(|t| Ok(ty::mt {ty: t, mutbl: m}) )
          }

          m_mutbl => {
            self.infcx.try(|| {
                eq_tys(&self, a.ty, b.ty).then(|| {
                    Ok(ty::mt {ty: a.ty, mutbl: m})
                })
            }).chain_err(|_e| {
                self.tys(a.ty, b.ty).chain(|t| {
                    Ok(ty::mt {ty: t, mutbl: m_const})
                })
            })
          }
        }
    }

    fn contratys(a: ty::t, b: ty::t) -> cres<ty::t> {
        Glb(*self).tys(a, b)
    }

    fn purities(a: purity, b: purity) -> cres<purity> {
        match (a, b) {
          (unsafe_fn, _) | (_, unsafe_fn) => Ok(unsafe_fn),
          (impure_fn, _) | (_, impure_fn) => Ok(impure_fn),
          (extern_fn, _) | (_, extern_fn) => Ok(extern_fn),
          (pure_fn, pure_fn) => Ok(pure_fn)
        }
    }

    fn oncenesses(a: Onceness, b: Onceness) -> cres<Onceness> {
        match (a, b) {
            (Once, _) | (_, Once) => Ok(Once),
            (Many, Many) => Ok(Many)
        }
    }

    fn contraregions(a: ty::Region, b: ty::Region) -> cres<ty::Region> {
        return Glb(*self).regions(a, b);
    }

    fn regions(a: ty::Region, b: ty::Region) -> cres<ty::Region> {
        debug!("%s.regions(%?, %?)",
               self.tag(),
               a.inf_str(self.infcx),
               b.inf_str(self.infcx));

        do indent {
            self.infcx.region_vars.lub_regions(self.span, a, b)
        }
    }

    fn fn_sigs(a: &ty::FnSig, b: &ty::FnSig) -> cres<ty::FnSig> {
        // Note: this is a subtle algorithm.  For a full explanation,
        // please see the large comment in `region_inference.rs`.

        // Take a snapshot.  We'll never roll this back, but in later
        // phases we do want to be able to examine "all bindings that
        // were created as part of this type comparison", and making a
        // snapshot is a convenient way to do that.
        let snapshot = self.infcx.region_vars.start_snapshot();

        // Instantiate each bound region with a fresh region variable.
        let (a_with_fresh, a_isr) =
            self.infcx.replace_bound_regions_with_fresh_regions(
                self.span, a);
        let (b_with_fresh, _) =
            self.infcx.replace_bound_regions_with_fresh_regions(
                self.span, b);

        // Collect constraints.
        let sig0 = if_ok!(super_fn_sigs(&self, &a_with_fresh, &b_with_fresh));
        debug!("sig0 = %s", sig0.inf_str(self.infcx));

        // Generalize the regions appearing in sig0 if possible
        let new_vars =
            self.infcx.region_vars.vars_created_since_snapshot(snapshot);
        let sig1 =
            self.infcx.fold_regions_in_sig(
                &sig0,
                |r, _in_fn| generalize_region(&self, snapshot, new_vars,
                                              a_isr, r));
        return Ok(move sig1);

        fn generalize_region(self: &Lub,
                             snapshot: uint,
                             new_vars: &[RegionVid],
                             a_isr: isr_alist,
                             r0: ty::Region) -> ty::Region {
            // Regions that pre-dated the LUB computation stay as they are.
            if !is_var_in_set(new_vars, r0) {
                debug!("generalize_region(r0=%?): not new variable", r0);
                return r0;
            }

            let tainted = self.infcx.region_vars.tainted(snapshot, r0);

            // Variables created during LUB computation which are
            // *related* to regions that pre-date the LUB computation
            // stay as they are.
            if !tainted.all(|r| is_var_in_set(new_vars, *r)) {
                debug!("generalize_region(r0=%?): \
                        non-new-variables found in %?",
                       r0, tainted);
                return r0;
            }

            // Otherwise, the variable must be associated with at
            // least one of the variables representing bound regions
            // in both A and B.  Replace the variable with the "first"
            // bound region from A that we find it to be associated
            // with.
            for list::each(a_isr) |pair| {
                let (a_br, a_r) = *pair;
                if tainted.contains(&a_r) {
                    debug!("generalize_region(r0=%?): \
                            replacing with %?, tainted=%?",
                           r0, a_br, tainted);
                    return ty::re_bound(a_br);
                }
            }

            self.infcx.tcx.sess.span_bug(
                self.span,
                fmt!("Region %? is not associated with \
                      any bound region from A!", r0));
        }
    }

    fn fns(a: &ty::FnTy, b: &ty::FnTy) -> cres<ty::FnTy> {
        super_fns(&self, a, b)
    }

    fn fn_metas(a: &ty::FnMeta, b: &ty::FnMeta) -> cres<ty::FnMeta> {
        super_fn_metas(&self, a, b)
    }

    // Traits please (FIXME: #2794):

    fn protos(p1: ast::Proto, p2: ast::Proto) -> cres<ast::Proto> {
        super_protos(&self, p1, p2)
    }

    fn tys(a: ty::t, b: ty::t) -> cres<ty::t> {
        super_lattice_tys(&self, a, b)
    }

    fn flds(a: ty::field, b: ty::field) -> cres<ty::field> {
        super_flds(&self, a, b)
    }

    fn vstores(vk: ty::terr_vstore_kind,
               a: ty::vstore, b: ty::vstore) -> cres<ty::vstore> {
        super_vstores(&self, vk, a, b)
    }

    fn modes(a: ast::mode, b: ast::mode) -> cres<ast::mode> {
        super_modes(&self, a, b)
    }

    fn args(a: ty::arg, b: ty::arg) -> cres<ty::arg> {
        super_args(&self, a, b)
    }

    fn substs(did: ast::def_id,
              as_: &ty::substs,
              bs: &ty::substs) -> cres<ty::substs> {
        super_substs(&self, did, as_, bs)
    }

    fn tps(as_: &[ty::t], bs: &[ty::t]) -> cres<~[ty::t]> {
        super_tps(&self, as_, bs)
    }

    fn self_tys(a: Option<ty::t>, b: Option<ty::t>) -> cres<Option<ty::t>> {
        super_self_tys(&self, a, b)
    }
}
