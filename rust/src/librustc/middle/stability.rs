// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A pass that annotates every item and method with its stability level,
//! propagating default levels lexically from parent to children ast nodes.

use util::nodemap::{NodeMap, DefIdMap};
use syntax::codemap::Span;
use syntax::{attr, visit};
use syntax::ast;
use syntax::ast::{Attribute, Block, Crate, DefId, FnDecl, NodeId, Variant};
use syntax::ast::{Item, RequiredMethod, ProvidedMethod, TraitItem, TypeMethod, Method};
use syntax::ast::{Generics, StructDef, StructField, Ident};
use syntax::ast_util::is_local;
use syntax::attr::Stability;
use syntax::visit::{FnKind, FkMethod, Visitor};
use middle::ty;
use metadata::csearch;

/// A stability index, giving the stability level for items and methods.
pub struct Index {
    // stability for crate-local items; unmarked stability == no entry
    local: NodeMap<Stability>,
    // cache for extern-crate items; unmarked stability == entry with None
    extern_cache: DefIdMap<Option<Stability>>
}

// A private tree-walker for producing an Index.
struct Annotator {
    index: Index
}

impl Annotator {
    // Determine the stability for a node based on its attributes and inherited
    // stability. The stability is recorded in the index and returned.
    fn annotate(&mut self, id: NodeId, attrs: &[Attribute],
                parent: Option<Stability>) -> Option<Stability> {
        match attr::find_stability(attrs).or(parent) {
            Some(stab) => {
                self.index.local.insert(id, stab.clone());
                Some(stab)
            }
            None => None
        }
    }
}

impl Visitor<Option<Stability>> for Annotator {
    fn visit_item(&mut self, i: &Item, parent: Option<Stability>) {
        let stab = self.annotate(i.id, i.attrs.as_slice(), parent);
        visit::walk_item(self, i, stab)
    }

    fn visit_fn(&mut self, fk: &FnKind, fd: &FnDecl, b: &Block,
                s: Span, _: NodeId, parent: Option<Stability>) {
        let stab = match *fk {
            FkMethod(_, _, meth) =>
                self.annotate(meth.id, meth.attrs.as_slice(), parent),
            _ => parent
        };
        visit::walk_fn(self, fk, fd, b, s, stab)
    }

    fn visit_trait_item(&mut self, t: &TraitItem, parent: Option<Stability>) {
        let stab = match *t {
            RequiredMethod(TypeMethod {attrs: ref attrs, id: id, ..}) =>
                self.annotate(id, attrs.as_slice(), parent),

            // work around lack of pattern matching for @ types
            ProvidedMethod(method) => match *method {
                Method {attrs: ref attrs, id: id, ..} =>
                    self.annotate(id, attrs.as_slice(), parent)
            }
        };
        visit::walk_trait_item(self, t, stab)
    }

    fn visit_variant(&mut self, v: &Variant, g: &Generics, parent: Option<Stability>) {
        let stab = self.annotate(v.node.id, v.node.attrs.as_slice(), parent);
        visit::walk_variant(self, v, g, stab)
    }

    fn visit_struct_def(&mut self, s: &StructDef, _: Ident, _: &Generics,
                        _: NodeId, parent: Option<Stability>) {
        s.ctor_id.map(|id| self.annotate(id, &[], parent.clone()));
        visit::walk_struct_def(self, s, parent)
    }

    fn visit_struct_field(&mut self, s: &StructField, parent: Option<Stability>) {
        let stab = self.annotate(s.node.id, s.node.attrs.as_slice(), parent);
        visit::walk_struct_field(self, s, stab)
    }
}

impl Index {
    /// Construct the stability index for a crate being compiled.
    pub fn build(krate: &Crate) -> Index {
        let mut annotator = Annotator {
            index: Index {
                local: NodeMap::new(),
                extern_cache: DefIdMap::new()
            }
        };
        let stab = annotator.annotate(ast::CRATE_NODE_ID, krate.attrs.as_slice(), None);
        visit::walk_crate(&mut annotator, krate, stab);
        annotator.index
    }
}

/// Lookup the stability for a node, loading external crate
/// metadata as necessary.
pub fn lookup(tcx: &ty::ctxt, id: DefId) -> Option<Stability> {
    // is this definition the implementation of a trait method?
    match ty::trait_item_of_item(tcx, id) {
        Some(ty::MethodTraitItemId(trait_method_id))
                if trait_method_id != id => {
            lookup(tcx, trait_method_id)
        }
        _ if is_local(id) => {
            tcx.stability.borrow().local.find_copy(&id.node)
        }
        _ => {
            let stab = csearch::get_stability(&tcx.sess.cstore, id);
            let mut index = tcx.stability.borrow_mut();
            (*index).extern_cache.insert(id, stab.clone());
            stab
        }
    }
}
