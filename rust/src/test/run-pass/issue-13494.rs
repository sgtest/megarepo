// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// This test may not always fail, but it can be flaky if the race it used to
// expose is still present.

use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread::Thread;

fn helper(rx: Receiver<Sender<()>>) {
    for tx in rx.iter() {
        let _ = tx.send(());
    }
}

fn main() {
    let (tx, rx) = channel();
    let _t = Thread::spawn(move|| { helper(rx) }).detach();
    let (snd, rcv) = channel::<int>();
    for _ in range(1i, 100000i) {
        snd.send(1i).unwrap();
        let (tx2, rx2) = channel();
        tx.send(tx2).unwrap();
        select! {
            _ = rx2.recv() => (),
            _ = rcv.recv() => ()
        }
    }
}
