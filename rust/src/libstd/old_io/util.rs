// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Utility implementations of Reader and Writer

use prelude::v1::*;
use cmp;
use old_io;
use slice::bytes::MutableByteVector;

/// Wraps a `Reader`, limiting the number of bytes that can be read from it.
#[derive(Debug)]
pub struct LimitReader<R> {
    limit: uint,
    inner: R
}

impl<R: Reader> LimitReader<R> {
    /// Creates a new `LimitReader`
    pub fn new(r: R, limit: uint) -> LimitReader<R> {
        LimitReader { limit: limit, inner: r }
    }

    /// Consumes the `LimitReader`, returning the underlying `Reader`.
    pub fn into_inner(self) -> R { self.inner }

    /// Returns the number of bytes that can be read before the `LimitReader`
    /// will return EOF.
    ///
    /// # Note
    ///
    /// The reader may reach EOF after reading fewer bytes than indicated by
    /// this method if the underlying reader reaches EOF.
    pub fn limit(&self) -> uint { self.limit }
}

impl<R: Reader> Reader for LimitReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> old_io::IoResult<uint> {
        if self.limit == 0 {
            return Err(old_io::standard_error(old_io::EndOfFile));
        }

        let len = cmp::min(self.limit, buf.len());
        let res = self.inner.read(&mut buf[..len]);
        match res {
            Ok(len) => self.limit -= len,
            _ => {}
        }
        res
    }
}

impl<R: Buffer> Buffer for LimitReader<R> {
    fn fill_buf<'a>(&'a mut self) -> old_io::IoResult<&'a [u8]> {
        let amt = try!(self.inner.fill_buf());
        let buf = &amt[..cmp::min(amt.len(), self.limit)];
        if buf.len() == 0 {
            Err(old_io::standard_error(old_io::EndOfFile))
        } else {
            Ok(buf)
        }
    }

    fn consume(&mut self, amt: uint) {
        // Don't let callers reset the limit by passing an overlarge value
        let amt = cmp::min(amt, self.limit);
        self.limit -= amt;
        self.inner.consume(amt);
    }

}

/// A `Writer` which ignores bytes written to it, like /dev/null.
#[derive(Copy, Debug)]
pub struct NullWriter;

impl Writer for NullWriter {
    #[inline]
    fn write_all(&mut self, _buf: &[u8]) -> old_io::IoResult<()> { Ok(()) }
}

/// A `Reader` which returns an infinite stream of 0 bytes, like /dev/zero.
#[derive(Copy, Debug)]
pub struct ZeroReader;

impl Reader for ZeroReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> old_io::IoResult<uint> {
        buf.set_memory(0);
        Ok(buf.len())
    }
}

impl Buffer for ZeroReader {
    fn fill_buf<'a>(&'a mut self) -> old_io::IoResult<&'a [u8]> {
        static DATA: [u8; 64] = [0; 64];
        Ok(&DATA)
    }

    fn consume(&mut self, _amt: uint) {}
}

/// A `Reader` which is always at EOF, like /dev/null.
#[derive(Copy, Debug)]
pub struct NullReader;

impl Reader for NullReader {
    #[inline]
    fn read(&mut self, _buf: &mut [u8]) -> old_io::IoResult<uint> {
        Err(old_io::standard_error(old_io::EndOfFile))
    }
}

impl Buffer for NullReader {
    fn fill_buf<'a>(&'a mut self) -> old_io::IoResult<&'a [u8]> {
        Err(old_io::standard_error(old_io::EndOfFile))
    }
    fn consume(&mut self, _amt: uint) {}
}

/// A `Writer` which multiplexes writes to a set of `Writer`s.
///
/// The `Writer`s are delegated to in order. If any `Writer` returns an error,
/// that error is returned immediately and remaining `Writer`s are not called.
#[derive(Debug)]
pub struct MultiWriter<W> {
    writers: Vec<W>
}

impl<W> MultiWriter<W> where W: Writer {
    /// Creates a new `MultiWriter`
    pub fn new(writers: Vec<W>) -> MultiWriter<W> {
        MultiWriter { writers: writers }
    }
}

impl<W> Writer for MultiWriter<W> where W: Writer {
    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> old_io::IoResult<()> {
        for writer in &mut self.writers {
            try!(writer.write_all(buf));
        }
        Ok(())
    }

    #[inline]
    fn flush(&mut self) -> old_io::IoResult<()> {
        for writer in &mut self.writers {
            try!(writer.flush());
        }
        Ok(())
    }
}

/// A `Reader` which chains input from multiple `Reader`s, reading each to
/// completion before moving onto the next.
#[derive(Clone, Debug)]
pub struct ChainedReader<I, R> {
    readers: I,
    cur_reader: Option<R>,
}

impl<R: Reader, I: Iterator<Item=R>> ChainedReader<I, R> {
    /// Creates a new `ChainedReader`
    pub fn new(mut readers: I) -> ChainedReader<I, R> {
        let r = readers.next();
        ChainedReader { readers: readers, cur_reader: r }
    }
}

impl<R: Reader, I: Iterator<Item=R>> Reader for ChainedReader<I, R> {
    fn read(&mut self, buf: &mut [u8]) -> old_io::IoResult<uint> {
        loop {
            let err = match self.cur_reader {
                Some(ref mut r) => {
                    match r.read(buf) {
                        Ok(len) => return Ok(len),
                        Err(ref e) if e.kind == old_io::EndOfFile => None,
                        Err(e) => Some(e),
                    }
                }
                None => break
            };
            self.cur_reader = self.readers.next();
            match err {
                Some(e) => return Err(e),
                None => {}
            }
        }
        Err(old_io::standard_error(old_io::EndOfFile))
    }
}

/// A `Reader` which forwards input from another `Reader`, passing it along to
/// a `Writer` as well. Similar to the `tee(1)` command.
#[derive(Debug)]
pub struct TeeReader<R, W> {
    reader: R,
    writer: W,
}

impl<R: Reader, W: Writer> TeeReader<R, W> {
    /// Creates a new `TeeReader`
    pub fn new(r: R, w: W) -> TeeReader<R, W> {
        TeeReader { reader: r, writer: w }
    }

    /// Consumes the `TeeReader`, returning the underlying `Reader` and
    /// `Writer`.
    pub fn into_inner(self) -> (R, W) {
        let TeeReader { reader, writer } = self;
        (reader, writer)
    }
}

impl<R: Reader, W: Writer> Reader for TeeReader<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> old_io::IoResult<uint> {
        self.reader.read(buf).and_then(|len| {
            self.writer.write_all(&mut buf[..len]).map(|()| len)
        })
    }
}

/// Copies all data from a `Reader` to a `Writer`.
pub fn copy<R: Reader, W: Writer>(r: &mut R, w: &mut W) -> old_io::IoResult<()> {
    let mut buf = [0; super::DEFAULT_BUF_SIZE];
    loop {
        let len = match r.read(&mut buf) {
            Ok(len) => len,
            Err(ref e) if e.kind == old_io::EndOfFile => return Ok(()),
            Err(e) => return Err(e),
        };
        try!(w.write_all(&buf[..len]));
    }
}

/// An adaptor converting an `Iterator<u8>` to a `Reader`.
#[derive(Clone, Debug)]
pub struct IterReader<T> {
    iter: T,
}

impl<T: Iterator<Item=u8>> IterReader<T> {
    /// Creates a new `IterReader` which will read from the specified
    /// `Iterator`.
    pub fn new(iter: T) -> IterReader<T> {
        IterReader { iter: iter }
    }
}

impl<T: Iterator<Item=u8>> Reader for IterReader<T> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> old_io::IoResult<uint> {
        let mut len = 0;
        for (slot, elt) in buf.iter_mut().zip(self.iter.by_ref()) {
            *slot = elt;
            len += 1;
        }
        if len == 0 && buf.len() != 0 {
            Err(old_io::standard_error(old_io::EndOfFile))
        } else {
            Ok(len)
        }
    }
}

#[cfg(test)]
mod test {
    use prelude::v1::*;

    use old_io::{MemReader, ByRefReader};
    use old_io;
    use super::*;

    #[test]
    fn test_limit_reader_unlimited() {
        let mut r = MemReader::new(vec!(0, 1, 2));
        {
            let mut r = LimitReader::new(r.by_ref(), 4);
            assert_eq!([0, 1, 2], r.read_to_end().unwrap());
        }
    }

    #[test]
    fn test_limit_reader_limited() {
        let mut r = MemReader::new(vec!(0, 1, 2));
        {
            let mut r = LimitReader::new(r.by_ref(), 2);
            assert_eq!([0, 1], r.read_to_end().unwrap());
        }
        assert_eq!([2], r.read_to_end().unwrap());
    }

    #[test]
    fn test_limit_reader_limit() {
        let r = MemReader::new(vec!(0, 1, 2));
        let mut r = LimitReader::new(r, 3);
        assert_eq!(3, r.limit());
        assert_eq!(0, r.read_byte().unwrap());
        assert_eq!(2, r.limit());
        assert_eq!([1, 2], r.read_to_end().unwrap());
        assert_eq!(0, r.limit());
    }

    #[test]
    fn test_limit_reader_overlong_consume() {
        let mut r = MemReader::new(vec![0, 1, 2, 3, 4, 5]);
        let mut r = LimitReader::new(r.by_ref(), 1);
        r.consume(2);
        assert_eq!([], r.read_to_end().unwrap());
    }

    #[test]
    fn test_null_writer() {
        let mut s = NullWriter;
        let buf = vec![0, 0, 0];
        s.write_all(&buf).unwrap();
        s.flush().unwrap();
    }

    #[test]
    fn test_zero_reader() {
        let mut s = ZeroReader;
        let mut buf = vec![1, 2, 3];
        assert_eq!(s.read(&mut buf), Ok(3));
        assert_eq!([0, 0, 0], buf);
    }

    #[test]
    fn test_null_reader() {
        let mut r = NullReader;
        let mut buf = vec![0];
        assert!(r.read(&mut buf).is_err());
    }

    #[test]
    fn test_multi_writer() {
        static mut writes: uint = 0;
        static mut flushes: uint = 0;

        struct TestWriter;
        impl Writer for TestWriter {
            fn write_all(&mut self, _buf: &[u8]) -> old_io::IoResult<()> {
                unsafe { writes += 1 }
                Ok(())
            }

            fn flush(&mut self) -> old_io::IoResult<()> {
                unsafe { flushes += 1 }
                Ok(())
            }
        }

        let mut multi = MultiWriter::new(vec!(box TestWriter as Box<Writer>,
                                              box TestWriter as Box<Writer>));
        multi.write_all(&[1, 2, 3]).unwrap();
        assert_eq!(2, unsafe { writes });
        assert_eq!(0, unsafe { flushes });
        multi.flush().unwrap();
        assert_eq!(2, unsafe { writes });
        assert_eq!(2, unsafe { flushes });
    }

    #[test]
    fn test_chained_reader() {
        let rs = vec!(MemReader::new(vec!(0, 1)), MemReader::new(vec!()),
                      MemReader::new(vec!(2, 3)));
        let mut r = ChainedReader::new(rs.into_iter());
        assert_eq!([0, 1, 2, 3], r.read_to_end().unwrap());
    }

    #[test]
    fn test_tee_reader() {
        let mut r = TeeReader::new(MemReader::new(vec!(0, 1, 2)),
                                   Vec::new());
        assert_eq!([0, 1, 2], r.read_to_end().unwrap());
        let (_, w) = r.into_inner();
        assert_eq!([0, 1, 2], w);
    }

    #[test]
    fn test_copy() {
        let mut r = MemReader::new(vec!(0, 1, 2, 3, 4));
        let mut w = Vec::new();
        copy(&mut r, &mut w).unwrap();
        assert_eq!([0, 1, 2, 3, 4], w);
    }

    #[test]
    fn limit_reader_buffer() {
        let r = &mut b"0123456789\n0123456789\n";
        {
            let mut r = LimitReader::new(r.by_ref(), 3);
            assert_eq!(r.read_line(), Ok("012".to_string()));
            assert_eq!(r.limit(), 0);
            assert_eq!(r.read_line().err().unwrap().kind, old_io::EndOfFile);
        }
        {
            let mut r = LimitReader::new(r.by_ref(), 9);
            assert_eq!(r.read_line(), Ok("3456789\n".to_string()));
            assert_eq!(r.limit(), 1);
            assert_eq!(r.read_line(), Ok("0".to_string()));
        }
        {
            let mut r = LimitReader::new(r.by_ref(), 100);
            assert_eq!(r.read_char(), Ok('1'));
            assert_eq!(r.limit(), 99);
            assert_eq!(r.read_line(), Ok("23456789\n".to_string()));
        }
    }

    #[test]
    fn test_iter_reader() {
        let mut r = IterReader::new(0u8..8);
        let mut buf = [0, 0, 0];
        let len = r.read(&mut buf).unwrap();
        assert_eq!(len, 3);
        assert!(buf == [0, 1, 2]);

        let len = r.read(&mut buf).unwrap();
        assert_eq!(len, 3);
        assert!(buf == [3, 4, 5]);

        let len = r.read(&mut buf).unwrap();
        assert_eq!(len, 2);
        assert!(buf == [6, 7, 5]);

        assert_eq!(r.read(&mut buf).unwrap_err().kind, old_io::EndOfFile);
    }

    #[test]
    fn iter_reader_zero_length() {
        let mut r = IterReader::new(0u8..8);
        let mut buf = [];
        assert_eq!(Ok(0), r.read(&mut buf));
    }
}
