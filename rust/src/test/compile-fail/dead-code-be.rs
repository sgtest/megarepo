// xfail-stage0
// -*- rust -*-

// error-pattern: dead

fn f(str caller) {
  log caller;
}

fn main() {
  be f("main");
  log "Paul is dead";
}

