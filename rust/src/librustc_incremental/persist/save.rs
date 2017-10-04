// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rustc::dep_graph::DepGraph;
use rustc::hir::def_id::DefId;
use rustc::hir::svh::Svh;
use rustc::ich::Fingerprint;
use rustc::middle::cstore::EncodedMetadataHashes;
use rustc::session::Session;
use rustc::ty::TyCtxt;
use rustc::util::common::time;
use rustc::util::nodemap::DefIdMap;
use rustc_data_structures::fx::FxHashMap;
use rustc_serialize::Encodable as RustcEncodable;
use rustc_serialize::opaque::Encoder;
use std::io::{self, Cursor, Write};
use std::fs::{self, File};
use std::path::PathBuf;

use super::data::*;
use super::fs::*;
use super::dirty_clean;
use super::file_format;
use super::work_product;

use super::load::load_prev_metadata_hashes;

pub fn save_dep_graph<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                metadata_hashes: &EncodedMetadataHashes,
                                svh: Svh) {
    debug!("save_dep_graph()");
    let _ignore = tcx.dep_graph.in_ignore();
    let sess = tcx.sess;
    if sess.opts.incremental.is_none() {
        return;
    }

    // We load the previous metadata hashes now before overwriting the file
    // (if we need them for testing).
    let prev_metadata_hashes = if tcx.sess.opts.debugging_opts.query_dep_graph {
        load_prev_metadata_hashes(tcx)
    } else {
        DefIdMap()
    };

    let mut current_metadata_hashes = FxHashMap();

    if sess.opts.debugging_opts.incremental_cc ||
       sess.opts.debugging_opts.query_dep_graph {
        save_in(sess,
                metadata_hash_export_path(sess),
                |e| encode_metadata_hashes(tcx,
                                           svh,
                                           metadata_hashes,
                                           &mut current_metadata_hashes,
                                           e));
    }

    time(sess.time_passes(), "persist dep-graph", || {
        save_in(sess,
                dep_graph_path(sess),
                |e| encode_dep_graph(tcx, e));
    });

    dirty_clean::check_dirty_clean_annotations(tcx);
    dirty_clean::check_dirty_clean_metadata(tcx,
                                            &prev_metadata_hashes,
                                            &current_metadata_hashes);
}

pub fn save_work_products(sess: &Session, dep_graph: &DepGraph) {
    if sess.opts.incremental.is_none() {
        return;
    }

    debug!("save_work_products()");
    let _ignore = dep_graph.in_ignore();
    let path = work_products_path(sess);
    save_in(sess, path, |e| encode_work_products(dep_graph, e));

    // We also need to clean out old work-products, as not all of them are
    // deleted during invalidation. Some object files don't change their
    // content, they are just not needed anymore.
    let new_work_products = dep_graph.work_products();
    let previous_work_products = dep_graph.previous_work_products();

    for (id, wp) in previous_work_products.iter() {
        if !new_work_products.contains_key(id) {
            work_product::delete_workproduct_files(sess, wp);
            debug_assert!(wp.saved_files.iter().all(|&(_, ref file_name)| {
                !in_incr_comp_dir_sess(sess, file_name).exists()
            }));
        }
    }

    // Check that we did not delete one of the current work-products:
    debug_assert!({
        new_work_products.iter()
                         .flat_map(|(_, wp)| wp.saved_files
                                               .iter()
                                               .map(|&(_, ref name)| name))
                         .map(|name| in_incr_comp_dir_sess(sess, name))
                         .all(|path| path.exists())
    });
}

fn save_in<F>(sess: &Session, path_buf: PathBuf, encode: F)
    where F: FnOnce(&mut Encoder) -> io::Result<()>
{
    debug!("save: storing data in {}", path_buf.display());

    // delete the old dep-graph, if any
    // Note: It's important that we actually delete the old file and not just
    // truncate and overwrite it, since it might be a shared hard-link, the
    // underlying data of which we don't want to modify
    if path_buf.exists() {
        match fs::remove_file(&path_buf) {
            Ok(()) => {
                debug!("save: remove old file");
            }
            Err(err) => {
                sess.err(&format!("unable to delete old dep-graph at `{}`: {}",
                                  path_buf.display(),
                                  err));
                return;
            }
        }
    }

    // generate the data in a memory buffer
    let mut wr = Cursor::new(Vec::new());
    file_format::write_file_header(&mut wr).unwrap();
    match encode(&mut Encoder::new(&mut wr)) {
        Ok(()) => {}
        Err(err) => {
            sess.err(&format!("could not encode dep-graph to `{}`: {}",
                              path_buf.display(),
                              err));
            return;
        }
    }

    // write the data out
    let data = wr.into_inner();
    match File::create(&path_buf).and_then(|mut file| file.write_all(&data)) {
        Ok(_) => {
            debug!("save: data written to disk successfully");
        }
        Err(err) => {
            sess.err(&format!("failed to write dep-graph to `{}`: {}",
                              path_buf.display(),
                              err));
            return;
        }
    }
}

fn encode_dep_graph(tcx: TyCtxt,
                    encoder: &mut Encoder)
                    -> io::Result<()> {
    // First encode the commandline arguments hash
    tcx.sess.opts.dep_tracking_hash().encode(encoder)?;

    // Encode the graph data.
    let serialized_graph = tcx.dep_graph.serialize();
    serialized_graph.encode(encoder)?;

    Ok(())
}

fn encode_metadata_hashes(tcx: TyCtxt,
                          svh: Svh,
                          metadata_hashes: &EncodedMetadataHashes,
                          current_metadata_hashes: &mut FxHashMap<DefId, Fingerprint>,
                          encoder: &mut Encoder)
                          -> io::Result<()> {
    assert_eq!(metadata_hashes.hashes.len(),
        metadata_hashes.hashes.iter().map(|x| (x.def_index, ())).collect::<FxHashMap<_,_>>().len());

    let mut serialized_hashes = SerializedMetadataHashes {
        entry_hashes: metadata_hashes.hashes.to_vec(),
        index_map: FxHashMap()
    };

    if tcx.sess.opts.debugging_opts.query_dep_graph {
        for serialized_hash in &serialized_hashes.entry_hashes {
            let def_id = DefId::local(serialized_hash.def_index);

            // Store entry in the index_map
            let def_path_hash = tcx.def_path_hash(def_id);
            serialized_hashes.index_map.insert(def_id.index, def_path_hash);

            // Record hash in current_metadata_hashes
            current_metadata_hashes.insert(def_id, serialized_hash.hash);
        }

        debug!("save: stored index_map (len={}) for serialized hashes",
               serialized_hashes.index_map.len());
    }

    // Encode everything.
    svh.encode(encoder)?;
    serialized_hashes.encode(encoder)?;

    Ok(())
}

fn encode_work_products(dep_graph: &DepGraph,
                        encoder: &mut Encoder) -> io::Result<()> {
    let work_products: Vec<_> = dep_graph
        .work_products()
        .iter()
        .map(|(id, work_product)| {
            SerializedWorkProduct {
                id: id.clone(),
                work_product: work_product.clone(),
            }
        })
        .collect();

    work_products.encode(encoder)
}
