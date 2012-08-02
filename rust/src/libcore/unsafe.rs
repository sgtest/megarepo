//! Unsafe operations

export reinterpret_cast, forget, bump_box_refcount, transmute;

#[abi = "rust-intrinsic"]
extern mod rusti {
    fn forget<T>(-x: T);
    fn reinterpret_cast<T, U>(e: T) -> U;
}

/// Casts the value at `src` to U. The two types must have the same length.
#[inline(always)]
unsafe fn reinterpret_cast<T, U>(src: T) -> U {
    rusti::reinterpret_cast(src)
}

/**
 * Move a thing into the void
 *
 * The forget function will take ownership of the provided value but neglect
 * to run any required cleanup or memory-management operations on it. This
 * can be used for various acts of magick, particularly when using
 * reinterpret_cast on managed pointer types.
 */
#[inline(always)]
unsafe fn forget<T>(-thing: T) { rusti::forget(thing); }

/**
 * Force-increment the reference count on a shared box. If used
 * uncarefully, this can leak the box. Use this in conjunction with transmute
 * and/or reinterpret_cast when such calls would otherwise scramble a box's
 * reference count
 */
unsafe fn bump_box_refcount<T>(+t: @T) { forget(t); }

/**
 * Transform a value of one type into a value of another type.
 * Both types must have the same size and alignment.
 *
 * # Example
 *
 *     assert transmute("L") == ~[76u8, 0u8];
 */
unsafe fn transmute<L, G>(-thing: L) -> G {
    let newthing = reinterpret_cast(thing);
    forget(thing);
    return newthing;
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_reinterpret_cast() {
        assert unsafe { reinterpret_cast(1) } == 1u;
    }

    #[test]
    fn test_bump_box_refcount() {
        unsafe {
            let box = @~"box box box";       // refcount 1
            bump_box_refcount(box);         // refcount 2
            let ptr: *int = transmute(box); // refcount 2
            let _box1: @~str = reinterpret_cast(ptr);
            let _box2: @~str = reinterpret_cast(ptr);
            assert *_box1 == ~"box box box";
            assert *_box2 == ~"box box box";
            // Will destroy _box1 and _box2. Without the bump, this would
            // use-after-free. With too many bumps, it would leak.
        }
    }

    #[test]
    fn test_transmute() {
        unsafe {
            let x = @1;
            let x: *int = transmute(x);
            assert *x == 1;
            let _x: @int = transmute(x);
        }
    }

    #[test]
    fn test_transmute2() {
        unsafe {
            assert transmute(~"L") == ~[76u8, 0u8];
        }
    }
}
