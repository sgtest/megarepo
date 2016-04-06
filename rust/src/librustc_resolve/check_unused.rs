// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


//
// Unused import checking
//
// Although this is mostly a lint pass, it lives in here because it depends on
// resolve data structures and because it finalises the privacy information for
// `use` directives.
//

use std::ops::{Deref, DerefMut};

use Resolver;
use Namespace::{TypeNS, ValueNS};

use rustc::lint;
use syntax::ast;
use syntax::codemap::{Span, DUMMY_SP};

use rustc::hir;
use rustc::hir::{ViewPathGlob, ViewPathList, ViewPathSimple};
use rustc::hir::intravisit::Visitor;

struct UnusedImportCheckVisitor<'a, 'b: 'a, 'tcx: 'b> {
    resolver: &'a mut Resolver<'b, 'tcx>,
}

// Deref and DerefMut impls allow treating UnusedImportCheckVisitor as Resolver.
impl<'a, 'b, 'tcx:'b> Deref for UnusedImportCheckVisitor<'a, 'b, 'tcx> {
    type Target = Resolver<'b, 'tcx>;

    fn deref<'c>(&'c self) -> &'c Resolver<'b, 'tcx> {
        &*self.resolver
    }
}

impl<'a, 'b, 'tcx:'b> DerefMut for UnusedImportCheckVisitor<'a, 'b, 'tcx> {
    fn deref_mut<'c>(&'c mut self) -> &'c mut Resolver<'b, 'tcx> {
        &mut *self.resolver
    }
}

impl<'a, 'b, 'tcx> UnusedImportCheckVisitor<'a, 'b, 'tcx> {
    // We have information about whether `use` (import) directives are actually
    // used now. If an import is not used at all, we signal a lint error.
    fn check_import(&mut self, id: ast::NodeId, span: Span) {
        if !self.used_imports.contains(&(id, TypeNS)) &&
           !self.used_imports.contains(&(id, ValueNS)) {
            self.session.add_lint(lint::builtin::UNUSED_IMPORTS,
                                  id,
                                  span,
                                  "unused import".to_string());
        }
    }
}

impl<'a, 'b, 'v, 'tcx> Visitor<'v> for UnusedImportCheckVisitor<'a, 'b, 'tcx> {
    fn visit_item(&mut self, item: &hir::Item) {
        // Ignore is_public import statements because there's no way to be sure
        // whether they're used or not. Also ignore imports with a dummy span
        // because this means that they were generated in some fashion by the
        // compiler and we don't need to consider them.
        if item.vis == hir::Public || item.span.source_equal(&DUMMY_SP) {
            return;
        }

        match item.node {
            hir::ItemExternCrate(_) => {
                if let Some(crate_num) = self.session.cstore.extern_mod_stmt_cnum(item.id) {
                    if !self.used_crates.contains(&crate_num) {
                        self.session.add_lint(lint::builtin::UNUSED_EXTERN_CRATES,
                                              item.id,
                                              item.span,
                                              "unused extern crate".to_string());
                    }
                }
            }
            hir::ItemUse(ref p) => {
                match p.node {
                    ViewPathSimple(_, _) => {
                        self.check_import(item.id, p.span)
                    }

                    ViewPathList(_, ref list) => {
                        for i in list {
                            self.check_import(i.node.id(), i.span);
                        }
                    }
                    ViewPathGlob(_) => {
                        self.check_import(item.id, p.span)
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn check_crate(resolver: &mut Resolver, krate: &hir::Crate) {
    let mut visitor = UnusedImportCheckVisitor { resolver: resolver };
    krate.visit_all_items(&mut visitor);
}
