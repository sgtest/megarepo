// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![stable(feature = "rust1", since = "1.0.0")]

use fs;
use os::windows::raw;
use net;
use sys_common::{net2, AsInner, FromInner};
use sys;

/// Raw HANDLEs.
#[stable(feature = "rust1", since = "1.0.0")]
pub type RawHandle = raw::HANDLE;

/// Raw SOCKETs.
#[stable(feature = "rust1", since = "1.0.0")]
pub type RawSocket = raw::SOCKET;

/// Extract raw handles.
#[stable(feature = "rust1", since = "1.0.0")]
pub trait AsRawHandle {
    /// Extracts the raw handle, without taking any ownership.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn as_raw_handle(&self) -> RawHandle;
}

/// Construct I/O objects from raw handles.
#[unstable(feature = "from_raw_os",
           reason = "recent addition to the std::os::windows::io module")]
pub trait FromRawHandle {
    /// Constructs a new I/O object from the specified raw handle.
    ///
    /// This function will **consume ownership** of the handle given,
    /// passing responsibility for closing the handle to the returned
    /// object.
    ///
    /// This function is also unsafe as the primitives currently returned
    /// have the contract that they are the sole owner of the file
    /// descriptor they are wrapping. Usage of this function could
    /// accidentally allow violating this contract which can cause memory
    /// unsafety in code that relies on it being true.
    unsafe fn from_raw_handle(handle: RawHandle) -> Self;
}

#[stable(feature = "rust1", since = "1.0.0")]
impl AsRawHandle for fs::File {
    fn as_raw_handle(&self) -> RawHandle {
        self.as_inner().handle().raw() as RawHandle
    }
}

#[unstable(feature = "from_raw_os", reason = "trait is unstable")]
impl FromRawHandle for fs::File {
    unsafe fn from_raw_handle(handle: RawHandle) -> fs::File {
        let handle = handle as ::libc::HANDLE;
        fs::File::from_inner(sys::fs2::File::from_inner(handle))
    }
}

/// Extract raw sockets.
#[stable(feature = "rust1", since = "1.0.0")]
pub trait AsRawSocket {
    /// Extracts the underlying raw socket from this object.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn as_raw_socket(&self) -> RawSocket;
}

/// Create I/O objects from raw sockets.
#[unstable(feature = "from_raw_os", reason = "recent addition to module")]
pub trait FromRawSocket {
    /// Creates a new I/O object from the given raw socket.
    ///
    /// This function will **consume ownership** of the socket provided and
    /// it will be closed when the returned object goes out of scope.
    ///
    /// This function is also unsafe as the primitives currently returned
    /// have the contract that they are the sole owner of the file
    /// descriptor they are wrapping. Usage of this function could
    /// accidentally allow violating this contract which can cause memory
    /// unsafety in code that relies on it being true.
    unsafe fn from_raw_socket(sock: RawSocket) -> Self;
}

#[stable(feature = "rust1", since = "1.0.0")]
impl AsRawSocket for net::TcpStream {
    fn as_raw_socket(&self) -> RawSocket {
        *self.as_inner().socket().as_inner()
    }
}
#[stable(feature = "rust1", since = "1.0.0")]
impl AsRawSocket for net::TcpListener {
    fn as_raw_socket(&self) -> RawSocket {
        *self.as_inner().socket().as_inner()
    }
}
#[stable(feature = "rust1", since = "1.0.0")]
impl AsRawSocket for net::UdpSocket {
    fn as_raw_socket(&self) -> RawSocket {
        *self.as_inner().socket().as_inner()
    }
}

#[unstable(feature = "from_raw_os", reason = "trait is unstable")]
impl FromRawSocket for net::TcpStream {
    unsafe fn from_raw_socket(sock: RawSocket) -> net::TcpStream {
        let sock = sys::net::Socket::from_inner(sock);
        net::TcpStream::from_inner(net2::TcpStream::from_inner(sock))
    }
}
#[unstable(feature = "from_raw_os", reason = "trait is unstable")]
impl FromRawSocket for net::TcpListener {
    unsafe fn from_raw_socket(sock: RawSocket) -> net::TcpListener {
        let sock = sys::net::Socket::from_inner(sock);
        net::TcpListener::from_inner(net2::TcpListener::from_inner(sock))
    }
}
#[unstable(feature = "from_raw_os", reason = "trait is unstable")]
impl FromRawSocket for net::UdpSocket {
    unsafe fn from_raw_socket(sock: RawSocket) -> net::UdpSocket {
        let sock = sys::net::Socket::from_inner(sock);
        net::UdpSocket::from_inner(net2::UdpSocket::from_inner(sock))
    }
}
