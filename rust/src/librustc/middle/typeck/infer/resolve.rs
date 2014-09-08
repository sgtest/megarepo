// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Resolution is the process of removing type variables and replacing
// them with their inferred values.  Unfortunately our inference has
// become fairly complex and so there are a number of options to
// control *just how much* you want to resolve and how you want to do
// it.
//
// # Controlling the scope of resolution
//
// The options resolve_* determine what kinds of variables get
// resolved.  Generally resolution starts with a top-level type
// variable; we will always resolve this.  However, once we have
// resolved that variable, we may end up with a type that still
// contains type variables.  For example, if we resolve `<T0>` we may
// end up with something like `[<T1>]`.  If the option
// `resolve_nested_tvar` is passed, we will then go and recursively
// resolve `<T1>`.
//
// The options `resolve_rvar` controls whether we resolve region
// variables. The options `resolve_fvar` and `resolve_ivar` control
// whether we resolve floating point and integral variables,
// respectively.
//
// # What do if things are unconstrained
//
// Sometimes we will encounter a variable that has no constraints, and
// therefore cannot sensibly be mapped to any particular result.  By
// default, we will leave such variables as is (so you will get back a
// variable in your result).  The options force_* will cause the
// resolution to fail in this case instead, except for the case of
// integral variables, which resolve to `int` if forced.
//
// # resolve_all and force_all
//
// The options are a bit set, so you can use the *_all to resolve or
// force all kinds of variables (including those we may add in the
// future).  If you want to resolve everything but one type, you are
// probably better off writing `resolve_all - resolve_ivar`.


use middle::ty::{FloatVar, FloatVid, IntVar, IntVid, RegionVid, TyVar, TyVid};
use middle::ty::{IntType, UintType};
use middle::ty;
use middle::ty_fold;
use middle::typeck::infer::{cyclic_ty, fixup_err, fres, InferCtxt};
use middle::typeck::infer::{unresolved_int_ty,unresolved_float_ty,unresolved_ty};
use syntax::codemap::Span;
use util::common::indent;
use util::ppaux::{Repr, ty_to_string};

pub static resolve_nested_tvar: uint = 0b0000000001;
pub static resolve_rvar: uint        = 0b0000000010;
pub static resolve_ivar: uint        = 0b0000000100;
pub static resolve_fvar: uint        = 0b0000001000;
pub static resolve_all: uint         = 0b0000001111;
pub static force_tvar: uint          = 0b0000100000;
pub static force_rvar: uint          = 0b0001000000;
pub static force_ivar: uint          = 0b0010000000;
pub static force_fvar: uint          = 0b0100000000;
pub static force_all: uint           = 0b0111100000;

pub static not_regions: uint         = !(force_rvar | resolve_rvar);

pub static try_resolve_tvar_shallow: uint = 0;
pub static resolve_and_force_all_but_regions: uint =
    (resolve_all | force_all) & not_regions;

pub struct ResolveState<'a, 'tcx: 'a> {
    infcx: &'a InferCtxt<'a, 'tcx>,
    modes: uint,
    err: Option<fixup_err>,
    v_seen: Vec<TyVid> ,
    type_depth: uint,
}

pub fn resolver<'a, 'tcx>(infcx: &'a InferCtxt<'a, 'tcx>,
                          modes: uint,
                          _: Option<Span>)
                          -> ResolveState<'a, 'tcx> {
    ResolveState {
        infcx: infcx,
        modes: modes,
        err: None,
        v_seen: Vec::new(),
        type_depth: 0,
    }
}

impl<'a, 'tcx> ty_fold::TypeFolder<'tcx> for ResolveState<'a, 'tcx> {
    fn tcx<'a>(&'a self) -> &'a ty::ctxt<'tcx> {
        self.infcx.tcx
    }

    fn fold_ty(&mut self, t: ty::t) -> ty::t {
        self.resolve_type(t)
    }

    fn fold_region(&mut self, r: ty::Region) -> ty::Region {
        self.resolve_region(r)
    }
}

impl<'a, 'tcx> ResolveState<'a, 'tcx> {
    pub fn should(&mut self, mode: uint) -> bool {
        (self.modes & mode) == mode
    }

    pub fn resolve_type_chk(&mut self,
                            typ: ty::t)
                            -> fres<ty::t> {
        self.err = None;

        debug!("Resolving {} (modes={:x})",
               ty_to_string(self.infcx.tcx, typ),
               self.modes);

        // n.b. This is a hokey mess because the current fold doesn't
        // allow us to pass back errors in any useful way.

        assert!(self.v_seen.is_empty());
        let rty = indent(|| self.resolve_type(typ) );
        assert!(self.v_seen.is_empty());
        match self.err {
          None => {
            debug!("Resolved {} to {} (modes={:x})",
                   ty_to_string(self.infcx.tcx, typ),
                   ty_to_string(self.infcx.tcx, rty),
                   self.modes);
            return Ok(rty);
          }
          Some(e) => return Err(e)
        }
    }

    pub fn resolve_region_chk(&mut self,
                              orig: ty::Region)
                              -> fres<ty::Region> {
        self.err = None;
        let resolved = indent(|| self.resolve_region(orig) );
        match self.err {
          None => Ok(resolved),
          Some(e) => Err(e)
        }
    }

    pub fn resolve_type(&mut self, typ: ty::t) -> ty::t {
        debug!("resolve_type({})", typ.repr(self.infcx.tcx));

        if !ty::type_needs_infer(typ) {
            return typ;
        }

        if self.type_depth > 0 && !self.should(resolve_nested_tvar) {
            return typ;
        }

        match ty::get(typ).sty {
            ty::ty_infer(TyVar(vid)) => {
                self.resolve_ty_var(vid)
            }
            ty::ty_infer(IntVar(vid)) => {
                self.resolve_int_var(vid)
            }
            ty::ty_infer(FloatVar(vid)) => {
                self.resolve_float_var(vid)
            }
            _ => {
                if self.modes & resolve_all == 0 {
                    // if we are only resolving top-level type
                    // variables, and this is not a top-level type
                    // variable, then shortcircuit for efficiency
                    typ
                } else {
                    self.type_depth += 1;
                    let result = ty_fold::super_fold_ty(self, typ);
                    self.type_depth -= 1;
                    result
                }
            }
        }
    }

    pub fn resolve_region(&mut self, orig: ty::Region) -> ty::Region {
        debug!("Resolve_region({})", orig.repr(self.infcx.tcx));
        match orig {
          ty::ReInfer(ty::ReVar(rid)) => self.resolve_region_var(rid),
          _ => orig
        }
    }

    pub fn resolve_region_var(&mut self, rid: RegionVid) -> ty::Region {
        if !self.should(resolve_rvar) {
            return ty::ReInfer(ty::ReVar(rid));
        }
        self.infcx.region_vars.resolve_var(rid)
    }

    pub fn resolve_ty_var(&mut self, vid: TyVid) -> ty::t {
        if self.v_seen.contains(&vid) {
            self.err = Some(cyclic_ty(vid));
            return ty::mk_var(self.infcx.tcx, vid);
        } else {
            self.v_seen.push(vid);
            let tcx = self.infcx.tcx;

            // Nonobvious: prefer the most specific type
            // (i.e., the lower bound) to the more general
            // one.  More general types in Rust (e.g., fn())
            // tend to carry more restrictions or higher
            // perf. penalties, so it pays to know more.

            let t1 = match self.infcx.type_variables.borrow().probe(vid) {
                Some(t) => {
                    self.resolve_type(t)
                }
                None => {
                    if self.should(force_tvar) {
                        self.err = Some(unresolved_ty(vid));
                    }
                    ty::mk_var(tcx, vid)
                }
            };
            self.v_seen.pop().unwrap();
            return t1;
        }
    }

    pub fn resolve_int_var(&mut self, vid: IntVid) -> ty::t {
        if !self.should(resolve_ivar) {
            return ty::mk_int_var(self.infcx.tcx, vid);
        }

        let tcx = self.infcx.tcx;
        let table = &self.infcx.int_unification_table;
        let node = table.borrow_mut().get(tcx, vid);
        match node.value {
          Some(IntType(t)) => ty::mk_mach_int(t),
          Some(UintType(t)) => ty::mk_mach_uint(t),
          None => {
            if self.should(force_ivar) {
                // As a last resort, emit an error.
                self.err = Some(unresolved_int_ty(vid));
            }
            ty::mk_int_var(self.infcx.tcx, vid)
          }
        }
    }

    pub fn resolve_float_var(&mut self, vid: FloatVid) -> ty::t {
        if !self.should(resolve_fvar) {
            return ty::mk_float_var(self.infcx.tcx, vid);
        }

        let tcx = self.infcx.tcx;
        let table = &self.infcx.float_unification_table;
        let node = table.borrow_mut().get(tcx, vid);
        match node.value {
          Some(t) => ty::mk_mach_float(t),
          None => {
            if self.should(force_fvar) {
                // As a last resort, emit an error.
                self.err = Some(unresolved_float_ty(vid));
            }
            ty::mk_float_var(self.infcx.tcx, vid)
          }
        }
    }
}
