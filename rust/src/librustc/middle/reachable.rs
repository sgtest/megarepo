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

use middle::ty;
use middle::typeck;
use middle::privacy;

use std::cell::RefCell;
use std::hashmap::HashSet;
use syntax::ast;
use syntax::ast_map;
use syntax::ast_util::{def_id_of_def, is_local};
use syntax::attr;
use syntax::visit::Visitor;
use syntax::visit;

// Returns true if the given set of attributes contains the `#[inline]`
// attribute.
fn attributes_specify_inlining(attrs: &[ast::Attribute]) -> bool {
    attr::contains_name(attrs, "inline")
}

// Returns true if the given set of generics implies that the item it's
// associated with must be inlined.
fn generics_require_inlining(generics: &ast::Generics) -> bool {
    !generics.ty_params.is_empty()
}

// Returns true if the given item must be inlined because it may be
// monomorphized or it was marked with `#[inline]`. This will only return
// true for functions.
fn item_might_be_inlined(item: &ast::Item) -> bool {
    if attributes_specify_inlining(item.attrs) {
        return true
    }

    match item.node {
        ast::ItemImpl(ref generics, _, _, _) |
        ast::ItemFn(_, _, _, ref generics, _) => {
            generics_require_inlining(generics)
        }
        _ => false,
    }
}

fn method_might_be_inlined(tcx: ty::ctxt, method: &ast::Method,
                           impl_src: ast::DefId) -> bool {
    if attributes_specify_inlining(method.attrs) ||
        generics_require_inlining(&method.generics) {
        return true
    }
    if is_local(impl_src) {
        {
            match tcx.map.find(impl_src.node) {
                Some(ast_map::NodeItem(item)) => {
                    item_might_be_inlined(item)
                }
                Some(..) | None => {
                    tcx.sess.span_bug(method.span, "impl did is not an item")
                }
            }
        }
    } else {
        tcx.sess.span_bug(method.span, "found a foreign impl as a parent of a \
                                        local method")
    }
}

// Information needed while computing reachability.
struct ReachableContext {
    // The type context.
    tcx: ty::ctxt,
    // The method map, which links node IDs of method call expressions to the
    // methods they've been resolved to.
    method_map: typeck::method_map,
    // The set of items which must be exported in the linkage sense.
    reachable_symbols: @RefCell<HashSet<ast::NodeId>>,
    // A worklist of item IDs. Each item ID in this worklist will be inlined
    // and will be scanned for further references.
    worklist: @RefCell<~[ast::NodeId]>,
}

struct MarkSymbolVisitor {
    worklist: @RefCell<~[ast::NodeId]>,
    method_map: typeck::method_map,
    tcx: ty::ctxt,
    reachable_symbols: @RefCell<HashSet<ast::NodeId>>,
}

impl Visitor<()> for MarkSymbolVisitor {

    fn visit_expr(&mut self, expr: &ast::Expr, _: ()) {

        match expr.node {
            ast::ExprPath(_) => {
                let def_map = self.tcx.def_map.borrow();
                let def = match def_map.get().find(&expr.id) {
                    Some(&def) => def,
                    None => {
                        self.tcx.sess.span_bug(expr.span,
                                               "def ID not in def map?!")
                    }
                };

                let def_id = def_id_of_def(def);
                if is_local(def_id) {
                    if ReachableContext::
                        def_id_represents_local_inlined_item(self.tcx, def_id) {
                            {
                                let mut worklist = self.worklist.borrow_mut();
                                worklist.get().push(def_id.node)
                            }
                    } else {
                        match def {
                            // If this path leads to a static, then we may have
                            // to do some work to figure out whether the static
                            // is indeed reachable (address_insignificant
                            // statics are *never* reachable).
                            ast::DefStatic(..) => {
                                let mut worklist = self.worklist.borrow_mut();
                                worklist.get().push(def_id.node);
                            }

                            // If this wasn't a static, then this destination is
                            // surely reachable.
                            _ => {
                                let mut reachable_symbols =
                                    self.reachable_symbols.borrow_mut();
                                reachable_symbols.get().insert(def_id.node);
                            }
                        }
                    }
                }
            }
            ast::ExprMethodCall(..) => {
                let method_map = self.method_map.borrow();
                match method_map.get().find(&expr.id) {
                    Some(&typeck::method_static(def_id)) => {
                        if is_local(def_id) {
                            if ReachableContext::
                                def_id_represents_local_inlined_item(
                                    self.tcx,
                                    def_id) {
                                {
                                    let mut worklist = self.worklist
                                                           .borrow_mut();
                                    worklist.get().push(def_id.node)
                                }
                            }
                            {
                                let mut reachable_symbols =
                                    self.reachable_symbols.borrow_mut();
                                reachable_symbols.get().insert(def_id.node);
                            }
                        }
                    }
                    Some(_) => {}
                    None => {
                        self.tcx.sess.span_bug(expr.span,
                            "method call expression not in method map?!")
                    }
                }
            }
            _ => {}
        }

        visit::walk_expr(self, expr, ())
    }

    fn visit_item(&mut self, _item: &ast::Item, _: ()) {
        // Do not recurse into items. These items will be added to the worklist
        // and recursed into manually if necessary.
    }
}

impl ReachableContext {
    // Creates a new reachability computation context.
    fn new(tcx: ty::ctxt, method_map: typeck::method_map) -> ReachableContext {
        ReachableContext {
            tcx: tcx,
            method_map: method_map,
            reachable_symbols: @RefCell::new(HashSet::new()),
            worklist: @RefCell::new(~[]),
        }
    }

    // Returns true if the given def ID represents a local item that is
    // eligible for inlining and false otherwise.
    fn def_id_represents_local_inlined_item(tcx: ty::ctxt, def_id: ast::DefId)
                                            -> bool {
        if def_id.krate != ast::LOCAL_CRATE {
            return false
        }

        let node_id = def_id.node;
        match tcx.map.find(node_id) {
            Some(ast_map::NodeItem(item)) => {
                match item.node {
                    ast::ItemFn(..) => item_might_be_inlined(item),
                    _ => false,
                }
            }
            Some(ast_map::NodeTraitMethod(trait_method)) => {
                match *trait_method {
                    ast::Required(_) => false,
                    ast::Provided(_) => true,
                }
            }
            Some(ast_map::NodeMethod(method)) => {
                if generics_require_inlining(&method.generics) ||
                        attributes_specify_inlining(method.attrs) {
                    true
                } else {
                    let impl_did = tcx.map.get_parent_did(node_id);
                    // Check the impl. If the generics on the self type of the
                    // impl require inlining, this method does too.
                    assert!(impl_did.krate == ast::LOCAL_CRATE);
                    match tcx.map.expect_item(impl_did.node).node {
                        ast::ItemImpl(ref generics, _, _, _) => {
                            generics_require_inlining(generics)
                        }
                        _ => false
                    }
                }
            }
            Some(_) => false,
            None => false   // This will happen for default methods.
        }
    }

    // Helper function to set up a visitor for `propagate()` below.
    fn init_visitor(&self) -> MarkSymbolVisitor {
        let (worklist, method_map) = (self.worklist, self.method_map);
        let (tcx, reachable_symbols) = (self.tcx, self.reachable_symbols);

        MarkSymbolVisitor {
            worklist: worklist,
            method_map: method_map,
            tcx: tcx,
            reachable_symbols: reachable_symbols,
        }
    }

    // Step 2: Mark all symbols that the symbols on the worklist touch.
    fn propagate(&self) {
        let mut visitor = self.init_visitor();
        let mut scanned = HashSet::new();
        loop {
            let search_item = {
                let mut worklist = self.worklist.borrow_mut();
                if worklist.get().len() == 0 {
                    break
                }
                let search_item = worklist.get().pop().unwrap();
                if scanned.contains(&search_item) {
                    continue
                }
                search_item
            };

            scanned.insert(search_item);
            match self.tcx.map.find(search_item) {
                Some(ref item) => self.propagate_node(item, search_item,
                                                  &mut visitor),
                None if search_item == ast::CRATE_NODE_ID => {}
                None => {
                    self.tcx.sess.bug(format!("found unmapped ID in worklist: \
                                               {}",
                                              search_item))
                }
            }
        }
    }

    fn propagate_node(&self, node: &ast_map::Node,
                      search_item: ast::NodeId,
                      visitor: &mut MarkSymbolVisitor) {
        if !self.tcx.sess.building_library.get() {
            // If we are building an executable, then there's no need to flag
            // anything as external except for `extern fn` types. These
            // functions may still participate in some form of native interface,
            // but all other rust-only interfaces can be private (they will not
            // participate in linkage after this product is produced)
            match *node {
                ast_map::NodeItem(item) => {
                    match item.node {
                        ast::ItemFn(_, ast::ExternFn, _, _, _) => {
                            let mut reachable_symbols =
                                self.reachable_symbols.borrow_mut();
                            reachable_symbols.get().insert(search_item);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        } else {
            // If we are building a library, then reachable symbols will
            // continue to participate in linkage after this product is
            // produced. In this case, we traverse the ast node, recursing on
            // all reachable nodes from this one.
            let mut reachable_symbols = self.reachable_symbols.borrow_mut();
            reachable_symbols.get().insert(search_item);
        }

        match *node {
            ast_map::NodeItem(item) => {
                match item.node {
                    ast::ItemFn(_, _, _, _, search_block) => {
                        if item_might_be_inlined(item) {
                            visit::walk_block(visitor, search_block, ())
                        }
                    }

                    // Statics with insignificant addresses are not reachable
                    // because they're inlined specially into all other crates.
                    ast::ItemStatic(..) => {
                        if attr::contains_name(item.attrs,
                                               "address_insignificant") {
                            let mut reachable_symbols =
                                self.reachable_symbols.borrow_mut();
                            reachable_symbols.get().remove(&search_item);
                        }
                    }

                    // These are normal, nothing reachable about these
                    // inherently and their children are already in the
                    // worklist, as determined by the privacy pass
                    ast::ItemTy(..) |
                    ast::ItemMod(..) | ast::ItemForeignMod(..) |
                    ast::ItemImpl(..) | ast::ItemTrait(..) |
                    ast::ItemStruct(..) | ast::ItemEnum(..) => {}

                    _ => {
                        self.tcx.sess.span_bug(item.span,
                                               "found non-function item \
                                                in worklist?!")
                    }
                }
            }
            ast_map::NodeTraitMethod(trait_method) => {
                match *trait_method {
                    ast::Required(..) => {
                        // Keep going, nothing to get exported
                    }
                    ast::Provided(ref method) => {
                        visit::walk_block(visitor, method.body, ())
                    }
                }
            }
            ast_map::NodeMethod(method) => {
                let did = self.tcx.map.get_parent_did(search_item);
                if method_might_be_inlined(self.tcx, method, did) {
                    visit::walk_block(visitor, method.body, ())
                }
            }
            // Nothing to recurse on for these
            ast_map::NodeForeignItem(_) |
            ast_map::NodeVariant(_) |
            ast_map::NodeStructCtor(_) => {}
            _ => {
                self.tcx.sess.bug(format!("found unexpected thingy in \
                                           worklist: {}",
                                          self.tcx.map.node_to_str(search_item)))
            }
        }
    }

    // Step 3: Mark all destructors as reachable.
    //
    // FIXME(pcwalton): This is a conservative overapproximation, but fixing
    // this properly would result in the necessity of computing *type*
    // reachability, which might result in a compile time loss.
    fn mark_destructors_reachable(&self) {
        let destructor_for_type = self.tcx.destructor_for_type.borrow();
        for (_, destructor_def_id) in destructor_for_type.get().iter() {
            if destructor_def_id.krate == ast::LOCAL_CRATE {
                let mut reachable_symbols = self.reachable_symbols
                                                .borrow_mut();
                reachable_symbols.get().insert(destructor_def_id.node);
            }
        }
    }
}

pub fn find_reachable(tcx: ty::ctxt,
                      method_map: typeck::method_map,
                      exported_items: &privacy::ExportedItems)
                      -> @RefCell<HashSet<ast::NodeId>> {
    let reachable_context = ReachableContext::new(tcx, method_map);

    // Step 1: Seed the worklist with all nodes which were found to be public as
    //         a result of the privacy pass along with all local lang items. If
    //         other crates link to us, they're going to expect to be able to
    //         use the lang items, so we need to be sure to mark them as
    //         exported.
    let mut worklist = reachable_context.worklist.borrow_mut();
    for &id in exported_items.iter() {
        worklist.get().push(id);
    }
    for (_, item) in tcx.lang_items.items() {
        match *item {
            Some(did) if is_local(did) => {
                worklist.get().push(did.node);
            }
            _ => {}
        }
    }
    drop(worklist);

    // Step 2: Mark all symbols that the symbols on the worklist touch.
    reachable_context.propagate();

    // Step 3: Mark all destructors as reachable.
    reachable_context.mark_destructors_reachable();

    // Return the set of reachable symbols.
    reachable_context.reachable_symbols
}
