// xfail-stage0
// error-pattern:assigning to immutable obj field
obj objy(x: int) {
    fn foo() { x = 5; }
}
fn main() { }