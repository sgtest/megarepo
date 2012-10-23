fn test(x: bool, foo: @{x: int, y: int, z: int}) -> int {
    let bar = foo;
    let mut y: @{x: int, y: int, z: int};
    if x { y = move bar; } else { y = @{x: 4, y: 5, z: 6}; }
    return y.y;
}

fn main() {
    let x = @{x: 1, y: 2, z: 3};
    assert (test(true, x) == 2);
    assert (test(true, x) == 2);
    assert (test(true, x) == 2);
    assert (test(false, x) == 5);
}
