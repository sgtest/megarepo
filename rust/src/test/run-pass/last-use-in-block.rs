// Issue #1818

fn lp<T>(s: ~str, f: fn(~str) -> T) -> T {
    while false {
        let r = f(s);
        return (move r);
    }
    fail;
}

fn apply<T>(s: ~str, f: fn(~str) -> T) -> T {
    fn g<T>(s: ~str, f: fn(~str) -> T) -> T {f(s)}
    g(s, |v| { let r = f(v); move r })
}

fn main() {}
