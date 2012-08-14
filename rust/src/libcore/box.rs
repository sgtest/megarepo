//! Operations on shared box types

// NB: transitionary, de-mode-ing.
#[forbid(deprecated_mode)];
#[forbid(deprecated_pattern)];

export ptr_eq;

pure fn ptr_eq<T>(a: @T, b: @T) -> bool {
    //! Determine if two shared boxes point to the same object
    unsafe { ptr::addr_of(*a) == ptr::addr_of(*b) }
}

#[test]
fn test() {
    let x = @3;
    let y = @3;
    assert (ptr_eq::<int>(x, x));
    assert (ptr_eq::<int>(y, y));
    assert (!ptr_eq::<int>(x, y));
    assert (!ptr_eq::<int>(y, x));
}
