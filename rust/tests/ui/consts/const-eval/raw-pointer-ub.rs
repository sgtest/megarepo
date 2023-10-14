// normalize-stderr-test "alloc\d+" -> "allocN"
#![feature(const_pointer_byte_offsets)]
#![feature(pointer_byte_offsets)]
#![feature(const_mut_refs)]

const MISALIGNED_LOAD: () = unsafe {
    let mem = [0u32; 8];
    let ptr = mem.as_ptr().byte_add(1);
    let _val = *ptr; //~ERROR: evaluation of constant value failed
    //~^NOTE: accessing memory with alignment 1, but alignment 4 is required
};

const MISALIGNED_STORE: () = unsafe {
    let mut mem = [0u32; 8];
    let ptr = mem.as_mut_ptr().byte_add(1);
    *ptr = 0; //~ERROR: evaluation of constant value failed
    //~^NOTE: accessing memory with alignment 1, but alignment 4 is required
};

const MISALIGNED_COPY: () = unsafe {
    let x = &[0_u8; 4];
    let y = x.as_ptr().cast::<u32>();
    let mut z = 123;
    y.copy_to_nonoverlapping(&mut z, 1);
    //~^NOTE
    // The actual error points into the implementation of `copy_to_nonoverlapping`.
};

const OOB: () = unsafe {
    let mem = [0u32; 1];
    let ptr = mem.as_ptr().cast::<u64>();
    let _val = *ptr; //~ERROR: evaluation of constant value failed
    //~^NOTE: size 4, so pointer to 8 bytes starting at offset 0 is out-of-bounds
};

fn main() {}
