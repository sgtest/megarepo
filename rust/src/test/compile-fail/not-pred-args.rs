// -*- rust -*-

// error-pattern:constraint args must be

pure fn f(q: int) -> bool { ret true; }

fn main() {
    // should fail to typecheck, as pred args must be slot variables
    // or literals
    check (f(42 * 17));
}
