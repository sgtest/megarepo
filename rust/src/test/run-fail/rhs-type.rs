


// Tests that trans treats the rhs of pth's decl
// as a _|_-typed thing, not a str-typed thing
// xfail-stage0
// error-pattern:bye
fn main() { let pth = fail "bye"; let rs: {t: str} = {t: pth}; }