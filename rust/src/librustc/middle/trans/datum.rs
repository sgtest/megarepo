// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
 * See the section on datums in `doc.rs` for an overview of what
 * Datums are and how they are intended to be used.
 */

use llvm::ValueRef;
use middle::trans::base::*;
use middle::trans::build::Load;
use middle::trans::common::*;
use middle::trans::cleanup;
use middle::trans::cleanup::CleanupMethods;
use middle::trans::expr;
use middle::trans::glue;
use middle::trans::tvec;
use middle::trans::type_of;
use middle::ty;
use util::ppaux::{ty_to_string};

use syntax::ast;

/**
 * A `Datum` encapsulates the result of evaluating an expression.  It
 * describes where the value is stored, what Rust type the value has,
 * whether it is addressed by reference, and so forth. Please refer
 * the section on datums in `doc.rs` for more details.
 */
#[deriving(Clone)]
pub struct Datum<K> {
    /// The llvm value.  This is either a pointer to the Rust value or
    /// the value itself, depending on `kind` below.
    pub val: ValueRef,

    /// The rust type of the value.
    pub ty: ty::t,

    /// Indicates whether this is by-ref or by-value.
    pub kind: K,
}

pub struct DatumBlock<'a, K> {
    pub bcx: &'a Block<'a>,
    pub datum: Datum<K>,
}

pub enum Expr {
    /// a fresh value that was produced and which has no cleanup yet
    /// because it has not yet "landed" into its permanent home
    RvalueExpr(Rvalue),

    /// `val` is a pointer into memory for which a cleanup is scheduled
    /// (and thus has type *T). If you move out of an Lvalue, you must
    /// zero out the memory (FIXME #5016).
    LvalueExpr,
}

#[deriving(Clone)]
pub struct Lvalue;

pub struct Rvalue {
    pub mode: RvalueMode
}

impl Rvalue {
    pub fn new(m: RvalueMode) -> Rvalue {
        Rvalue { mode: m }
    }
}

// Make Datum linear for more type safety.
impl Drop for Rvalue {
    fn drop(&mut self) { }
}

#[deriving(PartialEq, Eq, Hash)]
pub enum RvalueMode {
    /// `val` is a pointer to the actual value (and thus has type *T)
    ByRef,

    /// `val` is the actual value (*only used for immediates* like ints, ptrs)
    ByValue,
}

pub fn immediate_rvalue(val: ValueRef, ty: ty::t) -> Datum<Rvalue> {
    return Datum::new(val, ty, Rvalue::new(ByValue));
}

pub fn immediate_rvalue_bcx<'a>(bcx: &'a Block<'a>,
                                val: ValueRef,
                                ty: ty::t)
                                -> DatumBlock<'a, Rvalue> {
    return DatumBlock::new(bcx, immediate_rvalue(val, ty))
}


pub fn lvalue_scratch_datum<'a, A>(bcx: &'a Block<'a>,
                                   ty: ty::t,
                                   name: &str,
                                   zero: bool,
                                   scope: cleanup::ScopeId,
                                   arg: A,
                                   populate: |A, &'a Block<'a>, ValueRef|
                                             -> &'a Block<'a>)
                                   -> DatumBlock<'a, Lvalue> {
    /*!
     * Allocates temporary space on the stack using alloca() and
     * returns a by-ref Datum pointing to it. The memory will be
     * dropped upon exit from `scope`. The callback `populate` should
     * initialize the memory. If `zero` is true, the space will be
     * zeroed when it is allocated; this is not necessary unless `bcx`
     * does not dominate the end of `scope`.
     */

    let scratch = if zero {
        alloca_zeroed(bcx, ty, name)
    } else {
        let llty = type_of::type_of(bcx.ccx(), ty);
        alloca(bcx, llty, name)
    };

    // Subtle. Populate the scratch memory *before* scheduling cleanup.
    let bcx = populate(arg, bcx, scratch);
    bcx.fcx.schedule_lifetime_end(scope, scratch);
    bcx.fcx.schedule_drop_mem(scope, scratch, ty);

    DatumBlock::new(bcx, Datum::new(scratch, ty, Lvalue))
}

pub fn rvalue_scratch_datum(bcx: &Block,
                            ty: ty::t,
                            name: &str)
                            -> Datum<Rvalue> {
    /*!
     * Allocates temporary space on the stack using alloca() and
     * returns a by-ref Datum pointing to it.  If `zero` is true, the
     * space will be zeroed when it is allocated; this is normally not
     * necessary, but in the case of automatic rooting in match
     * statements it is possible to have temporaries that may not get
     * initialized if a certain arm is not taken, so we must zero
     * them. You must arrange any cleanups etc yourself!
     */

    let llty = type_of::type_of(bcx.ccx(), ty);
    let scratch = alloca(bcx, llty, name);
    Datum::new(scratch, ty, Rvalue::new(ByRef))
}

pub fn appropriate_rvalue_mode(ccx: &CrateContext, ty: ty::t) -> RvalueMode {
    /*!
     * Indicates the "appropriate" mode for this value,
     * which is either by ref or by value, depending
     * on whether type is immediate or not.
     */

    if type_is_immediate(ccx, ty) {
        ByValue
    } else {
        ByRef
    }
}

fn add_rvalue_clean(mode: RvalueMode,
                    fcx: &FunctionContext,
                    scope: cleanup::ScopeId,
                    val: ValueRef,
                    ty: ty::t) {
    match mode {
        ByValue => { fcx.schedule_drop_immediate(scope, val, ty); }
        ByRef => {
            fcx.schedule_lifetime_end(scope, val);
            fcx.schedule_drop_mem(scope, val, ty);
        }
    }
}

pub trait KindOps {

    /**
     * Take appropriate action after the value in `datum` has been
     * stored to a new location.
     */
    fn post_store<'a>(&self,
                      bcx: &'a Block<'a>,
                      val: ValueRef,
                      ty: ty::t)
                      -> &'a Block<'a>;

    /**
     * True if this mode is a reference mode, meaning that the datum's
     * val field is a pointer to the actual value
     */
    fn is_by_ref(&self) -> bool;

    /**
     * Converts to an Expr kind
     */
    fn to_expr_kind(self) -> Expr;

}

impl KindOps for Rvalue {
    fn post_store<'a>(&self,
                      bcx: &'a Block<'a>,
                      _val: ValueRef,
                      _ty: ty::t)
                      -> &'a Block<'a> {
        // No cleanup is scheduled for an rvalue, so we don't have
        // to do anything after a move to cancel or duplicate it.
        bcx
    }

    fn is_by_ref(&self) -> bool {
        self.mode == ByRef
    }

    fn to_expr_kind(self) -> Expr {
        RvalueExpr(self)
    }
}

impl KindOps for Lvalue {
    fn post_store<'a>(&self,
                      bcx: &'a Block<'a>,
                      val: ValueRef,
                      ty: ty::t)
                      -> &'a Block<'a> {
        /*!
         * If an lvalue is moved, we must zero out the memory in which
         * it resides so as to cancel cleanup. If an @T lvalue is
         * copied, we must increment the reference count.
         */

        if ty::type_needs_drop(bcx.tcx(), ty) {
            if ty::type_moves_by_default(bcx.tcx(), ty) {
                // cancel cleanup of affine values by zeroing out
                let () = zero_mem(bcx, val, ty);
                bcx
            } else {
                // incr. refcount for @T or newtype'd @T
                glue::take_ty(bcx, val, ty)
            }
        } else {
            bcx
        }
    }

    fn is_by_ref(&self) -> bool {
        true
    }

    fn to_expr_kind(self) -> Expr {
        LvalueExpr
    }
}

impl KindOps for Expr {
    fn post_store<'a>(&self,
                      bcx: &'a Block<'a>,
                      val: ValueRef,
                      ty: ty::t)
                      -> &'a Block<'a> {
        match *self {
            LvalueExpr => Lvalue.post_store(bcx, val, ty),
            RvalueExpr(ref r) => r.post_store(bcx, val, ty),
        }
    }

    fn is_by_ref(&self) -> bool {
        match *self {
            LvalueExpr => Lvalue.is_by_ref(),
            RvalueExpr(ref r) => r.is_by_ref()
        }
    }

    fn to_expr_kind(self) -> Expr {
        self
    }
}

impl Datum<Rvalue> {
    pub fn add_clean(self,
                     fcx: &FunctionContext,
                     scope: cleanup::ScopeId)
                     -> ValueRef {
        /*!
         * Schedules a cleanup for this datum in the given scope.
         * That means that this datum is no longer an rvalue datum;
         * hence, this function consumes the datum and returns the
         * contained ValueRef.
         */

        add_rvalue_clean(self.kind.mode, fcx, scope, self.val, self.ty);
        self.val
    }

    pub fn to_lvalue_datum_in_scope<'a>(self,
                                        bcx: &'a Block<'a>,
                                        name: &str,
                                        scope: cleanup::ScopeId)
                                        -> DatumBlock<'a, Lvalue> {
        /*!
         * Returns an lvalue datum (that is, a by ref datum with
         * cleanup scheduled). If `self` is not already an lvalue,
         * cleanup will be scheduled in the temporary scope for `expr_id`.
         */
        let fcx = bcx.fcx;

        match self.kind.mode {
            ByRef => {
                add_rvalue_clean(ByRef, fcx, scope, self.val, self.ty);
                DatumBlock::new(bcx, Datum::new(self.val, self.ty, Lvalue))
            }

            ByValue => {
                lvalue_scratch_datum(
                    bcx, self.ty, name, false, scope, self,
                    |this, bcx, llval| this.store_to(bcx, llval))
            }
        }
    }

    pub fn to_ref_datum<'a>(self, bcx: &'a Block<'a>) -> DatumBlock<'a, Rvalue> {
        let mut bcx = bcx;
        match self.kind.mode {
            ByRef => DatumBlock::new(bcx, self),
            ByValue => {
                let scratch = rvalue_scratch_datum(bcx, self.ty, "to_ref");
                bcx = self.store_to(bcx, scratch.val);
                DatumBlock::new(bcx, scratch)
            }
        }
    }

    pub fn to_appropriate_datum<'a>(self,
                                    bcx: &'a Block<'a>)
                                    -> DatumBlock<'a, Rvalue> {
        match self.appropriate_rvalue_mode(bcx.ccx()) {
            ByRef => {
                self.to_ref_datum(bcx)
            }
            ByValue => {
                match self.kind.mode {
                    ByValue => DatumBlock::new(bcx, self),
                    ByRef => {
                        let llval = load_ty(bcx, self.val, self.ty);
                        DatumBlock::new(bcx, Datum::new(llval, self.ty, Rvalue::new(ByValue)))
                    }
                }
            }
        }
    }
}

/**
 * Methods suitable for "expr" datums that could be either lvalues or
 * rvalues. These include coercions into lvalues/rvalues but also a number
 * of more general operations. (Some of those operations could be moved to
 * the more general `impl<K> Datum<K>`, but it's convenient to have them
 * here since we can `match self.kind` rather than having to implement
 * generic methods in `KindOps`.)
 */
impl Datum<Expr> {
    fn match_kind<R>(self,
                     if_lvalue: |Datum<Lvalue>| -> R,
                     if_rvalue: |Datum<Rvalue>| -> R)
                     -> R {
        let Datum { val, ty, kind } = self;
        match kind {
            LvalueExpr => if_lvalue(Datum::new(val, ty, Lvalue)),
            RvalueExpr(r) => if_rvalue(Datum::new(val, ty, r)),
        }
    }

    #[allow(dead_code)] // potentially useful
    pub fn assert_lvalue(self, bcx: &Block) -> Datum<Lvalue> {
        /*!
         * Asserts that this datum *is* an lvalue and returns it.
         */

        self.match_kind(
            |d| d,
            |_| bcx.sess().bug("assert_lvalue given rvalue"))
    }

    pub fn assert_rvalue(self, bcx: &Block) -> Datum<Rvalue> {
        /*!
         * Asserts that this datum *is* an lvalue and returns it.
         */

        self.match_kind(
            |_| bcx.sess().bug("assert_rvalue given lvalue"),
            |r| r)
    }

    pub fn store_to_dest<'a>(self,
                             bcx: &'a Block<'a>,
                             dest: expr::Dest,
                             expr_id: ast::NodeId)
                             -> &'a Block<'a> {
        match dest {
            expr::Ignore => {
                self.add_clean_if_rvalue(bcx, expr_id);
                bcx
            }
            expr::SaveIn(addr) => {
                self.store_to(bcx, addr)
            }
        }
    }

    pub fn add_clean_if_rvalue<'a>(self,
                                   bcx: &'a Block<'a>,
                                   expr_id: ast::NodeId) {
        /*!
         * Arranges cleanup for `self` if it is an rvalue. Use when
         * you are done working with a value that may need drop.
         */

        self.match_kind(
            |_| { /* Nothing to do, cleanup already arranged */ },
            |r| {
                let scope = cleanup::temporary_scope(bcx.tcx(), expr_id);
                r.add_clean(bcx.fcx, scope);
            })
    }

    pub fn clean<'a>(self,
                     bcx: &'a Block<'a>,
                     name: &'static str,
                     expr_id: ast::NodeId)
                     -> &'a Block<'a> {
        /*!
         * Ensures that `self` will get cleaned up, if it is not an lvalue
         * already.
         */

        self.to_lvalue_datum(bcx, name, expr_id).bcx
    }

    pub fn to_lvalue_datum<'a>(self,
                               bcx: &'a Block<'a>,
                               name: &str,
                               expr_id: ast::NodeId)
                               -> DatumBlock<'a, Lvalue> {
        debug!("to_lvalue_datum self: {}", self.to_string(bcx.ccx()));

        assert!(ty::lltype_is_sized(bcx.tcx(), self.ty),
                "Trying to convert unsized value to lval");
        self.match_kind(
            |l| DatumBlock::new(bcx, l),
            |r| {
                let scope = cleanup::temporary_scope(bcx.tcx(), expr_id);
                r.to_lvalue_datum_in_scope(bcx, name, scope)
            })
    }

    pub fn to_rvalue_datum<'a>(self,
                               bcx: &'a Block<'a>,
                               name: &'static str)
                               -> DatumBlock<'a, Rvalue> {
        /*!
         * Ensures that we have an rvalue datum (that is, a datum with
         * no cleanup scheduled).
         */

        self.match_kind(
            |l| {
                let mut bcx = bcx;
                match l.appropriate_rvalue_mode(bcx.ccx()) {
                    ByRef => {
                        let scratch = rvalue_scratch_datum(bcx, l.ty, name);
                        bcx = l.store_to(bcx, scratch.val);
                        DatumBlock::new(bcx, scratch)
                    }
                    ByValue => {
                        let v = load_ty(bcx, l.val, l.ty);
                        bcx = l.kind.post_store(bcx, l.val, l.ty);
                        DatumBlock::new(bcx, Datum::new(v, l.ty, Rvalue::new(ByValue)))
                    }
                }
            },
            |r| DatumBlock::new(bcx, r))
    }

}

/**
 * Methods suitable only for lvalues. These include the various
 * operations to extract components out of compound data structures,
 * such as extracting the field from a struct or a particular element
 * from an array.
 */
impl Datum<Lvalue> {
    pub fn to_llref(self) -> ValueRef {
        /*!
         * Converts a datum into a by-ref value. The datum type must
         * be one which is always passed by reference.
         */

        self.val
    }

    // Extracts a component of a compound data structure (e.g., a field from a
    // struct). Note that if self is an opened, unsized type then the returned
    // datum may also be unsized _without the size information_. It is the
    // callers responsibility to package the result in some way to make a valid
    // datum in that case (e.g., by making a fat pointer or opened pair).
    pub fn get_element<'a>(&self,
                           bcx: &'a Block<'a>,
                           ty: ty::t,
                           gep: |ValueRef| -> ValueRef)
                           -> Datum<Lvalue> {
        let val = match ty::get(self.ty).sty {
            _ if ty::type_is_sized(bcx.tcx(), self.ty) => gep(self.val),
            ty::ty_open(_) => {
                let base = Load(bcx, expr::get_dataptr(bcx, self.val));
                gep(base)
            }
            _ => bcx.tcx().sess.bug(
                format!("Unexpected unsized type in get_element: {}",
                        bcx.ty_to_string(self.ty)).as_slice())
        };
        Datum {
            val: val,
            kind: Lvalue,
            ty: ty,
        }
    }

    pub fn get_vec_base_and_len<'a>(&self, bcx: &'a Block<'a>) -> (ValueRef, ValueRef) {
        //! Converts a vector into the slice pair.

        tvec::get_base_and_len(bcx, self.val, self.ty)
    }
}

/**
 * Generic methods applicable to any sort of datum.
 */
impl<K:KindOps> Datum<K> {
    pub fn new(val: ValueRef, ty: ty::t, kind: K) -> Datum<K> {
        Datum { val: val, ty: ty, kind: kind }
    }

    pub fn to_expr_datum(self) -> Datum<Expr> {
        let Datum { val, ty, kind } = self;
        Datum { val: val, ty: ty, kind: kind.to_expr_kind() }
    }

    pub fn store_to<'a>(self,
                        bcx: &'a Block<'a>,
                        dst: ValueRef)
                        -> &'a Block<'a> {
        /*!
         * Moves or copies this value into a new home, as appropriate
         * depending on the type of the datum. This method consumes
         * the datum, since it would be incorrect to go on using the
         * datum if the value represented is affine (and hence the value
         * is moved).
         */

        self.shallow_copy(bcx, dst);

        self.kind.post_store(bcx, self.val, self.ty)
    }

    fn shallow_copy<'a>(&self,
                        bcx: &'a Block<'a>,
                        dst: ValueRef)
                        -> &'a Block<'a> {
        /*!
         * Helper function that performs a shallow copy of this value
         * into `dst`, which should be a pointer to a memory location
         * suitable for `self.ty`. `dst` should contain uninitialized
         * memory (either newly allocated, zeroed, or dropped).
         *
         * This function is private to datums because it leaves memory
         * in an unstable state, where the source value has been
         * copied but not zeroed. Public methods are `store_to` (if
         * you no longer need the source value) or
         * `shallow_copy_and_take` (if you wish the source value to
         * remain valid).
         */

        let _icx = push_ctxt("copy_to_no_check");

        if type_is_zero_size(bcx.ccx(), self.ty) {
            return bcx;
        }

        if self.kind.is_by_ref() {
            memcpy_ty(bcx, dst, self.val, self.ty);
        } else {
            store_ty(bcx, self.val, dst, self.ty);
        }

        return bcx;
    }

    pub fn shallow_copy_and_take<'a>(&self,
                                     bcx: &'a Block<'a>,
                                     dst: ValueRef)
                                     -> &'a Block<'a> {
        /*!
         * Copies the value into a new location and runs any necessary
         * take glue on the new location. This function always
         * preserves the existing datum as a valid value. Therefore,
         * it does not consume `self` and, also, cannot be applied to
         * affine values (since they must never be duplicated).
         */

        assert!(!ty::type_moves_by_default(bcx.tcx(), self.ty));
        let mut bcx = bcx;
        bcx = self.shallow_copy(bcx, dst);
        glue::take_ty(bcx, dst, self.ty)
    }

    #[allow(dead_code)] // useful for debugging
    pub fn to_string(&self, ccx: &CrateContext) -> String {
        format!("Datum({}, {}, {:?})",
                ccx.tn.val_to_string(self.val),
                ty_to_string(ccx.tcx(), self.ty),
                self.kind)
    }

    pub fn appropriate_rvalue_mode(&self, ccx: &CrateContext) -> RvalueMode {
        /*! See the `appropriate_rvalue_mode()` function */

        appropriate_rvalue_mode(ccx, self.ty)
    }

    pub fn to_llscalarish<'a>(self, bcx: &'a Block<'a>) -> ValueRef {
        /*!
         * Converts `self` into a by-value `ValueRef`. Consumes this
         * datum (i.e., absolves you of responsibility to cleanup the
         * value). For this to work, the value must be something
         * scalar-ish (like an int or a pointer) which (1) does not
         * require drop glue and (2) is naturally passed around by
         * value, and not by reference.
         */

        assert!(!ty::type_needs_drop(bcx.tcx(), self.ty));
        assert!(self.appropriate_rvalue_mode(bcx.ccx()) == ByValue);
        if self.kind.is_by_ref() {
            load_ty(bcx, self.val, self.ty)
        } else {
            self.val
        }
    }

    pub fn to_llbool<'a>(self, bcx: &'a Block<'a>) -> ValueRef {
        assert!(ty::type_is_bool(self.ty) || ty::type_is_bot(self.ty))
        self.to_llscalarish(bcx)
    }
}

impl <'a, K> DatumBlock<'a, K> {
    pub fn new(bcx: &'a Block<'a>, datum: Datum<K>) -> DatumBlock<'a, K> {
        DatumBlock { bcx: bcx, datum: datum }
    }
}

impl<'a, K:KindOps> DatumBlock<'a, K> {
    pub fn to_expr_datumblock(self) -> DatumBlock<'a, Expr> {
        DatumBlock::new(self.bcx, self.datum.to_expr_datum())
    }
}

impl<'a> DatumBlock<'a, Expr> {
    pub fn store_to_dest(self,
                         dest: expr::Dest,
                         expr_id: ast::NodeId) -> &'a Block<'a> {
        let DatumBlock { bcx, datum } = self;
        datum.store_to_dest(bcx, dest, expr_id)
    }

    pub fn to_llbool(self) -> Result<'a> {
        let DatumBlock { datum, bcx } = self;
        Result::new(bcx, datum.to_llbool(bcx))
    }
}
