use super::abi;
use crate::alloc::{GlobalAlloc, Layout, System};

#[stable(feature = "alloc_system_type", since = "1.28.0")]
unsafe impl GlobalAlloc for System {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        abi::sys_alloc_aligned(layout.size(), layout.align())
    }

    #[inline]
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // this allocator never deallocates memory
    }
}
