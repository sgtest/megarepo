fn to_lambda(f: fn@(uint) -> uint) -> fn@(uint) -> uint {
    return f;
}

fn main() {
    let x: fn@(uint) -> uint = to_lambda(|x| x * 2u );
    let y = to_lambda(x);

    let x_r = x(22u);
    let y_r = y(x_r);

    assert x_r == 44u;
    assert y_r == 88u;
}
