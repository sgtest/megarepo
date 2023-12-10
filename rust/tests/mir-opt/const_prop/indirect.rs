// EMIT_MIR_FOR_EACH_PANIC_STRATEGY
// unit-test: ConstProp
// compile-flags: -C overflow-checks=on

// EMIT_MIR indirect.main.ConstProp.diff
fn main() {
    // CHECK-LABEL: fn main(
    // CHECK: debug x => [[x:_.*]];
    // CHECK: [[x]] = const 3_u8;
    let x = (2u32 as u8) + 1;
}
