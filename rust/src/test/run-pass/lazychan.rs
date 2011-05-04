// xfail-stage0
// xfail-stage1
// xfail-stage2
// -*- rust -*-

fn main() {
  let port[int] p = port();
  auto c = chan(p);
  let int y;

  spawn child(c);
  y <- p;
  log "received 1";
  log y;
  assert (y == 10);

  spawn child(c);
  y <- p;
  log "received 2";
  log y;
  assert (y == 10);
}

fn child(chan[int] c) {
  c <| 10;
}
