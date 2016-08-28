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

use dep_graph::DepNode;
use hir::map as ast_map;
use hir::{self, pat_util, PatKind};
use hir::intravisit::{self, Visitor};

use middle::privacy;
use ty::{self, TyCtxt};
use hir::def::Def;
use hir::def_id::{DefId};
use lint;
use util::nodemap::FnvHashSet;

use syntax::{ast, codemap};
use syntax::attr;
use syntax_pos;

// Any local node that may call something in its body block should be
// explored. For example, if it's a live NodeItem that is a
// function, then we should explore its block to check for codes that
// may need to be marked as live.
fn should_explore<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                            node_id: ast::NodeId) -> bool {
    match tcx.map.find(node_id) {
        Some(ast_map::NodeItem(..)) |
        Some(ast_map::NodeImplItem(..)) |
        Some(ast_map::NodeForeignItem(..)) |
        Some(ast_map::NodeTraitItem(..)) =>
            true,
        _ =>
            false
    }
}

struct MarkSymbolVisitor<'a, 'tcx: 'a> {
    worklist: Vec<ast::NodeId>,
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    live_symbols: Box<FnvHashSet<ast::NodeId>>,
    struct_has_extern_repr: bool,
    ignore_non_const_paths: bool,
    inherited_pub_visibility: bool,
    ignore_variant_stack: Vec<DefId>,
}

impl<'a, 'tcx> MarkSymbolVisitor<'a, 'tcx> {
    fn new(tcx: TyCtxt<'a, 'tcx, 'tcx>,
           worklist: Vec<ast::NodeId>) -> MarkSymbolVisitor<'a, 'tcx> {
        MarkSymbolVisitor {
            worklist: worklist,
            tcx: tcx,
            live_symbols: box FnvHashSet(),
            struct_has_extern_repr: false,
            ignore_non_const_paths: false,
            inherited_pub_visibility: false,
            ignore_variant_stack: vec![],
        }
    }

    fn check_def_id(&mut self, def_id: DefId) {
        if let Some(node_id) = self.tcx.map.as_local_node_id(def_id) {
            if should_explore(self.tcx, node_id) {
                self.worklist.push(node_id);
            }
            self.live_symbols.insert(node_id);
        }
    }

    fn insert_def_id(&mut self, def_id: DefId) {
        if let Some(node_id) = self.tcx.map.as_local_node_id(def_id) {
            debug_assert!(!should_explore(self.tcx, node_id));
            self.live_symbols.insert(node_id);
        }
    }

    fn lookup_and_handle_definition(&mut self, id: ast::NodeId) {
        use ty::TypeVariants::{TyEnum, TyStruct};

        let def = self.tcx.expect_def(id);

        // If `bar` is a trait item, make sure to mark Foo as alive in `Foo::bar`
        match def {
            Def::AssociatedTy(..) | Def::Method(_) | Def::AssociatedConst(_)
            if self.tcx.trait_of_item(def.def_id()).is_some() => {
                if let Some(substs) = self.tcx.tables.borrow().item_substs.get(&id) {
                    match substs.substs.type_at(0).sty {
                        TyEnum(tyid, _) | TyStruct(tyid, _) => {
                            self.check_def_id(tyid.did)
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }

        match def {
            Def::Const(_) | Def::AssociatedConst(..) => {
                self.check_def_id(def.def_id());
            }
            _ if self.ignore_non_const_paths => (),
            Def::PrimTy(_) => (),
            Def::SelfTy(..) => (),
            Def::Variant(enum_id, variant_id) => {
                self.check_def_id(enum_id);
                if !self.ignore_variant_stack.contains(&variant_id) {
                    self.check_def_id(variant_id);
                }
            }
            _ => {
                self.check_def_id(def.def_id());
            }
        }
    }

    fn lookup_and_handle_method(&mut self, id: ast::NodeId) {
        let method_call = ty::MethodCall::expr(id);
        let method = self.tcx.tables.borrow().method_map[&method_call];
        self.check_def_id(method.def_id);
    }

    fn handle_field_access(&mut self, lhs: &hir::Expr, name: ast::Name) {
        if let ty::TyStruct(def, _) = self.tcx.expr_ty_adjusted(lhs).sty {
            self.insert_def_id(def.struct_variant().field_named(name).did);
        } else {
            span_bug!(lhs.span, "named field access on non-struct")
        }
    }

    fn handle_tup_field_access(&mut self, lhs: &hir::Expr, idx: usize) {
        if let ty::TyStruct(def, _) = self.tcx.expr_ty_adjusted(lhs).sty {
            self.insert_def_id(def.struct_variant().fields[idx].did);
        }
    }

    fn handle_field_pattern_match(&mut self, lhs: &hir::Pat,
                                  pats: &[codemap::Spanned<hir::FieldPat>]) {
        let variant = match self.tcx.node_id_to_type(lhs.id).sty {
            ty::TyStruct(adt, _) | ty::TyEnum(adt, _) => {
                adt.variant_of_def(self.tcx.expect_def(lhs.id))
            }
            _ => span_bug!(lhs.span, "non-ADT in struct pattern")
        };
        for pat in pats {
            if let PatKind::Wild = pat.node.pat.node {
                continue;
            }
            self.insert_def_id(variant.field_named(pat.node.name).did);
        }
    }

    fn mark_live_symbols(&mut self) {
        let mut scanned = FnvHashSet();
        while !self.worklist.is_empty() {
            let id = self.worklist.pop().unwrap();
            if scanned.contains(&id) {
                continue
            }
            scanned.insert(id);

            if let Some(ref node) = self.tcx.map.find(id) {
                self.live_symbols.insert(id);
                self.visit_node(node);
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
                    hir::ItemStruct(..) => {
                        self.struct_has_extern_repr = item.attrs.iter().any(|attr| {
                            attr::find_repr_attrs(self.tcx.sess.diagnostic(), attr)
                                .contains(&attr::ReprExtern)
                        });

                        intravisit::walk_item(self, &item);
                    }
                    hir::ItemEnum(..) => {
                        self.inherited_pub_visibility = item.vis == hir::Public;
                        intravisit::walk_item(self, &item);
                    }
                    hir::ItemFn(..)
                    | hir::ItemTy(..)
                    | hir::ItemStatic(..)
                    | hir::ItemConst(..) => {
                        intravisit::walk_item(self, &item);
                    }
                    _ => ()
                }
            }
            ast_map::NodeTraitItem(trait_item) => {
                intravisit::walk_trait_item(self, trait_item);
            }
            ast_map::NodeImplItem(impl_item) => {
                intravisit::walk_impl_item(self, impl_item);
            }
            ast_map::NodeForeignItem(foreign_item) => {
                intravisit::walk_foreign_item(self, &foreign_item);
            }
            _ => ()
        }
        self.struct_has_extern_repr = had_extern_repr;
        self.inherited_pub_visibility = had_inherited_pub_visibility;
    }
}

impl<'a, 'tcx, 'v> Visitor<'v> for MarkSymbolVisitor<'a, 'tcx> {

    fn visit_variant_data(&mut self, def: &hir::VariantData, _: ast::Name,
                        _: &hir::Generics, _: ast::NodeId, _: syntax_pos::Span) {
        let has_extern_repr = self.struct_has_extern_repr;
        let inherited_pub_visibility = self.inherited_pub_visibility;
        let live_fields = def.fields().iter().filter(|f| {
            has_extern_repr || inherited_pub_visibility || f.vis == hir::Public
        });
        self.live_symbols.extend(live_fields.map(|f| f.id));

        intravisit::walk_struct_def(self, def);
    }

    fn visit_expr(&mut self, expr: &hir::Expr) {
        match expr.node {
            hir::ExprMethodCall(..) => {
                self.lookup_and_handle_method(expr.id);
            }
            hir::ExprField(ref lhs, ref name) => {
                self.handle_field_access(&lhs, name.node);
            }
            hir::ExprTupField(ref lhs, idx) => {
                self.handle_tup_field_access(&lhs, idx.node);
            }
            _ => ()
        }

        intravisit::walk_expr(self, expr);
    }

    fn visit_arm(&mut self, arm: &hir::Arm) {
        if arm.pats.len() == 1 {
            let pat = &*arm.pats[0];
            let variants = pat_util::necessary_variants(&self.tcx.def_map.borrow(), pat);

            // Inside the body, ignore constructions of variants
            // necessary for the pattern to match. Those construction sites
            // can't be reached unless the variant is constructed elsewhere.
            let len = self.ignore_variant_stack.len();
            self.ignore_variant_stack.extend_from_slice(&variants);
            intravisit::walk_arm(self, arm);
            self.ignore_variant_stack.truncate(len);
        } else {
            intravisit::walk_arm(self, arm);
        }
    }

    fn visit_pat(&mut self, pat: &hir::Pat) {
        let def_map = &self.tcx.def_map;
        match pat.node {
            PatKind::Struct(_, ref fields, _) => {
                self.handle_field_pattern_match(pat, fields);
            }
            _ if pat_util::pat_is_const(&def_map.borrow(), pat) => {
                // it might be the only use of a const
                self.lookup_and_handle_definition(pat.id)
            }
            _ => ()
        }

        self.ignore_non_const_paths = true;
        intravisit::walk_pat(self, pat);
        self.ignore_non_const_paths = false;
    }

    fn visit_path(&mut self, path: &hir::Path, id: ast::NodeId) {
        self.lookup_and_handle_definition(id);
        intravisit::walk_path(self, path);
    }

    fn visit_path_list_item(&mut self, path: &hir::Path, item: &hir::PathListItem) {
        self.lookup_and_handle_definition(item.node.id());
        intravisit::walk_path_list_item(self, path, item);
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
    fn visit_item(&mut self, item: &hir::Item) {
        let allow_dead_code = has_allow_dead_code_or_lang_attr(&item.attrs);
        if allow_dead_code {
            self.worklist.push(item.id);
        }
        match item.node {
            hir::ItemEnum(ref enum_def, _) if allow_dead_code => {
                self.worklist.extend(enum_def.variants.iter()
                                                      .map(|variant| variant.node.data.id()));
            }
            hir::ItemTrait(_, _, _, ref trait_items) => {
                for trait_item in trait_items {
                    match trait_item.node {
                        hir::ConstTraitItem(_, Some(_)) |
                        hir::MethodTraitItem(_, Some(_)) => {
                            if has_allow_dead_code_or_lang_attr(&trait_item.attrs) {
                                self.worklist.push(trait_item.id);
                            }
                        }
                        _ => {}
                    }
                }
            }
            hir::ItemImpl(_, _, _, ref opt_trait, _, ref impl_items) => {
                for impl_item in impl_items {
                    if opt_trait.is_some() ||
                            has_allow_dead_code_or_lang_attr(&impl_item.attrs) {
                        self.worklist.push(impl_item.id);
                    }
                }
            }
            _ => ()
        }
    }
}

fn create_and_seed_worklist<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                      access_levels: &privacy::AccessLevels,
                                      krate: &hir::Crate)
                                      -> Vec<ast::NodeId> {
    let mut worklist = Vec::new();
    for (id, _) in &access_levels.map {
        worklist.push(*id);
    }

    // Seed entry point
    if let Some((id, _)) = *tcx.sess.entry_fn.borrow() {
        worklist.push(id);
    }

    // Seed implemented trait items
    let mut life_seeder = LifeSeeder {
        worklist: worklist
    };
    krate.visit_all_items(&mut life_seeder);

    return life_seeder.worklist;
}

fn find_live<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                       access_levels: &privacy::AccessLevels,
                       krate: &hir::Crate)
                       -> Box<FnvHashSet<ast::NodeId>> {
    let worklist = create_and_seed_worklist(tcx, access_levels, krate);
    let mut symbol_visitor = MarkSymbolVisitor::new(tcx, worklist);
    symbol_visitor.mark_live_symbols();
    symbol_visitor.live_symbols
}

fn get_struct_ctor_id(item: &hir::Item) -> Option<ast::NodeId> {
    match item.node {
        hir::ItemStruct(ref struct_def, _) if !struct_def.is_struct() => {
            Some(struct_def.id())
        }
        _ => None
    }
}

struct DeadVisitor<'a, 'tcx: 'a> {
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    live_symbols: Box<FnvHashSet<ast::NodeId>>,
}

impl<'a, 'tcx> DeadVisitor<'a, 'tcx> {
    fn should_warn_about_item(&mut self, item: &hir::Item) -> bool {
        let should_warn = match item.node {
            hir::ItemStatic(..)
            | hir::ItemConst(..)
            | hir::ItemFn(..)
            | hir::ItemEnum(..)
            | hir::ItemStruct(..) => true,
            _ => false
        };
        let ctor_id = get_struct_ctor_id(item);
        should_warn && !self.symbol_is_live(item.id, ctor_id)
    }

    fn should_warn_about_field(&mut self, field: &hir::StructField) -> bool {
        let field_type = self.tcx.node_id_to_type(field.id);
        let is_marker_field = match field_type.ty_to_def_id() {
            Some(def_id) => self.tcx.lang_items.items().iter().any(|item| *item == Some(def_id)),
            _ => false
        };
        !field.is_positional()
            && !self.symbol_is_live(field.id, None)
            && !is_marker_field
            && !has_allow_dead_code_or_lang_attr(&field.attrs)
    }

    fn should_warn_about_variant(&mut self, variant: &hir::Variant_) -> bool {
        !self.symbol_is_live(variant.data.id(), None)
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
    fn symbol_is_live(&mut self,
                      id: ast::NodeId,
                      ctor_id: Option<ast::NodeId>)
                      -> bool {
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
        if let Some(impl_list) =
                self.tcx.inherent_impls.borrow().get(&self.tcx.map.local_def_id(id)) {
            for impl_did in impl_list.iter() {
                for item_did in impl_items.get(impl_did).unwrap().iter() {
                    if let Some(item_node_id) =
                            self.tcx.map.as_local_node_id(item_did.def_id()) {
                        if self.live_symbols.contains(&item_node_id) {
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
                      span: syntax_pos::Span,
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
    /// Walk nested items in place so that we don't report dead-code
    /// on inner functions when the outer function is already getting
    /// an error. We could do this also by checking the parents, but
    /// this is how the code is setup and it seems harmless enough.
    fn visit_nested_item(&mut self, item: hir::ItemId) {
        let tcx = self.tcx;
        self.visit_item(tcx.map.expect_item(item.id))
    }

    fn visit_item(&mut self, item: &hir::Item) {
        if self.should_warn_about_item(item) {
            self.warn_dead_code(
                item.id,
                item.span,
                item.name,
                item.node.descriptive_variant()
            );
        } else {
            // Only continue if we didn't warn
            intravisit::walk_item(self, item);
        }
    }

    fn visit_variant(&mut self, variant: &hir::Variant, g: &hir::Generics, id: ast::NodeId) {
        if self.should_warn_about_variant(&variant.node) {
            self.warn_dead_code(variant.node.data.id(), variant.span,
                                variant.node.name, "variant");
        } else {
            intravisit::walk_variant(self, variant, g, id);
        }
    }

    fn visit_foreign_item(&mut self, fi: &hir::ForeignItem) {
        if !self.symbol_is_live(fi.id, None) {
            self.warn_dead_code(fi.id, fi.span, fi.name, fi.node.descriptive_variant());
        }
        intravisit::walk_foreign_item(self, fi);
    }

    fn visit_struct_field(&mut self, field: &hir::StructField) {
        if self.should_warn_about_field(&field) {
            self.warn_dead_code(field.id, field.span,
                                field.name, "struct field");
        }

        intravisit::walk_struct_field(self, field);
    }

    fn visit_impl_item(&mut self, impl_item: &hir::ImplItem) {
        match impl_item.node {
            hir::ImplItemKind::Const(_, ref expr) => {
                if !self.symbol_is_live(impl_item.id, None) {
                    self.warn_dead_code(impl_item.id, impl_item.span,
                                        impl_item.name, "associated const");
                }
                intravisit::walk_expr(self, expr)
            }
            hir::ImplItemKind::Method(_, ref body) => {
                if !self.symbol_is_live(impl_item.id, None) {
                    self.warn_dead_code(impl_item.id, impl_item.span,
                                        impl_item.name, "method");
                }
                intravisit::walk_block(self, body)
            }
            hir::ImplItemKind::Type(..) => {}
        }
    }

    // Overwrite so that we don't warn the trait item itself.
    fn visit_trait_item(&mut self, trait_item: &hir::TraitItem) {
        match trait_item.node {
            hir::ConstTraitItem(_, Some(ref expr)) => {
                intravisit::walk_expr(self, expr)
            }
            hir::MethodTraitItem(_, Some(ref body)) => {
                intravisit::walk_block(self, body)
            }
            hir::ConstTraitItem(_, None) |
            hir::MethodTraitItem(_, None) |
            hir::TypeTraitItem(..) => {}
        }
    }
}

pub fn check_crate<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                             access_levels: &privacy::AccessLevels) {
    let _task = tcx.dep_graph.in_task(DepNode::DeadCheck);
    let krate = tcx.map.krate();
    let live_symbols = find_live(tcx, access_levels, krate);
    let mut visitor = DeadVisitor { tcx: tcx, live_symbols: live_symbols };
    intravisit::walk_crate(&mut visitor, krate);
}
