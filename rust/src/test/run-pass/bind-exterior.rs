fn foo(@int a, @int b) -> int {
  ret a+b;
}

fn main() {
  auto f1 = bind foo(@10, @12);
  check(f1() == 22);
}
