// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rustc::dep_graph::{DepGraphQuery, DepNode, DepKind};
use rustc::ich::Fingerprint;
use rustc_data_structures::fx::FxHashMap;
use rustc_data_structures::graph::{Graph, NodeIndex};

use super::hash::*;

mod compress;

/// A data-structure that makes it easy to enumerate the hashable
/// predecessors of any given dep-node.
pub struct Predecessors<'query> {
    // A reduced version of the input graph that contains fewer nodes.
    // This is intended to keep all of the base inputs (i.e., HIR
    // nodes) and all of the "work-products" we may care about
    // later. Other nodes may be retained if it keeps the overall size
    // of the graph down.
    pub reduced_graph: Graph<&'query DepNode, ()>,

    // These are output nodes that have no incoming edges. We have to
    // track these specially because, when we load the data back up
    // again, we want to make sure and recreate these nodes (we want
    // to recreate the nodes where all incoming edges are clean; but
    // since we ordinarily just serialize edges, we wind up just
    // forgetting that bootstrap outputs even exist in that case.)
    pub bootstrap_outputs: Vec<&'query DepNode>,

    // For the inputs (hir/foreign-metadata), we include hashes.
    pub hashes: FxHashMap<&'query DepNode, Fingerprint>,
}

impl<'q> Predecessors<'q> {
    pub fn new(query: &'q DepGraphQuery, hcx: &mut HashContext) -> Self {
        let tcx = hcx.tcx;

        // Find the set of "start nodes". These are nodes that we will
        // possibly query later.
        let is_output = |node: &DepNode| -> bool {
            match node.kind {
                DepKind::WorkProduct => true,
                DepKind::MetaData => {
                    // We do *not* create dep-nodes for the current crate's
                    // metadata anymore, just for metadata that we import/read
                    // from other crates.
                    debug_assert!(!node.extract_def_id(tcx).unwrap().is_local());
                    false
                }
                // if -Z query-dep-graph is passed, save more extended data
                // to enable better unit testing
                DepKind::TypeckTables => tcx.sess.opts.debugging_opts.query_dep_graph,

                _ => false,
            }
        };

        // Reduce the graph to the most important nodes.
        let compress::Reduction { graph, input_nodes } =
            compress::reduce_graph(&query.graph,
                                   |n| HashContext::is_hashable(tcx, n),
                                   |n| is_output(n));

        let mut hashes = FxHashMap();
        for input_index in input_nodes {
            let input = *graph.node_data(input_index);
            debug!("computing hash for input node `{:?}`", input);
            hashes.entry(input)
                  .or_insert_with(|| hcx.hash(input).unwrap());
        }

        if tcx.sess.opts.debugging_opts.query_dep_graph {
            // Not all inputs might have been reachable from an output node,
            // but we still want their hash for our unit tests.
            let hir_nodes = query.graph.all_nodes().iter().filter_map(|node| {
                match node.data.kind {
                    DepKind::Hir => Some(&node.data),
                    _ => None,
                }
            });

            for node in hir_nodes {
                hashes.entry(node)
                      .or_insert_with(|| hcx.hash(node).unwrap());
            }
        }

        let bootstrap_outputs: Vec<&'q DepNode> =
            (0 .. graph.len_nodes())
            .map(NodeIndex)
            .filter(|&n| graph.incoming_edges(n).next().is_none())
            .map(|n| *graph.node_data(n))
            .filter(|n| is_output(n))
            .collect();

        Predecessors {
            reduced_graph: graph,
            bootstrap_outputs,
            hashes,
        }
    }
}
