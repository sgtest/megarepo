trait Reverser {
    fn reverse(&self);
}

fn bar(v: &mut [uint]) {
    vec::reverse(v);
    vec::reverse(v);
    vec::reverse(v);
}

pub fn main() {
    let mut the_vec = ~[1, 2, 3, 100];
    bar(the_vec);
    fail_unless!(the_vec == ~[100, 3, 2, 1]);
}
