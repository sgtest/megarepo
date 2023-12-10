// EMIT_MIR_FOR_EACH_PANIC_STRATEGY
// unit-test: ConstProp

#[inline(never)]
fn read(_: usize) { }

// EMIT_MIR pointer_expose_address.main.ConstProp.diff
fn main() {
    // CHECK-LABEL: fn main(
    // CHECK: [[ptr:_.*]] = const _;
    // CHECK: [[ref:_.*]] = &raw const (*[[ptr]]);
    // CHECK: [[x:_.*]] = move [[ref]] as usize (PointerExposeAddress);
    // CHECK: [[arg:_.*]] = [[x]];
    // CHECK: = read(move [[arg]])
    const FOO: &i32 = &1;
    let x = FOO as *const i32 as usize;
    read(x);
}
