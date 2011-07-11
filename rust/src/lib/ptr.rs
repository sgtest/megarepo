// Unsafe pointer utility functions.

native "rust-intrinsic" mod rusti {
    fn addr_of[T](&mutable T val) -> *mutable T;
    fn ptr_offset[T](*T ptr, uint count) -> *T;
}

fn addr_of[T](&mutable T val) -> *mutable T { ret rusti::addr_of(val); }
fn offset[T](*T ptr, uint count) -> *T { ret rusti::ptr_offset(ptr, count); }

