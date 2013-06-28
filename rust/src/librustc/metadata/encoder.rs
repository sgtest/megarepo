// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Metadata encoding

use core::prelude::*;

use metadata::common::*;
use metadata::cstore;
use metadata::decoder;
use metadata::tyencode;
use middle::ty::node_id_to_type;
use middle::ty;
use middle;
use util::ppaux::ty_to_str;

use core::hash::HashUtil;
use core::hashmap::{HashMap, HashSet};
use core::int;
use core::io;
use core::str;
use core::uint;
use core::vec;
use extra::flate;
use extra::serialize::Encodable;
use extra;
use syntax::abi::AbiSet;
use syntax::ast::*;
use syntax::ast;
use syntax::ast_map;
use syntax::ast_util::*;
use syntax::attr;
use syntax::diagnostic::span_handler;
use syntax::opt_vec::OptVec;
use syntax::opt_vec;
use syntax::parse::token::special_idents;
use syntax::{ast_util, visit};
use syntax::parse::token;
use syntax;
use writer = extra::ebml::writer;

use core::cast;

// used by astencode:
type abbrev_map = @mut HashMap<ty::t, tyencode::ty_abbrev>;

pub type encode_inlined_item<'self> = &'self fn(ecx: &EncodeContext,
                                   ebml_w: &mut writer::Encoder,
                                   path: &[ast_map::path_elt],
                                   ii: ast::inlined_item);

pub struct EncodeParams<'self> {
    diag: @span_handler,
    tcx: ty::ctxt,
    reexports2: middle::resolve::ExportMap2,
    item_symbols: &'self HashMap<ast::node_id, ~str>,
    discrim_symbols: &'self HashMap<ast::node_id, @str>,
    link_meta: &'self LinkMeta,
    cstore: @mut cstore::CStore,
    encode_inlined_item: encode_inlined_item<'self>,
    reachable: @mut HashSet<ast::node_id>,
}

struct Stats {
    inline_bytes: uint,
    attr_bytes: uint,
    dep_bytes: uint,
    lang_item_bytes: uint,
    link_args_bytes: uint,
    misc_bytes: uint,
    item_bytes: uint,
    index_bytes: uint,
    zero_bytes: uint,
    total_bytes: uint,

    n_inlines: uint
}

pub struct EncodeContext<'self> {
    diag: @span_handler,
    tcx: ty::ctxt,
    stats: @mut Stats,
    reexports2: middle::resolve::ExportMap2,
    item_symbols: &'self HashMap<ast::node_id, ~str>,
    discrim_symbols: &'self HashMap<ast::node_id, @str>,
    link_meta: &'self LinkMeta,
    cstore: &'self cstore::CStore,
    encode_inlined_item: encode_inlined_item<'self>,
    type_abbrevs: abbrev_map,
    reachable: @mut HashSet<ast::node_id>,
}

pub fn reachable(ecx: &EncodeContext, id: node_id) -> bool {
    ecx.reachable.contains(&id)
}

fn encode_name(ecx: &EncodeContext,
               ebml_w: &mut writer::Encoder,
               name: ident) {
    ebml_w.wr_tagged_str(tag_paths_data_name, ecx.tcx.sess.str_of(name));
}

fn encode_impl_type_basename(ecx: &EncodeContext,
                             ebml_w: &mut writer::Encoder,
                             name: ident) {
    ebml_w.wr_tagged_str(tag_item_impl_type_basename,
                         ecx.tcx.sess.str_of(name));
}

pub fn encode_def_id(ebml_w: &mut writer::Encoder, id: def_id) {
    ebml_w.wr_tagged_str(tag_def_id, def_to_str(id));
}

fn encode_region_param(ecx: &EncodeContext,
                       ebml_w: &mut writer::Encoder,
                       it: @ast::item) {
    let opt_rp = ecx.tcx.region_paramd_items.find(&it.id);
    for opt_rp.iter().advance |rp| {
        ebml_w.start_tag(tag_region_param);
        rp.encode(ebml_w);
        ebml_w.end_tag();
    }
}

struct entry<T> {
    val: T,
    pos: uint
}

fn add_to_index(ebml_w: &mut writer::Encoder,
                path: &[ident],
                index: &mut ~[entry<~str>],
                name: ident) {
    let mut full_path = ~[];
    full_path.push_all(path);
    full_path.push(name);
    index.push(
        entry {
            val: ast_util::path_name_i(full_path),
            pos: ebml_w.writer.tell()
        });
}

fn encode_trait_ref(ebml_w: &mut writer::Encoder,
                    ecx: &EncodeContext,
                    trait_ref: &ty::TraitRef,
                    tag: uint) {
    let r = ecx.reachable;
    let ty_str_ctxt = @tyencode::ctxt {
        diag: ecx.diag,
        ds: def_to_str,
        tcx: ecx.tcx,
        abbrevs: tyencode::ac_use_abbrevs(ecx.type_abbrevs)
    };

    ebml_w.start_tag(tag);
    tyencode::enc_trait_ref(ebml_w.writer, ty_str_ctxt, trait_ref);
    ebml_w.end_tag();
}

// Item info table encoding
fn encode_family(ebml_w: &mut writer::Encoder, c: char) {
    ebml_w.start_tag(tag_items_data_item_family);
    ebml_w.writer.write(&[c as u8]);
    ebml_w.end_tag();
}

pub fn def_to_str(did: def_id) -> ~str {
    fmt!("%d:%d", did.crate, did.node)
}

fn encode_ty_type_param_defs(ebml_w: &mut writer::Encoder,
                             ecx: &EncodeContext,
                             params: @~[ty::TypeParameterDef],
                             tag: uint) {
    let r = ecx.reachable;
    let ty_str_ctxt = @tyencode::ctxt {
        diag: ecx.diag,
        ds: def_to_str,
        tcx: ecx.tcx,
        abbrevs: tyencode::ac_use_abbrevs(ecx.type_abbrevs)
    };
    for params.iter().advance |param| {
        ebml_w.start_tag(tag);
        tyencode::enc_type_param_def(ebml_w.writer, ty_str_ctxt, param);
        ebml_w.end_tag();
    }
}

fn encode_type_param_bounds(ebml_w: &mut writer::Encoder,
                            ecx: &EncodeContext,
                            params: &OptVec<TyParam>) {
    let ty_param_defs =
        @params.map_to_vec(|param| ecx.tcx.ty_param_defs.get_copy(&param.id));
    encode_ty_type_param_defs(ebml_w, ecx, ty_param_defs,
                              tag_items_data_item_ty_param_bounds);
}

fn encode_variant_id(ebml_w: &mut writer::Encoder, vid: def_id) {
    ebml_w.start_tag(tag_items_data_item_variant);
    let s = def_to_str(vid);
    ebml_w.writer.write(s.as_bytes());
    ebml_w.end_tag();
}

pub fn write_type(ecx: &EncodeContext,
                  ebml_w: &mut writer::Encoder,
                  typ: ty::t) {
    let r = ecx.reachable;
    let ty_str_ctxt = @tyencode::ctxt {
        diag: ecx.diag,
        ds: def_to_str,
        tcx: ecx.tcx,
        abbrevs: tyencode::ac_use_abbrevs(ecx.type_abbrevs)
    };
    tyencode::enc_ty(ebml_w.writer, ty_str_ctxt, typ);
}

pub fn write_vstore(ecx: &EncodeContext,
                    ebml_w: &mut writer::Encoder,
                    vstore: ty::vstore) {
    let r = ecx.reachable;
    let ty_str_ctxt = @tyencode::ctxt {
        diag: ecx.diag,
        ds: def_to_str,
        tcx: ecx.tcx,
        abbrevs: tyencode::ac_use_abbrevs(ecx.type_abbrevs)
    };
    tyencode::enc_vstore(ebml_w.writer, ty_str_ctxt, vstore);
}

fn encode_type(ecx: &EncodeContext,
               ebml_w: &mut writer::Encoder,
               typ: ty::t) {
    ebml_w.start_tag(tag_items_data_item_type);
    write_type(ecx, ebml_w, typ);
    ebml_w.end_tag();
}

fn encode_transformed_self_ty(ecx: &EncodeContext,
                              ebml_w: &mut writer::Encoder,
                              opt_typ: Option<ty::t>) {
    for opt_typ.iter().advance |&typ| {
        ebml_w.start_tag(tag_item_method_transformed_self_ty);
        write_type(ecx, ebml_w, typ);
        ebml_w.end_tag();
    }
}

fn encode_method_fty(ecx: &EncodeContext,
                     ebml_w: &mut writer::Encoder,
                     typ: &ty::BareFnTy) {
    ebml_w.start_tag(tag_item_method_fty);

    let r = ecx.reachable;
    let ty_str_ctxt = @tyencode::ctxt {
        diag: ecx.diag,
        ds: def_to_str,
        tcx: ecx.tcx,
        abbrevs: tyencode::ac_use_abbrevs(ecx.type_abbrevs)
    };
    tyencode::enc_bare_fn_ty(ebml_w.writer, ty_str_ctxt, typ);

    ebml_w.end_tag();
}

fn encode_symbol(ecx: &EncodeContext,
                 ebml_w: &mut writer::Encoder,
                 id: node_id) {
    ebml_w.start_tag(tag_items_data_item_symbol);
    match ecx.item_symbols.find(&id) {
        Some(x) => {
            debug!("encode_symbol(id=%?, str=%s)", id, *x);
            ebml_w.writer.write(x.as_bytes());
        }
        None => {
            ecx.diag.handler().bug(
                fmt!("encode_symbol: id not found %d", id));
        }
    }
    ebml_w.end_tag();
}

fn encode_discriminant(ecx: &EncodeContext,
                       ebml_w: &mut writer::Encoder,
                       id: node_id) {
    ebml_w.start_tag(tag_items_data_item_symbol);
    ebml_w.writer.write(ecx.discrim_symbols.get_copy(&id).as_bytes());
    ebml_w.end_tag();
}

fn encode_disr_val(_: &EncodeContext,
                   ebml_w: &mut writer::Encoder,
                   disr_val: int) {
    ebml_w.start_tag(tag_disr_val);
    let s = int::to_str(disr_val);
    ebml_w.writer.write(s.as_bytes());
    ebml_w.end_tag();
}

fn encode_parent_item(ebml_w: &mut writer::Encoder, id: def_id) {
    ebml_w.start_tag(tag_items_data_parent_item);
    let s = def_to_str(id);
    ebml_w.writer.write(s.as_bytes());
    ebml_w.end_tag();
}

fn encode_enum_variant_info(ecx: &EncodeContext,
                            ebml_w: &mut writer::Encoder,
                            id: node_id,
                            variants: &[variant],
                            path: &[ast_map::path_elt],
                            index: @mut ~[entry<int>],
                            generics: &ast::Generics) {
    debug!("encode_enum_variant_info(id=%?)", id);

    let mut disr_val = 0;
    let mut i = 0;
    let vi = ty::enum_variants(ecx.tcx,
                               ast::def_id { crate: local_crate, node: id });
    for variants.iter().advance |variant| {
        index.push(entry {val: variant.node.id, pos: ebml_w.writer.tell()});
        ebml_w.start_tag(tag_items_data_item);
        encode_def_id(ebml_w, local_def(variant.node.id));
        encode_family(ebml_w, 'v');
        encode_name(ecx, ebml_w, variant.node.name);
        encode_parent_item(ebml_w, local_def(id));
        encode_visibility(ebml_w, variant.node.vis);
        encode_type(ecx, ebml_w,
                    node_id_to_type(ecx.tcx, variant.node.id));
        match variant.node.kind {
            ast::tuple_variant_kind(ref args)
                    if args.len() > 0 && generics.ty_params.len() == 0 => {
                encode_symbol(ecx, ebml_w, variant.node.id);
            }
            ast::tuple_variant_kind(_) | ast::struct_variant_kind(_) => {}
        }
        encode_discriminant(ecx, ebml_w, variant.node.id);
        if vi[i].disr_val != disr_val {
            encode_disr_val(ecx, ebml_w, vi[i].disr_val);
            disr_val = vi[i].disr_val;
        }
        encode_type_param_bounds(ebml_w, ecx, &generics.ty_params);
        encode_path(ecx, ebml_w, path,
                    ast_map::path_name(variant.node.name));
        ebml_w.end_tag();
        disr_val += 1;
        i += 1;
    }
}

fn encode_path(ecx: &EncodeContext,
               ebml_w: &mut writer::Encoder,
               path: &[ast_map::path_elt],
               name: ast_map::path_elt) {
    fn encode_path_elt(ecx: &EncodeContext,
                       ebml_w: &mut writer::Encoder,
                       elt: ast_map::path_elt) {
        let (tag, name) = match elt {
          ast_map::path_mod(name) => (tag_path_elt_mod, name),
          ast_map::path_name(name) => (tag_path_elt_name, name)
        };

        ebml_w.wr_tagged_str(tag, ecx.tcx.sess.str_of(name));
    }

    ebml_w.start_tag(tag_path);
    ebml_w.wr_tagged_u32(tag_path_len, (path.len() + 1) as u32);
    for path.iter().advance |pe| {
        encode_path_elt(ecx, ebml_w, *pe);
    }
    encode_path_elt(ecx, ebml_w, name);
    ebml_w.end_tag();
}

fn encode_reexported_static_method(ecx: &EncodeContext,
                                   ebml_w: &mut writer::Encoder,
                                   exp: &middle::resolve::Export2,
                                   method_def_id: def_id,
                                   method_ident: ident) {
    debug!("(encode reexported static method) %s::%s",
            exp.name, ecx.tcx.sess.str_of(method_ident));
    ebml_w.start_tag(tag_items_data_item_reexport);
    ebml_w.start_tag(tag_items_data_item_reexport_def_id);
    ebml_w.wr_str(def_to_str(method_def_id));
    ebml_w.end_tag();
    ebml_w.start_tag(tag_items_data_item_reexport_name);
    ebml_w.wr_str(fmt!("%s::%s", exp.name, ecx.tcx.sess.str_of(method_ident)));
    ebml_w.end_tag();
    ebml_w.end_tag();
}

fn encode_reexported_static_base_methods(ecx: &EncodeContext,
                                         ebml_w: &mut writer::Encoder,
                                         exp: &middle::resolve::Export2)
                                         -> bool {
    match ecx.tcx.base_impls.find(&exp.def_id) {
        Some(implementations) => {
            for implementations.iter().advance |&base_impl| {
                for base_impl.methods.iter().advance |&m| {
                    if m.explicit_self == ast::sty_static {
                        encode_reexported_static_method(ecx, ebml_w, exp,
                                                        m.did, m.ident);
                    }
                }
            }

            true
        }
        None => { false }
    }
}

fn encode_reexported_static_trait_methods(ecx: &EncodeContext,
                                          ebml_w: &mut writer::Encoder,
                                          exp: &middle::resolve::Export2)
                                          -> bool {
    match ecx.tcx.trait_methods_cache.find(&exp.def_id) {
        Some(methods) => {
            for methods.iter().advance |&m| {
                if m.explicit_self == ast::sty_static {
                    encode_reexported_static_method(ecx, ebml_w, exp,
                                                    m.def_id, m.ident);
                }
            }

            true
        }
        None => { false }
    }
}

fn encode_reexported_static_methods(ecx: &EncodeContext,
                                    ebml_w: &mut writer::Encoder,
                                    mod_path: &[ast_map::path_elt],
                                    exp: &middle::resolve::Export2) {
    match ecx.tcx.items.find(&exp.def_id.node) {
        Some(&ast_map::node_item(item, path)) => {
            let original_name = ecx.tcx.sess.str_of(item.ident);

            //
            // We don't need to reexport static methods on items
            // declared in the same module as our `pub use ...` since
            // that's done when we encode the item itself.
            //
            // The only exception is when the reexport *changes* the
            // name e.g. `pub use Foo = self::Bar` -- we have
            // encoded metadata for static methods relative to Bar,
            // but not yet for Foo.
            //
            if mod_path != *path || exp.name != original_name {
                if !encode_reexported_static_base_methods(ecx, ebml_w, exp) {
                    if encode_reexported_static_trait_methods(ecx, ebml_w, exp) {
                        debug!(fmt!("(encode reexported static methods) %s \
                                    [trait]",
                                    original_name));
                    }
                }
                else {
                    debug!(fmt!("(encode reexported static methods) %s [base]",
                                original_name));
                }
            }
        }
        _ => {}
    }
}

/// Iterates through "auxiliary node IDs", which are node IDs that describe
/// top-level items that are sub-items of the given item. Specifically:
///
/// * For enums, iterates through the node IDs of the variants.
///
/// * For newtype structs, iterates through the node ID of the constructor.
fn each_auxiliary_node_id(item: @item, callback: &fn(node_id) -> bool)
                          -> bool {
    let mut continue = true;
    match item.node {
        item_enum(ref enum_def, _) => {
            for enum_def.variants.iter().advance |variant| {
                continue = callback(variant.node.id);
                if !continue {
                    break
                }
            }
        }
        item_struct(struct_def, _) => {
            // If this is a newtype struct, return the constructor.
            match struct_def.ctor_id {
                Some(ctor_id) if struct_def.fields.len() > 0 &&
                        struct_def.fields[0].node.kind ==
                        ast::unnamed_field => {
                    continue = callback(ctor_id);
                }
                _ => {}
            }
        }
        _ => {}
    }

    continue
}

fn encode_reexports(ecx: &EncodeContext,
                    ebml_w: &mut writer::Encoder,
                    id: node_id,
                    path: &[ast_map::path_elt]) {
    debug!("(encoding info for module) encoding reexports for %d", id);
    match ecx.reexports2.find(&id) {
        Some(ref exports) => {
            debug!("(encoding info for module) found reexports for %d", id);
            for exports.iter().advance |exp| {
                debug!("(encoding info for module) reexport '%s' for %d",
                       exp.name, id);
                ebml_w.start_tag(tag_items_data_item_reexport);
                ebml_w.start_tag(tag_items_data_item_reexport_def_id);
                ebml_w.wr_str(def_to_str(exp.def_id));
                ebml_w.end_tag();
                ebml_w.start_tag(tag_items_data_item_reexport_name);
                ebml_w.wr_str(exp.name);
                ebml_w.end_tag();
                ebml_w.end_tag();
                encode_reexported_static_methods(ecx, ebml_w, path, exp);
            }
        }
        None => {
            debug!("(encoding info for module) found no reexports for %d",
                   id);
        }
    }
}

fn encode_info_for_mod(ecx: &EncodeContext,
                       ebml_w: &mut writer::Encoder,
                       md: &_mod,
                       id: node_id,
                       path: &[ast_map::path_elt],
                       name: ident,
                       vis: visibility) {
    ebml_w.start_tag(tag_items_data_item);
    encode_def_id(ebml_w, local_def(id));
    encode_family(ebml_w, 'm');
    encode_name(ecx, ebml_w, name);
    debug!("(encoding info for module) encoding info for module ID %d", id);

    // Encode info about all the module children.
    for md.items.iter().advance |item| {
        ebml_w.start_tag(tag_mod_child);
        ebml_w.wr_str(def_to_str(local_def(item.id)));
        ebml_w.end_tag();

        for each_auxiliary_node_id(*item) |auxiliary_node_id| {
            ebml_w.start_tag(tag_mod_child);
            ebml_w.wr_str(def_to_str(local_def(auxiliary_node_id)));
            ebml_w.end_tag();
        }

        match item.node {
            item_impl(*) => {
                let (ident, did) = (item.ident, item.id);
                debug!("(encoding info for module) ... encoding impl %s \
                        (%?/%?)",
                        ecx.tcx.sess.str_of(ident),
                        did,
                        ast_map::node_id_to_str(ecx.tcx.items, did, token::get_ident_interner()));

                ebml_w.start_tag(tag_mod_impl);
                ebml_w.wr_str(def_to_str(local_def(did)));
                ebml_w.end_tag();
            }
            _ => {}
        }
    }

    encode_path(ecx, ebml_w, path, ast_map::path_mod(name));

    // Encode the reexports of this module, if this module is public.
    if vis == public {
        debug!("(encoding info for module) encoding reexports for %d", id);
        encode_reexports(ecx, ebml_w, id, path);
    }

    ebml_w.end_tag();
}

fn encode_struct_field_family(ebml_w: &mut writer::Encoder,
                              visibility: visibility) {
    encode_family(ebml_w, match visibility {
        public => 'g',
        private => 'j',
        inherited => 'N'
    });
}

fn encode_visibility(ebml_w: &mut writer::Encoder, visibility: visibility) {
    ebml_w.start_tag(tag_items_data_item_visibility);
    let ch = match visibility {
        public => 'y',
        private => 'n',
        inherited => 'i',
    };
    ebml_w.wr_str(str::from_char(ch));
    ebml_w.end_tag();
}

fn encode_explicit_self(ebml_w: &mut writer::Encoder, explicit_self: ast::explicit_self_) {
    ebml_w.start_tag(tag_item_trait_method_explicit_self);

    // Encode the base self type.
    match explicit_self {
        sty_static => {
            ebml_w.writer.write(&[ 's' as u8 ]);
        }
        sty_value => {
            ebml_w.writer.write(&[ 'v' as u8 ]);
        }
        sty_region(_, m) => {
            // FIXME(#4846) encode custom lifetime
            ebml_w.writer.write(&[ '&' as u8 ]);
            encode_mutability(ebml_w, m);
        }
        sty_box(m) => {
            ebml_w.writer.write(&[ '@' as u8 ]);
            encode_mutability(ebml_w, m);
        }
        sty_uniq(m) => {
            ebml_w.writer.write(&[ '~' as u8 ]);
            encode_mutability(ebml_w, m);
        }
    }

    ebml_w.end_tag();

    fn encode_mutability(ebml_w: &writer::Encoder,
                         m: ast::mutability) {
        match m {
            m_imm => {
                ebml_w.writer.write(&[ 'i' as u8 ]);
            }
            m_mutbl => {
                ebml_w.writer.write(&[ 'm' as u8 ]);
            }
            m_const => {
                ebml_w.writer.write(&[ 'c' as u8 ]);
            }
        }
    }
}

fn encode_method_sort(ebml_w: &mut writer::Encoder, sort: char) {
    ebml_w.start_tag(tag_item_trait_method_sort);
    ebml_w.writer.write(&[ sort as u8 ]);
    ebml_w.end_tag();
}

/* Returns an index of items in this class */
fn encode_info_for_struct(ecx: &EncodeContext,
                          ebml_w: &mut writer::Encoder,
                          path: &[ast_map::path_elt],
                          fields: &[@struct_field],
                          global_index: @mut ~[entry<int>])
                          -> ~[entry<int>] {
    /* Each class has its own index, since different classes
       may have fields with the same name */
    let index = @mut ~[];
    let tcx = ecx.tcx;
     /* We encode both private and public fields -- need to include
        private fields to get the offsets right */
    for fields.iter().advance |field| {
        let (nm, vis) = match field.node.kind {
            named_field(nm, vis) => (nm, vis),
            unnamed_field => (special_idents::unnamed_field, inherited)
        };

        let id = field.node.id;
        index.push(entry {val: id, pos: ebml_w.writer.tell()});
        global_index.push(entry {val: id, pos: ebml_w.writer.tell()});
        ebml_w.start_tag(tag_items_data_item);
        debug!("encode_info_for_struct: doing %s %d",
               tcx.sess.str_of(nm), id);
        encode_struct_field_family(ebml_w, vis);
        encode_name(ecx, ebml_w, nm);
        encode_path(ecx, ebml_w, path, ast_map::path_name(nm));
        encode_type(ecx, ebml_w, node_id_to_type(tcx, id));
        encode_def_id(ebml_w, local_def(id));
        ebml_w.end_tag();
    }
    /*bad*/copy *index
}

// This is for encoding info for ctors and dtors
fn encode_info_for_ctor(ecx: &EncodeContext,
                        ebml_w: &mut writer::Encoder,
                        id: node_id,
                        ident: ident,
                        path: &[ast_map::path_elt],
                        item: Option<inlined_item>,
                        generics: &ast::Generics) {
        ebml_w.start_tag(tag_items_data_item);
        encode_name(ecx, ebml_w, ident);
        encode_def_id(ebml_w, local_def(id));
        encode_family(ebml_w, purity_fn_family(ast::impure_fn));
        encode_type_param_bounds(ebml_w, ecx, &generics.ty_params);
        let its_ty = node_id_to_type(ecx.tcx, id);
        debug!("fn name = %s ty = %s its node id = %d",
               ecx.tcx.sess.str_of(ident),
               ty_to_str(ecx.tcx, its_ty), id);
        encode_type(ecx, ebml_w, its_ty);
        encode_path(ecx, ebml_w, path, ast_map::path_name(ident));
        match item {
           Some(it) => {
             (ecx.encode_inlined_item)(ecx, ebml_w, path, it);
           }
           None => {
             encode_symbol(ecx, ebml_w, id);
           }
        }
        ebml_w.end_tag();
}

fn encode_info_for_struct_ctor(ecx: &EncodeContext,
                               ebml_w: &mut writer::Encoder,
                               path: &[ast_map::path_elt],
                               name: ast::ident,
                               ctor_id: node_id,
                               index: @mut ~[entry<int>]) {
    index.push(entry { val: ctor_id, pos: ebml_w.writer.tell() });

    ebml_w.start_tag(tag_items_data_item);
    encode_def_id(ebml_w, local_def(ctor_id));
    encode_family(ebml_w, 'f');
    encode_name(ecx, ebml_w, name);
    encode_type(ecx, ebml_w, node_id_to_type(ecx.tcx, ctor_id));
    encode_path(ecx, ebml_w, path, ast_map::path_name(name));

    if ecx.item_symbols.contains_key(&ctor_id) {
        encode_symbol(ecx, ebml_w, ctor_id);
    }

    ebml_w.end_tag();
}

fn encode_method_ty_fields(ecx: &EncodeContext,
                           ebml_w: &mut writer::Encoder,
                           method_ty: &ty::Method) {
    encode_def_id(ebml_w, method_ty.def_id);
    encode_name(ecx, ebml_w, method_ty.ident);
    encode_ty_type_param_defs(ebml_w, ecx,
                              method_ty.generics.type_param_defs,
                              tag_item_method_tps);
    encode_transformed_self_ty(ecx, ebml_w, method_ty.transformed_self_ty);
    encode_method_fty(ecx, ebml_w, &method_ty.fty);
    encode_visibility(ebml_w, method_ty.vis);
    encode_explicit_self(ebml_w, method_ty.explicit_self);
}

fn encode_info_for_method(ecx: &EncodeContext,
                          ebml_w: &mut writer::Encoder,
                          impl_path: &[ast_map::path_elt],
                          should_inline: bool,
                          parent_id: node_id,
                          m: @method,
                          owner_generics: &ast::Generics,
                          method_generics: &ast::Generics) {
    debug!("encode_info_for_method: %d %s %u %u", m.id,
           ecx.tcx.sess.str_of(m.ident),
           owner_generics.ty_params.len(),
           method_generics.ty_params.len());
    ebml_w.start_tag(tag_items_data_item);

    let method_def_id = local_def(m.id);
    let method_ty = ty::method(ecx.tcx, method_def_id);
    encode_method_ty_fields(ecx, ebml_w, method_ty);

    match m.explicit_self.node {
        ast::sty_static => {
            encode_family(ebml_w, purity_static_method_family(m.purity));
        }
        _ => encode_family(ebml_w, purity_fn_family(m.purity))
    }

    let mut combined_ty_params = opt_vec::Empty;
    for owner_generics.ty_params.iter().advance |x| { combined_ty_params.push(copy *x) }
    for method_generics.ty_params.iter().advance |x| { combined_ty_params.push(copy *x) }
    let len = combined_ty_params.len();
    encode_type_param_bounds(ebml_w, ecx, &combined_ty_params);

    encode_type(ecx, ebml_w, node_id_to_type(ecx.tcx, m.id));
    encode_path(ecx, ebml_w, impl_path, ast_map::path_name(m.ident));

    if len > 0u || should_inline {
        (ecx.encode_inlined_item)(
           ecx, ebml_w, impl_path,
           ii_method(local_def(parent_id), m));
    } else {
        encode_symbol(ecx, ebml_w, m.id);
    }

    ebml_w.end_tag();
}

fn purity_fn_family(p: purity) -> char {
    match p {
      unsafe_fn => 'u',
      impure_fn => 'f',
      extern_fn => 'e'
    }
}

fn purity_static_method_family(p: purity) -> char {
    match p {
      unsafe_fn => 'U',
      impure_fn => 'F',
      _ => fail!("extern fn can't be static")
    }
}


fn should_inline(attrs: &[attribute]) -> bool {
    match attr::find_inline_attr(attrs) {
        attr::ia_none | attr::ia_never  => false,
        attr::ia_hint | attr::ia_always => true
    }
}

fn encode_info_for_item(ecx: &EncodeContext,
                        ebml_w: &mut writer::Encoder,
                        item: @item,
                        index: @mut ~[entry<int>],
                        path: &[ast_map::path_elt]) {
    let tcx = ecx.tcx;

    fn add_to_index_(item: @item, ebml_w: &writer::Encoder,
                     index: @mut ~[entry<int>]) {
        index.push(entry { val: item.id, pos: ebml_w.writer.tell() });
    }
    let add_to_index: &fn() = || add_to_index_(item, ebml_w, index);

    debug!("encoding info for item at %s",
           ecx.tcx.sess.codemap.span_to_str(item.span));

    match item.node {
      item_static(_, m, _) => {
        add_to_index();
        ebml_w.start_tag(tag_items_data_item);
        encode_def_id(ebml_w, local_def(item.id));
        if m == ast::m_mutbl {
            encode_family(ebml_w, 'b');
        } else {
            encode_family(ebml_w, 'c');
        }
        encode_type(ecx, ebml_w, node_id_to_type(tcx, item.id));
        encode_symbol(ecx, ebml_w, item.id);
        encode_name(ecx, ebml_w, item.ident);
        encode_path(ecx, ebml_w, path, ast_map::path_name(item.ident));
        (ecx.encode_inlined_item)(ecx, ebml_w, path, ii_item(item));
        ebml_w.end_tag();
      }
      item_fn(_, purity, _, ref generics, _) => {
        add_to_index();
        ebml_w.start_tag(tag_items_data_item);
        encode_def_id(ebml_w, local_def(item.id));
        encode_family(ebml_w, purity_fn_family(purity));
        let tps_len = generics.ty_params.len();
        encode_type_param_bounds(ebml_w, ecx, &generics.ty_params);
        encode_type(ecx, ebml_w, node_id_to_type(tcx, item.id));
        encode_name(ecx, ebml_w, item.ident);
        encode_path(ecx, ebml_w, path, ast_map::path_name(item.ident));
        encode_attributes(ebml_w, item.attrs);
        if tps_len > 0u || should_inline(item.attrs) {
            (ecx.encode_inlined_item)(ecx, ebml_w, path, ii_item(item));
        } else {
            encode_symbol(ecx, ebml_w, item.id);
        }
        ebml_w.end_tag();
      }
      item_mod(ref m) => {
        add_to_index();
        encode_info_for_mod(ecx,
                            ebml_w,
                            m,
                            item.id,
                            path,
                            item.ident,
                            item.vis);
      }
      item_foreign_mod(ref fm) => {
        add_to_index();
        ebml_w.start_tag(tag_items_data_item);
        encode_def_id(ebml_w, local_def(item.id));
        encode_family(ebml_w, 'n');
        encode_name(ecx, ebml_w, item.ident);
        encode_path(ecx, ebml_w, path, ast_map::path_name(item.ident));

        // Encode all the items in this module.
        for fm.items.iter().advance |foreign_item| {
            ebml_w.start_tag(tag_mod_child);
            ebml_w.wr_str(def_to_str(local_def(foreign_item.id)));
            ebml_w.end_tag();
        }

        ebml_w.end_tag();
      }
      item_ty(_, ref generics) => {
        add_to_index();
        ebml_w.start_tag(tag_items_data_item);
        encode_def_id(ebml_w, local_def(item.id));
        encode_family(ebml_w, 'y');
        encode_type_param_bounds(ebml_w, ecx, &generics.ty_params);
        encode_type(ecx, ebml_w, node_id_to_type(tcx, item.id));
        encode_name(ecx, ebml_w, item.ident);
        encode_path(ecx, ebml_w, path, ast_map::path_name(item.ident));
        encode_region_param(ecx, ebml_w, item);
        ebml_w.end_tag();
      }
      item_enum(ref enum_definition, ref generics) => {
        add_to_index();

        ebml_w.start_tag(tag_items_data_item);
        encode_def_id(ebml_w, local_def(item.id));
        encode_family(ebml_w, 't');
        encode_type_param_bounds(ebml_w, ecx, &generics.ty_params);
        encode_type(ecx, ebml_w, node_id_to_type(tcx, item.id));
        encode_name(ecx, ebml_w, item.ident);
        for (*enum_definition).variants.iter().advance |v| {
            encode_variant_id(ebml_w, local_def(v.node.id));
        }
        (ecx.encode_inlined_item)(ecx, ebml_w, path, ii_item(item));
        encode_path(ecx, ebml_w, path, ast_map::path_name(item.ident));
        encode_region_param(ecx, ebml_w, item);
        ebml_w.end_tag();

        encode_enum_variant_info(ecx,
                                 ebml_w,
                                 item.id,
                                 (*enum_definition).variants,
                                 path,
                                 index,
                                 generics);
      }
      item_struct(struct_def, ref generics) => {
        /* First, encode the fields
           These come first because we need to write them to make
           the index, and the index needs to be in the item for the
           class itself */
        let idx = encode_info_for_struct(ecx, ebml_w, path,
                                         struct_def.fields, index);

        /* Index the class*/
        add_to_index();

        /* Now, make an item for the class itself */
        ebml_w.start_tag(tag_items_data_item);
        encode_def_id(ebml_w, local_def(item.id));
        encode_family(ebml_w, 'S');
        encode_type_param_bounds(ebml_w, ecx, &generics.ty_params);
        encode_type(ecx, ebml_w, node_id_to_type(tcx, item.id));

        encode_name(ecx, ebml_w, item.ident);
        encode_attributes(ebml_w, item.attrs);
        encode_path(ecx, ebml_w, path, ast_map::path_name(item.ident));
        encode_region_param(ecx, ebml_w, item);

        /* Encode def_ids for each field and method
         for methods, write all the stuff get_trait_method
        needs to know*/
        for struct_def.fields.iter().advance |f| {
            match f.node.kind {
                named_field(ident, vis) => {
                   ebml_w.start_tag(tag_item_field);
                   encode_struct_field_family(ebml_w, vis);
                   encode_name(ecx, ebml_w, ident);
                   encode_def_id(ebml_w, local_def(f.node.id));
                   ebml_w.end_tag();
                }
                unnamed_field => {
                    ebml_w.start_tag(tag_item_unnamed_field);
                    encode_def_id(ebml_w, local_def(f.node.id));
                    ebml_w.end_tag();
                }
            }
        }

        /* Each class has its own index -- encode it */
        let bkts = create_index(idx);
        encode_index(ebml_w, bkts, write_int);
        ebml_w.end_tag();

        // If this is a tuple- or enum-like struct, encode the type of the
        // constructor.
        if struct_def.fields.len() > 0 &&
                struct_def.fields[0].node.kind == ast::unnamed_field {
            let ctor_id = match struct_def.ctor_id {
                Some(ctor_id) => ctor_id,
                None => ecx.tcx.sess.bug("struct def didn't have ctor id"),
            };

            encode_info_for_struct_ctor(ecx,
                                        ebml_w,
                                        path,
                                        item.ident,
                                        ctor_id,
                                        index);
        }
      }
      item_impl(ref generics, opt_trait, ty, ref methods) => {
        add_to_index();
        ebml_w.start_tag(tag_items_data_item);
        encode_def_id(ebml_w, local_def(item.id));
        encode_family(ebml_w, 'i');
        encode_region_param(ecx, ebml_w, item);
        encode_type_param_bounds(ebml_w, ecx, &generics.ty_params);
        encode_type(ecx, ebml_w, node_id_to_type(tcx, item.id));
        encode_name(ecx, ebml_w, item.ident);
        encode_attributes(ebml_w, item.attrs);
        match ty.node {
            ast::ty_path(path, bounds, _) if path.idents.len() == 1 => {
                assert!(bounds.is_none());
                encode_impl_type_basename(ecx, ebml_w,
                                          ast_util::path_to_ident(path));
            }
            _ => {}
        }
        for methods.iter().advance |m| {
            ebml_w.start_tag(tag_item_impl_method);
            let method_def_id = local_def(m.id);
            let s = def_to_str(method_def_id);
            ebml_w.writer.write(s.as_bytes());
            ebml_w.end_tag();
        }
        for opt_trait.iter().advance |ast_trait_ref| {
            let trait_ref = ty::node_id_to_trait_ref(ecx.tcx, ast_trait_ref.ref_id);
            encode_trait_ref(ebml_w, ecx, trait_ref, tag_item_trait_ref);
        }
        encode_path(ecx, ebml_w, path, ast_map::path_name(item.ident));
        ebml_w.end_tag();

        // >:-<
        let mut impl_path = vec::append(~[], path);
        impl_path.push(ast_map::path_name(item.ident));

        for methods.iter().advance |m| {
            index.push(entry {val: m.id, pos: ebml_w.writer.tell()});
            encode_info_for_method(ecx,
                                   ebml_w,
                                   impl_path,
                                   should_inline(m.attrs),
                                   item.id,
                                   *m,
                                   generics,
                                   &m.generics);
        }
      }
      item_trait(ref generics, ref super_traits, ref ms) => {
        add_to_index();
        ebml_w.start_tag(tag_items_data_item);
        encode_def_id(ebml_w, local_def(item.id));
        encode_family(ebml_w, 'I');
        encode_region_param(ecx, ebml_w, item);
        encode_type_param_bounds(ebml_w, ecx, &generics.ty_params);
        let trait_def = ty::lookup_trait_def(tcx, local_def(item.id));
        encode_trait_ref(ebml_w, ecx, trait_def.trait_ref, tag_item_trait_ref);
        encode_name(ecx, ebml_w, item.ident);
        encode_attributes(ebml_w, item.attrs);
        for ty::trait_method_def_ids(tcx, local_def(item.id)).iter().advance |&method_def_id| {
            ebml_w.start_tag(tag_item_trait_method);
            encode_def_id(ebml_w, method_def_id);
            ebml_w.end_tag();

            ebml_w.start_tag(tag_mod_child);
            ebml_w.wr_str(def_to_str(method_def_id));
            ebml_w.end_tag();
        }
        encode_path(ecx, ebml_w, path, ast_map::path_name(item.ident));
        for super_traits.iter().advance |ast_trait_ref| {
            let trait_ref = ty::node_id_to_trait_ref(ecx.tcx, ast_trait_ref.ref_id);
            encode_trait_ref(ebml_w, ecx, trait_ref, tag_item_super_trait_ref);
        }
        ebml_w.end_tag();

        // Now output the method info for each method.
        let r = ty::trait_method_def_ids(tcx, local_def(item.id));
        for r.iter().enumerate().advance |(i, &method_def_id)| {
            assert_eq!(method_def_id.crate, ast::local_crate);

            let method_ty = ty::method(tcx, method_def_id);

            index.push(entry {val: method_def_id.node, pos: ebml_w.writer.tell()});

            ebml_w.start_tag(tag_items_data_item);

            encode_method_ty_fields(ecx, ebml_w, method_ty);

            encode_parent_item(ebml_w, local_def(item.id));

            let mut trait_path = vec::append(~[], path);
            trait_path.push(ast_map::path_name(item.ident));
            encode_path(ecx, ebml_w, trait_path, ast_map::path_name(method_ty.ident));

            match method_ty.explicit_self {
                sty_static => {
                    encode_family(ebml_w,
                                  purity_static_method_family(
                                      method_ty.fty.purity));

                    let tpt = ty::lookup_item_type(tcx, method_def_id);
                    encode_ty_type_param_defs(ebml_w, ecx,
                                              tpt.generics.type_param_defs,
                                              tag_items_data_item_ty_param_bounds);
                    encode_type(ecx, ebml_w, tpt.ty);
                }

                _ => {
                    encode_family(ebml_w,
                                  purity_fn_family(
                                      method_ty.fty.purity));
                }
            }

            match ms[i] {
                required(_) => {
                    encode_method_sort(ebml_w, 'r');
                }

                provided(m) => {
                    // This is obviously a bogus assert but I don't think this
                    // ever worked before anyhow...near as I can tell, before
                    // we would emit two items.
                    if method_ty.explicit_self == sty_static {
                        tcx.sess.span_unimpl(
                            item.span,
                            fmt!("Method %s is both provided and static",
                                 token::ident_to_str(&method_ty.ident)));
                    }
                    encode_type_param_bounds(ebml_w, ecx,
                                             &m.generics.ty_params);
                    encode_method_sort(ebml_w, 'p');
                    (ecx.encode_inlined_item)(
                        ecx, ebml_w, path,
                        ii_method(local_def(item.id), m));
                }
            }

            ebml_w.end_tag();
        }
      }
      item_mac(*) => fail!("item macros unimplemented")
    }
}

fn encode_info_for_foreign_item(ecx: &EncodeContext,
                                ebml_w: &mut writer::Encoder,
                                nitem: @foreign_item,
                                index: @mut ~[entry<int>],
                                path: ast_map::path,
                                abi: AbiSet) {
    index.push(entry { val: nitem.id, pos: ebml_w.writer.tell() });

    ebml_w.start_tag(tag_items_data_item);
    match nitem.node {
      foreign_item_fn(_, purity, ref generics) => {
        encode_def_id(ebml_w, local_def(nitem.id));
        encode_family(ebml_w, purity_fn_family(purity));
        encode_type_param_bounds(ebml_w, ecx, &generics.ty_params);
        encode_type(ecx, ebml_w, node_id_to_type(ecx.tcx, nitem.id));
        encode_name(ecx, ebml_w, nitem.ident);
        if abi.is_intrinsic() {
            (ecx.encode_inlined_item)(ecx, ebml_w, path, ii_foreign(nitem));
        } else {
            encode_symbol(ecx, ebml_w, nitem.id);
        }
        encode_path(ecx, ebml_w, path, ast_map::path_name(nitem.ident));
      }
      foreign_item_static(_, mutbl) => {
        encode_def_id(ebml_w, local_def(nitem.id));
        if mutbl {
            encode_family(ebml_w, 'b');
        } else {
            encode_family(ebml_w, 'c');
        }
        encode_type(ecx, ebml_w, node_id_to_type(ecx.tcx, nitem.id));
        encode_symbol(ecx, ebml_w, nitem.id);
        encode_name(ecx, ebml_w, nitem.ident);
        encode_path(ecx, ebml_w, path, ast_map::path_name(nitem.ident));
      }
    }
    ebml_w.end_tag();
}

fn encode_info_for_items(ecx: &EncodeContext,
                         ebml_w: &mut writer::Encoder,
                         crate: &crate)
                         -> ~[entry<int>] {
    let index = @mut ~[];
    ebml_w.start_tag(tag_items_data);
    index.push(entry { val: crate_node_id, pos: ebml_w.writer.tell() });
    encode_info_for_mod(ecx,
                        ebml_w,
                        &crate.node.module,
                        crate_node_id,
                        [],
                        syntax::parse::token::special_idents::invalid,
                        public);
    let items = ecx.tcx.items;

    // See comment in `encode_side_tables_for_ii` in astencode
    let ecx_ptr : *() = unsafe { cast::transmute(ecx) };

    visit::visit_crate(crate, ((), visit::mk_vt(@visit::Visitor {
        visit_expr: |_e, (_cx, _v)| { },
        visit_item: {
            let ebml_w = copy *ebml_w;
            |i, (cx, v)| {
                visit::visit_item(i, (cx, v));
                match items.get_copy(&i.id) {
                    ast_map::node_item(_, pt) => {
                        let mut ebml_w = copy ebml_w;
                        // See above
                        let ecx : &EncodeContext = unsafe { cast::transmute(ecx_ptr) };
                        encode_info_for_item(ecx, &mut ebml_w, i, index, *pt);
                    }
                    _ => fail!("bad item")
                }
            }
        },
        visit_foreign_item: {
            let ebml_w = copy *ebml_w;
            |ni, (cx, v)| {
                visit::visit_foreign_item(ni, (cx, v));
                match items.get_copy(&ni.id) {
                    ast_map::node_foreign_item(_, abi, _, pt) => {
                        debug!("writing foreign item %s::%s",
                               ast_map::path_to_str(
                                *pt,
                                token::get_ident_interner()),
                                token::ident_to_str(&ni.ident));

                        let mut ebml_w = copy ebml_w;
                        // See above
                        let ecx : &EncodeContext = unsafe { cast::transmute(ecx_ptr) };
                        encode_info_for_foreign_item(ecx,
                                                     &mut ebml_w,
                                                     ni,
                                                     index,
                                                     /*bad*/copy *pt,
                                                     abi);
                    }
                    // case for separate item and foreign-item tables
                    _ => fail!("bad foreign item")
                }
            }
        },
        ..*visit::default_visitor()
    })));
    ebml_w.end_tag();
    return /*bad*/copy *index;
}


// Path and definition ID indexing

fn create_index<T:Copy + Hash + IterBytes>(index: ~[entry<T>]) ->
   ~[@~[entry<T>]] {
    let mut buckets: ~[@mut ~[entry<T>]] = ~[];
    for uint::range(0u, 256u) |_i| { buckets.push(@mut ~[]); };
    for index.iter().advance |elt| {
        let h = elt.val.hash() as uint;
        buckets[h % 256].push(copy *elt);
    }

    let mut buckets_frozen = ~[];
    for buckets.iter().advance |bucket| {
        buckets_frozen.push(@/*bad*/copy **bucket);
    }
    return buckets_frozen;
}

fn encode_index<T>(ebml_w: &mut writer::Encoder,
                   buckets: ~[@~[entry<T>]],
                   write_fn: &fn(@io::Writer, &T)) {
    let writer = ebml_w.writer;
    ebml_w.start_tag(tag_index);
    let mut bucket_locs: ~[uint] = ~[];
    ebml_w.start_tag(tag_index_buckets);
    for buckets.iter().advance |bucket| {
        bucket_locs.push(ebml_w.writer.tell());
        ebml_w.start_tag(tag_index_buckets_bucket);
        for (**bucket).iter().advance |elt| {
            ebml_w.start_tag(tag_index_buckets_bucket_elt);
            assert!(elt.pos < 0xffff_ffff);
            writer.write_be_u32(elt.pos as u32);
            write_fn(writer, &elt.val);
            ebml_w.end_tag();
        }
        ebml_w.end_tag();
    }
    ebml_w.end_tag();
    ebml_w.start_tag(tag_index_table);
    for bucket_locs.iter().advance |pos| {
        assert!(*pos < 0xffff_ffff);
        writer.write_be_u32(*pos as u32);
    }
    ebml_w.end_tag();
    ebml_w.end_tag();
}

fn write_str(writer: @io::Writer, s: ~str) {
    writer.write_str(s);
}

fn write_int(writer: @io::Writer, &n: &int) {
    assert!(n < 0x7fff_ffff);
    writer.write_be_u32(n as u32);
}

fn encode_meta_item(ebml_w: &mut writer::Encoder, mi: @meta_item) {
    match mi.node {
      meta_word(name) => {
        ebml_w.start_tag(tag_meta_item_word);
        ebml_w.start_tag(tag_meta_item_name);
        ebml_w.writer.write(name.as_bytes());
        ebml_w.end_tag();
        ebml_w.end_tag();
      }
      meta_name_value(name, value) => {
        match value.node {
          lit_str(value) => {
            ebml_w.start_tag(tag_meta_item_name_value);
            ebml_w.start_tag(tag_meta_item_name);
            ebml_w.writer.write(name.as_bytes());
            ebml_w.end_tag();
            ebml_w.start_tag(tag_meta_item_value);
            ebml_w.writer.write(value.as_bytes());
            ebml_w.end_tag();
            ebml_w.end_tag();
          }
          _ => {/* FIXME (#623): encode other variants */ }
        }
      }
      meta_list(name, ref items) => {
        ebml_w.start_tag(tag_meta_item_list);
        ebml_w.start_tag(tag_meta_item_name);
        ebml_w.writer.write(name.as_bytes());
        ebml_w.end_tag();
        for items.iter().advance |inner_item| {
            encode_meta_item(ebml_w, *inner_item);
        }
        ebml_w.end_tag();
      }
    }
}

fn encode_attributes(ebml_w: &mut writer::Encoder, attrs: &[attribute]) {
    ebml_w.start_tag(tag_attributes);
    for attrs.iter().advance |attr| {
        ebml_w.start_tag(tag_attribute);
        encode_meta_item(ebml_w, attr.node.value);
        ebml_w.end_tag();
    }
    ebml_w.end_tag();
}

// So there's a special crate attribute called 'link' which defines the
// metadata that Rust cares about for linking crates. This attribute requires
// 'name' and 'vers' items, so if the user didn't provide them we will throw
// them in anyway with default values.
fn synthesize_crate_attrs(ecx: &EncodeContext,
                          crate: &crate) -> ~[attribute] {

    fn synthesize_link_attr(ecx: &EncodeContext, items: ~[@meta_item]) ->
       attribute {

        assert!(!ecx.link_meta.name.is_empty());
        assert!(!ecx.link_meta.vers.is_empty());

        let name_item =
            attr::mk_name_value_item_str(@"name",
                                         ecx.link_meta.name);
        let vers_item =
            attr::mk_name_value_item_str(@"vers",
                                         ecx.link_meta.vers);

        let other_items =
            {
                let tmp = attr::remove_meta_items_by_name(items, "name");
                attr::remove_meta_items_by_name(tmp, "vers")
            };

        let meta_items = vec::append(~[name_item, vers_item], other_items);
        let link_item = attr::mk_list_item(@"link", meta_items);

        return attr::mk_attr(link_item);
    }

    let mut attrs: ~[attribute] = ~[];
    let mut found_link_attr = false;
    for crate.node.attrs.iter().advance |attr| {
        attrs.push(
            if "link" != attr::get_attr_name(attr)  {
                copy *attr
            } else {
                match attr.node.value.node {
                  meta_list(_, ref l) => {
                    found_link_attr = true;;
                    synthesize_link_attr(ecx, /*bad*/copy *l)
                  }
                  _ => copy *attr
                }
            });
    }

    if !found_link_attr { attrs.push(synthesize_link_attr(ecx, ~[])); }

    return attrs;
}

fn encode_crate_deps(ecx: &EncodeContext,
                     ebml_w: &mut writer::Encoder,
                     cstore: &cstore::CStore) {
    fn get_ordered_deps(ecx: &EncodeContext, cstore: &cstore::CStore)
                     -> ~[decoder::crate_dep] {
        type numdep = decoder::crate_dep;

        // Pull the cnums and name,vers,hash out of cstore
        let mut deps = ~[];
        do cstore::iter_crate_data(cstore) |key, val| {
            let dep = decoder::crate_dep {cnum: key,
                       name: ecx.tcx.sess.ident_of(val.name),
                       vers: decoder::get_crate_vers(val.data),
                       hash: decoder::get_crate_hash(val.data)};
            deps.push(dep);
        };

        // Sort by cnum
        extra::sort::quick_sort(deps, |kv1, kv2| kv1.cnum <= kv2.cnum);

        // Sanity-check the crate numbers
        let mut expected_cnum = 1;
        for deps.iter().advance |n| {
            assert_eq!(n.cnum, expected_cnum);
            expected_cnum += 1;
        }

        // mut -> immutable hack for vec::map
        deps.slice(0, deps.len()).to_owned()
    }

    // We're just going to write a list of crate 'name-hash-version's, with
    // the assumption that they are numbered 1 to n.
    // FIXME (#2166): This is not nearly enough to support correct versioning
    // but is enough to get transitive crate dependencies working.
    ebml_w.start_tag(tag_crate_deps);
    let r = get_ordered_deps(ecx, cstore);
    for r.iter().advance |dep| {
        encode_crate_dep(ecx, ebml_w, *dep);
    }
    ebml_w.end_tag();
}

fn encode_lang_items(ecx: &EncodeContext, ebml_w: &mut writer::Encoder) {
    ebml_w.start_tag(tag_lang_items);

    for ecx.tcx.lang_items.each_item |def_id, i| {
        if def_id.crate != local_crate {
            loop;
        }

        ebml_w.start_tag(tag_lang_items_item);

        ebml_w.start_tag(tag_lang_items_item_id);
        ebml_w.writer.write_be_u32(i as u32);
        ebml_w.end_tag();   // tag_lang_items_item_id

        ebml_w.start_tag(tag_lang_items_item_node_id);
        ebml_w.writer.write_be_u32(def_id.node as u32);
        ebml_w.end_tag();   // tag_lang_items_item_node_id

        ebml_w.end_tag();   // tag_lang_items_item
    }

    ebml_w.end_tag();   // tag_lang_items
}

fn encode_link_args(ecx: &EncodeContext, ebml_w: &mut writer::Encoder) {
    ebml_w.start_tag(tag_link_args);

    let link_args = cstore::get_used_link_args(ecx.cstore);
    for link_args.iter().advance |link_arg| {
        ebml_w.start_tag(tag_link_args_arg);
        ebml_w.writer.write_str(link_arg.to_str());
        ebml_w.end_tag();
    }

    ebml_w.end_tag();
}

fn encode_misc_info(ecx: &EncodeContext,
                    crate: &crate,
                    ebml_w: &mut writer::Encoder) {
    ebml_w.start_tag(tag_misc_info);
    ebml_w.start_tag(tag_misc_info_crate_items);
    for crate.node.module.items.iter().advance |&item| {
        ebml_w.start_tag(tag_mod_child);
        ebml_w.wr_str(def_to_str(local_def(item.id)));
        ebml_w.end_tag();

        for each_auxiliary_node_id(item) |auxiliary_node_id| {
            ebml_w.start_tag(tag_mod_child);
            ebml_w.wr_str(def_to_str(local_def(auxiliary_node_id)));
            ebml_w.end_tag();
        }
    }

    // Encode reexports for the root module.
    encode_reexports(ecx, ebml_w, 0, []);

    ebml_w.end_tag();
    ebml_w.end_tag();
}

fn encode_crate_dep(ecx: &EncodeContext,
                    ebml_w: &mut writer::Encoder,
                    dep: decoder::crate_dep) {
    ebml_w.start_tag(tag_crate_dep);
    ebml_w.start_tag(tag_crate_dep_name);
    let s = ecx.tcx.sess.str_of(dep.name);
    ebml_w.writer.write(s.as_bytes());
    ebml_w.end_tag();
    ebml_w.start_tag(tag_crate_dep_vers);
    ebml_w.writer.write(dep.vers.as_bytes());
    ebml_w.end_tag();
    ebml_w.start_tag(tag_crate_dep_hash);
    ebml_w.writer.write(dep.hash.as_bytes());
    ebml_w.end_tag();
    ebml_w.end_tag();
}

fn encode_hash(ebml_w: &mut writer::Encoder, hash: &str) {
    ebml_w.start_tag(tag_crate_hash);
    ebml_w.writer.write(hash.as_bytes());
    ebml_w.end_tag();
}

// NB: Increment this as you change the metadata encoding version.
pub static metadata_encoding_version : &'static [u8] =
    &[0x72, //'r' as u8,
      0x75, //'u' as u8,
      0x73, //'s' as u8,
      0x74, //'t' as u8,
      0, 0, 0, 1 ];

pub fn encode_metadata(parms: EncodeParams, crate: &crate) -> ~[u8] {
    let wr = @io::BytesWriter::new();
    let stats = Stats {
        inline_bytes: 0,
        attr_bytes: 0,
        dep_bytes: 0,
        lang_item_bytes: 0,
        link_args_bytes: 0,
        misc_bytes: 0,
        item_bytes: 0,
        index_bytes: 0,
        zero_bytes: 0,
        total_bytes: 0,
        n_inlines: 0
    };
    let EncodeParams {
        item_symbols,
        diag,
        tcx,
        reexports2,
        discrim_symbols,
        cstore,
        encode_inlined_item,
        link_meta,
        reachable,
        _
    } = parms;
    let type_abbrevs = @mut HashMap::new();
    let stats = @mut stats;
    let ecx = EncodeContext {
        diag: diag,
        tcx: tcx,
        stats: stats,
        reexports2: reexports2,
        item_symbols: item_symbols,
        discrim_symbols: discrim_symbols,
        link_meta: link_meta,
        cstore: cstore,
        encode_inlined_item: encode_inlined_item,
        type_abbrevs: type_abbrevs,
        reachable: reachable,
     };

    let mut ebml_w = writer::Encoder(wr as @io::Writer);

    encode_hash(&mut ebml_w, ecx.link_meta.extras_hash);

    let mut i = *wr.pos;
    let crate_attrs = synthesize_crate_attrs(&ecx, crate);
    encode_attributes(&mut ebml_w, crate_attrs);
    ecx.stats.attr_bytes = *wr.pos - i;

    i = *wr.pos;
    encode_crate_deps(&ecx, &mut ebml_w, ecx.cstore);
    ecx.stats.dep_bytes = *wr.pos - i;

    // Encode the language items.
    i = *wr.pos;
    encode_lang_items(&ecx, &mut ebml_w);
    ecx.stats.lang_item_bytes = *wr.pos - i;

    // Encode the link args.
    i = *wr.pos;
    encode_link_args(&ecx, &mut ebml_w);
    ecx.stats.link_args_bytes = *wr.pos - i;

    // Encode miscellaneous info.
    i = *wr.pos;
    encode_misc_info(&ecx, crate, &mut ebml_w);
    ecx.stats.misc_bytes = *wr.pos - i;

    // Encode and index the items.
    ebml_w.start_tag(tag_items);
    i = *wr.pos;
    let items_index = encode_info_for_items(&ecx, &mut ebml_w, crate);
    ecx.stats.item_bytes = *wr.pos - i;

    i = *wr.pos;
    let items_buckets = create_index(items_index);
    encode_index(&mut ebml_w, items_buckets, write_int);
    ecx.stats.index_bytes = *wr.pos - i;
    ebml_w.end_tag();

    ecx.stats.total_bytes = *wr.pos;

    if (tcx.sess.meta_stats()) {
        for wr.bytes.iter().advance |e| {
            if *e == 0 {
                ecx.stats.zero_bytes += 1;
            }
        }

        io::println("metadata stats:");
        io::println(fmt!("    inline bytes: %u", ecx.stats.inline_bytes));
        io::println(fmt!(" attribute bytes: %u", ecx.stats.attr_bytes));
        io::println(fmt!("       dep bytes: %u", ecx.stats.dep_bytes));
        io::println(fmt!(" lang item bytes: %u", ecx.stats.lang_item_bytes));
        io::println(fmt!(" link args bytes: %u", ecx.stats.link_args_bytes));
        io::println(fmt!("      misc bytes: %u", ecx.stats.misc_bytes));
        io::println(fmt!("      item bytes: %u", ecx.stats.item_bytes));
        io::println(fmt!("     index bytes: %u", ecx.stats.index_bytes));
        io::println(fmt!("      zero bytes: %u", ecx.stats.zero_bytes));
        io::println(fmt!("     total bytes: %u", ecx.stats.total_bytes));
    }

    // Pad this, since something (LLVM, presumably) is cutting off the
    // remaining % 4 bytes.
    wr.write(&[0u8, 0u8, 0u8, 0u8]);

    let writer_bytes: &mut ~[u8] = wr.bytes;

    vec::to_owned(metadata_encoding_version) +
        flate::deflate_bytes(*writer_bytes)
}

// Get the encoded string for a type
pub fn encoded_ty(tcx: ty::ctxt, t: ty::t) -> ~str {
    let cx = @tyencode::ctxt {
        diag: tcx.diag,
        ds: def_to_str,
        tcx: tcx,
        abbrevs: tyencode::ac_no_abbrevs};
    do io::with_str_writer |wr| {
        tyencode::enc_ty(wr, cx, t);
    }
}
