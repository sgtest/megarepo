fn main() {
    let a = ~[~mut 10];
    let b = copy a;

    assert *a[0] == 10;
    assert *b[0] == 10;

    // This should only modify the value in a, not b
    *a[0] = 20;

    assert *a[0] == 20;
    assert *b[0] == 10;
}
