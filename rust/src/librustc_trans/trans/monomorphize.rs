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
use session;
use llvm::ValueRef;
use llvm;
use middle::subst;
use middle::subst::Subst;
use trans::base::{set_llvm_fn_attrs, set_inline_hint};
use trans::base::{trans_enum_variant, push_ctxt, get_item_val};
use trans::base::{trans_fn, decl_internal_rust_fn};
use trans::base;
use trans::common::*;
use trans::foreign;
use middle::ty::{mod, Ty};
use util::ppaux::Repr;

use syntax::abi;
use syntax::ast;
use syntax::ast_map;
use syntax::ast_util::{local_def, PostExpansionMethod};
use syntax::attr;
use std::hash::{sip, Hash};

pub fn monomorphic_fn<'a, 'tcx>(ccx: &CrateContext<'a, 'tcx>,
                                fn_id: ast::DefId,
                                psubsts: &subst::Substs<'tcx>,
                                ref_id: Option<ast::NodeId>)
    -> (ValueRef, bool) {
    debug!("monomorphic_fn(\
            fn_id={}, \
            real_substs={}, \
            ref_id={})",
           fn_id.repr(ccx.tcx()),
           psubsts.repr(ccx.tcx()),
           ref_id);

    assert!(psubsts.types.all(|t| {
        !ty::type_needs_infer(*t) && !ty::type_has_params(*t)
    }));

    let _icx = push_ctxt("monomorphic_fn");

    let hash_id = MonoId {
        def: fn_id,
        params: psubsts.types.clone()
    };

    match ccx.monomorphized().borrow().get(&hash_id) {
        Some(&val) => {
            debug!("leaving monomorphic fn {}",
            ty::item_path_str(ccx.tcx(), fn_id));
            return (val, false);
        }
        None => ()
    }

    debug!("monomorphic_fn(\
            fn_id={}, \
            psubsts={}, \
            hash_id={})",
           fn_id.repr(ccx.tcx()),
           psubsts.repr(ccx.tcx()),
           hash_id);

    let tpt = ty::lookup_item_type(ccx.tcx(), fn_id);
    let llitem_ty = tpt.ty;

    let map_node = session::expect(
        ccx.sess(),
        ccx.tcx().map.find(fn_id.node),
        || {
            format!("while monomorphizing {}, couldn't find it in \
                     the item map (may have attempted to monomorphize \
                     an item defined in a different crate?)",
                    fn_id)
        });

    match map_node {
        ast_map::NodeForeignItem(_) => {
            if ccx.tcx().map.get_foreign_abi(fn_id.node) != abi::RustIntrinsic {
                // Foreign externs don't have to be monomorphized.
                return (get_item_val(ccx, fn_id.node), true);
            }
        }
        _ => {}
    }

    debug!("monomorphic_fn about to subst into {}", llitem_ty.repr(ccx.tcx()));
    let mono_ty = llitem_ty.subst(ccx.tcx(), psubsts);

    ccx.stats().n_monos.set(ccx.stats().n_monos.get() + 1);

    let depth;
    {
        let mut monomorphizing = ccx.monomorphizing().borrow_mut();
        depth = match monomorphizing.get(&fn_id) {
            Some(&d) => d, None => 0
        };

        // Random cut-off -- code that needs to instantiate the same function
        // recursively more than thirty times can probably safely be assumed
        // to be causing an infinite expansion.
        if depth > ccx.sess().recursion_limit.get() {
            ccx.sess().span_fatal(ccx.tcx().map.span(fn_id.node),
                "reached the recursion limit during monomorphization");
        }

        monomorphizing.insert(fn_id, depth + 1);
    }

    let hash;
    let s = {
        let mut state = sip::SipState::new();
        hash_id.hash(&mut state);
        mono_ty.hash(&mut state);

        hash = format!("h{}", state.result());
        ccx.tcx().map.with_path(fn_id.node, |path| {
            exported_name(path, hash.as_slice())
        })
    };

    debug!("monomorphize_fn mangled to {}", s);

    // This shouldn't need to option dance.
    let mut hash_id = Some(hash_id);
    let mk_lldecl = |abi: abi::Abi| {
        let lldecl = if abi != abi::Rust {
            foreign::decl_rust_fn_with_foreign_abi(ccx, mono_ty, s.as_slice())
        } else {
            decl_internal_rust_fn(ccx, mono_ty, s.as_slice())
        };

        ccx.monomorphized().borrow_mut().insert(hash_id.take().unwrap(), lldecl);
        lldecl
    };
    let setup_lldecl = |lldecl, attrs: &[ast::Attribute]| {
        base::update_linkage(ccx, lldecl, None, base::OriginalTranslation);
        set_llvm_fn_attrs(ccx, attrs, lldecl);

        let is_first = !ccx.available_monomorphizations().borrow().contains(&s);
        if is_first {
            ccx.available_monomorphizations().borrow_mut().insert(s.clone());
        }

        let trans_everywhere = attr::requests_inline(attrs);
        if trans_everywhere && !is_first {
            llvm::SetLinkage(lldecl, llvm::AvailableExternallyLinkage);
        }

        // If `true`, then `lldecl` should be given a function body.
        // Otherwise, it should be left as a declaration of an external
        // function, with no definition in the current compilation unit.
        trans_everywhere || is_first
    };

    let lldecl = match map_node {
        ast_map::NodeItem(i) => {
            match *i {
              ast::Item {
                  node: ast::ItemFn(ref decl, _, abi, _, ref body),
                  ..
              } => {
                  let d = mk_lldecl(abi);
                  let needs_body = setup_lldecl(d, i.attrs.as_slice());
                  if needs_body {
                      if abi != abi::Rust {
                          foreign::trans_rust_fn_with_foreign_abi(
                              ccx, &**decl, &**body, &[], d, psubsts, fn_id.node,
                              Some(hash.as_slice()));
                      } else {
                          trans_fn(ccx, &**decl, &**body, d, psubsts, fn_id.node, &[]);
                      }
                  }

                  d
              }
              _ => {
                ccx.sess().bug("Can't monomorphize this kind of item")
              }
            }
        }
        ast_map::NodeVariant(v) => {
            let parent = ccx.tcx().map.get_parent(fn_id.node);
            let tvs = ty::enum_variants(ccx.tcx(), local_def(parent));
            let this_tv = tvs.iter().find(|tv| { tv.id.node == fn_id.node}).unwrap();
            let d = mk_lldecl(abi::Rust);
            set_inline_hint(d);
            match v.node.kind {
                ast::TupleVariantKind(ref args) => {
                    trans_enum_variant(ccx,
                                       parent,
                                       &*v,
                                       args.as_slice(),
                                       this_tv.disr_val,
                                       psubsts,
                                       d);
                }
                ast::StructVariantKind(_) =>
                    ccx.sess().bug("can't monomorphize struct variants"),
            }
            d
        }
        ast_map::NodeImplItem(ii) => {
            match *ii {
                ast::MethodImplItem(ref mth) => {
                    let d = mk_lldecl(abi::Rust);
                    let needs_body = setup_lldecl(d, mth.attrs.as_slice());
                    if needs_body {
                        trans_fn(ccx,
                                 mth.pe_fn_decl(),
                                 mth.pe_body(),
                                 d,
                                 psubsts,
                                 mth.id,
                                 &[]);
                    }
                    d
                }
                ast::TypeImplItem(_) => {
                    ccx.sess().bug("can't monomorphize an associated type")
                }
            }
        }
        ast_map::NodeTraitItem(method) => {
            match *method {
                ast::ProvidedMethod(ref mth) => {
                    let d = mk_lldecl(abi::Rust);
                    let needs_body = setup_lldecl(d, mth.attrs.as_slice());
                    if needs_body {
                        trans_fn(ccx, mth.pe_fn_decl(), mth.pe_body(), d,
                                 psubsts, mth.id, &[]);
                    }
                    d
                }
                _ => {
                    ccx.sess().bug(format!("can't monomorphize a {}",
                                           map_node).as_slice())
                }
            }
        }
        ast_map::NodeStructCtor(struct_def) => {
            let d = mk_lldecl(abi::Rust);
            set_inline_hint(d);
            base::trans_tuple_struct(ccx,
                                     struct_def.fields.as_slice(),
                                     struct_def.ctor_id.expect("ast-mapped tuple struct \
                                                                didn't have a ctor id"),
                                     psubsts,
                                     d);
            d
        }

        // Ugh -- but this ensures any new variants won't be forgotten
        ast_map::NodeForeignItem(..) |
        ast_map::NodeLifetime(..) |
        ast_map::NodeExpr(..) |
        ast_map::NodeStmt(..) |
        ast_map::NodeArg(..) |
        ast_map::NodeBlock(..) |
        ast_map::NodePat(..) |
        ast_map::NodeLocal(..) => {
            ccx.sess().bug(format!("can't monomorphize a {}",
                                   map_node).as_slice())
        }
    };

    ccx.monomorphizing().borrow_mut().insert(fn_id, depth);

    debug!("leaving monomorphic fn {}", ty::item_path_str(ccx.tcx(), fn_id));
    (lldecl, true)
}

#[deriving(PartialEq, Eq, Hash, Show)]
pub struct MonoId<'tcx> {
    pub def: ast::DefId,
    pub params: subst::VecPerParamSpace<Ty<'tcx>>
}
