trait to_str {
    fn to_str() -> ~str;
}
impl of to_str for int {
    fn to_str() -> ~str { int::str(self) }
}
impl of to_str for ~str {
    fn to_str() -> ~str { self }
}
impl of to_str for () {
    fn to_str() -> ~str { ~"()" }
}

trait map<T> {
    fn map<U>(f: fn(T) -> U) -> ~[U];
}
impl <T> of map<T> for ~[T] {
    fn map<U>(f: fn(T) -> U) -> ~[U] {
        let mut r = ~[];
        for self.each |x| { r += ~[f(x)]; }
        r
    }
}

fn foo<U, T: map<U>>(x: T) -> ~[~str] {
    x.map(|_e| ~"hi" )
}
fn bar<U: to_str, T: map<U>>(x: T) -> ~[~str] {
    x.map(|_e| _e.to_str() )
}

fn main() {
    assert foo(~[1]) == ~[~"hi"];
    assert bar::<int, ~[int]>(~[4, 5]) == ~[~"4", ~"5"];
    assert bar::<~str, ~[~str]>(~[~"x", ~"y"]) == ~[~"x", ~"y"];
    assert bar::<(), ~[()]>(~[()]) == ~[~"()"];
}
