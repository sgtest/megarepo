// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use super::MethodError;
use super::ItemIndex;
use super::{CandidateSource,ImplSource,TraitSource};
use super::suggest;

use check;
use check::{FnCtxt, NoPreference, UnresolvedTypeAction};
use middle::fast_reject;
use middle::subst;
use middle::subst::Subst;
use middle::traits;
use middle::ty::{self, RegionEscape, Ty, ToPolyTraitRef};
use middle::ty_fold::TypeFoldable;
use middle::infer;
use middle::infer::InferCtxt;
use syntax::ast;
use syntax::codemap::{Span, DUMMY_SP};
use std::collections::HashSet;
use std::mem;
use std::rc::Rc;
use util::ppaux::Repr;

use self::CandidateKind::*;
pub use self::PickKind::*;

struct ProbeContext<'a, 'tcx:'a> {
    fcx: &'a FnCtxt<'a, 'tcx>,
    span: Span,
    mode: Mode,
    item_name: ast::Name,
    steps: Rc<Vec<CandidateStep<'tcx>>>,
    opt_simplified_steps: Option<Vec<fast_reject::SimplifiedType>>,
    inherent_candidates: Vec<Candidate<'tcx>>,
    extension_candidates: Vec<Candidate<'tcx>>,
    impl_dups: HashSet<ast::DefId>,
    static_candidates: Vec<CandidateSource>,
}

struct CandidateStep<'tcx> {
    self_ty: Ty<'tcx>,
    autoderefs: usize,
    unsize: bool
}

struct Candidate<'tcx> {
    xform_self_ty: Ty<'tcx>,
    item: ty::ImplOrTraitItem<'tcx>,
    kind: CandidateKind<'tcx>,
}

enum CandidateKind<'tcx> {
    InherentImplCandidate(/* Impl */ ast::DefId, subst::Substs<'tcx>),
    ObjectCandidate(/* Trait */ ast::DefId, /* method_num */ usize, /* vtable index */ usize),
    ExtensionImplCandidate(/* Impl */ ast::DefId, ty::TraitRef<'tcx>,
                           subst::Substs<'tcx>, ItemIndex),
    ClosureCandidate(/* Trait */ ast::DefId, ItemIndex),
    WhereClauseCandidate(ty::PolyTraitRef<'tcx>, ItemIndex),
    ProjectionCandidate(ast::DefId, ItemIndex),
}

pub struct Pick<'tcx> {
    pub item: ty::ImplOrTraitItem<'tcx>,
    pub kind: PickKind<'tcx>,

    // Indicates that the source expression should be autoderef'd N times
    //
    // A = expr | *expr | **expr | ...
    pub autoderefs: usize,

    // Indicates that an autoref is applied after the optional autoderefs
    //
    // B = A | &A | &mut A
    pub autoref: Option<ast::Mutability>,

    // Indicates that the source expression should be "unsized" to a
    // target type. This should probably eventually go away in favor
    // of just coercing method receivers.
    //
    // C = B | unsize(B)
    pub unsize: Option<Ty<'tcx>>,
}

#[derive(Clone,Debug)]
pub enum PickKind<'tcx> {
    InherentImplPick(/* Impl */ ast::DefId),
    ObjectPick(/* Trait */ ast::DefId, /* method_num */ usize, /* real_index */ usize),
    ExtensionImplPick(/* Impl */ ast::DefId, ItemIndex),
    TraitPick(/* Trait */ ast::DefId, ItemIndex),
    WhereClausePick(/* Trait */ ty::PolyTraitRef<'tcx>, ItemIndex),
}

pub type PickResult<'tcx> = Result<Pick<'tcx>, MethodError>;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum Mode {
    // An expression of the form `receiver.method_name(...)`.
    // Autoderefs are performed on `receiver`, lookup is done based on the
    // `self` argument  of the method, and static methods aren't considered.
    MethodCall,
    // An expression of the form `Type::item` or `<T>::item`.
    // No autoderefs are performed, lookup is done based on the type each
    // implementation is for, and static methods are included.
    Path
}

pub fn probe<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                       span: Span,
                       mode: Mode,
                       item_name: ast::Name,
                       self_ty: Ty<'tcx>,
                       scope_expr_id: ast::NodeId)
                       -> PickResult<'tcx>
{
    debug!("probe(self_ty={}, item_name={}, scope_expr_id={})",
           self_ty.repr(fcx.tcx()),
           item_name,
           scope_expr_id);

    // FIXME(#18741) -- right now, creating the steps involves evaluating the
    // `*` operator, which registers obligations that then escape into
    // the global fulfillment context and thus has global
    // side-effects. This is a bit of a pain to refactor. So just let
    // it ride, although it's really not great, and in fact could I
    // think cause spurious errors. Really though this part should
    // take place in the `fcx.infcx().probe` below.
    let steps = if mode == Mode::MethodCall {
        match create_steps(fcx, span, self_ty) {
            Some(steps) => steps,
            None => return Err(MethodError::NoMatch(Vec::new(), Vec::new())),
        }
    } else {
        vec![CandidateStep {
            self_ty: self_ty,
            autoderefs: 0,
            unsize: false
        }]
    };

    // Create a list of simplified self types, if we can.
    let mut simplified_steps = Vec::new();
    for step in &steps {
        match fast_reject::simplify_type(fcx.tcx(), step.self_ty, true) {
            None => { break; }
            Some(simplified_type) => { simplified_steps.push(simplified_type); }
        }
    }
    let opt_simplified_steps =
        if simplified_steps.len() < steps.len() {
            None // failed to convert at least one of the steps
        } else {
            Some(simplified_steps)
        };

    debug!("ProbeContext: steps for self_ty={} are {}",
           self_ty.repr(fcx.tcx()),
           steps.repr(fcx.tcx()));

    // this creates one big transaction so that all type variables etc
    // that we create during the probe process are removed later
    fcx.infcx().probe(|_| {
        let mut probe_cx = ProbeContext::new(fcx,
                                             span,
                                             mode,
                                             item_name,
                                             steps,
                                             opt_simplified_steps);
        probe_cx.assemble_inherent_candidates();
        try!(probe_cx.assemble_extension_candidates_for_traits_in_scope(scope_expr_id));
        probe_cx.pick()
    })
}

fn create_steps<'a, 'tcx>(fcx: &FnCtxt<'a, 'tcx>,
                          span: Span,
                          self_ty: Ty<'tcx>)
                          -> Option<Vec<CandidateStep<'tcx>>> {
    let mut steps = Vec::new();

    let (final_ty, dereferences, _) = check::autoderef(fcx,
                                                       span,
                                                       self_ty,
                                                       None,
                                                       UnresolvedTypeAction::Error,
                                                       NoPreference,
                                                       |t, d| {
        steps.push(CandidateStep {
            self_ty: t,
            autoderefs: d,
            unsize: false
        });
        None::<()> // keep iterating until we can't anymore
    });

    match final_ty.sty {
        ty::ty_vec(elem_ty, Some(_)) => {
            let slice_ty = ty::mk_vec(fcx.tcx(), elem_ty, None);
            steps.push(CandidateStep {
                self_ty: slice_ty,
                autoderefs: dereferences,
                unsize: true
            });
        }
        ty::ty_err => return None,
        _ => (),
    }

    Some(steps)
}

impl<'a,'tcx> ProbeContext<'a,'tcx> {
    fn new(fcx: &'a FnCtxt<'a,'tcx>,
           span: Span,
           mode: Mode,
           item_name: ast::Name,
           steps: Vec<CandidateStep<'tcx>>,
           opt_simplified_steps: Option<Vec<fast_reject::SimplifiedType>>)
           -> ProbeContext<'a,'tcx>
    {
        ProbeContext {
            fcx: fcx,
            span: span,
            mode: mode,
            item_name: item_name,
            inherent_candidates: Vec::new(),
            extension_candidates: Vec::new(),
            impl_dups: HashSet::new(),
            steps: Rc::new(steps),
            opt_simplified_steps: opt_simplified_steps,
            static_candidates: Vec::new(),
        }
    }

    fn reset(&mut self) {
        self.inherent_candidates.clear();
        self.extension_candidates.clear();
        self.impl_dups.clear();
        self.static_candidates.clear();
    }

    fn tcx(&self) -> &'a ty::ctxt<'tcx> {
        self.fcx.tcx()
    }

    fn infcx(&self) -> &'a InferCtxt<'a, 'tcx> {
        self.fcx.infcx()
    }

    ///////////////////////////////////////////////////////////////////////////
    // CANDIDATE ASSEMBLY

    fn assemble_inherent_candidates(&mut self) {
        let steps = self.steps.clone();
        for step in &*steps {
            self.assemble_probe(step.self_ty);
        }
    }

    fn assemble_probe(&mut self, self_ty: Ty<'tcx>) {
        debug!("assemble_probe: self_ty={}",
               self_ty.repr(self.tcx()));

        match self_ty.sty {
            ty::ty_trait(box ref data) => {
                self.assemble_inherent_candidates_from_object(self_ty, data);
                self.assemble_inherent_impl_candidates_for_type(data.principal_def_id());
            }
            ty::ty_enum(did, _) |
            ty::ty_struct(did, _) |
            ty::ty_closure(did, _) => {
                self.assemble_inherent_impl_candidates_for_type(did);
            }
            ty::ty_uniq(_) => {
                if let Some(box_did) = self.tcx().lang_items.owned_box() {
                    self.assemble_inherent_impl_candidates_for_type(box_did);
                }
            }
            ty::ty_param(p) => {
                self.assemble_inherent_candidates_from_param(self_ty, p);
            }
            ty::ty_char => {
                let lang_def_id = self.tcx().lang_items.char_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_str => {
                let lang_def_id = self.tcx().lang_items.str_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_vec(_, None) => {
                let lang_def_id = self.tcx().lang_items.slice_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_ptr(ty::mt { ty: _, mutbl: ast::MutImmutable }) => {
                let lang_def_id = self.tcx().lang_items.const_ptr_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_ptr(ty::mt { ty: _, mutbl: ast::MutMutable }) => {
                let lang_def_id = self.tcx().lang_items.mut_ptr_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_int(ast::TyI8) => {
                let lang_def_id = self.tcx().lang_items.i8_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_int(ast::TyI16) => {
                let lang_def_id = self.tcx().lang_items.i16_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_int(ast::TyI32) => {
                let lang_def_id = self.tcx().lang_items.i32_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_int(ast::TyI64) => {
                let lang_def_id = self.tcx().lang_items.i64_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_int(ast::TyIs) => {
                let lang_def_id = self.tcx().lang_items.isize_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_uint(ast::TyU8) => {
                let lang_def_id = self.tcx().lang_items.u8_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_uint(ast::TyU16) => {
                let lang_def_id = self.tcx().lang_items.u16_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_uint(ast::TyU32) => {
                let lang_def_id = self.tcx().lang_items.u32_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_uint(ast::TyU64) => {
                let lang_def_id = self.tcx().lang_items.u64_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_uint(ast::TyUs) => {
                let lang_def_id = self.tcx().lang_items.usize_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_float(ast::TyF32) => {
                let lang_def_id = self.tcx().lang_items.f32_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            ty::ty_float(ast::TyF64) => {
                let lang_def_id = self.tcx().lang_items.f64_impl();
                self.assemble_inherent_impl_for_primitive(lang_def_id);
            }
            _ => {
            }
        }
    }

    fn assemble_inherent_impl_for_primitive(&mut self, lang_def_id: Option<ast::DefId>) {
        if let Some(impl_def_id) = lang_def_id {
            ty::populate_implementations_for_primitive_if_necessary(self.tcx(), impl_def_id);

            self.assemble_inherent_impl_probe(impl_def_id);
        }
    }

    fn assemble_inherent_impl_candidates_for_type(&mut self, def_id: ast::DefId) {
        // Read the inherent implementation candidates for this type from the
        // metadata if necessary.
        ty::populate_implementations_for_type_if_necessary(self.tcx(), def_id);

        if let Some(impl_infos) = self.tcx().inherent_impls.borrow().get(&def_id) {
            for &impl_def_id in &***impl_infos {
                self.assemble_inherent_impl_probe(impl_def_id);
            }
        }
    }

    fn assemble_inherent_impl_probe(&mut self, impl_def_id: ast::DefId) {
        if !self.impl_dups.insert(impl_def_id) {
            return; // already visited
        }

        debug!("assemble_inherent_impl_probe {:?}", impl_def_id);

        let item = match impl_item(self.tcx(), impl_def_id, self.item_name) {
            Some(m) => m,
            None => { return; } // No method with correct name on this impl
        };

        if !self.has_applicable_self(&item) {
            // No receiver declared. Not a candidate.
            return self.record_static_candidate(ImplSource(impl_def_id));
        }

        let (impl_ty, impl_substs) = self.impl_ty_and_substs(impl_def_id);
        let impl_ty = self.fcx.instantiate_type_scheme(self.span, &impl_substs, &impl_ty);

        // Determine the receiver type that the method itself expects.
        let xform_self_ty =
            self.xform_self_ty(&item, impl_ty, &impl_substs);

        self.inherent_candidates.push(Candidate {
            xform_self_ty: xform_self_ty,
            item: item,
            kind: InherentImplCandidate(impl_def_id, impl_substs)
        });
    }

    fn assemble_inherent_candidates_from_object(&mut self,
                                                self_ty: Ty<'tcx>,
                                                data: &ty::TyTrait<'tcx>) {
        debug!("assemble_inherent_candidates_from_object(self_ty={})",
               self_ty.repr(self.tcx()));

        let tcx = self.tcx();

        // It is illegal to invoke a method on a trait instance that
        // refers to the `Self` type. An error will be reported by
        // `enforce_object_limitations()` if the method refers to the
        // `Self` type anywhere other than the receiver. Here, we use
        // a substitution that replaces `Self` with the object type
        // itself. Hence, a `&self` method will wind up with an
        // argument type like `&Trait`.
        let trait_ref = data.principal_trait_ref_with_self_ty(self.tcx(), self_ty);
        self.elaborate_bounds(&[trait_ref.clone()], |this, new_trait_ref, item, item_num| {
            let new_trait_ref = this.erase_late_bound_regions(&new_trait_ref);

            let vtable_index =
                traits::get_vtable_index_of_object_method(tcx,
                                                          trait_ref.clone(),
                                                          new_trait_ref.def_id,
                                                          item_num);

            let xform_self_ty = this.xform_self_ty(&item,
                                                   new_trait_ref.self_ty(),
                                                   new_trait_ref.substs);

            this.inherent_candidates.push(Candidate {
                xform_self_ty: xform_self_ty,
                item: item,
                kind: ObjectCandidate(new_trait_ref.def_id, item_num, vtable_index)
            });
        });
    }

    fn assemble_inherent_candidates_from_param(&mut self,
                                               _rcvr_ty: Ty<'tcx>,
                                               param_ty: ty::ParamTy) {
        // FIXME -- Do we want to commit to this behavior for param bounds?

        let bounds: Vec<_> =
            self.fcx.inh.param_env.caller_bounds
            .iter()
            .filter_map(|predicate| {
                match *predicate {
                    ty::Predicate::Trait(ref trait_predicate) => {
                        match trait_predicate.0.trait_ref.self_ty().sty {
                            ty::ty_param(ref p) if *p == param_ty => {
                                Some(trait_predicate.to_poly_trait_ref())
                            }
                            _ => None
                        }
                    }
                    ty::Predicate::Equate(..) |
                    ty::Predicate::Projection(..) |
                    ty::Predicate::RegionOutlives(..) |
                    ty::Predicate::TypeOutlives(..) => {
                        None
                    }
                }
            })
            .collect();

        self.elaborate_bounds(&bounds, |this, poly_trait_ref, item, item_num| {
            let trait_ref =
                this.erase_late_bound_regions(&poly_trait_ref);

            let xform_self_ty =
                this.xform_self_ty(&item,
                                   trait_ref.self_ty(),
                                   trait_ref.substs);

            if let Some(ref m) = item.as_opt_method() {
                debug!("found match: trait_ref={} substs={} m={}",
                       trait_ref.repr(this.tcx()),
                       trait_ref.substs.repr(this.tcx()),
                       m.repr(this.tcx()));
                assert_eq!(m.generics.types.get_slice(subst::TypeSpace).len(),
                           trait_ref.substs.types.get_slice(subst::TypeSpace).len());
                assert_eq!(m.generics.regions.get_slice(subst::TypeSpace).len(),
                           trait_ref.substs.regions().get_slice(subst::TypeSpace).len());
                assert_eq!(m.generics.types.get_slice(subst::SelfSpace).len(),
                           trait_ref.substs.types.get_slice(subst::SelfSpace).len());
                assert_eq!(m.generics.regions.get_slice(subst::SelfSpace).len(),
                           trait_ref.substs.regions().get_slice(subst::SelfSpace).len());
            }

            // Because this trait derives from a where-clause, it
            // should not contain any inference variables or other
            // artifacts. This means it is safe to put into the
            // `WhereClauseCandidate` and (eventually) into the
            // `WhereClausePick`.
            assert!(trait_ref.substs.types.iter().all(|&t| !ty::type_needs_infer(t)));

            this.inherent_candidates.push(Candidate {
                xform_self_ty: xform_self_ty,
                item: item,
                kind: WhereClauseCandidate(poly_trait_ref, item_num)
            });
        });
    }

    // Do a search through a list of bounds, using a callback to actually
    // create the candidates.
    fn elaborate_bounds<F>(
        &mut self,
        bounds: &[ty::PolyTraitRef<'tcx>],
        mut mk_cand: F,
    ) where
        F: for<'b> FnMut(
            &mut ProbeContext<'b, 'tcx>,
            ty::PolyTraitRef<'tcx>,
            ty::ImplOrTraitItem<'tcx>,
            usize,
        ),
    {
        debug!("elaborate_bounds(bounds={})", bounds.repr(self.tcx()));

        let tcx = self.tcx();
        for bound_trait_ref in traits::transitive_bounds(tcx, bounds) {
            let (pos, item) = match trait_item(tcx,
                                               bound_trait_ref.def_id(),
                                               self.item_name) {
                Some(v) => v,
                None => { continue; }
            };

            if !self.has_applicable_self(&item) {
                self.record_static_candidate(TraitSource(bound_trait_ref.def_id()));
            } else {
                mk_cand(self, bound_trait_ref, item, pos);
            }
        }
    }

    fn assemble_extension_candidates_for_traits_in_scope(&mut self,
                                                         expr_id: ast::NodeId)
                                                         -> Result<(),MethodError>
    {
        let mut duplicates = HashSet::new();
        let opt_applicable_traits = self.fcx.ccx.trait_map.get(&expr_id);
        if let Some(applicable_traits) = opt_applicable_traits {
            for &trait_did in applicable_traits {
                if duplicates.insert(trait_did) {
                    try!(self.assemble_extension_candidates_for_trait(trait_did));
                }
            }
        }
        Ok(())
    }

    fn assemble_extension_candidates_for_all_traits(&mut self) -> Result<(),MethodError> {
        let mut duplicates = HashSet::new();
        for trait_info in suggest::all_traits(self.fcx.ccx) {
            if duplicates.insert(trait_info.def_id) {
                try!(self.assemble_extension_candidates_for_trait(trait_info.def_id));
            }
        }
        Ok(())
    }

    fn assemble_extension_candidates_for_trait(&mut self,
                                               trait_def_id: ast::DefId)
                                               -> Result<(),MethodError>
    {
        debug!("assemble_extension_candidates_for_trait(trait_def_id={})",
               trait_def_id.repr(self.tcx()));

        // Check whether `trait_def_id` defines a method with suitable name:
        let trait_items =
            ty::trait_items(self.tcx(), trait_def_id);
        let matching_index =
            trait_items.iter()
                       .position(|item| item.name() == self.item_name);
        let matching_index = match matching_index {
            Some(i) => i,
            None => { return Ok(()); }
        };
        let ref item = (&*trait_items)[matching_index];

        // Check whether `trait_def_id` defines a method with suitable name:
        if !self.has_applicable_self(item) {
            debug!("method has inapplicable self");
            self.record_static_candidate(TraitSource(trait_def_id));
            return Ok(());
        }

        self.assemble_extension_candidates_for_trait_impls(trait_def_id,
                                                           item.clone(),
                                                           matching_index);

        try!(self.assemble_closure_candidates(trait_def_id,
                                              item.clone(),
                                              matching_index));

        self.assemble_projection_candidates(trait_def_id,
                                            item.clone(),
                                            matching_index);

        self.assemble_where_clause_candidates(trait_def_id,
                                              item.clone(),
                                              matching_index);

        Ok(())
    }

    fn assemble_extension_candidates_for_trait_impls(&mut self,
                                                     trait_def_id: ast::DefId,
                                                     item: ty::ImplOrTraitItem<'tcx>,
                                                     item_index: usize)
    {
        let trait_def = ty::lookup_trait_def(self.tcx(), trait_def_id);

        // FIXME(arielb1): can we use for_each_relevant_impl here?
        trait_def.for_each_impl(self.tcx(), |impl_def_id| {
            debug!("assemble_extension_candidates_for_trait_impl: trait_def_id={} impl_def_id={}",
                   trait_def_id.repr(self.tcx()),
                   impl_def_id.repr(self.tcx()));

            if !self.impl_can_possibly_match(impl_def_id) {
                return;
            }

            let (_, impl_substs) = self.impl_ty_and_substs(impl_def_id);

            debug!("impl_substs={}", impl_substs.repr(self.tcx()));

            let impl_trait_ref =
                ty::impl_trait_ref(self.tcx(), impl_def_id)
                .unwrap() // we know this is a trait impl
                .subst(self.tcx(), &impl_substs);

            debug!("impl_trait_ref={}", impl_trait_ref.repr(self.tcx()));

            // Determine the receiver type that the method itself expects.
            let xform_self_ty =
                self.xform_self_ty(&item,
                                   impl_trait_ref.self_ty(),
                                   impl_trait_ref.substs);

            debug!("xform_self_ty={}", xform_self_ty.repr(self.tcx()));

            self.extension_candidates.push(Candidate {
                xform_self_ty: xform_self_ty,
                item: item.clone(),
                kind: ExtensionImplCandidate(impl_def_id, impl_trait_ref, impl_substs, item_index)
            });
        });
    }

    fn impl_can_possibly_match(&self, impl_def_id: ast::DefId) -> bool {
        let simplified_steps = match self.opt_simplified_steps {
            Some(ref simplified_steps) => simplified_steps,
            None => { return true; }
        };

        let impl_type = ty::lookup_item_type(self.tcx(), impl_def_id);
        let impl_simplified_type =
            match fast_reject::simplify_type(self.tcx(), impl_type.ty, false) {
                Some(simplified_type) => simplified_type,
                None => { return true; }
            };

        simplified_steps.contains(&impl_simplified_type)
    }

    fn assemble_closure_candidates(&mut self,
                                   trait_def_id: ast::DefId,
                                   item: ty::ImplOrTraitItem<'tcx>,
                                   item_index: usize)
                                   -> Result<(),MethodError>
    {
        // Check if this is one of the Fn,FnMut,FnOnce traits.
        let tcx = self.tcx();
        let kind = if Some(trait_def_id) == tcx.lang_items.fn_trait() {
            ty::FnClosureKind
        } else if Some(trait_def_id) == tcx.lang_items.fn_mut_trait() {
            ty::FnMutClosureKind
        } else if Some(trait_def_id) == tcx.lang_items.fn_once_trait() {
            ty::FnOnceClosureKind
        } else {
            return Ok(());
        };

        // Check if there is an unboxed-closure self-type in the list of receivers.
        // If so, add "synthetic impls".
        let steps = self.steps.clone();
        for step in &*steps {
            let closure_def_id = match step.self_ty.sty {
                ty::ty_closure(a, _) => a,
                _ => continue,
            };

            let closure_kinds = self.fcx.inh.closure_kinds.borrow();
            let closure_kind = match closure_kinds.get(&closure_def_id) {
                Some(&k) => k,
                None => {
                    return Err(MethodError::ClosureAmbiguity(trait_def_id));
                }
            };

            // this closure doesn't implement the right kind of `Fn` trait
            if !closure_kind.extends(kind) {
                continue;
            }

            // create some substitutions for the argument/return type;
            // for the purposes of our method lookup, we only take
            // receiver type into account, so we can just substitute
            // fresh types here to use during substitution and subtyping.
            let trait_def = ty::lookup_trait_def(self.tcx(), trait_def_id);
            let substs = self.infcx().fresh_substs_for_trait(self.span,
                                                             &trait_def.generics,
                                                             step.self_ty);

            let xform_self_ty = self.xform_self_ty(&item,
                                                   step.self_ty,
                                                   &substs);
            self.inherent_candidates.push(Candidate {
                xform_self_ty: xform_self_ty,
                item: item.clone(),
                kind: ClosureCandidate(trait_def_id, item_index)
            });
        }

        Ok(())
    }

    fn assemble_projection_candidates(&mut self,
                                      trait_def_id: ast::DefId,
                                      item: ty::ImplOrTraitItem<'tcx>,
                                      item_index: usize)
    {
        debug!("assemble_projection_candidates(\
               trait_def_id={}, \
               item={}, \
               item_index={})",
               trait_def_id.repr(self.tcx()),
               item.repr(self.tcx()),
               item_index);

        for step in &*self.steps {
            debug!("assemble_projection_candidates: step={}",
                   step.repr(self.tcx()));

            let projection_trait_ref = match step.self_ty.sty {
                ty::ty_projection(ref data) => &data.trait_ref,
                _ => continue,
            };

            debug!("assemble_projection_candidates: projection_trait_ref={}",
                   projection_trait_ref.repr(self.tcx()));

            let trait_predicates = ty::lookup_predicates(self.tcx(),
                                                         projection_trait_ref.def_id);
            let bounds = trait_predicates.instantiate(self.tcx(), projection_trait_ref.substs);
            let predicates = bounds.predicates.into_vec();
            debug!("assemble_projection_candidates: predicates={}",
                   predicates.repr(self.tcx()));
            for poly_bound in
                traits::elaborate_predicates(self.tcx(), predicates)
                .filter_map(|p| p.to_opt_poly_trait_ref())
                .filter(|b| b.def_id() == trait_def_id)
            {
                let bound = self.erase_late_bound_regions(&poly_bound);

                debug!("assemble_projection_candidates: projection_trait_ref={} bound={}",
                       projection_trait_ref.repr(self.tcx()),
                       bound.repr(self.tcx()));

                if self.infcx().can_equate(&step.self_ty, &bound.self_ty()).is_ok() {
                    let xform_self_ty = self.xform_self_ty(&item,
                                                           bound.self_ty(),
                                                           bound.substs);

                    debug!("assemble_projection_candidates: bound={} xform_self_ty={}",
                           bound.repr(self.tcx()),
                           xform_self_ty.repr(self.tcx()));

                    self.extension_candidates.push(Candidate {
                        xform_self_ty: xform_self_ty,
                        item: item.clone(),
                        kind: ProjectionCandidate(trait_def_id, item_index)
                    });
                }
            }
        }
    }

    fn assemble_where_clause_candidates(&mut self,
                                        trait_def_id: ast::DefId,
                                        item: ty::ImplOrTraitItem<'tcx>,
                                        item_index: usize)
    {
        debug!("assemble_where_clause_candidates(trait_def_id={})",
               trait_def_id.repr(self.tcx()));

        let caller_predicates = self.fcx.inh.param_env.caller_bounds.clone();
        for poly_bound in traits::elaborate_predicates(self.tcx(), caller_predicates)
                          .filter_map(|p| p.to_opt_poly_trait_ref())
                          .filter(|b| b.def_id() == trait_def_id)
        {
            let bound = self.erase_late_bound_regions(&poly_bound);
            let xform_self_ty = self.xform_self_ty(&item,
                                                   bound.self_ty(),
                                                   bound.substs);

            debug!("assemble_where_clause_candidates: bound={} xform_self_ty={}",
                   bound.repr(self.tcx()),
                   xform_self_ty.repr(self.tcx()));

            self.extension_candidates.push(Candidate {
                xform_self_ty: xform_self_ty,
                item: item.clone(),
                kind: WhereClauseCandidate(poly_bound, item_index)
            });
        }
    }

    ///////////////////////////////////////////////////////////////////////////
    // THE ACTUAL SEARCH

    fn pick(mut self) -> PickResult<'tcx> {
        match self.pick_core() {
            Some(r) => return r,
            None => {}
        }

        let static_candidates = mem::replace(&mut self.static_candidates, vec![]);

        // things failed, so lets look at all traits, for diagnostic purposes now:
        self.reset();

        let span = self.span;
        let tcx = self.tcx();

        try!(self.assemble_extension_candidates_for_all_traits());

        let out_of_scope_traits = match self.pick_core() {
            Some(Ok(p)) => vec![p.item.container().id()],
            Some(Err(MethodError::Ambiguity(v))) => v.into_iter().map(|source| {
                match source {
                    TraitSource(id) => id,
                    ImplSource(impl_id) => {
                        match ty::trait_id_of_impl(tcx, impl_id) {
                            Some(id) => id,
                            None =>
                                tcx.sess.span_bug(span,
                                                  "found inherent method when looking at traits")
                        }
                    }
                }
            }).collect(),
            Some(Err(MethodError::NoMatch(_, others))) => {
                assert!(others.is_empty());
                vec![]
            }
            Some(Err(MethodError::ClosureAmbiguity(..))) => {
                // this error only occurs when assembling candidates
                tcx.sess.span_bug(span, "encountered ClosureAmbiguity from pick_core");
            }
            None => vec![],
        };

        Err(MethodError::NoMatch(static_candidates, out_of_scope_traits))
    }

    fn pick_core(&mut self) -> Option<PickResult<'tcx>> {
        let steps = self.steps.clone();

        // find the first step that works
        steps.iter().filter_map(|step| self.pick_step(step)).next()
    }

    fn pick_step(&mut self, step: &CandidateStep<'tcx>) -> Option<PickResult<'tcx>> {
        debug!("pick_step: step={}", step.repr(self.tcx()));

        if ty::type_is_error(step.self_ty) {
            return None;
        }

        match self.pick_by_value_method(step) {
            Some(result) => return Some(result),
            None => {}
        }

        self.pick_autorefd_method(step)
    }

    fn pick_by_value_method(&mut self,
                            step: &CandidateStep<'tcx>)
                            -> Option<PickResult<'tcx>>
    {
        /*!
         * For each type `T` in the step list, this attempts to find a
         * method where the (transformed) self type is exactly `T`. We
         * do however do one transformation on the adjustment: if we
         * are passing a region pointer in, we will potentially
         * *reborrow* it to a shorter lifetime. This allows us to
         * transparently pass `&mut` pointers, in particular, without
         * consuming them for their entire lifetime.
         */

        if step.unsize {
            return None;
        }

        self.pick_method(step.self_ty).map(|r| r.map(|mut pick| {
            pick.autoderefs = step.autoderefs;

            // Insert a `&*` or `&mut *` if this is a reference type:
            if let ty::ty_rptr(_, mt) = step.self_ty.sty {
                pick.autoderefs += 1;
                pick.autoref = Some(mt.mutbl);
            }

            pick
        }))
    }

    fn pick_autorefd_method(&mut self,
                            step: &CandidateStep<'tcx>)
                            -> Option<PickResult<'tcx>>
    {
        let tcx = self.tcx();

        // In general, during probing we erase regions. See
        // `impl_self_ty()` for an explanation.
        let region = tcx.mk_region(ty::ReStatic);

        // Search through mutabilities in order to find one where pick works:
        [ast::MutImmutable, ast::MutMutable].iter().filter_map(|&m| {
            let autoref_ty = ty::mk_rptr(tcx, region, ty::mt {
                ty: step.self_ty,
                mutbl: m
            });
            self.pick_method(autoref_ty).map(|r| r.map(|mut pick| {
                pick.autoderefs = step.autoderefs;
                pick.autoref = Some(m);
                pick.unsize = if step.unsize {
                    Some(step.self_ty)
                } else {
                    None
                };
                pick
            }))
        }).nth(0)
    }

    fn pick_method(&mut self, self_ty: Ty<'tcx>) -> Option<PickResult<'tcx>> {
        debug!("pick_method(self_ty={})", self.infcx().ty_to_string(self_ty));

        debug!("searching inherent candidates");
        match self.consider_candidates(self_ty, &self.inherent_candidates) {
            None => {}
            Some(pick) => {
                return Some(pick);
            }
        }

        debug!("searching extension candidates");
        self.consider_candidates(self_ty, &self.extension_candidates)
    }

    fn consider_candidates(&self,
                           self_ty: Ty<'tcx>,
                           probes: &[Candidate<'tcx>])
                           -> Option<PickResult<'tcx>> {
        let mut applicable_candidates: Vec<_> =
            probes.iter()
                  .filter(|&probe| self.consider_probe(self_ty, probe))
                  .collect();

        debug!("applicable_candidates: {}", applicable_candidates.repr(self.tcx()));

        if applicable_candidates.len() > 1 {
            match self.collapse_candidates_to_trait_pick(&applicable_candidates[..]) {
                Some(pick) => { return Some(Ok(pick)); }
                None => { }
            }
        }

        if applicable_candidates.len() > 1 {
            let sources = probes.iter().map(|p| p.to_source()).collect();
            return Some(Err(MethodError::Ambiguity(sources)));
        }

        applicable_candidates.pop().map(|probe| {
            let pick = probe.to_unadjusted_pick();
            Ok(pick)
        })
    }

    fn consider_probe(&self, self_ty: Ty<'tcx>, probe: &Candidate<'tcx>) -> bool {
        debug!("consider_probe: self_ty={} probe={}",
               self_ty.repr(self.tcx()),
               probe.repr(self.tcx()));

        self.infcx().probe(|_| {
            // First check that the self type can be related.
            match self.make_sub_ty(self_ty, probe.xform_self_ty) {
                Ok(()) => { }
                Err(_) => {
                    debug!("--> cannot relate self-types");
                    return false;
                }
            }

            // If so, impls may carry other conditions (e.g., where
            // clauses) that must be considered. Make sure that those
            // match as well (or at least may match, sometimes we
            // don't have enough information to fully evaluate).
            match probe.kind {
                InherentImplCandidate(impl_def_id, ref substs) |
                ExtensionImplCandidate(impl_def_id, _, ref substs, _) => {
                    let selcx = &mut traits::SelectionContext::new(self.infcx(), self.fcx);
                    let cause = traits::ObligationCause::misc(self.span, self.fcx.body_id);

                    // Check whether the impl imposes obligations we have to worry about.
                    let impl_bounds = ty::lookup_predicates(self.tcx(), impl_def_id);
                    let impl_bounds = impl_bounds.instantiate(self.tcx(), substs);
                    let traits::Normalized { value: impl_bounds,
                                             obligations: norm_obligations } =
                        traits::normalize(selcx, cause.clone(), &impl_bounds);

                    // Convert the bounds into obligations.
                    let obligations =
                        traits::predicates_for_generics(self.tcx(),
                                                        cause.clone(),
                                                        &impl_bounds);
                    debug!("impl_obligations={}", obligations.repr(self.tcx()));

                    // Evaluate those obligations to see if they might possibly hold.
                    obligations.all(|o| selcx.evaluate_obligation(o)) &&
                        norm_obligations.iter().all(|o| selcx.evaluate_obligation(o))
                }

                ProjectionCandidate(..) |
                ObjectCandidate(..) |
                ClosureCandidate(..) |
                WhereClauseCandidate(..) => {
                    // These have no additional conditions to check.
                    true
                }
            }
        })
    }

    /// Sometimes we get in a situation where we have multiple probes that are all impls of the
    /// same trait, but we don't know which impl to use. In this case, since in all cases the
    /// external interface of the method can be determined from the trait, it's ok not to decide.
    /// We can basically just collapse all of the probes for various impls into one where-clause
    /// probe. This will result in a pending obligation so when more type-info is available we can
    /// make the final decision.
    ///
    /// Example (`src/test/run-pass/method-two-trait-defer-resolution-1.rs`):
    ///
    /// ```
    /// trait Foo { ... }
    /// impl Foo for Vec<int> { ... }
    /// impl Foo for Vec<usize> { ... }
    /// ```
    ///
    /// Now imagine the receiver is `Vec<_>`. It doesn't really matter at this time which impl we
    /// use, so it's ok to just commit to "using the method from the trait Foo".
    fn collapse_candidates_to_trait_pick(&self,
                                         probes: &[&Candidate<'tcx>])
                                         -> Option<Pick<'tcx>> {
        // Do all probes correspond to the same trait?
        let trait_data = match probes[0].to_trait_data() {
            Some(data) => data,
            None => return None,
        };
        if probes[1..].iter().any(|p| p.to_trait_data() != Some(trait_data)) {
            return None;
        }

        // If so, just use this trait and call it a day.
        let (trait_def_id, item_num) = trait_data;
        let item = probes[0].item.clone();
        Some(Pick {
            item: item,
            kind: TraitPick(trait_def_id, item_num),
            autoderefs: 0,
            autoref: None,
            unsize: None
        })
    }

    ///////////////////////////////////////////////////////////////////////////
    // MISCELLANY

    fn make_sub_ty(&self, sub: Ty<'tcx>, sup: Ty<'tcx>) -> infer::UnitResult<'tcx> {
        self.infcx().sub_types(false, infer::Misc(DUMMY_SP), sub, sup)
    }

    fn has_applicable_self(&self, item: &ty::ImplOrTraitItem) -> bool {
        // "fast track" -- check for usage of sugar
        match *item {
            ty::ImplOrTraitItem::MethodTraitItem(ref method) =>
                match method.explicit_self {
                    ty::StaticExplicitSelfCategory => self.mode == Mode::Path,
                    ty::ByValueExplicitSelfCategory |
                    ty::ByReferenceExplicitSelfCategory(..) |
                    ty::ByBoxExplicitSelfCategory => true,
                },
            ty::ImplOrTraitItem::ConstTraitItem(..) => self.mode == Mode::Path,
            _ => false,
        }
        // FIXME -- check for types that deref to `Self`,
        // like `Rc<Self>` and so on.
        //
        // Note also that the current code will break if this type
        // includes any of the type parameters defined on the method
        // -- but this could be overcome.
    }

    fn record_static_candidate(&mut self, source: CandidateSource) {
        self.static_candidates.push(source);
    }

    fn xform_self_ty(&self,
                     item: &ty::ImplOrTraitItem<'tcx>,
                     impl_ty: Ty<'tcx>,
                     substs: &subst::Substs<'tcx>)
                     -> Ty<'tcx>
    {
        match item.as_opt_method() {
            Some(ref method) => self.xform_method_self_ty(method, impl_ty,
                                                          substs),
            None => impl_ty,
        }
    }

    fn xform_method_self_ty(&self,
                            method: &Rc<ty::Method<'tcx>>,
                            impl_ty: Ty<'tcx>,
                            substs: &subst::Substs<'tcx>)
                            -> Ty<'tcx>
    {
        debug!("xform_self_ty(impl_ty={}, self_ty={}, substs={})",
               impl_ty.repr(self.tcx()),
               method.fty.sig.0.inputs.get(0).repr(self.tcx()),
               substs.repr(self.tcx()));

        assert!(!substs.has_escaping_regions());

        // It is possible for type parameters or early-bound lifetimes
        // to appear in the signature of `self`. The substitutions we
        // are given do not include type/lifetime parameters for the
        // method yet. So create fresh variables here for those too,
        // if there are any.
        assert_eq!(substs.types.len(subst::FnSpace), 0);
        assert_eq!(substs.regions().len(subst::FnSpace), 0);

        if self.mode == Mode::Path {
            return impl_ty;
        }

        let placeholder;
        let mut substs = substs;
        if
            !method.generics.types.is_empty_in(subst::FnSpace) ||
            !method.generics.regions.is_empty_in(subst::FnSpace)
        {
            let method_types =
                self.infcx().next_ty_vars(
                    method.generics.types.len(subst::FnSpace));

            // In general, during probe we erase regions. See
            // `impl_self_ty()` for an explanation.
            let method_regions =
                method.generics.regions.get_slice(subst::FnSpace)
                .iter()
                .map(|_| ty::ReStatic)
                .collect();

            placeholder = (*substs).clone().with_method(method_types, method_regions);
            substs = &placeholder;
        }

        // Erase any late-bound regions from the method and substitute
        // in the values from the substitution.
        let xform_self_ty = method.fty.sig.input(0);
        let xform_self_ty = self.erase_late_bound_regions(&xform_self_ty);
        let xform_self_ty = xform_self_ty.subst(self.tcx(), substs);

        xform_self_ty
    }

    /// Get the type of an impl and generate substitutions with placeholders.
    fn impl_ty_and_substs(&self,
                          impl_def_id: ast::DefId)
                          -> (Ty<'tcx>, subst::Substs<'tcx>)
    {
        let impl_pty = ty::lookup_item_type(self.tcx(), impl_def_id);

        let type_vars =
            impl_pty.generics.types.map(
                |_| self.infcx().next_ty_var());

        let region_placeholders =
            impl_pty.generics.regions.map(
                |_| ty::ReStatic); // see erase_late_bound_regions() for an expl of why 'static

        let substs = subst::Substs::new(type_vars, region_placeholders);
        (impl_pty.ty, substs)
    }

    /// Replace late-bound-regions bound by `value` with `'static` using
    /// `ty::erase_late_bound_regions`.
    ///
    /// This is only a reasonable thing to do during the *probe* phase, not the *confirm* phase, of
    /// method matching. It is reasonable during the probe phase because we don't consider region
    /// relationships at all. Therefore, we can just replace all the region variables with 'static
    /// rather than creating fresh region variables. This is nice for two reasons:
    ///
    /// 1. Because the numbers of the region variables would otherwise be fairly unique to this
    ///    particular method call, it winds up creating fewer types overall, which helps for memory
    ///    usage. (Admittedly, this is a rather small effect, though measureable.)
    ///
    /// 2. It makes it easier to deal with higher-ranked trait bounds, because we can replace any
    ///    late-bound regions with 'static. Otherwise, if we were going to replace late-bound
    ///    regions with actual region variables as is proper, we'd have to ensure that the same
    ///    region got replaced with the same variable, which requires a bit more coordination
    ///    and/or tracking the substitution and
    ///    so forth.
    fn erase_late_bound_regions<T>(&self, value: &ty::Binder<T>) -> T
        where T : TypeFoldable<'tcx> + Repr<'tcx>
    {
        ty::erase_late_bound_regions(self.tcx(), value)
    }
}

fn impl_item<'tcx>(tcx: &ty::ctxt<'tcx>,
                   impl_def_id: ast::DefId,
                   item_name: ast::Name)
                   -> Option<ty::ImplOrTraitItem<'tcx>>
{
    let impl_items = tcx.impl_items.borrow();
    let impl_items = impl_items.get(&impl_def_id).unwrap();
    impl_items
        .iter()
        .map(|&did| ty::impl_or_trait_item(tcx, did.def_id()))
        .find(|item| item.name() == item_name)
}

/// Find item with name `item_name` defined in `trait_def_id` and return it,
/// along with its index (or `None`, if no such item).
fn trait_item<'tcx>(tcx: &ty::ctxt<'tcx>,
                    trait_def_id: ast::DefId,
                    item_name: ast::Name)
                    -> Option<(usize, ty::ImplOrTraitItem<'tcx>)>
{
    let trait_items = ty::trait_items(tcx, trait_def_id);
    debug!("trait_method; items: {:?}", trait_items);
    trait_items
        .iter()
        .enumerate()
        .find(|&(_, ref item)| item.name() == item_name)
        .map(|(num, ref item)| (num, (*item).clone()))
}

impl<'tcx> Candidate<'tcx> {
    fn to_unadjusted_pick(&self) -> Pick<'tcx> {
        Pick {
            item: self.item.clone(),
            kind: match self.kind {
                InherentImplCandidate(def_id, _) => {
                    InherentImplPick(def_id)
                }
                ObjectCandidate(def_id, item_num, real_index) => {
                    ObjectPick(def_id, item_num, real_index)
                }
                ExtensionImplCandidate(def_id, _, _, index) => {
                    ExtensionImplPick(def_id, index)
                }
                ClosureCandidate(trait_def_id, index) => {
                    TraitPick(trait_def_id, index)
                }
                WhereClauseCandidate(ref trait_ref, index) => {
                    // Only trait derived from where-clauses should
                    // appear here, so they should not contain any
                    // inference variables or other artifacts. This
                    // means they are safe to put into the
                    // `WhereClausePick`.
                    assert!(trait_ref.substs().types.iter().all(|&t| !ty::type_needs_infer(t)));

                    WhereClausePick((*trait_ref).clone(), index)
                }
                ProjectionCandidate(def_id, index) => {
                    TraitPick(def_id, index)
                }
            },
            autoderefs: 0,
            autoref: None,
            unsize: None
        }
    }

    fn to_source(&self) -> CandidateSource {
        match self.kind {
            InherentImplCandidate(def_id, _) => ImplSource(def_id),
            ObjectCandidate(def_id, _, _) => TraitSource(def_id),
            ExtensionImplCandidate(def_id, _, _, _) => ImplSource(def_id),
            ClosureCandidate(trait_def_id, _) => TraitSource(trait_def_id),
            WhereClauseCandidate(ref trait_ref, _) => TraitSource(trait_ref.def_id()),
            ProjectionCandidate(trait_def_id, _) => TraitSource(trait_def_id),
        }
    }

    fn to_trait_data(&self) -> Option<(ast::DefId, ItemIndex)> {
        match self.kind {
            InherentImplCandidate(..) => {
                None
            }
            ObjectCandidate(trait_def_id, item_num, _) => {
                Some((trait_def_id, item_num))
            }
            ClosureCandidate(trait_def_id, item_num) => {
                Some((trait_def_id, item_num))
            }
            ExtensionImplCandidate(_, ref trait_ref, _, item_num) => {
                Some((trait_ref.def_id, item_num))
            }
            WhereClauseCandidate(ref trait_ref, item_num) => {
                Some((trait_ref.def_id(), item_num))
            }
            ProjectionCandidate(trait_def_id, item_num) => {
                Some((trait_def_id, item_num))
            }
        }
    }
}

impl<'tcx> Repr<'tcx> for Candidate<'tcx> {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        format!("Candidate(xform_self_ty={}, kind={})",
                self.xform_self_ty.repr(tcx),
                self.kind.repr(tcx))
    }
}

impl<'tcx> Repr<'tcx> for CandidateKind<'tcx> {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        match *self {
            InherentImplCandidate(ref a, ref b) =>
                format!("InherentImplCandidate({},{})", a.repr(tcx), b.repr(tcx)),
            ObjectCandidate(a, b, c) =>
                format!("ObjectCandidate({},{},{})", a.repr(tcx), b, c),
            ExtensionImplCandidate(ref a, ref b, ref c, ref d) =>
                format!("ExtensionImplCandidate({},{},{},{})", a.repr(tcx), b.repr(tcx),
                        c.repr(tcx), d),
            ClosureCandidate(ref a, ref b) =>
                format!("ClosureCandidate({},{})", a.repr(tcx), b),
            WhereClauseCandidate(ref a, ref b) =>
                format!("WhereClauseCandidate({},{})", a.repr(tcx), b),
            ProjectionCandidate(ref a, ref b) =>
                format!("ProjectionCandidate({},{})", a.repr(tcx), b),
        }
    }
}

impl<'tcx> Repr<'tcx> for CandidateStep<'tcx> {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        format!("CandidateStep({}, autoderefs={}, unsize={})",
                self.self_ty.repr(tcx),
                self.autoderefs,
                self.unsize)
    }
}

impl<'tcx> Repr<'tcx> for PickKind<'tcx> {
    fn repr(&self, _tcx: &ty::ctxt) -> String {
        format!("{:?}", self)
    }
}

impl<'tcx> Repr<'tcx> for Pick<'tcx> {
    fn repr(&self, tcx: &ty::ctxt<'tcx>) -> String {
        format!("Pick(item={}, autoderefs={},
                 autoref={}, unsize={}, kind={:?})",
                self.item.repr(tcx),
                self.autoderefs,
                self.autoref.repr(tcx),
                self.unsize.repr(tcx),
                self.kind)
    }
}
