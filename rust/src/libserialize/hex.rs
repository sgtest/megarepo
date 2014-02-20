// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Hex binary-to-text encoding
use std::str;
use std::vec;

/// A trait for converting a value to hexadecimal encoding
pub trait ToHex {
    /// Converts the value of `self` to a hex value, returning the owned
    /// string.
    fn to_hex(&self) -> ~str;
}

static CHARS: &'static[u8] = bytes!("0123456789abcdef");

impl<'a> ToHex for &'a [u8] {
    /**
     * Turn a vector of `u8` bytes into a hexadecimal string.
     *
     * # Example
     *
     * ```rust
     * extern crate serialize;
     * use serialize::hex::ToHex;
     *
     * fn main () {
     *     let str = [52,32].to_hex();
     *     println!("{}", str);
     * }
     * ```
     */
    fn to_hex(&self) -> ~str {
        let mut v = vec::with_capacity(self.len() * 2);
        for &byte in self.iter() {
            v.push(CHARS[byte >> 4]);
            v.push(CHARS[byte & 0xf]);
        }

        unsafe {
            str::raw::from_utf8_owned(v)
        }
    }
}

/// A trait for converting hexadecimal encoded values
pub trait FromHex {
    /// Converts the value of `self`, interpreted as hexadecimal encoded data,
    /// into an owned vector of bytes, returning the vector.
    fn from_hex(&self) -> Result<~[u8], FromHexError>;
}

/// Errors that can occur when decoding a hex encoded string
pub enum FromHexError {
    /// The input contained a character not part of the hex format
    InvalidHexCharacter(char, uint),
    /// The input had an invalid length
    InvalidHexLength,
}

impl ToStr for FromHexError {
    fn to_str(&self) -> ~str {
        match *self {
            InvalidHexCharacter(ch, idx) =>
                format!("Invalid character '{}' at position {}", ch, idx),
            InvalidHexLength => ~"Invalid input length",
        }
    }
}

impl<'a> FromHex for &'a str {
    /**
     * Convert any hexadecimal encoded string (literal, `@`, `&`, or `~`)
     * to the byte values it encodes.
     *
     * You can use the `from_utf8_owned` function in `std::str`
     * to turn a `[u8]` into a string with characters corresponding to those
     * values.
     *
     * # Example
     *
     * This converts a string literal to hexadecimal and back.
     *
     * ```rust
     * extern crate serialize;
     * use serialize::hex::{FromHex, ToHex};
     * use std::str;
     *
     * fn main () {
     *     let hello_str = "Hello, World".as_bytes().to_hex();
     *     println!("{}", hello_str);
     *     let bytes = hello_str.from_hex().unwrap();
     *     println!("{:?}", bytes);
     *     let result_str = str::from_utf8_owned(bytes).unwrap();
     *     println!("{}", result_str);
     * }
     * ```
     */
    fn from_hex(&self) -> Result<~[u8], FromHexError> {
        // This may be an overestimate if there is any whitespace
        let mut b = vec::with_capacity(self.len() / 2);
        let mut modulus = 0;
        let mut buf = 0u8;

        for (idx, byte) in self.bytes().enumerate() {
            buf <<= 4;

            match byte as char {
                'A'..'F' => buf |= byte - ('A' as u8) + 10,
                'a'..'f' => buf |= byte - ('a' as u8) + 10,
                '0'..'9' => buf |= byte - ('0' as u8),
                ' '|'\r'|'\n'|'\t' => {
                    buf >>= 4;
                    continue
                }
                _ => return Err(InvalidHexCharacter(self.char_at(idx), idx)),
            }

            modulus += 1;
            if modulus == 2 {
                modulus = 0;
                b.push(buf);
            }
        }

        match modulus {
            0 => Ok(b),
            _ => Err(InvalidHexLength),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate test;
    use self::test::BenchHarness;
    use hex::{FromHex, ToHex};

    #[test]
    pub fn test_to_hex() {
        assert_eq!("foobar".as_bytes().to_hex(), ~"666f6f626172");
    }

    #[test]
    pub fn test_from_hex_okay() {
        assert_eq!("666f6f626172".from_hex().unwrap(),
                   "foobar".as_bytes().to_owned());
        assert_eq!("666F6F626172".from_hex().unwrap(),
                   "foobar".as_bytes().to_owned());
    }

    #[test]
    pub fn test_from_hex_odd_len() {
        assert!("666".from_hex().is_err());
        assert!("66 6".from_hex().is_err());
    }

    #[test]
    pub fn test_from_hex_invalid_char() {
        assert!("66y6".from_hex().is_err());
    }

    #[test]
    pub fn test_from_hex_ignores_whitespace() {
        assert_eq!("666f 6f6\r\n26172 ".from_hex().unwrap(),
                   "foobar".as_bytes().to_owned());
    }

    #[test]
    pub fn test_to_hex_all_bytes() {
        for i in range(0, 256) {
            assert_eq!([i as u8].to_hex(), format!("{:02x}", i as uint));
        }
    }

    #[test]
    pub fn test_from_hex_all_bytes() {
        for i in range(0, 256) {
            assert_eq!(format!("{:02x}", i as uint).from_hex().unwrap(), ~[i as u8]);
            assert_eq!(format!("{:02X}", i as uint).from_hex().unwrap(), ~[i as u8]);
        }
    }

    #[bench]
    pub fn bench_to_hex(bh: & mut BenchHarness) {
        let s = "イロハニホヘト チリヌルヲ ワカヨタレソ ツネナラム \
                 ウヰノオクヤマ ケフコエテ アサキユメミシ ヱヒモセスン";
        bh.iter(|| {
            s.as_bytes().to_hex();
        });
        bh.bytes = s.len() as u64;
    }

    #[bench]
    pub fn bench_from_hex(bh: & mut BenchHarness) {
        let s = "イロハニホヘト チリヌルヲ ワカヨタレソ ツネナラム \
                 ウヰノオクヤマ ケフコエテ アサキユメミシ ヱヒモセスン";
        let b = s.as_bytes().to_hex();
        bh.iter(|| {
            b.from_hex().unwrap();
        });
        bh.bytes = b.len() as u64;
    }
}
