// error-pattern: mismatched types

fn main() {
    let v = [mutable [mutable 0]];

    fn f(&&v: [mutable [mutable? int]]) {
        v[0] = [3]
    }

    f(v);
}
