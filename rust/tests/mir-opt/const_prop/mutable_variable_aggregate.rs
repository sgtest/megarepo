// unit-test: ConstProp

// EMIT_MIR mutable_variable_aggregate.main.ConstProp.diff
fn main() {
    // CHECK-LABEL: fn main(
    // CHECK: debug x => [[x:_.*]];
    // CHECK: debug y => [[y:_.*]];
    // CHECK: [[x]] = const (42_i32, 43_i32);
    // CHECK: ([[x]].1: i32) = const 99_i32;
    // CHECK: [[y]] = const (42_i32, 99_i32);
    let mut x = (42, 43);
    x.1 = 99;
    let y = x;
}
