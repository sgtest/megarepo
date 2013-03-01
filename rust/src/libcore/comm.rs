// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use either::{Either, Left, Right};
use kinds::Owned;
use option;
use option::{Option, Some, None, unwrap};
use private;
use vec;

use pipes::{recv, try_recv, wait_many, peek, PacketHeader};

// NOTE Making this public exposes some plumbing from pipes. Needs
// some refactoring
pub use pipes::Selectable;

/// A trait for things that can send multiple messages.
pub trait GenericChan<T> {
    /// Sends a message.
    fn send(x: T);
}

/// Things that can send multiple messages and can detect when the receiver
/// is closed
pub trait GenericSmartChan<T> {
    /// Sends a message, or report if the receiver has closed the connection.
    fn try_send(x: T) -> bool;
}

/// A trait for things that can receive multiple messages.
pub trait GenericPort<T> {
    /// Receives a message, or fails if the connection closes.
    fn recv() -> T;

    /** Receives a message, or returns `none` if
    the connection is closed or closes.
    */
    fn try_recv() -> Option<T>;
}

/// Ports that can `peek`
pub trait Peekable<T> {
    /// Returns true if a message is available
    pure fn peek() -> bool;
}

/// Returns the index of an endpoint that is ready to receive.
pub fn selecti<T: Selectable>(endpoints: &[T]) -> uint {
    wait_many(endpoints)
}

/// Returns 0 or 1 depending on which endpoint is ready to receive
pub fn select2i<A: Selectable, B: Selectable>(a: &A, b: &B) ->
        Either<(), ()> {
    match wait_many([a.header(), b.header()]) {
      0 => Left(()),
      1 => Right(()),
      _ => fail!(~"wait returned unexpected index")
    }
}

// Streams - Make pipes a little easier in general.

proto! streamp (
    Open:send<T: Owned> {
        data(T) -> Open<T>
    }
)

#[doc(hidden)]
struct Chan_<T> {
    mut endp: Option<streamp::client::Open<T>>
}

/// An endpoint that can send many messages.
pub enum Chan<T> {
    Chan_(Chan_<T>)
}

struct Port_<T> {
    mut endp: Option<streamp::server::Open<T>>,
}

/// An endpoint that can receive many messages.
pub enum Port<T> {
    Port_(Port_<T>)
}

/** Creates a `(chan, port)` pair.

These allow sending or receiving an unlimited number of messages.

*/
pub fn stream<T:Owned>() -> (Port<T>, Chan<T>) {
    let (c, s) = streamp::init();

    (Port_(Port_ { endp: Some(s) }), Chan_(Chan_{ endp: Some(c) }))
}

impl<T: Owned> GenericChan<T> for Chan<T> {
    fn send(x: T) {
        let mut endp = None;
        endp <-> self.endp;
        self.endp = Some(
            streamp::client::data(unwrap(endp), x))
    }
}

impl<T: Owned> GenericSmartChan<T> for Chan<T> {

    fn try_send(x: T) -> bool {
        let mut endp = None;
        endp <-> self.endp;
        match streamp::client::try_data(unwrap(endp), x) {
            Some(next) => {
                self.endp = Some(next);
                true
            }
            None => false
        }
    }
}

impl<T: Owned> GenericPort<T> for Port<T> {
    fn recv() -> T {
        let mut endp = None;
        endp <-> self.endp;
        let streamp::data(x, endp) = recv(unwrap(endp));
        self.endp = Some(endp);
        x
    }

    fn try_recv() -> Option<T> {
        let mut endp = None;
        endp <-> self.endp;
        match try_recv(unwrap(endp)) {
          Some(streamp::data(x, endp)) => {
            self.endp = Some(endp);
            Some(x)
          }
          None => None
        }
    }
}

impl<T: Owned> Peekable<T> for Port<T> {
    pure fn peek() -> bool {
        unsafe {
            let mut endp = None;
            endp <-> self.endp;
            let peek = match &endp {
              &Some(ref endp) => peek(endp),
              &None => fail!(~"peeking empty stream")
            };
            self.endp <-> endp;
            peek
        }
    }
}

impl<T: Owned> Selectable for Port<T> {
    pure fn header() -> *PacketHeader {
        unsafe {
            match self.endp {
              Some(ref endp) => endp.header(),
              None => fail!(~"peeking empty stream")
            }
        }
    }
}

/// Treat many ports as one.
pub struct PortSet<T> {
    mut ports: ~[Port<T>],
}

pub fn PortSet<T: Owned>() -> PortSet<T>{
    PortSet {
        ports: ~[]
    }
}

pub impl<T: Owned> PortSet<T> {

    fn add(port: Port<T>) {
        self.ports.push(port)
    }

    fn chan() -> Chan<T> {
        let (po, ch) = stream();
        self.add(po);
        ch
    }
}

impl<T: Owned> GenericPort<T> for PortSet<T> {

    fn try_recv() -> Option<T> {
        let mut result = None;
        // we have to swap the ports array so we aren't borrowing
        // aliasable mutable memory.
        let mut ports = ~[];
        ports <-> self.ports;
        while result.is_none() && ports.len() > 0 {
            let i = wait_many(ports);
            match ports[i].try_recv() {
                Some(m) => {
                  result = Some(m);
                }
                None => {
                    // Remove this port.
                    let _ = ports.swap_remove(i);
                }
            }
        }
        ports <-> self.ports;
        result
    }

    fn recv() -> T {
        self.try_recv().expect("port_set: endpoints closed")
    }

}

impl<T: Owned> Peekable<T> for PortSet<T> {
    pure fn peek() -> bool {
        // It'd be nice to use self.port.each, but that version isn't
        // pure.
        for vec::each(self.ports) |p| {
            if p.peek() { return true }
        }
        false
    }
}

/// A channel that can be shared between many senders.
pub type SharedChan<T> = private::Exclusive<Chan<T>>;

impl<T: Owned> GenericChan<T> for SharedChan<T> {
    fn send(x: T) {
        let mut xx = Some(x);
        do self.with_imm |chan| {
            let mut x = None;
            x <-> xx;
            chan.send(option::unwrap(x))
        }
    }
}

impl<T: Owned> GenericSmartChan<T> for SharedChan<T> {
    fn try_send(x: T) -> bool {
        let mut xx = Some(x);
        do self.with_imm |chan| {
            let mut x = None;
            x <-> xx;
            chan.try_send(option::unwrap(x))
        }
    }
}

/// Converts a `chan` into a `shared_chan`.
pub fn SharedChan<T:Owned>(c: Chan<T>) -> SharedChan<T> {
    private::exclusive(c)
}

/// Receive a message from one of two endpoints.
pub trait Select2<T: Owned, U: Owned> {
    /// Receive a message or return `None` if a connection closes.
    fn try_select() -> Either<Option<T>, Option<U>>;
    /// Receive a message or fail if a connection closes.
    fn select() -> Either<T, U>;
}

impl<T: Owned, U: Owned,
     Left: Selectable + GenericPort<T>,
     Right: Selectable + GenericPort<U>>
    Select2<T, U> for (Left, Right) {

    fn select() -> Either<T, U> {
        match self {
          (ref lp, ref rp) => match select2i(lp, rp) {
            Left(()) => Left (lp.recv()),
            Right(()) => Right(rp.recv())
          }
        }
    }

    fn try_select() -> Either<Option<T>, Option<U>> {
        match self {
          (ref lp, ref rp) => match select2i(lp, rp) {
            Left(()) => Left (lp.try_recv()),
            Right(()) => Right(rp.try_recv())
          }
        }
    }
}

proto! oneshot (
    Oneshot:send<T:Owned> {
        send(T) -> !
    }
)

/// The send end of a oneshot pipe.
pub type ChanOne<T> = oneshot::client::Oneshot<T>;
/// The receive end of a oneshot pipe.
pub type PortOne<T> = oneshot::server::Oneshot<T>;

/// Initialiase a (send-endpoint, recv-endpoint) oneshot pipe pair.
pub fn oneshot<T: Owned>() -> (PortOne<T>, ChanOne<T>) {
    let (chan, port) = oneshot::init();
    (port, chan)
}

pub impl<T: Owned> PortOne<T> {
    fn recv(self) -> T { recv_one(self) }
    fn try_recv(self) -> Option<T> { try_recv_one(self) }
}

pub impl<T: Owned> ChanOne<T> {
    fn send(self, data: T) { send_one(self, data) }
    fn try_send(self, data: T) -> bool { try_send_one(self, data) }
}

/**
 * Receive a message from a oneshot pipe, failing if the connection was
 * closed.
 */
pub fn recv_one<T: Owned>(port: PortOne<T>) -> T {
    let oneshot::send(message) = recv(port);
    message
}

/// Receive a message from a oneshot pipe unless the connection was closed.
pub fn try_recv_one<T: Owned> (port: PortOne<T>) -> Option<T> {
    let message = try_recv(port);

    if message.is_none() { None }
    else {
        let oneshot::send(message) = option::unwrap(message);
        Some(message)
    }
}

/// Send a message on a oneshot pipe, failing if the connection was closed.
pub fn send_one<T: Owned>(chan: ChanOne<T>, data: T) {
    oneshot::client::send(chan, data);
}

/**
 * Send a message on a oneshot pipe, or return false if the connection was
 * closed.
 */
pub fn try_send_one<T: Owned>(chan: ChanOne<T>, data: T)
        -> bool {
    oneshot::client::try_send(chan, data).is_some()
}

#[cfg(test)]
pub mod test {
    use either::{Either, Left, Right};
    use super::{Chan, Port, oneshot, recv_one, stream};

    #[test]
    pub fn test_select2() {
        let (p1, c1) = stream();
        let (p2, c2) = stream();

        c1.send(~"abc");

        match (p1, p2).select() {
          Right(_) => fail!(),
          _ => ()
        }

        c2.send(123);
    }

    #[test]
    pub fn test_oneshot() {
        let (c, p) = oneshot::init();

        oneshot::client::send(c, ());

        recv_one(p)
    }

    #[test]
    fn test_peek_terminated() {
        let (port, chan): (Port<int>, Chan<int>) = stream();

        {
            // Destroy the channel
            let _chan = chan;
        }

        assert !port.peek();
    }
}
