fn f<T: Copy>(t: T) -> T {
    let t1 = t;
    t1
}

fn main() {
    let t = f(~100);
    assert t == ~100;
    let t = f(~@~[100]);
    assert t == ~@~[100];
}
