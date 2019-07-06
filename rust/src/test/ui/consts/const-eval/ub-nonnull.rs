#![feature(rustc_attrs, const_transmute)]
#![allow(const_err)] // make sure we cannot allow away the errors tested here

use std::mem;
use std::ptr::NonNull;
use std::num::{NonZeroU8, NonZeroUsize};

const NON_NULL: NonNull<u8> = unsafe { mem::transmute(1usize) };
const NON_NULL_PTR: NonNull<u8> = unsafe { mem::transmute(&1) };

const NULL_PTR: NonNull<u8> = unsafe { mem::transmute(0usize) };
//~^ ERROR it is undefined behavior to use this value

const OUT_OF_BOUNDS_PTR: NonNull<u8> = { unsafe {
//~^ ERROR it is undefined behavior to use this value
    let ptr: &(u8, u8, u8) = mem::transmute(&0u8); // &0 gets promoted so it does not dangle
    let out_of_bounds_ptr = &ptr.2; // use address-of-field for pointer arithmetic
    mem::transmute(out_of_bounds_ptr)
} };

const NULL_U8: NonZeroU8 = unsafe { mem::transmute(0u8) };
//~^ ERROR it is undefined behavior to use this value
const NULL_USIZE: NonZeroUsize = unsafe { mem::transmute(0usize) };
//~^ ERROR it is undefined behavior to use this value

union Transmute {
    uninit: (),
    out: NonZeroU8,
}
const UNINIT: NonZeroU8 = unsafe { Transmute { uninit: () }.out };
//~^ ERROR it is undefined behavior to use this value

// Also test other uses of rustc_layout_scalar_valid_range_start

#[rustc_layout_scalar_valid_range_start(10)]
#[rustc_layout_scalar_valid_range_end(30)]
struct RestrictedRange1(u32);
const BAD_RANGE1: RestrictedRange1 = unsafe { RestrictedRange1(42) };
//~^ ERROR it is undefined behavior to use this value

#[rustc_layout_scalar_valid_range_start(30)]
#[rustc_layout_scalar_valid_range_end(10)]
struct RestrictedRange2(u32);
const BAD_RANGE2: RestrictedRange2 = unsafe { RestrictedRange2(20) };
//~^ ERROR it is undefined behavior to use this value

fn main() {}
