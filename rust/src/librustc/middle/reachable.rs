// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Finds items that are externally reachable, to determine which items
// need to have their metadata (and possibly their AST) serialized.
// All items that can be referred to through an exported name are
// reachable, and when a reachable thing is inline or generic, it
// makes all other generics or inline functions that it references
// reachable as well.

use dep_graph::DepNode;
use hir::map as ast_map;
use hir::def::Def;
use hir::def_id::DefId;
use ty::{self, TyCtxt};
use middle::privacy;
use session::config;
use util::nodemap::{NodeSet, FnvHashSet};

use syntax::abi::Abi;
use syntax::ast;
use syntax::attr;
use hir;
use hir::intravisit::Visitor;
use hir::intravisit;

// Returns true if the given set of generics implies that the item it's
// associated with must be inlined.
fn generics_require_inlining(generics: &hir::Generics) -> bool {
    !generics.ty_params.is_empty()
}

// Returns true if the given item must be inlined because it may be
// monomorphized or it was marked with `#[inline]`. This will only return
// true for functions.
fn item_might_be_inlined(item: &hir::Item) -> bool {
    if attr::requests_inline(&item.attrs) {
        return true
    }

    match item.node {
        hir::ItemImpl(_, _, ref generics, _, _, _) |
        hir::ItemFn(_, _, _, _, ref generics, _) => {
            generics_require_inlining(generics)
        }
        _ => false,
    }
}

fn method_might_be_inlined<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                     sig: &hir::MethodSig,
                                     impl_item: &hir::ImplItem,
                                     impl_src: DefId) -> bool {
    if attr::requests_inline(&impl_item.attrs) ||
        generics_require_inlining(&sig.generics) {
        return true
    }
    if let Some(impl_node_id) = tcx.map.as_local_node_id(impl_src) {
        match tcx.map.find(impl_node_id) {
            Some(ast_map::NodeItem(item)) =>
                item_might_be_inlined(&item),
            Some(..) | None =>
                span_bug!(impl_item.span, "impl did is not an item")
        }
    } else {
        span_bug!(impl_item.span, "found a foreign impl as a parent of a local method")
    }
}

// Information needed while computing reachability.
struct ReachableContext<'a, 'tcx: 'a> {
    // The type context.
    tcx: TyCtxt<'a, 'tcx, 'tcx>,
    // The set of items which must be exported in the linkage sense.
    reachable_symbols: NodeSet,
    // A worklist of item IDs. Each item ID in this worklist will be inlined
    // and will be scanned for further references.
    worklist: Vec<ast::NodeId>,
    // Whether any output of this compilation is a library
    any_library: bool,
}

impl<'a, 'tcx, 'v> Visitor<'v> for ReachableContext<'a, 'tcx> {
    fn visit_expr(&mut self, expr: &hir::Expr) {
        match expr.node {
            hir::ExprPath(..) => {
                let def = self.tcx.expect_def(expr.id);
                let def_id = def.def_id();
                if let Some(node_id) = self.tcx.map.as_local_node_id(def_id) {
                    if self.def_id_represents_local_inlined_item(def_id) {
                        self.worklist.push(node_id);
                    } else {
                        match def {
                            // If this path leads to a constant, then we need to
                            // recurse into the constant to continue finding
                            // items that are reachable.
                            Def::Const(..) | Def::AssociatedConst(..) => {
                                self.worklist.push(node_id);
                            }

                            // If this wasn't a static, then the destination is
                            // surely reachable.
                            _ => {
                                self.reachable_symbols.insert(node_id);
                            }
                        }
                    }
                }
            }
            hir::ExprMethodCall(..) => {
                let method_call = ty::MethodCall::expr(expr.id);
                let def_id = self.tcx.tables.borrow().method_map[&method_call].def_id;

                // Mark the trait item (and, possibly, its default impl) as reachable
                // Or mark inherent impl item as reachable
                if let Some(node_id) = self.tcx.map.as_local_node_id(def_id) {
                    if self.def_id_represents_local_inlined_item(def_id) {
                        self.worklist.push(node_id)
                    }
                    self.reachable_symbols.insert(node_id);
                }
            }
            _ => {}
        }

        intravisit::walk_expr(self, expr)
    }
}

impl<'a, 'tcx> ReachableContext<'a, 'tcx> {
    // Creates a new reachability computation context.
    fn new(tcx: TyCtxt<'a, 'tcx, 'tcx>) -> ReachableContext<'a, 'tcx> {
        let any_library = tcx.sess.crate_types.borrow().iter().any(|ty| {
            *ty == config::CrateTypeRlib || *ty == config::CrateTypeDylib
        });
        ReachableContext {
            tcx: tcx,
            reachable_symbols: NodeSet(),
            worklist: Vec::new(),
            any_library: any_library,
        }
    }

    // Returns true if the given def ID represents a local item that is
    // eligible for inlining and false otherwise.
    fn def_id_represents_local_inlined_item(&self, def_id: DefId) -> bool {
        let node_id = match self.tcx.map.as_local_node_id(def_id) {
            Some(node_id) => node_id,
            None => { return false; }
        };

        match self.tcx.map.find(node_id) {
            Some(ast_map::NodeItem(item)) => {
                match item.node {
                    hir::ItemFn(..) => item_might_be_inlined(&item),
                    _ => false,
                }
            }
            Some(ast_map::NodeTraitItem(trait_method)) => {
                match trait_method.node {
                    hir::ConstTraitItem(_, ref default) => default.is_some(),
                    hir::MethodTraitItem(_, ref body) => body.is_some(),
                    hir::TypeTraitItem(..) => false,
                }
            }
            Some(ast_map::NodeImplItem(impl_item)) => {
                match impl_item.node {
                    hir::ImplItemKind::Const(..) => true,
                    hir::ImplItemKind::Method(ref sig, _) => {
                        if generics_require_inlining(&sig.generics) ||
                                attr::requests_inline(&impl_item.attrs) {
                            true
                        } else {
                            let impl_did = self.tcx
                                               .map
                                               .get_parent_did(node_id);
                            // Check the impl. If the generics on the self
                            // type of the impl require inlining, this method
                            // does too.
                            let impl_node_id = self.tcx.map.as_local_node_id(impl_did).unwrap();
                            match self.tcx.map.expect_item(impl_node_id).node {
                                hir::ItemImpl(_, _, ref generics, _, _, _) => {
                                    generics_require_inlining(generics)
                                }
                                _ => false
                            }
                        }
                    }
                    hir::ImplItemKind::Type(_) => false,
                }
            }
            Some(_) => false,
            None => false   // This will happen for default methods.
        }
    }

    // Step 2: Mark all symbols that the symbols on the worklist touch.
    fn propagate(&mut self) {
        let mut scanned = FnvHashSet();
        loop {
            let search_item = match self.worklist.pop() {
                Some(item) => item,
                None => break,
            };
            if !scanned.insert(search_item) {
                continue
            }

            if let Some(ref item) = self.tcx.map.find(search_item) {
                self.propagate_node(item, search_item);
            }
        }
    }

    fn propagate_node(&mut self, node: &ast_map::Node,
                      search_item: ast::NodeId) {
        if !self.any_library {
            // If we are building an executable, only explicitly extern
            // types need to be exported.
            if let ast_map::NodeItem(item) = *node {
                let reachable = if let hir::ItemFn(_, _, _, abi, _, _) = item.node {
                    abi != Abi::Rust
                } else {
                    false
                };
                let is_extern = attr::contains_extern_indicator(&self.tcx.sess.diagnostic(),
                                                                &item.attrs);
                if reachable || is_extern {
                    self.reachable_symbols.insert(search_item);
                }
            }
        } else {
            // If we are building a library, then reachable symbols will
            // continue to participate in linkage after this product is
            // produced. In this case, we traverse the ast node, recursing on
            // all reachable nodes from this one.
            self.reachable_symbols.insert(search_item);
        }

        match *node {
            ast_map::NodeItem(item) => {
                match item.node {
                    hir::ItemFn(_, _, _, _, _, ref search_block) => {
                        if item_might_be_inlined(&item) {
                            intravisit::walk_block(self, &search_block)
                        }
                    }

                    // Reachable constants will be inlined into other crates
                    // unconditionally, so we need to make sure that their
                    // contents are also reachable.
                    hir::ItemConst(_, ref init) => {
                        self.visit_expr(&init);
                    }

                    // These are normal, nothing reachable about these
                    // inherently and their children are already in the
                    // worklist, as determined by the privacy pass
                    hir::ItemExternCrate(_) | hir::ItemUse(_) |
                    hir::ItemTy(..) | hir::ItemStatic(_, _, _) |
                    hir::ItemMod(..) | hir::ItemForeignMod(..) |
                    hir::ItemImpl(..) | hir::ItemTrait(..) |
                    hir::ItemStruct(..) | hir::ItemEnum(..) |
                    hir::ItemDefaultImpl(..) => {}
                }
            }
            ast_map::NodeTraitItem(trait_method) => {
                match trait_method.node {
                    hir::ConstTraitItem(_, None) |
                    hir::MethodTraitItem(_, None) => {
                        // Keep going, nothing to get exported
                    }
                    hir::ConstTraitItem(_, Some(ref expr)) => {
                        self.visit_expr(&expr);
                    }
                    hir::MethodTraitItem(_, Some(ref body)) => {
                        intravisit::walk_block(self, body);
                    }
                    hir::TypeTraitItem(..) => {}
                }
            }
            ast_map::NodeImplItem(impl_item) => {
                match impl_item.node {
                    hir::ImplItemKind::Const(_, ref expr) => {
                        self.visit_expr(&expr);
                    }
                    hir::ImplItemKind::Method(ref sig, ref body) => {
                        let did = self.tcx.map.get_parent_did(search_item);
                        if method_might_be_inlined(self.tcx, sig, impl_item, did) {
                            intravisit::walk_block(self, body)
                        }
                    }
                    hir::ImplItemKind::Type(_) => {}
                }
            }
            // Nothing to recurse on for these
            ast_map::NodeForeignItem(_) |
            ast_map::NodeVariant(_) |
            ast_map::NodeStructCtor(_) => {}
            _ => {
                bug!("found unexpected thingy in worklist: {}",
                     self.tcx.map.node_to_string(search_item))
            }
        }
    }
}

// Some methods from non-exported (completely private) trait impls still have to be
// reachable if they are called from inlinable code. Generally, it's not known until
// monomorphization if a specific trait impl item can be reachable or not. So, we
// conservatively mark all of them as reachable.
// FIXME: One possible strategy for pruning the reachable set is to avoid marking impl
// items of non-exported traits (or maybe all local traits?) unless their respective
// trait items are used from inlinable code through method call syntax or UFCS, or their
// trait is a lang item.
struct CollectPrivateImplItemsVisitor<'a> {
    access_levels: &'a privacy::AccessLevels,
    worklist: &'a mut Vec<ast::NodeId>,
}

impl<'a, 'v> Visitor<'v> for CollectPrivateImplItemsVisitor<'a> {
    fn visit_item(&mut self, item: &hir::Item) {
        // We need only trait impls here, not inherent impls, and only non-exported ones
        if let hir::ItemImpl(_, _, _, Some(_), _, ref impl_items) = item.node {
            if !self.access_levels.is_reachable(item.id) {
                for impl_item in impl_items {
                    self.worklist.push(impl_item.id);
                }
            }
        }
    }
}

pub fn find_reachable<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                access_levels: &privacy::AccessLevels)
                                -> NodeSet {
    let _task = tcx.dep_graph.in_task(DepNode::Reachability);

    let mut reachable_context = ReachableContext::new(tcx);

    // Step 1: Seed the worklist with all nodes which were found to be public as
    //         a result of the privacy pass along with all local lang items and impl items.
    //         If other crates link to us, they're going to expect to be able to
    //         use the lang items, so we need to be sure to mark them as
    //         exported.
    for (id, _) in &access_levels.map {
        reachable_context.worklist.push(*id);
    }
    for item in tcx.lang_items.items().iter() {
        if let Some(did) = *item {
            if let Some(node_id) = tcx.map.as_local_node_id(did) {
                reachable_context.worklist.push(node_id);
            }
        }
    }
    {
        let mut collect_private_impl_items = CollectPrivateImplItemsVisitor {
            access_levels: access_levels,
            worklist: &mut reachable_context.worklist,
        };
        tcx.map.krate().visit_all_items(&mut collect_private_impl_items);
    }

    // Step 2: Mark all symbols that the symbols on the worklist touch.
    reachable_context.propagate();

    // Return the set of reachable symbols.
    reachable_context.reachable_symbols
}
