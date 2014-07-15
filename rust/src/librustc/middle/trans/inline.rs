// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use llvm::{AvailableExternallyLinkage, SetLinkage};
use metadata::csearch;
use middle::astencode;
use middle::trans::base::{push_ctxt, trans_item, get_item_val, trans_fn};
use middle::trans::common::*;
use middle::ty;

use syntax::ast;
use syntax::ast_util::local_def;
use syntax::ast_util;

pub fn maybe_instantiate_inline(ccx: &CrateContext, fn_id: ast::DefId)
    -> ast::DefId {
    let _icx = push_ctxt("maybe_instantiate_inline");
    match ccx.external.borrow().find(&fn_id) {
        Some(&Some(node_id)) => {
            // Already inline
            debug!("maybe_instantiate_inline({}): already inline as node id {}",
                   ty::item_path_str(ccx.tcx(), fn_id), node_id);
            return local_def(node_id);
        }
        Some(&None) => {
            return fn_id; // Not inlinable
        }
        None => {
            // Not seen yet
        }
    }

    let csearch_result =
        csearch::maybe_get_item_ast(
            ccx.tcx(), fn_id,
            |a,b,c,d| astencode::decode_inlined_item(a, b, c, d));
    return match csearch_result {
        csearch::not_found => {
            ccx.external.borrow_mut().insert(fn_id, None);
            fn_id
        }
        csearch::found(ast::IIItem(item)) => {
            ccx.external.borrow_mut().insert(fn_id, Some(item.id));
            ccx.external_srcs.borrow_mut().insert(item.id, fn_id);

            ccx.stats.n_inlines.set(ccx.stats.n_inlines.get() + 1);
            trans_item(ccx, &*item);

            // We're bringing an external global into this crate, but we don't
            // want to create two copies of the global. If we do this, then if
            // you take the address of the global in two separate crates you get
            // two different addresses. This is bad for things like conditions,
            // but it could possibly have other adverse side effects. We still
            // want to achieve the optimizations related to this global,
            // however, so we use the available_externally linkage which llvm
            // provides
            match item.node {
                ast::ItemStatic(_, mutbl, _) => {
                    let g = get_item_val(ccx, item.id);
                    // see the comment in get_item_val() as to why this check is
                    // performed here.
                    if ast_util::static_has_significant_address(
                            mutbl,
                            item.attrs.as_slice()) {
                        SetLinkage(g, AvailableExternallyLinkage);
                    }
                }
                _ => {}
            }

            local_def(item.id)
        }
        csearch::found(ast::IIForeign(item)) => {
            ccx.external.borrow_mut().insert(fn_id, Some(item.id));
            ccx.external_srcs.borrow_mut().insert(item.id, fn_id);
            local_def(item.id)
        }
        csearch::found_parent(parent_id, ast::IIItem(item)) => {
            ccx.external.borrow_mut().insert(parent_id, Some(item.id));
            ccx.external_srcs.borrow_mut().insert(item.id, parent_id);

          let mut my_id = 0;
          match item.node {
            ast::ItemEnum(_, _) => {
              let vs_here = ty::enum_variants(ccx.tcx(), local_def(item.id));
              let vs_there = ty::enum_variants(ccx.tcx(), parent_id);
              for (here, there) in vs_here.iter().zip(vs_there.iter()) {
                  if there.id == fn_id { my_id = here.id.node; }
                  ccx.external.borrow_mut().insert(there.id, Some(here.id.node));
              }
            }
            ast::ItemStruct(ref struct_def, _) => {
              match struct_def.ctor_id {
                None => {}
                Some(ctor_id) => {
                    ccx.external.borrow_mut().insert(fn_id, Some(ctor_id));
                    my_id = ctor_id;
                }
              }
            }
            _ => ccx.sess().bug("maybe_instantiate_inline: item has a \
                                 non-enum, non-struct parent")
          }
          trans_item(ccx, &*item);
          local_def(my_id)
        }
        csearch::found_parent(_, _) => {
            ccx.sess().bug("maybe_get_item_ast returned a found_parent \
             with a non-item parent");
        }
        csearch::found(ast::IIMethod(impl_did, is_provided, mth)) => {
            ccx.external.borrow_mut().insert(fn_id, Some(mth.id));
            ccx.external_srcs.borrow_mut().insert(mth.id, fn_id);

            ccx.stats.n_inlines.set(ccx.stats.n_inlines.get() + 1);

            // If this is a default method, we can't look up the
            // impl type. But we aren't going to translate anyways, so don't.
            if is_provided { return local_def(mth.id); }

            let impl_tpt = ty::lookup_item_type(ccx.tcx(), impl_did);
            let unparameterized =
                impl_tpt.generics.types.is_empty() &&
                ast_util::method_generics(&*mth).ty_params.is_empty();

          if unparameterized {
              let llfn = get_item_val(ccx, mth.id);
              trans_fn(ccx, ast_util::method_fn_decl(&*mth),
                       ast_util::method_body(&*mth), llfn,
                       &param_substs::empty(), mth.id, []);
          }
          local_def(mth.id)
        }
    };
}
