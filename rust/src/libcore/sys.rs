//! Misc low level stuff

export type_desc;
export get_type_desc;
export size_of;
export min_align_of;
export pref_align_of;
export refcount;
export log_str;
export little_lock, methods;
export shape_eq, shape_lt, shape_le;

import task::atomically;

enum type_desc = {
    size: uint,
    align: uint
    // Remaining fields not listed
};

type rust_little_lock = *libc::c_void;

#[abi = "cdecl"]
extern mod rustrt {
    pure fn shape_log_str(t: *sys::type_desc, data: *()) -> ~str;

    fn rust_create_little_lock() -> rust_little_lock;
    fn rust_destroy_little_lock(lock: rust_little_lock);
    fn rust_lock_little_lock(lock: rust_little_lock);
    fn rust_unlock_little_lock(lock: rust_little_lock);
}

#[abi = "rust-intrinsic"]
extern mod rusti {
    fn get_tydesc<T>() -> *();
    fn size_of<T>() -> uint;
    fn pref_align_of<T>() -> uint;
    fn min_align_of<T>() -> uint;
}

/// Compares contents of two pointers using the default method.
/// Equivalent to `*x1 == *x2`.  Useful for hashtables.
pure fn shape_eq<T>(x1: &T, x2: &T) -> bool {
    *x1 == *x2
}

pure fn shape_lt<T>(x1: &T, x2: &T) -> bool {
    *x1 < *x2
}

pure fn shape_le<T>(x1: &T, x2: &T) -> bool {
    *x1 < *x2
}

/**
 * Returns a pointer to a type descriptor.
 *
 * Useful for calling certain function in the Rust runtime or otherwise
 * performing dark magick.
 */
pure fn get_type_desc<T>() -> *type_desc {
    unchecked { rusti::get_tydesc::<T>() as *type_desc }
}

/// Returns the size of a type
#[inline(always)]
pure fn size_of<T>() -> uint {
    unchecked { rusti::size_of::<T>() }
}

/**
 * Returns the ABI-required minimum alignment of a type
 *
 * This is the alignment used for struct fields. It may be smaller
 * than the preferred alignment.
 */
pure fn min_align_of<T>() -> uint {
    unchecked { rusti::min_align_of::<T>() }
}

/// Returns the preferred alignment of a type
pure fn pref_align_of<T>() -> uint {
    unchecked { rusti::pref_align_of::<T>() }
}

/// Returns the refcount of a shared box (as just before calling this)
pure fn refcount<T>(+t: @T) -> uint {
    unsafe {
        let ref_ptr: *uint = unsafe::reinterpret_cast(t);
        *ref_ptr - 1
    }
}

pure fn log_str<T>(t: T) -> ~str {
    unsafe {
        let data_ptr: *() = unsafe::reinterpret_cast(ptr::addr_of(t));
        rustrt::shape_log_str(get_type_desc::<T>(), data_ptr)
    }
}

class little_lock {
    let l: rust_little_lock;
    new() {
        self.l = rustrt::rust_create_little_lock();
    }
    drop { rustrt::rust_destroy_little_lock(self.l); }
}

impl methods for little_lock {
    unsafe fn lock<T>(f: fn() -> T) -> T {
        class unlock {
            let l: rust_little_lock;
            new(l: rust_little_lock) { self.l = l; }
            drop { rustrt::rust_unlock_little_lock(self.l); }
        }

        do atomically {
            rustrt::rust_lock_little_lock(self.l);
            let _r = unlock(self.l);
            f()
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn size_of_basic() {
        assert size_of::<u8>() == 1u;
        assert size_of::<u16>() == 2u;
        assert size_of::<u32>() == 4u;
        assert size_of::<u64>() == 8u;
    }

    #[test]
    #[cfg(target_arch = "x86")]
    #[cfg(target_arch = "arm")]
    fn size_of_32() {
        assert size_of::<uint>() == 4u;
        assert size_of::<*uint>() == 4u;
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn size_of_64() {
        assert size_of::<uint>() == 8u;
        assert size_of::<*uint>() == 8u;
    }

    #[test]
    fn align_of_basic() {
        assert pref_align_of::<u8>() == 1u;
        assert pref_align_of::<u16>() == 2u;
        assert pref_align_of::<u32>() == 4u;
    }

    #[test]
    #[cfg(target_arch = "x86")]
    #[cfg(target_arch = "arm")]
    fn align_of_32() {
        assert pref_align_of::<uint>() == 4u;
        assert pref_align_of::<*uint>() == 4u;
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn align_of_64() {
        assert pref_align_of::<uint>() == 8u;
        assert pref_align_of::<*uint>() == 8u;
    }
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
