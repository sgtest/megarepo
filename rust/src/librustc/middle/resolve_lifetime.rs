// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Name resolution for lifetimes.
//!
//! Name resolution for lifetimes follows MUCH simpler rules than the
//! full resolve. For example, lifetime names are never exported or
//! used between functions, and they operate in a purely top-down
//! way. Therefore we break lifetime name resolution into a separate pass.

pub use self::DefRegion::*;
use self::ScopeChain::*;

use dep_graph::DepNode;
use hir::map::Map;
use session::Session;
use hir::def::{Def, DefMap};
use hir::def_id::DefId;
use middle::region;
use ty::subst;
use ty;
use std::fmt;
use std::mem::replace;
use syntax::ast;
use syntax::parse::token::keywords;
use syntax_pos::Span;
use util::nodemap::NodeMap;

use rustc_data_structures::fnv::FnvHashSet;
use hir;
use hir::print::lifetime_to_string;
use hir::intravisit::{self, Visitor, FnKind};

#[derive(Clone, Copy, PartialEq, Eq, Hash, RustcEncodable, RustcDecodable, Debug)]
pub enum DefRegion {
    DefStaticRegion,
    DefEarlyBoundRegion(/* space */ subst::ParamSpace,
                        /* index */ u32,
                        /* lifetime decl */ ast::NodeId),
    DefLateBoundRegion(ty::DebruijnIndex,
                       /* lifetime decl */ ast::NodeId),
    DefFreeRegion(region::CallSiteScopeData,
                  /* lifetime decl */ ast::NodeId),
}

// Maps the id of each lifetime reference to the lifetime decl
// that it corresponds to.
pub struct NamedRegionMap {
    // maps from every use of a named (not anonymous) lifetime to a
    // `DefRegion` describing how that region is bound
    pub defs: NodeMap<DefRegion>,

    // the set of lifetime def ids that are late-bound; late-bound ids
    // are named regions appearing in fn arguments that do not appear
    // in where-clauses
    pub late_bound: NodeMap<ty::Issue32330>,
}

struct LifetimeContext<'a, 'tcx: 'a> {
    sess: &'a Session,
    hir_map: &'a Map<'tcx>,
    map: &'a mut NamedRegionMap,
    scope: Scope<'a>,
    def_map: &'a DefMap,
    // Deep breath. Our representation for poly trait refs contains a single
    // binder and thus we only allow a single level of quantification. However,
    // the syntax of Rust permits quantification in two places, e.g., `T: for <'a> Foo<'a>`
    // and `for <'a, 'b> &'b T: Foo<'a>`. In order to get the de Bruijn indices
    // correct when representing these constraints, we should only introduce one
    // scope. However, we want to support both locations for the quantifier and
    // during lifetime resolution we want precise information (so we can't
    // desugar in an earlier phase).

    // SO, if we encounter a quantifier at the outer scope, we set
    // trait_ref_hack to true (and introduce a scope), and then if we encounter
    // a quantifier at the inner scope, we error. If trait_ref_hack is false,
    // then we introduce the scope at the inner quantifier.

    // I'm sorry.
    trait_ref_hack: bool,

    // List of labels in the function/method currently under analysis.
    labels_in_fn: Vec<(ast::Name, Span)>,
}

enum ScopeChain<'a> {
    /// EarlyScope(i, ['a, 'b, ...], s) extends s with early-bound
    /// lifetimes, assigning indexes 'a => i, 'b => i+1, ... etc.
    EarlyScope(subst::ParamSpace, &'a [hir::LifetimeDef], Scope<'a>),
    /// LateScope(['a, 'b, ...], s) extends s with late-bound
    /// lifetimes introduced by the declaration binder_id.
    LateScope(&'a [hir::LifetimeDef], Scope<'a>),

    /// lifetimes introduced by a fn are scoped to the call-site for that fn.
    FnScope { fn_id: ast::NodeId, body_id: ast::NodeId, s: Scope<'a> },
    RootScope
}

type Scope<'a> = &'a ScopeChain<'a>;

static ROOT_SCOPE: ScopeChain<'static> = RootScope;

pub fn krate(sess: &Session,
             hir_map: &Map,
             def_map: &DefMap)
             -> Result<NamedRegionMap, usize> {
    let _task = hir_map.dep_graph.in_task(DepNode::ResolveLifetimes);
    let krate = hir_map.krate();
    let mut map = NamedRegionMap {
        defs: NodeMap(),
        late_bound: NodeMap(),
    };
    sess.track_errors(|| {
        krate.visit_all_items(&mut LifetimeContext {
            sess: sess,
            hir_map: hir_map,
            map: &mut map,
            scope: &ROOT_SCOPE,
            def_map: def_map,
            trait_ref_hack: false,
            labels_in_fn: vec![],
        });
    })?;
    Ok(map)
}

impl<'a, 'tcx, 'v> Visitor<'v> for LifetimeContext<'a, 'tcx> {
    fn visit_item(&mut self, item: &hir::Item) {
        assert!(self.labels_in_fn.is_empty());

        // Items always introduce a new root scope
        self.with(RootScope, |_, this| {
            match item.node {
                hir::ItemFn(..) => {
                    // Fn lifetimes get added in visit_fn below:
                    intravisit::walk_item(this, item);
                }
                hir::ItemExternCrate(_) |
                hir::ItemUse(_) |
                hir::ItemMod(..) |
                hir::ItemDefaultImpl(..) |
                hir::ItemForeignMod(..) |
                hir::ItemStatic(..) |
                hir::ItemConst(..) => {
                    // These sorts of items have no lifetime parameters at all.
                    intravisit::walk_item(this, item);
                }
                hir::ItemTy(_, ref generics) |
                hir::ItemEnum(_, ref generics) |
                hir::ItemStruct(_, ref generics) |
                hir::ItemTrait(_, ref generics, _, _) |
                hir::ItemImpl(_, _, ref generics, _, _, _) => {
                    // These kinds of items have only early bound lifetime parameters.
                    let lifetimes = &generics.lifetimes;
                    let early_scope = EarlyScope(subst::TypeSpace, lifetimes, &ROOT_SCOPE);
                    this.with(early_scope, |old_scope, this| {
                        this.check_lifetime_defs(old_scope, lifetimes);
                        intravisit::walk_item(this, item);
                    });
                }
            }
        });

        // Done traversing the item; remove any labels it created
        self.labels_in_fn.truncate(0);
    }

    fn visit_foreign_item(&mut self, item: &hir::ForeignItem) {
        // Items save/restore the set of labels. This way inner items
        // can freely reuse names, be they loop labels or lifetimes.
        let saved = replace(&mut self.labels_in_fn, vec![]);

        // Items always introduce a new root scope
        self.with(RootScope, |_, this| {
            match item.node {
                hir::ForeignItemFn(ref decl, ref generics) => {
                    this.visit_early_late(item.id,
                                          subst::FnSpace,
                                          decl,
                                          generics,
                                          |this| {
                        intravisit::walk_foreign_item(this, item);
                    })
                }
                hir::ForeignItemStatic(..) => {
                    intravisit::walk_foreign_item(this, item);
                }
            }
        });

        // Done traversing the item; restore saved set of labels.
        replace(&mut self.labels_in_fn, saved);
    }

    fn visit_fn(&mut self, fk: FnKind<'v>, decl: &'v hir::FnDecl,
                b: &'v hir::Block, s: Span, fn_id: ast::NodeId) {
        match fk {
            FnKind::ItemFn(_, generics, _, _, _, _, _) => {
                self.visit_early_late(fn_id, subst::FnSpace, decl, generics, |this| {
                    this.add_scope_and_walk_fn(fk, decl, b, s, fn_id)
                })
            }
            FnKind::Method(_, sig, _, _) => {
                self.visit_early_late(
                    fn_id,
                    subst::FnSpace,
                    decl,
                    &sig.generics,
                    |this| this.add_scope_and_walk_fn(fk, decl, b, s, fn_id));
            }
            FnKind::Closure(_) => {
                // Closures have their own set of labels, save labels just
                // like for foreign items above.
                let saved = replace(&mut self.labels_in_fn, vec![]);
                let result = self.add_scope_and_walk_fn(fk, decl, b, s, fn_id);
                replace(&mut self.labels_in_fn, saved);
                result
            }
        }
    }

    fn visit_ty(&mut self, ty: &hir::Ty) {
        match ty.node {
            hir::TyBareFn(ref c) => {
                self.with(LateScope(&c.lifetimes, self.scope), |old_scope, this| {
                    // a bare fn has no bounds, so everything
                    // contained within is scoped within its binder.
                    this.check_lifetime_defs(old_scope, &c.lifetimes);
                    intravisit::walk_ty(this, ty);
                });
            }
            hir::TyPath(None, ref path) => {
                // if this path references a trait, then this will resolve to
                // a trait ref, which introduces a binding scope.
                match self.def_map.get(&ty.id).map(|d| (d.base_def, d.depth)) {
                    Some((Def::Trait(..), 0)) => {
                        self.with(LateScope(&[], self.scope), |_, this| {
                            this.visit_path(path, ty.id);
                        });
                    }
                    _ => {
                        intravisit::walk_ty(self, ty);
                    }
                }
            }
            _ => {
                intravisit::walk_ty(self, ty)
            }
        }
    }

    fn visit_trait_item(&mut self, trait_item: &hir::TraitItem) {
        // We reset the labels on every trait item, so that different
        // methods in an impl can reuse label names.
        let saved = replace(&mut self.labels_in_fn, vec![]);

        if let hir::MethodTraitItem(ref sig, None) = trait_item.node {
            self.visit_early_late(
                trait_item.id, subst::FnSpace,
                &sig.decl, &sig.generics,
                |this| intravisit::walk_trait_item(this, trait_item))
        } else {
            intravisit::walk_trait_item(self, trait_item);
        }

        replace(&mut self.labels_in_fn, saved);
    }

    fn visit_lifetime(&mut self, lifetime_ref: &hir::Lifetime) {
        if lifetime_ref.name == keywords::StaticLifetime.name() {
            self.insert_lifetime(lifetime_ref, DefStaticRegion);
            return;
        }
        self.resolve_lifetime_ref(lifetime_ref);
    }

    fn visit_generics(&mut self, generics: &hir::Generics) {
        for ty_param in generics.ty_params.iter() {
            walk_list!(self, visit_ty_param_bound, &ty_param.bounds);
            match ty_param.default {
                Some(ref ty) => self.visit_ty(&ty),
                None => {}
            }
        }
        for predicate in &generics.where_clause.predicates {
            match predicate {
                &hir::WherePredicate::BoundPredicate(hir::WhereBoundPredicate{ ref bounded_ty,
                                                                               ref bounds,
                                                                               ref bound_lifetimes,
                                                                               .. }) => {
                    if !bound_lifetimes.is_empty() {
                        self.trait_ref_hack = true;
                        let result = self.with(LateScope(bound_lifetimes, self.scope),
                                               |old_scope, this| {
                            this.check_lifetime_defs(old_scope, bound_lifetimes);
                            this.visit_ty(&bounded_ty);
                            walk_list!(this, visit_ty_param_bound, bounds);
                        });
                        self.trait_ref_hack = false;
                        result
                    } else {
                        self.visit_ty(&bounded_ty);
                        walk_list!(self, visit_ty_param_bound, bounds);
                    }
                }
                &hir::WherePredicate::RegionPredicate(hir::WhereRegionPredicate{ref lifetime,
                                                                                ref bounds,
                                                                                .. }) => {

                    self.visit_lifetime(lifetime);
                    for bound in bounds {
                        self.visit_lifetime(bound);
                    }
                }
                &hir::WherePredicate::EqPredicate(hir::WhereEqPredicate{ id,
                                                                         ref path,
                                                                         ref ty,
                                                                         .. }) => {
                    self.visit_path(path, id);
                    self.visit_ty(&ty);
                }
            }
        }
    }

    fn visit_poly_trait_ref(&mut self,
                            trait_ref: &hir::PolyTraitRef,
                            _modifier: &hir::TraitBoundModifier) {
        debug!("visit_poly_trait_ref trait_ref={:?}", trait_ref);

        if !self.trait_ref_hack || !trait_ref.bound_lifetimes.is_empty() {
            if self.trait_ref_hack {
                println!("{:?}", trait_ref.span);
                span_err!(self.sess, trait_ref.span, E0316,
                          "nested quantification of lifetimes");
            }
            self.with(LateScope(&trait_ref.bound_lifetimes, self.scope), |old_scope, this| {
                this.check_lifetime_defs(old_scope, &trait_ref.bound_lifetimes);
                for lifetime in &trait_ref.bound_lifetimes {
                    this.visit_lifetime_def(lifetime);
                }
                intravisit::walk_path(this, &trait_ref.trait_ref.path)
            })
        } else {
            self.visit_trait_ref(&trait_ref.trait_ref)
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
enum ShadowKind { Label, Lifetime }
struct Original { kind: ShadowKind, span: Span }
struct Shadower { kind: ShadowKind, span: Span }

fn original_label(span: Span) -> Original {
    Original { kind: ShadowKind::Label, span: span }
}
fn shadower_label(span: Span) -> Shadower {
    Shadower { kind: ShadowKind::Label, span: span }
}
fn original_lifetime(l: &hir::Lifetime) -> Original {
    Original { kind: ShadowKind::Lifetime, span: l.span }
}
fn shadower_lifetime(l: &hir::Lifetime) -> Shadower {
    Shadower { kind: ShadowKind::Lifetime, span: l.span }
}

impl ShadowKind {
    fn desc(&self) -> &'static str {
        match *self {
            ShadowKind::Label => "label",
            ShadowKind::Lifetime => "lifetime",
        }
    }
}

fn signal_shadowing_problem(sess: &Session, name: ast::Name, orig: Original, shadower: Shadower) {
    let mut err = if let (ShadowKind::Lifetime, ShadowKind::Lifetime) = (orig.kind, shadower.kind) {
        // lifetime/lifetime shadowing is an error
        struct_span_err!(sess, shadower.span, E0496,
                         "{} name `{}` shadows a \
                          {} name that is already in scope",
                         shadower.kind.desc(), name, orig.kind.desc())
    } else {
        // shadowing involving a label is only a warning, due to issues with
        // labels and lifetimes not being macro-hygienic.
        sess.struct_span_warn(shadower.span,
                              &format!("{} name `{}` shadows a \
                                        {} name that is already in scope",
                                       shadower.kind.desc(), name, orig.kind.desc()))
    };
    err.span_note(orig.span,
                  &format!("shadowed {} `{}` declared here",
                           orig.kind.desc(), name));
    err.emit();
}

// Adds all labels in `b` to `ctxt.labels_in_fn`, signalling a warning
// if one of the label shadows a lifetime or another label.
fn extract_labels(ctxt: &mut LifetimeContext, b: &hir::Block) {
    struct GatherLabels<'a> {
        sess: &'a Session,
        scope: Scope<'a>,
        labels_in_fn: &'a mut Vec<(ast::Name, Span)>,
    }

    let mut gather = GatherLabels {
        sess: ctxt.sess,
        scope: ctxt.scope,
        labels_in_fn: &mut ctxt.labels_in_fn,
    };
    gather.visit_block(b);
    return;

    impl<'v, 'a> Visitor<'v> for GatherLabels<'a> {
        fn visit_expr(&mut self, ex: &'v hir::Expr) {
            // do not recurse into closures defined in the block
            // since they are treated as separate fns from the POV of
            // labels_in_fn
            if let hir::ExprClosure(..) = ex.node {
                return
            }
            if let Some((label, label_span)) = expression_label(ex) {
                for &(prior, prior_span) in &self.labels_in_fn[..] {
                    // FIXME (#24278): non-hygienic comparison
                    if label == prior {
                        signal_shadowing_problem(self.sess,
                                                 label,
                                                 original_label(prior_span),
                                                 shadower_label(label_span));
                    }
                }

                check_if_label_shadows_lifetime(self.sess,
                                                self.scope,
                                                label,
                                                label_span);

                self.labels_in_fn.push((label, label_span));
            }
            intravisit::walk_expr(self, ex)
        }

        fn visit_item(&mut self, _: &hir::Item) {
            // do not recurse into items defined in the block
        }
    }

    fn expression_label(ex: &hir::Expr) -> Option<(ast::Name, Span)> {
        match ex.node {
            hir::ExprWhile(_, _, Some(label)) |
            hir::ExprLoop(_, Some(label)) => Some((label.node, label.span)),
            _ => None,
        }
    }

    fn check_if_label_shadows_lifetime<'a>(sess: &'a Session,
                                           mut scope: Scope<'a>,
                                           label: ast::Name,
                                           label_span: Span) {
        loop {
            match *scope {
                FnScope { s, .. } => { scope = s; }
                RootScope => { return; }

                EarlyScope(_, lifetimes, s) |
                LateScope(lifetimes, s) => {
                    for lifetime_def in lifetimes {
                        // FIXME (#24278): non-hygienic comparison
                        if label == lifetime_def.lifetime.name {
                            signal_shadowing_problem(
                                sess,
                                label,
                                original_lifetime(&lifetime_def.lifetime),
                                shadower_label(label_span));
                            return;
                        }
                    }
                    scope = s;
                }
            }
        }
    }
}

impl<'a, 'tcx> LifetimeContext<'a, 'tcx> {
    fn add_scope_and_walk_fn<'b>(&mut self,
                                 fk: FnKind,
                                 fd: &hir::FnDecl,
                                 fb: &'b hir::Block,
                                 _span: Span,
                                 fn_id: ast::NodeId) {

        match fk {
            FnKind::ItemFn(_, generics, _, _, _, _, _) => {
                intravisit::walk_fn_decl(self, fd);
                self.visit_generics(generics);
            }
            FnKind::Method(_, sig, _, _) => {
                intravisit::walk_fn_decl(self, fd);
                self.visit_generics(&sig.generics);
            }
            FnKind::Closure(_) => {
                intravisit::walk_fn_decl(self, fd);
            }
        }

        // After inpsecting the decl, add all labels from the body to
        // `self.labels_in_fn`.
        extract_labels(self, fb);

        self.with(FnScope { fn_id: fn_id, body_id: fb.id, s: self.scope },
                  |_old_scope, this| this.visit_block(fb))
    }

    fn with<F>(&mut self, wrap_scope: ScopeChain, f: F) where
        F: FnOnce(Scope, &mut LifetimeContext),
    {
        let LifetimeContext {sess, hir_map, ref mut map, ..} = *self;
        let mut this = LifetimeContext {
            sess: sess,
            hir_map: hir_map,
            map: *map,
            scope: &wrap_scope,
            def_map: self.def_map,
            trait_ref_hack: self.trait_ref_hack,
            labels_in_fn: self.labels_in_fn.clone(),
        };
        debug!("entering scope {:?}", this.scope);
        f(self.scope, &mut this);
        debug!("exiting scope {:?}", this.scope);
    }

    /// Visits self by adding a scope and handling recursive walk over the contents with `walk`.
    ///
    /// Handles visiting fns and methods. These are a bit complicated because we must distinguish
    /// early- vs late-bound lifetime parameters. We do this by checking which lifetimes appear
    /// within type bounds; those are early bound lifetimes, and the rest are late bound.
    ///
    /// For example:
    ///
    ///    fn foo<'a,'b,'c,T:Trait<'b>>(...)
    ///
    /// Here `'a` and `'c` are late bound but `'b` is early bound. Note that early- and late-bound
    /// lifetimes may be interspersed together.
    ///
    /// If early bound lifetimes are present, we separate them into their own list (and likewise
    /// for late bound). They will be numbered sequentially, starting from the lowest index that is
    /// already in scope (for a fn item, that will be 0, but for a method it might not be). Late
    /// bound lifetimes are resolved by name and associated with a binder id (`binder_id`), so the
    /// ordering is not important there.
    fn visit_early_late<F>(&mut self,
                           fn_id: ast::NodeId,
                           early_space: subst::ParamSpace,
                           decl: &hir::FnDecl,
                           generics: &hir::Generics,
                           walk: F) where
        F: FnOnce(&mut LifetimeContext),
    {
        let fn_def_id = self.hir_map.local_def_id(fn_id);
        insert_late_bound_lifetimes(self.map,
                                    fn_def_id,
                                    decl,
                                    generics);

        let (late, early): (Vec<_>, _) =
            generics.lifetimes
                    .iter()
                    .cloned()
                    .partition(|l| self.map.late_bound.contains_key(&l.lifetime.id));

        let this = self;
        this.with(EarlyScope(early_space, &early, this.scope), move |old_scope, this| {
            this.with(LateScope(&late, this.scope), move |_, this| {
                this.check_lifetime_defs(old_scope, &generics.lifetimes);
                walk(this);
            });
        });
    }

    fn resolve_lifetime_ref(&mut self, lifetime_ref: &hir::Lifetime) {
        // Walk up the scope chain, tracking the number of fn scopes
        // that we pass through, until we find a lifetime with the
        // given name or we run out of scopes. If we encounter a code
        // block, then the lifetime is not bound but free, so switch
        // over to `resolve_free_lifetime_ref()` to complete the
        // search.
        let mut late_depth = 0;
        let mut scope = self.scope;
        loop {
            match *scope {
                FnScope {fn_id, body_id, s } => {
                    return self.resolve_free_lifetime_ref(
                        region::CallSiteScopeData { fn_id: fn_id, body_id: body_id },
                        lifetime_ref,
                        s);
                }

                RootScope => {
                    break;
                }

                EarlyScope(space, lifetimes, s) => {
                    match search_lifetimes(lifetimes, lifetime_ref) {
                        Some((index, lifetime_def)) => {
                            let decl_id = lifetime_def.id;
                            let def = DefEarlyBoundRegion(space, index, decl_id);
                            self.insert_lifetime(lifetime_ref, def);
                            return;
                        }
                        None => {
                            scope = s;
                        }
                    }
                }

                LateScope(lifetimes, s) => {
                    match search_lifetimes(lifetimes, lifetime_ref) {
                        Some((_index, lifetime_def)) => {
                            let decl_id = lifetime_def.id;
                            let debruijn = ty::DebruijnIndex::new(late_depth + 1);
                            let def = DefLateBoundRegion(debruijn, decl_id);
                            self.insert_lifetime(lifetime_ref, def);
                            return;
                        }

                        None => {
                            late_depth += 1;
                            scope = s;
                        }
                    }
                }
            }
        }

        self.unresolved_lifetime_ref(lifetime_ref);
    }

    fn resolve_free_lifetime_ref(&mut self,
                                 scope_data: region::CallSiteScopeData,
                                 lifetime_ref: &hir::Lifetime,
                                 scope: Scope) {
        debug!("resolve_free_lifetime_ref \
                scope_data: {:?} lifetime_ref: {:?} scope: {:?}",
               scope_data, lifetime_ref, scope);

        // Walk up the scope chain, tracking the outermost free scope,
        // until we encounter a scope that contains the named lifetime
        // or we run out of scopes.
        let mut scope_data = scope_data;
        let mut scope = scope;
        let mut search_result = None;
        loop {
            debug!("resolve_free_lifetime_ref \
                    scope_data: {:?} scope: {:?} search_result: {:?}",
                   scope_data, scope, search_result);
            match *scope {
                FnScope { fn_id, body_id, s } => {
                    scope_data = region::CallSiteScopeData {
                        fn_id: fn_id, body_id: body_id
                    };
                    scope = s;
                }

                RootScope => {
                    break;
                }

                EarlyScope(_, lifetimes, s) |
                LateScope(lifetimes, s) => {
                    search_result = search_lifetimes(lifetimes, lifetime_ref);
                    if search_result.is_some() {
                        break;
                    }
                    scope = s;
                }
            }
        }

        match search_result {
            Some((_depth, lifetime)) => {
                let def = DefFreeRegion(scope_data, lifetime.id);
                self.insert_lifetime(lifetime_ref, def);
            }

            None => {
                self.unresolved_lifetime_ref(lifetime_ref);
            }
        }

    }

    fn unresolved_lifetime_ref(&self, lifetime_ref: &hir::Lifetime) {
        span_err!(self.sess, lifetime_ref.span, E0261,
            "use of undeclared lifetime name `{}`",
                    lifetime_ref.name);
    }

    fn check_lifetime_defs(&mut self, old_scope: Scope, lifetimes: &[hir::LifetimeDef]) {
        for i in 0..lifetimes.len() {
            let lifetime_i = &lifetimes[i];

            for lifetime in lifetimes {
                if lifetime.lifetime.name == keywords::StaticLifetime.name() {
                    span_err!(self.sess, lifetime.lifetime.span, E0262,
                        "invalid lifetime parameter name: `{}`", lifetime.lifetime.name);
                }
            }

            // It is a hard error to shadow a lifetime within the same scope.
            for j in i + 1..lifetimes.len() {
                let lifetime_j = &lifetimes[j];

                if lifetime_i.lifetime.name == lifetime_j.lifetime.name {
                    span_err!(self.sess, lifetime_j.lifetime.span, E0263,
                        "lifetime name `{}` declared twice in \
                                the same scope",
                                lifetime_j.lifetime.name);
                }
            }

            // It is a soft error to shadow a lifetime within a parent scope.
            self.check_lifetime_def_for_shadowing(old_scope, &lifetime_i.lifetime);

            for bound in &lifetime_i.bounds {
                self.resolve_lifetime_ref(bound);
            }
        }
    }

    fn check_lifetime_def_for_shadowing(&self,
                                        mut old_scope: Scope,
                                        lifetime: &hir::Lifetime)
    {
        for &(label, label_span) in &self.labels_in_fn {
            // FIXME (#24278): non-hygienic comparison
            if lifetime.name == label {
                signal_shadowing_problem(self.sess,
                                         lifetime.name,
                                         original_label(label_span),
                                         shadower_lifetime(&lifetime));
                return;
            }
        }

        loop {
            match *old_scope {
                FnScope { s, .. } => {
                    old_scope = s;
                }

                RootScope => {
                    return;
                }

                EarlyScope(_, lifetimes, s) |
                LateScope(lifetimes, s) => {
                    if let Some((_, lifetime_def)) = search_lifetimes(lifetimes, lifetime) {
                        signal_shadowing_problem(
                            self.sess,
                            lifetime.name,
                            original_lifetime(&lifetime_def),
                            shadower_lifetime(&lifetime));
                        return;
                    }

                    old_scope = s;
                }
            }
        }
    }

    fn insert_lifetime(&mut self,
                       lifetime_ref: &hir::Lifetime,
                       def: DefRegion) {
        if lifetime_ref.id == ast::DUMMY_NODE_ID {
            span_bug!(lifetime_ref.span,
                      "lifetime reference not renumbered, \
                       probably a bug in syntax::fold");
        }

        debug!("lifetime_ref={:?} id={:?} resolved to {:?} span={:?}",
               lifetime_to_string(lifetime_ref),
               lifetime_ref.id,
               def,
               self.sess.codemap().span_to_string(lifetime_ref.span));
        self.map.defs.insert(lifetime_ref.id, def);
    }
}

fn search_lifetimes<'a>(lifetimes: &'a [hir::LifetimeDef],
                    lifetime_ref: &hir::Lifetime)
                    -> Option<(u32, &'a hir::Lifetime)> {
    for (i, lifetime_decl) in lifetimes.iter().enumerate() {
        if lifetime_decl.lifetime.name == lifetime_ref.name {
            return Some((i as u32, &lifetime_decl.lifetime));
        }
    }
    return None;
}

///////////////////////////////////////////////////////////////////////////

/// Detects late-bound lifetimes and inserts them into
/// `map.late_bound`.
///
/// A region declared on a fn is **late-bound** if:
/// - it is constrained by an argument type;
/// - it does not appear in a where-clause.
///
/// "Constrained" basically means that it appears in any type but
/// not amongst the inputs to a projection.  In other words, `<&'a
/// T as Trait<''b>>::Foo` does not constrain `'a` or `'b`.
fn insert_late_bound_lifetimes(map: &mut NamedRegionMap,
                               fn_def_id: DefId,
                               decl: &hir::FnDecl,
                               generics: &hir::Generics) {
    debug!("insert_late_bound_lifetimes(decl={:?}, generics={:?})", decl, generics);

    let mut constrained_by_input = ConstrainedCollector { regions: FnvHashSet() };
    for arg in &decl.inputs {
        constrained_by_input.visit_ty(&arg.ty);
    }

    let mut appears_in_output = AllCollector { regions: FnvHashSet() };
    intravisit::walk_fn_ret_ty(&mut appears_in_output, &decl.output);

    debug!("insert_late_bound_lifetimes: constrained_by_input={:?}",
           constrained_by_input.regions);

    // Walk the lifetimes that appear in where clauses.
    //
    // Subtle point: because we disallow nested bindings, we can just
    // ignore binders here and scrape up all names we see.
    let mut appears_in_where_clause = AllCollector { regions: FnvHashSet() };
    for ty_param in generics.ty_params.iter() {
        walk_list!(&mut appears_in_where_clause,
                   visit_ty_param_bound,
                   &ty_param.bounds);
    }
    walk_list!(&mut appears_in_where_clause,
               visit_where_predicate,
               &generics.where_clause.predicates);
    for lifetime_def in &generics.lifetimes {
        if !lifetime_def.bounds.is_empty() {
            // `'a: 'b` means both `'a` and `'b` are referenced
            appears_in_where_clause.visit_lifetime_def(lifetime_def);
        }
    }

    debug!("insert_late_bound_lifetimes: appears_in_where_clause={:?}",
           appears_in_where_clause.regions);

    // Late bound regions are those that:
    // - appear in the inputs
    // - do not appear in the where-clauses
    for lifetime in &generics.lifetimes {
        let name = lifetime.lifetime.name;

        // appears in the where clauses? early-bound.
        if appears_in_where_clause.regions.contains(&name) { continue; }

        // does not appear in the inputs, but appears in the return
        // type? eventually this will be early-bound, but for now we
        // just mark it so we can issue warnings.
        let constrained_by_input = constrained_by_input.regions.contains(&name);
        let appears_in_output = appears_in_output.regions.contains(&name);
        let will_change = !constrained_by_input && appears_in_output;
        let issue_32330 = if will_change {
            ty::Issue32330::WillChange {
                fn_def_id: fn_def_id,
                region_name: name,
            }
        } else {
            ty::Issue32330::WontChange
        };

        debug!("insert_late_bound_lifetimes: \
                lifetime {:?} with id {:?} is late-bound ({:?}",
               lifetime.lifetime.name, lifetime.lifetime.id, issue_32330);

        let prev = map.late_bound.insert(lifetime.lifetime.id, issue_32330);
        assert!(prev.is_none(), "visited lifetime {:?} twice", lifetime.lifetime.id);
    }

    return;

    struct ConstrainedCollector {
        regions: FnvHashSet<ast::Name>,
    }

    impl<'v> Visitor<'v> for ConstrainedCollector {
        fn visit_ty(&mut self, ty: &'v hir::Ty) {
            match ty.node {
                hir::TyPath(Some(_), _) => {
                    // ignore lifetimes appearing in associated type
                    // projections, as they are not *constrained*
                    // (defined above)
                }

                hir::TyPath(None, ref path) => {
                    // consider only the lifetimes on the final
                    // segment; I am not sure it's even currently
                    // valid to have them elsewhere, but even if it
                    // is, those would be potentially inputs to
                    // projections
                    if let Some(last_segment) = path.segments.last() {
                        self.visit_path_segment(path.span, last_segment);
                    }
                }

                _ => {
                    intravisit::walk_ty(self, ty);
                }
            }
        }

        fn visit_lifetime(&mut self, lifetime_ref: &'v hir::Lifetime) {
            self.regions.insert(lifetime_ref.name);
        }
    }

    struct AllCollector {
        regions: FnvHashSet<ast::Name>,
    }

    impl<'v> Visitor<'v> for AllCollector {
        fn visit_lifetime(&mut self, lifetime_ref: &'v hir::Lifetime) {
            self.regions.insert(lifetime_ref.name);
        }
    }
}

impl<'a> fmt::Debug for ScopeChain<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EarlyScope(space, defs, _) => write!(fmt, "EarlyScope({:?}, {:?})", space, defs),
            LateScope(defs, _) => write!(fmt, "LateScope({:?})", defs),
            FnScope { fn_id, body_id, s: _ } => write!(fmt, "FnScope({:?}, {:?})", fn_id, body_id),
            RootScope => write!(fmt, "RootScope"),
        }
    }
}
