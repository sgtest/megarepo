// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Named pipes

This module contains the ability to communicate over named pipes with
synchronous I/O. On windows, this corresponds to talking over a Named Pipe,
while on Unix it corresponds to UNIX domain sockets.

These pipes are similar to TCP in the sense that you can have both a stream to a
server and a server itself. The server provided accepts other `UnixStream`
instances as clients.

*/

#![allow(missing_doc)]

use prelude::*;

use c_str::ToCStr;
use clone::Clone;
use io::{Listener, Acceptor, Reader, Writer, IoResult, IoError};
use kinds::Send;
use owned::Box;
use rt::rtio::{IoFactory, LocalIo, RtioUnixListener};
use rt::rtio::{RtioUnixAcceptor, RtioPipe};

/// A stream which communicates over a named pipe.
pub struct UnixStream {
    obj: Box<RtioPipe + Send>,
}

impl UnixStream {
    /// Connect to a pipe named by `path`. This will attempt to open a
    /// connection to the underlying socket.
    ///
    /// The returned stream will be closed when the object falls out of scope.
    ///
    /// # Example
    ///
    /// ```rust
    /// # #![allow(unused_must_use)]
    /// use std::io::net::unix::UnixStream;
    ///
    /// let server = Path::new("path/to/my/socket");
    /// let mut stream = UnixStream::connect(&server);
    /// stream.write([1, 2, 3]);
    /// ```
    pub fn connect<P: ToCStr>(path: &P) -> IoResult<UnixStream> {
        LocalIo::maybe_raise(|io| {
            io.unix_connect(&path.to_c_str(), None).map(|p| UnixStream { obj: p })
        }).map_err(IoError::from_rtio_error)
    }

    /// Connect to a pipe named by `path`, timing out if the specified number of
    /// milliseconds.
    ///
    /// This function is similar to `connect`, except that if `timeout_ms`
    /// elapses the function will return an error of kind `TimedOut`.
    #[experimental = "the timeout argument is likely to change types"]
    pub fn connect_timeout<P: ToCStr>(path: &P,
                                      timeout_ms: u64) -> IoResult<UnixStream> {
        LocalIo::maybe_raise(|io| {
            let s = io.unix_connect(&path.to_c_str(), Some(timeout_ms));
            s.map(|p| UnixStream { obj: p })
        }).map_err(IoError::from_rtio_error)
    }


    /// Closes the reading half of this connection.
    ///
    /// This method will close the reading portion of this connection, causing
    /// all pending and future reads to immediately return with an error.
    ///
    /// Note that this method affects all cloned handles associated with this
    /// stream, not just this one handle.
    pub fn close_read(&mut self) -> IoResult<()> {
        self.obj.close_read().map_err(IoError::from_rtio_error)
    }

    /// Closes the writing half of this connection.
    ///
    /// This method will close the writing portion of this connection, causing
    /// all pending and future writes to immediately return with an error.
    ///
    /// Note that this method affects all cloned handles associated with this
    /// stream, not just this one handle.
    pub fn close_write(&mut self) -> IoResult<()> {
        self.obj.close_write().map_err(IoError::from_rtio_error)
    }

    /// Sets the read/write timeout for this socket.
    ///
    /// For more information, see `TcpStream::set_timeout`
    #[experimental = "the timeout argument may change in type and value"]
    pub fn set_timeout(&mut self, timeout_ms: Option<u64>) {
        self.obj.set_timeout(timeout_ms)
    }

    /// Sets the read timeout for this socket.
    ///
    /// For more information, see `TcpStream::set_timeout`
    #[experimental = "the timeout argument may change in type and value"]
    pub fn set_read_timeout(&mut self, timeout_ms: Option<u64>) {
        self.obj.set_read_timeout(timeout_ms)
    }

    /// Sets the write timeout for this socket.
    ///
    /// For more information, see `TcpStream::set_timeout`
    #[experimental = "the timeout argument may change in type and value"]
    pub fn set_write_timeout(&mut self, timeout_ms: Option<u64>) {
        self.obj.set_write_timeout(timeout_ms)
    }
}

impl Clone for UnixStream {
    fn clone(&self) -> UnixStream {
        UnixStream { obj: self.obj.clone() }
    }
}

impl Reader for UnixStream {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        self.obj.read(buf).map_err(IoError::from_rtio_error)
    }
}

impl Writer for UnixStream {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        self.obj.write(buf).map_err(IoError::from_rtio_error)
    }
}

/// A value that can listen for incoming named pipe connection requests.
pub struct UnixListener {
    /// The internal, opaque runtime Unix listener.
    obj: Box<RtioUnixListener + Send>,
}

impl UnixListener {

    /// Creates a new listener, ready to receive incoming connections on the
    /// specified socket. The server will be named by `path`.
    ///
    /// This listener will be closed when it falls out of scope.
    ///
    /// # Example
    ///
    /// ```
    /// # fn main() {}
    /// # fn foo() {
    /// # #![allow(unused_must_use)]
    /// use std::io::net::unix::UnixListener;
    /// use std::io::{Listener, Acceptor};
    ///
    /// let server = Path::new("/path/to/my/socket");
    /// let stream = UnixListener::bind(&server);
    /// for mut client in stream.listen().incoming() {
    ///     client.write([1, 2, 3, 4]);
    /// }
    /// # }
    /// ```
    pub fn bind<P: ToCStr>(path: &P) -> IoResult<UnixListener> {
        LocalIo::maybe_raise(|io| {
            io.unix_bind(&path.to_c_str()).map(|s| UnixListener { obj: s })
        }).map_err(IoError::from_rtio_error)
    }
}

impl Listener<UnixStream, UnixAcceptor> for UnixListener {
    fn listen(self) -> IoResult<UnixAcceptor> {
        self.obj.listen().map(|obj| {
            UnixAcceptor { obj: obj }
        }).map_err(IoError::from_rtio_error)
    }
}

/// A value that can accept named pipe connections, returned from `listen()`.
pub struct UnixAcceptor {
    /// The internal, opaque runtime Unix acceptor.
    obj: Box<RtioUnixAcceptor + Send>,
}

impl UnixAcceptor {
    /// Sets a timeout for this acceptor, after which accept() will no longer
    /// block indefinitely.
    ///
    /// The argument specified is the amount of time, in milliseconds, into the
    /// future after which all invocations of accept() will not block (and any
    /// pending invocation will return). A value of `None` will clear any
    /// existing timeout.
    ///
    /// When using this method, it is likely necessary to reset the timeout as
    /// appropriate, the timeout specified is specific to this object, not
    /// specific to the next request.
    #[experimental = "the name and arguments to this function are likely \
                      to change"]
    pub fn set_timeout(&mut self, timeout_ms: Option<u64>) {
        self.obj.set_timeout(timeout_ms)
    }
}

impl Acceptor<UnixStream> for UnixAcceptor {
    fn accept(&mut self) -> IoResult<UnixStream> {
        self.obj.accept().map(|s| {
            UnixStream { obj: s }
        }).map_err(IoError::from_rtio_error)
    }
}

#[cfg(test)]
#[allow(experimental)]
mod tests {
    use prelude::*;
    use super::*;
    use io::*;
    use io::test::*;

    pub fn smalltest(server: proc(UnixStream):Send, client: proc(UnixStream):Send) {
        let path1 = next_test_unix();
        let path2 = path1.clone();

        let mut acceptor = UnixListener::bind(&path1).listen();

        spawn(proc() {
            match UnixStream::connect(&path2) {
                Ok(c) => client(c),
                Err(e) => fail!("failed connect: {}", e),
            }
        });

        match acceptor.accept() {
            Ok(c) => server(c),
            Err(e) => fail!("failed accept: {}", e),
        }
    }

    iotest!(fn bind_error() {
        let path = "path/to/nowhere";
        match UnixListener::bind(&path) {
            Ok(..) => fail!(),
            Err(e) => {
                assert!(e.kind == PermissionDenied || e.kind == FileNotFound ||
                        e.kind == InvalidInput);
            }
        }
    })

    iotest!(fn connect_error() {
        let path = if cfg!(windows) {
            r"\\.\pipe\this_should_not_exist_ever"
        } else {
            "path/to/nowhere"
        };
        match UnixStream::connect(&path) {
            Ok(..) => fail!(),
            Err(e) => {
                assert!(e.kind == FileNotFound || e.kind == OtherIoError);
            }
        }
    })

    iotest!(fn smoke() {
        smalltest(proc(mut server) {
            let mut buf = [0];
            server.read(buf).unwrap();
            assert!(buf[0] == 99);
        }, proc(mut client) {
            client.write([99]).unwrap();
        })
    })

    iotest!(fn read_eof() {
        smalltest(proc(mut server) {
            let mut buf = [0];
            assert!(server.read(buf).is_err());
            assert!(server.read(buf).is_err());
        }, proc(_client) {
            // drop the client
        })
    } #[ignore(cfg(windows))]) // FIXME(#12516)

    iotest!(fn write_begone() {
        smalltest(proc(mut server) {
            let buf = [0];
            loop {
                match server.write(buf) {
                    Ok(..) => {}
                    Err(e) => {
                        assert!(e.kind == BrokenPipe ||
                                e.kind == NotConnected ||
                                e.kind == ConnectionReset,
                                "unknown error {:?}", e);
                        break;
                    }
                }
            }
        }, proc(_client) {
            // drop the client
        })
    })

    iotest!(fn accept_lots() {
        let times = 10;
        let path1 = next_test_unix();
        let path2 = path1.clone();

        let mut acceptor = match UnixListener::bind(&path1).listen() {
            Ok(a) => a,
            Err(e) => fail!("failed listen: {}", e),
        };

        spawn(proc() {
            for _ in range(0, times) {
                let mut stream = UnixStream::connect(&path2);
                match stream.write([100]) {
                    Ok(..) => {}
                    Err(e) => fail!("failed write: {}", e)
                }
            }
        });

        for _ in range(0, times) {
            let mut client = acceptor.accept();
            let mut buf = [0];
            match client.read(buf) {
                Ok(..) => {}
                Err(e) => fail!("failed read/accept: {}", e),
            }
            assert_eq!(buf[0], 100);
        }
    })

    #[cfg(unix)]
    iotest!(fn path_exists() {
        let path = next_test_unix();
        let _acceptor = UnixListener::bind(&path).listen();
        assert!(path.exists());
    })

    iotest!(fn unix_clone_smoke() {
        let addr = next_test_unix();
        let mut acceptor = UnixListener::bind(&addr).listen();

        spawn(proc() {
            let mut s = UnixStream::connect(&addr);
            let mut buf = [0, 0];
            debug!("client reading");
            assert_eq!(s.read(buf), Ok(1));
            assert_eq!(buf[0], 1);
            debug!("client writing");
            s.write([2]).unwrap();
            debug!("client dropping");
        });

        let mut s1 = acceptor.accept().unwrap();
        let s2 = s1.clone();

        let (tx1, rx1) = channel();
        let (tx2, rx2) = channel();
        spawn(proc() {
            let mut s2 = s2;
            rx1.recv();
            debug!("writer writing");
            s2.write([1]).unwrap();
            debug!("writer done");
            tx2.send(());
        });
        tx1.send(());
        let mut buf = [0, 0];
        debug!("reader reading");
        assert_eq!(s1.read(buf), Ok(1));
        debug!("reader done");
        rx2.recv();
    })

    iotest!(fn unix_clone_two_read() {
        let addr = next_test_unix();
        let mut acceptor = UnixListener::bind(&addr).listen();
        let (tx1, rx) = channel();
        let tx2 = tx1.clone();

        spawn(proc() {
            let mut s = UnixStream::connect(&addr);
            s.write([1]).unwrap();
            rx.recv();
            s.write([2]).unwrap();
            rx.recv();
        });

        let mut s1 = acceptor.accept().unwrap();
        let s2 = s1.clone();

        let (done, rx) = channel();
        spawn(proc() {
            let mut s2 = s2;
            let mut buf = [0, 0];
            s2.read(buf).unwrap();
            tx2.send(());
            done.send(());
        });
        let mut buf = [0, 0];
        s1.read(buf).unwrap();
        tx1.send(());

        rx.recv();
    })

    iotest!(fn unix_clone_two_write() {
        let addr = next_test_unix();
        let mut acceptor = UnixListener::bind(&addr).listen();

        spawn(proc() {
            let mut s = UnixStream::connect(&addr);
            let mut buf = [0, 1];
            s.read(buf).unwrap();
            s.read(buf).unwrap();
        });

        let mut s1 = acceptor.accept().unwrap();
        let s2 = s1.clone();

        let (tx, rx) = channel();
        spawn(proc() {
            let mut s2 = s2;
            s2.write([1]).unwrap();
            tx.send(());
        });
        s1.write([2]).unwrap();

        rx.recv();
    })

    iotest!(fn drop_removes_listener_path() {
        let path = next_test_unix();
        let l = UnixListener::bind(&path).unwrap();
        assert!(path.exists());
        drop(l);
        assert!(!path.exists());
    } #[cfg(not(windows))])

    iotest!(fn drop_removes_acceptor_path() {
        let path = next_test_unix();
        let l = UnixListener::bind(&path).unwrap();
        assert!(path.exists());
        drop(l.listen().unwrap());
        assert!(!path.exists());
    } #[cfg(not(windows))])

    iotest!(fn accept_timeout() {
        let addr = next_test_unix();
        let mut a = UnixListener::bind(&addr).unwrap().listen().unwrap();

        a.set_timeout(Some(10));

        // Make sure we time out once and future invocations also time out
        let err = a.accept().err().unwrap();
        assert_eq!(err.kind, TimedOut);
        let err = a.accept().err().unwrap();
        assert_eq!(err.kind, TimedOut);

        // Also make sure that even though the timeout is expired that we will
        // continue to receive any pending connections.
        let (tx, rx) = channel();
        let addr2 = addr.clone();
        spawn(proc() {
            tx.send(UnixStream::connect(&addr2).unwrap());
        });
        let l = rx.recv();
        for i in range(0, 1001) {
            match a.accept() {
                Ok(..) => break,
                Err(ref e) if e.kind == TimedOut => {}
                Err(e) => fail!("error: {}", e),
            }
            ::task::deschedule();
            if i == 1000 { fail!("should have a pending connection") }
        }
        drop(l);

        // Unset the timeout and make sure that this always blocks.
        a.set_timeout(None);
        let addr2 = addr.clone();
        spawn(proc() {
            drop(UnixStream::connect(&addr2).unwrap());
        });
        a.accept().unwrap();
    })

    iotest!(fn connect_timeout_error() {
        let addr = next_test_unix();
        assert!(UnixStream::connect_timeout(&addr, 100).is_err());
    })

    iotest!(fn connect_timeout_success() {
        let addr = next_test_unix();
        let _a = UnixListener::bind(&addr).unwrap().listen().unwrap();
        assert!(UnixStream::connect_timeout(&addr, 100).is_ok());
    })

    iotest!(fn close_readwrite_smoke() {
        let addr = next_test_unix();
        let a = UnixListener::bind(&addr).listen().unwrap();
        let (_tx, rx) = channel::<()>();
        spawn(proc() {
            let mut a = a;
            let _s = a.accept().unwrap();
            let _ = rx.recv_opt();
        });

        let mut b = [0];
        let mut s = UnixStream::connect(&addr).unwrap();
        let mut s2 = s.clone();

        // closing should prevent reads/writes
        s.close_write().unwrap();
        assert!(s.write([0]).is_err());
        s.close_read().unwrap();
        assert!(s.read(b).is_err());

        // closing should affect previous handles
        assert!(s2.write([0]).is_err());
        assert!(s2.read(b).is_err());

        // closing should affect new handles
        let mut s3 = s.clone();
        assert!(s3.write([0]).is_err());
        assert!(s3.read(b).is_err());

        // make sure these don't die
        let _ = s2.close_read();
        let _ = s2.close_write();
        let _ = s3.close_read();
        let _ = s3.close_write();
    })

    iotest!(fn close_read_wakes_up() {
        let addr = next_test_unix();
        let a = UnixListener::bind(&addr).listen().unwrap();
        let (_tx, rx) = channel::<()>();
        spawn(proc() {
            let mut a = a;
            let _s = a.accept().unwrap();
            let _ = rx.recv_opt();
        });

        let mut s = UnixStream::connect(&addr).unwrap();
        let s2 = s.clone();
        let (tx, rx) = channel();
        spawn(proc() {
            let mut s2 = s2;
            assert!(s2.read([0]).is_err());
            tx.send(());
        });
        // this should wake up the child task
        s.close_read().unwrap();

        // this test will never finish if the child doesn't wake up
        rx.recv();
    })

    iotest!(fn readwrite_timeouts() {
        let addr = next_test_unix();
        let mut a = UnixListener::bind(&addr).listen().unwrap();
        let (tx, rx) = channel::<()>();
        spawn(proc() {
            let mut s = UnixStream::connect(&addr).unwrap();
            rx.recv();
            assert!(s.write([0]).is_ok());
            let _ = rx.recv_opt();
        });

        let mut s = a.accept().unwrap();
        s.set_timeout(Some(20));
        assert_eq!(s.read([0]).err().unwrap().kind, TimedOut);
        assert_eq!(s.read([0]).err().unwrap().kind, TimedOut);

        s.set_timeout(Some(20));
        for i in range(0, 1001) {
            match s.write([0, .. 128 * 1024]) {
                Ok(()) | Err(IoError { kind: ShortWrite(..), .. }) => {},
                Err(IoError { kind: TimedOut, .. }) => break,
                Err(e) => fail!("{}", e),
           }
           if i == 1000 { fail!("should have filled up?!"); }
        }

        // I'm not sure as to why, but apparently the write on windows always
        // succeeds after the previous timeout. Who knows?
        if !cfg!(windows) {
            assert_eq!(s.write([0]).err().unwrap().kind, TimedOut);
        }

        tx.send(());
        s.set_timeout(None);
        assert_eq!(s.read([0, 0]), Ok(1));
    })

    iotest!(fn read_timeouts() {
        let addr = next_test_unix();
        let mut a = UnixListener::bind(&addr).listen().unwrap();
        let (tx, rx) = channel::<()>();
        spawn(proc() {
            let mut s = UnixStream::connect(&addr).unwrap();
            rx.recv();
            let mut amt = 0;
            while amt < 100 * 128 * 1024 {
                match s.read([0, ..128 * 1024]) {
                    Ok(n) => { amt += n; }
                    Err(e) => fail!("{}", e),
                }
            }
            let _ = rx.recv_opt();
        });

        let mut s = a.accept().unwrap();
        s.set_read_timeout(Some(20));
        assert_eq!(s.read([0]).err().unwrap().kind, TimedOut);
        assert_eq!(s.read([0]).err().unwrap().kind, TimedOut);

        tx.send(());
        for _ in range(0, 100) {
            assert!(s.write([0, ..128 * 1024]).is_ok());
        }
    })

    iotest!(fn write_timeouts() {
        let addr = next_test_unix();
        let mut a = UnixListener::bind(&addr).listen().unwrap();
        let (tx, rx) = channel::<()>();
        spawn(proc() {
            let mut s = UnixStream::connect(&addr).unwrap();
            rx.recv();
            assert!(s.write([0]).is_ok());
            let _ = rx.recv_opt();
        });

        let mut s = a.accept().unwrap();
        s.set_write_timeout(Some(20));
        for i in range(0, 1001) {
            match s.write([0, .. 128 * 1024]) {
                Ok(()) | Err(IoError { kind: ShortWrite(..), .. }) => {},
                Err(IoError { kind: TimedOut, .. }) => break,
                Err(e) => fail!("{}", e),
           }
           if i == 1000 { fail!("should have filled up?!"); }
        }

        tx.send(());
        assert!(s.read([0]).is_ok());
    })

    iotest!(fn timeout_concurrent_read() {
        let addr = next_test_unix();
        let mut a = UnixListener::bind(&addr).listen().unwrap();
        let (tx, rx) = channel::<()>();
        spawn(proc() {
            let mut s = UnixStream::connect(&addr).unwrap();
            rx.recv();
            assert!(s.write([0]).is_ok());
            let _ = rx.recv_opt();
        });

        let mut s = a.accept().unwrap();
        let s2 = s.clone();
        let (tx2, rx2) = channel();
        spawn(proc() {
            let mut s2 = s2;
            assert!(s2.read([0]).is_ok());
            tx2.send(());
        });

        s.set_read_timeout(Some(20));
        assert_eq!(s.read([0]).err().unwrap().kind, TimedOut);
        tx.send(());

        rx2.recv();
    })
}
