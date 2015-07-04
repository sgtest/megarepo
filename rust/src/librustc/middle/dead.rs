// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// This implements the dead-code warning pass. It follows middle::reachable
// closely. The idea is that all reachable symbols are live, codes called
// from live codes are live, and everything else is dead.

use ast_map;
use middle::{def, pat_util, privacy, ty};
use lint;
use util::nodemap::NodeSet;

use std::collections::HashSet;
use syntax::{ast, codemap};
use syntax::ast_util::{local_def, is_local};
use syntax::attr::{self, AttrMetaMethods};
use syntax::visit::{self, Visitor};

// Any local node that may call something in its body block should be
// explored. For example, if it's a live NodeItem that is a
// function, then we should explore its block to check for codes that
// may need to be marked as live.
fn should_explore(tcx: &ty::ctxt, def_id: ast::DefId) -> bool {
    if !is_local(def_id) {
        return false;
    }

    match tcx.map.find(def_id.node) {
        Some(ast_map::NodeItem(..))
        | Some(ast_map::NodeImplItem(..))
        | Some(ast_map::NodeForeignItem(..))
        | Some(ast_map::NodeTraitItem(..)) => true,
        _ => false
    }
}

struct MarkSymbolVisitor<'a, 'tcx: 'a> {
    worklist: Vec<ast::NodeId>,
    tcx: &'a ty::ctxt<'tcx>,
    live_symbols: Box<HashSet<ast::NodeId>>,
    struct_has_extern_repr: bool,
    ignore_non_const_paths: bool,
    inherited_pub_visibility: bool,
    ignore_variant_stack: Vec<ast::NodeId>,
}

impl<'a, 'tcx> MarkSymbolVisitor<'a, 'tcx> {
    fn new(tcx: &'a ty::ctxt<'tcx>,
           worklist: Vec<ast::NodeId>) -> MarkSymbolVisitor<'a, 'tcx> {
        MarkSymbolVisitor {
            worklist: worklist,
            tcx: tcx,
            live_symbols: box HashSet::new(),
            struct_has_extern_repr: false,
            ignore_non_const_paths: false,
            inherited_pub_visibility: false,
            ignore_variant_stack: vec![],
        }
    }

    fn check_def_id(&mut self, def_id: ast::DefId) {
        if should_explore(self.tcx, def_id) {
            self.worklist.push(def_id.node);
        }
        self.live_symbols.insert(def_id.node);
    }

    fn lookup_and_handle_definition(&mut self, id: &ast::NodeId) {
        self.tcx.def_map.borrow().get(id).map(|def| {
            match def.full_def() {
                def::DefConst(_) | def::DefAssociatedConst(..) => {
                    self.check_def_id(def.def_id())
                }
                _ if self.ignore_non_const_paths => (),
                def::DefPrimTy(_) => (),
                def::DefVariant(enum_id, variant_id, _) => {
                    self.check_def_id(enum_id);
                    if !self.ignore_variant_stack.contains(&variant_id.node) {
                        self.check_def_id(variant_id);
                    }
                }
                _ => {
                    self.check_def_id(def.def_id());
                }
            }
        });
    }

    fn lookup_and_handle_method(&mut self, id: ast::NodeId) {
        let method_call = ty::MethodCall::expr(id);
        let method = self.tcx.tables.borrow().method_map[&method_call];
        self.check_def_id(method.def_id);
    }

    fn handle_field_access(&mut self, lhs: &ast::Expr, name: ast::Name) {
        match self.tcx.expr_ty_adjusted(lhs).sty {
            ty::TyStruct(id, _) => {
                let fields = self.tcx.lookup_struct_fields(id);
                let field_id = fields.iter()
                    .find(|field| field.name == name).unwrap().id;
                self.live_symbols.insert(field_id.node);
            },
            _ => ()
        }
    }

    fn handle_tup_field_access(&mut self, lhs: &ast::Expr, idx: usize) {
        match self.tcx.expr_ty_adjusted(lhs).sty {
            ty::TyStruct(id, _) => {
                let fields = self.tcx.lookup_struct_fields(id);
                let field_id = fields[idx].id;
                self.live_symbols.insert(field_id.node);
            },
            _ => ()
        }
    }

    fn handle_field_pattern_match(&mut self, lhs: &ast::Pat,
                                  pats: &[codemap::Spanned<ast::FieldPat>]) {
        let id = match self.tcx.def_map.borrow().get(&lhs.id).unwrap().full_def() {
            def::DefVariant(_, id, _) => id,
            _ => {
                match self.tcx.node_id_to_type(lhs.id).ty_to_def_id() {
                    None => {
                        self.tcx.sess.span_bug(lhs.span,
                                               "struct pattern wasn't of a \
                                                type with a def ID?!")
                    }
                    Some(def_id) => def_id,
                }
            }
        };
        let fields = self.tcx.lookup_struct_fields(id);
        for pat in pats {
            if let ast::PatWild(ast::PatWildSingle) = pat.node.pat.node {
                continue;
            }
            let field_id = fields.iter()
                .find(|field| field.name == pat.node.ident.name).unwrap().id;
            self.live_symbols.insert(field_id.node);
        }
    }

    fn mark_live_symbols(&mut self) {
        let mut scanned = HashSet::new();
        while !self.worklist.is_empty() {
            let id = self.worklist.pop().unwrap();
            if scanned.contains(&id) {
                continue
            }
            scanned.insert(id);

            match self.tcx.map.find(id) {
                Some(ref node) => {
                    self.live_symbols.insert(id);
                    self.visit_node(node);
                }
                None => (),
            }
        }
    }

    fn visit_node(&mut self, node: &ast_map::Node) {
        let had_extern_repr = self.struct_has_extern_repr;
        self.struct_has_extern_repr = false;
        let had_inherited_pub_visibility = self.inherited_pub_visibility;
        self.inherited_pub_visibility = false;
        match *node {
            ast_map::NodeItem(item) => {
                match item.node {
                    ast::ItemStruct(..) => {
                        self.struct_has_extern_repr = item.attrs.iter().any(|attr| {
                            attr::find_repr_attrs(self.tcx.sess.diagnostic(), attr)
                                .contains(&attr::ReprExtern)
                        });

                        visit::walk_item(self, &*item);
                    }
                    ast::ItemEnum(..) => {
                        self.inherited_pub_visibility = item.vis == ast::Public;
                        visit::walk_item(self, &*item);
                    }
                    ast::ItemFn(..)
                    | ast::ItemTy(..)
                    | ast::ItemStatic(..)
                    | ast::ItemConst(..) => {
                        visit::walk_item(self, &*item);
                    }
                    _ => ()
                }
            }
            ast_map::NodeTraitItem(trait_item) => {
                visit::walk_trait_item(self, trait_item);
            }
            ast_map::NodeImplItem(impl_item) => {
                visit::walk_impl_item(self, impl_item);
            }
            ast_map::NodeForeignItem(foreign_item) => {
                visit::walk_foreign_item(self, &*foreign_item);
            }
            _ => ()
        }
        self.struct_has_extern_repr = had_extern_repr;
        self.inherited_pub_visibility = had_inherited_pub_visibility;
    }
}

impl<'a, 'tcx, 'v> Visitor<'v> for MarkSymbolVisitor<'a, 'tcx> {

    fn visit_struct_def(&mut self, def: &ast::StructDef, _: ast::Ident,
                        _: &ast::Generics, _: ast::NodeId) {
        let has_extern_repr = self.struct_has_extern_repr;
        let inherited_pub_visibility = self.inherited_pub_visibility;
        let live_fields = def.fields.iter().filter(|f| {
            has_extern_repr || inherited_pub_visibility || match f.node.kind {
                ast::NamedField(_, ast::Public) => true,
                _ => false
            }
        });
        self.live_symbols.extend(live_fields.map(|f| f.node.id));

        visit::walk_struct_def(self, def);
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        match expr.node {
            ast::ExprMethodCall(..) => {
                self.lookup_and_handle_method(expr.id);
            }
            ast::ExprField(ref lhs, ref ident) => {
                self.handle_field_access(&**lhs, ident.node.name);
            }
            ast::ExprTupField(ref lhs, idx) => {
                self.handle_tup_field_access(&**lhs, idx.node);
            }
            _ => ()
        }

        visit::walk_expr(self, expr);
    }

    fn visit_arm(&mut self, arm: &ast::Arm) {
        if arm.pats.len() == 1 {
            let pat = &*arm.pats[0];
            let variants = pat_util::necessary_variants(&self.tcx.def_map, pat);

            // Inside the body, ignore constructions of variants
            // necessary for the pattern to match. Those construction sites
            // can't be reached unless the variant is constructed elsewhere.
            let len = self.ignore_variant_stack.len();
            self.ignore_variant_stack.push_all(&*variants);
            visit::walk_arm(self, arm);
            self.ignore_variant_stack.truncate(len);
        } else {
            visit::walk_arm(self, arm);
        }
    }

    fn visit_pat(&mut self, pat: &ast::Pat) {
        let def_map = &self.tcx.def_map;
        match pat.node {
            ast::PatStruct(_, ref fields, _) => {
                self.handle_field_pattern_match(pat, fields);
            }
            _ if pat_util::pat_is_const(def_map, pat) => {
                // it might be the only use of a const
                self.lookup_and_handle_definition(&pat.id)
            }
            _ => ()
        }

        self.ignore_non_const_paths = true;
        visit::walk_pat(self, pat);
        self.ignore_non_const_paths = false;
    }

    fn visit_path(&mut self, path: &ast::Path, id: ast::NodeId) {
        self.lookup_and_handle_definition(&id);
        visit::walk_path(self, path);
    }

    fn visit_item(&mut self, _: &ast::Item) {
        // Do not recurse into items. These items will be added to the
        // worklist and recursed into manually if necessary.
    }
}

fn has_allow_dead_code_or_lang_attr(attrs: &[ast::Attribute]) -> bool {
    if attr::contains_name(attrs, "lang") {
        return true;
    }

    let dead_code = lint::builtin::DEAD_CODE.name_lower();
    for attr in lint::gather_attrs(attrs) {
        match attr {
            Ok((ref name, lint::Allow, _))
                if &name[..] == dead_code => return true,
            _ => (),
        }
    }
    false
}

// This visitor seeds items that
//   1) We want to explicitly consider as live:
//     * Item annotated with #[allow(dead_code)]
//         - This is done so that if we want to suppress warnings for a
//           group of dead functions, we only have to annotate the "root".
//           For example, if both `f` and `g` are dead and `f` calls `g`,
//           then annotating `f` with `#[allow(dead_code)]` will suppress
//           warning for both `f` and `g`.
//     * Item annotated with #[lang=".."]
//         - This is because lang items are always callable from elsewhere.
//   or
//   2) We are not sure to be live or not
//     * Implementation of a trait method
struct LifeSeeder {
    worklist: Vec<ast::NodeId>
}

impl<'v> Visitor<'v> for LifeSeeder {
    fn visit_item(&mut self, item: &ast::Item) {
        let allow_dead_code = has_allow_dead_code_or_lang_attr(&item.attrs);
        if allow_dead_code {
            self.worklist.push(item.id);
        }
        match item.node {
            ast::ItemEnum(ref enum_def, _) if allow_dead_code => {
                self.worklist.extend(enum_def.variants.iter().map(|variant| variant.node.id));
            }
            ast::ItemTrait(_, _, _, ref trait_items) => {
                for trait_item in trait_items {
                    match trait_item.node {
                        ast::ConstTraitItem(_, Some(_)) |
                        ast::MethodTraitItem(_, Some(_)) => {
                            if has_allow_dead_code_or_lang_attr(&trait_item.attrs) {
                                self.worklist.push(trait_item.id);
                            }
                        }
                        _ => {}
                    }
                }
            }
            ast::ItemImpl(_, _, _, ref opt_trait, _, ref impl_items) => {
                for impl_item in impl_items {
                    match impl_item.node {
                        ast::ConstImplItem(..) |
                        ast::MethodImplItem(..) => {
                            if opt_trait.is_some() ||
                                    has_allow_dead_code_or_lang_attr(&impl_item.attrs) {
                                self.worklist.push(impl_item.id);
                            }
                        }
                        ast::TypeImplItem(_) => {}
                        ast::MacImplItem(_) => panic!("unexpanded macro")
                    }
                }
            }
            _ => ()
        }
        visit::walk_item(self, item);
    }
}

fn create_and_seed_worklist(tcx: &ty::ctxt,
                            exported_items: &privacy::ExportedItems,
                            reachable_symbols: &NodeSet,
                            krate: &ast::Crate) -> Vec<ast::NodeId> {
    let mut worklist = Vec::new();

    // Preferably, we would only need to seed the worklist with reachable
    // symbols. However, since the set of reachable symbols differs
    // depending on whether a crate is built as bin or lib, and we want
    // the warning to be consistent, we also seed the worklist with
    // exported symbols.
    for id in exported_items {
        worklist.push(*id);
    }
    for id in reachable_symbols {
        // Reachable variants can be dead, because we warn about
        // variants never constructed, not variants never used.
        if let Some(ast_map::NodeVariant(..)) = tcx.map.find(*id) {
            continue;
        }
        worklist.push(*id);
    }

    // Seed entry point
    match *tcx.sess.entry_fn.borrow() {
        Some((id, _)) => worklist.push(id),
        None => ()
    }

    // Seed implemented trait items
    let mut life_seeder = LifeSeeder {
        worklist: worklist
    };
    visit::walk_crate(&mut life_seeder, krate);

    return life_seeder.worklist;
}

fn find_live(tcx: &ty::ctxt,
             exported_items: &privacy::ExportedItems,
             reachable_symbols: &NodeSet,
             krate: &ast::Crate)
             -> Box<HashSet<ast::NodeId>> {
    let worklist = create_and_seed_worklist(tcx, exported_items,
                                            reachable_symbols, krate);
    let mut symbol_visitor = MarkSymbolVisitor::new(tcx, worklist);
    symbol_visitor.mark_live_symbols();
    symbol_visitor.live_symbols
}

fn get_struct_ctor_id(item: &ast::Item) -> Option<ast::NodeId> {
    match item.node {
        ast::ItemStruct(ref struct_def, _) => struct_def.ctor_id,
        _ => None
    }
}

struct DeadVisitor<'a, 'tcx: 'a> {
    tcx: &'a ty::ctxt<'tcx>,
    live_symbols: Box<HashSet<ast::NodeId>>,
}

impl<'a, 'tcx> DeadVisitor<'a, 'tcx> {
    fn should_warn_about_item(&mut self, item: &ast::Item) -> bool {
        let should_warn = match item.node {
            ast::ItemStatic(..)
            | ast::ItemConst(..)
            | ast::ItemFn(..)
            | ast::ItemEnum(..)
            | ast::ItemStruct(..) => true,
            _ => false
        };
        let ctor_id = get_struct_ctor_id(item);
        should_warn && !self.symbol_is_live(item.id, ctor_id)
    }

    fn should_warn_about_field(&mut self, node: &ast::StructField_) -> bool {
        let is_named = node.ident().is_some();
        let field_type = self.tcx.node_id_to_type(node.id);
        let is_marker_field = match field_type.ty_to_def_id() {
            Some(def_id) => self.tcx.lang_items.items().any(|(_, item)| *item == Some(def_id)),
            _ => false
        };
        is_named
            && !self.symbol_is_live(node.id, None)
            && !is_marker_field
            && !has_allow_dead_code_or_lang_attr(&node.attrs)
    }

    fn should_warn_about_variant(&mut self, variant: &ast::Variant_) -> bool {
        !self.symbol_is_live(variant.id, None)
            && !has_allow_dead_code_or_lang_attr(&variant.attrs)
    }

    // id := node id of an item's definition.
    // ctor_id := `Some` if the item is a struct_ctor (tuple struct),
    //            `None` otherwise.
    // If the item is a struct_ctor, then either its `id` or
    // `ctor_id` (unwrapped) is in the live_symbols set. More specifically,
    // DefMap maps the ExprPath of a struct_ctor to the node referred by
    // `ctor_id`. On the other hand, in a statement like
    // `type <ident> <generics> = <ty>;` where <ty> refers to a struct_ctor,
    // DefMap maps <ty> to `id` instead.
    fn symbol_is_live(&mut self, id: ast::NodeId,
                      ctor_id: Option<ast::NodeId>) -> bool {
        if self.live_symbols.contains(&id)
           || ctor_id.map_or(false,
                             |ctor| self.live_symbols.contains(&ctor)) {
            return true;
        }
        // If it's a type whose items are live, then it's live, too.
        // This is done to handle the case where, for example, the static
        // method of a private type is used, but the type itself is never
        // called directly.
        let impl_items = self.tcx.impl_items.borrow();
        match self.tcx.inherent_impls.borrow().get(&local_def(id)) {
            None => (),
            Some(impl_list) => {
                for impl_did in impl_list.iter() {
                    for item_did in impl_items.get(impl_did).unwrap().iter() {
                        if self.live_symbols.contains(&item_did.def_id()
                                                               .node) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn warn_dead_code(&mut self,
                      id: ast::NodeId,
                      span: codemap::Span,
                      name: ast::Name,
                      node_type: &str) {
        let name = name.as_str();
        if !name.starts_with("_") {
            self.tcx
                .sess
                .add_lint(lint::builtin::DEAD_CODE,
                          id,
                          span,
                          format!("{} is never used: `{}`", node_type, name));
        }
    }
}

impl<'a, 'tcx, 'v> Visitor<'v> for DeadVisitor<'a, 'tcx> {
    fn visit_item(&mut self, item: &ast::Item) {
        if self.should_warn_about_item(item) {
            self.warn_dead_code(
                item.id,
                item.span,
                item.ident.name,
                item.node.descriptive_variant()
            );
        } else {
            match item.node {
                ast::ItemEnum(ref enum_def, _) => {
                    for variant in &enum_def.variants {
                        if self.should_warn_about_variant(&variant.node) {
                            self.warn_dead_code(variant.node.id, variant.span,
                                                variant.node.name.name, "variant");
                        }
                    }
                },
                _ => ()
            }
        }
        visit::walk_item(self, item);
    }

    fn visit_foreign_item(&mut self, fi: &ast::ForeignItem) {
        if !self.symbol_is_live(fi.id, None) {
            self.warn_dead_code(fi.id, fi.span, fi.ident.name, fi.node.descriptive_variant());
        }
        visit::walk_foreign_item(self, fi);
    }

    fn visit_struct_field(&mut self, field: &ast::StructField) {
        if self.should_warn_about_field(&field.node) {
            self.warn_dead_code(field.node.id, field.span,
                                field.node.ident().unwrap().name, "struct field");
        }

        visit::walk_struct_field(self, field);
    }

    fn visit_impl_item(&mut self, impl_item: &ast::ImplItem) {
        match impl_item.node {
            ast::ConstImplItem(_, ref expr) => {
                if !self.symbol_is_live(impl_item.id, None) {
                    self.warn_dead_code(impl_item.id, impl_item.span,
                                        impl_item.ident.name, "associated const");
                }
                visit::walk_expr(self, expr)
            }
            ast::MethodImplItem(_, ref body) => {
                if !self.symbol_is_live(impl_item.id, None) {
                    self.warn_dead_code(impl_item.id, impl_item.span,
                                        impl_item.ident.name, "method");
                }
                visit::walk_block(self, body)
            }
            ast::TypeImplItem(..) |
            ast::MacImplItem(..) => {}
        }
    }

    // Overwrite so that we don't warn the trait item itself.
    fn visit_trait_item(&mut self, trait_item: &ast::TraitItem) {
        match trait_item.node {
            ast::ConstTraitItem(_, Some(ref expr)) => {
                visit::walk_expr(self, expr)
            }
            ast::MethodTraitItem(_, Some(ref body)) => {
                visit::walk_block(self, body)
            }
            ast::ConstTraitItem(_, None) |
            ast::MethodTraitItem(_, None) |
            ast::TypeTraitItem(..) => {}
        }
    }
}

pub fn check_crate(tcx: &ty::ctxt,
                   exported_items: &privacy::ExportedItems,
                   reachable_symbols: &NodeSet) {
    let krate = tcx.map.krate();
    let live_symbols = find_live(tcx, exported_items,
                                 reachable_symbols, krate);
    let mut visitor = DeadVisitor { tcx: tcx, live_symbols: live_symbols };
    visit::walk_crate(&mut visitor, krate);
}
