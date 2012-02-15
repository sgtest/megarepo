#[doc = "

A type representing values that may be computed concurrently and
operations for working with them.

Example:

> let delayed_fib = future::spawn {|| fib(5000) };
> make_a_sandwitch();
> io::println(#fmt(\"fib(5000) = %?\", delayed_fib.get()))

"];

export future;
export future::{};
export from_value;
export from_port;
export get;
export with;
export spawn;

import either = either::t;

#[doc = "The future type"]
enum future<A> = {
    mutable v: either<@A, comm::port<A>>
};

#[doc = "Methods on the `future` type"]
impl future<A:send> for future<A> {

    fn get() -> A {
        #[doc = "Get the value of the future"];

        get(self)
    }

    fn with<B>(blk: fn(A) -> B) -> B {
        #[doc = "Work with the value without copying it"];

        with(self, blk)
    }
}

fn from_value<A>(+val: A) -> future<A> {
    #[doc = "

    Create a future from a value. The value is immediately available
    and calling `get` later will not block.

    "];

    future({
        mutable v: either::left(@val)
    })
}

fn from_port<A>(-port: comm::port<A>) -> future<A> {
    #[doc = "

    Create a future from a port. The first time that the value is
    requested the task will block waiting for the result to be
    received on the port.

    "];

    future({
        mutable v: either::right(port)
    })
}

fn get<A:send>(future: future<A>) -> A {
    #[doc = "Get the value of the future"];

    with(future) {|v| v }
}

fn with<A:send,B>(future: future<A>, blk: fn(A) -> B) -> B {
    #[doc = "Work with the value without copying it"];

    let v = alt future.v {
      either::left(v) { v }
      either::right(po) {
        let v = @comm::recv(po);
        future.v = either::left(v);
        v
      }
    };
    blk(*v)
}

fn spawn<A:send>(+blk: fn~() -> A) -> future<A> {
    #[doc = "

    Create a future from a unique closure. The closure will be run
    in a new task and its result used as the value of the future.

    "];

    let po = comm::port();
    let ch = comm::chan(po);
    task::spawn {||
        comm::send(ch, blk())
    };
    from_port(po)
}

#[test]
fn test_from_value() {
    let f = from_value("snail");
    assert get(f) == "snail";
}

#[test]
fn test_from_port() {
    let po = comm::port();
    let ch = comm::chan(po);
    comm::send(ch, "whale");
    let f = from_port(po);
    assert get(f) == "whale";
}

#[test]
fn test_iface_get() {
    let f = from_value("fail");
    assert f.get() == "fail";
}

#[test]
fn test_with() {
    let f = from_value("nail");
    assert with(f) {|v| v} == "nail";
}

#[test]
fn test_iface_with() {
    let f = from_value("kale");
    assert f.with {|v| v} == "kale";
}

#[test]
fn test_spawn() {
    let f = spawn {|| "bale" };
    assert get(f) == "bale";
}

#[test]
#[should_fail]
#[ignore(cfg(target_os = "win32"))]
fn test_futurefail() {
    let f = spawn {|| fail };
    let _x: str = get(f);
}