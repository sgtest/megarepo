// error-pattern:fail

fn x(it: block(int)) {
    fail;
    it(0);
}

fn main() {
    let a = @0;
    x {|_i|};
}