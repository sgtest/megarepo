#![feature(const_ptr_offset_from)]
#![feature(core_intrinsics)]

use std::intrinsics::{ptr_offset_from, ptr_offset_from_unsigned};

#[repr(C)]
struct Struct {
    data: u8,
    field: u8,
}

pub const DIFFERENT_ALLOC: usize = {
    let uninit = std::mem::MaybeUninit::<Struct>::uninit();
    let base_ptr: *const Struct = &uninit as *const _ as *const Struct;
    let uninit2 = std::mem::MaybeUninit::<Struct>::uninit();
    let field_ptr: *const Struct = &uninit2 as *const _ as *const Struct;
    let offset = unsafe { ptr_offset_from(field_ptr, base_ptr) }; //~ERROR evaluation of constant value failed
    //~| ptr_offset_from cannot compute offset of pointers into different allocations.
    offset as usize
};

pub const NOT_PTR: usize = {
    unsafe { (42 as *const u8).offset_from(&5u8) as usize }
};

pub const NOT_MULTIPLE_OF_SIZE: isize = {
    let data = [5u8, 6, 7];
    let base_ptr = data.as_ptr();
    let field_ptr = &data[1] as *const u8 as *const u16;
    unsafe { ptr_offset_from(field_ptr, base_ptr as *const u16) } //~ERROR evaluation of constant value failed
    //~| 1_isize cannot be divided by 2_isize without remainder
};

pub const OFFSET_FROM_NULL: isize = {
    let ptr = 0 as *const u8;
    unsafe { ptr_offset_from(ptr, ptr) } //~ERROR evaluation of constant value failed
    //~| null pointer is not a valid pointer
};

pub const DIFFERENT_INT: isize = { // offset_from with two different integers: like DIFFERENT_ALLOC
    let ptr1 = 8 as *const u8;
    let ptr2 = 16 as *const u8;
    unsafe { ptr_offset_from(ptr2, ptr1) } //~ERROR evaluation of constant value failed
    //~| 0x10 is not a valid pointer
};

const OUT_OF_BOUNDS_1: isize = {
    let start_ptr = &4 as *const _ as *const u8;
    let length = 10;
    let end_ptr = (start_ptr).wrapping_add(length);
    // First ptr is out of bounds
    unsafe { ptr_offset_from(end_ptr, start_ptr) } //~ERROR evaluation of constant value failed
    //~| pointer at offset 10 is out-of-bounds
};

const OUT_OF_BOUNDS_2: isize = {
    let start_ptr = &4 as *const _ as *const u8;
    let length = 10;
    let end_ptr = (start_ptr).wrapping_add(length);
    // Second ptr is out of bounds
    unsafe { ptr_offset_from(start_ptr, end_ptr) } //~ERROR evaluation of constant value failed
    //~| pointer at offset 10 is out-of-bounds
};

const OUT_OF_BOUNDS_SAME: isize = {
    let start_ptr = &4 as *const _ as *const u8;
    let length = 10;
    let end_ptr = (start_ptr).wrapping_add(length);
    unsafe { ptr_offset_from(end_ptr, end_ptr) } //~ERROR evaluation of constant value failed
    //~| pointer at offset 10 is out-of-bounds
};

pub const DIFFERENT_ALLOC_UNSIGNED: usize = {
    let uninit = std::mem::MaybeUninit::<Struct>::uninit();
    let base_ptr: *const Struct = &uninit as *const _ as *const Struct;
    let uninit2 = std::mem::MaybeUninit::<Struct>::uninit();
    let field_ptr: *const Struct = &uninit2 as *const _ as *const Struct;
    let offset = unsafe { ptr_offset_from_unsigned(field_ptr, base_ptr) }; //~ERROR evaluation of constant value failed
    //~| ptr_offset_from_unsigned cannot compute offset of pointers into different allocations.
    offset as usize
};

const WRONG_ORDER_UNSIGNED: usize = {
    let a = ['a', 'b', 'c'];
    let p = a.as_ptr();
    unsafe { ptr_offset_from_unsigned(p, p.add(2) ) } //~ERROR evaluation of constant value failed
    //~| cannot compute a negative offset, but 0 < 8
};

fn main() {}
