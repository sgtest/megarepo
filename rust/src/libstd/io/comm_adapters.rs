// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use clone::Clone;
use cmp;
use sync::mpsc::{Sender, Receiver};
use io;
use option::Option::{None, Some};
use result::Result::{Ok, Err};
use slice::{bytes, SliceExt};
use super::{Buffer, Reader, Writer, IoResult};
use vec::Vec;

/// Allows reading from a rx.
///
/// # Example
///
/// ```
/// use std::sync::mpsc::channel;
/// use std::io::ChanReader;
///
/// let (tx, rx) = channel();
/// # drop(tx);
/// let mut reader = ChanReader::new(rx);
///
/// let mut buf = [0u8; 100];
/// match reader.read(&mut buf) {
///     Ok(nread) => println!("Read {} bytes", nread),
///     Err(e) => println!("read error: {}", e),
/// }
/// ```
pub struct ChanReader {
    buf: Vec<u8>,          // A buffer of bytes received but not consumed.
    pos: uint,             // How many of the buffered bytes have already be consumed.
    rx: Receiver<Vec<u8>>, // The Receiver to pull data from.
    closed: bool,          // Whether the channel this Receiver connects to has been closed.
}

impl ChanReader {
    /// Wraps a `Port` in a `ChanReader` structure
    pub fn new(rx: Receiver<Vec<u8>>) -> ChanReader {
        ChanReader {
            buf: Vec::new(),
            pos: 0,
            rx: rx,
            closed: false,
        }
    }
}

impl Buffer for ChanReader {
    fn fill_buf<'a>(&'a mut self) -> IoResult<&'a [u8]> {
        if self.pos >= self.buf.len() {
            self.pos = 0;
            match self.rx.recv() {
                Ok(bytes) => {
                    self.buf = bytes;
                },
                Err(..) => {
                    self.closed = true;
                    self.buf = Vec::new();
                }
            }
        }
        if self.closed {
            Err(io::standard_error(io::EndOfFile))
        } else {
            Ok(self.buf.slice_from(self.pos))
        }
    }

    fn consume(&mut self, amt: uint) {
        self.pos += amt;
        assert!(self.pos <= self.buf.len());
    }
}

impl Reader for ChanReader {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        let mut num_read = 0;
        loop {
            let count = match self.fill_buf().ok() {
                Some(src) => {
                    let dst = buf.slice_from_mut(num_read);
                    let count = cmp::min(src.len(), dst.len());
                    bytes::copy_memory(dst, &src[0..count]);
                    count
                },
                None => 0,
            };
            self.consume(count);
            num_read += count;
            if num_read == buf.len() || self.closed {
                break;
            }
        }
        if self.closed && num_read == 0 {
            Err(io::standard_error(io::EndOfFile))
        } else {
            Ok(num_read)
        }
    }
}

/// Allows writing to a tx.
///
/// # Example
///
/// ```
/// # #![allow(unused_must_use)]
/// use std::sync::mpsc::channel;
/// use std::io::ChanWriter;
///
/// let (tx, rx) = channel();
/// # drop(rx);
/// let mut writer = ChanWriter::new(tx);
/// writer.write("hello, world".as_bytes());
/// ```
pub struct ChanWriter {
    tx: Sender<Vec<u8>>,
}

impl ChanWriter {
    /// Wraps a channel in a `ChanWriter` structure
    pub fn new(tx: Sender<Vec<u8>>) -> ChanWriter {
        ChanWriter { tx: tx }
    }
}

#[stable]
impl Clone for ChanWriter {
    fn clone(&self) -> ChanWriter {
        ChanWriter { tx: self.tx.clone() }
    }
}

impl Writer for ChanWriter {
    fn write(&mut self, buf: &[u8]) -> IoResult<()> {
        self.tx.send(buf.to_vec()).map_err(|_| {
            io::IoError {
                kind: io::BrokenPipe,
                desc: "Pipe closed",
                detail: None
            }
        })
    }
}


#[cfg(test)]
mod test {
    use prelude::v1::*;

    use sync::mpsc::channel;
    use super::*;
    use io;
    use thread::Thread;

    #[test]
    fn test_rx_reader() {
        let (tx, rx) = channel();
        Thread::spawn(move|| {
          tx.send(vec![1u8, 2u8]).unwrap();
          tx.send(vec![]).unwrap();
          tx.send(vec![3u8, 4u8]).unwrap();
          tx.send(vec![5u8, 6u8]).unwrap();
          tx.send(vec![7u8, 8u8]).unwrap();
        });

        let mut reader = ChanReader::new(rx);
        let mut buf = [0u8; 3];

        assert_eq!(Ok(0), reader.read(&mut []));

        assert_eq!(Ok(3), reader.read(&mut buf));
        let a: &[u8] = &[1,2,3];
        assert_eq!(a, buf);

        assert_eq!(Ok(3), reader.read(&mut buf));
        let a: &[u8] = &[4,5,6];
        assert_eq!(a, buf);

        assert_eq!(Ok(2), reader.read(&mut buf));
        let a: &[u8] = &[7,8,6];
        assert_eq!(a, buf);

        match reader.read(buf.as_mut_slice()) {
            Ok(..) => panic!(),
            Err(e) => assert_eq!(e.kind, io::EndOfFile),
        }
        assert_eq!(a, buf);

        // Ensure it continues to panic in the same way.
        match reader.read(buf.as_mut_slice()) {
            Ok(..) => panic!(),
            Err(e) => assert_eq!(e.kind, io::EndOfFile),
        }
        assert_eq!(a, buf);
    }

    #[test]
    fn test_rx_buffer() {
        let (tx, rx) = channel();
        Thread::spawn(move|| {
          tx.send(b"he".to_vec()).unwrap();
          tx.send(b"llo wo".to_vec()).unwrap();
          tx.send(b"".to_vec()).unwrap();
          tx.send(b"rld\nhow ".to_vec()).unwrap();
          tx.send(b"are you?".to_vec()).unwrap();
          tx.send(b"".to_vec()).unwrap();
        });

        let mut reader = ChanReader::new(rx);

        assert_eq!(Ok("hello world\n".to_string()), reader.read_line());
        assert_eq!(Ok("how are you?".to_string()), reader.read_line());
        match reader.read_line() {
            Ok(..) => panic!(),
            Err(e) => assert_eq!(e.kind, io::EndOfFile),
        }
    }

    #[test]
    fn test_chan_writer() {
        let (tx, rx) = channel();
        let mut writer = ChanWriter::new(tx);
        writer.write_be_u32(42).unwrap();

        let wanted = vec![0u8, 0u8, 0u8, 42u8];
        let got = match Thread::scoped(move|| { rx.recv().unwrap() }).join() {
            Ok(got) => got,
            Err(_) => panic!(),
        };
        assert_eq!(wanted, got);

        match writer.write_u8(1) {
            Ok(..) => panic!(),
            Err(e) => assert_eq!(e.kind, io::BrokenPipe),
        }
    }
}
