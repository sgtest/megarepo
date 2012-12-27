// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use middle::ty;
use middle::typeck::infer::combine::*;
use middle::typeck::infer::lattice::*;
use middle::typeck::infer::sub::Sub;
use middle::typeck::infer::to_str::ToStr;

use syntax::ast::{Many, Once};

enum Glb = combine_fields;  // "greatest lower bound" (common subtype)

impl Glb: combine {
    fn infcx() -> infer_ctxt { self.infcx }
    fn tag() -> ~str { ~"glb" }
    fn a_is_expected() -> bool { self.a_is_expected }

    fn sub() -> Sub { Sub(*self) }
    fn lub() -> Lub { Lub(*self) }
    fn glb() -> Glb { Glb(*self) }

    fn mts(a: ty::mt, b: ty::mt) -> cres<ty::mt> {
        let tcx = self.infcx.tcx;

        debug!("%s.mts(%s, %s)",
               self.tag(),
               mt_to_str(tcx, a),
               mt_to_str(tcx, b));

        match (a.mutbl, b.mutbl) {
          // If one side or both is mut, then the GLB must use
          // the precise type from the mut side.
          (m_mutbl, m_const) => {
            Sub(*self).tys(a.ty, b.ty).chain(|_t| {
                Ok({ty: a.ty, mutbl: m_mutbl})
            })
          }
          (m_const, m_mutbl) => {
            Sub(*self).tys(b.ty, a.ty).chain(|_t| {
                Ok({ty: b.ty, mutbl: m_mutbl})
            })
          }
          (m_mutbl, m_mutbl) => {
            eq_tys(&self, a.ty, b.ty).then(|| {
                Ok({ty: a.ty, mutbl: m_mutbl})
            })
          }

          // If one side or both is immutable, we can use the GLB of
          // both sides but mutbl must be `m_imm`.
          (m_imm, m_const) |
          (m_const, m_imm) |
          (m_imm, m_imm) => {
            self.tys(a.ty, b.ty).chain(|t| {
                Ok({ty: t, mutbl: m_imm})
            })
          }

          // If both sides are const, then we can use GLB of both
          // sides and mutbl of only `m_const`.
          (m_const, m_const) => {
            self.tys(a.ty, b.ty).chain(|t| {
                Ok({ty: t, mutbl: m_const})
            })
          }

          // There is no mutual subtype of these combinations.
          (m_mutbl, m_imm) |
          (m_imm, m_mutbl) => {
              Err(ty::terr_mutability)
          }
        }
    }

    fn contratys(a: ty::t, b: ty::t) -> cres<ty::t> {
        Lub(*self).tys(a, b)
    }

    fn protos(p1: ast::Proto, p2: ast::Proto) -> cres<ast::Proto> {
        if p1 == p2 {Ok(p1)} else {Ok(ast::ProtoBare)}
    }

    fn purities(a: purity, b: purity) -> cres<purity> {
        match (a, b) {
          (pure_fn, _) | (_, pure_fn) => Ok(pure_fn),
          (extern_fn, _) | (_, extern_fn) => Ok(extern_fn),
          (impure_fn, _) | (_, impure_fn) => Ok(impure_fn),
          (unsafe_fn, unsafe_fn) => Ok(unsafe_fn)
        }
    }

    fn oncenesses(a: Onceness, b: Onceness) -> cres<Onceness> {
        match (a, b) {
            (Many, _) | (_, Many) => Ok(Many),
            (Once, Once) => Ok(Once)
        }
    }

    fn ret_styles(r1: ret_style, r2: ret_style) -> cres<ret_style> {
        match (r1, r2) {
          (ast::return_val, ast::return_val) => {
            Ok(ast::return_val)
          }
          (ast::noreturn, _) |
          (_, ast::noreturn) => {
            Ok(ast::noreturn)
          }
        }
    }

    fn regions(a: ty::Region, b: ty::Region) -> cres<ty::Region> {
        debug!("%s.regions(%?, %?)",
               self.tag(),
               a.to_str(self.infcx),
               b.to_str(self.infcx));

        do indent {
            self.infcx.region_vars.glb_regions(self.span, a, b)
        }
    }

    fn contraregions(a: ty::Region, b: ty::Region) -> cres<ty::Region> {
        Lub(*self).regions(a, b)
    }

    fn tys(a: ty::t, b: ty::t) -> cres<ty::t> {
        lattice_tys(&self, a, b)
    }

    // Traits please (FIXME: #2794):

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

    fn fns(a: &ty::FnTy, b: &ty::FnTy) -> cres<ty::FnTy> {
        super_fns(&self, a, b)
    }

    fn fn_metas(a: &ty::FnMeta, b: &ty::FnMeta) -> cres<ty::FnMeta> {
        super_fn_metas(&self, a, b)
    }

    fn fn_sigs(a: &ty::FnSig, b: &ty::FnSig) -> cres<ty::FnSig> {
        super_fn_sigs(&self, a, b)
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

