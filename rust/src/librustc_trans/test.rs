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

# Standalone Tests for the Inference Module

*/

use driver::diagnostic;
use driver::diagnostic::Emitter;
use driver::driver;
use middle::lang_items;
use middle::region;
use middle::resolve;
use middle::resolve_lifetime;
use middle::stability;
use middle::subst;
use middle::subst::Subst;
use middle::ty;
use middle::typeck::infer::combine::Combine;
use middle::typeck::infer;
use middle::typeck::infer::lub::Lub;
use middle::typeck::infer::glb::Glb;
use session::{mod,config};
use syntax::{abi, ast, ast_map, ast_util};
use syntax::codemap;
use syntax::codemap::{Span, CodeMap, DUMMY_SP};
use syntax::diagnostic::{Level, RenderSpan, Bug, Fatal, Error, Warning, Note, Help};
use syntax::parse::token;
use util::ppaux::{ty_to_string, Repr, UserString};

use arena::TypedArena;

struct Env<'a, 'tcx: 'a> {
    infcx: &'a infer::InferCtxt<'a, 'tcx>,
}

struct RH<'a> {
    id: ast::NodeId,
    sub: &'a [RH<'a>]
}

static EMPTY_SOURCE_STR: &'static str = "#![no_std]";

struct ExpectErrorEmitter {
    messages: Vec<String>
}

fn remove_message(e: &mut ExpectErrorEmitter, msg: &str, lvl: Level) {
    match lvl {
        Bug | Fatal | Error => { }
        Warning | Note | Help => { return; }
    }

    debug!("Error: {}", msg);
    match e.messages.iter().position(|m| msg.contains(m.as_slice())) {
        Some(i) => {
            e.messages.remove(i);
        }
        None => {
            panic!("Unexpected error: {} Expected: {}",
                  msg, e.messages);
        }
    }
}

impl Emitter for ExpectErrorEmitter {
    fn emit(&mut self,
            _cmsp: Option<(&codemap::CodeMap, Span)>,
            msg: &str,
            _: Option<&str>,
            lvl: Level)
    {
        remove_message(self, msg, lvl);
    }

    fn custom_emit(&mut self,
                   _cm: &codemap::CodeMap,
                   _sp: RenderSpan,
                   msg: &str,
                   lvl: Level)
    {
        remove_message(self, msg, lvl);
    }
}

fn errors(msgs: &[&str]) -> (Box<Emitter+Send>, uint) {
    let v = msgs.iter().map(|m| m.to_string()).collect();
    (box ExpectErrorEmitter { messages: v } as Box<Emitter+Send>, msgs.len())
}

fn test_env(source_string: &str,
            (emitter, expected_err_count): (Box<Emitter+Send>, uint),
            body: |Env|) {
    let mut options =
        config::basic_options();
    options.debugging_opts |= config::VERBOSE;
    let codemap =
        CodeMap::new();
    let diagnostic_handler =
        diagnostic::mk_handler(emitter);
    let span_diagnostic_handler =
        diagnostic::mk_span_handler(diagnostic_handler, codemap);

    let sess = session::build_session_(options, None, span_diagnostic_handler);
    let krate_config = Vec::new();
    let input = driver::StrInput(source_string.to_string());
    let krate = driver::phase_1_parse_input(&sess, krate_config, &input);
    let krate = driver::phase_2_configure_and_expand(&sess, krate, "test", None)
                    .expect("phase 2 aborted");

    let mut forest = ast_map::Forest::new(krate);
    let ast_map = driver::assign_node_ids_and_map(&sess, &mut forest);
    let krate = ast_map.krate();

    // run just enough stuff to build a tcx:
    let lang_items = lang_items::collect_language_items(krate, &sess);
    let resolve::CrateMap { def_map, freevars, capture_mode_map, .. } =
        resolve::resolve_crate(&sess, &lang_items, krate);
    let named_region_map = resolve_lifetime::krate(&sess, krate, &def_map);
    let region_map = region::resolve_crate(&sess, krate);
    let stability_index = stability::Index::build(krate);
    let type_arena = TypedArena::new();
    let tcx = ty::mk_ctxt(sess,
                          &type_arena,
                          def_map,
                          named_region_map,
                          ast_map,
                          freevars,
                          capture_mode_map,
                          region_map,
                          lang_items,
                          stability_index);
    let infcx = infer::new_infer_ctxt(&tcx);
    body(Env { infcx: &infcx });
    infcx.resolve_regions_and_report_errors();
    assert_eq!(tcx.sess.err_count(), expected_err_count);
}

impl<'a, 'tcx> Env<'a, 'tcx> {
    pub fn create_region_hierarchy(&self, rh: &RH) {
        for child_rh in rh.sub.iter() {
            self.create_region_hierarchy(child_rh);
            self.infcx.tcx.region_maps.record_encl_scope(child_rh.id, rh.id);
        }
    }

    pub fn create_simple_region_hierarchy(&self) {
        // creates a region hierarchy where 1 is root, 10 and 11 are
        // children of 1, etc
        self.create_region_hierarchy(
            &RH {id: 1,
                 sub: &[RH {id: 10,
                            sub: &[]},
                        RH {id: 11,
                            sub: &[]}]});
    }

    #[allow(dead_code)] // this seems like it could be useful, even if we don't use it now
    pub fn lookup_item(&self, names: &[String]) -> ast::NodeId {
        return match search_mod(self, &self.infcx.tcx.map.krate().module, 0, names) {
            Some(id) => id,
            None => {
                panic!("no item found: `{}`", names.connect("::"));
            }
        };

        fn search_mod(this: &Env,
                      m: &ast::Mod,
                      idx: uint,
                      names: &[String])
                      -> Option<ast::NodeId> {
            assert!(idx < names.len());
            for item in m.items.iter() {
                if item.ident.user_string(this.infcx.tcx) == names[idx] {
                    return search(this, &**item, idx+1, names);
                }
            }
            return None;
        }

        fn search(this: &Env,
                  it: &ast::Item,
                  idx: uint,
                  names: &[String])
                  -> Option<ast::NodeId> {
            if idx == names.len() {
                return Some(it.id);
            }

            return match it.node {
                ast::ItemConst(..) | ast::ItemStatic(..) | ast::ItemFn(..) |
                ast::ItemForeignMod(..) | ast::ItemTy(..) => {
                    None
                }

                ast::ItemEnum(..) | ast::ItemStruct(..) |
                ast::ItemTrait(..) | ast::ItemImpl(..) |
                ast::ItemMac(..) => {
                    None
                }

                ast::ItemMod(ref m) => {
                    search_mod(this, m, idx, names)
                }
            };
        }
    }

    pub fn make_subtype(&self, a: ty::t, b: ty::t) -> bool {
        match infer::mk_subty(self.infcx, true, infer::Misc(DUMMY_SP), a, b) {
            Ok(_) => true,
            Err(ref e) => panic!("Encountered error: {}",
                                ty::type_err_to_str(self.infcx.tcx, e))
        }
    }

    pub fn is_subtype(&self, a: ty::t, b: ty::t) -> bool {
        match infer::can_mk_subty(self.infcx, a, b) {
            Ok(_) => true,
            Err(_) => false
        }
    }

    pub fn assert_subtype(&self, a: ty::t, b: ty::t) {
        if !self.is_subtype(a, b) {
            panic!("{} is not a subtype of {}, but it should be",
                  self.ty_to_string(a),
                  self.ty_to_string(b));
        }
    }

    pub fn assert_eq(&self, a: ty::t, b: ty::t) {
        self.assert_subtype(a, b);
        self.assert_subtype(b, a);
    }

    pub fn ty_to_string(&self, a: ty::t) -> String {
        ty_to_string(self.infcx.tcx, a)
    }

    pub fn t_fn(&self,
                input_tys: &[ty::t],
                output_ty: ty::t)
                -> ty::t
    {
        ty::mk_ctor_fn(self.infcx.tcx, input_tys, output_ty)
    }

    pub fn t_nil(&self) -> ty::t {
        ty::mk_nil(self.infcx.tcx)
    }

    pub fn t_pair(&self, ty1: ty::t, ty2: ty::t) -> ty::t
    {
        ty::mk_tup(self.infcx.tcx, vec![ty1, ty2])
    }

    pub fn t_closure(&self,
                     input_tys: &[ty::t],
                     output_ty: ty::t,
                     region_bound: ty::Region)
                     -> ty::t
    {
        ty::mk_closure(self.infcx.tcx, ty::ClosureTy {
            fn_style: ast::NormalFn,
            onceness: ast::Many,
            store: ty::RegionTraitStore(region_bound, ast::MutMutable),
            bounds: ty::region_existential_bound(region_bound),
            sig: ty::FnSig {
                inputs: input_tys.to_vec(),
                output: ty::FnConverging(output_ty),
                variadic: false,
            },
            abi: abi::Rust,
        })
    }

    pub fn t_param(&self, space: subst::ParamSpace, index: uint) -> ty::t {
        ty::mk_param(self.infcx.tcx, space, index, ast_util::local_def(ast::DUMMY_NODE_ID))
    }

    pub fn re_early_bound(&self,
                          space: subst::ParamSpace,
                          index: uint,
                          name: &'static str)
                          -> ty::Region
    {
        let name = token::intern(name);
        ty::ReEarlyBound(ast::DUMMY_NODE_ID, space, index, name)
    }

    pub fn re_late_bound_with_debruijn(&self, id: uint, debruijn: ty::DebruijnIndex) -> ty::Region {
        ty::ReLateBound(debruijn, ty::BrAnon(id))
    }

    pub fn t_rptr(&self, r: ty::Region) -> ty::t {
        ty::mk_imm_rptr(self.infcx.tcx, r, ty::mk_int())
    }

    pub fn t_rptr_late_bound(&self, id: uint) -> ty::t {
        ty::mk_imm_rptr(self.infcx.tcx,
                        self.re_late_bound_with_debruijn(id, ty::DebruijnIndex::new(1)),
                        ty::mk_int())
    }

    pub fn t_rptr_late_bound_with_debruijn(&self, id: uint, debruijn: ty::DebruijnIndex) -> ty::t {
        ty::mk_imm_rptr(self.infcx.tcx,
                        self.re_late_bound_with_debruijn(id, debruijn),
                        ty::mk_int())
    }

    pub fn t_rptr_scope(&self, id: ast::NodeId) -> ty::t {
        ty::mk_imm_rptr(self.infcx.tcx, ty::ReScope(id), ty::mk_int())
    }

    pub fn re_free(&self, nid: ast::NodeId, id: uint) -> ty::Region {
        ty::ReFree(ty::FreeRegion {scope_id: nid,
                                   bound_region: ty::BrAnon(id)})
    }

    pub fn t_rptr_free(&self, nid: ast::NodeId, id: uint) -> ty::t {
        ty::mk_imm_rptr(self.infcx.tcx, self.re_free(nid, id), ty::mk_int())
    }

    pub fn t_rptr_static(&self) -> ty::t {
        ty::mk_imm_rptr(self.infcx.tcx, ty::ReStatic, ty::mk_int())
    }

    pub fn dummy_type_trace(&self) -> infer::TypeTrace {
        infer::TypeTrace::dummy()
    }

    pub fn lub(&self) -> Lub<'a, 'tcx> {
        let trace = self.dummy_type_trace();
        Lub(self.infcx.combine_fields(true, trace))
    }

    pub fn glb(&self) -> Glb<'a, 'tcx> {
        let trace = self.dummy_type_trace();
        Glb(self.infcx.combine_fields(true, trace))
    }

    pub fn make_lub_ty(&self, t1: ty::t, t2: ty::t) -> ty::t {
        match self.lub().tys(t1, t2) {
            Ok(t) => t,
            Err(ref e) => panic!("unexpected error computing LUB: {}",
                                ty::type_err_to_str(self.infcx.tcx, e))
        }
    }

    /// Checks that `LUB(t1,t2) == t_lub`
    pub fn check_lub(&self, t1: ty::t, t2: ty::t, t_lub: ty::t) {
        match self.lub().tys(t1, t2) {
            Ok(t) => {
                self.assert_eq(t, t_lub);
            }
            Err(ref e) => {
                panic!("unexpected error in LUB: {}",
                      ty::type_err_to_str(self.infcx.tcx, e))
            }
        }
    }

    /// Checks that `GLB(t1,t2) == t_glb`
    pub fn check_glb(&self, t1: ty::t, t2: ty::t, t_glb: ty::t) {
        debug!("check_glb(t1={}, t2={}, t_glb={})",
               self.ty_to_string(t1),
               self.ty_to_string(t2),
               self.ty_to_string(t_glb));
        match self.glb().tys(t1, t2) {
            Err(e) => {
                panic!("unexpected error computing LUB: {}", e)
            }
            Ok(t) => {
                self.assert_eq(t, t_glb);

                // sanity check for good measure:
                self.assert_subtype(t, t1);
                self.assert_subtype(t, t2);
            }
        }
    }
}

#[test]
fn contravariant_region_ptr_ok() {
    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        env.create_simple_region_hierarchy();
        let t_rptr1 = env.t_rptr_scope(1);
        let t_rptr10 = env.t_rptr_scope(10);
        env.assert_eq(t_rptr1, t_rptr1);
        env.assert_eq(t_rptr10, t_rptr10);
        env.make_subtype(t_rptr1, t_rptr10);
    })
}

#[test]
fn contravariant_region_ptr_err() {
    test_env(EMPTY_SOURCE_STR,
             errors(&["lifetime mismatch"]),
             |env| {
                 env.create_simple_region_hierarchy();
                 let t_rptr1 = env.t_rptr_scope(1);
                 let t_rptr10 = env.t_rptr_scope(10);
                 env.assert_eq(t_rptr1, t_rptr1);
                 env.assert_eq(t_rptr10, t_rptr10);

                 // will cause an error when regions are resolved
                 env.make_subtype(t_rptr10, t_rptr1);
             })
}

#[test]
fn lub_bound_bound() {
    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        let t_rptr_bound1 = env.t_rptr_late_bound(1);
        let t_rptr_bound2 = env.t_rptr_late_bound(2);
        env.check_lub(env.t_fn(&[t_rptr_bound1], ty::mk_int()),
                      env.t_fn(&[t_rptr_bound2], ty::mk_int()),
                      env.t_fn(&[t_rptr_bound1], ty::mk_int()));
    })
}

#[test]
fn lub_bound_free() {
    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        let t_rptr_bound1 = env.t_rptr_late_bound(1);
        let t_rptr_free1 = env.t_rptr_free(0, 1);
        env.check_lub(env.t_fn(&[t_rptr_bound1], ty::mk_int()),
                      env.t_fn(&[t_rptr_free1], ty::mk_int()),
                      env.t_fn(&[t_rptr_free1], ty::mk_int()));
    })
}

#[test]
fn lub_bound_static() {
    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        let t_rptr_bound1 = env.t_rptr_late_bound(1);
        let t_rptr_static = env.t_rptr_static();
        env.check_lub(env.t_fn(&[t_rptr_bound1], ty::mk_int()),
                      env.t_fn(&[t_rptr_static], ty::mk_int()),
                      env.t_fn(&[t_rptr_static], ty::mk_int()));
    })
}

#[test]
fn lub_bound_bound_inverse_order() {
    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        let t_rptr_bound1 = env.t_rptr_late_bound(1);
        let t_rptr_bound2 = env.t_rptr_late_bound(2);
        env.check_lub(env.t_fn(&[t_rptr_bound1, t_rptr_bound2], t_rptr_bound1),
                      env.t_fn(&[t_rptr_bound2, t_rptr_bound1], t_rptr_bound1),
                      env.t_fn(&[t_rptr_bound1, t_rptr_bound1], t_rptr_bound1));
    })
}

#[test]
fn lub_free_free() {
    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        let t_rptr_free1 = env.t_rptr_free(0, 1);
        let t_rptr_free2 = env.t_rptr_free(0, 2);
        let t_rptr_static = env.t_rptr_static();
        env.check_lub(env.t_fn(&[t_rptr_free1], ty::mk_int()),
                      env.t_fn(&[t_rptr_free2], ty::mk_int()),
                      env.t_fn(&[t_rptr_static], ty::mk_int()));
    })
}

#[test]
fn lub_returning_scope() {
    test_env(EMPTY_SOURCE_STR,
             errors(&["cannot infer an appropriate lifetime"]), |env| {
                 let t_rptr_scope10 = env.t_rptr_scope(10);
                 let t_rptr_scope11 = env.t_rptr_scope(11);

                 // this should generate an error when regions are resolved
                 env.make_lub_ty(env.t_fn(&[], t_rptr_scope10),
                                 env.t_fn(&[], t_rptr_scope11));
             })
}

#[test]
fn glb_free_free_with_common_scope() {
    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        let t_rptr_free1 = env.t_rptr_free(0, 1);
        let t_rptr_free2 = env.t_rptr_free(0, 2);
        let t_rptr_scope = env.t_rptr_scope(0);
        env.check_glb(env.t_fn(&[t_rptr_free1], ty::mk_int()),
                      env.t_fn(&[t_rptr_free2], ty::mk_int()),
                      env.t_fn(&[t_rptr_scope], ty::mk_int()));
    })
}

#[test]
fn glb_bound_bound() {
    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        let t_rptr_bound1 = env.t_rptr_late_bound(1);
        let t_rptr_bound2 = env.t_rptr_late_bound(2);
        env.check_glb(env.t_fn(&[t_rptr_bound1], ty::mk_int()),
                      env.t_fn(&[t_rptr_bound2], ty::mk_int()),
                      env.t_fn(&[t_rptr_bound1], ty::mk_int()));
    })
}

#[test]
fn glb_bound_free() {
    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        let t_rptr_bound1 = env.t_rptr_late_bound(1);
        let t_rptr_free1 = env.t_rptr_free(0, 1);
        env.check_glb(env.t_fn(&[t_rptr_bound1], ty::mk_int()),
                      env.t_fn(&[t_rptr_free1], ty::mk_int()),
                      env.t_fn(&[t_rptr_bound1], ty::mk_int()));
    })
}

#[test]
fn glb_bound_static() {
    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        let t_rptr_bound1 = env.t_rptr_late_bound(1);
        let t_rptr_static = env.t_rptr_static();
        env.check_glb(env.t_fn(&[t_rptr_bound1], ty::mk_int()),
                      env.t_fn(&[t_rptr_static], ty::mk_int()),
                      env.t_fn(&[t_rptr_bound1], ty::mk_int()));
    })
}

#[test]
fn subst_ty_renumber_bound() {
    /*!
     * Test substituting a bound region into a function, which introduces another
     * level of binding. This requires adjusting the Debruijn index.
     */

    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        // Situation:
        // Theta = [A -> &'a foo]

        let t_rptr_bound1 = env.t_rptr_late_bound(1);

        // t_source = fn(A)
        let t_source = {
            let t_param = env.t_param(subst::TypeSpace, 0);
            env.t_fn(&[t_param], env.t_nil())
        };

        let substs = subst::Substs::new_type(vec![t_rptr_bound1], vec![]);
        let t_substituted = t_source.subst(env.infcx.tcx, &substs);

        // t_expected = fn(&'a int)
        let t_expected = {
            let t_ptr_bound2 = env.t_rptr_late_bound_with_debruijn(1, ty::DebruijnIndex::new(2));
            env.t_fn(&[t_ptr_bound2], env.t_nil())
        };

        debug!("subst_bound: t_source={} substs={} t_substituted={} t_expected={}",
               t_source.repr(env.infcx.tcx),
               substs.repr(env.infcx.tcx),
               t_substituted.repr(env.infcx.tcx),
               t_expected.repr(env.infcx.tcx));

        assert_eq!(t_substituted, t_expected);
    })
}

#[test]
fn subst_ty_renumber_some_bounds() {
    /*!
     * Test substituting a bound region into a function, which introduces another
     * level of binding. This requires adjusting the Debruijn index.
     */

    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        // Situation:
        // Theta = [A -> &'a foo]

        let t_rptr_bound1 = env.t_rptr_late_bound(1);

        // t_source = (A, fn(A))
        let t_source = {
            let t_param = env.t_param(subst::TypeSpace, 0);
            env.t_pair(t_param, env.t_fn(&[t_param], env.t_nil()))
        };

        let substs = subst::Substs::new_type(vec![t_rptr_bound1], vec![]);
        let t_substituted = t_source.subst(env.infcx.tcx, &substs);

        // t_expected = (&'a int, fn(&'a int))
        //
        // but not that the Debruijn index is different in the different cases.
        let t_expected = {
            let t_rptr_bound2 = env.t_rptr_late_bound_with_debruijn(1, ty::DebruijnIndex::new(2));
            env.t_pair(t_rptr_bound1, env.t_fn(&[t_rptr_bound2], env.t_nil()))
        };

        debug!("subst_bound: t_source={} substs={} t_substituted={} t_expected={}",
               t_source.repr(env.infcx.tcx),
               substs.repr(env.infcx.tcx),
               t_substituted.repr(env.infcx.tcx),
               t_expected.repr(env.infcx.tcx));

        assert_eq!(t_substituted, t_expected);
    })
}

#[test]
fn escaping() {
    /*!
     * Test that we correctly compute whether a type has escaping
     * regions or not.
     */

    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        // Situation:
        // Theta = [A -> &'a foo]

        assert!(!ty::type_has_escaping_regions(env.t_nil()));

        let t_rptr_free1 = env.t_rptr_free(0, 1);
        assert!(!ty::type_has_escaping_regions(t_rptr_free1));

        let t_rptr_bound1 = env.t_rptr_late_bound_with_debruijn(1, ty::DebruijnIndex::new(1));
        assert!(ty::type_has_escaping_regions(t_rptr_bound1));

        let t_rptr_bound2 = env.t_rptr_late_bound_with_debruijn(1, ty::DebruijnIndex::new(2));
        assert!(ty::type_has_escaping_regions(t_rptr_bound2));

        // t_fn = fn(A)
        let t_param = env.t_param(subst::TypeSpace, 0);
        assert!(!ty::type_has_escaping_regions(t_param));
        let t_fn = env.t_fn(&[t_param], env.t_nil());
        assert!(!ty::type_has_escaping_regions(t_fn));

        // t_fn = |&int|+'a
        let t_fn = env.t_closure(&[t_rptr_bound1], env.t_nil(), env.re_free(0, 1));
        assert!(!ty::type_has_escaping_regions(t_fn));

        // t_fn = |&int|+'a (where &int has depth 2)
        let t_fn = env.t_closure(&[t_rptr_bound2], env.t_nil(), env.re_free(0, 1));
        assert!(ty::type_has_escaping_regions(t_fn));

        // t_fn = |&int|+&int
        let t_fn = env.t_closure(&[t_rptr_bound1], env.t_nil(),
                                 env.re_late_bound_with_debruijn(1, ty::DebruijnIndex::new(1)));
        assert!(ty::type_has_escaping_regions(t_fn));
    })
}

#[test]
fn subst_region_renumber_region() {
    /*!
     * Test applying a substitution where the value being substituted
     * for an early-bound region is a late-bound region.
     */

    test_env(EMPTY_SOURCE_STR, errors(&[]), |env| {
        let re_bound1 = env.re_late_bound_with_debruijn(1, ty::DebruijnIndex::new(1));

        // type t_source<'a> = fn(&'a int)
        let t_source = {
            let re_early = env.re_early_bound(subst::TypeSpace, 0, "'a");
            env.t_fn(&[env.t_rptr(re_early)], env.t_nil())
        };

        let substs = subst::Substs::new_type(vec![], vec![re_bound1]);
        let t_substituted = t_source.subst(env.infcx.tcx, &substs);

        // t_expected = fn(&'a int)
        //
        // but not that the Debruijn index is different in the different cases.
        let t_expected = {
            let t_rptr_bound2 = env.t_rptr_late_bound_with_debruijn(1, ty::DebruijnIndex::new(2));
            env.t_fn(&[t_rptr_bound2], env.t_nil())
        };

        debug!("subst_bound: t_source={} substs={} t_substituted={} t_expected={}",
               t_source.repr(env.infcx.tcx),
               substs.repr(env.infcx.tcx),
               t_substituted.repr(env.infcx.tcx),
               t_expected.repr(env.infcx.tcx));

        assert_eq!(t_substituted, t_expected);
    })
}

