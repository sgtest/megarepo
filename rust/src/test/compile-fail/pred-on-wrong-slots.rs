// xfail-stage0
// -*- rust -*-

// error-pattern: lt(a, c)

fn f(int a, int b) : lt(a,b) {
}

pred lt(int a, int b) -> bool {
  ret a < b;
}

fn main() {
  let int a = 10;
  let int b = 23;
  let int c = 77;
  check lt(a,b);
  check lt(b,c);
  f(a,b);
  f(a,c);
}
