// xfail-test #2587
// error-pattern: copying a noncopyable value

struct r {
  let i:int;
  new(i:int) {self.i = i;}
  drop {}
}

fn main() {
    // This can't make sense as it would copy the classes
    let i <- ~[r(0)];
    let j <- ~[r(1)];
    let k = i + j;
    log(debug, j);
}
