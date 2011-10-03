// -*- rust -*-
// error-pattern:1 == 2
// xfail-test Been deadlocking on mac
use std;
import std::task;
import std::comm::chan;
import std::comm::port;
import std::comm::recv;

fn child() { assert (1 == 2); }

fn parent() {
    let p = port::<int>();
    let f = child;
    task::spawn(f);
    let x = recv(p);
}

// This task is not linked to the failure chain, but since the other
// tasks are going to fail the kernel, this one will fail too
fn sleeper() {
    let p = port::<int>();
    let x = recv(p);
}

fn main() {
    let g = sleeper;
    task::spawn(g);
    let f = parent;
    task::spawn(f);
}