// xfail-stage0
// xfail-stage1
// xfail-stage2
// error-pattern: attempted dynamic environment-capture
fn foo[T]() { obj bar(T b) {} }
fn main() {}