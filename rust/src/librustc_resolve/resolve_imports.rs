// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use self::ImportDirectiveSubclass::*;

use DefModifiers;
use Module;
use ModuleKind;
use Namespace::{self, TypeNS, ValueNS};
use NameBindings;
use NamespaceResult::{BoundResult, UnboundResult, UnknownResult};
use NamespaceResult;
use NameSearchType;
use ResolveResult;
use Resolver;
use UseLexicalScopeFlag;
use {names_to_string, module_to_string};
use {resolve_error, ResolutionError};

use build_reduced_graph;

use rustc::middle::def::*;
use rustc::middle::privacy::*;

use syntax::ast::{DefId, NodeId, Name};
use syntax::attr::AttrMetaMethods;
use syntax::codemap::Span;

use std::mem::replace;
use std::rc::Rc;


/// Contains data for specific types of import directives.
#[derive(Copy, Clone,Debug)]
pub enum ImportDirectiveSubclass {
    SingleImport(Name /* target */, Name /* source */),
    GlobImport
}

/// Whether an import can be shadowed by another import.
#[derive(Debug,PartialEq,Clone,Copy)]
pub enum Shadowable {
    Always,
    Never
}

/// One import directive.
#[derive(Debug)]
pub struct ImportDirective {
    pub module_path: Vec<Name>,
    pub subclass: ImportDirectiveSubclass,
    pub span: Span,
    pub id: NodeId,
    pub is_public: bool, // see note in ImportResolution about how to use this
    pub shadowable: Shadowable,
}

impl ImportDirective {
    pub fn new(module_path: Vec<Name> ,
           subclass: ImportDirectiveSubclass,
           span: Span,
           id: NodeId,
           is_public: bool,
           shadowable: Shadowable)
           -> ImportDirective {
        ImportDirective {
            module_path: module_path,
            subclass: subclass,
            span: span,
            id: id,
            is_public: is_public,
            shadowable: shadowable,
        }
    }
}

/// The item that an import resolves to.
#[derive(Clone,Debug)]
pub struct Target {
    pub target_module: Rc<Module>,
    pub bindings: Rc<NameBindings>,
    pub shadowable: Shadowable,
}

impl Target {
    pub fn new(target_module: Rc<Module>,
           bindings: Rc<NameBindings>,
           shadowable: Shadowable)
           -> Target {
        Target {
            target_module: target_module,
            bindings: bindings,
            shadowable: shadowable,
        }
    }
}

/// An ImportResolution represents a particular `use` directive.
#[derive(Debug)]
pub struct ImportResolution {
    /// Whether this resolution came from a `use` or a `pub use`. Note that this
    /// should *not* be used whenever resolution is being performed. Privacy
    /// testing occurs during a later phase of compilation.
    pub is_public: bool,

    // The number of outstanding references to this name. When this reaches
    // zero, outside modules can count on the targets being correct. Before
    // then, all bets are off; future imports could override this name.
    // Note that this is usually either 0 or 1 - shadowing is forbidden the only
    // way outstanding_references is > 1 in a legal program is if the name is
    // used in both namespaces.
    pub outstanding_references: usize,

    /// The value that this `use` directive names, if there is one.
    pub value_target: Option<Target>,
    /// The source node of the `use` directive leading to the value target
    /// being non-none
    pub value_id: NodeId,

    /// The type that this `use` directive names, if there is one.
    pub type_target: Option<Target>,
    /// The source node of the `use` directive leading to the type target
    /// being non-none
    pub type_id: NodeId,
}

impl ImportResolution {
    pub fn new(id: NodeId, is_public: bool) -> ImportResolution {
        ImportResolution {
            type_id: id,
            value_id: id,
            outstanding_references: 0,
            value_target: None,
            type_target: None,
            is_public: is_public,
        }
    }

    pub fn target_for_namespace(&self, namespace: Namespace)
                                -> Option<Target> {
        match namespace {
            TypeNS  => self.type_target.clone(),
            ValueNS => self.value_target.clone(),
        }
    }

    pub fn id(&self, namespace: Namespace) -> NodeId {
        match namespace {
            TypeNS  => self.type_id,
            ValueNS => self.value_id,
        }
    }

    pub fn shadowable(&self, namespace: Namespace) -> Shadowable {
        let target = self.target_for_namespace(namespace);
        if target.is_none() {
            return Shadowable::Always;
        }

        target.unwrap().shadowable
    }

    pub fn set_target_and_id(&mut self,
                         namespace: Namespace,
                         target: Option<Target>,
                         id: NodeId) {
        match namespace {
            TypeNS  => {
                self.type_target = target;
                self.type_id = id;
            }
            ValueNS => {
                self.value_target = target;
                self.value_id = id;
            }
        }
    }
}


struct ImportResolver<'a, 'b:'a, 'tcx:'b> {
    resolver: &'a mut Resolver<'b, 'tcx>
}

impl<'a, 'b:'a, 'tcx:'b> ImportResolver<'a, 'b, 'tcx> {
    // Import resolution
    //
    // This is a fixed-point algorithm. We resolve imports until our efforts
    // are stymied by an unresolved import; then we bail out of the current
    // module and continue. We terminate successfully once no more imports
    // remain or unsuccessfully when no forward progress in resolving imports
    // is made.

    /// Resolves all imports for the crate. This method performs the fixed-
    /// point iteration.
    fn resolve_imports(&mut self) {
        let mut i = 0;
        let mut prev_unresolved_imports = 0;
        loop {
            debug!("(resolving imports) iteration {}, {} imports left",
                   i, self.resolver.unresolved_imports);

            let module_root = self.resolver.graph_root.get_module();
            self.resolve_imports_for_module_subtree(module_root.clone());

            if self.resolver.unresolved_imports == 0 {
                debug!("(resolving imports) success");
                break;
            }

            if self.resolver.unresolved_imports == prev_unresolved_imports {
                self.resolver.report_unresolved_imports(module_root);
                break;
            }

            i += 1;
            prev_unresolved_imports = self.resolver.unresolved_imports;
        }
    }

    /// Attempts to resolve imports for the given module and all of its
    /// submodules.
    fn resolve_imports_for_module_subtree(&mut self, module_: Rc<Module>) {
        debug!("(resolving imports for module subtree) resolving {}",
               module_to_string(&*module_));
        let orig_module = replace(&mut self.resolver.current_module, module_.clone());
        self.resolve_imports_for_module(module_.clone());
        self.resolver.current_module = orig_module;

        build_reduced_graph::populate_module_if_necessary(self.resolver, &module_);
        for (_, child_node) in module_.children.borrow().iter() {
            match child_node.get_module_if_available() {
                None => {
                    // Nothing to do.
                }
                Some(child_module) => {
                    self.resolve_imports_for_module_subtree(child_module);
                }
            }
        }

        for (_, child_module) in module_.anonymous_children.borrow().iter() {
            self.resolve_imports_for_module_subtree(child_module.clone());
        }
    }

    /// Attempts to resolve imports for the given module only.
    fn resolve_imports_for_module(&mut self, module: Rc<Module>) {
        if module.all_imports_resolved() {
            debug!("(resolving imports for module) all imports resolved for \
                   {}",
                   module_to_string(&*module));
            return;
        }

        let imports = module.imports.borrow();
        let import_count = imports.len();
        while module.resolved_import_count.get() < import_count {
            let import_index = module.resolved_import_count.get();
            let import_directive = &(*imports)[import_index];
            match self.resolve_import_for_module(module.clone(),
                                                 import_directive) {
                ResolveResult::Failed(err) => {
                    let (span, help) = match err {
                        Some((span, msg)) => (span, format!(". {}", msg)),
                        None => (import_directive.span, String::new())
                    };
                    resolve_error(self.resolver,
                                    span,
                                    ResolutionError::UnresolvedImport(
                                                Some((&*import_path_to_string(
                                                        &import_directive.module_path,
                                                        import_directive.subclass),
                                                      Some(&*help))))
                                   );
                }
                ResolveResult::Indeterminate => break, // Bail out. We'll come around next time.
                ResolveResult::Success(()) => () // Good. Continue.
            }

            module.resolved_import_count
                  .set(module.resolved_import_count.get() + 1);
        }
    }

    /// Attempts to resolve the given import. The return value indicates
    /// failure if we're certain the name does not exist, indeterminate if we
    /// don't know whether the name exists at the moment due to other
    /// currently-unresolved imports, or success if we know the name exists.
    /// If successful, the resolved bindings are written into the module.
    fn resolve_import_for_module(&mut self,
                                 module_: Rc<Module>,
                                 import_directive: &ImportDirective)
                                 -> ResolveResult<()> {
        let mut resolution_result = ResolveResult::Failed(None);
        let module_path = &import_directive.module_path;

        debug!("(resolving import for module) resolving import `{}::...` in `{}`",
               names_to_string(&module_path[..]),
               module_to_string(&*module_));

        // First, resolve the module path for the directive, if necessary.
        let container = if module_path.is_empty() {
            // Use the crate root.
            Some((self.resolver.graph_root.get_module(), LastMod(AllPublic)))
        } else {
            match self.resolver.resolve_module_path(module_.clone(),
                                                    &module_path[..],
                                                    UseLexicalScopeFlag::DontUseLexicalScope,
                                                    import_directive.span,
                                                    NameSearchType::ImportSearch) {
                ResolveResult::Failed(err) => {
                    resolution_result = ResolveResult::Failed(err);
                    None
                },
                ResolveResult::Indeterminate => {
                    resolution_result = ResolveResult::Indeterminate;
                    None
                }
                ResolveResult::Success(container) => Some(container),
            }
        };

        match container {
            None => {}
            Some((containing_module, lp)) => {
                // We found the module that the target is contained
                // within. Attempt to resolve the import within it.

                match import_directive.subclass {
                    SingleImport(target, source) => {
                        resolution_result =
                            self.resolve_single_import(&module_,
                                                       containing_module,
                                                       target,
                                                       source,
                                                       import_directive,
                                                       lp);
                    }
                    GlobImport => {
                        resolution_result =
                            self.resolve_glob_import(&module_,
                                                     containing_module,
                                                     import_directive,
                                                     lp);
                    }
                }
            }
        }

        // Decrement the count of unresolved imports.
        match resolution_result {
            ResolveResult::Success(()) => {
                assert!(self.resolver.unresolved_imports >= 1);
                self.resolver.unresolved_imports -= 1;
            }
            _ => {
                // Nothing to do here; just return the error.
            }
        }

        // Decrement the count of unresolved globs if necessary. But only if
        // the resolution result is indeterminate -- otherwise we'll stop
        // processing imports here. (See the loop in
        // resolve_imports_for_module).

        if !resolution_result.indeterminate() {
            match import_directive.subclass {
                GlobImport => {
                    assert!(module_.glob_count.get() >= 1);
                    module_.glob_count.set(module_.glob_count.get() - 1);
                }
                SingleImport(..) => {
                    // Ignore.
                }
            }
        }

        return resolution_result;
    }

    fn resolve_single_import(&mut self,
                             module_: &Module,
                             target_module: Rc<Module>,
                             target: Name,
                             source: Name,
                             directive: &ImportDirective,
                             lp: LastPrivate)
                             -> ResolveResult<()> {
        debug!("(resolving single import) resolving `{}` = `{}::{}` from \
                `{}` id {}, last private {:?}",
               target,
               module_to_string(&*target_module),
               source,
               module_to_string(module_),
               directive.id,
               lp);

        let lp = match lp {
            LastMod(lp) => lp,
            LastImport {..} => {
                self.resolver.session
                    .span_bug(directive.span,
                              "not expecting Import here, must be LastMod")
            }
        };

        // We need to resolve both namespaces for this to succeed.
        //

        let mut value_result = UnknownResult;
        let mut type_result = UnknownResult;

        // Search for direct children of the containing module.
        build_reduced_graph::populate_module_if_necessary(self.resolver, &target_module);

        match target_module.children.borrow().get(&source) {
            None => {
                // Continue.
            }
            Some(ref child_name_bindings) => {
                // pub_err makes sure we don't give the same error twice.
                let mut pub_err = false;
                if child_name_bindings.defined_in_namespace(ValueNS) {
                    debug!("(resolving single import) found value binding");
                    value_result = BoundResult(target_module.clone(),
                                               (*child_name_bindings).clone());
                    if directive.is_public && !child_name_bindings.is_public(ValueNS) {
                        let msg = format!("`{}` is private, and cannot be reexported",
                                          source);
                        let note_msg =
                            format!("Consider marking `{}` as `pub` in the imported module",
                                    source);
                        span_err!(self.resolver.session, directive.span, E0364, "{}", &msg);
                        self.resolver.session.span_note(directive.span, &note_msg);
                        pub_err = true;
                    }
                }
                if child_name_bindings.defined_in_namespace(TypeNS) {
                    debug!("(resolving single import) found type binding");
                    type_result = BoundResult(target_module.clone(),
                                              (*child_name_bindings).clone());
                    if !pub_err && directive.is_public && !child_name_bindings.is_public(TypeNS) {
                        let msg = format!("`{}` is private, and cannot be reexported",
                                          source);
                        let note_msg = format!("Consider declaring module `{}` as a `pub mod`",
                                               source);
                        span_err!(self.resolver.session, directive.span, E0365, "{}", &msg);
                        self.resolver.session.span_note(directive.span, &note_msg);
                    }
                }
            }
        }

        // Unless we managed to find a result in both namespaces (unlikely),
        // search imports as well.
        let mut value_used_reexport = false;
        let mut type_used_reexport = false;
        match (value_result.clone(), type_result.clone()) {
            (BoundResult(..), BoundResult(..)) => {} // Continue.
            _ => {
                // If there is an unresolved glob at this point in the
                // containing module, bail out. We don't know enough to be
                // able to resolve this import.

                if target_module.glob_count.get() > 0 {
                    debug!("(resolving single import) unresolved glob; \
                            bailing out");
                    return ResolveResult::Indeterminate;
                }

                // Now search the exported imports within the containing module.
                match target_module.import_resolutions.borrow().get(&source) {
                    None => {
                        debug!("(resolving single import) no import");
                        // The containing module definitely doesn't have an
                        // exported import with the name in question. We can
                        // therefore accurately report that the names are
                        // unbound.

                        if value_result.is_unknown() {
                            value_result = UnboundResult;
                        }
                        if type_result.is_unknown() {
                            type_result = UnboundResult;
                        }
                    }
                    Some(import_resolution)
                            if import_resolution.outstanding_references == 0 => {

                        fn get_binding(this: &mut Resolver,
                                       import_resolution: &ImportResolution,
                                       namespace: Namespace,
                                       source: &Name)
                                    -> NamespaceResult {

                            // Import resolutions must be declared with "pub"
                            // in order to be exported.
                            if !import_resolution.is_public {
                                return UnboundResult;
                            }

                            match import_resolution.target_for_namespace(namespace) {
                                None => {
                                    return UnboundResult;
                                }
                                Some(Target {
                                    target_module,
                                    bindings,
                                    shadowable: _
                                }) => {
                                    debug!("(resolving single import) found \
                                            import in ns {:?}", namespace);
                                    let id = import_resolution.id(namespace);
                                    // track used imports and extern crates as well
                                    this.used_imports.insert((id, namespace));
                                    this.record_import_use(id, *source);
                                    match target_module.def_id.get() {
                                        Some(DefId{krate: kid, ..}) => {
                                            this.used_crates.insert(kid);
                                        },
                                        _ => {}
                                    }
                                    return BoundResult(target_module, bindings);
                                }
                            }
                        }

                        // The name is an import which has been fully
                        // resolved. We can, therefore, just follow it.
                        if value_result.is_unknown() {
                            value_result = get_binding(self.resolver,
                                                       import_resolution,
                                                       ValueNS,
                                                       &source);
                            value_used_reexport = import_resolution.is_public;
                        }
                        if type_result.is_unknown() {
                            type_result = get_binding(self.resolver,
                                                      import_resolution,
                                                      TypeNS,
                                                      &source);
                            type_used_reexport = import_resolution.is_public;
                        }

                    }
                    Some(_) => {
                        // If target_module is the same module whose import we are resolving
                        // and there it has an unresolved import with the same name as `source`,
                        // then the user is actually trying to import an item that is declared
                        // in the same scope
                        //
                        // e.g
                        // use self::submodule;
                        // pub mod submodule;
                        //
                        // In this case we continue as if we resolved the import and let the
                        // check_for_conflicts_between_imports_and_items call below handle
                        // the conflict
                        match (module_.def_id.get(),  target_module.def_id.get()) {
                            (Some(id1), Some(id2)) if id1 == id2  => {
                                if value_result.is_unknown() {
                                    value_result = UnboundResult;
                                }
                                if type_result.is_unknown() {
                                    type_result = UnboundResult;
                                }
                            }
                            _ =>  {
                                // The import is unresolved. Bail out.
                                debug!("(resolving single import) unresolved import; \
                                        bailing out");
                                return ResolveResult::Indeterminate;
                            }
                        }
                    }
                }
            }
        }

        let mut value_used_public = false;
        let mut type_used_public = false;

        // If we didn't find a result in the type namespace, search the
        // external modules.
        match type_result {
            BoundResult(..) => {}
            _ => {
                match target_module.external_module_children.borrow_mut().get(&source).cloned() {
                    None => {} // Continue.
                    Some(module) => {
                        debug!("(resolving single import) found external module");
                        // track the module as used.
                        match module.def_id.get() {
                            Some(DefId{krate: kid, ..}) => {
                                self.resolver.used_crates.insert(kid);
                            }
                            _ => {}
                        }
                        let name_bindings =
                            Rc::new(Resolver::create_name_bindings_from_module(module));
                        type_result = BoundResult(target_module.clone(), name_bindings);
                        type_used_public = true;
                    }
                }
            }
        }

        // We've successfully resolved the import. Write the results in.
        let mut import_resolutions = module_.import_resolutions.borrow_mut();
        let import_resolution = import_resolutions.get_mut(&target).unwrap();

        {
            let mut check_and_write_import = |namespace, result: &_, used_public: &mut bool| {
                let namespace_name = match namespace {
                    TypeNS => "type",
                    ValueNS => "value",
                };

                match *result {
                    BoundResult(ref target_module, ref name_bindings) => {
                        debug!("(resolving single import) found {:?} target: {:?}",
                               namespace_name,
                               name_bindings.def_for_namespace(namespace));
                        self.check_for_conflicting_import(
                            &import_resolution,
                            directive.span,
                            target,
                            namespace);

                        self.check_that_import_is_importable(
                            &**name_bindings,
                            directive.span,
                            target,
                            namespace);

                        let target = Some(Target::new(target_module.clone(),
                                                      name_bindings.clone(),
                                                      directive.shadowable));
                        import_resolution.set_target_and_id(namespace, target, directive.id);
                        import_resolution.is_public = directive.is_public;
                        *used_public = name_bindings.defined_in_public_namespace(namespace);
                    }
                    UnboundResult => { /* Continue. */ }
                    UnknownResult => {
                        panic!("{:?} result should be known at this point", namespace_name);
                    }
                }
            };
            check_and_write_import(ValueNS, &value_result, &mut value_used_public);
            check_and_write_import(TypeNS, &type_result, &mut type_used_public);
        }

        self.check_for_conflicts_between_imports_and_items(
            module_,
            import_resolution,
            directive.span,
            target);

        if value_result.is_unbound() && type_result.is_unbound() {
            let msg = format!("There is no `{}` in `{}`",
                              source,
                              module_to_string(&target_module));
            return ResolveResult::Failed(Some((directive.span, msg)));
        }
        let value_used_public = value_used_reexport || value_used_public;
        let type_used_public = type_used_reexport || type_used_public;

        assert!(import_resolution.outstanding_references >= 1);
        import_resolution.outstanding_references -= 1;

        // Record what this import resolves to for later uses in documentation,
        // this may resolve to either a value or a type, but for documentation
        // purposes it's good enough to just favor one over the other.
        let value_def_and_priv = import_resolution.value_target.as_ref().map(|target| {
            let def = target.bindings.def_for_namespace(ValueNS).unwrap();
            (def, if value_used_public { lp } else { DependsOn(def.def_id()) })
        });
        let type_def_and_priv = import_resolution.type_target.as_ref().map(|target| {
            let def = target.bindings.def_for_namespace(TypeNS).unwrap();
            (def, if type_used_public { lp } else { DependsOn(def.def_id()) })
        });

        let import_lp = LastImport {
            value_priv: value_def_and_priv.map(|(_, p)| p),
            value_used: Used,
            type_priv: type_def_and_priv.map(|(_, p)| p),
            type_used: Used
        };

        if let Some((def, _)) = value_def_and_priv {
            self.resolver.def_map.borrow_mut().insert(directive.id, PathResolution {
                base_def: def,
                last_private: import_lp,
                depth: 0
            });
        }
        if let Some((def, _)) = type_def_and_priv {
            self.resolver.def_map.borrow_mut().insert(directive.id, PathResolution {
                base_def: def,
                last_private: import_lp,
                depth: 0
            });
        }

        debug!("(resolving single import) successfully resolved import");
        return ResolveResult::Success(());
    }

    // Resolves a glob import. Note that this function cannot fail; it either
    // succeeds or bails out (as importing * from an empty module or a module
    // that exports nothing is valid). target_module is the module we are
    // actually importing, i.e., `foo` in `use foo::*`.
    fn resolve_glob_import(&mut self,
                           module_: &Module,
                           target_module: Rc<Module>,
                           import_directive: &ImportDirective,
                           lp: LastPrivate)
                           -> ResolveResult<()> {
        let id = import_directive.id;
        let is_public = import_directive.is_public;

        // This function works in a highly imperative manner; it eagerly adds
        // everything it can to the list of import resolutions of the module
        // node.
        debug!("(resolving glob import) resolving glob import {}", id);

        // We must bail out if the node has unresolved imports of any kind
        // (including globs).
        if !(*target_module).all_imports_resolved() {
            debug!("(resolving glob import) target module has unresolved \
                    imports; bailing out");
            return ResolveResult::Indeterminate;
        }

        assert_eq!(target_module.glob_count.get(), 0);

        // Add all resolved imports from the containing module.
        let import_resolutions = target_module.import_resolutions.borrow();
        for (ident, target_import_resolution) in import_resolutions.iter() {
            debug!("(resolving glob import) writing module resolution \
                    {} into `{}`",
                   *ident,
                   module_to_string(module_));

            if !target_import_resolution.is_public {
                debug!("(resolving glob import) nevermind, just kidding");
                continue
            }

            // Here we merge two import resolutions.
            let mut import_resolutions = module_.import_resolutions.borrow_mut();
            match import_resolutions.get_mut(ident) {
                Some(dest_import_resolution) => {
                    // Merge the two import resolutions at a finer-grained
                    // level.

                    match target_import_resolution.value_target {
                        None => {
                            // Continue.
                        }
                        Some(ref value_target) => {
                            self.check_for_conflicting_import(&dest_import_resolution,
                                                              import_directive.span,
                                                              *ident,
                                                              ValueNS);
                            dest_import_resolution.value_target = Some(value_target.clone());
                        }
                    }
                    match target_import_resolution.type_target {
                        None => {
                            // Continue.
                        }
                        Some(ref type_target) => {
                            self.check_for_conflicting_import(&dest_import_resolution,
                                                              import_directive.span,
                                                              *ident,
                                                              TypeNS);
                            dest_import_resolution.type_target = Some(type_target.clone());
                        }
                    }
                    dest_import_resolution.is_public = is_public;
                    continue;
                }
                None => {}
            }

            // Simple: just copy the old import resolution.
            let mut new_import_resolution = ImportResolution::new(id, is_public);
            new_import_resolution.value_target =
                target_import_resolution.value_target.clone();
            new_import_resolution.type_target =
                target_import_resolution.type_target.clone();

            import_resolutions.insert(*ident, new_import_resolution);
        }

        // Add all children from the containing module.
        build_reduced_graph::populate_module_if_necessary(self.resolver, &target_module);

        for (&name, name_bindings) in target_module.children.borrow().iter() {
            self.merge_import_resolution(module_,
                                         target_module.clone(),
                                         import_directive,
                                         name,
                                         name_bindings.clone());

        }

        // Add external module children from the containing module.
        for (&name, module) in target_module.external_module_children.borrow().iter() {
            let name_bindings =
                Rc::new(Resolver::create_name_bindings_from_module(module.clone()));
            self.merge_import_resolution(module_,
                                         target_module.clone(),
                                         import_directive,
                                         name,
                                         name_bindings);
        }

        // Record the destination of this import
        if let Some(did) = target_module.def_id.get() {
            self.resolver.def_map.borrow_mut().insert(id, PathResolution {
                base_def: DefMod(did),
                last_private: lp,
                depth: 0
            });
        }

        debug!("(resolving glob import) successfully resolved import");
        return ResolveResult::Success(());
    }

    fn merge_import_resolution(&mut self,
                               module_: &Module,
                               containing_module: Rc<Module>,
                               import_directive: &ImportDirective,
                               name: Name,
                               name_bindings: Rc<NameBindings>) {
        let id = import_directive.id;
        let is_public = import_directive.is_public;

        let mut import_resolutions = module_.import_resolutions.borrow_mut();
        let dest_import_resolution = import_resolutions.entry(name)
            .or_insert_with(|| ImportResolution::new(id, is_public));

        debug!("(resolving glob import) writing resolution `{}` in `{}` \
               to `{}`",
               name,
               module_to_string(&*containing_module),
               module_to_string(module_));

        // Merge the child item into the import resolution.
        {
            let mut merge_child_item = |namespace| {
                let modifier = DefModifiers::IMPORTABLE | DefModifiers::PUBLIC;

                if name_bindings.defined_in_namespace_with(namespace, modifier) {
                    let namespace_name = match namespace {
                        TypeNS => "type",
                        ValueNS => "value",
                    };
                    debug!("(resolving glob import) ... for {} target", namespace_name);
                    if dest_import_resolution.shadowable(namespace) == Shadowable::Never {
                        let msg = format!("a {} named `{}` has already been imported \
                                           in this module",
                                          namespace_name,
                                          name);
                        span_err!(self.resolver.session, import_directive.span, E0251, "{}", msg);
                    } else {
                        let target = Target::new(containing_module.clone(),
                                                 name_bindings.clone(),
                                                 import_directive.shadowable);
                        dest_import_resolution.set_target_and_id(namespace,
                                                                 Some(target),
                                                                 id);
                    }
                }
            };
            merge_child_item(ValueNS);
            merge_child_item(TypeNS);
        }

        dest_import_resolution.is_public = is_public;

        self.check_for_conflicts_between_imports_and_items(
            module_,
            dest_import_resolution,
            import_directive.span,
            name);
    }

    /// Checks that imported names and items don't have the same name.
    fn check_for_conflicting_import(&mut self,
                                    import_resolution: &ImportResolution,
                                    import_span: Span,
                                    name: Name,
                                    namespace: Namespace) {
        let target = import_resolution.target_for_namespace(namespace);
        debug!("check_for_conflicting_import: {}; target exists: {}",
               name,
               target.is_some());

        match target {
            Some(ref target) if target.shadowable != Shadowable::Always => {
                let ns_word = match namespace {
                    TypeNS => {
                        if let Some(ref ty_def) = *target.bindings.type_def.borrow() {
                            match ty_def.module_def {
                                Some(ref module)
                                    if module.kind.get() == ModuleKind::NormalModuleKind =>
                                        "module",
                                Some(ref module)
                                    if module.kind.get() == ModuleKind::TraitModuleKind =>
                                        "trait",
                                _ => "type",
                            }
                        } else { "type" }
                    },
                    ValueNS => "value",
                };
                span_err!(self.resolver.session, import_span, E0252,
                          "a {} named `{}` has already been imported \
                           in this module", ns_word,
                                  name);
                let use_id = import_resolution.id(namespace);
                let item = self.resolver.ast_map.expect_item(use_id);
                // item is syntax::ast::Item;
                span_note!(self.resolver.session, item.span,
                            "previous import of `{}` here",
                            name);
            }
            Some(_) | None => {}
        }
    }

    /// Checks that an import is actually importable
    fn check_that_import_is_importable(&mut self,
                                       name_bindings: &NameBindings,
                                       import_span: Span,
                                       name: Name,
                                       namespace: Namespace) {
        if !name_bindings.defined_in_namespace_with(namespace, DefModifiers::IMPORTABLE) {
            let msg = format!("`{}` is not directly importable",
                              name);
            span_err!(self.resolver.session, import_span, E0253, "{}", &msg[..]);
        }
    }

    /// Checks that imported names and items don't have the same name.
    fn check_for_conflicts_between_imports_and_items(&mut self,
                                                     module: &Module,
                                                     import_resolution:
                                                     &ImportResolution,
                                                     import_span: Span,
                                                     name: Name) {
        // First, check for conflicts between imports and `extern crate`s.
        if module.external_module_children
                 .borrow()
                 .contains_key(&name) {
            match import_resolution.type_target {
                Some(ref target) if target.shadowable != Shadowable::Always => {
                    let msg = format!("import `{0}` conflicts with imported \
                                       crate in this module \
                                       (maybe you meant `use {0}::*`?)",
                                      name);
                    span_err!(self.resolver.session, import_span, E0254, "{}", &msg[..]);
                }
                Some(_) | None => {}
            }
        }

        // Check for item conflicts.
        let children = module.children.borrow();
        let name_bindings = match children.get(&name) {
            None => {
                // There can't be any conflicts.
                return
            }
            Some(ref name_bindings) => (*name_bindings).clone(),
        };

        match import_resolution.value_target {
            Some(ref target) if target.shadowable != Shadowable::Always => {
                if let Some(ref value) = *name_bindings.value_def.borrow() {
                    span_err!(self.resolver.session, import_span, E0255,
                              "import `{}` conflicts with value in this module",
                              name);
                    if let Some(span) = value.value_span {
                        self.resolver.session.span_note(span, "conflicting value here");
                    }
                }
            }
            Some(_) | None => {}
        }

        match import_resolution.type_target {
            Some(ref target) if target.shadowable != Shadowable::Always => {
                if let Some(ref ty) = *name_bindings.type_def.borrow() {
                    let (what, note) = match ty.module_def {
                        Some(ref module)
                            if module.kind.get() == ModuleKind::NormalModuleKind =>
                                ("existing submodule", "note conflicting module here"),
                        Some(ref module)
                            if module.kind.get() == ModuleKind::TraitModuleKind =>
                                ("trait in this module", "note conflicting trait here"),
                        _    => ("type in this module", "note conflicting type here"),
                    };
                    span_err!(self.resolver.session, import_span, E0256,
                              "import `{}` conflicts with {}",
                              name, what);
                    if let Some(span) = ty.type_span {
                        self.resolver.session.span_note(span, note);
                    }
                }
            }
            Some(_) | None => {}
        }
    }
}

fn import_path_to_string(names: &[Name],
                         subclass: ImportDirectiveSubclass)
                         -> String {
    if names.is_empty() {
        import_directive_subclass_to_string(subclass)
    } else {
        (format!("{}::{}",
                 names_to_string(names),
                 import_directive_subclass_to_string(subclass))).to_string()
    }
}

fn import_directive_subclass_to_string(subclass: ImportDirectiveSubclass) -> String {
    match subclass {
        SingleImport(_, source) => source.to_string(),
        GlobImport => "*".to_string()
    }
}

pub fn resolve_imports(resolver: &mut Resolver) {
    let mut import_resolver = ImportResolver {
        resolver: resolver,
    };
    import_resolver.resolve_imports();
}
