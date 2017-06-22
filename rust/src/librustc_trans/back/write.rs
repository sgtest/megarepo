// Copyright 2013-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use back::lto;
use back::link::{self, get_linker, remove};
use back::symbol_export::ExportedSymbols;
use rustc_incremental::{save_trans_partition, in_incr_comp_dir};
use rustc::session::config::{self, OutputFilenames, OutputType, OutputTypes, Passes, SomePasses,
                             AllPasses, Sanitizer};
use rustc::session::Session;
use llvm;
use llvm::{ModuleRef, TargetMachineRef, PassManagerRef, DiagnosticInfoRef, ContextRef};
use llvm::SMDiagnosticRef;
use {CrateTranslation, ModuleLlvm, ModuleSource, ModuleTranslation};
use rustc::hir::def_id::CrateNum;
use rustc::util::common::{time, time_depth, set_time_depth, path2cstr};
use rustc::util::fs::link_or_copy;
use errors::{self, Handler, Level, DiagnosticBuilder, FatalError};
use errors::emitter::Emitter;
use syntax::ext::hygiene::Mark;
use syntax_pos::MultiSpan;
use context::{is_pie_binary, get_reloc_model};
use jobserver::{Client, Acquired};
use crossbeam::{scope, Scope};

use std::cmp;
use std::ffi::CString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str;
use std::sync::mpsc::{channel, Sender};
use libc::{c_uint, c_void};

pub const RELOC_MODEL_ARGS : [(&'static str, llvm::RelocMode); 7] = [
    ("pic", llvm::RelocMode::PIC),
    ("static", llvm::RelocMode::Static),
    ("default", llvm::RelocMode::Default),
    ("dynamic-no-pic", llvm::RelocMode::DynamicNoPic),
    ("ropi", llvm::RelocMode::ROPI),
    ("rwpi", llvm::RelocMode::RWPI),
    ("ropi-rwpi", llvm::RelocMode::ROPI_RWPI),
];

pub const CODE_GEN_MODEL_ARGS : [(&'static str, llvm::CodeModel); 5] = [
    ("default", llvm::CodeModel::Default),
    ("small", llvm::CodeModel::Small),
    ("kernel", llvm::CodeModel::Kernel),
    ("medium", llvm::CodeModel::Medium),
    ("large", llvm::CodeModel::Large),
];

pub fn llvm_err(handler: &errors::Handler, msg: String) -> FatalError {
    match llvm::last_error() {
        Some(err) => handler.fatal(&format!("{}: {}", msg, err)),
        None => handler.fatal(&msg),
    }
}

pub fn write_output_file(
        handler: &errors::Handler,
        target: llvm::TargetMachineRef,
        pm: llvm::PassManagerRef,
        m: ModuleRef,
        output: &Path,
        file_type: llvm::FileType) -> Result<(), FatalError> {
    unsafe {
        let output_c = path2cstr(output);
        let result = llvm::LLVMRustWriteOutputFile(
                target, pm, m, output_c.as_ptr(), file_type);
        if result.into_result().is_err() {
            let msg = format!("could not write output to {}", output.display());
            Err(llvm_err(handler, msg))
        } else {
            Ok(())
        }
    }
}

// On android, we by default compile for armv7 processors. This enables
// things like double word CAS instructions (rather than emulating them)
// which are *far* more efficient. This is obviously undesirable in some
// cases, so if any sort of target feature is specified we don't append v7
// to the feature list.
//
// On iOS only armv7 and newer are supported. So it is useful to
// get all hardware potential via VFP3 (hardware floating point)
// and NEON (SIMD) instructions supported by LLVM.
// Note that without those flags various linking errors might
// arise as some of intrinsics are converted into function calls
// and nobody provides implementations those functions
fn target_feature(sess: &Session) -> String {
    let rustc_features = [
        "crt-static",
    ];
    let requested_features = sess.opts.cg.target_feature.split(',');
    let llvm_features = requested_features.filter(|f| {
        !rustc_features.iter().any(|s| f.contains(s))
    });
    format!("{},{}",
            sess.target.target.options.features,
            llvm_features.collect::<Vec<_>>().join(","))
}

fn get_llvm_opt_level(optimize: config::OptLevel) -> llvm::CodeGenOptLevel {
    match optimize {
      config::OptLevel::No => llvm::CodeGenOptLevel::None,
      config::OptLevel::Less => llvm::CodeGenOptLevel::Less,
      config::OptLevel::Default => llvm::CodeGenOptLevel::Default,
      config::OptLevel::Aggressive => llvm::CodeGenOptLevel::Aggressive,
      _ => llvm::CodeGenOptLevel::Default,
    }
}

fn get_llvm_opt_size(optimize: config::OptLevel) -> llvm::CodeGenOptSize {
    match optimize {
      config::OptLevel::Size => llvm::CodeGenOptSizeDefault,
      config::OptLevel::SizeMin => llvm::CodeGenOptSizeAggressive,
      _ => llvm::CodeGenOptSizeNone,
    }
}

pub fn create_target_machine(sess: &Session) -> TargetMachineRef {
    let reloc_model = get_reloc_model(sess);

    let opt_level = get_llvm_opt_level(sess.opts.optimize);
    let use_softfp = sess.opts.cg.soft_float;

    let ffunction_sections = sess.target.target.options.function_sections;
    let fdata_sections = ffunction_sections;

    let code_model_arg = match sess.opts.cg.code_model {
        Some(ref s) => &s,
        None => &sess.target.target.options.code_model,
    };

    let code_model = match CODE_GEN_MODEL_ARGS.iter().find(
        |&&arg| arg.0 == code_model_arg) {
        Some(x) => x.1,
        _ => {
            sess.err(&format!("{:?} is not a valid code model",
                             sess.opts
                                 .cg
                                 .code_model));
            sess.abort_if_errors();
            bug!();
        }
    };

    let triple = &sess.target.target.llvm_target;

    let tm = unsafe {
        let triple = CString::new(triple.as_bytes()).unwrap();
        let cpu = match sess.opts.cg.target_cpu {
            Some(ref s) => &**s,
            None => &*sess.target.target.options.cpu
        };
        let cpu = CString::new(cpu.as_bytes()).unwrap();
        let features = CString::new(target_feature(sess).as_bytes()).unwrap();
        llvm::LLVMRustCreateTargetMachine(
            triple.as_ptr(), cpu.as_ptr(), features.as_ptr(),
            code_model,
            reloc_model,
            opt_level,
            use_softfp,
            is_pie_binary(sess),
            ffunction_sections,
            fdata_sections,
        )
    };

    if tm.is_null() {
        let msg = format!("Could not create LLVM TargetMachine for triple: {}",
                          triple);
        panic!(llvm_err(sess.diagnostic(), msg));
    } else {
        return tm;
    };
}


/// Module-specific configuration for `optimize_and_codegen`.
#[derive(Clone)]
pub struct ModuleConfig {
    /// LLVM TargetMachine to use for codegen.
    tm: TargetMachineRef,
    /// Names of additional optimization passes to run.
    passes: Vec<String>,
    /// Some(level) to optimize at a certain level, or None to run
    /// absolutely no optimizations (used for the metadata module).
    opt_level: Option<llvm::CodeGenOptLevel>,

    /// Some(level) to optimize binary size, or None to not affect program size.
    opt_size: Option<llvm::CodeGenOptSize>,

    // Flags indicating which outputs to produce.
    emit_no_opt_bc: bool,
    emit_bc: bool,
    emit_lto_bc: bool,
    emit_ir: bool,
    emit_asm: bool,
    emit_obj: bool,
    // Miscellaneous flags.  These are mostly copied from command-line
    // options.
    no_verify: bool,
    no_prepopulate_passes: bool,
    no_builtins: bool,
    time_passes: bool,
    vectorize_loop: bool,
    vectorize_slp: bool,
    merge_functions: bool,
    inline_threshold: Option<usize>,
    // Instead of creating an object file by doing LLVM codegen, just
    // make the object file bitcode. Provides easy compatibility with
    // emscripten's ecc compiler, when used as the linker.
    obj_is_bitcode: bool,
}

unsafe impl Send for ModuleConfig { }

impl ModuleConfig {
    fn new(tm: TargetMachineRef, passes: Vec<String>) -> ModuleConfig {
        ModuleConfig {
            tm: tm,
            passes: passes,
            opt_level: None,
            opt_size: None,

            emit_no_opt_bc: false,
            emit_bc: false,
            emit_lto_bc: false,
            emit_ir: false,
            emit_asm: false,
            emit_obj: false,
            obj_is_bitcode: false,

            no_verify: false,
            no_prepopulate_passes: false,
            no_builtins: false,
            time_passes: false,
            vectorize_loop: false,
            vectorize_slp: false,
            merge_functions: false,
            inline_threshold: None
        }
    }

    fn set_flags(&mut self, sess: &Session, trans: &CrateTranslation) {
        self.no_verify = sess.no_verify();
        self.no_prepopulate_passes = sess.opts.cg.no_prepopulate_passes;
        self.no_builtins = trans.no_builtins;
        self.time_passes = sess.time_passes();
        self.inline_threshold = sess.opts.cg.inline_threshold;
        self.obj_is_bitcode = sess.target.target.options.obj_is_bitcode;

        // Copy what clang does by turning on loop vectorization at O2 and
        // slp vectorization at O3. Otherwise configure other optimization aspects
        // of this pass manager builder.
        // Turn off vectorization for emscripten, as it's not very well supported.
        self.vectorize_loop = !sess.opts.cg.no_vectorize_loops &&
                             (sess.opts.optimize == config::OptLevel::Default ||
                              sess.opts.optimize == config::OptLevel::Aggressive) &&
                             !sess.target.target.options.is_like_emscripten;

        self.vectorize_slp = !sess.opts.cg.no_vectorize_slp &&
                            sess.opts.optimize == config::OptLevel::Aggressive &&
                            !sess.target.target.options.is_like_emscripten;

        self.merge_functions = sess.opts.optimize == config::OptLevel::Default ||
                               sess.opts.optimize == config::OptLevel::Aggressive;
    }
}

/// Additional resources used by optimize_and_codegen (not module specific)
pub struct CodegenContext<'a> {
    // Resouces needed when running LTO
    pub time_passes: bool,
    pub lto: bool,
    pub no_landing_pads: bool,
    pub exported_symbols: &'a ExportedSymbols,
    pub opts: &'a config::Options,
    pub crate_types: Vec<config::CrateType>,
    pub each_linked_rlib_for_lto: Vec<(CrateNum, PathBuf)>,
    // Handler to use for diagnostics produced during codegen.
    pub handler: &'a Handler,
    // LLVM passes added by plugins.
    pub plugin_passes: Vec<String>,
    // LLVM optimizations for which we want to print remarks.
    pub remark: Passes,
    // Worker thread number
    pub worker: usize,
    // The incremental compilation session directory, or None if we are not
    // compiling incrementally
    pub incr_comp_session_dir: Option<PathBuf>,
    // Channel back to the main control thread to send messages to
    pub tx: Sender<Message>,
}

struct HandlerFreeVars<'a> {
    llcx: ContextRef,
    cgcx: &'a CodegenContext<'a>,
}

unsafe extern "C" fn report_inline_asm<'a, 'b>(cgcx: &'a CodegenContext<'a>,
                                               msg: &'b str,
                                               cookie: c_uint) {
    drop(cgcx.tx.send(Message::InlineAsmError(cookie as u32, msg.to_string())));
}

unsafe extern "C" fn inline_asm_handler(diag: SMDiagnosticRef,
                                        user: *const c_void,
                                        cookie: c_uint) {
    let HandlerFreeVars { cgcx, .. } = *(user as *const HandlerFreeVars);

    let msg = llvm::build_string(|s| llvm::LLVMRustWriteSMDiagnosticToString(diag, s))
        .expect("non-UTF8 SMDiagnostic");

    report_inline_asm(cgcx, &msg, cookie);
}

unsafe extern "C" fn diagnostic_handler(info: DiagnosticInfoRef, user: *mut c_void) {
    let HandlerFreeVars { llcx, cgcx } = *(user as *const HandlerFreeVars);

    match llvm::diagnostic::Diagnostic::unpack(info) {
        llvm::diagnostic::InlineAsm(inline) => {
            report_inline_asm(cgcx,
                              &llvm::twine_to_string(inline.message),
                              inline.cookie);
        }

        llvm::diagnostic::Optimization(opt) => {
            let enabled = match cgcx.remark {
                AllPasses => true,
                SomePasses(ref v) => v.iter().any(|s| *s == opt.pass_name),
            };

            if enabled {
                let loc = llvm::debug_loc_to_string(llcx, opt.debug_loc);
                cgcx.handler.note_without_error(&format!("optimization {} for {} at {}: {}",
                                                opt.kind.describe(),
                                                opt.pass_name,
                                                if loc.is_empty() { "[unknown]" } else { &*loc },
                                                opt.message));
            }
        }

        _ => (),
    }
}

// Unsafe due to LLVM calls.
unsafe fn optimize_and_codegen(cgcx: &CodegenContext,
                               mtrans: ModuleTranslation,
                               mllvm: ModuleLlvm,
                               config: ModuleConfig,
                               output_names: OutputFilenames)
    -> Result<(), FatalError>
{
    let llmod = mllvm.llmod;
    let llcx = mllvm.llcx;
    let tm = config.tm;

    // llcx doesn't outlive this function, so we can put this on the stack.
    let fv = HandlerFreeVars {
        llcx: llcx,
        cgcx: cgcx,
    };
    let fv = &fv as *const HandlerFreeVars as *mut c_void;

    llvm::LLVMRustSetInlineAsmDiagnosticHandler(llcx, inline_asm_handler, fv);
    llvm::LLVMContextSetDiagnosticHandler(llcx, diagnostic_handler, fv);

    let module_name = Some(&mtrans.name[..]);

    if config.emit_no_opt_bc {
        let out = output_names.temp_path_ext("no-opt.bc", module_name);
        let out = path2cstr(&out);
        llvm::LLVMWriteBitcodeToFile(llmod, out.as_ptr());
    }

    if config.opt_level.is_some() {
        // Create the two optimizing pass managers. These mirror what clang
        // does, and are by populated by LLVM's default PassManagerBuilder.
        // Each manager has a different set of passes, but they also share
        // some common passes.
        let fpm = llvm::LLVMCreateFunctionPassManagerForModule(llmod);
        let mpm = llvm::LLVMCreatePassManager();

        // If we're verifying or linting, add them to the function pass
        // manager.
        let addpass = |pass_name: &str| {
            let pass_name = CString::new(pass_name).unwrap();
            let pass = llvm::LLVMRustFindAndCreatePass(pass_name.as_ptr());
            if pass.is_null() {
                return false;
            }
            let pass_manager = match llvm::LLVMRustPassKind(pass) {
                llvm::PassKind::Function => fpm,
                llvm::PassKind::Module => mpm,
                llvm::PassKind::Other => {
                    cgcx.handler.err("Encountered LLVM pass kind we can't handle");
                    return true
                },
            };
            llvm::LLVMRustAddPass(pass_manager, pass);
            true
        };

        if !config.no_verify { assert!(addpass("verify")); }
        if !config.no_prepopulate_passes {
            llvm::LLVMRustAddAnalysisPasses(tm, fpm, llmod);
            llvm::LLVMRustAddAnalysisPasses(tm, mpm, llmod);
            with_llvm_pmb(llmod, &config, &mut |b| {
                llvm::LLVMPassManagerBuilderPopulateFunctionPassManager(b, fpm);
                llvm::LLVMPassManagerBuilderPopulateModulePassManager(b, mpm);
            })
        }

        for pass in &config.passes {
            if !addpass(pass) {
                cgcx.handler.warn(&format!("unknown pass `{}`, ignoring",
                                           pass));
            }
        }

        for pass in &cgcx.plugin_passes {
            if !addpass(pass) {
                cgcx.handler.err(&format!("a plugin asked for LLVM pass \
                                           `{}` but LLVM does not \
                                           recognize it", pass));
            }
        }

        cgcx.handler.abort_if_errors();

        // Finally, run the actual optimization passes
        time(config.time_passes, &format!("llvm function passes [{}]", cgcx.worker), ||
             llvm::LLVMRustRunFunctionPassManager(fpm, llmod));
        time(config.time_passes, &format!("llvm module passes [{}]", cgcx.worker), ||
             llvm::LLVMRunPassManager(mpm, llmod));

        // Deallocate managers that we're now done with
        llvm::LLVMDisposePassManager(fpm);
        llvm::LLVMDisposePassManager(mpm);

        if cgcx.lto {
            time(cgcx.time_passes, "all lto passes", || {
                let temp_no_opt_bc_filename =
                    output_names.temp_path_ext("no-opt.lto.bc", module_name);
                lto::run(cgcx,
                         llmod,
                         tm,
                         &config,
                         &temp_no_opt_bc_filename)
            })?;
            if config.emit_lto_bc {
                let out = output_names.temp_path_ext("lto.bc", module_name);
                let out = path2cstr(&out);
                llvm::LLVMWriteBitcodeToFile(llmod, out.as_ptr());
            }
        }
    }

    // A codegen-specific pass manager is used to generate object
    // files for an LLVM module.
    //
    // Apparently each of these pass managers is a one-shot kind of
    // thing, so we create a new one for each type of output. The
    // pass manager passed to the closure should be ensured to not
    // escape the closure itself, and the manager should only be
    // used once.
    unsafe fn with_codegen<F, R>(tm: TargetMachineRef,
                                 llmod: ModuleRef,
                                 no_builtins: bool,
                                 f: F) -> R
        where F: FnOnce(PassManagerRef) -> R,
    {
        let cpm = llvm::LLVMCreatePassManager();
        llvm::LLVMRustAddAnalysisPasses(tm, cpm, llmod);
        llvm::LLVMRustAddLibraryInfo(cpm, llmod, no_builtins);
        f(cpm)
    }

    // Change what we write and cleanup based on whether obj files are
    // just llvm bitcode. In that case write bitcode, and possibly
    // delete the bitcode if it wasn't requested. Don't generate the
    // machine code, instead copy the .o file from the .bc
    let write_bc = config.emit_bc || config.obj_is_bitcode;
    let rm_bc = !config.emit_bc && config.obj_is_bitcode;
    let write_obj = config.emit_obj && !config.obj_is_bitcode;
    let copy_bc_to_obj = config.emit_obj && config.obj_is_bitcode;

    let bc_out = output_names.temp_path(OutputType::Bitcode, module_name);
    let obj_out = output_names.temp_path(OutputType::Object, module_name);

    if write_bc {
        let bc_out_c = path2cstr(&bc_out);
        llvm::LLVMWriteBitcodeToFile(llmod, bc_out_c.as_ptr());
    }

    time(config.time_passes, &format!("codegen passes [{}]", cgcx.worker),
         || -> Result<(), FatalError> {
        if config.emit_ir {
            let out = output_names.temp_path(OutputType::LlvmAssembly, module_name);
            let out = path2cstr(&out);
            with_codegen(tm, llmod, config.no_builtins, |cpm| {
                llvm::LLVMRustPrintModule(cpm, llmod, out.as_ptr());
                llvm::LLVMDisposePassManager(cpm);
            })
        }

        if config.emit_asm {
            let path = output_names.temp_path(OutputType::Assembly, module_name);

            // We can't use the same module for asm and binary output, because that triggers
            // various errors like invalid IR or broken binaries, so we might have to clone the
            // module to produce the asm output
            let llmod = if config.emit_obj {
                llvm::LLVMCloneModule(llmod)
            } else {
                llmod
            };
            with_codegen(tm, llmod, config.no_builtins, |cpm| {
                write_output_file(cgcx.handler, tm, cpm, llmod, &path,
                                  llvm::FileType::AssemblyFile)
            })?;
            if config.emit_obj {
                llvm::LLVMDisposeModule(llmod);
            }
        }

        if write_obj {
            with_codegen(tm, llmod, config.no_builtins, |cpm| {
                write_output_file(cgcx.handler, tm, cpm, llmod, &obj_out,
                                  llvm::FileType::ObjectFile)
            })?;
        }

        Ok(())
    })?;

    if copy_bc_to_obj {
        debug!("copying bitcode {:?} to obj {:?}", bc_out, obj_out);
        if let Err(e) = link_or_copy(&bc_out, &obj_out) {
            cgcx.handler.err(&format!("failed to copy bitcode to object file: {}", e));
        }
    }

    if rm_bc {
        debug!("removing_bitcode {:?}", bc_out);
        if let Err(e) = fs::remove_file(&bc_out) {
            cgcx.handler.err(&format!("failed to remove bitcode: {}", e));
        }
    }

    llvm::LLVMRustDisposeTargetMachine(tm);
    Ok(())
}


pub fn cleanup_llvm(trans: &CrateTranslation) {
    for module in trans.modules.iter() {
        unsafe {
            match module.source {
                ModuleSource::Translated(llvm) => {
                    llvm::LLVMDisposeModule(llvm.llmod);
                    llvm::LLVMContextDispose(llvm.llcx);
                }
                ModuleSource::Preexisting(_) => {
                }
            }
        }
    }
}

pub fn run_passes(sess: &Session,
                  trans: &CrateTranslation,
                  output_types: &OutputTypes,
                  crate_output: &OutputFilenames) {
    // It's possible that we have `codegen_units > 1` but only one item in
    // `trans.modules`.  We could theoretically proceed and do LTO in that
    // case, but it would be confusing to have the validity of
    // `-Z lto -C codegen-units=2` depend on details of the crate being
    // compiled, so we complain regardless.
    if sess.lto() && sess.opts.cg.codegen_units > 1 {
        // This case is impossible to handle because LTO expects to be able
        // to combine the entire crate and all its dependencies into a
        // single compilation unit, but each codegen unit is in a separate
        // LLVM context, so they can't easily be combined.
        sess.fatal("can't perform LTO when using multiple codegen units");
    }

    // Sanity check
    assert!(trans.modules.len() == sess.opts.cg.codegen_units ||
            sess.opts.debugging_opts.incremental.is_some() ||
            !sess.opts.output_types.should_trans() ||
            sess.opts.debugging_opts.no_trans);

    let tm = create_target_machine(sess);

    // Figure out what we actually need to build.

    let mut modules_config = ModuleConfig::new(tm, sess.opts.cg.passes.clone());
    let mut metadata_config = ModuleConfig::new(tm, vec![]);

    if let Some(ref sanitizer) = sess.opts.debugging_opts.sanitizer {
        match *sanitizer {
            Sanitizer::Address => {
                modules_config.passes.push("asan".to_owned());
                modules_config.passes.push("asan-module".to_owned());
            }
            Sanitizer::Memory => {
                modules_config.passes.push("msan".to_owned())
            }
            Sanitizer::Thread => {
                modules_config.passes.push("tsan".to_owned())
            }
            _ => {}
        }
    }

    if sess.opts.debugging_opts.profile {
        modules_config.passes.push("insert-gcov-profiling".to_owned())
    }

    modules_config.opt_level = Some(get_llvm_opt_level(sess.opts.optimize));
    modules_config.opt_size = Some(get_llvm_opt_size(sess.opts.optimize));

    // Save all versions of the bytecode if we're saving our temporaries.
    if sess.opts.cg.save_temps {
        modules_config.emit_no_opt_bc = true;
        modules_config.emit_bc = true;
        modules_config.emit_lto_bc = true;
        metadata_config.emit_bc = true;
    }

    // Emit bitcode files for the crate if we're emitting an rlib.
    // Whenever an rlib is created, the bitcode is inserted into the
    // archive in order to allow LTO against it.
    let needs_crate_bitcode =
            sess.crate_types.borrow().contains(&config::CrateTypeRlib) &&
            sess.opts.output_types.contains_key(&OutputType::Exe);
    let needs_crate_object =
            sess.opts.output_types.contains_key(&OutputType::Exe);
    if needs_crate_bitcode {
        modules_config.emit_bc = true;
    }

    for output_type in output_types.keys() {
        match *output_type {
            OutputType::Bitcode => { modules_config.emit_bc = true; }
            OutputType::LlvmAssembly => { modules_config.emit_ir = true; }
            OutputType::Assembly => {
                modules_config.emit_asm = true;
                // If we're not using the LLVM assembler, this function
                // could be invoked specially with output_type_assembly, so
                // in this case we still want the metadata object file.
                if !sess.opts.output_types.contains_key(&OutputType::Assembly) {
                    metadata_config.emit_obj = true;
                }
            }
            OutputType::Object => { modules_config.emit_obj = true; }
            OutputType::Metadata => { metadata_config.emit_obj = true; }
            OutputType::Exe => {
                modules_config.emit_obj = true;
                metadata_config.emit_obj = true;
            },
            OutputType::Mir => {}
            OutputType::DepInfo => {}
        }
    }

    modules_config.set_flags(sess, trans);
    metadata_config.set_flags(sess, trans);


    // Populate a buffer with a list of codegen threads.  Items are processed in
    // LIFO order, just because it's a tiny bit simpler that way.  (The order
    // doesn't actually matter.)
    let mut work_items = Vec::with_capacity(1 + trans.modules.len());

    {
        let work = build_work_item(sess,
                                   trans.metadata_module.clone(),
                                   metadata_config.clone(),
                                   crate_output.clone());
        work_items.push(work);
    }

    for mtrans in trans.modules.iter() {
        let work = build_work_item(sess,
                                   mtrans.clone(),
                                   modules_config.clone(),
                                   crate_output.clone());
        work_items.push(work);
    }

    if sess.opts.debugging_opts.incremental_info {
        dump_incremental_data(&trans);
    }

    let client = sess.jobserver_from_env.clone().unwrap_or_else(|| {
        // Pick a "reasonable maximum" if we don't otherwise have a jobserver in
        // our environment, capping out at 32 so we don't take everything down
        // by hogging the process run queue.
        let num_workers = cmp::min(work_items.len() - 1, 32);
        Client::new(num_workers).expect("failed to create jobserver")
    });
    scope(|scope| {
        execute_work(sess, work_items, client, &trans.exported_symbols, scope);
    });

    // If in incr. comp. mode, preserve the `.o` files for potential re-use
    for mtrans in trans.modules.iter() {
        let mut files = vec![];

        if modules_config.emit_obj {
            let path = crate_output.temp_path(OutputType::Object, Some(&mtrans.name));
            files.push((OutputType::Object, path));
        }

        if modules_config.emit_bc {
            let path = crate_output.temp_path(OutputType::Bitcode, Some(&mtrans.name));
            files.push((OutputType::Bitcode, path));
        }

        save_trans_partition(sess, &mtrans.name, mtrans.symbol_name_hash, &files);
    }

    // All codegen is finished.
    unsafe {
        llvm::LLVMRustDisposeTargetMachine(tm);
    }

    // Produce final compile outputs.
    let copy_gracefully = |from: &Path, to: &Path| {
        if let Err(e) = fs::copy(from, to) {
            sess.err(&format!("could not copy {:?} to {:?}: {}", from, to, e));
        }
    };

    let copy_if_one_unit = |output_type: OutputType,
                            keep_numbered: bool| {
        if trans.modules.len() == 1 {
            // 1) Only one codegen unit.  In this case it's no difficulty
            //    to copy `foo.0.x` to `foo.x`.
            let module_name = Some(&trans.modules[0].name[..]);
            let path = crate_output.temp_path(output_type, module_name);
            copy_gracefully(&path,
                            &crate_output.path(output_type));
            if !sess.opts.cg.save_temps && !keep_numbered {
                // The user just wants `foo.x`, not `foo.#module-name#.x`.
                remove(sess, &path);
            }
        } else {
            let ext = crate_output.temp_path(output_type, None)
                                  .extension()
                                  .unwrap()
                                  .to_str()
                                  .unwrap()
                                  .to_owned();

            if crate_output.outputs.contains_key(&output_type) {
                // 2) Multiple codegen units, with `--emit foo=some_name`.  We have
                //    no good solution for this case, so warn the user.
                sess.warn(&format!("ignoring emit path because multiple .{} files \
                                    were produced", ext));
            } else if crate_output.single_output_file.is_some() {
                // 3) Multiple codegen units, with `-o some_name`.  We have
                //    no good solution for this case, so warn the user.
                sess.warn(&format!("ignoring -o because multiple .{} files \
                                    were produced", ext));
            } else {
                // 4) Multiple codegen units, but no explicit name.  We
                //    just leave the `foo.0.x` files in place.
                // (We don't have to do any work in this case.)
            }
        }
    };

    // Flag to indicate whether the user explicitly requested bitcode.
    // Otherwise, we produced it only as a temporary output, and will need
    // to get rid of it.
    let mut user_wants_bitcode = false;
    let mut user_wants_objects = false;
    for output_type in output_types.keys() {
        match *output_type {
            OutputType::Bitcode => {
                user_wants_bitcode = true;
                // Copy to .bc, but always keep the .0.bc.  There is a later
                // check to figure out if we should delete .0.bc files, or keep
                // them for making an rlib.
                copy_if_one_unit(OutputType::Bitcode, true);
            }
            OutputType::LlvmAssembly => {
                copy_if_one_unit(OutputType::LlvmAssembly, false);
            }
            OutputType::Assembly => {
                copy_if_one_unit(OutputType::Assembly, false);
            }
            OutputType::Object => {
                user_wants_objects = true;
                copy_if_one_unit(OutputType::Object, true);
            }
            OutputType::Mir |
            OutputType::Metadata |
            OutputType::Exe |
            OutputType::DepInfo => {}
        }
    }
    let user_wants_bitcode = user_wants_bitcode;

    // Clean up unwanted temporary files.

    // We create the following files by default:
    //  - #crate#.#module-name#.bc
    //  - #crate#.#module-name#.o
    //  - #crate#.crate.metadata.bc
    //  - #crate#.crate.metadata.o
    //  - #crate#.o (linked from crate.##.o)
    //  - #crate#.bc (copied from crate.##.bc)
    // We may create additional files if requested by the user (through
    // `-C save-temps` or `--emit=` flags).

    if !sess.opts.cg.save_temps {
        // Remove the temporary .#module-name#.o objects.  If the user didn't
        // explicitly request bitcode (with --emit=bc), and the bitcode is not
        // needed for building an rlib, then we must remove .#module-name#.bc as
        // well.

        // Specific rules for keeping .#module-name#.bc:
        //  - If we're building an rlib (`needs_crate_bitcode`), then keep
        //    it.
        //  - If the user requested bitcode (`user_wants_bitcode`), and
        //    codegen_units > 1, then keep it.
        //  - If the user requested bitcode but codegen_units == 1, then we
        //    can toss .#module-name#.bc because we copied it to .bc earlier.
        //  - If we're not building an rlib and the user didn't request
        //    bitcode, then delete .#module-name#.bc.
        // If you change how this works, also update back::link::link_rlib,
        // where .#module-name#.bc files are (maybe) deleted after making an
        // rlib.
        let keep_numbered_bitcode = needs_crate_bitcode ||
                (user_wants_bitcode && sess.opts.cg.codegen_units > 1);

        let keep_numbered_objects = needs_crate_object ||
                (user_wants_objects && sess.opts.cg.codegen_units > 1);

        for module_name in trans.modules.iter().map(|m| Some(&m.name[..])) {
            if modules_config.emit_obj && !keep_numbered_objects {
                let path = crate_output.temp_path(OutputType::Object, module_name);
                remove(sess, &path);
            }

            if modules_config.emit_bc && !keep_numbered_bitcode {
                let path = crate_output.temp_path(OutputType::Bitcode, module_name);
                remove(sess, &path);
            }
        }

        if metadata_config.emit_bc && !user_wants_bitcode {
            let path = crate_output.temp_path(OutputType::Bitcode,
                                              Some(&trans.metadata_module.name));
            remove(sess, &path);
        }
    }

    // We leave the following files around by default:
    //  - #crate#.o
    //  - #crate#.crate.metadata.o
    //  - #crate#.bc
    // These are used in linking steps and will be cleaned up afterward.

    // FIXME: time_llvm_passes support - does this use a global context or
    // something?
    if sess.opts.cg.codegen_units == 1 && sess.time_llvm_passes() {
        unsafe { llvm::LLVMRustPrintPassTimings(); }
    }
}

fn dump_incremental_data(trans: &CrateTranslation) {
    let mut reuse = 0;
    for mtrans in trans.modules.iter() {
        match mtrans.source {
            ModuleSource::Preexisting(..) => reuse += 1,
            ModuleSource::Translated(..) => (),
        }
    }
    println!("incremental: re-using {} out of {} modules", reuse, trans.modules.len());
}

struct WorkItem {
    mtrans: ModuleTranslation,
    config: ModuleConfig,
    output_names: OutputFilenames
}

fn build_work_item(sess: &Session,
                   mtrans: ModuleTranslation,
                   config: ModuleConfig,
                   output_names: OutputFilenames)
                   -> WorkItem
{
    let mut config = config;
    config.tm = create_target_machine(sess);
    WorkItem {
        mtrans: mtrans,
        config: config,
        output_names: output_names
    }
}

fn execute_work_item(cgcx: &CodegenContext, work_item: WorkItem)
    -> Result<(), FatalError>
{
    unsafe {
        match work_item.mtrans.source {
            ModuleSource::Translated(mllvm) => {
                debug!("llvm-optimizing {:?}", work_item.mtrans.name);
                optimize_and_codegen(cgcx,
                                     work_item.mtrans,
                                     mllvm,
                                     work_item.config,
                                     work_item.output_names)?;
            }
            ModuleSource::Preexisting(wp) => {
                let incr_comp_session_dir = cgcx.incr_comp_session_dir
                                                .as_ref()
                                                .unwrap();
                let name = &work_item.mtrans.name;
                for (kind, saved_file) in wp.saved_files {
                    let obj_out = work_item.output_names.temp_path(kind, Some(name));
                    let source_file = in_incr_comp_dir(&incr_comp_session_dir,
                                                       &saved_file);
                    debug!("copying pre-existing module `{}` from {:?} to {}",
                           work_item.mtrans.name,
                           source_file,
                           obj_out.display());
                    match link_or_copy(&source_file, &obj_out) {
                        Ok(_) => { }
                        Err(err) => {
                            cgcx.handler.err(&format!("unable to copy {} to {}: {}",
                                                      source_file.display(),
                                                      obj_out.display(),
                                                      err));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub enum Message {
    Token(io::Result<Acquired>),
    Diagnostic(Diagnostic),
    Done { success: bool },
    InlineAsmError(u32, String),
    AbortIfErrors,
}

pub struct Diagnostic {
    msg: String,
    code: Option<String>,
    lvl: Level,
}

fn execute_work<'a>(sess: &'a Session,
                    mut work_items: Vec<WorkItem>,
                    jobserver: Client,
                    exported_symbols: &'a ExportedSymbols,
                    scope: &Scope<'a>) {
    let (tx, rx) = channel();
    let tx2 = tx.clone();

    // First up, convert our jobserver into a helper thread so we can use normal
    // mpsc channels to manage our messages and such. Once we've got the helper
    // thread then request `n-1` tokens because all of our work items are ready
    // to go.
    //
    // Note that the `n-1` is here because we ourselves have a token (our
    // process) and we'll use that token to execute at least one unit of work.
    //
    // After we've requested all these tokens then we'll, when we can, get
    // tokens on `rx` above which will get managed in the main loop below.
    let helper = jobserver.into_helper_thread(move |token| {
        drop(tx2.send(Message::Token(token)));
    }).expect("failed to spawn helper thread");
    for _ in 0..work_items.len() - 1 {
        helper.request_token();
    }

    // This is the "main loop" of parallel work happening for parallel codegen.
    // It's here that we manage parallelism, schedule work, and work with
    // messages coming from clients.
    //
    // Our channel `rx` created above is a channel of messages coming from our
    // various worker threads. This includes the jobserver helper thread above
    // as well as the work we'll spawn off here. Each turn of this loop starts
    // off by trying to spawn as much work as possible. After we've done that we
    // then wait for an event and dispatch accordingly once the event is
    // received. We're only done once all our work items have been drained and
    // nothing is running, at which point we return back up the stack.
    //
    // ## Parallelism management
    //
    // It's worth also touching on the management of parallelism here. We don't
    // want to just spawn a thread per work item because while that's optimal
    // parallelism it may overload a system with too many threads or violate our
    // configuration for the maximum amount of cpu to use for this process. To
    // manage this we use the `jobserver` crate.
    //
    // Job servers are an artifact of GNU make and are used to manage
    // parallelism between processes. A jobserver is a glorified IPC semaphore
    // basically. Whenever we want to run some work we acquire the semaphore,
    // and whenever we're done with that work we release the semaphore. In this
    // manner we can ensure that the maximum number of parallel workers is
    // capped at any one point in time.
    //
    // The jobserver protocol is a little unique, however. We, as a running
    // process, already have an ephemeral token assigned to us. We're not going
    // to be doing any productive work in this thread though so we're going to
    // give this token to a worker thread (there's no actual token to give, this
    // is just conceptually). As a result you'll see a few `+1` and `-1`
    // instances below, and it's about working with this ephemeral token.
    //
    // To acquire tokens we have our `helper` thread above which is just in a
    // loop acquiring tokens and sending them to us. We then store all tokens
    // locally in a `tokens` vector once they're acquired. Currently we don't
    // literally send a token to a worker thread to assist with management of
    // our "ephemeral token".
    //
    // As a result, our "spawn as much work as possible" basically means that we
    // fill up the `running` counter up to the limit of the `tokens` list.
    // Whenever we get a new token this'll mean a new unit of work is spawned,
    // and then whenever a unit of work finishes we relinquish a token, if we
    // had one, to maybe get re-acquired later.
    //
    // Note that there's a race which may mean that we acquire more tokens than
    // we originally anticipated. For example let's say we have 2 units of work.
    // First we request one token from the helper thread and then we
    // immediately spawn one unit of work with our ephemeral token after. We may
    // then finish the first piece of work before the token is acquired, but we
    // can continue to spawn the second piece of work with our ephemeral token.
    // Before that work finishes, however, we may acquire a token. In that case
    // we actually wastefully acquired the token, so we relinquish it back to
    // the jobserver.
    let mut tokens = Vec::new();
    let mut running = 0;
    while work_items.len() > 0 || running > 0 {

        // Spin up what work we can, only doing this while we've got available
        // parallelism slots and work left to spawn.
        while work_items.len() > 0 && running < tokens.len() + 1 {
            let item = work_items.pop().unwrap();
            let index = work_items.len();
            spawn_work(sess, exported_symbols, scope, tx.clone(), item, index);
            running += 1;
        }

        // Relinquish accidentally acquired extra tokens
        tokens.truncate(running.saturating_sub(1));

        match rx.recv().unwrap() {
            // Save the token locally and the next turn of the loop will use
            // this to spawn a new unit of work, or it may get dropped
            // immediately if we have no more work to spawn.
            Message::Token(token) => {
                tokens.push(token.expect("failed to acquire jobserver token"));
            }

            // If a thread exits successfully then we drop a token associated
            // with that worker and update our `running` count. We may later
            // re-acquire a token to continue running more work. We may also not
            // actually drop a token here if the worker was running with an
            // "ephemeral token"
            //
            // Note that if the thread failed that means it panicked, so we
            // abort immediately.
            Message::Done { success: true } => {
                drop(tokens.pop());
                running -= 1;
            }
            Message::Done { success: false } => {
                sess.fatal("aborting due to worker thread panic");
            }

            // Our worker wants us to emit an error message, so get ahold of our
            // `sess` and print it out
            Message::Diagnostic(diag) => {
                let handler = sess.diagnostic();
                match diag.code {
                    Some(ref code) => {
                        handler.emit_with_code(&MultiSpan::new(),
                                               &diag.msg,
                                               &code,
                                               diag.lvl);
                    }
                    None => {
                        handler.emit(&MultiSpan::new(),
                                     &diag.msg,
                                     diag.lvl);
                    }
                }
            }
            Message::InlineAsmError(cookie, msg) => {
                match Mark::from_u32(cookie).expn_info() {
                    Some(ei) => sess.span_err(ei.call_site, &msg),
                    None     => sess.err(&msg),
                }
            }

            // Sent to us after a worker sends us a batch of error messages, and
            // it's the point at which we check for errors.
            Message::AbortIfErrors => sess.diagnostic().abort_if_errors(),
        }
    }

    // Just in case, check this on the way out.
    sess.diagnostic().abort_if_errors();
}

struct SharedEmitter {
    tx: Sender<Message>,
}

impl Emitter for SharedEmitter {
    fn emit(&mut self, db: &DiagnosticBuilder) {
        drop(self.tx.send(Message::Diagnostic(Diagnostic {
            msg: db.message(),
            code: db.code.clone(),
            lvl: db.level,
        })));
        for child in &db.children {
            drop(self.tx.send(Message::Diagnostic(Diagnostic {
                msg: child.message(),
                code: None,
                lvl: child.level,
            })));
        }
        drop(self.tx.send(Message::AbortIfErrors));
    }
}

fn spawn_work<'a>(sess: &'a Session,
                  exported_symbols: &'a ExportedSymbols,
                  scope: &Scope<'a>,
                  tx: Sender<Message>,
                  work: WorkItem,
                  idx: usize) {
    let plugin_passes = sess.plugin_llvm_passes.borrow().clone();
    let remark = sess.opts.cg.remark.clone();
    let incr_comp_session_dir = sess.incr_comp_session_dir_opt().map(|r| r.clone());
    let depth = time_depth();
    let lto = sess.lto();
    let crate_types = sess.crate_types.borrow().clone();
    let mut each_linked_rlib_for_lto = Vec::new();
    drop(link::each_linked_rlib(sess, &mut |cnum, path| {
        // `#![no_builtins]` crates don't participate in LTO.
        if sess.cstore.is_no_builtins(cnum) {
            return
        }
        each_linked_rlib_for_lto.push((cnum, path.to_path_buf()));
    }));
    let time_passes = sess.time_passes();
    let no_landing_pads = sess.no_landing_pads();
    let opts = &sess.opts;

    scope.spawn(move || {
        set_time_depth(depth);

        // Set up a destructor which will fire off a message that we're done as
        // we exit.
        struct Bomb {
            tx: Sender<Message>,
            success: bool,
        }
        impl Drop for Bomb {
            fn drop(&mut self) {
                drop(self.tx.send(Message::Done { success: self.success }));
            }
        }
        let mut bomb = Bomb {
            tx: tx.clone(),
            success: false,
        };

        // Set up our non-`Send` `CodegenContext` now that we're in a helper
        // thread and have all our info available to us.
        let emitter = SharedEmitter { tx: tx.clone() };
        let diag_handler = Handler::with_emitter(true, false, Box::new(emitter));

        let cgcx = CodegenContext {
            crate_types: crate_types,
            each_linked_rlib_for_lto: each_linked_rlib_for_lto,
            lto: lto,
            no_landing_pads: no_landing_pads,
            opts: opts,
            time_passes: time_passes,
            exported_symbols: exported_symbols,
            handler: &diag_handler,
            plugin_passes: plugin_passes,
            remark: remark,
            worker: idx,
            incr_comp_session_dir: incr_comp_session_dir,
            tx: tx.clone(),
        };

        // Execute the work itself, and if it finishes successfully then flag
        // ourselves as a success as well.
        //
        // Note that we ignore the result coming out of `execute_work_item`
        // which will tell us if the worker failed with a `FatalError`. If that
        // has happened, however, then a diagnostic was sent off to the main
        // thread, along with an `AbortIfErrors` message. In that case the main
        // thread is already exiting anyway most likely.
        //
        // In any case, there's no need for us to take further action here, so
        // we just ignore the result and then send off our message saying that
        // we're done, which if `execute_work_item` failed is unlikely to be
        // seen by the main thread, but hey we might as well try anyway.
        drop(execute_work_item(&cgcx, work).is_err());
        bomb.success = true;
    });
}

pub fn run_assembler(sess: &Session, outputs: &OutputFilenames) {
    let (pname, mut cmd, _) = get_linker(sess);

    for arg in &sess.target.target.options.asm_args {
        cmd.arg(arg);
    }

    cmd.arg("-c").arg("-o").arg(&outputs.path(OutputType::Object))
                           .arg(&outputs.temp_path(OutputType::Assembly, None));
    debug!("{:?}", cmd);

    match cmd.output() {
        Ok(prog) => {
            if !prog.status.success() {
                let mut note = prog.stderr.clone();
                note.extend_from_slice(&prog.stdout);

                sess.struct_err(&format!("linking with `{}` failed: {}",
                                         pname,
                                         prog.status))
                    .note(&format!("{:?}", &cmd))
                    .note(str::from_utf8(&note[..]).unwrap())
                    .emit();
                sess.abort_if_errors();
            }
        },
        Err(e) => {
            sess.err(&format!("could not exec the linker `{}`: {}", pname, e));
            sess.abort_if_errors();
        }
    }
}

pub unsafe fn with_llvm_pmb(llmod: ModuleRef,
                            config: &ModuleConfig,
                            f: &mut FnMut(llvm::PassManagerBuilderRef)) {
    // Create the PassManagerBuilder for LLVM. We configure it with
    // reasonable defaults and prepare it to actually populate the pass
    // manager.
    let builder = llvm::LLVMPassManagerBuilderCreate();
    let opt_level = config.opt_level.unwrap_or(llvm::CodeGenOptLevel::None);
    let opt_size = config.opt_size.unwrap_or(llvm::CodeGenOptSizeNone);
    let inline_threshold = config.inline_threshold;

    llvm::LLVMRustConfigurePassManagerBuilder(builder, opt_level,
                                              config.merge_functions,
                                              config.vectorize_slp,
                                              config.vectorize_loop);
    llvm::LLVMPassManagerBuilderSetSizeLevel(builder, opt_size as u32);

    if opt_size != llvm::CodeGenOptSizeNone {
        llvm::LLVMPassManagerBuilderSetDisableUnrollLoops(builder, 1);
    }

    llvm::LLVMRustAddBuilderLibraryInfo(builder, llmod, config.no_builtins);

    // Here we match what clang does (kinda). For O0 we only inline
    // always-inline functions (but don't add lifetime intrinsics), at O1 we
    // inline with lifetime intrinsics, and O2+ we add an inliner with a
    // thresholds copied from clang.
    match (opt_level, opt_size, inline_threshold) {
        (.., Some(t)) => {
            llvm::LLVMPassManagerBuilderUseInlinerWithThreshold(builder, t as u32);
        }
        (llvm::CodeGenOptLevel::Aggressive, ..) => {
            llvm::LLVMPassManagerBuilderUseInlinerWithThreshold(builder, 275);
        }
        (_, llvm::CodeGenOptSizeDefault, _) => {
            llvm::LLVMPassManagerBuilderUseInlinerWithThreshold(builder, 75);
        }
        (_, llvm::CodeGenOptSizeAggressive, _) => {
            llvm::LLVMPassManagerBuilderUseInlinerWithThreshold(builder, 25);
        }
        (llvm::CodeGenOptLevel::None, ..) => {
            llvm::LLVMRustAddAlwaysInlinePass(builder, false);
        }
        (llvm::CodeGenOptLevel::Less, ..) => {
            llvm::LLVMRustAddAlwaysInlinePass(builder, true);
        }
        (llvm::CodeGenOptLevel::Default, ..) => {
            llvm::LLVMPassManagerBuilderUseInlinerWithThreshold(builder, 225);
        }
        (llvm::CodeGenOptLevel::Other, ..) => {
            bug!("CodeGenOptLevel::Other selected")
        }
    }

    f(builder);
    llvm::LLVMPassManagerBuilderDispose(builder);
}
