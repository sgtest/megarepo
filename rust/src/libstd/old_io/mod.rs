// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
//
// ignore-lexer-test FIXME #15883

// FIXME: cover these topics:
//        path, reader, writer, stream, raii (close not needed),
//        stdio, print!, println!, file access, process spawning,
//        error handling


//! I/O, including files, networking, timers, and processes
//!
//! > **Warning**: This module is currently called `old_io` for a reason! The
//! > module is currently being redesigned in a number of RFCs. For more details
//! > follow the RFC repository in connection with [RFC 517][base] or follow
//! > some of these sub-RFCs
//! >
//! > * [String handling][osstr]
//! > * [Core I/O support][core]
//! > * [Deadlines][deadlines]
//! > * [std::env][env]
//! > * [std::process][process]
//!
//! [base]: https://github.com/rust-lang/rfcs/blob/master/text/0517-io-os-reform.md
//! [osstr]: https://github.com/rust-lang/rfcs/pull/575
//! [core]: https://github.com/rust-lang/rfcs/pull/576
//! [deadlines]: https://github.com/rust-lang/rfcs/pull/577
//! [env]: https://github.com/rust-lang/rfcs/pull/578
//! [process]: https://github.com/rust-lang/rfcs/pull/579
//!
//! `std::io` provides Rust's basic I/O types,
//! for reading and writing to files, TCP, UDP,
//! and other types of sockets and pipes,
//! manipulating the file system, spawning processes.
//!
//! # Examples
//!
//! Some examples of obvious things you might want to do
//!
//! * Read lines from stdin
//!
//!     ```rust
//!     # #![feature(old_io, old_path)]
//!     use std::old_io as io;
//!     use std::old_io::*;
//!
//!     let mut stdin = io::stdin();
//!     for line in stdin.lock().lines() {
//!         print!("{}", line.unwrap());
//!     }
//!     ```
//!
//! * Read a complete file
//!
//!     ```rust
//!     # #![feature(old_io, old_path)]
//!     use std::old_io::*;
//!     use std::old_path::Path;
//!
//!     let contents = File::open(&Path::new("message.txt")).read_to_end();
//!     ```
//!
//! * Write a line to a file
//!
//!     ```rust
//!     # #![feature(old_io, old_path)]
//!     # #![allow(unused_must_use)]
//!     use std::old_io::*;
//!     use std::old_path::Path;
//!
//!     let mut file = File::create(&Path::new("message.txt"));
//!     file.write_all(b"hello, file!\n");
//!     # drop(file);
//!     # ::std::old_io::fs::unlink(&Path::new("message.txt"));
//!     ```
//!
//! * Iterate over the lines of a file
//!
//!     ```rust,no_run
//!     # #![feature(old_io, old_path)]
//!     use std::old_io::*;
//!     use std::old_path::Path;
//!
//!     let path = Path::new("message.txt");
//!     let mut file = BufferedReader::new(File::open(&path));
//!     for line in file.lines() {
//!         print!("{}", line.unwrap());
//!     }
//!     ```
//!
//! * Pull the lines of a file into a vector of strings
//!
//!     ```rust,no_run
//!     # #![feature(old_io, old_path)]
//!     use std::old_io::*;
//!     use std::old_path::Path;
//!
//!     let path = Path::new("message.txt");
//!     let mut file = BufferedReader::new(File::open(&path));
//!     let lines: Vec<String> = file.lines().map(|x| x.unwrap()).collect();
//!     ```
//!
//! * Make a simple TCP client connection and request
//!
//!     ```rust
//!     # #![feature(old_io)]
//!     # #![allow(unused_must_use)]
//!     use std::old_io::*;
//!
//!     # // connection doesn't fail if a server is running on 8080
//!     # // locally, we still want to be type checking this code, so lets
//!     # // just stop it running (#11576)
//!     # if false {
//!     let mut socket = TcpStream::connect("127.0.0.1:8080").unwrap();
//!     socket.write_all(b"GET / HTTP/1.0\n\n");
//!     let response = socket.read_to_end();
//!     # }
//!     ```
//!
//! * Make a simple TCP server
//!
//!     ```rust
//!     # #![feature(old_io)]
//!     # fn main() { }
//!     # fn foo() {
//!     # #![allow(dead_code)]
//!     use std::old_io::*;
//!     use std::thread;
//!
//!     let listener = TcpListener::bind("127.0.0.1:80");
//!
//!     // bind the listener to the specified address
//!     let mut acceptor = listener.listen();
//!
//!     fn handle_client(mut stream: TcpStream) {
//!         // ...
//!     # &mut stream; // silence unused mutability/variable warning
//!     }
//!     // accept connections and process them, spawning a new tasks for each one
//!     for stream in acceptor.incoming() {
//!         match stream {
//!             Err(e) => { /* connection failed */ }
//!             Ok(stream) => {
//!                 thread::spawn(move|| {
//!                     // connection succeeded
//!                     handle_client(stream)
//!                 });
//!             }
//!         }
//!     }
//!
//!     // close the socket server
//!     drop(acceptor);
//!     # }
//!     ```
//!
//!
//! # Error Handling
//!
//! I/O is an area where nearly every operation can result in unexpected
//! errors. Errors should be painfully visible when they happen, and handling them
//! should be easy to work with. It should be convenient to handle specific I/O
//! errors, and it should also be convenient to not deal with I/O errors.
//!
//! Rust's I/O employs a combination of techniques to reduce boilerplate
//! while still providing feedback about errors. The basic strategy:
//!
//! * All I/O operations return `IoResult<T>` which is equivalent to
//!   `Result<T, IoError>`. The `Result` type is defined in the `std::result`
//!   module.
//! * If the `Result` type goes unused, then the compiler will by default emit a
//!   warning about the unused result. This is because `Result` has the
//!   `#[must_use]` attribute.
//! * Common traits are implemented for `IoResult`, e.g.
//!   `impl<R: Reader> Reader for IoResult<R>`, so that error values do not have
//!   to be 'unwrapped' before use.
//!
//! These features combine in the API to allow for expressions like
//! `File::create(&Path::new("diary.txt")).write_all(b"Met a girl.\n")`
//! without having to worry about whether "diary.txt" exists or whether
//! the write succeeds. As written, if either `new` or `write_line`
//! encounters an error then the result of the entire expression will
//! be an error.
//!
//! If you wanted to handle the error though you might write:
//!
//! ```rust
//! # #![feature(old_io, old_path)]
//! # #![allow(unused_must_use)]
//! use std::old_io::*;
//! use std::old_path::Path;
//!
//! match File::create(&Path::new("diary.txt")).write_all(b"Met a girl.\n") {
//!     Ok(()) => (), // succeeded
//!     Err(e) => println!("failed to write to my diary: {}", e),
//! }
//!
//! # ::std::old_io::fs::unlink(&Path::new("diary.txt"));
//! ```
//!
//! So what actually happens if `create` encounters an error?
//! It's important to know that what `new` returns is not a `File`
//! but an `IoResult<File>`.  If the file does not open, then `new` will simply
//! return `Err(..)`. Because there is an implementation of `Writer` (the trait
//! required ultimately required for types to implement `write_line`) there is no
//! need to inspect or unwrap the `IoResult<File>` and we simply call `write_line`
//! on it. If `new` returned an `Err(..)` then the followup call to `write_line`
//! will also return an error.
//!
//! ## `try!`
//!
//! Explicit pattern matching on `IoResult`s can get quite verbose, especially
//! when performing many I/O operations. Some examples (like those above) are
//! alleviated with extra methods implemented on `IoResult`, but others have more
//! complex interdependencies among each I/O operation.
//!
//! The `try!` macro from `std::macros` is provided as a method of early-return
//! inside `Result`-returning functions. It expands to an early-return on `Err`
//! and otherwise unwraps the contained `Ok` value.
//!
//! If you wanted to read several `u32`s from a file and return their product:
//!
//! ```rust
//! # #![feature(old_io, old_path)]
//! use std::old_io::*;
//! use std::old_path::Path;
//!
//! fn file_product(p: &Path) -> IoResult<u32> {
//!     let mut f = File::open(p);
//!     let x1 = try!(f.read_le_u32());
//!     let x2 = try!(f.read_le_u32());
//!
//!     Ok(x1 * x2)
//! }
//!
//! match file_product(&Path::new("numbers.bin")) {
//!     Ok(x) => println!("{}", x),
//!     Err(e) => println!("Failed to read numbers!")
//! }
//! ```
//!
//! With `try!` in `file_product`, each `read_le_u32` need not be directly
//! concerned with error handling; instead its caller is responsible for
//! responding to errors that may occur while attempting to read the numbers.

#![unstable(feature = "old_io")]
#![deny(unused_must_use)]
#![allow(deprecated)] // seriously this is all deprecated
#![allow(unused_imports)]
#![deprecated(since = "1.0.0",
              reasons = "APIs have been replaced with new I/O modules such as \
                         std::{io, fs, net, process}")]

pub use self::SeekStyle::*;
pub use self::FileMode::*;
pub use self::FileAccess::*;
pub use self::IoErrorKind::*;

use default::Default;
use error::Error;
use fmt;
use isize;
use iter::{Iterator, IteratorExt};
use marker::{PhantomFn, Sized};
use mem::transmute;
use ops::FnOnce;
use option::Option;
use option::Option::{Some, None};
use os;
use boxed::Box;
use result::Result;
use result::Result::{Ok, Err};
use sys;
use str;
use string::String;
use usize;
use unicode;
use vec::Vec;

// Reexports
pub use self::stdio::stdin;
pub use self::stdio::stdout;
pub use self::stdio::stderr;
pub use self::stdio::print;
pub use self::stdio::println;

pub use self::fs::File;
pub use self::timer::Timer;
pub use self::net::ip::IpAddr;
pub use self::net::tcp::TcpListener;
pub use self::net::tcp::TcpStream;
pub use self::pipe::PipeStream;
pub use self::process::{Process, Command};
pub use self::tempfile::TempDir;

pub use self::mem::{MemReader, BufReader, MemWriter, BufWriter};
pub use self::buffered::{BufferedReader, BufferedWriter, BufferedStream,
                         LineBufferedWriter};
pub use self::comm_adapters::{ChanReader, ChanWriter};

mod buffered;
mod comm_adapters;
mod mem;
mod result;
mod tempfile;
pub mod extensions;
pub mod fs;
pub mod net;
pub mod pipe;
pub mod process;
pub mod stdio;
pub mod timer;
pub mod util;

#[macro_use]
pub mod test;

/// The default buffer size for various I/O operations
// libuv recommends 64k buffers to maximize throughput
// https://groups.google.com/forum/#!topic/libuv/oQO1HJAIDdA
const DEFAULT_BUF_SIZE: usize = 1024 * 64;

/// A convenient typedef of the return value of any I/O action.
pub type IoResult<T> = Result<T, IoError>;

/// The type passed to I/O condition handlers to indicate error
///
/// # FIXME
///
/// Is something like this sufficient? It's kind of archaic
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct IoError {
    /// An enumeration which can be matched against for determining the flavor
    /// of error.
    pub kind: IoErrorKind,
    /// A human-readable description about the error
    pub desc: &'static str,
    /// Detailed information about this error, not always available
    pub detail: Option<String>
}

impl IoError {
    /// Convert an `errno` value into an `IoError`.
    ///
    /// If `detail` is `true`, the `detail` field of the `IoError`
    /// struct is filled with an allocated string describing the error
    /// in more detail, retrieved from the operating system.
    pub fn from_errno(errno: i32, detail: bool) -> IoError {
        let mut err = sys::decode_error(errno as i32);
        if detail && err.kind == OtherIoError {
            err.detail = Some(os::error_string(errno).to_lowercase());
        }
        err
    }

    /// Retrieve the last error to occur as a (detailed) IoError.
    ///
    /// This uses the OS `errno`, and so there should not be any task
    /// descheduling or migration (other than that performed by the
    /// operating system) between the call(s) for which errors are
    /// being checked and the call of this function.
    pub fn last_error() -> IoError {
        IoError::from_errno(os::errno(), true)
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl fmt::Display for IoError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            IoError { kind: OtherIoError, desc: "unknown error", detail: Some(ref detail) } =>
                write!(fmt, "{}", detail),
            IoError { detail: None, desc, .. } =>
                write!(fmt, "{}", desc),
            IoError { detail: Some(ref detail), desc, .. } =>
                write!(fmt, "{} ({})", desc, detail)
        }
    }
}

impl Error for IoError {
    fn description(&self) -> &str { self.desc }
}

/// A list specifying general categories of I/O error.
#[derive(Copy, PartialEq, Eq, Clone, Debug)]
pub enum IoErrorKind {
    /// Any I/O error not part of this list.
    OtherIoError,
    /// The operation could not complete because end of file was reached.
    EndOfFile,
    /// The file was not found.
    FileNotFound,
    /// The file permissions disallowed access to this file.
    PermissionDenied,
    /// A network connection failed for some reason not specified in this list.
    ConnectionFailed,
    /// The network operation failed because the network connection was closed.
    Closed,
    /// The connection was refused by the remote server.
    ConnectionRefused,
    /// The connection was reset by the remote server.
    ConnectionReset,
    /// The connection was aborted (terminated) by the remote server.
    ConnectionAborted,
    /// The network operation failed because it was not connected yet.
    NotConnected,
    /// The operation failed because a pipe was closed.
    BrokenPipe,
    /// A file already existed with that name.
    PathAlreadyExists,
    /// No file exists at that location.
    PathDoesntExist,
    /// The path did not specify the type of file that this operation required. For example,
    /// attempting to copy a directory with the `fs::copy()` operation will fail with this error.
    MismatchedFileTypeForOperation,
    /// The operation temporarily failed (for example, because a signal was received), and retrying
    /// may succeed.
    ResourceUnavailable,
    /// No I/O functionality is available for this task.
    IoUnavailable,
    /// A parameter was incorrect in a way that caused an I/O error not part of this list.
    InvalidInput,
    /// The I/O operation's timeout expired, causing it to be canceled.
    TimedOut,
    /// This write operation failed to write all of its data.
    ///
    /// Normally the write() method on a Writer guarantees that all of its data
    /// has been written, but some operations may be terminated after only
    /// partially writing some data. An example of this is a timed out write
    /// which successfully wrote a known number of bytes, but bailed out after
    /// doing so.
    ///
    /// The payload contained as part of this variant is the number of bytes
    /// which are known to have been successfully written.
    ShortWrite(usize),
    /// The Reader returned 0 bytes from `read()` too many times.
    NoProgress,
}

/// A trait that lets you add a `detail` to an IoError easily
trait UpdateIoError {
    /// Returns an IoError with updated description and detail
    fn update_err<D>(self, desc: &'static str, detail: D) -> Self where
        D: FnOnce(&IoError) -> String;

    /// Returns an IoError with updated detail
    fn update_detail<D>(self, detail: D) -> Self where
        D: FnOnce(&IoError) -> String;

    /// Returns an IoError with update description
    fn update_desc(self, desc: &'static str) -> Self;
}

impl<T> UpdateIoError for IoResult<T> {
    fn update_err<D>(self, desc: &'static str, detail: D) -> IoResult<T> where
        D: FnOnce(&IoError) -> String,
    {
        self.map_err(move |mut e| {
            let detail = detail(&e);
            e.desc = desc;
            e.detail = Some(detail);
            e
        })
    }

    fn update_detail<D>(self, detail: D) -> IoResult<T> where
        D: FnOnce(&IoError) -> String,
    {
        self.map_err(move |mut e| { e.detail = Some(detail(&e)); e })
    }

    fn update_desc(self, desc: &'static str) -> IoResult<T> {
        self.map_err(|mut e| { e.desc = desc; e })
    }
}

static NO_PROGRESS_LIMIT: usize = 1000;

/// A trait for objects which are byte-oriented streams. Readers are defined by
/// one method, `read`. This function will block until data is available,
/// filling in the provided buffer with any data read.
///
/// Readers are intended to be composable with one another. Many objects
/// throughout the I/O and related libraries take and provide types which
/// implement the `Reader` trait.
pub trait Reader {

    // Only method which need to get implemented for this trait

    /// Read bytes, up to the length of `buf` and place them in `buf`.
    /// Returns the number of bytes read. The number of bytes read may
    /// be less than the number requested, even 0. Returns `Err` on EOF.
    ///
    /// # Error
    ///
    /// If an error occurs during this I/O operation, then it is returned as
    /// `Err(IoError)`. Note that end-of-file is considered an error, and can be
    /// inspected for in the error's `kind` field. Also note that reading 0
    /// bytes is not considered an error in all circumstances
    ///
    /// # Implementation Note
    ///
    /// When implementing this method on a new Reader, you are strongly encouraged
    /// not to return 0 if you can avoid it.
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize>;

    // Convenient helper methods based on the above methods

    /// Reads at least `min` bytes and places them in `buf`.
    /// Returns the number of bytes read.
    ///
    /// This will continue to call `read` until at least `min` bytes have been
    /// read. If `read` returns 0 too many times, `NoProgress` will be
    /// returned.
    ///
    /// # Error
    ///
    /// If an error occurs at any point, that error is returned, and no further
    /// bytes are read.
    fn read_at_least(&mut self, min: usize, buf: &mut [u8]) -> IoResult<usize> {
        if min > buf.len() {
            return Err(IoError {
                detail: Some(String::from_str("the buffer is too short")),
                ..standard_error(InvalidInput)
            });
        }
        let mut read = 0;
        while read < min {
            let mut zeroes = 0;
            loop {
                match self.read(&mut buf[read..]) {
                    Ok(0) => {
                        zeroes += 1;
                        if zeroes >= NO_PROGRESS_LIMIT {
                            return Err(standard_error(NoProgress));
                        }
                    }
                    Ok(n) => {
                        read += n;
                        break;
                    }
                    err@Err(_) => return err
                }
            }
        }
        Ok(read)
    }

    /// Reads a single byte. Returns `Err` on EOF.
    fn read_byte(&mut self) -> IoResult<u8> {
        let mut buf = [0];
        try!(self.read_at_least(1, &mut buf));
        Ok(buf[0])
    }

    /// Reads up to `len` bytes and appends them to a vector.
    /// Returns the number of bytes read. The number of bytes read may be
    /// less than the number requested, even 0. Returns Err on EOF.
    ///
    /// # Error
    ///
    /// If an error occurs during this I/O operation, then it is returned
    /// as `Err(IoError)`. See `read()` for more details.
    fn push(&mut self, len: usize, buf: &mut Vec<u8>) -> IoResult<usize> {
        let start_len = buf.len();
        buf.reserve(len);

        let n = {
            let s = unsafe { slice_vec_capacity(buf, start_len, start_len + len) };
            try!(self.read(s))
        };
        unsafe { buf.set_len(start_len + n) };
        Ok(n)
    }

    /// Reads at least `min` bytes, but no more than `len`, and appends them to
    /// a vector.
    /// Returns the number of bytes read.
    ///
    /// This will continue to call `read` until at least `min` bytes have been
    /// read. If `read` returns 0 too many times, `NoProgress` will be
    /// returned.
    ///
    /// # Error
    ///
    /// If an error occurs at any point, that error is returned, and no further
    /// bytes are read.
    fn push_at_least(&mut self, min: usize, len: usize, buf: &mut Vec<u8>) -> IoResult<usize> {
        if min > len {
            return Err(IoError {
                detail: Some(String::from_str("the buffer is too short")),
                ..standard_error(InvalidInput)
            });
        }

        let start_len = buf.len();
        buf.reserve(len);

        // we can't just use self.read_at_least(min, slice) because we need to push
        // successful reads onto the vector before any returned errors.

        let mut read = 0;
        while read < min {
            read += {
                let s = unsafe { slice_vec_capacity(buf, start_len + read, start_len + len) };
                try!(self.read_at_least(1, s))
            };
            unsafe { buf.set_len(start_len + read) };
        }
        Ok(read)
    }

    /// Reads exactly `len` bytes and gives you back a new vector of length
    /// `len`
    ///
    /// # Error
    ///
    /// Fails with the same conditions as `read`. Additionally returns error
    /// on EOF. Note that if an error is returned, then some number of bytes may
    /// have already been consumed from the underlying reader, and they are lost
    /// (not returned as part of the error). If this is unacceptable, then it is
    /// recommended to use the `push_at_least` or `read` methods.
    fn read_exact(&mut self, len: usize) -> IoResult<Vec<u8>> {
        let mut buf = Vec::with_capacity(len);
        match self.push_at_least(len, len, &mut buf) {
            Ok(_) => Ok(buf),
            Err(e) => Err(e),
        }
    }

    /// Reads all remaining bytes from the stream.
    ///
    /// # Error
    ///
    /// Returns any non-EOF error immediately. Previously read bytes are
    /// discarded when an error is returned.
    ///
    /// When EOF is encountered, all bytes read up to that point are returned.
    fn read_to_end(&mut self) -> IoResult<Vec<u8>> {
        let mut buf = Vec::with_capacity(DEFAULT_BUF_SIZE);
        loop {
            match self.push_at_least(1, DEFAULT_BUF_SIZE, &mut buf) {
                Ok(_) => {}
                Err(ref e) if e.kind == EndOfFile => break,
                Err(e) => return Err(e)
            }
        }
        return Ok(buf);
    }

    /// Reads all of the remaining bytes of this stream, interpreting them as a
    /// UTF-8 encoded stream. The corresponding string is returned.
    ///
    /// # Error
    ///
    /// This function returns all of the same errors as `read_to_end` with an
    /// additional error if the reader's contents are not a valid sequence of
    /// UTF-8 bytes.
    fn read_to_string(&mut self) -> IoResult<String> {
        self.read_to_end().and_then(|s| {
            match String::from_utf8(s) {
                Ok(s)  => Ok(s),
                Err(_) => Err(standard_error(InvalidInput)),
            }
        })
    }

    // Byte conversion helpers

    /// Reads `n` little-endian unsigned integer bytes.
    ///
    /// `n` must be between 1 and 8, inclusive.
    fn read_le_uint_n(&mut self, nbytes: usize) -> IoResult<u64> {
        assert!(nbytes > 0 && nbytes <= 8);

        let mut val = 0;
        let mut pos = 0;
        let mut i = nbytes;
        while i > 0 {
            val += (try!(self.read_u8()) as u64) << pos;
            pos += 8;
            i -= 1;
        }
        Ok(val)
    }

    /// Reads `n` little-endian signed integer bytes.
    ///
    /// `n` must be between 1 and 8, inclusive.
    fn read_le_int_n(&mut self, nbytes: usize) -> IoResult<i64> {
        self.read_le_uint_n(nbytes).map(|i| extend_sign(i, nbytes))
    }

    /// Reads `n` big-endian unsigned integer bytes.
    ///
    /// `n` must be between 1 and 8, inclusive.
    fn read_be_uint_n(&mut self, nbytes: usize) -> IoResult<u64> {
        assert!(nbytes > 0 && nbytes <= 8);

        let mut val = 0;
        let mut i = nbytes;
        while i > 0 {
            i -= 1;
            val += (try!(self.read_u8()) as u64) << i * 8;
        }
        Ok(val)
    }

    /// Reads `n` big-endian signed integer bytes.
    ///
    /// `n` must be between 1 and 8, inclusive.
    fn read_be_int_n(&mut self, nbytes: usize) -> IoResult<i64> {
        self.read_be_uint_n(nbytes).map(|i| extend_sign(i, nbytes))
    }

    /// Reads a little-endian unsigned integer.
    ///
    /// The number of bytes returned is system-dependent.
    fn read_le_uint(&mut self) -> IoResult<usize> {
        self.read_le_uint_n(usize::BYTES as usize).map(|i| i as usize)
    }

    /// Reads a little-endian integer.
    ///
    /// The number of bytes returned is system-dependent.
    fn read_le_int(&mut self) -> IoResult<isize> {
        self.read_le_int_n(isize::BYTES as usize).map(|i| i as isize)
    }

    /// Reads a big-endian unsigned integer.
    ///
    /// The number of bytes returned is system-dependent.
    fn read_be_uint(&mut self) -> IoResult<usize> {
        self.read_be_uint_n(usize::BYTES as usize).map(|i| i as usize)
    }

    /// Reads a big-endian integer.
    ///
    /// The number of bytes returned is system-dependent.
    fn read_be_int(&mut self) -> IoResult<isize> {
        self.read_be_int_n(isize::BYTES as usize).map(|i| i as isize)
    }

    /// Reads a big-endian `u64`.
    ///
    /// `u64`s are 8 bytes long.
    fn read_be_u64(&mut self) -> IoResult<u64> {
        self.read_be_uint_n(8)
    }

    /// Reads a big-endian `u32`.
    ///
    /// `u32`s are 4 bytes long.
    fn read_be_u32(&mut self) -> IoResult<u32> {
        self.read_be_uint_n(4).map(|i| i as u32)
    }

    /// Reads a big-endian `u16`.
    ///
    /// `u16`s are 2 bytes long.
    fn read_be_u16(&mut self) -> IoResult<u16> {
        self.read_be_uint_n(2).map(|i| i as u16)
    }

    /// Reads a big-endian `i64`.
    ///
    /// `i64`s are 8 bytes long.
    fn read_be_i64(&mut self) -> IoResult<i64> {
        self.read_be_int_n(8)
    }

    /// Reads a big-endian `i32`.
    ///
    /// `i32`s are 4 bytes long.
    fn read_be_i32(&mut self) -> IoResult<i32> {
        self.read_be_int_n(4).map(|i| i as i32)
    }

    /// Reads a big-endian `i16`.
    ///
    /// `i16`s are 2 bytes long.
    fn read_be_i16(&mut self) -> IoResult<i16> {
        self.read_be_int_n(2).map(|i| i as i16)
    }

    /// Reads a big-endian `f64`.
    ///
    /// `f64`s are 8 byte, IEEE754 double-precision floating point numbers.
    fn read_be_f64(&mut self) -> IoResult<f64> {
        self.read_be_u64().map(|i| unsafe {
            transmute::<u64, f64>(i)
        })
    }

    /// Reads a big-endian `f32`.
    ///
    /// `f32`s are 4 byte, IEEE754 single-precision floating point numbers.
    fn read_be_f32(&mut self) -> IoResult<f32> {
        self.read_be_u32().map(|i| unsafe {
            transmute::<u32, f32>(i)
        })
    }

    /// Reads a little-endian `u64`.
    ///
    /// `u64`s are 8 bytes long.
    fn read_le_u64(&mut self) -> IoResult<u64> {
        self.read_le_uint_n(8)
    }

    /// Reads a little-endian `u32`.
    ///
    /// `u32`s are 4 bytes long.
    fn read_le_u32(&mut self) -> IoResult<u32> {
        self.read_le_uint_n(4).map(|i| i as u32)
    }

    /// Reads a little-endian `u16`.
    ///
    /// `u16`s are 2 bytes long.
    fn read_le_u16(&mut self) -> IoResult<u16> {
        self.read_le_uint_n(2).map(|i| i as u16)
    }

    /// Reads a little-endian `i64`.
    ///
    /// `i64`s are 8 bytes long.
    fn read_le_i64(&mut self) -> IoResult<i64> {
        self.read_le_int_n(8)
    }

    /// Reads a little-endian `i32`.
    ///
    /// `i32`s are 4 bytes long.
    fn read_le_i32(&mut self) -> IoResult<i32> {
        self.read_le_int_n(4).map(|i| i as i32)
    }

    /// Reads a little-endian `i16`.
    ///
    /// `i16`s are 2 bytes long.
    fn read_le_i16(&mut self) -> IoResult<i16> {
        self.read_le_int_n(2).map(|i| i as i16)
    }

    /// Reads a little-endian `f64`.
    ///
    /// `f64`s are 8 byte, IEEE754 double-precision floating point numbers.
    fn read_le_f64(&mut self) -> IoResult<f64> {
        self.read_le_u64().map(|i| unsafe {
            transmute::<u64, f64>(i)
        })
    }

    /// Reads a little-endian `f32`.
    ///
    /// `f32`s are 4 byte, IEEE754 single-precision floating point numbers.
    fn read_le_f32(&mut self) -> IoResult<f32> {
        self.read_le_u32().map(|i| unsafe {
            transmute::<u32, f32>(i)
        })
    }

    /// Read a u8.
    ///
    /// `u8`s are 1 byte.
    fn read_u8(&mut self) -> IoResult<u8> {
        self.read_byte()
    }

    /// Read an i8.
    ///
    /// `i8`s are 1 byte.
    fn read_i8(&mut self) -> IoResult<i8> {
        self.read_byte().map(|i| i as i8)
    }
}

/// A reader which can be converted to a RefReader.
pub trait ByRefReader {
    /// Creates a wrapper around a mutable reference to the reader.
    ///
    /// This is useful to allow applying adaptors while still
    /// retaining ownership of the original value.
    fn by_ref<'a>(&'a mut self) -> RefReader<'a, Self>;
}

impl<T: Reader> ByRefReader for T {
    fn by_ref<'a>(&'a mut self) -> RefReader<'a, T> {
        RefReader { inner: self }
    }
}

/// A reader which can be converted to bytes.
pub trait BytesReader {
    /// Create an iterator that reads a single byte on
    /// each iteration, until EOF.
    ///
    /// # Error
    ///
    /// Any error other than `EndOfFile` that is produced by the underlying Reader
    /// is returned by the iterator and should be handled by the caller.
    fn bytes<'r>(&'r mut self) -> extensions::Bytes<'r, Self>;
}

impl<T: Reader> BytesReader for T {
    fn bytes<'r>(&'r mut self) -> extensions::Bytes<'r, T> {
        extensions::Bytes::new(self)
    }
}

impl<'a> Reader for Box<Reader+'a> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        let reader: &mut Reader = &mut **self;
        reader.read(buf)
    }
}

impl<'a> Reader for &'a mut (Reader+'a) {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> { (*self).read(buf) }
}

/// Returns a slice of `v` between `start` and `end`.
///
/// Similar to `slice()` except this function only bounds the slice on the
/// capacity of `v`, not the length.
///
/// # Panics
///
/// Panics when `start` or `end` point outside the capacity of `v`, or when
/// `start` > `end`.
// Private function here because we aren't sure if we want to expose this as
// API yet. If so, it should be a method on Vec.
unsafe fn slice_vec_capacity<'a, T>(v: &'a mut Vec<T>, start: usize, end: usize) -> &'a mut [T] {
    use slice;

    assert!(start <= end);
    assert!(end <= v.capacity());
    slice::from_raw_parts_mut(
        v.as_mut_ptr().offset(start as isize),
        end - start
    )
}

/// A `RefReader` is a struct implementing `Reader` which contains a reference
/// to another reader. This is often useful when composing streams.
///
/// # Examples
///
/// ```
/// # #![feature(old_io)]
/// use std::old_io as io;
/// use std::old_io::*;
/// use std::old_io::util::LimitReader;
///
/// fn process_input<R: Reader>(r: R) {}
///
/// let mut stream = io::stdin();
///
/// // Only allow the function to process at most one kilobyte of input
/// {
///     let stream = LimitReader::new(stream.by_ref(), 1024);
///     process_input(stream);
/// }
///
/// // 'stream' is still available for use here
/// ```
pub struct RefReader<'a, R:'a> {
    /// The underlying reader which this is referencing
    inner: &'a mut R
}

impl<'a, R: Reader> Reader for RefReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> { self.inner.read(buf) }
}

impl<'a, R: Buffer> Buffer for RefReader<'a, R> {
    fn fill_buf(&mut self) -> IoResult<&[u8]> { self.inner.fill_buf() }
    fn consume(&mut self, amt: usize) { self.inner.consume(amt) }
}

fn extend_sign(val: u64, nbytes: usize) -> i64 {
    let shift = (8 - nbytes) * 8;
    (val << shift) as i64 >> shift
}

/// A trait for objects which are byte-oriented streams. Writers are defined by
/// one method, `write`. This function will block until the provided buffer of
/// bytes has been entirely written, and it will return any failures which occur.
///
/// Another commonly overridden method is the `flush` method for writers such as
/// buffered writers.
///
/// Writers are intended to be composable with one another. Many objects
/// throughout the I/O and related libraries take and provide types which
/// implement the `Writer` trait.
pub trait Writer {
    /// Write the entirety of a given buffer
    ///
    /// # Errors
    ///
    /// If an error happens during the I/O operation, the error is returned as
    /// `Err`. Note that it is considered an error if the entire buffer could
    /// not be written, and if an error is returned then it is unknown how much
    /// data (if any) was actually written.
    fn write_all(&mut self, buf: &[u8]) -> IoResult<()>;

    /// Deprecated, this method was renamed to `write_all`
    #[unstable(feature = "io")]
    #[deprecated(since = "1.0.0", reason = "renamed to `write_all`")]
    fn write(&mut self, buf: &[u8]) -> IoResult<()> { self.write_all(buf) }

    /// Flush this output stream, ensuring that all intermediately buffered
    /// contents reach their destination.
    ///
    /// This is by default a no-op and implementers of the `Writer` trait should
    /// decide whether their stream needs to be buffered or not.
    fn flush(&mut self) -> IoResult<()> { Ok(()) }

    /// Writes a formatted string into this writer, returning any error
    /// encountered.
    ///
    /// This method is primarily used to interface with the `format_args!`
    /// macro, but it is rare that this should explicitly be called. The
    /// `write!` macro should be favored to invoke this method instead.
    ///
    /// # Errors
    ///
    /// This function will return any I/O error reported while formatting.
    fn write_fmt(&mut self, fmt: fmt::Arguments) -> IoResult<()> {
        // Create a shim which translates a Writer to a fmt::Write and saves
        // off I/O errors. instead of discarding them
        struct Adaptor<'a, T: ?Sized +'a> {
            inner: &'a mut T,
            error: IoResult<()>,
        }

        impl<'a, T: ?Sized + Writer> fmt::Write for Adaptor<'a, T> {
            fn write_str(&mut self, s: &str) -> fmt::Result {
                match self.inner.write_all(s.as_bytes()) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        self.error = Err(e);
                        Err(fmt::Error)
                    }
                }
            }
        }

        let mut output = Adaptor { inner: self, error: Ok(()) };
        match fmt::write(&mut output, fmt) {
            Ok(()) => Ok(()),
            Err(..) => output.error
        }
    }


    /// Write a rust string into this sink.
    ///
    /// The bytes written will be the UTF-8 encoded version of the input string.
    /// If other encodings are desired, it is recommended to compose this stream
    /// with another performing the conversion, or to use `write` with a
    /// converted byte-array instead.
    #[inline]
    fn write_str(&mut self, s: &str) -> IoResult<()> {
        self.write_all(s.as_bytes())
    }

    /// Writes a string into this sink, and then writes a literal newline (`\n`)
    /// byte afterwards. Note that the writing of the newline is *not* atomic in
    /// the sense that the call to `write` is invoked twice (once with the
    /// string and once with a newline character).
    ///
    /// If other encodings or line ending flavors are desired, it is recommended
    /// that the `write` method is used specifically instead.
    #[inline]
    fn write_line(&mut self, s: &str) -> IoResult<()> {
        self.write_str(s).and_then(|()| self.write_all(&[b'\n']))
    }

    /// Write a single char, encoded as UTF-8.
    #[inline]
    fn write_char(&mut self, c: char) -> IoResult<()> {
        let mut buf = [0; 4];
        let n = c.encode_utf8(&mut buf).unwrap_or(0);
        self.write_all(&buf[..n])
    }

    /// Write the result of passing n through `isize::to_str_bytes`.
    #[inline]
    fn write_int(&mut self, n: isize) -> IoResult<()> {
        write!(self, "{}", n)
    }

    /// Write the result of passing n through `usize::to_str_bytes`.
    #[inline]
    fn write_uint(&mut self, n: usize) -> IoResult<()> {
        write!(self, "{}", n)
    }

    /// Write a little-endian usize (number of bytes depends on system).
    #[inline]
    fn write_le_uint(&mut self, n: usize) -> IoResult<()> {
        extensions::u64_to_le_bytes(n as u64, usize::BYTES as usize, |v| self.write_all(v))
    }

    /// Write a little-endian isize (number of bytes depends on system).
    #[inline]
    fn write_le_int(&mut self, n: isize) -> IoResult<()> {
        extensions::u64_to_le_bytes(n as u64, isize::BYTES as usize, |v| self.write_all(v))
    }

    /// Write a big-endian usize (number of bytes depends on system).
    #[inline]
    fn write_be_uint(&mut self, n: usize) -> IoResult<()> {
        extensions::u64_to_be_bytes(n as u64, usize::BYTES as usize, |v| self.write_all(v))
    }

    /// Write a big-endian isize (number of bytes depends on system).
    #[inline]
    fn write_be_int(&mut self, n: isize) -> IoResult<()> {
        extensions::u64_to_be_bytes(n as u64, isize::BYTES as usize, |v| self.write_all(v))
    }

    /// Write a big-endian u64 (8 bytes).
    #[inline]
    fn write_be_u64(&mut self, n: u64) -> IoResult<()> {
        extensions::u64_to_be_bytes(n, 8, |v| self.write_all(v))
    }

    /// Write a big-endian u32 (4 bytes).
    #[inline]
    fn write_be_u32(&mut self, n: u32) -> IoResult<()> {
        extensions::u64_to_be_bytes(n as u64, 4, |v| self.write_all(v))
    }

    /// Write a big-endian u16 (2 bytes).
    #[inline]
    fn write_be_u16(&mut self, n: u16) -> IoResult<()> {
        extensions::u64_to_be_bytes(n as u64, 2, |v| self.write_all(v))
    }

    /// Write a big-endian i64 (8 bytes).
    #[inline]
    fn write_be_i64(&mut self, n: i64) -> IoResult<()> {
        extensions::u64_to_be_bytes(n as u64, 8, |v| self.write_all(v))
    }

    /// Write a big-endian i32 (4 bytes).
    #[inline]
    fn write_be_i32(&mut self, n: i32) -> IoResult<()> {
        extensions::u64_to_be_bytes(n as u64, 4, |v| self.write_all(v))
    }

    /// Write a big-endian i16 (2 bytes).
    #[inline]
    fn write_be_i16(&mut self, n: i16) -> IoResult<()> {
        extensions::u64_to_be_bytes(n as u64, 2, |v| self.write_all(v))
    }

    /// Write a big-endian IEEE754 double-precision floating-point (8 bytes).
    #[inline]
    fn write_be_f64(&mut self, f: f64) -> IoResult<()> {
        unsafe {
            self.write_be_u64(transmute(f))
        }
    }

    /// Write a big-endian IEEE754 single-precision floating-point (4 bytes).
    #[inline]
    fn write_be_f32(&mut self, f: f32) -> IoResult<()> {
        unsafe {
            self.write_be_u32(transmute(f))
        }
    }

    /// Write a little-endian u64 (8 bytes).
    #[inline]
    fn write_le_u64(&mut self, n: u64) -> IoResult<()> {
        extensions::u64_to_le_bytes(n, 8, |v| self.write_all(v))
    }

    /// Write a little-endian u32 (4 bytes).
    #[inline]
    fn write_le_u32(&mut self, n: u32) -> IoResult<()> {
        extensions::u64_to_le_bytes(n as u64, 4, |v| self.write_all(v))
    }

    /// Write a little-endian u16 (2 bytes).
    #[inline]
    fn write_le_u16(&mut self, n: u16) -> IoResult<()> {
        extensions::u64_to_le_bytes(n as u64, 2, |v| self.write_all(v))
    }

    /// Write a little-endian i64 (8 bytes).
    #[inline]
    fn write_le_i64(&mut self, n: i64) -> IoResult<()> {
        extensions::u64_to_le_bytes(n as u64, 8, |v| self.write_all(v))
    }

    /// Write a little-endian i32 (4 bytes).
    #[inline]
    fn write_le_i32(&mut self, n: i32) -> IoResult<()> {
        extensions::u64_to_le_bytes(n as u64, 4, |v| self.write_all(v))
    }

    /// Write a little-endian i16 (2 bytes).
    #[inline]
    fn write_le_i16(&mut self, n: i16) -> IoResult<()> {
        extensions::u64_to_le_bytes(n as u64, 2, |v| self.write_all(v))
    }

    /// Write a little-endian IEEE754 double-precision floating-point
    /// (8 bytes).
    #[inline]
    fn write_le_f64(&mut self, f: f64) -> IoResult<()> {
        unsafe {
            self.write_le_u64(transmute(f))
        }
    }

    /// Write a little-endian IEEE754 single-precision floating-point
    /// (4 bytes).
    #[inline]
    fn write_le_f32(&mut self, f: f32) -> IoResult<()> {
        unsafe {
            self.write_le_u32(transmute(f))
        }
    }

    /// Write a u8 (1 byte).
    #[inline]
    fn write_u8(&mut self, n: u8) -> IoResult<()> {
        self.write_all(&[n])
    }

    /// Write an i8 (1 byte).
    #[inline]
    fn write_i8(&mut self, n: i8) -> IoResult<()> {
        self.write_all(&[n as u8])
    }
}

/// A writer which can be converted to a RefWriter.
pub trait ByRefWriter {
    /// Creates a wrapper around a mutable reference to the writer.
    ///
    /// This is useful to allow applying wrappers while still
    /// retaining ownership of the original value.
    #[inline]
    fn by_ref<'a>(&'a mut self) -> RefWriter<'a, Self>;
}

impl<T: Writer> ByRefWriter for T {
    fn by_ref<'a>(&'a mut self) -> RefWriter<'a, T> {
        RefWriter { inner: self }
    }
}

impl<'a> Writer for Box<Writer+'a> {
    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> IoResult<()> {
        (&mut **self).write_all(buf)
    }

    #[inline]
    fn flush(&mut self) -> IoResult<()> {
        (&mut **self).flush()
    }
}

impl<'a> Writer for &'a mut (Writer+'a) {
    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> IoResult<()> { (**self).write_all(buf) }

    #[inline]
    fn flush(&mut self) -> IoResult<()> { (**self).flush() }
}

/// A `RefWriter` is a struct implementing `Writer` which contains a reference
/// to another writer. This is often useful when composing streams.
///
/// # Examples
///
/// ```
/// # #![feature(old_io)]
/// use std::old_io::util::TeeReader;
/// use std::old_io::*;
///
/// fn process_input<R: Reader>(r: R) {}
///
/// let mut output = Vec::new();
///
/// {
///     // Don't give ownership of 'output' to the 'tee'. Instead we keep a
///     // handle to it in the outer scope
///     let mut tee = TeeReader::new(stdin(), output.by_ref());
///     process_input(tee);
/// }
///
/// println!("input processed: {:?}", output);
/// ```
pub struct RefWriter<'a, W:'a> {
    /// The underlying writer which this is referencing
    inner: &'a mut W
}

impl<'a, W: Writer> Writer for RefWriter<'a, W> {
    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> IoResult<()> { self.inner.write_all(buf) }

    #[inline]
    fn flush(&mut self) -> IoResult<()> { self.inner.flush() }
}


/// A Stream is a readable and a writable object. Data written is typically
/// received by the object which reads receive data from.
pub trait Stream: Reader + Writer { }

impl<T: Reader + Writer> Stream for T {}

/// An iterator that reads a line on each iteration,
/// until `.read_line()` encounters `EndOfFile`.
///
/// # Notes about the Iteration Protocol
///
/// The `Lines` may yield `None` and thus terminate
/// an iteration, but continue to yield elements if iteration
/// is attempted again.
///
/// # Error
///
/// Any error other than `EndOfFile` that is produced by the underlying Reader
/// is returned by the iterator and should be handled by the caller.
pub struct Lines<'r, T:'r> {
    buffer: &'r mut T,
}

impl<'r, T: Buffer> Iterator for Lines<'r, T> {
    type Item = IoResult<String>;

    fn next(&mut self) -> Option<IoResult<String>> {
        match self.buffer.read_line() {
            Ok(x) => Some(Ok(x)),
            Err(IoError { kind: EndOfFile, ..}) => None,
            Err(y) => Some(Err(y))
        }
    }
}

/// An iterator that reads a utf8-encoded character on each iteration,
/// until `.read_char()` encounters `EndOfFile`.
///
/// # Notes about the Iteration Protocol
///
/// The `Chars` may yield `None` and thus terminate
/// an iteration, but continue to yield elements if iteration
/// is attempted again.
///
/// # Error
///
/// Any error other than `EndOfFile` that is produced by the underlying Reader
/// is returned by the iterator and should be handled by the caller.
pub struct Chars<'r, T:'r> {
    buffer: &'r mut T
}

impl<'r, T: Buffer> Iterator for Chars<'r, T> {
    type Item = IoResult<char>;

    fn next(&mut self) -> Option<IoResult<char>> {
        match self.buffer.read_char() {
            Ok(x) => Some(Ok(x)),
            Err(IoError { kind: EndOfFile, ..}) => None,
            Err(y) => Some(Err(y))
        }
    }
}

/// A Buffer is a type of reader which has some form of internal buffering to
/// allow certain kinds of reading operations to be more optimized than others.
/// This type extends the `Reader` trait with a few methods that are not
/// possible to reasonably implement with purely a read interface.
pub trait Buffer: Reader {
    /// Fills the internal buffer of this object, returning the buffer contents.
    /// Note that none of the contents will be "read" in the sense that later
    /// calling `read` may return the same contents.
    ///
    /// The `consume` function must be called with the number of bytes that are
    /// consumed from this buffer returned to ensure that the bytes are never
    /// returned twice.
    ///
    /// # Error
    ///
    /// This function will return an I/O error if the underlying reader was
    /// read, but returned an error. Note that it is not an error to return a
    /// 0-length buffer.
    fn fill_buf<'a>(&'a mut self) -> IoResult<&'a [u8]>;

    /// Tells this buffer that `amt` bytes have been consumed from the buffer,
    /// so they should no longer be returned in calls to `read`.
    fn consume(&mut self, amt: usize);

    /// Reads the next line of input, interpreted as a sequence of UTF-8
    /// encoded Unicode codepoints. If a newline is encountered, then the
    /// newline is contained in the returned string.
    ///
    /// # Examples
    ///
    /// ```
    /// # #![feature(old_io)]
    /// use std::old_io::*;
    ///
    /// let mut reader = BufReader::new(b"hello\nworld");
    /// assert_eq!("hello\n", &*reader.read_line().unwrap());
    /// ```
    ///
    /// # Error
    ///
    /// This function has the same error semantics as `read_until`:
    ///
    /// * All non-EOF errors will be returned immediately
    /// * If an error is returned previously consumed bytes are lost
    /// * EOF is only returned if no bytes have been read
    /// * Reach EOF may mean that the delimiter is not present in the return
    ///   value
    ///
    /// Additionally, this function can fail if the line of input read is not a
    /// valid UTF-8 sequence of bytes.
    fn read_line(&mut self) -> IoResult<String> {
        self.read_until(b'\n').and_then(|line|
            match String::from_utf8(line) {
                Ok(s)  => Ok(s),
                Err(_) => Err(standard_error(InvalidInput)),
            }
        )
    }

    /// Reads a sequence of bytes leading up to a specified delimiter. Once the
    /// specified byte is encountered, reading ceases and the bytes up to and
    /// including the delimiter are returned.
    ///
    /// # Error
    ///
    /// If any I/O error is encountered other than EOF, the error is immediately
    /// returned. Note that this may discard bytes which have already been read,
    /// and those bytes will *not* be returned. It is recommended to use other
    /// methods if this case is worrying.
    ///
    /// If EOF is encountered, then this function will return EOF if 0 bytes
    /// have been read, otherwise the pending byte buffer is returned. This
    /// is the reason that the byte buffer returned may not always contain the
    /// delimiter.
    fn read_until(&mut self, byte: u8) -> IoResult<Vec<u8>> {
        let mut res = Vec::new();

        loop {
            let (done, used) = {
                let available = match self.fill_buf() {
                    Ok(n) => n,
                    Err(ref e) if res.len() > 0 && e.kind == EndOfFile => {
                        return Ok(res);
                    }
                    Err(e) => return Err(e)
                };
                match available.iter().position(|&b| b == byte) {
                    Some(i) => {
                        res.push_all(&available[..i + 1]);
                        (true, i + 1)
                    }
                    None => {
                        res.push_all(available);
                        (false, available.len())
                    }
                }
            };
            self.consume(used);
            if done {
                return Ok(res);
            }
        }
    }

    /// Reads the next utf8-encoded character from the underlying stream.
    ///
    /// # Error
    ///
    /// If an I/O error occurs, or EOF, then this function will return `Err`.
    /// This function will also return error if the stream does not contain a
    /// valid utf-8 encoded codepoint as the next few bytes in the stream.
    fn read_char(&mut self) -> IoResult<char> {
        let first_byte = try!(self.read_byte());
        let width = unicode::str::utf8_char_width(first_byte);
        if width == 1 { return Ok(first_byte as char) }
        if width == 0 { return Err(standard_error(InvalidInput)) } // not utf8
        let mut buf = [first_byte, 0, 0, 0];
        {
            let mut start = 1;
            while start < width {
                match try!(self.read(&mut buf[start .. width])) {
                    n if n == width - start => break,
                    n if n < width - start => { start += n; }
                    _ => return Err(standard_error(InvalidInput)),
                }
            }
        }
        match str::from_utf8(&buf[..width]).ok() {
            Some(s) => Ok(s.char_at(0)),
            None => Err(standard_error(InvalidInput))
        }
    }
}

/// Extension methods for the Buffer trait which are included in the prelude.
pub trait BufferPrelude {
    /// Create an iterator that reads a utf8-encoded character on each iteration
    /// until EOF.
    ///
    /// # Error
    ///
    /// Any error other than `EndOfFile` that is produced by the underlying Reader
    /// is returned by the iterator and should be handled by the caller.
    fn chars<'r>(&'r mut self) -> Chars<'r, Self>;

    /// Create an iterator that reads a line on each iteration until EOF.
    ///
    /// # Error
    ///
    /// Any error other than `EndOfFile` that is produced by the underlying Reader
    /// is returned by the iterator and should be handled by the caller.
    fn lines<'r>(&'r mut self) -> Lines<'r, Self>;
}

impl<T: Buffer> BufferPrelude for T {
    fn chars<'r>(&'r mut self) -> Chars<'r, T> {
        Chars { buffer: self }
    }

    fn lines<'r>(&'r mut self) -> Lines<'r, T> {
        Lines { buffer: self }
    }
}

/// When seeking, the resulting cursor is offset from a base by the offset given
/// to the `seek` function. The base used is specified by this enumeration.
#[derive(Copy)]
pub enum SeekStyle {
    /// Seek from the beginning of the stream
    SeekSet,
    /// Seek from the end of the stream
    SeekEnd,
    /// Seek from the current position
    SeekCur,
}

/// An object implementing `Seek` internally has some form of cursor which can
/// be moved within a stream of bytes. The stream typically has a fixed size,
/// allowing seeking relative to either end.
pub trait Seek {
    /// Return position of file cursor in the stream
    fn tell(&self) -> IoResult<u64>;

    /// Seek to an offset in a stream
    ///
    /// A successful seek clears the EOF indicator. Seeking beyond EOF is
    /// allowed, but seeking before position 0 is not allowed.
    ///
    /// # Errors
    ///
    /// * Seeking to a negative offset is considered an error
    /// * Seeking past the end of the stream does not modify the underlying
    ///   stream, but the next write may cause the previous data to be filled in
    ///   with a bit pattern.
    fn seek(&mut self, pos: i64, style: SeekStyle) -> IoResult<()>;
}

/// A listener is a value that can consume itself to start listening for
/// connections.
///
/// Doing so produces some sort of Acceptor.
pub trait Listener<A: Acceptor> {
    /// Spin up the listener and start queuing incoming connections
    ///
    /// # Error
    ///
    /// Returns `Err` if this listener could not be bound to listen for
    /// connections. In all cases, this listener is consumed.
    fn listen(self) -> IoResult<A>;
}

/// An acceptor is a value that presents incoming connections
pub trait Acceptor {
    /// Type of connection that is accepted by this acceptor.
    type Connection;

    /// Wait for and accept an incoming connection
    ///
    /// # Error
    ///
    /// Returns `Err` if an I/O error is encountered.
    fn accept(&mut self) -> IoResult<Self::Connection>;

    /// Create an iterator over incoming connection attempts.
    ///
    /// Note that I/O errors will be yielded by the iterator itself.
    fn incoming<'r>(&'r mut self) -> IncomingConnections<'r, Self> {
        IncomingConnections { inc: self }
    }
}

/// An infinite iterator over incoming connection attempts.
/// Calling `next` will block the task until a connection is attempted.
///
/// Since connection attempts can continue forever, this iterator always returns
/// `Some`. The `Some` contains the `IoResult` representing whether the
/// connection attempt was successful.  A successful connection will be wrapped
/// in `Ok`. A failed connection is represented as an `Err`.
pub struct IncomingConnections<'a, A: ?Sized +'a> {
    inc: &'a mut A,
}

impl<'a, A: ?Sized + Acceptor> Iterator for IncomingConnections<'a, A> {
    type Item = IoResult<A::Connection>;

    fn next(&mut self) -> Option<IoResult<A::Connection>> {
        Some(self.inc.accept())
    }
}

/// Creates a standard error for a commonly used flavor of error. The `detail`
/// field of the returned error will always be `None`.
///
/// # Examples
///
/// ```
/// # #![feature(old_io)]
/// use std::old_io as io;
///
/// let eof = io::standard_error(io::EndOfFile);
/// let einval = io::standard_error(io::InvalidInput);
/// ```
pub fn standard_error(kind: IoErrorKind) -> IoError {
    let desc = match kind {
        EndOfFile => "end of file",
        IoUnavailable => "I/O is unavailable",
        InvalidInput => "invalid input",
        OtherIoError => "unknown I/O error",
        FileNotFound => "file not found",
        PermissionDenied => "permission denied",
        ConnectionFailed => "connection failed",
        Closed => "stream is closed",
        ConnectionRefused => "connection refused",
        ConnectionReset => "connection reset",
        ConnectionAborted => "connection aborted",
        NotConnected => "not connected",
        BrokenPipe => "broken pipe",
        PathAlreadyExists => "file already exists",
        PathDoesntExist => "no such file",
        MismatchedFileTypeForOperation => "mismatched file type",
        ResourceUnavailable => "resource unavailable",
        TimedOut => "operation timed out",
        ShortWrite(..) => "short write",
        NoProgress => "no progress",
    };
    IoError {
        kind: kind,
        desc: desc,
        detail: None,
    }
}

/// A mode specifies how a file should be opened or created. These modes are
/// passed to `File::open_mode` and are used to control where the file is
/// positioned when it is initially opened.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FileMode {
    /// Opens a file positioned at the beginning.
    Open,
    /// Opens a file positioned at EOF.
    Append,
    /// Opens a file, truncating it if it already exists.
    Truncate,
}

/// Access permissions with which the file should be opened. `File`s
/// opened with `Read` will return an error if written to.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FileAccess {
    /// Read-only access, requests to write will result in an error
    Read,
    /// Write-only access, requests to read will result in an error
    Write,
    /// Read-write access, no requests are denied by default
    ReadWrite,
}

/// Different kinds of files which can be identified by a call to stat
#[derive(Copy, PartialEq, Debug, Hash, Clone)]
pub enum FileType {
    /// This is a normal file, corresponding to `S_IFREG`
    RegularFile,

    /// This file is a directory, corresponding to `S_IFDIR`
    Directory,

    /// This file is a named pipe, corresponding to `S_IFIFO`
    NamedPipe,

    /// This file is a block device, corresponding to `S_IFBLK`
    BlockSpecial,

    /// This file is a symbolic link to another file, corresponding to `S_IFLNK`
    Symlink,

    /// The type of this file is not recognized as one of the other categories
    Unknown,
}

/// A structure used to describe metadata information about a file. This
/// structure is created through the `stat` method on a `Path`.
///
/// # Examples
///
/// ```no_run
/// # #![feature(old_io, old_path)]
///
/// use std::old_io::fs::PathExtensions;
/// use std::old_path::Path;
///
/// let info = match Path::new("foo.txt").stat() {
///     Ok(stat) => stat,
///     Err(e) => panic!("couldn't read foo.txt: {}", e),
/// };
///
/// println!("byte size: {}", info.size);
/// ```
#[derive(Copy, Hash)]
pub struct FileStat {
    /// The size of the file, in bytes
    pub size: u64,
    /// The kind of file this path points to (directory, file, pipe, etc.)
    pub kind: FileType,
    /// The file permissions currently on the file
    pub perm: FilePermission,

    // FIXME(#10301): These time fields are pretty useless without an actual
    //                time representation, what are the milliseconds relative
    //                to?

    /// The time that the file was created at, in platform-dependent
    /// milliseconds
    pub created: u64,
    /// The time that this file was last modified, in platform-dependent
    /// milliseconds
    pub modified: u64,
    /// The time that this file was last accessed, in platform-dependent
    /// milliseconds
    pub accessed: u64,

    /// Information returned by stat() which is not guaranteed to be
    /// platform-independent. This information may be useful on some platforms,
    /// but it may have different meanings or no meaning at all on other
    /// platforms.
    ///
    /// Usage of this field is discouraged, but if access is desired then the
    /// fields are located here.
    #[unstable(feature = "io")]
    pub unstable: UnstableFileStat,
}

/// This structure represents all of the possible information which can be
/// returned from a `stat` syscall which is not contained in the `FileStat`
/// structure. This information is not necessarily platform independent, and may
/// have different meanings or no meaning at all on some platforms.
#[unstable(feature = "io")]
#[derive(Copy, Hash)]
pub struct UnstableFileStat {
    /// The ID of the device containing the file.
    pub device: u64,
    /// The file serial number.
    pub inode: u64,
    /// The device ID.
    pub rdev: u64,
    /// The number of hard links to this file.
    pub nlink: u64,
    /// The user ID of the file.
    pub uid: u64,
    /// The group ID of the file.
    pub gid: u64,
    /// The optimal block size for I/O.
    pub blksize: u64,
    /// The blocks allocated for this file.
    pub blocks: u64,
    /// User-defined flags for the file.
    pub flags: u64,
    /// The file generation number.
    pub gen: u64,
}


bitflags! {
    /// A set of permissions for a file or directory is represented by a set of
    /// flags which are or'd together.
    #[derive(Debug)]
    flags FilePermission: u32 {
        const USER_READ     = 0o400,
        const USER_WRITE    = 0o200,
        const USER_EXECUTE  = 0o100,
        const GROUP_READ    = 0o040,
        const GROUP_WRITE   = 0o020,
        const GROUP_EXECUTE = 0o010,
        const OTHER_READ    = 0o004,
        const OTHER_WRITE   = 0o002,
        const OTHER_EXECUTE = 0o001,

        const USER_RWX  = USER_READ.bits | USER_WRITE.bits | USER_EXECUTE.bits,
        const GROUP_RWX = GROUP_READ.bits | GROUP_WRITE.bits | GROUP_EXECUTE.bits,
        const OTHER_RWX = OTHER_READ.bits | OTHER_WRITE.bits | OTHER_EXECUTE.bits,

        /// Permissions for user owned files, equivalent to 0644 on unix-like
        /// systems.
        const USER_FILE = USER_READ.bits | USER_WRITE.bits | GROUP_READ.bits | OTHER_READ.bits,

        /// Permissions for user owned directories, equivalent to 0755 on
        /// unix-like systems.
        const USER_DIR  = USER_RWX.bits | GROUP_READ.bits | GROUP_EXECUTE.bits |
                   OTHER_READ.bits | OTHER_EXECUTE.bits,

        /// Permissions for user owned executables, equivalent to 0755
        /// on unix-like systems.
        const USER_EXEC = USER_DIR.bits,

        /// All possible permissions enabled.
        const ALL_PERMISSIONS = USER_RWX.bits | GROUP_RWX.bits | OTHER_RWX.bits,
    }
}


#[stable(feature = "rust1", since = "1.0.0")]
impl Default for FilePermission {
    #[stable(feature = "rust1", since = "1.0.0")]
    #[inline]
    fn default() -> FilePermission { FilePermission::empty() }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl fmt::Display for FilePermission {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:04o}", self.bits)
    }
}

#[cfg(test)]
mod tests {
    use self::BadReaderBehavior::*;
    use super::{IoResult, Reader, MemReader, NoProgress, InvalidInput, Writer};
    use super::Buffer;
    use prelude::v1::{Ok, Vec};
    use usize;

    #[derive(Clone, PartialEq, Debug)]
    enum BadReaderBehavior {
        GoodBehavior(usize),
        BadBehavior(usize)
    }

    struct BadReader<T> {
        r: T,
        behavior: Vec<BadReaderBehavior>,
    }

    impl<T: Reader> BadReader<T> {
        fn new(r: T, behavior: Vec<BadReaderBehavior>) -> BadReader<T> {
            BadReader { behavior: behavior, r: r }
        }
    }

    impl<T: Reader> Reader for BadReader<T> {
        fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
            let BadReader { ref mut behavior, ref mut r } = *self;
            loop {
                if behavior.is_empty() {
                    // fall back on good
                    return r.read(buf);
                }
                match (&mut **behavior)[0] {
                    GoodBehavior(0) => (),
                    GoodBehavior(ref mut x) => {
                        *x -= 1;
                        return r.read(buf);
                    }
                    BadBehavior(0) => (),
                    BadBehavior(ref mut x) => {
                        *x -= 1;
                        return Ok(0);
                    }
                };
                behavior.remove(0);
            }
        }
    }

    #[test]
    fn test_read_at_least() {
        let mut r = BadReader::new(MemReader::new(b"hello, world!".to_vec()),
                                   vec![GoodBehavior(usize::MAX)]);
        let buf = &mut [0; 5];
        assert!(r.read_at_least(1, buf).unwrap() >= 1);
        assert!(r.read_exact(5).unwrap().len() == 5); // read_exact uses read_at_least
        assert!(r.read_at_least(0, buf).is_ok());

        let mut r = BadReader::new(MemReader::new(b"hello, world!".to_vec()),
                                   vec![BadBehavior(50), GoodBehavior(usize::MAX)]);
        assert!(r.read_at_least(1, buf).unwrap() >= 1);

        let mut r = BadReader::new(MemReader::new(b"hello, world!".to_vec()),
                                   vec![BadBehavior(1), GoodBehavior(1),
                                        BadBehavior(50), GoodBehavior(usize::MAX)]);
        assert!(r.read_at_least(1, buf).unwrap() >= 1);
        assert!(r.read_at_least(1, buf).unwrap() >= 1);

        let mut r = BadReader::new(MemReader::new(b"hello, world!".to_vec()),
                                   vec![BadBehavior(usize::MAX)]);
        assert_eq!(r.read_at_least(1, buf).unwrap_err().kind, NoProgress);

        let mut r = MemReader::new(b"hello, world!".to_vec());
        assert_eq!(r.read_at_least(5, buf).unwrap(), 5);
        assert_eq!(r.read_at_least(6, buf).unwrap_err().kind, InvalidInput);
    }

    #[test]
    fn test_push_at_least() {
        let mut r = BadReader::new(MemReader::new(b"hello, world!".to_vec()),
                                   vec![GoodBehavior(usize::MAX)]);
        let mut buf = Vec::new();
        assert!(r.push_at_least(1, 5, &mut buf).unwrap() >= 1);
        assert!(r.push_at_least(0, 5, &mut buf).is_ok());

        let mut r = BadReader::new(MemReader::new(b"hello, world!".to_vec()),
                                   vec![BadBehavior(50), GoodBehavior(usize::MAX)]);
        assert!(r.push_at_least(1, 5, &mut buf).unwrap() >= 1);

        let mut r = BadReader::new(MemReader::new(b"hello, world!".to_vec()),
                                   vec![BadBehavior(1), GoodBehavior(1),
                                        BadBehavior(50), GoodBehavior(usize::MAX)]);
        assert!(r.push_at_least(1, 5, &mut buf).unwrap() >= 1);
        assert!(r.push_at_least(1, 5, &mut buf).unwrap() >= 1);

        let mut r = BadReader::new(MemReader::new(b"hello, world!".to_vec()),
                                   vec![BadBehavior(usize::MAX)]);
        assert_eq!(r.push_at_least(1, 5, &mut buf).unwrap_err().kind, NoProgress);

        let mut r = MemReader::new(b"hello, world!".to_vec());
        assert_eq!(r.push_at_least(5, 1, &mut buf).unwrap_err().kind, InvalidInput);
    }

    #[test]
    fn test_show() {
        use super::*;

        assert_eq!(format!("{}", USER_READ), "0400");
        assert_eq!(format!("{}", USER_FILE), "0644");
        assert_eq!(format!("{}", USER_EXEC), "0755");
        assert_eq!(format!("{}", USER_RWX),  "0700");
        assert_eq!(format!("{}", GROUP_RWX), "0070");
        assert_eq!(format!("{}", OTHER_RWX), "0007");
        assert_eq!(format!("{}", ALL_PERMISSIONS), "0777");
        assert_eq!(format!("{}", USER_READ | USER_WRITE | OTHER_WRITE), "0602");
    }

    fn _ensure_buffer_is_object_safe<T: Buffer>(x: &T) -> &Buffer {
        x as &Buffer
    }
}
