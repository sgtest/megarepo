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

use self::Family::*;

use astencode::decode_inlined_item;
use cstore::{self, crate_metadata};
use common::*;
use def_key;
use encoder::def_to_u64;
use index;
use tls_context;
use tydecode::TyDecoder;

use rustc::hir::svh::Svh;
use rustc::hir::map as hir_map;
use rustc::hir::map::DefKey;
use rustc::util::nodemap::FnvHashMap;
use rustc::hir;
use rustc::session::config::PanicStrategy;

use middle::cstore::{LOCAL_CRATE, FoundAst, InlinedItem, LinkagePreference};
use middle::cstore::{DefLike, DlDef, DlField, DlImpl, tls};
use rustc::hir::def::Def;
use rustc::hir::def_id::{DefId, DefIndex};
use middle::lang_items;
use rustc::ty::subst;
use rustc::ty::{ImplContainer, TraitContainer};
use rustc::ty::{self, Ty, TyCtxt, TypeFoldable, VariantKind};

use rustc_const_math::ConstInt;

use rustc::mir;
use rustc::mir::visit::MutVisitor;

use std::cell::Cell;
use std::io;
use std::rc::Rc;
use std::str;

use rbml::reader;
use rbml;
use rustc_serialize::Decodable;
use syntax::attr;
use syntax::parse::token::{self, IdentInterner};
use syntax::ast;
use syntax::abi::Abi;
use syntax::codemap;
use syntax::print::pprust;
use syntax::ptr::P;
use syntax_pos::{self, Span, BytePos, NO_EXPANSION};

pub type Cmd<'a> = &'a crate_metadata;

impl crate_metadata {
    fn get_item(&self, item_id: DefIndex) -> Option<rbml::Doc> {
        self.index.lookup_item(self.data(), item_id).map(|pos| {
            reader::doc_at(self.data(), pos as usize).unwrap().doc
        })
    }

    fn lookup_item(&self, item_id: DefIndex) -> rbml::Doc {
        match self.get_item(item_id) {
            None => bug!("lookup_item: id not found: {:?} in crate {:?} with number {}",
                         item_id,
                         self.name,
                         self.cnum),
            Some(d) => d
        }
    }
}

pub fn load_index(data: &[u8]) -> index::Index {
    let index = reader::get_doc(rbml::Doc::new(data), tag_index);
    index::Index::from_rbml(index)
}

pub fn crate_rustc_version(data: &[u8]) -> Option<String> {
    let doc = rbml::Doc::new(data);
    reader::maybe_get_doc(doc, tag_rustc_version).map(|s| s.as_str())
}

pub fn load_xrefs(data: &[u8]) -> index::DenseIndex {
    let index = reader::get_doc(rbml::Doc::new(data), tag_xref_index);
    index::DenseIndex::from_buf(index.data, index.start, index.end)
}

// Go through each item in the metadata and create a map from that
// item's def-key to the item's DefIndex.
pub fn load_key_map(data: &[u8]) -> FnvHashMap<DefKey, DefIndex> {
    let root_doc = rbml::Doc::new(data);
    let items_doc = reader::get_doc(root_doc, tag_items);
    let items_data_doc = reader::get_doc(items_doc, tag_items_data);
    reader::docs(items_data_doc)
        .filter(|&(tag, _)| tag == tag_items_data_item)
        .map(|(_, item_doc)| {
            // load def-key from item
            let key = item_def_key(item_doc);

            // load def-index from item; we only encode the full def-id,
            // so just pull out the index
            let def_id_doc = reader::get_doc(item_doc, tag_def_id);
            let def_id = untranslated_def_id(def_id_doc);
            assert!(def_id.is_local()); // local to the crate we are decoding, that is

            (key, def_id.index)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Family {
    ImmStatic,             // c
    MutStatic,             // b
    Fn,                    // f
    StaticMethod,          // F
    Method,                // h
    Type,                  // y
    Mod,                   // m
    ForeignMod,            // n
    Enum,                  // t
    Variant(VariantKind),  // V, v, w
    Impl,                  // i
    DefaultImpl,           // d
    Trait,                 // I
    Struct(VariantKind),   // S, s, u
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
      'F' => StaticMethod,
      'h' => Method,
      'y' => Type,
      'm' => Mod,
      'n' => ForeignMod,
      't' => Enum,
      'V' => Variant(VariantKind::Struct),
      'v' => Variant(VariantKind::Tuple),
      'w' => Variant(VariantKind::Unit),
      'i' => Impl,
      'd' => DefaultImpl,
      'I' => Trait,
      'S' => Struct(VariantKind::Struct),
      's' => Struct(VariantKind::Tuple),
      'u' => Struct(VariantKind::Unit),
      'g' => PublicField,
      'N' => InheritedField,
       c => bug!("unexpected family char: {}", c)
    }
}

fn item_visibility(item: rbml::Doc) -> ty::Visibility {
    match reader::maybe_get_doc(item, tag_items_data_item_visibility) {
        None => ty::Visibility::Public,
        Some(visibility_doc) => {
            match reader::doc_as_u8(visibility_doc) as char {
                'y' => ty::Visibility::Public,
                'i' => ty::Visibility::PrivateExternal,
                _ => bug!("unknown visibility character")
            }
        }
    }
}

fn fn_constness(item: rbml::Doc) -> hir::Constness {
    match reader::maybe_get_doc(item, tag_items_data_item_constness) {
        None => hir::Constness::NotConst,
        Some(constness_doc) => {
            match reader::doc_as_u8(constness_doc) as char {
                'c' => hir::Constness::Const,
                'n' => hir::Constness::NotConst,
                _ => bug!("unknown constness character")
            }
        }
    }
}

fn item_defaultness(item: rbml::Doc) -> hir::Defaultness {
    match reader::maybe_get_doc(item, tag_items_data_item_defaultness) {
        None => hir::Defaultness::Default, // should occur only for default impls on traits
        Some(defaultness_doc) => {
            match reader::doc_as_u8(defaultness_doc) as char {
                'd' => hir::Defaultness::Default,
                'f' => hir::Defaultness::Final,
                _ => bug!("unknown defaultness character")
            }
        }
    }
}

fn item_sort(item: rbml::Doc) -> Option<char> {
    reader::tagged_docs(item, tag_item_trait_item_sort).nth(0).map(|doc| {
        doc.as_str_slice().as_bytes()[0] as char
    })
}

fn untranslated_def_id(d: rbml::Doc) -> DefId {
    let id = reader::doc_as_u64(d);
    let index = DefIndex::new((id & 0xFFFF_FFFF) as usize);
    DefId { krate: (id >> 32) as u32, index: index }
}

fn translated_def_id(cdata: Cmd, d: rbml::Doc) -> DefId {
    let def_id = untranslated_def_id(d);
    translate_def_id(cdata, def_id)
}

fn item_parent_item(cdata: Cmd, d: rbml::Doc) -> Option<DefId> {
    reader::tagged_docs(d, tag_items_data_parent_item).nth(0).map(|did| {
        translated_def_id(cdata, did)
    })
}

fn item_require_parent_item(cdata: Cmd, d: rbml::Doc) -> DefId {
    translated_def_id(cdata, reader::get_doc(d, tag_items_data_parent_item))
}

fn item_def_id(d: rbml::Doc, cdata: Cmd) -> DefId {
    translated_def_id(cdata, reader::get_doc(d, tag_def_id))
}

fn reexports<'a>(d: rbml::Doc<'a>) -> reader::TaggedDocsIterator<'a> {
    reader::tagged_docs(d, tag_items_data_item_reexport)
}

fn variant_disr_val(d: rbml::Doc) -> Option<u64> {
    reader::maybe_get_doc(d, tag_disr_val).and_then(|val_doc| {
        reader::with_doc_data(val_doc, |data| {
            str::from_utf8(data).ok().and_then(|s| s.parse().ok())
        })
    })
}

fn doc_type<'a, 'tcx>(doc: rbml::Doc, tcx: TyCtxt<'a, 'tcx, 'tcx>, cdata: Cmd) -> Ty<'tcx> {
    let tp = reader::get_doc(doc, tag_items_data_item_type);
    TyDecoder::with_doc(tcx, cdata.cnum, tp,
                        &mut |did| translate_def_id(cdata, did))
        .parse_ty()
}

fn maybe_doc_type<'a, 'tcx>(doc: rbml::Doc, tcx: TyCtxt<'a, 'tcx, 'tcx>, cdata: Cmd)
                            -> Option<Ty<'tcx>> {
    reader::maybe_get_doc(doc, tag_items_data_item_type).map(|tp| {
        TyDecoder::with_doc(tcx, cdata.cnum, tp,
                            &mut |did| translate_def_id(cdata, did))
            .parse_ty()
    })
}

pub fn item_type<'a, 'tcx>(_item_id: DefId, item: rbml::Doc,
                           tcx: TyCtxt<'a, 'tcx, 'tcx>, cdata: Cmd) -> Ty<'tcx> {
    doc_type(item, tcx, cdata)
}

fn doc_trait_ref<'a, 'tcx>(doc: rbml::Doc, tcx: TyCtxt<'a, 'tcx, 'tcx>, cdata: Cmd)
                           -> ty::TraitRef<'tcx> {
    TyDecoder::with_doc(tcx, cdata.cnum, doc,
                        &mut |did| translate_def_id(cdata, did))
        .parse_trait_ref()
}

fn item_trait_ref<'a, 'tcx>(doc: rbml::Doc, tcx: TyCtxt<'a, 'tcx, 'tcx>, cdata: Cmd)
                            -> ty::TraitRef<'tcx> {
    let tp = reader::get_doc(doc, tag_item_trait_ref);
    doc_trait_ref(tp, tcx, cdata)
}

fn item_name(intr: &IdentInterner, item: rbml::Doc) -> ast::Name {
    maybe_item_name(intr, item).expect("no item in item_name")
}

fn maybe_item_name(intr: &IdentInterner, item: rbml::Doc) -> Option<ast::Name> {
    reader::maybe_get_doc(item, tag_paths_data_name).map(|name| {
        let string = name.as_str_slice();
        match intr.find(string) {
            None => token::intern(string),
            Some(val) => val,
        }
    })
}

fn family_to_variant_kind<'tcx>(family: Family) -> Option<ty::VariantKind> {
    match family {
        Struct(VariantKind::Struct) | Variant(VariantKind::Struct) =>
            Some(ty::VariantKind::Struct),
        Struct(VariantKind::Tuple) | Variant(VariantKind::Tuple) =>
            Some(ty::VariantKind::Tuple),
        Struct(VariantKind::Unit) | Variant(VariantKind::Unit) =>
            Some(ty::VariantKind::Unit),
        _ => None,
    }
}

fn item_to_def_like(cdata: Cmd, item: rbml::Doc, did: DefId) -> DefLike {
    let fam = item_family(item);
    match fam {
        Constant  => {
            // Check whether we have an associated const item.
            match item_sort(item) {
                Some('C') | Some('c') => {
                    DlDef(Def::AssociatedConst(did))
                }
                _ => {
                    // Regular const item.
                    DlDef(Def::Const(did))
                }
            }
        }
        ImmStatic => DlDef(Def::Static(did, false)),
        MutStatic => DlDef(Def::Static(did, true)),
        Struct(..) => DlDef(Def::Struct(did)),
        Fn        => DlDef(Def::Fn(did)),
        Method | StaticMethod => {
            DlDef(Def::Method(did))
        }
        Type => {
            if item_sort(item) == Some('t') {
                let trait_did = item_require_parent_item(cdata, item);
                DlDef(Def::AssociatedTy(trait_did, did))
            } else {
                DlDef(Def::TyAlias(did))
            }
        }
        Mod => DlDef(Def::Mod(did)),
        ForeignMod => DlDef(Def::ForeignMod(did)),
        Variant(..) => {
            let enum_did = item_require_parent_item(cdata, item);
            DlDef(Def::Variant(enum_did, did))
        }
        Trait => DlDef(Def::Trait(did)),
        Enum => DlDef(Def::Enum(did)),
        Impl | DefaultImpl => DlImpl(did),
        PublicField | InheritedField => DlField,
    }
}

fn parse_unsafety(item_doc: rbml::Doc) -> hir::Unsafety {
    let unsafety_doc = reader::get_doc(item_doc, tag_unsafety);
    if reader::doc_as_u8(unsafety_doc) != 0 {
        hir::Unsafety::Unsafe
    } else {
        hir::Unsafety::Normal
    }
}

fn parse_paren_sugar(item_doc: rbml::Doc) -> bool {
    let paren_sugar_doc = reader::get_doc(item_doc, tag_paren_sugar);
    reader::doc_as_u8(paren_sugar_doc) != 0
}

fn parse_polarity(item_doc: rbml::Doc) -> hir::ImplPolarity {
    let polarity_doc = reader::get_doc(item_doc, tag_polarity);
    if reader::doc_as_u8(polarity_doc) != 0 {
        hir::ImplPolarity::Negative
    } else {
        hir::ImplPolarity::Positive
    }
}

fn parse_associated_type_names(item_doc: rbml::Doc) -> Vec<ast::Name> {
    let names_doc = reader::get_doc(item_doc, tag_associated_type_names);
    reader::tagged_docs(names_doc, tag_associated_type_name)
        .map(|name_doc| token::intern(name_doc.as_str_slice()))
        .collect()
}

pub fn get_trait_def<'a, 'tcx>(cdata: Cmd,
                               item_id: DefIndex,
                               tcx: TyCtxt<'a, 'tcx, 'tcx>) -> ty::TraitDef<'tcx>
{
    let item_doc = cdata.lookup_item(item_id);
    let generics = doc_generics(item_doc, tcx, cdata, tag_item_generics);
    let unsafety = parse_unsafety(item_doc);
    let associated_type_names = parse_associated_type_names(item_doc);
    let paren_sugar = parse_paren_sugar(item_doc);

    ty::TraitDef::new(unsafety,
                      paren_sugar,
                      generics,
                      item_trait_ref(item_doc, tcx, cdata),
                      associated_type_names)
}

pub fn get_adt_def<'a, 'tcx>(intr: &IdentInterner,
                             cdata: Cmd,
                             item_id: DefIndex,
                             tcx: TyCtxt<'a, 'tcx, 'tcx>)
                             -> ty::AdtDefMaster<'tcx>
{
    fn expect_variant_kind(family: Family) -> ty::VariantKind {
        match family_to_variant_kind(family) {
            Some(kind) => kind,
            _ => bug!("unexpected family: {:?}", family),
        }
    }
    fn get_enum_variants<'tcx>(intr: &IdentInterner,
                               cdata: Cmd,
                               doc: rbml::Doc) -> Vec<ty::VariantDefData<'tcx, 'tcx>> {
        let mut disr_val = 0;
        reader::tagged_docs(doc, tag_items_data_item_variant).map(|p| {
            let did = translated_def_id(cdata, p);
            let item = cdata.lookup_item(did.index);

            if let Some(disr) = variant_disr_val(item) {
                disr_val = disr;
            }
            let disr = disr_val;
            disr_val = disr_val.wrapping_add(1);

            ty::VariantDefData {
                did: did,
                name: item_name(intr, item),
                fields: get_variant_fields(intr, cdata, item),
                disr_val: ConstInt::Infer(disr),
                kind: expect_variant_kind(item_family(item)),
            }
        }).collect()
    }
    fn get_variant_fields<'tcx>(intr: &IdentInterner,
                                cdata: Cmd,
                                doc: rbml::Doc) -> Vec<ty::FieldDefData<'tcx, 'tcx>> {
        let mut index = 0;
        reader::tagged_docs(doc, tag_item_field).map(|f| {
            let ff = item_family(f);
            match ff {
                PublicField | InheritedField => {},
                _ => bug!("expected field, found {:?}", ff)
            };
            ty::FieldDefData::new(item_def_id(f, cdata),
                                  item_name(intr, f),
                                  struct_field_family_to_visibility(ff))
        }).chain(reader::tagged_docs(doc, tag_item_unnamed_field).map(|f| {
            let ff = item_family(f);
            let name = intr.intern(&index.to_string());
            index += 1;
            ty::FieldDefData::new(item_def_id(f, cdata), name,
                                  struct_field_family_to_visibility(ff))
        })).collect()
    }
    fn get_struct_variant<'tcx>(intr: &IdentInterner,
                                cdata: Cmd,
                                doc: rbml::Doc,
                                did: DefId) -> ty::VariantDefData<'tcx, 'tcx> {
        ty::VariantDefData {
            did: did,
            name: item_name(intr, doc),
            fields: get_variant_fields(intr, cdata, doc),
            disr_val: ConstInt::Infer(0),
            kind: expect_variant_kind(item_family(doc)),
        }
    }

    let doc = cdata.lookup_item(item_id);
    let did = DefId { krate: cdata.cnum, index: item_id };
    let (kind, variants) = match item_family(doc) {
        Enum => {
            (ty::AdtKind::Enum,
             get_enum_variants(intr, cdata, doc))
        }
        Struct(..) => {
            let ctor_did =
                reader::maybe_get_doc(doc, tag_items_data_item_struct_ctor).
                map_or(did, |ctor_doc| translated_def_id(cdata, ctor_doc));
            (ty::AdtKind::Struct,
             vec![get_struct_variant(intr, cdata, doc, ctor_did)])
        }
        _ => bug!("get_adt_def called on a non-ADT {:?} - {:?}",
                  item_family(doc), did)
    };

    let adt = tcx.intern_adt_def(did, kind, variants);

    // this needs to be done *after* the variant is interned,
    // to support recursive structures
    for variant in &adt.variants {
        if variant.kind() == ty::VariantKind::Tuple &&
            adt.adt_kind() == ty::AdtKind::Enum {
            // tuple-like enum variant fields aren't real items - get the types
            // from the ctor.
            debug!("evaluating the ctor-type of {:?}",
                   variant.name);
            let ctor_ty = get_type(cdata, variant.did.index, tcx).ty;
            debug!("evaluating the ctor-type of {:?}.. {:?}",
                   variant.name,
                   ctor_ty);
            let field_tys = match ctor_ty.sty {
                ty::TyFnDef(_, _, &ty::BareFnTy { sig: ty::Binder(ty::FnSig {
                    ref inputs, ..
                }), ..}) => {
                    // tuple-struct constructors don't have escaping regions
                    assert!(!inputs.has_escaping_regions());
                    inputs
                },
                _ => bug!("tuple-variant ctor is not an ADT")
            };
            for (field, &ty) in variant.fields.iter().zip(field_tys.iter()) {
                field.fulfill_ty(ty);
            }
        } else {
            for field in &variant.fields {
                debug!("evaluating the type of {:?}::{:?}", variant.name, field.name);
                let ty = get_type(cdata, field.did.index, tcx).ty;
                field.fulfill_ty(ty);
                debug!("evaluating the type of {:?}::{:?}: {:?}",
                       variant.name, field.name, ty);
            }
        }
    }

    adt
}

pub fn get_predicates<'a, 'tcx>(cdata: Cmd,
                                item_id: DefIndex,
                                tcx: TyCtxt<'a, 'tcx, 'tcx>)
                                -> ty::GenericPredicates<'tcx>
{
    let item_doc = cdata.lookup_item(item_id);
    doc_predicates(item_doc, tcx, cdata, tag_item_generics)
}

pub fn get_super_predicates<'a, 'tcx>(cdata: Cmd,
                                      item_id: DefIndex,
                                      tcx: TyCtxt<'a, 'tcx, 'tcx>)
                                      -> ty::GenericPredicates<'tcx>
{
    let item_doc = cdata.lookup_item(item_id);
    doc_predicates(item_doc, tcx, cdata, tag_item_super_predicates)
}

pub fn get_type<'a, 'tcx>(cdata: Cmd, id: DefIndex, tcx: TyCtxt<'a, 'tcx, 'tcx>)
                          -> ty::TypeScheme<'tcx>
{
    let item_doc = cdata.lookup_item(id);
    let t = item_type(DefId { krate: cdata.cnum, index: id }, item_doc, tcx,
                      cdata);
    let generics = doc_generics(item_doc, tcx, cdata, tag_item_generics);
    ty::TypeScheme {
        generics: generics,
        ty: t
    }
}

pub fn get_stability(cdata: Cmd, id: DefIndex) -> Option<attr::Stability> {
    let item = cdata.lookup_item(id);
    reader::maybe_get_doc(item, tag_items_data_item_stability).map(|doc| {
        let mut decoder = reader::Decoder::new(doc);
        Decodable::decode(&mut decoder).unwrap()
    })
}

pub fn get_deprecation(cdata: Cmd, id: DefIndex) -> Option<attr::Deprecation> {
    let item = cdata.lookup_item(id);
    reader::maybe_get_doc(item, tag_items_data_item_deprecation).map(|doc| {
        let mut decoder = reader::Decoder::new(doc);
        Decodable::decode(&mut decoder).unwrap()
    })
}

pub fn get_visibility(cdata: Cmd, id: DefIndex) -> ty::Visibility {
    item_visibility(cdata.lookup_item(id))
}

pub fn get_parent_impl(cdata: Cmd, id: DefIndex) -> Option<DefId> {
    let item = cdata.lookup_item(id);
    reader::maybe_get_doc(item, tag_items_data_parent_impl).map(|doc| {
        translated_def_id(cdata, doc)
    })
}

pub fn get_repr_attrs(cdata: Cmd, id: DefIndex) -> Vec<attr::ReprAttr> {
    let item = cdata.lookup_item(id);
    match reader::maybe_get_doc(item, tag_items_data_item_repr).map(|doc| {
        let mut decoder = reader::Decoder::new(doc);
        Decodable::decode(&mut decoder).unwrap()
    }) {
        Some(attrs) => attrs,
        None => Vec::new(),
    }
}

pub fn get_impl_polarity<'tcx>(cdata: Cmd,
                               id: DefIndex)
                               -> Option<hir::ImplPolarity>
{
    let item_doc = cdata.lookup_item(id);
    let fam = item_family(item_doc);
    match fam {
        Family::Impl => {
            Some(parse_polarity(item_doc))
        }
        _ => None
    }
}

pub fn get_custom_coerce_unsized_kind<'tcx>(
    cdata: Cmd,
    id: DefIndex)
    -> Option<ty::adjustment::CustomCoerceUnsized>
{
    let item_doc = cdata.lookup_item(id);
    reader::maybe_get_doc(item_doc, tag_impl_coerce_unsized_kind).map(|kind_doc| {
        let mut decoder = reader::Decoder::new(kind_doc);
        Decodable::decode(&mut decoder).unwrap()
    })
}

pub fn get_impl_trait<'a, 'tcx>(cdata: Cmd,
                                id: DefIndex,
                                tcx: TyCtxt<'a, 'tcx, 'tcx>)
                                -> Option<ty::TraitRef<'tcx>>
{
    let item_doc = cdata.lookup_item(id);
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

/// Iterates over the language items in the given crate.
pub fn each_lang_item<F>(cdata: Cmd, mut f: F) -> bool where
    F: FnMut(DefIndex, usize) -> bool,
{
    let root = rbml::Doc::new(cdata.data());
    let lang_items = reader::get_doc(root, tag_lang_items);
    reader::tagged_docs(lang_items, tag_lang_items_item).all(|item_doc| {
        let id_doc = reader::get_doc(item_doc, tag_lang_items_item_id);
        let id = reader::doc_as_u32(id_doc) as usize;
        let index_doc = reader::get_doc(item_doc, tag_lang_items_item_index);
        let index = DefIndex::from_u32(reader::doc_as_u32(index_doc));

        f(index, id)
    })
}

fn each_child_of_item_or_crate<F, G>(intr: Rc<IdentInterner>,
                                     cdata: Cmd,
                                     item_doc: rbml::Doc,
                                     mut get_crate_data: G,
                                     mut callback: F) where
    F: FnMut(DefLike, ast::Name, ty::Visibility),
    G: FnMut(ast::CrateNum) -> Rc<crate_metadata>,
{
    // Iterate over all children.
    for child_info_doc in reader::tagged_docs(item_doc, tag_mod_child) {
        let child_def_id = translated_def_id(cdata, child_info_doc);

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

        // Get the item.
        match crate_data.get_item(child_def_id.index) {
            None => {}
            Some(child_item_doc) => {
                // Hand off the item to the callback.
                let child_name = item_name(&intr, child_item_doc);
                let def_like = item_to_def_like(crate_data, child_item_doc, child_def_id);
                let visibility = item_visibility(child_item_doc);
                callback(def_like, child_name, visibility);
            }
        }
    }

    // As a special case, iterate over all static methods of
    // associated implementations too. This is a bit of a botch.
    // --pcwalton
    for inherent_impl_def_id_doc in reader::tagged_docs(item_doc,
                                                             tag_items_data_item_inherent_impl) {
        let inherent_impl_def_id = item_def_id(inherent_impl_def_id_doc, cdata);
        if let Some(inherent_impl_doc) = cdata.get_item(inherent_impl_def_id.index) {
            for impl_item_def_id_doc in reader::tagged_docs(inherent_impl_doc,
                                                                 tag_item_impl_item) {
                let impl_item_def_id = item_def_id(impl_item_def_id_doc,
                                                   cdata);
                if let Some(impl_method_doc) = cdata.get_item(impl_item_def_id.index) {
                    if let StaticMethod = item_family(impl_method_doc) {
                        // Hand off the static method to the callback.
                        let static_method_name = item_name(&intr, impl_method_doc);
                        let static_method_def_like = item_to_def_like(cdata, impl_method_doc,
                                                                      impl_item_def_id);
                        callback(static_method_def_like,
                                 static_method_name,
                                 item_visibility(impl_method_doc));
                    }
                }
            }
        }
    }

    for reexport_doc in reexports(item_doc) {
        let def_id_doc = reader::get_doc(reexport_doc,
                                         tag_items_data_item_reexport_def_id);
        let child_def_id = translated_def_id(cdata, def_id_doc);

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

        // Get the item.
        if let Some(child_item_doc) = crate_data.get_item(child_def_id.index) {
            // Hand off the item to the callback.
            let def_like = item_to_def_like(crate_data, child_item_doc, child_def_id);
            // These items have a public visibility because they're part of
            // a public re-export.
            callback(def_like, token::intern(name), ty::Visibility::Public);
        }
    }
}

/// Iterates over each child of the given item.
pub fn each_child_of_item<F, G>(intr: Rc<IdentInterner>,
                               cdata: Cmd,
                               id: DefIndex,
                               get_crate_data: G,
                               callback: F) where
    F: FnMut(DefLike, ast::Name, ty::Visibility),
    G: FnMut(ast::CrateNum) -> Rc<crate_metadata>,
{
    // Find the item.
    let item_doc = match cdata.get_item(id) {
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
    F: FnMut(DefLike, ast::Name, ty::Visibility),
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

pub fn get_item_name(intr: &IdentInterner, cdata: Cmd, id: DefIndex) -> ast::Name {
    item_name(intr, cdata.lookup_item(id))
}

pub fn maybe_get_item_name(intr: &IdentInterner, cdata: Cmd, id: DefIndex)
                         -> Option<ast::Name> {
    maybe_item_name(intr, cdata.lookup_item(id))
}

pub fn maybe_get_item_ast<'a, 'tcx>(cdata: Cmd, tcx: TyCtxt<'a, 'tcx, 'tcx>, id: DefIndex)
                                    -> FoundAst<'tcx> {
    debug!("Looking up item: {:?}", id);
    let item_doc = cdata.lookup_item(id);
    let item_did = item_def_id(item_doc, cdata);
    let parent_def_id = DefId {
        krate: cdata.cnum,
        index: def_key(cdata, id).parent.unwrap()
    };
    let mut parent_def_path = def_path(cdata, id);
    parent_def_path.data.pop();
    if let Some(ast_doc) = reader::maybe_get_doc(item_doc, tag_ast as usize) {
        let ii = decode_inlined_item(cdata,
                                     tcx,
                                     parent_def_path,
                                     parent_def_id,
                                     ast_doc,
                                     item_did);
        return FoundAst::Found(ii);
    } else if let Some(parent_did) = item_parent_item(cdata, item_doc) {
        // Remove the last element from the paths, since we are now
        // trying to inline the parent.
        let grandparent_def_id = DefId {
            krate: cdata.cnum,
            index: def_key(cdata, parent_def_id.index).parent.unwrap()
        };
        let mut grandparent_def_path = parent_def_path;
        grandparent_def_path.data.pop();
        let parent_doc = cdata.lookup_item(parent_did.index);
        if let Some(ast_doc) = reader::maybe_get_doc(parent_doc, tag_ast as usize) {
            let ii = decode_inlined_item(cdata,
                                         tcx,
                                         grandparent_def_path,
                                         grandparent_def_id,
                                         ast_doc,
                                         parent_did);
            if let &InlinedItem::Item(ref i) = ii {
                return FoundAst::FoundParent(parent_did, i);
            }
        }
    }
    FoundAst::NotFound
}

pub fn is_item_mir_available<'tcx>(cdata: Cmd, id: DefIndex) -> bool {
    if let Some(item_doc) = cdata.get_item(id) {
        return reader::maybe_get_doc(item_doc, tag_mir as usize).is_some();
    }

    false
}

pub fn maybe_get_item_mir<'a, 'tcx>(cdata: Cmd,
                                    tcx: TyCtxt<'a, 'tcx, 'tcx>,
                                    id: DefIndex)
                                    -> Option<mir::repr::Mir<'tcx>> {
    let item_doc = cdata.lookup_item(id);

    return reader::maybe_get_doc(item_doc, tag_mir as usize).map(|mir_doc| {
        let dcx = tls_context::DecodingContext {
            crate_metadata: cdata,
            tcx: tcx,
        };
        let mut decoder = reader::Decoder::new(mir_doc);

        let mut mir = decoder.read_opaque(|opaque_decoder, _| {
            tls::enter_decoding_context(&dcx, opaque_decoder, |_, opaque_decoder| {
                Decodable::decode(opaque_decoder)
            })
        }).unwrap();

        assert!(decoder.position() == mir_doc.end);

        let mut def_id_and_span_translator = MirDefIdAndSpanTranslator {
            crate_metadata: cdata,
            codemap: tcx.sess.codemap(),
            last_filemap_index_hint: Cell::new(0),
        };

        def_id_and_span_translator.visit_mir(&mut mir);
        for promoted in &mut mir.promoted {
            def_id_and_span_translator.visit_mir(promoted);
        }

        mir
    });

    struct MirDefIdAndSpanTranslator<'cdata, 'codemap> {
        crate_metadata: Cmd<'cdata>,
        codemap: &'codemap codemap::CodeMap,
        last_filemap_index_hint: Cell<usize>
    }

    impl<'v, 'cdata, 'codemap> mir::visit::MutVisitor<'v>
        for MirDefIdAndSpanTranslator<'cdata, 'codemap>
    {
        fn visit_def_id(&mut self, def_id: &mut DefId) {
            *def_id = translate_def_id(self.crate_metadata, *def_id);
        }

        fn visit_span(&mut self, span: &mut Span) {
            *span = translate_span(self.crate_metadata,
                                   self.codemap,
                                   &self.last_filemap_index_hint,
                                   *span);
        }
    }
}

fn get_explicit_self(item: rbml::Doc) -> ty::ExplicitSelfCategory {
    fn get_mutability(ch: u8) -> hir::Mutability {
        match ch as char {
            'i' => hir::MutImmutable,
            'm' => hir::MutMutable,
            _ => bug!("unknown mutability character: `{}`", ch as char),
        }
    }

    let explicit_self_doc = reader::get_doc(item, tag_item_trait_method_explicit_self);
    let string = explicit_self_doc.as_str_slice();

    let explicit_self_kind = string.as_bytes()[0];
    match explicit_self_kind as char {
        's' => ty::ExplicitSelfCategory::Static,
        'v' => ty::ExplicitSelfCategory::ByValue,
        '~' => ty::ExplicitSelfCategory::ByBox,
        // FIXME(#4846) expl. region
        '&' => {
            ty::ExplicitSelfCategory::ByReference(
                ty::ReEmpty,
                get_mutability(string.as_bytes()[1]))
        }
        _ => bug!("unknown self type code: `{}`", explicit_self_kind as char)
    }
}

/// Returns the def IDs of all the items in the given implementation.
pub fn get_impl_items(cdata: Cmd, impl_id: DefIndex)
                      -> Vec<ty::ImplOrTraitItemId> {
    reader::tagged_docs(cdata.lookup_item(impl_id), tag_item_impl_item).map(|doc| {
        let def_id = item_def_id(doc, cdata);
        match item_sort(doc) {
            Some('C') | Some('c') => ty::ConstTraitItemId(def_id),
            Some('r') | Some('p') => ty::MethodTraitItemId(def_id),
            Some('t') => ty::TypeTraitItemId(def_id),
            _ => bug!("unknown impl item sort"),
        }
    }).collect()
}

pub fn get_trait_name(intr: Rc<IdentInterner>,
                      cdata: Cmd,
                      id: DefIndex)
                      -> ast::Name {
    let doc = cdata.lookup_item(id);
    item_name(&intr, doc)
}

pub fn is_static_method(cdata: Cmd, id: DefIndex) -> bool {
    let doc = cdata.lookup_item(id);
    match item_sort(doc) {
        Some('r') | Some('p') => {
            get_explicit_self(doc) == ty::ExplicitSelfCategory::Static
        }
        _ => false
    }
}

pub fn get_impl_or_trait_item<'a, 'tcx>(intr: Rc<IdentInterner>,
                                        cdata: Cmd,
                                        id: DefIndex,
                                        tcx: TyCtxt<'a, 'tcx, 'tcx>)
                                        -> Option<ty::ImplOrTraitItem<'tcx>> {
    let item_doc = cdata.lookup_item(id);

    let def_id = item_def_id(item_doc, cdata);

    let container_id = if let Some(id) = item_parent_item(cdata, item_doc) {
        id
    } else {
        return None;
    };
    let container_doc = cdata.lookup_item(container_id.index);
    let container = match item_family(container_doc) {
        Trait => TraitContainer(container_id),
        _ => ImplContainer(container_id),
    };

    let name = item_name(&intr, item_doc);
    let vis = item_visibility(item_doc);
    let defaultness = item_defaultness(item_doc);

    Some(match item_sort(item_doc) {
        sort @ Some('C') | sort @ Some('c') => {
            let ty = doc_type(item_doc, tcx, cdata);
            ty::ConstTraitItem(Rc::new(ty::AssociatedConst {
                name: name,
                ty: ty,
                vis: vis,
                defaultness: defaultness,
                def_id: def_id,
                container: container,
                has_value: sort == Some('C')
            }))
        }
        Some('r') | Some('p') => {
            let generics = doc_generics(item_doc, tcx, cdata, tag_method_ty_generics);
            let predicates = doc_predicates(item_doc, tcx, cdata, tag_method_ty_generics);
            let ity = tcx.lookup_item_type(def_id).ty;
            let fty = match ity.sty {
                ty::TyFnDef(_, _, fty) => fty,
                _ => bug!(
                    "the type {:?} of the method {:?} is not a function?",
                    ity, name)
            };
            let explicit_self = get_explicit_self(item_doc);

            ty::MethodTraitItem(Rc::new(ty::Method::new(name,
                                                        generics,
                                                        predicates,
                                                        fty,
                                                        explicit_self,
                                                        vis,
                                                        defaultness,
                                                        def_id,
                                                        container)))
        }
        Some('t') => {
            let ty = maybe_doc_type(item_doc, tcx, cdata);
            ty::TypeTraitItem(Rc::new(ty::AssociatedType {
                name: name,
                ty: ty,
                vis: vis,
                defaultness: defaultness,
                def_id: def_id,
                container: container,
            }))
        }
        _ => return None
    })
}

pub fn get_trait_item_def_ids(cdata: Cmd, id: DefIndex)
                              -> Vec<ty::ImplOrTraitItemId> {
    let item = cdata.lookup_item(id);
    reader::tagged_docs(item, tag_item_trait_item).map(|mth| {
        let def_id = item_def_id(mth, cdata);
        match item_sort(mth) {
            Some('C') | Some('c') => ty::ConstTraitItemId(def_id),
            Some('r') | Some('p') => ty::MethodTraitItemId(def_id),
            Some('t') => ty::TypeTraitItemId(def_id),
            _ => bug!("unknown trait item sort"),
        }
    }).collect()
}

pub fn get_item_variances(cdata: Cmd, id: DefIndex) -> ty::ItemVariances {
    let item_doc = cdata.lookup_item(id);
    let variance_doc = reader::get_doc(item_doc, tag_item_variances);
    let mut decoder = reader::Decoder::new(variance_doc);
    Decodable::decode(&mut decoder).unwrap()
}

pub fn get_provided_trait_methods<'a, 'tcx>(intr: Rc<IdentInterner>,
                                            cdata: Cmd,
                                            id: DefIndex,
                                            tcx: TyCtxt<'a, 'tcx, 'tcx>)
                                            -> Vec<Rc<ty::Method<'tcx>>> {
    let item = cdata.lookup_item(id);

    reader::tagged_docs(item, tag_item_trait_item).filter_map(|mth_id| {
        let did = item_def_id(mth_id, cdata);
        let mth = cdata.lookup_item(did.index);

        if item_sort(mth) == Some('p') {
            let trait_item = get_impl_or_trait_item(intr.clone(),
                                                    cdata,
                                                    did.index,
                                                    tcx);
            if let Some(ty::MethodTraitItem(ref method)) = trait_item {
                Some((*method).clone())
            } else {
                None
            }
        } else {
            None
        }
    }).collect()
}

pub fn get_associated_consts<'a, 'tcx>(intr: Rc<IdentInterner>,
                                       cdata: Cmd,
                                       id: DefIndex,
                                       tcx: TyCtxt<'a, 'tcx, 'tcx>)
                                       -> Vec<Rc<ty::AssociatedConst<'tcx>>> {
    let item = cdata.lookup_item(id);

    [tag_item_trait_item, tag_item_impl_item].iter().flat_map(|&tag| {
        reader::tagged_docs(item, tag).filter_map(|ac_id| {
            let did = item_def_id(ac_id, cdata);
            let ac_doc = cdata.lookup_item(did.index);

            match item_sort(ac_doc) {
                Some('C') | Some('c') => {
                    let trait_item = get_impl_or_trait_item(intr.clone(),
                                                            cdata,
                                                            did.index,
                                                            tcx);
                    if let Some(ty::ConstTraitItem(ref ac)) = trait_item {
                        Some((*ac).clone())
                    } else {
                        None
                    }
                }
                _ => None
            }
        })
    }).collect()
}

pub fn get_variant_kind(cdata: Cmd, node_id: DefIndex) -> Option<VariantKind>
{
    let item = cdata.lookup_item(node_id);
    family_to_variant_kind(item_family(item))
}

pub fn get_struct_ctor_def_id(cdata: Cmd, node_id: DefIndex) -> Option<DefId>
{
    let item = cdata.lookup_item(node_id);
    reader::maybe_get_doc(item, tag_items_data_item_struct_ctor).
        map(|ctor_doc| translated_def_id(cdata, ctor_doc))
}

/// If node_id is the constructor of a tuple struct, retrieve the NodeId of
/// the actual type definition, otherwise, return None
pub fn get_tuple_struct_definition_if_ctor(cdata: Cmd,
                                           node_id: DefIndex)
    -> Option<DefId>
{
    let item = cdata.lookup_item(node_id);
    reader::tagged_docs(item, tag_items_data_item_is_tuple_struct_ctor).next().map(|_| {
        item_require_parent_item(cdata, item)
    })
}

pub fn get_item_attrs(cdata: Cmd,
                      orig_node_id: DefIndex)
                      -> Vec<ast::Attribute> {
    // The attributes for a tuple struct are attached to the definition, not the ctor;
    // we assume that someone passing in a tuple struct ctor is actually wanting to
    // look at the definition
    let node_id = get_tuple_struct_definition_if_ctor(cdata, orig_node_id);
    let node_id = node_id.map(|x| x.index).unwrap_or(orig_node_id);
    let item = cdata.lookup_item(node_id);
    get_attributes(item)
}

pub fn get_struct_field_attrs(cdata: Cmd) -> FnvHashMap<DefId, Vec<ast::Attribute>> {
    let data = rbml::Doc::new(cdata.data());
    let fields = reader::get_doc(data, tag_struct_fields);
    reader::tagged_docs(fields, tag_struct_field).map(|field| {
        let def_id = translated_def_id(cdata, reader::get_doc(field, tag_def_id));
        let attrs = get_attributes(field);
        (def_id, attrs)
    }).collect()
}

fn struct_field_family_to_visibility(family: Family) -> ty::Visibility {
    match family {
        PublicField => ty::Visibility::Public,
        InheritedField => ty::Visibility::PrivateExternal,
        _ => bug!()
    }
}

pub fn get_struct_field_names(intr: &IdentInterner, cdata: Cmd, id: DefIndex)
    -> Vec<ast::Name> {
    let item = cdata.lookup_item(id);
    let mut index = 0;
    reader::tagged_docs(item, tag_item_field).map(|an_item| {
        item_name(intr, an_item)
    }).chain(reader::tagged_docs(item, tag_item_unnamed_field).map(|_| {
        let name = intr.intern(&index.to_string());
        index += 1;
        name
    })).collect()
}

fn get_meta_items(md: rbml::Doc) -> Vec<P<ast::MetaItem>> {
    reader::tagged_docs(md, tag_meta_item_word).map(|meta_item_doc| {
        let nd = reader::get_doc(meta_item_doc, tag_meta_item_name);
        let n = token::intern_and_get_ident(nd.as_str_slice());
        attr::mk_word_item(n)
    }).chain(reader::tagged_docs(md, tag_meta_item_name_value).map(|meta_item_doc| {
        let nd = reader::get_doc(meta_item_doc, tag_meta_item_name);
        let vd = reader::get_doc(meta_item_doc, tag_meta_item_value);
        let n = token::intern_and_get_ident(nd.as_str_slice());
        let v = token::intern_and_get_ident(vd.as_str_slice());
        // FIXME (#623): Should be able to decode MetaItemKind::NameValue variants,
        // but currently the encoder just drops them
        attr::mk_name_value_item_str(n, v)
    })).chain(reader::tagged_docs(md, tag_meta_item_list).map(|meta_item_doc| {
        let nd = reader::get_doc(meta_item_doc, tag_meta_item_name);
        let n = token::intern_and_get_ident(nd.as_str_slice());
        let subitems = get_meta_items(meta_item_doc);
        attr::mk_list_item(n, subitems)
    })).collect()
}

fn get_attributes(md: rbml::Doc) -> Vec<ast::Attribute> {
    match reader::maybe_get_doc(md, tag_attributes) {
        Some(attrs_d) => {
            reader::tagged_docs(attrs_d, tag_attribute).map(|attr_doc| {
                let is_sugared_doc = reader::doc_as_u8(
                    reader::get_doc(attr_doc, tag_attribute_is_sugared_doc)
                ) == 1;
                let meta_items = get_meta_items(attr_doc);
                // Currently it's only possible to have a single meta item on
                // an attribute
                assert_eq!(meta_items.len(), 1);
                let meta_item = meta_items.into_iter().nth(0).unwrap();
                codemap::Spanned {
                    node: ast::Attribute_ {
                        id: attr::mk_attr_id(),
                        style: ast::AttrStyle::Outer,
                        value: meta_item,
                        is_sugared_doc: is_sugared_doc,
                    },
                    span: syntax_pos::DUMMY_SP
                }
            }).collect()
        },
        None => vec![],
    }
}

fn list_crate_attributes(md: rbml::Doc, hash: &Svh,
                         out: &mut io::Write) -> io::Result<()> {
    write!(out, "=Crate Attributes ({})=\n", *hash)?;

    let r = get_attributes(md);
    for attr in &r {
        write!(out, "{}\n", pprust::attribute_to_string(attr))?;
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
    pub explicitly_linked: bool,
}

pub fn get_crate_deps(data: &[u8]) -> Vec<CrateDep> {
    let cratedoc = rbml::Doc::new(data);
    let depsdoc = reader::get_doc(cratedoc, tag_crate_deps);

    fn docstr(doc: rbml::Doc, tag_: usize) -> String {
        let d = reader::get_doc(doc, tag_);
        d.as_str_slice().to_string()
    }

    reader::tagged_docs(depsdoc, tag_crate_dep).enumerate().map(|(crate_num, depdoc)| {
        let name = docstr(depdoc, tag_crate_dep_crate_name);
        let hash = Svh::new(reader::doc_as_u64(reader::get_doc(depdoc, tag_crate_dep_hash)));
        let doc = reader::get_doc(depdoc, tag_crate_dep_explicitly_linked);
        let explicitly_linked = reader::doc_as_u8(doc) != 0;
        CrateDep {
            cnum: crate_num as u32 + 1,
            name: name,
            hash: hash,
            explicitly_linked: explicitly_linked,
        }
    }).collect()
}

fn list_crate_deps(data: &[u8], out: &mut io::Write) -> io::Result<()> {
    write!(out, "=External Dependencies=\n")?;
    for dep in &get_crate_deps(data) {
        write!(out, "{} {}-{}\n", dep.cnum, dep.name, dep.hash)?;
    }
    write!(out, "\n")?;
    Ok(())
}

pub fn maybe_get_crate_hash(data: &[u8]) -> Option<Svh> {
    let cratedoc = rbml::Doc::new(data);
    reader::maybe_get_doc(cratedoc, tag_crate_hash).map(|doc| {
        Svh::new(reader::doc_as_u64(doc))
    })
}

pub fn get_crate_hash(data: &[u8]) -> Svh {
    let cratedoc = rbml::Doc::new(data);
    let hashdoc = reader::get_doc(cratedoc, tag_crate_hash);
    Svh::new(reader::doc_as_u64(hashdoc))
}

pub fn maybe_get_crate_name(data: &[u8]) -> Option<&str> {
    let cratedoc = rbml::Doc::new(data);
    reader::maybe_get_doc(cratedoc, tag_crate_crate_name).map(|doc| {
        doc.as_str_slice()
    })
}

pub fn get_crate_disambiguator<'a>(data: &'a [u8]) -> &'a str {
    let crate_doc = rbml::Doc::new(data);
    let disambiguator_doc = reader::get_doc(crate_doc, tag_crate_disambiguator);
    let slice: &'a str = disambiguator_doc.as_str_slice();
    slice
}

pub fn get_crate_triple(data: &[u8]) -> Option<String> {
    let cratedoc = rbml::Doc::new(data);
    let triple_doc = reader::maybe_get_doc(cratedoc, tag_crate_triple);
    triple_doc.map(|s| s.as_str().to_string())
}

pub fn get_crate_name(data: &[u8]) -> &str {
    maybe_get_crate_name(data).expect("no crate name in crate")
}

pub fn list_crate_metadata(bytes: &[u8], out: &mut io::Write) -> io::Result<()> {
    let hash = get_crate_hash(bytes);
    let md = rbml::Doc::new(bytes);
    list_crate_attributes(md, &hash, out)?;
    list_crate_deps(bytes, out)
}

// Translates a def_id from an external crate to a def_id for the current
// compilation environment. We use this when trying to load types from
// external crates - if those types further refer to types in other crates
// then we must translate the crate number from that encoded in the external
// crate to the correct local crate number.
pub fn translate_def_id(cdata: Cmd, did: DefId) -> DefId {
    if did.is_local() {
        return DefId { krate: cdata.cnum, index: did.index };
    }

    match cdata.cnum_map.borrow().get(&did.krate) {
        Some(&n) => {
            DefId {
                krate: n,
                index: did.index,
            }
        }
        None => bug!("didn't find a crate in the cnum_map")
    }
}

// Translate a DefId from the current compilation environment to a DefId
// for an external crate.
fn reverse_translate_def_id(cdata: Cmd, did: DefId) -> Option<DefId> {
    if did.krate == cdata.cnum {
        return Some(DefId { krate: LOCAL_CRATE, index: did.index });
    }

    for (&local, &global) in cdata.cnum_map.borrow().iter() {
        if global == did.krate {
            return Some(DefId { krate: local, index: did.index });
        }
    }

    None
}

/// Translates a `Span` from an extern crate to the corresponding `Span`
/// within the local crate's codemap.
pub fn translate_span(cdata: Cmd,
                      codemap: &codemap::CodeMap,
                      last_filemap_index_hint: &Cell<usize>,
                      span: syntax_pos::Span)
                      -> syntax_pos::Span {
    let span = if span.lo > span.hi {
        // Currently macro expansion sometimes produces invalid Span values
        // where lo > hi. In order not to crash the compiler when trying to
        // translate these values, let's transform them into something we
        // can handle (and which will produce useful debug locations at
        // least some of the time).
        // This workaround is only necessary as long as macro expansion is
        // not fixed. FIXME(#23480)
        syntax_pos::mk_sp(span.lo, span.lo)
    } else {
        span
    };

    let imported_filemaps = cdata.imported_filemaps(&codemap);
    let filemap = {
        // Optimize for the case that most spans within a translated item
        // originate from the same filemap.
        let last_filemap_index = last_filemap_index_hint.get();
        let last_filemap = &imported_filemaps[last_filemap_index];

        if span.lo >= last_filemap.original_start_pos &&
           span.lo <= last_filemap.original_end_pos &&
           span.hi >= last_filemap.original_start_pos &&
           span.hi <= last_filemap.original_end_pos {
            last_filemap
        } else {
            let mut a = 0;
            let mut b = imported_filemaps.len();

            while b - a > 1 {
                let m = (a + b) / 2;
                if imported_filemaps[m].original_start_pos > span.lo {
                    b = m;
                } else {
                    a = m;
                }
            }

            last_filemap_index_hint.set(a);
            &imported_filemaps[a]
        }
    };

    let lo = (span.lo - filemap.original_start_pos) +
              filemap.translated_filemap.start_pos;
    let hi = (span.hi - filemap.original_start_pos) +
              filemap.translated_filemap.start_pos;

    syntax_pos::mk_sp(lo, hi)
}

pub fn each_inherent_implementation_for_type<F>(cdata: Cmd,
                                                id: DefIndex,
                                                mut callback: F)
    where F: FnMut(DefId),
{
    let item_doc = cdata.lookup_item(id);
    for impl_doc in reader::tagged_docs(item_doc, tag_items_data_item_inherent_impl) {
        if reader::maybe_get_doc(impl_doc, tag_item_trait_ref).is_none() {
            callback(item_def_id(impl_doc, cdata));
        }
    }
}

pub fn each_implementation_for_trait<F>(cdata: Cmd,
                                        def_id: DefId,
                                        mut callback: F) where
    F: FnMut(DefId),
{
    // Do a reverse lookup beforehand to avoid touching the crate_num
    // hash map in the loop below.
    if let Some(crate_local_did) = reverse_translate_def_id(cdata, def_id) {
        let def_id_u64 = def_to_u64(crate_local_did);

        let impls_doc = reader::get_doc(rbml::Doc::new(cdata.data()), tag_impls);
        for trait_doc in reader::tagged_docs(impls_doc, tag_impls_trait) {
            let trait_def_id = reader::get_doc(trait_doc, tag_def_id);
            if reader::doc_as_u64(trait_def_id) != def_id_u64 {
                continue;
            }
            for impl_doc in reader::tagged_docs(trait_doc, tag_impls_trait_impl) {
                callback(translated_def_id(cdata, impl_doc));
            }
        }
    }
}

pub fn get_trait_of_item<'a, 'tcx>(cdata: Cmd,
                                   id: DefIndex,
                                   tcx: TyCtxt<'a, 'tcx, 'tcx>)
                                   -> Option<DefId> {
    let item_doc = cdata.lookup_item(id);
    let parent_item_id = match item_parent_item(cdata, item_doc) {
        None => return None,
        Some(item_id) => item_id,
    };
    let parent_item_doc = cdata.lookup_item(parent_item_id.index);
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
    reader::tagged_docs(libraries, tag_native_libraries_lib).map(|lib_doc| {
        let kind_doc = reader::get_doc(lib_doc, tag_native_libraries_kind);
        let name_doc = reader::get_doc(lib_doc, tag_native_libraries_name);
        let kind: cstore::NativeLibraryKind =
            cstore::NativeLibraryKind::from_u32(reader::doc_as_u32(kind_doc)).unwrap();
        let name = name_doc.as_str().to_string();
        (kind, name)
    }).collect()
}

pub fn get_plugin_registrar_fn(data: &[u8]) -> Option<DefIndex> {
    reader::maybe_get_doc(rbml::Doc::new(data), tag_plugin_registrar_fn)
        .map(|doc| DefIndex::from_u32(reader::doc_as_u32(doc)))
}

pub fn each_exported_macro<F>(data: &[u8], intr: &IdentInterner, mut f: F) where
    F: FnMut(ast::Name, Vec<ast::Attribute>, Span, String) -> bool,
{
    let macros = reader::get_doc(rbml::Doc::new(data), tag_macro_defs);
    for macro_doc in reader::tagged_docs(macros, tag_macro_def) {
        let name = item_name(intr, macro_doc);
        let attrs = get_attributes(macro_doc);
        let span = get_macro_span(macro_doc);
        let body = reader::get_doc(macro_doc, tag_macro_def_body);
        if !f(name, attrs, span, body.as_str().to_string()) {
            break;
        }
    }
}

pub fn get_macro_span(doc: rbml::Doc) -> Span {
    let lo_doc = reader::get_doc(doc, tag_macro_def_span_lo);
    let lo = BytePos(reader::doc_as_u32(lo_doc));
    let hi_doc = reader::get_doc(doc, tag_macro_def_span_hi);
    let hi = BytePos(reader::doc_as_u32(hi_doc));
    return Span { lo: lo, hi: hi, expn_id: NO_EXPANSION };
}

pub fn get_dylib_dependency_formats(cdata: Cmd)
    -> Vec<(ast::CrateNum, LinkagePreference)>
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
        let cnum = match cdata.cnum_map.borrow().get(&cnum) {
            Some(&n) => n,
            None => bug!("didn't find a crate in the cnum_map")
        };
        result.push((cnum, if link == "d" {
            LinkagePreference::RequireDynamic
        } else {
            LinkagePreference::RequireStatic
        }));
    }
    return result;
}

pub fn get_missing_lang_items(cdata: Cmd)
    -> Vec<lang_items::LangItem>
{
    let items = reader::get_doc(rbml::Doc::new(cdata.data()), tag_lang_items);
    reader::tagged_docs(items, tag_lang_items_missing).map(|missing_docs| {
        lang_items::LangItem::from_u32(reader::doc_as_u32(missing_docs)).unwrap()
    }).collect()
}

pub fn get_method_arg_names(cdata: Cmd, id: DefIndex) -> Vec<String> {
    let method_doc = cdata.lookup_item(id);
    match reader::maybe_get_doc(method_doc, tag_method_argument_names) {
        Some(args_doc) => {
            reader::tagged_docs(args_doc, tag_method_argument_name).map(|name_doc| {
                name_doc.as_str_slice().to_string()
            }).collect()
        },
        None => vec![],
    }
}

pub fn get_reachable_ids(cdata: Cmd) -> Vec<DefId> {
    let items = reader::get_doc(rbml::Doc::new(cdata.data()),
                                tag_reachable_ids);
    reader::tagged_docs(items, tag_reachable_id).map(|doc| {
        DefId {
            krate: cdata.cnum,
            index: DefIndex::from_u32(reader::doc_as_u32(doc)),
        }
    }).collect()
}

pub fn is_typedef(cdata: Cmd, id: DefIndex) -> bool {
    let item_doc = cdata.lookup_item(id);
    match item_family(item_doc) {
        Type => true,
        _ => false,
    }
}

pub fn is_const_fn(cdata: Cmd, id: DefIndex) -> bool {
    let item_doc = cdata.lookup_item(id);
    match fn_constness(item_doc) {
        hir::Constness::Const => true,
        hir::Constness::NotConst => false,
    }
}

pub fn is_extern_item<'a, 'tcx>(cdata: Cmd,
                                id: DefIndex,
                                tcx: TyCtxt<'a, 'tcx, 'tcx>)
                                -> bool {
    let item_doc = match cdata.get_item(id) {
        Some(doc) => doc,
        None => return false,
    };
    let applicable = match item_family(item_doc) {
        ImmStatic | MutStatic => true,
        Fn => {
            let ty::TypeScheme { generics, ty } = get_type(cdata, id, tcx);
            let no_generics = generics.types.is_empty();
            match ty.sty {
                ty::TyFnDef(_, _, fn_ty) | ty::TyFnPtr(fn_ty)
                    if fn_ty.abi != Abi::Rust => return no_generics,
                _ => no_generics,
            }
        },
        _ => false,
    };

    if applicable {
        attr::contains_extern_indicator(tcx.sess.diagnostic(),
                                        &get_attributes(item_doc))
    } else {
        false
    }
}

pub fn is_foreign_item(cdata: Cmd, id: DefIndex) -> bool {
    let item_doc = cdata.lookup_item(id);
    let parent_item_id = match item_parent_item(cdata, item_doc) {
        None => return false,
        Some(item_id) => item_id,
    };
    let parent_item_doc = cdata.lookup_item(parent_item_id.index);
    item_family(parent_item_doc) == ForeignMod
}

pub fn is_impl(cdata: Cmd, id: DefIndex) -> bool {
    let item_doc = cdata.lookup_item(id);
    match item_family(item_doc) {
        Impl => true,
        _ => false,
    }
}

fn doc_generics<'a, 'tcx>(base_doc: rbml::Doc,
                          tcx: TyCtxt<'a, 'tcx, 'tcx>,
                          cdata: Cmd,
                          tag: usize)
                          -> ty::Generics<'tcx>
{
    let doc = reader::get_doc(base_doc, tag);

    let mut types = subst::VecPerParamSpace::empty();
    for p in reader::tagged_docs(doc, tag_type_param_def) {
        let bd =
            TyDecoder::with_doc(tcx, cdata.cnum, p,
                                &mut |did| translate_def_id(cdata, did))
            .parse_type_param_def();
        types.push(bd.space, bd);
    }

    let mut regions = subst::VecPerParamSpace::empty();
    for p in reader::tagged_docs(doc, tag_region_param_def) {
        let bd =
            TyDecoder::with_doc(tcx, cdata.cnum, p,
                                &mut |did| translate_def_id(cdata, did))
            .parse_region_param_def();
        regions.push(bd.space, bd);
    }

    ty::Generics { types: types, regions: regions }
}

fn doc_predicate<'a, 'tcx>(cdata: Cmd,
                           doc: rbml::Doc,
                           tcx: TyCtxt<'a, 'tcx, 'tcx>)
                           -> ty::Predicate<'tcx>
{
    let predicate_pos = cdata.xref_index.lookup(
        cdata.data(), reader::doc_as_u32(doc)).unwrap() as usize;
    TyDecoder::new(
        cdata.data(), cdata.cnum, predicate_pos, tcx,
        &mut |did| translate_def_id(cdata, did)
    ).parse_predicate()
}

fn doc_predicates<'a, 'tcx>(base_doc: rbml::Doc,
                            tcx: TyCtxt<'a, 'tcx, 'tcx>,
                            cdata: Cmd,
                            tag: usize)
                            -> ty::GenericPredicates<'tcx>
{
    let doc = reader::get_doc(base_doc, tag);

    let mut predicates = subst::VecPerParamSpace::empty();
    for predicate_doc in reader::tagged_docs(doc, tag_type_predicate) {
        predicates.push(subst::TypeSpace,
                        doc_predicate(cdata, predicate_doc, tcx));
    }
    for predicate_doc in reader::tagged_docs(doc, tag_self_predicate) {
        predicates.push(subst::SelfSpace,
                        doc_predicate(cdata, predicate_doc, tcx));
    }
    for predicate_doc in reader::tagged_docs(doc, tag_fn_predicate) {
        predicates.push(subst::FnSpace,
                        doc_predicate(cdata, predicate_doc, tcx));
    }

    ty::GenericPredicates { predicates: predicates }
}

pub fn is_defaulted_trait(cdata: Cmd, trait_id: DefIndex) -> bool {
    let trait_doc = cdata.lookup_item(trait_id);
    assert!(item_family(trait_doc) == Family::Trait);
    let defaulted_doc = reader::get_doc(trait_doc, tag_defaulted_trait);
    reader::doc_as_u8(defaulted_doc) != 0
}

pub fn is_default_impl(cdata: Cmd, impl_id: DefIndex) -> bool {
    let impl_doc = cdata.lookup_item(impl_id);
    item_family(impl_doc) == Family::DefaultImpl
}

pub fn get_imported_filemaps(metadata: &[u8]) -> Vec<syntax_pos::FileMap> {
    let crate_doc = rbml::Doc::new(metadata);
    let cm_doc = reader::get_doc(crate_doc, tag_codemap);

    reader::tagged_docs(cm_doc, tag_codemap_filemap).map(|filemap_doc| {
        let mut decoder = reader::Decoder::new(filemap_doc);
        decoder.read_opaque(|opaque_decoder, _| {
            Decodable::decode(opaque_decoder)
        }).unwrap()
    }).collect()
}

pub fn closure_kind(cdata: Cmd, closure_id: DefIndex) -> ty::ClosureKind {
    let closure_doc = cdata.lookup_item(closure_id);
    let closure_kind_doc = reader::get_doc(closure_doc, tag_items_closure_kind);
    let mut decoder = reader::Decoder::new(closure_kind_doc);
    ty::ClosureKind::decode(&mut decoder).unwrap()
}

pub fn closure_ty<'a, 'tcx>(cdata: Cmd, closure_id: DefIndex, tcx: TyCtxt<'a, 'tcx, 'tcx>)
                            -> ty::ClosureTy<'tcx> {
    let closure_doc = cdata.lookup_item(closure_id);
    let closure_ty_doc = reader::get_doc(closure_doc, tag_items_closure_ty);
    TyDecoder::with_doc(tcx, cdata.cnum, closure_ty_doc, &mut |did| translate_def_id(cdata, did))
        .parse_closure_ty()
}

pub fn def_key(cdata: Cmd, id: DefIndex) -> hir_map::DefKey {
    debug!("def_key: id={:?}", id);
    let item_doc = cdata.lookup_item(id);
    item_def_key(item_doc)
}

fn item_def_key(item_doc: rbml::Doc) -> hir_map::DefKey {
    match reader::maybe_get_doc(item_doc, tag_def_key) {
        Some(def_key_doc) => {
            let mut decoder = reader::Decoder::new(def_key_doc);
            let simple_key = def_key::DefKey::decode(&mut decoder).unwrap();
            let name = reader::maybe_get_doc(item_doc, tag_paths_data_name).map(|name| {
                token::intern(name.as_str_slice())
            });
            def_key::recover_def_key(simple_key, name)
        }
        None => {
            bug!("failed to find block with tag {:?} for item with family {:?}",
                   tag_def_key,
                   item_family(item_doc))
        }
    }
}

pub fn def_path(cdata: Cmd, id: DefIndex) -> hir_map::DefPath {
    debug!("def_path(id={:?})", id);
    hir_map::DefPath::make(cdata.cnum, id, |parent| def_key(cdata, parent))
}

pub fn get_panic_strategy(data: &[u8]) -> PanicStrategy {
    let crate_doc = rbml::Doc::new(data);
    let strat_doc = reader::get_doc(crate_doc, tag_panic_strategy);
    match reader::doc_as_u8(strat_doc) {
        b'U' => PanicStrategy::Unwind,
        b'A' => PanicStrategy::Abort,
        b => panic!("unknown panic strategy in metadata: {}", b),
    }
}
