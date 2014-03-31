// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! UDP (User Datagram Protocol) network connections.
//!
//! This module contains the ability to open a UDP stream to a socket address.
//! The destination and binding addresses can either be an IPv4 or IPv6
//! address. There is no corresponding notion of a server because UDP is a
//! datagram protocol.

use clone::Clone;
use io::net::ip::SocketAddr;
use io::{Reader, Writer, IoResult};
use kinds::Send;
use result::{Ok, Err};
use rt::rtio::{RtioSocket, RtioUdpSocket, IoFactory, LocalIo};

/// A User Datagram Protocol socket.
///
/// This is an implementation of a bound UDP socket. This supports both IPv4 and
/// IPv6 addresses, and there is no corresponding notion of a server because UDP
/// is a datagram protocol.
///
/// # Example
///
/// ```rust,no_run
/// # #[allow(unused_must_use)];
/// use std::io::net::udp::UdpSocket;
/// use std::io::net::ip::{Ipv4Addr, SocketAddr};
///
/// let addr = SocketAddr { ip: Ipv4Addr(127, 0, 0, 1), port: 34254 };
/// let mut socket = match UdpSocket::bind(addr) {
///     Ok(s) => s,
///     Err(e) => fail!("couldn't bind socket: {}", e),
/// };
///
/// let mut buf = [0, ..10];
/// match socket.recvfrom(buf) {
///     Ok((amt, src)) => {
///         // Send a reply to the socket we received data from
///         let buf = buf.mut_slice_to(amt);
///         buf.reverse();
///         socket.sendto(buf, src);
///     }
///     Err(e) => println!("couldn't receive a datagram: {}", e)
/// }
/// drop(socket); // close the socket
/// ```
pub struct UdpSocket {
    obj: ~RtioUdpSocket:Send
}

impl UdpSocket {
    /// Creates a UDP socket from the given socket address.
    pub fn bind(addr: SocketAddr) -> IoResult<UdpSocket> {
        LocalIo::maybe_raise(|io| {
            io.udp_bind(addr).map(|s| UdpSocket { obj: s })
        })
    }

    /// Receives data from the socket. On success, returns the number of bytes
    /// read and the address from whence the data came.
    pub fn recvfrom(&mut self, buf: &mut [u8])
                    -> IoResult<(uint, SocketAddr)> {
        self.obj.recvfrom(buf)
    }

    /// Sends data on the socket to the given address. Returns nothing on
    /// success.
    pub fn sendto(&mut self, buf: &[u8], dst: SocketAddr) -> IoResult<()> {
        self.obj.sendto(buf, dst)
    }

    /// Creates a `UdpStream`, which allows use of the `Reader` and `Writer`
    /// traits to receive and send data from the same address. This transfers
    /// ownership of the socket to the stream.
    ///
    /// Note that this call does not perform any actual network communication,
    /// because UDP is a datagram protocol.
    pub fn connect(self, other: SocketAddr) -> UdpStream {
        UdpStream {
            socket: self,
            connected_to: other,
        }
    }

    /// Returns the socket address that this socket was created from.
    pub fn socket_name(&mut self) -> IoResult<SocketAddr> {
        self.obj.socket_name()
    }
}

impl Clone for UdpSocket {
    /// Creates a new handle to this UDP socket, allowing for simultaneous
    /// reads and writes of the socket.
    ///
    /// The underlying UDP socket will not be closed until all handles to the
    /// socket have been deallocated. Two concurrent reads will not receive
    /// the same data. Instead, the first read will receive the first packet
    /// received, and the second read will receive the second packet.
    fn clone(&self) -> UdpSocket {
        UdpSocket {
            obj: self.obj.clone(),
        }
    }
}

/// A type that allows convenient usage of a UDP stream connected to one
/// address via the `Reader` and `Writer` traits.
pub struct UdpStream {
    socket: UdpSocket,
    connected_to: SocketAddr
}

impl UdpStream {
    /// Allows access to the underlying UDP socket owned by this stream. This
    /// is useful to, for example, use the socket to send data to hosts other
    /// than the one that this stream is connected to.
    pub fn as_socket<T>(&mut self, f: |&mut UdpSocket| -> T) -> T {
        f(&mut self.socket)
    }

    /// Consumes this UDP stream and returns out the underlying socket.
    pub fn disconnect(self) -> UdpSocket {
        self.socket
    }
}

impl Reader for UdpStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        let peer = self.connected_to;
        self.as_socket(|sock| {
            match sock.recvfrom(buf) {
                Ok((_nread, src)) if src != peer => Ok(0),
                Ok((nread, _src)) => Ok(nread),
                Err(e) => Err(e),
            }
        })
    }
}

impl Writer for UdpStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        let connected_to = self.connected_to;
        self.as_socket(|sock| sock.sendto(buf, connected_to))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use io::net::ip::{SocketAddr};

    // FIXME #11530 this fails on android because tests are run as root
    iotest!(fn bind_error() {
        let addr = SocketAddr { ip: Ipv4Addr(0, 0, 0, 0), port: 1 };
        match UdpSocket::bind(addr) {
            Ok(..) => fail!(),
            Err(e) => assert_eq!(e.kind, PermissionDenied),
        }
    } #[ignore(cfg(windows))] #[ignore(cfg(target_os = "android"))])

    iotest!(fn socket_smoke_test_ip4() {
        let server_ip = next_test_ip4();
        let client_ip = next_test_ip4();
        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();

        spawn(proc() {
            match UdpSocket::bind(client_ip) {
                Ok(ref mut client) => {
                    rx1.recv();
                    client.sendto([99], server_ip).unwrap()
                }
                Err(..) => fail!()
            }
            tx2.send(());
        });

        match UdpSocket::bind(server_ip) {
            Ok(ref mut server) => {
                tx1.send(());
                let mut buf = [0];
                match server.recvfrom(buf) {
                    Ok((nread, src)) => {
                        assert_eq!(nread, 1);
                        assert_eq!(buf[0], 99);
                        assert_eq!(src, client_ip);
                    }
                    Err(..) => fail!()
                }
            }
            Err(..) => fail!()
        }
        rx2.recv();
    })

    iotest!(fn socket_smoke_test_ip6() {
        let server_ip = next_test_ip6();
        let client_ip = next_test_ip6();
        let (tx, rx) = channel::<()>();

        spawn(proc() {
            match UdpSocket::bind(client_ip) {
                Ok(ref mut client) => {
                    rx.recv();
                    client.sendto([99], server_ip).unwrap()
                }
                Err(..) => fail!()
            }
        });

        match UdpSocket::bind(server_ip) {
            Ok(ref mut server) => {
                tx.send(());
                let mut buf = [0];
                match server.recvfrom(buf) {
                    Ok((nread, src)) => {
                        assert_eq!(nread, 1);
                        assert_eq!(buf[0], 99);
                        assert_eq!(src, client_ip);
                    }
                    Err(..) => fail!()
                }
            }
            Err(..) => fail!()
        }
    })

    iotest!(fn stream_smoke_test_ip4() {
        let server_ip = next_test_ip4();
        let client_ip = next_test_ip4();
        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();

        spawn(proc() {
            match UdpSocket::bind(client_ip) {
                Ok(client) => {
                    let client = ~client;
                    let mut stream = client.connect(server_ip);
                    rx1.recv();
                    stream.write([99]).unwrap();
                }
                Err(..) => fail!()
            }
            tx2.send(());
        });

        match UdpSocket::bind(server_ip) {
            Ok(server) => {
                let server = ~server;
                let mut stream = server.connect(client_ip);
                tx1.send(());
                let mut buf = [0];
                match stream.read(buf) {
                    Ok(nread) => {
                        assert_eq!(nread, 1);
                        assert_eq!(buf[0], 99);
                    }
                    Err(..) => fail!()
                }
            }
            Err(..) => fail!()
        }
        rx2.recv();
    })

    iotest!(fn stream_smoke_test_ip6() {
        let server_ip = next_test_ip6();
        let client_ip = next_test_ip6();
        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();

        spawn(proc() {
            match UdpSocket::bind(client_ip) {
                Ok(client) => {
                    let client = ~client;
                    let mut stream = client.connect(server_ip);
                    rx1.recv();
                    stream.write([99]).unwrap();
                }
                Err(..) => fail!()
            }
            tx2.send(());
        });

        match UdpSocket::bind(server_ip) {
            Ok(server) => {
                let server = ~server;
                let mut stream = server.connect(client_ip);
                tx1.send(());
                let mut buf = [0];
                match stream.read(buf) {
                    Ok(nread) => {
                        assert_eq!(nread, 1);
                        assert_eq!(buf[0], 99);
                    }
                    Err(..) => fail!()
                }
            }
            Err(..) => fail!()
        }
        rx2.recv();
    })

    pub fn socket_name(addr: SocketAddr) {
        let server = UdpSocket::bind(addr);

        assert!(server.is_ok());
        let mut server = server.unwrap();

        // Make sure socket_name gives
        // us the socket we binded to.
        let so_name = server.socket_name();
        assert!(so_name.is_ok());
        assert_eq!(addr, so_name.unwrap());
    }

    iotest!(fn socket_name_ip4() {
        socket_name(next_test_ip4());
    })

    iotest!(fn socket_name_ip6() {
        socket_name(next_test_ip6());
    })

    iotest!(fn udp_clone_smoke() {
        let addr1 = next_test_ip4();
        let addr2 = next_test_ip4();
        let mut sock1 = UdpSocket::bind(addr1).unwrap();
        let sock2 = UdpSocket::bind(addr2).unwrap();

        spawn(proc() {
            let mut sock2 = sock2;
            let mut buf = [0, 0];
            assert_eq!(sock2.recvfrom(buf), Ok((1, addr1)));
            assert_eq!(buf[0], 1);
            sock2.sendto([2], addr1).unwrap();
        });

        let sock3 = sock1.clone();

        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();
        spawn(proc() {
            let mut sock3 = sock3;
            rx1.recv();
            sock3.sendto([1], addr2).unwrap();
            tx2.send(());
        });
        tx1.send(());
        let mut buf = [0, 0];
        assert_eq!(sock1.recvfrom(buf), Ok((1, addr2)));
        rx2.recv();
    })

    iotest!(fn udp_clone_two_read() {
        let addr1 = next_test_ip4();
        let addr2 = next_test_ip4();
        let mut sock1 = UdpSocket::bind(addr1).unwrap();
        let sock2 = UdpSocket::bind(addr2).unwrap();
        let (tx1, rx) = channel();
        let tx2 = tx1.clone();

        spawn(proc() {
            let mut sock2 = sock2;
            sock2.sendto([1], addr1).unwrap();
            rx.recv();
            sock2.sendto([2], addr1).unwrap();
            rx.recv();
        });

        let sock3 = sock1.clone();

        let (done, rx) = channel();
        spawn(proc() {
            let mut sock3 = sock3;
            let mut buf = [0, 0];
            sock3.recvfrom(buf).unwrap();
            tx2.send(());
            done.send(());
        });
        let mut buf = [0, 0];
        sock1.recvfrom(buf).unwrap();
        tx1.send(());

        rx.recv();
    })

    iotest!(fn udp_clone_two_write() {
        let addr1 = next_test_ip4();
        let addr2 = next_test_ip4();
        let mut sock1 = UdpSocket::bind(addr1).unwrap();
        let sock2 = UdpSocket::bind(addr2).unwrap();

        let (tx, rx) = channel();
        let (serv_tx, serv_rx) = channel();

        spawn(proc() {
            let mut sock2 = sock2;
            let mut buf = [0, 1];

            rx.recv();
            match sock2.recvfrom(buf) {
                Ok(..) => {}
                Err(e) => fail!("failed receive: {}", e),
            }
            serv_tx.send(());
        });

        let sock3 = sock1.clone();

        let (done, rx) = channel();
        let tx2 = tx.clone();
        spawn(proc() {
            let mut sock3 = sock3;
            match sock3.sendto([1], addr2) {
                Ok(..) => { let _ = tx2.try_send(()); }
                Err(..) => {}
            }
            done.send(());
        });
        match sock1.sendto([2], addr2) {
            Ok(..) => { let _ = tx.try_send(()); }
            Err(..) => {}
        }
        drop(tx);

        rx.recv();
        serv_rx.recv();
    })
}
