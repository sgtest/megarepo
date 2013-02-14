// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!
 * An implementation of the SHA-1 cryptographic hash.
 *
 * First create a `sha1` object using the `sha1` constructor, then
 * feed it input using the `input` or `input_str` methods, which may be
 * called any number of times.
 *
 * After the entire input has been fed to the hash read the result using
 * the `result` or `result_str` methods.
 *
 * The `sha1` object may be reused to create multiple hashes by calling
 * the `reset` method.
 */

use core::str;
use core::uint;
use core::vec;

/*
 * A SHA-1 implementation derived from Paul E. Jones's reference
 * implementation, which is written for clarity, not speed. At some
 * point this will want to be rewritten.
 */

/// The SHA-1 interface
trait Sha1 {
    /// Provide message input as bytes
    fn input(&mut self, &[const u8]);
    /// Provide message input as string
    fn input_str(&mut self, &str);
    /**
     * Read the digest as a vector of 20 bytes. After calling this no further
     * input may be provided until reset is called.
     */
    fn result(&mut self) -> ~[u8];
    /**
     * Read the digest as a hex string. After calling this no further
     * input may be provided until reset is called.
     */
    fn result_str(&mut self) -> ~str;
    /// Reset the SHA-1 state for reuse
    fn reset(&mut self);
}

// Some unexported constants
const digest_buf_len: uint = 5u;
const msg_block_len: uint = 64u;
const work_buf_len: uint = 80u;
const k0: u32 = 0x5A827999u32;
const k1: u32 = 0x6ED9EBA1u32;
const k2: u32 = 0x8F1BBCDCu32;
const k3: u32 = 0xCA62C1D6u32;


/// Construct a `sha` object
pub fn sha1() -> Sha1 {
    struct Sha1State
        { h: ~[u32],
          len_low: u32,
          len_high: u32,
          msg_block: ~[u8],
          msg_block_idx: uint,
          computed: bool,
          work_buf: @mut ~[u32]};

    fn add_input(st: &mut Sha1State, msg: &[const u8]) {
        assert (!st.computed);
        for vec::each_const(msg) |element| {
            st.msg_block[st.msg_block_idx] = *element;
            st.msg_block_idx += 1u;
            st.len_low += 8u32;
            if st.len_low == 0u32 {
                st.len_high += 1u32;
                if st.len_high == 0u32 {
                    // FIXME: Need better failure mode (#2346)
                    fail!();
                }
            }
            if st.msg_block_idx == msg_block_len { process_msg_block(st); }
        }
    }
    fn process_msg_block(st: &mut Sha1State) {
        assert (vec::len(st.h) == digest_buf_len);
        assert (vec::len(*st.work_buf) == work_buf_len);
        let mut t: int; // Loop counter
        let mut w = st.work_buf;

        // Initialize the first 16 words of the vector w
        t = 0;
        while t < 16 {
            let mut tmp;
            tmp = (st.msg_block[t * 4] as u32) << 24u32;
            tmp = tmp | (st.msg_block[t * 4 + 1] as u32) << 16u32;
            tmp = tmp | (st.msg_block[t * 4 + 2] as u32) << 8u32;
            tmp = tmp | (st.msg_block[t * 4 + 3] as u32);
            w[t] = tmp;
            t += 1;
        }

        // Initialize the rest of vector w
        while t < 80 {
            let val = w[t - 3] ^ w[t - 8] ^ w[t - 14] ^ w[t - 16];
            w[t] = circular_shift(1u32, val);
            t += 1;
        }
        let mut a = st.h[0];
        let mut b = st.h[1];
        let mut c = st.h[2];
        let mut d = st.h[3];
        let mut e = st.h[4];
        let mut temp: u32;
        t = 0;
        while t < 20 {
            temp = circular_shift(5u32, a) + (b & c | !b & d) + e + w[t] + k0;
            e = d;
            d = c;
            c = circular_shift(30u32, b);
            b = a;
            a = temp;
            t += 1;
        }
        while t < 40 {
            temp = circular_shift(5u32, a) + (b ^ c ^ d) + e + w[t] + k1;
            e = d;
            d = c;
            c = circular_shift(30u32, b);
            b = a;
            a = temp;
            t += 1;
        }
        while t < 60 {
            temp =
                circular_shift(5u32, a) + (b & c | b & d | c & d) + e + w[t] +
                    k2;
            e = d;
            d = c;
            c = circular_shift(30u32, b);
            b = a;
            a = temp;
            t += 1;
        }
        while t < 80 {
            temp = circular_shift(5u32, a) + (b ^ c ^ d) + e + w[t] + k3;
            e = d;
            d = c;
            c = circular_shift(30u32, b);
            b = a;
            a = temp;
            t += 1;
        }
        st.h[0] = st.h[0] + a;
        st.h[1] = st.h[1] + b;
        st.h[2] = st.h[2] + c;
        st.h[3] = st.h[3] + d;
        st.h[4] = st.h[4] + e;
        st.msg_block_idx = 0u;
    }
    fn circular_shift(bits: u32, word: u32) -> u32 {
        return word << bits | word >> 32u32 - bits;
    }
    fn mk_result(st: &mut Sha1State) -> ~[u8] {
        if !(*st).computed { pad_msg(st); (*st).computed = true; }
        let mut rs: ~[u8] = ~[];
        for vec::each_mut((*st).h) |ptr_hpart| {
            let hpart = *ptr_hpart;
            let a = (hpart >> 24u32 & 0xFFu32) as u8;
            let b = (hpart >> 16u32 & 0xFFu32) as u8;
            let c = (hpart >> 8u32 & 0xFFu32) as u8;
            let d = (hpart & 0xFFu32) as u8;
            rs = vec::append(rs, ~[a, b, c, d]);
        }
        return rs;
    }

    /*
     * According to the standard, the message must be padded to an even
     * 512 bits.  The first padding bit must be a '1'.  The last 64 bits
     * represent the length of the original message.  All bits in between
     * should be 0.  This function will pad the message according to those
     * rules by filling the msg_block vector accordingly.  It will also
     * call process_msg_block() appropriately.  When it returns, it
     * can be assumed that the message digest has been computed.
     */
    fn pad_msg(st: &mut Sha1State) {
        assert (vec::len((*st).msg_block) == msg_block_len);

        /*
         * Check to see if the current message block is too small to hold
         * the initial padding bits and length.  If so, we will pad the
         * block, process it, and then continue padding into a second block.
         */
        if (*st).msg_block_idx > 55u {
            (*st).msg_block[(*st).msg_block_idx] = 0x80u8;
            (*st).msg_block_idx += 1u;
            while (*st).msg_block_idx < msg_block_len {
                (*st).msg_block[(*st).msg_block_idx] = 0u8;
                (*st).msg_block_idx += 1u;
            }
            process_msg_block(st);
        } else {
            (*st).msg_block[(*st).msg_block_idx] = 0x80u8;
            (*st).msg_block_idx += 1u;
        }
        while (*st).msg_block_idx < 56u {
            (*st).msg_block[(*st).msg_block_idx] = 0u8;
            (*st).msg_block_idx += 1u;
        }

        // Store the message length as the last 8 octets
        (*st).msg_block[56] = ((*st).len_high >> 24u32 & 0xFFu32) as u8;
        (*st).msg_block[57] = ((*st).len_high >> 16u32 & 0xFFu32) as u8;
        (*st).msg_block[58] = ((*st).len_high >> 8u32 & 0xFFu32) as u8;
        (*st).msg_block[59] = ((*st).len_high & 0xFFu32) as u8;
        (*st).msg_block[60] = ((*st).len_low >> 24u32 & 0xFFu32) as u8;
        (*st).msg_block[61] = ((*st).len_low >> 16u32 & 0xFFu32) as u8;
        (*st).msg_block[62] = ((*st).len_low >> 8u32 & 0xFFu32) as u8;
        (*st).msg_block[63] = ((*st).len_low & 0xFFu32) as u8;
        process_msg_block(st);
    }

    impl Sha1 for Sha1State {
        fn reset(&mut self) {
            assert (vec::len(self.h) == digest_buf_len);
            self.len_low = 0u32;
            self.len_high = 0u32;
            self.msg_block_idx = 0u;
            self.h[0] = 0x67452301u32;
            self.h[1] = 0xEFCDAB89u32;
            self.h[2] = 0x98BADCFEu32;
            self.h[3] = 0x10325476u32;
            self.h[4] = 0xC3D2E1F0u32;
            self.computed = false;
        }
        fn input(&mut self, msg: &[const u8]) { add_input(self, msg); }
        fn input_str(&mut self, msg: &str) {
            let bs = str::to_bytes(msg);
            add_input(self, bs);
        }
        fn result(&mut self) -> ~[u8] { return mk_result(self); }
        fn result_str(&mut self) -> ~str {
            let rr = mk_result(self);
            let mut s = ~"";
            for vec::each(rr) |b| {
                s += uint::to_str_radix(*b as uint, 16u);
            }
            return s;
        }
    }
    let mut st = Sha1State {
         h: vec::from_elem(digest_buf_len, 0u32),
         len_low: 0u32,
         len_high: 0u32,
         msg_block: vec::from_elem(msg_block_len, 0u8),
         msg_block_idx: 0u,
         computed: false,
         work_buf: @mut vec::from_elem(work_buf_len, 0u32)
    };
    let mut sh = (move st) as Sha1;
    sh.reset();
    return sh;
}

#[cfg(test)]
mod tests {
    use sha1;

    use core::str;
    use core::vec;

    #[test]
    pub fn test() {
        unsafe {
            struct Test {
                input: ~str,
                output: ~[u8],
            }

            fn a_million_letter_a() -> ~str {
                let mut i = 0;
                let mut rs = ~"";
                while i < 100000 {
                    str::push_str(&mut rs, ~"aaaaaaaaaa");
                    i += 1;
                }
                return rs;
            }
            // Test messages from FIPS 180-1

            let fips_180_1_tests = ~[
                Test {
                    input: ~"abc",
                    output: ~[
                        0xA9u8, 0x99u8, 0x3Eu8, 0x36u8,
                        0x47u8, 0x06u8, 0x81u8, 0x6Au8,
                        0xBAu8, 0x3Eu8, 0x25u8, 0x71u8,
                        0x78u8, 0x50u8, 0xC2u8, 0x6Cu8,
                        0x9Cu8, 0xD0u8, 0xD8u8, 0x9Du8,
                    ],
                },
                Test {
                    input:
                         ~"abcdbcdecdefdefgefghfghighij" +
                         ~"hijkijkljklmklmnlmnomnopnopq",
                    output: ~[
                        0x84u8, 0x98u8, 0x3Eu8, 0x44u8,
                        0x1Cu8, 0x3Bu8, 0xD2u8, 0x6Eu8,
                        0xBAu8, 0xAEu8, 0x4Au8, 0xA1u8,
                        0xF9u8, 0x51u8, 0x29u8, 0xE5u8,
                        0xE5u8, 0x46u8, 0x70u8, 0xF1u8,
                    ],
                },
                Test {
                    input: a_million_letter_a(),
                    output: ~[
                        0x34u8, 0xAAu8, 0x97u8, 0x3Cu8,
                        0xD4u8, 0xC4u8, 0xDAu8, 0xA4u8,
                        0xF6u8, 0x1Eu8, 0xEBu8, 0x2Bu8,
                        0xDBu8, 0xADu8, 0x27u8, 0x31u8,
                        0x65u8, 0x34u8, 0x01u8, 0x6Fu8,
                    ],
                },
            ];
            // Examples from wikipedia

            let wikipedia_tests = ~[
                Test {
                    input: ~"The quick brown fox jumps over the lazy dog",
                    output: ~[
                        0x2fu8, 0xd4u8, 0xe1u8, 0xc6u8,
                        0x7au8, 0x2du8, 0x28u8, 0xfcu8,
                        0xedu8, 0x84u8, 0x9eu8, 0xe1u8,
                        0xbbu8, 0x76u8, 0xe7u8, 0x39u8,
                        0x1bu8, 0x93u8, 0xebu8, 0x12u8,
                    ],
                },
                Test {
                    input: ~"The quick brown fox jumps over the lazy cog",
                    output: ~[
                        0xdeu8, 0x9fu8, 0x2cu8, 0x7fu8,
                        0xd2u8, 0x5eu8, 0x1bu8, 0x3au8,
                        0xfau8, 0xd3u8, 0xe8u8, 0x5au8,
                        0x0bu8, 0xd1u8, 0x7du8, 0x9bu8,
                        0x10u8, 0x0du8, 0xb4u8, 0xb3u8,
                    ],
                },
            ];
            let tests = fips_180_1_tests + wikipedia_tests;
            fn check_vec_eq(v0: ~[u8], v1: ~[u8]) {
                assert (vec::len::<u8>(v0) == vec::len::<u8>(v1));
                let len = vec::len::<u8>(v0);
                let mut i = 0u;
                while i < len {
                    let a = v0[i];
                    let b = v1[i];
                    assert (a == b);
                    i += 1u;
                }
            }
            // Test that it works when accepting the message all at once

            let mut sh = sha1::sha1();
            for vec::each(tests) |t| {
                sh.input_str(t.input);
                let out = sh.result();
                check_vec_eq(t.output, out);
                sh.reset();
            }


            // Test that it works when accepting the message in pieces
            for vec::each(tests) |t| {
                let len = str::len(t.input);
                let mut left = len;
                while left > 0u {
                    let take = (left + 1u) / 2u;
                    sh.input_str(str::slice(t.input, len - left,
                                 take + len - left));
                    left = left - take;
                }
                let out = sh.result();
                check_vec_eq(t.output, out);
                sh.reset();
            }
        }
    }
}

// Local Variables:
// mode: rust;
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// End:
