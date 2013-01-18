// Test that expected type propagates through `{}` expressions.  If it
// did not, then the type of `x` would not be known and a compilation
// error would result.

fn main() {
    let y = ~3;
    let foo: @fn(&int) -> int = {
        let y = copy y;
        |x| *x + *y
    };
    assert foo(@22) == 25;
}