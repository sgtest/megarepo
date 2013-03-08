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

This file actually contains two passes related to regions.  The first
pass builds up the `region_map`, which describes the parent links in
the region hierarchy.  The second pass infers which types must be
region parameterized.

*/

use core::prelude::*;

use driver::session::Session;
use metadata::csearch;
use middle::resolve;
use middle::ty::{region_variance, rv_covariant, rv_invariant};
use middle::ty::{rv_contravariant};
use middle::ty;

use core::dvec::DVec;
use core::vec;
use std::oldmap::HashMap;
use syntax::ast_map;
use syntax::codemap::span;
use syntax::print::pprust;
use syntax::{ast, visit};

pub type parent = Option<ast::node_id>;

/**
Encodes the bounding lifetime for a given AST node:

- Expressions are mapped to the expression or block encoding the maximum
  (static) lifetime of a value produced by that expression.  This is
  generally the innermost call, statement, match, or block.

- Variables and bindings are mapped to the block in which they are declared.

*/
pub type region_map = HashMap<ast::node_id, ast::node_id>;

pub struct ctxt {
    sess: Session,
    def_map: resolve::DefMap,

    // Generated maps:
    region_map: region_map,

    // Generally speaking, expressions are parented to their innermost
    // enclosing block. But some kinds of expressions serve as
    // parents: calls, methods, etc.  In addition, some expressions
    // serve as parents by virtue of where they appear.  For example,
    // the condition in a while loop is always a parent.  In those
    // cases, we add the node id of such an expression to this set so
    // that when we visit it we can view it as a parent.
    root_exprs: HashMap<ast::node_id, ()>,

    // The parent scope is the innermost block, statement, call, or match
    // expression during the execution of which the current expression
    // will be evaluated.  Generally speaking, the innermost parent
    // scope is also the closest suitable ancestor in the AST tree.
    //
    // There is a subtle point concerning call arguments.  Imagine
    // you have a call:
    //
    // { // block a
    //     foo( // call b
    //        x,
    //        y);
    // }
    //
    // In what lifetime are the expressions `x` and `y` evaluated?  At
    // first, I imagine the answer was the block `a`, as the arguments
    // are evaluated before the call takes place.  But this turns out
    // to be wrong.  The lifetime of the call must encompass the
    // argument evaluation as well.
    //
    // The reason is that evaluation of an earlier argument could
    // create a borrow which exists during the evaluation of later
    // arguments.  Consider this torture test, for example,
    //
    // fn test1(x: @mut ~int) {
    //     foo(&**x, *x = ~5);
    // }
    //
    // Here, the first argument `&**x` will be a borrow of the `~int`,
    // but the second argument overwrites that very value! Bad.
    // (This test is borrowck-pure-scope-in-call.rs, btw)
    parent: parent,
}

/// Returns true if `subscope` is equal to or is lexically nested inside
/// `superscope` and false otherwise.
pub fn scope_contains(region_map: region_map, superscope: ast::node_id,
                      subscope: ast::node_id) -> bool {
    let mut subscope = subscope;
    while superscope != subscope {
        match region_map.find(&subscope) {
            None => return false,
            Some(scope) => subscope = scope
        }
    }
    return true;
}

/// Determines whether one region is a subregion of another.  This is
/// intended to run *after inference* and sadly the logic is somewhat
/// duplicated with the code in infer.rs.
pub fn is_subregion_of(region_map: region_map,
                       sub_region: ty::Region,
                       super_region: ty::Region) -> bool {
    sub_region == super_region ||
        match (sub_region, super_region) {
            (_, ty::re_static) => {
                true
            }

            (ty::re_scope(sub_scope), ty::re_scope(super_scope)) |
            (ty::re_scope(sub_scope), ty::re_free(super_scope, _)) => {
                scope_contains(region_map, super_scope, sub_scope)
            }

            _ => {
                false
            }
        }
}

/// Finds the nearest common ancestor (if any) of two scopes.  That
/// is, finds the smallest scope which is greater than or equal to
/// both `scope_a` and `scope_b`.
pub fn nearest_common_ancestor(region_map: region_map,
                               scope_a: ast::node_id,
                               scope_b: ast::node_id)
                            -> Option<ast::node_id> {

    fn ancestors_of(region_map: region_map, scope: ast::node_id)
                    -> ~[ast::node_id] {
        let mut result = ~[scope];
        let mut scope = scope;
        loop {
            match region_map.find(&scope) {
                None => return result,
                Some(superscope) => {
                    result.push(superscope);
                    scope = superscope;
                }
            }
        }
    }

    if scope_a == scope_b { return Some(scope_a); }

    let a_ancestors = ancestors_of(region_map, scope_a);
    let b_ancestors = ancestors_of(region_map, scope_b);
    let mut a_index = vec::len(a_ancestors) - 1u;
    let mut b_index = vec::len(b_ancestors) - 1u;

    // Here, ~[ab]_ancestors is a vector going from narrow to broad.
    // The end of each vector will be the item where the scope is
    // defined; if there are any common ancestors, then the tails of
    // the vector will be the same.  So basically we want to walk
    // backwards from the tail of each vector and find the first point
    // where they diverge.  If one vector is a suffix of the other,
    // then the corresponding scope is a superscope of the other.

    if a_ancestors[a_index] != b_ancestors[b_index] {
        return None;
    }

    loop {
        // Loop invariant: a_ancestors[a_index] == b_ancestors[b_index]
        // for all indices between a_index and the end of the array
        if a_index == 0u { return Some(scope_a); }
        if b_index == 0u { return Some(scope_b); }
        a_index -= 1u;
        b_index -= 1u;
        if a_ancestors[a_index] != b_ancestors[b_index] {
            return Some(a_ancestors[a_index + 1u]);
        }
    }
}

/// Extracts that current parent from cx, failing if there is none.
pub fn parent_id(cx: ctxt, span: span) -> ast::node_id {
    match cx.parent {
      None => {
        cx.sess.span_bug(span, ~"crate should not be parent here");
      }
      Some(parent_id) => {
        parent_id
      }
    }
}

/// Records the current parent (if any) as the parent of `child_id`.
pub fn record_parent(cx: ctxt, child_id: ast::node_id) {
    for cx.parent.each |parent_id| {
        debug!("parent of node %d is node %d", child_id, *parent_id);
        cx.region_map.insert(child_id, *parent_id);
    }
}

pub fn resolve_block(blk: &ast::blk, cx: ctxt, visitor: visit::vt<ctxt>) {
    // Record the parent of this block.
    record_parent(cx, blk.node.id);

    // Descend.
    let new_cx: ctxt = ctxt {parent: Some(blk.node.id),.. cx};
    visit::visit_block(blk, new_cx, visitor);
}

pub fn resolve_arm(arm: &ast::arm, cx: ctxt, visitor: visit::vt<ctxt>) {
    visit::visit_arm(arm, cx, visitor);
}

pub fn resolve_pat(pat: @ast::pat, cx: ctxt, visitor: visit::vt<ctxt>) {
    match pat.node {
      ast::pat_ident(*) => {
        let defn_opt = cx.def_map.find(&pat.id);
        match defn_opt {
          Some(ast::def_variant(_,_)) => {
            /* Nothing to do; this names a variant. */
          }
          _ => {
            /* This names a local. Bind it to the containing scope. */
            record_parent(cx, pat.id);
          }
        }
      }
      _ => { /* no-op */ }
    }

    visit::visit_pat(pat, cx, visitor);
}

pub fn resolve_stmt(stmt: @ast::stmt, cx: ctxt, visitor: visit::vt<ctxt>) {
    match stmt.node {
      ast::stmt_decl(*) => {
        visit::visit_stmt(stmt, cx, visitor);
      }
      // This code has to be kept consistent with trans::base::trans_stmt
      ast::stmt_expr(_, stmt_id) |
      ast::stmt_semi(_, stmt_id) => {
        record_parent(cx, stmt_id);
        let mut expr_cx = cx;
        expr_cx.parent = Some(stmt_id);
        visit::visit_stmt(stmt, expr_cx, visitor);
      }
      ast::stmt_mac(*) => cx.sess.bug(~"unexpanded macro")
    }
}

pub fn resolve_expr(expr: @ast::expr, cx: ctxt, visitor: visit::vt<ctxt>) {
    record_parent(cx, expr.id);

    let mut new_cx = cx;
    match expr.node {
      // Calls or overloadable operators
      // FIXME #3387
      // ast::expr_index(*) | ast::expr_binary(*) |
      // ast::expr_unary(*) |
      ast::expr_call(*) | ast::expr_method_call(*) => {
        debug!("node %d: %s", expr.id, pprust::expr_to_str(expr,
                                                           cx.sess.intr()));
        new_cx.parent = Some(expr.id);
      }
      ast::expr_match(*) => {
        debug!("node %d: %s", expr.id, pprust::expr_to_str(expr,
                                                           cx.sess.intr()));
        new_cx.parent = Some(expr.id);
      }
      ast::expr_while(cond, _) => {
        new_cx.root_exprs.insert(cond.id, ());
      }
      _ => {}
    };

    if new_cx.root_exprs.contains_key(&expr.id) {
        new_cx.parent = Some(expr.id);
    }

    visit::visit_expr(expr, new_cx, visitor);
}

pub fn resolve_local(local: @ast::local,
                     cx: ctxt,
                     visitor: visit::vt<ctxt>) {
    record_parent(cx, local.node.id);
    visit::visit_local(local, cx, visitor);
}

pub fn resolve_item(item: @ast::item, cx: ctxt, visitor: visit::vt<ctxt>) {
    // Items create a new outer block scope as far as we're concerned.
    let new_cx: ctxt = ctxt {parent: None,.. cx};
    visit::visit_item(item, new_cx, visitor);
}

pub fn resolve_fn(fk: &visit::fn_kind,
                  decl: &ast::fn_decl,
                  body: &ast::blk,
                  sp: span,
                  id: ast::node_id,
                  cx: ctxt,
                  visitor: visit::vt<ctxt>) {
    let fn_cx = match *fk {
        visit::fk_item_fn(*) | visit::fk_method(*) |
        visit::fk_dtor(*) => {
            // Top-level functions are a root scope.
            ctxt {parent: Some(id),.. cx}
        }

        visit::fk_anon(*) | visit::fk_fn_block(*) => {
            // Closures continue with the inherited scope.
            cx
        }
    };

    // Record the ID of `self`.
    match *fk {
        visit::fk_method(_, _, method) => {
            cx.region_map.insert(method.self_id, body.node.id);
        }
        _ => {}
    }

    debug!("visiting fn with body %d. cx.parent: %? \
            fn_cx.parent: %?",
           body.node.id, cx.parent, fn_cx.parent);

    for decl.inputs.each |input| {
        cx.region_map.insert(input.id, body.node.id);
    }

    visit::visit_fn(fk, decl, body, sp, id, fn_cx, visitor);
}

pub fn resolve_crate(sess: Session,
                     def_map: resolve::DefMap,
                     crate: @ast::crate)
                  -> region_map {
    let cx: ctxt = ctxt {sess: sess,
                         def_map: def_map,
                         region_map: HashMap(),
                         root_exprs: HashMap(),
                         parent: None};
    let visitor = visit::mk_vt(@visit::Visitor {
        visit_block: resolve_block,
        visit_item: resolve_item,
        visit_fn: resolve_fn,
        visit_arm: resolve_arm,
        visit_pat: resolve_pat,
        visit_stmt: resolve_stmt,
        visit_expr: resolve_expr,
        visit_local: resolve_local,
        .. *visit::default_visitor()
    });
    visit::visit_crate(*crate, cx, visitor);
    return cx.region_map;
}

// ___________________________________________________________________________
// Determining region parameterization
//
// Infers which type defns must be region parameterized---this is done
// by scanning their contents to see whether they reference a region
// type, directly or indirectly.  This is a fixed-point computation.
//
// We do it in two passes.  First we walk the AST and construct a map
// from each type defn T1 to other defns which make use of it.  For example,
// if we have a type like:
//
//    type S = *int;
//    type T = S;
//
// Then there would be a map entry from S to T.  During the same walk,
// we also construct add any types that reference regions to a set and
// a worklist.  We can then process the worklist, propagating indirect
// dependencies until a fixed point is reached.

pub type region_paramd_items = HashMap<ast::node_id, region_variance>;

#[deriving_eq]
pub struct region_dep {
    ambient_variance: region_variance,
    id: ast::node_id
}

pub type dep_map = HashMap<ast::node_id, @DVec<region_dep>>;

pub struct DetermineRpCtxt {
    sess: Session,
    ast_map: ast_map::map,
    def_map: resolve::DefMap,
    region_paramd_items: region_paramd_items,
    dep_map: dep_map,
    worklist: ~[ast::node_id],

    // the innermost enclosing item id
    item_id: ast::node_id,

    // true when we are within an item but not within a method.
    // see long discussion on region_is_relevant().
    anon_implies_rp: bool,

    // true when we are not within an &self method.
    // see long discussion on region_is_relevant().
    self_implies_rp: bool,

    // encodes the context of the current type; invariant if
    // mutable, covariant otherwise
    ambient_variance: region_variance,
}

pub fn join_variance(++variance1: region_variance,
                     ++variance2: region_variance)
                  -> region_variance {
    match (variance1, variance2) {
      (rv_invariant, _) => {rv_invariant}
      (_, rv_invariant) => {rv_invariant}
      (rv_covariant, rv_contravariant) => {rv_invariant}
      (rv_contravariant, rv_covariant) => {rv_invariant}
      (rv_covariant, rv_covariant) => {rv_covariant}
      (rv_contravariant, rv_contravariant) => {rv_contravariant}
    }
}

/// Combines the ambient variance with the variance of a
/// particular site to yield the final variance of the reference.
///
/// Example: if we are checking function arguments then the ambient
/// variance is contravariant.  If we then find a `&r/T` pointer, `r`
/// appears in a co-variant position.  This implies that this
/// occurrence of `r` is contra-variant with respect to the current
/// item, and hence the function returns `rv_contravariant`.
pub fn add_variance(+ambient_variance: region_variance,
                    +variance: region_variance)
                 -> region_variance {
    match (ambient_variance, variance) {
      (rv_invariant, _) => rv_invariant,
      (_, rv_invariant) => rv_invariant,
      (rv_covariant, c) => c,
      (c, rv_covariant) => c,
      (rv_contravariant, rv_contravariant) => rv_covariant
    }
}

pub impl DetermineRpCtxt {
    fn add_variance(&self, variance: region_variance) -> region_variance {
        add_variance(self.ambient_variance, variance)
    }

    /// Records that item `id` is region-parameterized with the
    /// variance `variance`.  If `id` was already parameterized, then
    /// the new variance is joined with the old variance.
    fn add_rp(&mut self, id: ast::node_id, variance: region_variance) {
        fail_unless!(id != 0);
        let old_variance = self.region_paramd_items.find(&id);
        let joined_variance = match old_variance {
          None => variance,
          Some(v) => join_variance(v, variance)
        };

        debug!("add_rp() variance for %s: %? == %? ^ %?",
               ast_map::node_id_to_str(self.ast_map, id,
                                       self.sess.parse_sess.interner),
               joined_variance, old_variance, variance);

        if Some(joined_variance) != old_variance {
            let region_paramd_items = self.region_paramd_items;
            region_paramd_items.insert(id, joined_variance);
            self.worklist.push(id);
        }
    }

    /// Indicates that the region-parameterization of the current item
    /// is dependent on the region-parameterization of the item
    /// `from`.  Put another way, it indicates that the current item
    /// contains a value of type `from`, so if `from` is
    /// region-parameterized, so is the current item.
    fn add_dep(&mut self, from: ast::node_id) {
        debug!("add dependency from %d -> %d (%s -> %s) with variance %?",
               from, self.item_id,
               ast_map::node_id_to_str(self.ast_map, from,
                                       self.sess.parse_sess.interner),
               ast_map::node_id_to_str(self.ast_map, self.item_id,
                                       self.sess.parse_sess.interner),
               copy self.ambient_variance);
        let vec = match self.dep_map.find(&from) {
            Some(vec) => vec,
            None => {
                let vec = @DVec();
                let dep_map = self.dep_map;
                dep_map.insert(from, vec);
                vec
            }
        };
        let dep = region_dep {
            ambient_variance: self.ambient_variance,
            id: self.item_id
        };
        if !vec.contains(&dep) { vec.push(dep); }
    }

    // Determines whether a reference to a region that appears in the
    // AST implies that the enclosing type is region-parameterized (RP).
    // This point is subtle.  Here are some examples to make it more
    // concrete.
    //
    // 1. impl foo for &int { ... }
    // 2. impl foo for &self/int { ... }
    // 3. impl foo for bar { fn m(@self) -> &self/int { ... } }
    // 4. impl foo for bar { fn m(&self) -> &self/int { ... } }
    // 5. impl foo for bar { fn m(&self) -> &int { ... } }
    //
    // In case 1, the anonymous region is being referenced,
    // but it appears in a context where the anonymous region
    // resolves to self, so the impl foo is RP.
    //
    // In case 2, the self parameter is written explicitly.
    //
    // In case 3, the method refers to the region `self`, so that
    // implies that the impl must be region parameterized.  (If the
    // type bar is not region parameterized, that is an error, because
    // the self region is effectively unconstrained, but that is
    // detected elsewhere).
    //
    // In case 4, the method refers to the region `self`, but the
    // `self` region is bound by the `&self` receiver, and so this
    // does not require that `bar` be RP.
    //
    // In case 5, the anonymous region is referenced, but it
    // bound by the method, so it does not refer to self.  This impl
    // need not be region parameterized.
    //
    // Normally, & or &self implies that the enclosing item is RP.
    // However, within a function, & is always bound.  Within a method
    // with &self type, &self is also bound.  We detect those last two
    // cases via flags (anon_implies_rp and self_implies_rp) that are
    // true when the anon or self region implies RP.
    fn region_is_relevant(&self, r: @ast::region) -> bool {
        match r.node {
            ast::re_static => false,
            ast::re_anon => self.anon_implies_rp,
            ast::re_self => self.self_implies_rp,
            ast::re_named(_) => false
        }
    }

    // For named types like Foo, if there is no explicit region
    // parameter, then we will add the anonymous region, so there is
    // a dependency if the anonymous region implies rp.
    //
    // If the region is explicitly specified, then we follows the
    // normal rules.
    fn opt_region_is_relevant(&self,
                              opt_r: Option<@ast::region>)
                           -> bool {
        debug!("opt_region_is_relevant: %? (anon_implies_rp=%b)",
               opt_r, self.anon_implies_rp);
        match opt_r {
          None => self.anon_implies_rp,
          Some(r) => self.region_is_relevant(r)
        }
    }

    fn with(@mut self,
            item_id: ast::node_id,
            anon_implies_rp: bool,
            self_implies_rp: bool,
            f: &fn()) {
        let old_item_id = self.item_id;
        let old_anon_implies_rp = self.anon_implies_rp;
        let old_self_implies_rp = self.self_implies_rp;
        self.item_id = item_id;
        self.anon_implies_rp = anon_implies_rp;
        self.self_implies_rp = self_implies_rp;
        debug!("with_item_id(%d, %b, %b)",
               item_id,
               anon_implies_rp,
               self_implies_rp);
        let _i = ::util::common::indenter();
        f();
        self.item_id = old_item_id;
        self.anon_implies_rp = old_anon_implies_rp;
        self.self_implies_rp = old_self_implies_rp;
    }

    fn with_ambient_variance(@mut self, variance: region_variance, f: &fn()) {
        let old_ambient_variance = self.ambient_variance;
        self.ambient_variance = self.add_variance(variance);
        f();
        self.ambient_variance = old_ambient_variance;
    }
}

pub fn determine_rp_in_item(item: @ast::item,
                            &&cx: @mut DetermineRpCtxt,
                            visitor: visit::vt<@mut DetermineRpCtxt>) {
    do cx.with(item.id, true, true) {
        visit::visit_item(item, cx, visitor);
    }
}

pub fn determine_rp_in_fn(fk: &visit::fn_kind,
                          decl: &ast::fn_decl,
                          body: &ast::blk,
                          _: span,
                          _: ast::node_id,
                          &&cx: @mut DetermineRpCtxt,
                          visitor: visit::vt<@mut DetermineRpCtxt>) {
    let self_implies_rp = match fk {
        &visit::fk_method(_, _, m) => !m.self_ty.node.is_borrowed(),
        _ => true
    };

    do cx.with(cx.item_id, false, self_implies_rp) {
        do cx.with_ambient_variance(rv_contravariant) {
            for decl.inputs.each |a| {
                (visitor.visit_ty)(a.ty, cx, visitor);
            }
        }
        (visitor.visit_ty)(decl.output, cx, visitor);
        let generics = visit::generics_of_fn(fk);
        (visitor.visit_generics)(&generics, cx, visitor);
        (visitor.visit_block)(body, cx, visitor);
    }
}

pub fn determine_rp_in_ty_method(ty_m: &ast::ty_method,
                                 &&cx: @mut DetermineRpCtxt,
                                 visitor: visit::vt<@mut DetermineRpCtxt>) {
    do cx.with(cx.item_id, false, !ty_m.self_ty.node.is_borrowed()) {
        visit::visit_ty_method(ty_m, cx, visitor);
    }
}

pub fn determine_rp_in_ty(ty: @ast::Ty,
                          &&cx: @mut DetermineRpCtxt,
                          visitor: visit::vt<@mut DetermineRpCtxt>) {
    // we are only interested in types that will require an item to
    // be region-parameterized.  if cx.item_id is zero, then this type
    // is not a member of a type defn nor is it a constitutent of an
    // impl etc.  So we can ignore it and its components.
    if cx.item_id == 0 { return; }

    // if this type directly references a region pointer like &r/ty,
    // add to the worklist/set.  Note that &r/ty is contravariant with
    // respect to &r, because &r/ty can be used whereever a *smaller*
    // region is expected (and hence is a supertype of those
    // locations)
    let sess = cx.sess;
    match ty.node {
        ast::ty_rptr(r, _) => {
            debug!("referenced rptr type %s",
                   pprust::ty_to_str(ty, sess.intr()));

            if cx.region_is_relevant(r) {
                cx.add_rp(cx.item_id, cx.add_variance(rv_contravariant))
            }
        }

        ast::ty_closure(ref f) => {
            debug!("referenced fn type: %s",
                   pprust::ty_to_str(ty, sess.intr()));
            match f.region {
                Some(r) => {
                    if cx.region_is_relevant(r) {
                        cx.add_rp(cx.item_id,
                                  cx.add_variance(rv_contravariant))
                    }
                }
                None => {
                    if f.sigil == ast::BorrowedSigil && cx.anon_implies_rp {
                        cx.add_rp(cx.item_id,
                                  cx.add_variance(rv_contravariant));
                    }
                }
            }
        }

        _ => {}
    }

    // if this references another named type, add the dependency
    // to the dep_map.  If the type is not defined in this crate,
    // then check whether it is region-parameterized and consider
    // that as a direct dependency.
    match ty.node {
      ast::ty_path(path, id) => {
        match cx.def_map.find(&id) {
          Some(ast::def_ty(did)) | Some(ast::def_struct(did)) => {
            if did.crate == ast::local_crate {
                if cx.opt_region_is_relevant(path.rp) {
                    cx.add_dep(did.node);
                }
            } else {
                let cstore = sess.cstore;
                match csearch::get_region_param(cstore, did) {
                  None => {}
                  Some(variance) => {
                    debug!("reference to external, rp'd type %s",
                           pprust::ty_to_str(ty, sess.intr()));
                    if cx.opt_region_is_relevant(path.rp) {
                        cx.add_rp(cx.item_id, cx.add_variance(variance))
                    }
                  }
                }
            }
          }
          _ => {}
        }
      }
      _ => {}
    }

    match ty.node {
      ast::ty_box(mt) | ast::ty_uniq(mt) | ast::ty_vec(mt) |
      ast::ty_rptr(_, mt) | ast::ty_ptr(mt) => {
        visit_mt(mt, cx, visitor);
      }

      ast::ty_path(path, _) => {
        // type parameters are---for now, anyway---always invariant
        do cx.with_ambient_variance(rv_invariant) {
            for path.types.each |tp| {
                (visitor.visit_ty)(*tp, cx, visitor);
            }
        }
      }

      ast::ty_closure(@ast::TyClosure {decl: ref decl, _}) |
      ast::ty_bare_fn(@ast::TyBareFn {decl: ref decl, _}) => {
        // fn() binds the & region, so do not consider &T types that
        // appear *inside* a fn() type to affect the enclosing item:
        do cx.with(cx.item_id, false, true) {
            // parameters are contravariant
            do cx.with_ambient_variance(rv_contravariant) {
                for decl.inputs.each |a| {
                    (visitor.visit_ty)(a.ty, cx, visitor);
                }
            }
            (visitor.visit_ty)(decl.output, cx, visitor);
        }
      }

      _ => {
        visit::visit_ty(ty, cx, visitor);
      }
    }

    fn visit_mt(mt: ast::mt,
                &&cx: @mut DetermineRpCtxt,
                visitor: visit::vt<@mut DetermineRpCtxt>) {
        // mutability is invariant
        if mt.mutbl == ast::m_mutbl {
            do cx.with_ambient_variance(rv_invariant) {
                (visitor.visit_ty)(mt.ty, cx, visitor);
            }
        } else {
            (visitor.visit_ty)(mt.ty, cx, visitor);
        }
    }
}

pub fn determine_rp_in_struct_field(
        cm: @ast::struct_field,
        &&cx: @mut DetermineRpCtxt,
        visitor: visit::vt<@mut DetermineRpCtxt>) {
    match cm.node.kind {
      ast::named_field(_, ast::struct_mutable, _) => {
        do cx.with_ambient_variance(rv_invariant) {
            visit::visit_struct_field(cm, cx, visitor);
        }
      }
      ast::named_field(_, ast::struct_immutable, _) |
      ast::unnamed_field => {
        visit::visit_struct_field(cm, cx, visitor);
      }
    }
}

pub fn determine_rp_in_crate(sess: Session,
                             ast_map: ast_map::map,
                             def_map: resolve::DefMap,
                             crate: @ast::crate)
                          -> region_paramd_items {
    let cx = @mut DetermineRpCtxt {
        sess: sess,
        ast_map: ast_map,
        def_map: def_map,
        region_paramd_items: HashMap(),
        dep_map: HashMap(),
        worklist: ~[],
        item_id: 0,
        anon_implies_rp: false,
        self_implies_rp: true,
        ambient_variance: rv_covariant
    };

    // Gather up the base set, worklist and dep_map
    let visitor = visit::mk_vt(@visit::Visitor {
        visit_fn: determine_rp_in_fn,
        visit_item: determine_rp_in_item,
        visit_ty: determine_rp_in_ty,
        visit_ty_method: determine_rp_in_ty_method,
        visit_struct_field: determine_rp_in_struct_field,
        .. *visit::default_visitor()
    });
    visit::visit_crate(*crate, cx, visitor);

    // Propagate indirect dependencies
    //
    // Each entry in the worklist is the id of an item C whose region
    // parameterization has been updated.  So we pull ids off of the
    // worklist, find the current variance, and then iterate through
    // all of the dependent items (that is, those items that reference
    // C).  For each dependent item D, we combine the variance of C
    // with the ambient variance where the reference occurred and then
    // update the region-parameterization of D to reflect the result.
    while cx.worklist.len() != 0 {
        let c_id = cx.worklist.pop();
        let c_variance = cx.region_paramd_items.get(&c_id);
        debug!("popped %d from worklist", c_id);
        match cx.dep_map.find(&c_id) {
          None => {}
          Some(deps) => {
            for deps.each |dep| {
                let v = add_variance(dep.ambient_variance, c_variance);
                cx.add_rp(dep.id, v);
            }
          }
        }
    }

    debug!("%s", {
        debug!("Region variance results:");
        let region_paramd_items = cx.region_paramd_items;
        for region_paramd_items.each |&key, &value| {
            debug!("item %? (%s) is parameterized with variance %?",
                   key,
                   ast_map::node_id_to_str(ast_map, key,
                                           sess.parse_sess.interner),
                   value);
        }
        "----"
    });

    // return final set
    return cx.region_paramd_items;
}

