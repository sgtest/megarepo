// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![crate_name = "rustc_privacy"]
#![crate_type = "dylib"]
#![crate_type = "rlib"]
#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
       html_root_url = "https://doc.rust-lang.org/nightly/")]
#![deny(warnings)]

#![feature(rustc_diagnostic_macros)]

#![cfg_attr(stage0, unstable(feature = "rustc_private", issue = "27812"))]
#![cfg_attr(stage0, feature(rustc_private))]
#![cfg_attr(stage0, feature(staged_api))]

extern crate rustc;
#[macro_use] extern crate syntax;
extern crate syntax_pos;

use rustc::hir::{self, PatKind};
use rustc::hir::def::Def;
use rustc::hir::def_id::{LOCAL_CRATE, CrateNum, DefId};
use rustc::hir::intravisit::{self, Visitor, NestedVisitorMap};
use rustc::hir::itemlikevisit::DeepVisitor;
use rustc::lint;
use rustc::middle::privacy::{AccessLevel, AccessLevels};
use rustc::ty::{self, TyCtxt, Ty, TypeFoldable};
use rustc::ty::fold::TypeVisitor;
use rustc::ty::maps::Providers;
use rustc::util::nodemap::NodeSet;
use syntax::ast::{self, CRATE_NODE_ID, Ident};
use syntax::symbol::keywords;
use syntax_pos::Span;

use std::cmp;
use std::mem::replace;
use std::rc::Rc;

pub mod diagnostics;

////////////////////////////////////////////////////////////////////////////////
/// Visitor used to determine if pub(restricted) is used anywhere in the crate.
///
/// This is done so that `private_in_public` warnings can be turned into hard errors
/// in crates that have been updated to use pub(restricted).
////////////////////////////////////////////////////////////////////////////////
struct PubRestrictedVisitor<'a, 'tcx: 'a> {
    tcx:  TyCtxt<'a, 'tcx, 'tcx>,
    has_pub_restricted: bool,
}

impl<'a, 'tcx> Visitor<'tcx> for PubRestrictedVisitor<'a, 'tcx> {
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'tcx> {
        NestedVisitorMap::All(&self.tcx.hir)
    }
    fn visit_vis(&mut self, vis: &'tcx hir::Visibility) {
        self.has_pub_restricted = self.has_pub_restricted || vis.is_pub_restricted();
    }
}

////////////////////////////////////////////////////////////////////////////////
/// The embargo visitor, used to determine the exports of the ast
////////////////////////////////////////////////////////////////////////////////

struct EmbargoVisitor<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,

    // Accessibility levels for reachable nodes
    access_levels: AccessLevels,
    // Previous accessibility level, None means unreachable
    prev_level: Option<AccessLevel>,
    // Have something changed in the level map?
    changed: bool,
}

struct ReachEverythingInTheInterfaceVisitor<'b, 'a: 'b, 'tcx: 'a> {
    item_def_id: DefId,
    ev: &'b mut EmbargoVisitor<'a, 'tcx>,
}

impl<'a, 'tcx> EmbargoVisitor<'a, 'tcx> {
    fn item_ty_level(&self, item_def_id: DefId) -> Option<AccessLevel> {
        let ty_def_id = match self.tcx.type_of(item_def_id).sty {
            ty::TyAdt(adt, _) => adt.did,
            ty::TyDynamic(ref obj, ..) if obj.principal().is_some() =>
                obj.principal().unwrap().def_id(),
            ty::TyProjection(ref proj) => proj.trait_ref.def_id,
            _ => return Some(AccessLevel::Public)
        };
        if let Some(node_id) = self.tcx.hir.as_local_node_id(ty_def_id) {
            self.get(node_id)
        } else {
            Some(AccessLevel::Public)
        }
    }

    fn impl_trait_level(&self, impl_def_id: DefId) -> Option<AccessLevel> {
        if let Some(trait_ref) = self.tcx.impl_trait_ref(impl_def_id) {
            if let Some(node_id) = self.tcx.hir.as_local_node_id(trait_ref.def_id) {
                return self.get(node_id);
            }
        }
        Some(AccessLevel::Public)
    }

    fn get(&self, id: ast::NodeId) -> Option<AccessLevel> {
        self.access_levels.map.get(&id).cloned()
    }

    // Updates node level and returns the updated level
    fn update(&mut self, id: ast::NodeId, level: Option<AccessLevel>) -> Option<AccessLevel> {
        let old_level = self.get(id);
        // Accessibility levels can only grow
        if level > old_level {
            self.access_levels.map.insert(id, level.unwrap());
            self.changed = true;
            level
        } else {
            old_level
        }
    }

    fn reach<'b>(&'b mut self, item_id: ast::NodeId)
                 -> ReachEverythingInTheInterfaceVisitor<'b, 'a, 'tcx> {
        ReachEverythingInTheInterfaceVisitor {
            item_def_id: self.tcx.hir.local_def_id(item_id),
            ev: self,
        }
    }
}

impl<'a, 'tcx> Visitor<'tcx> for EmbargoVisitor<'a, 'tcx> {
    /// We want to visit items in the context of their containing
    /// module and so forth, so supply a crate for doing a deep walk.
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'tcx> {
        NestedVisitorMap::All(&self.tcx.hir)
    }

    fn visit_item(&mut self, item: &'tcx hir::Item) {
        let inherited_item_level = match item.node {
            // Impls inherit level from their types and traits
            hir::ItemImpl(..) => {
                let def_id = self.tcx.hir.local_def_id(item.id);
                cmp::min(self.item_ty_level(def_id), self.impl_trait_level(def_id))
            }
            hir::ItemDefaultImpl(..) => {
                let def_id = self.tcx.hir.local_def_id(item.id);
                self.impl_trait_level(def_id)
            }
            // Foreign mods inherit level from parents
            hir::ItemForeignMod(..) => {
                self.prev_level
            }
            // Other `pub` items inherit levels from parents
            hir::ItemConst(..) | hir::ItemEnum(..) | hir::ItemExternCrate(..) |
            hir::ItemGlobalAsm(..) | hir::ItemFn(..) | hir::ItemMod(..) |
            hir::ItemStatic(..) | hir::ItemStruct(..) | hir::ItemTrait(..) |
            hir::ItemTy(..) | hir::ItemUnion(..) | hir::ItemUse(..) => {
                if item.vis == hir::Public { self.prev_level } else { None }
            }
        };

        // Update level of the item itself
        let item_level = self.update(item.id, inherited_item_level);

        // Update levels of nested things
        match item.node {
            hir::ItemEnum(ref def, _) => {
                for variant in &def.variants {
                    let variant_level = self.update(variant.node.data.id(), item_level);
                    for field in variant.node.data.fields() {
                        self.update(field.id, variant_level);
                    }
                }
            }
            hir::ItemImpl(.., None, _, ref impl_item_refs) => {
                for impl_item_ref in impl_item_refs {
                    if impl_item_ref.vis == hir::Public {
                        self.update(impl_item_ref.id.node_id, item_level);
                    }
                }
            }
            hir::ItemImpl(.., Some(_), _, ref impl_item_refs) => {
                for impl_item_ref in impl_item_refs {
                    self.update(impl_item_ref.id.node_id, item_level);
                }
            }
            hir::ItemTrait(.., ref trait_item_refs) => {
                for trait_item_ref in trait_item_refs {
                    self.update(trait_item_ref.id.node_id, item_level);
                }
            }
            hir::ItemStruct(ref def, _) | hir::ItemUnion(ref def, _) => {
                if !def.is_struct() {
                    self.update(def.id(), item_level);
                }
                for field in def.fields() {
                    if field.vis == hir::Public {
                        self.update(field.id, item_level);
                    }
                }
            }
            hir::ItemForeignMod(ref foreign_mod) => {
                for foreign_item in &foreign_mod.items {
                    if foreign_item.vis == hir::Public {
                        self.update(foreign_item.id, item_level);
                    }
                }
            }
            hir::ItemUse(..) | hir::ItemStatic(..) | hir::ItemConst(..) |
            hir::ItemGlobalAsm(..) | hir::ItemTy(..) | hir::ItemMod(..) |
            hir::ItemFn(..) | hir::ItemExternCrate(..) | hir::ItemDefaultImpl(..) => {}
        }

        // Mark all items in interfaces of reachable items as reachable
        match item.node {
            // The interface is empty
            hir::ItemExternCrate(..) => {}
            // All nested items are checked by visit_item
            hir::ItemMod(..) => {}
            // Reexports are handled in visit_mod
            hir::ItemUse(..) => {}
            // The interface is empty
            hir::ItemDefaultImpl(..) => {}
            // The interface is empty
            hir::ItemGlobalAsm(..) => {}
            // Visit everything
            hir::ItemConst(..) | hir::ItemStatic(..) |
            hir::ItemFn(..) | hir::ItemTy(..) => {
                if item_level.is_some() {
                    self.reach(item.id).generics().predicates().ty();
                }
            }
            hir::ItemTrait(.., ref trait_item_refs) => {
                if item_level.is_some() {
                    self.reach(item.id).generics().predicates();

                    for trait_item_ref in trait_item_refs {
                        let mut reach = self.reach(trait_item_ref.id.node_id);
                        reach.generics().predicates();

                        if trait_item_ref.kind == hir::AssociatedItemKind::Type &&
                           !trait_item_ref.defaultness.has_value() {
                            // No type to visit.
                        } else {
                            reach.ty();
                        }
                    }
                }
            }
            // Visit everything except for private impl items
            hir::ItemImpl(.., ref trait_ref, _, ref impl_item_refs) => {
                if item_level.is_some() {
                    self.reach(item.id).generics().predicates().impl_trait_ref();

                    for impl_item_ref in impl_item_refs {
                        let id = impl_item_ref.id.node_id;
                        if trait_ref.is_some() || self.get(id).is_some() {
                            self.reach(id).generics().predicates().ty();
                        }
                    }
                }
            }

            // Visit everything, but enum variants have their own levels
            hir::ItemEnum(ref def, _) => {
                if item_level.is_some() {
                    self.reach(item.id).generics().predicates();
                }
                for variant in &def.variants {
                    if self.get(variant.node.data.id()).is_some() {
                        for field in variant.node.data.fields() {
                            self.reach(field.id).ty();
                        }
                        // Corner case: if the variant is reachable, but its
                        // enum is not, make the enum reachable as well.
                        self.update(item.id, Some(AccessLevel::Reachable));
                    }
                }
            }
            // Visit everything, but foreign items have their own levels
            hir::ItemForeignMod(ref foreign_mod) => {
                for foreign_item in &foreign_mod.items {
                    if self.get(foreign_item.id).is_some() {
                        self.reach(foreign_item.id).generics().predicates().ty();
                    }
                }
            }
            // Visit everything except for private fields
            hir::ItemStruct(ref struct_def, _) |
            hir::ItemUnion(ref struct_def, _) => {
                if item_level.is_some() {
                    self.reach(item.id).generics().predicates();
                    for field in struct_def.fields() {
                        if self.get(field.id).is_some() {
                            self.reach(field.id).ty();
                        }
                    }
                }
            }
        }

        let orig_level = self.prev_level;
        self.prev_level = item_level;

        intravisit::walk_item(self, item);

        self.prev_level = orig_level;
    }

    fn visit_block(&mut self, b: &'tcx hir::Block) {
        let orig_level = replace(&mut self.prev_level, None);

        // Blocks can have public items, for example impls, but they always
        // start as completely private regardless of publicity of a function,
        // constant, type, field, etc. in which this block resides
        intravisit::walk_block(self, b);

        self.prev_level = orig_level;
    }

    fn visit_mod(&mut self, m: &'tcx hir::Mod, _sp: Span, id: ast::NodeId) {
        // This code is here instead of in visit_item so that the
        // crate module gets processed as well.
        if self.prev_level.is_some() {
            if let Some(exports) = self.tcx.export_map.get(&id) {
                for export in exports {
                    if let Some(node_id) = self.tcx.hir.as_local_node_id(export.def.def_id()) {
                        self.update(node_id, Some(AccessLevel::Exported));
                    }
                }
            }
        }

        intravisit::walk_mod(self, m, id);
    }

    fn visit_macro_def(&mut self, md: &'tcx hir::MacroDef) {
        if md.legacy {
            self.update(md.id, Some(AccessLevel::Public));
            return
        }

        let module_did = ty::DefIdTree::parent(self.tcx, self.tcx.hir.local_def_id(md.id)).unwrap();
        let mut module_id = self.tcx.hir.as_local_node_id(module_did).unwrap();
        let level = if md.vis == hir::Public { self.get(module_id) } else { None };
        let level = self.update(md.id, level);
        if level.is_none() {
            return
        }

        loop {
            let module = if module_id == ast::CRATE_NODE_ID {
                &self.tcx.hir.krate().module
            } else if let hir::ItemMod(ref module) = self.tcx.hir.expect_item(module_id).node {
                module
            } else {
                unreachable!()
            };
            for id in &module.item_ids {
                self.update(id.id, level);
            }
            if module_id == ast::CRATE_NODE_ID {
                break
            }
            module_id = self.tcx.hir.get_parent_node(module_id);
        }
    }

    fn visit_ty(&mut self, ty: &'tcx hir::Ty) {
        if let hir::TyImplTrait(..) = ty.node {
            if self.get(ty.id).is_some() {
                // Reach the (potentially private) type and the API being exposed.
                self.reach(ty.id).ty().predicates();
            }
        }

        intravisit::walk_ty(self, ty);
    }
}

impl<'b, 'a, 'tcx> ReachEverythingInTheInterfaceVisitor<'b, 'a, 'tcx> {
    fn generics(&mut self) -> &mut Self {
        for def in &self.ev.tcx.generics_of(self.item_def_id).types {
            if def.has_default {
                self.ev.tcx.type_of(def.def_id).visit_with(self);
            }
        }
        self
    }

    fn predicates(&mut self) -> &mut Self {
        self.ev.tcx.predicates_of(self.item_def_id).visit_with(self);
        self
    }

    fn ty(&mut self) -> &mut Self {
        self.ev.tcx.type_of(self.item_def_id).visit_with(self);
        self
    }

    fn impl_trait_ref(&mut self) -> &mut Self {
        self.ev.tcx.impl_trait_ref(self.item_def_id).visit_with(self);
        self
    }
}

impl<'b, 'a, 'tcx> TypeVisitor<'tcx> for ReachEverythingInTheInterfaceVisitor<'b, 'a, 'tcx> {
    fn visit_ty(&mut self, ty: Ty<'tcx>) -> bool {
        let ty_def_id = match ty.sty {
            ty::TyAdt(adt, _) => Some(adt.did),
            ty::TyDynamic(ref obj, ..) => obj.principal().map(|p| p.def_id()),
            ty::TyProjection(ref proj) => Some(proj.trait_ref.def_id),
            ty::TyFnDef(def_id, ..) |
            ty::TyAnon(def_id, _) => Some(def_id),
            _ => None
        };

        if let Some(def_id) = ty_def_id {
            if let Some(node_id) = self.ev.tcx.hir.as_local_node_id(def_id) {
                self.ev.update(node_id, Some(AccessLevel::Reachable));
            }
        }

        ty.super_visit_with(self)
    }

    fn visit_trait_ref(&mut self, trait_ref: ty::TraitRef<'tcx>) -> bool {
        if let Some(node_id) = self.ev.tcx.hir.as_local_node_id(trait_ref.def_id) {
            let item = self.ev.tcx.hir.expect_item(node_id);
            self.ev.update(item.id, Some(AccessLevel::Reachable));
        }

        trait_ref.super_visit_with(self)
    }
}

//////////////////////////////////////////////////////////////////////////////////////
/// Name privacy visitor, checks privacy and reports violations.
/// Most of name privacy checks are performed during the main resolution phase,
/// or later in type checking when field accesses and associated items are resolved.
/// This pass performs remaining checks for fields in struct expressions and patterns.
//////////////////////////////////////////////////////////////////////////////////////

struct NamePrivacyVisitor<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    tables: &'a ty::TypeckTables<'tcx>,
    current_item: ast::NodeId,
}

impl<'a, 'tcx> NamePrivacyVisitor<'a, 'tcx> {
    // Checks that a field is accessible.
    fn check_field(&mut self, span: Span, def: &'tcx ty::AdtDef, field: &'tcx ty::FieldDef) {
        let ident = Ident { ctxt: span.ctxt.modern(), ..keywords::Invalid.ident() };
        let def_id = self.tcx.adjust_ident(ident, def.did, self.current_item).1;
        if !def.is_enum() && !field.vis.is_accessible_from(def_id, self.tcx) {
            struct_span_err!(self.tcx.sess, span, E0451, "field `{}` of {} `{}` is private",
                             field.name, def.variant_descr(), self.tcx.item_path_str(def.did))
                .span_label(span, format!("field `{}` is private", field.name))
                .emit();
        }
    }
}

impl<'a, 'tcx> Visitor<'tcx> for NamePrivacyVisitor<'a, 'tcx> {
    /// We want to visit items in the context of their containing
    /// module and so forth, so supply a crate for doing a deep walk.
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'tcx> {
        NestedVisitorMap::All(&self.tcx.hir)
    }

    fn visit_nested_body(&mut self, body: hir::BodyId) {
        let orig_tables = replace(&mut self.tables, self.tcx.body_tables(body));
        let body = self.tcx.hir.body(body);
        self.visit_body(body);
        self.tables = orig_tables;
    }

    fn visit_item(&mut self, item: &'tcx hir::Item) {
        let orig_current_item = replace(&mut self.current_item, item.id);
        intravisit::walk_item(self, item);
        self.current_item = orig_current_item;
    }

    fn visit_expr(&mut self, expr: &'tcx hir::Expr) {
        match expr.node {
            hir::ExprStruct(ref qpath, ref fields, ref base) => {
                let def = self.tables.qpath_def(qpath, expr.id);
                let adt = self.tables.expr_ty(expr).ty_adt_def().unwrap();
                let variant = adt.variant_of_def(def);
                if let Some(ref base) = *base {
                    // If the expression uses FRU we need to make sure all the unmentioned fields
                    // are checked for privacy (RFC 736). Rather than computing the set of
                    // unmentioned fields, just check them all.
                    for variant_field in &variant.fields {
                        let field = fields.iter().find(|f| f.name.node == variant_field.name);
                        let span = if let Some(f) = field { f.span } else { base.span };
                        self.check_field(span, adt, variant_field);
                    }
                } else {
                    for field in fields {
                        self.check_field(field.span, adt, variant.field_named(field.name.node));
                    }
                }
            }
            _ => {}
        }

        intravisit::walk_expr(self, expr);
    }

    fn visit_pat(&mut self, pat: &'tcx hir::Pat) {
        match pat.node {
            PatKind::Struct(ref qpath, ref fields, _) => {
                let def = self.tables.qpath_def(qpath, pat.id);
                let adt = self.tables.pat_ty(pat).ty_adt_def().unwrap();
                let variant = adt.variant_of_def(def);
                for field in fields {
                    self.check_field(field.span, adt, variant.field_named(field.node.name));
                }
            }
            _ => {}
        }

        intravisit::walk_pat(self, pat);
    }
}

///////////////////////////////////////////////////////////////////////////////
/// Obsolete visitors for checking for private items in public interfaces.
/// These visitors are supposed to be kept in frozen state and produce an
/// "old error node set". For backward compatibility the new visitor reports
/// warnings instead of hard errors when the erroneous node is not in this old set.
///////////////////////////////////////////////////////////////////////////////

struct ObsoleteVisiblePrivateTypesVisitor<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    access_levels: &'a AccessLevels,
    in_variant: bool,
    // set of errors produced by this obsolete visitor
    old_error_set: NodeSet,
}

struct ObsoleteCheckTypeForPrivatenessVisitor<'a, 'b: 'a, 'tcx: 'b> {
    inner: &'a ObsoleteVisiblePrivateTypesVisitor<'b, 'tcx>,
    /// whether the type refers to private types.
    contains_private: bool,
    /// whether we've recurred at all (i.e. if we're pointing at the
    /// first type on which visit_ty was called).
    at_outer_type: bool,
    // whether that first type is a public path.
    outer_type_is_public_path: bool,
}

impl<'a, 'tcx> ObsoleteVisiblePrivateTypesVisitor<'a, 'tcx> {
    fn path_is_private_type(&self, path: &hir::Path) -> bool {
        let did = match path.def {
            Def::PrimTy(..) | Def::SelfTy(..) => return false,
            def => def.def_id(),
        };

        // A path can only be private if:
        // it's in this crate...
        if let Some(node_id) = self.tcx.hir.as_local_node_id(did) {
            // .. and it corresponds to a private type in the AST (this returns
            // None for type parameters)
            match self.tcx.hir.find(node_id) {
                Some(hir::map::NodeItem(ref item)) => item.vis != hir::Public,
                Some(_) | None => false,
            }
        } else {
            return false
        }
    }

    fn trait_is_public(&self, trait_id: ast::NodeId) -> bool {
        // FIXME: this would preferably be using `exported_items`, but all
        // traits are exported currently (see `EmbargoVisitor.exported_trait`)
        self.access_levels.is_public(trait_id)
    }

    fn check_ty_param_bound(&mut self,
                            ty_param_bound: &hir::TyParamBound) {
        if let hir::TraitTyParamBound(ref trait_ref, _) = *ty_param_bound {
            if self.path_is_private_type(&trait_ref.trait_ref.path) {
                self.old_error_set.insert(trait_ref.trait_ref.ref_id);
            }
        }
    }

    fn item_is_public(&self, id: &ast::NodeId, vis: &hir::Visibility) -> bool {
        self.access_levels.is_reachable(*id) || *vis == hir::Public
    }
}

impl<'a, 'b, 'tcx, 'v> Visitor<'v> for ObsoleteCheckTypeForPrivatenessVisitor<'a, 'b, 'tcx> {
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'v> {
        NestedVisitorMap::None
    }

    fn visit_ty(&mut self, ty: &hir::Ty) {
        if let hir::TyPath(hir::QPath::Resolved(_, ref path)) = ty.node {
            if self.inner.path_is_private_type(path) {
                self.contains_private = true;
                // found what we're looking for so let's stop
                // working.
                return
            }
        }
        if let hir::TyPath(_) = ty.node {
            if self.at_outer_type {
                self.outer_type_is_public_path = true;
            }
        }
        self.at_outer_type = false;
        intravisit::walk_ty(self, ty)
    }

    // don't want to recurse into [, .. expr]
    fn visit_expr(&mut self, _: &hir::Expr) {}
}

impl<'a, 'tcx> Visitor<'tcx> for ObsoleteVisiblePrivateTypesVisitor<'a, 'tcx> {
    /// We want to visit items in the context of their containing
    /// module and so forth, so supply a crate for doing a deep walk.
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'tcx> {
        NestedVisitorMap::All(&self.tcx.hir)
    }

    fn visit_item(&mut self, item: &'tcx hir::Item) {
        match item.node {
            // contents of a private mod can be reexported, so we need
            // to check internals.
            hir::ItemMod(_) => {}

            // An `extern {}` doesn't introduce a new privacy
            // namespace (the contents have their own privacies).
            hir::ItemForeignMod(_) => {}

            hir::ItemTrait(.., ref bounds, _) => {
                if !self.trait_is_public(item.id) {
                    return
                }

                for bound in bounds.iter() {
                    self.check_ty_param_bound(bound)
                }
            }

            // impls need some special handling to try to offer useful
            // error messages without (too many) false positives
            // (i.e. we could just return here to not check them at
            // all, or some worse estimation of whether an impl is
            // publicly visible).
            hir::ItemImpl(.., ref g, ref trait_ref, ref self_, ref impl_item_refs) => {
                // `impl [... for] Private` is never visible.
                let self_contains_private;
                // impl [... for] Public<...>, but not `impl [... for]
                // Vec<Public>` or `(Public,)` etc.
                let self_is_public_path;

                // check the properties of the Self type:
                {
                    let mut visitor = ObsoleteCheckTypeForPrivatenessVisitor {
                        inner: self,
                        contains_private: false,
                        at_outer_type: true,
                        outer_type_is_public_path: false,
                    };
                    visitor.visit_ty(&self_);
                    self_contains_private = visitor.contains_private;
                    self_is_public_path = visitor.outer_type_is_public_path;
                }

                // miscellaneous info about the impl

                // `true` iff this is `impl Private for ...`.
                let not_private_trait =
                    trait_ref.as_ref().map_or(true, // no trait counts as public trait
                                              |tr| {
                        let did = tr.path.def.def_id();

                        if let Some(node_id) = self.tcx.hir.as_local_node_id(did) {
                            self.trait_is_public(node_id)
                        } else {
                            true // external traits must be public
                        }
                    });

                // `true` iff this is a trait impl or at least one method is public.
                //
                // `impl Public { $( fn ...() {} )* }` is not visible.
                //
                // This is required over just using the methods' privacy
                // directly because we might have `impl<T: Foo<Private>> ...`,
                // and we shouldn't warn about the generics if all the methods
                // are private (because `T` won't be visible externally).
                let trait_or_some_public_method =
                    trait_ref.is_some() ||
                    impl_item_refs.iter()
                                 .any(|impl_item_ref| {
                                     let impl_item = self.tcx.hir.impl_item(impl_item_ref.id);
                                     match impl_item.node {
                                         hir::ImplItemKind::Const(..) |
                                         hir::ImplItemKind::Method(..) => {
                                             self.access_levels.is_reachable(impl_item.id)
                                         }
                                         hir::ImplItemKind::Type(_) => false,
                                     }
                                 });

                if !self_contains_private &&
                        not_private_trait &&
                        trait_or_some_public_method {

                    intravisit::walk_generics(self, g);

                    match *trait_ref {
                        None => {
                            for impl_item_ref in impl_item_refs {
                                // This is where we choose whether to walk down
                                // further into the impl to check its items. We
                                // should only walk into public items so that we
                                // don't erroneously report errors for private
                                // types in private items.
                                let impl_item = self.tcx.hir.impl_item(impl_item_ref.id);
                                match impl_item.node {
                                    hir::ImplItemKind::Const(..) |
                                    hir::ImplItemKind::Method(..)
                                        if self.item_is_public(&impl_item.id, &impl_item.vis) =>
                                    {
                                        intravisit::walk_impl_item(self, impl_item)
                                    }
                                    hir::ImplItemKind::Type(..) => {
                                        intravisit::walk_impl_item(self, impl_item)
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Some(ref tr) => {
                            // Any private types in a trait impl fall into three
                            // categories.
                            // 1. mentioned in the trait definition
                            // 2. mentioned in the type params/generics
                            // 3. mentioned in the associated types of the impl
                            //
                            // Those in 1. can only occur if the trait is in
                            // this crate and will've been warned about on the
                            // trait definition (there's no need to warn twice
                            // so we don't check the methods).
                            //
                            // Those in 2. are warned via walk_generics and this
                            // call here.
                            intravisit::walk_path(self, &tr.path);

                            // Those in 3. are warned with this call.
                            for impl_item_ref in impl_item_refs {
                                let impl_item = self.tcx.hir.impl_item(impl_item_ref.id);
                                if let hir::ImplItemKind::Type(ref ty) = impl_item.node {
                                    self.visit_ty(ty);
                                }
                            }
                        }
                    }
                } else if trait_ref.is_none() && self_is_public_path {
                    // impl Public<Private> { ... }. Any public static
                    // methods will be visible as `Public::foo`.
                    let mut found_pub_static = false;
                    for impl_item_ref in impl_item_refs {
                        if self.item_is_public(&impl_item_ref.id.node_id, &impl_item_ref.vis) {
                            let impl_item = self.tcx.hir.impl_item(impl_item_ref.id);
                            match impl_item_ref.kind {
                                hir::AssociatedItemKind::Const => {
                                    found_pub_static = true;
                                    intravisit::walk_impl_item(self, impl_item);
                                }
                                hir::AssociatedItemKind::Method { has_self: false } => {
                                    found_pub_static = true;
                                    intravisit::walk_impl_item(self, impl_item);
                                }
                                _ => {}
                            }
                        }
                    }
                    if found_pub_static {
                        intravisit::walk_generics(self, g)
                    }
                }
                return
            }

            // `type ... = ...;` can contain private types, because
            // we're introducing a new name.
            hir::ItemTy(..) => return,

            // not at all public, so we don't care
            _ if !self.item_is_public(&item.id, &item.vis) => {
                return;
            }

            _ => {}
        }

        // We've carefully constructed it so that if we're here, then
        // any `visit_ty`'s will be called on things that are in
        // public signatures, i.e. things that we're interested in for
        // this visitor.
        intravisit::walk_item(self, item);
    }

    fn visit_generics(&mut self, generics: &'tcx hir::Generics) {
        for ty_param in generics.ty_params.iter() {
            for bound in ty_param.bounds.iter() {
                self.check_ty_param_bound(bound)
            }
        }
        for predicate in &generics.where_clause.predicates {
            match predicate {
                &hir::WherePredicate::BoundPredicate(ref bound_pred) => {
                    for bound in bound_pred.bounds.iter() {
                        self.check_ty_param_bound(bound)
                    }
                }
                &hir::WherePredicate::RegionPredicate(_) => {}
                &hir::WherePredicate::EqPredicate(ref eq_pred) => {
                    self.visit_ty(&eq_pred.rhs_ty);
                }
            }
        }
    }

    fn visit_foreign_item(&mut self, item: &'tcx hir::ForeignItem) {
        if self.access_levels.is_reachable(item.id) {
            intravisit::walk_foreign_item(self, item)
        }
    }

    fn visit_ty(&mut self, t: &'tcx hir::Ty) {
        if let hir::TyPath(hir::QPath::Resolved(_, ref path)) = t.node {
            if self.path_is_private_type(path) {
                self.old_error_set.insert(t.id);
            }
        }
        intravisit::walk_ty(self, t)
    }

    fn visit_variant(&mut self,
                     v: &'tcx hir::Variant,
                     g: &'tcx hir::Generics,
                     item_id: ast::NodeId) {
        if self.access_levels.is_reachable(v.node.data.id()) {
            self.in_variant = true;
            intravisit::walk_variant(self, v, g, item_id);
            self.in_variant = false;
        }
    }

    fn visit_struct_field(&mut self, s: &'tcx hir::StructField) {
        if s.vis == hir::Public || self.in_variant {
            intravisit::walk_struct_field(self, s);
        }
    }

    // we don't need to introspect into these at all: an
    // expression/block context can't possibly contain exported things.
    // (Making them no-ops stops us from traversing the whole AST without
    // having to be super careful about our `walk_...` calls above.)
    fn visit_block(&mut self, _: &'tcx hir::Block) {}
    fn visit_expr(&mut self, _: &'tcx hir::Expr) {}
}

///////////////////////////////////////////////////////////////////////////////
/// SearchInterfaceForPrivateItemsVisitor traverses an item's interface and
/// finds any private components in it.
/// PrivateItemsInPublicInterfacesVisitor ensures there are no private types
/// and traits in public interfaces.
///////////////////////////////////////////////////////////////////////////////

struct SearchInterfaceForPrivateItemsVisitor<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    item_def_id: DefId,
    span: Span,
    /// The visitor checks that each component type is at least this visible
    required_visibility: ty::Visibility,
    /// The visibility of the least visible component that has been visited
    min_visibility: ty::Visibility,
    has_pub_restricted: bool,
    has_old_errors: bool,
}

impl<'a, 'tcx: 'a> SearchInterfaceForPrivateItemsVisitor<'a, 'tcx> {
    fn generics(&mut self) -> &mut Self {
        for def in &self.tcx.generics_of(self.item_def_id).types {
            if def.has_default {
                self.tcx.type_of(def.def_id).visit_with(self);
            }
        }
        self
    }

    fn predicates(&mut self) -> &mut Self {
        self.tcx.predicates_of(self.item_def_id).visit_with(self);
        self
    }

    fn ty(&mut self) -> &mut Self {
        self.tcx.type_of(self.item_def_id).visit_with(self);
        self
    }

    fn impl_trait_ref(&mut self) -> &mut Self {
        self.tcx.impl_trait_ref(self.item_def_id).visit_with(self);
        self
    }
}

impl<'a, 'tcx: 'a> TypeVisitor<'tcx> for SearchInterfaceForPrivateItemsVisitor<'a, 'tcx> {
    fn visit_ty(&mut self, ty: Ty<'tcx>) -> bool {
        let ty_def_id = match ty.sty {
            ty::TyAdt(adt, _) => Some(adt.did),
            ty::TyDynamic(ref obj, ..) => obj.principal().map(|p| p.def_id()),
            ty::TyProjection(ref proj) => {
                if self.required_visibility == ty::Visibility::Invisible {
                    // Conservatively approximate the whole type alias as public without
                    // recursing into its components when determining impl publicity.
                    // For example, `impl <Type as Trait>::Alias {...}` may be a public impl
                    // even if both `Type` and `Trait` are private.
                    // Ideally, associated types should be substituted in the same way as
                    // free type aliases, but this isn't done yet.
                    return false;
                }

                Some(proj.trait_ref.def_id)
            }
            _ => None
        };

        if let Some(def_id) = ty_def_id {
            // Non-local means public (private items can't leave their crate, modulo bugs)
            if let Some(node_id) = self.tcx.hir.as_local_node_id(def_id) {
                let item = self.tcx.hir.expect_item(node_id);
                let vis = ty::Visibility::from_hir(&item.vis, node_id, self.tcx);

                if !vis.is_at_least(self.min_visibility, self.tcx) {
                    self.min_visibility = vis;
                }
                if !vis.is_at_least(self.required_visibility, self.tcx) {
                    if self.has_pub_restricted || self.has_old_errors {
                        let mut err = struct_span_err!(self.tcx.sess, self.span, E0446,
                            "private type `{}` in public interface", ty);
                        err.span_label(self.span, "can't leak private type");
                        err.emit();
                    } else {
                        self.tcx.sess.add_lint(lint::builtin::PRIVATE_IN_PUBLIC,
                                               node_id,
                                               self.span,
                                               format!("private type `{}` in public \
                                                        interface (error E0446)", ty));
                    }
                }
            }
        }

        if let ty::TyProjection(ref proj) = ty.sty {
            // Avoid calling `visit_trait_ref` below on the trait,
            // as we have already checked the trait itself above.
            proj.trait_ref.super_visit_with(self)
        } else {
            ty.super_visit_with(self)
        }
    }

    fn visit_trait_ref(&mut self, trait_ref: ty::TraitRef<'tcx>) -> bool {
        // Non-local means public (private items can't leave their crate, modulo bugs)
        if let Some(node_id) = self.tcx.hir.as_local_node_id(trait_ref.def_id) {
            let item = self.tcx.hir.expect_item(node_id);
            let vis = ty::Visibility::from_hir(&item.vis, node_id, self.tcx);

            if !vis.is_at_least(self.min_visibility, self.tcx) {
                self.min_visibility = vis;
            }
            if !vis.is_at_least(self.required_visibility, self.tcx) {
                if self.has_pub_restricted || self.has_old_errors {
                    struct_span_err!(self.tcx.sess, self.span, E0445,
                                     "private trait `{}` in public interface", trait_ref)
                        .span_label(self.span, format!(
                                    "private trait can't be public"))
                        .emit();
                } else {
                    self.tcx.sess.add_lint(lint::builtin::PRIVATE_IN_PUBLIC,
                                           node_id,
                                           self.span,
                                           format!("private trait `{}` in public \
                                                    interface (error E0445)", trait_ref));
                }
            }
        }

        trait_ref.super_visit_with(self)
    }
}

struct PrivateItemsInPublicInterfacesVisitor<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    has_pub_restricted: bool,
    old_error_set: &'a NodeSet,
    inner_visibility: ty::Visibility,
}

impl<'a, 'tcx> PrivateItemsInPublicInterfacesVisitor<'a, 'tcx> {
    fn check(&self, item_id: ast::NodeId, required_visibility: ty::Visibility)
             -> SearchInterfaceForPrivateItemsVisitor<'a, 'tcx> {
        let mut has_old_errors = false;

        // Slow path taken only if there any errors in the crate.
        for &id in self.old_error_set {
            // Walk up the nodes until we find `item_id` (or we hit a root).
            let mut id = id;
            loop {
                if id == item_id {
                    has_old_errors = true;
                    break;
                }
                let parent = self.tcx.hir.get_parent_node(id);
                if parent == id {
                    break;
                }
                id = parent;
            }

            if has_old_errors {
                break;
            }
        }

        SearchInterfaceForPrivateItemsVisitor {
            tcx: self.tcx,
            item_def_id: self.tcx.hir.local_def_id(item_id),
            span: self.tcx.hir.span(item_id),
            min_visibility: ty::Visibility::Public,
            required_visibility: required_visibility,
            has_pub_restricted: self.has_pub_restricted,
            has_old_errors: has_old_errors,
        }
    }
}

impl<'a, 'tcx> Visitor<'tcx> for PrivateItemsInPublicInterfacesVisitor<'a, 'tcx> {
    fn nested_visit_map<'this>(&'this mut self) -> NestedVisitorMap<'this, 'tcx> {
        NestedVisitorMap::OnlyBodies(&self.tcx.hir)
    }

    fn visit_item(&mut self, item: &'tcx hir::Item) {
        let tcx = self.tcx;
        let min = |vis1: ty::Visibility, vis2| {
            if vis1.is_at_least(vis2, tcx) { vis2 } else { vis1 }
        };

        let item_visibility = ty::Visibility::from_hir(&item.vis, item.id, tcx);

        match item.node {
            // Crates are always public
            hir::ItemExternCrate(..) => {}
            // All nested items are checked by visit_item
            hir::ItemMod(..) => {}
            // Checked in resolve
            hir::ItemUse(..) => {}
            // No subitems
            hir::ItemGlobalAsm(..) => {}
            // Subitems of these items have inherited publicity
            hir::ItemConst(..) | hir::ItemStatic(..) | hir::ItemFn(..) |
            hir::ItemTy(..) => {
                self.check(item.id, item_visibility).generics().predicates().ty();

                // Recurse for e.g. `impl Trait` (see `visit_ty`).
                self.inner_visibility = item_visibility;
                intravisit::walk_item(self, item);
            }
            hir::ItemTrait(.., ref trait_item_refs) => {
                self.check(item.id, item_visibility).generics().predicates();

                for trait_item_ref in trait_item_refs {
                    let mut check = self.check(trait_item_ref.id.node_id, item_visibility);
                    check.generics().predicates();

                    if trait_item_ref.kind == hir::AssociatedItemKind::Type &&
                       !trait_item_ref.defaultness.has_value() {
                        // No type to visit.
                    } else {
                        check.ty();
                    }
                }
            }
            hir::ItemEnum(ref def, _) => {
                self.check(item.id, item_visibility).generics().predicates();

                for variant in &def.variants {
                    for field in variant.node.data.fields() {
                        self.check(field.id, item_visibility).ty();
                    }
                }
            }
            // Subitems of foreign modules have their own publicity
            hir::ItemForeignMod(ref foreign_mod) => {
                for foreign_item in &foreign_mod.items {
                    let vis = ty::Visibility::from_hir(&foreign_item.vis, item.id, tcx);
                    self.check(foreign_item.id, vis).generics().predicates().ty();
                }
            }
            // Subitems of structs and unions have their own publicity
            hir::ItemStruct(ref struct_def, _) |
            hir::ItemUnion(ref struct_def, _) => {
                self.check(item.id, item_visibility).generics().predicates();

                for field in struct_def.fields() {
                    let field_visibility = ty::Visibility::from_hir(&field.vis, item.id, tcx);
                    self.check(field.id, min(item_visibility, field_visibility)).ty();
                }
            }
            // The interface is empty
            hir::ItemDefaultImpl(..) => {}
            // An inherent impl is public when its type is public
            // Subitems of inherent impls have their own publicity
            hir::ItemImpl(.., None, _, ref impl_item_refs) => {
                let ty_vis =
                    self.check(item.id, ty::Visibility::Invisible).ty().min_visibility;
                self.check(item.id, ty_vis).generics().predicates();

                for impl_item_ref in impl_item_refs {
                    let impl_item = self.tcx.hir.impl_item(impl_item_ref.id);
                    let impl_item_vis =
                        ty::Visibility::from_hir(&impl_item.vis, item.id, tcx);
                    self.check(impl_item.id, min(impl_item_vis, ty_vis))
                        .generics().predicates().ty();

                    // Recurse for e.g. `impl Trait` (see `visit_ty`).
                    self.inner_visibility = impl_item_vis;
                    intravisit::walk_impl_item(self, impl_item);
                }
            }
            // A trait impl is public when both its type and its trait are public
            // Subitems of trait impls have inherited publicity
            hir::ItemImpl(.., Some(_), _, ref impl_item_refs) => {
                let vis = self.check(item.id, ty::Visibility::Invisible)
                              .ty().impl_trait_ref().min_visibility;
                self.check(item.id, vis).generics().predicates();
                for impl_item_ref in impl_item_refs {
                    let impl_item = self.tcx.hir.impl_item(impl_item_ref.id);
                    self.check(impl_item.id, vis).generics().predicates().ty();

                    // Recurse for e.g. `impl Trait` (see `visit_ty`).
                    self.inner_visibility = vis;
                    intravisit::walk_impl_item(self, impl_item);
                }
            }
        }
    }

    fn visit_impl_item(&mut self, _impl_item: &'tcx hir::ImplItem) {
        // handled in `visit_item` above
    }

    fn visit_ty(&mut self, ty: &'tcx hir::Ty) {
        if let hir::TyImplTrait(..) = ty.node {
            // Check the traits being exposed, as they're separate,
            // e.g. `impl Iterator<Item=T>` has two predicates,
            // `X: Iterator` and `<X as Iterator>::Item == T`,
            // where `X` is the `impl Iterator<Item=T>` itself,
            // stored in `predicates_of`, not in the `Ty` itself.
            self.check(ty.id, self.inner_visibility).predicates();
        }

        intravisit::walk_ty(self, ty);
    }

    // Don't recurse into expressions in array sizes or const initializers
    fn visit_expr(&mut self, _: &'tcx hir::Expr) {}
    // Don't recurse into patterns in function arguments
    fn visit_pat(&mut self, _: &'tcx hir::Pat) {}
}

pub fn provide(providers: &mut Providers) {
    *providers = Providers {
        privacy_access_levels,
        ..*providers
    };
}

pub fn check_crate<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>) -> Rc<AccessLevels> {
    tcx.dep_graph.with_ignore(|| { // FIXME
        tcx.privacy_access_levels(LOCAL_CRATE)
    })
}

fn privacy_access_levels<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                   krate: CrateNum)
                                   -> Rc<AccessLevels> {
    assert_eq!(krate, LOCAL_CRATE);

    let krate = tcx.hir.krate();

    // Check privacy of names not checked in previous compilation stages.
    let mut visitor = NamePrivacyVisitor {
        tcx: tcx,
        tables: &ty::TypeckTables::empty(),
        current_item: CRATE_NODE_ID,
    };
    intravisit::walk_crate(&mut visitor, krate);

    // Build up a set of all exported items in the AST. This is a set of all
    // items which are reachable from external crates based on visibility.
    let mut visitor = EmbargoVisitor {
        tcx: tcx,
        access_levels: Default::default(),
        prev_level: Some(AccessLevel::Public),
        changed: false,
    };
    loop {
        intravisit::walk_crate(&mut visitor, krate);
        if visitor.changed {
            visitor.changed = false;
        } else {
            break
        }
    }
    visitor.update(ast::CRATE_NODE_ID, Some(AccessLevel::Public));

    {
        let mut visitor = ObsoleteVisiblePrivateTypesVisitor {
            tcx: tcx,
            access_levels: &visitor.access_levels,
            in_variant: false,
            old_error_set: NodeSet(),
        };
        intravisit::walk_crate(&mut visitor, krate);


        let has_pub_restricted = {
            let mut pub_restricted_visitor = PubRestrictedVisitor {
                tcx: tcx,
                has_pub_restricted: false
            };
            intravisit::walk_crate(&mut pub_restricted_visitor, krate);
            pub_restricted_visitor.has_pub_restricted
        };

        // Check for private types and traits in public interfaces
        let mut visitor = PrivateItemsInPublicInterfacesVisitor {
            tcx: tcx,
            has_pub_restricted: has_pub_restricted,
            old_error_set: &visitor.old_error_set,
            inner_visibility: ty::Visibility::Public,
        };
        krate.visit_all_item_likes(&mut DeepVisitor::new(&mut visitor));
    }

    Rc::new(visitor.access_levels)
}

__build_diagnostic_array! { librustc_privacy, DIAGNOSTICS }
