// xfail-boot
fn main() {

  obj foo() {
      fn m1() {
          log "hi!";
      }
      fn m2() {
          self.m1();
      }
  }
  
  auto a = foo();
  a.m1();
  a.m2();
}
