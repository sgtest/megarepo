// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Higher level communication abstractions.

*/

use core::pipes::{GenericChan, GenericSmartChan, GenericPort};
use core::pipes::{Chan, Port, Selectable, Peekable};
use core::pipes;
use core::prelude::*;

/// An extension of `pipes::stream` that allows both sending and receiving.
pub struct DuplexStream<T, U> {
    priv chan: Chan<T>,
    priv port: Port<U>,
}

impl<T: Owned, U: Owned> GenericChan<T> for DuplexStream<T, U> {
    fn send(x: T) {
        self.chan.send(move x)
    }
}

impl<T: Owned, U: Owned> GenericSmartChan<T> for DuplexStream<T, U> {
    fn try_send(x: T) -> bool {
        self.chan.try_send(move x)
    }
}

impl<T: Owned, U: Owned> GenericPort<U> for DuplexStream<T, U> {
    fn recv() -> U {
        self.port.recv()
    }

    fn try_recv() -> Option<U> {
        self.port.try_recv()
    }
}

impl<T: Owned, U: Owned> Peekable<U> for DuplexStream<T, U> {
    pure fn peek() -> bool {
        self.port.peek()
    }
}

impl<T: Owned, U: Owned> Selectable for DuplexStream<T, U> {
    pure fn header() -> *pipes::PacketHeader {
        self.port.header()
    }
}

/// Creates a bidirectional stream.
pub fn DuplexStream<T: Owned, U: Owned>()
    -> (DuplexStream<T, U>, DuplexStream<U, T>)
{
    let (p1, c2) = pipes::stream();
    let (p2, c1) = pipes::stream();
    (DuplexStream {
        chan: move c1,
        port: move p1
    },
     DuplexStream {
         chan: move c2,
         port: move p2
     })
}

#[cfg(test)]
mod test {
    use comm::DuplexStream;

    #[test]
    pub fn DuplexStream1() {
        let (left, right) = DuplexStream();

        left.send(~"abc");
        right.send(123);

        assert left.recv() == 123;
        assert right.recv() == ~"abc";
    }
}
