//@compile-flags: -Zmir-opt-level=3
// Enable MIR inlining to ensure that `TerminatorKind::Terminate` is generated
// instead of just `UnwindAction::Terminate`.

#![feature(c_unwind)]

struct Foo;

impl Drop for Foo {
    fn drop(&mut self) {}
}

#[inline(always)]
fn has_cleanup() {
    let _f = Foo;
    panic!();
}

extern "C" fn panic_abort() {
    has_cleanup();
    //~^ ERROR: panic in a function that cannot unwind
}

fn main() {
    panic_abort();
}
