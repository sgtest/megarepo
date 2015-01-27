// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Experimental extensions to `std` for Windows.
//!
//! For now, this module is limited to extracting handles, file
//! descriptors, and sockets, but its functionality will grow over
//! time.

#![unstable]

pub use sys_common::wtf8::{Wtf8Buf, EncodeWide};

use sys::os_str::Buf;
use sys_common::{AsInner, FromInner};
use ffi::{OsStr, OsString};
use libc;

use old_io;

/// Raw HANDLEs.
pub type Handle = libc::HANDLE;

/// Raw SOCKETs.
pub type Socket = libc::SOCKET;

/// Extract raw handles.
pub trait AsRawHandle {
    /// Extract the raw handle, without taking any ownership.
    fn as_raw_handle(&self) -> Handle;
}

impl AsRawHandle for old_io::fs::File {
    fn as_raw_handle(&self) -> Handle {
        self.as_inner().handle()
    }
}

impl AsRawHandle for old_io::pipe::PipeStream {
    fn as_raw_handle(&self) -> Handle {
        self.as_inner().handle()
    }
}

impl AsRawHandle for old_io::net::pipe::UnixStream {
    fn as_raw_handle(&self) -> Handle {
        self.as_inner().handle()
    }
}

impl AsRawHandle for old_io::net::pipe::UnixListener {
    fn as_raw_handle(&self) -> Handle {
        self.as_inner().handle()
    }
}

impl AsRawHandle for old_io::net::pipe::UnixAcceptor {
    fn as_raw_handle(&self) -> Handle {
        self.as_inner().handle()
    }
}

/// Extract raw sockets.
pub trait AsRawSocket {
    fn as_raw_socket(&self) -> Socket;
}

impl AsRawSocket for old_io::net::tcp::TcpStream {
    fn as_raw_socket(&self) -> Socket {
        self.as_inner().fd()
    }
}

impl AsRawSocket for old_io::net::tcp::TcpListener {
    fn as_raw_socket(&self) -> Socket {
        self.as_inner().socket()
    }
}

impl AsRawSocket for old_io::net::tcp::TcpAcceptor {
    fn as_raw_socket(&self) -> Socket {
        self.as_inner().socket()
    }
}

impl AsRawSocket for old_io::net::udp::UdpSocket {
    fn as_raw_socket(&self) -> Socket {
        self.as_inner().fd()
    }
}

// Windows-specific extensions to `OsString`.
pub trait OsStringExt {
    /// Create an `OsString` from a potentially ill-formed UTF-16 slice of 16-bit code units.
    ///
    /// This is lossless: calling `.encode_wide()` on the resulting string
    /// will always return the original code units.
    fn from_wide(wide: &[u16]) -> Self;
}

impl OsStringExt for OsString {
    fn from_wide(wide: &[u16]) -> OsString {
        FromInner::from_inner(Buf { inner: Wtf8Buf::from_wide(wide) })
    }
}

// Windows-specific extensions to `OsStr`.
pub trait OsStrExt {
    fn encode_wide(&self) -> EncodeWide;
}

impl OsStrExt for OsStr {
    fn encode_wide(&self) -> EncodeWide {
        self.as_inner().inner.encode_wide()
    }
}

/// A prelude for conveniently writing platform-specific code.
///
/// Includes all extension traits, and some important type definitions.
pub mod prelude {
    pub use super::{Socket, Handle, AsRawSocket, AsRawHandle, OsStrExt, OsStringExt};
}
