// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::prelude::*;

use boxed::Box;
use cmp;
use io::{self, SeekFrom, Read, Write, Seek, BufRead, Error, ErrorKind};
use fmt;
use mem;
use slice;
use string::String;
use vec::Vec;

// =============================================================================
// Forwarding implementations

impl<'a, R: Read + ?Sized> Read for &'a mut R {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> { (**self).read(buf) }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<()> { (**self).read_to_end(buf) }

    fn read_to_string(&mut self, buf: &mut String) -> io::Result<()> {
        (**self).read_to_string(buf)
    }
}
impl<'a, W: Write + ?Sized> Write for &'a mut W {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> { (**self).write(buf) }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> { (**self).write_all(buf) }

    fn write_fmt(&mut self, fmt: fmt::Arguments) -> io::Result<()> { (**self).write_fmt(fmt) }

    fn flush(&mut self) -> io::Result<()> { (**self).flush() }
}
impl<'a, S: Seek + ?Sized> Seek for &'a mut S {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> { (**self).seek(pos) }
}
impl<'a, B: BufRead + ?Sized> BufRead for &'a mut B {
    fn fill_buf(&mut self) -> io::Result<&[u8]> { (**self).fill_buf() }

    fn consume(&mut self, amt: usize) { (**self).consume(amt) }

    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> io::Result<()> {
        (**self).read_until(byte, buf)
    }

    fn read_line(&mut self, buf: &mut String) -> io::Result<()> { (**self).read_line(buf) }
}

impl<R: Read + ?Sized> Read for Box<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> { (**self).read(buf) }
}
impl<W: Write + ?Sized> Write for Box<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> { (**self).write(buf) }
    fn flush(&mut self) -> io::Result<()> { (**self).flush() }
}
impl<S: Seek + ?Sized> Seek for Box<S> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> { (**self).seek(pos) }
}
impl<B: BufRead + ?Sized> BufRead for Box<B> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> { (**self).fill_buf() }
    fn consume(&mut self, amt: usize) { (**self).consume(amt) }
}

// =============================================================================
// In-memory buffer implementations

impl<'a> Read for &'a [u8] {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let amt = cmp::min(buf.len(), self.len());
        let (a, b) = self.split_at(amt);
        slice::bytes::copy_memory(buf, a);
        *self = b;
        Ok(amt)
    }
}

impl<'a> BufRead for &'a [u8] {
    fn fill_buf(&mut self) -> io::Result<&[u8]> { Ok(*self) }
    fn consume(&mut self, amt: usize) { *self = &self[amt..]; }
}

impl<'a> Write for &'a mut [u8] {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let amt = cmp::min(data.len(), self.len());
        let (a, b) = mem::replace(self, &mut []).split_at_mut(amt);
        slice::bytes::copy_memory(a, &data[..amt]);
        *self = b;
        Ok(amt)
    }

    fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        if try!(self.write(data)) == data.len() {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::WriteZero, "failed to write whole buffer", None))
        }
    }

    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

impl Write for Vec<u8> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.push_all(buf);
        Ok(buf.len())
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        try!(self.write(buf));
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}
