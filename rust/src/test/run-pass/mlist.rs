// -*- rust -*-

type mlist = tag(cons(int,mutable @mlist), nil());

fn main() {
  cons(10, cons(11, cons(12, nil())));
}
