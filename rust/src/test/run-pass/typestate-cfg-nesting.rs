

fn f() {
    let x = 10; let mut y = 11;
    if true { match x { _ => { y = x; } } } else { }
}

fn main() {
    let x = 10;
    let mut y = 11;
    if true { while false { y = x; } } else { }
}
