// skip-filecheck
// EMIT_MIR_FOR_EACH_PANIC_STRATEGY
// unit-test: ConstProp
// compile-flags: -Zmir-enable-passes=+InstSimplify
// EMIT_MIR_FOR_EACH_BIT_WIDTH

// EMIT_MIR slice_len.main.ConstProp.diff
fn main() {
    (&[1u32, 2, 3] as &[u32])[1];
}
