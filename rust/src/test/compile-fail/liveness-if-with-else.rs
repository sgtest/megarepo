fn foo(x: int) { log(debug, x); }

fn main() {
    let x: int;
    if 1 > 2 {
        debug!{"whoops"};
    } else {
        x = 10;
    }
    foo(x); //~ ERROR use of possibly uninitialized variable: `x`
}
