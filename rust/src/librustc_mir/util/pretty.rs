// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use rustc::hir;
use rustc::hir::def_id::{DefId, LOCAL_CRATE};
use rustc::mir::*;
use rustc::ty::TyCtxt;
use rustc::ty::item_path;
use rustc_data_structures::fx::FxHashMap;
use rustc_data_structures::indexed_vec::{Idx};
use std::fmt::Display;
use std::fs;
use std::io::{self, Write};
use std::path::{PathBuf, Path};
use super::graphviz::write_mir_fn_graphviz;
use transform::MirSource;

const INDENT: &'static str = "    ";
/// Alignment for lining up comments following MIR statements
const ALIGN: usize = 40;

/// An indication of where we are in the control flow graph. Used for printing
/// extra information in `dump_mir`
pub enum PassWhere {
    /// We have not started dumping the control flow graph, but we are about to.
    BeforeCFG,

    /// We just finished dumping the control flow graph. This is right before EOF
    AfterCFG,

    /// We are about to start dumping the given basic block.
    BeforeBlock(BasicBlock),

    /// We are just about to dumpt the given statement or terminator.
    InCFG(Location),
}

/// If the session is properly configured, dumps a human-readable
/// representation of the mir into:
///
/// ```text
/// rustc.node<node_id>.<pass_num>.<pass_name>.<disambiguator>
/// ```
///
/// Output from this function is controlled by passing `-Z dump-mir=<filter>`,
/// where `<filter>` takes the following forms:
///
/// - `all` -- dump MIR for all fns, all passes, all everything
/// - `substring1&substring2,...` -- `&`-separated list of substrings
///   that can appear in the pass-name or the `item_path_str` for the given
///   node-id. If any one of the substrings match, the data is dumped out.
pub fn dump_mir<'a, 'gcx, 'tcx, F>(tcx: TyCtxt<'a, 'gcx, 'tcx>,
                                   pass_num: Option<&Display>,
                                   pass_name: &str,
                                   disambiguator: &Display,
                                   source: MirSource,
                                   mir: &Mir<'tcx>,
                                   extra_data: F)
where
    F: FnMut(PassWhere, &mut Write) -> io::Result<()>
{
    if !dump_enabled(tcx, pass_name, source) {
        return;
    }

    let node_path = item_path::with_forced_impl_filename_line(|| { // see notes on #41697 below
        tcx.item_path_str(source.def_id)
    });
    dump_matched_mir_node(tcx, pass_num, pass_name, &node_path,
                          disambiguator, source, mir, extra_data);
}

pub fn dump_enabled<'a, 'gcx, 'tcx>(tcx: TyCtxt<'a, 'gcx, 'tcx>,
                                    pass_name: &str,
                                    source: MirSource)
                                    -> bool {
    let filters = match tcx.sess.opts.debugging_opts.dump_mir {
        None => return false,
        Some(ref filters) => filters,
    };
    let node_path = item_path::with_forced_impl_filename_line(|| { // see notes on #41697 below
        tcx.item_path_str(source.def_id)
    });
    filters.split("&")
           .any(|filter| {
               filter == "all" ||
                   pass_name.contains(filter) ||
                   node_path.contains(filter)
           })
}

// #41697 -- we use `with_forced_impl_filename_line()` because
// `item_path_str()` would otherwise trigger `type_of`, and this can
// run while we are already attempting to evaluate `type_of`.

fn dump_matched_mir_node<'a, 'gcx, 'tcx, F>(tcx: TyCtxt<'a, 'gcx, 'tcx>,
                                            pass_num: Option<&Display>,
                                            pass_name: &str,
                                            node_path: &str,
                                            disambiguator: &Display,
                                            source: MirSource,
                                            mir: &Mir<'tcx>,
                                            mut extra_data: F)
where
    F: FnMut(PassWhere, &mut Write) -> io::Result<()>
{
    let promotion_id = match source.promoted {
        Some(id) => format!("-{:?}", id),
        None => String::new()
    };

    let pass_num = if tcx.sess.opts.debugging_opts.dump_mir_exclude_pass_number {
        format!("")
    } else {
        match pass_num {
            None => format!(".-------"),
            Some(pass_num) => format!(".{}", pass_num),
        }
    };

    let mut file_path = PathBuf::new();
    if let Some(ref file_dir) = tcx.sess.opts.debugging_opts.dump_mir_dir {
        let p = Path::new(file_dir);
        file_path.push(p);
    };

    let _ = fs::create_dir_all(&file_path);
    let item_name = tcx.hir.def_path(source.def_id).to_filename_friendly_no_crate();
    let file_name = format!("rustc.{}{}{}.{}.{}.mir",
                            item_name, promotion_id, pass_num, pass_name, disambiguator);
    file_path.push(&file_name);
    let _ = fs::File::create(&file_path).and_then(|mut file| {
        writeln!(file, "// MIR for `{}`", node_path)?;
        writeln!(file, "// source = {:?}", source)?;
        writeln!(file, "// pass_name = {}", pass_name)?;
        writeln!(file, "// disambiguator = {}", disambiguator)?;
        if let Some(ref layout) = mir.generator_layout {
            writeln!(file, "// generator_layout = {:?}", layout)?;
        }
        writeln!(file, "")?;
        extra_data(PassWhere::BeforeCFG, &mut file)?;
        write_mir_fn(tcx, source, mir, &mut extra_data, &mut file)?;
        extra_data(PassWhere::AfterCFG, &mut file)?;
        Ok(())
    });

    if tcx.sess.opts.debugging_opts.dump_mir_graphviz {
        file_path.set_extension("dot");
        let _ = fs::File::create(&file_path).and_then(|mut file| {
            write_mir_fn_graphviz(tcx, source.def_id, mir, &mut file)?;
            Ok(())
        });
    }
}

/// Write out a human-readable textual representation for the given MIR.
pub fn write_mir_pretty<'a, 'gcx, 'tcx>(tcx: TyCtxt<'a, 'gcx, 'tcx>,
                                        single: Option<DefId>,
                                        w: &mut Write)
                                        -> io::Result<()>
{
    writeln!(w, "// WARNING: This output format is intended for human consumers only")?;
    writeln!(w, "// and is subject to change without notice. Knock yourself out.")?;

    let mut first = true;
    for def_id in dump_mir_def_ids(tcx, single) {
        let mir = &tcx.optimized_mir(def_id);

        if first {
            first = false;
        } else {
            // Put empty lines between all items
            writeln!(w, "")?;
        }

        write_mir_fn(tcx, MirSource::item(def_id), mir, &mut |_, _| Ok(()), w)?;

        for (i, mir) in mir.promoted.iter_enumerated() {
            writeln!(w, "")?;
            let src = MirSource {
                def_id,
                promoted: Some(i)
            };
            write_mir_fn(tcx, src, mir, &mut |_, _| Ok(()), w)?;
        }
    }
    Ok(())
}

pub fn write_mir_fn<'a, 'gcx, 'tcx, F>(tcx: TyCtxt<'a, 'gcx, 'tcx>,
                                       src: MirSource,
                                       mir: &Mir<'tcx>,
                                       extra_data: &mut F,
                                       w: &mut Write)
                                       -> io::Result<()>
where
    F: FnMut(PassWhere, &mut Write) -> io::Result<()>
{
    write_mir_intro(tcx, src, mir, w)?;
    for block in mir.basic_blocks().indices() {
        extra_data(PassWhere::BeforeBlock(block), w)?;
        write_basic_block(tcx, block, mir, extra_data, w)?;
        if block.index() + 1 != mir.basic_blocks().len() {
            writeln!(w, "")?;
        }
    }

    writeln!(w, "}}")?;
    Ok(())
}

/// Write out a human-readable textual representation for the given basic block.
pub fn write_basic_block<F>(tcx: TyCtxt,
                            block: BasicBlock,
                            mir: &Mir,
                            extra_data: &mut F,
                            w: &mut Write)
                            -> io::Result<()>
where
    F: FnMut(PassWhere, &mut Write) -> io::Result<()>
{
    let data = &mir[block];

    // Basic block label at the top.
    let cleanup_text = if data.is_cleanup { " // cleanup" } else { "" };
    let lbl = format!("{}{:?}: {{", INDENT, block);
    writeln!(w, "{0:1$}{2}", lbl, ALIGN, cleanup_text)?;

    // List of statements in the middle.
    let mut current_location = Location { block: block, statement_index: 0 };
    for statement in &data.statements {
        extra_data(PassWhere::InCFG(current_location), w)?;
        let indented_mir = format!("{0}{0}{1:?};", INDENT, statement);
        writeln!(w, "{0:1$} // {2}",
                 indented_mir,
                 ALIGN,
                 comment(tcx, statement.source_info))?;

        current_location.statement_index += 1;
    }

    // Terminator at the bottom.
    extra_data(PassWhere::InCFG(current_location), w)?;
    let indented_terminator = format!("{0}{0}{1:?};", INDENT, data.terminator().kind);
    writeln!(w, "{0:1$} // {2}",
             indented_terminator,
             ALIGN,
             comment(tcx, data.terminator().source_info))?;

    writeln!(w, "{}}}", INDENT)
}

fn comment(tcx: TyCtxt, SourceInfo { span, scope }: SourceInfo) -> String {
    format!("scope {} at {}", scope.index(), tcx.sess.codemap().span_to_string(span))
}

/// Prints user-defined variables in a scope tree.
///
/// Returns the total number of variables printed.
fn write_scope_tree(tcx: TyCtxt,
                    mir: &Mir,
                    scope_tree: &FxHashMap<VisibilityScope, Vec<VisibilityScope>>,
                    w: &mut Write,
                    parent: VisibilityScope,
                    depth: usize)
                    -> io::Result<()> {
    let indent = depth * INDENT.len();

    let children = match scope_tree.get(&parent) {
        Some(childs) => childs,
        None => return Ok(()),
    };

    for &child in children {
        let data = &mir.visibility_scopes[child];
        assert_eq!(data.parent_scope, Some(parent));
        writeln!(w, "{0:1$}scope {2} {{", "", indent, child.index())?;

        // User variable types (including the user's name in a comment).
        for local in mir.vars_iter() {
            let var = &mir.local_decls[local];
            let (name, source_info) = if var.source_info.scope == child {
                (var.name.unwrap(), var.source_info)
            } else {
                // Not a variable or not declared in this scope.
                continue;
            };

            let mut_str = if var.mutability == Mutability::Mut {
                "mut "
            } else {
                ""
            };

            let indent = indent + INDENT.len();
            let indented_var = format!("{0:1$}let {2}{3:?}: {4};",
                                       INDENT,
                                       indent,
                                       mut_str,
                                       local,
                                       var.ty);
            writeln!(w, "{0:1$} // \"{2}\" in {3}",
                     indented_var,
                     ALIGN,
                     name,
                     comment(tcx, source_info))?;
        }

        write_scope_tree(tcx, mir, scope_tree, w, child, depth + 1)?;

        writeln!(w, "{0:1$}}}", "", depth * INDENT.len())?;
    }

    Ok(())
}

/// Write out a human-readable textual representation of the MIR's `fn` type and the types of its
/// local variables (both user-defined bindings and compiler temporaries).
pub fn write_mir_intro<'a, 'gcx, 'tcx>(tcx: TyCtxt<'a, 'gcx, 'tcx>,
                                       src: MirSource,
                                       mir: &Mir,
                                       w: &mut Write)
                                       -> io::Result<()> {
    write_mir_sig(tcx, src, mir, w)?;
    writeln!(w, " {{")?;

    // construct a scope tree and write it out
    let mut scope_tree: FxHashMap<VisibilityScope, Vec<VisibilityScope>> = FxHashMap();
    for (index, scope_data) in mir.visibility_scopes.iter().enumerate() {
        if let Some(parent) = scope_data.parent_scope {
            scope_tree.entry(parent)
                      .or_insert(vec![])
                      .push(VisibilityScope::new(index));
        } else {
            // Only the argument scope has no parent, because it's the root.
            assert_eq!(index, ARGUMENT_VISIBILITY_SCOPE.index());
        }
    }

    // Print return place
    let indented_retptr = format!("{}let mut {:?}: {};",
                                  INDENT,
                                  RETURN_PLACE,
                                  mir.local_decls[RETURN_PLACE].ty);
    writeln!(w, "{0:1$} // return place",
             indented_retptr,
             ALIGN)?;

    write_scope_tree(tcx, mir, &scope_tree, w, ARGUMENT_VISIBILITY_SCOPE, 1)?;

    write_temp_decls(mir, w)?;

    // Add an empty line before the first block is printed.
    writeln!(w, "")?;

    Ok(())
}

fn write_mir_sig(tcx: TyCtxt, src: MirSource, mir: &Mir, w: &mut Write)
                 -> io::Result<()>
{
    let id = tcx.hir.as_local_node_id(src.def_id).unwrap();
    let body_owner_kind = tcx.hir.body_owner_kind(id);
    match (body_owner_kind, src.promoted) {
        (_, Some(i)) => write!(w, "{:?} in", i)?,
        (hir::BodyOwnerKind::Fn, _) => write!(w, "fn")?,
        (hir::BodyOwnerKind::Const, _) => write!(w, "const")?,
        (hir::BodyOwnerKind::Static(hir::MutImmutable), _) => write!(w, "static")?,
        (hir::BodyOwnerKind::Static(hir::MutMutable), _) => write!(w, "static mut")?,
    }

    item_path::with_forced_impl_filename_line(|| { // see notes on #41697 elsewhere
        write!(w, " {}", tcx.item_path_str(src.def_id))
    })?;

    match (body_owner_kind, src.promoted) {
        (hir::BodyOwnerKind::Fn, None) => {
            write!(w, "(")?;

            // fn argument types.
            for (i, arg) in mir.args_iter().enumerate() {
                if i != 0 {
                    write!(w, ", ")?;
                }
                write!(w, "{:?}: {}", Place::Local(arg), mir.local_decls[arg].ty)?;
            }

            write!(w, ") -> {}", mir.return_ty())
        }
        (hir::BodyOwnerKind::Const, _) |
        (hir::BodyOwnerKind::Static(_), _) |
        (_, Some(_)) => {
            assert_eq!(mir.arg_count, 0);
            write!(w, ": {} =", mir.return_ty())
        }
    }
}

fn write_temp_decls(mir: &Mir, w: &mut Write) -> io::Result<()> {
    // Compiler-introduced temporary types.
    for temp in mir.temps_iter() {
        writeln!(w, "{}let mut {:?}: {};", INDENT, temp, mir.local_decls[temp].ty)?;
    }

    Ok(())
}

pub fn dump_mir_def_ids(tcx: TyCtxt, single: Option<DefId>) -> Vec<DefId> {
    if let Some(i) = single {
        vec![i]
    } else {
        tcx.mir_keys(LOCAL_CRATE).iter().cloned().collect()
    }
}
