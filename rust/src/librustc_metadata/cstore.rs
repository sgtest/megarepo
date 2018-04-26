// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// The crate store - a central repo for information collected about external
// crates and libraries

use schema;

use rustc::hir::def_id::{CRATE_DEF_INDEX, CrateNum, DefIndex};
use rustc::hir::map::definitions::DefPathTable;
use rustc::hir::svh::Svh;
use rustc::middle::cstore::{DepKind, ExternCrate, MetadataLoader};
use rustc::session::{Session, CrateDisambiguator};
use rustc_target::spec::PanicStrategy;
use rustc_data_structures::indexed_vec::IndexVec;
use rustc::util::nodemap::{FxHashMap, NodeMap};

use rustc_data_structures::sync::{Lrc, RwLock, Lock};
use syntax::{ast, attr};
use syntax::ext::base::SyntaxExtension;
use syntax::symbol::Symbol;
use syntax_pos;

pub use rustc::middle::cstore::{NativeLibrary, NativeLibraryKind, LinkagePreference};
pub use rustc::middle::cstore::NativeLibraryKind::*;
pub use rustc::middle::cstore::{CrateSource, LibSource, ForeignModule};

pub use cstore_impl::{provide, provide_extern};

// A map from external crate numbers (as decoded from some crate file) to
// local crate numbers (as generated during this session). Each external
// crate may refer to types in other external crates, and each has their
// own crate numbers.
pub type CrateNumMap = IndexVec<CrateNum, CrateNum>;

pub use rustc_data_structures::sync::MetadataRef;

pub struct MetadataBlob(pub MetadataRef);

/// Holds information about a syntax_pos::FileMap imported from another crate.
/// See `imported_filemaps()` for more information.
pub struct ImportedFileMap {
    /// This FileMap's byte-offset within the codemap of its original crate
    pub original_start_pos: syntax_pos::BytePos,
    /// The end of this FileMap within the codemap of its original crate
    pub original_end_pos: syntax_pos::BytePos,
    /// The imported FileMap's representation within the local codemap
    pub translated_filemap: Lrc<syntax_pos::FileMap>,
}

pub struct CrateMetadata {
    pub name: Symbol,

    /// Information about the extern crate that caused this crate to
    /// be loaded. If this is `None`, then the crate was injected
    /// (e.g., by the allocator)
    pub extern_crate: Lock<Option<ExternCrate>>,

    pub blob: MetadataBlob,
    pub cnum_map: Lock<CrateNumMap>,
    pub cnum: CrateNum,
    pub codemap_import_info: RwLock<Vec<ImportedFileMap>>,
    pub attribute_cache: Lock<[Vec<Option<Lrc<[ast::Attribute]>>>; 2]>,

    pub root: schema::CrateRoot,

    /// For each public item in this crate, we encode a key.  When the
    /// crate is loaded, we read all the keys and put them in this
    /// hashmap, which gives the reverse mapping.  This allows us to
    /// quickly retrace a `DefPath`, which is needed for incremental
    /// compilation support.
    pub def_path_table: Lrc<DefPathTable>,

    pub trait_impls: FxHashMap<(u32, DefIndex), schema::LazySeq<DefIndex>>,

    pub dep_kind: Lock<DepKind>,
    pub source: CrateSource,

    pub proc_macros: Option<Vec<(ast::Name, Lrc<SyntaxExtension>)>>,
}

pub struct CStore {
    metas: RwLock<IndexVec<CrateNum, Option<Lrc<CrateMetadata>>>>,
    /// Map from NodeId's of local extern crate statements to crate numbers
    extern_mod_crate_map: Lock<NodeMap<CrateNum>>,
    pub metadata_loader: Box<MetadataLoader + Sync>,
}

impl CStore {
    pub fn new(metadata_loader: Box<MetadataLoader + Sync>) -> CStore {
        CStore {
            metas: RwLock::new(IndexVec::new()),
            extern_mod_crate_map: Lock::new(FxHashMap()),
            metadata_loader,
        }
    }

    /// You cannot use this function to allocate a CrateNum in a thread-safe manner.
    /// It is currently only used in CrateLoader which is single-threaded code.
    pub fn next_crate_num(&self) -> CrateNum {
        CrateNum::new(self.metas.borrow().len() + 1)
    }

    pub fn get_crate_data(&self, cnum: CrateNum) -> Lrc<CrateMetadata> {
        self.metas.borrow()[cnum].clone().unwrap()
    }

    pub fn set_crate_data(&self, cnum: CrateNum, data: Lrc<CrateMetadata>) {
        use rustc_data_structures::indexed_vec::Idx;
        let mut met = self.metas.borrow_mut();
        while met.len() <= cnum.index() {
            met.push(None);
        }
        met[cnum] = Some(data);
    }

    pub fn iter_crate_data<I>(&self, mut i: I)
        where I: FnMut(CrateNum, &Lrc<CrateMetadata>)
    {
        for (k, v) in self.metas.borrow().iter_enumerated() {
            if let &Some(ref v) = v {
                i(k, v);
            }
        }
    }

    pub fn crate_dependencies_in_rpo(&self, krate: CrateNum) -> Vec<CrateNum> {
        let mut ordering = Vec::new();
        self.push_dependencies_in_postorder(&mut ordering, krate);
        ordering.reverse();
        ordering
    }

    pub fn push_dependencies_in_postorder(&self, ordering: &mut Vec<CrateNum>, krate: CrateNum) {
        if ordering.contains(&krate) {
            return;
        }

        let data = self.get_crate_data(krate);
        for &dep in data.cnum_map.borrow().iter() {
            if dep != krate {
                self.push_dependencies_in_postorder(ordering, dep);
            }
        }

        ordering.push(krate);
    }

    pub fn do_postorder_cnums_untracked(&self) -> Vec<CrateNum> {
        let mut ordering = Vec::new();
        for (num, v) in self.metas.borrow().iter_enumerated() {
            if let &Some(_) = v {
                self.push_dependencies_in_postorder(&mut ordering, num);
            }
        }
        return ordering
    }

    pub fn add_extern_mod_stmt_cnum(&self, emod_id: ast::NodeId, cnum: CrateNum) {
        self.extern_mod_crate_map.borrow_mut().insert(emod_id, cnum);
    }

    pub fn do_extern_mod_stmt_cnum(&self, emod_id: ast::NodeId) -> Option<CrateNum> {
        self.extern_mod_crate_map.borrow().get(&emod_id).cloned()
    }
}

impl CrateMetadata {
    pub fn name(&self) -> Symbol {
        self.root.name
    }
    pub fn hash(&self) -> Svh {
        self.root.hash
    }
    pub fn disambiguator(&self) -> CrateDisambiguator {
        self.root.disambiguator
    }

    pub fn needs_allocator(&self, sess: &Session) -> bool {
        let attrs = self.get_item_attrs(CRATE_DEF_INDEX, sess);
        attr::contains_name(&attrs, "needs_allocator")
    }

    pub fn has_global_allocator(&self) -> bool {
        self.root.has_global_allocator.clone()
    }

    pub fn has_default_lib_allocator(&self) -> bool {
        self.root.has_default_lib_allocator.clone()
    }

    pub fn is_panic_runtime(&self, sess: &Session) -> bool {
        let attrs = self.get_item_attrs(CRATE_DEF_INDEX, sess);
        attr::contains_name(&attrs, "panic_runtime")
    }

    pub fn needs_panic_runtime(&self, sess: &Session) -> bool {
        let attrs = self.get_item_attrs(CRATE_DEF_INDEX, sess);
        attr::contains_name(&attrs, "needs_panic_runtime")
    }

    pub fn is_compiler_builtins(&self, sess: &Session) -> bool {
        let attrs = self.get_item_attrs(CRATE_DEF_INDEX, sess);
        attr::contains_name(&attrs, "compiler_builtins")
    }

    pub fn is_sanitizer_runtime(&self, sess: &Session) -> bool {
        let attrs = self.get_item_attrs(CRATE_DEF_INDEX, sess);
        attr::contains_name(&attrs, "sanitizer_runtime")
    }

    pub fn is_profiler_runtime(&self, sess: &Session) -> bool {
        let attrs = self.get_item_attrs(CRATE_DEF_INDEX, sess);
        attr::contains_name(&attrs, "profiler_runtime")
    }

    pub fn is_no_builtins(&self, sess: &Session) -> bool {
        let attrs = self.get_item_attrs(CRATE_DEF_INDEX, sess);
        attr::contains_name(&attrs, "no_builtins")
    }

    pub fn panic_strategy(&self) -> PanicStrategy {
        self.root.panic_strategy.clone()
    }
}
