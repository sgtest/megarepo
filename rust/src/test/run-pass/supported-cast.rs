// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

fn main() {
  let f = 1 as *libc::FILE;
  log(debug, f as int);
  log(debug, f as uint);
  log(debug, f as i8);
  log(debug, f as i16);
  log(debug, f as i32);
  log(debug, f as i64);
  log(debug, f as u8);
  log(debug, f as u16);
  log(debug, f as u32);
  log(debug, f as u64);

  log(debug, 1 as int);
  log(debug, 1 as uint);
  log(debug, 1 as float);
  log(debug, 1 as bool);
  log(debug, 1 as *libc::FILE);
  log(debug, 1 as i8);
  log(debug, 1 as i16);
  log(debug, 1 as i32);
  log(debug, 1 as i64);
  log(debug, 1 as u8);
  log(debug, 1 as u16);
  log(debug, 1 as u32);
  log(debug, 1 as u64);
  log(debug, 1 as f32);
  log(debug, 1 as f64);

  log(debug, 1u as int);
  log(debug, 1u as uint);
  log(debug, 1u as float);
  log(debug, 1u as bool);
  log(debug, 1u as *libc::FILE);
  log(debug, 1u as i8);
  log(debug, 1u as i16);
  log(debug, 1u as i32);
  log(debug, 1u as i64);
  log(debug, 1u as u8);
  log(debug, 1u as u16);
  log(debug, 1u as u32);
  log(debug, 1u as u64);
  log(debug, 1u as f32);
  log(debug, 1u as f64);

  log(debug, 1i8 as int);
  log(debug, 1i8 as uint);
  log(debug, 1i8 as float);
  log(debug, 1i8 as bool);
  log(debug, 1i8 as *libc::FILE);
  log(debug, 1i8 as i8);
  log(debug, 1i8 as i16);
  log(debug, 1i8 as i32);
  log(debug, 1i8 as i64);
  log(debug, 1i8 as u8);
  log(debug, 1i8 as u16);
  log(debug, 1i8 as u32);
  log(debug, 1i8 as u64);
  log(debug, 1i8 as f32);
  log(debug, 1i8 as f64);

  log(debug, 1u8 as int);
  log(debug, 1u8 as uint);
  log(debug, 1u8 as float);
  log(debug, 1u8 as bool);
  log(debug, 1u8 as *libc::FILE);
  log(debug, 1u8 as i8);
  log(debug, 1u8 as i16);
  log(debug, 1u8 as i32);
  log(debug, 1u8 as i64);
  log(debug, 1u8 as u8);
  log(debug, 1u8 as u16);
  log(debug, 1u8 as u32);
  log(debug, 1u8 as u64);
  log(debug, 1u8 as f32);
  log(debug, 1u8 as f64);

  log(debug, 1i16 as int);
  log(debug, 1i16 as uint);
  log(debug, 1i16 as float);
  log(debug, 1i16 as bool);
  log(debug, 1i16 as *libc::FILE);
  log(debug, 1i16 as i8);
  log(debug, 1i16 as i16);
  log(debug, 1i16 as i32);
  log(debug, 1i16 as i64);
  log(debug, 1i16 as u8);
  log(debug, 1i16 as u16);
  log(debug, 1i16 as u32);
  log(debug, 1i16 as u64);
  log(debug, 1i16 as f32);
  log(debug, 1i16 as f64);

  log(debug, 1u16 as int);
  log(debug, 1u16 as uint);
  log(debug, 1u16 as float);
  log(debug, 1u16 as bool);
  log(debug, 1u16 as *libc::FILE);
  log(debug, 1u16 as i8);
  log(debug, 1u16 as i16);
  log(debug, 1u16 as i32);
  log(debug, 1u16 as i64);
  log(debug, 1u16 as u8);
  log(debug, 1u16 as u16);
  log(debug, 1u16 as u32);
  log(debug, 1u16 as u64);
  log(debug, 1u16 as f32);
  log(debug, 1u16 as f64);

  log(debug, 1i32 as int);
  log(debug, 1i32 as uint);
  log(debug, 1i32 as float);
  log(debug, 1i32 as bool);
  log(debug, 1i32 as *libc::FILE);
  log(debug, 1i32 as i8);
  log(debug, 1i32 as i16);
  log(debug, 1i32 as i32);
  log(debug, 1i32 as i64);
  log(debug, 1i32 as u8);
  log(debug, 1i32 as u16);
  log(debug, 1i32 as u32);
  log(debug, 1i32 as u64);
  log(debug, 1i32 as f32);
  log(debug, 1i32 as f64);

  log(debug, 1u32 as int);
  log(debug, 1u32 as uint);
  log(debug, 1u32 as float);
  log(debug, 1u32 as bool);
  log(debug, 1u32 as *libc::FILE);
  log(debug, 1u32 as i8);
  log(debug, 1u32 as i16);
  log(debug, 1u32 as i32);
  log(debug, 1u32 as i64);
  log(debug, 1u32 as u8);
  log(debug, 1u32 as u16);
  log(debug, 1u32 as u32);
  log(debug, 1u32 as u64);
  log(debug, 1u32 as f32);
  log(debug, 1u32 as f64);

  log(debug, 1i64 as int);
  log(debug, 1i64 as uint);
  log(debug, 1i64 as float);
  log(debug, 1i64 as bool);
  log(debug, 1i64 as *libc::FILE);
  log(debug, 1i64 as i8);
  log(debug, 1i64 as i16);
  log(debug, 1i64 as i32);
  log(debug, 1i64 as i64);
  log(debug, 1i64 as u8);
  log(debug, 1i64 as u16);
  log(debug, 1i64 as u32);
  log(debug, 1i64 as u64);
  log(debug, 1i64 as f32);
  log(debug, 1i64 as f64);

  log(debug, 1u64 as int);
  log(debug, 1u64 as uint);
  log(debug, 1u64 as float);
  log(debug, 1u64 as bool);
  log(debug, 1u64 as *libc::FILE);
  log(debug, 1u64 as i8);
  log(debug, 1u64 as i16);
  log(debug, 1u64 as i32);
  log(debug, 1u64 as i64);
  log(debug, 1u64 as u8);
  log(debug, 1u64 as u16);
  log(debug, 1u64 as u32);
  log(debug, 1u64 as u64);
  log(debug, 1u64 as f32);
  log(debug, 1u64 as f64);

  log(debug, 1u64 as int);
  log(debug, 1u64 as uint);
  log(debug, 1u64 as float);
  log(debug, 1u64 as bool);
  log(debug, 1u64 as *libc::FILE);
  log(debug, 1u64 as i8);
  log(debug, 1u64 as i16);
  log(debug, 1u64 as i32);
  log(debug, 1u64 as i64);
  log(debug, 1u64 as u8);
  log(debug, 1u64 as u16);
  log(debug, 1u64 as u32);
  log(debug, 1u64 as u64);
  log(debug, 1u64 as f32);
  log(debug, 1u64 as f64);

  log(debug, true as int);
  log(debug, true as uint);
  log(debug, true as float);
  log(debug, true as bool);
  log(debug, true as *libc::FILE);
  log(debug, true as i8);
  log(debug, true as i16);
  log(debug, true as i32);
  log(debug, true as i64);
  log(debug, true as u8);
  log(debug, true as u16);
  log(debug, true as u32);
  log(debug, true as u64);
  log(debug, true as f32);
  log(debug, true as f64);

  log(debug, 1. as int);
  log(debug, 1. as uint);
  log(debug, 1. as float);
  log(debug, 1. as bool);
  log(debug, 1. as i8);
  log(debug, 1. as i16);
  log(debug, 1. as i32);
  log(debug, 1. as i64);
  log(debug, 1. as u8);
  log(debug, 1. as u16);
  log(debug, 1. as u32);
  log(debug, 1. as u64);
  log(debug, 1. as f32);
  log(debug, 1. as f64);

  log(debug, 1f32 as int);
  log(debug, 1f32 as uint);
  log(debug, 1f32 as float);
  log(debug, 1f32 as bool);
  log(debug, 1f32 as i8);
  log(debug, 1f32 as i16);
  log(debug, 1f32 as i32);
  log(debug, 1f32 as i64);
  log(debug, 1f32 as u8);
  log(debug, 1f32 as u16);
  log(debug, 1f32 as u32);
  log(debug, 1f32 as u64);
  log(debug, 1f32 as f32);
  log(debug, 1f32 as f64);

  log(debug, 1f64 as int);
  log(debug, 1f64 as uint);
  log(debug, 1f64 as float);
  log(debug, 1f64 as bool);
  log(debug, 1f64 as i8);
  log(debug, 1f64 as i16);
  log(debug, 1f64 as i32);
  log(debug, 1f64 as i64);
  log(debug, 1f64 as u8);
  log(debug, 1f64 as u16);
  log(debug, 1f64 as u32);
  log(debug, 1f64 as u64);
  log(debug, 1f64 as f32);
  log(debug, 1f64 as f64);
}
