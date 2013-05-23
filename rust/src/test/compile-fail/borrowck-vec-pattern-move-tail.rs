fn main() {
    let mut a = [1, 2, 3, 4];
    let t = match a {
        [1, 2, ..tail] => tail,
        _ => std::util::unreachable()
    };
    a[0] = 0; //~ ERROR cannot assign to `a[]` because it is borrowed
    t[0];
}
