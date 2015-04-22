// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// Decoding metadata from a single crate's metadata

#![allow(non_camel_case_types)]

pub use self::DefLike::*;
use self::Family::*;

use back::svh::Svh;
use metadata::cstore::crate_metadata;
use metadata::common::*;
use metadata::csearch::MethodInfo;
use metadata::csearch;
use metadata::cstore;
use metadata::tydecode::{parse_ty_data, parse_region_data, parse_def_id,
                         parse_type_param_def_data, parse_bare_fn_ty_data,
                         parse_trait_ref_data, parse_predicate_data};
use middle::def;
use middle::lang_items;
use middle::subst;
use middle::ty::{ImplContainer, TraitContainer};
use middle::ty::{self, Ty};
use middle::astencode::vtable_decoder_helpers;

use std::collections::HashMap;
use std::hash::{self, Hash, SipHasher};
use std::io::prelude::*;
use std::io;
use std::rc::Rc;
use std::slice::bytes;
use std::str;

use rbml::reader;
use rbml;
use serialize::Decodable;
use syntax::ast_map;
use syntax::attr;
use syntax::parse::token::{IdentInterner, special_idents};
use syntax::parse::token;
use syntax::print::pprust;
use syntax::ast;
use syntax::codemap;
use syntax::ptr::P;

pub type Cmd<'a> = &'a crate_metadata;

// A function that takes a def_id relative to the crate being searched and
// returns a def_id relative to the compilation environment, i.e. if we hit a
// def_id for an item defined in another crate, somebody needs to figure out
// what crate that's in and give us a def_id that makes sense for the current
// build.

fn u32_from_be_bytes(bytes: &[u8]) -> u32 {
    let mut b = [0; 4];
    bytes::copy_memory(&bytes[..4], &mut b);
    unsafe { (*(b.as_ptr() as *const u32)).to_be() }
}

fn lookup_hash<'a, F>(d: rbml::Doc<'a>, mut eq_fn: F, hash: u64) -> Option<rbml::Doc<'a>> where
    F: FnMut(&[u8]) -> bool,
{
    let index = reader::get_doc(d, tag_index);
    let table = reader::get_doc(index, tag_index_table);
    let hash_pos = table.start + (hash % 256 * 4) as usize;
    let pos = u32_from_be_bytes(&d.data[hash_pos..]) as usize;
    let tagged_doc = reader::doc_at(d.data, pos).unwrap();

    let belt = tag_index_buckets_bucket_elt;

    let mut ret = None;
    reader::tagged_docs(tagged_doc.doc, belt, |elt| {
        let pos = u32_from_be_bytes(&elt.data[elt.start..]) as usize;
        if eq_fn(&elt.data[elt.start + 4 .. elt.end]) {
            ret = Some(reader::doc_at(d.data, pos).unwrap().doc);
            false
        } else {
            true
        }
    });
    ret
}

pub fn maybe_find_item<'a>(item_id: ast::NodeId,
                           items: rbml::Doc<'a>) -> Option<rbml::Doc<'a>> {
    fn eq_item(bytes: &[u8], item_id: ast::NodeId) -> bool {
        u32_from_be_bytes(bytes) == item_id
    }
    lookup_hash(items,
                |a| eq_item(a, item_id),
                hash::hash::<i64, SipHasher>(&(item_id as i64)))
}

fn find_item<'a>(item_id: ast::NodeId, items: rbml::Doc<'a>) -> rbml::Doc<'a> {
    match maybe_find_item(item_id, items) {
       None => panic!("lookup_item: id not found: {}", item_id),
       Some(d) => d
    }
}

// Looks up an item in the given metadata and returns an rbml doc pointing
// to the item data.
fn lookup_item<'a>(item_id: ast::NodeId, data: &'a [u8]) -> rbml::Doc<'a> {
    let items = reader::get_doc(rbml::Doc::new(data), tag_items);
    find_item(item_id, items)
}

#[derive(PartialEq)]
enum Family {
    ImmStatic,             // c
    MutStatic,             // b
    Fn,                    // f
    CtorFn,                // o
    StaticMethod,          // F
    Method,                // h
    Type,                  // y
    Mod,                   // m
    ForeignMod,            // n
    Enum,                  // t
    TupleVariant,          // v
    StructVariant,         // V
    Impl,                  // i
    DefaultImpl,              // d
    Trait,                 // I
    Struct,                // S
    PublicField,           // g
    InheritedField,        // N
    Constant,              // C
}

fn item_family(item: rbml::Doc) -> Family {
    let fam = reader::get_doc(item, tag_items_data_item_family);
    match reader::doc_as_u8(fam) as char {
      'C' => Constant,
      'c' => ImmStatic,
      'b' => MutStatic,
      'f' => Fn,
      'o' => CtorFn,
      'F' => StaticMethod,
      'h' => Method,
      'y' => Type,
      'm' => Mod,
      'n' => ForeignMod,
      't' => Enum,
      'v' => TupleVariant,
      'V' => StructVariant,
      'i' => Impl,
      'd' => DefaultImpl,
      'I' => Trait,
      'S' => Struct,
      'g' => PublicField,
      'N' => InheritedField,
       c => panic!("unexpected family char: {}", c)
    }
}

fn item_visibility(item: rbml::Doc) -> ast::Visibility {
    match reader::maybe_get_doc(item, tag_items_data_item_visibility) {
        None => ast::Public,
        Some(visibility_doc) => {
            match reader::doc_as_u8(visibility_doc) as char {
                'y' => ast::Public,
                'i' => ast::Inherited,
                _ => panic!("unknown visibility character")
            }
        }
    }
}

fn item_sort(item: rbml::Doc) -> Option<char> {
    let mut ret = None;
    reader::tagged_docs(item, tag_item_trait_item_sort, |doc| {
        ret = Some(doc.as_str_slice().as_bytes()[0] as char);
        false
    });
    ret
}

fn item_symbol(item: rbml::Doc) -> String {
    reader::get_doc(item, tag_items_data_item_symbol).as_str().to_string()
}

fn item_parent_item(d: rbml::Doc) -> Option<ast::DefId> {
    let mut ret = None;
    reader::tagged_docs(d, tag_items_data_parent_item, |did| {
        ret = Some(reader::with_doc_data(did, parse_def_id));
        false
    });
    ret
}

fn item_reqd_and_translated_parent_item(cnum: ast::CrateNum,
                                        d: rbml::Doc) -> ast::DefId {
    let trait_did = item_parent_item(d).expect("item without parent");
    ast::DefId { krate: cnum, node: trait_did.node }
}

fn item_def_id(d: rbml::Doc, cdata: Cmd) -> ast::DefId {
    let tagdoc = reader::get_doc(d, tag_def_id);
    return translate_def_id(cdata, reader::with_doc_data(tagdoc, parse_def_id));
}

fn get_provided_source(d: rbml::Doc, cdata: Cmd) -> Option<ast::DefId> {
    reader::maybe_get_doc(d, tag_item_method_provided_source).map(|doc| {
        translate_def_id(cdata, reader::with_doc_data(doc, parse_def_id))
    })
}

fn each_reexport<F>(d: rbml::Doc, f: F) -> bool where
    F: FnMut(rbml::Doc) -> bool,
{
    reader::tagged_docs(d, tag_items_data_item_reexport, f)
}

fn variant_disr_val(d: rbml::Doc) -> Option<ty::Disr> {
    reader::maybe_get_doc(d, tag_disr_val).and_then(|val_doc| {
        reader::with_doc_data(val_doc, |data| {
            str::from_utf8(data).ok().and_then(|s| s.parse().ok())
        })
    })
}

fn doc_type<'tcx>(doc: rbml::Doc, tcx: &ty::ctxt<'tcx>, cdata: Cmd) -> Ty<'tcx> {
    let tp = reader::get_doc(doc, tag_items_data_item_type);
    parse_ty_data(tp.data, cdata.cnum, tp.start, tcx,
                  |_, did| translate_def_id(cdata, did))
}

fn doc_method_fty<'tcx>(doc: rbml::Doc, tcx: &ty::ctxt<'tcx>,
                        cdata: Cmd) -> ty::BareFnTy<'tcx> {
    let tp = reader::get_doc(doc, tag_item_method_fty);
    parse_bare_fn_ty_data(tp.data, cdata.cnum, tp.start, tcx,
                          |_, did| translate_def_id(cdata, did))
}

pub fn item_type<'tcx>(_item_id: ast::DefId, item: rbml::Doc,
                       tcx: &ty::ctxt<'tcx>, cdata: Cmd) -> Ty<'tcx> {
    doc_type(item, tcx, cdata)
}

fn doc_trait_ref<'tcx>(doc: rbml::Doc, tcx: &ty::ctxt<'tcx>, cdata: Cmd)
                       -> Rc<ty::TraitRef<'tcx>> {
    parse_trait_ref_data(doc.data, cdata.cnum, doc.start, tcx,
                         |_, did| translate_def_id(cdata, did))
}

fn item_trait_ref<'tcx>(doc: rbml::Doc, tcx: &ty::ctxt<'tcx>, cdata: Cmd)
                        -> Rc<ty::TraitRef<'tcx>> {
    let tp = reader::get_doc(doc, tag_item_trait_ref);
    doc_trait_ref(tp, tcx, cdata)
}

fn enum_variant_ids(item: rbml::Doc, cdata: Cmd) -> Vec<ast::DefId> {
    let mut ids: Vec<ast::DefId> = Vec::new();
    let v = tag_items_data_item_variant;
    reader::tagged_docs(item, v, |p| {
        let ext = reader::with_doc_data(p, parse_def_id);
        ids.push(ast::DefId { krate: cdata.cnum, node: ext.node });
        true
    });
    return ids;
}

fn item_path(item_doc: rbml::Doc) -> Vec<ast_map::PathElem> {
    let path_doc = reader::get_doc(item_doc, tag_path);

    let len_doc = reader::get_doc(path_doc, tag_path_len);
    let len = reader::doc_as_u32(len_doc) as usize;

    let mut result = Vec::with_capacity(len);
    reader::docs(path_doc, |tag, elt_doc| {
        if tag == tag_path_elem_mod {
            let s = elt_doc.as_str_slice();
            result.push(ast_map::PathMod(token::intern(s)));
        } else if tag == tag_path_elem_name {
            let s = elt_doc.as_str_slice();
            result.push(ast_map::PathName(token::intern(s)));
        } else {
            // ignore tag_path_len element
        }
        true
    });

    result
}

fn item_name(intr: &IdentInterner, item: rbml::Doc) -> ast::Name {
    let name = reader::get_doc(item, tag_paths_data_name);
    let string = name.as_str_slice();
    match intr.find(string) {
        None => token::intern(string),
        Some(val) => val,
    }
}

fn item_to_def_like(item: rbml::Doc, did: ast::DefId, cnum: ast::CrateNum)
    -> DefLike {
    let fam = item_family(item);
    match fam {
        Constant  => DlDef(def::DefConst(did)),
        ImmStatic => DlDef(def::DefStatic(did, false)),
        MutStatic => DlDef(def::DefStatic(did, true)),
        Struct    => DlDef(def::DefStruct(did)),
        Fn        => DlDef(def::DefFn(did, false)),
        CtorFn    => DlDef(def::DefFn(did, true)),
        Method | StaticMethod => {
            // def_static_method carries an optional field of its enclosing
            // trait or enclosing impl (if this is an inherent static method).
            // So we need to detect whether this is in a trait or not, which
            // we do through the mildly hacky way of checking whether there is
            // a trait_parent_sort.
            let provenance = if reader::maybe_get_doc(
                  item, tag_item_trait_parent_sort).is_some() {
                def::FromTrait(item_reqd_and_translated_parent_item(cnum,
                                                                    item))
            } else {
                def::FromImpl(item_reqd_and_translated_parent_item(cnum,
                                                                   item))
            };
            DlDef(def::DefMethod(did, provenance))
        }
        Type => {
            if item_sort(item) == Some('t') {
                let trait_did = item_reqd_and_translated_parent_item(cnum, item);
                DlDef(def::DefAssociatedTy(trait_did, did))
            } else {
                DlDef(def::DefTy(did, false))
            }
        }
        Mod => DlDef(def::DefMod(did)),
        ForeignMod => DlDef(def::DefForeignMod(did)),
        StructVariant => {
            let enum_did = item_reqd_and_translated_parent_item(cnum, item);
            DlDef(def::DefVariant(enum_did, did, true))
        }
        TupleVariant => {
            let enum_did = item_reqd_and_translated_parent_item(cnum, item);
            DlDef(def::DefVariant(enum_did, did, false))
        }
        Trait => DlDef(def::DefTrait(did)),
        Enum => DlDef(def::DefTy(did, true)),
        Impl | DefaultImpl => DlImpl(did),
        PublicField | InheritedField => DlField,
    }
}

fn parse_unsafety(item_doc: rbml::Doc) -> ast::Unsafety {
    let unsafety_doc = reader::get_doc(item_doc, tag_unsafety);
    if reader::doc_as_u8(unsafety_doc) != 0 {
        ast::Unsafety::Unsafe
    } else {
        ast::Unsafety::Normal
    }
}

fn parse_paren_sugar(item_doc: rbml::Doc) -> bool {
    let paren_sugar_doc = reader::get_doc(item_doc, tag_paren_sugar);
    reader::doc_as_u8(paren_sugar_doc) != 0
}

fn parse_polarity(item_doc: rbml::Doc) -> ast::ImplPolarity {
    let polarity_doc = reader::get_doc(item_doc, tag_polarity);
    if reader::doc_as_u8(polarity_doc) != 0 {
        ast::ImplPolarity::Negative
    } else {
        ast::ImplPolarity::Positive
    }
}

fn parse_associated_type_names(item_doc: rbml::Doc) -> Vec<ast::Name> {
    let names_doc = reader::get_doc(item_doc, tag_associated_type_names);
    let mut names = Vec::new();
    reader::tagged_docs(names_doc, tag_associated_type_name, |name_doc| {
        let name = token::intern(name_doc.as_str_slice());
        names.push(name);
        true
    });
    names
}

pub fn get_trait_def<'tcx>(cdata: Cmd,
                           item_id: ast::NodeId,
                           tcx: &ty::ctxt<'tcx>) -> ty::TraitDef<'tcx>
{
    let item_doc = lookup_item(item_id, cdata.data());
    let generics = doc_generics(item_doc, tcx, cdata, tag_item_generics);
    let unsafety = parse_unsafety(item_doc);
    let associated_type_names = parse_associated_type_names(item_doc);
    let paren_sugar = parse_paren_sugar(item_doc);

    ty::TraitDef {
        paren_sugar: paren_sugar,
        unsafety: unsafety,
        generics: generics,
        trait_ref: item_trait_ref(item_doc, tcx, cdata),
        associated_type_names: associated_type_names,
    }
}

pub fn get_predicates<'tcx>(cdata: Cmd,
                            item_id: ast::NodeId,
                            tcx: &ty::ctxt<'tcx>)
                            -> ty::GenericPredicates<'tcx>
{
    let item_doc = lookup_item(item_id, cdata.data());
    doc_predicates(item_doc, tcx, cdata, tag_item_generics)
}

pub fn get_super_predicates<'tcx>(cdata: Cmd,
                                  item_id: ast::NodeId,
                                  tcx: &ty::ctxt<'tcx>)
                                  -> ty::GenericPredicates<'tcx>
{
    let item_doc = lookup_item(item_id, cdata.data());
    doc_predicates(item_doc, tcx, cdata, tag_item_super_predicates)
}

pub fn get_type<'tcx>(cdata: Cmd, id: ast::NodeId, tcx: &ty::ctxt<'tcx>)
                      -> ty::TypeScheme<'tcx>
{
    let item_doc = lookup_item(id, cdata.data());
    let t = item_type(ast::DefId { krate: cdata.cnum, node: id }, item_doc, tcx,
                      cdata);
    let generics = doc_generics(item_doc, tcx, cdata, tag_item_generics);
    ty::TypeScheme {
        generics: generics,
        ty: t
    }
}

pub fn get_stability(cdata: Cmd, id: ast::NodeId) -> Option<attr::Stability> {
    let item = lookup_item(id, cdata.data());
    reader::maybe_get_doc(item, tag_items_data_item_stability).map(|doc| {
        let mut decoder = reader::Decoder::new(doc);
        Decodable::decode(&mut decoder).unwrap()
    })
}

pub fn get_repr_attrs(cdata: Cmd, id: ast::NodeId) -> Vec<attr::ReprAttr> {
    let item = lookup_item(id, cdata.data());
    match reader::maybe_get_doc(item, tag_items_data_item_repr).map(|doc| {
        let mut decoder = reader::Decoder::new(doc);
        Decodable::decode(&mut decoder).unwrap()
    }) {
        Some(attrs) => attrs,
        None => Vec::new(),
    }
}

pub fn get_impl_polarity<'tcx>(cdata: Cmd,
                               id: ast::NodeId)
                               -> Option<ast::ImplPolarity>
{
    let item_doc = lookup_item(id, cdata.data());
    let fam = item_family(item_doc);
    match fam {
        Family::Impl => {
            Some(parse_polarity(item_doc))
        }
        _ => None
    }
}

pub fn get_impl_trait<'tcx>(cdata: Cmd,
                            id: ast::NodeId,
                            tcx: &ty::ctxt<'tcx>)
                            -> Option<Rc<ty::TraitRef<'tcx>>>
{
    let item_doc = lookup_item(id, cdata.data());
    let fam = item_family(item_doc);
    match fam {
        Family::Impl | Family::DefaultImpl => {
            reader::maybe_get_doc(item_doc, tag_item_trait_ref).map(|tp| {
                doc_trait_ref(tp, tcx, cdata)
            })
        }
        _ => None
    }
}

pub fn get_impl_vtables<'tcx>(cdata: Cmd,
                              id: ast::NodeId,
                              tcx: &ty::ctxt<'tcx>)
                              -> ty::vtable_res<'tcx>
{
    let item_doc = lookup_item(id, cdata.data());
    let vtables_doc = reader::get_doc(item_doc, tag_item_impl_vtables);
    let mut decoder = reader::Decoder::new(vtables_doc);
    decoder.read_vtable_res(tcx, cdata)
}


pub fn get_symbol(data: &[u8], id: ast::NodeId) -> String {
    return item_symbol(lookup_item(id, data));
}

// Something that a name can resolve to.
#[derive(Copy, Clone, Debug)]
pub enum DefLike {
    DlDef(def::Def),
    DlImpl(ast::DefId),
    DlField
}

/// Iterates over the language items in the given crate.
pub fn each_lang_item<F>(cdata: Cmd, mut f: F) -> bool where
    F: FnMut(ast::NodeId, usize) -> bool,
{
    let root = rbml::Doc::new(cdata.data());
    let lang_items = reader::get_doc(root, tag_lang_items);
    reader::tagged_docs(lang_items, tag_lang_items_item, |item_doc| {
        let id_doc = reader::get_doc(item_doc, tag_lang_items_item_id);
        let id = reader::doc_as_u32(id_doc) as usize;
        let node_id_doc = reader::get_doc(item_doc,
                                          tag_lang_items_item_node_id);
        let node_id = reader::doc_as_u32(node_id_doc) as ast::NodeId;

        f(node_id, id)
    })
}

fn each_child_of_item_or_crate<F, G>(intr: Rc<IdentInterner>,
                                     cdata: Cmd,
                                     item_doc: rbml::Doc,
                                     mut get_crate_data: G,
                                     mut callback: F) where
    F: FnMut(DefLike, ast::Name, ast::Visibility),
    G: FnMut(ast::CrateNum) -> Rc<crate_metadata>,
{
    // Iterate over all children.
    let _ = reader::tagged_docs(item_doc, tag_mod_child, |child_info_doc| {
        let child_def_id = reader::with_doc_data(child_info_doc,
                                                 parse_def_id);
        let child_def_id = translate_def_id(cdata, child_def_id);

        // This item may be in yet another crate if it was the child of a
        // reexport.
        let crate_data = if child_def_id.krate == cdata.cnum {
            None
        } else {
            Some(get_crate_data(child_def_id.krate))
        };
        let crate_data = match crate_data {
            Some(ref cdata) => &**cdata,
            None => cdata
        };

        let other_crates_items = reader::get_doc(rbml::Doc::new(crate_data.data()), tag_items);

        // Get the item.
        match maybe_find_item(child_def_id.node, other_crates_items) {
            None => {}
            Some(child_item_doc) => {
                // Hand off the item to the callback.
                let child_name = item_name(&*intr, child_item_doc);
                let def_like = item_to_def_like(child_item_doc,
                                                child_def_id,
                                                cdata.cnum);
                let visibility = item_visibility(child_item_doc);
                callback(def_like, child_name, visibility);

            }
        }

        true
    });

    // As a special case, iterate over all static methods of
    // associated implementations too. This is a bit of a botch.
    // --pcwalton
    let _ = reader::tagged_docs(item_doc,
                                tag_items_data_item_inherent_impl,
                                |inherent_impl_def_id_doc| {
        let inherent_impl_def_id = item_def_id(inherent_impl_def_id_doc,
                                               cdata);
        let items = reader::get_doc(rbml::Doc::new(cdata.data()), tag_items);
        if let Some(inherent_impl_doc) = maybe_find_item(inherent_impl_def_id.node, items) {
            let _ = reader::tagged_docs(inherent_impl_doc,
                                        tag_item_impl_item,
                                        |impl_item_def_id_doc| {
                let impl_item_def_id = item_def_id(impl_item_def_id_doc,
                                                   cdata);
                if let Some(impl_method_doc) = maybe_find_item(impl_item_def_id.node, items) {
                    if let StaticMethod = item_family(impl_method_doc) {
                        // Hand off the static method to the callback.
                        let static_method_name = item_name(&*intr, impl_method_doc);
                        let static_method_def_like = item_to_def_like(impl_method_doc,
                                                                      impl_item_def_id,
                                                                      cdata.cnum);
                        callback(static_method_def_like,
                                 static_method_name,
                                 item_visibility(impl_method_doc));
                    }
                }
                true
            });
        }
        true
    });

    // Iterate over all reexports.
    let _ = each_reexport(item_doc, |reexport_doc| {
        let def_id_doc = reader::get_doc(reexport_doc,
                                         tag_items_data_item_reexport_def_id);
        let child_def_id = reader::with_doc_data(def_id_doc,
                                                 parse_def_id);
        let child_def_id = translate_def_id(cdata, child_def_id);

        let name_doc = reader::get_doc(reexport_doc,
                                       tag_items_data_item_reexport_name);
        let name = name_doc.as_str_slice();

        // This reexport may be in yet another crate.
        let crate_data = if child_def_id.krate == cdata.cnum {
            None
        } else {
            Some(get_crate_data(child_def_id.krate))
        };
        let crate_data = match crate_data {
            Some(ref cdata) => &**cdata,
            None => cdata
        };

        let other_crates_items = reader::get_doc(rbml::Doc::new(crate_data.data()), tag_items);

        // Get the item.
        if let Some(child_item_doc) = maybe_find_item(child_def_id.node, other_crates_items) {
            // Hand off the item to the callback.
            let def_like = item_to_def_like(child_item_doc,
                                            child_def_id,
                                            child_def_id.krate);
            // These items have a public visibility because they're part of
            // a public re-export.
            callback(def_like, token::intern(name), ast::Public);
        }

        true
    });
}

/// Iterates over each child of the given item.
pub fn each_child_of_item<F, G>(intr: Rc<IdentInterner>,
                               cdata: Cmd,
                               id: ast::NodeId,
                               get_crate_data: G,
                               callback: F) where
    F: FnMut(DefLike, ast::Name, ast::Visibility),
    G: FnMut(ast::CrateNum) -> Rc<crate_metadata>,
{
    // Find the item.
    let root_doc = rbml::Doc::new(cdata.data());
    let items = reader::get_doc(root_doc, tag_items);
    let item_doc = match maybe_find_item(id, items) {
        None => return,
        Some(item_doc) => item_doc,
    };

    each_child_of_item_or_crate(intr,
                                cdata,
                                item_doc,
                                get_crate_data,
                                callback)
}

/// Iterates over all the top-level crate items.
pub fn each_top_level_item_of_crate<F, G>(intr: Rc<IdentInterner>,
                                          cdata: Cmd,
                                          get_crate_data: G,
                                          callback: F) where
    F: FnMut(DefLike, ast::Name, ast::Visibility),
    G: FnMut(ast::CrateNum) -> Rc<crate_metadata>,
{
    let root_doc = rbml::Doc::new(cdata.data());
    let misc_info_doc = reader::get_doc(root_doc, tag_misc_info);
    let crate_items_doc = reader::get_doc(misc_info_doc,
                                          tag_misc_info_crate_items);

    each_child_of_item_or_crate(intr,
                                cdata,
                                crate_items_doc,
                                get_crate_data,
                                callback)
}

pub fn get_item_path(cdata: Cmd, id: ast::NodeId) -> Vec<ast_map::PathElem> {
    item_path(lookup_item(id, cdata.data()))
}

pub type DecodeInlinedItem<'a> =
    Box<for<'tcx> FnMut(Cmd,
                        &ty::ctxt<'tcx>,
                        Vec<ast_map::PathElem>,
                        rbml::Doc)
                        -> Result<&'tcx ast::InlinedItem, Vec<ast_map::PathElem>> + 'a>;

pub fn maybe_get_item_ast<'tcx>(cdata: Cmd, tcx: &ty::ctxt<'tcx>, id: ast::NodeId,
                                mut decode_inlined_item: DecodeInlinedItem)
                                -> csearch::FoundAst<'tcx> {
    debug!("Looking up item: {}", id);
    let item_doc = lookup_item(id, cdata.data());
    let path = item_path(item_doc).init().to_vec();
    match decode_inlined_item(cdata, tcx, path, item_doc) {
        Ok(ii) => csearch::FoundAst::Found(ii),
        Err(path) => {
            match item_parent_item(item_doc) {
                Some(did) => {
                    let did = translate_def_id(cdata, did);
                    let parent_item = lookup_item(did.node, cdata.data());
                    match decode_inlined_item(cdata, tcx, path, parent_item) {
                        Ok(ii) => csearch::FoundAst::FoundParent(did, ii),
                        Err(_) => csearch::FoundAst::NotFound
                    }
                }
                None => csearch::FoundAst::NotFound
            }
        }
    }
}

pub fn get_enum_variant_defs(intr: &IdentInterner,
                             cdata: Cmd,
                             id: ast::NodeId)
                             -> Vec<(def::Def, ast::Name, ast::Visibility)> {
    let data = cdata.data();
    let items = reader::get_doc(rbml::Doc::new(data), tag_items);
    let item = find_item(id, items);
    enum_variant_ids(item, cdata).iter().map(|did| {
        let item = find_item(did.node, items);
        let name = item_name(intr, item);
        let visibility = item_visibility(item);
        match item_to_def_like(item, *did, cdata.cnum) {
            DlDef(def @ def::DefVariant(..)) => (def, name, visibility),
            _ => unreachable!()
        }
    }).collect()
}

pub fn get_enum_variants<'tcx>(intr: Rc<IdentInterner>, cdata: Cmd, id: ast::NodeId,
                               tcx: &ty::ctxt<'tcx>) -> Vec<Rc<ty::VariantInfo<'tcx>>> {
    let data = cdata.data();
    let items = reader::get_doc(rbml::Doc::new(data), tag_items);
    let item = find_item(id, items);
    let mut disr_val = 0;
    enum_variant_ids(item, cdata).iter().map(|did| {
        let item = find_item(did.node, items);
        let ctor_ty = item_type(ast::DefId { krate: cdata.cnum, node: id},
                                item, tcx, cdata);
        let name = item_name(&*intr, item);
        let (ctor_ty, arg_tys, arg_names) = match ctor_ty.sty {
            ty::ty_bare_fn(_, ref f) =>
                (Some(ctor_ty), f.sig.0.inputs.clone(), None),
            _ => { // Nullary or struct enum variant.
                let mut arg_names = Vec::new();
                let arg_tys = get_struct_fields(intr.clone(), cdata, did.node)
                    .iter()
                    .map(|field_ty| {
                        arg_names.push(field_ty.name);
                        get_type(cdata, field_ty.id.node, tcx).ty
                    })
                    .collect();
                let arg_names = if arg_names.is_empty() { None } else { Some(arg_names) };

                (None, arg_tys, arg_names)
            }
        };
        match variant_disr_val(item) {
            Some(val) => { disr_val = val; }
            _         => { /* empty */ }
        }
        let old_disr_val = disr_val;
        disr_val = disr_val.wrapping_add(1);
        Rc::new(ty::VariantInfo {
            args: arg_tys,
            arg_names: arg_names,
            ctor_ty: ctor_ty,
            name: name,
            // I'm not even sure if we encode visibility
            // for variants -- TEST -- tjc
            id: *did,
            disr_val: old_disr_val,
            vis: ast::Inherited
        })
    }).collect()
}

fn get_explicit_self(item: rbml::Doc) -> ty::ExplicitSelfCategory {
    fn get_mutability(ch: u8) -> ast::Mutability {
        match ch as char {
            'i' => ast::MutImmutable,
            'm' => ast::MutMutable,
            _ => panic!("unknown mutability character: `{}`", ch as char),
        }
    }

    let explicit_self_doc = reader::get_doc(item, tag_item_trait_method_explicit_self);
    let string = explicit_self_doc.as_str_slice();

    let explicit_self_kind = string.as_bytes()[0];
    match explicit_self_kind as char {
        's' => ty::StaticExplicitSelfCategory,
        'v' => ty::ByValueExplicitSelfCategory,
        '~' => ty::ByBoxExplicitSelfCategory,
        // FIXME(#4846) expl. region
        '&' => {
            ty::ByReferenceExplicitSelfCategory(
                ty::ReEmpty,
                get_mutability(string.as_bytes()[1]))
        }
        _ => panic!("unknown self type code: `{}`", explicit_self_kind as char)
    }
}

/// Returns the def IDs of all the items in the given implementation.
pub fn get_impl_items(cdata: Cmd, impl_id: ast::NodeId)
                      -> Vec<ty::ImplOrTraitItemId> {
    let mut impl_items = Vec::new();
    reader::tagged_docs(lookup_item(impl_id, cdata.data()),
                        tag_item_impl_item, |doc| {
        let def_id = item_def_id(doc, cdata);
        match item_sort(doc) {
            Some('r') | Some('p') => {
                impl_items.push(ty::MethodTraitItemId(def_id))
            }
            Some('t') => impl_items.push(ty::TypeTraitItemId(def_id)),
            _ => panic!("unknown impl item sort"),
        }
        true
    });

    impl_items
}

pub fn get_trait_name(intr: Rc<IdentInterner>,
                      cdata: Cmd,
                      id: ast::NodeId)
                      -> ast::Name {
    let doc = lookup_item(id, cdata.data());
    item_name(&*intr, doc)
}

pub fn is_static_method(cdata: Cmd, id: ast::NodeId) -> bool {
    let doc = lookup_item(id, cdata.data());
    match item_sort(doc) {
        Some('r') | Some('p') => {
            get_explicit_self(doc) == ty::StaticExplicitSelfCategory
        }
        _ => false
    }
}

pub fn get_impl_or_trait_item<'tcx>(intr: Rc<IdentInterner>,
                                    cdata: Cmd,
                                    id: ast::NodeId,
                                    tcx: &ty::ctxt<'tcx>)
                                    -> ty::ImplOrTraitItem<'tcx> {
    let method_doc = lookup_item(id, cdata.data());

    let def_id = item_def_id(method_doc, cdata);

    let container_id = item_reqd_and_translated_parent_item(cdata.cnum,
                                                            method_doc);
    let container_doc = lookup_item(container_id.node, cdata.data());
    let container = match item_family(container_doc) {
        Trait => TraitContainer(container_id),
        _ => ImplContainer(container_id),
    };

    let name = item_name(&*intr, method_doc);
    let vis = item_visibility(method_doc);

    match item_sort(method_doc) {
        Some('r') | Some('p') => {
            let generics = doc_generics(method_doc, tcx, cdata, tag_method_ty_generics);
            let predicates = doc_predicates(method_doc, tcx, cdata, tag_method_ty_generics);
            let fty = doc_method_fty(method_doc, tcx, cdata);
            let explicit_self = get_explicit_self(method_doc);
            let provided_source = get_provided_source(method_doc, cdata);

            ty::MethodTraitItem(Rc::new(ty::Method::new(name,
                                                        generics,
                                                        predicates,
                                                        fty,
                                                        explicit_self,
                                                        vis,
                                                        def_id,
                                                        container,
                                                        provided_source)))
        }
        Some('t') => {
            ty::TypeTraitItem(Rc::new(ty::AssociatedType {
                name: name,
                vis: vis,
                def_id: def_id,
                container: container,
            }))
        }
        _ => panic!("unknown impl/trait item sort"),
    }
}

pub fn get_trait_item_def_ids(cdata: Cmd, id: ast::NodeId)
                              -> Vec<ty::ImplOrTraitItemId> {
    let data = cdata.data();
    let item = lookup_item(id, data);
    let mut result = Vec::new();
    reader::tagged_docs(item, tag_item_trait_item, |mth| {
        let def_id = item_def_id(mth, cdata);
        match item_sort(mth) {
            Some('r') | Some('p') => {
                result.push(ty::MethodTraitItemId(def_id));
            }
            Some('t') => result.push(ty::TypeTraitItemId(def_id)),
            _ => panic!("unknown trait item sort"),
        }
        true
    });
    result
}

pub fn get_item_variances(cdata: Cmd, id: ast::NodeId) -> ty::ItemVariances {
    let data = cdata.data();
    let item_doc = lookup_item(id, data);
    let variance_doc = reader::get_doc(item_doc, tag_item_variances);
    let mut decoder = reader::Decoder::new(variance_doc);
    Decodable::decode(&mut decoder).unwrap()
}

pub fn get_provided_trait_methods<'tcx>(intr: Rc<IdentInterner>,
                                        cdata: Cmd,
                                        id: ast::NodeId,
                                        tcx: &ty::ctxt<'tcx>)
                                        -> Vec<Rc<ty::Method<'tcx>>> {
    let data = cdata.data();
    let item = lookup_item(id, data);
    let mut result = Vec::new();

    reader::tagged_docs(item, tag_item_trait_item, |mth_id| {
        let did = item_def_id(mth_id, cdata);
        let mth = lookup_item(did.node, data);

        if item_sort(mth) == Some('p') {
            let trait_item = get_impl_or_trait_item(intr.clone(),
                                                    cdata,
                                                    did.node,
                                                    tcx);
            match trait_item {
                ty::MethodTraitItem(ref method) => {
                    result.push((*method).clone())
                }
                ty::TypeTraitItem(_) => {}
            }
        }
        true
    });

    return result;
}

pub fn get_type_name_if_impl(cdata: Cmd,
                             node_id: ast::NodeId) -> Option<ast::Name> {
    let item = lookup_item(node_id, cdata.data());
    if item_family(item) != Impl {
        return None;
    }

    let mut ret = None;
    reader::tagged_docs(item, tag_item_impl_type_basename, |doc| {
        ret = Some(token::intern(doc.as_str_slice()));
        false
    });

    ret
}

pub fn get_methods_if_impl(intr: Rc<IdentInterner>,
                                  cdata: Cmd,
                                  node_id: ast::NodeId)
                               -> Option<Vec<MethodInfo> > {
    let item = lookup_item(node_id, cdata.data());
    if item_family(item) != Impl {
        return None;
    }

    // If this impl implements a trait, don't consider it.
    let ret = reader::tagged_docs(item, tag_item_trait_ref, |_doc| {
        false
    });

    if !ret { return None }

    let mut impl_method_ids = Vec::new();
    reader::tagged_docs(item, tag_item_impl_item, |impl_method_doc| {
        impl_method_ids.push(item_def_id(impl_method_doc, cdata));
        true
    });

    let mut impl_methods = Vec::new();
    for impl_method_id in &impl_method_ids {
        let impl_method_doc = lookup_item(impl_method_id.node, cdata.data());
        let family = item_family(impl_method_doc);
        match family {
            StaticMethod | Method => {
                impl_methods.push(MethodInfo {
                    name: item_name(&*intr, impl_method_doc),
                    def_id: item_def_id(impl_method_doc, cdata),
                    vis: item_visibility(impl_method_doc),
                });
            }
            _ => {}
        }
    }

    return Some(impl_methods);
}

/// If node_id is the constructor of a tuple struct, retrieve the NodeId of
/// the actual type definition, otherwise, return None
pub fn get_tuple_struct_definition_if_ctor(cdata: Cmd,
                                           node_id: ast::NodeId)
    -> Option<ast::DefId>
{
    let item = lookup_item(node_id, cdata.data());
    let mut ret = None;
    reader::tagged_docs(item, tag_items_data_item_is_tuple_struct_ctor, |_| {
        ret = Some(item_reqd_and_translated_parent_item(cdata.cnum, item));
        false
    });
    ret
}

pub fn get_item_attrs(cdata: Cmd,
                      orig_node_id: ast::NodeId)
                      -> Vec<ast::Attribute> {
    // The attributes for a tuple struct are attached to the definition, not the ctor;
    // we assume that someone passing in a tuple struct ctor is actually wanting to
    // look at the definition
    let node_id = get_tuple_struct_definition_if_ctor(cdata, orig_node_id);
    let node_id = node_id.map(|x| x.node).unwrap_or(orig_node_id);
    let item = lookup_item(node_id, cdata.data());
    get_attributes(item)
}

pub fn get_struct_field_attrs(cdata: Cmd) -> HashMap<ast::NodeId, Vec<ast::Attribute>> {
    let data = rbml::Doc::new(cdata.data());
    let fields = reader::get_doc(data, tag_struct_fields);
    let mut map = HashMap::new();
    reader::tagged_docs(fields, tag_struct_field, |field| {
        let id = reader::doc_as_u32(reader::get_doc(field, tag_struct_field_id));
        let attrs = get_attributes(field);
        map.insert(id, attrs);
        true
    });
    map
}

fn struct_field_family_to_visibility(family: Family) -> ast::Visibility {
    match family {
      PublicField => ast::Public,
      InheritedField => ast::Inherited,
      _ => panic!()
    }
}

pub fn get_struct_fields(intr: Rc<IdentInterner>, cdata: Cmd, id: ast::NodeId)
    -> Vec<ty::field_ty> {
    let data = cdata.data();
    let item = lookup_item(id, data);
    let mut result = Vec::new();
    reader::tagged_docs(item, tag_item_field, |an_item| {
        let f = item_family(an_item);
        if f == PublicField || f == InheritedField {
            let name = item_name(&*intr, an_item);
            let did = item_def_id(an_item, cdata);
            let tagdoc = reader::get_doc(an_item, tag_item_field_origin);
            let origin_id =  translate_def_id(cdata, reader::with_doc_data(tagdoc, parse_def_id));
            result.push(ty::field_ty {
                name: name,
                id: did,
                vis: struct_field_family_to_visibility(f),
                origin: origin_id,
            });
        }
        true
    });
    reader::tagged_docs(item, tag_item_unnamed_field, |an_item| {
        let did = item_def_id(an_item, cdata);
        let tagdoc = reader::get_doc(an_item, tag_item_field_origin);
        let f = item_family(an_item);
        let origin_id =  translate_def_id(cdata, reader::with_doc_data(tagdoc, parse_def_id));
        result.push(ty::field_ty {
            name: special_idents::unnamed_field.name,
            id: did,
            vis: struct_field_family_to_visibility(f),
            origin: origin_id,
        });
        true
    });
    result
}

fn get_meta_items(md: rbml::Doc) -> Vec<P<ast::MetaItem>> {
    let mut items: Vec<P<ast::MetaItem>> = Vec::new();
    reader::tagged_docs(md, tag_meta_item_word, |meta_item_doc| {
        let nd = reader::get_doc(meta_item_doc, tag_meta_item_name);
        let n = token::intern_and_get_ident(nd.as_str_slice());
        items.push(attr::mk_word_item(n));
        true
    });
    reader::tagged_docs(md, tag_meta_item_name_value, |meta_item_doc| {
        let nd = reader::get_doc(meta_item_doc, tag_meta_item_name);
        let vd = reader::get_doc(meta_item_doc, tag_meta_item_value);
        let n = token::intern_and_get_ident(nd.as_str_slice());
        let v = token::intern_and_get_ident(vd.as_str_slice());
        // FIXME (#623): Should be able to decode MetaNameValue variants,
        // but currently the encoder just drops them
        items.push(attr::mk_name_value_item_str(n, v));
        true
    });
    reader::tagged_docs(md, tag_meta_item_list, |meta_item_doc| {
        let nd = reader::get_doc(meta_item_doc, tag_meta_item_name);
        let n = token::intern_and_get_ident(nd.as_str_slice());
        let subitems = get_meta_items(meta_item_doc);
        items.push(attr::mk_list_item(n, subitems.into_iter().collect()));
        true
    });
    return items;
}

fn get_attributes(md: rbml::Doc) -> Vec<ast::Attribute> {
    let mut attrs: Vec<ast::Attribute> = Vec::new();
    match reader::maybe_get_doc(md, tag_attributes) {
      Some(attrs_d) => {
        reader::tagged_docs(attrs_d, tag_attribute, |attr_doc| {
            let is_sugared_doc = reader::doc_as_u8(
                reader::get_doc(attr_doc, tag_attribute_is_sugared_doc)
            ) == 1;
            let meta_items = get_meta_items(attr_doc);
            // Currently it's only possible to have a single meta item on
            // an attribute
            assert_eq!(meta_items.len(), 1);
            let meta_item = meta_items.into_iter().nth(0).unwrap();
            attrs.push(
                codemap::Spanned {
                    node: ast::Attribute_ {
                        id: attr::mk_attr_id(),
                        style: ast::AttrOuter,
                        value: meta_item,
                        is_sugared_doc: is_sugared_doc,
                    },
                    span: codemap::DUMMY_SP
                });
            true
        });
      }
      None => ()
    }
    return attrs;
}

fn list_crate_attributes(md: rbml::Doc, hash: &Svh,
                         out: &mut io::Write) -> io::Result<()> {
    try!(write!(out, "=Crate Attributes ({})=\n", *hash));

    let r = get_attributes(md);
    for attr in &r {
        try!(write!(out, "{}\n", pprust::attribute_to_string(attr)));
    }

    write!(out, "\n\n")
}

pub fn get_crate_attributes(data: &[u8]) -> Vec<ast::Attribute> {
    get_attributes(rbml::Doc::new(data))
}

#[derive(Clone)]
pub struct CrateDep {
    pub cnum: ast::CrateNum,
    pub name: String,
    pub hash: Svh,
}

pub fn get_crate_deps(data: &[u8]) -> Vec<CrateDep> {
    let mut deps: Vec<CrateDep> = Vec::new();
    let cratedoc = rbml::Doc::new(data);
    let depsdoc = reader::get_doc(cratedoc, tag_crate_deps);
    let mut crate_num = 1;
    fn docstr(doc: rbml::Doc, tag_: usize) -> String {
        let d = reader::get_doc(doc, tag_);
        d.as_str_slice().to_string()
    }
    reader::tagged_docs(depsdoc, tag_crate_dep, |depdoc| {
        let name = docstr(depdoc, tag_crate_dep_crate_name);
        let hash = Svh::new(&docstr(depdoc, tag_crate_dep_hash));
        deps.push(CrateDep {
            cnum: crate_num,
            name: name,
            hash: hash,
        });
        crate_num += 1;
        true
    });
    return deps;
}

fn list_crate_deps(data: &[u8], out: &mut io::Write) -> io::Result<()> {
    try!(write!(out, "=External Dependencies=\n"));
    for dep in &get_crate_deps(data) {
        try!(write!(out, "{} {}-{}\n", dep.cnum, dep.name, dep.hash));
    }
    try!(write!(out, "\n"));
    Ok(())
}

pub fn maybe_get_crate_hash(data: &[u8]) -> Option<Svh> {
    let cratedoc = rbml::Doc::new(data);
    reader::maybe_get_doc(cratedoc, tag_crate_hash).map(|doc| {
        Svh::new(doc.as_str_slice())
    })
}

pub fn get_crate_hash(data: &[u8]) -> Svh {
    let cratedoc = rbml::Doc::new(data);
    let hashdoc = reader::get_doc(cratedoc, tag_crate_hash);
    Svh::new(hashdoc.as_str_slice())
}

pub fn maybe_get_crate_name(data: &[u8]) -> Option<String> {
    let cratedoc = rbml::Doc::new(data);
    reader::maybe_get_doc(cratedoc, tag_crate_crate_name).map(|doc| {
        doc.as_str_slice().to_string()
    })
}

pub fn get_crate_triple(data: &[u8]) -> Option<String> {
    let cratedoc = rbml::Doc::new(data);
    let triple_doc = reader::maybe_get_doc(cratedoc, tag_crate_triple);
    triple_doc.map(|s| s.as_str().to_string())
}

pub fn get_crate_name(data: &[u8]) -> String {
    maybe_get_crate_name(data).expect("no crate name in crate")
}

pub fn list_crate_metadata(bytes: &[u8], out: &mut io::Write) -> io::Result<()> {
    let hash = get_crate_hash(bytes);
    let md = rbml::Doc::new(bytes);
    try!(list_crate_attributes(md, &hash, out));
    list_crate_deps(bytes, out)
}

// Translates a def_id from an external crate to a def_id for the current
// compilation environment. We use this when trying to load types from
// external crates - if those types further refer to types in other crates
// then we must translate the crate number from that encoded in the external
// crate to the correct local crate number.
pub fn translate_def_id(cdata: Cmd, did: ast::DefId) -> ast::DefId {
    if did.krate == ast::LOCAL_CRATE {
        return ast::DefId { krate: cdata.cnum, node: did.node };
    }

    match cdata.cnum_map.get(&did.krate) {
        Some(&n) => {
            ast::DefId {
                krate: n,
                node: did.node,
            }
        }
        None => panic!("didn't find a crate in the cnum_map")
    }
}

pub fn each_impl<F>(cdata: Cmd, mut callback: F) where
    F: FnMut(ast::DefId),
{
    let impls_doc = reader::get_doc(rbml::Doc::new(cdata.data()), tag_impls);
    let _ = reader::tagged_docs(impls_doc, tag_impls_impl, |impl_doc| {
        callback(item_def_id(impl_doc, cdata));
        true
    });
}

pub fn each_implementation_for_type<F>(cdata: Cmd,
                                       id: ast::NodeId,
                                       mut callback: F)
    where F: FnMut(ast::DefId),
{
    let item_doc = lookup_item(id, cdata.data());
    reader::tagged_docs(item_doc,
                        tag_items_data_item_inherent_impl,
                        |impl_doc| {
        let implementation_def_id = item_def_id(impl_doc, cdata);
        callback(implementation_def_id);
        true
    });
}

pub fn each_implementation_for_trait<F>(cdata: Cmd,
                                        id: ast::NodeId,
                                        mut callback: F) where
    F: FnMut(ast::DefId),
{
    let item_doc = lookup_item(id, cdata.data());

    let _ = reader::tagged_docs(item_doc,
                                tag_items_data_item_extension_impl,
                                |impl_doc| {
        let implementation_def_id = item_def_id(impl_doc, cdata);
        callback(implementation_def_id);
        true
    });
}

pub fn get_trait_of_item(cdata: Cmd, id: ast::NodeId, tcx: &ty::ctxt)
                         -> Option<ast::DefId> {
    let item_doc = lookup_item(id, cdata.data());
    let parent_item_id = match item_parent_item(item_doc) {
        None => return None,
        Some(item_id) => item_id,
    };
    let parent_item_id = translate_def_id(cdata, parent_item_id);
    let parent_item_doc = lookup_item(parent_item_id.node, cdata.data());
    match item_family(parent_item_doc) {
        Trait => Some(item_def_id(parent_item_doc, cdata)),
        Impl | DefaultImpl => {
            reader::maybe_get_doc(parent_item_doc, tag_item_trait_ref)
                .map(|_| item_trait_ref(parent_item_doc, tcx, cdata).def_id)
        }
        _ => None
    }
}


pub fn get_native_libraries(cdata: Cmd)
                            -> Vec<(cstore::NativeLibraryKind, String)> {
    let libraries = reader::get_doc(rbml::Doc::new(cdata.data()),
                                    tag_native_libraries);
    let mut result = Vec::new();
    reader::tagged_docs(libraries, tag_native_libraries_lib, |lib_doc| {
        let kind_doc = reader::get_doc(lib_doc, tag_native_libraries_kind);
        let name_doc = reader::get_doc(lib_doc, tag_native_libraries_name);
        let kind: cstore::NativeLibraryKind =
            cstore::NativeLibraryKind::from_u32(reader::doc_as_u32(kind_doc)).unwrap();
        let name = name_doc.as_str().to_string();
        result.push((kind, name));
        true
    });
    return result;
}

pub fn get_plugin_registrar_fn(data: &[u8]) -> Option<ast::NodeId> {
    reader::maybe_get_doc(rbml::Doc::new(data), tag_plugin_registrar_fn)
        .map(|doc| reader::doc_as_u32(doc))
}

pub fn each_exported_macro<F>(data: &[u8], intr: &IdentInterner, mut f: F) where
    F: FnMut(ast::Name, Vec<ast::Attribute>, String) -> bool,
{
    let macros = reader::get_doc(rbml::Doc::new(data), tag_macro_defs);
    reader::tagged_docs(macros, tag_macro_def, |macro_doc| {
        let name = item_name(intr, macro_doc);
        let attrs = get_attributes(macro_doc);
        let body = reader::get_doc(macro_doc, tag_macro_def_body);
        f(name, attrs, body.as_str().to_string())
    });
}

pub fn get_dylib_dependency_formats(cdata: Cmd)
    -> Vec<(ast::CrateNum, cstore::LinkagePreference)>
{
    let formats = reader::get_doc(rbml::Doc::new(cdata.data()),
                                  tag_dylib_dependency_formats);
    let mut result = Vec::new();

    debug!("found dylib deps: {}", formats.as_str_slice());
    for spec in formats.as_str_slice().split(',') {
        if spec.is_empty() { continue }
        let cnum = spec.split(':').nth(0).unwrap();
        let link = spec.split(':').nth(1).unwrap();
        let cnum: ast::CrateNum = cnum.parse().unwrap();
        let cnum = match cdata.cnum_map.get(&cnum) {
            Some(&n) => n,
            None => panic!("didn't find a crate in the cnum_map")
        };
        result.push((cnum, if link == "d" {
            cstore::RequireDynamic
        } else {
            cstore::RequireStatic
        }));
    }
    return result;
}

pub fn get_missing_lang_items(cdata: Cmd)
    -> Vec<lang_items::LangItem>
{
    let items = reader::get_doc(rbml::Doc::new(cdata.data()), tag_lang_items);
    let mut result = Vec::new();
    reader::tagged_docs(items, tag_lang_items_missing, |missing_docs| {
        let item: lang_items::LangItem =
            lang_items::LangItem::from_u32(reader::doc_as_u32(missing_docs)).unwrap();
        result.push(item);
        true
    });
    return result;
}

pub fn get_method_arg_names(cdata: Cmd, id: ast::NodeId) -> Vec<String> {
    let mut ret = Vec::new();
    let method_doc = lookup_item(id, cdata.data());
    match reader::maybe_get_doc(method_doc, tag_method_argument_names) {
        Some(args_doc) => {
            reader::tagged_docs(args_doc, tag_method_argument_name, |name_doc| {
                ret.push(name_doc.as_str_slice().to_string());
                true
            });
        }
        None => {}
    }
    return ret;
}

pub fn get_reachable_extern_fns(cdata: Cmd) -> Vec<ast::DefId> {
    let mut ret = Vec::new();
    let items = reader::get_doc(rbml::Doc::new(cdata.data()),
                                tag_reachable_extern_fns);
    reader::tagged_docs(items, tag_reachable_extern_fn_id, |doc| {
        ret.push(ast::DefId {
            krate: cdata.cnum,
            node: reader::doc_as_u32(doc),
        });
        true
    });
    return ret;
}

pub fn is_typedef(cdata: Cmd, id: ast::NodeId) -> bool {
    let item_doc = lookup_item(id, cdata.data());
    match item_family(item_doc) {
        Type => true,
        _ => false,
    }
}

fn doc_generics<'tcx>(base_doc: rbml::Doc,
                      tcx: &ty::ctxt<'tcx>,
                      cdata: Cmd,
                      tag: usize)
                      -> ty::Generics<'tcx>
{
    let doc = reader::get_doc(base_doc, tag);

    let mut types = subst::VecPerParamSpace::empty();
    reader::tagged_docs(doc, tag_type_param_def, |p| {
        let bd = parse_type_param_def_data(
            p.data, p.start, cdata.cnum, tcx,
            |_, did| translate_def_id(cdata, did));
        types.push(bd.space, bd);
        true
    });

    let mut regions = subst::VecPerParamSpace::empty();
    reader::tagged_docs(doc, tag_region_param_def, |rp_doc| {
        let ident_str_doc = reader::get_doc(rp_doc,
                                            tag_region_param_def_ident);
        let name = item_name(&*token::get_ident_interner(), ident_str_doc);
        let def_id_doc = reader::get_doc(rp_doc,
                                         tag_region_param_def_def_id);
        let def_id = reader::with_doc_data(def_id_doc, parse_def_id);
        let def_id = translate_def_id(cdata, def_id);

        let doc = reader::get_doc(rp_doc, tag_region_param_def_space);
        let space = subst::ParamSpace::from_uint(reader::doc_as_u64(doc) as usize);

        let doc = reader::get_doc(rp_doc, tag_region_param_def_index);
        let index = reader::doc_as_u64(doc) as u32;

        let mut bounds = Vec::new();
        reader::tagged_docs(rp_doc, tag_items_data_region, |p| {
            bounds.push(
                parse_region_data(
                    p.data, cdata.cnum, p.start, tcx,
                    |_, did| translate_def_id(cdata, did)));
            true
        });

        regions.push(space, ty::RegionParameterDef { name: name,
                                                     def_id: def_id,
                                                     space: space,
                                                     index: index,
                                                     bounds: bounds });

        true
    });

    ty::Generics { types: types, regions: regions }
}

fn doc_predicates<'tcx>(base_doc: rbml::Doc,
                        tcx: &ty::ctxt<'tcx>,
                        cdata: Cmd,
                        tag: usize)
                        -> ty::GenericPredicates<'tcx>
{
    let doc = reader::get_doc(base_doc, tag);

    let mut predicates = subst::VecPerParamSpace::empty();
    reader::tagged_docs(doc, tag_predicate, |predicate_doc| {
        let space_doc = reader::get_doc(predicate_doc, tag_predicate_space);
        let space = subst::ParamSpace::from_uint(reader::doc_as_u8(space_doc) as usize);

        let data_doc = reader::get_doc(predicate_doc, tag_predicate_data);
        let data = parse_predicate_data(data_doc.data, data_doc.start, cdata.cnum, tcx,
                                        |_, did| translate_def_id(cdata, did));

        predicates.push(space, data);
        true
    });

    ty::GenericPredicates { predicates: predicates }
}

pub fn is_associated_type(cdata: Cmd, id: ast::NodeId) -> bool {
    let items = reader::get_doc(rbml::Doc::new(cdata.data()), tag_items);
    match maybe_find_item(id, items) {
        None => false,
        Some(item) => item_sort(item) == Some('t'),
    }
}

pub fn is_defaulted_trait(cdata: Cmd, trait_id: ast::NodeId) -> bool {
    let trait_doc = lookup_item(trait_id, cdata.data());
    assert!(item_family(trait_doc) == Family::Trait);
    let defaulted_doc = reader::get_doc(trait_doc, tag_defaulted_trait);
    reader::doc_as_u8(defaulted_doc) != 0
}

pub fn is_default_impl(cdata: Cmd, impl_id: ast::NodeId) -> bool {
    let impl_doc = lookup_item(impl_id, cdata.data());
    item_family(impl_doc) == Family::DefaultImpl
}

pub fn get_imported_filemaps(metadata: &[u8]) -> Vec<codemap::FileMap> {
    let crate_doc = rbml::Doc::new(metadata);
    let cm_doc = reader::get_doc(crate_doc, tag_codemap);

    let mut filemaps = vec![];

    reader::tagged_docs(cm_doc, tag_codemap_filemap, |filemap_doc| {
        let mut decoder = reader::Decoder::new(filemap_doc);
        let filemap: codemap::FileMap = Decodable::decode(&mut decoder).unwrap();
        filemaps.push(filemap);
        true
    });

    return filemaps;
}
