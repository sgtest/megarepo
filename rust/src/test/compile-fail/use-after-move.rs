// error-pattern: Unsatisfied precondition constraint (for example, init(x
fn main() { let x = @5; let y <- x; log_full(core::debug, *x); }
