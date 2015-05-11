// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Traits, helpers, and type definitions for core I/O functionality.

#![stable(feature = "rust1", since = "1.0.0")]

use cmp;
use rustc_unicode::str as core_str;
use error as std_error;
use fmt;
use iter::{self, Iterator, Extend};
use marker::Sized;
use ops::{Drop, FnOnce};
use option::Option::{self, Some, None};
use result::Result::{Ok, Err};
use result;
use string::String;
use str;
use vec::Vec;

pub use self::buffered::{BufReader, BufWriter, BufStream, LineWriter};
pub use self::buffered::IntoInnerError;
pub use self::cursor::Cursor;
pub use self::error::{Result, Error, ErrorKind};
pub use self::util::{copy, sink, Sink, empty, Empty, repeat, Repeat};
pub use self::stdio::{stdin, stdout, stderr, _print, Stdin, Stdout, Stderr};
pub use self::stdio::{StdoutLock, StderrLock, StdinLock};
#[doc(no_inline, hidden)]
pub use self::stdio::{set_panic, set_print};

#[macro_use] mod lazy;

pub mod prelude;
mod buffered;
mod cursor;
mod error;
mod impls;
mod util;
mod stdio;

const DEFAULT_BUF_SIZE: usize = 64 * 1024;

// A few methods below (read_to_string, read_line) will append data into a
// `String` buffer, but we need to be pretty careful when doing this. The
// implementation will just call `.as_mut_vec()` and then delegate to a
// byte-oriented reading method, but we must ensure that when returning we never
// leave `buf` in a state such that it contains invalid UTF-8 in its bounds.
//
// To this end, we use an RAII guard (to protect against panics) which updates
// the length of the string when it is dropped. This guard initially truncates
// the string to the prior length and only after we've validated that the
// new contents are valid UTF-8 do we allow it to set a longer length.
//
// The unsafety in this function is twofold:
//
// 1. We're looking at the raw bytes of `buf`, so we take on the burden of UTF-8
//    checks.
// 2. We're passing a raw buffer to the function `f`, and it is expected that
//    the function only *appends* bytes to the buffer. We'll get undefined
//    behavior if existing bytes are overwritten to have non-UTF-8 data.
fn append_to_string<F>(buf: &mut String, f: F) -> Result<usize>
    where F: FnOnce(&mut Vec<u8>) -> Result<usize>
{
    struct Guard<'a> { s: &'a mut Vec<u8>, len: usize }
        impl<'a> Drop for Guard<'a> {
        fn drop(&mut self) {
            unsafe { self.s.set_len(self.len); }
        }
    }

    unsafe {
        let mut g = Guard { len: buf.len(), s: buf.as_mut_vec() };
        let ret = f(g.s);
        if str::from_utf8(&g.s[g.len..]).is_err() {
            ret.and_then(|_| {
                Err(Error::new(ErrorKind::InvalidInput,
                               "stream did not contain valid UTF-8"))
            })
        } else {
            g.len = g.s.len();
            ret
        }
    }
}

// This uses an adaptive system to extend the vector when it fills. We want to
// avoid paying to allocate and zero a huge chunk of memory if the reader only
// has 4 bytes while still making large reads if the reader does have a ton
// of data to return. Simply tacking on an extra DEFAULT_BUF_SIZE space every
// time is 4,500 times (!) slower than this if the reader has a very small
// amount of data to return.
fn read_to_end<R: Read + ?Sized>(r: &mut R, buf: &mut Vec<u8>) -> Result<usize> {
    let start_len = buf.len();
    let mut len = start_len;
    let mut new_write_size = 16;
    let ret;
    loop {
        if len == buf.len() {
            if new_write_size < DEFAULT_BUF_SIZE {
                new_write_size *= 2;
            }
            buf.extend(iter::repeat(0).take(new_write_size));
        }

        match r.read(&mut buf[len..]) {
            Ok(0) => {
                ret = Ok(len - start_len);
                break;
            }
            Ok(n) => len += n,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => {
                ret = Err(e);
                break;
            }
        }
    }

    buf.truncate(len);
    ret
}

/// A trait for objects which are byte-oriented sources.
///
/// Readers are defined by one method, `read`. Each call to `read` will attempt
/// to pull bytes from this source into a provided buffer.
///
/// Readers are intended to be composable with one another. Many objects
/// throughout the I/O and related libraries take and provide types which
/// implement the `Read` trait.
#[stable(feature = "rust1", since = "1.0.0")]
pub trait Read {
    /// Pull some bytes from this source into the specified buffer, returning
    /// how many bytes were read.
    ///
    /// This function does not provide any guarantees about whether it blocks
    /// waiting for data, but if an object needs to block for a read but cannot
    /// it will typically signal this via an `Err` return value.
    ///
    /// If the return value of this method is `Ok(n)`, then it must be
    /// guaranteed that `0 <= n <= buf.len()`. A nonzero `n` value indicates
    /// that the buffer `buf` has been filled in with `n` bytes of data from this
    /// source. If `n` is `0`, then it can indicate one of two scenarios:
    ///
    /// 1. This reader has reached its "end of file" and will likely no longer
    ///    be able to produce bytes. Note that this does not mean that the
    ///    reader will *always* no longer be able to produce bytes.
    /// 2. The buffer specified was 0 bytes in length.
    ///
    /// No guarantees are provided about the contents of `buf` when this
    /// function is called, implementations cannot rely on any property of the
    /// contents of `buf` being true. It is recommended that implementations
    /// only write data to `buf` instead of reading its contents.
    ///
    /// # Errors
    ///
    /// If this function encounters any form of I/O or other error, an error
    /// variant will be returned. If an error is returned then it must be
    /// guaranteed that no bytes were read.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Read all bytes until EOF in this source, placing them into `buf`.
    ///
    /// All bytes read from this source will be appended to the specified buffer
    /// `buf`. This function will continuously call `read` to append more data to
    /// `buf` until `read` returns either `Ok(0)` or an error of
    /// non-`ErrorKind::Interrupted` kind.
    ///
    /// If successful, this function will return the total number of bytes read.
    ///
    /// # Errors
    ///
    /// If this function encounters an error of the kind
    /// `ErrorKind::Interrupted` then the error is ignored and the operation
    /// will continue.
    ///
    /// If any other read error is encountered then this function immediately
    /// returns. Any bytes which have already been read will be appended to
    /// `buf`.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> Result<usize> {
        read_to_end(self, buf)
    }

    /// Read all bytes until EOF in this source, placing them into `buf`.
    ///
    /// If successful, this function returns the number of bytes which were read
    /// and appended to `buf`.
    ///
    /// # Errors
    ///
    /// If the data in this stream is *not* valid UTF-8 then an error is
    /// returned and `buf` is unchanged.
    ///
    /// See `read_to_end` for other error semantics.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn read_to_string(&mut self, buf: &mut String) -> Result<usize> {
        // Note that we do *not* call `.read_to_end()` here. We are passing
        // `&mut Vec<u8>` (the raw contents of `buf`) into the `read_to_end`
        // method to fill it up. An arbitrary implementation could overwrite the
        // entire contents of the vector, not just append to it (which is what
        // we are expecting).
        //
        // To prevent extraneously checking the UTF-8-ness of the entire buffer
        // we pass it to our hardcoded `read_to_end` implementation which we
        // know is guaranteed to only read data into the end of the buffer.
        append_to_string(buf, |b| read_to_end(self, b))
    }

    /// Creates a "by reference" adaptor for this instance of `Read`.
    ///
    /// The returned adaptor also implements `Read` and will simply borrow this
    /// current reader.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn by_ref(&mut self) -> &mut Self where Self: Sized { self }

    /// Transforms this `Read` instance to an `Iterator` over its bytes.
    ///
    /// The returned type implements `Iterator` where the `Item` is `Result<u8,
    /// R::Err>`.  The yielded item is `Ok` if a byte was successfully read and
    /// `Err` otherwise for I/O errors. EOF is mapped to returning `None` from
    /// this iterator.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn bytes(self) -> Bytes<Self> where Self: Sized {
        Bytes { inner: self }
    }

    /// Transforms this `Read` instance to an `Iterator` over `char`s.
    ///
    /// This adaptor will attempt to interpret this reader as a UTF-8 encoded
    /// sequence of characters. The returned iterator will return `None` once
    /// EOF is reached for this reader. Otherwise each element yielded will be a
    /// `Result<char, E>` where `E` may contain information about what I/O error
    /// occurred or where decoding failed.
    ///
    /// Currently this adaptor will discard intermediate data read, and should
    /// be avoided if this is not desired.
    #[unstable(feature = "io", reason = "the semantics of a partial read/write \
                                         of where errors happen is currently \
                                         unclear and may change")]
    fn chars(self) -> Chars<Self> where Self: Sized {
        Chars { inner: self }
    }

    /// Creates an adaptor which will chain this stream with another.
    ///
    /// The returned `Read` instance will first read all bytes from this object
    /// until EOF is encountered. Afterwards the output is equivalent to the
    /// output of `next`.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn chain<R: Read>(self, next: R) -> Chain<Self, R> where Self: Sized {
        Chain { first: self, second: next, done_first: false }
    }

    /// Creates an adaptor which will read at most `limit` bytes from it.
    ///
    /// This function returns a new instance of `Read` which will read at most
    /// `limit` bytes, after which it will always return EOF (`Ok(0)`). Any
    /// read errors will not count towards the number of bytes read and future
    /// calls to `read` may succeed.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn take(self, limit: u64) -> Take<Self> where Self: Sized {
        Take { inner: self, limit: limit }
    }

    /// Creates a reader adaptor which will write all read data into the given
    /// output stream.
    ///
    /// Whenever the returned `Read` instance is read it will write the read
    /// data to `out`. The current semantics of this implementation imply that
    /// a `write` error will not report how much data was initially read.
    #[unstable(feature = "io", reason = "the semantics of a partial read/write \
                                         of where errors happen is currently \
                                         unclear and may change")]
    fn tee<W: Write>(self, out: W) -> Tee<Self, W> where Self: Sized {
        Tee { reader: self, writer: out }
    }
}

/// A trait for objects which are byte-oriented sinks.
///
/// The `write` method will attempt to write some data into the object,
/// returning how many bytes were successfully written.
///
/// The `flush` method is useful for adaptors and explicit buffers themselves
/// for ensuring that all buffered data has been pushed out to the "true sink".
///
/// Writers are intended to be composable with one another. Many objects
/// throughout the I/O and related libraries take and provide types which
/// implement the `Write` trait.
#[stable(feature = "rust1", since = "1.0.0")]
pub trait Write {
    /// Write a buffer into this object, returning how many bytes were written.
    ///
    /// This function will attempt to write the entire contents of `buf`, but
    /// the entire write may not succeed, or the write may also generate an
    /// error. A call to `write` represents *at most one* attempt to write to
    /// any wrapped object.
    ///
    /// Calls to `write` are not guaranteed to block waiting for data to be
    /// written, and a write which would otherwise block can indicated through
    /// an `Err` variant.
    ///
    /// If the return value is `Ok(n)` then it must be guaranteed that
    /// `0 <= n <= buf.len()`. A return value of `0` typically means that the
    /// underlying object is no longer able to accept bytes and will likely not
    /// be able to in the future as well, or that the buffer provided is empty.
    ///
    /// # Errors
    ///
    /// Each call to `write` may generate an I/O error indicating that the
    /// operation could not be completed. If an error is returned then no bytes
    /// in the buffer were written to this writer.
    ///
    /// It is **not** considered an error if the entire buffer could not be
    /// written to this writer.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn write(&mut self, buf: &[u8]) -> Result<usize>;

    /// Flush this output stream, ensuring that all intermediately buffered
    /// contents reach their destination.
    ///
    /// # Errors
    ///
    /// It is considered an error if not all bytes could be written due to
    /// I/O errors or EOF being reached.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn flush(&mut self) -> Result<()>;

    /// Attempts to write an entire buffer into this write.
    ///
    /// This method will continuously call `write` while there is more data to
    /// write. This method will not return until the entire buffer has been
    /// successfully written or an error occurs. The first error generated from
    /// this method will be returned.
    ///
    /// # Errors
    ///
    /// This function will return the first error that `write` returns.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => return Err(Error::new(ErrorKind::WriteZero,
                                               "failed to write whole buffer")),
                Ok(n) => buf = &buf[n..],
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Writes a formatted string into this writer, returning any error
    /// encountered.
    ///
    /// This method is primarily used to interface with the `format_args!`
    /// macro, but it is rare that this should explicitly be called. The
    /// `write!` macro should be favored to invoke this method instead.
    ///
    /// This function internally uses the `write_all` method on this trait and
    /// hence will continuously write data so long as no errors are received.
    /// This also means that partial writes are not indicated in this signature.
    ///
    /// # Errors
    ///
    /// This function will return any I/O error reported while formatting.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn write_fmt(&mut self, fmt: fmt::Arguments) -> Result<()> {
        // Create a shim which translates a Write to a fmt::Write and saves
        // off I/O errors. instead of discarding them
        struct Adaptor<'a, T: ?Sized + 'a> {
            inner: &'a mut T,
            error: Result<()>,
        }

        impl<'a, T: Write + ?Sized> fmt::Write for Adaptor<'a, T> {
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

    /// Creates a "by reference" adaptor for this instance of `Write`.
    ///
    /// The returned adaptor also implements `Write` and will simply borrow this
    /// current writer.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn by_ref(&mut self) -> &mut Self where Self: Sized { self }

    /// Creates a new writer which will write all data to both this writer and
    /// another writer.
    ///
    /// All data written to the returned writer will both be written to `self`
    /// as well as `other`. Note that the error semantics of the current
    /// implementation do not precisely track where errors happen. For example
    /// an error on the second call to `write` will not report that the first
    /// call to `write` succeeded.
    #[unstable(feature = "io", reason = "the semantics of a partial read/write \
                                         of where errors happen is currently \
                                         unclear and may change")]
    fn broadcast<W: Write>(self, other: W) -> Broadcast<Self, W>
        where Self: Sized
    {
        Broadcast { first: self, second: other }
    }
}

/// An object implementing `Seek` internally has some form of cursor which can
/// be moved within a stream of bytes.
///
/// The stream typically has a fixed size, allowing seeking relative to either
/// end or the current offset.
#[stable(feature = "rust1", since = "1.0.0")]
pub trait Seek {
    /// Seek to an offset, in bytes, in a stream
    ///
    /// A seek beyond the end of a stream is allowed, but seeking before offset
    /// 0 is an error.
    ///
    /// The behavior when seeking past the end of the stream is implementation
    /// defined.
    ///
    /// This method returns the new position within the stream if the seek
    /// operation completed successfully.
    ///
    /// # Errors
    ///
    /// Seeking to a negative offset is considered an error
    #[stable(feature = "rust1", since = "1.0.0")]
    fn seek(&mut self, pos: SeekFrom) -> Result<u64>;
}

/// Enumeration of possible methods to seek within an I/O object.
#[derive(Copy, PartialEq, Eq, Clone, Debug)]
#[stable(feature = "rust1", since = "1.0.0")]
pub enum SeekFrom {
    /// Set the offset to the provided number of bytes.
    #[stable(feature = "rust1", since = "1.0.0")]
    Start(u64),

    /// Set the offset to the size of this object plus the specified number of
    /// bytes.
    ///
    /// It is possible to seek beyond the end of an object, but is an error to
    /// seek before byte 0.
    #[stable(feature = "rust1", since = "1.0.0")]
    End(i64),

    /// Set the offset to the current position plus the specified number of
    /// bytes.
    ///
    /// It is possible to seek beyond the end of an object, but is an error to
    /// seek before byte 0.
    #[stable(feature = "rust1", since = "1.0.0")]
    Current(i64),
}

fn read_until<R: BufRead + ?Sized>(r: &mut R, delim: u8, buf: &mut Vec<u8>)
                                   -> Result<usize> {
    let mut read = 0;
    loop {
        let (done, used) = {
            let available = match r.fill_buf() {
                Ok(n) => n,
                Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e)
            };
            match available.position_elem(&delim) {
                Some(i) => {
                    buf.push_all(&available[..i + 1]);
                    (true, i + 1)
                }
                None => {
                    buf.push_all(available);
                    (false, available.len())
                }
            }
        };
        r.consume(used);
        read += used;
        if done || used == 0 {
            return Ok(read);
        }
    }
}

/// A Buffer is a type of reader which has some form of internal buffering to
/// allow certain kinds of reading operations to be more optimized than others.
///
/// This type extends the `Read` trait with a few methods that are not
/// possible to reasonably implement with purely a read interface.
#[stable(feature = "rust1", since = "1.0.0")]
pub trait BufRead: Read {
    /// Fills the internal buffer of this object, returning the buffer contents.
    ///
    /// None of the contents will be "read" in the sense that later calling
    /// `read` may return the same contents.
    ///
    /// The `consume` function must be called with the number of bytes that are
    /// consumed from this buffer returned to ensure that the bytes are never
    /// returned twice.
    ///
    /// An empty buffer returned indicates that the stream has reached EOF.
    ///
    /// # Errors
    ///
    /// This function will return an I/O error if the underlying reader was
    /// read, but returned an error.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn fill_buf(&mut self) -> Result<&[u8]>;

    /// Tells this buffer that `amt` bytes have been consumed from the buffer,
    /// so they should no longer be returned in calls to `read`.
    ///
    /// This function does not perform any I/O, it simply informs this object
    /// that some amount of its buffer, returned from `fill_buf`, has been
    /// consumed and should no longer be returned.
    ///
    /// This function is used to tell the buffer how many bytes you've consumed
    /// from the return value of `fill_buf`, and so may do odd things if
    /// `fill_buf` isn't called before calling this.
    ///
    /// The `amt` must be `<=` the number of bytes in the buffer returned by `fill_buf`.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn consume(&mut self, amt: usize);

    /// Read all bytes until the delimiter `byte` is reached.
    ///
    /// This function will continue to read (and buffer) bytes from the
    /// underlying stream until the delimiter or EOF is found. Once found, all
    /// bytes up to, and including, the delimiter (if found) will be appended to
    /// `buf`.
    ///
    /// If this buffered reader is currently at EOF, then this function will not
    /// place any more bytes into `buf` and will return `Ok(n)` where `n` is the
    /// number of bytes which were read.
    ///
    /// # Errors
    ///
    /// This function will ignore all instances of `ErrorKind::Interrupted` and
    /// will otherwise return any errors returned by `fill_buf`.
    ///
    /// If an I/O error is encountered then all bytes read so far will be
    /// present in `buf` and its length will have been adjusted appropriately.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> Result<usize> {
        read_until(self, byte, buf)
    }

    /// Read all bytes until a newline byte (the 0xA byte) is reached, and
    /// append them to the provided buffer.
    ///
    /// This function will continue to read (and buffer) bytes from the
    /// underlying stream until the newline delimiter (the 0xA byte) or EOF is
    /// found. Once found, all bytes up to, and including, the delimiter (if
    /// found) will be appended to `buf`.
    ///
    /// If this reader is currently at EOF then this function will not modify
    /// `buf` and will return `Ok(n)` where `n` is the number of bytes which
    /// were read.
    ///
    /// # Errors
    ///
    /// This function has the same error semantics as `read_until` and will also
    /// return an error if the read bytes are not valid UTF-8. If an I/O error
    /// is encountered then `buf` may contain some bytes already read in the
    /// event that all data read so far was valid UTF-8.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn read_line(&mut self, buf: &mut String) -> Result<usize> {
        // Note that we are not calling the `.read_until` method here, but
        // rather our hardcoded implementation. For more details as to why, see
        // the comments in `read_to_end`.
        append_to_string(buf, |b| read_until(self, b'\n', b))
    }

    /// Returns an iterator over the contents of this reader split on the byte
    /// `byte`.
    ///
    /// The iterator returned from this function will return instances of
    /// `io::Result<Vec<u8>>`. Each vector returned will *not* have the
    /// delimiter byte at the end.
    ///
    /// This function will yield errors whenever `read_until` would have also
    /// yielded an error.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn split(self, byte: u8) -> Split<Self> where Self: Sized {
        Split { buf: self, delim: byte }
    }

    /// Returns an iterator over the lines of this reader.
    ///
    /// The iterator returned from this function will yield instances of
    /// `io::Result<String>`. Each string returned will *not* have a newline
    /// byte (the 0xA byte) at the end.
    #[stable(feature = "rust1", since = "1.0.0")]
    fn lines(self) -> Lines<Self> where Self: Sized {
        Lines { buf: self }
    }
}

/// A `Write` adaptor which will write data to multiple locations.
///
/// For more information, see `Write::broadcast`.
#[unstable(feature = "io", reason = "awaiting stability of Write::broadcast")]
pub struct Broadcast<T, U> {
    first: T,
    second: U,
}

#[unstable(feature = "io", reason = "awaiting stability of Write::broadcast")]
impl<T: Write, U: Write> Write for Broadcast<T, U> {
    fn write(&mut self, data: &[u8]) -> Result<usize> {
        let n = try!(self.first.write(data));
        // FIXME: what if the write fails? (we wrote something)
        try!(self.second.write_all(&data[..n]));
        Ok(n)
    }

    fn flush(&mut self) -> Result<()> {
        self.first.flush().and(self.second.flush())
    }
}

/// Adaptor to chain together two instances of `Read`.
///
/// For more information, see `Read::chain`.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct Chain<T, U> {
    first: T,
    second: U,
    done_first: bool,
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<T: Read, U: Read> Read for Chain<T, U> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.done_first {
            match try!(self.first.read(buf)) {
                0 => { self.done_first = true; }
                n => return Ok(n),
            }
        }
        self.second.read(buf)
    }
}

/// Reader adaptor which limits the bytes read from an underlying reader.
///
/// For more information, see `Read::take`.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct Take<T> {
    inner: T,
    limit: u64,
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<T> Take<T> {
    /// Returns the number of bytes that can be read before this instance will
    /// return EOF.
    ///
    /// # Note
    ///
    /// This instance may reach EOF after reading fewer bytes than indicated by
    /// this method if the underlying `Read` instance reaches EOF.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn limit(&self) -> u64 { self.limit }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<T: Read> Read for Take<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // Don't call into inner reader at all at EOF because it may still block
        if self.limit == 0 {
            return Ok(0);
        }

        let max = cmp::min(buf.len() as u64, self.limit) as usize;
        let n = try!(self.inner.read(&mut buf[..max]));
        self.limit -= n as u64;
        Ok(n)
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<T: BufRead> BufRead for Take<T> {
    fn fill_buf(&mut self) -> Result<&[u8]> {
        let buf = try!(self.inner.fill_buf());
        let cap = cmp::min(buf.len() as u64, self.limit) as usize;
        Ok(&buf[..cap])
    }

    fn consume(&mut self, amt: usize) {
        // Don't let callers reset the limit by passing an overlarge value
        let amt = cmp::min(amt as u64, self.limit) as usize;
        self.limit -= amt as u64;
        self.inner.consume(amt);
    }
}

/// An adaptor which will emit all read data to a specified writer as well.
///
/// For more information see `Read::tee`
#[unstable(feature = "io", reason = "awaiting stability of Read::tee")]
pub struct Tee<R, W> {
    reader: R,
    writer: W,
}

#[unstable(feature = "io", reason = "awaiting stability of Read::tee")]
impl<R: Read, W: Write> Read for Tee<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = try!(self.reader.read(buf));
        // FIXME: what if the write fails? (we read something)
        try!(self.writer.write_all(&buf[..n]));
        Ok(n)
    }
}

/// A bridge from implementations of `Read` to an `Iterator` of `u8`.
///
/// See `Read::bytes` for more information.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct Bytes<R> {
    inner: R,
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<R: Read> Iterator for Bytes<R> {
    type Item = Result<u8>;

    fn next(&mut self) -> Option<Result<u8>> {
        let mut buf = [0];
        match self.inner.read(&mut buf) {
            Ok(0) => None,
            Ok(..) => Some(Ok(buf[0])),
            Err(e) => Some(Err(e)),
        }
    }
}

/// A bridge from implementations of `Read` to an `Iterator` of `char`.
///
/// See `Read::chars` for more information.
#[unstable(feature = "io", reason = "awaiting stability of Read::chars")]
pub struct Chars<R> {
    inner: R,
}

/// An enumeration of possible errors that can be generated from the `Chars`
/// adapter.
#[derive(Debug)]
#[unstable(feature = "io", reason = "awaiting stability of Read::chars")]
pub enum CharsError {
    /// Variant representing that the underlying stream was read successfully
    /// but it did not contain valid utf8 data.
    NotUtf8,

    /// Variant representing that an I/O error occurred.
    Other(Error),
}

#[unstable(feature = "io", reason = "awaiting stability of Read::chars")]
impl<R: Read> Iterator for Chars<R> {
    type Item = result::Result<char, CharsError>;

    fn next(&mut self) -> Option<result::Result<char, CharsError>> {
        let mut buf = [0];
        let first_byte = match self.inner.read(&mut buf) {
            Ok(0) => return None,
            Ok(..) => buf[0],
            Err(e) => return Some(Err(CharsError::Other(e))),
        };
        let width = core_str::utf8_char_width(first_byte);
        if width == 1 { return Some(Ok(first_byte as char)) }
        if width == 0 { return Some(Err(CharsError::NotUtf8)) }
        let mut buf = [first_byte, 0, 0, 0];
        {
            let mut start = 1;
            while start < width {
                match self.inner.read(&mut buf[start..width]) {
                    Ok(0) => return Some(Err(CharsError::NotUtf8)),
                    Ok(n) => start += n,
                    Err(e) => return Some(Err(CharsError::Other(e))),
                }
            }
        }
        Some(match str::from_utf8(&buf[..width]).ok() {
            Some(s) => Ok(s.char_at(0)),
            None => Err(CharsError::NotUtf8),
        })
    }
}

#[unstable(feature = "io", reason = "awaiting stability of Read::chars")]
impl std_error::Error for CharsError {
    fn description(&self) -> &str {
        match *self {
            CharsError::NotUtf8 => "invalid utf8 encoding",
            CharsError::Other(ref e) => std_error::Error::description(e),
        }
    }
    fn cause(&self) -> Option<&std_error::Error> {
        match *self {
            CharsError::NotUtf8 => None,
            CharsError::Other(ref e) => e.cause(),
        }
    }
}

#[unstable(feature = "io", reason = "awaiting stability of Read::chars")]
impl fmt::Display for CharsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CharsError::NotUtf8 => {
                "byte stream did not contain valid utf8".fmt(f)
            }
            CharsError::Other(ref e) => e.fmt(f),
        }
    }
}

/// An iterator over the contents of an instance of `BufRead` split on a
/// particular byte.
///
/// See `BufRead::split` for more information.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct Split<B> {
    buf: B,
    delim: u8,
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<B: BufRead> Iterator for Split<B> {
    type Item = Result<Vec<u8>>;

    fn next(&mut self) -> Option<Result<Vec<u8>>> {
        let mut buf = Vec::new();
        match self.buf.read_until(self.delim, &mut buf) {
            Ok(0) => None,
            Ok(_n) => {
                if buf[buf.len() - 1] == self.delim {
                    buf.pop();
                }
                Some(Ok(buf))
            }
            Err(e) => Some(Err(e))
        }
    }
}

/// An iterator over the lines of an instance of `BufRead` split on a newline
/// byte.
///
/// See `BufRead::lines` for more information.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct Lines<B> {
    buf: B,
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<B: BufRead> Iterator for Lines<B> {
    type Item = Result<String>;

    fn next(&mut self) -> Option<Result<String>> {
        let mut buf = String::new();
        match self.buf.read_line(&mut buf) {
            Ok(0) => None,
            Ok(_n) => {
                if buf.ends_with("\n") {
                    buf.pop();
                }
                Some(Ok(buf))
            }
            Err(e) => Some(Err(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use prelude::v1::*;
    use io::prelude::*;
    use io;
    use super::Cursor;

    #[test]
    fn read_until() {
        let mut buf = Cursor::new(&b"12"[..]);
        let mut v = Vec::new();
        assert_eq!(buf.read_until(b'3', &mut v).unwrap(), 2);
        assert_eq!(v, b"12");

        let mut buf = Cursor::new(&b"1233"[..]);
        let mut v = Vec::new();
        assert_eq!(buf.read_until(b'3', &mut v).unwrap(), 3);
        assert_eq!(v, b"123");
        v.truncate(0);
        assert_eq!(buf.read_until(b'3', &mut v).unwrap(), 1);
        assert_eq!(v, b"3");
        v.truncate(0);
        assert_eq!(buf.read_until(b'3', &mut v).unwrap(), 0);
        assert_eq!(v, []);
    }

    #[test]
    fn split() {
        let buf = Cursor::new(&b"12"[..]);
        let mut s = buf.split(b'3');
        assert_eq!(s.next().unwrap().unwrap(), vec![b'1', b'2']);
        assert!(s.next().is_none());

        let buf = Cursor::new(&b"1233"[..]);
        let mut s = buf.split(b'3');
        assert_eq!(s.next().unwrap().unwrap(), vec![b'1', b'2']);
        assert_eq!(s.next().unwrap().unwrap(), vec![]);
        assert!(s.next().is_none());
    }

    #[test]
    fn read_line() {
        let mut buf = Cursor::new(&b"12"[..]);
        let mut v = String::new();
        assert_eq!(buf.read_line(&mut v).unwrap(), 2);
        assert_eq!(v, "12");

        let mut buf = Cursor::new(&b"12\n\n"[..]);
        let mut v = String::new();
        assert_eq!(buf.read_line(&mut v).unwrap(), 3);
        assert_eq!(v, "12\n");
        v.truncate(0);
        assert_eq!(buf.read_line(&mut v).unwrap(), 1);
        assert_eq!(v, "\n");
        v.truncate(0);
        assert_eq!(buf.read_line(&mut v).unwrap(), 0);
        assert_eq!(v, "");
    }

    #[test]
    fn lines() {
        let buf = Cursor::new(&b"12"[..]);
        let mut s = buf.lines();
        assert_eq!(s.next().unwrap().unwrap(), "12".to_string());
        assert!(s.next().is_none());

        let buf = Cursor::new(&b"12\n\n"[..]);
        let mut s = buf.lines();
        assert_eq!(s.next().unwrap().unwrap(), "12".to_string());
        assert_eq!(s.next().unwrap().unwrap(), "".to_string());
        assert!(s.next().is_none());
    }

    #[test]
    fn read_to_end() {
        let mut c = Cursor::new(&b""[..]);
        let mut v = Vec::new();
        assert_eq!(c.read_to_end(&mut v).unwrap(), 0);
        assert_eq!(v, []);

        let mut c = Cursor::new(&b"1"[..]);
        let mut v = Vec::new();
        assert_eq!(c.read_to_end(&mut v).unwrap(), 1);
        assert_eq!(v, b"1");
    }

    #[test]
    fn read_to_string() {
        let mut c = Cursor::new(&b""[..]);
        let mut v = String::new();
        assert_eq!(c.read_to_string(&mut v).unwrap(), 0);
        assert_eq!(v, "");

        let mut c = Cursor::new(&b"1"[..]);
        let mut v = String::new();
        assert_eq!(c.read_to_string(&mut v).unwrap(), 1);
        assert_eq!(v, "1");

        let mut c = Cursor::new(&b"\xff"[..]);
        let mut v = String::new();
        assert!(c.read_to_string(&mut v).is_err());
    }

    #[test]
    fn take_eof() {
        struct R;

        impl Read for R {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::Other, ""))
            }
        }

        let mut buf = [0; 1];
        assert_eq!(0, R.take(0).read(&mut buf).unwrap());
    }
}
