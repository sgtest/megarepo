// xfail-stage0
// xfail-stage1
// xfail-stage2
use std;
use libc();
use zed(name = "std");
use bar(name = "std", ver = "0.0.1");

// FIXME: commented out since resolve doesn't know how to handle crates yet.
// import std::str;
// import x = std::str;

mod baz {
  use std;
  use libc();
  use zed(name = "std");
  use bar(name = "std", ver = "0.0.1");

  // import std::str;
  // import x = std::str;
}

fn main() {
}
