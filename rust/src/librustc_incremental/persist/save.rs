// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rbml::opaque::Encoder;
use rustc::dep_graph::DepNode;
use rustc::hir::def_id::DefId;
use rustc::middle::cstore::LOCAL_CRATE;
use rustc::session::Session;
use rustc::ty::TyCtxt;
use rustc_data_structures::fnv::FnvHashMap;
use rustc_serialize::Encodable as RustcEncodable;
use std::hash::{Hash, Hasher, SipHasher};
use std::io::{self, Cursor, Write};
use std::fs::{self, File};
use std::path::PathBuf;

use super::data::*;
use super::directory::*;
use super::hash::*;
use super::preds::*;
use super::util::*;

pub fn save_dep_graph<'a, 'tcx>(tcx: TyCtxt<'a, 'tcx, 'tcx>) {
    debug!("save_dep_graph()");
    let _ignore = tcx.dep_graph.in_ignore();
    let sess = tcx.sess;
    if sess.opts.incremental.is_none() {
        return;
    }
    let mut hcx = HashContext::new(tcx);
    let mut builder = DefIdDirectoryBuilder::new(tcx);
    let query = tcx.dep_graph.query();
    let preds = Predecessors::new(&query, &mut hcx);
    save_in(sess,
            dep_graph_path(tcx),
            |e| encode_dep_graph(&preds, &mut builder, e));
    save_in(sess,
            metadata_hash_path(tcx, LOCAL_CRATE),
            |e| encode_metadata_hashes(tcx, &preds, &mut builder, e));
}

pub fn save_work_products(sess: &Session, local_crate_name: &str) {
    debug!("save_work_products()");
    let _ignore = sess.dep_graph.in_ignore();
    let path = sess_work_products_path(sess, local_crate_name);
    save_in(sess, path, |e| encode_work_products(sess, e));
}

fn save_in<F>(sess: &Session, opt_path_buf: Option<PathBuf>, encode: F)
    where F: FnOnce(&mut Encoder) -> io::Result<()>
{
    let path_buf = match opt_path_buf {
        Some(p) => p,
        None => return,
    };

    // FIXME(#32754) lock file?

    // delete the old dep-graph, if any
    if path_buf.exists() {
        match fs::remove_file(&path_buf) {
            Ok(()) => {}
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
        Ok(_) => {}
        Err(err) => {
            sess.err(&format!("failed to write dep-graph to `{}`: {}",
                              path_buf.display(),
                              err));
            return;
        }
    }
}

pub fn encode_dep_graph(preds: &Predecessors,
                        builder: &mut DefIdDirectoryBuilder,
                        encoder: &mut Encoder)
                        -> io::Result<()> {
    // First encode the commandline arguments hash
    let tcx = builder.tcx();
    try!(tcx.sess.opts.dep_tracking_hash().encode(encoder));

    // Create a flat list of (Input, WorkProduct) edges for
    // serialization.
    let mut edges = vec![];
    for (&target, sources) in &preds.inputs {
        match *target {
            DepNode::MetaData(ref def_id) => {
                // Metadata *targets* are always local metadata nodes. We handle
                // those in `encode_metadata_hashes`, which comes later.
                assert!(def_id.is_local());
                continue;
            }
            _ => (),
        }
        let target = builder.map(target);
        for &source in sources {
            let source = builder.map(source);
            edges.push((source, target.clone()));
        }
    }

    // Create the serialized dep-graph.
    let graph = SerializedDepGraph {
        edges: edges,
        hashes: preds.hashes
            .iter()
            .map(|(&dep_node, &hash)| {
                SerializedHash {
                    dep_node: builder.map(dep_node),
                    hash: hash,
                }
            })
            .collect(),
    };

    debug!("graph = {:#?}", graph);

    // Encode the directory and then the graph data.
    try!(builder.directory().encode(encoder));
    try!(graph.encode(encoder));

    Ok(())
}

pub fn encode_metadata_hashes(tcx: TyCtxt,
                              preds: &Predecessors,
                              builder: &mut DefIdDirectoryBuilder,
                              encoder: &mut Encoder)
                              -> io::Result<()> {
    let mut def_id_hashes = FnvHashMap();
    let mut def_id_hash = |def_id: DefId| -> u64 {
        *def_id_hashes.entry(def_id)
            .or_insert_with(|| {
                let index = builder.add(def_id);
                let path = builder.lookup_def_path(index);
                path.deterministic_hash(tcx)
            })
    };

    // For each `MetaData(X)` node where `X` is local, accumulate a
    // hash.  These are the metadata items we export. Downstream
    // crates will want to see a hash that tells them whether we might
    // have changed the metadata for a given item since they last
    // compiled.
    //
    // (I initially wrote this with an iterator, but it seemed harder to read.)
    let mut serialized_hashes = SerializedMetadataHashes { hashes: vec![] };
    for (&target, sources) in &preds.inputs {
        let def_id = match *target {
            DepNode::MetaData(def_id) => {
                assert!(def_id.is_local());
                def_id
            }
            _ => continue,
        };

        // To create the hash for each item `X`, we don't hash the raw
        // bytes of the metadata (though in principle we
        // could). Instead, we walk the predecessors of `MetaData(X)`
        // from the dep-graph. This corresponds to all the inputs that
        // were read to construct the metadata. To create the hash for
        // the metadata, we hash (the hash of) all of those inputs.
        debug!("save: computing metadata hash for {:?}", def_id);

        // Create a vector containing a pair of (source-id, hash).
        // The source-id is stored as a `DepNode<u64>`, where the u64
        // is the det. hash of the def-path. This is convenient
        // because we can sort this to get a stable ordering across
        // compilations, even if the def-ids themselves have changed.
        let mut hashes: Vec<(DepNode<u64>, u64)> = sources.iter()
            .map(|dep_node| {
                let hash_dep_node = dep_node.map_def(|&def_id| Some(def_id_hash(def_id))).unwrap();
                let hash = preds.hashes[dep_node];
                (hash_dep_node, hash)
            })
            .collect();

        hashes.sort();
        let mut state = SipHasher::new();
        hashes.hash(&mut state);
        let hash = state.finish();

        debug!("save: metadata hash for {:?} is {}", def_id, hash);
        serialized_hashes.hashes.push(SerializedMetadataHash {
            def_index: def_id.index,
            hash: hash,
        });
    }

    // Encode everything.
    try!(serialized_hashes.encode(encoder));

    Ok(())
}

pub fn encode_work_products(sess: &Session, encoder: &mut Encoder) -> io::Result<()> {
    let work_products: Vec<_> = sess.dep_graph
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
