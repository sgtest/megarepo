// -*- rust -*-

obj counter(mutable int x) {
  fn hello() -> int {
    ret 12345;
  }
  fn incr() {
    x = x + 1;
  }
  fn get() -> int {
    ret x;
  }
}

fn main() {
  auto y = counter(0);
  assert (y.hello() == 12345);
  log y.get();
  y.incr();
  y.incr();
  log y.get();
  assert (y.get() == 2);
}
