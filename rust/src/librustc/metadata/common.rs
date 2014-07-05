// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(non_camel_case_types)]

use std::mem;
use back::svh::Svh;

// EBML enum definitions and utils shared by the encoder and decoder

pub static tag_items: uint = 0x00;

pub static tag_paths_data_name: uint = 0x01;

pub static tag_def_id: uint = 0x02;

pub static tag_items_data: uint = 0x03;

pub static tag_items_data_item: uint = 0x04;

pub static tag_items_data_item_family: uint = 0x05;

pub static tag_items_data_item_ty_param_bounds: uint = 0x06;

pub static tag_items_data_item_type: uint = 0x07;

pub static tag_items_data_item_symbol: uint = 0x08;

pub static tag_items_data_item_variant: uint = 0x09;

pub static tag_items_data_parent_item: uint = 0x0a;

pub static tag_items_data_item_is_tuple_struct_ctor: uint = 0x0b;

pub static tag_index: uint = 0x0c;

pub static tag_index_buckets: uint = 0x0d;

pub static tag_index_buckets_bucket: uint = 0x0e;

pub static tag_index_buckets_bucket_elt: uint = 0x0f;

pub static tag_index_table: uint = 0x10;

pub static tag_meta_item_name_value: uint = 0x11;

pub static tag_meta_item_name: uint = 0x12;

pub static tag_meta_item_value: uint = 0x13;

pub static tag_attributes: uint = 0x14;

pub static tag_attribute: uint = 0x15;

pub static tag_meta_item_word: uint = 0x16;

pub static tag_meta_item_list: uint = 0x17;

// The list of crates that this crate depends on
pub static tag_crate_deps: uint = 0x18;

// A single crate dependency
pub static tag_crate_dep: uint = 0x19;

pub static tag_crate_hash: uint = 0x1a;
pub static tag_crate_crate_name: uint = 0x1b;

pub static tag_crate_dep_crate_name: uint = 0x1d;
pub static tag_crate_dep_hash: uint = 0x1e;

pub static tag_mod_impl: uint = 0x1f;

pub static tag_item_trait_method: uint = 0x20;

pub static tag_item_trait_ref: uint = 0x21;
pub static tag_item_super_trait_ref: uint = 0x22;

// discriminator value for variants
pub static tag_disr_val: uint = 0x23;

// used to encode ast_map::PathElem
pub static tag_path: uint = 0x24;
pub static tag_path_len: uint = 0x25;
pub static tag_path_elem_mod: uint = 0x26;
pub static tag_path_elem_name: uint = 0x27;
pub static tag_item_field: uint = 0x28;
pub static tag_item_field_origin: uint = 0x29;

pub static tag_item_variances: uint = 0x2a;
/*
  trait items contain tag_item_trait_method elements,
  impl items contain tag_item_impl_method elements, and classes
  have both. That's because some code treats classes like traits,
  and other code treats them like impls. Because classes can contain
  both, tag_item_trait_method and tag_item_impl_method have to be two
  different tags.
 */
pub static tag_item_impl_method: uint = 0x30;
pub static tag_item_trait_method_explicit_self: uint = 0x31;


// Reexports are found within module tags. Each reexport contains def_ids
// and names.
pub static tag_items_data_item_reexport: uint = 0x38;
pub static tag_items_data_item_reexport_def_id: uint = 0x39;
pub static tag_items_data_item_reexport_name: uint = 0x3a;

// used to encode crate_ctxt side tables
#[deriving(PartialEq)]
#[repr(uint)]
pub enum astencode_tag { // Reserves 0x40 -- 0x5f
    tag_ast = 0x40,

    tag_tree = 0x41,

    tag_id_range = 0x42,

    tag_table = 0x43,
    tag_table_id = 0x44,
    tag_table_val = 0x45,
    tag_table_def = 0x46,
    tag_table_node_type = 0x47,
    tag_table_item_subst = 0x48,
    tag_table_freevars = 0x49,
    tag_table_tcache = 0x4a,
    tag_table_param_defs = 0x4b,
    tag_table_mutbl = 0x4c,
    tag_table_last_use = 0x4d,
    tag_table_spill = 0x4e,
    tag_table_method_map = 0x4f,
    tag_table_vtable_map = 0x50,
    tag_table_adjustments = 0x51,
    tag_table_moves_map = 0x52,
    tag_table_capture_map = 0x53
}
static first_astencode_tag: uint = tag_ast as uint;
static last_astencode_tag: uint = tag_table_capture_map as uint;
impl astencode_tag {
    pub fn from_uint(value : uint) -> Option<astencode_tag> {
        let is_a_tag = first_astencode_tag <= value && value <= last_astencode_tag;
        if !is_a_tag { None } else {
            Some(unsafe { mem::transmute(value) })
        }
    }
}

pub static tag_item_trait_method_sort: uint = 0x60;

pub static tag_item_impl_type_basename: uint = 0x61;

// Language items are a top-level directory (for speed). Hierarchy:
//
// tag_lang_items
// - tag_lang_items_item
//   - tag_lang_items_item_id: u32
//   - tag_lang_items_item_node_id: u32

pub static tag_lang_items: uint = 0x70;
pub static tag_lang_items_item: uint = 0x71;
pub static tag_lang_items_item_id: uint = 0x72;
pub static tag_lang_items_item_node_id: uint = 0x73;
pub static tag_lang_items_missing: uint = 0x74;

pub static tag_item_unnamed_field: uint = 0x75;
pub static tag_items_data_item_visibility: uint = 0x76;
pub static tag_items_data_item_sized: uint = 0x77;

pub static tag_item_method_tps: uint = 0x79;
pub static tag_item_method_fty: uint = 0x7a;

pub static tag_mod_child: uint = 0x7b;
pub static tag_misc_info: uint = 0x7c;
pub static tag_misc_info_crate_items: uint = 0x7d;

pub static tag_item_method_provided_source: uint = 0x7e;
pub static tag_item_impl_vtables: uint = 0x7f;

pub static tag_impls: uint = 0x80;
pub static tag_impls_impl: uint = 0x81;

pub static tag_items_data_item_inherent_impl: uint = 0x82;
pub static tag_items_data_item_extension_impl: uint = 0x83;

// GAP 0x84, 0x85, 0x86

pub static tag_native_libraries: uint = 0x87;
pub static tag_native_libraries_lib: uint = 0x88;
pub static tag_native_libraries_name: uint = 0x89;
pub static tag_native_libraries_kind: uint = 0x8a;

pub static tag_plugin_registrar_fn: uint = 0x8b;
pub static tag_exported_macros: uint = 0x8c;
pub static tag_macro_def: uint = 0x8d;

pub static tag_crate_triple: uint = 0x66;

pub static tag_dylib_dependency_formats: uint = 0x67;

pub static tag_method_argument_names: uint = 0x8e;
pub static tag_method_argument_name: uint = 0x8f;

pub static tag_reachable_extern_fns: uint = 0x90;
pub static tag_reachable_extern_fn_id: uint = 0x91;

pub static tag_items_data_item_stability: uint = 0x92;


#[deriving(Clone, Show)]
pub struct LinkMeta {
    pub crate_name: String,
    pub crate_hash: Svh,
}

pub static tag_region_param_def: uint = 0x90;
pub static tag_region_param_def_ident: uint = 0x91;
pub static tag_region_param_def_def_id: uint = 0x92;
pub static tag_region_param_def_space: uint = 0x93;
pub static tag_region_param_def_index: uint = 0x94;
