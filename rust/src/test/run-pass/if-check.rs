pure fn even(x: uint) -> bool {
    if x < 2u {
        ret false;
    } else if x == 2u { ret true; } else { ret even(x - 2u); }
}

fn foo(x: uint) {
    if even(x) {
        log(debug, x);
    } else {
        fail;
    }
}

fn main() { foo(2u); }
