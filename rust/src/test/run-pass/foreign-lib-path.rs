// xfail-test FIXME I don't know how to test this (#2604)
// compile-flags:-L.
// The -L flag is also used for linking foreign libraries

// FIXME: I want to name a mod that would not link successfully
// wouthout providing a -L argument to the compiler, and that
// will also be found successfully at runtime.
extern mod WHATGOESHERE {
    #[legacy_exports];
    fn IDONTKNOW() -> u32;
}

fn main() {
    assert IDONTKNOW() == 0x_BAD_DOOD_u32;
}