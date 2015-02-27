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
// ignore-lexer-test FIXME #15679

//! Readers and Writers for in-memory buffers

use cmp::min;
use option::Option::None;
use result::Result::{Err, Ok};
use old_io;
use old_io::{Reader, Writer, Seek, Buffer, IoError, SeekStyle, IoResult};
use slice::{self, SliceExt};
use vec::Vec;

const BUF_CAPACITY: uint = 128;

fn combine(seek: SeekStyle, cur: uint, end: uint, offset: i64) -> IoResult<u64> {
    // compute offset as signed and clamp to prevent overflow
    let pos = match seek {
        old_io::SeekSet => 0,
        old_io::SeekEnd => end,
        old_io::SeekCur => cur,
    } as i64;

    if offset + pos < 0 {
        Err(IoError {
            kind: old_io::InvalidInput,
            desc: "invalid seek to a negative offset",
            detail: None
        })
    } else {
        Ok((offset + pos) as u64)
    }
}

impl Writer for Vec<u8> {
    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> IoResult<()> {
        self.push_all(buf);
        Ok(())
    }
}

/// Writes to an owned, growable byte vector
///
/// # Example
///
/// ```rust
/// # #![allow(unused_must_use)]
/// use std::old_io::MemWriter;
///
/// let mut w = MemWriter::new();
/// w.write(&[0, 1, 2]);
///
/// assert_eq!(w.into_inner(), [0, 1, 2]);
/// ```
#[unstable(feature = "io")]
#[deprecated(since = "1.0.0",
             reason = "use the Vec<u8> Writer implementation directly")]
#[derive(Clone)]
#[allow(deprecated)]
pub struct MemWriter {
    buf: Vec<u8>,
}

#[allow(deprecated)]
impl MemWriter {
    /// Create a new `MemWriter`.
    #[inline]
    pub fn new() -> MemWriter {
        MemWriter::with_capacity(BUF_CAPACITY)
    }
    /// Create a new `MemWriter`, allocating at least `n` bytes for
    /// the internal buffer.
    #[inline]
    pub fn with_capacity(n: uint) -> MemWriter {
        MemWriter::from_vec(Vec::with_capacity(n))
    }
    /// Create a new `MemWriter` that will append to an existing `Vec`.
    #[inline]
    pub fn from_vec(buf: Vec<u8>) -> MemWriter {
        MemWriter { buf: buf }
    }

    /// Acquires an immutable reference to the underlying buffer of this
    /// `MemWriter`.
    #[inline]
    pub fn get_ref<'a>(&'a self) -> &'a [u8] { &self.buf }

    /// Unwraps this `MemWriter`, returning the underlying buffer
    #[inline]
    pub fn into_inner(self) -> Vec<u8> { self.buf }
}

impl Writer for MemWriter {
    #[inline]
    #[allow(deprecated)]
    fn write_all(&mut self, buf: &[u8]) -> IoResult<()> {
        self.buf.push_all(buf);
        Ok(())
    }
}

/// Reads from an owned byte vector
///
/// # Example
///
/// ```rust
/// # #![allow(unused_must_use)]
/// use std::old_io::MemReader;
///
/// let mut r = MemReader::new(vec!(0, 1, 2));
///
/// assert_eq!(r.read_to_end().unwrap(), [0, 1, 2]);
/// ```
pub struct MemReader {
    buf: Vec<u8>,
    pos: uint
}

impl MemReader {
    /// Creates a new `MemReader` which will read the buffer given. The buffer
    /// can be re-acquired through `unwrap`
    #[inline]
    pub fn new(buf: Vec<u8>) -> MemReader {
        MemReader {
            buf: buf,
            pos: 0
        }
    }

    /// Tests whether this reader has read all bytes in its buffer.
    ///
    /// If `true`, then this will no longer return bytes from `read`.
    #[inline]
    pub fn eof(&self) -> bool { self.pos >= self.buf.len() }

    /// Acquires an immutable reference to the underlying buffer of this
    /// `MemReader`.
    ///
    /// No method is exposed for acquiring a mutable reference to the buffer
    /// because it could corrupt the state of this `MemReader`.
    #[inline]
    pub fn get_ref<'a>(&'a self) -> &'a [u8] { &self.buf }

    /// Unwraps this `MemReader`, returning the underlying buffer
    #[inline]
    pub fn into_inner(self) -> Vec<u8> { self.buf }
}

impl Reader for MemReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        if self.eof() { return Err(old_io::standard_error(old_io::EndOfFile)) }

        let write_len = min(buf.len(), self.buf.len() - self.pos);
        {
            let input = &self.buf[self.pos.. self.pos + write_len];
            let output = &mut buf[..write_len];
            assert_eq!(input.len(), output.len());
            slice::bytes::copy_memory(output, input);
        }
        self.pos += write_len;
        assert!(self.pos <= self.buf.len());

        return Ok(write_len);
    }
}

impl Seek for MemReader {
    #[inline]
    fn tell(&self) -> IoResult<u64> { Ok(self.pos as u64) }

    #[inline]
    fn seek(&mut self, pos: i64, style: SeekStyle) -> IoResult<()> {
        let new = try!(combine(style, self.pos, self.buf.len(), pos));
        self.pos = new as uint;
        Ok(())
    }
}

impl Buffer for MemReader {
    #[inline]
    fn fill_buf<'a>(&'a mut self) -> IoResult<&'a [u8]> {
        if self.pos < self.buf.len() {
            Ok(&self.buf[self.pos..])
        } else {
            Err(old_io::standard_error(old_io::EndOfFile))
        }
    }

    #[inline]
    fn consume(&mut self, amt: uint) { self.pos += amt; }
}

impl<'a> Reader for &'a [u8] {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        if self.is_empty() { return Err(old_io::standard_error(old_io::EndOfFile)); }

        let write_len = min(buf.len(), self.len());
        {
            let input = &self[..write_len];
            let output = &mut buf[.. write_len];
            slice::bytes::copy_memory(output, input);
        }

        *self = &self[write_len..];

        Ok(write_len)
    }
}

impl<'a> Buffer for &'a [u8] {
    #[inline]
    fn fill_buf(&mut self) -> IoResult<&[u8]> {
        if self.is_empty() {
            Err(old_io::standard_error(old_io::EndOfFile))
        } else {
            Ok(*self)
        }
    }

    #[inline]
    fn consume(&mut self, amt: uint) {
        *self = &self[amt..];
    }
}


/// Writes to a fixed-size byte slice
///
/// If a write will not fit in the buffer, it returns an error and does not
/// write any data.
///
/// # Example
///
/// ```rust
/// # #![allow(unused_must_use)]
/// use std::old_io::BufWriter;
///
/// let mut buf = [0; 4];
/// {
///     let mut w = BufWriter::new(&mut buf);
///     w.write(&[0, 1, 2]);
/// }
/// assert!(buf == [0, 1, 2, 0]);
/// ```
pub struct BufWriter<'a> {
    buf: &'a mut [u8],
    pos: uint
}

impl<'a> BufWriter<'a> {
    /// Creates a new `BufWriter` which will wrap the specified buffer. The
    /// writer initially starts at position 0.
    #[inline]
    pub fn new(buf: &'a mut [u8]) -> BufWriter<'a> {
        BufWriter {
            buf: buf,
            pos: 0
        }
    }
}

impl<'a> Writer for BufWriter<'a> {
    #[inline]
    fn write_all(&mut self, src: &[u8]) -> IoResult<()> {
        let dst = &mut self.buf[self.pos..];
        let dst_len = dst.len();

        if dst_len == 0 {
            return Err(old_io::standard_error(old_io::EndOfFile));
        }

        let src_len = src.len();

        if dst_len >= src_len {
            slice::bytes::copy_memory(dst, src);

            self.pos += src_len;

            Ok(())
        } else {
            slice::bytes::copy_memory(dst, &src[..dst_len]);

            self.pos += dst_len;

            Err(old_io::standard_error(old_io::ShortWrite(dst_len)))
        }
    }
}

impl<'a> Seek for BufWriter<'a> {
    #[inline]
    fn tell(&self) -> IoResult<u64> { Ok(self.pos as u64) }

    #[inline]
    fn seek(&mut self, pos: i64, style: SeekStyle) -> IoResult<()> {
        let new = try!(combine(style, self.pos, self.buf.len(), pos));
        self.pos = min(new as uint, self.buf.len());
        Ok(())
    }
}

/// Reads from a fixed-size byte slice
///
/// # Example
///
/// ```rust
/// # #![allow(unused_must_use)]
/// use std::old_io::BufReader;
///
/// let buf = [0, 1, 2, 3];
/// let mut r = BufReader::new(&buf);
///
/// assert_eq!(r.read_to_end().unwrap(), [0, 1, 2, 3]);
/// ```
pub struct BufReader<'a> {
    buf: &'a [u8],
    pos: uint
}

impl<'a> BufReader<'a> {
    /// Creates a new buffered reader which will read the specified buffer
    #[inline]
    pub fn new(buf: &'a [u8]) -> BufReader<'a> {
        BufReader {
            buf: buf,
            pos: 0
        }
    }

    /// Tests whether this reader has read all bytes in its buffer.
    ///
    /// If `true`, then this will no longer return bytes from `read`.
    #[inline]
    pub fn eof(&self) -> bool { self.pos >= self.buf.len() }
}

impl<'a> Reader for BufReader<'a> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        if self.eof() { return Err(old_io::standard_error(old_io::EndOfFile)) }

        let write_len = min(buf.len(), self.buf.len() - self.pos);
        {
            let input = &self.buf[self.pos.. self.pos + write_len];
            let output = &mut buf[..write_len];
            assert_eq!(input.len(), output.len());
            slice::bytes::copy_memory(output, input);
        }
        self.pos += write_len;
        assert!(self.pos <= self.buf.len());

        return Ok(write_len);
     }
}

impl<'a> Seek for BufReader<'a> {
    #[inline]
    fn tell(&self) -> IoResult<u64> { Ok(self.pos as u64) }

    #[inline]
    fn seek(&mut self, pos: i64, style: SeekStyle) -> IoResult<()> {
        let new = try!(combine(style, self.pos, self.buf.len(), pos));
        self.pos = new as uint;
        Ok(())
    }
}

impl<'a> Buffer for BufReader<'a> {
    #[inline]
    fn fill_buf(&mut self) -> IoResult<&[u8]> {
        if self.pos < self.buf.len() {
            Ok(&self.buf[self.pos..])
        } else {
            Err(old_io::standard_error(old_io::EndOfFile))
        }
    }

    #[inline]
    fn consume(&mut self, amt: uint) { self.pos += amt; }
}

#[cfg(test)]
mod test {
    extern crate "test" as test_crate;
    use old_io::{SeekSet, SeekCur, SeekEnd, Reader, Writer, Seek};
    use prelude::v1::{Ok, Err, range,  Vec, Buffer,  AsSlice, SliceExt};
    use prelude::v1::IteratorExt;
    use old_io;
    use iter::repeat;
    use self::test_crate::Bencher;
    use super::*;

    #[test]
    fn test_vec_writer() {
        let mut writer = Vec::new();
        writer.write(&[0]).unwrap();
        writer.write(&[1, 2, 3]).unwrap();
        writer.write(&[4, 5, 6, 7]).unwrap();
        let b: &[_] = &[0, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(writer, b);
    }

    #[test]
    fn test_mem_writer() {
        let mut writer = MemWriter::new();
        writer.write(&[0]).unwrap();
        writer.write(&[1, 2, 3]).unwrap();
        writer.write(&[4, 5, 6, 7]).unwrap();
        let b: &[_] = &[0, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(writer.get_ref(), b);
    }

    #[test]
    fn test_buf_writer() {
        let mut buf = [0 as u8; 9];
        {
            let mut writer = BufWriter::new(&mut buf);
            assert_eq!(writer.tell(), Ok(0));
            writer.write(&[0]).unwrap();
            assert_eq!(writer.tell(), Ok(1));
            writer.write(&[1, 2, 3]).unwrap();
            writer.write(&[4, 5, 6, 7]).unwrap();
            assert_eq!(writer.tell(), Ok(8));
            writer.write(&[]).unwrap();
            assert_eq!(writer.tell(), Ok(8));

            assert_eq!(writer.write(&[8, 9]).err().unwrap().kind, old_io::ShortWrite(1));
            assert_eq!(writer.write(&[10]).err().unwrap().kind, old_io::EndOfFile);
        }
        let b: &[_] = &[0, 1, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(buf, b);
    }

    #[test]
    fn test_buf_writer_seek() {
        let mut buf = [0 as u8; 8];
        {
            let mut writer = BufWriter::new(&mut buf);
            assert_eq!(writer.tell(), Ok(0));
            writer.write(&[1]).unwrap();
            assert_eq!(writer.tell(), Ok(1));

            writer.seek(2, SeekSet).unwrap();
            assert_eq!(writer.tell(), Ok(2));
            writer.write(&[2]).unwrap();
            assert_eq!(writer.tell(), Ok(3));

            writer.seek(-2, SeekCur).unwrap();
            assert_eq!(writer.tell(), Ok(1));
            writer.write(&[3]).unwrap();
            assert_eq!(writer.tell(), Ok(2));

            writer.seek(-1, SeekEnd).unwrap();
            assert_eq!(writer.tell(), Ok(7));
            writer.write(&[4]).unwrap();
            assert_eq!(writer.tell(), Ok(8));

        }
        let b: &[_] = &[1, 3, 2, 0, 0, 0, 0, 4];
        assert_eq!(buf, b);
    }

    #[test]
    fn test_buf_writer_error() {
        let mut buf = [0 as u8; 2];
        let mut writer = BufWriter::new(&mut buf);
        writer.write(&[0]).unwrap();

        match writer.write(&[0, 0]) {
            Ok(..) => panic!(),
            Err(e) => assert_eq!(e.kind, old_io::ShortWrite(1)),
        }
    }

    #[test]
    fn test_mem_reader() {
        let mut reader = MemReader::new(vec!(0, 1, 2, 3, 4, 5, 6, 7));
        let mut buf = [];
        assert_eq!(reader.read(&mut buf), Ok(0));
        assert_eq!(reader.tell(), Ok(0));
        let mut buf = [0];
        assert_eq!(reader.read(&mut buf), Ok(1));
        assert_eq!(reader.tell(), Ok(1));
        let b: &[_] = &[0];
        assert_eq!(buf, b);
        let mut buf = [0; 4];
        assert_eq!(reader.read(&mut buf), Ok(4));
        assert_eq!(reader.tell(), Ok(5));
        let b: &[_] = &[1, 2, 3, 4];
        assert_eq!(buf, b);
        assert_eq!(reader.read(&mut buf), Ok(3));
        let b: &[_] = &[5, 6, 7];
        assert_eq!(&buf[..3], b);
        assert!(reader.read(&mut buf).is_err());
        let mut reader = MemReader::new(vec!(0, 1, 2, 3, 4, 5, 6, 7));
        assert_eq!(reader.read_until(3).unwrap(), [0, 1, 2, 3]);
        assert_eq!(reader.read_until(3).unwrap(), [4, 5, 6, 7]);
        assert!(reader.read(&mut buf).is_err());
    }

    #[test]
    fn test_slice_reader() {
        let in_buf = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let mut reader = &mut &*in_buf;
        let mut buf = [];
        assert_eq!(reader.read(&mut buf), Ok(0));
        let mut buf = [0];
        assert_eq!(reader.read(&mut buf), Ok(1));
        assert_eq!(reader.len(), 7);
        let b: &[_] = &[0];
        assert_eq!(buf, b);
        let mut buf = [0; 4];
        assert_eq!(reader.read(&mut buf), Ok(4));
        assert_eq!(reader.len(), 3);
        let b: &[_] = &[1, 2, 3, 4];
        assert_eq!(buf, b);
        assert_eq!(reader.read(&mut buf), Ok(3));
        let b: &[_] = &[5, 6, 7];
        assert_eq!(&buf[..3], b);
        assert!(reader.read(&mut buf).is_err());
        let mut reader = &mut &*in_buf;
        assert_eq!(reader.read_until(3).unwrap(), [0, 1, 2, 3]);
        assert_eq!(reader.read_until(3).unwrap(), [4, 5, 6, 7]);
        assert!(reader.read(&mut buf).is_err());
    }

    #[test]
    fn test_buf_reader() {
        let in_buf = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let mut reader = BufReader::new(&in_buf);
        let mut buf = [];
        assert_eq!(reader.read(&mut buf), Ok(0));
        assert_eq!(reader.tell(), Ok(0));
        let mut buf = [0];
        assert_eq!(reader.read(&mut buf), Ok(1));
        assert_eq!(reader.tell(), Ok(1));
        let b: &[_] = &[0];
        assert_eq!(buf, b);
        let mut buf = [0; 4];
        assert_eq!(reader.read(&mut buf), Ok(4));
        assert_eq!(reader.tell(), Ok(5));
        let b: &[_] = &[1, 2, 3, 4];
        assert_eq!(buf, b);
        assert_eq!(reader.read(&mut buf), Ok(3));
        let b: &[_] = &[5, 6, 7];
        assert_eq!(&buf[..3], b);
        assert!(reader.read(&mut buf).is_err());
        let mut reader = BufReader::new(&in_buf);
        assert_eq!(reader.read_until(3).unwrap(), [0, 1, 2, 3]);
        assert_eq!(reader.read_until(3).unwrap(), [4, 5, 6, 7]);
        assert!(reader.read(&mut buf).is_err());
    }

    #[test]
    fn test_read_char() {
        let b = b"Vi\xE1\xBB\x87t";
        let mut r = BufReader::new(b);
        assert_eq!(r.read_char(), Ok('V'));
        assert_eq!(r.read_char(), Ok('i'));
        assert_eq!(r.read_char(), Ok('ệ'));
        assert_eq!(r.read_char(), Ok('t'));
        assert!(r.read_char().is_err());
    }

    #[test]
    fn test_read_bad_char() {
        let b = b"\x80";
        let mut r = BufReader::new(b);
        assert!(r.read_char().is_err());
    }

    #[test]
    fn test_write_strings() {
        let mut writer = MemWriter::new();
        writer.write_str("testing").unwrap();
        writer.write_line("testing").unwrap();
        writer.write_str("testing").unwrap();
        let mut r = BufReader::new(writer.get_ref());
        assert_eq!(r.read_to_string().unwrap(), "testingtesting\ntesting");
    }

    #[test]
    fn test_write_char() {
        let mut writer = MemWriter::new();
        writer.write_char('a').unwrap();
        writer.write_char('\n').unwrap();
        writer.write_char('ệ').unwrap();
        let mut r = BufReader::new(writer.get_ref());
        assert_eq!(r.read_to_string().unwrap(), "a\nệ");
    }

    #[test]
    fn test_read_whole_string_bad() {
        let buf = [0xff];
        let mut r = BufReader::new(&buf);
        match r.read_to_string() {
            Ok(..) => panic!(),
            Err(..) => {}
        }
    }

    #[test]
    fn seek_past_end() {
        let buf = [0xff];
        let mut r = BufReader::new(&buf);
        r.seek(10, SeekSet).unwrap();
        assert!(r.read(&mut []).is_err());

        let mut r = MemReader::new(vec!(10));
        r.seek(10, SeekSet).unwrap();
        assert!(r.read(&mut []).is_err());

        let mut buf = [0];
        let mut r = BufWriter::new(&mut buf);
        r.seek(10, SeekSet).unwrap();
        assert!(r.write(&[3]).is_err());
    }

    #[test]
    fn seek_before_0() {
        let buf = [0xff];
        let mut r = BufReader::new(&buf);
        assert!(r.seek(-1, SeekSet).is_err());

        let mut r = MemReader::new(vec!(10));
        assert!(r.seek(-1, SeekSet).is_err());

        let mut buf = [0];
        let mut r = BufWriter::new(&mut buf);
        assert!(r.seek(-1, SeekSet).is_err());
    }

    #[test]
    fn io_read_at_least() {
        let mut r = MemReader::new(vec![1, 2, 3, 4, 5, 6, 7, 8]);
        let mut buf = [0; 3];
        assert!(r.read_at_least(buf.len(), &mut buf).is_ok());
        let b: &[_] = &[1, 2, 3];
        assert_eq!(buf, b);
        assert!(r.read_at_least(0, &mut buf[..0]).is_ok());
        assert_eq!(buf, b);
        assert!(r.read_at_least(buf.len(), &mut buf).is_ok());
        let b: &[_] = &[4, 5, 6];
        assert_eq!(buf, b);
        assert!(r.read_at_least(buf.len(), &mut buf).is_err());
        let b: &[_] = &[7, 8, 6];
        assert_eq!(buf, b);
    }

    fn do_bench_mem_writer(b: &mut Bencher, times: uint, len: uint) {
        let src: Vec<u8> = repeat(5).take(len).collect();

        b.bytes = (times * len) as u64;
        b.iter(|| {
            let mut wr = MemWriter::new();
            for _ in 0..times {
                wr.write(&src).unwrap();
            }

            let v = wr.into_inner();
            assert_eq!(v.len(), times * len);
            assert!(v.iter().all(|x| *x == 5));
        });
    }

    #[bench]
    fn bench_mem_writer_001_0000(b: &mut Bencher) {
        do_bench_mem_writer(b, 1, 0)
    }

    #[bench]
    fn bench_mem_writer_001_0010(b: &mut Bencher) {
        do_bench_mem_writer(b, 1, 10)
    }

    #[bench]
    fn bench_mem_writer_001_0100(b: &mut Bencher) {
        do_bench_mem_writer(b, 1, 100)
    }

    #[bench]
    fn bench_mem_writer_001_1000(b: &mut Bencher) {
        do_bench_mem_writer(b, 1, 1000)
    }

    #[bench]
    fn bench_mem_writer_100_0000(b: &mut Bencher) {
        do_bench_mem_writer(b, 100, 0)
    }

    #[bench]
    fn bench_mem_writer_100_0010(b: &mut Bencher) {
        do_bench_mem_writer(b, 100, 10)
    }

    #[bench]
    fn bench_mem_writer_100_0100(b: &mut Bencher) {
        do_bench_mem_writer(b, 100, 100)
    }

    #[bench]
    fn bench_mem_writer_100_1000(b: &mut Bencher) {
        do_bench_mem_writer(b, 100, 1000)
    }

    #[bench]
    fn bench_mem_reader(b: &mut Bencher) {
        b.iter(|| {
            let buf = [5 as u8; 100].to_vec();
            {
                let mut rdr = MemReader::new(buf);
                for _i in 0..10 {
                    let mut buf = [0 as u8; 10];
                    rdr.read(&mut buf).unwrap();
                    assert_eq!(buf, [5; 10]);
                }
            }
        });
    }

    #[bench]
    fn bench_buf_writer(b: &mut Bencher) {
        b.iter(|| {
            let mut buf = [0 as u8; 100];
            {
                let mut wr = BufWriter::new(&mut buf);
                for _i in 0..10 {
                    wr.write(&[5; 10]).unwrap();
                }
            }
            assert_eq!(buf.as_slice(), [5; 100].as_slice());
        });
    }

    #[bench]
    fn bench_buf_reader(b: &mut Bencher) {
        b.iter(|| {
            let buf = [5 as u8; 100];
            {
                let mut rdr = BufReader::new(&buf);
                for _i in 0..10 {
                    let mut buf = [0 as u8; 10];
                    rdr.read(&mut buf).unwrap();
                    assert_eq!(buf, [5; 10]);
                }
            }
        });
    }
}
