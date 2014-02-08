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
pass builds up the `scope_map`, which describes the parent links in
the region hierarchy.  The second pass infers which types must be
region parameterized.

Most of the documentation on regions can be found in
`middle/typeck/infer/region_inference.rs`

*/


use driver::session::Session;
use middle::ty::{FreeRegion};
use middle::ty;

use std::cell::RefCell;
use std::hashmap::{HashMap, HashSet};
use syntax::codemap::Span;
use syntax::{ast, visit};
use syntax::visit::{Visitor, FnKind};
use syntax::ast::{Block, Item, FnDecl, NodeId, Arm, Pat, Stmt, Expr, Local};
use syntax::ast_util::{stmt_id};

/**
The region maps encode information about region relationships.

- `scope_map` maps from:
  - an expression to the expression or block encoding the maximum
    (static) lifetime of a value produced by that expression.  This is
    generally the innermost call, statement, match, or block.
  - a variable or binding id to the block in which that variable is declared.
- `free_region_map` maps from:
  - a free region `a` to a list of free regions `bs` such that
    `a <= b for all b in bs`
  - the free region map is populated during type check as we check
    each function. See the function `relate_free_regions` for
    more information.
- `temporary_scopes` includes scopes where cleanups for temporaries occur.
  These are statements and loop/fn bodies.
*/
pub struct RegionMaps {
    priv scope_map: RefCell<HashMap<ast::NodeId, ast::NodeId>>,
    priv var_map: RefCell<HashMap<ast::NodeId, ast::NodeId>>,
    priv free_region_map: RefCell<HashMap<FreeRegion, ~[FreeRegion]>>,
    priv rvalue_scopes: RefCell<HashMap<ast::NodeId, ast::NodeId>>,
    priv terminating_scopes: RefCell<HashSet<ast::NodeId>>,
}

#[deriving(Clone)]
pub struct Context {
    var_parent: Option<ast::NodeId>,

    // Innermost enclosing expression
    parent: Option<ast::NodeId>,
}

struct RegionResolutionVisitor<'a> {
    sess: Session,

    // Generated maps:
    region_maps: &'a RegionMaps,
}


impl RegionMaps {
    pub fn relate_free_regions(&self, sub: FreeRegion, sup: FreeRegion) {
        let mut free_region_map = self.free_region_map.borrow_mut();
        match free_region_map.get().find_mut(&sub) {
            Some(sups) => {
                if !sups.iter().any(|x| x == &sup) {
                    sups.push(sup);
                }
                return;
            }
            None => {}
        }

        debug!("relate_free_regions(sub={:?}, sup={:?})", sub, sup);

        free_region_map.get().insert(sub, ~[sup]);
    }

    pub fn record_encl_scope(&self, sub: ast::NodeId, sup: ast::NodeId) {
        debug!("record_encl_scope(sub={}, sup={})", sub, sup);
        assert!(sub != sup);

        let mut scope_map = self.scope_map.borrow_mut();
        scope_map.get().insert(sub, sup);
    }

    pub fn record_var_scope(&self, var: ast::NodeId, lifetime: ast::NodeId) {
        debug!("record_var_scope(sub={}, sup={})", var, lifetime);
        assert!(var != lifetime);

        let mut var_map = self.var_map.borrow_mut();
        var_map.get().insert(var, lifetime);
    }

    pub fn record_rvalue_scope(&self, var: ast::NodeId, lifetime: ast::NodeId) {
        debug!("record_rvalue_scope(sub={}, sup={})", var, lifetime);
        assert!(var != lifetime);

        let mut rvalue_scopes = self.rvalue_scopes.borrow_mut();
        rvalue_scopes.get().insert(var, lifetime);
    }

    pub fn mark_as_terminating_scope(&self, scope_id: ast::NodeId) {
        /*!
         * Records that a scope is a TERMINATING SCOPE. Whenever we
         * create automatic temporaries -- e.g. by an
         * expression like `a().f` -- they will be freed within
         * the innermost terminating scope.
         */

        debug!("record_terminating_scope(scope_id={})", scope_id);
        let mut terminating_scopes = self.terminating_scopes.borrow_mut();
        terminating_scopes.get().insert(scope_id);
    }

    pub fn opt_encl_scope(&self, id: ast::NodeId) -> Option<ast::NodeId> {
        //! Returns the narrowest scope that encloses `id`, if any.

        let scope_map = self.scope_map.borrow();
        scope_map.get().find(&id).map(|x| *x)
    }

    pub fn encl_scope(&self, id: ast::NodeId) -> ast::NodeId {
        //! Returns the narrowest scope that encloses `id`, if any.

        let scope_map = self.scope_map.borrow();
        match scope_map.get().find(&id) {
            Some(&r) => r,
            None => { fail!("no enclosing scope for id {}", id); }
        }
    }

    pub fn var_scope(&self, var_id: ast::NodeId) -> ast::NodeId {
        /*!
         * Returns the lifetime of the local variable `var_id`
         */

        let var_map = self.var_map.borrow();
        match var_map.get().find(&var_id) {
            Some(&r) => r,
            None => { fail!("no enclosing scope for id {}", var_id); }
        }
    }

    pub fn temporary_scope(&self, expr_id: ast::NodeId) -> Option<ast::NodeId> {
        //! Returns the scope when temp created by expr_id will be cleaned up

        // check for a designated rvalue scope
        let rvalue_scopes = self.rvalue_scopes.borrow();
        match rvalue_scopes.get().find(&expr_id) {
            Some(&s) => {
                debug!("temporary_scope({}) = {} [custom]", expr_id, s);
                return Some(s);
            }
            None => { }
        }

        // else, locate the innermost terminating scope
        // if there's one. Static items, for instance, won't
        // have an enclusing scope, hence no scope will be
        // returned.
        let mut id = match self.opt_encl_scope(expr_id) {
            Some(i) => i,
            None => { return None; }
        };

        let terminating_scopes = self.terminating_scopes.borrow();
        while !terminating_scopes.get().contains(&id) {
            match self.opt_encl_scope(id) {
                Some(p) => {
                    id = p;
                }
                None => {
                    debug!("temporary_scope({}) = None", expr_id);
                    return None;
                }
            }
        }
        debug!("temporary_scope({}) = {} [enclosing]", expr_id, id);
        return Some(id);
    }

    pub fn encl_region(&self, id: ast::NodeId) -> ty::Region {
        //! Returns the narrowest scope region that encloses `id`, if any.

        ty::ReScope(self.encl_scope(id))
    }

    pub fn var_region(&self, id: ast::NodeId) -> ty::Region {
        //! Returns the lifetime of the variable `id`.

        ty::ReScope(self.var_scope(id))
    }

    pub fn scopes_intersect(&self, scope1: ast::NodeId, scope2: ast::NodeId)
                            -> bool {
        self.is_subscope_of(scope1, scope2) ||
        self.is_subscope_of(scope2, scope1)
    }

    pub fn is_subscope_of(&self,
                          subscope: ast::NodeId,
                          superscope: ast::NodeId)
                          -> bool {
        /*!
         * Returns true if `subscope` is equal to or is lexically
         * nested inside `superscope` and false otherwise.
         */

        let mut s = subscope;
        while superscope != s {
            let scope_map = self.scope_map.borrow();
            match scope_map.get().find(&s) {
                None => {
                    debug!("is_subscope_of({}, {}, s={})=false",
                           subscope, superscope, s);

                    return false;
                }
                Some(&scope) => s = scope
            }
        }

        debug!("is_subscope_of({}, {})=true",
               subscope, superscope);

        return true;
    }

    pub fn sub_free_region(&self, sub: FreeRegion, sup: FreeRegion) -> bool {
        /*!
         * Determines whether two free regions have a subregion relationship
         * by walking the graph encoded in `free_region_map`.  Note that
         * it is possible that `sub != sup` and `sub <= sup` and `sup <= sub`
         * (that is, the user can give two different names to the same lifetime).
         */

        if sub == sup {
            return true;
        }

        // Do a little breadth-first-search here.  The `queue` list
        // doubles as a way to detect if we've seen a particular FR
        // before.  Note that we expect this graph to be an *extremely
        // shallow* tree.
        let mut queue = ~[sub];
        let mut i = 0;
        while i < queue.len() {
            let free_region_map = self.free_region_map.borrow();
            match free_region_map.get().find(&queue[i]) {
                Some(parents) => {
                    for parent in parents.iter() {
                        if *parent == sup {
                            return true;
                        }

                        if !queue.iter().any(|x| x == parent) {
                            queue.push(*parent);
                        }
                    }
                }
                None => {}
            }
            i += 1;
        }
        return false;
    }

    pub fn is_subregion_of(&self,
                           sub_region: ty::Region,
                           super_region: ty::Region)
                           -> bool {
        /*!
         * Determines whether one region is a subregion of another.  This is
         * intended to run *after inference* and sadly the logic is somewhat
         * duplicated with the code in infer.rs.
         */

        debug!("is_subregion_of(sub_region={:?}, super_region={:?})",
               sub_region, super_region);

        sub_region == super_region || {
            match (sub_region, super_region) {
                (_, ty::ReStatic) => {
                    true
                }

                (ty::ReScope(sub_scope), ty::ReScope(super_scope)) => {
                    self.is_subscope_of(sub_scope, super_scope)
                }

                (ty::ReScope(sub_scope), ty::ReFree(ref fr)) => {
                    self.is_subscope_of(sub_scope, fr.scope_id)
                }

                (ty::ReFree(sub_fr), ty::ReFree(super_fr)) => {
                    self.sub_free_region(sub_fr, super_fr)
                }

                _ => {
                    false
                }
            }
        }
    }

    pub fn nearest_common_ancestor(&self,
                                   scope_a: ast::NodeId,
                                   scope_b: ast::NodeId)
                                   -> Option<ast::NodeId> {
        /*!
         * Finds the nearest common ancestor (if any) of two scopes.  That
         * is, finds the smallest scope which is greater than or equal to
         * both `scope_a` and `scope_b`.
         */

        if scope_a == scope_b { return Some(scope_a); }

        let a_ancestors = ancestors_of(self, scope_a);
        let b_ancestors = ancestors_of(self, scope_b);
        let mut a_index = a_ancestors.len() - 1u;
        let mut b_index = b_ancestors.len() - 1u;

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

        fn ancestors_of(this: &RegionMaps, scope: ast::NodeId)
            -> ~[ast::NodeId]
        {
            // debug!("ancestors_of(scope={})", scope);
            let mut result = ~[scope];
            let mut scope = scope;
            loop {
                let scope_map = this.scope_map.borrow();
                match scope_map.get().find(&scope) {
                    None => return result,
                    Some(&superscope) => {
                        result.push(superscope);
                        scope = superscope;
                    }
                }
                // debug!("ancestors_of_loop(scope={})", scope);
            }
        }
    }
}

/// Records the current parent (if any) as the parent of `child_id`.
fn record_superlifetime(visitor: &mut RegionResolutionVisitor,
                        cx: Context,
                        child_id: ast::NodeId,
                        _sp: Span) {
    for &parent_id in cx.parent.iter() {
        visitor.region_maps.record_encl_scope(child_id, parent_id);
    }
}

/// Records the lifetime of a local variable as `cx.var_parent`
fn record_var_lifetime(visitor: &mut RegionResolutionVisitor,
                       cx: Context,
                       var_id: ast::NodeId,
                       _sp: Span) {
    match cx.var_parent {
        Some(parent_id) => {
            visitor.region_maps.record_var_scope(var_id, parent_id);
        }
        None => {
            // this can happen in extern fn declarations like
            //
            // extern fn isalnum(c: c_int) -> c_int
        }
    }
}

fn resolve_block(visitor: &mut RegionResolutionVisitor,
                 blk: &ast::Block,
                 cx: Context) {
    debug!("resolve_block(blk.id={})", blk.id);

    // Record the parent of this block.
    record_superlifetime(visitor, cx, blk.id, blk.span);

    // We treat the tail expression in the block (if any) somewhat
    // differently from the statements. The issue has to do with
    // temporary lifetimes. If the user writes:
    //
    //   {
    //     ... (&foo()) ...
    //   }
    //

    let subcx = Context {var_parent: Some(blk.id), parent: Some(blk.id)};
    visit::walk_block(visitor, blk, subcx);
}

fn resolve_arm(visitor: &mut RegionResolutionVisitor,
               arm: &ast::Arm,
               cx: Context) {
    visitor.region_maps.mark_as_terminating_scope(arm.body.id);

    match arm.guard {
        Some(expr) => {
            visitor.region_maps.mark_as_terminating_scope(expr.id);
        }
        None => { }
    }

    visit::walk_arm(visitor, arm, cx);
}

fn resolve_pat(visitor: &mut RegionResolutionVisitor,
               pat: &ast::Pat,
               cx: Context) {
    record_superlifetime(visitor, cx, pat.id, pat.span);

    // If this is a binding (or maybe a binding, I'm too lazy to check
    // the def map) then record the lifetime of that binding.
    match pat.node {
        ast::PatIdent(..) => {
            record_var_lifetime(visitor, cx, pat.id, pat.span);
        }
        _ => { }
    }

    visit::walk_pat(visitor, pat, cx);
}

fn resolve_stmt(visitor: &mut RegionResolutionVisitor,
                stmt: &ast::Stmt,
                cx: Context) {
    let stmt_id = stmt_id(stmt);
    debug!("resolve_stmt(stmt.id={})", stmt_id);

    visitor.region_maps.mark_as_terminating_scope(stmt_id);
    record_superlifetime(visitor, cx, stmt_id, stmt.span);

    let subcx = Context {parent: Some(stmt_id), ..cx};
    visit::walk_stmt(visitor, stmt, subcx);
}

fn resolve_expr(visitor: &mut RegionResolutionVisitor,
                expr: &ast::Expr,
                cx: Context) {
    debug!("resolve_expr(expr.id={})", expr.id);

    record_superlifetime(visitor, cx, expr.id, expr.span);

    let mut new_cx = cx;
    new_cx.parent = Some(expr.id);
    match expr.node {
        // Conditional or repeating scopes are always terminating
        // scopes, meaning that temporaries cannot outlive them.
        // This ensures fixed size stacks.

        ast::ExprBinary(_, ast::BiAnd, _, r) |
        ast::ExprBinary(_, ast::BiOr, _, r) => {
            // For shortcircuiting operators, mark the RHS as a terminating
            // scope since it only executes conditionally.
            visitor.region_maps.mark_as_terminating_scope(r.id);
        }

        ast::ExprIf(_, then, Some(otherwise)) => {
            visitor.region_maps.mark_as_terminating_scope(then.id);
            visitor.region_maps.mark_as_terminating_scope(otherwise.id);
        }

        ast::ExprIf(_, then, None) => {
            visitor.region_maps.mark_as_terminating_scope(then.id);
        }

        ast::ExprLoop(body, _) |
        ast::ExprWhile(_, body) => {
            visitor.region_maps.mark_as_terminating_scope(body.id);
        }

        ast::ExprMatch(..) => {
            new_cx.var_parent = Some(expr.id);
        }

        ast::ExprAssignOp(..) | ast::ExprIndex(..) |
        ast::ExprUnary(..) | ast::ExprCall(..) | ast::ExprMethodCall(..) => {
            // FIXME(#6268) Nested method calls
            //
            // The lifetimes for a call or method call look as follows:
            //
            // call.id
            // - arg0.id
            // - ...
            // - argN.id
            // - call.callee_id
            //
            // The idea is that call.callee_id represents *the time when
            // the invoked function is actually running* and call.id
            // represents *the time to prepare the arguments and make the
            // call*.  See the section "Borrows in Calls" borrowck/doc.rs
            // for an extended explanantion of why this distinction is
            // important.
            //
            // record_superlifetime(new_cx, expr.callee_id);
        }

        _ => {}
    };


    visit::walk_expr(visitor, expr, new_cx);
}

fn resolve_local(visitor: &mut RegionResolutionVisitor,
                 local: &ast::Local,
                 cx: Context) {
    debug!("resolve_local(local.id={},local.init={})",
           local.id,local.init.is_some());

    let blk_id = match cx.var_parent {
        Some(id) => id,
        None => {
            visitor.sess.span_bug(
                local.span,
                "local without enclosing block");
        }
    };

    // For convenience in trans, associate with the local-id the var
    // scope that will be used for any bindings declared in this
    // pattern.
    visitor.region_maps.record_var_scope(local.id, blk_id);

    // As an exception to the normal rules governing temporary
    // lifetimes, initializers in a let have a temporary lifetime
    // of the enclosing block. This means that e.g. a program
    // like the following is legal:
    //
    //     let ref x = HashMap::new();
    //
    // Because the hash map will be freed in the enclosing block.
    //
    // We express the rules more formally based on 3 grammars (defined
    // fully in the helpers below that implement them):
    //
    // 1. `E&`, which matches expressions like `&<rvalue>` that
    //    own a pointer into the stack.
    //
    // 2. `P&`, which matches patterns like `ref x` or `(ref x, ref
    //    y)` that produce ref bindings into the value they are
    //    matched against or something (at least partially) owned by
    //    the value they are matched against. (By partially owned,
    //    I mean that creating a binding into a ref-counted or managed value
    //    would still count.)
    //
    // 3. `ET`, which matches both rvalues like `foo()` as well as lvalues
    //    based on rvalues like `foo().x[2].y`.
    //
    // A subexpression `<rvalue>` that appears in a let initializer
    // `let pat [: ty] = expr` has an extended temporary lifetime if
    // any of the following conditions are met:
    //
    // A. `pat` matches `P&` and `expr` matches `ET`
    //    (covers cases where `pat` creates ref bindings into an rvalue
    //     produced by `expr`)
    // B. `ty` is a borrowed pointer and `expr` matches `ET`
    //    (covers cases where coercion creates a borrow)
    // C. `expr` matches `E&`
    //    (covers cases `expr` borrows an rvalue that is then assigned
    //     to memory (at least partially) owned by the binding)
    //
    // Here are some examples hopefully giving an intution where each
    // rule comes into play and why:
    //
    // Rule A. `let (ref x, ref y) = (foo().x, 44)`. The rvalue `(22, 44)`
    // would have an extended lifetime, but not `foo()`.
    //
    // Rule B. `let x: &[...] = [foo().x]`. The rvalue `[foo().x]`
    // would have an extended lifetime, but not `foo()`.
    //
    // Rule C. `let x = &foo().x`. The rvalue ``foo()` would have extended
    // lifetime.
    //
    // In some cases, multiple rules may apply (though not to the same
    // rvalue). For example:
    //
    //     let ref x = [&a(), &b()];
    //
    // Here, the expression `[...]` has an extended lifetime due to rule
    // A, but the inner rvalues `a()` and `b()` have an extended lifetime
    // due to rule C.
    //
    // FIXME(#6308) -- Note that `[]` patterns work more smoothly post-DST.

    match local.init {
        Some(expr) => {
            record_rvalue_scope_if_borrow_expr(visitor, expr, blk_id);

            if is_binding_pat(local.pat) || is_borrowed_ty(local.ty) {
                record_rvalue_scope(visitor, expr, blk_id);
            }
        }

        None => { }
    }

    visit::walk_local(visitor, local, cx);

    fn is_binding_pat(pat: &ast::Pat) -> bool {
        /*!
         * True if `pat` match the `P&` nonterminal:
         *
         *     P& = ref X
         *        | StructName { ..., P&, ... }
         *        | VariantName(..., P&, ...)
         *        | [ ..., P&, ... ]
         *        | ( ..., P&, ... )
         *        | ~P&
         *        | box P&
         */

        match pat.node {
            ast::PatIdent(ast::BindByRef(_), _, _) => true,

            ast::PatStruct(_, ref field_pats, _) => {
                field_pats.iter().any(|fp| is_binding_pat(fp.pat))
            }

            ast::PatVec(ref pats1, ref pats2, ref pats3) => {
                pats1.iter().any(|&p| is_binding_pat(p)) ||
                pats2.iter().any(|&p| is_binding_pat(p)) ||
                pats3.iter().any(|&p| is_binding_pat(p))
            }

            ast::PatEnum(_, Some(ref subpats)) |
            ast::PatTup(ref subpats) => {
                subpats.iter().any(|&p| is_binding_pat(p))
            }

            ast::PatUniq(subpat) => {
                is_binding_pat(subpat)
            }

            _ => false,
        }
    }

    fn is_borrowed_ty(ty: &ast::Ty) -> bool {
        /*!
         * True if `ty` is a borrowed pointer type
         * like `&int` or `&[...]`.
         */

        match ty.node {
            ast::TyRptr(..) => true,
            _ => false
        }
    }

    fn record_rvalue_scope_if_borrow_expr(visitor: &mut RegionResolutionVisitor,
                                          expr: &ast::Expr,
                                          blk_id: ast::NodeId) {
        /*!
         * If `expr` matches the `E&` grammar, then records an extended
         * rvalue scope as appropriate:
         *
         *     E& = & ET
         *        | StructName { ..., f: E&, ... }
         *        | [ ..., E&, ... ]
         *        | ( ..., E&, ... )
         *        | {...; E&}
         *        | ~E&
         *        | E& as ...
         *        | ( E& )
         */

        match expr.node {
            ast::ExprAddrOf(_, subexpr) => {
                record_rvalue_scope_if_borrow_expr(visitor, subexpr, blk_id);
                record_rvalue_scope(visitor, subexpr, blk_id);
            }
            ast::ExprStruct(_, ref fields, _) => {
                for field in fields.iter() {
                    record_rvalue_scope_if_borrow_expr(
                        visitor, field.expr, blk_id);
                }
            }
            ast::ExprVstore(subexpr, _) => {
                visitor.region_maps.record_rvalue_scope(subexpr.id, blk_id);
                record_rvalue_scope_if_borrow_expr(visitor, subexpr, blk_id);
            }
            ast::ExprVec(ref subexprs, _) |
            ast::ExprTup(ref subexprs) => {
                for &subexpr in subexprs.iter() {
                    record_rvalue_scope_if_borrow_expr(
                        visitor, subexpr, blk_id);
                }
            }
            ast::ExprUnary(_, ast::UnUniq, subexpr) => {
                record_rvalue_scope_if_borrow_expr(visitor, subexpr, blk_id);
            }
            ast::ExprCast(subexpr, _) |
            ast::ExprParen(subexpr) => {
                record_rvalue_scope_if_borrow_expr(visitor, subexpr, blk_id)
            }
            ast::ExprBlock(ref block) => {
                match block.expr {
                    Some(subexpr) => {
                        record_rvalue_scope_if_borrow_expr(
                            visitor, subexpr, blk_id);
                    }
                    None => { }
                }
            }
            _ => {
            }
        }
    }

    fn record_rvalue_scope<'a>(visitor: &mut RegionResolutionVisitor,
                               expr: &'a ast::Expr,
                               blk_id: ast::NodeId) {
        /*!
         * Applied to an expression `expr` if `expr` -- or something
         * owned or partially owned by `expr` -- is going to be
         * indirectly referenced by a variable in a let statement. In
         * that case, the "temporary lifetime" or `expr` is extended
         * to be the block enclosing the `let` statement.
         *
         * More formally, if `expr` matches the grammar `ET`, record
         * the rvalue scope of the matching `<rvalue>` as `blk_id`:
         *
         *     ET = *ET
         *        | ET[...]
         *        | ET.f
         *        | (ET)
         *        | <rvalue>
         *
         * Note: ET is intended to match "rvalues or
         * lvalues based on rvalues".
         */

        let mut expr = expr;
        loop {
            // Note: give all the expressions matching `ET` with the
            // extended temporary lifetime, not just the innermost rvalue,
            // because in trans if we must compile e.g. `*rvalue()`
            // into a temporary, we request the temporary scope of the
            // outer expression.
            visitor.region_maps.record_rvalue_scope(expr.id, blk_id);

            match expr.node {
                ast::ExprAddrOf(_, ref subexpr) |
                ast::ExprUnary(_, ast::UnDeref, ref subexpr) |
                ast::ExprField(ref subexpr, _, _) |
                ast::ExprIndex(_, ref subexpr, _) |
                ast::ExprParen(ref subexpr) => {
                    let subexpr: &'a @Expr = subexpr; // FIXME(#11586)
                    expr = &**subexpr;
                }
                _ => {
                    return;
                }
            }
        }
    }
}

fn resolve_item(visitor: &mut RegionResolutionVisitor,
                item: &ast::Item,
                cx: Context) {
    // Items create a new outer block scope as far as we're concerned.
    let new_cx = Context {var_parent: None, parent: None, ..cx};
    visit::walk_item(visitor, item, new_cx);
}

fn resolve_fn(visitor: &mut RegionResolutionVisitor,
              fk: &FnKind,
              decl: &ast::FnDecl,
              body: &ast::Block,
              sp: Span,
              id: ast::NodeId,
              cx: Context) {
    debug!("region::resolve_fn(id={}, \
                               span={:?}, \
                               body.id={}, \
                               cx.parent={})",
           id,
           visitor.sess.codemap.span_to_str(sp),
           body.id,
           cx.parent);

    visitor.region_maps.mark_as_terminating_scope(body.id);

    // The arguments and `self` are parented to the body of the fn.
    let decl_cx = Context {parent: Some(body.id),
                           var_parent: Some(body.id)};
    visit::walk_fn_decl(visitor, decl, decl_cx);

    // The body of the fn itself is either a root scope (top-level fn)
    // or it continues with the inherited scope (closures).
    let body_cx = match *fk {
        visit::FkItemFn(..) | visit::FkMethod(..) => {
            Context {parent: None, var_parent: None, ..cx}
        }
        visit::FkFnBlock(..) => cx
    };
    visitor.visit_block(body, body_cx);
}

impl<'a> Visitor<Context> for RegionResolutionVisitor<'a> {

    fn visit_block(&mut self, b: &Block, cx: Context) {
        resolve_block(self, b, cx);
    }

    fn visit_item(&mut self, i: &Item, cx: Context) {
        resolve_item(self, i, cx);
    }

    fn visit_fn(&mut self, fk: &FnKind, fd: &FnDecl,
                b: &Block, s: Span, n: NodeId, cx: Context) {
        resolve_fn(self, fk, fd, b, s, n, cx);
    }
    fn visit_arm(&mut self, a: &Arm, cx: Context) {
        resolve_arm(self, a, cx);
    }
    fn visit_pat(&mut self, p: &Pat, cx: Context) {
        resolve_pat(self, p, cx);
    }
    fn visit_stmt(&mut self, s: &Stmt, cx: Context) {
        resolve_stmt(self, s, cx);
    }
    fn visit_expr(&mut self, ex: &Expr, cx: Context) {
        resolve_expr(self, ex, cx);
    }
    fn visit_local(&mut self, l: &Local, cx: Context) {
        resolve_local(self, l, cx);
    }
}

pub fn resolve_crate(sess: Session, crate: &ast::Crate) -> RegionMaps {
    let maps = RegionMaps {
        scope_map: RefCell::new(HashMap::new()),
        var_map: RefCell::new(HashMap::new()),
        free_region_map: RefCell::new(HashMap::new()),
        rvalue_scopes: RefCell::new(HashMap::new()),
        terminating_scopes: RefCell::new(HashSet::new()),
    };
    {
        let mut visitor = RegionResolutionVisitor {
            sess: sess,
            region_maps: &maps
        };
        let cx = Context { parent: None, var_parent: None };
        visit::walk_crate(&mut visitor, crate, cx);
    }
    return maps;
}

pub fn resolve_inlined_item(sess: Session,
                            region_maps: &RegionMaps,
                            item: &ast::InlinedItem) {
    let cx = Context {parent: None,
                      var_parent: None};
    let mut visitor = RegionResolutionVisitor {
        sess: sess,
        region_maps: region_maps,
    };
    visit::walk_inlined_item(&mut visitor, item, cx);
}

