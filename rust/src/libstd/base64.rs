// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::iter;
use core::str;
use core::vec;

pub trait ToBase64 {
    fn to_base64(&self) -> ~str;
}

static CHARS: [char * 64] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M',
    'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm',
    'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '+', '/'
];

impl<'self> ToBase64 for &'self [u8] {
    fn to_base64(&self) -> ~str {
        let mut s = ~"";
        unsafe {
            let len = self.len();
            str::reserve(&mut s, ((len + 3u) / 4u) * 3u);

            let mut i = 0u;

            while i < len - (len % 3u) {
                let n = (self[i] as uint) << 16u |
                        (self[i + 1u] as uint) << 8u |
                        (self[i + 2u] as uint);

                // This 24-bit number gets separated into four 6-bit numbers.
                str::push_char(&mut s, CHARS[(n >> 18u) & 63u]);
                str::push_char(&mut s, CHARS[(n >> 12u) & 63u]);
                str::push_char(&mut s, CHARS[(n >> 6u) & 63u]);
                str::push_char(&mut s, CHARS[n & 63u]);

                i += 3u;
            }

            // Heh, would be cool if we knew this was exhaustive
            // (the dream of bounded integer types)
            match len % 3 {
              0 => (),
              1 => {
                let n = (self[i] as uint) << 16u;
                str::push_char(&mut s, CHARS[(n >> 18u) & 63u]);
                str::push_char(&mut s, CHARS[(n >> 12u) & 63u]);
                str::push_char(&mut s, '=');
                str::push_char(&mut s, '=');
              }
              2 => {
                let n = (self[i] as uint) << 16u |
                    (self[i + 1u] as uint) << 8u;
                str::push_char(&mut s, CHARS[(n >> 18u) & 63u]);
                str::push_char(&mut s, CHARS[(n >> 12u) & 63u]);
                str::push_char(&mut s, CHARS[(n >> 6u) & 63u]);
                str::push_char(&mut s, '=');
              }
              _ => fail!(~"Algebra is broken, please alert the math police")
            }
        }
        s
    }
}

impl<'self> ToBase64 for &'self str {
    fn to_base64(&self) -> ~str {
        str::to_bytes(*self).to_base64()
    }
}

pub trait FromBase64 {
    fn from_base64(&self) -> ~[u8];
}

impl FromBase64 for ~[u8] {
    fn from_base64(&self) -> ~[u8] {
        if self.len() % 4u != 0u { fail!(~"invalid base64 length"); }

        let len = self.len();
        let mut padding = 0u;

        if len != 0u {
            if self[len - 1u] == '=' as u8 { padding += 1u; }
            if self[len - 2u] == '=' as u8 { padding += 1u; }
        }

        let mut r = vec::with_capacity((len / 4u) * 3u - padding);

        unsafe {
            let mut i = 0u;
            while i < len {
                let mut n = 0u;

                for iter::repeat(4u) {
                    let ch = self[i] as char;
                    n <<= 6u;

                    if ch >= 'A' && ch <= 'Z' {
                        n |= (ch as uint) - 0x41u;
                    } else if ch >= 'a' && ch <= 'z' {
                        n |= (ch as uint) - 0x47u;
                    } else if ch >= '0' && ch <= '9' {
                        n |= (ch as uint) + 0x04u;
                    } else if ch == '+' {
                        n |= 0x3Eu;
                    } else if ch == '/' {
                        n |= 0x3Fu;
                    } else if ch == '=' {
                        match len - i {
                          1u => {
                            r.push(((n >> 16u) & 0xFFu) as u8);
                            r.push(((n >> 8u ) & 0xFFu) as u8);
                            return copy r;
                          }
                          2u => {
                            r.push(((n >> 10u) & 0xFFu) as u8);
                            return copy r;
                          }
                          _ => fail!(~"invalid base64 padding")
                        }
                    } else {
                        fail!(~"invalid base64 character");
                    }

                    i += 1u;
                };

                r.push(((n >> 16u) & 0xFFu) as u8);
                r.push(((n >> 8u ) & 0xFFu) as u8);
                r.push(((n       ) & 0xFFu) as u8);
            }
        }
        r
    }
}

impl FromBase64 for ~str {
    fn from_base64(&self) -> ~[u8] {
        str::to_bytes(*self).from_base64()
    }
}

#[cfg(test)]
mod tests {
    use core::str;

    #[test]
    pub fn test_to_base64() {
        fail_unless!((~"").to_base64()       == ~"");
        fail_unless!((~"f").to_base64()      == ~"Zg==");
        fail_unless!((~"fo").to_base64()     == ~"Zm8=");
        fail_unless!((~"foo").to_base64()    == ~"Zm9v");
        fail_unless!((~"foob").to_base64()   == ~"Zm9vYg==");
        fail_unless!((~"fooba").to_base64()  == ~"Zm9vYmE=");
        fail_unless!((~"foobar").to_base64() == ~"Zm9vYmFy");
    }

    #[test]
    pub fn test_from_base64() {
        fail_unless!((~"").from_base64() == str::to_bytes(~""));
        fail_unless!((~"Zg==").from_base64() == str::to_bytes(~"f"));
        fail_unless!((~"Zm8=").from_base64() == str::to_bytes(~"fo"));
        fail_unless!((~"Zm9v").from_base64() == str::to_bytes(~"foo"));
        fail_unless!((~"Zm9vYg==").from_base64() == str::to_bytes(~"foob"));
        fail_unless!((~"Zm9vYmE=").from_base64() == str::to_bytes(~"fooba"));
        fail_unless!((~"Zm9vYmFy").from_base64() == str::to_bytes(~"foobar"));
    }
}
