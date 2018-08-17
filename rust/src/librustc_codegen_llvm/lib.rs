// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! The Rust compiler.
//!
//! # Note
//!
//! This API is completely unstable and subject to change.

#![doc(html_logo_url = "https://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
      html_favicon_url = "https://doc.rust-lang.org/favicon.ico",
      html_root_url = "https://doc.rust-lang.org/nightly/")]

#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(crate_visibility_modifier)]
#![feature(custom_attribute)]
#![feature(extern_types)]
#![feature(in_band_lifetimes)]
#![allow(unused_attributes)]
#![feature(libc)]
#![cfg_attr(not(stage0), feature(nll))]
#![feature(quote)]
#![feature(range_contains)]
#![feature(rustc_diagnostic_macros)]
#![feature(slice_sort_by_cached_key)]
#![feature(optin_builtin_traits)]
#![feature(concat_idents)]
#![feature(link_args)]
#![feature(static_nobundle)]

use back::write::create_target_machine;
use rustc::dep_graph::WorkProduct;
use syntax_pos::symbol::Symbol;

#[macro_use] extern crate bitflags;
extern crate flate2;
extern crate libc;
#[macro_use] extern crate rustc;
extern crate jobserver;
extern crate num_cpus;
extern crate rustc_mir;
extern crate rustc_allocator;
extern crate rustc_apfloat;
extern crate rustc_target;
#[macro_use] extern crate rustc_data_structures;
extern crate rustc_demangle;
extern crate rustc_incremental;
extern crate rustc_llvm;
extern crate rustc_platform_intrinsics as intrinsics;
extern crate rustc_codegen_utils;
extern crate rustc_fs_util;

#[macro_use] extern crate log;
#[macro_use] extern crate syntax;
extern crate syntax_pos;
extern crate rustc_errors as errors;
extern crate serialize;
extern crate cc; // Used to locate MSVC
extern crate tempfile;

use back::bytecode::RLIB_BYTECODE_EXTENSION;

pub use llvm_util::target_features;

use std::any::Any;
use std::path::PathBuf;
use std::sync::mpsc;
use rustc_data_structures::sync::Lrc;

use rustc::dep_graph::DepGraph;
use rustc::hir::def_id::CrateNum;
use rustc::middle::cstore::MetadataLoader;
use rustc::middle::cstore::{NativeLibrary, CrateSource, LibSource};
use rustc::middle::lang_items::LangItem;
use rustc::session::{Session, CompileIncomplete};
use rustc::session::config::{OutputFilenames, OutputType, PrintRequest};
use rustc::ty::{self, TyCtxt};
use rustc::util::time_graph;
use rustc::util::nodemap::{FxHashSet, FxHashMap};
use rustc::util::profiling::ProfileCategory;
use rustc_mir::monomorphize;
use rustc_codegen_utils::codegen_backend::CodegenBackend;

mod diagnostics;

mod back {
    pub use rustc_codegen_utils::symbol_names;
    mod archive;
    pub mod bytecode;
    mod command;
    pub mod linker;
    pub mod link;
    mod lto;
    pub mod symbol_export;
    pub mod write;
    mod rpath;
    pub mod wasm;
}

mod abi;
mod allocator;
mod asm;
mod attributes;
mod base;
mod builder;
mod callee;
mod common;
mod consts;
mod context;
mod debuginfo;
mod declare;
mod glue;
mod intrinsic;
pub mod llvm;
mod llvm_util;
mod metadata;
mod meth;
mod mir;
mod mono_item;
mod type_;
mod type_of;
mod value;

pub struct LlvmCodegenBackend(());

impl !Send for LlvmCodegenBackend {} // Llvm is on a per-thread basis
impl !Sync for LlvmCodegenBackend {}

impl LlvmCodegenBackend {
    pub fn new() -> Box<dyn CodegenBackend> {
        box LlvmCodegenBackend(())
    }
}

impl CodegenBackend for LlvmCodegenBackend {
    fn init(&self, sess: &Session) {
        llvm_util::init(sess); // Make sure llvm is inited
    }

    fn print(&self, req: PrintRequest, sess: &Session) {
        match req {
            PrintRequest::RelocationModels => {
                println!("Available relocation models:");
                for &(name, _) in back::write::RELOC_MODEL_ARGS.iter() {
                    println!("    {}", name);
                }
                println!("");
            }
            PrintRequest::CodeModels => {
                println!("Available code models:");
                for &(name, _) in back::write::CODE_GEN_MODEL_ARGS.iter(){
                    println!("    {}", name);
                }
                println!("");
            }
            PrintRequest::TlsModels => {
                println!("Available TLS models:");
                for &(name, _) in back::write::TLS_MODEL_ARGS.iter(){
                    println!("    {}", name);
                }
                println!("");
            }
            req => llvm_util::print(req, sess),
        }
    }

    fn print_passes(&self) {
        llvm_util::print_passes();
    }

    fn print_version(&self) {
        llvm_util::print_version();
    }

    fn diagnostics(&self) -> &[(&'static str, &'static str)] {
        &DIAGNOSTICS
    }

    fn target_features(&self, sess: &Session) -> Vec<Symbol> {
        target_features(sess)
    }

    fn metadata_loader(&self) -> Box<dyn MetadataLoader + Sync> {
        box metadata::LlvmMetadataLoader
    }

    fn provide(&self, providers: &mut ty::query::Providers) {
        back::symbol_names::provide(providers);
        back::symbol_export::provide(providers);
        base::provide(providers);
        attributes::provide(providers);
    }

    fn provide_extern(&self, providers: &mut ty::query::Providers) {
        back::symbol_export::provide_extern(providers);
        base::provide_extern(providers);
        attributes::provide_extern(providers);
    }

    fn codegen_crate<'a, 'tcx>(
        &self,
        tcx: TyCtxt<'a, 'tcx, 'tcx>,
        rx: mpsc::Receiver<Box<dyn Any + Send>>
    ) -> Box<dyn Any> {
        box base::codegen_crate(tcx, rx)
    }

    fn join_codegen_and_link(
        &self,
        ongoing_codegen: Box<dyn Any>,
        sess: &Session,
        dep_graph: &DepGraph,
        outputs: &OutputFilenames,
    ) -> Result<(), CompileIncomplete>{
        use rustc::util::common::time;
        let (ongoing_codegen, work_products) =
            ongoing_codegen.downcast::<::back::write::OngoingCodegen>()
                .expect("Expected LlvmCodegenBackend's OngoingCodegen, found Box<Any>")
                .join(sess);
        if sess.opts.debugging_opts.incremental_info {
            back::write::dump_incremental_data(&ongoing_codegen);
        }

        time(sess,
             "serialize work products",
             move || rustc_incremental::save_work_product_index(sess, &dep_graph, work_products));

        sess.compile_status()?;

        if !sess.opts.output_types.keys().any(|&i| i == OutputType::Exe ||
                                                   i == OutputType::Metadata) {
            return Ok(());
        }

        // Run the linker on any artifacts that resulted from the LLVM run.
        // This should produce either a finished executable or library.
        sess.profiler(|p| p.start_activity(ProfileCategory::Linking));
        time(sess, "linking", || {
            back::link::link_binary(sess, &ongoing_codegen,
                                    outputs, &ongoing_codegen.crate_name.as_str());
        });
        sess.profiler(|p| p.end_activity(ProfileCategory::Linking));

        // Now that we won't touch anything in the incremental compilation directory
        // any more, we can finalize it (which involves renaming it)
        rustc_incremental::finalize_session_directory(sess, ongoing_codegen.link.crate_hash);

        Ok(())
    }
}

/// This is the entrypoint for a hot plugged rustc_codegen_llvm
#[no_mangle]
pub fn __rustc_codegen_backend() -> Box<dyn CodegenBackend> {
    LlvmCodegenBackend::new()
}

struct ModuleCodegen {
    /// The name of the module. When the crate may be saved between
    /// compilations, incremental compilation requires that name be
    /// unique amongst **all** crates.  Therefore, it should contain
    /// something unique to this crate (e.g., a module path) as well
    /// as the crate name and disambiguator.
    /// We currently generate these names via CodegenUnit::build_cgu_name().
    name: String,
    source: ModuleSource,
    kind: ModuleKind,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum ModuleKind {
    Regular,
    Metadata,
    Allocator,
}

impl ModuleCodegen {
    fn llvm(&self) -> Option<&ModuleLlvm> {
        match self.source {
            ModuleSource::Codegened(ref llvm) => Some(llvm),
            ModuleSource::Preexisting(_) => None,
        }
    }

    fn into_compiled_module(self,
                                emit_obj: bool,
                                emit_bc: bool,
                                emit_bc_compressed: bool,
                                outputs: &OutputFilenames) -> CompiledModule {
        let pre_existing = match self.source {
            ModuleSource::Preexisting(_) => true,
            ModuleSource::Codegened(_) => false,
        };
        let object = if emit_obj {
            Some(outputs.temp_path(OutputType::Object, Some(&self.name)))
        } else {
            None
        };
        let bytecode = if emit_bc {
            Some(outputs.temp_path(OutputType::Bitcode, Some(&self.name)))
        } else {
            None
        };
        let bytecode_compressed = if emit_bc_compressed {
            Some(outputs.temp_path(OutputType::Bitcode, Some(&self.name))
                    .with_extension(RLIB_BYTECODE_EXTENSION))
        } else {
            None
        };

        CompiledModule {
            name: self.name.clone(),
            kind: self.kind,
            pre_existing,
            object,
            bytecode,
            bytecode_compressed,
        }
    }
}

#[derive(Debug)]
struct CompiledModule {
    name: String,
    kind: ModuleKind,
    pre_existing: bool,
    object: Option<PathBuf>,
    bytecode: Option<PathBuf>,
    bytecode_compressed: Option<PathBuf>,
}

enum ModuleSource {
    /// Copy the `.o` files or whatever from the incr. comp. directory.
    Preexisting(WorkProduct),

    /// Rebuild from this LLVM module.
    Codegened(ModuleLlvm),
}

struct ModuleLlvm {
    llcx: &'static mut llvm::Context,
    llmod_raw: *const llvm::Module,
    tm: &'static mut llvm::TargetMachine,
}

unsafe impl Send for ModuleLlvm { }
unsafe impl Sync for ModuleLlvm { }

impl ModuleLlvm {
    fn new(sess: &Session, mod_name: &str) -> Self {
        unsafe {
            let llcx = llvm::LLVMRustContextCreate(sess.fewer_names());
            let llmod_raw = context::create_module(sess, llcx, mod_name) as *const _;

            ModuleLlvm {
                llmod_raw,
                llcx,
                tm: create_target_machine(sess, false),
            }
        }
    }

    fn llmod(&self) -> &llvm::Module {
        unsafe {
            &*self.llmod_raw
        }
    }
}

impl Drop for ModuleLlvm {
    fn drop(&mut self) {
        unsafe {
            llvm::LLVMContextDispose(&mut *(self.llcx as *mut _));
            llvm::LLVMRustDisposeTargetMachine(&mut *(self.tm as *mut _));
        }
    }
}

struct CodegenResults {
    crate_name: Symbol,
    modules: Vec<CompiledModule>,
    allocator_module: Option<CompiledModule>,
    metadata_module: CompiledModule,
    link: rustc::middle::cstore::LinkMeta,
    metadata: rustc::middle::cstore::EncodedMetadata,
    windows_subsystem: Option<String>,
    linker_info: back::linker::LinkerInfo,
    crate_info: CrateInfo,
}

/// Misc info we load from metadata to persist beyond the tcx
struct CrateInfo {
    panic_runtime: Option<CrateNum>,
    compiler_builtins: Option<CrateNum>,
    profiler_runtime: Option<CrateNum>,
    sanitizer_runtime: Option<CrateNum>,
    is_no_builtins: FxHashSet<CrateNum>,
    native_libraries: FxHashMap<CrateNum, Lrc<Vec<NativeLibrary>>>,
    crate_name: FxHashMap<CrateNum, String>,
    used_libraries: Lrc<Vec<NativeLibrary>>,
    link_args: Lrc<Vec<String>>,
    used_crate_source: FxHashMap<CrateNum, Lrc<CrateSource>>,
    used_crates_static: Vec<(CrateNum, LibSource)>,
    used_crates_dynamic: Vec<(CrateNum, LibSource)>,
    wasm_imports: FxHashMap<String, String>,
    lang_item_to_crate: FxHashMap<LangItem, CrateNum>,
    missing_lang_items: FxHashMap<CrateNum, Vec<LangItem>>,
}

__build_diagnostic_array! { librustc_codegen_llvm, DIAGNOSTICS }
