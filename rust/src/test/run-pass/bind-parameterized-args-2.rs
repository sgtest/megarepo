fn main() {
    fn echo<T>(c: int, x: native fn(T)) { #error("wee"); }

    let y = bind echo(42, _);

    y(fn(&&i: str) { });
}
