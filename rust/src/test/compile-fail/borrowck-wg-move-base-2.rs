fn foo(x: &mut int) {
    let mut a = 3;
    let mut _y = &mut *x;
    let _z = &mut *_y;
    _y = &mut a; //~ ERROR assigning to mutable local variable prohibited
}

fn main() {
}


