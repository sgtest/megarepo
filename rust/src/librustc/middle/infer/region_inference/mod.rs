// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! See doc.rs

pub use self::Constraint::*;
pub use self::Verify::*;
pub use self::UndoLogEntry::*;
pub use self::CombineMapType::*;
pub use self::RegionResolutionError::*;
pub use self::VarValue::*;
use self::Classification::*;

use super::cres;
use super::{RegionVariableOrigin, SubregionOrigin, TypeTrace, MiscVariable};

use middle::region;
use middle::ty::{self, Ty};
use middle::ty::{BoundRegion, FreeRegion, Region, RegionVid};
use middle::ty::{ReEmpty, ReStatic, ReInfer, ReFree, ReEarlyBound};
use middle::ty::{ReLateBound, ReScope, ReVar, ReSkolemized, BrFresh};
use middle::graph;
use middle::graph::{Direction, NodeIndex};
use util::common::indenter;
use util::nodemap::{FnvHashMap, FnvHashSet};
use util::ppaux::{Repr, UserString};

use std::cell::{Cell, RefCell};
use std::cmp::Ordering::{self, Less, Greater, Equal};
use std::iter::repeat;
use std::u32;
use syntax::ast;

mod graphviz;

// A constraint that influences the inference process.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Constraint {
    // One region variable is subregion of another
    ConstrainVarSubVar(RegionVid, RegionVid),

    // Concrete region is subregion of region variable
    ConstrainRegSubVar(Region, RegionVid),

    // Region variable is subregion of concrete region
    ConstrainVarSubReg(RegionVid, Region),
}

// Something we have to verify after region inference is done, but
// which does not directly influence the inference process
pub enum Verify<'tcx> {
    // VerifyRegSubReg(a, b): Verify that `a <= b`. Neither `a` nor
    // `b` are inference variables.
    VerifyRegSubReg(SubregionOrigin<'tcx>, Region, Region),

    // VerifyGenericBound(T, _, R, RS): The parameter type `T` (or
    // associated type) must outlive the region `R`. `T` is known to
    // outlive `RS`. Therefore verify that `R <= RS[i]` for some
    // `i`. Inference variables may be involved (but this verification
    // step doesn't influence inference).
    VerifyGenericBound(GenericKind<'tcx>, SubregionOrigin<'tcx>, Region, Vec<Region>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenericKind<'tcx> {
    Param(ty::ParamTy),
    Projection(ty::ProjectionTy<'tcx>),
}

#[derive(Copy, PartialEq, Eq, Hash)]
pub struct TwoRegions {
    a: Region,
    b: Region,
}

#[derive(Copy, PartialEq)]
pub enum UndoLogEntry {
    OpenSnapshot,
    CommitedSnapshot,
    AddVar(RegionVid),
    AddConstraint(Constraint),
    AddVerify(uint),
    AddGiven(ty::FreeRegion, ty::RegionVid),
    AddCombination(CombineMapType, TwoRegions)
}

#[derive(Copy, PartialEq)]
pub enum CombineMapType {
    Lub, Glb
}

#[derive(Clone, Debug)]
pub enum RegionResolutionError<'tcx> {
    /// `ConcreteFailure(o, a, b)`:
    ///
    /// `o` requires that `a <= b`, but this does not hold
    ConcreteFailure(SubregionOrigin<'tcx>, Region, Region),

    /// `GenericBoundFailure(p, s, a, bs)
    ///
    /// The parameter/associated-type `p` must be known to outlive the lifetime
    /// `a`, but it is only known to outlive `bs` (and none of the
    /// regions in `bs` outlive `a`).
    GenericBoundFailure(SubregionOrigin<'tcx>, GenericKind<'tcx>, Region, Vec<Region>),

    /// `SubSupConflict(v, sub_origin, sub_r, sup_origin, sup_r)`:
    ///
    /// Could not infer a value for `v` because `sub_r <= v` (due to
    /// `sub_origin`) but `v <= sup_r` (due to `sup_origin`) and
    /// `sub_r <= sup_r` does not hold.
    SubSupConflict(RegionVariableOrigin<'tcx>,
                   SubregionOrigin<'tcx>, Region,
                   SubregionOrigin<'tcx>, Region),

    /// `SupSupConflict(v, origin1, r1, origin2, r2)`:
    ///
    /// Could not infer a value for `v` because `v <= r1` (due to
    /// `origin1`) and `v <= r2` (due to `origin2`) and
    /// `r1` and `r2` have no intersection.
    SupSupConflict(RegionVariableOrigin<'tcx>,
                   SubregionOrigin<'tcx>, Region,
                   SubregionOrigin<'tcx>, Region),

    /// For subsets of `ConcreteFailure` and `SubSupConflict`, we can derive
    /// more specific errors message by suggesting to the user where they
    /// should put a lifetime. In those cases we process and put those errors
    /// into `ProcessedErrors` before we do any reporting.
    ProcessedErrors(Vec<RegionVariableOrigin<'tcx>>,
                    Vec<(TypeTrace<'tcx>, ty::type_err<'tcx>)>,
                    Vec<SameRegions>),
}

/// SameRegions is used to group regions that we think are the same and would
/// like to indicate so to the user.
/// For example, the following function
/// ```
/// struct Foo { bar: int }
/// fn foo2<'a, 'b>(x: &'a Foo) -> &'b int {
///    &x.bar
/// }
/// ```
/// would report an error because we expect 'a and 'b to match, and so we group
/// 'a and 'b together inside a SameRegions struct
#[derive(Clone, Debug)]
pub struct SameRegions {
    pub scope_id: ast::NodeId,
    pub regions: Vec<BoundRegion>
}

impl SameRegions {
    pub fn contains(&self, other: &BoundRegion) -> bool {
        self.regions.contains(other)
    }

    pub fn push(&mut self, other: BoundRegion) {
        self.regions.push(other);
    }
}

pub type CombineMap = FnvHashMap<TwoRegions, RegionVid>;

pub struct RegionVarBindings<'a, 'tcx: 'a> {
    tcx: &'a ty::ctxt<'tcx>,
    var_origins: RefCell<Vec<RegionVariableOrigin<'tcx>>>,

    // Constraints of the form `A <= B` introduced by the region
    // checker.  Here at least one of `A` and `B` must be a region
    // variable.
    constraints: RefCell<FnvHashMap<Constraint, SubregionOrigin<'tcx>>>,

    // A "verify" is something that we need to verify after inference is
    // done, but which does not directly affect inference in any way.
    //
    // An example is a `A <= B` where neither `A` nor `B` are
    // inference variables.
    verifys: RefCell<Vec<Verify<'tcx>>>,

    // A "given" is a relationship that is known to hold. In particular,
    // we often know from closure fn signatures that a particular free
    // region must be a subregion of a region variable:
    //
    //    foo.iter().filter(<'a> |x: &'a &'b T| ...)
    //
    // In situations like this, `'b` is in fact a region variable
    // introduced by the call to `iter()`, and `'a` is a bound region
    // on the closure (as indicated by the `<'a>` prefix). If we are
    // naive, we wind up inferring that `'b` must be `'static`,
    // because we require that it be greater than `'a` and we do not
    // know what `'a` is precisely.
    //
    // This hashmap is used to avoid that naive scenario. Basically we
    // record the fact that `'a <= 'b` is implied by the fn signature,
    // and then ignore the constraint when solving equations. This is
    // a bit of a hack but seems to work.
    givens: RefCell<FnvHashSet<(ty::FreeRegion, ty::RegionVid)>>,

    lubs: RefCell<CombineMap>,
    glbs: RefCell<CombineMap>,
    skolemization_count: Cell<u32>,
    bound_count: Cell<u32>,

    // The undo log records actions that might later be undone.
    //
    // Note: when the undo_log is empty, we are not actively
    // snapshotting. When the `start_snapshot()` method is called, we
    // push an OpenSnapshot entry onto the list to indicate that we
    // are now actively snapshotting. The reason for this is that
    // otherwise we end up adding entries for things like the lower
    // bound on a variable and so forth, which can never be rolled
    // back.
    undo_log: RefCell<Vec<UndoLogEntry>>,

    // This contains the results of inference.  It begins as an empty
    // option and only acquires a value after inference is complete.
    values: RefCell<Option<Vec<VarValue>>>,
}

#[derive(Debug)]
pub struct RegionSnapshot {
    length: uint,
    skolemization_count: u32,
}

impl<'a, 'tcx> RegionVarBindings<'a, 'tcx> {
    pub fn new(tcx: &'a ty::ctxt<'tcx>) -> RegionVarBindings<'a, 'tcx> {
        RegionVarBindings {
            tcx: tcx,
            var_origins: RefCell::new(Vec::new()),
            values: RefCell::new(None),
            constraints: RefCell::new(FnvHashMap()),
            verifys: RefCell::new(Vec::new()),
            givens: RefCell::new(FnvHashSet()),
            lubs: RefCell::new(FnvHashMap()),
            glbs: RefCell::new(FnvHashMap()),
            skolemization_count: Cell::new(0),
            bound_count: Cell::new(0),
            undo_log: RefCell::new(Vec::new())
        }
    }

    fn in_snapshot(&self) -> bool {
        self.undo_log.borrow().len() > 0
    }

    pub fn start_snapshot(&self) -> RegionSnapshot {
        let length = self.undo_log.borrow().len();
        debug!("RegionVarBindings: start_snapshot({})", length);
        self.undo_log.borrow_mut().push(OpenSnapshot);
        RegionSnapshot { length: length, skolemization_count: self.skolemization_count.get() }
    }

    pub fn commit(&self, snapshot: RegionSnapshot) {
        debug!("RegionVarBindings: commit({})", snapshot.length);
        assert!(self.undo_log.borrow().len() > snapshot.length);
        assert!((*self.undo_log.borrow())[snapshot.length] == OpenSnapshot);

        let mut undo_log = self.undo_log.borrow_mut();
        if snapshot.length == 0 {
            undo_log.truncate(0);
        } else {
            (*undo_log)[snapshot.length] = CommitedSnapshot;
        }
        self.skolemization_count.set(snapshot.skolemization_count);
    }

    pub fn rollback_to(&self, snapshot: RegionSnapshot) {
        debug!("RegionVarBindings: rollback_to({:?})", snapshot);
        let mut undo_log = self.undo_log.borrow_mut();
        assert!(undo_log.len() > snapshot.length);
        assert!((*undo_log)[snapshot.length] == OpenSnapshot);
        while undo_log.len() > snapshot.length + 1 {
            match undo_log.pop().unwrap() {
                OpenSnapshot => {
                    panic!("Failure to observe stack discipline");
                }
                CommitedSnapshot => { }
                AddVar(vid) => {
                    let mut var_origins = self.var_origins.borrow_mut();
                    var_origins.pop().unwrap();
                    assert_eq!(var_origins.len(), vid.index as uint);
                }
                AddConstraint(ref constraint) => {
                    self.constraints.borrow_mut().remove(constraint);
                }
                AddVerify(index) => {
                    self.verifys.borrow_mut().pop();
                    assert_eq!(self.verifys.borrow().len(), index);
                }
                AddGiven(sub, sup) => {
                    self.givens.borrow_mut().remove(&(sub, sup));
                }
                AddCombination(Glb, ref regions) => {
                    self.glbs.borrow_mut().remove(regions);
                }
                AddCombination(Lub, ref regions) => {
                    self.lubs.borrow_mut().remove(regions);
                }
            }
        }
        let c = undo_log.pop().unwrap();
        assert!(c == OpenSnapshot);
        self.skolemization_count.set(snapshot.skolemization_count);
    }

    pub fn num_vars(&self) -> u32 {
        let len = self.var_origins.borrow().len();
        // enforce no overflow
        assert!(len as u32 as uint == len);
        len as u32
    }

    pub fn new_region_var(&self, origin: RegionVariableOrigin<'tcx>) -> RegionVid {
        let id = self.num_vars();
        self.var_origins.borrow_mut().push(origin.clone());
        let vid = RegionVid { index: id };
        if self.in_snapshot() {
            self.undo_log.borrow_mut().push(AddVar(vid));
        }
        debug!("created new region variable {:?} with origin {}",
               vid, origin.repr(self.tcx));
        return vid;
    }

    /// Creates a new skolemized region. Skolemized regions are fresh
    /// regions used when performing higher-ranked computations. They
    /// must be used in a very particular way and are never supposed
    /// to "escape" out into error messages or the code at large.
    ///
    /// The idea is to always create a snapshot. Skolemized regions
    /// can be created in the context of this snapshot, but once the
    /// snapshot is committed or rolled back, their numbers will be
    /// recycled, so you must be finished with them. See the extensive
    /// comments in `higher_ranked.rs` to see how it works (in
    /// particular, the subtyping comparison).
    ///
    /// The `snapshot` argument to this function is not really used;
    /// it's just there to make it explicit which snapshot bounds the
    /// skolemized region that results.
    pub fn new_skolemized(&self, br: ty::BoundRegion, snapshot: &RegionSnapshot) -> Region {
        assert!(self.in_snapshot());
        assert!(self.undo_log.borrow()[snapshot.length] == OpenSnapshot);

        let sc = self.skolemization_count.get();
        self.skolemization_count.set(sc + 1);
        ReInfer(ReSkolemized(sc, br))
    }

    pub fn new_bound(&self, debruijn: ty::DebruijnIndex) -> Region {
        // Creates a fresh bound variable for use in GLB computations.
        // See discussion of GLB computation in the large comment at
        // the top of this file for more details.
        //
        // This computation is potentially wrong in the face of
        // rollover.  It's conceivable, if unlikely, that one might
        // wind up with accidental capture for nested functions in
        // that case, if the outer function had bound regions created
        // a very long time before and the inner function somehow
        // wound up rolling over such that supposedly fresh
        // identifiers were in fact shadowed. For now, we just assert
        // that there is no rollover -- eventually we should try to be
        // robust against this possibility, either by checking the set
        // of bound identifiers that appear in a given expression and
        // ensure that we generate one that is distinct, or by
        // changing the representation of bound regions in a fn
        // declaration

        let sc = self.bound_count.get();
        self.bound_count.set(sc + 1);

        if sc >= self.bound_count.get() {
            self.tcx.sess.bug("rollover in RegionInference new_bound()");
        }

        ReLateBound(debruijn, BrFresh(sc))
    }

    fn values_are_none(&self) -> bool {
        self.values.borrow().is_none()
    }

    fn add_constraint(&self,
                      constraint: Constraint,
                      origin: SubregionOrigin<'tcx>) {
        // cannot add constraints once regions are resolved
        assert!(self.values_are_none());

        debug!("RegionVarBindings: add_constraint({})",
               constraint.repr(self.tcx));

        if self.constraints.borrow_mut().insert(constraint, origin).is_none() {
            if self.in_snapshot() {
                self.undo_log.borrow_mut().push(AddConstraint(constraint));
            }
        }
    }

    fn add_verify(&self,
                  verify: Verify<'tcx>) {
        // cannot add verifys once regions are resolved
        assert!(self.values_are_none());

        debug!("RegionVarBindings: add_verify({})",
               verify.repr(self.tcx));

        let mut verifys = self.verifys.borrow_mut();
        let index = verifys.len();
        verifys.push(verify);
        if self.in_snapshot() {
            self.undo_log.borrow_mut().push(AddVerify(index));
        }
    }

    pub fn add_given(&self,
                     sub: ty::FreeRegion,
                     sup: ty::RegionVid) {
        // cannot add givens once regions are resolved
        assert!(self.values_are_none());

        let mut givens = self.givens.borrow_mut();
        if givens.insert((sub, sup)) {
            debug!("add_given({} <= {:?})",
                   sub.repr(self.tcx),
                   sup);

            self.undo_log.borrow_mut().push(AddGiven(sub, sup));
        }
    }

    pub fn make_eqregion(&self,
                         origin: SubregionOrigin<'tcx>,
                         sub: Region,
                         sup: Region) {
        if sub != sup {
            // Eventually, it would be nice to add direct support for
            // equating regions.
            self.make_subregion(origin.clone(), sub, sup);
            self.make_subregion(origin, sup, sub);
        }
    }

    pub fn make_subregion(&self,
                          origin: SubregionOrigin<'tcx>,
                          sub: Region,
                          sup: Region) {
        // cannot add constraints once regions are resolved
        assert!(self.values_are_none());

        debug!("RegionVarBindings: make_subregion({}, {}) due to {}",
               sub.repr(self.tcx),
               sup.repr(self.tcx),
               origin.repr(self.tcx));

        match (sub, sup) {
          (ReEarlyBound(..), ReEarlyBound(..)) => {
            // This case is used only to make sure that explicitly-specified
            // `Self` types match the real self type in implementations.
            //
            // FIXME(NDM) -- we really shouldn't be comparing bound things
            self.add_verify(VerifyRegSubReg(origin, sub, sup));
          }
          (ReEarlyBound(..), _) |
          (ReLateBound(..), _) |
          (_, ReEarlyBound(..)) |
          (_, ReLateBound(..)) => {
            self.tcx.sess.span_bug(
                origin.span(),
                &format!("cannot relate bound region: {} <= {}",
                        sub.repr(self.tcx),
                        sup.repr(self.tcx))[]);
          }
          (_, ReStatic) => {
            // all regions are subregions of static, so we can ignore this
          }
          (ReInfer(ReVar(sub_id)), ReInfer(ReVar(sup_id))) => {
            self.add_constraint(ConstrainVarSubVar(sub_id, sup_id), origin);
          }
          (r, ReInfer(ReVar(sup_id))) => {
            self.add_constraint(ConstrainRegSubVar(r, sup_id), origin);
          }
          (ReInfer(ReVar(sub_id)), r) => {
            self.add_constraint(ConstrainVarSubReg(sub_id, r), origin);
          }
          _ => {
            self.add_verify(VerifyRegSubReg(origin, sub, sup));
          }
        }
    }

    /// See `Verify::VerifyGenericBound`
    pub fn verify_generic_bound(&self,
                                origin: SubregionOrigin<'tcx>,
                                kind: GenericKind<'tcx>,
                                sub: Region,
                                sups: Vec<Region>) {
        self.add_verify(VerifyGenericBound(kind, origin, sub, sups));
    }

    pub fn lub_regions(&self,
                       origin: SubregionOrigin<'tcx>,
                       a: Region,
                       b: Region)
                       -> Region {
        // cannot add constraints once regions are resolved
        assert!(self.values_are_none());

        debug!("RegionVarBindings: lub_regions({}, {})",
               a.repr(self.tcx),
               b.repr(self.tcx));
        match (a, b) {
            (ReStatic, _) | (_, ReStatic) => {
                ReStatic // nothing lives longer than static
            }

            _ => {
                self.combine_vars(
                    Lub, a, b, origin.clone(),
                    |this, old_r, new_r|
                    this.make_subregion(origin.clone(), old_r, new_r))
            }
        }
    }

    pub fn glb_regions(&self,
                       origin: SubregionOrigin<'tcx>,
                       a: Region,
                       b: Region)
                       -> Region {
        // cannot add constraints once regions are resolved
        assert!(self.values_are_none());

        debug!("RegionVarBindings: glb_regions({}, {})",
               a.repr(self.tcx),
               b.repr(self.tcx));
        match (a, b) {
            (ReStatic, r) | (r, ReStatic) => {
                // static lives longer than everything else
                r
            }

            _ => {
                self.combine_vars(
                    Glb, a, b, origin.clone(),
                    |this, old_r, new_r|
                    this.make_subregion(origin.clone(), new_r, old_r))
            }
        }
    }

    pub fn resolve_var(&self, rid: RegionVid) -> ty::Region {
        match *self.values.borrow() {
            None => {
                self.tcx.sess.span_bug(
                    (*self.var_origins.borrow())[rid.index as uint].span(),
                    "attempt to resolve region variable before values have \
                     been computed!")
            }
            Some(ref values) => {
                let r = lookup(values, rid);
                debug!("resolve_var({:?}) = {}", rid, r.repr(self.tcx));
                r
            }
        }
    }

    fn combine_map(&self, t: CombineMapType)
                   -> &RefCell<CombineMap> {
        match t {
            Glb => &self.glbs,
            Lub => &self.lubs,
        }
    }

    pub fn combine_vars<F>(&self,
                           t: CombineMapType,
                           a: Region,
                           b: Region,
                           origin: SubregionOrigin<'tcx>,
                           mut relate: F)
                           -> Region where
        F: FnMut(&RegionVarBindings<'a, 'tcx>, Region, Region),
    {
        let vars = TwoRegions { a: a, b: b };
        match self.combine_map(t).borrow().get(&vars) {
            Some(&c) => {
                return ReInfer(ReVar(c));
            }
            None => {}
        }
        let c = self.new_region_var(MiscVariable(origin.span()));
        self.combine_map(t).borrow_mut().insert(vars, c);
        if self.in_snapshot() {
            self.undo_log.borrow_mut().push(AddCombination(t, vars));
        }
        relate(self, a, ReInfer(ReVar(c)));
        relate(self, b, ReInfer(ReVar(c)));
        debug!("combine_vars() c={:?}", c);
        ReInfer(ReVar(c))
    }

    pub fn vars_created_since_snapshot(&self, mark: &RegionSnapshot)
                                       -> Vec<RegionVid>
    {
        self.undo_log.borrow()[mark.length..]
            .iter()
            .filter_map(|&elt| match elt {
                AddVar(vid) => Some(vid),
                _ => None
            })
            .collect()
    }

    /// Computes all regions that have been related to `r0` in any way since the mark `mark` was
    /// made---`r0` itself will be the first entry. This is used when checking whether skolemized
    /// regions are being improperly related to other regions.
    pub fn tainted(&self, mark: &RegionSnapshot, r0: Region) -> Vec<Region> {
        debug!("tainted(mark={:?}, r0={})", mark, r0.repr(self.tcx));
        let _indenter = indenter();

        // `result_set` acts as a worklist: we explore all outgoing
        // edges and add any new regions we find to result_set.  This
        // is not a terribly efficient implementation.
        let mut result_set = vec!(r0);
        let mut result_index = 0;
        while result_index < result_set.len() {
            // nb: can't use uint::range() here because result_set grows
            let r = result_set[result_index];
            debug!("result_index={}, r={:?}", result_index, r);

            for undo_entry in
                self.undo_log.borrow()[mark.length..].iter()
            {
                match undo_entry {
                    &AddConstraint(ConstrainVarSubVar(a, b)) => {
                        consider_adding_bidirectional_edges(
                            &mut result_set, r,
                            ReInfer(ReVar(a)), ReInfer(ReVar(b)));
                    }
                    &AddConstraint(ConstrainRegSubVar(a, b)) => {
                        consider_adding_bidirectional_edges(
                            &mut result_set, r,
                            a, ReInfer(ReVar(b)));
                    }
                    &AddConstraint(ConstrainVarSubReg(a, b)) => {
                        consider_adding_bidirectional_edges(
                            &mut result_set, r,
                            ReInfer(ReVar(a)), b);
                    }
                    &AddGiven(a, b) => {
                        consider_adding_bidirectional_edges(
                            &mut result_set, r,
                            ReFree(a), ReInfer(ReVar(b)));
                    }
                    &AddVerify(i) => {
                        match (*self.verifys.borrow())[i] {
                            VerifyRegSubReg(_, a, b) => {
                                consider_adding_bidirectional_edges(
                                    &mut result_set, r,
                                    a, b);
                            }
                            VerifyGenericBound(_, _, a, ref bs) => {
                                for &b in bs {
                                    consider_adding_bidirectional_edges(
                                        &mut result_set, r,
                                        a, b);
                                }
                            }
                        }
                    }
                    &AddCombination(..) |
                    &AddVar(..) |
                    &OpenSnapshot |
                    &CommitedSnapshot => {
                    }
                }
            }

            result_index += 1;
        }

        return result_set;

        fn consider_adding_bidirectional_edges(result_set: &mut Vec<Region>,
                                               r: Region,
                                               r1: Region,
                                               r2: Region) {
            consider_adding_directed_edge(result_set, r, r1, r2);
            consider_adding_directed_edge(result_set, r, r2, r1);
        }

        fn consider_adding_directed_edge(result_set: &mut Vec<Region>,
                                         r: Region,
                                         r1: Region,
                                         r2: Region) {
            if r == r1 {
                // Clearly, this is potentially inefficient.
                if !result_set.iter().any(|x| *x == r2) {
                    result_set.push(r2);
                }
            }
        }
    }

    /// This function performs the actual region resolution.  It must be
    /// called after all constraints have been added.  It performs a
    /// fixed-point iteration to find region values which satisfy all
    /// constraints, assuming such values can be found; if they cannot,
    /// errors are reported.
    pub fn resolve_regions(&self, subject_node: ast::NodeId) -> Vec<RegionResolutionError<'tcx>> {
        debug!("RegionVarBindings: resolve_regions()");
        let mut errors = vec!();
        let v = self.infer_variable_values(&mut errors, subject_node);
        *self.values.borrow_mut() = Some(v);
        errors
    }

    fn is_subregion_of(&self, sub: Region, sup: Region) -> bool {
        self.tcx.region_maps.is_subregion_of(sub, sup)
    }

    fn lub_concrete_regions(&self, a: Region, b: Region) -> Region {
        match (a, b) {
          (ReLateBound(..), _) |
          (_, ReLateBound(..)) |
          (ReEarlyBound(..), _) |
          (_, ReEarlyBound(..)) => {
            self.tcx.sess.bug(
                &format!("cannot relate bound region: LUB({}, {})",
                        a.repr(self.tcx),
                        b.repr(self.tcx))[]);
          }

          (ReStatic, _) | (_, ReStatic) => {
            ReStatic // nothing lives longer than static
          }

          (ReEmpty, r) | (r, ReEmpty) => {
            r // everything lives longer than empty
          }

          (ReInfer(ReVar(v_id)), _) | (_, ReInfer(ReVar(v_id))) => {
            self.tcx.sess.span_bug(
                (*self.var_origins.borrow())[v_id.index as uint].span(),
                &format!("lub_concrete_regions invoked with \
                         non-concrete regions: {:?}, {:?}",
                        a,
                        b)[]);
          }

          (ReFree(ref fr), ReScope(s_id)) |
          (ReScope(s_id), ReFree(ref fr)) => {
            let f = ReFree(*fr);
            // A "free" region can be interpreted as "some region
            // at least as big as the block fr.scope_id".  So, we can
            // reasonably compare free regions and scopes:
            let fr_scope = fr.scope.to_code_extent();
            match self.tcx.region_maps.nearest_common_ancestor(fr_scope, s_id) {
              // if the free region's scope `fr.scope_id` is bigger than
              // the scope region `s_id`, then the LUB is the free
              // region itself:
              Some(r_id) if r_id == fr_scope => f,

              // otherwise, we don't know what the free region is,
              // so we must conservatively say the LUB is static:
              _ => ReStatic
            }
          }

          (ReScope(a_id), ReScope(b_id)) => {
            // The region corresponding to an outer block is a
            // subtype of the region corresponding to an inner
            // block.
            match self.tcx.region_maps.nearest_common_ancestor(a_id, b_id) {
              Some(r_id) => ReScope(r_id),
              _ => ReStatic
            }
          }

          (ReFree(ref a_fr), ReFree(ref b_fr)) => {
             self.lub_free_regions(a_fr, b_fr)
          }

          // For these types, we cannot define any additional
          // relationship:
          (ReInfer(ReSkolemized(..)), _) |
          (_, ReInfer(ReSkolemized(..))) => {
            if a == b {a} else {ReStatic}
          }
        }
    }

    /// Computes a region that encloses both free region arguments. Guarantee that if the same two
    /// regions are given as argument, in any order, a consistent result is returned.
    fn lub_free_regions(&self,
                        a: &FreeRegion,
                        b: &FreeRegion) -> ty::Region
    {
        return match a.cmp(b) {
            Less => helper(self, a, b),
            Greater => helper(self, b, a),
            Equal => ty::ReFree(*a)
        };

        fn helper(this: &RegionVarBindings,
                  a: &FreeRegion,
                  b: &FreeRegion) -> ty::Region
        {
            if this.tcx.region_maps.sub_free_region(*a, *b) {
                ty::ReFree(*b)
            } else if this.tcx.region_maps.sub_free_region(*b, *a) {
                ty::ReFree(*a)
            } else {
                ty::ReStatic
            }
        }
    }

    fn glb_concrete_regions(&self,
                            a: Region,
                            b: Region)
                         -> cres<'tcx, Region> {
        debug!("glb_concrete_regions({:?}, {:?})", a, b);
        match (a, b) {
            (ReLateBound(..), _) |
            (_, ReLateBound(..)) |
            (ReEarlyBound(..), _) |
            (_, ReEarlyBound(..)) => {
              self.tcx.sess.bug(
                  &format!("cannot relate bound region: GLB({}, {})",
                          a.repr(self.tcx),
                          b.repr(self.tcx))[]);
            }

            (ReStatic, r) | (r, ReStatic) => {
                // static lives longer than everything else
                Ok(r)
            }

            (ReEmpty, _) | (_, ReEmpty) => {
                // nothing lives shorter than everything else
                Ok(ReEmpty)
            }

            (ReInfer(ReVar(v_id)), _) |
            (_, ReInfer(ReVar(v_id))) => {
                self.tcx.sess.span_bug(
                    (*self.var_origins.borrow())[v_id.index as uint].span(),
                    &format!("glb_concrete_regions invoked with \
                             non-concrete regions: {:?}, {:?}",
                            a,
                            b)[]);
            }

            (ReFree(ref fr), ReScope(s_id)) |
            (ReScope(s_id), ReFree(ref fr)) => {
                let s = ReScope(s_id);
                // Free region is something "at least as big as
                // `fr.scope_id`."  If we find that the scope `fr.scope_id` is bigger
                // than the scope `s_id`, then we can say that the GLB
                // is the scope `s_id`.  Otherwise, as we do not know
                // big the free region is precisely, the GLB is undefined.
                let fr_scope = fr.scope.to_code_extent();
                match self.tcx.region_maps.nearest_common_ancestor(fr_scope, s_id) {
                    Some(r_id) if r_id == fr_scope => Ok(s),
                    _ => Err(ty::terr_regions_no_overlap(b, a))
                }
            }

            (ReScope(a_id), ReScope(b_id)) => {
                self.intersect_scopes(a, b, a_id, b_id)
            }

            (ReFree(ref a_fr), ReFree(ref b_fr)) => {
                self.glb_free_regions(a_fr, b_fr)
            }

            // For these types, we cannot define any additional
            // relationship:
            (ReInfer(ReSkolemized(..)), _) |
            (_, ReInfer(ReSkolemized(..))) => {
                if a == b {
                    Ok(a)
                } else {
                    Err(ty::terr_regions_no_overlap(b, a))
                }
            }
        }
    }

    /// Computes a region that is enclosed by both free region arguments, if any. Guarantees that
    /// if the same two regions are given as argument, in any order, a consistent result is
    /// returned.
    fn glb_free_regions(&self,
                        a: &FreeRegion,
                        b: &FreeRegion) -> cres<'tcx, ty::Region>
    {
        return match a.cmp(b) {
            Less => helper(self, a, b),
            Greater => helper(self, b, a),
            Equal => Ok(ty::ReFree(*a))
        };

        fn helper<'a, 'tcx>(this: &RegionVarBindings<'a, 'tcx>,
                            a: &FreeRegion,
                            b: &FreeRegion) -> cres<'tcx, ty::Region>
        {
            if this.tcx.region_maps.sub_free_region(*a, *b) {
                Ok(ty::ReFree(*a))
            } else if this.tcx.region_maps.sub_free_region(*b, *a) {
                Ok(ty::ReFree(*b))
            } else {
                this.intersect_scopes(ty::ReFree(*a), ty::ReFree(*b),
                                      a.scope.to_code_extent(),
                                      b.scope.to_code_extent())
            }
        }
    }

    fn intersect_scopes(&self,
                        region_a: ty::Region,
                        region_b: ty::Region,
                        scope_a: region::CodeExtent,
                        scope_b: region::CodeExtent) -> cres<'tcx, Region>
    {
        // We want to generate the intersection of two
        // scopes or two free regions.  So, if one of
        // these scopes is a subscope of the other, return
        // it. Otherwise fail.
        debug!("intersect_scopes(scope_a={:?}, scope_b={:?}, region_a={:?}, region_b={:?})",
               scope_a, scope_b, region_a, region_b);
        match self.tcx.region_maps.nearest_common_ancestor(scope_a, scope_b) {
            Some(r_id) if scope_a == r_id => Ok(ReScope(scope_b)),
            Some(r_id) if scope_b == r_id => Ok(ReScope(scope_a)),
            _ => Err(ty::terr_regions_no_overlap(region_a, region_b))
        }
    }
}

// ______________________________________________________________________

#[derive(Copy, PartialEq, Debug)]
enum Classification { Expanding, Contracting }

#[derive(Copy)]
pub enum VarValue { NoValue, Value(Region), ErrorValue }

struct VarData {
    classification: Classification,
    value: VarValue,
}

struct RegionAndOrigin<'tcx> {
    region: Region,
    origin: SubregionOrigin<'tcx>,
}

type RegionGraph = graph::Graph<(), Constraint>;

impl<'a, 'tcx> RegionVarBindings<'a, 'tcx> {
    fn infer_variable_values(&self,
                             errors: &mut Vec<RegionResolutionError<'tcx>>,
                             subject: ast::NodeId) -> Vec<VarValue>
    {
        let mut var_data = self.construct_var_data();

        // Dorky hack to cause `dump_constraints` to only get called
        // if debug mode is enabled:
        debug!("----() End constraint listing {:?}---", self.dump_constraints());
        graphviz::maybe_print_constraints_for(self, subject);

        self.expansion(&mut var_data);
        self.contraction(&mut var_data);
        let values =
            self.extract_values_and_collect_conflicts(&var_data[],
                                                      errors);
        self.collect_concrete_region_errors(&values, errors);
        values
    }

    fn construct_var_data(&self) -> Vec<VarData> {
        (0..self.num_vars() as uint).map(|_| {
            VarData {
                // All nodes are initially classified as contracting; during
                // the expansion phase, we will shift the classification for
                // those nodes that have a concrete region predecessor to
                // Expanding.
                classification: Contracting,
                value: NoValue,
            }
        }).collect()
    }

    fn dump_constraints(&self) {
        debug!("----() Start constraint listing ()----");
        for (idx, (constraint, _)) in self.constraints.borrow().iter().enumerate() {
            debug!("Constraint {} => {}", idx, constraint.repr(self.tcx));
        }
    }

    fn expansion(&self, var_data: &mut [VarData]) {
        self.iterate_until_fixed_point("Expansion", |constraint| {
            debug!("expansion: constraint={} origin={}",
                   constraint.repr(self.tcx),
                   self.constraints.borrow()
                                   .get(constraint)
                                   .unwrap()
                                   .repr(self.tcx));
            match *constraint {
              ConstrainRegSubVar(a_region, b_vid) => {
                let b_data = &mut var_data[b_vid.index as uint];
                self.expand_node(a_region, b_vid, b_data)
              }
              ConstrainVarSubVar(a_vid, b_vid) => {
                match var_data[a_vid.index as uint].value {
                  NoValue | ErrorValue => false,
                  Value(a_region) => {
                    let b_node = &mut var_data[b_vid.index as uint];
                    self.expand_node(a_region, b_vid, b_node)
                  }
                }
              }
              ConstrainVarSubReg(..) => {
                // This is a contraction constraint.  Ignore it.
                false
              }
            }
        })
    }

    fn expand_node(&self,
                   a_region: Region,
                   b_vid: RegionVid,
                   b_data: &mut VarData)
                   -> bool
    {
        debug!("expand_node({}, {:?} == {})",
               a_region.repr(self.tcx),
               b_vid,
               b_data.value.repr(self.tcx));

        // Check if this relationship is implied by a given.
        match a_region {
            ty::ReFree(fr) => {
                if self.givens.borrow().contains(&(fr, b_vid)) {
                    debug!("given");
                    return false;
                }
            }
            _ => { }
        }

        b_data.classification = Expanding;
        match b_data.value {
          NoValue => {
            debug!("Setting initial value of {:?} to {}",
                   b_vid, a_region.repr(self.tcx));

            b_data.value = Value(a_region);
            return true;
          }

          Value(cur_region) => {
            let lub = self.lub_concrete_regions(a_region, cur_region);
            if lub == cur_region {
                return false;
            }

            debug!("Expanding value of {:?} from {} to {}",
                   b_vid,
                   cur_region.repr(self.tcx),
                   lub.repr(self.tcx));

            b_data.value = Value(lub);
            return true;
          }

          ErrorValue => {
            return false;
          }
        }
    }

    fn contraction(&self,
                   var_data: &mut [VarData]) {
        self.iterate_until_fixed_point("Contraction", |constraint| {
            debug!("contraction: constraint={} origin={}",
                   constraint.repr(self.tcx),
                   self.constraints.borrow()
                                   .get(constraint)
                                   .unwrap()
                                   .repr(self.tcx));
            match *constraint {
              ConstrainRegSubVar(..) => {
                // This is an expansion constraint.  Ignore.
                false
              }
              ConstrainVarSubVar(a_vid, b_vid) => {
                match var_data[b_vid.index as uint].value {
                  NoValue | ErrorValue => false,
                  Value(b_region) => {
                    let a_data = &mut var_data[a_vid.index as uint];
                    self.contract_node(a_vid, a_data, b_region)
                  }
                }
              }
              ConstrainVarSubReg(a_vid, b_region) => {
                let a_data = &mut var_data[a_vid.index as uint];
                self.contract_node(a_vid, a_data, b_region)
              }
            }
        })
    }

    fn contract_node(&self,
                     a_vid: RegionVid,
                     a_data: &mut VarData,
                     b_region: Region)
                     -> bool {
        debug!("contract_node({:?} == {}/{:?}, {})",
               a_vid, a_data.value.repr(self.tcx),
               a_data.classification, b_region.repr(self.tcx));

        return match a_data.value {
            NoValue => {
                assert_eq!(a_data.classification, Contracting);
                a_data.value = Value(b_region);
                true // changed
            }

            ErrorValue => {
                false // no change
            }

            Value(a_region) => {
                match a_data.classification {
                    Expanding => {
                        check_node(self, a_vid, a_data, a_region, b_region)
                    }
                    Contracting => {
                        adjust_node(self, a_vid, a_data, a_region, b_region)
                    }
                }
            }
        };

        fn check_node(this: &RegionVarBindings,
                      a_vid: RegionVid,
                      a_data: &mut VarData,
                      a_region: Region,
                      b_region: Region)
                   -> bool {
            if !this.is_subregion_of(a_region, b_region) {
                debug!("Setting {:?} to ErrorValue: {} not subregion of {}",
                       a_vid,
                       a_region.repr(this.tcx),
                       b_region.repr(this.tcx));
                a_data.value = ErrorValue;
            }
            false
        }

        fn adjust_node(this: &RegionVarBindings,
                       a_vid: RegionVid,
                       a_data: &mut VarData,
                       a_region: Region,
                       b_region: Region)
                    -> bool {
            match this.glb_concrete_regions(a_region, b_region) {
                Ok(glb) => {
                    if glb == a_region {
                        false
                    } else {
                        debug!("Contracting value of {:?} from {} to {}",
                               a_vid,
                               a_region.repr(this.tcx),
                               glb.repr(this.tcx));
                        a_data.value = Value(glb);
                        true
                    }
                }
                Err(_) => {
                    debug!("Setting {:?} to ErrorValue: no glb of {}, {}",
                           a_vid,
                           a_region.repr(this.tcx),
                           b_region.repr(this.tcx));
                    a_data.value = ErrorValue;
                    false
                }
            }
        }
    }

    fn collect_concrete_region_errors(&self,
                                      values: &Vec<VarValue>,
                                      errors: &mut Vec<RegionResolutionError<'tcx>>)
    {
        let mut reg_reg_dups = FnvHashSet();
        for verify in &*self.verifys.borrow() {
            match *verify {
                VerifyRegSubReg(ref origin, sub, sup) => {
                    if self.is_subregion_of(sub, sup) {
                        continue;
                    }

                    if !reg_reg_dups.insert((sub, sup)) {
                        continue;
                    }

                    debug!("ConcreteFailure: !(sub <= sup): sub={}, sup={}",
                           sub.repr(self.tcx),
                           sup.repr(self.tcx));
                    errors.push(ConcreteFailure((*origin).clone(), sub, sup));
                }

                VerifyGenericBound(ref kind, ref origin, sub, ref sups) => {
                    let sub = normalize(values, sub);
                    if sups.iter()
                           .map(|&sup| normalize(values, sup))
                           .any(|sup| self.is_subregion_of(sub, sup))
                    {
                        continue;
                    }

                    let sups = sups.iter().map(|&sup| normalize(values, sup))
                                          .collect();
                    errors.push(
                        GenericBoundFailure(
                            (*origin).clone(), kind.clone(), sub, sups));
                }
            }
        }
    }

    fn extract_values_and_collect_conflicts(
        &self,
        var_data: &[VarData],
        errors: &mut Vec<RegionResolutionError<'tcx>>)
        -> Vec<VarValue>
    {
        debug!("extract_values_and_collect_conflicts()");

        // This is the best way that I have found to suppress
        // duplicate and related errors. Basically we keep a set of
        // flags for every node. Whenever an error occurs, we will
        // walk some portion of the graph looking to find pairs of
        // conflicting regions to report to the user. As we walk, we
        // trip the flags from false to true, and if we find that
        // we've already reported an error involving any particular
        // node we just stop and don't report the current error.  The
        // idea is to report errors that derive from independent
        // regions of the graph, but not those that derive from
        // overlapping locations.
        let mut dup_vec: Vec<_> = repeat(u32::MAX).take(self.num_vars() as uint).collect();

        let mut opt_graph = None;

        for idx in 0..self.num_vars() as uint {
            match var_data[idx].value {
                Value(_) => {
                    /* Inference successful */
                }
                NoValue => {
                    /* Unconstrained inference: do not report an error
                       until the value of this variable is requested.
                       After all, sometimes we make region variables but never
                       really use their values. */
                }
                ErrorValue => {
                    /* Inference impossible, this value contains
                       inconsistent constraints.

                       I think that in this case we should report an
                       error now---unlike the case above, we can't
                       wait to see whether the user needs the result
                       of this variable.  The reason is that the mere
                       existence of this variable implies that the
                       region graph is inconsistent, whether or not it
                       is used.

                       For example, we may have created a region
                       variable that is the GLB of two other regions
                       which do not have a GLB.  Even if that variable
                       is not used, it implies that those two regions
                       *should* have a GLB.

                       At least I think this is true. It may be that
                       the mere existence of a conflict in a region variable
                       that is not used is not a problem, so if this rule
                       starts to create problems we'll have to revisit
                       this portion of the code and think hard about it. =) */

                    if opt_graph.is_none() {
                        opt_graph = Some(self.construct_graph());
                    }
                    let graph = opt_graph.as_ref().unwrap();

                    let node_vid = RegionVid { index: idx as u32 };
                    match var_data[idx].classification {
                        Expanding => {
                            self.collect_error_for_expanding_node(
                                graph, var_data, &mut dup_vec,
                                node_vid, errors);
                        }
                        Contracting => {
                            self.collect_error_for_contracting_node(
                                graph, var_data, &mut dup_vec,
                                node_vid, errors);
                        }
                    }
                }
            }
        }

        (0..self.num_vars() as uint).map(|idx| var_data[idx].value).collect()
    }

    fn construct_graph(&self) -> RegionGraph {
        let num_vars = self.num_vars();

        let constraints = self.constraints.borrow();
        let num_edges = constraints.len();

        let mut graph = graph::Graph::with_capacity(num_vars as uint + 1,
                                                    num_edges);

        for _ in 0..num_vars {
            graph.add_node(());
        }
        let dummy_idx = graph.add_node(());

        for (constraint, _) in &*constraints {
            match *constraint {
                ConstrainVarSubVar(a_id, b_id) => {
                    graph.add_edge(NodeIndex(a_id.index as uint),
                                   NodeIndex(b_id.index as uint),
                                   *constraint);
                }
                ConstrainRegSubVar(_, b_id) => {
                    graph.add_edge(dummy_idx,
                                   NodeIndex(b_id.index as uint),
                                   *constraint);
                }
                ConstrainVarSubReg(a_id, _) => {
                    graph.add_edge(NodeIndex(a_id.index as uint),
                                   dummy_idx,
                                   *constraint);
                }
            }
        }

        return graph;
    }

    fn collect_error_for_expanding_node(
        &self,
        graph: &RegionGraph,
        var_data: &[VarData],
        dup_vec: &mut [u32],
        node_idx: RegionVid,
        errors: &mut Vec<RegionResolutionError<'tcx>>)
    {
        // Errors in expanding nodes result from a lower-bound that is
        // not contained by an upper-bound.
        let (mut lower_bounds, lower_dup) =
            self.collect_concrete_regions(graph, var_data, node_idx,
                                          graph::Incoming, dup_vec);
        let (mut upper_bounds, upper_dup) =
            self.collect_concrete_regions(graph, var_data, node_idx,
                                          graph::Outgoing, dup_vec);

        if lower_dup || upper_dup {
            return;
        }

        // We place free regions first because we are special casing
        // SubSupConflict(ReFree, ReFree) when reporting error, and so
        // the user will more likely get a specific suggestion.
        fn free_regions_first(a: &RegionAndOrigin,
                              b: &RegionAndOrigin)
                              -> Ordering {
            match (a.region, b.region) {
                (ReFree(..), ReFree(..)) => Equal,
                (ReFree(..), _) => Less,
                (_, ReFree(..)) => Greater,
                (_, _) => Equal,
            }
        }
        lower_bounds.sort_by(|a, b| { free_regions_first(a, b) });
        upper_bounds.sort_by(|a, b| { free_regions_first(a, b) });

        for lower_bound in &lower_bounds {
            for upper_bound in &upper_bounds {
                if !self.is_subregion_of(lower_bound.region,
                                         upper_bound.region) {
                    debug!("pushing SubSupConflict sub: {:?} sup: {:?}",
                           lower_bound.region, upper_bound.region);
                    errors.push(SubSupConflict(
                        (*self.var_origins.borrow())[node_idx.index as uint].clone(),
                        lower_bound.origin.clone(),
                        lower_bound.region,
                        upper_bound.origin.clone(),
                        upper_bound.region));
                    return;
                }
            }
        }

        self.tcx.sess.span_bug(
            (*self.var_origins.borrow())[node_idx.index as uint].span(),
            &format!("collect_error_for_expanding_node() could not find error \
                    for var {:?}, lower_bounds={}, upper_bounds={}",
                    node_idx,
                    lower_bounds.repr(self.tcx),
                    upper_bounds.repr(self.tcx))[]);
    }

    fn collect_error_for_contracting_node(
        &self,
        graph: &RegionGraph,
        var_data: &[VarData],
        dup_vec: &mut [u32],
        node_idx: RegionVid,
        errors: &mut Vec<RegionResolutionError<'tcx>>)
    {
        // Errors in contracting nodes result from two upper-bounds
        // that have no intersection.
        let (upper_bounds, dup_found) =
            self.collect_concrete_regions(graph, var_data, node_idx,
                                          graph::Outgoing, dup_vec);

        if dup_found {
            return;
        }

        for upper_bound_1 in &upper_bounds {
            for upper_bound_2 in &upper_bounds {
                match self.glb_concrete_regions(upper_bound_1.region,
                                                upper_bound_2.region) {
                  Ok(_) => {}
                  Err(_) => {
                    errors.push(SupSupConflict(
                        (*self.var_origins.borrow())[node_idx.index as uint].clone(),
                        upper_bound_1.origin.clone(),
                        upper_bound_1.region,
                        upper_bound_2.origin.clone(),
                        upper_bound_2.region));
                    return;
                  }
                }
            }
        }

        self.tcx.sess.span_bug(
            (*self.var_origins.borrow())[node_idx.index as uint].span(),
            &format!("collect_error_for_contracting_node() could not find error \
                     for var {:?}, upper_bounds={}",
                    node_idx,
                    upper_bounds.repr(self.tcx))[]);
    }

    fn collect_concrete_regions(&self,
                                graph: &RegionGraph,
                                var_data: &[VarData],
                                orig_node_idx: RegionVid,
                                dir: Direction,
                                dup_vec: &mut [u32])
                                -> (Vec<RegionAndOrigin<'tcx>>, bool) {
        struct WalkState<'tcx> {
            set: FnvHashSet<RegionVid>,
            stack: Vec<RegionVid>,
            result: Vec<RegionAndOrigin<'tcx>>,
            dup_found: bool
        }
        let mut state = WalkState {
            set: FnvHashSet(),
            stack: vec!(orig_node_idx),
            result: Vec::new(),
            dup_found: false
        };
        state.set.insert(orig_node_idx);

        // to start off the process, walk the source node in the
        // direction specified
        process_edges(self, &mut state, graph, orig_node_idx, dir);

        while !state.stack.is_empty() {
            let node_idx = state.stack.pop().unwrap();
            let classification = var_data[node_idx.index as uint].classification;

            // check whether we've visited this node on some previous walk
            if dup_vec[node_idx.index as uint] == u32::MAX {
                dup_vec[node_idx.index as uint] = orig_node_idx.index;
            } else if dup_vec[node_idx.index as uint] != orig_node_idx.index {
                state.dup_found = true;
            }

            debug!("collect_concrete_regions(orig_node_idx={:?}, node_idx={:?}, \
                    classification={:?})",
                   orig_node_idx, node_idx, classification);

            // figure out the direction from which this node takes its
            // values, and search for concrete regions etc in that direction
            let dir = match classification {
                Expanding => graph::Incoming,
                Contracting => graph::Outgoing,
            };

            process_edges(self, &mut state, graph, node_idx, dir);
        }

        let WalkState {result, dup_found, ..} = state;
        return (result, dup_found);

        fn process_edges<'a, 'tcx>(this: &RegionVarBindings<'a, 'tcx>,
                         state: &mut WalkState<'tcx>,
                         graph: &RegionGraph,
                         source_vid: RegionVid,
                         dir: Direction) {
            debug!("process_edges(source_vid={:?}, dir={:?})", source_vid, dir);

            let source_node_index = NodeIndex(source_vid.index as uint);
            graph.each_adjacent_edge(source_node_index, dir, |_, edge| {
                match edge.data {
                    ConstrainVarSubVar(from_vid, to_vid) => {
                        let opp_vid =
                            if from_vid == source_vid {to_vid} else {from_vid};
                        if state.set.insert(opp_vid) {
                            state.stack.push(opp_vid);
                        }
                    }

                    ConstrainRegSubVar(region, _) |
                    ConstrainVarSubReg(_, region) => {
                        state.result.push(RegionAndOrigin {
                            region: region,
                            origin: this.constraints.borrow()[edge.data].clone()
                        });
                    }
                }
                true
            });
        }
    }

    fn iterate_until_fixed_point<F>(&self, tag: &str, mut body: F) where
        F: FnMut(&Constraint) -> bool,
    {
        let mut iteration = 0;
        let mut changed = true;
        while changed {
            changed = false;
            iteration += 1;
            debug!("---- {} Iteration {}{}", "#", tag, iteration);
            for (constraint, _) in &*self.constraints.borrow() {
                let edge_changed = body(constraint);
                if edge_changed {
                    debug!("Updated due to constraint {}",
                           constraint.repr(self.tcx));
                    changed = true;
                }
            }
        }
        debug!("---- {} Complete after {} iteration(s)", tag, iteration);
    }

}

impl<'tcx> Repr<'tcx> for Constraint {
    fn repr(&self, tcx: &ty::ctxt) -> String {
        match *self {
            ConstrainVarSubVar(a, b) => {
                format!("ConstrainVarSubVar({}, {})", a.repr(tcx), b.repr(tcx))
            }
            ConstrainRegSubVar(a, b) => {
                format!("ConstrainRegSubVar({}, {})", a.repr(tcx), b.repr(tcx))
            }
            ConstrainVarSubReg(a, b) => {
                format!("ConstrainVarSubReg({}, {})", a.repr(tcx), b.repr(tcx))
            }
        }
    }
}

impl<'tcx> Repr<'tcx> for Verify<'tcx> {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        match *self {
            VerifyRegSubReg(_, ref a, ref b) => {
                format!("VerifyRegSubReg({}, {})", a.repr(tcx), b.repr(tcx))
            }
            VerifyGenericBound(_, ref p, ref a, ref bs) => {
                format!("VerifyGenericBound({}, {}, {})",
                        p.repr(tcx), a.repr(tcx), bs.repr(tcx))
            }
        }
    }
}

fn normalize(values: &Vec<VarValue>, r: ty::Region) -> ty::Region {
    match r {
        ty::ReInfer(ReVar(rid)) => lookup(values, rid),
        _ => r
    }
}

fn lookup(values: &Vec<VarValue>, rid: ty::RegionVid) -> ty::Region {
    match values[rid.index as uint] {
        Value(r) => r,
        NoValue => ReEmpty, // No constraints, return ty::ReEmpty
        ErrorValue => ReStatic, // Previously reported error.
    }
}

impl<'tcx> Repr<'tcx> for VarValue {
    fn repr(&self, tcx: &ty::ctxt) -> String {
        match *self {
            NoValue => format!("NoValue"),
            Value(r) => format!("Value({})", r.repr(tcx)),
            ErrorValue => format!("ErrorValue"),
        }
    }
}

impl<'tcx> Repr<'tcx> for RegionAndOrigin<'tcx> {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        format!("RegionAndOrigin({},{})",
                self.region.repr(tcx),
                self.origin.repr(tcx))
    }
}

impl<'tcx> Repr<'tcx> for GenericKind<'tcx> {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        match *self {
            GenericKind::Param(ref p) => p.repr(tcx),
            GenericKind::Projection(ref p) => p.repr(tcx),
        }
    }
}

impl<'tcx> UserString<'tcx> for GenericKind<'tcx> {
    fn user_string(&self, tcx: &ty::ctxt<'tcx>) -> String {
        match *self {
            GenericKind::Param(ref p) => p.user_string(tcx),
            GenericKind::Projection(ref p) => p.user_string(tcx),
        }
    }
}

impl<'tcx> GenericKind<'tcx> {
    pub fn to_ty(&self, tcx: &ty::ctxt<'tcx>) -> Ty<'tcx> {
        match *self {
            GenericKind::Param(ref p) =>
                p.to_ty(tcx),
            GenericKind::Projection(ref p) =>
                ty::mk_projection(tcx, p.trait_ref.clone(), p.item_name),
        }
    }
}
