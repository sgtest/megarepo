// xfail-stage0
// error-pattern: non-type context
type base =
  obj {
    fn foo();
  };
obj derived() {
  fn foo() {}
  fn bar() {}
}
fn main() {
  let derived d = derived();
  let base b = base(d);
}
