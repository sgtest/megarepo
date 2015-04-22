// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! This module implements only the Sha256 function since that is all that is needed for internal
//! use. This implementation is not intended for external use or for any use where security is
//! important.

use std::iter::repeat;
use std::slice::bytes::{MutableByteVector, copy_memory};
use serialize::hex::ToHex;

/// Write a u32 into a vector, which must be 4 bytes long. The value is written in big-endian
/// format.
fn write_u32_be(dst: &mut[u8], input: u32) {
    dst[0] = (input >> 24) as u8;
    dst[1] = (input >> 16) as u8;
    dst[2] = (input >> 8) as u8;
    dst[3] = input as u8;
}

/// Read the value of a vector of bytes as a u32 value in big-endian format.
fn read_u32_be(input: &[u8]) -> u32 {
    return
        (input[0] as u32) << 24 |
        (input[1] as u32) << 16 |
        (input[2] as u32) << 8 |
        (input[3] as u32);
}

/// Read a vector of bytes into a vector of u32s. The values are read in big-endian format.
fn read_u32v_be(dst: &mut[u32], input: &[u8]) {
    assert!(dst.len() * 4 == input.len());
    let mut pos = 0;
    for chunk in input.chunks(4) {
        dst[pos] = read_u32_be(chunk);
        pos += 1;
    }
}

trait ToBits {
    /// Convert the value in bytes to the number of bits, a tuple where the 1st item is the
    /// high-order value and the 2nd item is the low order value.
    fn to_bits(self) -> (Self, Self);
}

impl ToBits for u64 {
    fn to_bits(self) -> (u64, u64) {
        return (self >> 61, self << 3);
    }
}

/// Adds the specified number of bytes to the bit count. panic!() if this would cause numeric
/// overflow.
fn add_bytes_to_bits(bits: u64, bytes: u64) -> u64 {
    let (new_high_bits, new_low_bits) = bytes.to_bits();

    if new_high_bits > 0 {
        panic!("numeric overflow occurred.")
    }

    match bits.checked_add(new_low_bits) {
        Some(x) => return x,
        None => panic!("numeric overflow occurred.")
    }
}

/// A FixedBuffer, likes its name implies, is a fixed size buffer. When the buffer becomes full, it
/// must be processed. The input() method takes care of processing and then clearing the buffer
/// automatically. However, other methods do not and require the caller to process the buffer. Any
/// method that modifies the buffer directory or provides the caller with bytes that can be modified
/// results in those bytes being marked as used by the buffer.
trait FixedBuffer {
    /// Input a vector of bytes. If the buffer becomes full, process it with the provided
    /// function and then clear the buffer.
    fn input<F>(&mut self, input: &[u8], func: F) where
        F: FnMut(&[u8]);

    /// Reset the buffer.
    fn reset(&mut self);

    /// Zero the buffer up until the specified index. The buffer position currently must not be
    /// greater than that index.
    fn zero_until(&mut self, idx: usize);

    /// Get a slice of the buffer of the specified size. There must be at least that many bytes
    /// remaining in the buffer.
    fn next<'s>(&'s mut self, len: usize) -> &'s mut [u8];

    /// Get the current buffer. The buffer must already be full. This clears the buffer as well.
    fn full_buffer<'s>(&'s mut self) -> &'s [u8];

    /// Get the current position of the buffer.
    fn position(&self) -> usize;

    /// Get the number of bytes remaining in the buffer until it is full.
    fn remaining(&self) -> usize;

    /// Get the size of the buffer
    fn size(&self) -> usize;
}

/// A FixedBuffer of 64 bytes useful for implementing Sha256 which has a 64 byte blocksize.
struct FixedBuffer64 {
    buffer: [u8; 64],
    buffer_idx: usize,
}

impl FixedBuffer64 {
    /// Create a new FixedBuffer64
    fn new() -> FixedBuffer64 {
        return FixedBuffer64 {
            buffer: [0; 64],
            buffer_idx: 0
        };
    }
}

impl FixedBuffer for FixedBuffer64 {
    fn input<F>(&mut self, input: &[u8], mut func: F) where
        F: FnMut(&[u8]),
    {
        let mut i = 0;

        let size = self.size();

        // If there is already data in the buffer, copy as much as we can into it and process
        // the data if the buffer becomes full.
        if self.buffer_idx != 0 {
            let buffer_remaining = size - self.buffer_idx;
            if input.len() >= buffer_remaining {
                    copy_memory(
                        &input[..buffer_remaining],
                        &mut self.buffer[self.buffer_idx..size]);
                self.buffer_idx = 0;
                func(&self.buffer);
                i += buffer_remaining;
            } else {
                copy_memory(
                    input,
                    &mut self.buffer[self.buffer_idx..self.buffer_idx + input.len()]);
                self.buffer_idx += input.len();
                return;
            }
        }

        // While we have at least a full buffer size chunk's worth of data, process that data
        // without copying it into the buffer
        while input.len() - i >= size {
            func(&input[i..i + size]);
            i += size;
        }

        // Copy any input data into the buffer. At this point in the method, the amount of
        // data left in the input vector will be less than the buffer size and the buffer will
        // be empty.
        let input_remaining = input.len() - i;
        copy_memory(
            &input[i..],
            &mut self.buffer[..input_remaining]);
        self.buffer_idx += input_remaining;
    }

    fn reset(&mut self) {
        self.buffer_idx = 0;
    }

    fn zero_until(&mut self, idx: usize) {
        assert!(idx >= self.buffer_idx);
        self.buffer[self.buffer_idx..idx].set_memory(0);
        self.buffer_idx = idx;
    }

    fn next<'s>(&'s mut self, len: usize) -> &'s mut [u8] {
        self.buffer_idx += len;
        return &mut self.buffer[self.buffer_idx - len..self.buffer_idx];
    }

    fn full_buffer<'s>(&'s mut self) -> &'s [u8] {
        assert!(self.buffer_idx == 64);
        self.buffer_idx = 0;
        return &self.buffer[..64];
    }

    fn position(&self) -> usize { self.buffer_idx }

    fn remaining(&self) -> usize { 64 - self.buffer_idx }

    fn size(&self) -> usize { 64 }
}

/// The StandardPadding trait adds a method useful for Sha256 to a FixedBuffer struct.
trait StandardPadding {
    /// Add padding to the buffer. The buffer must not be full when this method is called and is
    /// guaranteed to have exactly rem remaining bytes when it returns. If there are not at least
    /// rem bytes available, the buffer will be zero padded, processed, cleared, and then filled
    /// with zeros again until only rem bytes are remaining.
    fn standard_padding<F>(&mut self, rem: usize, func: F) where F: FnMut(&[u8]);
}

impl <T: FixedBuffer> StandardPadding for T {
    fn standard_padding<F>(&mut self, rem: usize, mut func: F) where F: FnMut(&[u8]) {
        let size = self.size();

        self.next(1)[0] = 128;

        if self.remaining() < rem {
            self.zero_until(size);
            func(self.full_buffer());
        }

        self.zero_until(size - rem);
    }
}

/// The Digest trait specifies an interface common to digest functions, such as SHA-1 and the SHA-2
/// family of digest functions.
pub trait Digest {
    /// Provide message data.
    ///
    /// # Arguments
    ///
    /// * input - A vector of message data
    fn input(&mut self, input: &[u8]);

    /// Retrieve the digest result. This method may be called multiple times.
    ///
    /// # Arguments
    ///
    /// * out - the vector to hold the result. Must be large enough to contain output_bits().
    fn result(&mut self, out: &mut [u8]);

    /// Reset the digest. This method must be called after result() and before supplying more
    /// data.
    fn reset(&mut self);

    /// Get the output size in bits.
    fn output_bits(&self) -> usize;

    /// Convenience function that feeds a string into a digest.
    ///
    /// # Arguments
    ///
    /// * `input` The string to feed into the digest
    fn input_str(&mut self, input: &str) {
        self.input(input.as_bytes());
    }

    /// Convenience function that retrieves the result of a digest as a
    /// newly allocated vec of bytes.
    fn result_bytes(&mut self) -> Vec<u8> {
        let mut buf: Vec<u8> = repeat(0).take((self.output_bits()+7)/8).collect();
        self.result(&mut buf);
        buf
    }

    /// Convenience function that retrieves the result of a digest as a
    /// String in hexadecimal format.
    fn result_str(&mut self) -> String {
        self.result_bytes().to_hex().to_string()
    }
}

// A structure that represents that state of a digest computation for the SHA-2 512 family of digest
// functions
struct Engine256State {
    h0: u32,
    h1: u32,
    h2: u32,
    h3: u32,
    h4: u32,
    h5: u32,
    h6: u32,
    h7: u32,
}

impl Engine256State {
    fn new(h: &[u32; 8]) -> Engine256State {
        return Engine256State {
            h0: h[0],
            h1: h[1],
            h2: h[2],
            h3: h[3],
            h4: h[4],
            h5: h[5],
            h6: h[6],
            h7: h[7]
        };
    }

    fn reset(&mut self, h: &[u32; 8]) {
        self.h0 = h[0];
        self.h1 = h[1];
        self.h2 = h[2];
        self.h3 = h[3];
        self.h4 = h[4];
        self.h5 = h[5];
        self.h6 = h[6];
        self.h7 = h[7];
    }

    fn process_block(&mut self, data: &[u8]) {
        fn ch(x: u32, y: u32, z: u32) -> u32 {
            ((x & y) ^ ((!x) & z))
        }

        fn maj(x: u32, y: u32, z: u32) -> u32 {
            ((x & y) ^ (x & z) ^ (y & z))
        }

        fn sum0(x: u32) -> u32 {
            ((x >> 2) | (x << 30)) ^ ((x >> 13) | (x << 19)) ^ ((x >> 22) | (x << 10))
        }

        fn sum1(x: u32) -> u32 {
            ((x >> 6) | (x << 26)) ^ ((x >> 11) | (x << 21)) ^ ((x >> 25) | (x << 7))
        }

        fn sigma0(x: u32) -> u32 {
            ((x >> 7) | (x << 25)) ^ ((x >> 18) | (x << 14)) ^ (x >> 3)
        }

        fn sigma1(x: u32) -> u32 {
            ((x >> 17) | (x << 15)) ^ ((x >> 19) | (x << 13)) ^ (x >> 10)
        }

        let mut a = self.h0;
        let mut b = self.h1;
        let mut c = self.h2;
        let mut d = self.h3;
        let mut e = self.h4;
        let mut f = self.h5;
        let mut g = self.h6;
        let mut h = self.h7;

        let mut w = [0; 64];

        // Sha-512 and Sha-256 use basically the same calculations which are implemented
        // by these macros. Inlining the calculations seems to result in better generated code.
        macro_rules! schedule_round { ($t:expr) => (
            w[$t] = sigma1(w[$t - 2]).wrapping_add(w[$t - 7])
                .wrapping_add(sigma0(w[$t - 15])).wrapping_add(w[$t - 16]);
            )
        }

        macro_rules! sha2_round {
            ($A:ident, $B:ident, $C:ident, $D:ident,
             $E:ident, $F:ident, $G:ident, $H:ident, $K:ident, $t:expr) => (
                {
                    $H = $H.wrapping_add(sum1($E)).wrapping_add(ch($E, $F, $G))
                        .wrapping_add($K[$t]).wrapping_add(w[$t]);
                    $D = $D.wrapping_add($H);
                    $H = $H.wrapping_add(sum0($A)).wrapping_add(maj($A, $B, $C));
                }
             )
        }

        read_u32v_be(&mut w[0..16], data);

        // Putting the message schedule inside the same loop as the round calculations allows for
        // the compiler to generate better code.
        for t in (0..48).step_by(8) {
            schedule_round!(t + 16);
            schedule_round!(t + 17);
            schedule_round!(t + 18);
            schedule_round!(t + 19);
            schedule_round!(t + 20);
            schedule_round!(t + 21);
            schedule_round!(t + 22);
            schedule_round!(t + 23);

            sha2_round!(a, b, c, d, e, f, g, h, K32, t);
            sha2_round!(h, a, b, c, d, e, f, g, K32, t + 1);
            sha2_round!(g, h, a, b, c, d, e, f, K32, t + 2);
            sha2_round!(f, g, h, a, b, c, d, e, K32, t + 3);
            sha2_round!(e, f, g, h, a, b, c, d, K32, t + 4);
            sha2_round!(d, e, f, g, h, a, b, c, K32, t + 5);
            sha2_round!(c, d, e, f, g, h, a, b, K32, t + 6);
            sha2_round!(b, c, d, e, f, g, h, a, K32, t + 7);
        }

        for t in (48..64).step_by(8) {
            sha2_round!(a, b, c, d, e, f, g, h, K32, t);
            sha2_round!(h, a, b, c, d, e, f, g, K32, t + 1);
            sha2_round!(g, h, a, b, c, d, e, f, K32, t + 2);
            sha2_round!(f, g, h, a, b, c, d, e, K32, t + 3);
            sha2_round!(e, f, g, h, a, b, c, d, K32, t + 4);
            sha2_round!(d, e, f, g, h, a, b, c, K32, t + 5);
            sha2_round!(c, d, e, f, g, h, a, b, K32, t + 6);
            sha2_round!(b, c, d, e, f, g, h, a, K32, t + 7);
        }

        self.h0 = self.h0.wrapping_add(a);
        self.h1 = self.h1.wrapping_add(b);
        self.h2 = self.h2.wrapping_add(c);
        self.h3 = self.h3.wrapping_add(d);
        self.h4 = self.h4.wrapping_add(e);
        self.h5 = self.h5.wrapping_add(f);
        self.h6 = self.h6.wrapping_add(g);
        self.h7 = self.h7.wrapping_add(h);
    }
}

static K32: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
];

// A structure that keeps track of the state of the Sha-256 operation and contains the logic
// necessary to perform the final calculations.
struct Engine256 {
    length_bits: u64,
    buffer: FixedBuffer64,
    state: Engine256State,
    finished: bool,
}

impl Engine256 {
    fn new(h: &[u32; 8]) -> Engine256 {
        return Engine256 {
            length_bits: 0,
            buffer: FixedBuffer64::new(),
            state: Engine256State::new(h),
            finished: false
        }
    }

    fn reset(&mut self, h: &[u32; 8]) {
        self.length_bits = 0;
        self.buffer.reset();
        self.state.reset(h);
        self.finished = false;
    }

    fn input(&mut self, input: &[u8]) {
        assert!(!self.finished);
        // Assumes that input.len() can be converted to u64 without overflow
        self.length_bits = add_bytes_to_bits(self.length_bits, input.len() as u64);
        let self_state = &mut self.state;
        self.buffer.input(input, |input: &[u8]| { self_state.process_block(input) });
    }

    fn finish(&mut self) {
        if self.finished {
            return;
        }

        let self_state = &mut self.state;
        self.buffer.standard_padding(8, |input: &[u8]| { self_state.process_block(input) });
        write_u32_be(self.buffer.next(4), (self.length_bits >> 32) as u32 );
        write_u32_be(self.buffer.next(4), self.length_bits as u32);
        self_state.process_block(self.buffer.full_buffer());

        self.finished = true;
    }
}

/// The SHA-256 hash algorithm
pub struct Sha256 {
    engine: Engine256
}

impl Sha256 {
    /// Construct a new instance of a SHA-256 digest.
    pub fn new() -> Sha256 {
        Sha256 {
            engine: Engine256::new(&H256)
        }
    }
}

impl Digest for Sha256 {
    fn input(&mut self, d: &[u8]) {
        self.engine.input(d);
    }

    fn result(&mut self, out: &mut [u8]) {
        self.engine.finish();

        write_u32_be(&mut out[0..4], self.engine.state.h0);
        write_u32_be(&mut out[4..8], self.engine.state.h1);
        write_u32_be(&mut out[8..12], self.engine.state.h2);
        write_u32_be(&mut out[12..16], self.engine.state.h3);
        write_u32_be(&mut out[16..20], self.engine.state.h4);
        write_u32_be(&mut out[20..24], self.engine.state.h5);
        write_u32_be(&mut out[24..28], self.engine.state.h6);
        write_u32_be(&mut out[28..32], self.engine.state.h7);
    }

    fn reset(&mut self) {
        self.engine.reset(&H256);
    }

    fn output_bits(&self) -> usize { 256 }
}

static H256: [u32; 8] = [
    0x6a09e667,
    0xbb67ae85,
    0x3c6ef372,
    0xa54ff53a,
    0x510e527f,
    0x9b05688c,
    0x1f83d9ab,
    0x5be0cd19
];

#[cfg(test)]
mod tests {
    #![allow(deprecated)]
    extern crate rand;

    use self::rand::Rng;
    use self::rand::isaac::IsaacRng;
    use serialize::hex::FromHex;
    use std::iter::repeat;
    use std::u64;
    use super::{Digest, Sha256, FixedBuffer};

    // A normal addition - no overflow occurs
    #[test]
    fn test_add_bytes_to_bits_ok() {
        assert!(super::add_bytes_to_bits(100, 10) == 180);
    }

    // A simple failure case - adding 1 to the max value
    #[test]
    #[should_panic]
    fn test_add_bytes_to_bits_overflow() {
        super::add_bytes_to_bits(u64::MAX, 1);
    }

    struct Test {
        input: String,
        output_str: String,
    }

    fn test_hash<D: Digest>(sh: &mut D, tests: &[Test]) {
        // Test that it works when accepting the message all at once
        for t in tests {
            sh.reset();
            sh.input_str(&t.input);
            let out_str = sh.result_str();
            assert!(out_str == t.output_str);
        }

        // Test that it works when accepting the message in pieces
        for t in tests {
            sh.reset();
            let len = t.input.len();
            let mut left = len;
            while left > 0 {
                let take = (left + 1) / 2;
                sh.input_str(&t.input[len - left..take + len - left]);
                left = left - take;
            }
            let out_str = sh.result_str();
            assert!(out_str == t.output_str);
        }
    }

    #[test]
    fn test_sha256() {
        // Examples from wikipedia
        let wikipedia_tests = vec!(
            Test {
                input: "".to_string(),
                output_str: "e3b0c44298fc1c149afb\
            f4c8996fb92427ae41e4649b934ca495991b7852b855".to_string()
            },
            Test {
                input: "The quick brown fox jumps over the lazy \
                        dog".to_string(),
                output_str: "d7a8fbb307d7809469ca\
            9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592".to_string()
            },
            Test {
                input: "The quick brown fox jumps over the lazy \
                        dog.".to_string(),
                output_str: "ef537f25c895bfa78252\
            6529a9b63d97aa631564d5d789c2b765448c8635fb6c".to_string()
            });

        let tests = wikipedia_tests;

        let mut sh: Box<_> = box Sha256::new();

        test_hash(&mut *sh, &tests);
    }

    /// Feed 1,000,000 'a's into the digest with varying input sizes and check that the result is
    /// correct.
    fn test_digest_1million_random<D: Digest>(digest: &mut D, blocksize: usize, expected: &str) {
        let total_size = 1000000;
        let buffer: Vec<u8> = repeat('a' as u8).take(blocksize * 2).collect();
        let mut rng = IsaacRng::new_unseeded();
        let mut count = 0;

        digest.reset();

        while count < total_size {
            let next: usize = rng.gen_range(0, 2 * blocksize + 1);
            let remaining = total_size - count;
            let size = if next > remaining { remaining } else { next };
            digest.input(&buffer[..size]);
            count += size;
        }

        let result_str = digest.result_str();
        let result_bytes = digest.result_bytes();

        assert_eq!(expected, result_str);

        let expected_vec: Vec<u8> = expected.from_hex()
                                            .unwrap()
                                            .into_iter()
                                            .collect();
        assert_eq!(expected_vec, result_bytes);
    }

    #[test]
    fn test_1million_random_sha256() {
        let mut sh = Sha256::new();
        test_digest_1million_random(
            &mut sh,
            64,
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }
}

#[cfg(test)]
mod bench {
    extern crate test;
    use self::test::Bencher;
    use super::{Sha256, FixedBuffer, Digest};

    #[bench]
    pub fn sha256_10(b: &mut Bencher) {
        let mut sh = Sha256::new();
        let bytes = [1; 10];
        b.iter(|| {
            sh.input(&bytes);
        });
        b.bytes = bytes.len() as u64;
    }

    #[bench]
    pub fn sha256_1k(b: &mut Bencher) {
        let mut sh = Sha256::new();
        let bytes = [1; 1024];
        b.iter(|| {
            sh.input(&bytes);
        });
        b.bytes = bytes.len() as u64;
    }

    #[bench]
    pub fn sha256_64k(b: &mut Bencher) {
        let mut sh = Sha256::new();
        let bytes = [1; 65536];
        b.iter(|| {
            sh.input(&bytes);
        });
        b.bytes = bytes.len() as u64;
    }
}
