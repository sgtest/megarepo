use crate::rmeta::def_path_hash_map::DefPathHashMapRef;
use crate::rmeta::table::TableBuilder;
use crate::rmeta::*;

use rustc_data_structures::fingerprint::Fingerprint;
use rustc_data_structures::fx::{FxHashMap, FxIndexSet};
use rustc_data_structures::memmap::{Mmap, MmapMut};
use rustc_data_structures::stable_hasher::{HashStable, StableHasher};
use rustc_data_structures::sync::{join, par_iter, Lrc, ParallelIterator};
use rustc_data_structures::temp_dir::MaybeTempDir;
use rustc_hir as hir;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::{
    CrateNum, DefId, DefIndex, LocalDefId, CRATE_DEF_ID, CRATE_DEF_INDEX, LOCAL_CRATE,
};
use rustc_hir::definitions::DefPathData;
use rustc_hir::intravisit::{self, Visitor};
use rustc_hir::lang_items;
use rustc_hir::{AnonConst, GenericParamKind};
use rustc_index::bit_set::GrowableBitSet;
use rustc_middle::hir::nested_filter;
use rustc_middle::middle::dependency_format::Linkage;
use rustc_middle::middle::exported_symbols::{
    metadata_symbol_name, ExportedSymbol, SymbolExportInfo,
};
use rustc_middle::mir::interpret;
use rustc_middle::traits::specialization_graph;
use rustc_middle::ty::codec::TyEncoder;
use rustc_middle::ty::fast_reject::{self, SimplifiedType, TreatParams};
use rustc_middle::ty::query::Providers;
use rustc_middle::ty::{self, SymbolName, Ty, TyCtxt};
use rustc_serialize::{opaque, Decodable, Decoder, Encodable, Encoder};
use rustc_session::config::CrateType;
use rustc_session::cstore::{ForeignModule, LinkagePreference, NativeLib};
use rustc_span::hygiene::{ExpnIndex, HygieneEncodeContext, MacroKind};
use rustc_span::symbol::{sym, Symbol};
use rustc_span::{
    self, DebuggerVisualizerFile, ExternalSource, FileName, SourceFile, Span, SyntaxContext,
};
use rustc_target::abi::VariantIdx;
use std::borrow::Borrow;
use std::hash::Hash;
use std::io::{Read, Seek, Write};
use std::iter;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use tracing::{debug, trace};

pub(super) struct EncodeContext<'a, 'tcx> {
    opaque: opaque::FileEncoder,
    tcx: TyCtxt<'tcx>,
    feat: &'tcx rustc_feature::Features,

    tables: TableBuilders,

    lazy_state: LazyState,
    type_shorthands: FxHashMap<Ty<'tcx>, usize>,
    predicate_shorthands: FxHashMap<ty::PredicateKind<'tcx>, usize>,

    interpret_allocs: FxIndexSet<interpret::AllocId>,

    // This is used to speed up Span encoding.
    // The `usize` is an index into the `MonotonicVec`
    // that stores the `SourceFile`
    source_file_cache: (Lrc<SourceFile>, usize),
    // The indices (into the `SourceMap`'s `MonotonicVec`)
    // of all of the `SourceFiles` that we need to serialize.
    // When we serialize a `Span`, we insert the index of its
    // `SourceFile` into the `GrowableBitSet`.
    //
    // This needs to be a `GrowableBitSet` and not a
    // regular `BitSet` because we may actually import new `SourceFiles`
    // during metadata encoding, due to executing a query
    // with a result containing a foreign `Span`.
    required_source_files: Option<GrowableBitSet<usize>>,
    is_proc_macro: bool,
    hygiene_ctxt: &'a HygieneEncodeContext,
}

/// If the current crate is a proc-macro, returns early with `Lazy:empty()`.
/// This is useful for skipping the encoding of things that aren't needed
/// for proc-macro crates.
macro_rules! empty_proc_macro {
    ($self:ident) => {
        if $self.is_proc_macro {
            return LazyArray::empty();
        }
    };
}

macro_rules! encoder_methods {
    ($($name:ident($ty:ty);)*) => {
        $(fn $name(&mut self, value: $ty) {
            self.opaque.$name(value)
        })*
    }
}

impl<'a, 'tcx> Encoder for EncodeContext<'a, 'tcx> {
    encoder_methods! {
        emit_usize(usize);
        emit_u128(u128);
        emit_u64(u64);
        emit_u32(u32);
        emit_u16(u16);
        emit_u8(u8);

        emit_isize(isize);
        emit_i128(i128);
        emit_i64(i64);
        emit_i32(i32);
        emit_i16(i16);
        emit_i8(i8);

        emit_bool(bool);
        emit_f64(f64);
        emit_f32(f32);
        emit_char(char);
        emit_str(&str);
        emit_raw_bytes(&[u8]);
    }
}

impl<'a, 'tcx, T> Encodable<EncodeContext<'a, 'tcx>> for LazyValue<T> {
    fn encode(&self, e: &mut EncodeContext<'a, 'tcx>) {
        e.emit_lazy_distance(self.position);
    }
}

impl<'a, 'tcx, T> Encodable<EncodeContext<'a, 'tcx>> for LazyArray<T> {
    fn encode(&self, e: &mut EncodeContext<'a, 'tcx>) {
        e.emit_usize(self.num_elems);
        if self.num_elems > 0 {
            e.emit_lazy_distance(self.position)
        }
    }
}

impl<'a, 'tcx, I, T> Encodable<EncodeContext<'a, 'tcx>> for LazyTable<I, T> {
    fn encode(&self, e: &mut EncodeContext<'a, 'tcx>) {
        e.emit_usize(self.encoded_size);
        e.emit_lazy_distance(self.position);
    }
}

impl<'a, 'tcx> Encodable<EncodeContext<'a, 'tcx>> for CrateNum {
    fn encode(&self, s: &mut EncodeContext<'a, 'tcx>) {
        if *self != LOCAL_CRATE && s.is_proc_macro {
            panic!("Attempted to encode non-local CrateNum {:?} for proc-macro crate", self);
        }
        s.emit_u32(self.as_u32());
    }
}

impl<'a, 'tcx> Encodable<EncodeContext<'a, 'tcx>> for DefIndex {
    fn encode(&self, s: &mut EncodeContext<'a, 'tcx>) {
        s.emit_u32(self.as_u32());
    }
}

impl<'a, 'tcx> Encodable<EncodeContext<'a, 'tcx>> for ExpnIndex {
    fn encode(&self, s: &mut EncodeContext<'a, 'tcx>) {
        s.emit_u32(self.as_u32());
    }
}

impl<'a, 'tcx> Encodable<EncodeContext<'a, 'tcx>> for SyntaxContext {
    fn encode(&self, s: &mut EncodeContext<'a, 'tcx>) {
        rustc_span::hygiene::raw_encode_syntax_context(*self, &s.hygiene_ctxt, s);
    }
}

impl<'a, 'tcx> Encodable<EncodeContext<'a, 'tcx>> for ExpnId {
    fn encode(&self, s: &mut EncodeContext<'a, 'tcx>) {
        if self.krate == LOCAL_CRATE {
            // We will only write details for local expansions.  Non-local expansions will fetch
            // data from the corresponding crate's metadata.
            // FIXME(#43047) FIXME(#74731) We may eventually want to avoid relying on external
            // metadata from proc-macro crates.
            s.hygiene_ctxt.schedule_expn_data_for_encoding(*self);
        }
        self.krate.encode(s);
        self.local_id.encode(s);
    }
}

impl<'a, 'tcx> Encodable<EncodeContext<'a, 'tcx>> for Span {
    fn encode(&self, s: &mut EncodeContext<'a, 'tcx>) {
        let span = self.data();

        // Don't serialize any `SyntaxContext`s from a proc-macro crate,
        // since we don't load proc-macro dependencies during serialization.
        // This means that any hygiene information from macros used *within*
        // a proc-macro crate (e.g. invoking a macro that expands to a proc-macro
        // definition) will be lost.
        //
        // This can show up in two ways:
        //
        // 1. Any hygiene information associated with identifier of
        // a proc macro (e.g. `#[proc_macro] pub fn $name`) will be lost.
        // Since proc-macros can only be invoked from a different crate,
        // real code should never need to care about this.
        //
        // 2. Using `Span::def_site` or `Span::mixed_site` will not
        // include any hygiene information associated with the definition
        // site. This means that a proc-macro cannot emit a `$crate`
        // identifier which resolves to one of its dependencies,
        // which also should never come up in practice.
        //
        // Additionally, this affects `Span::parent`, and any other
        // span inspection APIs that would otherwise allow traversing
        // the `SyntaxContexts` associated with a span.
        //
        // None of these user-visible effects should result in any
        // cross-crate inconsistencies (getting one behavior in the same
        // crate, and a different behavior in another crate) due to the
        // limited surface that proc-macros can expose.
        //
        // IMPORTANT: If this is ever changed, be sure to update
        // `rustc_span::hygiene::raw_encode_expn_id` to handle
        // encoding `ExpnData` for proc-macro crates.
        if s.is_proc_macro {
            SyntaxContext::root().encode(s);
        } else {
            span.ctxt.encode(s);
        }

        if self.is_dummy() {
            return TAG_PARTIAL_SPAN.encode(s);
        }

        // The Span infrastructure should make sure that this invariant holds:
        debug_assert!(span.lo <= span.hi);

        if !s.source_file_cache.0.contains(span.lo) {
            let source_map = s.tcx.sess.source_map();
            let source_file_index = source_map.lookup_source_file_idx(span.lo);
            s.source_file_cache =
                (source_map.files()[source_file_index].clone(), source_file_index);
        }

        if !s.source_file_cache.0.contains(span.hi) {
            // Unfortunately, macro expansion still sometimes generates Spans
            // that malformed in this way.
            return TAG_PARTIAL_SPAN.encode(s);
        }

        let source_files = s.required_source_files.as_mut().expect("Already encoded SourceMap!");
        // Record the fact that we need to encode the data for this `SourceFile`
        source_files.insert(s.source_file_cache.1);

        // There are two possible cases here:
        // 1. This span comes from a 'foreign' crate - e.g. some crate upstream of the
        // crate we are writing metadata for. When the metadata for *this* crate gets
        // deserialized, the deserializer will need to know which crate it originally came
        // from. We use `TAG_VALID_SPAN_FOREIGN` to indicate that a `CrateNum` should
        // be deserialized after the rest of the span data, which tells the deserializer
        // which crate contains the source map information.
        // 2. This span comes from our own crate. No special handling is needed - we just
        // write `TAG_VALID_SPAN_LOCAL` to let the deserializer know that it should use
        // our own source map information.
        //
        // If we're a proc-macro crate, we always treat this as a local `Span`.
        // In `encode_source_map`, we serialize foreign `SourceFile`s into our metadata
        // if we're a proc-macro crate.
        // This allows us to avoid loading the dependencies of proc-macro crates: all of
        // the information we need to decode `Span`s is stored in the proc-macro crate.
        let (tag, lo, hi) = if s.source_file_cache.0.is_imported() && !s.is_proc_macro {
            // To simplify deserialization, we 'rebase' this span onto the crate it originally came from
            // (the crate that 'owns' the file it references. These rebased 'lo' and 'hi' values
            // are relative to the source map information for the 'foreign' crate whose CrateNum
            // we write into the metadata. This allows `imported_source_files` to binary
            // search through the 'foreign' crate's source map information, using the
            // deserialized 'lo' and 'hi' values directly.
            //
            // All of this logic ensures that the final result of deserialization is a 'normal'
            // Span that can be used without any additional trouble.
            let external_start_pos = {
                // Introduce a new scope so that we drop the 'lock()' temporary
                match &*s.source_file_cache.0.external_src.lock() {
                    ExternalSource::Foreign { original_start_pos, .. } => *original_start_pos,
                    src => panic!("Unexpected external source {:?}", src),
                }
            };
            let lo = (span.lo - s.source_file_cache.0.start_pos) + external_start_pos;
            let hi = (span.hi - s.source_file_cache.0.start_pos) + external_start_pos;

            (TAG_VALID_SPAN_FOREIGN, lo, hi)
        } else {
            (TAG_VALID_SPAN_LOCAL, span.lo, span.hi)
        };

        tag.encode(s);
        lo.encode(s);

        // Encode length which is usually less than span.hi and profits more
        // from the variable-length integer encoding that we use.
        let len = hi - lo;
        len.encode(s);

        if tag == TAG_VALID_SPAN_FOREIGN {
            // This needs to be two lines to avoid holding the `s.source_file_cache`
            // while calling `cnum.encode(s)`
            let cnum = s.source_file_cache.0.cnum;
            cnum.encode(s);
        }
    }
}

impl<'a, 'tcx> TyEncoder for EncodeContext<'a, 'tcx> {
    const CLEAR_CROSS_CRATE: bool = true;

    type I = TyCtxt<'tcx>;

    fn position(&self) -> usize {
        self.opaque.position()
    }

    fn type_shorthands(&mut self) -> &mut FxHashMap<Ty<'tcx>, usize> {
        &mut self.type_shorthands
    }

    fn predicate_shorthands(&mut self) -> &mut FxHashMap<ty::PredicateKind<'tcx>, usize> {
        &mut self.predicate_shorthands
    }

    fn encode_alloc_id(&mut self, alloc_id: &rustc_middle::mir::interpret::AllocId) {
        let (index, _) = self.interpret_allocs.insert_full(*alloc_id);

        index.encode(self);
    }
}

// Shorthand for `$self.$tables.$table.set($def_id.index, $self.lazy_value($value))`, which would
// normally need extra variables to avoid errors about multiple mutable borrows.
macro_rules! record {
    ($self:ident.$tables:ident.$table:ident[$def_id:expr] <- $value:expr) => {{
        {
            let value = $value;
            let lazy = $self.lazy(value);
            $self.$tables.$table.set($def_id.index, lazy);
        }
    }};
}

// Shorthand for `$self.$tables.$table.set($def_id.index, $self.lazy_value($value))`, which would
// normally need extra variables to avoid errors about multiple mutable borrows.
macro_rules! record_array {
    ($self:ident.$tables:ident.$table:ident[$def_id:expr] <- $value:expr) => {{
        {
            let value = $value;
            let lazy = $self.lazy_array(value);
            $self.$tables.$table.set($def_id.index, lazy);
        }
    }};
}

impl<'a, 'tcx> EncodeContext<'a, 'tcx> {
    fn emit_lazy_distance(&mut self, position: NonZeroUsize) {
        let pos = position.get();
        let distance = match self.lazy_state {
            LazyState::NoNode => bug!("emit_lazy_distance: outside of a metadata node"),
            LazyState::NodeStart(start) => {
                let start = start.get();
                assert!(pos <= start);
                start - pos
            }
            LazyState::Previous(last_pos) => {
                assert!(
                    last_pos <= position,
                    "make sure that the calls to `lazy*` \
                     are in the same order as the metadata fields",
                );
                position.get() - last_pos.get()
            }
        };
        self.lazy_state = LazyState::Previous(NonZeroUsize::new(pos).unwrap());
        self.emit_usize(distance);
    }

    fn lazy<T: ParameterizedOverTcx, B: Borrow<T::Value<'tcx>>>(&mut self, value: B) -> LazyValue<T>
    where
        T::Value<'tcx>: Encodable<EncodeContext<'a, 'tcx>>,
    {
        let pos = NonZeroUsize::new(self.position()).unwrap();

        assert_eq!(self.lazy_state, LazyState::NoNode);
        self.lazy_state = LazyState::NodeStart(pos);
        value.borrow().encode(self);
        self.lazy_state = LazyState::NoNode;

        assert!(pos.get() <= self.position());

        LazyValue::from_position(pos)
    }

    fn lazy_array<T: ParameterizedOverTcx, I: IntoIterator<Item = B>, B: Borrow<T::Value<'tcx>>>(
        &mut self,
        values: I,
    ) -> LazyArray<T>
    where
        T::Value<'tcx>: Encodable<EncodeContext<'a, 'tcx>>,
    {
        let pos = NonZeroUsize::new(self.position()).unwrap();

        assert_eq!(self.lazy_state, LazyState::NoNode);
        self.lazy_state = LazyState::NodeStart(pos);
        let len = values.into_iter().map(|value| value.borrow().encode(self)).count();
        self.lazy_state = LazyState::NoNode;

        assert!(pos.get() <= self.position());

        LazyArray::from_position_and_num_elems(pos, len)
    }

    fn encode_info_for_items(&mut self) {
        self.encode_info_for_mod(CRATE_DEF_ID, self.tcx.hir().root_module());

        // Proc-macro crates only export proc-macro items, which are looked
        // up using `proc_macro_data`
        if self.is_proc_macro {
            return;
        }

        self.tcx.hir().visit_all_item_likes_in_crate(self);
    }

    fn encode_def_path_table(&mut self) {
        let table = self.tcx.def_path_table();
        if self.is_proc_macro {
            for def_index in std::iter::once(CRATE_DEF_INDEX)
                .chain(self.tcx.resolutions(()).proc_macros.iter().map(|p| p.local_def_index))
            {
                let def_key = self.lazy(table.def_key(def_index));
                let def_path_hash = table.def_path_hash(def_index);
                self.tables.def_keys.set(def_index, def_key);
                self.tables.def_path_hashes.set(def_index, def_path_hash);
            }
        } else {
            for (def_index, def_key, def_path_hash) in table.enumerated_keys_and_path_hashes() {
                let def_key = self.lazy(def_key);
                self.tables.def_keys.set(def_index, def_key);
                self.tables.def_path_hashes.set(def_index, *def_path_hash);
            }
        }
    }

    fn encode_def_path_hash_map(&mut self) -> LazyValue<DefPathHashMapRef<'static>> {
        self.lazy(DefPathHashMapRef::BorrowedFromTcx(self.tcx.def_path_hash_to_def_index_map()))
    }

    fn encode_source_map(&mut self) -> LazyArray<rustc_span::SourceFile> {
        let source_map = self.tcx.sess.source_map();
        let all_source_files = source_map.files();

        // By replacing the `Option` with `None`, we ensure that we can't
        // accidentally serialize any more `Span`s after the source map encoding
        // is done.
        let required_source_files = self.required_source_files.take().unwrap();

        let working_directory = &self.tcx.sess.opts.working_dir;

        let adapted = all_source_files
            .iter()
            .enumerate()
            .filter(|(idx, source_file)| {
                // Only serialize `SourceFile`s that were used
                // during the encoding of a `Span`
                required_source_files.contains(*idx) &&
                // Don't serialize imported `SourceFile`s, unless
                // we're in a proc-macro crate.
                (!source_file.is_imported() || self.is_proc_macro)
            })
            .map(|(_, source_file)| {
                // At export time we expand all source file paths to absolute paths because
                // downstream compilation sessions can have a different compiler working
                // directory, so relative paths from this or any other upstream crate
                // won't be valid anymore.
                //
                // At this point we also erase the actual on-disk path and only keep
                // the remapped version -- as is necessary for reproducible builds.
                match source_file.name {
                    FileName::Real(ref original_file_name) => {
                        let adapted_file_name =
                            source_map.path_mapping().to_embeddable_absolute_path(
                                original_file_name.clone(),
                                working_directory,
                            );

                        if adapted_file_name != *original_file_name {
                            let mut adapted: SourceFile = (**source_file).clone();
                            adapted.name = FileName::Real(adapted_file_name);
                            adapted.name_hash = {
                                let mut hasher: StableHasher = StableHasher::new();
                                adapted.name.hash(&mut hasher);
                                hasher.finish::<u128>()
                            };
                            Lrc::new(adapted)
                        } else {
                            // Nothing to adapt
                            source_file.clone()
                        }
                    }
                    // expanded code, not from a file
                    _ => source_file.clone(),
                }
            })
            .map(|mut source_file| {
                // We're serializing this `SourceFile` into our crate metadata,
                // so mark it as coming from this crate.
                // This also ensures that we don't try to deserialize the
                // `CrateNum` for a proc-macro dependency - since proc macro
                // dependencies aren't loaded when we deserialize a proc-macro,
                // trying to remap the `CrateNum` would fail.
                if self.is_proc_macro {
                    Lrc::make_mut(&mut source_file).cnum = LOCAL_CRATE;
                }
                source_file
            })
            .collect::<Vec<_>>();

        self.lazy_array(adapted.iter().map(|rc| &**rc))
    }

    fn encode_crate_root(&mut self) -> LazyValue<CrateRoot> {
        let tcx = self.tcx;
        let mut i = 0;
        let preamble_bytes = self.position() - i;

        // Encode the crate deps
        i = self.position();
        let crate_deps = self.encode_crate_deps();
        let dylib_dependency_formats = self.encode_dylib_dependency_formats();
        let dep_bytes = self.position() - i;

        // Encode the lib features.
        i = self.position();
        let lib_features = self.encode_lib_features();
        let lib_feature_bytes = self.position() - i;

        // Encode the stability implications.
        i = self.position();
        let stability_implications = self.encode_stability_implications();
        let stability_implications_bytes = self.position() - i;

        // Encode the language items.
        i = self.position();
        let lang_items = self.encode_lang_items();
        let lang_items_missing = self.encode_lang_items_missing();
        let lang_item_bytes = self.position() - i;

        // Encode the diagnostic items.
        i = self.position();
        let diagnostic_items = self.encode_diagnostic_items();
        let diagnostic_item_bytes = self.position() - i;

        // Encode the native libraries used
        i = self.position();
        let native_libraries = self.encode_native_libraries();
        let native_lib_bytes = self.position() - i;

        i = self.position();
        let foreign_modules = self.encode_foreign_modules();
        let foreign_modules_bytes = self.position() - i;

        // Encode DefPathTable
        i = self.position();
        self.encode_def_path_table();
        let def_path_table_bytes = self.position() - i;

        // Encode the def IDs of traits, for rustdoc and diagnostics.
        i = self.position();
        let traits = self.encode_traits();
        let traits_bytes = self.position() - i;

        // Encode the def IDs of impls, for coherence checking.
        i = self.position();
        let impls = self.encode_impls();
        let impls_bytes = self.position() - i;

        i = self.position();
        let incoherent_impls = self.encode_incoherent_impls();
        let incoherent_impls_bytes = self.position() - i;

        // Encode MIR.
        i = self.position();
        self.encode_mir();
        let mir_bytes = self.position() - i;

        // Encode the items.
        i = self.position();
        self.encode_def_ids();
        self.encode_info_for_items();
        let item_bytes = self.position() - i;

        // Encode the allocation index
        i = self.position();
        let interpret_alloc_index = {
            let mut interpret_alloc_index = Vec::new();
            let mut n = 0;
            trace!("beginning to encode alloc ids");
            loop {
                let new_n = self.interpret_allocs.len();
                // if we have found new ids, serialize those, too
                if n == new_n {
                    // otherwise, abort
                    break;
                }
                trace!("encoding {} further alloc ids", new_n - n);
                for idx in n..new_n {
                    let id = self.interpret_allocs[idx];
                    let pos = self.position() as u32;
                    interpret_alloc_index.push(pos);
                    interpret::specialized_encode_alloc_id(self, tcx, id);
                }
                n = new_n;
            }
            self.lazy_array(interpret_alloc_index)
        };
        let interpret_alloc_index_bytes = self.position() - i;

        // Encode the proc macro data. This affects 'tables',
        // so we need to do this before we encode the tables.
        // This overwrites def_keys, so it must happen after encode_def_path_table.
        i = self.position();
        let proc_macro_data = self.encode_proc_macros();
        let proc_macro_data_bytes = self.position() - i;

        i = self.position();
        let tables = self.tables.encode(&mut self.opaque);
        let tables_bytes = self.position() - i;

        i = self.position();
        let debugger_visualizers = self.encode_debugger_visualizers();
        let debugger_visualizers_bytes = self.position() - i;

        // Encode exported symbols info. This is prefetched in `encode_metadata` so we encode
        // this as late as possible to give the prefetching as much time as possible to complete.
        i = self.position();
        let exported_symbols = tcx.exported_symbols(LOCAL_CRATE);
        let exported_symbols = self.encode_exported_symbols(&exported_symbols);
        let exported_symbols_bytes = self.position() - i;

        // Encode the hygiene data,
        // IMPORTANT: this *must* be the last thing that we encode (other than `SourceMap`). The process
        // of encoding other items (e.g. `optimized_mir`) may cause us to load
        // data from the incremental cache. If this causes us to deserialize a `Span`,
        // then we may load additional `SyntaxContext`s into the global `HygieneData`.
        // Therefore, we need to encode the hygiene data last to ensure that we encode
        // any `SyntaxContext`s that might be used.
        i = self.position();
        let (syntax_contexts, expn_data, expn_hashes) = self.encode_hygiene();
        let hygiene_bytes = self.position() - i;

        i = self.position();
        let def_path_hash_map = self.encode_def_path_hash_map();
        let def_path_hash_map_bytes = self.position() - i;

        // Encode source_map. This needs to be done last,
        // since encoding `Span`s tells us which `SourceFiles` we actually
        // need to encode.
        i = self.position();
        let source_map = self.encode_source_map();
        let source_map_bytes = self.position() - i;

        i = self.position();
        let attrs = tcx.hir().krate_attrs();
        let has_default_lib_allocator = tcx.sess.contains_name(&attrs, sym::default_lib_allocator);
        let root = self.lazy(CrateRoot {
            name: tcx.crate_name(LOCAL_CRATE),
            extra_filename: tcx.sess.opts.cg.extra_filename.clone(),
            triple: tcx.sess.opts.target_triple.clone(),
            hash: tcx.crate_hash(LOCAL_CRATE),
            stable_crate_id: tcx.def_path_hash(LOCAL_CRATE.as_def_id()).stable_crate_id(),
            required_panic_strategy: tcx.required_panic_strategy(LOCAL_CRATE),
            panic_in_drop_strategy: tcx.sess.opts.unstable_opts.panic_in_drop,
            edition: tcx.sess.edition(),
            has_global_allocator: tcx.has_global_allocator(LOCAL_CRATE),
            has_panic_handler: tcx.has_panic_handler(LOCAL_CRATE),
            has_default_lib_allocator,
            proc_macro_data,
            debugger_visualizers,
            compiler_builtins: tcx.sess.contains_name(&attrs, sym::compiler_builtins),
            needs_allocator: tcx.sess.contains_name(&attrs, sym::needs_allocator),
            needs_panic_runtime: tcx.sess.contains_name(&attrs, sym::needs_panic_runtime),
            no_builtins: tcx.sess.contains_name(&attrs, sym::no_builtins),
            panic_runtime: tcx.sess.contains_name(&attrs, sym::panic_runtime),
            profiler_runtime: tcx.sess.contains_name(&attrs, sym::profiler_runtime),
            symbol_mangling_version: tcx.sess.opts.get_symbol_mangling_version(),

            crate_deps,
            dylib_dependency_formats,
            lib_features,
            stability_implications,
            lang_items,
            diagnostic_items,
            lang_items_missing,
            native_libraries,
            foreign_modules,
            source_map,
            traits,
            impls,
            incoherent_impls,
            exported_symbols,
            interpret_alloc_index,
            tables,
            syntax_contexts,
            expn_data,
            expn_hashes,
            def_path_hash_map,
        });
        let final_bytes = self.position() - i;

        let total_bytes = self.position();

        let computed_total_bytes = preamble_bytes
            + dep_bytes
            + lib_feature_bytes
            + stability_implications_bytes
            + lang_item_bytes
            + diagnostic_item_bytes
            + native_lib_bytes
            + foreign_modules_bytes
            + def_path_table_bytes
            + traits_bytes
            + impls_bytes
            + incoherent_impls_bytes
            + mir_bytes
            + item_bytes
            + interpret_alloc_index_bytes
            + proc_macro_data_bytes
            + tables_bytes
            + debugger_visualizers_bytes
            + exported_symbols_bytes
            + hygiene_bytes
            + def_path_hash_map_bytes
            + source_map_bytes
            + final_bytes;
        assert_eq!(total_bytes, computed_total_bytes);

        if tcx.sess.meta_stats() {
            self.opaque.flush();

            // Rewind and re-read all the metadata to count the zero bytes we wrote.
            let pos_before_rewind = self.opaque.file().stream_position().unwrap();
            let mut zero_bytes = 0;
            self.opaque.file().rewind().unwrap();
            let file = std::io::BufReader::new(self.opaque.file());
            for e in file.bytes() {
                if e.unwrap() == 0 {
                    zero_bytes += 1;
                }
            }
            assert_eq!(self.opaque.file().stream_position().unwrap(), pos_before_rewind);

            let perc = |bytes| (bytes * 100) as f64 / total_bytes as f64;
            let p = |label, bytes| {
                eprintln!("{:>21}: {:>8} bytes ({:4.1}%)", label, bytes, perc(bytes));
            };

            eprintln!("");
            eprintln!(
                "{} metadata bytes, of which {} bytes ({:.1}%) are zero",
                total_bytes,
                zero_bytes,
                perc(zero_bytes)
            );
            p("preamble", preamble_bytes);
            p("dep", dep_bytes);
            p("lib feature", lib_feature_bytes);
            p("stability_implications", stability_implications_bytes);
            p("lang item", lang_item_bytes);
            p("diagnostic item", diagnostic_item_bytes);
            p("native lib", native_lib_bytes);
            p("foreign modules", foreign_modules_bytes);
            p("def-path table", def_path_table_bytes);
            p("traits", traits_bytes);
            p("impls", impls_bytes);
            p("incoherent_impls", incoherent_impls_bytes);
            p("mir", mir_bytes);
            p("item", item_bytes);
            p("interpret_alloc_index", interpret_alloc_index_bytes);
            p("proc-macro-data", proc_macro_data_bytes);
            p("tables", tables_bytes);
            p("debugger visualizers", debugger_visualizers_bytes);
            p("exported symbols", exported_symbols_bytes);
            p("hygiene", hygiene_bytes);
            p("def-path hashes", def_path_hash_map_bytes);
            p("source_map", source_map_bytes);
            p("final", final_bytes);
            eprintln!("");
        }

        root
    }
}

fn should_encode_visibility(def_kind: DefKind) -> bool {
    match def_kind {
        DefKind::Mod
        | DefKind::Struct
        | DefKind::Union
        | DefKind::Enum
        | DefKind::Variant
        | DefKind::Trait
        | DefKind::TyAlias
        | DefKind::ForeignTy
        | DefKind::TraitAlias
        | DefKind::AssocTy
        | DefKind::Fn
        | DefKind::Const
        | DefKind::Static(..)
        | DefKind::Ctor(..)
        | DefKind::AssocFn
        | DefKind::AssocConst
        | DefKind::Macro(..)
        | DefKind::Use
        | DefKind::ForeignMod
        | DefKind::OpaqueTy
        | DefKind::Impl
        | DefKind::Field => true,
        DefKind::TyParam
        | DefKind::ConstParam
        | DefKind::LifetimeParam
        | DefKind::AnonConst
        | DefKind::InlineConst
        | DefKind::GlobalAsm
        | DefKind::Closure
        | DefKind::Generator
        | DefKind::ExternCrate => false,
    }
}

fn should_encode_stability(def_kind: DefKind) -> bool {
    match def_kind {
        DefKind::Mod
        | DefKind::Ctor(..)
        | DefKind::Variant
        | DefKind::Field
        | DefKind::Struct
        | DefKind::AssocTy
        | DefKind::AssocFn
        | DefKind::AssocConst
        | DefKind::TyParam
        | DefKind::ConstParam
        | DefKind::Static(..)
        | DefKind::Const
        | DefKind::Fn
        | DefKind::ForeignMod
        | DefKind::TyAlias
        | DefKind::OpaqueTy
        | DefKind::Enum
        | DefKind::Union
        | DefKind::Impl
        | DefKind::Trait
        | DefKind::TraitAlias
        | DefKind::Macro(..)
        | DefKind::ForeignTy => true,
        DefKind::Use
        | DefKind::LifetimeParam
        | DefKind::AnonConst
        | DefKind::InlineConst
        | DefKind::GlobalAsm
        | DefKind::Closure
        | DefKind::Generator
        | DefKind::ExternCrate => false,
    }
}

/// Whether we should encode MIR.
///
/// Computing, optimizing and encoding the MIR is a relatively expensive operation.
/// We want to avoid this work when not required. Therefore:
/// - we only compute `mir_for_ctfe` on items with const-eval semantics;
/// - we skip `optimized_mir` for check runs.
///
/// Return a pair, resp. for CTFE and for LLVM.
fn should_encode_mir(tcx: TyCtxt<'_>, def_id: LocalDefId) -> (bool, bool) {
    match tcx.def_kind(def_id) {
        // Constructors
        DefKind::Ctor(_, _) => {
            let mir_opt_base = tcx.sess.opts.output_types.should_codegen()
                || tcx.sess.opts.unstable_opts.always_encode_mir;
            (true, mir_opt_base)
        }
        // Constants
        DefKind::AnonConst
        | DefKind::InlineConst
        | DefKind::AssocConst
        | DefKind::Static(..)
        | DefKind::Const => (true, false),
        // Full-fledged functions
        DefKind::AssocFn | DefKind::Fn => {
            let generics = tcx.generics_of(def_id);
            let needs_inline = (generics.requires_monomorphization(tcx)
                || tcx.codegen_fn_attrs(def_id).requests_inline())
                && tcx.sess.opts.output_types.should_codegen();
            // The function has a `const` modifier or is in a `#[const_trait]`.
            let is_const_fn = tcx.is_const_fn_raw(def_id.to_def_id())
                || tcx.is_const_default_method(def_id.to_def_id());
            let always_encode_mir = tcx.sess.opts.unstable_opts.always_encode_mir;
            (is_const_fn, needs_inline || always_encode_mir)
        }
        // Closures can't be const fn.
        DefKind::Closure => {
            let generics = tcx.generics_of(def_id);
            let needs_inline = (generics.requires_monomorphization(tcx)
                || tcx.codegen_fn_attrs(def_id).requests_inline())
                && tcx.sess.opts.output_types.should_codegen();
            let always_encode_mir = tcx.sess.opts.unstable_opts.always_encode_mir;
            (false, needs_inline || always_encode_mir)
        }
        // Generators require optimized MIR to compute layout.
        DefKind::Generator => (false, true),
        // The others don't have MIR.
        _ => (false, false),
    }
}

fn should_encode_variances(def_kind: DefKind) -> bool {
    match def_kind {
        DefKind::Struct
        | DefKind::Union
        | DefKind::Enum
        | DefKind::Variant
        | DefKind::Fn
        | DefKind::Ctor(..)
        | DefKind::AssocFn => true,
        DefKind::Mod
        | DefKind::Field
        | DefKind::AssocTy
        | DefKind::AssocConst
        | DefKind::TyParam
        | DefKind::ConstParam
        | DefKind::Static(..)
        | DefKind::Const
        | DefKind::ForeignMod
        | DefKind::TyAlias
        | DefKind::OpaqueTy
        | DefKind::Impl
        | DefKind::Trait
        | DefKind::TraitAlias
        | DefKind::Macro(..)
        | DefKind::ForeignTy
        | DefKind::Use
        | DefKind::LifetimeParam
        | DefKind::AnonConst
        | DefKind::InlineConst
        | DefKind::GlobalAsm
        | DefKind::Closure
        | DefKind::Generator
        | DefKind::ExternCrate => false,
    }
}

fn should_encode_generics(def_kind: DefKind) -> bool {
    match def_kind {
        DefKind::Struct
        | DefKind::Union
        | DefKind::Enum
        | DefKind::Variant
        | DefKind::Trait
        | DefKind::TyAlias
        | DefKind::ForeignTy
        | DefKind::TraitAlias
        | DefKind::AssocTy
        | DefKind::Fn
        | DefKind::Const
        | DefKind::Static(..)
        | DefKind::Ctor(..)
        | DefKind::AssocFn
        | DefKind::AssocConst
        | DefKind::AnonConst
        | DefKind::InlineConst
        | DefKind::OpaqueTy
        | DefKind::Impl
        | DefKind::Field
        | DefKind::TyParam
        | DefKind::Closure
        | DefKind::Generator => true,
        DefKind::Mod
        | DefKind::ForeignMod
        | DefKind::ConstParam
        | DefKind::Macro(..)
        | DefKind::Use
        | DefKind::LifetimeParam
        | DefKind::GlobalAsm
        | DefKind::ExternCrate => false,
    }
}

impl<'a, 'tcx> EncodeContext<'a, 'tcx> {
    fn encode_attrs(&mut self, def_id: LocalDefId) {
        let mut attrs = self
            .tcx
            .hir()
            .attrs(self.tcx.hir().local_def_id_to_hir_id(def_id))
            .iter()
            .filter(|attr| !rustc_feature::is_builtin_only_local(attr.name_or_empty()));

        record_array!(self.tables.attributes[def_id.to_def_id()] <- attrs.clone());
        if attrs.any(|attr| attr.may_have_doc_links()) {
            self.tables.may_have_doc_links.set(def_id.local_def_index, ());
        }
    }

    fn encode_def_ids(&mut self) {
        if self.is_proc_macro {
            return;
        }
        let tcx = self.tcx;
        for local_id in tcx.iter_local_def_id() {
            let def_id = local_id.to_def_id();
            let def_kind = tcx.opt_def_kind(local_id);
            let Some(def_kind) = def_kind else { continue };
            self.tables.opt_def_kind.set(def_id.index, def_kind);
            record!(self.tables.def_span[def_id] <- tcx.def_span(def_id));
            self.encode_attrs(local_id);
            record!(self.tables.expn_that_defined[def_id] <- self.tcx.expn_that_defined(def_id));
            if let Some(ident_span) = tcx.def_ident_span(def_id) {
                record!(self.tables.def_ident_span[def_id] <- ident_span);
            }
            if def_kind.has_codegen_attrs() {
                record!(self.tables.codegen_fn_attrs[def_id] <- self.tcx.codegen_fn_attrs(def_id));
            }
            if should_encode_visibility(def_kind) {
                record!(self.tables.visibility[def_id] <- self.tcx.visibility(def_id));
            }
            if should_encode_stability(def_kind) {
                self.encode_stability(def_id);
                self.encode_const_stability(def_id);
                self.encode_deprecation(def_id);
            }
            if should_encode_variances(def_kind) {
                let v = self.tcx.variances_of(def_id);
                record_array!(self.tables.variances_of[def_id] <- v);
            }
            if should_encode_generics(def_kind) {
                let g = tcx.generics_of(def_id);
                record!(self.tables.generics_of[def_id] <- g);
                record!(self.tables.explicit_predicates_of[def_id] <- self.tcx.explicit_predicates_of(def_id));
                let inferred_outlives = self.tcx.inferred_outlives_of(def_id);
                if !inferred_outlives.is_empty() {
                    record_array!(self.tables.inferred_outlives_of[def_id] <- inferred_outlives);
                }
            }
            if let DefKind::Trait | DefKind::TraitAlias = def_kind {
                record!(self.tables.super_predicates_of[def_id] <- self.tcx.super_predicates_of(def_id));
            }
        }
        let inherent_impls = tcx.crate_inherent_impls(());
        for (def_id, implementations) in inherent_impls.inherent_impls.iter() {
            if implementations.is_empty() {
                continue;
            }
            record_array!(self.tables.inherent_impls[def_id.to_def_id()] <- implementations.iter().map(|&def_id| {
                assert!(def_id.is_local());
                def_id.index
            }));
        }
    }

    fn encode_item_type(&mut self, def_id: DefId) {
        debug!("EncodeContext::encode_item_type({:?})", def_id);
        record!(self.tables.type_of[def_id] <- self.tcx.type_of(def_id));
    }

    fn encode_enum_variant_info(&mut self, def: ty::AdtDef<'tcx>, index: VariantIdx) {
        let tcx = self.tcx;
        let variant = &def.variant(index);
        let def_id = variant.def_id;
        debug!("EncodeContext::encode_enum_variant_info({:?})", def_id);

        let data = VariantData {
            ctor_kind: variant.ctor_kind,
            discr: variant.discr,
            ctor: variant.ctor_def_id.map(|did| did.index),
            is_non_exhaustive: variant.is_field_list_non_exhaustive(),
        };

        record!(self.tables.kind[def_id] <- EntryKind::Variant(self.lazy(data)));
        self.tables.constness.set(def_id.index, hir::Constness::Const);
        record_array!(self.tables.children[def_id] <- variant.fields.iter().map(|f| {
            assert!(f.did.is_local());
            f.did.index
        }));
        self.encode_item_type(def_id);
        if variant.ctor_kind == CtorKind::Fn {
            // FIXME(eddyb) encode signature only in `encode_enum_variant_ctor`.
            if let Some(ctor_def_id) = variant.ctor_def_id {
                record!(self.tables.fn_sig[def_id] <- tcx.fn_sig(ctor_def_id));
            }
        }
    }

    fn encode_enum_variant_ctor(&mut self, def: ty::AdtDef<'tcx>, index: VariantIdx) {
        let tcx = self.tcx;
        let variant = &def.variant(index);
        let def_id = variant.ctor_def_id.unwrap();
        debug!("EncodeContext::encode_enum_variant_ctor({:?})", def_id);

        // FIXME(eddyb) encode only the `CtorKind` for constructors.
        let data = VariantData {
            ctor_kind: variant.ctor_kind,
            discr: variant.discr,
            ctor: Some(def_id.index),
            is_non_exhaustive: variant.is_field_list_non_exhaustive(),
        };

        record!(self.tables.kind[def_id] <- EntryKind::Variant(self.lazy(data)));
        self.tables.constness.set(def_id.index, hir::Constness::Const);
        self.encode_item_type(def_id);
        if variant.ctor_kind == CtorKind::Fn {
            record!(self.tables.fn_sig[def_id] <- tcx.fn_sig(def_id));
        }
    }

    fn encode_info_for_mod(&mut self, local_def_id: LocalDefId, md: &hir::Mod<'_>) {
        let tcx = self.tcx;
        let def_id = local_def_id.to_def_id();
        debug!("EncodeContext::encode_info_for_mod({:?})", def_id);

        // If we are encoding a proc-macro crates, `encode_info_for_mod` will
        // only ever get called for the crate root. We still want to encode
        // the crate root for consistency with other crates (some of the resolver
        // code uses it). However, we skip encoding anything relating to child
        // items - we encode information about proc-macros later on.
        let reexports = if !self.is_proc_macro {
            match tcx.module_reexports(local_def_id) {
                Some(exports) => self.lazy_array(exports),
                _ => LazyArray::empty(),
            }
        } else {
            LazyArray::empty()
        };

        record!(self.tables.kind[def_id] <- EntryKind::Mod(reexports));
        if self.is_proc_macro {
            // Encode this here because we don't do it in encode_def_ids.
            record!(self.tables.expn_that_defined[def_id] <- tcx.expn_that_defined(local_def_id));
        } else {
            record_array!(self.tables.children[def_id] <- iter::from_generator(|| {
                for item_id in md.item_ids {
                    match tcx.hir().item(*item_id).kind {
                        // Foreign items are planted into their parent modules
                        // from name resolution point of view.
                        hir::ItemKind::ForeignMod { items, .. } => {
                            for foreign_item in items {
                                yield foreign_item.id.def_id.local_def_index;
                            }
                        }
                        // Only encode named non-reexport children, reexports are encoded
                        // separately and unnamed items are not used by name resolution.
                        hir::ItemKind::ExternCrate(..) => continue,
                        _ if tcx.def_key(item_id.def_id.to_def_id()).get_opt_name().is_some() => {
                            yield item_id.def_id.local_def_index;
                        }
                        _ => continue,
                    }
                }
            }));
        }
    }

    fn encode_field(
        &mut self,
        adt_def: ty::AdtDef<'tcx>,
        variant_index: VariantIdx,
        field_index: usize,
    ) {
        let variant = &adt_def.variant(variant_index);
        let field = &variant.fields[field_index];

        let def_id = field.did;
        debug!("EncodeContext::encode_field({:?})", def_id);

        record!(self.tables.kind[def_id] <- EntryKind::Field);
        self.encode_item_type(def_id);
    }

    fn encode_struct_ctor(&mut self, adt_def: ty::AdtDef<'tcx>, def_id: DefId) {
        debug!("EncodeContext::encode_struct_ctor({:?})", def_id);
        let tcx = self.tcx;
        let variant = adt_def.non_enum_variant();

        let data = VariantData {
            ctor_kind: variant.ctor_kind,
            discr: variant.discr,
            ctor: Some(def_id.index),
            is_non_exhaustive: variant.is_field_list_non_exhaustive(),
        };

        record!(self.tables.repr_options[def_id] <- adt_def.repr());
        self.tables.constness.set(def_id.index, hir::Constness::Const);
        record!(self.tables.kind[def_id] <- EntryKind::Struct(self.lazy(data)));
        self.encode_item_type(def_id);
        if variant.ctor_kind == CtorKind::Fn {
            record!(self.tables.fn_sig[def_id] <- tcx.fn_sig(def_id));
        }
    }

    fn encode_explicit_item_bounds(&mut self, def_id: DefId) {
        debug!("EncodeContext::encode_explicit_item_bounds({:?})", def_id);
        let bounds = self.tcx.explicit_item_bounds(def_id);
        if !bounds.is_empty() {
            record_array!(self.tables.explicit_item_bounds[def_id] <- bounds);
        }
    }

    fn encode_info_for_trait_item(&mut self, def_id: DefId) {
        debug!("EncodeContext::encode_info_for_trait_item({:?})", def_id);
        let tcx = self.tcx;

        let ast_item = tcx.hir().expect_trait_item(def_id.expect_local());
        self.tables.impl_defaultness.set(def_id.index, ast_item.defaultness);
        let trait_item = tcx.associated_item(def_id);

        match trait_item.kind {
            ty::AssocKind::Const => {
                let rendered = rustc_hir_pretty::to_string(
                    &(&self.tcx.hir() as &dyn intravisit::Map<'_>),
                    |s| s.print_trait_item(ast_item),
                );

                record!(self.tables.kind[def_id] <- EntryKind::AssocConst(ty::AssocItemContainer::TraitContainer));
                record!(self.tables.mir_const_qualif[def_id] <- mir::ConstQualifs::default());
                record!(self.tables.rendered_const[def_id] <- rendered);
            }
            ty::AssocKind::Fn => {
                let hir::TraitItemKind::Fn(m_sig, m) = &ast_item.kind else { bug!() };
                match *m {
                    hir::TraitFn::Required(ref names) => {
                        record_array!(self.tables.fn_arg_names[def_id] <- *names)
                    }
                    hir::TraitFn::Provided(body) => {
                        record_array!(self.tables.fn_arg_names[def_id] <- self.tcx.hir().body_param_names(body))
                    }
                };
                self.tables.asyncness.set(def_id.index, m_sig.header.asyncness);
                self.tables.constness.set(def_id.index, hir::Constness::NotConst);
                record!(self.tables.kind[def_id] <- EntryKind::AssocFn {
                    container: ty::AssocItemContainer::TraitContainer,
                    has_self: trait_item.fn_has_self_parameter,
                });
            }
            ty::AssocKind::Type => {
                self.encode_explicit_item_bounds(def_id);
                record!(self.tables.kind[def_id] <- EntryKind::AssocType(ty::AssocItemContainer::TraitContainer));
            }
        }
        match trait_item.kind {
            ty::AssocKind::Const | ty::AssocKind::Fn => {
                self.encode_item_type(def_id);
            }
            ty::AssocKind::Type => {
                if ast_item.defaultness.has_value() {
                    self.encode_item_type(def_id);
                }
            }
        }
        if trait_item.kind == ty::AssocKind::Fn {
            record!(self.tables.fn_sig[def_id] <- tcx.fn_sig(def_id));
        }
    }

    fn encode_info_for_impl_item(&mut self, def_id: DefId) {
        debug!("EncodeContext::encode_info_for_impl_item({:?})", def_id);
        let tcx = self.tcx;

        let ast_item = self.tcx.hir().expect_impl_item(def_id.expect_local());
        self.tables.impl_defaultness.set(def_id.index, ast_item.defaultness);
        let impl_item = self.tcx.associated_item(def_id);

        match impl_item.kind {
            ty::AssocKind::Const => {
                if let hir::ImplItemKind::Const(_, body_id) = ast_item.kind {
                    let qualifs = self.tcx.at(ast_item.span).mir_const_qualif(def_id);
                    let const_data = self.encode_rendered_const_for_body(body_id);

                    record!(self.tables.kind[def_id] <- EntryKind::AssocConst(ty::AssocItemContainer::ImplContainer));
                    record!(self.tables.mir_const_qualif[def_id] <- qualifs);
                    record!(self.tables.rendered_const[def_id] <- const_data);
                } else {
                    bug!()
                }
            }
            ty::AssocKind::Fn => {
                let hir::ImplItemKind::Fn(ref sig, body) = ast_item.kind else { bug!() };
                self.tables.asyncness.set(def_id.index, sig.header.asyncness);
                record_array!(self.tables.fn_arg_names[def_id] <- self.tcx.hir().body_param_names(body));
                // Can be inside `impl const Trait`, so using sig.header.constness is not reliable
                let constness = if self.tcx.is_const_fn_raw(def_id) {
                    hir::Constness::Const
                } else {
                    hir::Constness::NotConst
                };
                self.tables.constness.set(def_id.index, constness);
                record!(self.tables.kind[def_id] <- EntryKind::AssocFn {
                    container: ty::AssocItemContainer::ImplContainer,
                    has_self: impl_item.fn_has_self_parameter,
                });
            }
            ty::AssocKind::Type => {
                record!(self.tables.kind[def_id] <- EntryKind::AssocType(ty::AssocItemContainer::ImplContainer));
            }
        }
        self.encode_item_type(def_id);
        if let Some(trait_item_def_id) = impl_item.trait_item_def_id {
            self.tables.trait_item_def_id.set(def_id.index, trait_item_def_id.into());
        }
        if impl_item.kind == ty::AssocKind::Fn {
            record!(self.tables.fn_sig[def_id] <- tcx.fn_sig(def_id));
            if tcx.is_intrinsic(def_id) {
                self.tables.is_intrinsic.set(def_id.index, ());
            }
        }
    }

    fn encode_mir(&mut self) {
        if self.is_proc_macro {
            return;
        }

        let keys_and_jobs = self
            .tcx
            .mir_keys(())
            .iter()
            .filter_map(|&def_id| {
                let (encode_const, encode_opt) = should_encode_mir(self.tcx, def_id);
                if encode_const || encode_opt {
                    Some((def_id, encode_const, encode_opt))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for (def_id, encode_const, encode_opt) in keys_and_jobs.into_iter() {
            debug_assert!(encode_const || encode_opt);

            debug!("EntryBuilder::encode_mir({:?})", def_id);
            if encode_opt {
                record!(self.tables.optimized_mir[def_id.to_def_id()] <- self.tcx.optimized_mir(def_id));
            }
            if encode_const {
                record!(self.tables.mir_for_ctfe[def_id.to_def_id()] <- self.tcx.mir_for_ctfe(def_id));

                // FIXME(generic_const_exprs): this feels wrong to have in `encode_mir`
                let abstract_const = self.tcx.thir_abstract_const(def_id);
                if let Ok(Some(abstract_const)) = abstract_const {
                    record!(self.tables.thir_abstract_const[def_id.to_def_id()] <- abstract_const);
                }
            }
            record!(self.tables.promoted_mir[def_id.to_def_id()] <- self.tcx.promoted_mir(def_id));

            let instance =
                ty::InstanceDef::Item(ty::WithOptConstParam::unknown(def_id.to_def_id()));
            let unused = self.tcx.unused_generic_params(instance);
            if !unused.is_empty() {
                record!(self.tables.unused_generic_params[def_id.to_def_id()] <- unused);
            }
        }
    }

    fn encode_stability(&mut self, def_id: DefId) {
        debug!("EncodeContext::encode_stability({:?})", def_id);

        // The query lookup can take a measurable amount of time in crates with many items. Check if
        // the stability attributes are even enabled before using their queries.
        if self.feat.staged_api || self.tcx.sess.opts.unstable_opts.force_unstable_if_unmarked {
            if let Some(stab) = self.tcx.lookup_stability(def_id) {
                record!(self.tables.lookup_stability[def_id] <- stab)
            }
        }
    }

    fn encode_const_stability(&mut self, def_id: DefId) {
        debug!("EncodeContext::encode_const_stability({:?})", def_id);

        // The query lookup can take a measurable amount of time in crates with many items. Check if
        // the stability attributes are even enabled before using their queries.
        if self.feat.staged_api || self.tcx.sess.opts.unstable_opts.force_unstable_if_unmarked {
            if let Some(stab) = self.tcx.lookup_const_stability(def_id) {
                record!(self.tables.lookup_const_stability[def_id] <- stab)
            }
        }
    }

    fn encode_deprecation(&mut self, def_id: DefId) {
        debug!("EncodeContext::encode_deprecation({:?})", def_id);
        if let Some(depr) = self.tcx.lookup_deprecation(def_id) {
            record!(self.tables.lookup_deprecation_entry[def_id] <- depr);
        }
    }

    fn encode_rendered_const_for_body(&mut self, body_id: hir::BodyId) -> String {
        let hir = self.tcx.hir();
        let body = hir.body(body_id);
        rustc_hir_pretty::to_string(&(&hir as &dyn intravisit::Map<'_>), |s| {
            s.print_expr(&body.value)
        })
    }

    fn encode_info_for_item(&mut self, def_id: DefId, item: &'tcx hir::Item<'tcx>) {
        let tcx = self.tcx;

        debug!("EncodeContext::encode_info_for_item({:?})", def_id);

        let entry_kind = match item.kind {
            hir::ItemKind::Static(..) => EntryKind::Static,
            hir::ItemKind::Const(_, body_id) => {
                let qualifs = self.tcx.at(item.span).mir_const_qualif(def_id);
                let const_data = self.encode_rendered_const_for_body(body_id);
                record!(self.tables.mir_const_qualif[def_id] <- qualifs);
                record!(self.tables.rendered_const[def_id] <- const_data);
                EntryKind::Const
            }
            hir::ItemKind::Fn(ref sig, .., body) => {
                self.tables.asyncness.set(def_id.index, sig.header.asyncness);
                record_array!(self.tables.fn_arg_names[def_id] <- self.tcx.hir().body_param_names(body));
                self.tables.constness.set(def_id.index, sig.header.constness);
                EntryKind::Fn
            }
            hir::ItemKind::Macro(ref macro_def, _) => {
                EntryKind::MacroDef(self.lazy(&*macro_def.body), macro_def.macro_rules)
            }
            hir::ItemKind::Mod(ref m) => {
                return self.encode_info_for_mod(item.def_id, m);
            }
            hir::ItemKind::ForeignMod { .. } => EntryKind::ForeignMod,
            hir::ItemKind::GlobalAsm(..) => EntryKind::GlobalAsm,
            hir::ItemKind::TyAlias(..) => EntryKind::Type,
            hir::ItemKind::OpaqueTy(..) => {
                self.encode_explicit_item_bounds(def_id);
                EntryKind::OpaqueTy
            }
            hir::ItemKind::Enum(..) => {
                let adt_def = self.tcx.adt_def(def_id);
                record!(self.tables.repr_options[def_id] <- adt_def.repr());
                EntryKind::Enum
            }
            hir::ItemKind::Struct(ref struct_def, _) => {
                let adt_def = self.tcx.adt_def(def_id);
                record!(self.tables.repr_options[def_id] <- adt_def.repr());
                self.tables.constness.set(def_id.index, hir::Constness::Const);

                // Encode def_ids for each field and method
                // for methods, write all the stuff get_trait_method
                // needs to know
                let ctor = struct_def
                    .ctor_hir_id()
                    .map(|ctor_hir_id| self.tcx.hir().local_def_id(ctor_hir_id).local_def_index);

                let variant = adt_def.non_enum_variant();
                EntryKind::Struct(self.lazy(VariantData {
                    ctor_kind: variant.ctor_kind,
                    discr: variant.discr,
                    ctor,
                    is_non_exhaustive: variant.is_field_list_non_exhaustive(),
                }))
            }
            hir::ItemKind::Union(..) => {
                let adt_def = self.tcx.adt_def(def_id);
                record!(self.tables.repr_options[def_id] <- adt_def.repr());

                let variant = adt_def.non_enum_variant();
                EntryKind::Union(self.lazy(VariantData {
                    ctor_kind: variant.ctor_kind,
                    discr: variant.discr,
                    ctor: None,
                    is_non_exhaustive: variant.is_field_list_non_exhaustive(),
                }))
            }
            hir::ItemKind::Impl(hir::Impl { defaultness, constness, .. }) => {
                self.tables.impl_defaultness.set(def_id.index, *defaultness);
                self.tables.constness.set(def_id.index, *constness);

                let trait_ref = self.tcx.impl_trait_ref(def_id);
                if let Some(trait_ref) = trait_ref {
                    let trait_def = self.tcx.trait_def(trait_ref.def_id);
                    if let Some(mut an) = trait_def.ancestors(self.tcx, def_id).ok() {
                        if let Some(specialization_graph::Node::Impl(parent)) = an.nth(1) {
                            self.tables.impl_parent.set(def_id.index, parent.into());
                        }
                    }

                    // if this is an impl of `CoerceUnsized`, create its
                    // "unsized info", else just store None
                    if Some(trait_ref.def_id) == self.tcx.lang_items().coerce_unsized_trait() {
                        let coerce_unsized_info =
                            self.tcx.at(item.span).coerce_unsized_info(def_id);
                        record!(self.tables.coerce_unsized_info[def_id] <- coerce_unsized_info);
                    }
                }

                let polarity = self.tcx.impl_polarity(def_id);
                self.tables.impl_polarity.set(def_id.index, polarity);

                EntryKind::Impl
            }
            hir::ItemKind::Trait(..) => {
                let trait_def = self.tcx.trait_def(def_id);
                record!(self.tables.trait_def[def_id] <- trait_def);

                EntryKind::Trait
            }
            hir::ItemKind::TraitAlias(..) => {
                let trait_def = self.tcx.trait_def(def_id);
                record!(self.tables.trait_def[def_id] <- trait_def);

                EntryKind::TraitAlias
            }
            hir::ItemKind::ExternCrate(_) | hir::ItemKind::Use(..) => {
                bug!("cannot encode info for item {:?}", item)
            }
        };
        record!(self.tables.kind[def_id] <- entry_kind);
        // FIXME(eddyb) there should be a nicer way to do this.
        match item.kind {
            hir::ItemKind::Enum(..) => record_array!(self.tables.children[def_id] <-
                self.tcx.adt_def(def_id).variants().iter().map(|v| {
                    assert!(v.def_id.is_local());
                    v.def_id.index
                })
            ),
            hir::ItemKind::Struct(..) | hir::ItemKind::Union(..) => {
                record_array!(self.tables.children[def_id] <-
                    self.tcx.adt_def(def_id).non_enum_variant().fields.iter().map(|f| {
                        assert!(f.did.is_local());
                        f.did.index
                    })
                )
            }
            hir::ItemKind::Impl { .. } | hir::ItemKind::Trait(..) => {
                let associated_item_def_ids = self.tcx.associated_item_def_ids(def_id);
                record_array!(self.tables.children[def_id] <-
                    associated_item_def_ids.iter().map(|&def_id| {
                        assert!(def_id.is_local());
                        def_id.index
                    })
                );
            }
            _ => {}
        }
        match item.kind {
            hir::ItemKind::Static(..)
            | hir::ItemKind::Const(..)
            | hir::ItemKind::Fn(..)
            | hir::ItemKind::TyAlias(..)
            | hir::ItemKind::OpaqueTy(..)
            | hir::ItemKind::Enum(..)
            | hir::ItemKind::Struct(..)
            | hir::ItemKind::Union(..)
            | hir::ItemKind::Impl { .. } => self.encode_item_type(def_id),
            _ => {}
        }
        if let hir::ItemKind::Fn(..) = item.kind {
            record!(self.tables.fn_sig[def_id] <- tcx.fn_sig(def_id));
            if tcx.is_intrinsic(def_id) {
                self.tables.is_intrinsic.set(def_id.index, ());
            }
        }
        if let hir::ItemKind::Impl { .. } = item.kind {
            if let Some(trait_ref) = self.tcx.impl_trait_ref(def_id) {
                record!(self.tables.impl_trait_ref[def_id] <- trait_ref);
            }
        }
    }

    fn encode_info_for_generic_param(&mut self, def_id: DefId, kind: EntryKind, encode_type: bool) {
        record!(self.tables.kind[def_id] <- kind);
        if encode_type {
            self.encode_item_type(def_id);
        }
    }

    fn encode_info_for_closure(&mut self, hir_id: hir::HirId) {
        let def_id = self.tcx.hir().local_def_id(hir_id);
        debug!("EncodeContext::encode_info_for_closure({:?})", def_id);
        // NOTE(eddyb) `tcx.type_of(def_id)` isn't used because it's fully generic,
        // including on the signature, which is inferred in `typeck.
        let typeck_result: &'tcx ty::TypeckResults<'tcx> = self.tcx.typeck(def_id);
        let ty = typeck_result.node_type(hir_id);
        match ty.kind() {
            ty::Generator(..) => {
                let data = self.tcx.generator_kind(def_id).unwrap();
                let generator_diagnostic_data = typeck_result.get_generator_diagnostic_data();
                record!(self.tables.kind[def_id.to_def_id()] <- EntryKind::Generator);
                record!(self.tables.generator_kind[def_id.to_def_id()] <- data);
                record!(self.tables.generator_diagnostic_data[def_id.to_def_id()]  <- generator_diagnostic_data);
            }

            ty::Closure(..) => {
                record!(self.tables.kind[def_id.to_def_id()] <- EntryKind::Closure);
            }

            _ => bug!("closure that is neither generator nor closure"),
        }
        self.encode_item_type(def_id.to_def_id());
        if let ty::Closure(def_id, substs) = *ty.kind() {
            record!(self.tables.fn_sig[def_id] <- substs.as_closure().sig());
        }
    }

    fn encode_info_for_anon_const(&mut self, id: hir::HirId) {
        let def_id = self.tcx.hir().local_def_id(id);
        debug!("EncodeContext::encode_info_for_anon_const({:?})", def_id);
        let body_id = self.tcx.hir().body_owned_by(def_id);
        let const_data = self.encode_rendered_const_for_body(body_id);
        let qualifs = self.tcx.mir_const_qualif(def_id);

        record!(self.tables.kind[def_id.to_def_id()] <- EntryKind::AnonConst);
        record!(self.tables.mir_const_qualif[def_id.to_def_id()] <- qualifs);
        record!(self.tables.rendered_const[def_id.to_def_id()] <- const_data);
        self.encode_item_type(def_id.to_def_id());
    }

    fn encode_native_libraries(&mut self) -> LazyArray<NativeLib> {
        empty_proc_macro!(self);
        let used_libraries = self.tcx.native_libraries(LOCAL_CRATE);
        self.lazy_array(used_libraries.iter())
    }

    fn encode_foreign_modules(&mut self) -> LazyArray<ForeignModule> {
        empty_proc_macro!(self);
        let foreign_modules = self.tcx.foreign_modules(LOCAL_CRATE);
        self.lazy_array(foreign_modules.iter().map(|(_, m)| m).cloned())
    }

    fn encode_hygiene(&mut self) -> (SyntaxContextTable, ExpnDataTable, ExpnHashTable) {
        let mut syntax_contexts: TableBuilder<_, _> = Default::default();
        let mut expn_data_table: TableBuilder<_, _> = Default::default();
        let mut expn_hash_table: TableBuilder<_, _> = Default::default();

        self.hygiene_ctxt.encode(
            &mut (&mut *self, &mut syntax_contexts, &mut expn_data_table, &mut expn_hash_table),
            |(this, syntax_contexts, _, _), index, ctxt_data| {
                syntax_contexts.set(index, this.lazy(ctxt_data));
            },
            |(this, _, expn_data_table, expn_hash_table), index, expn_data, hash| {
                if let Some(index) = index.as_local() {
                    expn_data_table.set(index.as_raw(), this.lazy(expn_data));
                    expn_hash_table.set(index.as_raw(), this.lazy(hash));
                }
            },
        );

        (
            syntax_contexts.encode(&mut self.opaque),
            expn_data_table.encode(&mut self.opaque),
            expn_hash_table.encode(&mut self.opaque),
        )
    }

    fn encode_proc_macros(&mut self) -> Option<ProcMacroData> {
        let is_proc_macro = self.tcx.sess.crate_types().contains(&CrateType::ProcMacro);
        if is_proc_macro {
            let tcx = self.tcx;
            let hir = tcx.hir();

            let proc_macro_decls_static = tcx.proc_macro_decls_static(()).unwrap().local_def_index;
            let stability = tcx.lookup_stability(CRATE_DEF_ID);
            let macros =
                self.lazy_array(tcx.resolutions(()).proc_macros.iter().map(|p| p.local_def_index));
            let spans = self.tcx.sess.parse_sess.proc_macro_quoted_spans();
            for (i, span) in spans.into_iter().enumerate() {
                let span = self.lazy(span);
                self.tables.proc_macro_quoted_spans.set(i, span);
            }

            self.tables.opt_def_kind.set(LOCAL_CRATE.as_def_id().index, DefKind::Mod);
            record!(self.tables.def_span[LOCAL_CRATE.as_def_id()] <- tcx.def_span(LOCAL_CRATE.as_def_id()));
            self.encode_attrs(LOCAL_CRATE.as_def_id().expect_local());
            record!(self.tables.visibility[LOCAL_CRATE.as_def_id()] <- tcx.visibility(LOCAL_CRATE.as_def_id()));
            if let Some(stability) = stability {
                record!(self.tables.lookup_stability[LOCAL_CRATE.as_def_id()] <- stability);
            }
            self.encode_deprecation(LOCAL_CRATE.as_def_id());

            // Normally, this information is encoded when we walk the items
            // defined in this crate. However, we skip doing that for proc-macro crates,
            // so we manually encode just the information that we need
            for &proc_macro in &tcx.resolutions(()).proc_macros {
                let id = proc_macro;
                let proc_macro = hir.local_def_id_to_hir_id(proc_macro);
                let mut name = hir.name(proc_macro);
                let span = hir.span(proc_macro);
                // Proc-macros may have attributes like `#[allow_internal_unstable]`,
                // so downstream crates need access to them.
                let attrs = hir.attrs(proc_macro);
                let macro_kind = if tcx.sess.contains_name(attrs, sym::proc_macro) {
                    MacroKind::Bang
                } else if tcx.sess.contains_name(attrs, sym::proc_macro_attribute) {
                    MacroKind::Attr
                } else if let Some(attr) = tcx.sess.find_by_name(attrs, sym::proc_macro_derive) {
                    // This unwrap chain should have been checked by the proc-macro harness.
                    name = attr.meta_item_list().unwrap()[0]
                        .meta_item()
                        .unwrap()
                        .ident()
                        .unwrap()
                        .name;
                    MacroKind::Derive
                } else {
                    bug!("Unknown proc-macro type for item {:?}", id);
                };

                let mut def_key = self.tcx.hir().def_key(id);
                def_key.disambiguated_data.data = DefPathData::MacroNs(name);

                let def_id = id.to_def_id();
                self.tables.opt_def_kind.set(def_id.index, DefKind::Macro(macro_kind));
                record!(self.tables.kind[def_id] <- EntryKind::ProcMacro(macro_kind));
                self.encode_attrs(id);
                record!(self.tables.def_keys[def_id] <- def_key);
                record!(self.tables.def_ident_span[def_id] <- span);
                record!(self.tables.def_span[def_id] <- span);
                record!(self.tables.visibility[def_id] <- ty::Visibility::Public);
                if let Some(stability) = stability {
                    record!(self.tables.lookup_stability[def_id] <- stability);
                }
            }

            Some(ProcMacroData { proc_macro_decls_static, stability, macros })
        } else {
            None
        }
    }

    fn encode_debugger_visualizers(&mut self) -> LazyArray<DebuggerVisualizerFile> {
        empty_proc_macro!(self);
        self.lazy_array(self.tcx.debugger_visualizers(LOCAL_CRATE).iter())
    }

    fn encode_crate_deps(&mut self) -> LazyArray<CrateDep> {
        empty_proc_macro!(self);

        let deps = self
            .tcx
            .crates(())
            .iter()
            .map(|&cnum| {
                let dep = CrateDep {
                    name: self.tcx.crate_name(cnum),
                    hash: self.tcx.crate_hash(cnum),
                    host_hash: self.tcx.crate_host_hash(cnum),
                    kind: self.tcx.dep_kind(cnum),
                    extra_filename: self.tcx.extra_filename(cnum).clone(),
                };
                (cnum, dep)
            })
            .collect::<Vec<_>>();

        {
            // Sanity-check the crate numbers
            let mut expected_cnum = 1;
            for &(n, _) in &deps {
                assert_eq!(n, CrateNum::new(expected_cnum));
                expected_cnum += 1;
            }
        }

        // We're just going to write a list of crate 'name-hash-version's, with
        // the assumption that they are numbered 1 to n.
        // FIXME (#2166): This is not nearly enough to support correct versioning
        // but is enough to get transitive crate dependencies working.
        self.lazy_array(deps.iter().map(|&(_, ref dep)| dep))
    }

    fn encode_lib_features(&mut self) -> LazyArray<(Symbol, Option<Symbol>)> {
        empty_proc_macro!(self);
        let tcx = self.tcx;
        let lib_features = tcx.lib_features(());
        self.lazy_array(lib_features.to_vec())
    }

    fn encode_stability_implications(&mut self) -> LazyArray<(Symbol, Symbol)> {
        empty_proc_macro!(self);
        let tcx = self.tcx;
        let implications = tcx.stability_implications(LOCAL_CRATE);
        self.lazy_array(implications.iter().map(|(k, v)| (*k, *v)))
    }

    fn encode_diagnostic_items(&mut self) -> LazyArray<(Symbol, DefIndex)> {
        empty_proc_macro!(self);
        let tcx = self.tcx;
        let diagnostic_items = &tcx.diagnostic_items(LOCAL_CRATE).name_to_id;
        self.lazy_array(diagnostic_items.iter().map(|(&name, def_id)| (name, def_id.index)))
    }

    fn encode_lang_items(&mut self) -> LazyArray<(DefIndex, usize)> {
        empty_proc_macro!(self);
        let tcx = self.tcx;
        let lang_items = tcx.lang_items();
        let lang_items = lang_items.items().iter();
        self.lazy_array(lang_items.enumerate().filter_map(|(i, &opt_def_id)| {
            if let Some(def_id) = opt_def_id {
                if def_id.is_local() {
                    return Some((def_id.index, i));
                }
            }
            None
        }))
    }

    fn encode_lang_items_missing(&mut self) -> LazyArray<lang_items::LangItem> {
        empty_proc_macro!(self);
        let tcx = self.tcx;
        self.lazy_array(&tcx.lang_items().missing)
    }

    fn encode_traits(&mut self) -> LazyArray<DefIndex> {
        empty_proc_macro!(self);
        self.lazy_array(self.tcx.traits_in_crate(LOCAL_CRATE).iter().map(|def_id| def_id.index))
    }

    /// Encodes an index, mapping each trait to its (local) implementations.
    fn encode_impls(&mut self) -> LazyArray<TraitImpls> {
        debug!("EncodeContext::encode_traits_and_impls()");
        empty_proc_macro!(self);
        let tcx = self.tcx;
        let mut fx_hash_map: FxHashMap<DefId, Vec<(DefIndex, Option<SimplifiedType>)>> =
            FxHashMap::default();

        for id in tcx.hir().items() {
            if matches!(tcx.def_kind(id.def_id), DefKind::Impl) {
                if let Some(trait_ref) = tcx.impl_trait_ref(id.def_id.to_def_id()) {
                    let simplified_self_ty = fast_reject::simplify_type(
                        self.tcx,
                        trait_ref.self_ty(),
                        TreatParams::AsInfer,
                    );

                    fx_hash_map
                        .entry(trait_ref.def_id)
                        .or_default()
                        .push((id.def_id.local_def_index, simplified_self_ty));
                }
            }
        }

        let mut all_impls: Vec<_> = fx_hash_map.into_iter().collect();

        // Bring everything into deterministic order for hashing
        all_impls.sort_by_cached_key(|&(trait_def_id, _)| tcx.def_path_hash(trait_def_id));

        let all_impls: Vec<_> = all_impls
            .into_iter()
            .map(|(trait_def_id, mut impls)| {
                // Bring everything into deterministic order for hashing
                impls.sort_by_cached_key(|&(index, _)| {
                    tcx.hir().def_path_hash(LocalDefId { local_def_index: index })
                });

                TraitImpls {
                    trait_id: (trait_def_id.krate.as_u32(), trait_def_id.index),
                    impls: self.lazy_array(&impls),
                }
            })
            .collect();

        self.lazy_array(&all_impls)
    }

    fn encode_incoherent_impls(&mut self) -> LazyArray<IncoherentImpls> {
        debug!("EncodeContext::encode_traits_and_impls()");
        empty_proc_macro!(self);
        let tcx = self.tcx;
        let mut all_impls: Vec<_> = tcx.crate_inherent_impls(()).incoherent_impls.iter().collect();
        tcx.with_stable_hashing_context(|mut ctx| {
            all_impls.sort_by_cached_key(|&(&simp, _)| {
                let mut hasher = StableHasher::new();
                simp.hash_stable(&mut ctx, &mut hasher);
                hasher.finish::<Fingerprint>()
            })
        });
        let all_impls: Vec<_> = all_impls
            .into_iter()
            .map(|(&simp, impls)| {
                let mut impls: Vec<_> =
                    impls.into_iter().map(|def_id| def_id.local_def_index).collect();
                impls.sort_by_cached_key(|&local_def_index| {
                    tcx.hir().def_path_hash(LocalDefId { local_def_index })
                });

                IncoherentImpls { self_ty: simp, impls: self.lazy_array(impls) }
            })
            .collect();

        self.lazy_array(&all_impls)
    }

    // Encodes all symbols exported from this crate into the metadata.
    //
    // This pass is seeded off the reachability list calculated in the
    // middle::reachable module but filters out items that either don't have a
    // symbol associated with them (they weren't translated) or if they're an FFI
    // definition (as that's not defined in this crate).
    fn encode_exported_symbols(
        &mut self,
        exported_symbols: &[(ExportedSymbol<'tcx>, SymbolExportInfo)],
    ) -> LazyArray<(ExportedSymbol<'static>, SymbolExportInfo)> {
        empty_proc_macro!(self);
        // The metadata symbol name is special. It should not show up in
        // downstream crates.
        let metadata_symbol_name = SymbolName::new(self.tcx, &metadata_symbol_name(self.tcx));

        self.lazy_array(
            exported_symbols
                .iter()
                .filter(|&&(ref exported_symbol, _)| match *exported_symbol {
                    ExportedSymbol::NoDefId(symbol_name) => symbol_name != metadata_symbol_name,
                    _ => true,
                })
                .cloned(),
        )
    }

    fn encode_dylib_dependency_formats(&mut self) -> LazyArray<Option<LinkagePreference>> {
        empty_proc_macro!(self);
        let formats = self.tcx.dependency_formats(());
        for (ty, arr) in formats.iter() {
            if *ty != CrateType::Dylib {
                continue;
            }
            return self.lazy_array(arr.iter().map(|slot| match *slot {
                Linkage::NotLinked | Linkage::IncludedFromDylib => None,

                Linkage::Dynamic => Some(LinkagePreference::RequireDynamic),
                Linkage::Static => Some(LinkagePreference::RequireStatic),
            }));
        }
        LazyArray::empty()
    }

    fn encode_info_for_foreign_item(&mut self, def_id: DefId, nitem: &hir::ForeignItem<'_>) {
        let tcx = self.tcx;

        debug!("EncodeContext::encode_info_for_foreign_item({:?})", def_id);

        match nitem.kind {
            hir::ForeignItemKind::Fn(_, ref names, _) => {
                self.tables.asyncness.set(def_id.index, hir::IsAsync::NotAsync);
                record_array!(self.tables.fn_arg_names[def_id] <- *names);
                let constness = if self.tcx.is_const_fn_raw(def_id) {
                    hir::Constness::Const
                } else {
                    hir::Constness::NotConst
                };
                self.tables.constness.set(def_id.index, constness);
                record!(self.tables.kind[def_id] <- EntryKind::ForeignFn);
            }
            hir::ForeignItemKind::Static(..) => {
                record!(self.tables.kind[def_id] <- EntryKind::ForeignStatic);
            }
            hir::ForeignItemKind::Type => {
                record!(self.tables.kind[def_id] <- EntryKind::ForeignType);
            }
        }
        self.encode_item_type(def_id);
        if let hir::ForeignItemKind::Fn(..) = nitem.kind {
            record!(self.tables.fn_sig[def_id] <- tcx.fn_sig(def_id));
            if tcx.is_intrinsic(def_id) {
                self.tables.is_intrinsic.set(def_id.index, ());
            }
        }
    }
}

// FIXME(eddyb) make metadata encoding walk over all definitions, instead of HIR.
impl<'a, 'tcx> Visitor<'tcx> for EncodeContext<'a, 'tcx> {
    type NestedFilter = nested_filter::OnlyBodies;

    fn nested_visit_map(&mut self) -> Self::Map {
        self.tcx.hir()
    }
    fn visit_expr(&mut self, ex: &'tcx hir::Expr<'tcx>) {
        intravisit::walk_expr(self, ex);
        self.encode_info_for_expr(ex);
    }
    fn visit_anon_const(&mut self, c: &'tcx AnonConst) {
        intravisit::walk_anon_const(self, c);
        self.encode_info_for_anon_const(c.hir_id);
    }
    fn visit_item(&mut self, item: &'tcx hir::Item<'tcx>) {
        intravisit::walk_item(self, item);
        match item.kind {
            hir::ItemKind::ExternCrate(_) | hir::ItemKind::Use(..) => {} // ignore these
            _ => self.encode_info_for_item(item.def_id.to_def_id(), item),
        }
        self.encode_addl_info_for_item(item);
    }
    fn visit_foreign_item(&mut self, ni: &'tcx hir::ForeignItem<'tcx>) {
        intravisit::walk_foreign_item(self, ni);
        self.encode_info_for_foreign_item(ni.def_id.to_def_id(), ni);
    }
    fn visit_generics(&mut self, generics: &'tcx hir::Generics<'tcx>) {
        intravisit::walk_generics(self, generics);
        self.encode_info_for_generics(generics);
    }
}

impl<'a, 'tcx> EncodeContext<'a, 'tcx> {
    fn encode_fields(&mut self, adt_def: ty::AdtDef<'tcx>) {
        for (variant_index, variant) in adt_def.variants().iter_enumerated() {
            for (field_index, _field) in variant.fields.iter().enumerate() {
                self.encode_field(adt_def, variant_index, field_index);
            }
        }
    }

    fn encode_info_for_generics(&mut self, generics: &hir::Generics<'tcx>) {
        for param in generics.params {
            let def_id = self.tcx.hir().local_def_id(param.hir_id);
            match param.kind {
                GenericParamKind::Lifetime { .. } => continue,
                GenericParamKind::Type { default, .. } => {
                    self.encode_info_for_generic_param(
                        def_id.to_def_id(),
                        EntryKind::TypeParam,
                        default.is_some(),
                    );
                }
                GenericParamKind::Const { ref default, .. } => {
                    let def_id = def_id.to_def_id();
                    self.encode_info_for_generic_param(def_id, EntryKind::ConstParam, true);
                    if default.is_some() {
                        record!(self.tables.const_param_default[def_id] <- self.tcx.const_param_default(def_id))
                    }
                }
            }
        }
    }

    fn encode_info_for_expr(&mut self, expr: &hir::Expr<'_>) {
        if let hir::ExprKind::Closure { .. } = expr.kind {
            self.encode_info_for_closure(expr.hir_id);
        }
    }

    /// In some cases, along with the item itself, we also
    /// encode some sub-items. Usually we want some info from the item
    /// so it's easier to do that here then to wait until we would encounter
    /// normally in the visitor walk.
    fn encode_addl_info_for_item(&mut self, item: &hir::Item<'_>) {
        match item.kind {
            hir::ItemKind::Static(..)
            | hir::ItemKind::Const(..)
            | hir::ItemKind::Fn(..)
            | hir::ItemKind::Macro(..)
            | hir::ItemKind::Mod(..)
            | hir::ItemKind::ForeignMod { .. }
            | hir::ItemKind::GlobalAsm(..)
            | hir::ItemKind::ExternCrate(..)
            | hir::ItemKind::Use(..)
            | hir::ItemKind::TyAlias(..)
            | hir::ItemKind::OpaqueTy(..)
            | hir::ItemKind::TraitAlias(..) => {
                // no sub-item recording needed in these cases
            }
            hir::ItemKind::Enum(..) => {
                let def = self.tcx.adt_def(item.def_id.to_def_id());
                self.encode_fields(def);

                for (i, variant) in def.variants().iter_enumerated() {
                    self.encode_enum_variant_info(def, i);

                    if let Some(_ctor_def_id) = variant.ctor_def_id {
                        self.encode_enum_variant_ctor(def, i);
                    }
                }
            }
            hir::ItemKind::Struct(ref struct_def, _) => {
                let def = self.tcx.adt_def(item.def_id.to_def_id());
                self.encode_fields(def);

                // If the struct has a constructor, encode it.
                if let Some(ctor_hir_id) = struct_def.ctor_hir_id() {
                    let ctor_def_id = self.tcx.hir().local_def_id(ctor_hir_id);
                    self.encode_struct_ctor(def, ctor_def_id.to_def_id());
                }
            }
            hir::ItemKind::Union(..) => {
                let def = self.tcx.adt_def(item.def_id.to_def_id());
                self.encode_fields(def);
            }
            hir::ItemKind::Impl { .. } => {
                for &trait_item_def_id in
                    self.tcx.associated_item_def_ids(item.def_id.to_def_id()).iter()
                {
                    self.encode_info_for_impl_item(trait_item_def_id);
                }
            }
            hir::ItemKind::Trait(..) => {
                for &item_def_id in self.tcx.associated_item_def_ids(item.def_id.to_def_id()).iter()
                {
                    self.encode_info_for_trait_item(item_def_id);
                }
            }
        }
    }
}

/// Used to prefetch queries which will be needed later by metadata encoding.
/// Only a subset of the queries are actually prefetched to keep this code smaller.
fn prefetch_mir(tcx: TyCtxt<'_>) {
    if !tcx.sess.opts.output_types.should_codegen() {
        // We won't emit MIR, so don't prefetch it.
        return;
    }

    par_iter(tcx.mir_keys(())).for_each(|&def_id| {
        let (encode_const, encode_opt) = should_encode_mir(tcx, def_id);

        if encode_const {
            tcx.ensure().mir_for_ctfe(def_id);
        }
        if encode_opt {
            tcx.ensure().optimized_mir(def_id);
        }
        if encode_opt || encode_const {
            tcx.ensure().promoted_mir(def_id);
        }
    })
}

// NOTE(eddyb) The following comment was preserved for posterity, even
// though it's no longer relevant as EBML (which uses nested & tagged
// "documents") was replaced with a scheme that can't go out of bounds.
//
// And here we run into yet another obscure archive bug: in which metadata
// loaded from archives may have trailing garbage bytes. Awhile back one of
// our tests was failing sporadically on the macOS 64-bit builders (both nopt
// and opt) by having ebml generate an out-of-bounds panic when looking at
// metadata.
//
// Upon investigation it turned out that the metadata file inside of an rlib
// (and ar archive) was being corrupted. Some compilations would generate a
// metadata file which would end in a few extra bytes, while other
// compilations would not have these extra bytes appended to the end. These
// extra bytes were interpreted by ebml as an extra tag, so they ended up
// being interpreted causing the out-of-bounds.
//
// The root cause of why these extra bytes were appearing was never
// discovered, and in the meantime the solution we're employing is to insert
// the length of the metadata to the start of the metadata. Later on this
// will allow us to slice the metadata to the precise length that we just
// generated regardless of trailing bytes that end up in it.

pub struct EncodedMetadata {
    // The declaration order matters because `mmap` should be dropped before `_temp_dir`.
    mmap: Option<Mmap>,
    // We need to carry MaybeTempDir to avoid deleting the temporary
    // directory while accessing the Mmap.
    _temp_dir: Option<MaybeTempDir>,
}

impl EncodedMetadata {
    #[inline]
    pub fn from_path(path: PathBuf, temp_dir: Option<MaybeTempDir>) -> std::io::Result<Self> {
        let file = std::fs::File::open(&path)?;
        let file_metadata = file.metadata()?;
        if file_metadata.len() == 0 {
            return Ok(Self { mmap: None, _temp_dir: None });
        }
        let mmap = unsafe { Some(Mmap::map(file)?) };
        Ok(Self { mmap, _temp_dir: temp_dir })
    }

    #[inline]
    pub fn raw_data(&self) -> &[u8] {
        self.mmap.as_ref().map(|mmap| mmap.as_ref()).unwrap_or_default()
    }
}

impl<S: Encoder> Encodable<S> for EncodedMetadata {
    fn encode(&self, s: &mut S) {
        let slice = self.raw_data();
        slice.encode(s)
    }
}

impl<D: Decoder> Decodable<D> for EncodedMetadata {
    fn decode(d: &mut D) -> Self {
        let len = d.read_usize();
        let mmap = if len > 0 {
            let mut mmap = MmapMut::map_anon(len).unwrap();
            for _ in 0..len {
                (&mut mmap[..]).write(&[d.read_u8()]).unwrap();
            }
            mmap.flush().unwrap();
            Some(mmap.make_read_only().unwrap())
        } else {
            None
        };

        Self { mmap, _temp_dir: None }
    }
}

pub fn encode_metadata(tcx: TyCtxt<'_>, path: &Path) {
    let _prof_timer = tcx.prof.verbose_generic_activity("generate_crate_metadata");

    // Since encoding metadata is not in a query, and nothing is cached,
    // there's no need to do dep-graph tracking for any of it.
    tcx.dep_graph.assert_ignored();

    join(
        || encode_metadata_impl(tcx, path),
        || {
            if tcx.sess.threads() == 1 {
                return;
            }
            // Prefetch some queries used by metadata encoding.
            // This is not necessary for correctness, but is only done for performance reasons.
            // It can be removed if it turns out to cause trouble or be detrimental to performance.
            join(|| prefetch_mir(tcx), || tcx.exported_symbols(LOCAL_CRATE));
        },
    );
}

fn encode_metadata_impl(tcx: TyCtxt<'_>, path: &Path) {
    let mut encoder = opaque::FileEncoder::new(path)
        .unwrap_or_else(|err| tcx.sess.fatal(&format!("failed to create file encoder: {}", err)));
    encoder.emit_raw_bytes(METADATA_HEADER);

    // Will be filled with the root position after encoding everything.
    encoder.emit_raw_bytes(&[0, 0, 0, 0]);

    let source_map_files = tcx.sess.source_map().files();
    let source_file_cache = (source_map_files[0].clone(), 0);
    let required_source_files = Some(GrowableBitSet::with_capacity(source_map_files.len()));
    drop(source_map_files);

    let hygiene_ctxt = HygieneEncodeContext::default();

    let mut ecx = EncodeContext {
        opaque: encoder,
        tcx,
        feat: tcx.features(),
        tables: Default::default(),
        lazy_state: LazyState::NoNode,
        type_shorthands: Default::default(),
        predicate_shorthands: Default::default(),
        source_file_cache,
        interpret_allocs: Default::default(),
        required_source_files,
        is_proc_macro: tcx.sess.crate_types().contains(&CrateType::ProcMacro),
        hygiene_ctxt: &hygiene_ctxt,
    };

    // Encode the rustc version string in a predictable location.
    rustc_version().encode(&mut ecx);

    // Encode all the entries and extra information in the crate,
    // culminating in the `CrateRoot` which points to all of it.
    let root = ecx.encode_crate_root();

    ecx.opaque.flush();

    let mut file = ecx.opaque.file();
    // We will return to this position after writing the root position.
    let pos_before_seek = file.stream_position().unwrap();

    // Encode the root position.
    let header = METADATA_HEADER.len();
    file.seek(std::io::SeekFrom::Start(header as u64))
        .unwrap_or_else(|err| tcx.sess.fatal(&format!("failed to seek the file: {}", err)));
    let pos = root.position.get();
    file.write_all(&[(pos >> 24) as u8, (pos >> 16) as u8, (pos >> 8) as u8, (pos >> 0) as u8])
        .unwrap_or_else(|err| tcx.sess.fatal(&format!("failed to write to the file: {}", err)));

    // Return to the position where we are before writing the root position.
    file.seek(std::io::SeekFrom::Start(pos_before_seek)).unwrap();

    // Record metadata size for self-profiling
    tcx.prof.artifact_size(
        "crate_metadata",
        "crate_metadata",
        file.metadata().unwrap().len() as u64,
    );
}

pub fn provide(providers: &mut Providers) {
    *providers = Providers {
        traits_in_crate: |tcx, cnum| {
            assert_eq!(cnum, LOCAL_CRATE);

            let mut traits = Vec::new();
            for id in tcx.hir().items() {
                if matches!(tcx.def_kind(id.def_id), DefKind::Trait | DefKind::TraitAlias) {
                    traits.push(id.def_id.to_def_id())
                }
            }

            // Bring everything into deterministic order.
            traits.sort_by_cached_key(|&def_id| tcx.def_path_hash(def_id));
            tcx.arena.alloc_slice(&traits)
        },

        ..*providers
    }
}
