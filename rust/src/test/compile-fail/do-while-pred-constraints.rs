// error-pattern:unsatisfied precondition constraint (for example, even(y

fn print_even(y: int) : even(y) { log(debug, y); }

pure fn even(y: int) -> bool { true }

fn main() {
    let y: int = 42;
    check (even(y));
    loop {
        print_even(y);
        do  { do  { do  { y += 1; } while false } while false } while false
    }
}
