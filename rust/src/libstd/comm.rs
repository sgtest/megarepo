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

// NB: transitionary, de-mode-ing.
#[forbid(deprecated_mode)];

use core::pipes::{GenericChan, GenericSmartChan, GenericPort};
use core::pipes::{Chan, Port, Selectable, Peekable};
use core::pipes;

/// An extension of `pipes::stream` that allows both sending and receiving.
pub struct DuplexStream<T: Owned, U: Owned> {
    priv chan: Chan<T>,
    priv port: Port<U>,
}

impl<T: Owned, U: Owned> DuplexStream<T, U> : GenericChan<T> {
    fn send(x: T) {
        self.chan.send(move x)
    }
}

impl<T: Owned, U: Owned> DuplexStream<T, U> : GenericSmartChan<T> {
    fn try_send(x: T) -> bool {
        self.chan.try_send(move x)
    }
}

impl<T: Owned, U: Owned> DuplexStream<T, U> : GenericPort<U> {
    fn recv() -> U {
        self.port.recv()
    }

    fn try_recv() -> Option<U> {
        self.port.try_recv()
    }
}

impl<T: Owned, U: Owned> DuplexStream<T, U> : Peekable<U> {
    pure fn peek() -> bool {
        self.port.peek()
    }
}

impl<T: Owned, U: Owned> DuplexStream<T, U> : Selectable {
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
    #[legacy_exports];
    #[test]
    fn DuplexStream1() {
        let (left, right) = DuplexStream();

        left.send(~"abc");
        right.send(123);

        assert left.recv() == 123;
        assert right.recv() == ~"abc";
    }
}
