// xfail-stage0
// -*- rust -*-

use std;

import std::task::*;

fn main() {
  auto other = spawn child();
  log_err "1";
  yield();
  join(other);
  log_err "3";
}

fn child() {
  log_err "2";
}
