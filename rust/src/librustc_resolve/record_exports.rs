// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


// Export recording
//
// This pass simply determines what all "export" keywords refer to and
// writes the results into the export map.
//
// FIXME #4953 This pass will be removed once exports change to per-item.
// Then this operation can simply be performed as part of item (or import)
// processing.

use {Module, NameBindings, Resolver};
use Namespace::{mod, TypeNS, ValueNS};

use rustc::middle::def::Export;
use syntax::ast;
use syntax::parse::token;

use std::rc::Rc;

struct ExportRecorder<'a, 'b:'a, 'tcx:'b> {
    resolver: &'a mut Resolver<'b, 'tcx>
}

// Deref and DerefMut impls allow treating ExportRecorder as Resolver.
impl<'a, 'b, 'tcx:'b> Deref<Resolver<'b, 'tcx>> for ExportRecorder<'a, 'b, 'tcx> {
    fn deref<'c>(&'c self) -> &'c Resolver<'b, 'tcx> {
        &*self.resolver
    }
}

impl<'a, 'b, 'tcx:'b> DerefMut<Resolver<'b, 'tcx>> for ExportRecorder<'a, 'b, 'tcx> {
    fn deref_mut<'c>(&'c mut self) -> &'c mut Resolver<'b, 'tcx> {
        &mut *self.resolver
    }
}

impl<'a, 'b, 'tcx> ExportRecorder<'a, 'b, 'tcx> {
    fn record_exports_for_module_subtree(&mut self,
                                         module_: Rc<Module>) {
        // If this isn't a local krate, then bail out. We don't need to record
        // exports for nonlocal crates.

        match module_.def_id.get() {
            Some(def_id) if def_id.krate == ast::LOCAL_CRATE => {
                // OK. Continue.
                debug!("(recording exports for module subtree) recording \
                        exports for local module `{}`",
                       self.module_to_string(&*module_));
            }
            None => {
                // Record exports for the root module.
                debug!("(recording exports for module subtree) recording \
                        exports for root module `{}`",
                       self.module_to_string(&*module_));
            }
            Some(_) => {
                // Bail out.
                debug!("(recording exports for module subtree) not recording \
                        exports for `{}`",
                       self.module_to_string(&*module_));
                return;
            }
        }

        self.record_exports_for_module(&*module_);
        self.populate_module_if_necessary(&module_);

        for (_, child_name_bindings) in module_.children.borrow().iter() {
            match child_name_bindings.get_module_if_available() {
                None => {
                    // Nothing to do.
                }
                Some(child_module) => {
                    self.record_exports_for_module_subtree(child_module);
                }
            }
        }

        for (_, child_module) in module_.anonymous_children.borrow().iter() {
            self.record_exports_for_module_subtree(child_module.clone());
        }
    }

    fn record_exports_for_module(&mut self, module_: &Module) {
        let mut exports = Vec::new();

        self.add_exports_for_module(&mut exports, module_);
        match module_.def_id.get() {
            Some(def_id) => {
                self.export_map.insert(def_id.node, exports);
                debug!("(computing exports) writing exports for {} (some)",
                       def_id.node);
            }
            None => {}
        }
    }

    fn add_exports_of_namebindings(&mut self,
                                   exports: &mut Vec<Export>,
                                   name: ast::Name,
                                   namebindings: &NameBindings,
                                   ns: Namespace) {
        match namebindings.def_for_namespace(ns) {
            Some(d) => {
                debug!("(computing exports) YES: export '{}' => {}",
                       name, d.def_id());
                exports.push(Export {
                    name: name,
                    def_id: d.def_id()
                });
            }
            d_opt => {
                debug!("(computing exports) NO: {}", d_opt);
            }
        }
    }

    fn add_exports_for_module(&mut self,
                              exports: &mut Vec<Export>,
                              module_: &Module) {
        for (name, importresolution) in module_.import_resolutions.borrow().iter() {
            if !importresolution.is_public {
                continue
            }
            let xs = [TypeNS, ValueNS];
            for &ns in xs.iter() {
                match importresolution.target_for_namespace(ns) {
                    Some(target) => {
                        debug!("(computing exports) maybe export '{}'",
                               token::get_name(*name));
                        self.add_exports_of_namebindings(exports,
                                                         *name,
                                                         &*target.bindings,
                                                         ns)
                    }
                    _ => ()
                }
            }
        }
    }
}

pub fn record(resolver: &mut Resolver) {
    let mut recorder = ExportRecorder { resolver: resolver };
    let root_module = recorder.graph_root.get_module();
    recorder.record_exports_for_module_subtree(root_module);
}
