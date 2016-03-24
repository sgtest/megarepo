// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use back::link::exported_name;
use llvm::ValueRef;
use llvm;
use middle::def_id::DefId;
use middle::infer::normalize_associated_type;
use middle::subst;
use middle::subst::{Subst, Substs};
use middle::ty::fold::{TypeFolder, TypeFoldable};
use trans::attributes;
use trans::base::{push_ctxt};
use trans::base::trans_fn;
use trans::base;
use trans::common::*;
use trans::declare;
use middle::ty::{self, Ty, TyCtxt};
use trans::Disr;
use rustc::front::map as hir_map;
use rustc::util::ppaux;

use rustc_front::hir;

use syntax::attr;
use syntax::errors;

use std::fmt;
use std::hash::{Hasher, Hash, SipHasher};

pub fn monomorphic_fn<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                                fn_id: DefId,
                                psubsts: &'tcx subst::Substs<'tcx>)
                                -> (ValueRef, Ty<'tcx>) {
    debug!("monomorphic_fn(fn_id={:?}, real_substs={:?})", fn_id, psubsts);

    assert!(!psubsts.types.needs_infer() && !psubsts.types.has_param_types());

    let _icx = push_ctxt("monomorphic_fn");

    let instance = Instance::new(fn_id, psubsts);

    let item_ty = ccx.tcx().lookup_item_type(fn_id).ty;

    debug!("monomorphic_fn about to subst into {:?}", item_ty);
    let mono_ty = apply_param_substs(ccx.tcx(), psubsts, &item_ty);
    debug!("mono_ty = {:?} (post-substitution)", mono_ty);

    match ccx.instances().borrow().get(&instance) {
        Some(&val) => {
            debug!("leaving monomorphic fn {:?}", instance);
            return (val, mono_ty);
        }
        None => ()
    }

    debug!("monomorphic_fn({:?})", instance);

    ccx.stats().n_monos.set(ccx.stats().n_monos.get() + 1);

    let depth;
    {
        let mut monomorphizing = ccx.monomorphizing().borrow_mut();
        depth = match monomorphizing.get(&fn_id) {
            Some(&d) => d, None => 0
        };

        debug!("monomorphic_fn: depth for fn_id={:?} is {:?}", fn_id, depth+1);

        // Random cut-off -- code that needs to instantiate the same function
        // recursively more than thirty times can probably safely be assumed
        // to be causing an infinite expansion.
        if depth > ccx.sess().recursion_limit.get() {
            let error = format!("reached the recursion limit while instantiating `{}`",
                                instance);
            if let Some(id) = ccx.tcx().map.as_local_node_id(fn_id) {
                ccx.sess().span_fatal(ccx.tcx().map.span(id), &error);
            } else {
                ccx.sess().fatal(&error);
            }
        }

        monomorphizing.insert(fn_id, depth + 1);
    }

    let hash;
    let s = {
        let mut state = SipHasher::new();
        instance.hash(&mut state);
        mono_ty.hash(&mut state);

        hash = format!("h{}", state.finish());
        let path = ccx.tcx().map.def_path(fn_id);
        exported_name(path, &hash[..])
    };

    debug!("monomorphize_fn mangled to {}", s);
    assert!(declare::get_defined_value(ccx, &s).is_none());

    // FIXME(nagisa): perhaps needs a more fine grained selection?
    let lldecl = declare::define_internal_fn(ccx, &s, mono_ty);
    // FIXME(eddyb) Doubt all extern fn should allow unwinding.
    attributes::unwind(lldecl, true);

    ccx.instances().borrow_mut().insert(instance, lldecl);

    // we can only monomorphize things in this crate (or inlined into it)
    let fn_node_id = ccx.tcx().map.as_local_node_id(fn_id).unwrap();
    let map_node = errors::expect(
        ccx.sess().diagnostic(),
        ccx.tcx().map.find(fn_node_id),
        || {
            format!("while instantiating `{}`, couldn't find it in \
                     the item map (may have attempted to monomorphize \
                     an item defined in a different crate?)",
                    instance)
        });
    match map_node {
        hir_map::NodeItem(&hir::Item {
            ref attrs, node: hir::ItemFn(ref decl, _, _, _, _, ref body), ..
        }) |
        hir_map::NodeTraitItem(&hir::TraitItem {
            ref attrs, node: hir::MethodTraitItem(
                hir::MethodSig { ref decl, .. }, Some(ref body)), ..
        }) |
        hir_map::NodeImplItem(&hir::ImplItem {
            ref attrs, node: hir::ImplItemKind::Method(
                hir::MethodSig { ref decl, .. }, ref body), ..
        }) => {
            base::update_linkage(ccx, lldecl, None, base::OriginalTranslation);
            attributes::from_fn_attrs(ccx, attrs, lldecl);

            let is_first = !ccx.available_monomorphizations().borrow().contains(&s);
            if is_first {
                ccx.available_monomorphizations().borrow_mut().insert(s.clone());
            }

            let trans_everywhere = attr::requests_inline(attrs);
            if trans_everywhere && !is_first {
                llvm::SetLinkage(lldecl, llvm::AvailableExternallyLinkage);
            }

            if trans_everywhere || is_first {
                trans_fn(ccx, decl, body, lldecl, psubsts, fn_node_id);
            }
        }

        hir_map::NodeVariant(_) | hir_map::NodeStructCtor(_) => {
            let disr = match map_node {
                hir_map::NodeVariant(_) => {
                    Disr::from(inlined_variant_def(ccx, fn_node_id).disr_val)
                }
                hir_map::NodeStructCtor(_) => Disr(0),
                _ => unreachable!()
            };
            attributes::inline(lldecl, attributes::InlineAttr::Hint);
            base::trans_ctor_shim(ccx, fn_node_id, disr, psubsts, lldecl);
        }

        _ => unreachable!("can't monomorphize a {:?}", map_node)
    };

    ccx.monomorphizing().borrow_mut().insert(fn_id, depth);

    debug!("leaving monomorphic fn {}", ccx.tcx().item_path_str(fn_id));
    (lldecl, mono_ty)
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Instance<'tcx> {
    pub def: DefId,
    pub substs: &'tcx Substs<'tcx>,
}

impl<'tcx> fmt::Display for Instance<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        ppaux::parameterized(f, &self.substs, self.def, ppaux::Ns::Value, &[],
                             |tcx| tcx.lookup_item_type(self.def).generics)
    }
}

impl<'tcx> Instance<'tcx> {
    pub fn new(def_id: DefId, substs: &'tcx Substs<'tcx>)
               -> Instance<'tcx> {
        assert!(substs.regions.iter().all(|&r| r == ty::ReStatic));
        Instance { def: def_id, substs: substs }
    }
    pub fn mono(tcx: &TyCtxt<'tcx>, def_id: DefId) -> Instance<'tcx> {
        Instance::new(def_id, &tcx.mk_substs(Substs::empty()))
    }
}

/// Monomorphizes a type from the AST by first applying the in-scope
/// substitutions and then normalizing any associated types.
pub fn apply_param_substs<'tcx,T>(tcx: &TyCtxt<'tcx>,
                                  param_substs: &Substs<'tcx>,
                                  value: &T)
                                  -> T
    where T : TypeFoldable<'tcx>
{
    let substituted = value.subst(tcx, param_substs);
    normalize_associated_type(tcx, &substituted)
}


/// Returns the normalized type of a struct field
pub fn field_ty<'tcx>(tcx: &TyCtxt<'tcx>,
                      param_substs: &Substs<'tcx>,
                      f: ty::FieldDef<'tcx>)
                      -> Ty<'tcx>
{
    normalize_associated_type(tcx, &f.ty(tcx, param_substs))
}
