

fn foo<T>(o: myoption<T>) -> int {
    let mut x: int = 5;
    alt o { none::<T> => { } some::<T>(t) => { x += 1; } }
    return x;
}

enum myoption<T> { none, some(T), }

fn main() { log(debug, 5); }
