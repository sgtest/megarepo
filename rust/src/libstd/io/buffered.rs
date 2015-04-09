// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
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

//! Buffering wrappers for I/O traits

use prelude::v1::*;
use io::prelude::*;

use cmp;
use error;
use fmt;
use io::{self, DEFAULT_BUF_SIZE, Error, ErrorKind, SeekFrom};
use ptr;
use iter;

/// Wraps a `Read` and buffers input from it
///
/// It can be excessively inefficient to work directly with a `Read` instance.
/// For example, every call to `read` on `TcpStream` results in a system call.
/// A `BufReader` performs large, infrequent reads on the underlying `Read`
/// and maintains an in-memory buffer of the results.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct BufReader<R> {
    inner: R,
    buf: Vec<u8>,
    pos: usize,
    cap: usize,
}

impl<R: Read> BufReader<R> {
    /// Creates a new `BufReader` with a default buffer capacity
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn new(inner: R) -> BufReader<R> {
        BufReader::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    /// Creates a new `BufReader` with the specified buffer capacity
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn with_capacity(cap: usize, inner: R) -> BufReader<R> {
        let mut buf = Vec::with_capacity(cap);
        buf.extend(iter::repeat(0).take(cap));
        BufReader {
            inner: inner,
            buf: buf,
            pos: 0,
            cap: 0,
        }
    }

    /// Gets a reference to the underlying reader.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn get_ref(&self) -> &R { &self.inner }

    /// Gets a mutable reference to the underlying reader.
    ///
    /// # Warning
    ///
    /// It is inadvisable to directly read from the underlying reader.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn get_mut(&mut self) -> &mut R { &mut self.inner }

    /// Unwraps this `BufReader`, returning the underlying reader.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn into_inner(self) -> R { self.inner }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<R: Read> Read for BufReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // If we don't have any buffered data and we're doing a massive read
        // (larger than our internal buffer), bypass our internal buffer
        // entirely.
        if self.pos == self.cap && buf.len() >= self.buf.len() {
            return self.inner.read(buf);
        }
        let nread = {
            let mut rem = try!(self.fill_buf());
            try!(rem.read(buf))
        };
        self.consume(nread);
        Ok(nread)
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<R: Read> BufRead for BufReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        // If we've reached the end of our internal buffer then we need to fetch
        // some more data from the underlying reader.
        if self.pos == self.cap {
            self.cap = try!(self.inner.read(&mut self.buf));
            self.pos = 0;
        }
        Ok(&self.buf[self.pos..self.cap])
    }

    fn consume(&mut self, amt: usize) {
        self.pos = cmp::min(self.pos + amt, self.cap);
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<R> fmt::Debug for BufReader<R> where R: fmt::Debug {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BufReader")
            .field("reader", &self.inner)
            .field("buffer", &format_args!("{}/{}", self.cap - self.pos, self.buf.len()))
            .finish()
    }
}

#[unstable(feature = "buf_seek", reason = "recently added")]
impl<R: Seek> Seek for BufReader<R> {
    /// Seek to an offset, in bytes, in the underlying reader.
    ///
    /// The position used for seeking with `SeekFrom::Current(_)` is the
    /// position the underlying reader would be at if the `BufReader` had no
    /// internal buffer.
    ///
    /// Seeking always discards the internal buffer, even if the seek position
    /// would otherwise fall within it. This guarantees that calling
    /// `.unwrap()` immediately after a seek yields the underlying reader at
    /// the same position.
    ///
    /// See `std::io::Seek` for more details.
    ///
    /// Note: In the edge case where you're seeking with `SeekFrom::Current(n)`
    /// where `n` minus the internal buffer length underflows an `i64`, two
    /// seeks will be performed instead of one. If the second seek returns
    /// `Err`, the underlying reader will be left at the same position it would
    /// have if you seeked to `SeekFrom::Current(0)`.
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let result: u64;
        if let SeekFrom::Current(n) = pos {
            let remainder = (self.cap - self.pos) as i64;
            // it should be safe to assume that remainder fits within an i64 as the alternative
            // means we managed to allocate 8 ebibytes and that's absurd.
            // But it's not out of the realm of possibility for some weird underlying reader to
            // support seeking by i64::min_value() so we need to handle underflow when subtracting
            // remainder.
            if let Some(offset) = n.checked_sub(remainder) {
                result = try!(self.inner.seek(SeekFrom::Current(offset)));
            } else {
                // seek backwards by our remainder, and then by the offset
                try!(self.inner.seek(SeekFrom::Current(-remainder)));
                self.pos = self.cap; // empty the buffer
                result = try!(self.inner.seek(SeekFrom::Current(n)));
            }
        } else {
            // Seeking with Start/End doesn't care about our buffer length.
            result = try!(self.inner.seek(pos));
        }
        self.pos = self.cap; // empty the buffer
        Ok(result)
    }
}

/// Wraps a Writer and buffers output to it
///
/// It can be excessively inefficient to work directly with a `Write`. For
/// example, every call to `write` on `TcpStream` results in a system call. A
/// `BufWriter` keeps an in memory buffer of data and writes it to the
/// underlying `Write` in large, infrequent batches.
///
/// The buffer will be written out when the writer is dropped.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct BufWriter<W: Write> {
    inner: Option<W>,
    buf: Vec<u8>,
}

/// An error returned by `into_inner` which combines an error that
/// happened while writing out the buffer, and the buffered writer object
/// which may be used to recover from the condition.
#[derive(Debug)]
#[stable(feature = "rust1", since = "1.0.0")]
pub struct IntoInnerError<W>(W, Error);

impl<W: Write> BufWriter<W> {
    /// Creates a new `BufWriter` with a default buffer capacity
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn new(inner: W) -> BufWriter<W> {
        BufWriter::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    /// Creates a new `BufWriter` with the specified buffer capacity
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn with_capacity(cap: usize, inner: W) -> BufWriter<W> {
        BufWriter {
            inner: Some(inner),
            buf: Vec::with_capacity(cap),
        }
    }

    fn flush_buf(&mut self) -> io::Result<()> {
        let mut written = 0;
        let len = self.buf.len();
        let mut ret = Ok(());
        while written < len {
            match self.inner.as_mut().unwrap().write(&self.buf[written..]) {
                Ok(0) => {
                    ret = Err(Error::new(ErrorKind::WriteZero,
                                         "failed to write the buffered data"));
                    break;
                }
                Ok(n) => written += n,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => { ret = Err(e); break }

            }
        }
        if written > 0 {
            // NB: would be better expressed as .remove(0..n) if it existed
            unsafe {
                ptr::copy(self.buf.as_ptr().offset(written as isize),
                          self.buf.as_mut_ptr(),
                          len - written);
            }
        }
        self.buf.truncate(len - written);
        ret
    }

    /// Gets a reference to the underlying writer.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn get_ref(&self) -> &W { self.inner.as_ref().unwrap() }

    /// Gets a mutable reference to the underlying write.
    ///
    /// # Warning
    ///
    /// It is inadvisable to directly read from the underlying writer.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn get_mut(&mut self) -> &mut W { self.inner.as_mut().unwrap() }

    /// Unwraps this `BufWriter`, returning the underlying writer.
    ///
    /// The buffer is written out before returning the writer.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn into_inner(mut self) -> Result<W, IntoInnerError<BufWriter<W>>> {
        match self.flush_buf() {
            Err(e) => Err(IntoInnerError(self, e)),
            Ok(()) => Ok(self.inner.take().unwrap())
        }
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<W: Write> Write for BufWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.buf.len() + buf.len() > self.buf.capacity() {
            try!(self.flush_buf());
        }
        if buf.len() >= self.buf.capacity() {
            self.inner.as_mut().unwrap().write(buf)
        } else {
            let amt = cmp::min(buf.len(), self.buf.capacity());
            Write::write(&mut self.buf, &buf[..amt])
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flush_buf().and_then(|()| self.get_mut().flush())
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<W: Write> fmt::Debug for BufWriter<W> where W: fmt::Debug {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BufWriter")
            .field("writer", &self.inner.as_ref().unwrap())
            .field("buffer", &format_args!("{}/{}", self.buf.len(), self.buf.capacity()))
            .finish()
    }
}

#[unstable(feature = "buf_seek", reason = "recently added")]
impl<W: Write+Seek> Seek for BufWriter<W> {
    /// Seek to the offset, in bytes, in the underlying writer.
    ///
    /// Seeking always writes out the internal buffer before seeking.
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.flush_buf().and_then(|_| self.get_mut().seek(pos))
    }
}

#[unsafe_destructor]
impl<W: Write> Drop for BufWriter<W> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            // dtors should not panic, so we ignore a failed flush
            let _r = self.flush_buf();
        }
    }
}

impl<W> IntoInnerError<W> {
    /// Returns the error which caused the call to `into_inner` to fail.
    ///
    /// This error was returned when attempting to write the internal buffer.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn error(&self) -> &Error { &self.1 }

    /// Returns the buffered writer instance which generated the error.
    ///
    /// The returned object can be used for error recovery, such as
    /// re-inspecting the buffer.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn into_inner(self) -> W { self.0 }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<W> From<IntoInnerError<W>> for Error {
    fn from(iie: IntoInnerError<W>) -> Error { iie.1 }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<W: Send + fmt::Debug> error::Error for IntoInnerError<W> {
    fn description(&self) -> &str {
        error::Error::description(self.error())
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<W> fmt::Display for IntoInnerError<W> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.error().fmt(f)
    }
}

/// Wraps a Writer and buffers output to it, flushing whenever a newline
/// (`0x0a`, `'\n'`) is detected.
///
/// The buffer will be written out when the writer is dropped.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct LineWriter<W: Write> {
    inner: BufWriter<W>,
}

impl<W: Write> LineWriter<W> {
    /// Creates a new `LineWriter`
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn new(inner: W) -> LineWriter<W> {
        // Lines typically aren't that long, don't use a giant buffer
        LineWriter::with_capacity(1024, inner)
    }

    /// Creates a new `LineWriter` with a specified capacity for the internal
    /// buffer.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn with_capacity(cap: usize, inner: W) -> LineWriter<W> {
        LineWriter { inner: BufWriter::with_capacity(cap, inner) }
    }

    /// Gets a reference to the underlying writer.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn get_ref(&self) -> &W { self.inner.get_ref() }

    /// Gets a mutable reference to the underlying writer.
    ///
    /// Caution must be taken when calling methods on the mutable reference
    /// returned as extra writes could corrupt the output stream.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn get_mut(&mut self) -> &mut W { self.inner.get_mut() }

    /// Unwraps this `LineWriter`, returning the underlying writer.
    ///
    /// The internal buffer is written out before returning the writer.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn into_inner(self) -> Result<W, IntoInnerError<LineWriter<W>>> {
        self.inner.into_inner().map_err(|IntoInnerError(buf, e)| {
            IntoInnerError(LineWriter { inner: buf }, e)
        })
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<W: Write> Write for LineWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match buf.rposition_elem(&b'\n') {
            Some(i) => {
                let n = try!(self.inner.write(&buf[..i + 1]));
                if n != i + 1 { return Ok(n) }
                try!(self.inner.flush());
                self.inner.write(&buf[i + 1..]).map(|i| n + i)
            }
            None => self.inner.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> { self.inner.flush() }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<W: Write> fmt::Debug for LineWriter<W> where W: fmt::Debug {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("LineWriter")
            .field("writer", &self.inner.inner)
            .field("buffer",
                   &format_args!("{}/{}", self.inner.buf.len(), self.inner.buf.capacity()))
            .finish()
    }
}

struct InternalBufWriter<W: Write>(BufWriter<W>);

impl<W: Read + Write> InternalBufWriter<W> {
    fn get_mut(&mut self) -> &mut BufWriter<W> {
        let InternalBufWriter(ref mut w) = *self;
        return w;
    }
}

impl<W: Read + Write> Read for InternalBufWriter<W> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.get_mut().inner.as_mut().unwrap().read(buf)
    }
}

/// Wraps a Stream and buffers input and output to and from it.
///
/// It can be excessively inefficient to work directly with a `Read+Write`. For
/// example, every call to `read` or `write` on `TcpStream` results in a system
/// call. A `BufStream` keeps in memory buffers of data, making large,
/// infrequent calls to `read` and `write` on the underlying `Read+Write`.
///
/// The output buffer will be written out when this stream is dropped.
#[stable(feature = "rust1", since = "1.0.0")]
pub struct BufStream<S: Write> {
    inner: BufReader<InternalBufWriter<S>>
}

impl<S: Read + Write> BufStream<S> {
    /// Creates a new buffered stream with explicitly listed capacities for the
    /// reader/writer buffer.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn with_capacities(reader_cap: usize, writer_cap: usize, inner: S)
                           -> BufStream<S> {
        let writer = BufWriter::with_capacity(writer_cap, inner);
        let internal_writer = InternalBufWriter(writer);
        let reader = BufReader::with_capacity(reader_cap, internal_writer);
        BufStream { inner: reader }
    }

    /// Creates a new buffered stream with the default reader/writer buffer
    /// capacities.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn new(inner: S) -> BufStream<S> {
        BufStream::with_capacities(DEFAULT_BUF_SIZE, DEFAULT_BUF_SIZE, inner)
    }

    /// Gets a reference to the underlying stream.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn get_ref(&self) -> &S {
        let InternalBufWriter(ref w) = self.inner.inner;
        w.get_ref()
    }

    /// Gets a mutable reference to the underlying stream.
    ///
    /// # Warning
    ///
    /// It is inadvisable to read directly from or write directly to the
    /// underlying stream.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn get_mut(&mut self) -> &mut S {
        let InternalBufWriter(ref mut w) = self.inner.inner;
        w.get_mut()
    }

    /// Unwraps this `BufStream`, returning the underlying stream.
    ///
    /// The internal write buffer is written out before returning the stream.
    /// Any leftover data in the read buffer is lost.
    #[stable(feature = "rust1", since = "1.0.0")]
    pub fn into_inner(self) -> Result<S, IntoInnerError<BufStream<S>>> {
        let BufReader { inner: InternalBufWriter(w), buf, pos, cap } = self.inner;
        w.into_inner().map_err(|IntoInnerError(w, e)| {
            IntoInnerError(BufStream {
                inner: BufReader { inner: InternalBufWriter(w), buf: buf, pos: pos, cap: cap },
            }, e)
        })
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<S: Read + Write> BufRead for BufStream<S> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> { self.inner.fill_buf() }
    fn consume(&mut self, amt: usize) { self.inner.consume(amt) }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<S: Read + Write> Read for BufStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<S: Read + Write> Write for BufStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.inner.get_mut().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.inner.get_mut().flush()
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<S: Write> fmt::Debug for BufStream<S> where S: fmt::Debug {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let reader = &self.inner;
        let writer = &self.inner.inner.0;
        fmt.debug_struct("BufStream")
            .field("stream", &writer.inner)
            .field("write_buffer", &format_args!("{}/{}", writer.buf.len(), writer.buf.capacity()))
            .field("read_buffer",
                   &format_args!("{}/{}", reader.cap - reader.pos, reader.buf.len()))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use prelude::v1::*;
    use io::prelude::*;
    use io::{self, BufReader, BufWriter, BufStream, Cursor, LineWriter, SeekFrom};
    use test;

    /// A dummy reader intended at testing short-reads propagation.
    pub struct ShortReader {
        lengths: Vec<usize>,
    }

    impl Read for ShortReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            if self.lengths.is_empty() {
                Ok(0)
            } else {
                Ok(self.lengths.remove(0))
            }
        }
    }

    #[test]
    fn test_buffered_reader() {
        let inner: &[u8] = &[5, 6, 7, 0, 1, 2, 3, 4];
        let mut reader = BufReader::with_capacity(2, inner);

        let mut buf = [0, 0, 0];
        let nread = reader.read(&mut buf);
        assert_eq!(nread.unwrap(), 3);
        let b: &[_] = &[5, 6, 7];
        assert_eq!(buf, b);

        let mut buf = [0, 0];
        let nread = reader.read(&mut buf);
        assert_eq!(nread.unwrap(), 2);
        let b: &[_] = &[0, 1];
        assert_eq!(buf, b);

        let mut buf = [0];
        let nread = reader.read(&mut buf);
        assert_eq!(nread.unwrap(), 1);
        let b: &[_] = &[2];
        assert_eq!(buf, b);

        let mut buf = [0, 0, 0];
        let nread = reader.read(&mut buf);
        assert_eq!(nread.unwrap(), 1);
        let b: &[_] = &[3, 0, 0];
        assert_eq!(buf, b);

        let nread = reader.read(&mut buf);
        assert_eq!(nread.unwrap(), 1);
        let b: &[_] = &[4, 0, 0];
        assert_eq!(buf, b);

        assert_eq!(reader.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn test_buffered_reader_seek() {
        let inner: &[u8] = &[5, 6, 7, 0, 1, 2, 3, 4];
        let mut reader = BufReader::with_capacity(2, io::Cursor::new(inner));

        assert_eq!(reader.seek(SeekFrom::Start(3)).ok(), Some(3));
        assert_eq!(reader.fill_buf().ok(), Some(&[0, 1][..]));
        assert_eq!(reader.seek(SeekFrom::Current(0)).ok(), Some(3));
        assert_eq!(reader.fill_buf().ok(), Some(&[0, 1][..]));
        assert_eq!(reader.seek(SeekFrom::Current(1)).ok(), Some(4));
        assert_eq!(reader.fill_buf().ok(), Some(&[1, 2][..]));
        reader.consume(1);
        assert_eq!(reader.seek(SeekFrom::Current(-2)).ok(), Some(3));
    }

    #[test]
    fn test_buffered_reader_seek_underflow() {
        // gimmick reader that yields its position modulo 256 for each byte
        struct PositionReader {
            pos: u64
        }
        impl Read for PositionReader {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                let len = buf.len();
                for x in buf {
                    *x = self.pos as u8;
                    self.pos = self.pos.wrapping_add(1);
                }
                Ok(len)
            }
        }
        impl Seek for PositionReader {
            fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
                match pos {
                    SeekFrom::Start(n) => {
                        self.pos = n;
                    }
                    SeekFrom::Current(n) => {
                        self.pos = self.pos.wrapping_add(n as u64);
                    }
                    SeekFrom::End(n) => {
                        self.pos = u64::max_value().wrapping_add(n as u64);
                    }
                }
                Ok(self.pos)
            }
        }

        let mut reader = BufReader::with_capacity(5, PositionReader { pos: 0 });
        assert_eq!(reader.fill_buf().ok(), Some(&[0, 1, 2, 3, 4][..]));
        assert_eq!(reader.seek(SeekFrom::End(-5)).ok(), Some(u64::max_value()-5));
        assert_eq!(reader.fill_buf().ok().map(|s| s.len()), Some(5));
        // the following seek will require two underlying seeks
        let expected = 9223372036854775802;
        assert_eq!(reader.seek(SeekFrom::Current(i64::min_value())).ok(), Some(expected));
        assert_eq!(reader.fill_buf().ok().map(|s| s.len()), Some(5));
        // seeking to 0 should empty the buffer.
        assert_eq!(reader.seek(SeekFrom::Current(0)).ok(), Some(expected));
        assert_eq!(reader.get_ref().pos, expected);
    }

    #[test]
    fn test_buffered_writer() {
        let inner = Vec::new();
        let mut writer = BufWriter::with_capacity(2, inner);

        writer.write(&[0, 1]).unwrap();
        assert_eq!(*writer.get_ref(), [0, 1]);

        writer.write(&[2]).unwrap();
        assert_eq!(*writer.get_ref(), [0, 1]);

        writer.write(&[3]).unwrap();
        assert_eq!(*writer.get_ref(), [0, 1]);

        writer.flush().unwrap();
        assert_eq!(*writer.get_ref(), [0, 1, 2, 3]);

        writer.write(&[4]).unwrap();
        writer.write(&[5]).unwrap();
        assert_eq!(*writer.get_ref(), [0, 1, 2, 3]);

        writer.write(&[6]).unwrap();
        assert_eq!(*writer.get_ref(), [0, 1, 2, 3, 4, 5]);

        writer.write(&[7, 8]).unwrap();
        assert_eq!(*writer.get_ref(), [0, 1, 2, 3, 4, 5, 6, 7, 8]);

        writer.write(&[9, 10, 11]).unwrap();
        assert_eq!(*writer.get_ref(), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);

        writer.flush().unwrap();
        assert_eq!(*writer.get_ref(), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    }

    #[test]
    fn test_buffered_writer_inner_flushes() {
        let mut w = BufWriter::with_capacity(3, Vec::new());
        w.write(&[0, 1]).unwrap();
        assert_eq!(*w.get_ref(), []);
        let w = w.into_inner().unwrap();
        assert_eq!(w, [0, 1]);
    }

    #[test]
    fn test_buffered_writer_seek() {
        let mut w = BufWriter::with_capacity(3, io::Cursor::new(Vec::new()));
        w.write_all(&[0, 1, 2, 3, 4, 5]).unwrap();
        w.write_all(&[6, 7]).unwrap();
        assert_eq!(w.seek(SeekFrom::Current(0)).ok(), Some(8));
        assert_eq!(&w.get_ref().get_ref()[..], &[0, 1, 2, 3, 4, 5, 6, 7][..]);
        assert_eq!(w.seek(SeekFrom::Start(2)).ok(), Some(2));
        w.write_all(&[8, 9]).unwrap();
        assert_eq!(&w.into_inner().unwrap().into_inner()[..], &[0, 1, 8, 9, 4, 5, 6, 7]);
    }

    // This is just here to make sure that we don't infinite loop in the
    // newtype struct autoderef weirdness
    #[test]
    fn test_buffered_stream() {
        struct S;

        impl Write for S {
            fn write(&mut self, b: &[u8]) -> io::Result<usize> { Ok(b.len()) }
            fn flush(&mut self) -> io::Result<()> { Ok(()) }
        }

        impl Read for S {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> { Ok(0) }
        }

        let mut stream = BufStream::new(S);
        assert_eq!(stream.read(&mut [0; 10]).unwrap(), 0);
        stream.write(&[0; 10]).unwrap();
        stream.flush().unwrap();
    }

    #[test]
    fn test_read_until() {
        let inner: &[u8] = &[0, 1, 2, 1, 0];
        let mut reader = BufReader::with_capacity(2, inner);
        let mut v = Vec::new();
        reader.read_until(0, &mut v).unwrap();
        assert_eq!(v, [0]);
        v.truncate(0);
        reader.read_until(2, &mut v).unwrap();
        assert_eq!(v, [1, 2]);
        v.truncate(0);
        reader.read_until(1, &mut v).unwrap();
        assert_eq!(v, [1]);
        v.truncate(0);
        reader.read_until(8, &mut v).unwrap();
        assert_eq!(v, [0]);
        v.truncate(0);
        reader.read_until(9, &mut v).unwrap();
        assert_eq!(v, []);
    }

    #[test]
    fn test_line_buffer() {
        let mut writer = LineWriter::new(Vec::new());
        writer.write(&[0]).unwrap();
        assert_eq!(*writer.get_ref(), []);
        writer.write(&[1]).unwrap();
        assert_eq!(*writer.get_ref(), []);
        writer.flush().unwrap();
        assert_eq!(*writer.get_ref(), [0, 1]);
        writer.write(&[0, b'\n', 1, b'\n', 2]).unwrap();
        assert_eq!(*writer.get_ref(), [0, 1, 0, b'\n', 1, b'\n']);
        writer.flush().unwrap();
        assert_eq!(*writer.get_ref(), [0, 1, 0, b'\n', 1, b'\n', 2]);
        writer.write(&[3, b'\n']).unwrap();
        assert_eq!(*writer.get_ref(), [0, 1, 0, b'\n', 1, b'\n', 2, 3, b'\n']);
    }

    #[test]
    fn test_read_line() {
        let in_buf: &[u8] = b"a\nb\nc";
        let mut reader = BufReader::with_capacity(2, in_buf);
        let mut s = String::new();
        reader.read_line(&mut s).unwrap();
        assert_eq!(s, "a\n");
        s.truncate(0);
        reader.read_line(&mut s).unwrap();
        assert_eq!(s, "b\n");
        s.truncate(0);
        reader.read_line(&mut s).unwrap();
        assert_eq!(s, "c");
        s.truncate(0);
        reader.read_line(&mut s).unwrap();
        assert_eq!(s, "");
    }

    #[test]
    fn test_lines() {
        let in_buf: &[u8] = b"a\nb\nc";
        let reader = BufReader::with_capacity(2, in_buf);
        let mut it = reader.lines();
        assert_eq!(it.next().unwrap().unwrap(), "a".to_string());
        assert_eq!(it.next().unwrap().unwrap(), "b".to_string());
        assert_eq!(it.next().unwrap().unwrap(), "c".to_string());
        assert!(it.next().is_none());
    }

    #[test]
    fn test_short_reads() {
        let inner = ShortReader{lengths: vec![0, 1, 2, 0, 1, 0]};
        let mut reader = BufReader::new(inner);
        let mut buf = [0, 0];
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
        assert_eq!(reader.read(&mut buf).unwrap(), 1);
        assert_eq!(reader.read(&mut buf).unwrap(), 2);
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
        assert_eq!(reader.read(&mut buf).unwrap(), 1);
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
    }

    #[test]
    fn read_char_buffered() {
        let buf = [195, 159];
        let reader = BufReader::with_capacity(1, &buf[..]);
        assert_eq!(reader.chars().next().unwrap().unwrap(), 'ß');
    }

    #[test]
    fn test_chars() {
        let buf = [195, 159, b'a'];
        let reader = BufReader::with_capacity(1, &buf[..]);
        let mut it = reader.chars();
        assert_eq!(it.next().unwrap().unwrap(), 'ß');
        assert_eq!(it.next().unwrap().unwrap(), 'a');
        assert!(it.next().is_none());
    }

    #[test]
    #[should_panic]
    fn dont_panic_in_drop_on_panicked_flush() {
        struct FailFlushWriter;

        impl Write for FailFlushWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> { Ok(buf.len()) }
            fn flush(&mut self) -> io::Result<()> {
                Err(io::Error::last_os_error())
            }
        }

        let writer = FailFlushWriter;
        let _writer = BufWriter::new(writer);

        // If writer panics *again* due to the flush error then the process will
        // abort.
        panic!();
    }

    #[bench]
    fn bench_buffered_reader(b: &mut test::Bencher) {
        b.iter(|| {
            BufReader::new(io::empty())
        });
    }

    #[bench]
    fn bench_buffered_writer(b: &mut test::Bencher) {
        b.iter(|| {
            BufWriter::new(io::sink())
        });
    }

    #[bench]
    fn bench_buffered_stream(b: &mut test::Bencher) {
        let mut buf = Cursor::new(Vec::new());
        b.iter(|| {
            BufStream::new(&mut buf);
        });
    }
}
