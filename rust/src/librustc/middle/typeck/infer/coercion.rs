// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

# Type Coercion

Under certain circumstances we will coerce from one type to another,
for example by auto-borrowing.  This occurs in situations where the
compiler has a firm 'expected type' that was supplied from the user,
and where the actual type is similar to that expected type in purpose
but not in representation (so actual subtyping is inappropriate).

## Reborrowing

Note that if we are expecting a reference, we will *reborrow*
even if the argument provided was already a reference.  This is
useful for freezing mut/const things (that is, when the expected is &T
but you have &const T or &mut T) and also for avoiding the linearity
of mut things (when the expected is &mut T and you have &mut T).  See
the various `src/test/run-pass/coerce-reborrow-*.rs` tests for
examples of where this is useful.

## Subtle note

When deciding what type coercions to consider, we do not attempt to
resolve any type variables we may encounter.  This is because `b`
represents the expected type "as the user wrote it", meaning that if
the user defined a generic function like

   fn foo<A>(a: A, b: A) { ... }

and then we wrote `foo(&1, @2)`, we will not auto-borrow
either argument.  In older code we went to some lengths to
resolve the `b` variable, which could mean that we'd
auto-borrow later arguments but not earlier ones, which
seems very confusing.

## Subtler note

However, right now, if the user manually specifies the
values for the type variables, as so:

   foo::<&int>(@1, @2)

then we *will* auto-borrow, because we can't distinguish this from a
function that declared `&int`.  This is inconsistent but it's easiest
at the moment. The right thing to do, I think, is to consider the
*unsubstituted* type when deciding whether to auto-borrow, but the
*substituted* type when considering the bounds and so forth. But most
of our methods don't give access to the unsubstituted type, and
rightly so because they'd be error-prone.  So maybe the thing to do is
to actually determine the kind of coercions that should occur
separately and pass them in.  Or maybe it's ok as is.  Anyway, it's
sort of a minor point so I've opted to leave it for later---after all
we may want to adjust precisely when coercions occur.

*/

use middle::subst;
use middle::ty::{AutoPtr, AutoBorrowVec, AutoBorrowObj, AutoDerefRef};
use middle::ty::{mt};
use middle::ty;
use middle::typeck::infer::{CoerceResult, resolve_type, Coercion};
use middle::typeck::infer::combine::{CombineFields, Combine};
use middle::typeck::infer::sub::Sub;
use middle::typeck::infer::resolve::try_resolve_tvar_shallow;
use util::common::indenter;
use util::ppaux::Repr;

use syntax::abi;
use syntax::ast::MutImmutable;
use syntax::ast;

// Note: Coerce is not actually a combiner, in that it does not
// conform to the same interface, though it performs a similar
// function.
pub struct Coerce<'f>(pub CombineFields<'f>);

impl<'f> Coerce<'f> {
    pub fn get_ref<'a>(&'a self) -> &'a CombineFields<'f> {
        let Coerce(ref v) = *self; v
    }

    pub fn tys(&self, a: ty::t, b: ty::t) -> CoerceResult {
        debug!("Coerce.tys({} => {})",
               a.repr(self.get_ref().infcx.tcx),
               b.repr(self.get_ref().infcx.tcx));
        let _indent = indenter();

        // Examine the supertype and consider auto-borrowing.
        //
        // Note: does not attempt to resolve type variables we encounter.
        // See above for details.
        match ty::get(b).sty {
            ty::ty_rptr(r_b, mt_b) => {
                match ty::get(mt_b.ty).sty {
                    ty::ty_vec(mt_b, None) => {
                        return self.unpack_actual_value(a, |sty_a| {
                            self.coerce_borrowed_vector(a, sty_a, b, mt_b.mutbl)
                        });
                    }
                    ty::ty_vec(_, _) => {},
                    ty::ty_str => {
                        return self.unpack_actual_value(a, |sty_a| {
                            self.coerce_borrowed_string(a, sty_a, b)
                        });
                    }

                    ty::ty_trait(box ty::TyTrait { def_id, ref substs, bounds }) => {
                        let result = self.unpack_actual_value(a, |sty_a| {
                            match *sty_a {
                                ty::ty_rptr(_, mt_a) => match ty::get(mt_a.ty).sty {
                                    ty::ty_trait(..) => {
                                        self.coerce_borrowed_object(a, sty_a, b, mt_b.mutbl)
                                    }
                                    _ => self.coerce_object(a, sty_a, b, def_id, substs,
                                                            ty::RegionTraitStore(r_b, mt_b.mutbl),
                                                            bounds)
                                },
                                _ => self.coerce_borrowed_object(a, sty_a, b, mt_b.mutbl)
                            }
                        });

                        match result {
                            Ok(t) => return Ok(t),
                            Err(..) => {}
                        }
                    }

                    _ => {
                        return self.unpack_actual_value(a, |sty_a| {
                            self.coerce_borrowed_pointer(a, sty_a, b, mt_b)
                        });
                    }
                };
            }

            ty::ty_uniq(t_b) => {
                match ty::get(t_b).sty {
                    ty::ty_trait(box ty::TyTrait { def_id, ref substs, bounds }) => {
                        let result = self.unpack_actual_value(a, |sty_a| {
                            match *sty_a {
                                ty::ty_uniq(t_a) => match ty::get(t_a).sty {
                                    ty::ty_trait(..) => {
                                        Err(ty::terr_mismatch)
                                    }
                                    _ => self.coerce_object(a, sty_a, b, def_id, substs,
                                                            ty::UniqTraitStore, bounds)
                                },
                                _ => Err(ty::terr_mismatch)
                            }
                        });

                        match result {
                            Ok(t) => return Ok(t),
                            Err(..) => {}
                        }
                    }
                    _ => {}
                }
            }

            ty::ty_closure(box ty::ClosureTy {
                    store: ty::RegionTraitStore(..),
                    ..
                }) => {
                return self.unpack_actual_value(a, |sty_a| {
                    self.coerce_borrowed_fn(a, sty_a, b)
                });
            }

            ty::ty_ptr(mt_b) => {
                return self.unpack_actual_value(a, |sty_a| {
                    self.coerce_unsafe_ptr(a, sty_a, b, mt_b)
                });
            }

            _ => {}
        }

        self.unpack_actual_value(a, |sty_a| {
            match *sty_a {
                ty::ty_bare_fn(ref a_f) => {
                    // Bare functions are coercible to any closure type.
                    //
                    // FIXME(#3320) this should go away and be
                    // replaced with proper inference, got a patch
                    // underway - ndm
                    self.coerce_from_bare_fn(a, a_f, b)
                }
                _ => {
                    // Otherwise, just use subtyping rules.
                    self.subtype(a, b)
                }
            }
        })
    }

    pub fn subtype(&self, a: ty::t, b: ty::t) -> CoerceResult {
        match Sub(self.get_ref().clone()).tys(a, b) {
            Ok(_) => Ok(None),         // No coercion required.
            Err(ref e) => Err(*e)
        }
    }

    pub fn unpack_actual_value(&self, a: ty::t, f: |&ty::sty| -> CoerceResult)
                               -> CoerceResult {
        match resolve_type(self.get_ref().infcx, a, try_resolve_tvar_shallow) {
            Ok(t) => {
                f(&ty::get(t).sty)
            }
            Err(e) => {
                self.get_ref().infcx.tcx.sess.span_bug(
                    self.get_ref().trace.origin.span(),
                    format!("failed to resolve even without \
                          any force options: {:?}", e).as_slice());
            }
        }
    }

    pub fn coerce_borrowed_pointer(&self,
                                   a: ty::t,
                                   sty_a: &ty::sty,
                                   b: ty::t,
                                   mt_b: ty::mt)
                                   -> CoerceResult {
        debug!("coerce_borrowed_pointer(a={}, sty_a={:?}, b={}, mt_b={:?})",
               a.repr(self.get_ref().infcx.tcx), sty_a,
               b.repr(self.get_ref().infcx.tcx), mt_b);

        // If we have a parameter of type `&M T_a` and the value
        // provided is `expr`, we will be adding an implicit borrow,
        // meaning that we convert `f(expr)` to `f(&M *expr)`.  Therefore,
        // to type check, we will construct the type that `&M*expr` would
        // yield.

        let sub = Sub(self.get_ref().clone());
        let coercion = Coercion(self.get_ref().trace.clone());
        let r_borrow = self.get_ref().infcx.next_region_var(coercion);

        let inner_ty = match *sty_a {
            ty::ty_box(typ) | ty::ty_uniq(typ) => typ,
            ty::ty_rptr(_, mt_a) => mt_a.ty,
            _ => {
                return self.subtype(a, b);
            }
        };

        let a_borrowed = ty::mk_rptr(self.get_ref().infcx.tcx,
                                     r_borrow,
                                     mt {ty: inner_ty, mutbl: mt_b.mutbl});
        if_ok!(sub.tys(a_borrowed, b));
        Ok(Some(AutoDerefRef(AutoDerefRef {
            autoderefs: 1,
            autoref: Some(AutoPtr(r_borrow, mt_b.mutbl))
        })))
    }

    pub fn coerce_borrowed_string(&self,
                                  a: ty::t,
                                  sty_a: &ty::sty,
                                  b: ty::t)
                                  -> CoerceResult {
        debug!("coerce_borrowed_string(a={}, sty_a={:?}, b={})",
               a.repr(self.get_ref().infcx.tcx), sty_a,
               b.repr(self.get_ref().infcx.tcx));

        match *sty_a {
            ty::ty_uniq(t) => match ty::get(t).sty {
                ty::ty_str => {}
                _ => return self.subtype(a, b),
            },
            _ => {
                return self.subtype(a, b);
            }
        };

        let coercion = Coercion(self.get_ref().trace.clone());
        let r_a = self.get_ref().infcx.next_region_var(coercion);
        let a_borrowed = ty::mk_str_slice(self.get_ref().infcx.tcx, r_a, ast::MutImmutable);
        if_ok!(self.subtype(a_borrowed, b));
        Ok(Some(AutoDerefRef(AutoDerefRef {
            autoderefs: 0,
            autoref: Some(AutoBorrowVec(r_a, MutImmutable))
        })))
    }

    pub fn coerce_borrowed_vector(&self,
                                  a: ty::t,
                                  sty_a: &ty::sty,
                                  b: ty::t,
                                  mutbl_b: ast::Mutability)
                                  -> CoerceResult {
        debug!("coerce_borrowed_vector(a={}, sty_a={:?}, b={})",
               a.repr(self.get_ref().infcx.tcx), sty_a,
               b.repr(self.get_ref().infcx.tcx));

        let sub = Sub(self.get_ref().clone());
        let coercion = Coercion(self.get_ref().trace.clone());
        let r_borrow = self.get_ref().infcx.next_region_var(coercion);
        let ty_inner = match *sty_a {
            ty::ty_uniq(t) | ty::ty_ptr(ty::mt{ty: t, ..}) |
            ty::ty_rptr(_, ty::mt{ty: t, ..}) => match ty::get(t).sty {
                ty::ty_vec(mt, None) => mt.ty,
                _ => {
                    return self.subtype(a, b);
                }
            },
            ty::ty_vec(mt, _) => mt.ty,
            _ => {
                return self.subtype(a, b);
            }
        };

        let a_borrowed = ty::mk_slice(self.get_ref().infcx.tcx, r_borrow,
                                      mt {ty: ty_inner, mutbl: mutbl_b});
        if_ok!(sub.tys(a_borrowed, b));
        Ok(Some(AutoDerefRef(AutoDerefRef {
            autoderefs: 0,
            autoref: Some(AutoBorrowVec(r_borrow, mutbl_b))
        })))
    }

    fn coerce_borrowed_object(&self,
                              a: ty::t,
                              sty_a: &ty::sty,
                              b: ty::t,
                              b_mutbl: ast::Mutability) -> CoerceResult
    {
        debug!("coerce_borrowed_object(a={}, sty_a={:?}, b={})",
               a.repr(self.get_ref().infcx.tcx), sty_a,
               b.repr(self.get_ref().infcx.tcx));

        let tcx = self.get_ref().infcx.tcx;
        let coercion = Coercion(self.get_ref().trace.clone());
        let r_a = self.get_ref().infcx.next_region_var(coercion);

        let a_borrowed = match *sty_a {
            ty::ty_uniq(ty) | ty::ty_rptr(_, ty::mt{ty, ..}) => match ty::get(ty).sty {
                ty::ty_trait(box ty::TyTrait {
                        def_id,
                        ref substs,
                        bounds,
                        ..
                    }) => {
                    let tr = ty::mk_trait(tcx, def_id, substs.clone(), bounds);
                    ty::mk_rptr(tcx, r_a, ty::mt{ mutbl: b_mutbl, ty: tr })
                }
                _ => {
                    return self.subtype(a, b);
                }
            },
            _ => {
                return self.subtype(a, b);
            }
        };

        if_ok!(self.subtype(a_borrowed, b));
        Ok(Some(AutoDerefRef(AutoDerefRef {
            autoderefs: 0,
            autoref: Some(AutoBorrowObj(r_a, b_mutbl))
        })))
    }

    pub fn coerce_borrowed_fn(&self,
                              a: ty::t,
                              sty_a: &ty::sty,
                              b: ty::t)
                              -> CoerceResult {
        debug!("coerce_borrowed_fn(a={}, sty_a={:?}, b={})",
               a.repr(self.get_ref().infcx.tcx), sty_a,
               b.repr(self.get_ref().infcx.tcx));

        match *sty_a {
            ty::ty_bare_fn(ref f) => {
                self.coerce_from_bare_fn(a, f, b)
            }
            _ => {
                self.subtype(a, b)
            }
        }
    }

    fn coerce_from_bare_fn(&self, a: ty::t, fn_ty_a: &ty::BareFnTy, b: ty::t)
                           -> CoerceResult {
        /*!
         *
         * Attempts to coerce from a bare Rust function (`extern
         * "Rust" fn`) into a closure or a `proc`.
         */

        self.unpack_actual_value(b, |sty_b| {

            debug!("coerce_from_bare_fn(a={}, b={})",
                   a.repr(self.get_ref().infcx.tcx), b.repr(self.get_ref().infcx.tcx));

            if fn_ty_a.abi != abi::Rust || fn_ty_a.fn_style != ast::NormalFn {
                return self.subtype(a, b);
            }

            let fn_ty_b = match *sty_b {
                ty::ty_closure(ref f) => (*f).clone(),
                _ => return self.subtype(a, b)
            };

            let adj = ty::AutoAddEnv(fn_ty_b.store);
            let a_closure = ty::mk_closure(self.get_ref().infcx.tcx,
                                           ty::ClosureTy {
                                                sig: fn_ty_a.sig.clone(),
                                                .. *fn_ty_b
                                           });
            if_ok!(self.subtype(a_closure, b));
            Ok(Some(adj))
        })
    }

    pub fn coerce_unsafe_ptr(&self,
                             a: ty::t,
                             sty_a: &ty::sty,
                             b: ty::t,
                             mt_b: ty::mt)
                             -> CoerceResult {
        debug!("coerce_unsafe_ptr(a={}, sty_a={:?}, b={})",
               a.repr(self.get_ref().infcx.tcx), sty_a,
               b.repr(self.get_ref().infcx.tcx));

        let mt_a = match *sty_a {
            ty::ty_rptr(_, mt) => mt,
            _ => {
                return self.subtype(a, b);
            }
        };

        // check that the types which they point at are compatible
        let a_unsafe = ty::mk_ptr(self.get_ref().infcx.tcx, mt_a);
        if_ok!(self.subtype(a_unsafe, b));

        // although references and unsafe ptrs have the same
        // representation, we still register an AutoDerefRef so that
        // regionck knows that the region for `a` must be valid here
        Ok(Some(AutoDerefRef(AutoDerefRef {
            autoderefs: 1,
            autoref: Some(ty::AutoUnsafe(mt_b.mutbl))
        })))
    }

    pub fn coerce_object(&self,
                         a: ty::t,
                         sty_a: &ty::sty,
                         b: ty::t,
                         trait_def_id: ast::DefId,
                         trait_substs: &subst::Substs,
                         trait_store: ty::TraitStore,
                         bounds: ty::BuiltinBounds) -> CoerceResult {

        debug!("coerce_object(a={}, sty_a={:?}, b={})",
               a.repr(self.get_ref().infcx.tcx), sty_a,
               b.repr(self.get_ref().infcx.tcx));

        Ok(Some(ty::AutoObject(trait_store, bounds,
                               trait_def_id, trait_substs.clone())))
    }
}
