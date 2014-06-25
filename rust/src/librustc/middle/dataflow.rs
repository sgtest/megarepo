// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


/*!
 * A module for propagating forward dataflow information. The analysis
 * assumes that the items to be propagated can be represented as bits
 * and thus uses bitvectors. Your job is simply to specify the so-called
 * GEN and KILL bits for each expression.
 */


use middle::cfg;
use middle::cfg::CFGIndex;
use middle::ty;
use std::io;
use std::uint;
use syntax::ast;
use syntax::ast_util::IdRange;
use syntax::visit;
use syntax::print::{pp, pprust};
use util::nodemap::NodeMap;

#[deriving(Clone)]
pub struct DataFlowContext<'a, O> {
    tcx: &'a ty::ctxt,

    /// a name for the analysis using this dataflow instance
    analysis_name: &'static str,

    /// the data flow operator
    oper: O,

    /// number of bits to propagate per id
    bits_per_id: uint,

    /// number of words we will use to store bits_per_id.
    /// equal to bits_per_id/uint::BITS rounded up.
    words_per_id: uint,

    // mapping from cfg node index to bitset index.
    index_to_bitset: Vec<Option<uint>>,

    // mapping from node to cfg node index
    // FIXME (#6298): Shouldn't this go with CFG?
    nodeid_to_index: NodeMap<CFGIndex>,

    // Bit sets per cfg node.  The following three fields (`gens`, `kills`,
    // and `on_entry`) all have the same structure. For each id in
    // `id_range`, there is a range of words equal to `words_per_id`.
    // So, to access the bits for any given id, you take a slice of
    // the full vector (see the method `compute_id_range()`).

    /// bits generated as we exit the cfg node. Updated by `add_gen()`.
    gens: Vec<uint>,

    /// bits killed as we exit the cfg node. Updated by `add_kill()`.
    kills: Vec<uint>,

    /// bits that are valid on entry to the cfg node. Updated by
    /// `propagate()`.
    on_entry: Vec<uint>,
}

pub trait BitwiseOperator {
    /// Joins two predecessor bits together, typically either `|` or `&`
    fn join(&self, succ: uint, pred: uint) -> uint;
}

/// Parameterization for the precise form of data flow that is used.
pub trait DataFlowOperator : BitwiseOperator {
    /// Specifies the initial value for each bit in the `on_entry` set
    fn initial_value(&self) -> bool;
}

struct PropagationContext<'a, 'b, O> {
    dfcx: &'a mut DataFlowContext<'b, O>,
    changed: bool
}

fn to_cfgidx_or_die(id: ast::NodeId, index: &NodeMap<CFGIndex>) -> CFGIndex {
    let opt_cfgindex = index.find(&id).map(|&i|i);
    opt_cfgindex.unwrap_or_else(|| {
        fail!("nodeid_to_index does not have entry for NodeId {}", id);
    })
}

impl<'a, O:DataFlowOperator> DataFlowContext<'a, O> {
    fn has_bitset(&self, n: ast::NodeId) -> bool {
        assert!(n != ast::DUMMY_NODE_ID);
        match self.nodeid_to_index.find(&n) {
            None => false,
            Some(&cfgidx) => {
                let node_id = cfgidx.node_id();
                node_id < self.index_to_bitset.len() &&
                    self.index_to_bitset.get(node_id).is_some()
            }
        }
    }
    fn get_bitset_index(&self, cfgidx: CFGIndex) -> uint {
        let node_id = cfgidx.node_id();
        self.index_to_bitset.get(node_id).unwrap()
    }
    fn get_or_create_bitset_index(&mut self, cfgidx: CFGIndex) -> uint {
        assert!(self.words_per_id > 0);
        let len = self.gens.len() / self.words_per_id;
        let expanded;
        let n;
        if self.index_to_bitset.len() <= cfgidx.node_id() {
            self.index_to_bitset.grow_set(cfgidx.node_id(), &None, Some(len));
            expanded = true;
            n = len;
        } else {
            let entry = self.index_to_bitset.get_mut(cfgidx.node_id());
            match *entry {
                None => {
                    *entry = Some(len);
                    expanded = true;
                    n = len;
                }
                Some(bitidx) => {
                    expanded = false;
                    n = bitidx;
                }
            }
        }
        if expanded {
            let entry = if self.oper.initial_value() { uint::MAX } else {0};
            for _ in range(0, self.words_per_id) {
                self.gens.push(0);
                self.kills.push(0);
                self.on_entry.push(entry);
            }
        }

        let start = n * self.words_per_id;
        let end = start + self.words_per_id;
        let len = self.gens.len();
        assert!(start < len);
        assert!(end <= len);
        n
    }
}

impl<'a, O:DataFlowOperator> pprust::PpAnn for DataFlowContext<'a, O> {
    fn pre(&self,
           ps: &mut pprust::State,
           node: pprust::AnnNode) -> io::IoResult<()> {
        let id = match node {
            pprust::NodeExpr(expr) => expr.id,
            pprust::NodeBlock(blk) => blk.id,
            pprust::NodeItem(_) => 0,
            pprust::NodePat(pat) => pat.id
        };

        if self.has_bitset(id) {
            let cfgidx = to_cfgidx_or_die(id, &self.nodeid_to_index);
            let (start, end) = self.compute_id_range_frozen(cfgidx);
            let on_entry = self.on_entry.slice(start, end);
            let entry_str = bits_to_str(on_entry);

            let gens = self.gens.slice(start, end);
            let gens_str = if gens.iter().any(|&u| u != 0) {
                format!(" gen: {}", bits_to_str(gens))
            } else {
                "".to_string()
            };

            let kills = self.kills.slice(start, end);
            let kills_str = if kills.iter().any(|&u| u != 0) {
                format!(" kill: {}", bits_to_str(kills))
            } else {
                "".to_string()
            };

            try!(ps.synth_comment(format!("id {}: {}{}{}", id, entry_str,
                                          gens_str, kills_str)));
            try!(pp::space(&mut ps.s));
        }
        Ok(())
    }
}

fn build_nodeid_to_index(decl: Option<&ast::FnDecl>,
                         cfg: &cfg::CFG) -> NodeMap<CFGIndex> {
    let mut index = NodeMap::new();

    // FIXME (#6298): Would it be better to fold formals from decl
    // into cfg itself?  i.e. introduce a fn-based flow-graph in
    // addition to the current block-based flow-graph, rather than
    // have to put traversals like this here?
    match decl {
        None => {}
        Some(decl) => add_entries_from_fn_decl(&mut index, decl, cfg.entry)
    }

    cfg.graph.each_node(|node_idx, node| {
        if node.data.id != ast::DUMMY_NODE_ID {
            index.insert(node.data.id, node_idx);
        }
        true
    });

    return index;

    fn add_entries_from_fn_decl(index: &mut NodeMap<CFGIndex>,
                                decl: &ast::FnDecl,
                                entry: CFGIndex) {
        //! add mappings from the ast nodes for the formal bindings to
        //! the entry-node in the graph.
        struct Formals<'a> {
            entry: CFGIndex,
            index: &'a mut NodeMap<CFGIndex>,
        }
        let mut formals = Formals { entry: entry, index: index };
        visit::walk_fn_decl(&mut formals, decl, ());
        impl<'a> visit::Visitor<()> for Formals<'a> {
            fn visit_pat(&mut self, p: &ast::Pat, e: ()) {
                self.index.insert(p.id, self.entry);
                visit::walk_pat(self, p, e)
            }
        }
    }
}

impl<'a, O:DataFlowOperator> DataFlowContext<'a, O> {
    pub fn new(tcx: &'a ty::ctxt,
               analysis_name: &'static str,
               decl: Option<&ast::FnDecl>,
               cfg: &cfg::CFG,
               oper: O,
               id_range: IdRange,
               bits_per_id: uint) -> DataFlowContext<'a, O> {
        let words_per_id = (bits_per_id + uint::BITS - 1) / uint::BITS;

        debug!("DataFlowContext::new(analysis_name: {:s}, id_range={:?}, \
                                     bits_per_id={:?}, words_per_id={:?})",
               analysis_name, id_range, bits_per_id, words_per_id);

        let gens = Vec::new();
        let kills = Vec::new();
        let on_entry = Vec::new();

        let nodeid_to_index = build_nodeid_to_index(decl, cfg);

        DataFlowContext {
            tcx: tcx,
            analysis_name: analysis_name,
            words_per_id: words_per_id,
            index_to_bitset: Vec::new(),
            nodeid_to_index: nodeid_to_index,
            bits_per_id: bits_per_id,
            oper: oper,
            gens: gens,
            kills: kills,
            on_entry: on_entry
        }
    }

    pub fn add_gen(&mut self, id: ast::NodeId, bit: uint) {
        //! Indicates that `id` generates `bit`
        debug!("{:s} add_gen(id={:?}, bit={:?})",
               self.analysis_name, id, bit);
        assert!(self.nodeid_to_index.contains_key(&id));
        let cfgidx = to_cfgidx_or_die(id, &self.nodeid_to_index);
        let (start, end) = self.compute_id_range(cfgidx);
        let gens = self.gens.mut_slice(start, end);
        set_bit(gens, bit);
    }

    pub fn add_kill(&mut self, id: ast::NodeId, bit: uint) {
        //! Indicates that `id` kills `bit`
        debug!("{:s} add_kill(id={:?}, bit={:?})",
               self.analysis_name, id, bit);
        assert!(self.nodeid_to_index.contains_key(&id));
        let cfgidx = to_cfgidx_or_die(id, &self.nodeid_to_index);
        let (start, end) = self.compute_id_range(cfgidx);
        let kills = self.kills.mut_slice(start, end);
        set_bit(kills, bit);
    }

    fn apply_gen_kill(&mut self, cfgidx: CFGIndex, bits: &mut [uint]) {
        //! Applies the gen and kill sets for `id` to `bits`
        debug!("{:s} apply_gen_kill(cfgidx={}, bits={}) [before]",
               self.analysis_name, cfgidx, mut_bits_to_str(bits));
        let (start, end) = self.compute_id_range(cfgidx);
        let gens = self.gens.slice(start, end);
        bitwise(bits, gens, &Union);
        let kills = self.kills.slice(start, end);
        bitwise(bits, kills, &Subtract);

        debug!("{:s} apply_gen_kill(cfgidx={}, bits={}) [after]",
               self.analysis_name, cfgidx, mut_bits_to_str(bits));
    }

    fn compute_id_range_frozen(&self, cfgidx: CFGIndex) -> (uint, uint) {
        let n = self.get_bitset_index(cfgidx);
        let start = n * self.words_per_id;
        let end = start + self.words_per_id;
        (start, end)
    }

    fn compute_id_range(&mut self, cfgidx: CFGIndex) -> (uint, uint) {
        let n = self.get_or_create_bitset_index(cfgidx);
        let start = n * self.words_per_id;
        let end = start + self.words_per_id;

        assert!(start < self.gens.len());
        assert!(end <= self.gens.len());
        assert!(self.gens.len() == self.kills.len());
        assert!(self.gens.len() == self.on_entry.len());

        (start, end)
    }


    pub fn each_bit_on_entry_frozen(&self,
                                    id: ast::NodeId,
                                    f: |uint| -> bool)
                                    -> bool {
        //! Iterates through each bit that is set on entry to `id`.
        //! Only useful after `propagate()` has been called.
        if !self.has_bitset(id) {
            return true;
        }
        let cfgidx = to_cfgidx_or_die(id, &self.nodeid_to_index);
        let (start, end) = self.compute_id_range_frozen(cfgidx);
        let on_entry = self.on_entry.slice(start, end);
        debug!("{:s} each_bit_on_entry_frozen(id={:?}, on_entry={})",
               self.analysis_name, id, bits_to_str(on_entry));
        self.each_bit(on_entry, f)
    }

    pub fn each_gen_bit_frozen(&self, id: ast::NodeId, f: |uint| -> bool)
                               -> bool {
        //! Iterates through each bit in the gen set for `id`.
        if !self.has_bitset(id) {
            return true;
        }
        let cfgidx = to_cfgidx_or_die(id, &self.nodeid_to_index);
        let (start, end) = self.compute_id_range_frozen(cfgidx);
        let gens = self.gens.slice(start, end);
        debug!("{:s} each_gen_bit(id={:?}, gens={})",
               self.analysis_name, id, bits_to_str(gens));
        self.each_bit(gens, f)
    }

    fn each_bit(&self, words: &[uint], f: |uint| -> bool) -> bool {
        //! Helper for iterating over the bits in a bit set.

        for (word_index, &word) in words.iter().enumerate() {
            if word != 0 {
                let base_index = word_index * uint::BITS;
                for offset in range(0u, uint::BITS) {
                    let bit = 1 << offset;
                    if (word & bit) != 0 {
                        // NB: we round up the total number of bits
                        // that we store in any given bit set so that
                        // it is an even multiple of uint::BITS.  This
                        // means that there may be some stray bits at
                        // the end that do not correspond to any
                        // actual value.  So before we callback, check
                        // whether the bit_index is greater than the
                        // actual value the user specified and stop
                        // iterating if so.
                        let bit_index = base_index + offset;
                        if bit_index >= self.bits_per_id {
                            return true;
                        } else if !f(bit_index) {
                            return false;
                        }
                    }
                }
            }
        }
        return true;
    }

    pub fn add_kills_from_flow_exits(&mut self, cfg: &cfg::CFG) {
        //! Whenever you have a `break` or `continue` statement, flow
        //! exits through any number of enclosing scopes on its way to
        //! the new destination. This function infers the kill bits of
        //! those control operators based on the kill bits associated
        //! with those scopes.
        //!
        //! This is usually called (if it is called at all), after
        //! all add_gen and add_kill calls, but before propagate.

        debug!("{:s} add_kills_from_flow_exits", self.analysis_name);
        if self.bits_per_id == 0 {
            // Skip the surprisingly common degenerate case.  (Note
            // compute_id_range requires self.words_per_id > 0.)
            return;
        }
        cfg.graph.each_edge(|_edge_index, edge| {
            let flow_exit = edge.source();
            let (start, end) = self.compute_id_range(flow_exit);
            let mut orig_kills = self.kills.slice(start, end).to_owned();

            let mut changed = false;
            for &node_id in edge.data.exiting_scopes.iter() {
                let opt_cfg_idx = self.nodeid_to_index.find(&node_id).map(|&i|i);
                match opt_cfg_idx {
                    Some(cfg_idx) => {
                        let (start, end) = self.compute_id_range(cfg_idx);
                        let kills = self.kills.slice(start, end);
                        if bitwise(orig_kills.as_mut_slice(), kills, &Union) {
                            changed = true;
                        }
                    }
                    None => {
                        debug!("{:s} add_kills_from_flow_exits flow_exit={} \
                                no cfg_idx for exiting_scope={:?}",
                               self.analysis_name, flow_exit, node_id);
                    }
                }
            }

            if changed {
                let bits = self.kills.mut_slice(start, end);
                debug!("{:s} add_kills_from_flow_exits flow_exit={} bits={} [before]",
                       self.analysis_name, flow_exit, mut_bits_to_str(bits));
                bits.copy_from(orig_kills.as_slice());
                debug!("{:s} add_kills_from_flow_exits flow_exit={} bits={} [after]",
                       self.analysis_name, flow_exit, mut_bits_to_str(bits));
            }
            true
        });
    }
}

impl<'a, O:DataFlowOperator+Clone+'static> DataFlowContext<'a, O> {
//                          ^^^^^^^^^^^^^ only needed for pretty printing
    pub fn propagate(&mut self, cfg: &cfg::CFG, blk: &ast::Block) {
        //! Performs the data flow analysis.

        if self.bits_per_id == 0 {
            // Optimize the surprisingly common degenerate case.
            return;
        }

        {
            let words_per_id = self.words_per_id;
            let mut propcx = PropagationContext {
                dfcx: &mut *self,
                changed: true
            };

            let mut temp = Vec::from_elem(words_per_id, 0u);
            while propcx.changed {
                propcx.changed = false;
                propcx.reset(temp.as_mut_slice());
                propcx.walk_cfg(cfg, temp.as_mut_slice());
            }
        }

        debug!("Dataflow result for {:s}:", self.analysis_name);
        debug!("{}", {
            self.pretty_print_to(box io::stderr(), blk).unwrap();
            ""
        });
    }

    fn pretty_print_to(&self, wr: Box<io::Writer>,
                       blk: &ast::Block) -> io::IoResult<()> {
        let mut ps = pprust::rust_printer_annotated(wr, self);
        try!(ps.cbox(pprust::indent_unit));
        try!(ps.ibox(0u));
        try!(ps.print_block(blk));
        pp::eof(&mut ps.s)
    }
}

impl<'a, 'b, O:DataFlowOperator> PropagationContext<'a, 'b, O> {
    fn walk_cfg(&mut self,
                cfg: &cfg::CFG,
                in_out: &mut [uint]) {
        debug!("DataFlowContext::walk_cfg(in_out={}) {:s}",
               bits_to_str(in_out), self.dfcx.analysis_name);
        cfg.graph.each_node(|node_index, node| {
            debug!("DataFlowContext::walk_cfg idx={} id={} begin in_out={}",
                   node_index, node.data.id, bits_to_str(in_out));

            let (start, end) = self.dfcx.compute_id_range(node_index);

            // Initialize local bitvector with state on-entry.
            in_out.copy_from(self.dfcx.on_entry.slice(start, end));

            // Compute state on-exit by applying transfer function to
            // state on-entry.
            self.dfcx.apply_gen_kill(node_index, in_out);

            // Propagate state on-exit from node into its successors.
            self.propagate_bits_into_graph_successors_of(in_out, cfg, node_index);
            true // continue to next node
        });
    }

    fn reset(&mut self, bits: &mut [uint]) {
        let e = if self.dfcx.oper.initial_value() {uint::MAX} else {0};
        for b in bits.mut_iter() {
            *b = e;
        }
    }

    fn propagate_bits_into_graph_successors_of(&mut self,
                                               pred_bits: &[uint],
                                               cfg: &cfg::CFG,
                                               cfgidx: CFGIndex) {
        cfg.graph.each_outgoing_edge(cfgidx, |_e_idx, edge| {
            self.propagate_bits_into_entry_set_for(pred_bits, edge);
            true
        });
    }

    fn propagate_bits_into_entry_set_for(&mut self,
                                         pred_bits: &[uint],
                                         edge: &cfg::CFGEdge) {
        let source = edge.source();
        let cfgidx = edge.target();
        debug!("{:s} propagate_bits_into_entry_set_for(pred_bits={}, {} to {})",
               self.dfcx.analysis_name, bits_to_str(pred_bits), source, cfgidx);
        let (start, end) = self.dfcx.compute_id_range(cfgidx);
        let changed = {
            // (scoping mutable borrow of self.dfcx.on_entry)
            let on_entry = self.dfcx.on_entry.mut_slice(start, end);
            bitwise(on_entry, pred_bits, &self.dfcx.oper)
        };
        if changed {
            debug!("{:s} changed entry set for {:?} to {}",
                   self.dfcx.analysis_name, cfgidx,
                   bits_to_str(self.dfcx.on_entry.slice(start, end)));
            self.changed = true;
        }
    }
}

fn mut_bits_to_str(words: &mut [uint]) -> String {
    bits_to_str(words)
}

fn bits_to_str(words: &[uint]) -> String {
    let mut result = String::new();
    let mut sep = '[';

    // Note: this is a little endian printout of bytes.

    for &word in words.iter() {
        let mut v = word;
        for _ in range(0u, uint::BYTES) {
            result.push_char(sep);
            result.push_str(format!("{:02x}", v & 0xFF).as_slice());
            v >>= 8;
            sep = '-';
        }
    }
    result.push_char(']');
    return result
}

#[inline]
fn bitwise<Op:BitwiseOperator>(out_vec: &mut [uint],
                               in_vec: &[uint],
                               op: &Op) -> bool {
    assert_eq!(out_vec.len(), in_vec.len());
    let mut changed = false;
    for (out_elt, in_elt) in out_vec.mut_iter().zip(in_vec.iter()) {
        let old_val = *out_elt;
        let new_val = op.join(old_val, *in_elt);
        *out_elt = new_val;
        changed |= old_val != new_val;
    }
    changed
}

fn set_bit(words: &mut [uint], bit: uint) -> bool {
    debug!("set_bit: words={} bit={}",
           mut_bits_to_str(words), bit_str(bit));
    let word = bit / uint::BITS;
    let bit_in_word = bit % uint::BITS;
    let bit_mask = 1 << bit_in_word;
    debug!("word={} bit_in_word={} bit_mask={}", word, bit_in_word, word);
    let oldv = words[word];
    let newv = oldv | bit_mask;
    words[word] = newv;
    oldv != newv
}

fn bit_str(bit: uint) -> String {
    let byte = bit >> 8;
    let lobits = 1u << (bit & 0xFF);
    format!("[{}:{}-{:02x}]", bit, byte, lobits)
}

struct Union;
impl BitwiseOperator for Union {
    fn join(&self, a: uint, b: uint) -> uint { a | b }
}
struct Subtract;
impl BitwiseOperator for Subtract {
    fn join(&self, a: uint, b: uint) -> uint { a & !b }
}
