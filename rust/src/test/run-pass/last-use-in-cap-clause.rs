// Make sure #1399 stays fixed

fn foo() -> fn@() -> int {
    let k = ~22;
    let _u = {a: copy k};
    return fn@(move k) -> int { 22 };
}

fn main() {
    assert foo()() == 22;
}
