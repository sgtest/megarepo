// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Buffering wrappers for I/O traits

use container::Container;
use io::{Reader, Writer, Stream, Buffer, DEFAULT_BUF_SIZE, IoResult};
use iter::ExactSize;
use num;
use option::{Some, None};
use result::{Ok, Err};
use vec::{OwnedVector, ImmutableVector, MutableVector};
use vec;

/// Wraps a Reader and buffers input from it
///
/// It can be excessively inefficient to work directly with a `Reader` or
/// `Writer`. Every call to `read` or `write` on `TcpStream` results in a
/// system call, for example. This module provides structures that wrap
/// `Readers`, `Writers`, and `Streams` and buffer input and output to them.
///
/// # Example
///
/// ```rust
/// use std::io::{BufferedReader, File};
///
/// let file = File::open(&Path::new("message.txt"));
/// let mut reader = BufferedReader::new(file);
///
/// let mut buf = [0, ..100];
/// match reader.read(buf) {
///     Ok(nread) => println!("Read {} bytes", nread),
///     Err(e) => println!("error reading: {}", e)
/// }
/// ```
pub struct BufferedReader<R> {
    priv inner: R,
    priv buf: ~[u8],
    priv pos: uint,
    priv cap: uint,
    priv eof: bool,
}

impl<R: Reader> BufferedReader<R> {
    /// Creates a new `BufferedReader` with with the specified buffer capacity
    pub fn with_capacity(cap: uint, inner: R) -> BufferedReader<R> {
        // It's *much* faster to create an uninitialized buffer than it is to
        // fill everything in with 0. This buffer is entirely an implementation
        // detail and is never exposed, so we're safe to not initialize
        // everything up-front. This allows creation of BufferedReader instances
        // to be very cheap (large mallocs are not nearly as expensive as large
        // callocs).
        let mut buf = vec::with_capacity(cap);
        unsafe { buf.set_len(cap); }
        BufferedReader {
            inner: inner,
            buf: buf,
            pos: 0,
            cap: 0,
            eof: false,
        }
    }

    /// Creates a new `BufferedReader` with a default buffer capacity
    pub fn new(inner: R) -> BufferedReader<R> {
        BufferedReader::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    /// Gets a reference to the underlying reader.
    ///
    /// This type does not expose the ability to get a mutable reference to the
    /// underlying reader because that could possibly corrupt the buffer.
    pub fn get_ref<'a>(&'a self) -> &'a R { &self.inner }

    /// Unwraps this buffer, returning the underlying reader.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    pub fn unwrap(self) -> R { self.inner }
}

impl<R: Reader> Buffer for BufferedReader<R> {
    fn fill<'a>(&'a mut self) -> IoResult<&'a [u8]> {
        if self.pos == self.cap {
            self.cap = if_ok!(self.inner.read(self.buf));
            self.pos = 0;
        }
        Ok(self.buf.slice(self.pos, self.cap))
    }

    fn consume(&mut self, amt: uint) {
        self.pos += amt;
        assert!(self.pos <= self.cap);
    }
}

impl<R: Reader> Reader for BufferedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        let nread = {
            let available = if_ok!(self.fill());
            let nread = num::min(available.len(), buf.len());
            vec::bytes::copy_memory(buf, available.slice_to(nread));
            nread
        };
        self.pos += nread;
        Ok(nread)
    }
}

/// Wraps a Writer and buffers output to it
///
/// Note that `BufferedWriter` will NOT flush its buffer when dropped.
///
/// # Example
///
/// ```rust
/// # #[allow(unused_must_use)];
/// use std::io::{BufferedWriter, File};
///
/// let file = File::open(&Path::new("message.txt"));
/// let mut writer = BufferedWriter::new(file);
///
/// writer.write_str("hello, world");
/// writer.flush();
/// ```
pub struct BufferedWriter<W> {
    priv inner: W,
    priv buf: ~[u8],
    priv pos: uint
}

impl<W: Writer> BufferedWriter<W> {
    /// Creates a new `BufferedWriter` with with the specified buffer capacity
    pub fn with_capacity(cap: uint, inner: W) -> BufferedWriter<W> {
        // See comments in BufferedReader for why this uses unsafe code.
        let mut buf = vec::with_capacity(cap);
        unsafe { buf.set_len(cap); }
        BufferedWriter {
            inner: inner,
            buf: buf,
            pos: 0
        }
    }

    /// Creates a new `BufferedWriter` with a default buffer capacity
    pub fn new(inner: W) -> BufferedWriter<W> {
        BufferedWriter::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    fn flush_buf(&mut self) -> IoResult<()> {
        if self.pos != 0 {
            let ret = self.inner.write(self.buf.slice_to(self.pos));
            self.pos = 0;
            ret
        } else {
            Ok(())
        }
    }

    /// Gets a reference to the underlying writer.
    ///
    /// This type does not expose the ability to get a mutable reference to the
    /// underlying reader because that could possibly corrupt the buffer.
    pub fn get_ref<'a>(&'a self) -> &'a W { &self.inner }

    /// Unwraps this buffer, returning the underlying writer.
    ///
    /// The buffer is flushed before returning the writer.
    pub fn unwrap(mut self) -> W {
        // FIXME: is failing the right thing to do if flushing fails?
        self.flush_buf().unwrap();
        self.inner
    }
}

impl<W: Writer> Writer for BufferedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        if self.pos + buf.len() > self.buf.len() {
            if_ok!(self.flush_buf());
        }

        if buf.len() > self.buf.len() {
            self.inner.write(buf)
        } else {
            let dst = self.buf.mut_slice_from(self.pos);
            vec::bytes::copy_memory(dst, buf);
            self.pos += buf.len();
            Ok(())
        }
    }

    fn flush(&mut self) -> IoResult<()> {
        self.flush_buf().and_then(|()| self.inner.flush())
    }
}

/// Wraps a Writer and buffers output to it, flushing whenever a newline (`0x0a`,
/// `'\n'`) is detected.
///
/// Note that this structure does NOT flush the output when dropped.
pub struct LineBufferedWriter<W> {
    priv inner: BufferedWriter<W>,
}

impl<W: Writer> LineBufferedWriter<W> {
    /// Creates a new `LineBufferedWriter`
    pub fn new(inner: W) -> LineBufferedWriter<W> {
        // Lines typically aren't that long, don't use a giant buffer
        LineBufferedWriter {
            inner: BufferedWriter::with_capacity(1024, inner)
        }
    }

    /// Gets a reference to the underlying writer.
    ///
    /// This type does not expose the ability to get a mutable reference to the
    /// underlying reader because that could possibly corrupt the buffer.
    pub fn get_ref<'a>(&'a self) -> &'a W { self.inner.get_ref() }

    /// Unwraps this buffer, returning the underlying writer.
    ///
    /// The internal buffer is flushed before returning the writer.
    pub fn unwrap(self) -> W { self.inner.unwrap() }
}

impl<W: Writer> Writer for LineBufferedWriter<W> {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        match buf.iter().rposition(|&b| b == '\n' as u8) {
            Some(i) => {
                if_ok!(self.inner.write(buf.slice_to(i + 1)));
                if_ok!(self.inner.flush());
                if_ok!(self.inner.write(buf.slice_from(i + 1)));
                Ok(())
            }
            None => self.inner.write(buf),
        }
    }

    fn flush(&mut self) -> IoResult<()> { self.inner.flush() }
}

struct InternalBufferedWriter<W>(BufferedWriter<W>);

impl<W> InternalBufferedWriter<W> {
    fn get_mut_ref<'a>(&'a mut self) -> &'a mut BufferedWriter<W> {
        let InternalBufferedWriter(ref mut w) = *self;
        return w;
    }
}

impl<W: Reader> Reader for InternalBufferedWriter<W> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        self.get_mut_ref().inner.read(buf)
    }
}

/// Wraps a Stream and buffers input and output to and from it
///
/// Note that `BufferedStream` will NOT flush its output buffer when dropped.
///
/// # Example
///
/// ```rust
/// # #[allow(unused_must_use)];
/// use std::io::{BufferedStream, File};
///
/// let file = File::open(&Path::new("message.txt"));
/// let mut stream = BufferedStream::new(file);
///
/// stream.write("hello, world".as_bytes());
/// stream.flush();
///
/// let mut buf = [0, ..100];
/// match stream.read(buf) {
///     Ok(nread) => println!("Read {} bytes", nread),
///     Err(e) => println!("error reading: {}", e)
/// }
/// ```
pub struct BufferedStream<S> {
    priv inner: BufferedReader<InternalBufferedWriter<S>>
}

impl<S: Stream> BufferedStream<S> {
    /// Creates a new buffered stream with explicitly listed capacities for the
    /// reader/writer buffer.
    pub fn with_capacities(reader_cap: uint, writer_cap: uint, inner: S)
                           -> BufferedStream<S> {
        let writer = BufferedWriter::with_capacity(writer_cap, inner);
        let internal_writer = InternalBufferedWriter(writer);
        let reader = BufferedReader::with_capacity(reader_cap,
                                                   internal_writer);
        BufferedStream { inner: reader }
    }

    /// Creates a new buffered stream with the default reader/writer buffer
    /// capacities.
    pub fn new(inner: S) -> BufferedStream<S> {
        BufferedStream::with_capacities(DEFAULT_BUF_SIZE, DEFAULT_BUF_SIZE,
                                        inner)
    }

    /// Gets a reference to the underlying stream.
    ///
    /// This type does not expose the ability to get a mutable reference to the
    /// underlying reader because that could possibly corrupt the buffer.
    pub fn get_ref<'a>(&'a self) -> &'a S {
        let InternalBufferedWriter(ref w) = self.inner.inner;
        w.get_ref()
    }

    /// Unwraps this buffer, returning the underlying stream.
    ///
    /// The internal buffer is flushed before returning the stream. Any leftover
    /// data in the read buffer is lost.
    pub fn unwrap(self) -> S {
        let InternalBufferedWriter(w) = self.inner.inner;
        w.unwrap()
    }
}

impl<S: Stream> Buffer for BufferedStream<S> {
    fn fill<'a>(&'a mut self) -> IoResult<&'a [u8]> { self.inner.fill() }
    fn consume(&mut self, amt: uint) { self.inner.consume(amt) }
}

impl<S: Stream> Reader for BufferedStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        self.inner.read(buf)
    }
}

impl<S: Stream> Writer for BufferedStream<S> {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        self.inner.inner.get_mut_ref().write(buf)
    }
    fn flush(&mut self) -> IoResult<()> {
        self.inner.inner.get_mut_ref().flush()
    }
}

#[cfg(test)]
mod test {
    use io;
    use prelude::*;
    use super::*;
    use super::super::mem::{MemReader, MemWriter, BufReader};
    use Harness = extra::test::BenchHarness;

    /// A type, free to create, primarily intended for benchmarking creation of
    /// wrappers that, just for construction, don't need a Reader/Writer that
    /// does anything useful. Is equivalent to `/dev/null` in semantics.
    #[deriving(Clone,Eq,Ord)]
    pub struct NullStream;

    impl Reader for NullStream {
        fn read(&mut self, _: &mut [u8]) -> io::IoResult<uint> {
            Err(io::standard_error(io::EndOfFile))
        }
    }

    impl Writer for NullStream {
        fn write(&mut self, _: &[u8]) -> io::IoResult<()> { Ok(()) }
    }

    /// A dummy reader intended at testing short-reads propagation.
    pub struct ShortReader {
        priv lengths: ~[uint],
    }

    impl Reader for ShortReader {
        fn read(&mut self, _: &mut [u8]) -> io::IoResult<uint> {
            match self.lengths.shift() {
                Some(i) => Ok(i),
                None => Err(io::standard_error(io::EndOfFile))
            }
        }
    }

    #[test]
    fn test_buffered_reader() {
        let inner = MemReader::new(~[0, 1, 2, 3, 4]);
        let mut reader = BufferedReader::with_capacity(2, inner);

        let mut buf = [0, 0, 0];
        let nread = reader.read(buf);
        assert_eq!(Ok(2), nread);
        assert_eq!([0, 1, 0], buf);

        let mut buf = [0];
        let nread = reader.read(buf);
        assert_eq!(Ok(1), nread);
        assert_eq!([2], buf);

        let mut buf = [0, 0, 0];
        let nread = reader.read(buf);
        assert_eq!(Ok(1), nread);
        assert_eq!([3, 0, 0], buf);

        let nread = reader.read(buf);
        assert_eq!(Ok(1), nread);
        assert_eq!([4, 0, 0], buf);

        assert!(reader.read(buf).is_err());
    }

    #[test]
    fn test_buffered_writer() {
        let inner = MemWriter::new();
        let mut writer = BufferedWriter::with_capacity(2, inner);

        writer.write([0, 1]).unwrap();
        assert_eq!([], writer.get_ref().get_ref());

        writer.write([2]).unwrap();
        assert_eq!([0, 1], writer.get_ref().get_ref());

        writer.write([3]).unwrap();
        assert_eq!([0, 1], writer.get_ref().get_ref());

        writer.flush().unwrap();
        assert_eq!([0, 1, 2, 3], writer.get_ref().get_ref());

        writer.write([4]).unwrap();
        writer.write([5]).unwrap();
        assert_eq!([0, 1, 2, 3], writer.get_ref().get_ref());

        writer.write([6]).unwrap();
        assert_eq!([0, 1, 2, 3, 4, 5],
                   writer.get_ref().get_ref());

        writer.write([7, 8]).unwrap();
        assert_eq!([0, 1, 2, 3, 4, 5, 6],
                   writer.get_ref().get_ref());

        writer.write([9, 10, 11]).unwrap();
        assert_eq!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                   writer.get_ref().get_ref());

        writer.flush().unwrap();
        assert_eq!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
                   writer.get_ref().get_ref());
    }

    #[test]
    fn test_buffered_writer_inner_flushes() {
        let mut w = BufferedWriter::with_capacity(3, MemWriter::new());
        w.write([0, 1]).unwrap();
        assert_eq!([], w.get_ref().get_ref());
        let w = w.unwrap();
        assert_eq!([0, 1], w.get_ref());
    }

    // This is just here to make sure that we don't infinite loop in the
    // newtype struct autoderef weirdness
    #[test]
    fn test_buffered_stream() {
        struct S;

        impl io::Writer for S {
            fn write(&mut self, _: &[u8]) -> io::IoResult<()> { Ok(()) }
        }

        impl io::Reader for S {
            fn read(&mut self, _: &mut [u8]) -> io::IoResult<uint> {
                Err(io::standard_error(io::EndOfFile))
            }
        }

        let mut stream = BufferedStream::new(S);
        let mut buf = [];
        assert!(stream.read(buf).is_err());
        stream.write(buf).unwrap();
        stream.flush().unwrap();
    }

    #[test]
    fn test_read_until() {
        let inner = MemReader::new(~[0, 1, 2, 1, 0]);
        let mut reader = BufferedReader::with_capacity(2, inner);
        assert_eq!(reader.read_until(0), Ok(~[0]));
        assert_eq!(reader.read_until(2), Ok(~[1, 2]));
        assert_eq!(reader.read_until(1), Ok(~[1]));
        assert_eq!(reader.read_until(8), Ok(~[0]));
        assert!(reader.read_until(9).is_err());
    }

    #[test]
    fn test_line_buffer() {
        let mut writer = LineBufferedWriter::new(MemWriter::new());
        writer.write([0]).unwrap();
        assert_eq!(writer.get_ref().get_ref(), []);
        writer.write([1]).unwrap();
        assert_eq!(writer.get_ref().get_ref(), []);
        writer.flush().unwrap();
        assert_eq!(writer.get_ref().get_ref(), [0, 1]);
        writer.write([0, '\n' as u8, 1, '\n' as u8, 2]).unwrap();
        assert_eq!(writer.get_ref().get_ref(),
            [0, 1, 0, '\n' as u8, 1, '\n' as u8]);
        writer.flush().unwrap();
        assert_eq!(writer.get_ref().get_ref(),
            [0, 1, 0, '\n' as u8, 1, '\n' as u8, 2]);
        writer.write([3, '\n' as u8]).unwrap();
        assert_eq!(writer.get_ref().get_ref(),
            [0, 1, 0, '\n' as u8, 1, '\n' as u8, 2, 3, '\n' as u8]);
    }

    #[test]
    fn test_read_line() {
        let in_buf = MemReader::new(bytes!("a\nb\nc").to_owned());
        let mut reader = BufferedReader::with_capacity(2, in_buf);
        assert_eq!(reader.read_line(), Ok(~"a\n"));
        assert_eq!(reader.read_line(), Ok(~"b\n"));
        assert_eq!(reader.read_line(), Ok(~"c"));
        assert!(reader.read_line().is_err());
    }

    #[test]
    fn test_lines() {
        let in_buf = MemReader::new(bytes!("a\nb\nc").to_owned());
        let mut reader = BufferedReader::with_capacity(2, in_buf);
        let mut it = reader.lines();
        assert_eq!(it.next(), Some(~"a\n"));
        assert_eq!(it.next(), Some(~"b\n"));
        assert_eq!(it.next(), Some(~"c"));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn test_short_reads() {
        let inner = ShortReader{lengths: ~[0, 1, 2, 0, 1, 0]};
        let mut reader = BufferedReader::new(inner);
        let mut buf = [0, 0];
        assert_eq!(reader.read(buf), Ok(0));
        assert_eq!(reader.read(buf), Ok(1));
        assert_eq!(reader.read(buf), Ok(2));
        assert_eq!(reader.read(buf), Ok(0));
        assert_eq!(reader.read(buf), Ok(1));
        assert_eq!(reader.read(buf), Ok(0));
        assert!(reader.read(buf).is_err());
    }

    #[test]
    fn read_char_buffered() {
        let buf = [195u8, 159u8];
        let mut reader = BufferedReader::with_capacity(1, BufReader::new(buf));
        assert_eq!(reader.read_char(), Ok('ß'));
    }

    #[bench]
    fn bench_buffered_reader(bh: &mut Harness) {
        bh.iter(|| {
            BufferedReader::new(NullStream);
        });
    }

    #[bench]
    fn bench_buffered_writer(bh: &mut Harness) {
        bh.iter(|| {
            BufferedWriter::new(NullStream);
        });
    }

    #[bench]
    fn bench_buffered_stream(bh: &mut Harness) {
        bh.iter(|| {
            BufferedStream::new(NullStream);
        });
    }
}
