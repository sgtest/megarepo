// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.




const rc_base_field_refcnt: uint = 0u;

const task_field_refcnt: uint = 0u;

const task_field_stk: uint = 2u;

const task_field_runtime_sp: uint = 3u;

const task_field_rust_sp: uint = 4u;

const task_field_gc_alloc_chain: uint = 5u;

const task_field_dom: uint = 6u;

const n_visible_task_fields: uint = 7u;

const dom_field_interrupt_flag: uint = 1u;

const frame_glue_fns_field_mark: uint = 0u;

const frame_glue_fns_field_drop: uint = 1u;

const frame_glue_fns_field_reloc: uint = 2u;

const box_field_refcnt: uint = 0u;
const box_field_tydesc: uint = 1u;
const box_field_prev: uint = 2u;
const box_field_next: uint = 3u;
const box_field_body: uint = 4u;

const general_code_alignment: uint = 16u;

const tydesc_field_size: uint = 0u;
const tydesc_field_align: uint = 1u;
const tydesc_field_take_glue: uint = 2u;
const tydesc_field_drop_glue: uint = 3u;
const tydesc_field_free_glue: uint = 4u;
const tydesc_field_visit_glue: uint = 5u;
const tydesc_field_shape: uint = 6u;
const tydesc_field_shape_tables: uint = 7u;
const n_tydesc_fields: uint = 8u;

// The two halves of a closure: code and environment.
const fn_field_code: uint = 0u;
const fn_field_box: uint = 1u;

const vec_elt_fill: uint = 0u;

const vec_elt_alloc: uint = 1u;

const vec_elt_elems: uint = 2u;

const slice_elt_base: uint = 0u;
const slice_elt_len: uint = 1u;

const worst_case_glue_call_args: uint = 7u;

const abi_version: uint = 1u;

fn memcpy_glue_name() -> ~str { return ~"rust_memcpy_glue"; }

fn bzero_glue_name() -> ~str { return ~"rust_bzero_glue"; }

fn yield_glue_name() -> ~str { return ~"rust_yield_glue"; }

fn no_op_type_glue_name() -> ~str { return ~"rust_no_op_type_glue"; }
//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
//
