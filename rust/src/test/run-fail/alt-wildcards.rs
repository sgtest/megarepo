// error-pattern:squirrelcupcake
fn cmp() -> int {
    alt check (option::some('a'), option::none::<char>) {
        (option::some(_), _) => { fail ~"squirrelcupcake"; }
        (_, option::some(_)) => { fail; }
    }
}

fn main() { log(error, cmp()); }
