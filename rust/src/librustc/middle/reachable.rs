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

use middle::def;
use middle::ty;
use middle::privacy;
use session::config;
use util::nodemap::NodeSet;

use std::collections::HashSet;
use syntax::abi;
use syntax::ast;
use syntax::ast_map;
use syntax::ast_util::{is_local, PostExpansionMethod};
use syntax::attr::{InlineAlways, InlineHint, InlineNever, InlineNone};
use syntax::attr;
use syntax::visit::Visitor;
use syntax::visit;

// Returns true if the given set of attributes contains the `#[inline]`
// attribute.
fn attributes_specify_inlining(attrs: &[ast::Attribute]) -> bool {
    match attr::find_inline_attr(attrs) {
        InlineNone | InlineNever => false,
        InlineAlways | InlineHint => true,
    }
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
    if attributes_specify_inlining(&item.attrs[]) {
        return true
    }

    match item.node {
        ast::ItemImpl(_, _, ref generics, _, _, _) |
        ast::ItemFn(_, _, _, ref generics, _) => {
            generics_require_inlining(generics)
        }
        _ => false,
    }
}

fn method_might_be_inlined(tcx: &ty::ctxt, method: &ast::Method,
                           impl_src: ast::DefId) -> bool {
    if attributes_specify_inlining(&method.attrs[]) ||
        generics_require_inlining(method.pe_generics()) {
        return true
    }
    if is_local(impl_src) {
        {
            match tcx.map.find(impl_src.node) {
                Some(ast_map::NodeItem(item)) => {
                    item_might_be_inlined(&*item)
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
struct ReachableContext<'a, 'tcx: 'a> {
    // The type context.
    tcx: &'a ty::ctxt<'tcx>,
    // The set of items which must be exported in the linkage sense.
    reachable_symbols: NodeSet,
    // A worklist of item IDs. Each item ID in this worklist will be inlined
    // and will be scanned for further references.
    worklist: Vec<ast::NodeId>,
    // Whether any output of this compilation is a library
    any_library: bool,
}

impl<'a, 'tcx, 'v> Visitor<'v> for ReachableContext<'a, 'tcx> {

    fn visit_expr(&mut self, expr: &ast::Expr) {

        match expr.node {
            ast::ExprPath(_) | ast::ExprQPath(_) => {
                let def = match self.tcx.def_map.borrow().get(&expr.id) {
                    Some(&def) => def,
                    None => {
                        self.tcx.sess.span_bug(expr.span,
                                               "def ID not in def map?!")
                    }
                };

                let def_id = def.def_id();
                if is_local(def_id) {
                    if self.def_id_represents_local_inlined_item(def_id) {
                        self.worklist.push(def_id.node)
                    } else {
                        match def {
                            // If this path leads to a constant, then we need to
                            // recurse into the constant to continue finding
                            // items that are reachable.
                            def::DefConst(..) => {
                                self.worklist.push(def_id.node);
                            }

                            // If this wasn't a static, then the destination is
                            // surely reachable.
                            _ => {
                                self.reachable_symbols.insert(def_id.node);
                            }
                        }
                    }
                }
            }
            ast::ExprMethodCall(..) => {
                let method_call = ty::MethodCall::expr(expr.id);
                match (*self.tcx.method_map.borrow())[method_call].origin {
                    ty::MethodStatic(def_id) => {
                        if is_local(def_id) {
                            if self.def_id_represents_local_inlined_item(def_id) {
                                self.worklist.push(def_id.node)
                            }
                            self.reachable_symbols.insert(def_id.node);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        visit::walk_expr(self, expr)
    }

    fn visit_item(&mut self, _item: &ast::Item) {
        // Do not recurse into items. These items will be added to the worklist
        // and recursed into manually if necessary.
    }
}

impl<'a, 'tcx> ReachableContext<'a, 'tcx> {
    // Creates a new reachability computation context.
    fn new(tcx: &'a ty::ctxt<'tcx>) -> ReachableContext<'a, 'tcx> {
        let any_library = tcx.sess.crate_types.borrow().iter().any(|ty| {
            *ty != config::CrateTypeExecutable
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
    fn def_id_represents_local_inlined_item(&self, def_id: ast::DefId) -> bool {
        if def_id.krate != ast::LOCAL_CRATE {
            return false
        }

        let node_id = def_id.node;
        match self.tcx.map.find(node_id) {
            Some(ast_map::NodeItem(item)) => {
                match item.node {
                    ast::ItemFn(..) => item_might_be_inlined(&*item),
                    _ => false,
                }
            }
            Some(ast_map::NodeTraitItem(trait_method)) => {
                match *trait_method {
                    ast::RequiredMethod(_) => false,
                    ast::ProvidedMethod(_) => true,
                    ast::TypeTraitItem(_) => false,
                }
            }
            Some(ast_map::NodeImplItem(impl_item)) => {
                match *impl_item {
                    ast::MethodImplItem(ref method) => {
                        if generics_require_inlining(method.pe_generics()) ||
                                attributes_specify_inlining(
                                    &method.attrs[]) {
                            true
                        } else {
                            let impl_did = self.tcx
                                               .map
                                               .get_parent_did(node_id);
                            // Check the impl. If the generics on the self
                            // type of the impl require inlining, this method
                            // does too.
                            assert!(impl_did.krate == ast::LOCAL_CRATE);
                            match self.tcx
                                      .map
                                      .expect_item(impl_did.node)
                                      .node {
                                ast::ItemImpl(_, _, ref generics, _, _, _) => {
                                    generics_require_inlining(generics)
                                }
                                _ => false
                            }
                        }
                    }
                    ast::TypeImplItem(_) => false,
                }
            }
            Some(_) => false,
            None => false   // This will happen for default methods.
        }
    }

    // Step 2: Mark all symbols that the symbols on the worklist touch.
    fn propagate(&mut self) {
        let mut scanned = HashSet::new();
        loop {
            let search_item = match self.worklist.pop() {
                Some(item) => item,
                None => break,
            };
            if !scanned.insert(search_item) {
                continue
            }

            match self.tcx.map.find(search_item) {
                Some(ref item) => self.propagate_node(item, search_item),
                None if search_item == ast::CRATE_NODE_ID => {}
                None => {
                    self.tcx.sess.bug(&format!("found unmapped ID in worklist: \
                                               {}",
                                              search_item)[])
                }
            }
        }
    }

    fn propagate_node(&mut self, node: &ast_map::Node,
                      search_item: ast::NodeId) {
        if !self.any_library {
            // If we are building an executable, then there's no need to flag
            // anything as external except for `extern fn` types. These
            // functions may still participate in some form of native interface,
            // but all other rust-only interfaces can be private (they will not
            // participate in linkage after this product is produced)
            if let ast_map::NodeItem(item) = *node {
                if let ast::ItemFn(_, _, abi, _, _) = item.node {
                    if abi != abi::Rust {
                        self.reachable_symbols.insert(search_item);
                    }
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
                    ast::ItemFn(_, _, _, _, ref search_block) => {
                        if item_might_be_inlined(&*item) {
                            visit::walk_block(self, &**search_block)
                        }
                    }

                    // Reachable constants will be inlined into other crates
                    // unconditionally, so we need to make sure that their
                    // contents are also reachable.
                    ast::ItemConst(_, ref init) => {
                        self.visit_expr(&**init);
                    }

                    // These are normal, nothing reachable about these
                    // inherently and their children are already in the
                    // worklist, as determined by the privacy pass
                    ast::ItemExternCrate(_) | ast::ItemUse(_) |
                    ast::ItemTy(..) | ast::ItemStatic(_, _, _) |
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
            ast_map::NodeTraitItem(trait_method) => {
                match *trait_method {
                    ast::RequiredMethod(..) => {
                        // Keep going, nothing to get exported
                    }
                    ast::ProvidedMethod(ref method) => {
                        visit::walk_block(self, &*method.pe_body());
                    }
                    ast::TypeTraitItem(_) => {}
                }
            }
            ast_map::NodeImplItem(impl_item) => {
                match *impl_item {
                    ast::MethodImplItem(ref method) => {
                        let did = self.tcx.map.get_parent_did(search_item);
                        if method_might_be_inlined(self.tcx, &**method, did) {
                            visit::walk_block(self, method.pe_body())
                        }
                    }
                    ast::TypeImplItem(_) => {}
                }
            }
            // Nothing to recurse on for these
            ast_map::NodeForeignItem(_) |
            ast_map::NodeVariant(_) |
            ast_map::NodeStructCtor(_) => {}
            _ => {
                self.tcx
                    .sess
                    .bug(&format!("found unexpected thingy in worklist: {}",
                                 self.tcx
                                     .map
                                     .node_to_string(search_item))[])
            }
        }
    }

    // Step 3: Mark all destructors as reachable.
    //
    // FIXME(pcwalton): This is a conservative overapproximation, but fixing
    // this properly would result in the necessity of computing *type*
    // reachability, which might result in a compile time loss.
    fn mark_destructors_reachable(&mut self) {
        for (_, destructor_def_id) in self.tcx.destructor_for_type.borrow().iter() {
            if destructor_def_id.krate == ast::LOCAL_CRATE {
                self.reachable_symbols.insert(destructor_def_id.node);
            }
        }
    }
}

pub fn find_reachable(tcx: &ty::ctxt,
                      exported_items: &privacy::ExportedItems)
                      -> NodeSet {
    let mut reachable_context = ReachableContext::new(tcx);

    // Step 1: Seed the worklist with all nodes which were found to be public as
    //         a result of the privacy pass along with all local lang items. If
    //         other crates link to us, they're going to expect to be able to
    //         use the lang items, so we need to be sure to mark them as
    //         exported.
    for id in exported_items.iter() {
        reachable_context.worklist.push(*id);
    }
    for (_, item) in tcx.lang_items.items() {
        match *item {
            Some(did) if is_local(did) => {
                reachable_context.worklist.push(did.node);
            }
            _ => {}
        }
    }

    // Step 2: Mark all symbols that the symbols on the worklist touch.
    reachable_context.propagate();

    // Step 3: Mark all destructors as reachable.
    reachable_context.mark_destructors_reachable();

    // Return the set of reachable symbols.
    reachable_context.reachable_symbols
}
