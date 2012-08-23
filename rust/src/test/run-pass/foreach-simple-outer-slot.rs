


// -*- rust -*-
fn main() {
    let mut sum: int = 0;
    do first_ten |i| { debug!("main"); log(debug, i); sum = sum + i; }
    debug!("sum");
    log(debug, sum);
    assert (sum == 45);
}

fn first_ten(it: fn(int)) {
    let mut i: int = 0;
    while i < 10 { debug!("first_ten"); it(i); i = i + 1; }
}
