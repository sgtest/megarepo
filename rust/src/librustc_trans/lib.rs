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
#![deny(warnings)]

#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(custom_attribute)]
#![allow(unused_attributes)]
#![feature(i128_type)]
#![feature(libc)]
#![feature(quote)]
#![feature(rustc_diagnostic_macros)]
#![feature(slice_patterns)]
#![feature(conservative_impl_trait)]

#![cfg_attr(stage0, feature(const_fn))]
#![cfg_attr(not(stage0), feature(const_atomic_bool_new))]
#![cfg_attr(not(stage0), feature(const_once_new))]

use rustc::dep_graph::WorkProduct;
use syntax_pos::symbol::Symbol;

#[macro_use]
extern crate bitflags;
extern crate flate2;
extern crate libc;
extern crate owning_ref;
#[macro_use] extern crate rustc;
extern crate rustc_allocator;
extern crate rustc_back;
extern crate rustc_data_structures;
extern crate rustc_incremental;
extern crate rustc_llvm as llvm;
extern crate rustc_platform_intrinsics as intrinsics;
extern crate rustc_const_math;
extern crate rustc_trans_utils;
extern crate rustc_demangle;
extern crate jobserver;
extern crate num_cpus;

#[macro_use] extern crate log;
#[macro_use] extern crate syntax;
extern crate syntax_pos;
extern crate rustc_errors as errors;
extern crate serialize;
#[cfg(windows)]
extern crate cc; // Used to locate MSVC

pub use base::trans_crate;

pub use metadata::LlvmMetadataLoader;
pub use llvm_util::{init, target_features, print_version, print_passes, print, enable_llvm_debug};

use std::any::Any;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;

use rustc::dep_graph::DepGraph;
use rustc::hir::def_id::CrateNum;
use rustc::middle::cstore::MetadataLoader;
use rustc::middle::cstore::{NativeLibrary, CrateSource, LibSource};
use rustc::session::Session;
use rustc::session::config::{OutputFilenames, OutputType};
use rustc::ty::maps::Providers;
use rustc::ty::{self, TyCtxt};
use rustc::util::nodemap::{FxHashSet, FxHashMap};

mod diagnostics;

pub mod back {
    mod archive;
    mod bytecode;
    mod command;
    pub(crate) mod linker;
    pub mod link;
    mod lto;
    pub(crate) mod symbol_export;
    pub(crate) mod symbol_names;
    pub mod write;
    mod rpath;
}

mod abi;
mod adt;
mod allocator;
mod asm;
mod assert_module_sources;
mod attributes;
mod base;
mod builder;
mod cabi_aarch64;
mod cabi_arm;
mod cabi_asmjs;
mod cabi_hexagon;
mod cabi_mips;
mod cabi_mips64;
mod cabi_msp430;
mod cabi_nvptx;
mod cabi_nvptx64;
mod cabi_powerpc;
mod cabi_powerpc64;
mod cabi_s390x;
mod cabi_sparc;
mod cabi_sparc64;
mod cabi_x86;
mod cabi_x86_64;
mod cabi_x86_win64;
mod callee;
mod collector;
mod common;
mod consts;
mod context;
mod debuginfo;
mod declare;
mod glue;
mod intrinsic;
mod llvm_util;
mod machine;
mod metadata;
mod meth;
mod mir;
mod monomorphize;
mod partitioning;
mod symbol_names_test;
mod time_graph;
mod trans_item;
mod tvec;
mod type_;
mod type_of;
mod value;

pub struct LlvmTransCrate(());

impl LlvmTransCrate {
    pub fn new() -> Self {
        LlvmTransCrate(())
    }
}

impl rustc_trans_utils::trans_crate::TransCrate for LlvmTransCrate {
    type MetadataLoader = metadata::LlvmMetadataLoader;
    type OngoingCrateTranslation = back::write::OngoingCrateTranslation;
    type TranslatedCrate = CrateTranslation;

    fn metadata_loader() -> Box<MetadataLoader> {
        box metadata::LlvmMetadataLoader
    }

    fn provide_local(providers: &mut ty::maps::Providers) {
        provide_local(providers);
    }

    fn provide_extern(providers: &mut ty::maps::Providers) {
        provide_extern(providers);
    }

    fn trans_crate<'a, 'tcx>(
        tcx: TyCtxt<'a, 'tcx, 'tcx>,
        rx: mpsc::Receiver<Box<Any + Send>>
    ) -> Self::OngoingCrateTranslation {
        base::trans_crate(tcx, rx)
    }

    fn join_trans(
        trans: Self::OngoingCrateTranslation,
        sess: &Session,
        dep_graph: &DepGraph
    ) -> Self::TranslatedCrate {
        trans.join(sess, dep_graph)
    }

    fn link_binary(sess: &Session, trans: &Self::TranslatedCrate, outputs: &OutputFilenames) {
        back::link::link_binary(sess, trans, outputs, &trans.crate_name.as_str());
    }

    fn dump_incremental_data(trans: &Self::TranslatedCrate) {
        back::write::dump_incremental_data(trans);
    }
}

pub struct ModuleTranslation {
    /// The name of the module. When the crate may be saved between
    /// compilations, incremental compilation requires that name be
    /// unique amongst **all** crates.  Therefore, it should contain
    /// something unique to this crate (e.g., a module path) as well
    /// as the crate name and disambiguator.
    name: String,
    llmod_id: String,
    symbol_name_hash: u64,
    pub source: ModuleSource,
    pub kind: ModuleKind,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum ModuleKind {
    Regular,
    Metadata,
    Allocator,
}

impl ModuleTranslation {
    pub fn llvm(&self) -> Option<&ModuleLlvm> {
        match self.source {
            ModuleSource::Translated(ref llvm) => Some(llvm),
            ModuleSource::Preexisting(_) => None,
        }
    }

    pub fn into_compiled_module(self,
                                emit_obj: bool,
                                emit_bc: bool,
                                outputs: &OutputFilenames) -> CompiledModule {
        let pre_existing = match self.source {
            ModuleSource::Preexisting(_) => true,
            ModuleSource::Translated(_) => false,
        };
        let object = outputs.temp_path(OutputType::Object, Some(&self.name));

        CompiledModule {
            llmod_id: self.llmod_id,
            name: self.name.clone(),
            kind: self.kind,
            symbol_name_hash: self.symbol_name_hash,
            pre_existing,
            emit_obj,
            emit_bc,
            object,
        }
    }
}

#[derive(Debug)]
pub struct CompiledModule {
    pub name: String,
    pub llmod_id: String,
    pub object: PathBuf,
    pub kind: ModuleKind,
    pub symbol_name_hash: u64,
    pub pre_existing: bool,
    pub emit_obj: bool,
    pub emit_bc: bool,
}

pub enum ModuleSource {
    /// Copy the `.o` files or whatever from the incr. comp. directory.
    Preexisting(WorkProduct),

    /// Rebuild from this LLVM module.
    Translated(ModuleLlvm),
}

#[derive(Debug)]
pub struct ModuleLlvm {
    llcx: llvm::ContextRef,
    pub llmod: llvm::ModuleRef,
    tm: llvm::TargetMachineRef,
}

unsafe impl Send for ModuleLlvm { }
unsafe impl Sync for ModuleLlvm { }

impl Drop for ModuleLlvm {
    fn drop(&mut self) {
        unsafe {
            llvm::LLVMDisposeModule(self.llmod);
            llvm::LLVMContextDispose(self.llcx);
            llvm::LLVMRustDisposeTargetMachine(self.tm);
        }
    }
}

pub struct CrateTranslation {
    pub crate_name: Symbol,
    pub modules: Vec<CompiledModule>,
    allocator_module: Option<CompiledModule>,
    pub link: rustc::middle::cstore::LinkMeta,
    pub metadata: rustc::middle::cstore::EncodedMetadata,
    windows_subsystem: Option<String>,
    linker_info: back::linker::LinkerInfo,
    crate_info: CrateInfo,
}

// Misc info we load from metadata to persist beyond the tcx
pub struct CrateInfo {
    panic_runtime: Option<CrateNum>,
    compiler_builtins: Option<CrateNum>,
    profiler_runtime: Option<CrateNum>,
    sanitizer_runtime: Option<CrateNum>,
    is_no_builtins: FxHashSet<CrateNum>,
    native_libraries: FxHashMap<CrateNum, Rc<Vec<NativeLibrary>>>,
    crate_name: FxHashMap<CrateNum, String>,
    used_libraries: Rc<Vec<NativeLibrary>>,
    link_args: Rc<Vec<String>>,
    used_crate_source: FxHashMap<CrateNum, Rc<CrateSource>>,
    used_crates_static: Vec<(CrateNum, LibSource)>,
    used_crates_dynamic: Vec<(CrateNum, LibSource)>,
}

__build_diagnostic_array! { librustc_trans, DIAGNOSTICS }

pub fn provide_local(providers: &mut Providers) {
    back::symbol_names::provide(providers);
    back::symbol_export::provide_local(providers);
    base::provide_local(providers);
}

pub fn provide_extern(providers: &mut Providers) {
    back::symbol_names::provide(providers);
    back::symbol_export::provide_extern(providers);
    base::provide_extern(providers);
}
