use super::alloc::copy_to_userspace;
use super::alloc::User;

#[test]
fn test_copy_function() {
    let mut src = [0u8; 100];
    let mut dst = User::<[u8]>::uninitialized(100);

    for i in 0..src.len() {
        src[i] = i as _;
    }

    for size in 0..48 {
        // For all possible alignment
        for offset in 0..8 {
            // overwrite complete dst
            dst.copy_from_enclave(&[0u8; 100]);

            // Copy src[0..size] to dst + offset
            unsafe { copy_to_userspace(src.as_ptr(), dst.as_mut_ptr().offset(offset), size) };

            // Verify copy
            for byte in 0..size {
                unsafe {
                    assert_eq!(*dst.as_ptr().offset(offset + byte as isize), src[byte as usize]);
                }
            }
        }
    }
}
