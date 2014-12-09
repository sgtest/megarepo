// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rustc::session::Session;
use rustc::session::config::{mod, Input, OutputFilenames};
use rustc::lint;
use rustc::metadata::creader;
use rustc::middle::{stability, ty, reachable};
use rustc::middle::dependency_format;
use rustc::middle;
use rustc::plugin::load::Plugins;
use rustc::plugin::registry::Registry;
use rustc::plugin;
use rustc::util::common::time;
use rustc_trans::back::link;
use rustc_trans::back::write;
use rustc_trans::save;
use rustc_trans::trans;
use rustc_typeck as typeck;

use serialize::{json, Encodable};

use std::io;
use std::io::fs;
use std::os;
use arena::TypedArena;
use syntax::ast;
use syntax::ast_map;
use syntax::attr;
use syntax::attr::{AttrMetaMethods};
use syntax::diagnostics;
use syntax::parse;
use syntax::parse::token;
use syntax;

pub fn compile_input(sess: Session,
                     cfg: ast::CrateConfig,
                     input: &Input,
                     outdir: &Option<Path>,
                     output: &Option<Path>,
                     addl_plugins: Option<Plugins>) {
    // These may be left in an incoherent state after a previous compile.
    // `clear_tables` and `get_ident_interner().clear()` can be used to free
    // memory, but they do not restore the initial state.
    syntax::ext::mtwt::reset_tables();
    token::reset_ident_interner();

    // We need nested scopes here, because the intermediate results can keep
    // large chunks of memory alive and we want to free them as soon as
    // possible to keep the peak memory usage low
    let (outputs, trans, sess) = {
        let (outputs, expanded_crate, id) = {
            let krate = phase_1_parse_input(&sess, cfg, input);
            if stop_after_phase_1(&sess) { return; }
            let outputs = build_output_filenames(input,
                                                 outdir,
                                                 output,
                                                 krate.attrs.as_slice(),
                                                 &sess);
            let id = link::find_crate_name(Some(&sess), krate.attrs.as_slice(),
                                           input);
            let expanded_crate
                = match phase_2_configure_and_expand(&sess, krate, id.as_slice(),
                                                     addl_plugins) {
                    None => return,
                    Some(k) => k
                };

            (outputs, expanded_crate, id)
        };

        let mut forest = ast_map::Forest::new(expanded_crate);
        let ast_map = assign_node_ids_and_map(&sess, &mut forest);

        write_out_deps(&sess, input, &outputs, id.as_slice());

        if stop_after_phase_2(&sess) { return; }

        let type_arena = TypedArena::new();
        let analysis = phase_3_run_analysis_passes(sess, ast_map, &type_arena, id);
        phase_save_analysis(&analysis.ty_cx.sess, analysis.ty_cx.map.krate(), &analysis, outdir);
        if stop_after_phase_3(&analysis.ty_cx.sess) { return; }
        let (tcx, trans) = phase_4_translate_to_llvm(analysis);

        // Discard interned strings as they are no longer required.
        token::get_ident_interner().clear();

        (outputs, trans, tcx.sess)
    };
    phase_5_run_llvm_passes(&sess, &trans, &outputs);
    if stop_after_phase_5(&sess) { return; }
    phase_6_link_output(&sess, &trans, &outputs);
}

/// The name used for source code that doesn't originate in a file
/// (e.g. source from stdin or a string)
pub fn anon_src() -> String {
    "<anon>".to_string()
}

pub fn source_name(input: &Input) -> String {
    match *input {
        // FIXME (#9639): This needs to handle non-utf8 paths
        Input::File(ref ifile) => ifile.as_str().unwrap().to_string(),
        Input::Str(_) => anon_src()
    }
}

pub fn phase_1_parse_input(sess: &Session, cfg: ast::CrateConfig, input: &Input)
    -> ast::Crate {
    let krate = time(sess.time_passes(), "parsing", (), |_| {
        match *input {
            Input::File(ref file) => {
                parse::parse_crate_from_file(&(*file), cfg.clone(), &sess.parse_sess)
            }
            Input::Str(ref src) => {
                parse::parse_crate_from_source_str(anon_src().to_string(),
                                                   src.to_string(),
                                                   cfg.clone(),
                                                   &sess.parse_sess)
            }
        }
    });

    if sess.opts.debugging_opts & config::AST_JSON_NOEXPAND != 0 {
        let mut stdout = io::BufferedWriter::new(io::stdout());
        let mut json = json::PrettyEncoder::new(&mut stdout);
        // unwrapping so IoError isn't ignored
        krate.encode(&mut json).unwrap();
    }

    if sess.show_span() {
        syntax::show_span::run(sess.diagnostic(), &krate);
    }

    krate
}

// For continuing compilation after a parsed crate has been
// modified

/// Run the "early phases" of the compiler: initial `cfg` processing,
/// loading compiler plugins (including those from `addl_plugins`),
/// syntax expansion, secondary `cfg` expansion, synthesis of a test
/// harness if one is to be provided and injection of a dependency on the
/// standard library and prelude.
///
/// Returns `None` if we're aborting after handling -W help.
pub fn phase_2_configure_and_expand(sess: &Session,
                                    mut krate: ast::Crate,
                                    crate_name: &str,
                                    addl_plugins: Option<Plugins>)
                                    -> Option<ast::Crate> {
    let time_passes = sess.time_passes();

    *sess.crate_types.borrow_mut() =
        collect_crate_types(sess, krate.attrs.as_slice());
    *sess.crate_metadata.borrow_mut() =
        collect_crate_metadata(sess, krate.attrs.as_slice());

    time(time_passes, "gated feature checking", (), |_| {
        let (features, unknown_features) =
            syntax::feature_gate::check_crate(&sess.parse_sess.span_diagnostic, &krate);

        for uf in unknown_features.iter() {
            sess.add_lint(lint::builtin::UNKNOWN_FEATURES,
                          ast::CRATE_NODE_ID,
                          *uf,
                          "unknown feature".to_string());
        }

        sess.abort_if_errors();
        *sess.features.borrow_mut() = features;
    });

    time(time_passes, "recursion limit", (), |_| {
        middle::recursion_limit::update_recursion_limit(sess, &krate);
    });

    // strip before expansion to allow macros to depend on
    // configuration variables e.g/ in
    //
    //   #[macro_escape] #[cfg(foo)]
    //   mod bar { macro_rules! baz!(() => {{}}) }
    //
    // baz! should not use this definition unless foo is enabled.

    krate = time(time_passes, "configuration 1", krate, |krate|
                 syntax::config::strip_unconfigured_items(sess.diagnostic(), krate));

    krate = time(time_passes, "crate injection", krate, |krate|
                 syntax::std_inject::maybe_inject_crates_ref(krate,
                                                             sess.opts.alt_std_name.clone()));

    let mut addl_plugins = Some(addl_plugins);
    let Plugins { macros, registrars }
        = time(time_passes, "plugin loading", (), |_|
               plugin::load::load_plugins(sess, &krate, addl_plugins.take().unwrap()));

    let mut registry = Registry::new(&krate);

    time(time_passes, "plugin registration", (), |_| {
        if sess.features.borrow().rustc_diagnostic_macros {
            registry.register_macro("__diagnostic_used",
                diagnostics::plugin::expand_diagnostic_used);
            registry.register_macro("__register_diagnostic",
                diagnostics::plugin::expand_register_diagnostic);
            registry.register_macro("__build_diagnostic_array",
                diagnostics::plugin::expand_build_diagnostic_array);
        }

        for &registrar in registrars.iter() {
            registrar(&mut registry);
        }
    });

    let Registry { syntax_exts, lint_passes, lint_groups, .. } = registry;

    {
        let mut ls = sess.lint_store.borrow_mut();
        for pass in lint_passes.into_iter() {
            ls.register_pass(Some(sess), true, pass);
        }

        for (name, to) in lint_groups.into_iter() {
            ls.register_group(Some(sess), true, name, to);
        }
    }

    // Lint plugins are registered; now we can process command line flags.
    if sess.opts.describe_lints {
        super::describe_lints(&*sess.lint_store.borrow(), true);
        return None;
    }
    sess.lint_store.borrow_mut().process_command_line(sess);

    // Abort if there are errors from lint processing or a plugin registrar.
    sess.abort_if_errors();

    krate = time(time_passes, "expansion", (krate, macros, syntax_exts),
        |(krate, macros, syntax_exts)| {
            // Windows dlls do not have rpaths, so they don't know how to find their
            // dependencies. It's up to us to tell the system where to find all the
            // dependent dlls. Note that this uses cfg!(windows) as opposed to
            // targ_cfg because syntax extensions are always loaded for the host
            // compiler, not for the target.
            let mut _old_path = String::new();
            if cfg!(windows) {
                _old_path = os::getenv("PATH").unwrap_or(_old_path);
                let mut new_path = sess.host_filesearch().get_dylib_search_paths();
                new_path.extend(os::split_paths(_old_path.as_slice()).into_iter());
                os::setenv("PATH", os::join_paths(new_path.as_slice()).unwrap());
            }
            let cfg = syntax::ext::expand::ExpansionConfig {
                crate_name: crate_name.to_string(),
                deriving_hash_type_parameter: sess.features.borrow().default_type_params,
                enable_quotes: sess.features.borrow().quote,
                recursion_limit: sess.recursion_limit.get(),
            };
            let ret = syntax::ext::expand::expand_crate(&sess.parse_sess,
                                              cfg,
                                              macros,
                                              syntax_exts,
                                              krate);
            if cfg!(windows) {
                os::setenv("PATH", _old_path);
            }
            ret
        }
    );

    // JBC: make CFG processing part of expansion to avoid this problem:

    // strip again, in case expansion added anything with a #[cfg].
    krate = time(time_passes, "configuration 2", krate, |krate|
                 syntax::config::strip_unconfigured_items(sess.diagnostic(), krate));

    krate = time(time_passes, "maybe building test harness", krate, |krate|
                 syntax::test::modify_for_testing(&sess.parse_sess,
                                                  &sess.opts.cfg,
                                                  krate,
                                                  sess.diagnostic()));

    krate = time(time_passes, "prelude injection", krate, |krate|
                 syntax::std_inject::maybe_inject_prelude(krate));

    time(time_passes, "checking that all macro invocations are gone", &krate, |krate|
         syntax::ext::expand::check_for_macros(&sess.parse_sess, krate));

    Some(krate)
}

pub fn assign_node_ids_and_map<'ast>(sess: &Session,
                                     forest: &'ast mut ast_map::Forest)
                                     -> ast_map::Map<'ast> {
    struct NodeIdAssigner<'a> {
        sess: &'a Session
    }

    impl<'a> ast_map::FoldOps for NodeIdAssigner<'a> {
        fn new_id(&self, old_id: ast::NodeId) -> ast::NodeId {
            assert_eq!(old_id, ast::DUMMY_NODE_ID);
            self.sess.next_node_id()
        }
    }

    let map = time(sess.time_passes(), "assigning node ids and indexing ast", forest, |forest|
                   ast_map::map_crate(forest, NodeIdAssigner { sess: sess }));

    if sess.opts.debugging_opts & config::AST_JSON != 0 {
        let mut stdout = io::BufferedWriter::new(io::stdout());
        let mut json = json::PrettyEncoder::new(&mut stdout);
        // unwrapping so IoError isn't ignored
        map.krate().encode(&mut json).unwrap();
    }

    map
}

/// Run the resolution, typechecking, region checking and other
/// miscellaneous analysis passes on the crate. Return various
/// structures carrying the results of the analysis.
pub fn phase_3_run_analysis_passes<'tcx>(sess: Session,
                                         ast_map: ast_map::Map<'tcx>,
                                         type_arena: &'tcx TypedArena<ty::TyS<'tcx>>,
                                         name: String) -> ty::CrateAnalysis<'tcx> {
    let time_passes = sess.time_passes();
    let krate = ast_map.krate();

    time(time_passes, "external crate/lib resolution", (), |_|
         creader::read_crates(&sess, krate));

    let lang_items = time(time_passes, "language item collection", (), |_|
                          middle::lang_items::collect_language_items(krate, &sess));

    let middle::resolve::CrateMap {
        def_map,
        freevars,
        capture_mode_map,
        exp_map2,
        trait_map,
        external_exports,
        last_private_map
    } =
        time(time_passes, "resolution", (), |_|
             middle::resolve::resolve_crate(&sess, &lang_items, krate));

    // Discard MTWT tables that aren't required past resolution.
    syntax::ext::mtwt::clear_tables();

    let named_region_map = time(time_passes, "lifetime resolution", (),
                                |_| middle::resolve_lifetime::krate(&sess, krate, &def_map));

    time(time_passes, "looking for entry point", (),
         |_| middle::entry::find_entry_point(&sess, &ast_map));

    sess.plugin_registrar_fn.set(
        time(time_passes, "looking for plugin registrar", (), |_|
            plugin::build::find_plugin_registrar(
                sess.diagnostic(), krate)));

    let region_map = time(time_passes, "region resolution", (), |_|
                          middle::region::resolve_crate(&sess, krate));

    time(time_passes, "loop checking", (), |_|
         middle::check_loop::check_crate(&sess, krate));

    let stability_index = time(time_passes, "stability index", (), |_|
                               stability::Index::build(krate));

    time(time_passes, "static item recursion checking", (), |_|
         middle::check_static_recursion::check_crate(&sess, krate, &def_map, &ast_map));

    let ty_cx = ty::mk_ctxt(sess,
                            type_arena,
                            def_map,
                            named_region_map,
                            ast_map,
                            freevars,
                            capture_mode_map,
                            region_map,
                            lang_items,
                            stability_index);

    // passes are timed inside typeck
    typeck::check_crate(&ty_cx, trait_map);

    time(time_passes, "check static items", (), |_|
         middle::check_static::check_crate(&ty_cx));

    // These next two const passes can probably be merged
    time(time_passes, "const marking", (), |_|
         middle::const_eval::process_crate(&ty_cx));

    time(time_passes, "const checking", (), |_|
         middle::check_const::check_crate(&ty_cx));

    let maps = (external_exports, last_private_map);
    let (exported_items, public_items) =
            time(time_passes, "privacy checking", maps, |(a, b)|
                 middle::privacy::check_crate(&ty_cx, &exp_map2, a, b));

    time(time_passes, "intrinsic checking", (), |_|
         middle::intrinsicck::check_crate(&ty_cx));

    time(time_passes, "effect checking", (), |_|
         middle::effect::check_crate(&ty_cx));

    time(time_passes, "match checking", (), |_|
         middle::check_match::check_crate(&ty_cx));

    time(time_passes, "liveness checking", (), |_|
         middle::liveness::check_crate(&ty_cx));

    time(time_passes, "borrow checking", (), |_|
         middle::borrowck::check_crate(&ty_cx));

    time(time_passes, "rvalue checking", (), |_|
         middle::check_rvalues::check_crate(&ty_cx, krate));

    // Avoid overwhelming user with errors if type checking failed.
    // I'm not sure how helpful this is, to be honest, but it avoids a
    // lot of annoying errors in the compile-fail tests (basically,
    // lint warnings and so on -- kindck used to do this abort, but
    // kindck is gone now). -nmatsakis
    ty_cx.sess.abort_if_errors();

    let reachable_map =
        time(time_passes, "reachability checking", (), |_|
             reachable::find_reachable(&ty_cx, &exported_items));

    time(time_passes, "death checking", (), |_| {
        middle::dead::check_crate(&ty_cx,
                                  &exported_items,
                                  &reachable_map)
    });

    time(time_passes, "lint checking", (), |_|
         lint::check_crate(&ty_cx, &exported_items));

    ty::CrateAnalysis {
        exp_map2: exp_map2,
        ty_cx: ty_cx,
        exported_items: exported_items,
        public_items: public_items,
        reachable: reachable_map,
        name: name,
    }
}

pub fn phase_save_analysis(sess: &Session,
                           krate: &ast::Crate,
                           analysis: &ty::CrateAnalysis,
                           odir: &Option<Path>) {
    if (sess.opts.debugging_opts & config::SAVE_ANALYSIS) == 0 {
        return;
    }
    time(sess.time_passes(), "save analysis", krate, |krate|
         save::process_crate(sess, krate, analysis, odir));
}

/// Run the translation phase to LLVM, after which the AST and analysis can
/// be discarded.
pub fn phase_4_translate_to_llvm<'tcx>(analysis: ty::CrateAnalysis<'tcx>)
                                       -> (ty::ctxt<'tcx>, trans::CrateTranslation) {
    let time_passes = analysis.ty_cx.sess.time_passes();

    time(time_passes, "resolving dependency formats", (), |_|
         dependency_format::calculate(&analysis.ty_cx));

    // Option dance to work around the lack of stack once closures.
    time(time_passes, "translation", analysis, |analysis|
         trans::trans_crate(analysis))
}

/// Run LLVM itself, producing a bitcode file, assembly file or object file
/// as a side effect.
pub fn phase_5_run_llvm_passes(sess: &Session,
                               trans: &trans::CrateTranslation,
                               outputs: &OutputFilenames) {
    if sess.opts.cg.no_integrated_as {
        let output_type = config::OutputTypeAssembly;

        time(sess.time_passes(), "LLVM passes", (), |_|
            write::run_passes(sess, trans, &[output_type], outputs));

        write::run_assembler(sess, outputs);

        // Remove assembly source, unless --save-temps was specified
        if !sess.opts.cg.save_temps {
            fs::unlink(&outputs.temp_path(config::OutputTypeAssembly)).unwrap();
        }
    } else {
        time(sess.time_passes(), "LLVM passes", (), |_|
            write::run_passes(sess,
                              trans,
                              sess.opts.output_types.as_slice(),
                              outputs));
    }

    sess.abort_if_errors();
}

/// Run the linker on any artifacts that resulted from the LLVM run.
/// This should produce either a finished executable or library.
pub fn phase_6_link_output(sess: &Session,
                           trans: &trans::CrateTranslation,
                           outputs: &OutputFilenames) {
    let old_path = os::getenv("PATH").unwrap_or_else(||String::new());
    let mut new_path = sess.host_filesearch().get_tools_search_paths();
    new_path.extend(os::split_paths(old_path.as_slice()).into_iter());
    os::setenv("PATH", os::join_paths(new_path.as_slice()).unwrap());

    time(sess.time_passes(), "linking", (), |_|
         link::link_binary(sess,
                           trans,
                           outputs,
                           trans.link.crate_name.as_slice()));

    os::setenv("PATH", old_path);
}

pub fn stop_after_phase_3(sess: &Session) -> bool {
   if sess.opts.no_trans {
        debug!("invoked with --no-trans, returning early from compile_input");
        return true;
    }
    return false;
}

pub fn stop_after_phase_1(sess: &Session) -> bool {
    if sess.opts.parse_only {
        debug!("invoked with --parse-only, returning early from compile_input");
        return true;
    }
    if sess.show_span() {
        return true;
    }
    return sess.opts.debugging_opts & config::AST_JSON_NOEXPAND != 0;
}

pub fn stop_after_phase_2(sess: &Session) -> bool {
    if sess.opts.no_analysis {
        debug!("invoked with --no-analysis, returning early from compile_input");
        return true;
    }
    return sess.opts.debugging_opts & config::AST_JSON != 0;
}

pub fn stop_after_phase_5(sess: &Session) -> bool {
    if !sess.opts.output_types.iter().any(|&i| i == config::OutputTypeExe) {
        debug!("not building executable, returning early from compile_input");
        return true;
    }
    return false;
}

fn escape_dep_filename(filename: &str) -> String {
    // Apparently clang and gcc *only* escape spaces:
    // http://llvm.org/klaus/clang/commit/9d50634cfc268ecc9a7250226dd5ca0e945240d4
    filename.replace(" ", "\\ ")
}

fn write_out_deps(sess: &Session,
                  input: &Input,
                  outputs: &OutputFilenames,
                  id: &str) {

    let mut out_filenames = Vec::new();
    for output_type in sess.opts.output_types.iter() {
        let file = outputs.path(*output_type);
        match *output_type {
            config::OutputTypeExe => {
                for output in sess.crate_types.borrow().iter() {
                    let p = link::filename_for_input(sess, *output,
                                                     id, &file);
                    out_filenames.push(p);
                }
            }
            _ => { out_filenames.push(file); }
        }
    }

    // Write out dependency rules to the dep-info file if requested with
    // --dep-info
    let deps_filename = match sess.opts.write_dependency_info {
        // Use filename from --dep-file argument if given
        (true, Some(ref filename)) => filename.clone(),
        // Use default filename: crate source filename with extension replaced
        // by ".d"
        (true, None) => match *input {
            Input::File(..) => outputs.with_extension("d"),
            Input::Str(..) => {
                sess.warn("can not write --dep-info without a filename \
                           when compiling stdin.");
                return
            },
        },
        _ => return,
    };

    let result = (|| -> io::IoResult<()> {
        // Build a list of files used to compile the output and
        // write Makefile-compatible dependency rules
        let files: Vec<String> = sess.codemap().files.borrow()
                                   .iter().filter(|fmap| fmap.is_real_file())
                                   .map(|fmap| escape_dep_filename(fmap.name.as_slice()))
                                   .collect();
        let mut file = try!(io::File::create(&deps_filename));
        for path in out_filenames.iter() {
            try!(write!(&mut file as &mut Writer,
                          "{}: {}\n\n", path.display(), files.connect(" ")));
        }
        Ok(())
    })();

    match result {
        Ok(()) => {}
        Err(e) => {
            sess.fatal(format!("error writing dependencies to `{}`: {}",
                               deps_filename.display(), e).as_slice());
        }
    }
}

pub fn collect_crate_types(session: &Session,
                           attrs: &[ast::Attribute]) -> Vec<config::CrateType> {
    // Unconditionally collect crate types from attributes to make them used
    let attr_types: Vec<config::CrateType> = attrs.iter().filter_map(|a| {
        if a.check_name("crate_type") {
            match a.value_str() {
                Some(ref n) if *n == "rlib" => {
                    Some(config::CrateTypeRlib)
                }
                Some(ref n) if *n == "dylib" => {
                    Some(config::CrateTypeDylib)
                }
                Some(ref n) if *n == "lib" => {
                    Some(config::default_lib_output())
                }
                Some(ref n) if *n == "staticlib" => {
                    Some(config::CrateTypeStaticlib)
                }
                Some(ref n) if *n == "bin" => Some(config::CrateTypeExecutable),
                Some(_) => {
                    session.add_lint(lint::builtin::UNKNOWN_CRATE_TYPES,
                                     ast::CRATE_NODE_ID,
                                     a.span,
                                     "invalid `crate_type` \
                                      value".to_string());
                    None
                }
                _ => {
                    session.add_lint(lint::builtin::UNKNOWN_CRATE_TYPES,
                                     ast::CRATE_NODE_ID,
                                     a.span,
                                     "`crate_type` requires a \
                                      value".to_string());
                    None
                }
            }
        } else {
            None
        }
    }).collect();

    // If we're generating a test executable, then ignore all other output
    // styles at all other locations
    if session.opts.test {
        return vec!(config::CrateTypeExecutable)
    }

    // Only check command line flags if present. If no types are specified by
    // command line, then reuse the empty `base` Vec to hold the types that
    // will be found in crate attributes.
    let mut base = session.opts.crate_types.clone();
    if base.len() == 0 {
        base.extend(attr_types.into_iter());
        if base.len() == 0 {
            base.push(link::default_output_for_target(session));
        }
        base.sort();
        base.dedup();
    }

    base.into_iter().filter(|crate_type| {
        let res = !link::invalid_output_for_target(session, *crate_type);

        if !res {
            session.warn(format!("dropping unsupported crate type `{}` \
                                   for target `{}`",
                                 *crate_type, session.opts.target_triple).as_slice());
        }

        res
    }).collect()
}

pub fn collect_crate_metadata(session: &Session,
                              _attrs: &[ast::Attribute]) -> Vec<String> {
    session.opts.cg.metadata.clone()
}

pub fn build_output_filenames(input: &Input,
                              odir: &Option<Path>,
                              ofile: &Option<Path>,
                              attrs: &[ast::Attribute],
                              sess: &Session)
                           -> OutputFilenames {
    match *ofile {
        None => {
            // "-" as input file will cause the parser to read from stdin so we
            // have to make up a name
            // We want to toss everything after the final '.'
            let dirpath = match *odir {
                Some(ref d) => d.clone(),
                None => Path::new(".")
            };

            // If a crate name is present, we use it as the link name
            let stem = sess.opts.crate_name.clone().or_else(|| {
                attr::find_crate_name(attrs).map(|n| n.get().to_string())
            }).unwrap_or(input.filestem());

            OutputFilenames {
                out_directory: dirpath,
                out_filestem: stem,
                single_output_file: None,
                extra: sess.opts.cg.extra_filename.clone(),
            }
        }

        Some(ref out_file) => {
            let ofile = if sess.opts.output_types.len() > 1 {
                sess.warn("ignoring specified output filename because multiple \
                           outputs were requested");
                None
            } else {
                Some(out_file.clone())
            };
            if *odir != None {
                sess.warn("ignoring --out-dir flag due to -o flag.");
            }
            OutputFilenames {
                out_directory: out_file.dir_path(),
                out_filestem: out_file.filestem_str().unwrap().to_string(),
                single_output_file: ofile,
                extra: sess.opts.cg.extra_filename.clone(),
            }
        }
    }
}
