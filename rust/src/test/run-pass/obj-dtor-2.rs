// xfail-stage0
// xfail-stage1
// xfail-stage2

obj foo(@mutable int x) {
  drop {
    log "running dtor";
    *x = ((*x) + 1);
  }
}



fn main() {
  auto mbox = @mutable 10;
  {
    auto x = foo(mbox);
  }
  assert ((*mbox) == 11);
}