


// -*- rust -*-
pub fn main() {
    let mut i: int = 90;
    while i < 100 {
        log(debug, i);
        i = i + 1;
        if i == 95 {
            let v: ~[int] =
                ~[1, 2, 3, 4, 5]; // we check that it is freed by break

            debug!("breaking");
            break;
        }
    }
    fail_unless!((i == 95));
}
