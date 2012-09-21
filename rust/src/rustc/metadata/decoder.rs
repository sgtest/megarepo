// Decoding metadata from a single crate's metadata

use std::{ebml, map};
use std::map::HashMap;
use io::WriterUtil;
use dvec::DVec;
use syntax::{ast, ast_util};
use syntax::attr;
use middle::ty;
use syntax::ast_map;
use tydecode::{parse_ty_data, parse_def_id, parse_bounds_data,
        parse_ident};
use syntax::print::pprust;
use cmd=cstore::crate_metadata;
use util::ppaux::ty_to_str;
use syntax::diagnostic::span_handler;
use common::*;
use syntax::parse::token::ident_interner;
use hash::{Hash, HashUtil};

export class_dtor;
export get_class_fields;
export get_symbol;
export get_enum_variants;
export get_type;
export get_region_param;
export get_type_param_count;
export get_impl_traits;
export get_class_method;
export get_impl_method;
export lookup_def;
export resolve_path;
export get_crate_attributes;
export list_crate_metadata;
export crate_dep;
export get_crate_deps;
export get_crate_hash;
export get_crate_vers;
export get_impls_for_mod;
export get_trait_methods;
export get_method_names_if_trait;
export get_item_attrs;
export get_crate_module_paths;
export def_like;
export dl_def;
export dl_impl;
export dl_field;
export path_entry;
export each_path;
export get_item_path;
export maybe_find_item; // sketchy
export item_type; // sketchy
export maybe_get_item_ast;
export decode_inlined_item;
export method_info, _impl;

// Used internally by astencode:
export translate_def_id;

// A function that takes a def_id relative to the crate being searched and
// returns a def_id relative to the compilation environment, i.e. if we hit a
// def_id for an item defined in another crate, somebody needs to figure out
// what crate that's in and give us a def_id that makes sense for the current
// build.

fn lookup_hash(d: ebml::Doc, eq_fn: fn(x:&[u8]) -> bool, hash: uint) ->
   Option<ebml::Doc> {
    let index = ebml::get_doc(d, tag_index);
    let table = ebml::get_doc(index, tag_index_table);
    let hash_pos = table.start + hash % 256u * 4u;
    let pos = io::u64_from_be_bytes(*d.data, hash_pos, 4u) as uint;
    let {tag:_, doc:bucket} = ebml::doc_at(d.data, pos);

    let belt = tag_index_buckets_bucket_elt;
    for ebml::tagged_docs(bucket, belt) |elt| {
        let pos = io::u64_from_be_bytes(*elt.data, elt.start, 4u) as uint;
        if eq_fn(vec::view(*elt.data, elt.start + 4u, elt.end)) {
            return Some(ebml::doc_at(d.data, pos).doc);
        }
    };
    None
}

fn maybe_find_item(item_id: int, items: ebml::Doc) -> Option<ebml::Doc> {
    fn eq_item(bytes: &[u8], item_id: int) -> bool {
        return io::u64_from_be_bytes(vec::view(bytes, 0u, 4u), 0u, 4u) as int
            == item_id;
    }
    lookup_hash(items,
                |a| eq_item(a, item_id),
                item_id.hash() as uint)
}

fn find_item(item_id: int, items: ebml::Doc) -> ebml::Doc {
    return option::get(maybe_find_item(item_id, items));
}

// Looks up an item in the given metadata and returns an ebml doc pointing
// to the item data.
fn lookup_item(item_id: int, data: @~[u8]) -> ebml::Doc {
    let items = ebml::get_doc(ebml::Doc(data), tag_items);
    match maybe_find_item(item_id, items) {
       None => fail(fmt!("lookup_item: id not found: %d", item_id)),
       Some(d) => d
    }
}

enum Family {
    Const,                 // c
    Fn,                    // f
    UnsafeFn,              // u
    PureFn,                // p
    StaticMethod,          // F
    UnsafeStaticMethod,    // U
    PureStaticMethod,      // P
    ForeignFn,             // e
    Type,                  // y
    ForeignType,           // T
    Mod,                   // m
    ForeignMod,            // n
    Enum,                  // t
    Variant,               // v
    Impl,                  // i
    Trait,                 // I
    Class,                 // C
    Struct,                // S
    PublicField,           // g
    PrivateField,          // j
    InheritedField         // N
}

#[cfg(stage0)]
impl Family : cmp::Eq {
    pure fn eq(&&other: Family) -> bool {
        (self as uint) == (other as uint)
    }
    pure fn ne(&&other: Family) -> bool { !self.eq(other) }
}
#[cfg(stage1)]
#[cfg(stage2)]
impl Family : cmp::Eq {
    pure fn eq(other: &Family) -> bool {
        (self as uint) == ((*other) as uint)
    }
    pure fn ne(other: &Family) -> bool { !self.eq(other) }
}

fn item_family(item: ebml::Doc) -> Family {
    let fam = ebml::get_doc(item, tag_items_data_item_family);
    match ebml::doc_as_u8(fam) as char {
      'c' => Const,
      'f' => Fn,
      'u' => UnsafeFn,
      'p' => PureFn,
      'F' => StaticMethod,
      'U' => UnsafeStaticMethod,
      'P' => PureStaticMethod,
      'e' => ForeignFn,
      'y' => Type,
      'T' => ForeignType,
      'm' => Mod,
      'n' => ForeignMod,
      't' => Enum,
      'v' => Variant,
      'i' => Impl,
      'I' => Trait,
      'C' => Class,
      'S' => Struct,
      'g' => PublicField,
      'j' => PrivateField,
      'N' => InheritedField,
       c => fail (#fmt("unexpected family char: %c", c))
    }
}

fn item_symbol(item: ebml::Doc) -> ~str {
    let sym = ebml::get_doc(item, tag_items_data_item_symbol);
    return str::from_bytes(ebml::doc_data(sym));
}

fn item_parent_item(d: ebml::Doc) -> Option<ast::def_id> {
    for ebml::tagged_docs(d, tag_items_data_parent_item) |did| {
        return Some(ebml::with_doc_data(did, |d| parse_def_id(d)));
    }
    None
}

fn item_def_id(d: ebml::Doc, cdata: cmd) -> ast::def_id {
    let tagdoc = ebml::get_doc(d, tag_def_id);
    return translate_def_id(cdata, ebml::with_doc_data(tagdoc,
                                                    |d| parse_def_id(d)));
}

fn each_reexport(d: ebml::Doc, f: fn(ebml::Doc) -> bool) {
    for ebml::tagged_docs(d, tag_items_data_item_reexport) |reexport_doc| {
        if !f(reexport_doc) {
            return;
        }
    }
}

fn field_mutability(d: ebml::Doc) -> ast::class_mutability {
    // Use maybe_get_doc in case it's a method
    option::map_default(
        ebml::maybe_get_doc(d, tag_class_mut),
        ast::class_immutable,
        |d| {
            match ebml::doc_as_u8(d) as char {
              'm' => ast::class_mutable,
              _   => ast::class_immutable
            }
        })
}

fn variant_disr_val(d: ebml::Doc) -> Option<int> {
    do option::chain(ebml::maybe_get_doc(d, tag_disr_val)) |val_doc| {
        int::parse_bytes(ebml::doc_data(val_doc), 10u)
    }
}

fn doc_type(doc: ebml::Doc, tcx: ty::ctxt, cdata: cmd) -> ty::t {
    let tp = ebml::get_doc(doc, tag_items_data_item_type);
    parse_ty_data(tp.data, cdata.cnum, tp.start, tcx, |did| {
        translate_def_id(cdata, did)
    })
}

fn item_type(item_id: ast::def_id, item: ebml::Doc,
             tcx: ty::ctxt, cdata: cmd) -> ty::t {
    let t = doc_type(item, tcx, cdata);
    if family_names_type(item_family(item)) {
        ty::mk_with_id(tcx, t, item_id)
    } else { t }
}

fn item_impl_traits(item: ebml::Doc, tcx: ty::ctxt, cdata: cmd) -> ~[ty::t] {
    let mut results = ~[];
    for ebml::tagged_docs(item, tag_impl_trait) |ity| {
        vec::push(results, doc_type(ity, tcx, cdata));
    };
    results
}

fn item_ty_param_bounds(item: ebml::Doc, tcx: ty::ctxt, cdata: cmd)
    -> @~[ty::param_bounds] {
    let mut bounds = ~[];
    for ebml::tagged_docs(item, tag_items_data_item_ty_param_bounds) |p| {
        let bd = parse_bounds_data(p.data, p.start, cdata.cnum, tcx, |did| {
            translate_def_id(cdata, did)
        });
        vec::push(bounds, bd);
    }
    @bounds
}

fn item_ty_region_param(item: ebml::Doc) -> Option<ty::region_variance> {
    ebml::maybe_get_doc(item, tag_region_param).map(|doc| {
        let d = ebml::ebml_deserializer(doc);
        ty::deserialize_region_variance(d)
    })
}

fn item_ty_param_count(item: ebml::Doc) -> uint {
    let mut n = 0u;
    ebml::tagged_docs(item, tag_items_data_item_ty_param_bounds,
                      |_p| { n += 1u; true } );
    n
}

fn enum_variant_ids(item: ebml::Doc, cdata: cmd) -> ~[ast::def_id] {
    let mut ids: ~[ast::def_id] = ~[];
    let v = tag_items_data_item_variant;
    for ebml::tagged_docs(item, v) |p| {
        let ext = ebml::with_doc_data(p, |d| parse_def_id(d));
        vec::push(ids, {crate: cdata.cnum, node: ext.node});
    };
    return ids;
}

fn item_path(intr: ident_interner, item_doc: ebml::Doc) -> ast_map::path {
    let path_doc = ebml::get_doc(item_doc, tag_path);

    let len_doc = ebml::get_doc(path_doc, tag_path_len);
    let len = ebml::doc_as_u32(len_doc) as uint;

    let mut result = ~[];
    vec::reserve(result, len);

    for ebml::docs(path_doc) |tag, elt_doc| {
        if tag == tag_path_elt_mod {
            let str = ebml::doc_as_str(elt_doc);
            vec::push(result, ast_map::path_mod(intr.intern(@str)));
        } else if tag == tag_path_elt_name {
            let str = ebml::doc_as_str(elt_doc);
            vec::push(result, ast_map::path_name(intr.intern(@str)));
        } else {
            // ignore tag_path_len element
        }
    }

    return result;
}

fn item_name(intr: ident_interner, item: ebml::Doc) -> ast::ident {
    let name = ebml::get_doc(item, tag_paths_data_name);
    intr.intern(@str::from_bytes(ebml::doc_data(name)))
}

fn item_to_def_like(item: ebml::Doc, did: ast::def_id, cnum: ast::crate_num)
        -> def_like {
    let fam = item_family(item);
    match fam {
      Const     => dl_def(ast::def_const(did)),
      Class     => dl_def(ast::def_class(did, true)),
      Struct    => dl_def(ast::def_class(did, false)),
      UnsafeFn  => dl_def(ast::def_fn(did, ast::unsafe_fn)),
      Fn        => dl_def(ast::def_fn(did, ast::impure_fn)),
      PureFn    => dl_def(ast::def_fn(did, ast::pure_fn)),
      ForeignFn => dl_def(ast::def_fn(did, ast::extern_fn)),
      UnsafeStaticMethod => dl_def(ast::def_static_method(did,
                                                          ast::unsafe_fn)),
      StaticMethod => dl_def(ast::def_static_method(did, ast::impure_fn)),
      PureStaticMethod => dl_def(ast::def_static_method(did, ast::pure_fn)),
      Type | ForeignType => dl_def(ast::def_ty(did)),
      Mod => dl_def(ast::def_mod(did)),
      ForeignMod => dl_def(ast::def_foreign_mod(did)),
      Variant => {
          match item_parent_item(item) {
              Some(t) => {
                let tid = {crate: cnum, node: t.node};
                dl_def(ast::def_variant(tid, did))
              }
              None => fail ~"item_to_def_like: enum item has no parent"
          }
      }
      Trait | Enum => dl_def(ast::def_ty(did)),
      Impl => dl_impl(did),
      PublicField | PrivateField | InheritedField => dl_field,
    }
}

fn lookup_def(cnum: ast::crate_num, data: @~[u8], did_: ast::def_id) ->
   ast::def {
    let item = lookup_item(did_.node, data);
    let did = {crate: cnum, node: did_.node};
    // We treat references to enums as references to types.
    return def_like_to_def(item_to_def_like(item, did, cnum));
}

fn get_type(cdata: cmd, id: ast::node_id, tcx: ty::ctxt)
    -> ty::ty_param_bounds_and_ty {

    let item = lookup_item(id, cdata.data);
    let t = item_type({crate: cdata.cnum, node: id}, item, tcx, cdata);
    let tp_bounds = if family_has_type_params(item_family(item)) {
        item_ty_param_bounds(item, tcx, cdata)
    } else { @~[] };
    let rp = item_ty_region_param(item);
    return {bounds: tp_bounds,
            region_param: rp,
            ty: t};
}

fn get_region_param(cdata: cmd, id: ast::node_id)
    -> Option<ty::region_variance> {

    let item = lookup_item(id, cdata.data);
    return item_ty_region_param(item);
}

fn get_type_param_count(data: @~[u8], id: ast::node_id) -> uint {
    item_ty_param_count(lookup_item(id, data))
}

fn get_impl_traits(cdata: cmd, id: ast::node_id, tcx: ty::ctxt) -> ~[ty::t] {
    item_impl_traits(lookup_item(id, cdata.data), tcx, cdata)
}

fn get_impl_method(intr: ident_interner, cdata: cmd, id: ast::node_id,
                   name: ast::ident) -> ast::def_id {
    let items = ebml::get_doc(ebml::Doc(cdata.data), tag_items);
    let mut found = None;
    for ebml::tagged_docs(find_item(id, items), tag_item_impl_method) |mid| {
        let m_did = ebml::with_doc_data(mid, |d| parse_def_id(d));
        if item_name(intr, find_item(m_did.node, items)) == name {
            found = Some(translate_def_id(cdata, m_did));
        }
    }
    option::get(found)
}

fn get_class_method(intr: ident_interner, cdata: cmd, id: ast::node_id,
                    name: ast::ident) -> ast::def_id {
    let items = ebml::get_doc(ebml::Doc(cdata.data), tag_items);
    let mut found = None;
    let cls_items = match maybe_find_item(id, items) {
      Some(it) => it,
      None => fail (fmt!("get_class_method: class id not found \
                              when looking up method %s", *intr.get(name)))
    };
    for ebml::tagged_docs(cls_items, tag_item_trait_method) |mid| {
        let m_did = item_def_id(mid, cdata);
        if item_name(intr, mid) == name {
            found = Some(m_did);
        }
    }
    match found {
      Some(found) => found,
      None => fail (fmt!("get_class_method: no method named %s",
                         *intr.get(name)))
    }
}

fn class_dtor(cdata: cmd, id: ast::node_id) -> Option<ast::def_id> {
    let items = ebml::get_doc(ebml::Doc(cdata.data), tag_items);
    let mut found = None;
    let cls_items = match maybe_find_item(id, items) {
            Some(it) => it,
            None     => fail (fmt!("class_dtor: class id not found \
              when looking up dtor for %d", id))
    };
    for ebml::tagged_docs(cls_items, tag_item_dtor) |doc| {
         let doc1 = ebml::get_doc(doc, tag_def_id);
         let did = ebml::with_doc_data(doc1, |d| parse_def_id(d));
         found = Some(translate_def_id(cdata, did));
    };
    found
}

fn get_symbol(data: @~[u8], id: ast::node_id) -> ~str {
    return item_symbol(lookup_item(id, data));
}

// Something that a name can resolve to.
enum def_like {
    dl_def(ast::def),
    dl_impl(ast::def_id),
    dl_field
}

fn def_like_to_def(def_like: def_like) -> ast::def {
    match def_like {
        dl_def(def) => return def,
        dl_impl(*) => fail ~"found impl in def_like_to_def",
        dl_field => fail ~"found field in def_like_to_def"
    }
}

// A path.
struct path_entry {
    // The full path, separated by '::'.
    path_string: ~str,
    // The definition, implementation, or field that this path corresponds to.
    def_like: def_like,
}

fn path_entry(path_string: ~str, def_like: def_like) -> path_entry {
    path_entry {
        path_string: path_string,
        def_like: def_like
    }
}

/// Iterates over all the paths in the given crate.
fn each_path(intr: ident_interner, cdata: cmd, f: fn(path_entry) -> bool) {
    let root = ebml::Doc(cdata.data);
    let items = ebml::get_doc(root, tag_items);
    let items_data = ebml::get_doc(items, tag_items_data);

    let mut broken = false;

    // First, go through all the explicit items.
    for ebml::tagged_docs(items_data, tag_items_data_item) |item_doc| {
        if !broken {
            let path = ast_map::path_to_str_with_sep(
                item_path(intr, item_doc), ~"::", intr);
            if path != ~"" {
                // Extract the def ID.
                let def_id = item_def_id(item_doc, cdata);

                // Construct the def for this item.
                debug!("(each_path) yielding explicit item: %s", path);
                let def_like = item_to_def_like(item_doc, def_id, cdata.cnum);

                // Hand the information off to the iteratee.
                let this_path_entry = path_entry(path, def_like);
                if !f(this_path_entry) {
                    broken = true;      // XXX: This is awful.
                }
            }

            // If this is a module, find the reexports.
            for each_reexport(item_doc) |reexport_doc| {
                if !broken {
                    let def_id_doc =
                        ebml::get_doc(reexport_doc,
                            tag_items_data_item_reexport_def_id);
                    let def_id =
                        ebml::with_doc_data(def_id_doc, |d| parse_def_id(d));
                    let def_id = translate_def_id(cdata, def_id);

                    let reexport_name_doc =
                        ebml::get_doc(reexport_doc,
                                      tag_items_data_item_reexport_name);
                    let reexport_name = ebml::doc_as_str(reexport_name_doc);

                    let reexport_path;
                    if path == ~"" {
                        reexport_path = reexport_name;
                    } else {
                        reexport_path = path + ~"::" + reexport_name;
                    }

                    // Get the item.
                    match maybe_find_item(def_id.node, items) {
                        None => {}
                        Some(item_doc) => {
                            // Construct the def for this item.
                            let def_like = item_to_def_like(item_doc,
                                                            def_id,
                                                            cdata.cnum);

                            // Hand the information off to the iteratee.
                            debug!("(each_path) yielding reexported \
                                    item: %s", reexport_path);

                            let this_path_entry =
                                path_entry(reexport_path, def_like);
                            if (!f(this_path_entry)) {
                                broken = true;  // XXX: This is awful.
                            }
                        }
                    }
                }
            }
        }
    }

    // If broken, stop here.
    if broken {
        return;
    }
}

fn get_item_path(intr: ident_interner, cdata: cmd, id: ast::node_id)
    -> ast_map::path {
    item_path(intr, lookup_item(id, cdata.data))
}

type decode_inlined_item = fn(
    cdata: cstore::crate_metadata,
    tcx: ty::ctxt,
    path: ast_map::path,
    par_doc: ebml::Doc) -> Option<ast::inlined_item>;

fn maybe_get_item_ast(intr: ident_interner, cdata: cmd, tcx: ty::ctxt,
                      id: ast::node_id,
                      decode_inlined_item: decode_inlined_item
                     ) -> csearch::found_ast {
    debug!("Looking up item: %d", id);
    let item_doc = lookup_item(id, cdata.data);
    let path = vec::init(item_path(intr, item_doc));
    match decode_inlined_item(cdata, tcx, path, item_doc) {
      Some(ii) => csearch::found(ii),
      None => {
        match item_parent_item(item_doc) {
          Some(did) => {
            let did = translate_def_id(cdata, did);
            let parent_item = lookup_item(did.node, cdata.data);
            match decode_inlined_item(cdata, tcx, path,
                                               parent_item) {
              Some(ii) => csearch::found_parent(did, ii),
              None => csearch::not_found
            }
          }
          None => csearch::not_found
        }
      }
    }
}

fn get_enum_variants(intr: ident_interner, cdata: cmd, id: ast::node_id,
                     tcx: ty::ctxt) -> ~[ty::variant_info] {
    let data = cdata.data;
    let items = ebml::get_doc(ebml::Doc(data), tag_items);
    let item = find_item(id, items);
    let mut infos: ~[ty::variant_info] = ~[];
    let variant_ids = enum_variant_ids(item, cdata);
    let mut disr_val = 0;
    for variant_ids.each |did| {
        let item = find_item(did.node, items);
        let ctor_ty = item_type({crate: cdata.cnum, node: id}, item,
                                tcx, cdata);
        let name = item_name(intr, item);
        let mut arg_tys: ~[ty::t] = ~[];
        match ty::get(ctor_ty).sty {
          ty::ty_fn(f) => {
            for f.sig.inputs.each |a| { vec::push(arg_tys, a.ty); }
          }
          _ => { /* Nullary enum variant. */ }
        }
        match variant_disr_val(item) {
          Some(val) => { disr_val = val; }
          _         => { /* empty */ }
        }
        vec::push(infos, @{args: arg_tys, ctor_ty: ctor_ty, name: name,
                           id: *did, disr_val: disr_val});
        disr_val += 1;
    }
    return infos;
}

// NB: These types are duplicated in resolve.rs
type method_info = {
    did: ast::def_id,
    n_tps: uint,
    ident: ast::ident,
    self_type: ast::self_ty_
};

type _impl = {did: ast::def_id, ident: ast::ident, methods: ~[@method_info]};

fn get_self_ty(item: ebml::Doc) -> ast::self_ty_ {
    fn get_mutability(ch: u8) -> ast::mutability {
        match ch as char {
            'i' => { ast::m_imm }
            'm' => { ast::m_mutbl }
            'c' => { ast::m_const }
            _ => {
                fail fmt!("unknown mutability character: `%c`", ch as char)
            }
        }
    }

    let self_type_doc = ebml::get_doc(item, tag_item_trait_method_self_ty);
    let string = ebml::doc_as_str(self_type_doc);

    let self_ty_kind = string[0];
    match self_ty_kind as char {
        's' => { return ast::sty_static; }
        'r' => { return ast::sty_by_ref; }
        'v' => { return ast::sty_value; }
        '@' => { return ast::sty_box(get_mutability(string[1])); }
        '~' => { return ast::sty_uniq(get_mutability(string[1])); }
        '&' => { return ast::sty_region(get_mutability(string[1])); }
        _ => {
            fail fmt!("unknown self type code: `%c`", self_ty_kind as char);
        }
    }
}

fn item_impl_methods(intr: ident_interner, cdata: cmd, item: ebml::Doc,
                     base_tps: uint) -> ~[@method_info] {
    let mut rslt = ~[];
    for ebml::tagged_docs(item, tag_item_impl_method) |doc| {
        let m_did = ebml::with_doc_data(doc, |d| parse_def_id(d));
        let mth_item = lookup_item(m_did.node, cdata.data);
        let self_ty = get_self_ty(mth_item);
        vec::push(rslt, @{did: translate_def_id(cdata, m_did),
                    /* FIXME (maybe #2323) tjc: take a look at this. */
                   n_tps: item_ty_param_count(mth_item) - base_tps,
                   ident: item_name(intr, mth_item),
                   self_type: self_ty});
    }
    rslt
}

fn get_impls_for_mod(intr: ident_interner, cdata: cmd,
                     m_id: ast::node_id, name: Option<ast::ident>,
                     get_cdata: fn(ast::crate_num) -> cmd)
                  -> @~[@_impl] {

    let data = cdata.data;
    let mod_item = lookup_item(m_id, data);
    let mut result = ~[];
    for ebml::tagged_docs(mod_item, tag_mod_impl) |doc| {
        let did = ebml::with_doc_data(doc, |d| parse_def_id(d));
        let local_did = translate_def_id(cdata, did);
        debug!("(get impls for mod) getting did %? for '%?'",
               local_did, name);
          // The impl may be defined in a different crate. Ask the caller
          // to give us the metadata
        let impl_cdata = get_cdata(local_did.crate);
        let impl_data = impl_cdata.data;
        let item = lookup_item(local_did.node, impl_data);
        let nm = item_name(intr, item);
        if match name { Some(n) => { n == nm } None => { true } } {
           let base_tps = item_ty_param_count(item);
           vec::push(result, @{
                did: local_did, ident: nm,
                methods: item_impl_methods(intr, impl_cdata, item, base_tps)
            });
        };
    }
    @result
}

/* Works for both classes and traits */
fn get_trait_methods(intr: ident_interner, cdata: cmd, id: ast::node_id,
                     tcx: ty::ctxt) -> @~[ty::method] {
    let data = cdata.data;
    let item = lookup_item(id, data);
    let mut result = ~[];
    for ebml::tagged_docs(item, tag_item_trait_method) |mth| {
        let bounds = item_ty_param_bounds(mth, tcx, cdata);
        let name = item_name(intr, mth);
        let ty = doc_type(mth, tcx, cdata);
        let fty = match ty::get(ty).sty {
          ty::ty_fn(f) => f,
          _ => {
            tcx.diag.handler().bug(
                ~"get_trait_methods: id has non-function type");
        } };
        let self_ty = get_self_ty(mth);
        vec::push(result, {ident: name, tps: bounds, fty: fty,
                           self_ty: self_ty,
                           vis: ast::public});
    }
    #debug("get_trait_methods: }");
    @result
}

// If the item in question is a trait, returns its set of methods and
// their self types. Otherwise, returns none. This overlaps in an
// annoying way with get_trait_methods.
fn get_method_names_if_trait(intr: ident_interner, cdata: cmd,
                             node_id: ast::node_id)
                          -> Option<@DVec<(ast::ident, ast::self_ty_)>> {

    let item = lookup_item(node_id, cdata.data);
    if item_family(item) != Trait {
        return None;
    }

    let resulting_methods = @DVec();
    for ebml::tagged_docs(item, tag_item_trait_method) |method| {
        resulting_methods.push(
            (item_name(intr, method), get_self_ty(method)));
    }
    return Some(resulting_methods);
}

fn get_item_attrs(cdata: cmd,
                  node_id: ast::node_id,
                  f: fn(~[@ast::meta_item])) {

    let item = lookup_item(node_id, cdata.data);
    for ebml::tagged_docs(item, tag_attributes) |attributes| {
        for ebml::tagged_docs(attributes, tag_attribute) |attribute| {
            f(get_meta_items(attribute));
        }
    }
}

// Helper function that gets either fields or methods
fn get_class_members(intr: ident_interner, cdata: cmd, id: ast::node_id,
                     p: fn(Family) -> bool) -> ~[ty::field_ty] {
    let data = cdata.data;
    let item = lookup_item(id, data);
    let mut result = ~[];
    for ebml::tagged_docs(item, tag_item_field) |an_item| {
       let f = item_family(an_item);
       if p(f) {
          let name = item_name(intr, an_item);
          let did = item_def_id(an_item, cdata);
          let mt = field_mutability(an_item);
          vec::push(result, {ident: name, id: did, vis:
                  family_to_visibility(f), mutability: mt});
       }
    }
    result
}

pure fn family_to_visibility(family: Family) -> ast::visibility {
    match family {
      PublicField => ast::public,
      PrivateField => ast::private,
      InheritedField => ast::inherited,
      _ => fail
    }
}

fn get_class_fields(intr: ident_interner, cdata: cmd, id: ast::node_id)
    -> ~[ty::field_ty] {
    get_class_members(intr, cdata, id, |f| f == PublicField
                      || f == PrivateField || f == InheritedField)
}

fn family_has_type_params(fam: Family) -> bool {
    match fam {
      Const | ForeignType | Mod | ForeignMod | PublicField | PrivateField
      | ForeignFn => false,
      _           => true
    }
}

fn family_names_type(fam: Family) -> bool {
    match fam { Type | Mod | Trait => true, _ => false }
}

fn read_path(d: ebml::Doc) -> {path: ~str, pos: uint} {
    let desc = ebml::doc_data(d);
    let pos = io::u64_from_be_bytes(desc, 0u, 4u) as uint;
    let pathbytes = vec::slice::<u8>(desc, 4u, vec::len::<u8>(desc));
    let path = str::from_bytes(pathbytes);
    return {path: path, pos: pos};
}

fn describe_def(items: ebml::Doc, id: ast::def_id) -> ~str {
    if id.crate != ast::local_crate { return ~"external"; }
    let it = match maybe_find_item(id.node, items) {
        Some(it) => it,
        None => fail (fmt!("describe_def: item not found %?", id))
    };
    return item_family_to_str(item_family(it));
}

fn item_family_to_str(fam: Family) -> ~str {
    match fam {
      Const => ~"const",
      Fn => ~"fn",
      UnsafeFn => ~"unsafe fn",
      PureFn => ~"pure fn",
      StaticMethod => ~"static method",
      UnsafeStaticMethod => ~"unsafe static method",
      PureStaticMethod => ~"pure static method",
      ForeignFn => ~"foreign fn",
      Type => ~"type",
      ForeignType => ~"foreign type",
      Mod => ~"mod",
      ForeignMod => ~"foreign mod",
      Enum => ~"enum",
      Variant => ~"variant",
      Impl => ~"impl",
      Trait => ~"trait",
      Class => ~"class",
      Struct => ~"struct",
      PublicField => ~"public field",
      PrivateField => ~"private field",
      InheritedField => ~"inherited field",
    }
}

fn get_meta_items(md: ebml::Doc) -> ~[@ast::meta_item] {
    let mut items: ~[@ast::meta_item] = ~[];
    for ebml::tagged_docs(md, tag_meta_item_word) |meta_item_doc| {
        let nd = ebml::get_doc(meta_item_doc, tag_meta_item_name);
        let n = str::from_bytes(ebml::doc_data(nd));
        vec::push(items, attr::mk_word_item(n));
    };
    for ebml::tagged_docs(md, tag_meta_item_name_value) |meta_item_doc| {
        let nd = ebml::get_doc(meta_item_doc, tag_meta_item_name);
        let vd = ebml::get_doc(meta_item_doc, tag_meta_item_value);
        let n = str::from_bytes(ebml::doc_data(nd));
        let v = str::from_bytes(ebml::doc_data(vd));
        // FIXME (#623): Should be able to decode meta_name_value variants,
        // but currently the encoder just drops them
        vec::push(items, attr::mk_name_value_item_str(n, v));
    };
    for ebml::tagged_docs(md, tag_meta_item_list) |meta_item_doc| {
        let nd = ebml::get_doc(meta_item_doc, tag_meta_item_name);
        let n = str::from_bytes(ebml::doc_data(nd));
        let subitems = get_meta_items(meta_item_doc);
        vec::push(items, attr::mk_list_item(n, subitems));
    };
    return items;
}

fn get_attributes(md: ebml::Doc) -> ~[ast::attribute] {
    let mut attrs: ~[ast::attribute] = ~[];
    match ebml::maybe_get_doc(md, tag_attributes) {
      option::Some(attrs_d) => {
        for ebml::tagged_docs(attrs_d, tag_attribute) |attr_doc| {
            let meta_items = get_meta_items(attr_doc);
            // Currently it's only possible to have a single meta item on
            // an attribute
            assert (vec::len(meta_items) == 1u);
            let meta_item = meta_items[0];
            vec::push(attrs,
                      {node: {style: ast::attr_outer, value: *meta_item,
                              is_sugared_doc: false},
                       span: ast_util::dummy_sp()});
        };
      }
      option::None => ()
    }
    return attrs;
}

fn list_meta_items(intr: ident_interner,
                   meta_items: ebml::Doc, out: io::Writer) {
    for get_meta_items(meta_items).each |mi| {
        out.write_str(fmt!("%s\n", pprust::meta_item_to_str(*mi, intr)));
    }
}

fn list_crate_attributes(intr: ident_interner, md: ebml::Doc, hash: ~str,
                         out: io::Writer) {
    out.write_str(fmt!("=Crate Attributes (%s)=\n", hash));

    for get_attributes(md).each |attr| {
        out.write_str(fmt!("%s\n", pprust::attribute_to_str(*attr, intr)));
    }

    out.write_str(~"\n\n");
}

fn get_crate_attributes(data: @~[u8]) -> ~[ast::attribute] {
    return get_attributes(ebml::Doc(data));
}

type crate_dep = {cnum: ast::crate_num, name: ast::ident,
                  vers: ~str, hash: ~str};

fn get_crate_deps(intr: ident_interner, data: @~[u8]) -> ~[crate_dep] {
    let mut deps: ~[crate_dep] = ~[];
    let cratedoc = ebml::Doc(data);
    let depsdoc = ebml::get_doc(cratedoc, tag_crate_deps);
    let mut crate_num = 1;
    fn docstr(doc: ebml::Doc, tag_: uint) -> ~str {
        str::from_bytes(ebml::doc_data(ebml::get_doc(doc, tag_)))
    }
    for ebml::tagged_docs(depsdoc, tag_crate_dep) |depdoc| {
        vec::push(deps, {cnum: crate_num,
                  name: intr.intern(@docstr(depdoc, tag_crate_dep_name)),
                  vers: docstr(depdoc, tag_crate_dep_vers),
                  hash: docstr(depdoc, tag_crate_dep_hash)});
        crate_num += 1;
    };
    return deps;
}

fn list_crate_deps(intr: ident_interner, data: @~[u8], out: io::Writer) {
    out.write_str(~"=External Dependencies=\n");

    for get_crate_deps(intr, data).each |dep| {
        out.write_str(
            fmt!("%d %s-%s-%s\n",
                 dep.cnum, *intr.get(dep.name), dep.hash, dep.vers));
    }

    out.write_str(~"\n");
}

fn get_crate_hash(data: @~[u8]) -> ~str {
    let cratedoc = ebml::Doc(data);
    let hashdoc = ebml::get_doc(cratedoc, tag_crate_hash);
    return str::from_bytes(ebml::doc_data(hashdoc));
}

fn get_crate_vers(data: @~[u8]) -> ~str {
    let attrs = decoder::get_crate_attributes(data);
    return match attr::last_meta_item_value_str_by_name(
        attr::find_linkage_metas(attrs), ~"vers") {
      Some(ver) => ver,
      None => ~"0.0"
    };
}

fn iter_crate_items(intr: ident_interner,
                    cdata: cmd, proc: fn(~str, ast::def_id)) {
    for each_path(intr, cdata) |path_entry| {
        match path_entry.def_like {
            dl_impl(*) | dl_field => {}
            dl_def(def) => {
                proc(path_entry.path_string, ast_util::def_id_of_def(def))
            }
        }
    }
}

fn get_crate_module_paths(intr: ident_interner, cdata: cmd)
                                    -> ~[(ast::def_id, ~str)] {
    fn mod_of_path(p: ~str) -> ~str {
        str::connect(vec::init(str::split_str(p, ~"::")), ~"::")
    }

    // find all module (path, def_ids), which are not
    // fowarded path due to renamed import or reexport
    let mut res = ~[];
    let mods = map::HashMap::<~str,bool>();
    do iter_crate_items(intr, cdata) |path, did| {
        let m = mod_of_path(path);
        if str::is_not_empty(m) {
            // if m has a sub-item, it must be a module
            mods.insert(m, true);
        }
        // Collect everything by now. There might be multiple
        // paths pointing to the same did. Those will be
        // unified later by using the mods map
        vec::push(res, (did, path));
    }
    return do vec::filter(res) |x| {
        let (_, xp) = x;
        mods.contains_key(xp)
    }
}

fn list_crate_metadata(intr: ident_interner, bytes: @~[u8],
                       out: io::Writer) {
    let hash = get_crate_hash(bytes);
    let md = ebml::Doc(bytes);
    list_crate_attributes(intr, md, hash, out);
    list_crate_deps(intr, bytes, out);
}

// Translates a def_id from an external crate to a def_id for the current
// compilation environment. We use this when trying to load types from
// external crates - if those types further refer to types in other crates
// then we must translate the crate number from that encoded in the external
// crate to the correct local crate number.
fn translate_def_id(cdata: cmd, did: ast::def_id) -> ast::def_id {
    if did.crate == ast::local_crate {
        return {crate: cdata.cnum, node: did.node};
    }

    match cdata.cnum_map.find(did.crate) {
      option::Some(n) => return {crate: n, node: did.node},
      option::None => fail ~"didn't find a crate in the cnum_map"
    }
}

// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
