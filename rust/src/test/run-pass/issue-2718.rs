// xfail-fast

// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// tjc: I don't know why
pub mod pipes {
    use core::cast::{forget, transmute};

    pub struct Stuff<T> {
        state: state,
        blocked_task: Option<task::Task>,
        payload: Option<T>
    }

    #[deriving_eq]
    pub enum state {
        empty,
        full,
        blocked,
        terminated
    }

    pub struct packet<T> {
        state: state,
        blocked_task: Option<task::Task>,
        payload: Option<T>
    }

    pub fn packet<T:Owned>() -> *packet<T> {
        unsafe {
            let p: *packet<T> = cast::transmute(~Stuff{
                state: empty,
                blocked_task: None::<task::Task>,
                payload: None::<T>
            });
            p
        }
    }

    #[abi = "rust-intrinsic"]
    mod rusti {
      pub fn atomic_xchg(_dst: &mut int, _src: int) -> int { fail!(); }
      pub fn atomic_xchg_acq(_dst: &mut int, _src: int) -> int { fail!(); }
      pub fn atomic_xchg_rel(_dst: &mut int, _src: int) -> int { fail!(); }
    }

    // We should consider moving this to ::core::unsafe, although I
    // suspect graydon would want us to use void pointers instead.
    pub unsafe fn uniquify<T>(+x: *T) -> ~T {
        unsafe { cast::transmute(x) }
    }

    pub fn swap_state_acq(+dst: &mut state, src: state) -> state {
        unsafe {
            transmute(rusti::atomic_xchg_acq(transmute(dst), src as int))
        }
    }

    pub fn swap_state_rel(+dst: &mut state, src: state) -> state {
        unsafe {
            transmute(rusti::atomic_xchg_rel(transmute(dst), src as int))
        }
    }

    pub fn send<T:Owned>(mut p: send_packet<T>, -payload: T) {
        let mut p = p.unwrap();
        let mut p = unsafe { uniquify(p) };
        fail_unless!((*p).payload.is_none());
        (*p).payload = Some(payload);
        let old_state = swap_state_rel(&mut (*p).state, full);
        match old_state {
          empty => {
            // Yay, fastpath.

            // The receiver will eventually clean this up.
            unsafe { forget(p); }
          }
          full => { fail!(~"duplicate send") }
          blocked => {

            // The receiver will eventually clean this up.
            unsafe { forget(p); }
          }
          terminated => {
            // The receiver will never receive this. Rely on drop_glue
            // to clean everything up.
          }
        }
    }

    pub fn recv<T:Owned>(mut p: recv_packet<T>) -> Option<T> {
        let mut p = p.unwrap();
        let mut p = unsafe { uniquify(p) };
        loop {
            let old_state = swap_state_acq(&mut (*p).state,
                                           blocked);
            match old_state {
              empty | blocked => { task::yield(); }
              full => {
                let mut payload = None;
                payload <-> (*p).payload;
                return Some(option::unwrap(payload))
              }
              terminated => {
                fail_unless!(old_state == terminated);
                return None;
              }
            }
        }
    }

    pub fn sender_terminate<T:Owned>(mut p: *packet<T>) {
        let mut p = unsafe { uniquify(p) };
        match swap_state_rel(&mut (*p).state, terminated) {
          empty | blocked => {
            // The receiver will eventually clean up.
            unsafe { forget(p) }
          }
          full => {
            // This is impossible
            fail!(~"you dun goofed")
          }
          terminated => {
            // I have to clean up, use drop_glue
          }
        }
    }

    pub fn receiver_terminate<T:Owned>(mut p: *packet<T>) {
        let mut p = unsafe { uniquify(p) };
        match swap_state_rel(&mut (*p).state, terminated) {
          empty => {
            // the sender will clean up
            unsafe { forget(p) }
          }
          blocked => {
            // this shouldn't happen.
            fail!(~"terminating a blocked packet")
          }
          terminated | full => {
            // I have to clean up, use drop_glue
          }
        }
    }

    pub struct send_packet<T> {
        p: Option<*packet<T>>,
    }

    impl<T:Owned> Drop for send_packet<T> {
        fn finalize(&self) {
            unsafe {
                if self.p != None {
                    let mut p = None;
                    let self_p: &mut Option<*packet<T>> =
                        cast::transmute(&self.p);
                    p <-> *self_p;
                    sender_terminate(option::unwrap(p))
                }
            }
        }
    }

    pub impl<T:Owned> send_packet<T> {
        fn unwrap(&mut self) -> *packet<T> {
            let mut p = None;
            p <-> self.p;
            option::unwrap(p)
        }
    }

    pub fn send_packet<T:Owned>(p: *packet<T>) -> send_packet<T> {
        send_packet {
            p: Some(p)
        }
    }

    pub struct recv_packet<T> {
        p: Option<*packet<T>>,
    }

    impl<T:Owned> Drop for recv_packet<T> {
        fn finalize(&self) {
            unsafe {
                if self.p != None {
                    let mut p = None;
                    let self_p: &mut Option<*packet<T>> =
                        cast::transmute(&self.p);
                    p <-> *self_p;
                    receiver_terminate(option::unwrap(p))
                }
            }
        }
    }

    pub impl<T:Owned> recv_packet<T> {
        fn unwrap(&mut self) -> *packet<T> {
            let mut p = None;
            p <-> self.p;
            option::unwrap(p)
        }
    }

    pub fn recv_packet<T:Owned>(p: *packet<T>) -> recv_packet<T> {
        recv_packet {
            p: Some(p)
        }
    }

    pub fn entangle<T:Owned>() -> (send_packet<T>, recv_packet<T>) {
        let p = packet();
        (send_packet(p), recv_packet(p))
    }
}

pub mod pingpong {
    use core::cast;
    use core::ptr;

    pub enum ping = ::pipes::send_packet<pong>;
    pub enum pong = ::pipes::send_packet<ping>;

    pub fn liberate_ping(-p: ping) -> ::pipes::send_packet<pong> {
        unsafe {
            let addr : *::pipes::send_packet<pong> = match &p {
              &ping(ref x) => { cast::transmute(ptr::addr_of(x)) }
            };
            let liberated_value = *addr;
            cast::forget(p);
            liberated_value
        }
    }

    pub fn liberate_pong(-p: pong) -> ::pipes::send_packet<ping> {
        unsafe {
            let addr : *::pipes::send_packet<ping> = match &p {
              &pong(ref x) => { cast::transmute(ptr::addr_of(x)) }
            };
            let liberated_value = *addr;
            cast::forget(p);
            liberated_value
        }
    }

    pub fn init() -> (client::ping, server::ping) {
        ::pipes::entangle()
    }

    pub mod client {
        use core::option;
        use pingpong;

        pub type ping = ::pipes::send_packet<pingpong::ping>;
        pub type pong = ::pipes::recv_packet<pingpong::pong>;

        pub fn do_ping(-c: ping) -> pong {
            let (sp, rp) = ::pipes::entangle();

            ::pipes::send(c, pingpong::ping(sp));
            rp
        }

        pub fn do_pong(-c: pong) -> (ping, ()) {
            let packet = ::pipes::recv(c);
            if packet.is_none() {
                fail!(~"sender closed the connection")
            }
            (pingpong::liberate_pong(option::unwrap(packet)), ())
        }
    }

    pub mod server {
        use pingpong;

        pub type ping = ::pipes::recv_packet<pingpong::ping>;
        pub type pong = ::pipes::send_packet<pingpong::pong>;

        pub fn do_ping(-c: ping) -> (pong, ()) {
            let packet = ::pipes::recv(c);
            if packet.is_none() {
                fail!(~"sender closed the connection")
            }
            (pingpong::liberate_ping(option::unwrap(packet)), ())
        }

        pub fn do_pong(-c: pong) -> ping {
            let (sp, rp) = ::pipes::entangle();
            ::pipes::send(c, pingpong::pong(sp));
            rp
        }
    }
}

fn client(-chan: pingpong::client::ping) {
    let chan = pingpong::client::do_ping(chan);
    log(error, ~"Sent ping");
    let (_chan, _data) = pingpong::client::do_pong(chan);
    log(error, ~"Received pong");
}

fn server(-chan: pingpong::server::ping) {
    let (chan, _data) = pingpong::server::do_ping(chan);
    log(error, ~"Received ping");
    let _chan = pingpong::server::do_pong(chan);
    log(error, ~"Sent pong");
}

pub fn main() {
  /*
//    Commented out because of option::get error

    let (client_, server_) = pingpong::init();
    let client_ = Cell(client_);
    let server_ = Cell(server_);

    task::spawn {|client_|
        let client__ = client_.take();
        client(client__);
    };
    task::spawn {|server_|
        let server__ = server_.take();
        server(server_ˊ);
    };
  */
}
