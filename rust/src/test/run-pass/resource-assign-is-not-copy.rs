struct r {
  let i: @mut int;
  new(i: @mut int) {
    self.i = i;
  }
  drop { *(self.i) += 1; }
}

fn main() {
    let i = @mut 0;
    // Even though these look like copies, they are guaranteed not to be
    {
        let a = r(i);
        let b = (a, 10);
        let (c, _d) = b;
        log(debug, c);
    }
    assert *i == 1;
}
