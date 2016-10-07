// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Validates all used crates and extern libraries and loads their metadata

use cstore::{self, CStore, CrateSource, MetadataBlob};
use loader::{self, CratePaths};
use macro_import;
use schema::CrateRoot;

use rustc::hir::def_id::{CrateNum, DefIndex};
use rustc::hir::svh::Svh;
use rustc::middle::cstore::LoadedMacro;
use rustc::session::{config, Session};
use rustc_back::PanicStrategy;
use rustc::session::search_paths::PathKind;
use rustc::middle;
use rustc::middle::cstore::{CrateStore, validate_crate_name, ExternCrate};
use rustc::util::nodemap::{FnvHashMap, FnvHashSet};
use rustc::hir::map as hir_map;

use std::cell::{RefCell, Cell};
use std::ops::Deref;
use std::path::PathBuf;
use std::rc::Rc;
use std::fs;

use syntax::ast;
use syntax::abi::Abi;
use syntax::parse;
use syntax::attr;
use syntax::parse::token::InternedString;
use syntax_pos::{self, Span, mk_sp};
use log;

pub struct CrateLoader<'a> {
    pub sess: &'a Session,
    pub creader: CrateReader<'a>,
    cstore: &'a CStore,
}

pub struct CrateReader<'a> {
    sess: &'a Session,
    cstore: &'a CStore,
    next_crate_num: CrateNum,
    foreign_item_map: FnvHashMap<String, Vec<ast::NodeId>>,
    local_crate_name: String,
    local_crate_config: ast::CrateConfig,
}

fn dump_crates(cstore: &CStore) {
    info!("resolved crates:");
    cstore.iter_crate_data_origins(|_, data, opt_source| {
        info!("  name: {}", data.name());
        info!("  cnum: {}", data.cnum);
        info!("  hash: {}", data.hash());
        info!("  reqd: {}", data.explicitly_linked.get());
        opt_source.map(|cs| {
            let CrateSource { dylib, rlib, cnum: _ } = cs;
            dylib.map(|dl| info!("  dylib: {}", dl.0.display()));
            rlib.map(|rl|  info!("   rlib: {}", rl.0.display()));
        });
    })
}

fn should_link(i: &ast::Item) -> bool {
    !attr::contains_name(&i.attrs, "no_link")
}

#[derive(Debug)]
struct ExternCrateInfo {
    ident: String,
    name: String,
    id: ast::NodeId,
    should_link: bool,
}

fn register_native_lib(sess: &Session,
                       cstore: &CStore,
                       span: Option<Span>,
                       name: String,
                       kind: cstore::NativeLibraryKind) {
    if name.is_empty() {
        match span {
            Some(span) => {
                struct_span_err!(sess, span, E0454,
                                 "#[link(name = \"\")] given with empty name")
                    .span_label(span, &format!("empty name given"))
                    .emit();
            }
            None => {
                sess.err("empty library name given via `-l`");
            }
        }
        return
    }
    let is_osx = sess.target.target.options.is_like_osx;
    if kind == cstore::NativeFramework && !is_osx {
        let msg = "native frameworks are only available on OSX targets";
        match span {
            Some(span) => {
                span_err!(sess, span, E0455,
                          "{}", msg)
            }
            None => sess.err(msg),
        }
    }
    cstore.add_used_library(name, kind);
}

// Extra info about a crate loaded for plugins or exported macros.
struct ExtensionCrate {
    metadata: PMDSource,
    dylib: Option<PathBuf>,
    target_only: bool,

    ident: String,
    name: String,
    span: Span,
    should_link: bool,
}

enum PMDSource {
    Registered(Rc<cstore::CrateMetadata>),
    Owned(loader::Library),
}

impl Deref for PMDSource {
    type Target = MetadataBlob;

    fn deref(&self) -> &MetadataBlob {
        match *self {
            PMDSource::Registered(ref cmd) => &cmd.blob,
            PMDSource::Owned(ref lib) => &lib.metadata
        }
    }
}

enum LoadResult {
    Previous(CrateNum),
    Loaded(loader::Library),
}

pub struct Macros {
    pub macro_rules: Vec<ast::MacroDef>,

    /// An array of pairs where the first element is the name of the custom
    /// derive (e.g. the trait being derived) and the second element is the
    /// index of the definition.
    pub custom_derive_registrar: Option<DefIndex>,
    pub svh: Svh,
    pub dylib: Option<PathBuf>,
}

impl<'a> CrateReader<'a> {
    pub fn new(sess: &'a Session,
               cstore: &'a CStore,
               local_crate_name: &str,
               local_crate_config: ast::CrateConfig)
               -> CrateReader<'a> {
        CrateReader {
            sess: sess,
            cstore: cstore,
            next_crate_num: cstore.next_crate_num(),
            foreign_item_map: FnvHashMap(),
            local_crate_name: local_crate_name.to_owned(),
            local_crate_config: local_crate_config,
        }
    }

    fn extract_crate_info(&self, i: &ast::Item) -> Option<ExternCrateInfo> {
        match i.node {
            ast::ItemKind::ExternCrate(ref path_opt) => {
                debug!("resolving extern crate stmt. ident: {} path_opt: {:?}",
                       i.ident, path_opt);
                let name = match *path_opt {
                    Some(name) => {
                        validate_crate_name(Some(self.sess), &name.as_str(),
                                            Some(i.span));
                        name.to_string()
                    }
                    None => i.ident.to_string(),
                };
                Some(ExternCrateInfo {
                    ident: i.ident.to_string(),
                    name: name,
                    id: i.id,
                    should_link: should_link(i),
                })
            }
            _ => None
        }
    }

    fn existing_match(&self, name: &str, hash: Option<&Svh>, kind: PathKind)
                      -> Option<CrateNum> {
        let mut ret = None;
        self.cstore.iter_crate_data(|cnum, data| {
            if data.name != name { return }

            match hash {
                Some(hash) if *hash == data.hash() => { ret = Some(cnum); return }
                Some(..) => return,
                None => {}
            }

            // When the hash is None we're dealing with a top-level dependency
            // in which case we may have a specification on the command line for
            // this library. Even though an upstream library may have loaded
            // something of the same name, we have to make sure it was loaded
            // from the exact same location as well.
            //
            // We're also sure to compare *paths*, not actual byte slices. The
            // `source` stores paths which are normalized which may be different
            // from the strings on the command line.
            let source = self.cstore.used_crate_source(cnum);
            if let Some(locs) = self.sess.opts.externs.get(name) {
                let found = locs.iter().any(|l| {
                    let l = fs::canonicalize(l).ok();
                    source.dylib.as_ref().map(|p| &p.0) == l.as_ref() ||
                    source.rlib.as_ref().map(|p| &p.0) == l.as_ref()
                });
                if found {
                    ret = Some(cnum);
                }
                return
            }

            // Alright, so we've gotten this far which means that `data` has the
            // right name, we don't have a hash, and we don't have a --extern
            // pointing for ourselves. We're still not quite yet done because we
            // have to make sure that this crate was found in the crate lookup
            // path (this is a top-level dependency) as we don't want to
            // implicitly load anything inside the dependency lookup path.
            let prev_kind = source.dylib.as_ref().or(source.rlib.as_ref())
                                  .unwrap().1;
            if ret.is_none() && (prev_kind == kind || prev_kind == PathKind::All) {
                ret = Some(cnum);
            }
        });
        return ret;
    }

    fn verify_no_symbol_conflicts(&self,
                                  span: Span,
                                  root: &CrateRoot) {
        // Check for (potential) conflicts with the local crate
        if self.local_crate_name == root.name &&
           self.sess.local_crate_disambiguator() == &root.disambiguator[..] {
            span_fatal!(self.sess, span, E0519,
                        "the current crate is indistinguishable from one of its \
                         dependencies: it has the same crate-name `{}` and was \
                         compiled with the same `-C metadata` arguments. This \
                         will result in symbol conflicts between the two.",
                        root.name)
        }

        // Check for conflicts with any crate loaded so far
        self.cstore.iter_crate_data(|_, other| {
            if other.name() == root.name && // same crate-name
               other.disambiguator() == root.disambiguator &&  // same crate-disambiguator
               other.hash() != root.hash { // but different SVH
                span_fatal!(self.sess, span, E0523,
                        "found two different crates with name `{}` that are \
                         not distinguished by differing `-C metadata`. This \
                         will result in symbol conflicts between the two.",
                        root.name)
            }
        });
    }

    fn register_crate(&mut self,
                      root: &Option<CratePaths>,
                      ident: &str,
                      name: &str,
                      span: Span,
                      lib: loader::Library,
                      explicitly_linked: bool)
                      -> (CrateNum, Rc<cstore::CrateMetadata>,
                          cstore::CrateSource) {
        info!("register crate `extern crate {} as {}`", name, ident);
        let crate_root = lib.metadata.get_root();
        self.verify_no_symbol_conflicts(span, &crate_root);

        // Claim this crate number and cache it
        let cnum = self.next_crate_num;
        self.next_crate_num = CrateNum::from_u32(cnum.as_u32() + 1);

        // Stash paths for top-most crate locally if necessary.
        let crate_paths = if root.is_none() {
            Some(CratePaths {
                ident: ident.to_string(),
                dylib: lib.dylib.clone().map(|p| p.0),
                rlib:  lib.rlib.clone().map(|p| p.0),
            })
        } else {
            None
        };
        // Maintain a reference to the top most crate.
        let root = if root.is_some() { root } else { &crate_paths };

        let loader::Library { dylib, rlib, metadata } = lib;

        let cnum_map = self.resolve_crate_deps(root, &crate_root, &metadata, cnum, span);

        if crate_root.macro_derive_registrar.is_some() {
            self.sess.span_err(span, "crates of the `proc-macro` crate type \
                                      cannot be linked at runtime");
        }

        let cmeta = Rc::new(cstore::CrateMetadata {
            name: name.to_string(),
            extern_crate: Cell::new(None),
            key_map: metadata.load_key_map(crate_root.index),
            root: crate_root,
            blob: metadata,
            cnum_map: RefCell::new(cnum_map),
            cnum: cnum,
            codemap_import_info: RefCell::new(vec![]),
            explicitly_linked: Cell::new(explicitly_linked),
        });

        let source = cstore::CrateSource {
            dylib: dylib,
            rlib: rlib,
            cnum: cnum,
        };

        self.cstore.set_crate_data(cnum, cmeta.clone());
        self.cstore.add_used_crate_source(source.clone());
        (cnum, cmeta, source)
    }

    fn resolve_crate(&mut self,
                     root: &Option<CratePaths>,
                     ident: &str,
                     name: &str,
                     hash: Option<&Svh>,
                     span: Span,
                     kind: PathKind,
                     explicitly_linked: bool)
                     -> (CrateNum, Rc<cstore::CrateMetadata>, cstore::CrateSource) {
        info!("resolving crate `extern crate {} as {}`", name, ident);
        let result = match self.existing_match(name, hash, kind) {
            Some(cnum) => LoadResult::Previous(cnum),
            None => {
                info!("falling back to a load");
                let mut load_ctxt = loader::Context {
                    sess: self.sess,
                    span: span,
                    ident: ident,
                    crate_name: name,
                    hash: hash.map(|a| &*a),
                    filesearch: self.sess.target_filesearch(kind),
                    target: &self.sess.target.target,
                    triple: &self.sess.opts.target_triple,
                    root: root,
                    rejected_via_hash: vec!(),
                    rejected_via_triple: vec!(),
                    rejected_via_kind: vec!(),
                    rejected_via_version: vec!(),
                    should_match_name: true,
                };
                match self.load(&mut load_ctxt) {
                    Some(result) => result,
                    None => load_ctxt.report_load_errs(),
                }
            }
        };

        match result {
            LoadResult::Previous(cnum) => {
                let data = self.cstore.get_crate_data(cnum);
                if explicitly_linked && !data.explicitly_linked.get() {
                    data.explicitly_linked.set(explicitly_linked);
                }
                (cnum, data, self.cstore.used_crate_source(cnum))
            }
            LoadResult::Loaded(library) => {
                self.register_crate(root, ident, name, span, library,
                                    explicitly_linked)
            }
        }
    }

    fn load(&mut self, loader: &mut loader::Context) -> Option<LoadResult> {
        let library = match loader.maybe_load_library_crate() {
            Some(lib) => lib,
            None => return None,
        };

        // In the case that we're loading a crate, but not matching
        // against a hash, we could load a crate which has the same hash
        // as an already loaded crate. If this is the case prevent
        // duplicates by just using the first crate.
        //
        // Note that we only do this for target triple crates, though, as we
        // don't want to match a host crate against an equivalent target one
        // already loaded.
        let root = library.metadata.get_root();
        if loader.triple == self.sess.opts.target_triple {
            let mut result = LoadResult::Loaded(library);
            self.cstore.iter_crate_data(|cnum, data| {
                if data.name() == root.name && root.hash == data.hash() {
                    assert!(loader.hash.is_none());
                    info!("load success, going to previous cnum: {}", cnum);
                    result = LoadResult::Previous(cnum);
                }
            });
            Some(result)
        } else {
            Some(LoadResult::Loaded(library))
        }
    }

    fn update_extern_crate(&mut self,
                           cnum: CrateNum,
                           mut extern_crate: ExternCrate,
                           visited: &mut FnvHashSet<(CrateNum, bool)>)
    {
        if !visited.insert((cnum, extern_crate.direct)) { return }

        let cmeta = self.cstore.get_crate_data(cnum);
        let old_extern_crate = cmeta.extern_crate.get();

        // Prefer:
        // - something over nothing (tuple.0);
        // - direct extern crate to indirect (tuple.1);
        // - shorter paths to longer (tuple.2).
        let new_rank = (true, extern_crate.direct, !extern_crate.path_len);
        let old_rank = match old_extern_crate {
            None => (false, false, !0),
            Some(ref c) => (true, c.direct, !c.path_len),
        };

        if old_rank >= new_rank {
            return; // no change needed
        }

        cmeta.extern_crate.set(Some(extern_crate));
        // Propagate the extern crate info to dependencies.
        extern_crate.direct = false;
        for &dep_cnum in cmeta.cnum_map.borrow().iter() {
            self.update_extern_crate(dep_cnum, extern_crate, visited);
        }
    }

    // Go through the crate metadata and load any crates that it references
    fn resolve_crate_deps(&mut self,
                          root: &Option<CratePaths>,
                          crate_root: &CrateRoot,
                          metadata: &MetadataBlob,
                          krate: CrateNum,
                          span: Span)
                          -> cstore::CrateNumMap {
        debug!("resolving deps of external crate");
        // The map from crate numbers in the crate we're resolving to local crate
        // numbers
        let deps = crate_root.crate_deps.decode(metadata);
        let map: FnvHashMap<_, _> = deps.enumerate().map(|(crate_num, dep)| {
            debug!("resolving dep crate {} hash: `{}`", dep.name, dep.hash);
            let (local_cnum, ..) = self.resolve_crate(root,
                                                        &dep.name.as_str(),
                                                        &dep.name.as_str(),
                                                        Some(&dep.hash),
                                                        span,
                                                        PathKind::Dependency,
                                                        dep.explicitly_linked);
            (CrateNum::new(crate_num + 1), local_cnum)
        }).collect();

        let max_cnum = map.values().cloned().max().map(|cnum| cnum.as_u32()).unwrap_or(0);

        // we map 0 and all other holes in the map to our parent crate. The "additional"
        // self-dependencies should be harmless.
        (0..max_cnum+1).map(|cnum| {
            map.get(&CrateNum::from_u32(cnum)).cloned().unwrap_or(krate)
        }).collect()
    }

    fn read_extension_crate(&mut self, span: Span, info: &ExternCrateInfo) -> ExtensionCrate {
        info!("read extension crate {} `extern crate {} as {}` linked={}",
              info.id, info.name, info.ident, info.should_link);
        let target_triple = &self.sess.opts.target_triple[..];
        let is_cross = target_triple != config::host_triple();
        let mut should_link = info.should_link && !is_cross;
        let mut target_only = false;
        let ident = info.ident.clone();
        let name = info.name.clone();
        let mut load_ctxt = loader::Context {
            sess: self.sess,
            span: span,
            ident: &ident[..],
            crate_name: &name[..],
            hash: None,
            filesearch: self.sess.host_filesearch(PathKind::Crate),
            target: &self.sess.host,
            triple: config::host_triple(),
            root: &None,
            rejected_via_hash: vec!(),
            rejected_via_triple: vec!(),
            rejected_via_kind: vec!(),
            rejected_via_version: vec!(),
            should_match_name: true,
        };
        let library = self.load(&mut load_ctxt).or_else(|| {
            if !is_cross {
                return None
            }
            // Try loading from target crates. This will abort later if we
            // try to load a plugin registrar function,
            target_only = true;
            should_link = info.should_link;

            load_ctxt.target = &self.sess.target.target;
            load_ctxt.triple = target_triple;
            load_ctxt.filesearch = self.sess.target_filesearch(PathKind::Crate);

            self.load(&mut load_ctxt)
        });
        let library = match library {
            Some(l) => l,
            None => load_ctxt.report_load_errs(),
        };

        let (dylib, metadata) = match library {
            LoadResult::Previous(cnum) => {
                let dylib = self.cstore.opt_used_crate_source(cnum).unwrap().dylib;
                let data = self.cstore.get_crate_data(cnum);
                (dylib, PMDSource::Registered(data))
            }
            LoadResult::Loaded(library) => {
                let dylib = library.dylib.clone();
                let metadata = PMDSource::Owned(library);
                (dylib, metadata)
            }
        };

        ExtensionCrate {
            metadata: metadata,
            dylib: dylib.map(|p| p.0),
            target_only: target_only,
            name: info.name.to_string(),
            ident: info.ident.to_string(),
            span: span,
            should_link: should_link,
        }
    }

    pub fn read_macros(&mut self, item: &ast::Item) -> Macros {
        let ci = self.extract_crate_info(item).unwrap();
        let ekrate = self.read_extension_crate(item.span, &ci);

        let root = ekrate.metadata.get_root();
        let source_name = format!("<{} macros>", item.ident);
        let mut ret = Macros {
            macro_rules: Vec::new(),
            custom_derive_registrar: None,
            svh: root.hash,
            dylib: None,
        };
        for def in root.macro_defs.decode(&*ekrate.metadata) {
            // NB: Don't use parse::parse_tts_from_source_str because it parses with
            // quote_depth > 0.
            let mut p = parse::new_parser_from_source_str(&self.sess.parse_sess,
                                                          self.local_crate_config.clone(),
                                                          source_name.clone(),
                                                          def.body);
            let lo = p.span.lo;
            let body = match p.parse_all_token_trees() {
                Ok(body) => body,
                Err(mut err) => {
                    err.emit();
                    self.sess.abort_if_errors();
                    unreachable!();
                }
            };
            let local_span = mk_sp(lo, p.last_span.hi);

            // Mark the attrs as used
            for attr in &def.attrs {
                attr::mark_used(attr);
            }

            ret.macro_rules.push(ast::MacroDef {
                ident: ast::Ident::with_empty_ctxt(def.name),
                attrs: def.attrs,
                id: ast::DUMMY_NODE_ID,
                span: local_span,
                imported_from: Some(item.ident),
                // overridden in plugin/load.rs
                export: false,
                use_locally: false,
                allow_internal_unstable: false,

                body: body,
            });
            self.sess.imported_macro_spans.borrow_mut()
                .insert(local_span, (def.name.as_str().to_string(), def.span));
        }

        match root.macro_derive_registrar {
            Some(id) => ret.custom_derive_registrar = Some(id),

            // If this crate is not a proc-macro crate then we might be able to
            // register it with the local crate store to prevent loading the
            // metadata twice.
            //
            // If it's a proc-macro crate, though, then we definitely don't
            // want to register it with the local crate store as we're just
            // going to use it as we would a plugin.
            None => {
                ekrate.register(self);
                return ret
            }
        }

        self.cstore.add_used_for_derive_macros(item);
        ret.dylib = ekrate.dylib.clone();
        if ret.dylib.is_none() {
            span_bug!(item.span, "proc-macro crate not dylib");
        }

        if ekrate.target_only {
            let message = format!("proc-macro crate is not available for \
                                   triple `{}` (only found {})",
                                  config::host_triple(),
                                  self.sess.opts.target_triple);
            self.sess.span_fatal(item.span, &message);
        }

        return ret
    }

    /// Look for a plugin registrar. Returns library path, crate
    /// SVH and DefIndex of the registrar function.
    pub fn find_plugin_registrar(&mut self, span: Span, name: &str)
                                 -> Option<(PathBuf, Svh, DefIndex)> {
        let ekrate = self.read_extension_crate(span, &ExternCrateInfo {
             name: name.to_string(),
             ident: name.to_string(),
             id: ast::DUMMY_NODE_ID,
             should_link: false,
        });

        if ekrate.target_only {
            // Need to abort before syntax expansion.
            let message = format!("plugin `{}` is not available for triple `{}` \
                                   (only found {})",
                                  name,
                                  config::host_triple(),
                                  self.sess.opts.target_triple);
            span_fatal!(self.sess, span, E0456, "{}", &message[..]);
        }

        let root = ekrate.metadata.get_root();
        match (ekrate.dylib.as_ref(), root.plugin_registrar_fn) {
            (Some(dylib), Some(reg)) => {
                Some((dylib.to_path_buf(), root.hash, reg))
            }
            (None, Some(_)) => {
                span_err!(self.sess, span, E0457,
                          "plugin `{}` only found in rlib format, but must be available \
                           in dylib format",
                          name);
                // No need to abort because the loading code will just ignore this
                // empty dylib.
                None
            }
            _ => None,
        }
    }

    fn register_statically_included_foreign_items(&mut self) {
        let libs = self.cstore.get_used_libraries();
        for (lib, list) in self.foreign_item_map.iter() {
            let is_static = libs.borrow().iter().any(|&(ref name, kind)| {
                lib == name && kind == cstore::NativeStatic
            });
            if is_static {
                for id in list {
                    self.cstore.add_statically_included_foreign_item(*id);
                }
            }
        }
    }

    fn inject_panic_runtime(&mut self, krate: &ast::Crate) {
        // If we're only compiling an rlib, then there's no need to select a
        // panic runtime, so we just skip this section entirely.
        let any_non_rlib = self.sess.crate_types.borrow().iter().any(|ct| {
            *ct != config::CrateTypeRlib
        });
        if !any_non_rlib {
            info!("panic runtime injection skipped, only generating rlib");
            return
        }

        // If we need a panic runtime, we try to find an existing one here. At
        // the same time we perform some general validation of the DAG we've got
        // going such as ensuring everything has a compatible panic strategy.
        //
        // The logic for finding the panic runtime here is pretty much the same
        // as the allocator case with the only addition that the panic strategy
        // compilation mode also comes into play.
        let desired_strategy = self.sess.panic_strategy();
        let mut runtime_found = false;
        let mut needs_panic_runtime = attr::contains_name(&krate.attrs,
                                                          "needs_panic_runtime");
        self.cstore.iter_crate_data(|cnum, data| {
            needs_panic_runtime = needs_panic_runtime || data.needs_panic_runtime();
            if data.is_panic_runtime() {
                // Inject a dependency from all #![needs_panic_runtime] to this
                // #![panic_runtime] crate.
                self.inject_dependency_if(cnum, "a panic runtime",
                                          &|data| data.needs_panic_runtime());
                runtime_found = runtime_found || data.explicitly_linked.get();
            }
        });

        // If an explicitly linked and matching panic runtime was found, or if
        // we just don't need one at all, then we're done here and there's
        // nothing else to do.
        if !needs_panic_runtime || runtime_found {
            return
        }

        // By this point we know that we (a) need a panic runtime and (b) no
        // panic runtime was explicitly linked. Here we just load an appropriate
        // default runtime for our panic strategy and then inject the
        // dependencies.
        //
        // We may resolve to an already loaded crate (as the crate may not have
        // been explicitly linked prior to this) and we may re-inject
        // dependencies again, but both of those situations are fine.
        //
        // Also note that we have yet to perform validation of the crate graph
        // in terms of everyone has a compatible panic runtime format, that's
        // performed later as part of the `dependency_format` module.
        let name = match desired_strategy {
            PanicStrategy::Unwind => "panic_unwind",
            PanicStrategy::Abort => "panic_abort",
        };
        info!("panic runtime not found -- loading {}", name);

        let (cnum, data, _) = self.resolve_crate(&None, name, name, None,
                                                 syntax_pos::DUMMY_SP,
                                                 PathKind::Crate, false);

        // Sanity check the loaded crate to ensure it is indeed a panic runtime
        // and the panic strategy is indeed what we thought it was.
        if !data.is_panic_runtime() {
            self.sess.err(&format!("the crate `{}` is not a panic runtime",
                                   name));
        }
        if data.panic_strategy() != desired_strategy {
            self.sess.err(&format!("the crate `{}` does not have the panic \
                                    strategy `{}`",
                                   name, desired_strategy.desc()));
        }

        self.sess.injected_panic_runtime.set(Some(cnum));
        self.inject_dependency_if(cnum, "a panic runtime",
                                  &|data| data.needs_panic_runtime());
    }

    fn inject_allocator_crate(&mut self) {
        // Make sure that we actually need an allocator, if none of our
        // dependencies need one then we definitely don't!
        //
        // Also, if one of our dependencies has an explicit allocator, then we
        // also bail out as we don't need to implicitly inject one.
        let mut needs_allocator = false;
        let mut found_required_allocator = false;
        self.cstore.iter_crate_data(|cnum, data| {
            needs_allocator = needs_allocator || data.needs_allocator();
            if data.is_allocator() {
                info!("{} required by rlib and is an allocator", data.name());
                self.inject_dependency_if(cnum, "an allocator",
                                          &|data| data.needs_allocator());
                found_required_allocator = found_required_allocator ||
                    data.explicitly_linked.get();
            }
        });
        if !needs_allocator || found_required_allocator { return }

        // At this point we've determined that we need an allocator and no
        // previous allocator has been activated. We look through our outputs of
        // crate types to see what kind of allocator types we may need.
        //
        // The main special output type here is that rlibs do **not** need an
        // allocator linked in (they're just object files), only final products
        // (exes, dylibs, staticlibs) need allocators.
        let mut need_lib_alloc = false;
        let mut need_exe_alloc = false;
        for ct in self.sess.crate_types.borrow().iter() {
            match *ct {
                config::CrateTypeExecutable => need_exe_alloc = true,
                config::CrateTypeDylib |
                config::CrateTypeProcMacro |
                config::CrateTypeCdylib |
                config::CrateTypeStaticlib => need_lib_alloc = true,
                config::CrateTypeRlib => {}
            }
        }
        if !need_lib_alloc && !need_exe_alloc { return }

        // The default allocator crate comes from the custom target spec, and we
        // choose between the standard library allocator or exe allocator. This
        // distinction exists because the default allocator for binaries (where
        // the world is Rust) is different than library (where the world is
        // likely *not* Rust).
        //
        // If a library is being produced, but we're also flagged with `-C
        // prefer-dynamic`, then we interpret this as a *Rust* dynamic library
        // is being produced so we use the exe allocator instead.
        //
        // What this boils down to is:
        //
        // * Binaries use jemalloc
        // * Staticlibs and Rust dylibs use system malloc
        // * Rust dylibs used as dependencies to rust use jemalloc
        let name = if need_lib_alloc && !self.sess.opts.cg.prefer_dynamic {
            &self.sess.target.target.options.lib_allocation_crate
        } else {
            &self.sess.target.target.options.exe_allocation_crate
        };
        let (cnum, data, _) = self.resolve_crate(&None, name, name, None,
                                                 syntax_pos::DUMMY_SP,
                                                 PathKind::Crate, false);

        // Sanity check the crate we loaded to ensure that it is indeed an
        // allocator.
        if !data.is_allocator() {
            self.sess.err(&format!("the allocator crate `{}` is not tagged \
                                    with #![allocator]", data.name()));
        }

        self.sess.injected_allocator.set(Some(cnum));
        self.inject_dependency_if(cnum, "an allocator",
                                  &|data| data.needs_allocator());
    }

    fn inject_dependency_if(&self,
                            krate: CrateNum,
                            what: &str,
                            needs_dep: &Fn(&cstore::CrateMetadata) -> bool) {
        // don't perform this validation if the session has errors, as one of
        // those errors may indicate a circular dependency which could cause
        // this to stack overflow.
        if self.sess.has_errors() {
            return
        }

        // Before we inject any dependencies, make sure we don't inject a
        // circular dependency by validating that this crate doesn't
        // transitively depend on any crates satisfying `needs_dep`.
        for dep in self.cstore.crate_dependencies_in_rpo(krate) {
            let data = self.cstore.get_crate_data(dep);
            if needs_dep(&data) {
                self.sess.err(&format!("the crate `{}` cannot depend \
                                        on a crate that needs {}, but \
                                        it depends on `{}`",
                                       self.cstore.get_crate_data(krate).name(),
                                       what,
                                       data.name()));
            }
        }

        // All crates satisfying `needs_dep` do not explicitly depend on the
        // crate provided for this compile, but in order for this compilation to
        // be successfully linked we need to inject a dependency (to order the
        // crates on the command line correctly).
        self.cstore.iter_crate_data(|cnum, data| {
            if !needs_dep(data) {
                return
            }

            info!("injecting a dep from {} to {}", cnum, krate);
            data.cnum_map.borrow_mut().push(krate);
        });
    }
}

impl ExtensionCrate {
    fn register(self, creader: &mut CrateReader) {
        if !self.should_link {
            return
        }

        let library = match self.metadata {
            PMDSource::Owned(lib) => lib,
            PMDSource::Registered(_) => return,
        };

        // Register crate now to avoid double-reading metadata
        creader.register_crate(&None,
                               &self.ident,
                               &self.name,
                               self.span,
                               library,
                               true);
    }
}

impl<'a> CrateLoader<'a> {
    pub fn new(sess: &'a Session, cstore: &'a CStore, krate: &ast::Crate, crate_name: &str)
               -> Self {
        let loader = CrateLoader {
            sess: sess,
            cstore: cstore,
            creader: CrateReader::new(sess, cstore, crate_name, krate.config.clone()),
        };

        for attr in krate.attrs.iter().filter(|m| m.name() == "link_args") {
            if let Some(ref linkarg) = attr.value_str() {
                loader.cstore.add_used_link_args(&linkarg);
            }
        }

        loader
    }

    fn process_foreign_mod(&mut self, i: &ast::Item, fm: &ast::ForeignMod) {
        if fm.abi == Abi::Rust || fm.abi == Abi::RustIntrinsic || fm.abi == Abi::PlatformIntrinsic {
            return;
        }

        // First, add all of the custom #[link_args] attributes
        for m in i.attrs.iter().filter(|a| a.check_name("link_args")) {
            if let Some(linkarg) = m.value_str() {
                self.cstore.add_used_link_args(&linkarg);
            }
        }

        // Next, process all of the #[link(..)]-style arguments
        for m in i.attrs.iter().filter(|a| a.check_name("link")) {
            let items = match m.meta_item_list() {
                Some(item) => item,
                None => continue,
            };
            let kind = items.iter().find(|k| {
                k.check_name("kind")
            }).and_then(|a| a.value_str());
            let kind = match kind.as_ref().map(|s| &s[..]) {
                Some("static") => cstore::NativeStatic,
                Some("dylib") => cstore::NativeUnknown,
                Some("framework") => cstore::NativeFramework,
                Some(k) => {
                    struct_span_err!(self.sess, m.span, E0458,
                              "unknown kind: `{}`", k)
                        .span_label(m.span, &format!("unknown kind")).emit();
                    cstore::NativeUnknown
                }
                None => cstore::NativeUnknown
            };
            let n = items.iter().find(|n| {
                n.check_name("name")
            }).and_then(|a| a.value_str());
            let n = match n {
                Some(n) => n,
                None => {
                    struct_span_err!(self.sess, m.span, E0459,
                                     "#[link(...)] specified without `name = \"foo\"`")
                        .span_label(m.span, &format!("missing `name` argument")).emit();
                    InternedString::new("foo")
                }
            };
            register_native_lib(self.sess, self.cstore, Some(m.span), n.to_string(), kind);
        }

        // Finally, process the #[linked_from = "..."] attribute
        for m in i.attrs.iter().filter(|a| a.check_name("linked_from")) {
            let lib_name = match m.value_str() {
                Some(name) => name,
                None => continue,
            };
            let list = self.creader.foreign_item_map.entry(lib_name.to_string())
                                                    .or_insert(Vec::new());
            list.extend(fm.items.iter().map(|it| it.id));
        }
    }
}

impl<'a> middle::cstore::CrateLoader for CrateLoader<'a> {
    fn postprocess(&mut self, krate: &ast::Crate) {
        self.creader.inject_allocator_crate();
        self.creader.inject_panic_runtime(krate);

        if log_enabled!(log::INFO) {
            dump_crates(&self.cstore);
        }

        for &(ref name, kind) in &self.sess.opts.libs {
            register_native_lib(self.sess, self.cstore, None, name.clone(), kind);
        }
        self.creader.register_statically_included_foreign_items();
    }

    fn process_item(&mut self, item: &ast::Item, definitions: &hir_map::Definitions) {
        match item.node {
            ast::ItemKind::ExternCrate(_) => {}
            ast::ItemKind::ForeignMod(ref fm) => return self.process_foreign_mod(item, fm),
            _ => return,
        }

        // If this `extern crate` item has `#[macro_use]` then we can safely skip it.
        // These annotations were processed during macro expansion and are already loaded
        // (if necessary) into our crate store.
        //
        // Note that it's important we *don't* fall through below as some `#[macro_use]`
        // crates are explicitly not linked (e.g. macro crates) so we want to ensure
        // we avoid `resolve_crate` with those.
        if attr::contains_name(&item.attrs, "macro_use") {
            if self.cstore.was_used_for_derive_macros(item) {
                return
            }
        }

        if let Some(info) = self.creader.extract_crate_info(item) {
            if !info.should_link {
                return;
            }

            let (cnum, ..) = self.creader.resolve_crate(
                &None, &info.ident, &info.name, None, item.span, PathKind::Crate, true,
            );

            let def_id = definitions.opt_local_def_id(item.id).unwrap();
            let len = definitions.def_path(def_id.index).data.len();

            let extern_crate =
                ExternCrate { def_id: def_id, span: item.span, direct: true, path_len: len };
            self.creader.update_extern_crate(cnum, extern_crate, &mut FnvHashSet());

            self.cstore.add_extern_mod_stmt_cnum(info.id, cnum);
        }
    }

    fn load_macros(&mut self, extern_crate: &ast::Item, allows_macros: bool) -> Vec<LoadedMacro> {
        macro_import::load_macros(self, extern_crate, allows_macros)
    }
}
