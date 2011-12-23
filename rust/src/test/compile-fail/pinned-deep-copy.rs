// error-pattern: copying a noncopyable value

resource r(i: @mutable int) {
    *i = *i + 1;
}

fn main() {
    let i = @mutable 0;
    {
        // Can't do this copy
        let x = ~~~{y: r(i)};
        let z = x;
        log(debug, x);
    }
    log(error, *i);
}