// xfail-stage1
// xfail-stage2
// xfail-stage3

use std;
import std::comm;
import std::task;

fn start(c : comm::_chan<int>, n: int) {
    let i: int = n;

    while i > 0 { comm::send(c, 0); i = i - 1; }
}

fn main() {
    let p = comm::mk_port<comm::_chan<int>>();
    // Spawn a task that sends us back messages. The parent task
    // is likely to terminate before the child completes, so from
    // the child's point of view the receiver may die. We should
    // drop messages on the floor in this case, and not crash!
    let child = task::_spawn(bind start(p.mk_chan(), 10));
    let c = p.recv();
}
