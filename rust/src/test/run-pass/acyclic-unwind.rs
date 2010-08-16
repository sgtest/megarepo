// -*- rust -*-

io fn f(chan[int] c)
{
  type t = tup(int,int,int);

  // Allocate a box.
  let @t x = @tup(1,2,3);

  // Signal parent that we've allocated a box.
  c <| 1;

  while (true) {
    // spin waiting for the parent to kill us.
    log "child waiting to die...";

    // while waiting to die, the messages we are
    // sending to the channel are never received
    // by the parent, therefore this test cases drops
    // messages on the floor
    c <| 1;
  }
}


io fn main() {
  let port[int] p = port();
  spawn f(chan(p));
  let int i;

  // synchronize on event from child.
  i <- p;

  log "parent exiting, killing child";
}
