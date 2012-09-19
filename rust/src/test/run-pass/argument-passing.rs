// xfail-fast
#[legacy_modes];

fn f1(a: {mut x: int}, &b: int, -c: int) -> int {
    let r = a.x + b + c;
    a.x = 0;
    b = 10;
    return r;
}

fn f2(a: int, f: fn(int)) -> int { f(1); return a; }

fn main() {
    let mut a = {mut x: 1}, b = 2, c = 3;
    assert (f1(a, b, c) == 6);
    assert (a.x == 0);
    assert (b == 10);
    assert (f2(a.x, |x| a.x = 50 ) == 0);
    assert (a.x == 50);
}
