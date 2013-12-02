// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use back::archive::Archive;
use back::rpath;
use driver::session::Session;
use driver::session;
use lib::llvm::llvm;
use lib::llvm::ModuleRef;
use lib;
use metadata::common::LinkMeta;
use metadata::{encoder, cstore, filesearch, csearch};
use middle::trans::context::CrateContext;
use middle::trans::common::gensym_name;
use middle::ty;
use util::ppaux;

use std::c_str::ToCStr;
use std::char;
use std::hash::Streaming;
use std::hash;
use std::os::consts::{macos, freebsd, linux, android, win32};
use std::ptr;
use std::run;
use std::str;
use std::io::fs;
use syntax::abi;
use syntax::ast;
use syntax::ast_map::{path, path_mod, path_name, path_pretty_name};
use syntax::attr;
use syntax::attr::{AttrMetaMethods};
use syntax::print::pprust;

#[deriving(Clone, Eq)]
pub enum output_type {
    output_type_none,
    output_type_bitcode,
    output_type_assembly,
    output_type_llvm_assembly,
    output_type_object,
    output_type_exe,
}

fn write_string<W:Writer>(writer: &mut W, string: &str) {
    writer.write(string.as_bytes());
}

pub fn llvm_err(sess: Session, msg: ~str) -> ! {
    unsafe {
        let cstr = llvm::LLVMRustGetLastError();
        if cstr == ptr::null() {
            sess.fatal(msg);
        } else {
            sess.fatal(msg + ": " + str::raw::from_c_str(cstr));
        }
    }
}

pub fn WriteOutputFile(
        sess: Session,
        Target: lib::llvm::TargetMachineRef,
        PM: lib::llvm::PassManagerRef,
        M: ModuleRef,
        Output: &Path,
        FileType: lib::llvm::FileType) {
    unsafe {
        Output.with_c_str(|Output| {
            let result = llvm::LLVMRustWriteOutputFile(
                    Target, PM, M, Output, FileType);
            if !result {
                llvm_err(sess, ~"Could not write output");
            }
        })
    }
}

pub mod jit {

    use back::link::llvm_err;
    use driver::session::Session;
    use lib::llvm::llvm;
    use lib::llvm::{ModuleRef, ContextRef, ExecutionEngineRef};

    use std::c_str::ToCStr;
    use std::cast;
    use std::local_data;
    use std::unstable::intrinsics;

    struct LLVMJITData {
        ee: ExecutionEngineRef,
        llcx: ContextRef
    }

    pub trait Engine {}
    impl Engine for LLVMJITData {}

    impl Drop for LLVMJITData {
        fn drop(&mut self) {
            unsafe {
                llvm::LLVMDisposeExecutionEngine(self.ee);
                llvm::LLVMContextDispose(self.llcx);
            }
        }
    }

    pub fn exec(sess: Session,
                c: ContextRef,
                m: ModuleRef,
                stacks: bool) {
        unsafe {
            let manager = llvm::LLVMRustPrepareJIT(intrinsics::morestack_addr());

            // We need to tell JIT where to resolve all linked
            // symbols from. The equivalent of -lstd, -lcore, etc.
            // By default the JIT will resolve symbols from the extra and
            // core linked into rustc. We don't want that,
            // incase the user wants to use an older extra library.

            // We custom-build a JIT execution engine via some rust wrappers
            // first. This wrappers takes ownership of the module passed in.
            let ee = llvm::LLVMRustBuildJIT(manager, m, stacks);
            if ee.is_null() {
                llvm::LLVMContextDispose(c);
                llvm_err(sess, ~"Could not create the JIT");
            }

            // Next, we need to get a handle on the _rust_main function by
            // looking up it's corresponding ValueRef and then requesting that
            // the execution engine compiles the function.
            let fun = "_rust_main".with_c_str(|entry| {
                llvm::LLVMGetNamedFunction(m, entry)
            });
            if fun.is_null() {
                llvm::LLVMDisposeExecutionEngine(ee);
                llvm::LLVMContextDispose(c);
                llvm_err(sess, ~"Could not find _rust_main in the JIT");
            }

            // Finally, once we have the pointer to the code, we can do some
            // closure magic here to turn it straight into a callable rust
            // closure
            let code = llvm::LLVMGetPointerToGlobal(ee, fun);
            assert!(!code.is_null());
            let func: extern "Rust" fn() = cast::transmute(code);
            func();

            // Currently there is no method of re-using the executing engine
            // from LLVM in another call to the JIT. While this kinda defeats
            // the purpose of having a JIT in the first place, there isn't
            // actually much code currently which would re-use data between
            // different invocations of this. Additionally, the compilation
            // model currently isn't designed to support this scenario.
            //
            // We can't destroy the engine/context immediately here, however,
            // because of annihilation. The JIT code contains drop glue for any
            // types defined in the crate we just ran, and if any of those boxes
            // are going to be dropped during annihilation, the drop glue must
            // be run. Hence, we need to transfer ownership of this jit engine
            // to the caller of this function. To be convenient for now, we
            // shove it into TLS and have someone else remove it later on.
            let data = ~LLVMJITData { ee: ee, llcx: c };
            set_engine(data as ~Engine);
        }
    }

    // The stage1 compiler won't work, but that doesn't really matter. TLS
    // changed only very recently to allow storage of owned values.
    local_data_key!(engine_key: ~Engine)

    fn set_engine(engine: ~Engine) {
        local_data::set(engine_key, engine)
    }

    pub fn consume_engine() -> Option<~Engine> {
        local_data::pop(engine_key)
    }
}

pub mod write {

    use back::link::jit;
    use back::link::{WriteOutputFile, output_type};
    use back::link::{output_type_assembly, output_type_bitcode};
    use back::link::{output_type_exe, output_type_llvm_assembly};
    use back::link::{output_type_object};
    use driver::session::Session;
    use driver::session;
    use lib::llvm::llvm;
    use lib::llvm::{ModuleRef, ContextRef};
    use lib;

    use std::c_str::ToCStr;
    use std::libc::{c_uint, c_int};
    use std::path::Path;
    use std::run;
    use std::str;

    pub fn run_passes(sess: Session,
                      llcx: ContextRef,
                      llmod: ModuleRef,
                      output_type: output_type,
                      output: &Path) {
        unsafe {
            llvm::LLVMInitializePasses();

            // Only initialize the platforms supported by Rust here, because
            // using --llvm-root will have multiple platforms that rustllvm
            // doesn't actually link to and it's pointless to put target info
            // into the registry that Rust can not generate machine code for.
            llvm::LLVMInitializeX86TargetInfo();
            llvm::LLVMInitializeX86Target();
            llvm::LLVMInitializeX86TargetMC();
            llvm::LLVMInitializeX86AsmPrinter();
            llvm::LLVMInitializeX86AsmParser();

            llvm::LLVMInitializeARMTargetInfo();
            llvm::LLVMInitializeARMTarget();
            llvm::LLVMInitializeARMTargetMC();
            llvm::LLVMInitializeARMAsmPrinter();
            llvm::LLVMInitializeARMAsmParser();

            llvm::LLVMInitializeMipsTargetInfo();
            llvm::LLVMInitializeMipsTarget();
            llvm::LLVMInitializeMipsTargetMC();
            llvm::LLVMInitializeMipsAsmPrinter();
            llvm::LLVMInitializeMipsAsmParser();

            if sess.opts.save_temps {
                output.with_extension("no-opt.bc").with_c_str(|buf| {
                    llvm::LLVMWriteBitcodeToFile(llmod, buf);
                })
            }

            configure_llvm(sess);

            let OptLevel = match sess.opts.optimize {
              session::No => lib::llvm::CodeGenLevelNone,
              session::Less => lib::llvm::CodeGenLevelLess,
              session::Default => lib::llvm::CodeGenLevelDefault,
              session::Aggressive => lib::llvm::CodeGenLevelAggressive,
            };
            let use_softfp = sess.opts.debugging_opts & session::use_softfp != 0;

            let tm = sess.targ_cfg.target_strs.target_triple.with_c_str(|T| {
                sess.opts.target_cpu.with_c_str(|CPU| {
                    sess.opts.target_feature.with_c_str(|Features| {
                        llvm::LLVMRustCreateTargetMachine(
                            T, CPU, Features,
                            lib::llvm::CodeModelDefault,
                            lib::llvm::RelocPIC,
                            OptLevel,
                            true,
                            use_softfp
                        )
                    })
                })
            });

            // Create the two optimizing pass managers. These mirror what clang
            // does, and are by populated by LLVM's default PassManagerBuilder.
            // Each manager has a different set of passes, but they also share
            // some common passes.
            let fpm = llvm::LLVMCreateFunctionPassManagerForModule(llmod);
            let mpm = llvm::LLVMCreatePassManager();

            // If we're verifying or linting, add them to the function pass
            // manager.
            let addpass = |pass: &str| {
                pass.with_c_str(|s| llvm::LLVMRustAddPass(fpm, s))
            };
            if !sess.no_verify() { assert!(addpass("verify")); }
            if sess.lint_llvm()  { assert!(addpass("lint"));   }

            if !sess.no_prepopulate_passes() {
                llvm::LLVMRustAddAnalysisPasses(tm, fpm, llmod);
                llvm::LLVMRustAddAnalysisPasses(tm, mpm, llmod);
                populate_llvm_passes(fpm, mpm, llmod, OptLevel);
            }

            for pass in sess.opts.custom_passes.iter() {
                pass.with_c_str(|s| {
                    if !llvm::LLVMRustAddPass(mpm, s) {
                        sess.warn(format!("Unknown pass {}, ignoring", *pass));
                    }
                })
            }

            // Finally, run the actual optimization passes
            llvm::LLVMRustRunFunctionPassManager(fpm, llmod);
            llvm::LLVMRunPassManager(mpm, llmod);

            // Deallocate managers that we're now done with
            llvm::LLVMDisposePassManager(fpm);
            llvm::LLVMDisposePassManager(mpm);

            if sess.opts.save_temps {
                output.with_extension("bc").with_c_str(|buf| {
                    llvm::LLVMWriteBitcodeToFile(llmod, buf);
                })
            }

            if sess.opts.jit {
                // If we are using JIT, go ahead and create and execute the
                // engine now. JIT execution takes ownership of the module and
                // context, so don't dispose
                jit::exec(sess, llcx, llmod, true);
            } else {
                // Create a codegen-specific pass manager to emit the actual
                // assembly or object files. This may not end up getting used,
                // but we make it anyway for good measure.
                let cpm = llvm::LLVMCreatePassManager();
                llvm::LLVMRustAddAnalysisPasses(tm, cpm, llmod);
                llvm::LLVMRustAddLibraryInfo(cpm, llmod);

                match output_type {
                    output_type_none => {}
                    output_type_bitcode => {
                        output.with_c_str(|buf| {
                            llvm::LLVMWriteBitcodeToFile(llmod, buf);
                        })
                    }
                    output_type_llvm_assembly => {
                        output.with_c_str(|output| {
                            llvm::LLVMRustPrintModule(cpm, llmod, output)
                        })
                    }
                    output_type_assembly => {
                        WriteOutputFile(sess, tm, cpm, llmod, output, lib::llvm::AssemblyFile);
                    }
                    output_type_exe | output_type_object => {
                        WriteOutputFile(sess, tm, cpm, llmod, output, lib::llvm::ObjectFile);
                    }
                }

                llvm::LLVMDisposePassManager(cpm);
            }

            llvm::LLVMRustDisposeTargetMachine(tm);
            // the jit takes ownership of these two items
            if !sess.opts.jit {
                llvm::LLVMDisposeModule(llmod);
                llvm::LLVMContextDispose(llcx);
            }
            if sess.time_llvm_passes() { llvm::LLVMRustPrintPassTimings(); }
        }
    }

    pub fn run_assembler(sess: Session, assembly: &Path, object: &Path) {
        let cc = super::get_cc_prog(sess);

        // FIXME (#9639): This needs to handle non-utf8 paths
        let args = [
            ~"-c",
            ~"-o", object.as_str().unwrap().to_owned(),
            assembly.as_str().unwrap().to_owned()];

        debug!("{} '{}'", cc, args.connect("' '"));
        let prog = run::process_output(cc, args);

        if !prog.status.success() {
            sess.err(format!("linking with `{}` failed: {}", cc, prog.status));
            sess.note(format!("{} arguments: '{}'", cc, args.connect("' '")));
            sess.note(str::from_utf8(prog.error + prog.output));
            sess.abort_if_errors();
        }
    }

    unsafe fn configure_llvm(sess: Session) {
        // Copy what clan does by turning on loop vectorization at O2 and
        // slp vectorization at O3
        let vectorize_loop = !sess.no_vectorize_loops() &&
                             (sess.opts.optimize == session::Default ||
                              sess.opts.optimize == session::Aggressive);
        let vectorize_slp = !sess.no_vectorize_slp() &&
                            sess.opts.optimize == session::Aggressive;

        let mut llvm_c_strs = ~[];
        let mut llvm_args = ~[];
        let add = |arg: &str| {
            let s = arg.to_c_str();
            llvm_args.push(s.with_ref(|p| p));
            llvm_c_strs.push(s);
        };
        add("rustc"); // fake program name
        add("-arm-enable-ehabi");
        add("-arm-enable-ehabi-descriptors");
        if vectorize_loop { add("-vectorize-loops"); }
        if vectorize_slp  { add("-vectorize-slp");   }
        if sess.time_llvm_passes() { add("-time-passes"); }
        if sess.print_llvm_passes() { add("-debug-pass=Structure"); }

        for arg in sess.opts.llvm_args.iter() {
            add(*arg);
        }

        llvm_args.as_imm_buf(|p, len| {
            llvm::LLVMRustSetLLVMOptions(len as c_int, p);
        })
    }

    unsafe fn populate_llvm_passes(fpm: lib::llvm::PassManagerRef,
                                   mpm: lib::llvm::PassManagerRef,
                                   llmod: ModuleRef,
                                   opt: lib::llvm::CodeGenOptLevel) {
        // Create the PassManagerBuilder for LLVM. We configure it with
        // reasonable defaults and prepare it to actually populate the pass
        // manager.
        let builder = llvm::LLVMPassManagerBuilderCreate();
        match opt {
            lib::llvm::CodeGenLevelNone => {
                // Don't add lifetime intrinsics add O0
                llvm::LLVMRustAddAlwaysInlinePass(builder, false);
            }
            lib::llvm::CodeGenLevelLess => {
                llvm::LLVMRustAddAlwaysInlinePass(builder, true);
            }
            // numeric values copied from clang
            lib::llvm::CodeGenLevelDefault => {
                llvm::LLVMPassManagerBuilderUseInlinerWithThreshold(builder,
                                                                    225);
            }
            lib::llvm::CodeGenLevelAggressive => {
                llvm::LLVMPassManagerBuilderUseInlinerWithThreshold(builder,
                                                                    275);
            }
        }
        llvm::LLVMPassManagerBuilderSetOptLevel(builder, opt as c_uint);
        llvm::LLVMRustAddBuilderLibraryInfo(builder, llmod);

        // Use the builder to populate the function/module pass managers.
        llvm::LLVMPassManagerBuilderPopulateFunctionPassManager(builder, fpm);
        llvm::LLVMPassManagerBuilderPopulateModulePassManager(builder, mpm);
        llvm::LLVMPassManagerBuilderDispose(builder);
    }
}


/*
 * Name mangling and its relationship to metadata. This is complex. Read
 * carefully.
 *
 * The semantic model of Rust linkage is, broadly, that "there's no global
 * namespace" between crates. Our aim is to preserve the illusion of this
 * model despite the fact that it's not *quite* possible to implement on
 * modern linkers. We initially didn't use system linkers at all, but have
 * been convinced of their utility.
 *
 * There are a few issues to handle:
 *
 *  - Linkers operate on a flat namespace, so we have to flatten names.
 *    We do this using the C++ namespace-mangling technique. Foo::bar
 *    symbols and such.
 *
 *  - Symbols with the same name but different types need to get different
 *    linkage-names. We do this by hashing a string-encoding of the type into
 *    a fixed-size (currently 16-byte hex) cryptographic hash function (CHF:
 *    we use SHA1) to "prevent collisions". This is not airtight but 16 hex
 *    digits on uniform probability means you're going to need 2**32 same-name
 *    symbols in the same process before you're even hitting birthday-paradox
 *    collision probability.
 *
 *  - Symbols in different crates but with same names "within" the crate need
 *    to get different linkage-names.
 *
 * So here is what we do:
 *
 *  - Separate the meta tags into two sets: exported and local. Only work with
 *    the exported ones when considering linkage.
 *
 *  - Consider two exported tags as special (and mandatory): name and vers.
 *    Every crate gets them; if it doesn't name them explicitly we infer them
 *    as basename(crate) and "0.1", respectively. Call these CNAME, CVERS.
 *
 *  - Define CMETA as all the non-name, non-vers exported meta tags in the
 *    crate (in sorted order).
 *
 *  - Define CMH as hash(CMETA + hashes of dependent crates).
 *
 *  - Compile our crate to lib CNAME-CMH-CVERS.so
 *
 *  - Define STH(sym) as hash(CNAME, CMH, type_str(sym))
 *
 *  - Suffix a mangled sym with ::STH@CVERS, so that it is unique in the
 *    name, non-name metadata, and type sense, and versioned in the way
 *    system linkers understand.
 *
 */

pub fn build_link_meta(sess: Session,
                       c: &ast::Crate,
                       output: &Path,
                       symbol_hasher: &mut hash::State)
                       -> LinkMeta {
    struct ProvidedMetas {
        name: Option<@str>,
        vers: Option<@str>,
        pkg_id: Option<@str>,
        cmh_items: ~[@ast::MetaItem]
    }

    fn provided_link_metas(sess: Session, c: &ast::Crate) ->
       ProvidedMetas {
        let mut name = None;
        let mut vers = None;
        let mut pkg_id = None;
        let mut cmh_items = ~[];
        let linkage_metas = attr::find_linkage_metas(c.attrs);
        attr::require_unique_names(sess.diagnostic(), linkage_metas);
        for meta in linkage_metas.iter() {
            match meta.name_str_pair() {
                Some((n, value)) if "name" == n => name = Some(value),
                Some((n, value)) if "vers" == n => vers = Some(value),
                Some((n, value)) if "package_id" == n => pkg_id = Some(value),
                _ => cmh_items.push(*meta)
            }
        }

        ProvidedMetas {
            name: name,
            vers: vers,
            pkg_id: pkg_id,
            cmh_items: cmh_items
        }
    }

    // This calculates CMH as defined above
    fn crate_meta_extras_hash(symbol_hasher: &mut hash::State,
                              cmh_items: ~[@ast::MetaItem],
                              dep_hashes: ~[@str],
                              pkg_id: Option<@str>) -> @str {
        fn len_and_str(s: &str) -> ~str {
            format!("{}_{}", s.len(), s)
        }

        fn len_and_str_lit(l: ast::lit) -> ~str {
            len_and_str(pprust::lit_to_str(@l))
        }

        let cmh_items = attr::sort_meta_items(cmh_items);

        fn hash(symbol_hasher: &mut hash::State, m: &@ast::MetaItem) {
            match m.node {
              ast::MetaNameValue(key, value) => {
                write_string(symbol_hasher, len_and_str(key));
                write_string(symbol_hasher, len_and_str_lit(value));
              }
              ast::MetaWord(name) => {
                write_string(symbol_hasher, len_and_str(name));
              }
              ast::MetaList(name, ref mis) => {
                write_string(symbol_hasher, len_and_str(name));
                for m_ in mis.iter() {
                    hash(symbol_hasher, m_);
                }
              }
            }
        }

        symbol_hasher.reset();
        for m in cmh_items.iter() {
            hash(symbol_hasher, m);
        }

        for dh in dep_hashes.iter() {
            write_string(symbol_hasher, len_and_str(*dh));
        }

        for p in pkg_id.iter() {
            write_string(symbol_hasher, len_and_str(*p));
        }

        return truncated_hash_result(symbol_hasher).to_managed();
    }

    fn warn_missing(sess: Session, name: &str, default: &str) {
        if !*sess.building_library { return; }
        sess.warn(format!("missing crate link meta `{}`, using `{}` as default",
                       name, default));
    }

    fn crate_meta_name(sess: Session, output: &Path, opt_name: Option<@str>)
        -> @str {
        match opt_name {
            Some(v) if !v.is_empty() => v,
            _ => {
                // to_managed could go away if there was a version of
                // filestem that returned an @str
                // FIXME (#9639): Non-utf8 filenames will give a misleading error
                let name = session::expect(sess,
                                           output.filestem_str(),
                                           || format!("output file name `{}` doesn't\
                                                    appear to have a stem",
                                                   output.display())).to_managed();
                if name.is_empty() {
                    sess.fatal("missing crate link meta `name`, and the \
                                inferred name is blank");
                }
                warn_missing(sess, "name", name);
                name
            }
        }
    }

    fn crate_meta_vers(sess: Session, opt_vers: Option<@str>) -> @str {
        match opt_vers {
            Some(v) if !v.is_empty() => v,
            _ => {
                let vers = @"0.0";
                warn_missing(sess, "vers", vers);
                vers
            }
        }
    }

    fn crate_meta_pkgid(sess: Session, name: @str, opt_pkg_id: Option<@str>)
        -> @str {
        match opt_pkg_id {
            Some(v) if !v.is_empty() => v,
            _ => {
                let pkg_id = name.clone();
                warn_missing(sess, "package_id", pkg_id);
                pkg_id
            }
        }
    }

    let ProvidedMetas {
        name: opt_name,
        vers: opt_vers,
        pkg_id: opt_pkg_id,
        cmh_items: cmh_items
    } = provided_link_metas(sess, c);
    let name = crate_meta_name(sess, output, opt_name);
    let vers = crate_meta_vers(sess, opt_vers);
    let pkg_id = crate_meta_pkgid(sess, name, opt_pkg_id);
    let dep_hashes = cstore::get_dep_hashes(sess.cstore);
    let extras_hash =
        crate_meta_extras_hash(symbol_hasher, cmh_items,
                               dep_hashes, Some(pkg_id));

    LinkMeta {
        name: name,
        vers: vers,
        package_id: Some(pkg_id),
        extras_hash: extras_hash
    }
}

pub fn truncated_hash_result(symbol_hasher: &mut hash::State) -> ~str {
    symbol_hasher.result_str()
}


// This calculates STH for a symbol, as defined above
pub fn symbol_hash(tcx: ty::ctxt,
                   symbol_hasher: &mut hash::State,
                   t: ty::t,
                   link_meta: LinkMeta) -> @str {
    // NB: do *not* use abbrevs here as we want the symbol names
    // to be independent of one another in the crate.

    symbol_hasher.reset();
    write_string(symbol_hasher, link_meta.name);
    write_string(symbol_hasher, "-");
    write_string(symbol_hasher, link_meta.extras_hash);
    write_string(symbol_hasher, "-");
    write_string(symbol_hasher, encoder::encoded_ty(tcx, t));
    let mut hash = truncated_hash_result(symbol_hasher);
    // Prefix with 'h' so that it never blends into adjacent digits
    hash.unshift_char('h');
    // tjc: allocation is unfortunate; need to change std::hash
    hash.to_managed()
}

pub fn get_symbol_hash(ccx: &mut CrateContext, t: ty::t) -> @str {
    match ccx.type_hashcodes.find(&t) {
      Some(&h) => h,
      None => {
        let hash = symbol_hash(ccx.tcx, &mut ccx.symbol_hasher, t, ccx.link_meta);
        ccx.type_hashcodes.insert(t, hash);
        hash
      }
    }
}


// Name sanitation. LLVM will happily accept identifiers with weird names, but
// gas doesn't!
// gas accepts the following characters in symbols: a-z, A-Z, 0-9, ., _, $
pub fn sanitize(s: &str) -> ~str {
    let mut result = ~"";
    for c in s.chars() {
        match c {
            // Escape these with $ sequences
            '@' => result.push_str("$SP$"),
            '~' => result.push_str("$UP$"),
            '*' => result.push_str("$RP$"),
            '&' => result.push_str("$BP$"),
            '<' => result.push_str("$LT$"),
            '>' => result.push_str("$GT$"),
            '(' => result.push_str("$LP$"),
            ')' => result.push_str("$RP$"),
            ',' => result.push_str("$C$"),

            // '.' doesn't occur in types and functions, so reuse it
            // for ':'
            ':' => result.push_char('.'),

            // These are legal symbols
            'a' .. 'z'
            | 'A' .. 'Z'
            | '0' .. '9'
            | '_' | '.' | '$' => result.push_char(c),

            _ => {
                let mut tstr = ~"";
                char::escape_unicode(c, |c| tstr.push_char(c));
                result.push_char('$');
                result.push_str(tstr.slice_from(1));
            }
        }
    }

    // Underscore-qualify anything that didn't start as an ident.
    if result.len() > 0u &&
        result[0] != '_' as u8 &&
        ! char::is_XID_start(result[0] as char) {
        return ~"_" + result;
    }

    return result;
}

pub fn mangle(sess: Session, ss: path,
              hash: Option<&str>, vers: Option<&str>) -> ~str {
    // Follow C++ namespace-mangling style, see
    // http://en.wikipedia.org/wiki/Name_mangling for more info.
    //
    // It turns out that on OSX you can actually have arbitrary symbols in
    // function names (at least when given to LLVM), but this is not possible
    // when using unix's linker. Perhaps one day when we just a linker from LLVM
    // we won't need to do this name mangling. The problem with name mangling is
    // that it seriously limits the available characters. For example we can't
    // have things like @T or ~[T] in symbol names when one would theoretically
    // want them for things like impls of traits on that type.
    //
    // To be able to work on all platforms and get *some* reasonable output, we
    // use C++ name-mangling.

    let mut n = ~"_ZN"; // _Z == Begin name-sequence, N == nested

    let push = |s: &str| {
        let sani = sanitize(s);
        n.push_str(format!("{}{}", sani.len(), sani));
    };

    // First, connect each component with <len, name> pairs.
    for s in ss.iter() {
        match *s {
            path_name(s) | path_mod(s) | path_pretty_name(s, _) => {
                push(sess.str_of(s))
            }
        }
    }

    // next, if any identifiers are "pretty" and need extra information tacked
    // on, then use the hash to generate two unique characters. For now
    // hopefully 2 characters is enough to avoid collisions.
    static EXTRA_CHARS: &'static str =
        "abcdefghijklmnopqrstuvwxyz\
         ABCDEFGHIJKLMNOPQRSTUVWXYZ\
         0123456789";
    let mut hash = match hash { Some(s) => s.to_owned(), None => ~"" };
    for s in ss.iter() {
        match *s {
            path_pretty_name(_, extra) => {
                let hi = (extra >> 32) as u32 as uint;
                let lo = extra as u32 as uint;
                hash.push_char(EXTRA_CHARS[hi % EXTRA_CHARS.len()] as char);
                hash.push_char(EXTRA_CHARS[lo % EXTRA_CHARS.len()] as char);
            }
            _ => {}
        }
    }
    if hash.len() > 0 {
        push(hash);
    }
    match vers {
        Some(s) => push(s),
        None => {}
    }

    n.push_char('E'); // End name-sequence.
    n
}

pub fn exported_name(sess: Session,
                     path: path,
                     hash: &str,
                     vers: &str) -> ~str {
    // The version will get mangled to have a leading '_', but it makes more
    // sense to lead with a 'v' b/c this is a version...
    let vers = if vers.len() > 0 && !char::is_XID_start(vers.char_at(0)) {
        "v" + vers
    } else {
        vers.to_owned()
    };

    mangle(sess, path, Some(hash), Some(vers.as_slice()))
}

pub fn mangle_exported_name(ccx: &mut CrateContext,
                            path: path,
                            t: ty::t) -> ~str {
    let hash = get_symbol_hash(ccx, t);
    return exported_name(ccx.sess, path,
                         hash,
                         ccx.link_meta.vers);
}

pub fn mangle_internal_name_by_type_only(ccx: &mut CrateContext,
                                         t: ty::t,
                                         name: &str) -> ~str {
    let s = ppaux::ty_to_short_str(ccx.tcx, t);
    let hash = get_symbol_hash(ccx, t);
    return mangle(ccx.sess,
                  ~[path_name(ccx.sess.ident_of(name)),
                    path_name(ccx.sess.ident_of(s))],
                  Some(hash.as_slice()),
                  None);
}

pub fn mangle_internal_name_by_type_and_seq(ccx: &mut CrateContext,
                                            t: ty::t,
                                            name: &str) -> ~str {
    let s = ppaux::ty_to_str(ccx.tcx, t);
    let hash = get_symbol_hash(ccx, t);
    let (_, name) = gensym_name(name);
    return mangle(ccx.sess,
                  ~[path_name(ccx.sess.ident_of(s)), name],
                  Some(hash.as_slice()),
                  None);
}

pub fn mangle_internal_name_by_path_and_seq(ccx: &mut CrateContext,
                                            mut path: path,
                                            flav: &str) -> ~str {
    let (_, name) = gensym_name(flav);
    path.push(name);
    mangle(ccx.sess, path, None, None)
}

pub fn mangle_internal_name_by_path(ccx: &mut CrateContext, path: path) -> ~str {
    mangle(ccx.sess, path, None, None)
}

pub fn output_lib_filename(lm: LinkMeta) -> ~str {
    format!("{}-{}-{}", lm.name, lm.extras_hash, lm.vers)
}

pub fn get_cc_prog(sess: Session) -> ~str {
    match sess.opts.linker {
        Some(ref linker) => return linker.to_owned(),
        None => {}
    }

    // In the future, FreeBSD will use clang as default compiler.
    // It would be flexible to use cc (system's default C compiler)
    // instead of hard-coded gcc.
    // For win32, there is no cc command, so we add a condition to make it use
    // g++.  We use g++ rather than gcc because it automatically adds linker
    // options required for generation of dll modules that correctly register
    // stack unwind tables.
    match sess.targ_cfg.os {
        abi::OsAndroid => match sess.opts.android_cross_path {
            Some(ref path) => format!("{}/bin/arm-linux-androideabi-gcc", *path),
            None => {
                sess.fatal("need Android NDK path for linking \
                            (--android-cross-path)")
            }
        },
        abi::OsWin32 => ~"g++",
        _ => ~"cc",
    }
}

/// Perform the linkage portion of the compilation phase. This will generate all
/// of the requested outputs for this compilation session.
pub fn link_binary(sess: Session,
                   crate_types: &[~str],
                   obj_filename: &Path,
                   out_filename: &Path,
                   lm: LinkMeta) {
    let outputs = if sess.opts.test {
        // If we're generating a test executable, then ignore all other output
        // styles at all other locations
        ~[session::OutputExecutable]
    } else {
        // Always generate whatever was specified on the command line, but also
        // look at what was in the crate file itself for generating output
        // formats.
        let mut outputs = sess.opts.outputs.clone();
        for ty in crate_types.iter() {
            if "bin" == *ty {
                outputs.push(session::OutputExecutable);
            } else if "dylib" == *ty || "lib" == *ty {
                outputs.push(session::OutputDylib);
            } else if "rlib" == *ty {
                outputs.push(session::OutputRlib);
            } else if "staticlib" == *ty {
                outputs.push(session::OutputStaticlib);
            }
        }
        if outputs.len() == 0 {
            outputs.push(session::OutputExecutable);
        }
        outputs
    };

    for output in outputs.move_iter() {
        link_binary_output(sess, output, obj_filename, out_filename, lm);
    }

    // Remove the temporary object file if we aren't saving temps
    if !sess.opts.save_temps {
        fs::unlink(obj_filename);
    }
}

fn is_writeable(p: &Path) -> bool {
    use std::io;

    match io::result(|| p.stat()) {
        Err(..) => true,
        Ok(m) => m.perm & io::UserWrite == io::UserWrite
    }
}

fn link_binary_output(sess: Session,
                      output: session::OutputStyle,
                      obj_filename: &Path,
                      out_filename: &Path,
                      lm: LinkMeta) {
    let libname = output_lib_filename(lm);
    let out_filename = match output {
        session::OutputRlib => {
            out_filename.with_filename(format!("lib{}.rlib", libname))
        }
        session::OutputDylib => {
            let (prefix, suffix) = match sess.targ_cfg.os {
                abi::OsWin32 => (win32::DLL_PREFIX, win32::DLL_SUFFIX),
                abi::OsMacos => (macos::DLL_PREFIX, macos::DLL_SUFFIX),
                abi::OsLinux => (linux::DLL_PREFIX, linux::DLL_SUFFIX),
                abi::OsAndroid => (android::DLL_PREFIX, android::DLL_SUFFIX),
                abi::OsFreebsd => (freebsd::DLL_PREFIX, freebsd::DLL_SUFFIX),
            };
            out_filename.with_filename(format!("{}{}{}", prefix, libname, suffix))
        }
        session::OutputStaticlib => {
            out_filename.with_filename(format!("lib{}.a", libname))
        }
        session::OutputExecutable => out_filename.clone(),
    };

    // Make sure the output and obj_filename are both writeable.
    // Mac, FreeBSD, and Windows system linkers check this already --
    // however, the Linux linker will happily overwrite a read-only file.
    // We should be consistent.
    let obj_is_writeable = is_writeable(obj_filename);
    let out_is_writeable = is_writeable(&out_filename);
    if !out_is_writeable {
        sess.fatal(format!("Output file {} is not writeable -- check its permissions.",
                           out_filename.display()));
    }
    else if !obj_is_writeable {
        sess.fatal(format!("Object file {} is not writeable -- check its permissions.",
                           obj_filename.display()));
    }

    match output {
        session::OutputRlib => {
            link_rlib(sess, obj_filename, &out_filename);
        }
        session::OutputStaticlib => {
            link_staticlib(sess, obj_filename, &out_filename);
        }
        session::OutputExecutable => {
            link_natively(sess, false, obj_filename, &out_filename);
        }
        session::OutputDylib => {
            link_natively(sess, true, obj_filename, &out_filename);
        }
    }
}

// Create an 'rlib'
//
// An rlib in its current incarnation is essentially a renamed .a file. The
// rlib primarily contains the object file of the crate, but it also contains
// all of the object files from native libraries. This is done by unzipping
// native libraries and inserting all of the contents into this archive.
fn link_rlib(sess: Session, obj_filename: &Path,
             out_filename: &Path) -> Archive {
    let mut a = Archive::create(sess, out_filename, obj_filename);
    for &(ref l, kind) in cstore::get_used_libraries(sess.cstore).iter() {
        match kind {
            cstore::NativeStatic => {
                a.add_native_library(l.as_slice());
            }
            cstore::NativeFramework | cstore::NativeUnknown => {}
        }
    }
    return a;
}

// Create a static archive
//
// This is essentially the same thing as an rlib, but it also involves adding
// all of the upstream crates' objects into the the archive. This will slurp in
// all of the native libraries of upstream dependencies as well.
//
// Additionally, there's no way for us to link dynamic libraries, so we warn
// about all dynamic library dependencies that they're not linked in.
fn link_staticlib(sess: Session, obj_filename: &Path, out_filename: &Path) {
    let mut a = link_rlib(sess, obj_filename, out_filename);
    a.add_native_library("morestack");

    let crates = cstore::get_used_crates(sess.cstore, cstore::RequireStatic);
    for &(cnum, ref path) in crates.iter() {
        let p = match *path {
            Some(ref p) => p.clone(), None => {
                sess.err(format!("could not find rlib for: `{}`",
                                 cstore::get_crate_data(sess.cstore, cnum).name));
                continue
            }
        };
        a.add_rlib(&p);
        let native_libs = csearch::get_native_libraries(sess.cstore, cnum);
        for &(kind, ref lib) in native_libs.iter() {
            let name = match kind {
                cstore::NativeStatic => "static library",
                cstore::NativeUnknown => "library",
                cstore::NativeFramework => "framework",
            };
            sess.warn(format!("unlinked native {}: {}", name, *lib));
        }
    }
}

// Create a dynamic library or executable
//
// This will invoke the system linker/cc to create the resulting file. This
// links to all upstream files as well.
fn link_natively(sess: Session, dylib: bool, obj_filename: &Path,
                 out_filename: &Path) {
    // The invocations of cc share some flags across platforms
    let cc_prog = get_cc_prog(sess);
    let mut cc_args = sess.targ_cfg.target_strs.cc_args.clone();
    cc_args.push_all_move(link_args(sess, dylib, obj_filename, out_filename));
    if (sess.opts.debugging_opts & session::print_link_args) != 0 {
        println!("{} link args: '{}'", cc_prog, cc_args.connect("' '"));
    }

    // May have not found libraries in the right formats.
    sess.abort_if_errors();

    // Invoke the system linker
    debug!("{} {}", cc_prog, cc_args.connect(" "));
    let prog = run::process_output(cc_prog, cc_args);

    if !prog.status.success() {
        sess.err(format!("linking with `{}` failed: {}", cc_prog, prog.status));
        sess.note(format!("{} arguments: '{}'", cc_prog, cc_args.connect("' '")));
        sess.note(str::from_utf8(prog.error + prog.output));
        sess.abort_if_errors();
    }


    // On OSX, debuggers need this utility to get run to do some munging of
    // the symbols
    if sess.targ_cfg.os == abi::OsMacos && sess.opts.debuginfo {
        // FIXME (#9639): This needs to handle non-utf8 paths
        run::process_status("dsymutil",
                            [out_filename.as_str().unwrap().to_owned()]);
    }
}

fn link_args(sess: Session,
             dylib: bool,
             obj_filename: &Path,
             out_filename: &Path) -> ~[~str] {

    // The default library location, we need this to find the runtime.
    // The location of crates will be determined as needed.
    // FIXME (#9639): This needs to handle non-utf8 paths
    let lib_path = sess.filesearch.get_target_lib_path();
    let stage: ~str = ~"-L" + lib_path.as_str().unwrap();

    let mut args = ~[stage];

    // FIXME (#9639): This needs to handle non-utf8 paths
    args.push_all([
        ~"-o", out_filename.as_str().unwrap().to_owned(),
        obj_filename.as_str().unwrap().to_owned()]);

    if sess.targ_cfg.os == abi::OsLinux {
        // GNU-style linkers will use this to omit linking to libraries which
        // don't actually fulfill any relocations, but only for libraries which
        // follow this flag. Thus, use it before specifing libraries to link to.
        args.push(~"-Wl,--as-needed");

        // GNU-style linkers support optimization with -O. --gc-sections
        // removes metadata and potentially other useful things, so don't
        // include it. GNU ld doesn't need a numeric argument, but other linkers
        // do.
        if sess.opts.optimize == session::Default ||
           sess.opts.optimize == session::Aggressive {
            args.push(~"-Wl,-O1");
        }
    }

    add_upstream_rust_crates(&mut args, sess, dylib);
    add_local_native_libraries(&mut args, sess);

    // # Telling the linker what we're doing

    if dylib {
        // On mac we need to tell the linker to let this library be rpathed
        if sess.targ_cfg.os == abi::OsMacos {
            args.push(~"-dynamiclib");
            args.push(~"-Wl,-dylib");
            // FIXME (#9639): This needs to handle non-utf8 paths
            args.push(~"-Wl,-install_name,@rpath/" +
                      out_filename.filename_str().unwrap());
        } else {
            args.push(~"-shared")
        }
    }

    if sess.targ_cfg.os == abi::OsFreebsd {
        args.push_all([~"-L/usr/local/lib",
                       ~"-L/usr/local/lib/gcc46",
                       ~"-L/usr/local/lib/gcc44"]);
    }

    // Stack growth requires statically linking a __morestack function
    args.push(~"-lmorestack");

    // FIXME (#2397): At some point we want to rpath our guesses as to
    // where extern libraries might live, based on the
    // addl_lib_search_paths
    args.push_all(rpath::get_rpath_flags(sess, out_filename));

    // Finally add all the linker arguments provided on the command line along
    // with any #[link_args] attributes found inside the crate
    args.push_all(sess.opts.linker_args);
    for arg in cstore::get_used_link_args(sess.cstore).iter() {
        args.push(arg.clone());
    }
    return args;
}

// # Rust Crate linking
//
// Rust crates are not considered at all when creating an rlib output. All
// dependencies will be linked when producing the final output (instead of
// the intermediate rlib version)
fn add_upstream_rust_crates(args: &mut ~[~str], sess: Session,
                            dylib: bool) {
    // Converts a library file-stem into a cc -l argument
    fn unlib(config: @session::config, stem: &str) -> ~str {
        if stem.starts_with("lib") &&
            config.os != abi::OsWin32 {
            stem.slice(3, stem.len()).to_owned()
        } else {
            stem.to_owned()
        }
    }

    let cstore = sess.cstore;
    if !dylib && !sess.prefer_dynamic() {
        // With an executable, things get a little interesting. As a limitation
        // of the current implementation, we require that everything must be
        // static, or everything must be dynamic. The reasons for this are a
        // little subtle, but as with the above two cases, the goal is to
        // prevent duplicate copies of the same library showing up. For example,
        // a static immediate dependency might show up as an upstream dynamic
        // dependency and we currently have no way of knowing that. We know that
        // all dynamic libaries require dynamic dependencies (see above), so
        // it's satisfactory to include either all static libraries or all
        // dynamic libraries.
        let crates = cstore::get_used_crates(cstore,
                                             cstore::RequireStatic);
        if crates.iter().all(|&(_, ref p)| p.is_some()) {
            for &(cnum, ref path) in crates.iter() {
                let cratepath = path.clone().unwrap();

                // If we're linking to the static version of the crate, then
                // we're mostly good to go. The caveat here is that we need to
                // pull in the static crate's native dependencies.
                args.push(cratepath.as_str().unwrap().to_owned());

                let libs = csearch::get_native_libraries(sess.cstore, cnum);
                for &(kind, ref lib) in libs.iter() {
                    match kind {
                        cstore::NativeUnknown => args.push("-l" + *lib),
                        cstore::NativeFramework => {
                            args.push(~"-framework");
                            args.push(lib.to_owned());
                        }
                        cstore::NativeStatic => {
                            sess.bug("statics shouldn't be propagated");
                        }
                    }
                }
            }
            return;
        }
    }

    // This is a fallback of three different  cases of linking:
    //
    // * When creating a dynamic library, all inputs are required to be dynamic
    //   as well
    // * If an executable is created with a preference on dynamic linking, then
    //   this case is the fallback
    // * If an executable is being created, and one of the inputs is missing as
    //   a static library, then this is the fallback case.
    let crates = cstore::get_used_crates(cstore, cstore::RequireDynamic);
    for &(cnum, ref path) in crates.iter() {
        let cratepath = match *path {
            Some(ref p) => p.clone(),
            None => {
                sess.err(format!("could not find dynamic library for: `{}`",
                                 cstore::get_crate_data(sess.cstore, cnum).name));
                return
            }
        };
        // Just need to tell the linker about where the library lives and what
        // its name is
        let dir = cratepath.dirname_str().unwrap();
        if !dir.is_empty() { args.push("-L" + dir); }
        let libarg = unlib(sess.targ_cfg, cratepath.filestem_str().unwrap());
        args.push("-l" + libarg);
    }
}

// # Native library linking
//
// User-supplied library search paths (-L on the cammand line) These are
// the same paths used to find Rust crates, so some of them may have been
// added already by the previous crate linking code. This only allows them
// to be found at compile time so it is still entirely up to outside
// forces to make sure that library can be found at runtime.
//
// Also note that the native libraries linked here are only the ones located
// in the current crate. Upstream crates with native library dependencies
// may have their native library pulled in above.
fn add_local_native_libraries(args: &mut ~[~str], sess: Session) {
    for path in sess.opts.addl_lib_search_paths.iter() {
        // FIXME (#9639): This needs to handle non-utf8 paths
        args.push("-L" + path.as_str().unwrap().to_owned());
    }

    let rustpath = filesearch::rust_path();
    for path in rustpath.iter() {
        // FIXME (#9639): This needs to handle non-utf8 paths
        args.push("-L" + path.as_str().unwrap().to_owned());
    }

    for &(ref l, kind) in cstore::get_used_libraries(sess.cstore).iter() {
        match kind {
            cstore::NativeUnknown | cstore::NativeStatic => {
                args.push("-l" + *l);
            }
            cstore::NativeFramework => {
                args.push(~"-framework");
                args.push(l.to_owned());
            }
        }
    }
}
