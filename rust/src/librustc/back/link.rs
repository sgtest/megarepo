// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use back::rpath;
use driver::session;
use lib::llvm::llvm;
use lib::llvm::{ModuleRef, mk_pass_manager, mk_target_data, True, False};
use lib::llvm::{PassManagerRef, FileType};
use lib;
use metadata::common::link_meta;
use metadata::filesearch;
use metadata::{encoder, cstore};
use middle::trans::common::crate_ctxt;
use middle::ty;
use session::Session;
use session;
use util::ppaux;

use core::char;
use core::cmp;
use core::hash;
use core::io::{Writer, WriterUtil};
use core::libc::{c_int, c_uint, c_char};
use core::os::consts::{macos, freebsd, linux, android, win32};
use core::os;
use core::ptr;
use core::run;
use core::str;
use core::vec;
use std::map::HashMap;
use std::sha1::sha1;
use syntax::ast;
use syntax::ast_map::{path, path_mod, path_name};
use syntax::attr;
use syntax::print::pprust;

enum output_type {
    output_type_none,
    output_type_bitcode,
    output_type_assembly,
    output_type_llvm_assembly,
    output_type_object,
    output_type_exe,
}

impl output_type : cmp::Eq {
    pure fn eq(&self, other: &output_type) -> bool {
        ((*self) as uint) == ((*other) as uint)
    }
    pure fn ne(&self, other: &output_type) -> bool { !(*self).eq(other) }
}

pub fn llvm_err(sess: Session, +msg: ~str) -> ! unsafe {
    let cstr = llvm::LLVMRustGetLastError();
    if cstr == ptr::null() {
        sess.fatal(msg);
    } else {
        sess.fatal(msg + ~": " + str::raw::from_c_str(cstr));
    }
}

pub fn WriteOutputFile(sess: Session,
        PM: lib::llvm::PassManagerRef, M: ModuleRef,
        Triple: *c_char,
        // FIXME: When #2334 is fixed, change
        // c_uint to FileType
        Output: *c_char, FileType: c_uint,
        OptLevel: c_int,
        EnableSegmentedStacks: bool) {
    unsafe {
        let result = llvm::LLVMRustWriteOutputFile(
                PM,
                M,
                Triple,
                Output,
                FileType,
                OptLevel,
                EnableSegmentedStacks);
        if (!result) {
            llvm_err(sess, ~"Could not write output");
        }
    }
}

pub mod jit {
    #[legacy_exports];

    use back::link::llvm_err;
    use lib::llvm::llvm;
    use lib::llvm::{ModuleRef, PassManagerRef, mk_target_data};
    use metadata::cstore;
    use session::Session;

    use core::cast;
    use core::libc::c_int;
    use core::ptr;
    use core::str;

    #[nolink]
    #[abi = "rust-intrinsic"]
    extern mod rusti {
        #[legacy_exports];
        fn morestack_addr() -> *();
    }

    struct Closure {
        code: *(),
        env: *(),
    }

    fn exec(sess: Session,
            pm: PassManagerRef,
            m: ModuleRef,
            opt: c_int,
            stacks: bool) unsafe {
        let manager = llvm::LLVMRustPrepareJIT(rusti::morestack_addr());

        // We need to tell JIT where to resolve all linked
        // symbols from. The equivalent of -lstd, -lcore, etc.
        // By default the JIT will resolve symbols from the std and
        // core linked into rustc. We don't want that,
        // incase the user wants to use an older std library.

        let cstore = sess.cstore;
        for cstore::get_used_crate_files(cstore).each |cratepath| {
            let path = cratepath.to_str();

            debug!("linking: %s", path);

            let _: () = str::as_c_str(
                path,
                |buf_t| {
                    if !llvm::LLVMRustLoadCrate(manager, buf_t) {
                        llvm_err(sess, ~"Could not link");
                    }
                    debug!("linked: %s", path);
                });
        }

        // The execute function will return a void pointer
        // to the _rust_main function. We can do closure
        // magic here to turn it straight into a callable rust
        // closure. It will also cleanup the memory manager
        // for us.

        let entry = llvm::LLVMRustExecuteJIT(manager,
                                             pm, m, opt, stacks);

        if ptr::is_null(entry) {
            llvm_err(sess, ~"Could not JIT");
        } else {
            let closure = Closure {
                code: entry,
                env: ptr::null()
            };
            let func: fn(++argv: ~[~str]) = cast::transmute(move closure);

            func(~[/*bad*/copy sess.opts.binary]);
        }
    }
}

mod write {
    #[legacy_exports];

    use back::link::jit;
    use back::link::{ModuleRef, WriteOutputFile, output_type};
    use back::link::{output_type_assembly, output_type_bitcode};
    use back::link::{output_type_exe, output_type_llvm_assembly};
    use back::link::{output_type_object};
    use driver::session;
    use lib::llvm::llvm;
    use lib::llvm::{False, True, mk_pass_manager, mk_target_data};
    use lib;
    use session::Session;

    use core::char;
    use core::libc::{c_char, c_int, c_uint};
    use core::path::Path;
    use core::str;
    use core::vec;

    fn is_object_or_assembly_or_exe(ot: output_type) -> bool {
        if ot == output_type_assembly || ot == output_type_object ||
               ot == output_type_exe {
            return true;
        }
        return false;
    }

    fn run_passes(sess: Session, llmod: ModuleRef, output: &Path) {
        unsafe {
            let opts = sess.opts;
            if sess.time_llvm_passes() { llvm::LLVMRustEnableTimePasses(); }
            let mut pm = mk_pass_manager();
            let td = mk_target_data(
                /*bad*/copy sess.targ_cfg.target_strs.data_layout);
            llvm::LLVMAddTargetData(td.lltd, pm.llpm);
            // FIXME (#2812): run the linter here also, once there are llvm-c
            // bindings for it.

            // Generate a pre-optimization intermediate file if -save-temps
            // was specified.


            if opts.save_temps {
                match opts.output_type {
                  output_type_bitcode => {
                    if opts.optimize != session::No {
                        let filename = output.with_filetype("no-opt.bc");
                        str::as_c_str(filename.to_str(), |buf| {
                            llvm::LLVMWriteBitcodeToFile(llmod, buf)
                        });
                    }
                  }
                  _ => {
                    let filename = output.with_filetype("bc");
                    str::as_c_str(filename.to_str(), |buf| {
                        llvm::LLVMWriteBitcodeToFile(llmod, buf)
                    });
                  }
                }
            }
            if !sess.no_verify() { llvm::LLVMAddVerifierPass(pm.llpm); }
            // FIXME (#2396): This is mostly a copy of the bits of opt's -O2
            // that are available in the C api.
            // Also: We might want to add optimization levels like -O1, -O2,
            // -Os, etc
            // Also: Should we expose and use the pass lists used by the opt
            // tool?

            if opts.optimize != session::No {
                let fpm = mk_pass_manager();
                llvm::LLVMAddTargetData(td.lltd, fpm.llpm);

                let FPMB = llvm::LLVMPassManagerBuilderCreate();
                llvm::LLVMPassManagerBuilderSetOptLevel(FPMB, 2u as c_uint);
                llvm::LLVMPassManagerBuilderPopulateFunctionPassManager(
                    FPMB, fpm.llpm);
                llvm::LLVMPassManagerBuilderDispose(FPMB);

                llvm::LLVMRunPassManager(fpm.llpm, llmod);
                let mut threshold = 225;
                if opts.optimize == session::Aggressive { threshold = 275; }

                let MPMB = llvm::LLVMPassManagerBuilderCreate();
                llvm::LLVMPassManagerBuilderSetOptLevel(MPMB,
                                                        opts.optimize as
                                                            c_uint);
                llvm::LLVMPassManagerBuilderSetSizeLevel(MPMB, False);
                llvm::LLVMPassManagerBuilderSetDisableUnitAtATime(MPMB,
                                                                  False);
                llvm::LLVMPassManagerBuilderSetDisableUnrollLoops(MPMB,
                                                                  False);
                llvm::LLVMPassManagerBuilderSetDisableSimplifyLibCalls(MPMB,
                                                                       False);

                if threshold != 0u {
                    llvm::LLVMPassManagerBuilderUseInlinerWithThreshold
                        (MPMB, threshold as c_uint);
                }
                llvm::LLVMPassManagerBuilderPopulateModulePassManager(
                    MPMB, pm.llpm);

                llvm::LLVMPassManagerBuilderDispose(MPMB);
            }
            if !sess.no_verify() { llvm::LLVMAddVerifierPass(pm.llpm); }
            if is_object_or_assembly_or_exe(opts.output_type) || opts.jit {
                let LLVMOptNone       = 0 as c_int; // -O0
                let LLVMOptLess       = 1 as c_int; // -O1
                let LLVMOptDefault    = 2 as c_int; // -O2, -Os
                let LLVMOptAggressive = 3 as c_int; // -O3

                let mut CodeGenOptLevel = match opts.optimize {
                  session::No => LLVMOptNone,
                  session::Less => LLVMOptLess,
                  session::Default => LLVMOptDefault,
                  session::Aggressive => LLVMOptAggressive
                };

                if opts.jit {
                    // If we are using JIT, go ahead and create and
                    // execute the engine now.
                    // JIT execution takes ownership of the module,
                    // so don't dispose and return.

                    jit::exec(sess, pm.llpm, llmod, CodeGenOptLevel, true);

                    if sess.time_llvm_passes() {
                        llvm::LLVMRustPrintPassTimings();
                    }
                    return;
                }

                let mut FileType;
                if opts.output_type == output_type_object ||
                       opts.output_type == output_type_exe {
                   FileType = lib::llvm::ObjectFile;
                } else { FileType = lib::llvm::AssemblyFile; }
                // Write optimized bitcode if --save-temps was on.

                if opts.save_temps {
                    // Always output the bitcode file with --save-temps

                    let filename = output.with_filetype("opt.bc");
                    llvm::LLVMRunPassManager(pm.llpm, llmod);
                    str::as_c_str(filename.to_str(), |buf| {
                        llvm::LLVMWriteBitcodeToFile(llmod, buf)
                    });
                    pm = mk_pass_manager();
                    // Save the assembly file if -S is used

                    if opts.output_type == output_type_assembly {
                        let _: () = str::as_c_str(
                            sess.targ_cfg.target_strs.target_triple,
                            |buf_t| {
                                str::as_c_str(output.to_str(), |buf_o| {
                                    WriteOutputFile(
                                        sess,
                                        pm.llpm,
                                        llmod,
                                        buf_t,
                                        buf_o,
                                        lib::llvm::AssemblyFile as c_uint,
                                        CodeGenOptLevel,
                                        true)
                                })
                            });
                    }


                    // Save the object file for -c or --save-temps alone
                    // This .o is needed when an exe is built
                    if opts.output_type == output_type_object ||
                           opts.output_type == output_type_exe {
                        let _: () = str::as_c_str(
                            sess.targ_cfg.target_strs.target_triple,
                            |buf_t| {
                                str::as_c_str(output.to_str(), |buf_o| {
                                    WriteOutputFile(
                                        sess,
                                        pm.llpm,
                                        llmod,
                                        buf_t,
                                        buf_o,
                                        lib::llvm::ObjectFile as c_uint,
                                        CodeGenOptLevel,
                                        true)
                                })
                            });
                    }
                } else {
                    // If we aren't saving temps then just output the file
                    // type corresponding to the '-c' or '-S' flag used

                    let _: () = str::as_c_str(
                        sess.targ_cfg.target_strs.target_triple,
                        |buf_t| {
                            str::as_c_str(output.to_str(), |buf_o| {
                                WriteOutputFile(
                                    sess,
                                    pm.llpm,
                                    llmod,
                                    buf_t,
                                    buf_o,
                                    FileType as c_uint,
                                    CodeGenOptLevel,
                                    true)
                            })
                        });
                }
                // Clean up and return

                llvm::LLVMDisposeModule(llmod);
                if sess.time_llvm_passes() {
                    llvm::LLVMRustPrintPassTimings();
                }
                return;
            }

            if opts.output_type == output_type_llvm_assembly {
                // Given options "-S --emit-llvm": output LLVM assembly
                str::as_c_str(output.to_str(), |buf_o| {
                    llvm::LLVMRustAddPrintModulePass(pm.llpm, llmod, buf_o)});
            } else {
                // If only a bitcode file is asked for by using the
                // '--emit-llvm' flag, then output it here
                llvm::LLVMRunPassManager(pm.llpm, llmod);
                str::as_c_str(output.to_str(),
                            |buf| llvm::LLVMWriteBitcodeToFile(llmod, buf) );
            }

            llvm::LLVMDisposeModule(llmod);
            if sess.time_llvm_passes() { llvm::LLVMRustPrintPassTimings(); }
        }
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

fn build_link_meta(sess: Session, c: ast::crate, output: &Path,
                   symbol_hasher: &hash::State) -> link_meta {

    type provided_metas =
        {name: Option<~str>,
         vers: Option<~str>,
         cmh_items: ~[@ast::meta_item]};

    fn provided_link_metas(sess: Session, c: ast::crate) ->
       provided_metas {
        let mut name: Option<~str> = None;
        let mut vers: Option<~str> = None;
        let mut cmh_items: ~[@ast::meta_item] = ~[];
        let linkage_metas =
            attr::find_linkage_metas(/*bad*/copy c.node.attrs);
        // XXX: Bad copy.
        attr::require_unique_names(sess.diagnostic(), copy linkage_metas);
        for linkage_metas.each |meta| {
            if attr::get_meta_item_name(*meta) == ~"name" {
                match attr::get_meta_item_value_str(*meta) {
                  Some(ref v) => { name = Some((/*bad*/copy *v)); }
                  None => cmh_items.push(*meta)
                }
            } else if attr::get_meta_item_name(*meta) == ~"vers" {
                match attr::get_meta_item_value_str(*meta) {
                  Some(ref v) => { vers = Some((/*bad*/copy *v)); }
                  None => cmh_items.push(*meta)
                }
            } else { cmh_items.push(*meta); }
        }
        return {name: name, vers: vers, cmh_items: cmh_items};
    }

    // This calculates CMH as defined above
    fn crate_meta_extras_hash(symbol_hasher: &hash::State,
                              _crate: ast::crate,
                              metas: provided_metas,
                              dep_hashes: ~[~str]) -> ~str {
        fn len_and_str(s: ~str) -> ~str {
            return fmt!("%u_%s", str::len(s), s);
        }

        fn len_and_str_lit(l: ast::lit) -> ~str {
            return len_and_str(pprust::lit_to_str(@l));
        }

        let cmh_items = attr::sort_meta_items(/*bad*/copy metas.cmh_items);

        symbol_hasher.reset();
        for cmh_items.each |m| {
            match m.node {
              ast::meta_name_value(ref key, value) => {
                symbol_hasher.write_str(len_and_str((*key)));
                symbol_hasher.write_str(len_and_str_lit(value));
              }
              ast::meta_word(ref name) => {
                symbol_hasher.write_str(len_and_str((*name)));
              }
              ast::meta_list(_, _) => {
                // FIXME (#607): Implement this
                fail ~"unimplemented meta_item variant";
              }
            }
        }

        for dep_hashes.each |dh| {
            symbol_hasher.write_str(len_and_str(*dh));
        }

        return truncated_hash_result(symbol_hasher);
    }

    fn warn_missing(sess: Session, name: ~str, default: ~str) {
        if !sess.building_library { return; }
        sess.warn(fmt!("missing crate link meta `%s`, using `%s` as default",
                       name, default));
    }

    fn crate_meta_name(sess: Session, _crate: ast::crate,
                       output: &Path, metas: provided_metas) -> ~str {
        return match metas.name {
              Some(ref v) => (/*bad*/copy *v),
              None => {
                let name = match output.filestem() {
                  None => sess.fatal(fmt!("output file name `%s` doesn't\
                                           appear to have a stem",
                                          output.to_str())),
                  Some(ref s) => (/*bad*/copy *s)
                };
                // XXX: Bad copy.
                warn_missing(sess, ~"name", copy name);
                name
              }
            };
    }

    fn crate_meta_vers(sess: Session, _crate: ast::crate,
                       metas: provided_metas) -> ~str {
        return match metas.vers {
              Some(ref v) => (/*bad*/copy *v),
              None => {
                let vers = ~"0.0";
                // Bad copy.
                warn_missing(sess, ~"vers", copy vers);
                vers
              }
            };
    }

    let provided_metas = provided_link_metas(sess, c);
    let name = crate_meta_name(sess, c, output, provided_metas);
    let vers = crate_meta_vers(sess, c, provided_metas);
    let dep_hashes = cstore::get_dep_hashes(sess.cstore);
    let extras_hash =
        crate_meta_extras_hash(symbol_hasher, c, provided_metas, dep_hashes);

    return {name: name, vers: vers, extras_hash: extras_hash};
}

fn truncated_hash_result(symbol_hasher: &hash::State) -> ~str unsafe {
    symbol_hasher.result_str()
}


// This calculates STH for a symbol, as defined above
fn symbol_hash(tcx: ty::ctxt, symbol_hasher: &hash::State, t: ty::t,
               link_meta: link_meta) -> ~str {
    // NB: do *not* use abbrevs here as we want the symbol names
    // to be independent of one another in the crate.

    symbol_hasher.reset();
    symbol_hasher.write_str(link_meta.name);
    symbol_hasher.write_str(~"-");
    symbol_hasher.write_str(link_meta.extras_hash);
    symbol_hasher.write_str(~"-");
    symbol_hasher.write_str(encoder::encoded_ty(tcx, t));
    let hash = truncated_hash_result(symbol_hasher);
    // Prefix with _ so that it never blends into adjacent digits

    return ~"_" + hash;
}

fn get_symbol_hash(ccx: @crate_ctxt, t: ty::t) -> ~str {
    match ccx.type_hashcodes.find(t) {
      Some(ref h) => return (/*bad*/copy *h),
      None => {
        let hash = symbol_hash(ccx.tcx, ccx.symbol_hasher, t, ccx.link_meta);
        // XXX: Bad copy. Prefer `@str`?
        ccx.type_hashcodes.insert(t, copy hash);
        return hash;
      }
    }
}


// Name sanitation. LLVM will happily accept identifiers with weird names, but
// gas doesn't!
fn sanitize(s: ~str) -> ~str {
    let mut result = ~"";
    for str::chars_each(s) |c| {
        match c {
          '@' => result += ~"_sbox_",
          '~' => result += ~"_ubox_",
          '*' => result += ~"_ptr_",
          '&' => result += ~"_ref_",
          ',' => result += ~"_",

          '{' | '(' => result += ~"_of_",
          'a' .. 'z'
          | 'A' .. 'Z'
          | '0' .. '9'
          | '_' => str::push_char(&mut result, c),
          _ => {
            if c > 'z' && char::is_XID_continue(c) {
                str::push_char(&mut result, c);
            }
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

fn mangle(sess: Session, ss: path) -> ~str {
    // Follow C++ namespace-mangling style

    let mut n = ~"_ZN"; // Begin name-sequence.

    for ss.each |s| {
        match *s { path_name(s) | path_mod(s) => {
          let sani = sanitize(sess.str_of(s));
          n += fmt!("%u%s", str::len(sani), sani);
        } }
    }
    n += ~"E"; // End name-sequence.
    n
}

fn exported_name(sess: Session,
                 +path: path,
                 +hash: ~str,
                 +vers: ~str) -> ~str {
    return mangle(sess,
                  vec::append_one(
                      vec::append_one(path, path_name(sess.ident_of(hash))),
                      path_name(sess.ident_of(vers))));
}

fn mangle_exported_name(ccx: @crate_ctxt, +path: path, t: ty::t) -> ~str {
    let hash = get_symbol_hash(ccx, t);
    return exported_name(ccx.sess, path,
                         hash,
                         /*bad*/copy ccx.link_meta.vers);
}

fn mangle_internal_name_by_type_only(ccx: @crate_ctxt,
                                     t: ty::t,
                                     +name: ~str) -> ~str {
    let s = ppaux::ty_to_short_str(ccx.tcx, t);
    let hash = get_symbol_hash(ccx, t);
    return mangle(ccx.sess,
                  ~[path_name(ccx.sess.ident_of(name)),
                    path_name(ccx.sess.ident_of(s)),
                    path_name(ccx.sess.ident_of(hash))]);
}

fn mangle_internal_name_by_path_and_seq(ccx: @crate_ctxt,
                                        +path: path,
                                        +flav: ~str) -> ~str {
    return mangle(ccx.sess,
                  vec::append_one(path, path_name((ccx.names)(flav))));
}

fn mangle_internal_name_by_path(ccx: @crate_ctxt, +path: path) -> ~str {
    return mangle(ccx.sess, path);
}

fn mangle_internal_name_by_seq(ccx: @crate_ctxt, +flav: ~str) -> ~str {
    return fmt!("%s_%u", flav, (ccx.names)(flav).repr);
}


fn output_dll_filename(os: session::os, lm: &link_meta) -> ~str {
    let libname = fmt!("%s-%s-%s", lm.name, lm.extras_hash, lm.vers);
    let (dll_prefix, dll_suffix) = match os {
        session::os_win32 => (win32::DLL_PREFIX, win32::DLL_SUFFIX),
        session::os_macos => (macos::DLL_PREFIX, macos::DLL_SUFFIX),
        session::os_linux => (linux::DLL_PREFIX, linux::DLL_SUFFIX),
        session::os_android => (android::DLL_PREFIX, android::DLL_SUFFIX),
        session::os_freebsd => (freebsd::DLL_PREFIX, freebsd::DLL_SUFFIX),
    };
    return str::from_slice(dll_prefix) + libname +
           str::from_slice(dll_suffix);
}

// If the user wants an exe generated we need to invoke
// cc to link the object file with some libs
fn link_binary(sess: Session,
               obj_filename: &Path,
               out_filename: &Path,
               lm: link_meta) {
    // Converts a library file-stem into a cc -l argument
    fn unlib(config: @session::config, +stem: ~str) -> ~str {
        if stem.starts_with("lib") &&
            config.os != session::os_win32 {
            stem.slice(3, stem.len())
        } else {
            stem
        }
    }

    let output = if sess.building_library {
        let long_libname = output_dll_filename(sess.targ_cfg.os, &lm);
        debug!("link_meta.name:  %s", lm.name);
        debug!("long_libname: %s", long_libname);
        debug!("out_filename: %s", out_filename.to_str());
        debug!("dirname(out_filename): %s", out_filename.dir_path().to_str());

        out_filename.dir_path().push(long_libname)
    } else {
        /*bad*/copy *out_filename
    };

    log(debug, ~"output: " + output.to_str());

    // The default library location, we need this to find the runtime.
    // The location of crates will be determined as needed.
    let stage: ~str = ~"-L" + sess.filesearch.get_target_lib_path().to_str();

    // In the future, FreeBSD will use clang as default compiler.
    // It would be flexible to use cc (system's default C compiler)
    // instead of hard-coded gcc.
    // For win32, there is no cc command,
    // so we add a condition to make it use gcc.
    let cc_prog: ~str =
        if sess.targ_cfg.os == session::os_android {
            ~"arm-linux-androideabi-g++"
        } else if sess.targ_cfg.os == session::os_win32 { ~"gcc" }
        else { ~"cc" };
    // The invocations of cc share some flags across platforms

    let mut cc_args =
        vec::append(~[stage], sess.targ_cfg.target_strs.cc_args);
    cc_args.push(~"-o");
    cc_args.push(output.to_str());
    cc_args.push(obj_filename.to_str());

    let mut lib_cmd;
    let os = sess.targ_cfg.os;
    if os == session::os_macos {
        lib_cmd = ~"-dynamiclib";
    } else {
        lib_cmd = ~"-shared";
    }

    // # Crate linking

    let cstore = sess.cstore;
    for cstore::get_used_crate_files(cstore).each |cratepath| {
        if cratepath.filetype() == Some(~".rlib") {
            cc_args.push(cratepath.to_str());
            loop;
        }
        let dir = cratepath.dirname();
        if dir != ~"" { cc_args.push(~"-L" + dir); }
        let libarg = unlib(sess.targ_cfg, cratepath.filestem().get());
        cc_args.push(~"-l" + libarg);
    }

    let ula = cstore::get_used_link_args(cstore);
    for ula.each |arg| { cc_args.push(/*bad*/copy *arg); }

    // # Extern library linking

    // User-supplied library search paths (-L on the cammand line) These are
    // the same paths used to find Rust crates, so some of them may have been
    // added already by the previous crate linking code. This only allows them
    // to be found at compile time so it is still entirely up to outside
    // forces to make sure that library can be found at runtime.

    let addl_paths = /*bad*/copy sess.opts.addl_lib_search_paths;
    for addl_paths.each |path| { cc_args.push(~"-L" + path.to_str()); }

    // The names of the extern libraries
    let used_libs = cstore::get_used_libraries(cstore);
    for used_libs.each |l| { cc_args.push(~"-l" + *l); }

    if sess.building_library {
        cc_args.push(lib_cmd);

        // On mac we need to tell the linker to let this library
        // be rpathed
        if sess.targ_cfg.os == session::os_macos {
            cc_args.push(~"-Wl,-install_name,@rpath/"
                      + output.filename().get());
        }
    }

    // Always want the runtime linked in
    cc_args.push(~"-lrustrt");

    // On linux librt and libdl are an indirect dependencies via rustrt,
    // and binutils 2.22+ won't add them automatically
    if sess.targ_cfg.os == session::os_linux {
        cc_args.push_all(~[~"-lrt", ~"-ldl"]);

        // LLVM implements the `frem` instruction as a call to `fmod`,
        // which lives in libm. Similar to above, on some linuxes we
        // have to be explicit about linking to it. See #2510
        cc_args.push(~"-lm");
    }
    else if sess.targ_cfg.os == session::os_android {
        cc_args.push_all(~[~"-ldl", ~"-llog",  ~"-lsupc++",
                           ~"-lgnustl_shared"]);
        cc_args.push(~"-lm");
    }

    if sess.targ_cfg.os == session::os_freebsd {
        cc_args.push_all(~[~"-pthread", ~"-lrt",
                                ~"-L/usr/local/lib", ~"-lexecinfo",
                                ~"-L/usr/local/lib/gcc46",
                                ~"-L/usr/local/lib/gcc44", ~"-lstdc++",
                                ~"-Wl,-z,origin",
                                ~"-Wl,-rpath,/usr/local/lib/gcc46",
                                ~"-Wl,-rpath,/usr/local/lib/gcc44"]);
    }

    // OS X 10.6 introduced 'compact unwind info', which is produced by the
    // linker from the dwarf unwind info. Unfortunately, it does not seem to
    // understand how to unwind our __morestack frame, so we have to turn it
    // off. This has impacted some other projects like GHC.
    if sess.targ_cfg.os == session::os_macos {
        cc_args.push(~"-Wl,-no_compact_unwind");
    }

    // Stack growth requires statically linking a __morestack function
    if sess.targ_cfg.os != session::os_android {
    cc_args.push(~"-lmorestack");
    }

    // FIXME (#2397): At some point we want to rpath our guesses as to where
    // extern libraries might live, based on the addl_lib_search_paths
    cc_args.push_all(rpath::get_rpath_flags(sess, &output));

    debug!("%s link args: %s", cc_prog, str::connect(cc_args, ~" "));
    // We run 'cc' here
    let prog = run::program_output(cc_prog, cc_args);
    if 0 != prog.status {
        sess.err(fmt!("linking with `%s` failed with code %d",
                      cc_prog, prog.status));
        sess.note(fmt!("%s arguments: %s",
                       cc_prog, str::connect(cc_args, ~" ")));
        sess.note(prog.err + prog.out);
        sess.abort_if_errors();
    }

    // Clean up on Darwin
    if sess.targ_cfg.os == session::os_macos {
        run::run_program(~"dsymutil", ~[output.to_str()]);
    }

    // Remove the temporary object file if we aren't saving temps
    if !sess.opts.save_temps {
        if ! os::remove_file(obj_filename) {
            sess.warn(fmt!("failed to delete object file `%s`",
                           obj_filename.to_str()));
        }
    }
}
//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
