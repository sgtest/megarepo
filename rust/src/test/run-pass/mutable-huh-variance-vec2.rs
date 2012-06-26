// error-pattern: mismatched types

fn main() {
    let v = [[0]/~]/~;

    // This is ok because the outer vec is covariant with respect
    // to the inner vec. If the outer vec was mut then we
    // couldn't do this.
    fn f(&&v: [const [const int]/~]/~) {
    }

    f(v);
}
