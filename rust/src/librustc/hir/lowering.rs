// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Lowers the AST to the HIR.
//
// Since the AST and HIR are fairly similar, this is mostly a simple procedure,
// much like a fold. Where lowering involves a bit more work things get more
// interesting and there are some invariants you should know about. These mostly
// concern spans and ids.
//
// Spans are assigned to AST nodes during parsing and then are modified during
// expansion to indicate the origin of a node and the process it went through
// being expanded. Ids are assigned to AST nodes just before lowering.
//
// For the simpler lowering steps, ids and spans should be preserved. Unlike
// expansion we do not preserve the process of lowering in the spans, so spans
// should not be modified here. When creating a new node (as opposed to
// 'folding' an existing one), then you create a new id using `next_id()`.
//
// You must ensure that ids are unique. That means that you should only use the
// id from an AST node in a single HIR node (you can assume that AST node ids
// are unique). Every new node must have a unique id. Avoid cloning HIR nodes.
// If you do, you must then set the new node's id to a fresh one.
//
// Lowering must be reproducable (the compiler only lowers once, but tools and
// custom lints may lower an AST node to a HIR node to interact with the
// compiler). The most interesting bit of this is ids - if you lower an AST node
// and create new HIR nodes with fresh ids, when re-lowering the same node, you
// must ensure you get the same ids! To do this, we keep track of the next id
// when we translate a node which requires new ids. By checking this cache and
// using node ids starting with the cached id, we ensure ids are reproducible.
// To use this system, you just need to hold on to a CachedIdSetter object
// whilst lowering. This is an RAII object that takes care of setting and
// restoring the cached id, etc.
//
// This whole system relies on node ids being incremented one at a time and
// all increments being for lowering. This means that you should not call any
// non-lowering function which will use new node ids.
//
// We must also cache gensym'ed Idents to ensure that we get the same Ident
// every time we lower a node with gensym'ed names. One consequence of this is
// that you can only gensym a name once in a lowering (you don't need to worry
// about nested lowering though). That's because we cache based on the name and
// the currently cached node id, which is unique per lowered node.
//
// Spans are used for error messages and for tools to map semantics back to
// source code. It is therefore not as important with spans as ids to be strict
// about use (you can't break the compiler by screwing up a span). Obviously, a
// HIR node can only have a single span. But multiple nodes can have the same
// span and spans don't need to be kept in order, etc. Where code is preserved
// by lowering, it should have the same span as in the AST. Where HIR nodes are
// new it is probably best to give a span for the whole AST node being lowered.
// All nodes should have real spans, don't use dummy spans. Tools are likely to
// get confused if the spans from leaf AST nodes occur in multiple places
// in the HIR, especially for multiple identifiers.

use hir;
use hir::map::Definitions;
use hir::map::definitions::DefPathData;
use hir::def_id::DefIndex;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::iter;
use syntax::ast::*;
use syntax::attr::{ThinAttributes, ThinAttributesExt};
use syntax::errors::Handler;
use syntax::ext::mtwt;
use syntax::ptr::P;
use syntax::codemap::{respan, Spanned, Span};
use syntax::parse::token;
use syntax::std_inject;
use syntax::visit::{self, Visitor};

use std::cell::{Cell, RefCell};

pub struct LoweringContext<'a> {
    crate_root: Option<&'static str>,
    // Map AST ids to ids used for expanded nodes.
    id_cache: RefCell<HashMap<NodeId, NodeId>>,
    // Use if there are no cached ids for the current node.
    id_assigner: &'a NodeIdAssigner,
    // 0 == no cached id. Must be incremented to align with previous id
    // incrementing.
    cached_id: Cell<u32>,
    // Keep track of gensym'ed idents.
    gensym_cache: RefCell<HashMap<(NodeId, &'static str), hir::Ident>>,
    // A copy of cached_id, but is also set to an id while a node is lowered for
    // the first time.
    gensym_key: Cell<u32>,
    // We must keep the set of definitions up to date as we add nodes that
    // weren't in the AST.
    definitions: Option<&'a RefCell<Definitions>>,
    // As we walk the AST we must keep track of the current 'parent' def id (in
    // the form of a DefIndex) so that if we create a new node which introduces
    // a definition, then we can properly create the def id.
    parent_def: Cell<Option<DefIndex>>,
}

impl<'a, 'hir> LoweringContext<'a> {
    pub fn new(id_assigner: &'a NodeIdAssigner,
               c: Option<&Crate>,
               defs: &'a RefCell<Definitions>)
               -> LoweringContext<'a> {
        let crate_root = c.and_then(|c| {
            if std_inject::no_core(c) {
                None
            } else if std_inject::no_std(c) {
                Some("core")
            } else {
                Some("std")
            }
        });

        LoweringContext {
            crate_root: crate_root,
            id_cache: RefCell::new(HashMap::new()),
            id_assigner: id_assigner,
            cached_id: Cell::new(0),
            gensym_cache: RefCell::new(HashMap::new()),
            gensym_key: Cell::new(0),
            definitions: Some(defs),
            parent_def: Cell::new(None),
        }
    }

    // Only use this when you want a LoweringContext for testing and won't look
    // up def ids for anything created during lowering.
    pub fn testing_context(id_assigner: &'a NodeIdAssigner) -> LoweringContext<'a> {
        LoweringContext {
            crate_root: None,
            id_cache: RefCell::new(HashMap::new()),
            id_assigner: id_assigner,
            cached_id: Cell::new(0),
            gensym_cache: RefCell::new(HashMap::new()),
            gensym_key: Cell::new(0),
            definitions: None,
            parent_def: Cell::new(None),
        }
    }

    fn next_id(&self) -> NodeId {
        let cached_id = self.cached_id.get();
        if cached_id == 0 {
            return self.id_assigner.next_node_id();
        }

        self.cached_id.set(cached_id + 1);
        cached_id
    }

    fn str_to_ident(&self, s: &'static str) -> hir::Ident {
        let gensym_key = self.gensym_key.get();
        if gensym_key == 0 {
            return hir::Ident::from_name(token::gensym(s));
        }

        let cached = self.gensym_cache.borrow().contains_key(&(gensym_key, s));
        if cached {
            self.gensym_cache.borrow()[&(gensym_key, s)]
        } else {
            let result = hir::Ident::from_name(token::gensym(s));
            self.gensym_cache.borrow_mut().insert((gensym_key, s), result);
            result
        }
    }

    // Panics if this LoweringContext's NodeIdAssigner is not able to emit diagnostics.
    fn diagnostic(&self) -> &Handler {
        self.id_assigner.diagnostic()
    }

    fn with_parent_def<T, F: FnOnce() -> T>(&self, parent_id: NodeId, f: F) -> T {
        if self.definitions.is_none() {
            // This should only be used for testing.
            return f();
        }

        let old_def = self.parent_def.get();
        self.parent_def.set(Some(self.get_def(parent_id)));
        let result = f();
        self.parent_def.set(old_def);

        result
    }

    fn get_def(&self, id: NodeId) -> DefIndex {
        let defs = self.definitions.unwrap().borrow();
        defs.opt_def_index(id).unwrap()
    }
}

// Utility fn for setting and unsetting the cached id.
fn cache_ids<'a, OP, R>(lctx: &LoweringContext, expr_id: NodeId, op: OP) -> R
    where OP: FnOnce(&LoweringContext) -> R
{
    // Only reset the id if it was previously 0, i.e., was not cached.
    // If it was cached, we are in a nested node, but our id count will
    // still count towards the parent's count.
    let reset_cached_id = lctx.cached_id.get() == 0;
    // We always reset gensym_key so that if we use the same name in a nested
    // node and after that node, they get different values.
    let old_gensym_key = lctx.gensym_key.get();

    {
        let id_cache: &mut HashMap<_, _> = &mut lctx.id_cache.borrow_mut();

        if id_cache.contains_key(&expr_id) {
            let cached_id = lctx.cached_id.get();
            if cached_id == 0 {
                // We're entering a node where we need to track ids, but are not
                // yet tracking.
                lctx.cached_id.set(id_cache[&expr_id]);
            } else {
                // We're already tracking - check that the tracked id is the same
                // as the expected id.
                assert!(cached_id == id_cache[&expr_id], "id mismatch");
            }
            lctx.gensym_key.set(id_cache[&expr_id]);
        } else {
            // We've never lowered this node before, remember it for next time.
            let next_id = lctx.id_assigner.peek_node_id();
            id_cache.insert(expr_id, next_id);
            lctx.gensym_key.set(next_id);
            // self.cached_id is not set when we lower a node for the first time,
            // only on re-lowering.
        }
    }

    let result = op(lctx);

    if reset_cached_id {
        lctx.cached_id.set(0);
    }
    lctx.gensym_key.set(old_gensym_key);

    result
}

pub fn lower_ident(_lctx: &LoweringContext, ident: Ident) -> hir::Ident {
    hir::Ident {
        name: mtwt::resolve(ident),
        unhygienic_name: ident.name,
    }
}

pub fn lower_attrs(_lctx: &LoweringContext, attrs: &Vec<Attribute>) -> hir::HirVec<Attribute> {
    attrs.clone().into()
}

pub fn lower_view_path(lctx: &LoweringContext, view_path: &ViewPath) -> P<hir::ViewPath> {
    P(Spanned {
        node: match view_path.node {
            ViewPathSimple(ident, ref path) => {
                hir::ViewPathSimple(ident.name, lower_path(lctx, path))
            }
            ViewPathGlob(ref path) => {
                hir::ViewPathGlob(lower_path(lctx, path))
            }
            ViewPathList(ref path, ref path_list_idents) => {
                hir::ViewPathList(lower_path(lctx, path),
                                  path_list_idents.iter()
                                                  .map(lower_path_list_item)
                                                  .collect())
            }
        },
        span: view_path.span,
    })
}

fn lower_path_list_item(path_list_ident: &PathListItem) -> hir::PathListItem {
    Spanned {
        node: match path_list_ident.node {
            PathListItemKind::Ident { id, name, rename } => hir::PathListIdent {
                id: id,
                name: name.name,
                rename: rename.map(|x| x.name),
            },
            PathListItemKind::Mod { id, rename } => hir::PathListMod {
                id: id,
                rename: rename.map(|x| x.name),
            },
        },
        span: path_list_ident.span,
    }
}

pub fn lower_arm(lctx: &LoweringContext, arm: &Arm) -> hir::Arm {
    hir::Arm {
        attrs: lower_attrs(lctx, &arm.attrs),
        pats: arm.pats.iter().map(|x| lower_pat(lctx, x)).collect(),
        guard: arm.guard.as_ref().map(|ref x| lower_expr(lctx, x)),
        body: lower_expr(lctx, &arm.body),
    }
}

pub fn lower_decl(lctx: &LoweringContext, d: &Decl) -> P<hir::Decl> {
    match d.node {
        DeclKind::Local(ref l) => P(Spanned {
            node: hir::DeclLocal(lower_local(lctx, l)),
            span: d.span,
        }),
        DeclKind::Item(ref it) => P(Spanned {
            node: hir::DeclItem(lower_item_id(lctx, it)),
            span: d.span,
        }),
    }
}

pub fn lower_ty_binding(lctx: &LoweringContext, b: &TypeBinding) -> hir::TypeBinding {
    hir::TypeBinding {
        id: b.id,
        name: b.ident.name,
        ty: lower_ty(lctx, &b.ty),
        span: b.span,
    }
}

pub fn lower_ty(lctx: &LoweringContext, t: &Ty) -> P<hir::Ty> {
    use syntax::ast::TyKind::*;
    P(hir::Ty {
        id: t.id,
        node: match t.node {
            Infer => hir::TyInfer,
            Vec(ref ty) => hir::TyVec(lower_ty(lctx, ty)),
            Ptr(ref mt) => hir::TyPtr(lower_mt(lctx, mt)),
            Rptr(ref region, ref mt) => {
                hir::TyRptr(lower_opt_lifetime(lctx, region), lower_mt(lctx, mt))
            }
            BareFn(ref f) => {
                hir::TyBareFn(P(hir::BareFnTy {
                    lifetimes: lower_lifetime_defs(lctx, &f.lifetimes),
                    unsafety: lower_unsafety(lctx, f.unsafety),
                    abi: f.abi,
                    decl: lower_fn_decl(lctx, &f.decl),
                }))
            }
            Tup(ref tys) => hir::TyTup(tys.iter().map(|ty| lower_ty(lctx, ty)).collect()),
            Paren(ref ty) => {
                return lower_ty(lctx, ty);
            }
            Path(ref qself, ref path) => {
                let qself = qself.as_ref().map(|&QSelf { ref ty, position }| {
                    hir::QSelf {
                        ty: lower_ty(lctx, ty),
                        position: position,
                    }
                });
                hir::TyPath(qself, lower_path(lctx, path))
            }
            ObjectSum(ref ty, ref bounds) => {
                hir::TyObjectSum(lower_ty(lctx, ty), lower_bounds(lctx, bounds))
            }
            FixedLengthVec(ref ty, ref e) => {
                hir::TyFixedLengthVec(lower_ty(lctx, ty), lower_expr(lctx, e))
            }
            Typeof(ref expr) => {
                hir::TyTypeof(lower_expr(lctx, expr))
            }
            PolyTraitRef(ref bounds) => {
                hir::TyPolyTraitRef(bounds.iter().map(|b| lower_ty_param_bound(lctx, b)).collect())
            }
            Mac(_) => panic!("TyMac should have been expanded by now."),
        },
        span: t.span,
    })
}

pub fn lower_foreign_mod(lctx: &LoweringContext, fm: &ForeignMod) -> hir::ForeignMod {
    hir::ForeignMod {
        abi: fm.abi,
        items: fm.items.iter().map(|x| lower_foreign_item(lctx, x)).collect(),
    }
}

pub fn lower_variant(lctx: &LoweringContext, v: &Variant) -> hir::Variant {
    Spanned {
        node: hir::Variant_ {
            name: v.node.name.name,
            attrs: lower_attrs(lctx, &v.node.attrs),
            data: lower_variant_data(lctx, &v.node.data),
            disr_expr: v.node.disr_expr.as_ref().map(|e| lower_expr(lctx, e)),
        },
        span: v.span,
    }
}

// Path segments are usually unhygienic, hygienic path segments can occur only in
// identifier-like paths originating from `ExprPath`.
// Make life simpler for rustc_resolve by renaming only such segments.
pub fn lower_path_full(lctx: &LoweringContext, p: &Path, maybe_hygienic: bool) -> hir::Path {
    let maybe_hygienic = maybe_hygienic && !p.global && p.segments.len() == 1;
    hir::Path {
        global: p.global,
        segments: p.segments
                   .iter()
                   .map(|&PathSegment { identifier, ref parameters }| {
                       hir::PathSegment {
                           identifier: if maybe_hygienic {
                               lower_ident(lctx, identifier)
                           } else {
                               hir::Ident::from_name(identifier.name)
                           },
                           parameters: lower_path_parameters(lctx, parameters),
                       }
                   })
                   .collect(),
        span: p.span,
    }
}

pub fn lower_path(lctx: &LoweringContext, p: &Path) -> hir::Path {
    lower_path_full(lctx, p, false)
}

pub fn lower_path_parameters(lctx: &LoweringContext,
                             path_parameters: &PathParameters)
                             -> hir::PathParameters {
    match *path_parameters {
        PathParameters::AngleBracketed(ref data) =>
            hir::AngleBracketedParameters(lower_angle_bracketed_parameter_data(lctx, data)),
        PathParameters::Parenthesized(ref data) =>
            hir::ParenthesizedParameters(lower_parenthesized_parameter_data(lctx, data)),
    }
}

pub fn lower_angle_bracketed_parameter_data(lctx: &LoweringContext,
                                            data: &AngleBracketedParameterData)
                                            -> hir::AngleBracketedParameterData {
    let &AngleBracketedParameterData { ref lifetimes, ref types, ref bindings } = data;
    hir::AngleBracketedParameterData {
        lifetimes: lower_lifetimes(lctx, lifetimes),
        types: types.iter().map(|ty| lower_ty(lctx, ty)).collect(),
        bindings: bindings.iter().map(|b| lower_ty_binding(lctx, b)).collect(),
    }
}

pub fn lower_parenthesized_parameter_data(lctx: &LoweringContext,
                                          data: &ParenthesizedParameterData)
                                          -> hir::ParenthesizedParameterData {
    let &ParenthesizedParameterData { ref inputs, ref output, span } = data;
    hir::ParenthesizedParameterData {
        inputs: inputs.iter().map(|ty| lower_ty(lctx, ty)).collect(),
        output: output.as_ref().map(|ty| lower_ty(lctx, ty)),
        span: span,
    }
}

pub fn lower_local(lctx: &LoweringContext, l: &Local) -> P<hir::Local> {
    P(hir::Local {
        id: l.id,
        ty: l.ty.as_ref().map(|t| lower_ty(lctx, t)),
        pat: lower_pat(lctx, &l.pat),
        init: l.init.as_ref().map(|e| lower_expr(lctx, e)),
        span: l.span,
        attrs: l.attrs.clone(),
    })
}

pub fn lower_explicit_self_underscore(lctx: &LoweringContext,
                                      es: &SelfKind)
                                      -> hir::ExplicitSelf_ {
    match *es {
        SelfKind::Static => hir::SelfStatic,
        SelfKind::Value(v) => hir::SelfValue(v.name),
        SelfKind::Region(ref lifetime, m, ident) => {
            hir::SelfRegion(lower_opt_lifetime(lctx, lifetime),
                            lower_mutability(lctx, m),
                            ident.name)
        }
        SelfKind::Explicit(ref typ, ident) => {
            hir::SelfExplicit(lower_ty(lctx, typ), ident.name)
        }
    }
}

pub fn lower_mutability(_lctx: &LoweringContext, m: Mutability) -> hir::Mutability {
    match m {
        Mutability::Mutable => hir::MutMutable,
        Mutability::Immutable => hir::MutImmutable,
    }
}

pub fn lower_explicit_self(lctx: &LoweringContext, s: &ExplicitSelf) -> hir::ExplicitSelf {
    Spanned {
        node: lower_explicit_self_underscore(lctx, &s.node),
        span: s.span,
    }
}

pub fn lower_arg(lctx: &LoweringContext, arg: &Arg) -> hir::Arg {
    hir::Arg {
        id: arg.id,
        pat: lower_pat(lctx, &arg.pat),
        ty: lower_ty(lctx, &arg.ty),
    }
}

pub fn lower_fn_decl(lctx: &LoweringContext, decl: &FnDecl) -> P<hir::FnDecl> {
    P(hir::FnDecl {
        inputs: decl.inputs.iter().map(|x| lower_arg(lctx, x)).collect(),
        output: match decl.output {
            FunctionRetTy::Ty(ref ty) => hir::Return(lower_ty(lctx, ty)),
            FunctionRetTy::Default(span) => hir::DefaultReturn(span),
            FunctionRetTy::None(span) => hir::NoReturn(span),
        },
        variadic: decl.variadic,
    })
}

pub fn lower_ty_param_bound(lctx: &LoweringContext, tpb: &TyParamBound) -> hir::TyParamBound {
    match *tpb {
        TraitTyParamBound(ref ty, modifier) => {
            hir::TraitTyParamBound(lower_poly_trait_ref(lctx, ty),
                                   lower_trait_bound_modifier(lctx, modifier))
        }
        RegionTyParamBound(ref lifetime) => {
            hir::RegionTyParamBound(lower_lifetime(lctx, lifetime))
        }
    }
}

pub fn lower_ty_param(lctx: &LoweringContext, tp: &TyParam) -> hir::TyParam {
    hir::TyParam {
        id: tp.id,
        name: tp.ident.name,
        bounds: lower_bounds(lctx, &tp.bounds),
        default: tp.default.as_ref().map(|x| lower_ty(lctx, x)),
        span: tp.span,
    }
}

pub fn lower_ty_params(lctx: &LoweringContext,
                       tps: &P<[TyParam]>)
                       -> hir::HirVec<hir::TyParam> {
    tps.iter().map(|tp| lower_ty_param(lctx, tp)).collect()
}

pub fn lower_lifetime(_lctx: &LoweringContext, l: &Lifetime) -> hir::Lifetime {
    hir::Lifetime {
        id: l.id,
        name: l.name,
        span: l.span,
    }
}

pub fn lower_lifetime_def(lctx: &LoweringContext, l: &LifetimeDef) -> hir::LifetimeDef {
    hir::LifetimeDef {
        lifetime: lower_lifetime(lctx, &l.lifetime),
        bounds: lower_lifetimes(lctx, &l.bounds),
    }
}

pub fn lower_lifetimes(lctx: &LoweringContext, lts: &Vec<Lifetime>) -> hir::HirVec<hir::Lifetime> {
    lts.iter().map(|l| lower_lifetime(lctx, l)).collect()
}

pub fn lower_lifetime_defs(lctx: &LoweringContext,
                           lts: &Vec<LifetimeDef>)
                           -> hir::HirVec<hir::LifetimeDef> {
    lts.iter().map(|l| lower_lifetime_def(lctx, l)).collect()
}

pub fn lower_opt_lifetime(lctx: &LoweringContext,
                          o_lt: &Option<Lifetime>)
                          -> Option<hir::Lifetime> {
    o_lt.as_ref().map(|lt| lower_lifetime(lctx, lt))
}

pub fn lower_generics(lctx: &LoweringContext, g: &Generics) -> hir::Generics {
    hir::Generics {
        ty_params: lower_ty_params(lctx, &g.ty_params),
        lifetimes: lower_lifetime_defs(lctx, &g.lifetimes),
        where_clause: lower_where_clause(lctx, &g.where_clause),
    }
}

pub fn lower_where_clause(lctx: &LoweringContext, wc: &WhereClause) -> hir::WhereClause {
    hir::WhereClause {
        id: wc.id,
        predicates: wc.predicates
                      .iter()
                      .map(|predicate| lower_where_predicate(lctx, predicate))
                      .collect(),
    }
}

pub fn lower_where_predicate(lctx: &LoweringContext,
                             pred: &WherePredicate)
                             -> hir::WherePredicate {
    match *pred {
        WherePredicate::BoundPredicate(WhereBoundPredicate{ ref bound_lifetimes,
                                                            ref bounded_ty,
                                                            ref bounds,
                                                            span}) => {
            hir::WherePredicate::BoundPredicate(hir::WhereBoundPredicate {
                bound_lifetimes: lower_lifetime_defs(lctx, bound_lifetimes),
                bounded_ty: lower_ty(lctx, bounded_ty),
                bounds: bounds.iter().map(|x| lower_ty_param_bound(lctx, x)).collect(),
                span: span,
            })
        }
        WherePredicate::RegionPredicate(WhereRegionPredicate{ ref lifetime,
                                                              ref bounds,
                                                              span}) => {
            hir::WherePredicate::RegionPredicate(hir::WhereRegionPredicate {
                span: span,
                lifetime: lower_lifetime(lctx, lifetime),
                bounds: bounds.iter().map(|bound| lower_lifetime(lctx, bound)).collect(),
            })
        }
        WherePredicate::EqPredicate(WhereEqPredicate{ id,
                                                      ref path,
                                                      ref ty,
                                                      span}) => {
            hir::WherePredicate::EqPredicate(hir::WhereEqPredicate {
                id: id,
                path: lower_path(lctx, path),
                ty: lower_ty(lctx, ty),
                span: span,
            })
        }
    }
}

pub fn lower_variant_data(lctx: &LoweringContext, vdata: &VariantData) -> hir::VariantData {
    match *vdata {
        VariantData::Struct(ref fields, id) => {
            hir::VariantData::Struct(fields.iter()
                                           .enumerate()
                                           .map(|f| lower_struct_field(lctx, f))
                                           .collect(),
                                     id)
        }
        VariantData::Tuple(ref fields, id) => {
            hir::VariantData::Tuple(fields.iter()
                                          .enumerate()
                                          .map(|f| lower_struct_field(lctx, f))
                                          .collect(),
                                    id)
        }
        VariantData::Unit(id) => hir::VariantData::Unit(id),
    }
}

pub fn lower_trait_ref(lctx: &LoweringContext, p: &TraitRef) -> hir::TraitRef {
    hir::TraitRef {
        path: lower_path(lctx, &p.path),
        ref_id: p.ref_id,
    }
}

pub fn lower_poly_trait_ref(lctx: &LoweringContext, p: &PolyTraitRef) -> hir::PolyTraitRef {
    hir::PolyTraitRef {
        bound_lifetimes: lower_lifetime_defs(lctx, &p.bound_lifetimes),
        trait_ref: lower_trait_ref(lctx, &p.trait_ref),
        span: p.span,
    }
}

pub fn lower_struct_field(lctx: &LoweringContext,
                          (index, f): (usize, &StructField))
                          -> hir::StructField {
    hir::StructField {
        span: f.span,
        id: f.id,
        name: f.ident.map(|ident| ident.name).unwrap_or(token::intern(&index.to_string())),
        vis: lower_visibility(lctx, &f.vis),
        ty: lower_ty(lctx, &f.ty),
        attrs: lower_attrs(lctx, &f.attrs),
    }
}

pub fn lower_field(lctx: &LoweringContext, f: &Field) -> hir::Field {
    hir::Field {
        name: respan(f.ident.span, f.ident.node.name),
        expr: lower_expr(lctx, &f.expr),
        span: f.span,
    }
}

pub fn lower_mt(lctx: &LoweringContext, mt: &MutTy) -> hir::MutTy {
    hir::MutTy {
        ty: lower_ty(lctx, &mt.ty),
        mutbl: lower_mutability(lctx, mt.mutbl),
    }
}

pub fn lower_opt_bounds(lctx: &LoweringContext,
                        b: &Option<TyParamBounds>)
                        -> Option<hir::TyParamBounds> {
    b.as_ref().map(|ref bounds| lower_bounds(lctx, bounds))
}

fn lower_bounds(lctx: &LoweringContext, bounds: &TyParamBounds) -> hir::TyParamBounds {
    bounds.iter().map(|bound| lower_ty_param_bound(lctx, bound)).collect()
}

pub fn lower_block(lctx: &LoweringContext, b: &Block) -> P<hir::Block> {
    P(hir::Block {
        id: b.id,
        stmts: b.stmts.iter().map(|s| lower_stmt(lctx, s)).collect(),
        expr: b.expr.as_ref().map(|ref x| lower_expr(lctx, x)),
        rules: lower_block_check_mode(lctx, &b.rules),
        span: b.span,
    })
}

pub fn lower_item_kind(lctx: &LoweringContext, i: &ItemKind) -> hir::Item_ {
    match *i {
        ItemKind::ExternCrate(string) => hir::ItemExternCrate(string),
        ItemKind::Use(ref view_path) => {
            hir::ItemUse(lower_view_path(lctx, view_path))
        }
        ItemKind::Static(ref t, m, ref e) => {
            hir::ItemStatic(lower_ty(lctx, t),
                            lower_mutability(lctx, m),
                            lower_expr(lctx, e))
        }
        ItemKind::Const(ref t, ref e) => {
            hir::ItemConst(lower_ty(lctx, t), lower_expr(lctx, e))
        }
        ItemKind::Fn(ref decl, unsafety, constness, abi, ref generics, ref body) => {
            hir::ItemFn(lower_fn_decl(lctx, decl),
                        lower_unsafety(lctx, unsafety),
                        lower_constness(lctx, constness),
                        abi,
                        lower_generics(lctx, generics),
                        lower_block(lctx, body))
        }
        ItemKind::Mod(ref m) => hir::ItemMod(lower_mod(lctx, m)),
        ItemKind::ForeignMod(ref nm) => hir::ItemForeignMod(lower_foreign_mod(lctx, nm)),
        ItemKind::Ty(ref t, ref generics) => {
            hir::ItemTy(lower_ty(lctx, t), lower_generics(lctx, generics))
        }
        ItemKind::Enum(ref enum_definition, ref generics) => {
            hir::ItemEnum(hir::EnumDef {
                              variants: enum_definition.variants
                                                       .iter()
                                                       .map(|x| lower_variant(lctx, x))
                                                       .collect(),
                          },
                          lower_generics(lctx, generics))
        }
        ItemKind::Struct(ref struct_def, ref generics) => {
            let struct_def = lower_variant_data(lctx, struct_def);
            hir::ItemStruct(struct_def, lower_generics(lctx, generics))
        }
        ItemKind::DefaultImpl(unsafety, ref trait_ref) => {
            hir::ItemDefaultImpl(lower_unsafety(lctx, unsafety),
                                 lower_trait_ref(lctx, trait_ref))
        }
        ItemKind::Impl(unsafety, polarity, ref generics, ref ifce, ref ty, ref impl_items) => {
            let new_impl_items = impl_items.iter()
                                           .map(|item| lower_impl_item(lctx, item))
                                           .collect();
            let ifce = ifce.as_ref().map(|trait_ref| lower_trait_ref(lctx, trait_ref));
            hir::ItemImpl(lower_unsafety(lctx, unsafety),
                          lower_impl_polarity(lctx, polarity),
                          lower_generics(lctx, generics),
                          ifce,
                          lower_ty(lctx, ty),
                          new_impl_items)
        }
        ItemKind::Trait(unsafety, ref generics, ref bounds, ref items) => {
            let bounds = lower_bounds(lctx, bounds);
            let items = items.iter().map(|item| lower_trait_item(lctx, item)).collect();
            hir::ItemTrait(lower_unsafety(lctx, unsafety),
                           lower_generics(lctx, generics),
                           bounds,
                           items)
        }
        ItemKind::Mac(_) => panic!("Shouldn't still be around"),
    }
}

pub fn lower_trait_item(lctx: &LoweringContext, i: &TraitItem) -> hir::TraitItem {
    lctx.with_parent_def(i.id, || {
        hir::TraitItem {
            id: i.id,
            name: i.ident.name,
            attrs: lower_attrs(lctx, &i.attrs),
            node: match i.node {
                TraitItemKind::Const(ref ty, ref default) => {
                    hir::ConstTraitItem(lower_ty(lctx, ty),
                                        default.as_ref().map(|x| lower_expr(lctx, x)))
                }
                TraitItemKind::Method(ref sig, ref body) => {
                    hir::MethodTraitItem(lower_method_sig(lctx, sig),
                                         body.as_ref().map(|x| lower_block(lctx, x)))
                }
                TraitItemKind::Type(ref bounds, ref default) => {
                    hir::TypeTraitItem(lower_bounds(lctx, bounds),
                                       default.as_ref().map(|x| lower_ty(lctx, x)))
                }
            },
            span: i.span,
        }
    })
}

pub fn lower_impl_item(lctx: &LoweringContext, i: &ImplItem) -> hir::ImplItem {
    lctx.with_parent_def(i.id, || {
        hir::ImplItem {
            id: i.id,
            name: i.ident.name,
            attrs: lower_attrs(lctx, &i.attrs),
            vis: lower_visibility(lctx, &i.vis),
            defaultness: lower_defaultness(lctx, i.defaultness),
            node: match i.node {
                ImplItemKind::Const(ref ty, ref expr) => {
                    hir::ImplItemKind::Const(lower_ty(lctx, ty), lower_expr(lctx, expr))
                }
                ImplItemKind::Method(ref sig, ref body) => {
                    hir::ImplItemKind::Method(lower_method_sig(lctx, sig), lower_block(lctx, body))
                }
                ImplItemKind::Type(ref ty) => hir::ImplItemKind::Type(lower_ty(lctx, ty)),
                ImplItemKind::Macro(..) => panic!("Shouldn't exist any more"),
            },
            span: i.span,
        }
    })
}

pub fn lower_mod(lctx: &LoweringContext, m: &Mod) -> hir::Mod {
    hir::Mod {
        inner: m.inner,
        item_ids: m.items.iter().map(|x| lower_item_id(lctx, x)).collect(),
    }
}

struct ItemLowerer<'lcx, 'interner: 'lcx> {
    items: BTreeMap<NodeId, hir::Item>,
    lctx: &'lcx LoweringContext<'interner>,
}

impl<'lcx, 'interner> Visitor<'lcx> for ItemLowerer<'lcx, 'interner> {
    fn visit_item(&mut self, item: &'lcx Item) {
        self.items.insert(item.id, lower_item(self.lctx, item));
        visit::walk_item(self, item);
    }
}

pub fn lower_crate(lctx: &LoweringContext, c: &Crate) -> hir::Crate {
    let items = {
        let mut item_lowerer = ItemLowerer { items: BTreeMap::new(), lctx: lctx };
        visit::walk_crate(&mut item_lowerer, c);
        item_lowerer.items
    };

    hir::Crate {
        module: lower_mod(lctx, &c.module),
        attrs: lower_attrs(lctx, &c.attrs),
        config: c.config.clone().into(),
        span: c.span,
        exported_macros: c.exported_macros.iter().map(|m| lower_macro_def(lctx, m)).collect(),
        items: items,
    }
}

pub fn lower_macro_def(lctx: &LoweringContext, m: &MacroDef) -> hir::MacroDef {
    hir::MacroDef {
        name: m.ident.name,
        attrs: lower_attrs(lctx, &m.attrs),
        id: m.id,
        span: m.span,
        imported_from: m.imported_from.map(|x| x.name),
        export: m.export,
        use_locally: m.use_locally,
        allow_internal_unstable: m.allow_internal_unstable,
        body: m.body.clone().into(),
    }
}

pub fn lower_item_id(_lctx: &LoweringContext, i: &Item) -> hir::ItemId {
    hir::ItemId { id: i.id }
}

pub fn lower_item(lctx: &LoweringContext, i: &Item) -> hir::Item {
    let node = lctx.with_parent_def(i.id, || {
        lower_item_kind(lctx, &i.node)
    });

    hir::Item {
        id: i.id,
        name: i.ident.name,
        attrs: lower_attrs(lctx, &i.attrs),
        node: node,
        vis: lower_visibility(lctx, &i.vis),
        span: i.span,
    }
}

pub fn lower_foreign_item(lctx: &LoweringContext, i: &ForeignItem) -> hir::ForeignItem {
    lctx.with_parent_def(i.id, || {
        hir::ForeignItem {
            id: i.id,
            name: i.ident.name,
            attrs: lower_attrs(lctx, &i.attrs),
            node: match i.node {
                ForeignItemKind::Fn(ref fdec, ref generics) => {
                    hir::ForeignItemFn(lower_fn_decl(lctx, fdec), lower_generics(lctx, generics))
                }
                ForeignItemKind::Static(ref t, m) => {
                    hir::ForeignItemStatic(lower_ty(lctx, t), m)
                }
            },
            vis: lower_visibility(lctx, &i.vis),
            span: i.span,
        }
    })
}

pub fn lower_method_sig(lctx: &LoweringContext, sig: &MethodSig) -> hir::MethodSig {
    hir::MethodSig {
        generics: lower_generics(lctx, &sig.generics),
        abi: sig.abi,
        explicit_self: lower_explicit_self(lctx, &sig.explicit_self),
        unsafety: lower_unsafety(lctx, sig.unsafety),
        constness: lower_constness(lctx, sig.constness),
        decl: lower_fn_decl(lctx, &sig.decl),
    }
}

pub fn lower_unsafety(_lctx: &LoweringContext, u: Unsafety) -> hir::Unsafety {
    match u {
        Unsafety::Unsafe => hir::Unsafety::Unsafe,
        Unsafety::Normal => hir::Unsafety::Normal,
    }
}

pub fn lower_constness(_lctx: &LoweringContext, c: Constness) -> hir::Constness {
    match c {
        Constness::Const => hir::Constness::Const,
        Constness::NotConst => hir::Constness::NotConst,
    }
}

pub fn lower_unop(_lctx: &LoweringContext, u: UnOp) -> hir::UnOp {
    match u {
        UnOp::Deref => hir::UnDeref,
        UnOp::Not => hir::UnNot,
        UnOp::Neg => hir::UnNeg,
    }
}

pub fn lower_binop(_lctx: &LoweringContext, b: BinOp) -> hir::BinOp {
    Spanned {
        node: match b.node {
            BinOpKind::Add => hir::BiAdd,
            BinOpKind::Sub => hir::BiSub,
            BinOpKind::Mul => hir::BiMul,
            BinOpKind::Div => hir::BiDiv,
            BinOpKind::Rem => hir::BiRem,
            BinOpKind::And => hir::BiAnd,
            BinOpKind::Or => hir::BiOr,
            BinOpKind::BitXor => hir::BiBitXor,
            BinOpKind::BitAnd => hir::BiBitAnd,
            BinOpKind::BitOr => hir::BiBitOr,
            BinOpKind::Shl => hir::BiShl,
            BinOpKind::Shr => hir::BiShr,
            BinOpKind::Eq => hir::BiEq,
            BinOpKind::Lt => hir::BiLt,
            BinOpKind::Le => hir::BiLe,
            BinOpKind::Ne => hir::BiNe,
            BinOpKind::Ge => hir::BiGe,
            BinOpKind::Gt => hir::BiGt,
        },
        span: b.span,
    }
}

pub fn lower_pat(lctx: &LoweringContext, p: &Pat) -> P<hir::Pat> {
    P(hir::Pat {
        id: p.id,
        node: match p.node {
            PatKind::Wild => hir::PatKind::Wild,
            PatKind::Ident(ref binding_mode, pth1, ref sub) => {
                lctx.with_parent_def(p.id, || {
                    hir::PatKind::Ident(lower_binding_mode(lctx, binding_mode),
                                  respan(pth1.span, lower_ident(lctx, pth1.node)),
                                  sub.as_ref().map(|x| lower_pat(lctx, x)))
                })
            }
            PatKind::Lit(ref e) => hir::PatKind::Lit(lower_expr(lctx, e)),
            PatKind::TupleStruct(ref pth, ref pats) => {
                hir::PatKind::TupleStruct(lower_path(lctx, pth),
                             pats.as_ref()
                                 .map(|pats| pats.iter().map(|x| lower_pat(lctx, x)).collect()))
            }
            PatKind::Path(ref pth) => {
                hir::PatKind::Path(lower_path(lctx, pth))
            }
            PatKind::QPath(ref qself, ref pth) => {
                let qself = hir::QSelf {
                    ty: lower_ty(lctx, &qself.ty),
                    position: qself.position,
                };
                hir::PatKind::QPath(qself, lower_path(lctx, pth))
            }
            PatKind::Struct(ref pth, ref fields, etc) => {
                let pth = lower_path(lctx, pth);
                let fs = fields.iter()
                               .map(|f| {
                                   Spanned {
                                       span: f.span,
                                       node: hir::FieldPat {
                                           name: f.node.ident.name,
                                           pat: lower_pat(lctx, &f.node.pat),
                                           is_shorthand: f.node.is_shorthand,
                                       },
                                   }
                               })
                               .collect();
                hir::PatKind::Struct(pth, fs, etc)
            }
            PatKind::Tup(ref elts) => {
                hir::PatKind::Tup(elts.iter().map(|x| lower_pat(lctx, x)).collect())
            }
            PatKind::Box(ref inner) => hir::PatKind::Box(lower_pat(lctx, inner)),
            PatKind::Ref(ref inner, mutbl) => {
                hir::PatKind::Ref(lower_pat(lctx, inner), lower_mutability(lctx, mutbl))
            }
            PatKind::Range(ref e1, ref e2) => {
                hir::PatKind::Range(lower_expr(lctx, e1), lower_expr(lctx, e2))
            }
            PatKind::Vec(ref before, ref slice, ref after) => {
                hir::PatKind::Vec(before.iter().map(|x| lower_pat(lctx, x)).collect(),
                            slice.as_ref().map(|x| lower_pat(lctx, x)),
                            after.iter().map(|x| lower_pat(lctx, x)).collect())
            }
            PatKind::Mac(_) => panic!("Shouldn't exist here"),
        },
        span: p.span,
    })
}

pub fn lower_expr(lctx: &LoweringContext, e: &Expr) -> P<hir::Expr> {
    P(hir::Expr {
        id: e.id,
        node: match e.node {
            // Issue #22181:
            // Eventually a desugaring for `box EXPR`
            // (similar to the desugaring above for `in PLACE BLOCK`)
            // should go here, desugaring
            //
            // to:
            //
            // let mut place = BoxPlace::make_place();
            // let raw_place = Place::pointer(&mut place);
            // let value = $value;
            // unsafe {
            //     ::std::ptr::write(raw_place, value);
            //     Boxed::finalize(place)
            // }
            //
            // But for now there are type-inference issues doing that.
            ExprKind::Box(ref e) => {
                hir::ExprBox(lower_expr(lctx, e))
            }

            // Desugar ExprBox: `in (PLACE) EXPR`
            ExprKind::InPlace(ref placer, ref value_expr) => {
                // to:
                //
                // let p = PLACE;
                // let mut place = Placer::make_place(p);
                // let raw_place = Place::pointer(&mut place);
                // push_unsafe!({
                //     std::intrinsics::move_val_init(raw_place, pop_unsafe!( EXPR ));
                //     InPlace::finalize(place)
                // })
                return cache_ids(lctx, e.id, |lctx| {
                    let placer_expr = lower_expr(lctx, placer);
                    let value_expr = lower_expr(lctx, value_expr);

                    let placer_ident = lctx.str_to_ident("placer");
                    let place_ident = lctx.str_to_ident("place");
                    let p_ptr_ident = lctx.str_to_ident("p_ptr");

                    let make_place = ["ops", "Placer", "make_place"];
                    let place_pointer = ["ops", "Place", "pointer"];
                    let move_val_init = ["intrinsics", "move_val_init"];
                    let inplace_finalize = ["ops", "InPlace", "finalize"];

                    let make_call = |lctx: &LoweringContext, p, args| {
                        let path = core_path(lctx, e.span, p);
                        let path = expr_path(lctx, path, None);
                        expr_call(lctx, e.span, path, args, None)
                    };

                    let mk_stmt_let = |lctx: &LoweringContext, bind, expr| {
                        stmt_let(lctx, e.span, false, bind, expr, None)
                    };

                    let mk_stmt_let_mut = |lctx: &LoweringContext, bind, expr| {
                        stmt_let(lctx, e.span, true, bind, expr, None)
                    };

                    // let placer = <placer_expr> ;
                    let s1 = {
                        let placer_expr = signal_block_expr(lctx,
                                                            hir_vec![],
                                                            placer_expr,
                                                            e.span,
                                                            hir::PopUnstableBlock,
                                                            None);
                        mk_stmt_let(lctx, placer_ident, placer_expr)
                    };

                    // let mut place = Placer::make_place(placer);
                    let s2 = {
                        let placer = expr_ident(lctx, e.span, placer_ident, None);
                        let call = make_call(lctx, &make_place, hir_vec![placer]);
                        mk_stmt_let_mut(lctx, place_ident, call)
                    };

                    // let p_ptr = Place::pointer(&mut place);
                    let s3 = {
                        let agent = expr_ident(lctx, e.span, place_ident, None);
                        let args = hir_vec![expr_mut_addr_of(lctx, e.span, agent, None)];
                        let call = make_call(lctx, &place_pointer, args);
                        mk_stmt_let(lctx, p_ptr_ident, call)
                    };

                    // pop_unsafe!(EXPR));
                    let pop_unsafe_expr = {
                        let value_expr = signal_block_expr(lctx,
                                                           hir_vec![],
                                                           value_expr,
                                                           e.span,
                                                           hir::PopUnstableBlock,
                                                           None);
                        signal_block_expr(lctx,
                                          hir_vec![],
                                          value_expr,
                                          e.span,
                                          hir::PopUnsafeBlock(hir::CompilerGenerated), None)
                    };

                    // push_unsafe!({
                    //     std::intrinsics::move_val_init(raw_place, pop_unsafe!( EXPR ));
                    //     InPlace::finalize(place)
                    // })
                    let expr = {
                        let ptr = expr_ident(lctx, e.span, p_ptr_ident, None);
                        let call_move_val_init =
                            hir::StmtSemi(
                                make_call(lctx, &move_val_init, hir_vec![ptr, pop_unsafe_expr]),
                                lctx.next_id());
                        let call_move_val_init = respan(e.span, call_move_val_init);

                        let place = expr_ident(lctx, e.span, place_ident, None);
                        let call = make_call(lctx, &inplace_finalize, hir_vec![place]);
                        signal_block_expr(lctx,
                                          hir_vec![call_move_val_init],
                                          call,
                                          e.span,
                                          hir::PushUnsafeBlock(hir::CompilerGenerated), None)
                    };

                    signal_block_expr(lctx,
                                      hir_vec![s1, s2, s3],
                                      expr,
                                      e.span,
                                      hir::PushUnstableBlock,
                                      e.attrs.clone())
                });
            }

            ExprKind::Vec(ref exprs) => {
                hir::ExprVec(exprs.iter().map(|x| lower_expr(lctx, x)).collect())
            }
            ExprKind::Repeat(ref expr, ref count) => {
                let expr = lower_expr(lctx, expr);
                let count = lower_expr(lctx, count);
                hir::ExprRepeat(expr, count)
            }
            ExprKind::Tup(ref elts) => {
                hir::ExprTup(elts.iter().map(|x| lower_expr(lctx, x)).collect())
            }
            ExprKind::Call(ref f, ref args) => {
                let f = lower_expr(lctx, f);
                hir::ExprCall(f, args.iter().map(|x| lower_expr(lctx, x)).collect())
            }
            ExprKind::MethodCall(i, ref tps, ref args) => {
                let tps = tps.iter().map(|x| lower_ty(lctx, x)).collect();
                let args = args.iter().map(|x| lower_expr(lctx, x)).collect();
                hir::ExprMethodCall(respan(i.span, i.node.name), tps, args)
            }
            ExprKind::Binary(binop, ref lhs, ref rhs) => {
                let binop = lower_binop(lctx, binop);
                let lhs = lower_expr(lctx, lhs);
                let rhs = lower_expr(lctx, rhs);
                hir::ExprBinary(binop, lhs, rhs)
            }
            ExprKind::Unary(op, ref ohs) => {
                let op = lower_unop(lctx, op);
                let ohs = lower_expr(lctx, ohs);
                hir::ExprUnary(op, ohs)
            }
            ExprKind::Lit(ref l) => hir::ExprLit(P((**l).clone())),
            ExprKind::Cast(ref expr, ref ty) => {
                let expr = lower_expr(lctx, expr);
                hir::ExprCast(expr, lower_ty(lctx, ty))
            }
            ExprKind::Type(ref expr, ref ty) => {
                let expr = lower_expr(lctx, expr);
                hir::ExprType(expr, lower_ty(lctx, ty))
            }
            ExprKind::AddrOf(m, ref ohs) => {
                let m = lower_mutability(lctx, m);
                let ohs = lower_expr(lctx, ohs);
                hir::ExprAddrOf(m, ohs)
            }
            // More complicated than you might expect because the else branch
            // might be `if let`.
            ExprKind::If(ref cond, ref blk, ref else_opt) => {
                let else_opt = else_opt.as_ref().map(|els| {
                    match els.node {
                        ExprKind::IfLet(..) => {
                            cache_ids(lctx, e.id, |lctx| {
                                // wrap the if-let expr in a block
                                let span = els.span;
                                let els = lower_expr(lctx, els);
                                let id = lctx.next_id();
                                let blk = P(hir::Block {
                                    stmts: hir_vec![],
                                    expr: Some(els),
                                    id: id,
                                    rules: hir::DefaultBlock,
                                    span: span,
                                });
                                expr_block(lctx, blk, None)
                            })
                        }
                        _ => lower_expr(lctx, els),
                    }
                });

                hir::ExprIf(lower_expr(lctx, cond), lower_block(lctx, blk), else_opt)
            }
            ExprKind::While(ref cond, ref body, opt_ident) => {
                hir::ExprWhile(lower_expr(lctx, cond), lower_block(lctx, body),
                               opt_ident.map(|ident| lower_ident(lctx, ident)))
            }
            ExprKind::Loop(ref body, opt_ident) => {
                hir::ExprLoop(lower_block(lctx, body),
                              opt_ident.map(|ident| lower_ident(lctx, ident)))
            }
            ExprKind::Match(ref expr, ref arms) => {
                hir::ExprMatch(lower_expr(lctx, expr),
                               arms.iter().map(|x| lower_arm(lctx, x)).collect(),
                               hir::MatchSource::Normal)
            }
            ExprKind::Closure(capture_clause, ref decl, ref body) => {
                lctx.with_parent_def(e.id, || {
                    hir::ExprClosure(lower_capture_clause(lctx, capture_clause),
                                     lower_fn_decl(lctx, decl),
                                     lower_block(lctx, body))
                })
            }
            ExprKind::Block(ref blk) => hir::ExprBlock(lower_block(lctx, blk)),
            ExprKind::Assign(ref el, ref er) => {
                hir::ExprAssign(lower_expr(lctx, el), lower_expr(lctx, er))
            }
            ExprKind::AssignOp(op, ref el, ref er) => {
                hir::ExprAssignOp(lower_binop(lctx, op),
                                  lower_expr(lctx, el),
                                  lower_expr(lctx, er))
            }
            ExprKind::Field(ref el, ident) => {
                hir::ExprField(lower_expr(lctx, el), respan(ident.span, ident.node.name))
            }
            ExprKind::TupField(ref el, ident) => {
                hir::ExprTupField(lower_expr(lctx, el), ident)
            }
            ExprKind::Index(ref el, ref er) => {
                hir::ExprIndex(lower_expr(lctx, el), lower_expr(lctx, er))
            }
            ExprKind::Range(ref e1, ref e2, lims) => {
                fn make_struct(lctx: &LoweringContext,
                               ast_expr: &Expr,
                               path: &[&str],
                               fields: &[(&str, &P<Expr>)]) -> P<hir::Expr> {
                    let strs = std_path(lctx, &iter::once(&"ops")
                                                    .chain(path)
                                                    .map(|s| *s)
                                                    .collect::<Vec<_>>());

                    let structpath = path_global(ast_expr.span, strs);

                    let hir_expr = if fields.len() == 0 {
                        expr_path(lctx,
                                  structpath,
                                  ast_expr.attrs.clone())
                    } else {
                        expr_struct(lctx,
                                    ast_expr.span,
                                    structpath,
                                    fields.into_iter().map(|&(s, e)| {
                                        field(token::intern(s),
                                              signal_block_expr(lctx,
                                                                hir_vec![],
                                                                lower_expr(lctx, &**e),
                                                                e.span,
                                                                hir::PopUnstableBlock,
                                                                None),
                                              ast_expr.span)
                                    }).collect(),
                                    None,
                                    ast_expr.attrs.clone())
                    };

                    signal_block_expr(lctx,
                                      hir_vec![],
                                      hir_expr,
                                      ast_expr.span,
                                      hir::PushUnstableBlock,
                                      None)
                }

                return cache_ids(lctx, e.id, |lctx| {
                    use syntax::ast::RangeLimits::*;

                    match (e1, e2, lims) {
                        (&None,         &None,         HalfOpen) =>
                            make_struct(lctx, e, &["RangeFull"],
                                                 &[]),

                        (&Some(ref e1), &None,         HalfOpen) =>
                            make_struct(lctx, e, &["RangeFrom"],
                                                 &[("start", e1)]),

                        (&None,         &Some(ref e2), HalfOpen) =>
                            make_struct(lctx, e, &["RangeTo"],
                                                 &[("end", e2)]),

                        (&Some(ref e1), &Some(ref e2), HalfOpen) =>
                            make_struct(lctx, e, &["Range"],
                                                 &[("start", e1), ("end", e2)]),

                        (&None,         &Some(ref e2), Closed)   =>
                            make_struct(lctx, e, &["RangeToInclusive"],
                                                 &[("end", e2)]),

                        (&Some(ref e1), &Some(ref e2), Closed)   =>
                            make_struct(lctx, e, &["RangeInclusive", "NonEmpty"],
                                                 &[("start", e1), ("end", e2)]),

                        _ => panic!(lctx.diagnostic().span_fatal(e.span,
                                                                 "inclusive range with no end"))
                    }
                });
            }
            ExprKind::Path(ref qself, ref path) => {
                let hir_qself = qself.as_ref().map(|&QSelf { ref ty, position }| {
                    hir::QSelf {
                        ty: lower_ty(lctx, ty),
                        position: position,
                    }
                });
                hir::ExprPath(hir_qself, lower_path_full(lctx, path, qself.is_none()))
            }
            ExprKind::Break(opt_ident) => hir::ExprBreak(opt_ident.map(|sp_ident| {
                respan(sp_ident.span, lower_ident(lctx, sp_ident.node))
            })),
            ExprKind::Again(opt_ident) => hir::ExprAgain(opt_ident.map(|sp_ident| {
                respan(sp_ident.span, lower_ident(lctx, sp_ident.node))
            })),
            ExprKind::Ret(ref e) => hir::ExprRet(e.as_ref().map(|x| lower_expr(lctx, x))),
            ExprKind::InlineAsm(InlineAsm {
                    ref inputs,
                    ref outputs,
                    ref asm,
                    asm_str_style,
                    ref clobbers,
                    volatile,
                    alignstack,
                    dialect,
                    expn_id,
                }) => hir::ExprInlineAsm(hir::InlineAsm {
                inputs: inputs.iter().map(|&(ref c, _)| c.clone()).collect(),
                outputs: outputs.iter()
                                .map(|out| {
                                    hir::InlineAsmOutput {
                                        constraint: out.constraint.clone(),
                                        is_rw: out.is_rw,
                                        is_indirect: out.is_indirect,
                                    }
                                })
                                .collect(),
                asm: asm.clone(),
                asm_str_style: asm_str_style,
                clobbers: clobbers.clone().into(),
                volatile: volatile,
                alignstack: alignstack,
                dialect: dialect,
                expn_id: expn_id,
            }, outputs.iter().map(|out| lower_expr(lctx, &out.expr)).collect(),
               inputs.iter().map(|&(_, ref input)| lower_expr(lctx, input)).collect()),
            ExprKind::Struct(ref path, ref fields, ref maybe_expr) => {
                hir::ExprStruct(lower_path(lctx, path),
                                fields.iter().map(|x| lower_field(lctx, x)).collect(),
                                maybe_expr.as_ref().map(|x| lower_expr(lctx, x)))
            }
            ExprKind::Paren(ref ex) => {
                // merge attributes into the inner expression.
                return lower_expr(lctx, ex).map(|mut ex| {
                    ex.attrs.update(|attrs| {
                        attrs.prepend(e.attrs.clone())
                    });
                    ex
                });
            }

            // Desugar ExprIfLet
            // From: `if let <pat> = <sub_expr> <body> [<else_opt>]`
            ExprKind::IfLet(ref pat, ref sub_expr, ref body, ref else_opt) => {
                // to:
                //
                //   match <sub_expr> {
                //     <pat> => <body>,
                //     [_ if <else_opt_if_cond> => <else_opt_if_body>,]
                //     _ => [<else_opt> | ()]
                //   }

                return cache_ids(lctx, e.id, |lctx| {
                    // `<pat> => <body>`
                    let pat_arm = {
                        let body = lower_block(lctx, body);
                        let body_expr = expr_block(lctx, body, None);
                        arm(hir_vec![lower_pat(lctx, pat)], body_expr)
                    };

                    // `[_ if <else_opt_if_cond> => <else_opt_if_body>,]`
                    let mut else_opt = else_opt.as_ref().map(|e| lower_expr(lctx, e));
                    let else_if_arms = {
                        let mut arms = vec![];
                        loop {
                            let else_opt_continue = else_opt.and_then(|els| {
                                els.and_then(|els| {
                                    match els.node {
                                        // else if
                                        hir::ExprIf(cond, then, else_opt) => {
                                            let pat_under = pat_wild(lctx, e.span);
                                            arms.push(hir::Arm {
                                                attrs: hir_vec![],
                                                pats: hir_vec![pat_under],
                                                guard: Some(cond),
                                                body: expr_block(lctx, then, None),
                                            });
                                            else_opt.map(|else_opt| (else_opt, true))
                                        }
                                        _ => Some((P(els), false)),
                                    }
                                })
                            });
                            match else_opt_continue {
                                Some((e, true)) => {
                                    else_opt = Some(e);
                                }
                                Some((e, false)) => {
                                    else_opt = Some(e);
                                    break;
                                }
                                None => {
                                    else_opt = None;
                                    break;
                                }
                            }
                        }
                        arms
                    };

                    let contains_else_clause = else_opt.is_some();

                    // `_ => [<else_opt> | ()]`
                    let else_arm = {
                        let pat_under = pat_wild(lctx, e.span);
                        let else_expr =
                            else_opt.unwrap_or_else(
                                || expr_tuple(lctx, e.span, hir_vec![], None));
                        arm(hir_vec![pat_under], else_expr)
                    };

                    let mut arms = Vec::with_capacity(else_if_arms.len() + 2);
                    arms.push(pat_arm);
                    arms.extend(else_if_arms);
                    arms.push(else_arm);

                    let sub_expr = lower_expr(lctx, sub_expr);
                    // add attributes to the outer returned expr node
                    expr(lctx,
                         e.span,
                         hir::ExprMatch(sub_expr,
                                        arms.into(),
                                        hir::MatchSource::IfLetDesugar {
                                            contains_else_clause: contains_else_clause,
                                        }),
                         e.attrs.clone())
                });
            }

            // Desugar ExprWhileLet
            // From: `[opt_ident]: while let <pat> = <sub_expr> <body>`
            ExprKind::WhileLet(ref pat, ref sub_expr, ref body, opt_ident) => {
                // to:
                //
                //   [opt_ident]: loop {
                //     match <sub_expr> {
                //       <pat> => <body>,
                //       _ => break
                //     }
                //   }

                return cache_ids(lctx, e.id, |lctx| {
                    // `<pat> => <body>`
                    let pat_arm = {
                        let body = lower_block(lctx, body);
                        let body_expr = expr_block(lctx, body, None);
                        arm(hir_vec![lower_pat(lctx, pat)], body_expr)
                    };

                    // `_ => break`
                    let break_arm = {
                        let pat_under = pat_wild(lctx, e.span);
                        let break_expr = expr_break(lctx, e.span, None);
                        arm(hir_vec![pat_under], break_expr)
                    };

                    // `match <sub_expr> { ... }`
                    let arms = hir_vec![pat_arm, break_arm];
                    let sub_expr = lower_expr(lctx, sub_expr);
                    let match_expr = expr(lctx,
                                          e.span,
                                          hir::ExprMatch(sub_expr,
                                                         arms,
                                                         hir::MatchSource::WhileLetDesugar),
                                          None);

                    // `[opt_ident]: loop { ... }`
                    let loop_block = block_expr(lctx, match_expr);
                    let loop_expr = hir::ExprLoop(loop_block,
                                                  opt_ident.map(|ident| lower_ident(lctx, ident)));
                    // add attributes to the outer returned expr node
                    expr(lctx, e.span, loop_expr, e.attrs.clone())
                });
            }

            // Desugar ExprForLoop
            // From: `[opt_ident]: for <pat> in <head> <body>`
            ExprKind::ForLoop(ref pat, ref head, ref body, opt_ident) => {
                // to:
                //
                //   {
                //     let result = match ::std::iter::IntoIterator::into_iter(<head>) {
                //       mut iter => {
                //         [opt_ident]: loop {
                //           match ::std::iter::Iterator::next(&mut iter) {
                //             ::std::option::Option::Some(<pat>) => <body>,
                //             ::std::option::Option::None => break
                //           }
                //         }
                //       }
                //     };
                //     result
                //   }

                return cache_ids(lctx, e.id, |lctx| {
                    // expand <head>
                    let head = lower_expr(lctx, head);

                    let iter = lctx.str_to_ident("iter");

                    // `::std::option::Option::Some(<pat>) => <body>`
                    let pat_arm = {
                        let body_block = lower_block(lctx, body);
                        let body_span = body_block.span;
                        let body_expr = P(hir::Expr {
                            id: lctx.next_id(),
                            node: hir::ExprBlock(body_block),
                            span: body_span,
                            attrs: None,
                        });
                        let pat = lower_pat(lctx, pat);
                        let some_pat = pat_some(lctx, e.span, pat);

                        arm(hir_vec![some_pat], body_expr)
                    };

                    // `::std::option::Option::None => break`
                    let break_arm = {
                        let break_expr = expr_break(lctx, e.span, None);

                        arm(hir_vec![pat_none(lctx, e.span)], break_expr)
                    };

                    // `match ::std::iter::Iterator::next(&mut iter) { ... }`
                    let match_expr = {
                        let next_path = {
                            let strs = std_path(lctx, &["iter", "Iterator", "next"]);

                            path_global(e.span, strs)
                        };
                        let iter = expr_ident(lctx, e.span, iter, None);
                        let ref_mut_iter = expr_mut_addr_of(lctx, e.span, iter, None);
                        let next_path = expr_path(lctx, next_path, None);
                        let next_expr = expr_call(lctx,
                                                  e.span,
                                                  next_path,
                                                  hir_vec![ref_mut_iter],
                                                  None);
                        let arms = hir_vec![pat_arm, break_arm];

                        expr(lctx,
                             e.span,
                             hir::ExprMatch(next_expr, arms, hir::MatchSource::ForLoopDesugar),
                             None)
                    };

                    // `[opt_ident]: loop { ... }`
                    let loop_block = block_expr(lctx, match_expr);
                    let loop_expr = hir::ExprLoop(loop_block,
                                                  opt_ident.map(|ident| lower_ident(lctx, ident)));
                    let loop_expr = expr(lctx, e.span, loop_expr, None);

                    // `mut iter => { ... }`
                    let iter_arm = {
                        let iter_pat = pat_ident_binding_mode(lctx,
                                                              e.span,
                                                              iter,
                                                              hir::BindByValue(hir::MutMutable));
                        arm(hir_vec![iter_pat], loop_expr)
                    };

                    // `match ::std::iter::IntoIterator::into_iter(<head>) { ... }`
                    let into_iter_expr = {
                        let into_iter_path = {
                            let strs = std_path(lctx, &["iter", "IntoIterator", "into_iter"]);

                            path_global(e.span, strs)
                        };

                        let into_iter = expr_path(lctx, into_iter_path, None);
                        expr_call(lctx, e.span, into_iter, hir_vec![head], None)
                    };

                    let match_expr = expr_match(lctx,
                                                e.span,
                                                into_iter_expr,
                                                hir_vec![iter_arm],
                                                hir::MatchSource::ForLoopDesugar,
                                                None);

                    // `{ let _result = ...; _result }`
                    // underscore prevents an unused_variables lint if the head diverges
                    let result_ident = lctx.str_to_ident("_result");
                    let let_stmt = stmt_let(lctx,
                                            e.span,
                                            false,
                                            result_ident,
                                            match_expr,
                                            None);
                    let result = expr_ident(lctx, e.span, result_ident, None);
                    let block = block_all(lctx, e.span, hir_vec![let_stmt], Some(result));
                    // add the attributes to the outer returned expr node
                    expr_block(lctx, block, e.attrs.clone())
                });
            }

            // Desugar ExprKind::Try
            // From: `<expr>?`
            ExprKind::Try(ref sub_expr) => {
                // to:
                //
                // {
                //     match <expr> {
                //         Ok(val) => val,
                //         Err(err) => {
                //             return Err(From::from(err))
                //         }
                //     }
                // }

                return cache_ids(lctx, e.id, |lctx| {
                    // expand <expr>
                    let sub_expr = lower_expr(lctx, sub_expr);

                    // Ok(val) => val
                    let ok_arm = {
                        let val_ident = lctx.str_to_ident("val");
                        let val_pat = pat_ident(lctx, e.span, val_ident);
                        let val_expr = expr_ident(lctx, e.span, val_ident, None);
                        let ok_pat = pat_ok(lctx, e.span, val_pat);

                        arm(hir_vec![ok_pat], val_expr)
                    };

                    // Err(err) => return Err(From::from(err))
                    let err_arm = {
                        let err_ident = lctx.str_to_ident("err");
                        let from_expr = {
                            let path = std_path(lctx, &["convert", "From", "from"]);
                            let path = path_global(e.span, path);
                            let from = expr_path(lctx, path, None);
                            let err_expr = expr_ident(lctx, e.span, err_ident, None);

                            expr_call(lctx, e.span, from, hir_vec![err_expr], None)
                        };
                        let err_expr = {
                            let path = std_path(lctx, &["result", "Result", "Err"]);
                            let path = path_global(e.span, path);
                            let err_ctor = expr_path(lctx, path, None);
                            expr_call(lctx, e.span, err_ctor, hir_vec![from_expr], None)
                        };
                        let err_pat = pat_err(lctx, e.span,
                                              pat_ident(lctx, e.span, err_ident));
                        let ret_expr = expr(lctx, e.span,
                                            hir::Expr_::ExprRet(Some(err_expr)), None);

                        arm(hir_vec![err_pat], ret_expr)
                    };

                    expr_match(lctx, e.span, sub_expr, hir_vec![err_arm, ok_arm],
                               hir::MatchSource::TryDesugar, None)
                })
            }

            ExprKind::Mac(_) => panic!("Shouldn't exist here"),
        },
        span: e.span,
        attrs: e.attrs.clone(),
    })
}

pub fn lower_stmt(lctx: &LoweringContext, s: &Stmt) -> hir::Stmt {
    match s.node {
        StmtKind::Decl(ref d, id) => {
            Spanned {
                node: hir::StmtDecl(lower_decl(lctx, d), id),
                span: s.span,
            }
        }
        StmtKind::Expr(ref e, id) => {
            Spanned {
                node: hir::StmtExpr(lower_expr(lctx, e), id),
                span: s.span,
            }
        }
        StmtKind::Semi(ref e, id) => {
            Spanned {
                node: hir::StmtSemi(lower_expr(lctx, e), id),
                span: s.span,
            }
        }
        StmtKind::Mac(..) => panic!("Shouldn't exist here"),
    }
}

pub fn lower_capture_clause(_lctx: &LoweringContext, c: CaptureBy) -> hir::CaptureClause {
    match c {
        CaptureBy::Value => hir::CaptureByValue,
        CaptureBy::Ref => hir::CaptureByRef,
    }
}

pub fn lower_visibility(lctx: &LoweringContext, v: &Visibility) -> hir::Visibility {
    match *v {
        Visibility::Public => hir::Public,
        Visibility::Crate(_) => hir::Visibility::Crate,
        Visibility::Restricted { ref path, id } =>
            hir::Visibility::Restricted { path: P(lower_path(lctx, path)), id: id },
        Visibility::Inherited => hir::Inherited,
    }
}

pub fn lower_defaultness(_lctx: &LoweringContext, d: Defaultness) -> hir::Defaultness {
    match d {
        Defaultness::Default => hir::Defaultness::Default,
        Defaultness::Final => hir::Defaultness::Final,
    }
}

pub fn lower_block_check_mode(lctx: &LoweringContext, b: &BlockCheckMode) -> hir::BlockCheckMode {
    match *b {
        BlockCheckMode::Default => hir::DefaultBlock,
        BlockCheckMode::Unsafe(u) => hir::UnsafeBlock(lower_unsafe_source(lctx, u)),
    }
}

pub fn lower_binding_mode(lctx: &LoweringContext, b: &BindingMode) -> hir::BindingMode {
    match *b {
        BindingMode::ByRef(m) => hir::BindByRef(lower_mutability(lctx, m)),
        BindingMode::ByValue(m) => hir::BindByValue(lower_mutability(lctx, m)),
    }
}

pub fn lower_unsafe_source(_lctx: &LoweringContext, u: UnsafeSource) -> hir::UnsafeSource {
    match u {
        CompilerGenerated => hir::CompilerGenerated,
        UserProvided => hir::UserProvided,
    }
}

pub fn lower_impl_polarity(_lctx: &LoweringContext, i: ImplPolarity) -> hir::ImplPolarity {
    match i {
        ImplPolarity::Positive => hir::ImplPolarity::Positive,
        ImplPolarity::Negative => hir::ImplPolarity::Negative,
    }
}

pub fn lower_trait_bound_modifier(_lctx: &LoweringContext,
                                  f: TraitBoundModifier)
                                  -> hir::TraitBoundModifier {
    match f {
        TraitBoundModifier::None => hir::TraitBoundModifier::None,
        TraitBoundModifier::Maybe => hir::TraitBoundModifier::Maybe,
    }
}

// Helper methods for building HIR.

fn arm(pats: hir::HirVec<P<hir::Pat>>, expr: P<hir::Expr>) -> hir::Arm {
    hir::Arm {
        attrs: hir_vec![],
        pats: pats,
        guard: None,
        body: expr,
    }
}

fn field(name: Name, expr: P<hir::Expr>, span: Span) -> hir::Field {
    hir::Field {
        name: Spanned {
            node: name,
            span: span,
        },
        span: span,
        expr: expr,
    }
}

fn expr_break(lctx: &LoweringContext, span: Span,
              attrs: ThinAttributes) -> P<hir::Expr> {
    expr(lctx, span, hir::ExprBreak(None), attrs)
}

fn expr_call(lctx: &LoweringContext,
             span: Span,
             e: P<hir::Expr>,
             args: hir::HirVec<P<hir::Expr>>,
             attrs: ThinAttributes)
             -> P<hir::Expr> {
    expr(lctx, span, hir::ExprCall(e, args), attrs)
}

fn expr_ident(lctx: &LoweringContext, span: Span, id: hir::Ident,
              attrs: ThinAttributes) -> P<hir::Expr> {
    expr_path(lctx, path_ident(span, id), attrs)
}

fn expr_mut_addr_of(lctx: &LoweringContext, span: Span, e: P<hir::Expr>,
                    attrs: ThinAttributes) -> P<hir::Expr> {
    expr(lctx, span, hir::ExprAddrOf(hir::MutMutable, e), attrs)
}

fn expr_path(lctx: &LoweringContext, path: hir::Path,
             attrs: ThinAttributes) -> P<hir::Expr> {
    expr(lctx, path.span, hir::ExprPath(None, path), attrs)
}

fn expr_match(lctx: &LoweringContext,
              span: Span,
              arg: P<hir::Expr>,
              arms: hir::HirVec<hir::Arm>,
              source: hir::MatchSource,
              attrs: ThinAttributes)
              -> P<hir::Expr> {
    expr(lctx, span, hir::ExprMatch(arg, arms, source), attrs)
}

fn expr_block(lctx: &LoweringContext, b: P<hir::Block>,
              attrs: ThinAttributes) -> P<hir::Expr> {
    expr(lctx, b.span, hir::ExprBlock(b), attrs)
}

fn expr_tuple(lctx: &LoweringContext, sp: Span, exprs: hir::HirVec<P<hir::Expr>>,
              attrs: ThinAttributes) -> P<hir::Expr> {
    expr(lctx, sp, hir::ExprTup(exprs), attrs)
}

fn expr_struct(lctx: &LoweringContext,
               sp: Span,
               path: hir::Path,
               fields: hir::HirVec<hir::Field>,
               e: Option<P<hir::Expr>>,
               attrs: ThinAttributes) -> P<hir::Expr> {
    expr(lctx, sp, hir::ExprStruct(path, fields, e), attrs)
}

fn expr(lctx: &LoweringContext, span: Span, node: hir::Expr_,
        attrs: ThinAttributes) -> P<hir::Expr> {
    P(hir::Expr {
        id: lctx.next_id(),
        node: node,
        span: span,
        attrs: attrs,
    })
}

fn stmt_let(lctx: &LoweringContext,
            sp: Span,
            mutbl: bool,
            ident: hir::Ident,
            ex: P<hir::Expr>,
            attrs: ThinAttributes)
            -> hir::Stmt {
    let pat = if mutbl {
        pat_ident_binding_mode(lctx, sp, ident, hir::BindByValue(hir::MutMutable))
    } else {
        pat_ident(lctx, sp, ident)
    };
    let local = P(hir::Local {
        pat: pat,
        ty: None,
        init: Some(ex),
        id: lctx.next_id(),
        span: sp,
        attrs: attrs,
    });
    let decl = respan(sp, hir::DeclLocal(local));
    respan(sp, hir::StmtDecl(P(decl), lctx.next_id()))
}

fn block_expr(lctx: &LoweringContext, expr: P<hir::Expr>) -> P<hir::Block> {
    block_all(lctx, expr.span, hir::HirVec::new(), Some(expr))
}

fn block_all(lctx: &LoweringContext,
             span: Span,
             stmts: hir::HirVec<hir::Stmt>,
             expr: Option<P<hir::Expr>>)
             -> P<hir::Block> {
    P(hir::Block {
        stmts: stmts,
        expr: expr,
        id: lctx.next_id(),
        rules: hir::DefaultBlock,
        span: span,
    })
}

fn pat_ok(lctx: &LoweringContext, span: Span, pat: P<hir::Pat>) -> P<hir::Pat> {
    let ok = std_path(lctx, &["result", "Result", "Ok"]);
    let path = path_global(span, ok);
    pat_enum(lctx, span, path, hir_vec![pat])
}

fn pat_err(lctx: &LoweringContext, span: Span, pat: P<hir::Pat>) -> P<hir::Pat> {
    let err = std_path(lctx, &["result", "Result", "Err"]);
    let path = path_global(span, err);
    pat_enum(lctx, span, path, hir_vec![pat])
}

fn pat_some(lctx: &LoweringContext, span: Span, pat: P<hir::Pat>) -> P<hir::Pat> {
    let some = std_path(lctx, &["option", "Option", "Some"]);
    let path = path_global(span, some);
    pat_enum(lctx, span, path, hir_vec![pat])
}

fn pat_none(lctx: &LoweringContext, span: Span) -> P<hir::Pat> {
    let none = std_path(lctx, &["option", "Option", "None"]);
    let path = path_global(span, none);
    pat_enum(lctx, span, path, hir_vec![])
}

fn pat_enum(lctx: &LoweringContext,
            span: Span,
            path: hir::Path,
            subpats: hir::HirVec<P<hir::Pat>>)
            -> P<hir::Pat> {
    let pt = if subpats.is_empty() {
        hir::PatKind::Path(path)
    } else {
        hir::PatKind::TupleStruct(path, Some(subpats))
    };
    pat(lctx, span, pt)
}

fn pat_ident(lctx: &LoweringContext, span: Span, ident: hir::Ident) -> P<hir::Pat> {
    pat_ident_binding_mode(lctx, span, ident, hir::BindByValue(hir::MutImmutable))
}

fn pat_ident_binding_mode(lctx: &LoweringContext,
                          span: Span,
                          ident: hir::Ident,
                          bm: hir::BindingMode)
                          -> P<hir::Pat> {
    let pat_ident = hir::PatKind::Ident(bm,
                                        Spanned {
                                            span: span,
                                            node: ident,
                                        },
                                        None);

    let pat = pat(lctx, span, pat_ident);

    if let Some(defs) = lctx.definitions {
        let mut defs = defs.borrow_mut();
        defs.create_def_with_parent(lctx.parent_def.get(),
                                    pat.id,
                                    DefPathData::Binding(ident.name));
    }

    pat
}

fn pat_wild(lctx: &LoweringContext, span: Span) -> P<hir::Pat> {
    pat(lctx, span, hir::PatKind::Wild)
}

fn pat(lctx: &LoweringContext, span: Span, pat: hir::PatKind) -> P<hir::Pat> {
    P(hir::Pat {
        id: lctx.next_id(),
        node: pat,
        span: span,
    })
}

fn path_ident(span: Span, id: hir::Ident) -> hir::Path {
    path(span, vec![id])
}

fn path(span: Span, strs: Vec<hir::Ident>) -> hir::Path {
    path_all(span, false, strs, hir::HirVec::new(), hir::HirVec::new(), hir::HirVec::new())
}

fn path_global(span: Span, strs: Vec<hir::Ident>) -> hir::Path {
    path_all(span, true, strs, hir::HirVec::new(), hir::HirVec::new(), hir::HirVec::new())
}

fn path_all(sp: Span,
            global: bool,
            mut idents: Vec<hir::Ident>,
            lifetimes: hir::HirVec<hir::Lifetime>,
            types: hir::HirVec<P<hir::Ty>>,
            bindings: hir::HirVec<hir::TypeBinding>)
            -> hir::Path {
    let last_identifier = idents.pop().unwrap();
    let mut segments: Vec<hir::PathSegment> = idents.into_iter()
                                                    .map(|ident| {
                                                        hir::PathSegment {
                                                            identifier: ident,
                                                            parameters: hir::PathParameters::none(),
                                                        }
                                                    })
                                                    .collect();
    segments.push(hir::PathSegment {
        identifier: last_identifier,
        parameters: hir::AngleBracketedParameters(hir::AngleBracketedParameterData {
            lifetimes: lifetimes,
            types: types,
            bindings: bindings,
        }),
    });
    hir::Path {
        span: sp,
        global: global,
        segments: segments.into(),
    }
}

fn std_path(lctx: &LoweringContext, components: &[&str]) -> Vec<hir::Ident> {
    let mut v = Vec::new();
    if let Some(s) = lctx.crate_root {
        v.push(hir::Ident::from_name(token::intern(s)));
    }
    v.extend(components.iter().map(|s| hir::Ident::from_name(token::intern(s))));
    return v;
}

// Given suffix ["b","c","d"], returns path `::std::b::c::d` when
// `fld.cx.use_std`, and `::core::b::c::d` otherwise.
fn core_path(lctx: &LoweringContext, span: Span, components: &[&str]) -> hir::Path {
    let idents = std_path(lctx, components);
    path_global(span, idents)
}

fn signal_block_expr(lctx: &LoweringContext,
                     stmts: hir::HirVec<hir::Stmt>,
                     expr: P<hir::Expr>,
                     span: Span,
                     rule: hir::BlockCheckMode,
                     attrs: ThinAttributes)
                     -> P<hir::Expr> {
    let id = lctx.next_id();
    expr_block(lctx,
               P(hir::Block {
                   rules: rule,
                   span: span,
                   id: id,
                   stmts: stmts,
                   expr: Some(expr),
               }),
               attrs)
}



#[cfg(test)]
mod test {
    use super::*;
    use syntax::ast::{self, NodeId, NodeIdAssigner};
    use syntax::{parse, codemap};
    use syntax::fold::Folder;
    use std::cell::Cell;

    struct MockAssigner {
        next_id: Cell<NodeId>,
    }

    impl MockAssigner {
        fn new() -> MockAssigner {
            MockAssigner { next_id: Cell::new(0) }
        }
    }

    trait FakeExtCtxt {
        fn call_site(&self) -> codemap::Span;
        fn cfg(&self) -> ast::CrateConfig;
        fn ident_of(&self, st: &str) -> ast::Ident;
        fn name_of(&self, st: &str) -> ast::Name;
        fn parse_sess(&self) -> &parse::ParseSess;
    }

    impl FakeExtCtxt for parse::ParseSess {
        fn call_site(&self) -> codemap::Span {
            codemap::Span {
                lo: codemap::BytePos(0),
                hi: codemap::BytePos(0),
                expn_id: codemap::NO_EXPANSION,
            }
        }
        fn cfg(&self) -> ast::CrateConfig {
            Vec::new()
        }
        fn ident_of(&self, st: &str) -> ast::Ident {
            parse::token::str_to_ident(st)
        }
        fn name_of(&self, st: &str) -> ast::Name {
            parse::token::intern(st)
        }
        fn parse_sess(&self) -> &parse::ParseSess {
            self
        }
    }

    impl NodeIdAssigner for MockAssigner {
        fn next_node_id(&self) -> NodeId {
            let result = self.next_id.get();
            self.next_id.set(result + 1);
            result
        }

        fn peek_node_id(&self) -> NodeId {
            self.next_id.get()
        }
    }

    impl Folder for MockAssigner {
        fn new_id(&mut self, old_id: NodeId) -> NodeId {
            assert_eq!(old_id, ast::DUMMY_NODE_ID);
            self.next_node_id()
        }
    }

    #[test]
    fn test_preserves_ids() {
        let cx = parse::ParseSess::new();
        let mut assigner = MockAssigner::new();

        let ast_if_let = quote_expr!(&cx,
                                     if let Some(foo) = baz {
                                         bar(foo);
                                     });
        let ast_if_let = assigner.fold_expr(ast_if_let);
        let ast_while_let = quote_expr!(&cx,
                                        while let Some(foo) = baz {
                                            bar(foo);
                                        });
        let ast_while_let = assigner.fold_expr(ast_while_let);
        let ast_for = quote_expr!(&cx,
                                  for i in 0..10 {
                                      for j in 0..10 {
                                          foo(i, j);
                                      }
                                  });
        let ast_for = assigner.fold_expr(ast_for);
        let ast_in = quote_expr!(&cx, in HEAP { foo() });
        let ast_in = assigner.fold_expr(ast_in);

        let lctx = LoweringContext::testing_context(&assigner);

        let hir1 = lower_expr(&lctx, &ast_if_let);
        let hir2 = lower_expr(&lctx, &ast_if_let);
        assert!(hir1 == hir2);

        let hir1 = lower_expr(&lctx, &ast_while_let);
        let hir2 = lower_expr(&lctx, &ast_while_let);
        assert!(hir1 == hir2);

        let hir1 = lower_expr(&lctx, &ast_for);
        let hir2 = lower_expr(&lctx, &ast_for);
        assert!(hir1 == hir2);

        let hir1 = lower_expr(&lctx, &ast_in);
        let hir2 = lower_expr(&lctx, &ast_in);
        assert!(hir1 == hir2);
    }
}
