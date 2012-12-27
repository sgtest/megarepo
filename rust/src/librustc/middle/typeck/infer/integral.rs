// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Code related to integral type inference.

*/

use middle::ty;
use middle::typeck::infer::to_str::ToStr;

use core::uint;
use syntax::ast;

// Bitvector to represent sets of integral types
enum int_ty_set = uint;

// Constants representing singleton sets containing each of the
// integral types
const INT_TY_SET_EMPTY : uint = 0b00_0000_0000u;
const INT_TY_SET_i8    : uint = 0b00_0000_0001u;
const INT_TY_SET_u8    : uint = 0b00_0000_0010u;
const INT_TY_SET_i16   : uint = 0b00_0000_0100u;
const INT_TY_SET_u16   : uint = 0b00_0000_1000u;
const INT_TY_SET_i32   : uint = 0b00_0001_0000u;
const INT_TY_SET_u32   : uint = 0b00_0010_0000u;
const INT_TY_SET_i64   : uint = 0b00_0100_0000u;
const INT_TY_SET_u64   : uint = 0b00_1000_0000u;
const INT_TY_SET_i     : uint = 0b01_0000_0000u;
const INT_TY_SET_u     : uint = 0b10_0000_0000u;

fn int_ty_set_all()  -> int_ty_set {
    int_ty_set(INT_TY_SET_i8  | INT_TY_SET_u8 |
               INT_TY_SET_i16 | INT_TY_SET_u16 |
               INT_TY_SET_i32 | INT_TY_SET_u32 |
               INT_TY_SET_i64 | INT_TY_SET_u64 |
               INT_TY_SET_i   | INT_TY_SET_u)
}

fn intersection(a: int_ty_set, b: int_ty_set) -> int_ty_set {
    int_ty_set(*a & *b)
}

fn single_type_contained_in(tcx: ty::ctxt, a: int_ty_set) ->
    Option<ty::t> {
    debug!("single_type_contained_in(a=%s)", uint::to_str(*a, 10u));

    if *a == INT_TY_SET_i8    { return Some(ty::mk_i8(tcx)); }
    if *a == INT_TY_SET_u8    { return Some(ty::mk_u8(tcx)); }
    if *a == INT_TY_SET_i16   { return Some(ty::mk_i16(tcx)); }
    if *a == INT_TY_SET_u16   { return Some(ty::mk_u16(tcx)); }
    if *a == INT_TY_SET_i32   { return Some(ty::mk_i32(tcx)); }
    if *a == INT_TY_SET_u32   { return Some(ty::mk_u32(tcx)); }
    if *a == INT_TY_SET_i64   { return Some(ty::mk_i64(tcx)); }
    if *a == INT_TY_SET_u64   { return Some(ty::mk_u64(tcx)); }
    if *a == INT_TY_SET_i     { return Some(ty::mk_int(tcx)); }
    if *a == INT_TY_SET_u     { return Some(ty::mk_uint(tcx)); }
    return None;
}

fn convert_integral_ty_to_int_ty_set(tcx: ty::ctxt, t: ty::t)
    -> int_ty_set {

    match get(t).sty {
      ty_int(int_ty) => match int_ty {
        ast::ty_i8   => int_ty_set(INT_TY_SET_i8),
        ast::ty_i16  => int_ty_set(INT_TY_SET_i16),
        ast::ty_i32  => int_ty_set(INT_TY_SET_i32),
        ast::ty_i64  => int_ty_set(INT_TY_SET_i64),
        ast::ty_i    => int_ty_set(INT_TY_SET_i),
        ast::ty_char => tcx.sess.bug(
            ~"char type passed to convert_integral_ty_to_int_ty_set()")
      },
      ty_uint(uint_ty) => match uint_ty {
        ast::ty_u8  => int_ty_set(INT_TY_SET_u8),
        ast::ty_u16 => int_ty_set(INT_TY_SET_u16),
        ast::ty_u32 => int_ty_set(INT_TY_SET_u32),
        ast::ty_u64 => int_ty_set(INT_TY_SET_u64),
        ast::ty_u   => int_ty_set(INT_TY_SET_u)
      },
      _ => tcx.sess.bug(~"non-integral type passed to \
                          convert_integral_ty_to_int_ty_set()")
    }
}
