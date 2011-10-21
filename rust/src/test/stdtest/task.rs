use std;
import std::task;
import std::comm;

#[test]
fn test_sleep() { task::sleep(1000000u); }

#[test]
fn test_unsupervise() {
    fn# f(&&_i: ()) { task::unsupervise(); fail; }
    task::spawn2((), f);
}

#[test]
fn test_lib_spawn() {
    fn# foo(&&_i: ()) { log_err "Hello, World!"; }
    task::spawn2((), foo);
}

#[test]
fn test_lib_spawn2() {
    fn# foo(&&x: int) { assert (x == 42); }
    task::spawn2(42, foo);
}

#[test]
fn test_join_chan() {
    fn# winner(&&_i: ()) { }

    let p = comm::port();
    task::spawn_notify2((), winner, comm::chan(p));
    let s = comm::recv(p);
    log_err "received task status message";
    log_err s;
    alt s {
      task::exit(_, task::tr_success.) {/* yay! */ }
      _ { fail "invalid task status received" }
    }
}

#[test]
fn test_join_chan_fail() {
    fn# failer(&&_i: ()) { task::unsupervise(); fail }

    let p = comm::port();
    task::spawn_notify2((), failer, comm::chan(p));
    let s = comm::recv(p);
    log_err "received task status message";
    log_err s;
    alt s {
      task::exit(_, task::tr_failure.) {/* yay! */ }
      _ { fail "invalid task status received" }
    }
}

#[test]
fn test_join_convenient() {
    fn# winner(&&_i: ()) { }
    let handle = task::spawn_joinable2((), winner);
    assert (task::tr_success == task::join(handle));
}

#[test]
#[ignore]
fn spawn_polymorphic() {
    // FIXME #1038: Can't spawn palymorphic functions
    /*fn# foo<~T>(x: T) { log_err x; }

    task::spawn2(true, foo);
    task::spawn2(42, foo);*/
}
