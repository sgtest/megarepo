// error-pattern:assigning to variable declared in an outer block
// Make sure we can't write to upvars from fn@s
fn main() {
    let i = 0;
    let ctr = fn@ () -> int { i = i + 1; ret i; };
    log(error, ctr());
    log(error, ctr());
    log(error, ctr());
    log(error, ctr());
    log(error, ctr());
    log(error, i);
}
