// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/**

# Standalone Tests for the Inference Module

Note: This module is only compiled when doing unit testing.

*/

use core::prelude::*;

use driver::diagnostic;
use driver::driver::{optgroups, build_session_options, build_session};
use driver::driver::{str_input, build_configuration};
use middle::lang_items::{LanguageItems, language_items};
use middle::ty::{FnTyBase, FnMeta, FnSig};
use util::ppaux::ty_to_str;

use std::getopts::groups::{optopt, optmulti, optflag, optflagopt, getopts};
use std::getopts::groups;
use std::getopts::{opt_present};
use std::getopts;
use std::getopts;
use std::oldmap::HashMap;
use syntax::codemap::dummy_sp;
use syntax::parse::parse_crate_from_source_str;
use syntax::{ast, attr, parse};

struct Env {
    crate: @ast::crate,
    tcx: ty::ctxt,
    infcx: infer::infer_ctxt,
    err_messages: @DVec<~str>
}

struct RH {
    id: ast::node_id,
    sub: &[RH]
}

const EMPTY_SOURCE_STR: &str = "/* Hello, world! */";

fn setup_env(test_name: &str, source_string: &str) -> Env {
    let messages = @DVec();
    let matches = getopts(~[~"-Z", ~"verbose"], optgroups()).get();
    let diag = diagnostic::collect(messages);
    let sessopts = build_session_options(~"rustc", &matches, diag);
    let sess = build_session(sessopts, diag);
    let cfg = build_configuration(sess, ~"whatever", str_input(~""));
    let dm = HashMap();
    let amap = HashMap();
    let freevars = HashMap();
    let region_paramd_items = HashMap();
    let region_map = HashMap();
    let lang_items = LanguageItems::new();

    let parse_sess = parse::new_parse_sess(None);
    let crate = parse_crate_from_source_str(
        test_name.to_str(), @source_string.to_str(),
        cfg, parse_sess);

    let tcx = ty::mk_ctxt(sess, dm, amap, freevars, region_map,
                          region_paramd_items, lang_items, crate);

    let infcx = infer::new_infer_ctxt(tcx);

    return Env {crate: crate,
                tcx: tcx,
                infcx: infcx,
                err_messages: messages};
}

pub impl Env {
    fn create_region_hierarchy(&self, rh: &RH) {
        for rh.sub.each |child_rh| {
            self.create_region_hierarchy(child_rh);
            self.tcx.region_map.insert(child_rh.id, rh.id);
        }
    }

    fn create_simple_region_hierarchy(&self) {
        // creates a region hierarchy where 1 is root, 10 and 11 are
        // children of 1, etc
        self.create_region_hierarchy(
            &RH {id: 1,
                 sub: &[RH {id: 10,
                            sub: &[]},
                        RH {id: 11,
                            sub: &[]}]});
    }

    fn lookup_item(&self, names: &[~str]) -> ast::node_id {
        return match search_mod(self, &self.crate.node.module, 0, names) {
            Some(id) => id,
            None => {
                fail!(fmt!("No item found: `%s`", str::connect(names, "::")));
            }
        };

        fn search_mod(self: &Env,
                      m: &ast::_mod,
                      idx: uint,
                      names: &[~str]) -> Option<ast::node_id> {
            fail_unless!(idx < names.len());
            for m.items.each |item| {
                if self.tcx.sess.str_of(item.ident) == names[idx] {
                    return search(self, *item, idx+1, names);
                }
            }
            return None;
        }

        fn search(self: &Env,
                  it: @ast::item,
                  idx: uint,
                  names: &[~str]) -> Option<ast::node_id> {
            if idx == names.len() {
                return Some(it.id);
            }

            return match it.node {
                ast::item_const(*) | ast::item_fn(*) |
                ast::item_foreign_mod(*) | ast::item_ty(*) => {
                    None
                }

                ast::item_enum(*) | ast::item_struct(*) |
                ast::item_trait(*) | ast::item_impl(*) |
                ast::item_mac(*) => {
                    None
                }

                ast::item_mod(ref m) => {
                    search_mod(self, m, idx, names)
                }
            };
        }
    }

    fn is_subtype(&self, a: ty::t, b: ty::t) -> bool {
        match infer::can_mk_subty(self.infcx, a, b) {
            Ok(_) => true,
            Err(_) => false
        }
    }

    fn assert_subtype(&self, a: ty::t, b: ty::t) {
        if !self.is_subtype(a, b) {
            fail!(fmt!("%s is not a subtype of %s, but it should be",
                      self.ty_to_str(a),
                      self.ty_to_str(b)));
        }
    }

    fn assert_not_subtype(&self, a: ty::t, b: ty::t) {
        if self.is_subtype(a, b) {
            fail!(fmt!("%s is a subtype of %s, but it shouldn't be",
                      self.ty_to_str(a),
                      self.ty_to_str(b)));
        }
    }

    fn assert_strict_subtype(&self, a: ty::t, b: ty::t) {
        self.assert_subtype(a, b);
        self.assert_not_subtype(b, a);
    }

    fn assert_eq(&self, a: ty::t, b: ty::t) {
        self.assert_subtype(a, b);
        self.assert_subtype(b, a);
    }

    fn ty_to_str(&self, a: ty::t) -> ~str {
        ty_to_str(self.tcx, a)
    }

    fn t_fn(&self, input_tys: &[ty::t], output_ty: ty::t) -> ty::t {
        let inputs = input_tys.map(|t| {mode: ast::expl(ast::by_copy),
                                        ty: *t});
        ty::mk_fn(self.tcx, FnTyBase {
            meta: FnMeta {purity: ast::impure_fn,
                          proto: ast::ProtoBare,
                          onceness: ast::Many,
                          region: ty::re_static,
                          bounds: @~[]},
            sig: FnSig {inputs: inputs,
                        output: output_ty}
        })
    }

    fn t_int(&self) -> ty::t {
        ty::mk_int(self.tcx)
    }

    fn t_rptr_bound(&self, id: uint) -> ty::t {
        ty::mk_imm_rptr(self.tcx, ty::re_bound(ty::br_anon(id)), self.t_int())
    }

    fn t_rptr_scope(&self, id: ast::node_id) -> ty::t {
        ty::mk_imm_rptr(self.tcx, ty::re_scope(id), self.t_int())
    }

    fn t_rptr_free(&self, nid: ast::node_id, id: uint) -> ty::t {
        ty::mk_imm_rptr(self.tcx, ty::re_free(nid, ty::br_anon(id)),
                        self.t_int())
    }

    fn t_rptr_static(&self) -> ty::t {
        ty::mk_imm_rptr(self.tcx, ty::re_static, self.t_int())
    }

    fn lub() -> Lub { Lub(self.infcx.combine_fields(true, dummy_sp())) }

    fn glb() -> Glb { Glb(self.infcx.combine_fields(true, dummy_sp())) }

    fn resolve_regions(exp_count: uint) {
        debug!("resolve_regions(%u)", exp_count);

        self.infcx.resolve_regions();
        if self.err_messages.len() != exp_count {
            for self.err_messages.each |msg| {
                debug!("Error encountered: %s", *msg);
            }
            fmt!("Resolving regions encountered %u errors but expected %u!",
                 self.err_messages.len(),
                 exp_count);
        }
    }

    /// Checks that `LUB(t1,t2) == t_lub`
    fn check_lub(&self, t1: ty::t, t2: ty::t, t_lub: ty::t) {
        match self.lub().tys(t1, t2) {
            Err(e) => {
                fail!(fmt!("Unexpected error computing LUB: %?", e))
            }
            Ok(t) => {
                self.assert_eq(t, t_lub);

                // sanity check for good measure:
                self.assert_subtype(t1, t);
                self.assert_subtype(t2, t);

                self.resolve_regions(0);
            }
        }
    }

    /// Checks that `GLB(t1,t2) == t_glb`
    fn check_glb(&self, t1: ty::t, t2: ty::t, t_glb: ty::t) {
        debug!("check_glb(t1=%s, t2=%s, t_glb=%s)",
               self.ty_to_str(t1),
               self.ty_to_str(t2),
               self.ty_to_str(t_glb));
        match self.glb().tys(t1, t2) {
            Err(e) => {
                fail!(fmt!("Unexpected error computing LUB: %?", e))
            }
            Ok(t) => {
                self.assert_eq(t, t_glb);

                // sanity check for good measure:
                self.assert_subtype(t, t1);
                self.assert_subtype(t, t2);

                self.resolve_regions(0);
            }
        }
    }

    /// Checks that `LUB(t1,t2)` is undefined
    fn check_no_lub(&self, t1: ty::t, t2: ty::t) {
        match self.lub().tys(t1, t2) {
            Err(_) => {}
            Ok(t) => {
                fail!(fmt!("Unexpected success computing LUB: %?",
                          self.ty_to_str(t)))
            }
        }
    }

    /// Checks that `GLB(t1,t2)` is undefined
    fn check_no_glb(&self, t1: ty::t, t2: ty::t) {
        match self.glb().tys(t1, t2) {
            Err(_) => {}
            Ok(t) => {
                fail!(fmt!("Unexpected success computing GLB: %?",
                          self.ty_to_str(t)))
            }
        }
    }
}

#[test]
fn contravariant_region_ptr() {
    let env = setup_env("contravariant_region_ptr", EMPTY_SOURCE_STR);
    env.create_simple_region_hierarchy();
    let t_rptr1 = env.t_rptr_scope(1);
    let t_rptr10 = env.t_rptr_scope(10);
    env.assert_eq(t_rptr1, t_rptr1);
    env.assert_eq(t_rptr10, t_rptr10);
    env.assert_strict_subtype(t_rptr1, t_rptr10);
}

#[test]
fn lub_bound_bound() {
    let env = setup_env("lub_bound_bound", EMPTY_SOURCE_STR);
    let t_rptr_bound1 = env.t_rptr_bound(1);
    let t_rptr_bound2 = env.t_rptr_bound(2);
    env.check_lub(env.t_fn([t_rptr_bound1], env.t_int()),
                  env.t_fn([t_rptr_bound2], env.t_int()),
                  env.t_fn([t_rptr_bound1], env.t_int()));
}

#[test]
fn lub_bound_free() {
    let env = setup_env("lub_bound_free", EMPTY_SOURCE_STR);
    let t_rptr_bound1 = env.t_rptr_bound(1);
    let t_rptr_free1 = env.t_rptr_free(0, 1);
    env.check_lub(env.t_fn([t_rptr_bound1], env.t_int()),
                  env.t_fn([t_rptr_free1], env.t_int()),
                  env.t_fn([t_rptr_free1], env.t_int()));
}

#[test]
fn lub_bound_static() {
    let env = setup_env("lub_bound_static", EMPTY_SOURCE_STR);
    let t_rptr_bound1 = env.t_rptr_bound(1);
    let t_rptr_static = env.t_rptr_static();
    env.check_lub(env.t_fn([t_rptr_bound1], env.t_int()),
                  env.t_fn([t_rptr_static], env.t_int()),
                  env.t_fn([t_rptr_static], env.t_int()));
}

#[test]
fn lub_bound_bound_inverse_order() {
    let env = setup_env("lub_bound_bound_inverse_order", EMPTY_SOURCE_STR);
    let t_rptr_bound1 = env.t_rptr_bound(1);
    let t_rptr_bound2 = env.t_rptr_bound(2);
    env.check_lub(env.t_fn([t_rptr_bound1, t_rptr_bound2], t_rptr_bound1),
                  env.t_fn([t_rptr_bound2, t_rptr_bound1], t_rptr_bound1),
                  env.t_fn([t_rptr_bound1, t_rptr_bound1], t_rptr_bound1));
}

#[test]
fn lub_free_free() {
    let env = setup_env("lub_free_free", EMPTY_SOURCE_STR);
    let t_rptr_free1 = env.t_rptr_free(0, 1);
    let t_rptr_free2 = env.t_rptr_free(0, 2);
    let t_rptr_static = env.t_rptr_static();
    env.check_lub(env.t_fn([t_rptr_free1], env.t_int()),
                  env.t_fn([t_rptr_free2], env.t_int()),
                  env.t_fn([t_rptr_static], env.t_int()));
}

#[test]
fn lub_returning_scope() {
    let env = setup_env("lub_returning_scope", EMPTY_SOURCE_STR);
    let t_rptr_scope10 = env.t_rptr_scope(10);
    let t_rptr_scope11 = env.t_rptr_scope(11);
    env.check_no_lub(env.t_fn([], t_rptr_scope10),
                     env.t_fn([], t_rptr_scope11));
}

#[test]
fn glb_free_free_with_common_scope() {
    let env = setup_env("glb_free_free", EMPTY_SOURCE_STR);
    let t_rptr_free1 = env.t_rptr_free(0, 1);
    let t_rptr_free2 = env.t_rptr_free(0, 2);
    let t_rptr_scope = env.t_rptr_scope(0);
    env.check_glb(env.t_fn([t_rptr_free1], env.t_int()),
                  env.t_fn([t_rptr_free2], env.t_int()),
                  env.t_fn([t_rptr_scope], env.t_int()));
}

#[test]
fn glb_bound_bound() {
    let env = setup_env("glb_bound_bound", EMPTY_SOURCE_STR);
    let t_rptr_bound1 = env.t_rptr_bound(1);
    let t_rptr_bound2 = env.t_rptr_bound(2);
    env.check_glb(env.t_fn([t_rptr_bound1], env.t_int()),
                  env.t_fn([t_rptr_bound2], env.t_int()),
                  env.t_fn([t_rptr_bound1], env.t_int()));
}

#[test]
fn glb_bound_free() {
    let env = setup_env("glb_bound_free", EMPTY_SOURCE_STR);
    let t_rptr_bound1 = env.t_rptr_bound(1);
    let t_rptr_free1 = env.t_rptr_free(0, 1);
    env.check_glb(env.t_fn([t_rptr_bound1], env.t_int()),
                  env.t_fn([t_rptr_free1], env.t_int()),
                  env.t_fn([t_rptr_bound1], env.t_int()));
}

#[test]
fn glb_bound_static() {
    let env = setup_env("glb_bound_static", EMPTY_SOURCE_STR);
    let t_rptr_bound1 = env.t_rptr_bound(1);
    let t_rptr_static = env.t_rptr_static();
    env.check_glb(env.t_fn([t_rptr_bound1], env.t_int()),
                  env.t_fn([t_rptr_static], env.t_int()),
                  env.t_fn([t_rptr_bound1], env.t_int()));
}
