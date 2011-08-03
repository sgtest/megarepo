fn iter_vec[T](v: &vec[T], f: &block(&T) ) { for x: T  in v { f(x); } }

fn main() {
    let v = [1, 2, 3, 4, 5, 6, 7];
    let odds = 0;
    iter_vec(v,
             block (i: &int) {
                 log_err i;
                 if i % 2 == 1 { odds += 1; }
                 log_err odds;
             });
    log_err odds;
    assert (odds == 4);
}