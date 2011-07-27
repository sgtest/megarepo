// xfail-stage0

fn iter_vec[T](v: &vec[T], f: &block(&T) ) { for x: T  in v { f(x); } }

fn main() {
    let v = [1, 2, 3, 4, 5];
    let sum = 0;
    iter_vec(v,
             block (i: &int) {
                 iter_vec(v,
                          block (j: &int) { log_err i * j; sum += i * j; });
             });
    log_err sum;
    assert (sum == 225);
}