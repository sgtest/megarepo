fn main() unsafe {
    let a: uint = 1u;
    let b: uint = 4u;
    check (uint::le(a, b));
    log(debug, str::unsafe::slice_bytes_safe_range("kitties", a, b));
}
