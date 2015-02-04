// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#![allow(missing_copy_implementations)]

use prelude::v1::*;

use io::{self, Read, Write, ErrorKind};

/// Copies the entire contents of a reader into a writer.
///
/// This function will continuously read data from `r` and then write it into
/// `w` in a streaming fashion until `r` returns EOF.
///
/// On success the total number of bytes that were copied from `r` to `w` is
/// returned.
///
/// # Errors
///
/// This function will return an error immediately if any call to `read` or
/// `write` returns an error. All instances of `ErrorKind::Interrupted` are
/// handled by this function and the underlying operation is retried.
pub fn copy<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64> {
    let mut buf = [0; super::DEFAULT_BUF_SIZE];
    let mut written = 0;
    loop {
        let len = match r.read(&mut buf) {
            Ok(0) => return Ok(written),
            Ok(len) => len,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        try!(w.write_all(&buf[..len]));
        written += len as u64;
    }
}

/// A reader which is always at EOF.
pub struct Empty { _priv: () }

/// Creates an instance of an empty reader.
///
/// All reads from the returned reader will return `Ok(0)`.
pub fn empty() -> Empty { Empty { _priv: () } }

impl Read for Empty {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> { Ok(0) }
}

/// A reader which infinitely yields one byte.
pub struct Repeat { byte: u8 }

/// Creates an instance of a reader that infinitely repeats one byte.
///
/// All reads from this reader will succeed by filling the specified buffer with
/// the given byte.
pub fn repeat(byte: u8) -> Repeat { Repeat { byte: byte } }

impl Read for Repeat {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        for slot in buf.iter_mut() {
            *slot = self.byte;
        }
        Ok(buf.len())
    }
}

/// A writer which will move data into the void.
pub struct Sink { _priv: () }

/// Creates an instance of a writer which will successfully consume all data.
///
/// All calls to `write` on the returned instance will return `Ok(buf.len())`
/// and the contents of the buffer will not be inspected.
pub fn sink() -> Sink { Sink { _priv: () } }

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> { Ok(buf.len()) }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

#[cfg(test)]
mod test {
    use prelude::v1::*;

    use io::prelude::*;
    use io::{sink, empty, repeat};

    #[test]
    fn sink_sinks() {
        let mut s = sink();
        assert_eq!(s.write(&[]), Ok(0));
        assert_eq!(s.write(&[0]), Ok(1));
        assert_eq!(s.write(&[0; 1024]), Ok(1024));
        assert_eq!(s.by_ref().write(&[0; 1024]), Ok(1024));
    }

    #[test]
    fn empty_reads() {
        let mut e = empty();
        assert_eq!(e.read(&mut []), Ok(0));
        assert_eq!(e.read(&mut [0]), Ok(0));
        assert_eq!(e.read(&mut [0; 1024]), Ok(0));
        assert_eq!(e.by_ref().read(&mut [0; 1024]), Ok(0));
    }

    #[test]
    fn repeat_repeats() {
        let mut r = repeat(4);
        let mut b = [0; 1024];
        assert_eq!(r.read(&mut b), Ok(1024));
        assert!(b.iter().all(|b| *b == 4));
    }

    #[test]
    fn take_some_bytes() {
        assert_eq!(repeat(4).take(100).bytes().count(), 100);
        assert_eq!(repeat(4).take(100).bytes().next(), Some(Ok(4)));
        assert_eq!(repeat(1).take(10).chain(repeat(2).take(10)).bytes().count(), 20);
    }

    #[test]
    fn tee() {
        let mut buf = [0; 10];
        {
            let mut ptr: &mut [u8] = &mut buf;
            assert_eq!(repeat(4).tee(&mut ptr).take(5).read(&mut [0; 10]), Ok(5));
        }
        assert_eq!(buf, [4, 4, 4, 4, 4, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn broadcast() {
        let mut buf1 = [0; 10];
        let mut buf2 = [0; 10];
        {
            let mut ptr1: &mut [u8] = &mut buf1;
            let mut ptr2: &mut [u8] = &mut buf2;

            assert_eq!((&mut ptr1).broadcast(&mut ptr2)
                                  .write(&[1, 2, 3]), Ok(3));
        }
        assert_eq!(buf1, buf2);
        assert_eq!(buf1, [1, 2, 3, 0, 0, 0, 0, 0, 0, 0]);
    }
}
