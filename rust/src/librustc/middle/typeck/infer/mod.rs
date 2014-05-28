// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*! See doc.rs for documentation */

#![allow(non_camel_case_types)]

pub use middle::ty::IntVarValue;
pub use middle::typeck::infer::resolve::resolve_and_force_all_but_regions;
pub use middle::typeck::infer::resolve::{force_all, not_regions};
pub use middle::typeck::infer::resolve::{force_ivar};
pub use middle::typeck::infer::resolve::{force_tvar, force_rvar};
pub use middle::typeck::infer::resolve::{resolve_ivar, resolve_all};
pub use middle::typeck::infer::resolve::{resolve_nested_tvar};
pub use middle::typeck::infer::resolve::{resolve_rvar};

use collections::HashMap;
use collections::SmallIntMap;
use middle::ty::{TyVid, IntVid, FloatVid, RegionVid, Vid};
use middle::ty;
use middle::ty_fold;
use middle::ty_fold::TypeFolder;
use middle::typeck::check::regionmanip::replace_late_bound_regions_in_fn_sig;
use middle::typeck::infer::coercion::Coerce;
use middle::typeck::infer::combine::{Combine, CombineFields, eq_tys};
use middle::typeck::infer::region_inference::{RegionVarBindings};
use middle::typeck::infer::resolve::{resolver};
use middle::typeck::infer::sub::Sub;
use middle::typeck::infer::lub::Lub;
use middle::typeck::infer::to_str::InferStr;
use middle::typeck::infer::unify::{ValsAndBindings, Root};
use middle::typeck::infer::error_reporting::ErrorReporting;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use syntax::ast;
use syntax::codemap;
use syntax::codemap::Span;
use syntax::owned_slice::OwnedSlice;
use util::common::indent;
use util::ppaux::{bound_region_to_str, ty_to_str, trait_ref_to_str, Repr};

pub mod doc;
pub mod macros;
pub mod combine;
pub mod glb;
pub mod lattice;
pub mod lub;
pub mod region_inference;
pub mod resolve;
pub mod sub;
pub mod to_str;
pub mod unify;
pub mod coercion;
pub mod error_reporting;

pub type Bound<T> = Option<T>;

#[deriving(Clone)]
pub struct Bounds<T> {
    lb: Bound<T>,
    ub: Bound<T>
}

pub type cres<T> = Result<T,ty::type_err>; // "combine result"
pub type ures = cres<()>; // "unify result"
pub type fres<T> = Result<T, fixup_err>; // "fixup result"
pub type CoerceResult = cres<Option<ty::AutoAdjustment>>;

pub struct InferCtxt<'a> {
    pub tcx: &'a ty::ctxt,

    // We instantiate ValsAndBindings with bounds<ty::t> because the
    // types that might instantiate a general type variable have an
    // order, represented by its upper and lower bounds.
    pub ty_var_bindings: RefCell<ValsAndBindings<ty::TyVid, Bounds<ty::t>>>,
    pub ty_var_counter: Cell<uint>,

    // Map from integral variable to the kind of integer it represents
    pub int_var_bindings: RefCell<ValsAndBindings<ty::IntVid,
                                              Option<IntVarValue>>>,
    pub int_var_counter: Cell<uint>,

    // Map from floating variable to the kind of float it represents
    pub float_var_bindings: RefCell<ValsAndBindings<ty::FloatVid,
                                                Option<ast::FloatTy>>>,
    pub float_var_counter: Cell<uint>,

    // For region variables.
    pub region_vars: RegionVarBindings<'a>,
}

/// Why did we require that the two types be related?
///
/// See `error_reporting.rs` for more details
#[deriving(Clone)]
pub enum TypeOrigin {
    // Not yet categorized in a better way
    Misc(Span),

    // Checking that method of impl is compatible with trait
    MethodCompatCheck(Span),

    // Checking that this expression can be assigned where it needs to be
    // FIXME(eddyb) #11161 is the original Expr required?
    ExprAssignable(Span),

    // Relating trait refs when resolving vtables
    RelateTraitRefs(Span),

    // Relating trait refs when resolving vtables
    RelateSelfType(Span),

    // Computing common supertype in a match expression
    MatchExpression(Span),

    // Computing common supertype in an if expression
    IfExpression(Span),
}

/// See `error_reporting.rs` for more details
#[deriving(Clone)]
pub enum ValuePairs {
    Types(ty::expected_found<ty::t>),
    TraitRefs(ty::expected_found<Rc<ty::TraitRef>>),
}

/// The trace designates the path through inference that we took to
/// encounter an error or subtyping constraint.
///
/// See `error_reporting.rs` for more details.
#[deriving(Clone)]
pub struct TypeTrace {
    origin: TypeOrigin,
    values: ValuePairs,
}

/// The origin of a `r1 <= r2` constraint.
///
/// See `error_reporting.rs` for more details
#[deriving(Clone)]
pub enum SubregionOrigin {
    // Arose from a subtyping relation
    Subtype(TypeTrace),

    // Stack-allocated closures cannot outlive innermost loop
    // or function so as to ensure we only require finite stack
    InfStackClosure(Span),

    // Invocation of closure must be within its lifetime
    InvokeClosure(Span),

    // Dereference of reference must be within its lifetime
    DerefPointer(Span),

    // Closure bound must not outlive captured free variables
    FreeVariable(Span, ast::NodeId),

    // Index into slice must be within its lifetime
    IndexSlice(Span),

    // When casting `&'a T` to an `&'b Trait` object,
    // relating `'a` to `'b`
    RelateObjectBound(Span),

    // Creating a pointer `b` to contents of another reference
    Reborrow(Span),

    // Creating a pointer `b` to contents of an upvar
    ReborrowUpvar(Span, ty::UpvarId),

    // (&'a &'b T) where a >= b
    ReferenceOutlivesReferent(ty::t, Span),

    // A `ref b` whose region does not enclose the decl site
    BindingTypeIsNotValidAtDecl(Span),

    // Regions appearing in a method receiver must outlive method call
    CallRcvr(Span),

    // Regions appearing in a function argument must outlive func call
    CallArg(Span),

    // Region in return type of invoked fn must enclose call
    CallReturn(Span),

    // Region resulting from a `&` expr must enclose the `&` expr
    AddrOf(Span),

    // An auto-borrow that does not enclose the expr where it occurs
    AutoBorrow(Span),
}

/// Reasons to create a region inference variable
///
/// See `error_reporting.rs` for more details
#[deriving(Clone)]
pub enum RegionVariableOrigin {
    // Region variables created for ill-categorized reasons,
    // mostly indicates places in need of refactoring
    MiscVariable(Span),

    // Regions created by a `&P` or `[...]` pattern
    PatternRegion(Span),

    // Regions created by `&` operator
    AddrOfRegion(Span),

    // Regions created by `&[...]` literal
    AddrOfSlice(Span),

    // Regions created as part of an autoref of a method receiver
    Autoref(Span),

    // Regions created as part of an automatic coercion
    Coercion(TypeTrace),

    // Region variables created as the values for early-bound regions
    EarlyBoundRegion(Span, ast::Name),

    // Region variables created for bound regions
    // in a function or method that is called
    LateBoundRegion(Span, ty::BoundRegion),

    // Region variables created for bound regions
    // when doing subtyping/lub/glb computations
    BoundRegionInFnType(Span, ty::BoundRegion),

    UpvarRegion(ty::UpvarId, Span),

    BoundRegionInCoherence(ast::Name),
}

pub enum fixup_err {
    unresolved_int_ty(IntVid),
    unresolved_ty(TyVid),
    cyclic_ty(TyVid),
    unresolved_region(RegionVid),
    region_var_bound_by_region_var(RegionVid, RegionVid)
}

pub fn fixup_err_to_str(f: fixup_err) -> String {
    match f {
      unresolved_int_ty(_) => "unconstrained integral type".to_string(),
      unresolved_ty(_) => "unconstrained type".to_string(),
      cyclic_ty(_) => "cyclic type of infinite size".to_string(),
      unresolved_region(_) => "unconstrained region".to_string(),
      region_var_bound_by_region_var(r1, r2) => {
        format_strbuf!("region var {:?} bound by another region var {:?}; \
                        this is a bug in rustc",
                       r1,
                       r2)
      }
    }
}

fn new_ValsAndBindings<V:Clone,T:Clone>() -> ValsAndBindings<V, T> {
    ValsAndBindings {
        vals: SmallIntMap::new(),
        bindings: Vec::new()
    }
}

pub fn new_infer_ctxt<'a>(tcx: &'a ty::ctxt) -> InferCtxt<'a> {
    InferCtxt {
        tcx: tcx,

        ty_var_bindings: RefCell::new(new_ValsAndBindings()),
        ty_var_counter: Cell::new(0),

        int_var_bindings: RefCell::new(new_ValsAndBindings()),
        int_var_counter: Cell::new(0),

        float_var_bindings: RefCell::new(new_ValsAndBindings()),
        float_var_counter: Cell::new(0),

        region_vars: RegionVarBindings(tcx),
    }
}

pub fn common_supertype(cx: &InferCtxt,
                        origin: TypeOrigin,
                        a_is_expected: bool,
                        a: ty::t,
                        b: ty::t)
                        -> ty::t {
    /*!
     * Computes the least upper-bound of `a` and `b`. If this is
     * not possible, reports an error and returns ty::err.
     */

    debug!("common_supertype({}, {})", a.inf_str(cx), b.inf_str(cx));

    let trace = TypeTrace {
        origin: origin,
        values: Types(expected_found(a_is_expected, a, b))
    };

    let result = cx.commit(|| cx.lub(a_is_expected, trace.clone()).tys(a, b));
    match result {
        Ok(t) => t,
        Err(ref err) => {
            cx.report_and_explain_type_error(trace, err);
            ty::mk_err()
        }
    }
}

pub fn mk_subty(cx: &InferCtxt,
                a_is_expected: bool,
                origin: TypeOrigin,
                a: ty::t,
                b: ty::t)
             -> ures {
    debug!("mk_subty({} <: {})", a.inf_str(cx), b.inf_str(cx));
    indent(|| {
        cx.commit(|| {
            let trace = TypeTrace {
                origin: origin,
                values: Types(expected_found(a_is_expected, a, b))
            };
            cx.sub(a_is_expected, trace).tys(a, b)
        })
    }).to_ures()
}

pub fn can_mk_subty(cx: &InferCtxt, a: ty::t, b: ty::t) -> ures {
    debug!("can_mk_subty({} <: {})", a.inf_str(cx), b.inf_str(cx));
    indent(|| {
        cx.probe(|| {
            let trace = TypeTrace {
                origin: Misc(codemap::DUMMY_SP),
                values: Types(expected_found(true, a, b))
            };
            cx.sub(true, trace).tys(a, b)
        })
    }).to_ures()
}

pub fn mk_subr(cx: &InferCtxt,
               _a_is_expected: bool,
               origin: SubregionOrigin,
               a: ty::Region,
               b: ty::Region) {
    debug!("mk_subr({} <: {})", a.inf_str(cx), b.inf_str(cx));
    cx.region_vars.start_snapshot();
    cx.region_vars.make_subregion(origin, a, b);
    cx.region_vars.commit();
}

pub fn mk_eqty(cx: &InferCtxt,
               a_is_expected: bool,
               origin: TypeOrigin,
               a: ty::t,
               b: ty::t)
            -> ures {
    debug!("mk_eqty({} <: {})", a.inf_str(cx), b.inf_str(cx));
    indent(|| {
        cx.commit(|| {
            let trace = TypeTrace {
                origin: origin,
                values: Types(expected_found(a_is_expected, a, b))
            };
            let suber = cx.sub(a_is_expected, trace);
            eq_tys(&suber, a, b)
        })
    }).to_ures()
}

pub fn mk_sub_trait_refs(cx: &InferCtxt,
                         a_is_expected: bool,
                         origin: TypeOrigin,
                         a: Rc<ty::TraitRef>,
                         b: Rc<ty::TraitRef>)
    -> ures
{
    debug!("mk_sub_trait_refs({} <: {})",
           a.inf_str(cx), b.inf_str(cx));
    indent(|| {
        cx.commit(|| {
            let trace = TypeTrace {
                origin: origin,
                values: TraitRefs(expected_found(a_is_expected, a.clone(), b.clone()))
            };
            let suber = cx.sub(a_is_expected, trace);
            suber.trait_refs(&*a, &*b)
        })
    }).to_ures()
}

fn expected_found<T>(a_is_expected: bool,
                     a: T,
                     b: T) -> ty::expected_found<T> {
    if a_is_expected {
        ty::expected_found {expected: a, found: b}
    } else {
        ty::expected_found {expected: b, found: a}
    }
}

pub fn mk_coercety(cx: &InferCtxt,
                   a_is_expected: bool,
                   origin: TypeOrigin,
                   a: ty::t,
                   b: ty::t)
                -> CoerceResult {
    debug!("mk_coercety({} -> {})", a.inf_str(cx), b.inf_str(cx));
    indent(|| {
        cx.commit(|| {
            let trace = TypeTrace {
                origin: origin,
                values: Types(expected_found(a_is_expected, a, b))
            };
            Coerce(cx.combine_fields(a_is_expected, trace)).tys(a, b)
        })
    })
}

// See comment on the type `resolve_state` below
pub fn resolve_type(cx: &InferCtxt,
                    a: ty::t,
                    modes: uint)
                 -> fres<ty::t> {
    let mut resolver = resolver(cx, modes);
    resolver.resolve_type_chk(a)
}

pub fn resolve_region(cx: &InferCtxt, r: ty::Region, modes: uint)
                   -> fres<ty::Region> {
    let mut resolver = resolver(cx, modes);
    resolver.resolve_region_chk(r)
}

trait then {
    fn then<T:Clone>(&self, f: || -> Result<T,ty::type_err>)
        -> Result<T,ty::type_err>;
}

impl then for ures {
    fn then<T:Clone>(&self, f: || -> Result<T,ty::type_err>)
        -> Result<T,ty::type_err> {
        self.and_then(|_i| f())
    }
}

trait ToUres {
    fn to_ures(&self) -> ures;
}

impl<T> ToUres for cres<T> {
    fn to_ures(&self) -> ures {
        match *self {
          Ok(ref _v) => Ok(()),
          Err(ref e) => Err((*e))
        }
    }
}

trait CresCompare<T> {
    fn compare(&self, t: T, f: || -> ty::type_err) -> cres<T>;
}

impl<T:Clone + Eq> CresCompare<T> for cres<T> {
    fn compare(&self, t: T, f: || -> ty::type_err) -> cres<T> {
        (*self).clone().and_then(|s| {
            if s == t {
                (*self).clone()
            } else {
                Err(f())
            }
        })
    }
}

pub fn uok() -> ures {
    Ok(())
}

fn rollback_to<V:Clone + Vid,T:Clone>(vb: &mut ValsAndBindings<V, T>,
                                      len: uint) {
    while vb.bindings.len() != len {
        let (vid, old_v) = vb.bindings.pop().unwrap();
        vb.vals.insert(vid.to_uint(), old_v);
    }
}

pub struct Snapshot {
    ty_var_bindings_len: uint,
    int_var_bindings_len: uint,
    float_var_bindings_len: uint,
    region_vars_snapshot: uint,
}

impl<'a> InferCtxt<'a> {
    pub fn combine_fields<'a>(&'a self, a_is_expected: bool, trace: TypeTrace)
                              -> CombineFields<'a> {
        CombineFields {infcx: self,
                       a_is_expected: a_is_expected,
                       trace: trace}
    }

    pub fn sub<'a>(&'a self, a_is_expected: bool, trace: TypeTrace) -> Sub<'a> {
        Sub(self.combine_fields(a_is_expected, trace))
    }

    pub fn lub<'a>(&'a self, a_is_expected: bool, trace: TypeTrace) -> Lub<'a> {
        Lub(self.combine_fields(a_is_expected, trace))
    }

    pub fn in_snapshot(&self) -> bool {
        self.region_vars.in_snapshot()
    }

    pub fn start_snapshot(&self) -> Snapshot {
        Snapshot {
            ty_var_bindings_len: self.ty_var_bindings.borrow().bindings.len(),
            int_var_bindings_len: self.int_var_bindings.borrow().bindings.len(),
            float_var_bindings_len: self.float_var_bindings.borrow().bindings.len(),
            region_vars_snapshot: self.region_vars.start_snapshot(),
        }
    }

    pub fn rollback_to(&self, snapshot: &Snapshot) {
        debug!("rollback!");
        rollback_to(&mut *self.ty_var_bindings.borrow_mut(),
                    snapshot.ty_var_bindings_len);
        rollback_to(&mut *self.int_var_bindings.borrow_mut(),
                    snapshot.int_var_bindings_len);
        rollback_to(&mut *self.float_var_bindings.borrow_mut(),
                    snapshot.float_var_bindings_len);

        self.region_vars.rollback_to(snapshot.region_vars_snapshot);
    }

    /// Execute `f` and commit the bindings if successful
    pub fn commit<T,E>(&self, f: || -> Result<T,E>) -> Result<T,E> {
        assert!(!self.in_snapshot());

        debug!("commit()");
        indent(|| {
            let r = self.try(|| f());

            self.ty_var_bindings.borrow_mut().bindings.truncate(0);
            self.int_var_bindings.borrow_mut().bindings.truncate(0);
            self.region_vars.commit();
            r
        })
    }

    /// Execute `f`, unroll bindings on failure
    pub fn try<T,E>(&self, f: || -> Result<T,E>) -> Result<T,E> {
        debug!("try()");
        let snapshot = self.start_snapshot();
        let r = f();
        match r {
            Ok(_) => { debug!("success"); }
            Err(ref e) => {
                debug!("error: {:?}", *e);
                self.rollback_to(&snapshot)
            }
        }
        r
    }

    /// Execute `f` then unroll any bindings it creates
    pub fn probe<T,E>(&self, f: || -> Result<T,E>) -> Result<T,E> {
        debug!("probe()");
        indent(|| {
            let snapshot = self.start_snapshot();
            let r = f();
            self.rollback_to(&snapshot);
            r
        })
    }
}

fn next_simple_var<V:Clone,T:Clone>(counter: &mut uint,
                                    bindings: &mut ValsAndBindings<V,
                                                                   Option<T>>)
                                    -> uint {
    let id = *counter;
    *counter += 1;
    bindings.vals.insert(id, Root(None, 0));
    return id;
}

impl<'a> InferCtxt<'a> {
    pub fn next_ty_var_id(&self) -> TyVid {
        let id = self.ty_var_counter.get();
        self.ty_var_counter.set(id + 1);
        {
            let mut ty_var_bindings = self.ty_var_bindings.borrow_mut();
            let vals = &mut ty_var_bindings.vals;
            vals.insert(id, Root(Bounds { lb: None, ub: None }, 0u));
        }
        return TyVid(id);
    }

    pub fn next_ty_var(&self) -> ty::t {
        ty::mk_var(self.tcx, self.next_ty_var_id())
    }

    pub fn next_ty_vars(&self, n: uint) -> Vec<ty::t> {
        Vec::from_fn(n, |_i| self.next_ty_var())
    }

    pub fn next_int_var_id(&self) -> IntVid {
        let mut int_var_counter = self.int_var_counter.get();
        let mut int_var_bindings = self.int_var_bindings.borrow_mut();
        let result = IntVid(next_simple_var(&mut int_var_counter,
                                            &mut *int_var_bindings));
        self.int_var_counter.set(int_var_counter);
        result
    }

    pub fn next_float_var_id(&self) -> FloatVid {
        let mut float_var_counter = self.float_var_counter.get();
        let mut float_var_bindings = self.float_var_bindings.borrow_mut();
        let result = FloatVid(next_simple_var(&mut float_var_counter,
                                              &mut *float_var_bindings));
        self.float_var_counter.set(float_var_counter);
        result
    }

    pub fn next_region_var(&self, origin: RegionVariableOrigin) -> ty::Region {
        ty::ReInfer(ty::ReVar(self.region_vars.new_region_var(origin)))
    }

    pub fn region_vars_for_defs(&self,
                                span: Span,
                                defs: &[ty::RegionParameterDef])
                                -> OwnedSlice<ty::Region> {
        defs.iter()
            .map(|d| self.next_region_var(EarlyBoundRegion(span, d.name)))
            .collect()
    }

    pub fn fresh_bound_region(&self, binder_id: ast::NodeId) -> ty::Region {
        self.region_vars.new_bound(binder_id)
    }

    pub fn resolve_regions_and_report_errors(&self) {
        let errors = self.region_vars.resolve_regions();
        self.report_region_errors(&errors); // see error_reporting.rs
    }

    pub fn ty_to_str(&self, t: ty::t) -> String {
        ty_to_str(self.tcx,
                  self.resolve_type_vars_if_possible(t))
    }

    pub fn tys_to_str(&self, ts: &[ty::t]) -> String {
        let tstrs: Vec<String> = ts.iter().map(|t| self.ty_to_str(*t)).collect();
        format_strbuf!("({})", tstrs.connect(", "))
    }

    pub fn trait_ref_to_str(&self, t: &ty::TraitRef) -> String {
        let t = self.resolve_type_vars_in_trait_ref_if_possible(t);
        trait_ref_to_str(self.tcx, &t)
    }

    pub fn resolve_type_vars_if_possible(&self, typ: ty::t) -> ty::t {
        match resolve_type(self, typ, resolve_nested_tvar | resolve_ivar) {
            Ok(new_type) => new_type,
            Err(_) => typ
        }
    }

    pub fn resolve_type_vars_in_trait_ref_if_possible(&self,
                                                      trait_ref:
                                                      &ty::TraitRef)
                                                      -> ty::TraitRef {
        // make up a dummy type just to reuse/abuse the resolve machinery
        let dummy0 = ty::mk_trait(self.tcx,
                                  trait_ref.def_id,
                                  trait_ref.substs.clone(),
                                  ty::UniqTraitStore,
                                  ty::EmptyBuiltinBounds());
        let dummy1 = self.resolve_type_vars_if_possible(dummy0);
        match ty::get(dummy1).sty {
            ty::ty_trait(box ty::TyTrait { ref def_id, ref substs, .. }) => {
                ty::TraitRef {
                    def_id: *def_id,
                    substs: (*substs).clone(),
                }
            }
            _ => {
                self.tcx.sess.bug(
                    format!("resolve_type_vars_if_possible() yielded {} \
                             when supplied with {}",
                            self.ty_to_str(dummy0),
                            self.ty_to_str(dummy1)).as_slice());
            }
        }
    }

    // [Note-Type-error-reporting]
    // An invariant is that anytime the expected or actual type is ty_err (the special
    // error type, meaning that an error occurred when typechecking this expression),
    // this is a derived error. The error cascaded from another error (that was already
    // reported), so it's not useful to display it to the user.
    // The following four methods -- type_error_message_str, type_error_message_str_with_expected,
    // type_error_message, and report_mismatched_types -- implement this logic.
    // They check if either the actual or expected type is ty_err, and don't print the error
    // in this case. The typechecker should only ever report type errors involving mismatched
    // types using one of these four methods, and should not call span_err directly for such
    // errors.
    pub fn type_error_message_str(&self,
                                  sp: Span,
                                  mk_msg: |Option<String>, String| -> String,
                                  actual_ty: String,
                                  err: Option<&ty::type_err>) {
        self.type_error_message_str_with_expected(sp, mk_msg, None, actual_ty, err)
    }

    pub fn type_error_message_str_with_expected(&self,
                                                sp: Span,
                                                mk_msg: |Option<String>,
                                                         String|
                                                         -> String,
                                                expected_ty: Option<ty::t>,
                                                actual_ty: String,
                                                err: Option<&ty::type_err>) {
        debug!("hi! expected_ty = {:?}, actual_ty = {}", expected_ty, actual_ty);

        let error_str = err.map_or("".to_string(), |t_err| {
            format!(" ({})", ty::type_err_to_str(self.tcx, t_err))
        });
        let resolved_expected = expected_ty.map(|e_ty| {
            self.resolve_type_vars_if_possible(e_ty)
        });
        if !resolved_expected.map_or(false, |e| { ty::type_is_error(e) }) {
            match resolved_expected {
                None => {
                    self.tcx
                        .sess
                        .span_err(sp,
                                  format!("{}{}",
                                          mk_msg(None, actual_ty),
                                          error_str).as_slice())
                }
                Some(e) => {
                    self.tcx.sess.span_err(sp,
                        format!("{}{}",
                                mk_msg(Some(self.ty_to_str(e)), actual_ty),
                                error_str).as_slice());
                }
            }
            for err in err.iter() {
                ty::note_and_explain_type_err(self.tcx, *err)
            }
        }
    }

    pub fn type_error_message(&self,
                              sp: Span,
                              mk_msg: |String| -> String,
                              actual_ty: ty::t,
                              err: Option<&ty::type_err>) {
        let actual_ty = self.resolve_type_vars_if_possible(actual_ty);

        // Don't report an error if actual type is ty_err.
        if ty::type_is_error(actual_ty) {
            return;
        }

        self.type_error_message_str(sp, |_e, a| { mk_msg(a) }, self.ty_to_str(actual_ty), err);
    }

    pub fn report_mismatched_types(&self,
                                   sp: Span,
                                   e: ty::t,
                                   a: ty::t,
                                   err: &ty::type_err) {
        let resolved_expected =
            self.resolve_type_vars_if_possible(e);
        let mk_msg = match ty::get(resolved_expected).sty {
            // Don't report an error if expected is ty_err
            ty::ty_err => return,
            _ => {
                // if I leave out : String, it infers &str and complains
                |actual: String| {
                    format_strbuf!("mismatched types: expected `{}` but \
                                    found `{}`",
                                   self.ty_to_str(resolved_expected),
                                   actual)
                }
            }
        };
        self.type_error_message(sp, mk_msg, a, Some(err));
    }

    pub fn replace_late_bound_regions_with_fresh_regions(&self,
                                                         trace: TypeTrace,
                                                         fsig: &ty::FnSig)
                                                    -> (ty::FnSig,
                                                        HashMap<ty::BoundRegion,
                                                                ty::Region>) {
        let (map, fn_sig) =
            replace_late_bound_regions_in_fn_sig(self.tcx, fsig, |br| {
                let rvar = self.next_region_var(
                    BoundRegionInFnType(trace.origin.span(), br));
                debug!("Bound region {} maps to {:?}",
                       bound_region_to_str(self.tcx, "", false, br),
                       rvar);
                rvar
            });
        (fn_sig, map)
    }
}

pub fn fold_regions_in_sig(tcx: &ty::ctxt,
                           fn_sig: &ty::FnSig,
                           fldr: |r: ty::Region| -> ty::Region)
                           -> ty::FnSig {
    ty_fold::RegionFolder::regions(tcx, fldr).fold_sig(fn_sig)
}

impl TypeTrace {
    pub fn span(&self) -> Span {
        self.origin.span()
    }
}

impl Repr for TypeTrace {
    fn repr(&self, tcx: &ty::ctxt) -> String {
        format_strbuf!("TypeTrace({})", self.origin.repr(tcx))
    }
}

impl TypeOrigin {
    pub fn span(&self) -> Span {
        match *self {
            MethodCompatCheck(span) => span,
            ExprAssignable(span) => span,
            Misc(span) => span,
            RelateTraitRefs(span) => span,
            RelateSelfType(span) => span,
            MatchExpression(span) => span,
            IfExpression(span) => span,
        }
    }
}

impl Repr for TypeOrigin {
    fn repr(&self, tcx: &ty::ctxt) -> String {
        match *self {
            MethodCompatCheck(a) => {
                format_strbuf!("MethodCompatCheck({})", a.repr(tcx))
            }
            ExprAssignable(a) => {
                format_strbuf!("ExprAssignable({})", a.repr(tcx))
            }
            Misc(a) => format_strbuf!("Misc({})", a.repr(tcx)),
            RelateTraitRefs(a) => {
                format_strbuf!("RelateTraitRefs({})", a.repr(tcx))
            }
            RelateSelfType(a) => {
                format_strbuf!("RelateSelfType({})", a.repr(tcx))
            }
            MatchExpression(a) => {
                format_strbuf!("MatchExpression({})", a.repr(tcx))
            }
            IfExpression(a) => {
                format_strbuf!("IfExpression({})", a.repr(tcx))
            }
        }
    }
}

impl SubregionOrigin {
    pub fn span(&self) -> Span {
        match *self {
            Subtype(ref a) => a.span(),
            InfStackClosure(a) => a,
            InvokeClosure(a) => a,
            DerefPointer(a) => a,
            FreeVariable(a, _) => a,
            IndexSlice(a) => a,
            RelateObjectBound(a) => a,
            Reborrow(a) => a,
            ReborrowUpvar(a, _) => a,
            ReferenceOutlivesReferent(_, a) => a,
            BindingTypeIsNotValidAtDecl(a) => a,
            CallRcvr(a) => a,
            CallArg(a) => a,
            CallReturn(a) => a,
            AddrOf(a) => a,
            AutoBorrow(a) => a,
        }
    }
}

impl Repr for SubregionOrigin {
    fn repr(&self, tcx: &ty::ctxt) -> String {
        match *self {
            Subtype(ref a) => {
                format_strbuf!("Subtype({})", a.repr(tcx))
            }
            InfStackClosure(a) => {
                format_strbuf!("InfStackClosure({})", a.repr(tcx))
            }
            InvokeClosure(a) => {
                format_strbuf!("InvokeClosure({})", a.repr(tcx))
            }
            DerefPointer(a) => {
                format_strbuf!("DerefPointer({})", a.repr(tcx))
            }
            FreeVariable(a, b) => {
                format_strbuf!("FreeVariable({}, {})", a.repr(tcx), b)
            }
            IndexSlice(a) => {
                format_strbuf!("IndexSlice({})", a.repr(tcx))
            }
            RelateObjectBound(a) => {
                format_strbuf!("RelateObjectBound({})", a.repr(tcx))
            }
            Reborrow(a) => format_strbuf!("Reborrow({})", a.repr(tcx)),
            ReborrowUpvar(a, b) => {
                format_strbuf!("ReborrowUpvar({},{:?})", a.repr(tcx), b)
            }
            ReferenceOutlivesReferent(_, a) => {
                format_strbuf!("ReferenceOutlivesReferent({})", a.repr(tcx))
            }
            BindingTypeIsNotValidAtDecl(a) => {
                format_strbuf!("BindingTypeIsNotValidAtDecl({})", a.repr(tcx))
            }
            CallRcvr(a) => format_strbuf!("CallRcvr({})", a.repr(tcx)),
            CallArg(a) => format_strbuf!("CallArg({})", a.repr(tcx)),
            CallReturn(a) => format_strbuf!("CallReturn({})", a.repr(tcx)),
            AddrOf(a) => format_strbuf!("AddrOf({})", a.repr(tcx)),
            AutoBorrow(a) => format_strbuf!("AutoBorrow({})", a.repr(tcx)),
        }
    }
}

impl RegionVariableOrigin {
    pub fn span(&self) -> Span {
        match *self {
            MiscVariable(a) => a,
            PatternRegion(a) => a,
            AddrOfRegion(a) => a,
            AddrOfSlice(a) => a,
            Autoref(a) => a,
            Coercion(ref a) => a.span(),
            EarlyBoundRegion(a, _) => a,
            LateBoundRegion(a, _) => a,
            BoundRegionInFnType(a, _) => a,
            BoundRegionInCoherence(_) => codemap::DUMMY_SP,
            UpvarRegion(_, a) => a
        }
    }
}

impl Repr for RegionVariableOrigin {
    fn repr(&self, tcx: &ty::ctxt) -> String {
        match *self {
            MiscVariable(a) => {
                format_strbuf!("MiscVariable({})", a.repr(tcx))
            }
            PatternRegion(a) => {
                format_strbuf!("PatternRegion({})", a.repr(tcx))
            }
            AddrOfRegion(a) => {
                format_strbuf!("AddrOfRegion({})", a.repr(tcx))
            }
            AddrOfSlice(a) => format_strbuf!("AddrOfSlice({})", a.repr(tcx)),
            Autoref(a) => format_strbuf!("Autoref({})", a.repr(tcx)),
            Coercion(ref a) => format_strbuf!("Coercion({})", a.repr(tcx)),
            EarlyBoundRegion(a, b) => {
                format_strbuf!("EarlyBoundRegion({},{})",
                               a.repr(tcx),
                               b.repr(tcx))
            }
            LateBoundRegion(a, b) => {
                format_strbuf!("LateBoundRegion({},{})",
                               a.repr(tcx),
                               b.repr(tcx))
            }
            BoundRegionInFnType(a, b) => {
                format_strbuf!("bound_regionInFnType({},{})",
                               a.repr(tcx),
                               b.repr(tcx))
            }
            BoundRegionInCoherence(a) => {
                format_strbuf!("bound_regionInCoherence({})", a.repr(tcx))
            }
            UpvarRegion(a, b) => {
                format_strbuf!("UpvarRegion({}, {})",
                               a.repr(tcx),
                               b.repr(tcx))
            }
        }
    }
}
