// error-pattern: stack closure type can only appear

fn lol(f: fn()) -> fn() { return f; }
fn main() {
    let i = 8;
    let f = lol(fn&() { log(error, i); });
    f();
}
