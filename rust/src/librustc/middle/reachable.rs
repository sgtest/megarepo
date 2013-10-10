// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
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
use middle::resolve;

use std::hashmap::HashSet;
use syntax::ast::*;
use syntax::ast_map;
use syntax::ast_util::{def_id_of_def, is_local};
use syntax::attr;
use syntax::parse::token;
use syntax::visit::Visitor;
use syntax::visit;

// Returns true if the given set of attributes contains the `#[inline]`
// attribute.
fn attributes_specify_inlining(attrs: &[Attribute]) -> bool {
    attr::contains_name(attrs, "inline")
}

// Returns true if the given set of generics implies that the item it's
// associated with must be inlined.
fn generics_require_inlining(generics: &Generics) -> bool {
    !generics.ty_params.is_empty()
}

// Returns true if the given item must be inlined because it may be
// monomorphized or it was marked with `#[inline]`. This will only return
// true for functions.
fn item_might_be_inlined(item: @item) -> bool {
    if attributes_specify_inlining(item.attrs) {
        return true
    }

    match item.node {
        item_fn(_, _, _, ref generics, _) => {
            generics_require_inlining(generics)
        }
        _ => false,
    }
}

// Returns true if the given type method must be inlined because it may be
// monomorphized or it was marked with `#[inline]`.
fn ty_method_might_be_inlined(ty_method: &TypeMethod) -> bool {
    attributes_specify_inlining(ty_method.attrs) ||
        generics_require_inlining(&ty_method.generics)
}

// Returns true if the given trait method must be inlined because it may be
// monomorphized or it was marked with `#[inline]`.
fn trait_method_might_be_inlined(trait_method: &trait_method) -> bool {
    match *trait_method {
        required(ref ty_method) => ty_method_might_be_inlined(ty_method),
        provided(_) => true
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
    reachable_symbols: @mut HashSet<NodeId>,
    // A worklist of item IDs. Each item ID in this worklist will be inlined
    // and will be scanned for further references.
    worklist: @mut ~[NodeId],
    // Known reexports of modules
    exp_map2: resolve::ExportMap2,
}

struct MarkSymbolVisitor {
    worklist: @mut ~[NodeId],
    method_map: typeck::method_map,
    tcx: ty::ctxt,
    reachable_symbols: @mut HashSet<NodeId>,
}

impl Visitor<()> for MarkSymbolVisitor {

    fn visit_expr(&mut self, expr:@Expr, _:()) {

                match expr.node {
                    ExprPath(_) => {
                        let def = match self.tcx.def_map.find(&expr.id) {
                            Some(&def) => def,
                            None => {
                                self.tcx.sess.span_bug(expr.span,
                                                  "def ID not in def map?!")
                            }
                        };

                        let def_id = def_id_of_def(def);
                        if ReachableContext::
                                def_id_represents_local_inlined_item(self.tcx,
                                                                     def_id) {
                            self.worklist.push(def_id.node)
                        }
                        self.reachable_symbols.insert(def_id.node);
                    }
                    ExprMethodCall(*) => {
                        match self.method_map.find(&expr.id) {
                            Some(&typeck::method_map_entry {
                                origin: typeck::method_static(def_id),
                                _
                            }) => {
                                if ReachableContext::
                                    def_id_represents_local_inlined_item(
                                        self.tcx,
                                        def_id) {
                                    self.worklist.push(def_id.node)
                                }
                                self.reachable_symbols.insert(def_id.node);
                            }
                            Some(_) => {}
                            None => {
                                self.tcx.sess.span_bug(expr.span,
                                                  "method call expression \
                                                   not in method map?!")
                            }
                        }
                    }
                    _ => {}
                }

                visit::walk_expr(self, expr, ())
    }
}

impl ReachableContext {
    // Creates a new reachability computation context.
    fn new(tcx: ty::ctxt, method_map: typeck::method_map,
           exp_map2: resolve::ExportMap2) -> ReachableContext {
        ReachableContext {
            tcx: tcx,
            method_map: method_map,
            reachable_symbols: @mut HashSet::new(),
            worklist: @mut ~[],
            exp_map2: exp_map2,
        }
    }

    // Returns true if the given def ID represents a local item that is
    // eligible for inlining and false otherwise.
    fn def_id_represents_local_inlined_item(tcx: ty::ctxt, def_id: DefId)
                                            -> bool {
        if def_id.crate != LOCAL_CRATE {
            return false
        }

        let node_id = def_id.node;
        match tcx.items.find(&node_id) {
            Some(&ast_map::node_item(item, _)) => {
                match item.node {
                    item_fn(*) => item_might_be_inlined(item),
                    _ => false,
                }
            }
            Some(&ast_map::node_trait_method(trait_method, _, _)) => {
                match *trait_method {
                    required(_) => false,
                    provided(_) => true,
                }
            }
            Some(&ast_map::node_method(method, impl_did, _)) => {
                if generics_require_inlining(&method.generics) ||
                        attributes_specify_inlining(method.attrs) {
                    true
                } else {
                    // Check the impl. If the generics on the self type of the
                    // impl require inlining, this method does too.
                    assert!(impl_did.crate == LOCAL_CRATE);
                    match tcx.items.find(&impl_did.node) {
                        Some(&ast_map::node_item(item, _)) => {
                            match item.node {
                                item_impl(ref generics, _, _, _) => {
                                    generics_require_inlining(generics)
                                }
                                _ => false
                            }
                        }
                        Some(_) => {
                            tcx.sess.span_bug(method.span,
                                              "method is not inside an \
                                               impl?!")
                        }
                        None => {
                            tcx.sess.span_bug(method.span,
                                              "the impl that this method is \
                                               supposedly inside of doesn't \
                                               exist in the AST map?!")
                        }
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

    fn propagate_mod(&self, id: NodeId) {
        match self.exp_map2.find(&id) {
            Some(l) => {
                for reexport in l.iter() {
                    if reexport.reexport && is_local(reexport.def_id) {
                        self.worklist.push(reexport.def_id.node);
                    }
                }
            }
            None => {}
        }
    }

    // Step 2: Mark all symbols that the symbols on the worklist touch.
    fn propagate(&self) {
        let mut visitor = self.init_visitor();
        let mut scanned = HashSet::new();
        while self.worklist.len() > 0 {
            let search_item = self.worklist.pop();
            if scanned.contains(&search_item) {
                continue
            }
            scanned.insert(search_item);
            self.reachable_symbols.insert(search_item);

            // Find the AST block corresponding to the item and visit it,
            // marking all path expressions that resolve to something
            // interesting.
            match self.tcx.items.find(&search_item) {
                Some(&ast_map::node_item(item, _)) => {
                    match item.node {
                        item_fn(_, _, _, _, ref search_block) => {
                            visit::walk_block(&mut visitor, search_block, ())
                        }
                        // Our recursion into modules involves looking up their
                        // public reexports and the destinations of those
                        // exports. Privacy will put them in the worklist, but
                        // we won't find them in the ast_map, so this is where
                        // we deal with publicly re-exported items instead.
                        item_mod(*) => { self.propagate_mod(item.id); }
                        // These are normal, nothing reachable about these
                        // inherently and their children are already in the
                        // worklist
                        item_struct(*) | item_impl(*) | item_static(*) |
                        item_enum(*) | item_ty(*) | item_trait(*) |
                        item_foreign_mod(*) => {}
                        _ => {
                            self.tcx.sess.span_bug(item.span,
                                                   "found non-function item \
                                                    in worklist?!")
                        }
                    }
                }
                Some(&ast_map::node_trait_method(trait_method, _, _)) => {
                    match *trait_method {
                        required(*) => {
                            // Keep going, nothing to get exported
                        }
                        provided(ref method) => {
                            visit::walk_block(&mut visitor, &method.body, ())
                        }
                    }
                }
                Some(&ast_map::node_method(ref method, _, _)) => {
                    visit::walk_block(&mut visitor, &method.body, ())
                }
                // Nothing to recurse on for these
                Some(&ast_map::node_foreign_item(*)) |
                Some(&ast_map::node_variant(*)) |
                Some(&ast_map::node_struct_ctor(*)) => {}
                Some(_) => {
                    let ident_interner = token::get_ident_interner();
                    let desc = ast_map::node_id_to_str(self.tcx.items,
                                                       search_item,
                                                       ident_interner);
                    self.tcx.sess.bug(format!("found unexpected thingy in \
                                               worklist: {}",
                                               desc))
                }
                None if search_item == CRATE_NODE_ID => {
                    self.propagate_mod(search_item);
                }
                None => {
                    self.tcx.sess.bug(format!("found unmapped ID in worklist: \
                                               {}",
                                              search_item))
                }
            }
        }
    }

    // Step 3: Mark all destructors as reachable.
    //
    // XXX(pcwalton): This is a conservative overapproximation, but fixing
    // this properly would result in the necessity of computing *type*
    // reachability, which might result in a compile time loss.
    fn mark_destructors_reachable(&self) {
        for (_, destructor_def_id) in self.tcx.destructor_for_type.iter() {
            if destructor_def_id.crate == LOCAL_CRATE {
                self.reachable_symbols.insert(destructor_def_id.node);
            }
        }
    }
}

pub fn find_reachable(tcx: ty::ctxt,
                      method_map: typeck::method_map,
                      exp_map2: resolve::ExportMap2,
                      exported_items: &privacy::ExportedItems)
                      -> @mut HashSet<NodeId> {
    // XXX(pcwalton): We only need to mark symbols that are exported. But this
    // is more complicated than just looking at whether the symbol is `pub`,
    // because it might be the target of a `pub use` somewhere. For now, I
    // think we are fine, because you can't `pub use` something that wasn't
    // exported due to the bug whereby `use` only looks through public
    // modules even if you're inside the module the `use` appears in. When
    // this bug is fixed, however, this code will need to be updated. Probably
    // the easiest way to fix this (although a conservative overapproximation)
    // is to have the name resolution pass mark all targets of a `pub use` as
    // "must be reachable".

    let reachable_context = ReachableContext::new(tcx, method_map, exp_map2);

    // Step 1: Seed the worklist with all nodes which were found to be public as
    //         a result of the privacy pass
    for &id in exported_items.iter() {
        reachable_context.worklist.push(id);
    }

    // Step 2: Mark all symbols that the symbols on the worklist touch.
    reachable_context.propagate();

    // Step 3: Mark all destructors as reachable.
    reachable_context.mark_destructors_reachable();

    // Return the set of reachable symbols.
    reachable_context.reachable_symbols
}
