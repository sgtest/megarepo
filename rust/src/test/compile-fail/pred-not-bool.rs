// -*- rust -*-

// error-pattern: Non-boolean return type

// this checks that a pred with a non-bool return
// type is rejected, even if the pred is never used

pred bad(a: int) -> int { ret 37; }

fn main() { }
