// EBML tag definitions and utils shared by the encoder and decoder

import std::str;

const uint tag_paths = 0x01u;

const uint tag_items = 0x02u;

const uint tag_paths_data = 0x03u;

const uint tag_paths_data_name = 0x04u;

const uint tag_paths_data_item = 0x05u;

const uint tag_paths_data_mod = 0x06u;

const uint tag_def_id = 0x07u;

const uint tag_items_data = 0x08u;

const uint tag_items_data_item = 0x09u;

const uint tag_items_data_item_kind = 0x0au;

const uint tag_items_data_item_ty_param_count = 0x0bu;

const uint tag_items_data_item_type = 0x0cu;

const uint tag_items_data_item_symbol = 0x0du;

const uint tag_items_data_item_variant = 0x0eu;

const uint tag_items_data_item_tag_id = 0x0fu;

const uint tag_index = 0x11u;

const uint tag_index_buckets = 0x12u;

const uint tag_index_buckets_bucket = 0x13u;

const uint tag_index_buckets_bucket_elt = 0x14u;

const uint tag_index_table = 0x15u;

const uint tag_meta_item_name_value = 0x18u;

const uint tag_meta_item_name = 0x19u;

const uint tag_meta_item_value = 0x20u;

const uint tag_attributes = 0x21u;

const uint tag_attribute = 0x22u;

const uint tag_meta_item_word = 0x23u;

const uint tag_meta_item_list = 0x24u;

// djb's cdb hashes.
fn hash_node_id(&int node_id) -> uint { ret 177573u ^ (node_id as uint); }

fn hash_path(&str s) -> uint {
    auto h = 5381u;
    for (u8 ch in str::bytes(s)) { h = (h << 5u) + h ^ (ch as uint); }
    ret h;
}

