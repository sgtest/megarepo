// Metadata encoding

import std::ivec;
import std::str;
import std::vec;
import std::uint;
import std::io;
import std::option;
import std::option::some;
import std::option::none;
import std::ebml;
import syntax::ast::*;
import tags::*;
import middle::trans::crate_ctxt;
import middle::ty;
import middle::ty::node_id_to_monotype;
import front::attr;

export def_to_str;
export hash_path;
export hash_node_id;
export encode_metadata;

// Path table encoding
fn encode_name(&ebml::writer ebml_w, &str name) {
    ebml::start_tag(ebml_w, tag_paths_data_name);
    ebml_w.writer.write(str::bytes(name));
    ebml::end_tag(ebml_w);
}

fn encode_def_id(&ebml::writer ebml_w, &def_id id) {
    ebml::start_tag(ebml_w, tag_def_id);
    ebml_w.writer.write(str::bytes(def_to_str(id)));
    ebml::end_tag(ebml_w);
}

fn encode_tag_variant_paths(&ebml::writer ebml_w, &variant[] variants,
                            &vec[str] path,
                            &mutable vec[tup(str, uint)] index) {
    for (variant variant in variants) {
        add_to_index(ebml_w, path, index, variant.node.name);
        ebml::start_tag(ebml_w, tag_paths_data_item);
        encode_name(ebml_w, variant.node.name);
        encode_def_id(ebml_w, local_def(variant.node.id));
        ebml::end_tag(ebml_w);
    }
}

fn add_to_index(&ebml::writer ebml_w, &vec[str] path,
                &mutable vec[tup(str, uint)] index, &str name) {
    auto full_path = path + [name];
    index += [tup(str::connect(full_path, "::"), ebml_w.writer.tell())];
}

fn encode_native_module_item_paths(&ebml::writer ebml_w,
                                   &native_mod nmod, &vec[str] path,
                                   &mutable vec[tup(str, uint)] index) {
    for (@native_item nitem in nmod.items) {
        add_to_index(ebml_w, path, index, nitem.ident);
        ebml::start_tag(ebml_w, tag_paths_data_item);
        encode_name(ebml_w, nitem.ident);
        encode_def_id(ebml_w, local_def(nitem.id));
        ebml::end_tag(ebml_w);
    }
}

fn encode_module_item_paths(&ebml::writer ebml_w, &_mod module,
                            &vec[str] path,
                            &mutable vec[tup(str, uint)] index) {
    for (@item it in module.items) {
        if (!is_exported(it.ident, module)) { cont; }
        alt (it.node) {
            case (item_const(_, _)) {
                add_to_index(ebml_w, path, index, it.ident);
                ebml::start_tag(ebml_w, tag_paths_data_item);
                encode_name(ebml_w, it.ident);
                encode_def_id(ebml_w, local_def(it.id));
                ebml::end_tag(ebml_w);
            }
            case (item_fn(_, ?tps)) {
                add_to_index(ebml_w, path, index, it.ident);
                ebml::start_tag(ebml_w, tag_paths_data_item);
                encode_name(ebml_w, it.ident);
                encode_def_id(ebml_w, local_def(it.id));
                ebml::end_tag(ebml_w);
            }
            case (item_mod(?_mod)) {
                add_to_index(ebml_w, path, index, it.ident);
                ebml::start_tag(ebml_w, tag_paths_data_mod);
                encode_name(ebml_w, it.ident);
                encode_def_id(ebml_w, local_def(it.id));
                encode_module_item_paths(ebml_w, _mod, path + [it.ident],
                                         index);
                ebml::end_tag(ebml_w);
            }
            case (item_native_mod(?nmod)) {
                add_to_index(ebml_w, path, index, it.ident);
                ebml::start_tag(ebml_w, tag_paths_data_mod);
                encode_name(ebml_w, it.ident);
                encode_def_id(ebml_w, local_def(it.id));
                encode_native_module_item_paths(ebml_w, nmod,
                                                path + [it.ident], index);
                ebml::end_tag(ebml_w);
            }
            case (item_ty(_, ?tps)) {
                add_to_index(ebml_w, path, index, it.ident);
                ebml::start_tag(ebml_w, tag_paths_data_item);
                encode_name(ebml_w, it.ident);
                encode_def_id(ebml_w, local_def(it.id));
                ebml::end_tag(ebml_w);
            }
            case (item_res(_, _, ?tps, ?ctor_id)) {
                add_to_index(ebml_w, path, index, it.ident);
                ebml::start_tag(ebml_w, tag_paths_data_item);
                encode_name(ebml_w, it.ident);
                encode_def_id(ebml_w, local_def(ctor_id));
                ebml::end_tag(ebml_w);
                add_to_index(ebml_w, path, index, it.ident);
                ebml::start_tag(ebml_w, tag_paths_data_item);
                encode_name(ebml_w, it.ident);
                encode_def_id(ebml_w, local_def(it.id));
                ebml::end_tag(ebml_w);
            }
            case (item_tag(?variants, ?tps)) {
                add_to_index(ebml_w, path, index, it.ident);
                ebml::start_tag(ebml_w, tag_paths_data_item);
                encode_name(ebml_w, it.ident);
                encode_def_id(ebml_w, local_def(it.id));
                ebml::end_tag(ebml_w);
                encode_tag_variant_paths(ebml_w, variants, path, index);
            }
            case (item_obj(_, ?tps, ?ctor_id)) {
                add_to_index(ebml_w, path, index, it.ident);
                ebml::start_tag(ebml_w, tag_paths_data_item);
                encode_name(ebml_w, it.ident);
                encode_def_id(ebml_w, local_def(ctor_id));
                ebml::end_tag(ebml_w);
                add_to_index(ebml_w, path, index, it.ident);
                ebml::start_tag(ebml_w, tag_paths_data_item);
                encode_name(ebml_w, it.ident);
                encode_def_id(ebml_w, local_def(it.id));
                ebml::end_tag(ebml_w);
            }
        }
    }
}

fn encode_item_paths(&ebml::writer ebml_w, &@crate crate) ->
   vec[tup(str, uint)] {
    let vec[tup(str, uint)] index = [];
    let vec[str] path = [];
    ebml::start_tag(ebml_w, tag_paths);
    encode_module_item_paths(ebml_w, crate.node.module, path, index);
    ebml::end_tag(ebml_w);
    ret index;
}


// Item info table encoding
fn encode_kind(&ebml::writer ebml_w, u8 c) {
    ebml::start_tag(ebml_w, tag_items_data_item_kind);
    ebml_w.writer.write([c]);
    ebml::end_tag(ebml_w);
}

fn def_to_str(&def_id did) -> str { ret #fmt("%d:%d", did._0, did._1); }

fn encode_type_param_count(&ebml::writer ebml_w, &ty_param[] tps) {
    ebml::start_tag(ebml_w, tag_items_data_item_ty_param_count);
    ebml::write_vint(ebml_w.writer, ivec::len[ty_param](tps));
    ebml::end_tag(ebml_w);
}

fn encode_variant_id(&ebml::writer ebml_w, &def_id vid) {
    ebml::start_tag(ebml_w, tag_items_data_item_variant);
    ebml_w.writer.write(str::bytes(def_to_str(vid)));
    ebml::end_tag(ebml_w);
}

fn encode_type(&@crate_ctxt cx, &ebml::writer ebml_w, &ty::t typ) {
    ebml::start_tag(ebml_w, tag_items_data_item_type);
    auto f = def_to_str;
    auto ty_str_ctxt =
        @rec(ds=f, tcx=cx.tcx,
             abbrevs=tyencode::ac_use_abbrevs(cx.type_abbrevs));
    tyencode::enc_ty(io::new_writer_(ebml_w.writer), ty_str_ctxt, typ);
    ebml::end_tag(ebml_w);
}

fn encode_symbol(&@crate_ctxt cx, &ebml::writer ebml_w,
                 node_id id) {
    ebml::start_tag(ebml_w, tag_items_data_item_symbol);
    ebml_w.writer.write(str::bytes(cx.item_symbols.get(id)));
    ebml::end_tag(ebml_w);
}

fn encode_discriminant(&@crate_ctxt cx, &ebml::writer ebml_w,
                       node_id id) {
    ebml::start_tag(ebml_w, tag_items_data_item_symbol);
    ebml_w.writer.write(str::bytes(cx.discrim_symbols.get(id)));
    ebml::end_tag(ebml_w);
}

fn encode_tag_id(&ebml::writer ebml_w, &def_id id) {
    ebml::start_tag(ebml_w, tag_items_data_item_tag_id);
    ebml_w.writer.write(str::bytes(def_to_str(id)));
    ebml::end_tag(ebml_w);
}

fn encode_tag_variant_info(&@crate_ctxt cx, &ebml::writer ebml_w,
                           node_id id, &variant[] variants,
                           &mutable vec[tup(int, uint)] index,
                           &ty_param[] ty_params) {
    for (variant variant in variants) {
        index += [tup(variant.node.id, ebml_w.writer.tell())];
        ebml::start_tag(ebml_w, tag_items_data_item);
        encode_def_id(ebml_w, local_def(variant.node.id));
        encode_kind(ebml_w, 'v' as u8);
        encode_tag_id(ebml_w, local_def(id));
        encode_type(cx, ebml_w, node_id_to_monotype(cx.tcx, variant.node.id));
        if (vec::len[variant_arg](variant.node.args) > 0u) {
            encode_symbol(cx, ebml_w, variant.node.id);
        }
        encode_discriminant(cx, ebml_w, variant.node.id);
        encode_type_param_count(ebml_w, ty_params);
        ebml::end_tag(ebml_w);
    }
}

fn encode_info_for_item(@crate_ctxt cx, &ebml::writer ebml_w,
                        @item item, &mutable vec[tup(int, uint)] index) {
    alt (item.node) {
        case (item_const(_, _)) {
            ebml::start_tag(ebml_w, tag_items_data_item);
            encode_def_id(ebml_w, local_def(item.id));
            encode_kind(ebml_w, 'c' as u8);
            encode_type(cx, ebml_w, node_id_to_monotype(cx.tcx, item.id));
            encode_symbol(cx, ebml_w, item.id);
            ebml::end_tag(ebml_w);
        }
        case (item_fn(?fd, ?tps)) {
            ebml::start_tag(ebml_w, tag_items_data_item);
            encode_def_id(ebml_w, local_def(item.id));
            encode_kind(ebml_w, alt (fd.decl.purity) {
                                  case (pure_fn) { 'p' }
                                  case (impure_fn) { 'f' } } as u8);
            encode_type_param_count(ebml_w, tps);
            encode_type(cx, ebml_w, node_id_to_monotype(cx.tcx, item.id));
            encode_symbol(cx, ebml_w, item.id);
            ebml::end_tag(ebml_w);
        }
        case (item_mod(_)) {
            ebml::start_tag(ebml_w, tag_items_data_item);
            encode_def_id(ebml_w, local_def(item.id));
            encode_kind(ebml_w, 'm' as u8);
            ebml::end_tag(ebml_w);
        }
        case (item_native_mod(_)) {
            ebml::start_tag(ebml_w, tag_items_data_item);
            encode_def_id(ebml_w, local_def(item.id));
            encode_kind(ebml_w, 'n' as u8);
            ebml::end_tag(ebml_w);
        }
        case (item_ty(_, ?tps)) {
            ebml::start_tag(ebml_w, tag_items_data_item);
            encode_def_id(ebml_w, local_def(item.id));
            encode_kind(ebml_w, 'y' as u8);
            encode_type_param_count(ebml_w, tps);
            encode_type(cx, ebml_w, node_id_to_monotype(cx.tcx, item.id));
            ebml::end_tag(ebml_w);
        }
        case (item_tag(?variants, ?tps)) {
            ebml::start_tag(ebml_w, tag_items_data_item);
            encode_def_id(ebml_w, local_def(item.id));
            encode_kind(ebml_w, 't' as u8);
            encode_type_param_count(ebml_w, tps);
            encode_type(cx, ebml_w, node_id_to_monotype(cx.tcx, item.id));
            for (variant v in variants) {
                encode_variant_id(ebml_w, local_def(v.node.id));
            }
            ebml::end_tag(ebml_w);
            encode_tag_variant_info(cx, ebml_w, item.id, variants, index,
                                    tps);
        }
        case (item_res(_, _, ?tps, ?ctor_id)) {
            auto fn_ty = node_id_to_monotype(cx.tcx, ctor_id);

            ebml::start_tag(ebml_w, tag_items_data_item);
            encode_def_id(ebml_w, local_def(ctor_id));
            encode_kind(ebml_w, 'y' as u8);
            encode_type_param_count(ebml_w, tps);
            encode_type(cx, ebml_w, ty::ty_fn_ret(cx.tcx, fn_ty));
            encode_symbol(cx, ebml_w, item.id);
            ebml::end_tag(ebml_w);

            index += [tup(ctor_id, ebml_w.writer.tell())];
            ebml::start_tag(ebml_w, tag_items_data_item);
            encode_def_id(ebml_w, local_def(ctor_id));
            encode_kind(ebml_w, 'f' as u8);
            encode_type_param_count(ebml_w, tps);
            encode_type(cx, ebml_w, fn_ty);
            encode_symbol(cx, ebml_w, ctor_id);
            ebml::end_tag(ebml_w);
        }
        case (item_obj(_, ?tps, ?ctor_id)) {
            auto fn_ty = node_id_to_monotype(cx.tcx, ctor_id);

            ebml::start_tag(ebml_w, tag_items_data_item);
            encode_def_id(ebml_w, local_def(item.id));
            encode_kind(ebml_w, 'y' as u8);
            encode_type_param_count(ebml_w, tps);
            encode_type(cx, ebml_w, ty::ty_fn_ret(cx.tcx, fn_ty));
            ebml::end_tag(ebml_w);

            index += [tup(ctor_id, ebml_w.writer.tell())];
            ebml::start_tag(ebml_w, tag_items_data_item);
            encode_def_id(ebml_w, local_def(ctor_id));
            encode_kind(ebml_w, 'f' as u8);
            encode_type_param_count(ebml_w, tps);
            encode_type(cx, ebml_w, fn_ty);
            encode_symbol(cx, ebml_w, ctor_id);
            ebml::end_tag(ebml_w);
        }
    }
}

fn encode_info_for_native_item(&@crate_ctxt cx, &ebml::writer ebml_w,
                               &@native_item nitem) {
    ebml::start_tag(ebml_w, tag_items_data_item);
    alt (nitem.node) {
        case (native_item_ty) {
            encode_def_id(ebml_w, local_def(nitem.id));
            encode_kind(ebml_w, 'T' as u8);
            encode_type(cx, ebml_w,
                        ty::mk_native(cx.tcx, local_def(nitem.id)));
        }
        case (native_item_fn(_, _, ?tps)) {
            encode_def_id(ebml_w, local_def(nitem.id));
            encode_kind(ebml_w, 'F' as u8);
            encode_type_param_count(ebml_w, tps);
            encode_type(cx, ebml_w, node_id_to_monotype(cx.tcx, nitem.id));
            encode_symbol(cx, ebml_w, nitem.id);
        }
    }
    ebml::end_tag(ebml_w);
}

fn encode_info_for_items(&@crate_ctxt cx, &ebml::writer ebml_w) ->
   vec[tup(int, uint)] {
    let vec[tup(int, uint)] index = [];
    ebml::start_tag(ebml_w, tag_items_data);
    for each (@tup(node_id, middle::ast_map::ast_node) kvp in
              cx.ast_map.items()) {
        alt (kvp._1) {
            case (middle::ast_map::node_item(?i)) {
                index += [tup(kvp._0, ebml_w.writer.tell())];
                encode_info_for_item(cx, ebml_w, i, index);
            }
            case (middle::ast_map::node_native_item(?i)) {
                index += [tup(kvp._0, ebml_w.writer.tell())];
                encode_info_for_native_item(cx, ebml_w, i);
            }
            case (_) {}
        }
    }
    ebml::end_tag(ebml_w);
    ret index;
}


// Path and definition ID indexing

// djb's cdb hashes.
fn hash_node_id(&int node_id) -> uint { ret 177573u ^ (node_id as uint); }

fn hash_path(&str s) -> uint {
    auto h = 5381u;
    for (u8 ch in str::bytes(s)) { h = (h << 5u) + h ^ (ch as uint); }
    ret h;
}

fn create_index[T](&vec[tup(T, uint)] index, fn(&T) -> uint  hash_fn) ->
   vec[vec[tup(T, uint)]] {
    let vec[mutable vec[tup(T, uint)]] buckets = vec::empty_mut();
    for each (uint i in uint::range(0u, 256u)) { buckets += [mutable []]; }
    for (tup(T, uint) elt in index) {
        auto h = hash_fn(elt._0);
        buckets.(h % 256u) += [elt];
    }
    ret vec::freeze(buckets);
}

fn encode_index[T](&ebml::writer ebml_w, &vec[vec[tup(T, uint)]] buckets,
                   fn(&io::writer, &T)  write_fn) {
    auto writer = io::new_writer_(ebml_w.writer);
    ebml::start_tag(ebml_w, tag_index);
    let vec[uint] bucket_locs = [];
    ebml::start_tag(ebml_w, tag_index_buckets);
    for (vec[tup(T, uint)] bucket in buckets) {
        bucket_locs += [ebml_w.writer.tell()];
        ebml::start_tag(ebml_w, tag_index_buckets_bucket);
        for (tup(T, uint) elt in bucket) {
            ebml::start_tag(ebml_w, tag_index_buckets_bucket_elt);
            writer.write_be_uint(elt._1, 4u);
            write_fn(writer, elt._0);
            ebml::end_tag(ebml_w);
        }
        ebml::end_tag(ebml_w);
    }
    ebml::end_tag(ebml_w);
    ebml::start_tag(ebml_w, tag_index_table);
    for (uint pos in bucket_locs) { writer.write_be_uint(pos, 4u); }
    ebml::end_tag(ebml_w);
    ebml::end_tag(ebml_w);
}

fn write_str(&io::writer writer, &str s) { writer.write_str(s); }

fn write_int(&io::writer writer, &int n) {
    writer.write_be_uint(n as uint, 4u);
}

fn encode_meta_item(&ebml::writer ebml_w, &meta_item mi) {
    alt (mi.node) {
        case (meta_word(?name)) {
            ebml::start_tag(ebml_w, tag_meta_item_word);
            ebml::start_tag(ebml_w, tag_meta_item_name);
            ebml_w.writer.write(str::bytes(name));
            ebml::end_tag(ebml_w);
            ebml::end_tag(ebml_w);
        }
        case (meta_name_value(?name, ?value)) {
            alt (value.node) {
                case (lit_str(?value, _)) {
                    ebml::start_tag(ebml_w, tag_meta_item_name_value);
                    ebml::start_tag(ebml_w, tag_meta_item_name);
                    ebml_w.writer.write(str::bytes(name));
                    ebml::end_tag(ebml_w);
                    ebml::start_tag(ebml_w, tag_meta_item_value);
                    ebml_w.writer.write(str::bytes(value));
                    ebml::end_tag(ebml_w);
                    ebml::end_tag(ebml_w);
                }
                case (_) { /* FIXME (#611) */ }
            }
        }
        case (meta_list(?name, ?items)) {
            ebml::start_tag(ebml_w, tag_meta_item_list);
            ebml::start_tag(ebml_w, tag_meta_item_name);
            ebml_w.writer.write(str::bytes(name));
            ebml::end_tag(ebml_w);
            for (@meta_item inner_item in items) {
                encode_meta_item(ebml_w, *inner_item);
            }
            ebml::end_tag(ebml_w);
        }
    }
}

fn encode_attributes(&ebml::writer ebml_w, &vec[attribute] attrs) {
    ebml::start_tag(ebml_w, tag_attributes);
    for (attribute attr in attrs) {
        ebml::start_tag(ebml_w, tag_attribute);
        encode_meta_item(ebml_w, attr.node.value);
        ebml::end_tag(ebml_w);
    }
    ebml::end_tag(ebml_w);
}

// So there's a special crate attribute called 'link' which defines the
// metadata that Rust cares about for linking crates. This attribute requires
// 'name' and 'vers' items, so if the user didn't provide them we will throw
// them in anyway with default values.
fn synthesize_crate_attrs(&@crate_ctxt cx,
                          &@crate crate) -> vec[attribute] {

    fn synthesize_link_attr(&@crate_ctxt cx, &(@meta_item)[] items)
            -> attribute {

        assert cx.link_meta.name != "";
        assert cx.link_meta.vers != "";

        auto name_item = attr::mk_name_value_item_str("name",
                                                      cx.link_meta.name);
        auto vers_item = attr::mk_name_value_item_str("vers",
                                                      cx.link_meta.vers);

        auto other_items = {
            auto tmp = attr::remove_meta_items_by_name(items, "name");
            attr::remove_meta_items_by_name(tmp, "vers")
        };

        auto meta_items = ~[name_item, vers_item] + other_items;
        auto link_item = attr::mk_list_item("link", meta_items);

        ret attr::mk_attr(link_item);
    }

    let vec[attribute] attrs = [];
    auto found_link_attr = false;
    for (attribute attr in crate.node.attrs) {
        attrs += if (attr::get_attr_name(attr) != "link") {
            [attr]
        } else {
            alt (attr.node.value.node) {
                case (meta_list(?n, ?l)) {
                    found_link_attr = true;
                    [synthesize_link_attr(cx, l)]
                }
                case (_) { [attr] }
            }
        }
    }

    if (!found_link_attr) {
        attrs += [synthesize_link_attr(cx, ~[])];
    }

    ret attrs;
}

fn encode_metadata(&@crate_ctxt cx, &@crate crate) -> str {
    auto string_w = io::string_writer();
    auto buf_w = string_w.get_writer().get_buf_writer();
    auto ebml_w = ebml::create_writer(buf_w);

    auto crate_attrs = synthesize_crate_attrs(cx, crate);
    encode_attributes(ebml_w, crate_attrs);
    // Encode and index the paths.

    ebml::start_tag(ebml_w, tag_paths);
    auto paths_index = encode_item_paths(ebml_w, crate);
    auto str_writer = write_str;
    auto path_hasher = hash_path;
    auto paths_buckets = create_index[str](paths_index, path_hasher);
    encode_index[str](ebml_w, paths_buckets, str_writer);
    ebml::end_tag(ebml_w);
    // Encode and index the items.

    ebml::start_tag(ebml_w, tag_items);
    auto items_index = encode_info_for_items(cx, ebml_w);
    auto int_writer = write_int;
    auto item_hasher = hash_node_id;
    auto items_buckets = create_index[int](items_index, item_hasher);
    encode_index[int](ebml_w, items_buckets, int_writer);
    ebml::end_tag(ebml_w);
    // Pad this, since something (LLVM, presumably) is cutting off the
    // remaining % 4 bytes.

    buf_w.write([0u8, 0u8, 0u8, 0u8]);
    ret string_w.get_str();
}


// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
