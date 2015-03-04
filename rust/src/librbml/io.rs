// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::old_io::{IoError, IoResult, SeekStyle};
use std::old_io;
use std::slice;
use std::iter::repeat;

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

/// Writes to an owned, growable byte vector that supports seeking.
///
/// # Example
///
/// ```rust
/// # #![allow(unused_must_use)]
/// use rbml::io::SeekableMemWriter;
///
/// let mut w = SeekableMemWriter::new();
/// w.write(&[0, 1, 2]);
///
/// assert_eq!(w.unwrap(), [0, 1, 2]);
/// ```
pub struct SeekableMemWriter {
    buf: Vec<u8>,
    pos: uint,
}

impl SeekableMemWriter {
    /// Create a new `SeekableMemWriter`.
    #[inline]
    pub fn new() -> SeekableMemWriter {
        SeekableMemWriter::with_capacity(BUF_CAPACITY)
    }
    /// Create a new `SeekableMemWriter`, allocating at least `n` bytes for
    /// the internal buffer.
    #[inline]
    pub fn with_capacity(n: uint) -> SeekableMemWriter {
        SeekableMemWriter { buf: Vec::with_capacity(n), pos: 0 }
    }

    /// Acquires an immutable reference to the underlying buffer of this
    /// `SeekableMemWriter`.
    ///
    /// No method is exposed for acquiring a mutable reference to the buffer
    /// because it could corrupt the state of this `MemWriter`.
    #[inline]
    pub fn get_ref<'a>(&'a self) -> &'a [u8] { &self.buf }

    /// Unwraps this `SeekableMemWriter`, returning the underlying buffer
    #[inline]
    pub fn unwrap(self) -> Vec<u8> { self.buf }
}

impl Writer for SeekableMemWriter {
    #[inline]
    fn write_all(&mut self, buf: &[u8]) -> IoResult<()> {
        if self.pos == self.buf.len() {
            self.buf.push_all(buf)
        } else {
            // Make sure the internal buffer is as least as big as where we
            // currently are
            let difference = self.pos as i64 - self.buf.len() as i64;
            if difference > 0 {
                self.buf.extend(repeat(0).take(difference as uint));
            }

            // Figure out what bytes will be used to overwrite what's currently
            // there (left), and what will be appended on the end (right)
            let cap = self.buf.len() - self.pos;
            let (left, right) = if cap <= buf.len() {
                (&buf[..cap], &buf[cap..])
            } else {
                let result: (_, &[_]) = (buf, &[]);
                result
            };

            // Do the necessary writes
            if left.len() > 0 {
                slice::bytes::copy_memory(&mut self.buf[self.pos..], left);
            }
            if right.len() > 0 {
                self.buf.push_all(right);
            }
        }

        // Bump us forward
        self.pos += buf.len();
        Ok(())
    }
}

impl Seek for SeekableMemWriter {
    #[inline]
    fn tell(&self) -> IoResult<u64> { Ok(self.pos as u64) }

    #[inline]
    fn seek(&mut self, pos: i64, style: SeekStyle) -> IoResult<()> {
        let new = try!(combine(style, self.pos, self.buf.len(), pos));
        self.pos = new as uint;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate test;
    use super::SeekableMemWriter;
    use std::old_io;
    use std::iter::repeat;
    use test::Bencher;

    #[test]
    fn test_seekable_mem_writer() {
        let mut writer = SeekableMemWriter::new();
        assert_eq!(writer.tell(), Ok(0));
        writer.write_all(&[0]).unwrap();
        assert_eq!(writer.tell(), Ok(1));
        writer.write_all(&[1, 2, 3]).unwrap();
        writer.write_all(&[4, 5, 6, 7]).unwrap();
        assert_eq!(writer.tell(), Ok(8));
        let b: &[_] = &[0, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(writer.get_ref(), b);

        writer.seek(0, old_io::SeekSet).unwrap();
        assert_eq!(writer.tell(), Ok(0));
        writer.write_all(&[3, 4]).unwrap();
        let b: &[_] = &[3, 4, 2, 3, 4, 5, 6, 7];
        assert_eq!(writer.get_ref(), b);

        writer.seek(1, old_io::SeekCur).unwrap();
        writer.write_all(&[0, 1]).unwrap();
        let b: &[_] = &[3, 4, 2, 0, 1, 5, 6, 7];
        assert_eq!(writer.get_ref(), b);

        writer.seek(-1, old_io::SeekEnd).unwrap();
        writer.write_all(&[1, 2]).unwrap();
        let b: &[_] = &[3, 4, 2, 0, 1, 5, 6, 1, 2];
        assert_eq!(writer.get_ref(), b);

        writer.seek(1, old_io::SeekEnd).unwrap();
        writer.write_all(&[1]).unwrap();
        let b: &[_] = &[3, 4, 2, 0, 1, 5, 6, 1, 2, 0, 1];
        assert_eq!(writer.get_ref(), b);
    }

    #[test]
    fn seek_past_end() {
        let mut r = SeekableMemWriter::new();
        r.seek(10, old_io::SeekSet).unwrap();
        assert!(r.write_all(&[3]).is_ok());
    }

    #[test]
    fn seek_before_0() {
        let mut r = SeekableMemWriter::new();
        assert!(r.seek(-1, old_io::SeekSet).is_err());
    }

    fn do_bench_seekable_mem_writer(b: &mut Bencher, times: uint, len: uint) {
        let src: Vec<u8> = repeat(5).take(len).collect();

        b.bytes = (times * len) as u64;
        b.iter(|| {
            let mut wr = SeekableMemWriter::new();
            for _ in 0..times {
                wr.write_all(&src).unwrap();
            }

            let v = wr.unwrap();
            assert_eq!(v.len(), times * len);
            assert!(v.iter().all(|x| *x == 5));
        });
    }

    #[bench]
    fn bench_seekable_mem_writer_001_0000(b: &mut Bencher) {
        do_bench_seekable_mem_writer(b, 1, 0)
    }

    #[bench]
    fn bench_seekable_mem_writer_001_0010(b: &mut Bencher) {
        do_bench_seekable_mem_writer(b, 1, 10)
    }

    #[bench]
    fn bench_seekable_mem_writer_001_0100(b: &mut Bencher) {
        do_bench_seekable_mem_writer(b, 1, 100)
    }

    #[bench]
    fn bench_seekable_mem_writer_001_1000(b: &mut Bencher) {
        do_bench_seekable_mem_writer(b, 1, 1000)
    }

    #[bench]
    fn bench_seekable_mem_writer_100_0000(b: &mut Bencher) {
        do_bench_seekable_mem_writer(b, 100, 0)
    }

    #[bench]
    fn bench_seekable_mem_writer_100_0010(b: &mut Bencher) {
        do_bench_seekable_mem_writer(b, 100, 10)
    }

    #[bench]
    fn bench_seekable_mem_writer_100_0100(b: &mut Bencher) {
        do_bench_seekable_mem_writer(b, 100, 100)
    }

    #[bench]
    fn bench_seekable_mem_writer_100_1000(b: &mut Bencher) {
        do_bench_seekable_mem_writer(b, 100, 1000)
    }
}
