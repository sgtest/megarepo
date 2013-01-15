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

# Method lookup

Method lookup can be rather complex due to the interaction of a number
of factors, such as self types, autoderef, trait lookup, etc.  The
algorithm is divided into two parts: candidate collection and
candidate selection.

## Candidate collection

A `Candidate` is a method item that might plausibly be the method
being invoked.  Candidates are grouped into two kinds, inherent and
extension.  Inherent candidates are those that are derived from the
type of the receiver itself.  So, if you have a receiver of some
nominal type `Foo` (e.g., a struct), any methods defined within an
impl like `impl Foo` are inherent methods.  Nothing needs to be
imported to use an inherent method, they are associated with the type
itself (note that inherent impls can only be defined in the same
module as the type itself).

Inherent candidates are not always derived from impls.  If you have a
trait instance, such as a value of type `ToStr`, then the trait
methods (`to_str()`, in this case) are inherently associated with it.
Another case is type parameters, in which case the methods of their
bounds are inherent.

Extension candidates are derived from imported traits.  If I have the
trait `ToStr` imported, and I call `to_str()` on a value of type `T`,
then we will go off to find out whether there is an impl of `ToStr`
for `T`.  These kinds of method calls are called "extension methods".
They can be defined in any module, not only the one that defined `T`.
Furthermore, you must import the trait to call such a method.

For better or worse, we currently give weight to inherent methods over
extension methods during candidate selection (below).

## Candidate selection

Once we know the set of candidates, we can go off and try to select
which one is actually being called.  We do this by taking the type of
the receiver, let's call it R, and checking whether it matches against
the expected receiver type for each of the collected candidates.  We
first check for inherent candidates and see whether we get exactly one
match (zero means keep searching, more than one is an error).  If so,
we return that as the candidate.  Otherwise we search the extension
candidates in the same way.

If find no matching candidate at all, we proceed to auto-deref the
receiver type and search again.  We keep doing that until we cannot
auto-deref any longer.  At each step, we also check for candidates
based on "autoptr", which if the current type is `T`, checks for `&mut
T`, `&const T`, and `&T` receivers.  Finally, at the very end, we will
also try autoslice, which converts `~[]` to `&[]` (there is no point
at trying autoslice earlier, because no autoderefable type is also
sliceable).

## Why two phases?

You might wonder why we first collect the candidates and then select.
Both the inherent candidate collection and the candidate selection
proceed by progressively deref'ing the receiver type, after all.  The
answer is that two phases are needed to elegantly deal with explicit
self.  After all, if there is an impl for the type `Foo`, it can
define a method with the type `@self`, which means that it expects a
receiver of type `@Foo`.  If we have a receiver of type `@Foo`, but we
waited to search for that impl until we have deref'd the `@` away and
obtained the type `Foo`, we would never match this method.

*/

use core::prelude::*;

use middle::resolve::{Impl, MethodInfo};
use middle::resolve;
use middle::ty::*;
use middle::ty;
use middle::typeck::check::{fn_ctxt, impl_self_ty};
use middle::typeck::check::{structurally_resolved_type};
use middle::typeck::check::vtable::VtableContext;
use middle::typeck::check::vtable;
use middle::typeck::check;
use middle::typeck::coherence::get_base_type_def_id;
use middle::typeck::infer;
use middle::typeck::{method_map_entry, method_origin, method_param};
use middle::typeck::{method_self, method_static, method_trait};
use util::common::indenter;
use util::ppaux::expr_repr;

use core::dvec::DVec;
use core::result;
use core::uint;
use core::vec;
use std::map::HashMap;
use syntax::ast::{def_id, sty_by_ref, sty_value, sty_region, sty_box};
use syntax::ast::{sty_uniq, sty_static, node_id, by_copy, by_ref};
use syntax::ast::{m_const, m_mutbl, m_imm};
use syntax::ast;
use syntax::ast_map;
use syntax::ast_map::node_id_to_str;
use syntax::ast_util::dummy_sp;
use syntax::codemap::span;

fn lookup(
    fcx: @fn_ctxt,

    // In a call `a.b::<X, Y, ...>(...)`:
    expr: @ast::expr,        // The expression `a.b`.
    self_expr: @ast::expr,   // The expression `a`.
    callee_id: node_id, // Where to store the type of `a.b`
    m_name: ast::ident,      // The ident `b`.
    self_ty: ty::t,          // The type of `a`.
    supplied_tps: &[ty::t],  // The list of types X, Y, ... .
    deref_args: check::DerefArgs)   // Whether we autopointer first.
    -> Option<method_map_entry>
{
    let lcx = LookupContext {
        fcx: fcx,
        expr: expr,
        self_expr: self_expr,
        callee_id: callee_id,
        m_name: m_name,
        supplied_tps: supplied_tps,
        impl_dups: HashMap(),
        inherent_candidates: DVec(),
        extension_candidates: DVec(),
        deref_args: deref_args,
    };
    let mme = lcx.do_lookup(self_ty);
    debug!("method lookup for %s yielded %?",
           expr_repr(fcx.tcx(), expr), mme);
    return move mme;
}

struct LookupContext {
    fcx: @fn_ctxt,
    expr: @ast::expr,
    self_expr: @ast::expr,
    callee_id: node_id,
    m_name: ast::ident,
    supplied_tps: &[ty::t],
    impl_dups: HashMap<def_id, ()>,
    inherent_candidates: DVec<Candidate>,
    extension_candidates: DVec<Candidate>,
    deref_args: check::DerefArgs,
}

/**
 * A potential method that might be called, assuming the receiver
 * is of a suitable type. */
struct Candidate {
    rcvr_ty: ty::t,
    rcvr_substs: ty::substs,
    explicit_self: ast::self_ty_,

    // FIXME #3446---these two fields should be easily derived from
    // origin, yet are not
    num_method_tps: uint,
    self_mode: ast::rmode,

    origin: method_origin,
}

/**
 * How the self type should be transformed according to the form of explicit
 * self provided by the method.
 */
enum TransformTypeFlag {
    TransformTypeNormally,
    TransformTypeForObject,
}

impl LookupContext {
    fn do_lookup(&self, self_ty: ty::t) -> Option<method_map_entry> {
        debug!("do_lookup(self_ty=%s, expr=%s, self_expr=%s)",
               self.ty_to_str(self_ty),
               expr_repr(self.tcx(), self.expr),
               expr_repr(self.tcx(), self.self_expr));
        let _indenter = indenter();

        // Prepare the list of candidates
        self.push_inherent_candidates(self_ty);
        self.push_extension_candidates(self_ty);

        let enum_dids = DVec();
        let mut self_ty = self_ty;
        let mut autoderefs = 0;
        loop {
            debug!("loop: self_ty=%s autoderefs=%u",
                   self.ty_to_str(self_ty), autoderefs);

            match self.deref_args {
                check::DontDerefArgs => {
                    match self.search_for_autoderefd_method(self_ty,
                                                            autoderefs) {
                        Some(move mme) => { return Some(mme); }
                        None => {}
                    }

                    match self.search_for_autoptrd_method(self_ty,
                                                          autoderefs) {
                        Some(move mme) => { return Some(move mme); }
                        None => {}
                    }
                }
                check::DoDerefArgs => {
                    match self.search_for_autoptrd_method(self_ty,
                                                          autoderefs) {
                        Some(move mme) => { return Some(move mme); }
                        None => {}
                    }

                    match self.search_for_autoderefd_method(self_ty,
                                                            autoderefs) {
                        Some(move mme) => { return Some(mme); }
                        None => {}
                    }
                }
            }

            match self.deref(self_ty, &enum_dids) {
                None => { break; }
                Some(ty) => {
                    self_ty = ty;
                    autoderefs += 1;
                }
            }
        }

        self.search_for_autosliced_method(self_ty, autoderefs)
    }

    fn deref(ty: ty::t, enum_dids: &DVec<ast::def_id>) -> Option<ty::t> {
        match ty::get(ty).sty {
            ty_enum(did, _) => {
                // Watch out for newtype'd enums like "enum t = @T".
                // See discussion in typeck::check::do_autoderef().
                if enum_dids.contains(&did) {
                    return None;
                }
                enum_dids.push(did);
            }
            _ => {}
        }

        match ty::deref(self.tcx(), ty, false) {
            None => None,
            Some(t) => {
                //FIXME(#3211) -- probably want to force ivars
                Some(structurally_resolved_type(self.fcx,
                                                self.self_expr.span,
                                                t.ty))
            }
        }
    }

    // ______________________________________________________________________
    // Candidate collection (see comment at start of file)

    fn push_inherent_candidates(&self, self_ty: ty::t) {
        /*!
         *
         * Collect all inherent candidates into
         * `self.inherent_candidates`.  See comment at the start of
         * the file.  To find the inherent candidates, we repeatedly
         * deref the self-ty to find the "base-type".  So, for
         * example, if the receiver is @@C where `C` is a struct type,
         * we'll want to find the inherent impls for `C`. */

        let enum_dids = DVec();
        let mut self_ty = self_ty;
        loop {
            match get(self_ty).sty {
                ty_param(p) => {
                    self.push_inherent_candidates_from_param(self_ty, p);
                }
                ty_trait(did, ref substs, vstore) => {
                    self.push_inherent_candidates_from_trait(
                        self_ty, did, substs, vstore);
                    self.push_inherent_impl_candidates_for_type(did);
                }
                ty_self => {
                    // Call is of the form "self.foo()" and appears in one
                    // of a trait's default method implementations.
                    let self_did = self.fcx.self_info.expect(
                        ~"self_impl_def_id is undefined (`self` may not \
                          be in scope here").def_id;
                    let substs = {self_r: None, self_ty: None, tps: ~[]};
                    self.push_inherent_candidates_from_self(
                        self_ty, self_did, &substs);
                }
                ty_enum(did, _) | ty_struct(did, _) => {
                    self.push_inherent_impl_candidates_for_type(did);
                }
                _ => { /* No inherent methods in these types */ }
            }

            // n.b.: Generally speaking, we only loop if we hit the
            // fallthrough case in the match above.  The exception
            // would be newtype enums.
            self_ty = match self.deref(self_ty, &enum_dids) {
                None => { return; }
                Some(ty) => { ty }
            }
        }
    }

    fn push_extension_candidates(&self, self_ty: ty::t) {
        // If the method being called is associated with a trait, then
        // find all the impls of that trait.  Each of those are
        // candidates.
        let opt_applicable_traits = self.fcx.ccx.trait_map.find(self.expr.id);
        for opt_applicable_traits.each |applicable_traits| {
            for applicable_traits.each |trait_did| {
                let coherence_info = self.fcx.ccx.coherence_info;

                // Look for explicit implementations.
                let opt_impl_infos =
                    coherence_info.extension_methods.find(*trait_did);
                for opt_impl_infos.each |impl_infos| {
                    for impl_infos.each |impl_info| {
                        self.push_candidates_from_impl(
                            &self.extension_candidates, *impl_info);
                    }
                }

                // Look for default methods.
                match self.tcx().provided_methods.find(*trait_did) {
                    Some(methods) => {
                        self.push_candidates_from_provided_methods(
                            &self.extension_candidates, self_ty, *trait_did,
                            methods);
                    }
                    None => {}
                }
            }
        }
    }

    fn push_inherent_candidates_from_param(&self,
                                           rcvr_ty: ty::t,
                                           param_ty: param_ty) {
        debug!("push_inherent_candidates_from_param(param_ty=%?)",
               param_ty);
        let _indenter = indenter();

        let tcx = self.tcx();
        let mut next_bound_idx = 0; // count only trait bounds
        let bounds = tcx.ty_param_bounds.get(param_ty.def_id.node);

        for vec::each(*bounds) |bound| {
            let bound_trait_ty = match *bound {
                ty::bound_trait(bound_t) => bound_t,

                ty::bound_copy | ty::bound_owned |
                ty::bound_const | ty::bound_durable => {
                    loop; // skip non-trait bounds
                }
            };


            let bound_substs = match ty::get(bound_trait_ty).sty {
                ty::ty_trait(_, ref substs, _) => (/*bad*/copy *substs),
                _ => {
                    self.bug(fmt!("add_candidates_from_param: \
                                   non-trait bound %s",
                                  self.ty_to_str(bound_trait_ty)));
                }
            };


            // Loop over the trait and all of its supertraits.
            let mut worklist = ~[];

            let init_trait_ty = bound_trait_ty;
            let init_substs = bound_substs;

            // Replace any appearance of `self` with the type of the
            // generic parameter itself.  Note that this is the only
            // case where this replacement is necessary: in all other
            // cases, we are either invoking a method directly from an
            // impl or class (where the self type is not permitted),
            // or from a trait type (in which case methods that refer
            // to self are not permitted).
            let init_substs = {self_ty: Some(rcvr_ty), ..init_substs};

            worklist.push((init_trait_ty, init_substs));

            let mut i = 0;
            while i < worklist.len() {
                let (init_trait_ty, init_substs) = /*bad*/copy worklist[i];
                i += 1;

                let init_trait_id = ty::ty_to_def_id(init_trait_ty).get();

                // Add all the supertraits of this trait to the worklist.
                let supertraits = ty::trait_supertraits(tcx,
                                                        init_trait_id);
                for supertraits.each |supertrait| {
                    debug!("adding supertrait: %?",
                           supertrait.def_id);

                    let new_substs = ty::subst_substs(
                        tcx,
                        &supertrait.tpt.substs,
                        &init_substs);

                    // Again replacing the self type
                    let new_substs = {self_ty: Some(rcvr_ty), ..new_substs};

                    worklist.push((supertrait.tpt.ty, new_substs));
                }


                let this_bound_idx = next_bound_idx;
                next_bound_idx += 1;

                let trait_methods = ty::trait_methods(tcx, init_trait_id);
                let pos = {
                    // FIXME #3453 can't use trait_methods.position
                    match vec::position(*trait_methods,
                                        |m| (m.self_ty != ast::sty_static &&
                                             m.ident == self.m_name))
                    {
                        Some(pos) => pos,
                        None => {
                            debug!("trait doesn't contain method: %?",
                                   init_trait_id);
                            loop; // check next trait or bound
                        }
                    }
                };
                let method = &trait_methods[pos];

                let (rcvr_ty, rcvr_substs) =
                    self.create_rcvr_ty_and_substs_for_method(
                        method.self_ty,
                        rcvr_ty,
                        move init_substs,
                        TransformTypeNormally);

                let cand = Candidate {
                    rcvr_ty: rcvr_ty,
                    rcvr_substs: rcvr_substs,
                    explicit_self: method.self_ty,
                    num_method_tps: method.tps.len(),
                    self_mode: get_mode_from_self_type(method.self_ty),
                    origin: method_param({trait_id:init_trait_id,
                                          method_num:pos,
                                          param_num:param_ty.idx,
                                          bound_num:this_bound_idx})
                };

                debug!("pushing inherent candidate for param: %?", cand);
                self.inherent_candidates.push(cand);
            }
        }
    }

    fn push_inherent_candidates_from_trait(&self,
                                           self_ty: ty::t,
                                           did: def_id,
                                           substs: &ty::substs,
                                           vstore: ty::vstore) {
        debug!("push_inherent_candidates_from_trait(did=%s, substs=%s)",
               self.did_to_str(did),
               substs_to_str(self.tcx(), substs));
        let _indenter = indenter();

        let tcx = self.tcx();
        let ms = ty::trait_methods(tcx, did);
        let index = match vec::position(*ms, |m| m.ident == self.m_name) {
            Some(i) => i,
            None => { return; } // no method with the right name
        };
        let method = &ms[index];

        /* FIXME(#3157) we should transform the vstore in accordance
           with the self type

        match method.self_type {
            ast::sty_region(_) => {
                return; // inapplicable
            }
            ast::sty_by_ref | ast::sty_region(_) => vstore_slice(r)
            ast::sty_box(_) => vstore_box, // XXX NDM mutability
            ast::sty_uniq(_) => vstore_uniq
        }
        */

        // It is illegal to invoke a method on a trait instance that
        // refers to the `self` type.  Nonetheless, we substitute
        // `trait_ty` for `self` here, because it allows the compiler
        // to soldier on.  An error will be reported should this
        // candidate be selected if the method refers to `self`.
        let rcvr_substs = {self_ty: Some(self_ty), ../*bad*/copy *substs};

        let (rcvr_ty, rcvr_substs) =
            self.create_rcvr_ty_and_substs_for_method(method.self_ty,
                                                      self_ty,
                                                      move rcvr_substs,
                                                      TransformTypeForObject);

        self.inherent_candidates.push(Candidate {
            rcvr_ty: rcvr_ty,
            rcvr_substs: move rcvr_substs,
            explicit_self: method.self_ty,
            num_method_tps: method.tps.len(),
            self_mode: get_mode_from_self_type(method.self_ty),
            origin: method_trait(did, index, vstore)
        });
    }

    fn push_inherent_candidates_from_self(&self,
                                          self_ty: ty::t,
                                          did: def_id,
                                          substs: &ty::substs) {
        let tcx = self.tcx();
        let methods = ty::trait_methods(tcx, did);  // XXX: Inherited methods.
        let index;
        match vec::position(*methods, |m| m.ident == self.m_name) {
            Some(i) => index = i,
            None => return
        }
        let method = &methods[index];

        let rcvr_substs = { self_ty: Some(self_ty), ../*bad*/copy *substs };
        let (rcvr_ty, rcvr_substs) =
            self.create_rcvr_ty_and_substs_for_method(
                method.self_ty,
                self_ty,
                move rcvr_substs,
                TransformTypeNormally);

        self.inherent_candidates.push(Candidate {
            rcvr_ty: rcvr_ty,
            rcvr_substs: move rcvr_substs,
            explicit_self: method.self_ty,
            num_method_tps: method.tps.len(),
            self_mode: get_mode_from_self_type(method.self_ty),
            origin: method_self(did, index)
        });
    }

    fn push_inherent_impl_candidates_for_type(did: def_id) {
        let opt_impl_infos =
            self.fcx.ccx.coherence_info.inherent_methods.find(did);
        for opt_impl_infos.each |impl_infos| {
            for impl_infos.each |impl_info| {
                self.push_candidates_from_impl(
                    &self.inherent_candidates, *impl_info);
            }
        }
    }

    fn push_candidates_from_impl(&self, candidates: &DVec<Candidate>,
                                 impl_info: &resolve::Impl) {
        if !self.impl_dups.insert(impl_info.did, ()) {
            return; // already visited
        }

        let idx = {
            // FIXME #3453 can't use impl_info.methods.position
            match vec::position(impl_info.methods,
                                |m| m.ident == self.m_name) {
                Some(idx) => idx,
                None => { return; } // No method with the right name.
            }
        };

        let method = &impl_info.methods[idx];

        // determine the `self` of the impl with fresh
        // variables for each parameter:
        let location_info = &vtable::location_info_for_expr(self.self_expr);
        let vcx = VtableContext {
            ccx: self.fcx.ccx,
            infcx: self.fcx.infcx()
        };
        let {substs: impl_substs, ty: impl_ty} =
            impl_self_ty(&vcx, location_info, impl_info.did);

        let (impl_ty, impl_substs) =
            self.create_rcvr_ty_and_substs_for_method(
                method.self_type,
                impl_ty,
                move impl_substs,
                TransformTypeNormally);

        candidates.push(Candidate {
            rcvr_ty: impl_ty,
            rcvr_substs: move impl_substs,
            explicit_self: method.self_type,
            num_method_tps: method.n_tps,
            self_mode: get_mode_from_self_type(method.self_type),
            origin: method_static(method.did)
        });
    }

    fn push_candidates_from_provided_methods(
            &self,
            candidates: &DVec<Candidate>,
            self_ty: ty::t,
            trait_def_id: def_id,
            methods: @DVec<@ProvidedMethodInfo>) {
        debug!("(pushing candidates from provided methods) considering trait \
                id %d:%d",
               trait_def_id.crate,
               trait_def_id.node);

        for methods.each |provided_method_info| {
            if provided_method_info.method_info.ident != self.m_name { loop; }

            debug!("(pushing candidates from provided methods) adding \
                    candidate");

            // XXX: Needs to support generics.
            let dummy_substs = { self_r: None, self_ty: None, tps: ~[] };
            let (impl_ty, impl_substs) =
                self.create_rcvr_ty_and_substs_for_method(
                    provided_method_info.method_info.self_type,
                    self_ty,
                    dummy_substs,
                    TransformTypeNormally);

            candidates.push(Candidate {
                rcvr_ty: impl_ty,
                rcvr_substs: move impl_substs,
                explicit_self: provided_method_info.method_info.self_type,
                num_method_tps: provided_method_info.method_info.n_tps,
                self_mode: get_mode_from_self_type(
                    provided_method_info.method_info.self_type),
                origin: method_static(provided_method_info.method_info.did)
            });
        }
    }

    fn create_rcvr_ty_and_substs_for_method(&self,
                                            self_decl: ast::self_ty_,
                                            self_ty: ty::t,
                                            +self_substs: ty::substs,
                                            transform_type: TransformTypeFlag)
                                         -> (ty::t, ty::substs) {
        // If the self type includes a region (like &self), we need to
        // ensure that the receiver substitutions have a self region.
        // If the receiver type does not itself contain borrowed
        // pointers, there may not be one yet.
        //
        // FIXME(#3446)--this awkward situation comes about because
        // the regions in the receiver are substituted before (and
        // differently from) those in the argument types.  This
        // shouldn't really have to be.
        let rcvr_substs = {
            match self_decl {
                sty_static | sty_value | sty_by_ref |
                sty_box(_) | sty_uniq(_) => {
                    move self_substs
                }
                sty_region(_) if self_substs.self_r.is_some() => {
                    move self_substs
                }
                sty_region(_) => {
                    {self_r:
                         Some(self.infcx().next_region_var(
                             self.expr.span,
                             self.expr.id)),
                     ..self_substs}
                }
            }
        };

        let rcvr_ty = transform_self_type_for_method(self.tcx(),
                                                     rcvr_substs.self_r,
                                                     self_ty,
                                                     self_decl,
                                                     transform_type);

        (rcvr_ty, rcvr_substs)
    }

    // ______________________________________________________________________
    // Candidate selection (see comment at start of file)

    fn search_for_autoderefd_method(
        &self,
        self_ty: ty::t,
        autoderefs: uint)
        -> Option<method_map_entry>
    {
        match self.search_for_method(self_ty) {
            None => None,
            Some(move mme) => {
                debug!("(searching for autoderef'd method) writing \
                       adjustment (%u) to %d",
                       autoderefs,
                       self.self_expr.id);
                self.fcx.write_autoderef_adjustment(
                    self.self_expr.id, autoderefs);
                Some(mme)
            }
        }
    }

    fn search_for_autosliced_method(
        &self,
        self_ty: ty::t,
        autoderefs: uint)
        -> Option<method_map_entry>
    {
        /*!
         *
         * Searches for a candidate by converting things like
         * `~[]` to `&[]`. */

        let tcx = self.tcx();
        match ty::get(self_ty).sty {
            ty_evec(mt, vstore_box) |
            ty_evec(mt, vstore_uniq) |
            ty_evec(mt, vstore_fixed(_)) => {
                // First try to borrow to a slice
                let entry = self.search_for_some_kind_of_autorefd_method(
                    AutoBorrowVec, autoderefs, [m_const, m_imm, m_mutbl],
                    |m,r| ty::mk_evec(tcx,
                                      ty::mt {ty:mt.ty, mutbl:m},
                                      vstore_slice(r)));

                if entry.is_some() { return entry; }

                // Then try to borrow to a slice *and* borrow a pointer.
                self.search_for_some_kind_of_autorefd_method(
                    AutoBorrowVecRef, autoderefs, [m_const, m_imm, m_mutbl],
                    |m,r| {
                        let slice_ty = ty::mk_evec(tcx,
                                                   ty::mt {ty:mt.ty, mutbl:m},
                                                   vstore_slice(r));
                        // NB: we do not try to autoref to a mutable
                        // pointer. That would be creating a pointer
                        // to a temporary pointer (the borrowed
                        // slice), so any update the callee makes to
                        // it can't be observed.
                        ty::mk_rptr(tcx, r, ty::mt {ty:slice_ty, mutbl:m_imm})
                    })
            }

            ty_estr(vstore_box) |
            ty_estr(vstore_uniq) |
            ty_estr(vstore_fixed(_)) => {
                let entry = self.search_for_some_kind_of_autorefd_method(
                    AutoBorrowVec, autoderefs, [m_imm],
                    |_m,r| ty::mk_estr(tcx, vstore_slice(r)));

                if entry.is_some() { return entry; }

                self.search_for_some_kind_of_autorefd_method(
                    AutoBorrowVecRef, autoderefs, [m_imm],
                    |m,r| {
                        let slice_ty = ty::mk_estr(tcx, vstore_slice(r));
                        ty::mk_rptr(tcx, r, ty::mt {ty:slice_ty, mutbl:m})
                    })
            }

            ty_trait(*) | ty_fn(*) => {
                // NDM---eventually these should be some variant of autoref
                None
            }

            _ => None
        }
    }

    fn search_for_autoptrd_method(
        &self,
        self_ty: ty::t,
        autoderefs: uint)
        -> Option<method_map_entry>
    {
        /*!
         *
         * Converts any type `T` to `&M T` where `M` is an
         * appropriate mutability.
         */

        let tcx = self.tcx();
        match ty::get(self_ty).sty {
            ty_box(*) | ty_uniq(*) | ty_rptr(*) |
            ty_infer(IntVar(_)) | // FIXME(#3211)---should be resolved
            ty_infer(FloatVar(_)) | // FIXME(#3211)---should be resolved
            ty_self | ty_param(*) | ty_nil | ty_bot | ty_bool |
            ty_int(*) | ty_uint(*) |
            ty_float(*) | ty_enum(*) | ty_ptr(*) | ty_rec(*) |
            ty_struct(*) | ty_tup(*) | ty_estr(*) | ty_evec(*) |
            ty_trait(*) | ty_fn(*) => {
                self.search_for_some_kind_of_autorefd_method(
                    AutoPtr, autoderefs, [m_const, m_imm, m_mutbl],
                    |m,r| ty::mk_rptr(tcx, r, ty::mt {ty:self_ty, mutbl:m}))
            }

            ty_err => None,

            ty_opaque_closure_ptr(_) | ty_unboxed_vec(_) |
            ty_opaque_box | ty_type | ty_infer(TyVar(_)) => {
                self.bug(fmt!("Unexpected type: %s",
                              self.ty_to_str(self_ty)));
            }
        }
    }

    fn search_for_some_kind_of_autorefd_method(
        &self,
        kind: AutoRefKind,
        autoderefs: uint,
        mutbls: &[ast::mutability],
        mk_autoref_ty: &fn(ast::mutability, ty::Region) -> ty::t)
        -> Option<method_map_entry>
    {
        // This is hokey. We should have mutability inference as a
        // variable.  But for now, try &const, then &, then &mut:
        let region = self.infcx().next_region_var(self.expr.span,
                                                  self.expr.id);
        for mutbls.each |mutbl| {
            let autoref_ty = mk_autoref_ty(*mutbl, region);
            match self.search_for_method(autoref_ty) {
                None => {}
                Some(move mme) => {
                    self.fcx.write_adjustment(
                        self.self_expr.id,
                        @{autoderefs: autoderefs,
                          autoref: Some({kind: kind,
                                         region: region,
                                         mutbl: *mutbl})});
                    return Some(mme);
                }
            }
        }
        return None;
    }

    fn search_for_method(&self,
                         self_ty: ty::t)
        -> Option<method_map_entry>
    {
        debug!("search_for_method(self_ty=%s)", self.ty_to_str(self_ty));
        let _indenter = indenter();

        // I am not sure that inherent methods should have higher
        // priority, but it is necessary ATM to handle some of the
        // existing code.

        debug!("searching inherent candidates");
        match self.consider_candidates(self_ty, &self.inherent_candidates) {
            None => {}
            Some(move mme) => {
                return Some(move mme);
            }
        }

        debug!("searching extension candidates");
        match self.consider_candidates(self_ty, &self.extension_candidates) {
            None => {
                return None;
            }
            Some(move mme) => {
                return Some(move mme);
            }
        }
    }

    fn consider_candidates(&self,
                           self_ty: ty::t,
                           candidates: &DVec<Candidate>)
        -> Option<method_map_entry>
    {
        let relevant_candidates =
            candidates.filter_to_vec(|c| self.is_relevant(self_ty, c));

        let relevant_candidates = self.merge_candidates(relevant_candidates);

        if relevant_candidates.len() == 0 {
            return None;
        }

        if relevant_candidates.len() > 1 {
            self.tcx().sess.span_err(
                self.expr.span,
                ~"multiple applicable methods in scope");
            for uint::range(0, relevant_candidates.len()) |idx| {
                self.report_candidate(idx, &relevant_candidates[idx].origin);
            }
        }

        Some(self.confirm_candidate(self_ty, &relevant_candidates[0]))
    }

    fn merge_candidates(&self, candidates: &[Candidate]) -> ~[Candidate] {
        let mut merged = ~[];
        let mut i = 0;
        while i < candidates.len() {
            let candidate_a = /*bad*/copy candidates[i];

            let mut skip = false;

            let mut j = i + 1;
            while j < candidates.len() {
                let candidate_b = /*bad*/copy candidates[j];
                debug!("attempting to merge %? and %?",
                       candidate_a, candidate_b);
                let candidates_same = match (&candidate_a.origin,
                                             &candidate_b.origin) {
                    (&method_param(ref p1), &method_param(ref p2)) => {
                        let same_trait = p1.trait_id == p2.trait_id;
                        let same_method = p1.method_num == p2.method_num;
                        let same_param = p1.param_num == p2.param_num;
                        // The bound number may be different because
                        // multiple bounds may lead to the same trait
                        // impl
                        same_trait && same_method && same_param
                    }
                    _ => false
                };
                if candidates_same {
                    skip = true;
                    break;
                }
                j += 1;
            }

            i += 1;

            if skip {
                // There are more than one of these and we need only one
                loop;
            } else {
                merged.push(candidate_a);
            }
        }

        return merged;
    }

    fn confirm_candidate(&self,
                         self_ty: ty::t,
                         candidate: &Candidate)
        -> method_map_entry {
        let tcx = self.tcx();
        let fty = self.fn_ty_from_origin(&candidate.origin);

        self.enforce_trait_instance_limitations(fty, candidate);
        self.enforce_drop_trait_limitations(candidate);

        // before we only checked whether self_ty could be a subtype
        // of rcvr_ty; now we actually make it so (this may cause
        // variables to unify etc).  Since we checked beforehand, and
        // nothing has changed in the meantime, this unification
        // should never fail.
        match self.fcx.mk_subty(false, self.self_expr.span,
                                self_ty, candidate.rcvr_ty) {
            result::Ok(_) => (),
            result::Err(_) => {
                self.bug(fmt!("%s was assignable to %s but now is not?",
                              self.ty_to_str(self_ty),
                              self.ty_to_str(candidate.rcvr_ty)));
            }
        }

        // Determine the values for the type parameters of the method.
        // If they were not explicitly supplied, just construct fresh
        // type variables.
        let num_supplied_tps = self.supplied_tps.len();
        let m_substs = {
            if num_supplied_tps == 0u {
                self.fcx.infcx().next_ty_vars(candidate.num_method_tps)
            } else if candidate.num_method_tps == 0u {
                tcx.sess.span_err(
                    self.expr.span,
                    ~"this method does not take type parameters");
                self.fcx.infcx().next_ty_vars(candidate.num_method_tps)
            } else if num_supplied_tps != candidate.num_method_tps {
                tcx.sess.span_err(
                    self.expr.span,
                    ~"incorrect number of type \
                     parameters given for this method");
                self.fcx.infcx().next_ty_vars(candidate.num_method_tps)
            } else {
                self.supplied_tps.to_vec()
            }
        };

        // Construct the full set of type parameters for the method,
        // which is equal to the class tps + the method tps.
        let all_substs = {
            tps: vec::append(/*bad*/copy candidate.rcvr_substs.tps,
                             m_substs),
            ../*bad*/copy candidate.rcvr_substs
        };

        self.fcx.write_ty_substs(self.callee_id, fty, all_substs);
        return {self_arg: {mode: ast::expl(candidate.self_mode),
                           ty: candidate.rcvr_ty},
                explicit_self: candidate.explicit_self,
                origin: candidate.origin};
    }

    fn enforce_trait_instance_limitations(&self,
                                          method_fty: ty::t,
                                          candidate: &Candidate)
    {
        /*!
         *
         * There are some limitations to calling functions through a
         * traint instance, because (a) the self type is not known
         * (that's the whole point of a trait instance, after all, to
         * obscure the self type) and (b) the call must go through a
         * vtable and hence cannot be monomorphized. */

        match candidate.origin {
            method_static(*) | method_param(*) | method_self(*) => {
                return; // not a call to a trait instance
            }
            method_trait(*) => {}
        }

        if ty::type_has_self(method_fty) {
            self.tcx().sess.span_err(
                self.expr.span,
                ~"cannot call a method whose type contains a \
                  self-type through a boxed trait");
        }

        if candidate.num_method_tps > 0 {
            self.tcx().sess.span_err(
                self.expr.span,
                ~"cannot call a generic method through a boxed trait");
        }
    }

    fn enforce_drop_trait_limitations(&self, candidate: &Candidate) {
        // No code can call the finalize method explicitly.
        let bad;
        match candidate.origin {
            method_static(method_id) | method_self(method_id, _) => {
                bad = self.tcx().destructors.contains_key(method_id);
            }
            method_param({trait_id: trait_id, _}) |
            method_trait(trait_id, _, _) => {
                bad = self.tcx().destructor_for_type.contains_key(trait_id);
            }
        }

        if bad {
            self.tcx().sess.span_err(self.expr.span,
                                     ~"explicit call to destructor");
        }
    }

    fn is_relevant(&self, self_ty: ty::t, candidate: &Candidate) -> bool {
        debug!("is_relevant(self_ty=%s, candidate=%s)",
               self.ty_to_str(self_ty), self.cand_to_str(candidate));

        self.fcx.can_mk_subty(self_ty, candidate.rcvr_ty).is_ok()
    }

    fn fn_ty_from_origin(&self, origin: &method_origin) -> ty::t {
        return match *origin {
            method_static(did) => {
                ty::lookup_item_type(self.tcx(), did).ty
            }
            method_param(ref mp) => {
                type_of_trait_method(self.tcx(), mp.trait_id, mp.method_num)
            }
            method_trait(did, idx, _) | method_self(did, idx) => {
                type_of_trait_method(self.tcx(), did, idx)
            }
        };

        fn type_of_trait_method(tcx: ty::ctxt,
                                trait_did: def_id,
                                method_num: uint) -> ty::t {
            let trait_methods = ty::trait_methods(tcx, trait_did);
            ty::mk_fn(tcx, /*bad*/copy trait_methods[method_num].fty)
        }
    }

    fn report_candidate(&self, idx: uint, origin: &method_origin) {
        match *origin {
            method_static(impl_did) => {
                self.report_static_candidate(idx, impl_did)
            }
            method_param(ref mp) => {
                self.report_param_candidate(idx, (*mp).trait_id)
            }
            method_trait(trait_did, _, _) | method_self(trait_did, _) => {
                self.report_param_candidate(idx, trait_did)
            }
        }
    }

    fn report_static_candidate(&self, idx: uint, did: def_id) {
        let span = if did.crate == ast::local_crate {
            match self.tcx().items.get(did.node) {
              ast_map::node_method(m, _, _) => m.span,
              _ => fail ~"report_static_candidate: bad item"
            }
        } else {
            self.expr.span
        };
        self.tcx().sess.span_note(
            span,
            fmt!("candidate #%u is `%s`",
                 (idx+1u),
                 ty::item_path_str(self.tcx(), did)));
    }

    fn report_param_candidate(&self, idx: uint, did: def_id) {
        self.tcx().sess.span_note(
            self.expr.span,
            fmt!("candidate #%u derives from the bound `%s`",
                 (idx+1u),
                 ty::item_path_str(self.tcx(), did)));
    }

    fn report_trait_candidate(&self, idx: uint, did: def_id) {
        self.tcx().sess.span_note(
            self.expr.span,
            fmt!("candidate #%u derives from the type of the receiver, \
                  which is the trait `%s`",
                 (idx+1u),
                 ty::item_path_str(self.tcx(), did)));
    }

    fn infcx(&self) -> @infer::InferCtxt {
        self.fcx.inh.infcx
    }

    fn tcx(&self) -> ty::ctxt {
        self.fcx.tcx()
    }

    fn ty_to_str(&self, t: ty::t) -> ~str {
        self.fcx.infcx().ty_to_str(t)
    }

    fn cand_to_str(&self, cand: &Candidate) -> ~str {
        fmt!("Candidate(rcvr_ty=%s, rcvr_substs=%s, self_mode=%?, origin=%?)",
             self.ty_to_str(cand.rcvr_ty),
             ty::substs_to_str(self.tcx(), &cand.rcvr_substs),
             cand.self_mode,
             cand.origin)
    }

    fn did_to_str(&self, did: def_id) -> ~str {
        ty::item_path_str(self.tcx(), did)
    }

    fn bug(&self, +s: ~str) -> ! {
        self.tcx().sess.bug(s)
    }
}

fn transform_self_type_for_method(tcx: ty::ctxt,
                                  self_region: Option<ty::Region>,
                                  impl_ty: ty::t,
                                  self_type: ast::self_ty_,
                                  flag: TransformTypeFlag)
                               -> ty::t {
    match self_type {
      sty_static => {
        tcx.sess.bug(~"calling transform_self_type_for_method on \
                       static method");
      }
      sty_by_ref | sty_value => {
        impl_ty
      }
      sty_region(mutability) => {
        mk_rptr(tcx,
                self_region.expect(~"self region missing for &self param"),
                ty::mt { ty: impl_ty, mutbl: mutability })
      }
      sty_box(mutability) => {
        match flag {
            TransformTypeNormally => {
                mk_box(tcx, ty::mt { ty: impl_ty, mutbl: mutability })
            }
            TransformTypeForObject => impl_ty
        }
      }
      sty_uniq(mutability) => {
        match flag {
            TransformTypeNormally => {
                mk_uniq(tcx, ty::mt { ty: impl_ty, mutbl: mutability })
            }
            TransformTypeForObject => impl_ty
        }
      }
    }
}

fn get_mode_from_self_type(self_type: ast::self_ty_) -> ast::rmode {
    match self_type { sty_value => by_copy, _ => by_ref }
}
