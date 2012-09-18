// NB: transitionary, de-mode-ing.
#[forbid(deprecated_mode)];
#[forbid(deprecated_pattern)];

/*!
 * Implementation of SipHash 2-4
 *
 * See: http://131002.net/siphash/
 *
 * Consider this as a main "general-purpose" hash for all hashtables: it
 * runs at good speed (competitive with spooky and city) and permits
 * cryptographically strong _keyed_ hashing. Key your hashtables from a
 * CPRNG like rand::rng.
 */

use io::Writer;
use io::WriterUtil;
use to_bytes::IterBytes;

export Streaming, State, Hash, HashUtil;
export default_state;
export hash_bytes_keyed;
export hash_str_keyed;
export hash_u64_keyed;
export hash_u32_keyed;
export hash_u16_keyed;
export hash_u8_keyed;
export hash_uint_keyed;
export hash_bytes;
export hash_str;
export hash_u64;
export hash_u32;
export hash_u16;
export hash_u8;
export hash_uint;

/**
 * Types that can meaningfully be hashed should implement this.
 *
 * Note that this trait is likely to change somewhat as it is
 * closely related to `to_bytes::IterBytes` and in almost all
 * cases presently the two are (and must be) used together.
 *
 * In general, most types only need to implement `IterBytes`,
 * and the implementation of `Hash` below will take care of
 * the rest. This is the recommended approach, since constructing
 * good keyed hash functions is quite difficult.
 */
trait Hash {
    /**
     * Compute a "keyed" hash of the value implementing the trait,
     * taking `k0` and `k1` as "keying" parameters that randomize or
     * otherwise perturb the hash function in such a way that a
     * hash table built using such "keyed hash functions" cannot
     * be made to perform linearly by an attacker controlling the
     * hashtable's contents.
     *
     * In practical terms, we implement this using the SipHash 2-4
     * function and require most types to only implement the
     * IterBytes trait, that feeds SipHash.
     */
    pure fn hash_keyed(k0: u64, k1: u64) -> u64;
}

// When we have default methods, won't need this.
trait HashUtil {
    pure fn hash() -> u64;
}

impl <A: Hash> A: HashUtil {
    #[inline(always)]
    pure fn hash() -> u64 { self.hash_keyed(0,0) }
}

/// Streaming hash-functions should implement this.
trait Streaming {
    fn input((&[const u8]));
    // These can be refactored some when we have default methods.
    fn result_bytes() -> ~[u8];
    fn result_str() -> ~str;
    fn result_u64() -> u64;
    fn reset();
}

impl <A: IterBytes> A: Hash {
    #[inline(always)]
    pure fn hash_keyed(k0: u64, k1: u64) -> u64 {
        unsafe {
            let s = &State(k0, k1);
            for self.iter_bytes(true) |bytes| {
                s.input(bytes);
            }
            s.result_u64()
        }
    }
}

// implementations

pure fn hash_keyed_2<A: IterBytes,
                     B: IterBytes>(a: &A, b: &B,
                                   k0: u64, k1: u64) -> u64 {
    unsafe {
        let s = &State(k0, k1);
        for a.iter_bytes(true) |bytes| { s.input(bytes); }
        for b.iter_bytes(true) |bytes| { s.input(bytes); }
        s.result_u64()
    }
}

pure fn hash_keyed_3<A: IterBytes,
                     B: IterBytes,
                     C: IterBytes>(a: &A, b: &B, c: &C,
                                   k0: u64, k1: u64) -> u64 {
    unsafe {
        let s = &State(k0, k1);
        for a.iter_bytes(true) |bytes| { s.input(bytes); }
        for b.iter_bytes(true) |bytes| { s.input(bytes); }
        for c.iter_bytes(true) |bytes| { s.input(bytes); }
        s.result_u64()
    }
}

pure fn hash_keyed_4<A: IterBytes,
                     B: IterBytes,
                     C: IterBytes,
                     D: IterBytes>(a: &A, b: &B, c: &C, d: &D,
                                   k0: u64, k1: u64) -> u64 {
    unsafe {
        let s = &State(k0, k1);
        for a.iter_bytes(true) |bytes| { s.input(bytes); }
        for b.iter_bytes(true) |bytes| { s.input(bytes); }
        for c.iter_bytes(true) |bytes| { s.input(bytes); }
        for d.iter_bytes(true) |bytes| { s.input(bytes); }
        s.result_u64()
    }
}

pure fn hash_keyed_5<A: IterBytes,
                     B: IterBytes,
                     C: IterBytes,
                     D: IterBytes,
                     E: IterBytes>(a: &A, b: &B, c: &C, d: &D, e: &E,
                                   k0: u64, k1: u64) -> u64 {
    unsafe {
        let s = &State(k0, k1);
        for a.iter_bytes(true) |bytes| { s.input(bytes); }
        for b.iter_bytes(true) |bytes| { s.input(bytes); }
        for c.iter_bytes(true) |bytes| { s.input(bytes); }
        for d.iter_bytes(true) |bytes| { s.input(bytes); }
        for e.iter_bytes(true) |bytes| { s.input(bytes); }
        s.result_u64()
    }
}

pure fn hash_bytes_keyed(val: &[u8], k0: u64, k1: u64) -> u64 {
    val.hash_keyed(k0, k1)
}
pure fn hash_str_keyed(val: &str, k0: u64, k1: u64) -> u64 {
    val.hash_keyed(k0, k1)
}
pure fn hash_u64_keyed(val: u64, k0: u64, k1: u64) -> u64 {
    val.hash_keyed(k0, k1)
}
pure fn hash_u32_keyed(val: u32, k0: u64, k1: u64) -> u64 {
    val.hash_keyed(k0, k1)
}
pure fn hash_u16_keyed(val: u16, k0: u64, k1: u64) -> u64 {
    val.hash_keyed(k0, k1)
}
pure fn hash_u8_keyed(val: u8, k0: u64, k1: u64) -> u64 {
    val.hash_keyed(k0, k1)
}
pure fn hash_uint_keyed(val: uint, k0: u64, k1: u64) -> u64 {
    val.hash_keyed(k0, k1)
}

pure fn hash_bytes(val: &[u8]) -> u64 { hash_bytes_keyed(val, 0, 0) }
pure fn hash_str(val: &str) -> u64 { hash_str_keyed(val, 0, 0) }
pure fn hash_u64(val: u64) -> u64 { hash_u64_keyed(val, 0, 0) }
pure fn hash_u32(val: u32) -> u64 { hash_u32_keyed(val, 0, 0) }
pure fn hash_u16(val: u16) -> u64 { hash_u16_keyed(val, 0, 0) }
pure fn hash_u8(val: u8) -> u64 { hash_u8_keyed(val, 0, 0) }
pure fn hash_uint(val: uint) -> u64 { hash_uint_keyed(val, 0, 0) }


// Implement State as SipState

type State = SipState;

#[inline(always)]
fn State(k0: u64, k1: u64) -> State {
    SipState(k0, k1)
}

#[inline(always)]
fn default_state() -> State {
    State(0,0)
}

struct SipState {
    k0: u64,
    k1: u64,
    mut length: uint, // how many bytes we've processed
    mut v0: u64,      // hash state
    mut v1: u64,
    mut v2: u64,
    mut v3: u64,
    tail: [mut u8]/8, // unprocessed bytes
    mut ntail: uint,  // how many bytes in tail are valid
}

#[inline(always)]
fn SipState(key0: u64, key1: u64) -> SipState {
    let state = SipState {
        k0 : key0,
        k1 : key1,
        mut length : 0u,
        mut v0 : 0u64,
        mut v1 : 0u64,
        mut v2 : 0u64,
        mut v3 : 0u64,
        tail : [mut 0u8,0,0,0,0,0,0,0],
        mut ntail : 0u,
    };
    (&state).reset();
    move state
}


impl &SipState : io::Writer {

    // Methods for io::writer
    #[inline(always)]
    fn write(msg: &[const u8]) {

        macro_rules! u8to64_le (
            ($buf:expr, $i:expr) =>
            ($buf[0+$i] as u64 |
             $buf[1+$i] as u64 << 8 |
             $buf[2+$i] as u64 << 16 |
             $buf[3+$i] as u64 << 24 |
             $buf[4+$i] as u64 << 32 |
             $buf[5+$i] as u64 << 40 |
             $buf[6+$i] as u64 << 48 |
             $buf[7+$i] as u64 << 56)
        );

        macro_rules! rotl (
            ($x:expr, $b:expr) =>
            (($x << $b) | ($x >> (64 - $b)))
        );

        macro_rules! compress (
            ($v0:expr, $v1:expr, $v2:expr, $v3:expr) =>
            {
                $v0 += $v1; $v1 = rotl!($v1, 13); $v1 ^= $v0;
                $v0 = rotl!($v0, 32);
                $v2 += $v3; $v3 = rotl!($v3, 16); $v3 ^= $v2;
                $v0 += $v3; $v3 = rotl!($v3, 21); $v3 ^= $v0;
                $v2 += $v1; $v1 = rotl!($v1, 17); $v1 ^= $v2;
                $v2 = rotl!($v2, 32);
            }
        );

        let length = msg.len();
        self.length += length;

        let mut needed = 0u;

        if self.ntail != 0 {
            needed = 8 - self.ntail;

            if length < needed {
                let mut t = 0;
                while t < length {
                    self.tail[self.ntail+t] = msg[t];
                    t += 1;
                }
                self.ntail += length;
                return;
            }

            let mut t = 0;
            while t < needed {
                self.tail[self.ntail+t] = msg[t];
                t += 1;
            }

            let m = u8to64_le!(self.tail, 0);

            self.v3 ^= m;
            compress!(self.v0, self.v1, self.v2, self.v3);
            compress!(self.v0, self.v1, self.v2, self.v3);
            self.v0 ^= m;

            self.ntail = 0;
        }

        // Buffered tail is now flushed, process new input.
        let len = length - needed;
        let end = len & (!0x7);
        let left = len & 0x7;

        let mut i = needed;
        while i < end {
            let mi = u8to64_le!(msg, i);

            self.v3 ^= mi;
            compress!(self.v0, self.v1, self.v2, self.v3);
            compress!(self.v0, self.v1, self.v2, self.v3);
            self.v0 ^= mi;

            i += 8;
        }

        let mut t = 0u;
        while t < left {
            self.tail[t] = msg[i+t];
            t += 1
        }
        self.ntail = left;
    }

    fn seek(_x: int, _s: io::SeekStyle) {
        fail;
    }
    fn tell() -> uint {
        self.length
    }
    fn flush() -> int {
        0
    }
    fn get_type() -> io::WriterType {
        io::File
    }
}

impl &SipState : Streaming {

    #[inline(always)]
    fn input(buf: &[const u8]) {
        self.write(buf);
    }

    #[inline(always)]
    fn result_u64() -> u64 {
        let mut v0 = self.v0;
        let mut v1 = self.v1;
        let mut v2 = self.v2;
        let mut v3 = self.v3;

        let mut b : u64 = (self.length as u64 & 0xff) << 56;

        if self.ntail > 0 { b |= self.tail[0] as u64 <<  0; }
        if self.ntail > 1 { b |= self.tail[1] as u64 <<  8; }
        if self.ntail > 2 { b |= self.tail[2] as u64 << 16; }
        if self.ntail > 3 { b |= self.tail[3] as u64 << 24; }
        if self.ntail > 4 { b |= self.tail[4] as u64 << 32; }
        if self.ntail > 5 { b |= self.tail[5] as u64 << 40; }
        if self.ntail > 6 { b |= self.tail[6] as u64 << 48; }

        v3 ^= b;
        compress!(v0, v1, v2, v3);
        compress!(v0, v1, v2, v3);
        v0 ^= b;

        v2 ^= 0xff;
        compress!(v0, v1, v2, v3);
        compress!(v0, v1, v2, v3);
        compress!(v0, v1, v2, v3);
        compress!(v0, v1, v2, v3);

        return (v0 ^ v1 ^ v2 ^ v3);
    }

    fn result_bytes() -> ~[u8] {
        let h = self.result_u64();
        ~[(h >> 0) as u8,
          (h >> 8) as u8,
          (h >> 16) as u8,
          (h >> 24) as u8,
          (h >> 32) as u8,
          (h >> 40) as u8,
          (h >> 48) as u8,
          (h >> 56) as u8,
        ]
    }

    fn result_str() -> ~str {
        let r = self.result_bytes();
        let mut s = ~"";
        for vec::each(r) |b| { s += uint::to_str(b as uint, 16u); }
        move s
    }

    #[inline(always)]
    fn reset() {
        self.length = 0;
        self.v0 = self.k0 ^ 0x736f6d6570736575;
        self.v1 = self.k1 ^ 0x646f72616e646f6d;
        self.v2 = self.k0 ^ 0x6c7967656e657261;
        self.v3 = self.k1 ^ 0x7465646279746573;
        self.ntail = 0;
    }
}

#[test]
fn test_siphash() {
    let vecs : [[u8]/8]/64 = [
        [ 0x31, 0x0e, 0x0e, 0xdd, 0x47, 0xdb, 0x6f, 0x72, ]/_,
        [ 0xfd, 0x67, 0xdc, 0x93, 0xc5, 0x39, 0xf8, 0x74, ]/_,
        [ 0x5a, 0x4f, 0xa9, 0xd9, 0x09, 0x80, 0x6c, 0x0d, ]/_,
        [ 0x2d, 0x7e, 0xfb, 0xd7, 0x96, 0x66, 0x67, 0x85, ]/_,
        [ 0xb7, 0x87, 0x71, 0x27, 0xe0, 0x94, 0x27, 0xcf, ]/_,
        [ 0x8d, 0xa6, 0x99, 0xcd, 0x64, 0x55, 0x76, 0x18, ]/_,
        [ 0xce, 0xe3, 0xfe, 0x58, 0x6e, 0x46, 0xc9, 0xcb, ]/_,
        [ 0x37, 0xd1, 0x01, 0x8b, 0xf5, 0x00, 0x02, 0xab, ]/_,
        [ 0x62, 0x24, 0x93, 0x9a, 0x79, 0xf5, 0xf5, 0x93, ]/_,
        [ 0xb0, 0xe4, 0xa9, 0x0b, 0xdf, 0x82, 0x00, 0x9e, ]/_,
        [ 0xf3, 0xb9, 0xdd, 0x94, 0xc5, 0xbb, 0x5d, 0x7a, ]/_,
        [ 0xa7, 0xad, 0x6b, 0x22, 0x46, 0x2f, 0xb3, 0xf4, ]/_,
        [ 0xfb, 0xe5, 0x0e, 0x86, 0xbc, 0x8f, 0x1e, 0x75, ]/_,
        [ 0x90, 0x3d, 0x84, 0xc0, 0x27, 0x56, 0xea, 0x14, ]/_,
        [ 0xee, 0xf2, 0x7a, 0x8e, 0x90, 0xca, 0x23, 0xf7, ]/_,
        [ 0xe5, 0x45, 0xbe, 0x49, 0x61, 0xca, 0x29, 0xa1, ]/_,
        [ 0xdb, 0x9b, 0xc2, 0x57, 0x7f, 0xcc, 0x2a, 0x3f, ]/_,
        [ 0x94, 0x47, 0xbe, 0x2c, 0xf5, 0xe9, 0x9a, 0x69, ]/_,
        [ 0x9c, 0xd3, 0x8d, 0x96, 0xf0, 0xb3, 0xc1, 0x4b, ]/_,
        [ 0xbd, 0x61, 0x79, 0xa7, 0x1d, 0xc9, 0x6d, 0xbb, ]/_,
        [ 0x98, 0xee, 0xa2, 0x1a, 0xf2, 0x5c, 0xd6, 0xbe, ]/_,
        [ 0xc7, 0x67, 0x3b, 0x2e, 0xb0, 0xcb, 0xf2, 0xd0, ]/_,
        [ 0x88, 0x3e, 0xa3, 0xe3, 0x95, 0x67, 0x53, 0x93, ]/_,
        [ 0xc8, 0xce, 0x5c, 0xcd, 0x8c, 0x03, 0x0c, 0xa8, ]/_,
        [ 0x94, 0xaf, 0x49, 0xf6, 0xc6, 0x50, 0xad, 0xb8, ]/_,
        [ 0xea, 0xb8, 0x85, 0x8a, 0xde, 0x92, 0xe1, 0xbc, ]/_,
        [ 0xf3, 0x15, 0xbb, 0x5b, 0xb8, 0x35, 0xd8, 0x17, ]/_,
        [ 0xad, 0xcf, 0x6b, 0x07, 0x63, 0x61, 0x2e, 0x2f, ]/_,
        [ 0xa5, 0xc9, 0x1d, 0xa7, 0xac, 0xaa, 0x4d, 0xde, ]/_,
        [ 0x71, 0x65, 0x95, 0x87, 0x66, 0x50, 0xa2, 0xa6, ]/_,
        [ 0x28, 0xef, 0x49, 0x5c, 0x53, 0xa3, 0x87, 0xad, ]/_,
        [ 0x42, 0xc3, 0x41, 0xd8, 0xfa, 0x92, 0xd8, 0x32, ]/_,
        [ 0xce, 0x7c, 0xf2, 0x72, 0x2f, 0x51, 0x27, 0x71, ]/_,
        [ 0xe3, 0x78, 0x59, 0xf9, 0x46, 0x23, 0xf3, 0xa7, ]/_,
        [ 0x38, 0x12, 0x05, 0xbb, 0x1a, 0xb0, 0xe0, 0x12, ]/_,
        [ 0xae, 0x97, 0xa1, 0x0f, 0xd4, 0x34, 0xe0, 0x15, ]/_,
        [ 0xb4, 0xa3, 0x15, 0x08, 0xbe, 0xff, 0x4d, 0x31, ]/_,
        [ 0x81, 0x39, 0x62, 0x29, 0xf0, 0x90, 0x79, 0x02, ]/_,
        [ 0x4d, 0x0c, 0xf4, 0x9e, 0xe5, 0xd4, 0xdc, 0xca, ]/_,
        [ 0x5c, 0x73, 0x33, 0x6a, 0x76, 0xd8, 0xbf, 0x9a, ]/_,
        [ 0xd0, 0xa7, 0x04, 0x53, 0x6b, 0xa9, 0x3e, 0x0e, ]/_,
        [ 0x92, 0x59, 0x58, 0xfc, 0xd6, 0x42, 0x0c, 0xad, ]/_,
        [ 0xa9, 0x15, 0xc2, 0x9b, 0xc8, 0x06, 0x73, 0x18, ]/_,
        [ 0x95, 0x2b, 0x79, 0xf3, 0xbc, 0x0a, 0xa6, 0xd4, ]/_,
        [ 0xf2, 0x1d, 0xf2, 0xe4, 0x1d, 0x45, 0x35, 0xf9, ]/_,
        [ 0x87, 0x57, 0x75, 0x19, 0x04, 0x8f, 0x53, 0xa9, ]/_,
        [ 0x10, 0xa5, 0x6c, 0xf5, 0xdf, 0xcd, 0x9a, 0xdb, ]/_,
        [ 0xeb, 0x75, 0x09, 0x5c, 0xcd, 0x98, 0x6c, 0xd0, ]/_,
        [ 0x51, 0xa9, 0xcb, 0x9e, 0xcb, 0xa3, 0x12, 0xe6, ]/_,
        [ 0x96, 0xaf, 0xad, 0xfc, 0x2c, 0xe6, 0x66, 0xc7, ]/_,
        [ 0x72, 0xfe, 0x52, 0x97, 0x5a, 0x43, 0x64, 0xee, ]/_,
        [ 0x5a, 0x16, 0x45, 0xb2, 0x76, 0xd5, 0x92, 0xa1, ]/_,
        [ 0xb2, 0x74, 0xcb, 0x8e, 0xbf, 0x87, 0x87, 0x0a, ]/_,
        [ 0x6f, 0x9b, 0xb4, 0x20, 0x3d, 0xe7, 0xb3, 0x81, ]/_,
        [ 0xea, 0xec, 0xb2, 0xa3, 0x0b, 0x22, 0xa8, 0x7f, ]/_,
        [ 0x99, 0x24, 0xa4, 0x3c, 0xc1, 0x31, 0x57, 0x24, ]/_,
        [ 0xbd, 0x83, 0x8d, 0x3a, 0xaf, 0xbf, 0x8d, 0xb7, ]/_,
        [ 0x0b, 0x1a, 0x2a, 0x32, 0x65, 0xd5, 0x1a, 0xea, ]/_,
        [ 0x13, 0x50, 0x79, 0xa3, 0x23, 0x1c, 0xe6, 0x60, ]/_,
        [ 0x93, 0x2b, 0x28, 0x46, 0xe4, 0xd7, 0x06, 0x66, ]/_,
        [ 0xe1, 0x91, 0x5f, 0x5c, 0xb1, 0xec, 0xa4, 0x6c, ]/_,
        [ 0xf3, 0x25, 0x96, 0x5c, 0xa1, 0x6d, 0x62, 0x9f, ]/_,
        [ 0x57, 0x5f, 0xf2, 0x8e, 0x60, 0x38, 0x1b, 0xe5, ]/_,
        [ 0x72, 0x45, 0x06, 0xeb, 0x4c, 0x32, 0x8a, 0x95, ]/_
    ]/_;

    let k0 = 0x_07_06_05_04_03_02_01_00_u64;
    let k1 = 0x_0f_0e_0d_0c_0b_0a_09_08_u64;
    let mut buf : ~[u8] = ~[];
    let mut t = 0;
    let stream_inc = &State(k0,k1);
    let stream_full = &State(k0,k1);

    fn to_hex_str(r:  &[u8]/8) -> ~str {
        let mut s = ~"";
        for vec::each(*r) |b| { s += uint::to_str(b as uint, 16u); }
        return s;
    }

    while t < 64 {
        debug!("siphash test %?", t);
        let vec = u8to64_le!(vecs[t], 0);
        let out = buf.hash_keyed(k0, k1);
        debug!("got %?, expected %?", out, vec);
        assert vec == out;

        stream_full.reset();
        stream_full.input(buf);
        let f = stream_full.result_str();
        let i = stream_inc.result_str();
        let v = to_hex_str(&vecs[t]);
        debug!("%d: (%s) => inc=%s full=%s", t, v, i, f);

        assert f == i && f == v;

        buf += ~[t as u8];
        stream_inc.input(~[t as u8]);

        t += 1;
    }
}

#[test] #[cfg(target_arch = "arm")]
fn test_hash_uint() {
    let val = 0xdeadbeef_deadbeef_u64;
    assert hash_u64(val as u64) == hash_uint(val as uint);
    assert hash_u32(val as u32) != hash_uint(val as uint);
}
#[test] #[cfg(target_arch = "x86_64")]
fn test_hash_uint() {
    let val = 0xdeadbeef_deadbeef_u64;
    assert hash_u64(val as u64) == hash_uint(val as uint);
    assert hash_u32(val as u32) != hash_uint(val as uint);
}
#[test] #[cfg(target_arch = "x86")]
fn test_hash_uint() {
    let val = 0xdeadbeef_deadbeef_u64;
    assert hash_u64(val as u64) != hash_uint(val as uint);
    assert hash_u32(val as u32) == hash_uint(val as uint);
}

#[test]
fn test_hash_idempotent() {
    let val64 = 0xdeadbeef_deadbeef_u64;
    assert hash_u64(val64) == hash_u64(val64);
    let val32 = 0xdeadbeef_u32;
    assert hash_u32(val32) == hash_u32(val32);
}

#[test]
fn test_hash_no_bytes_dropped_64() {
    let val = 0xdeadbeef_deadbeef_u64;

    assert hash_u64(val) != hash_u64(zero_byte(val, 0));
    assert hash_u64(val) != hash_u64(zero_byte(val, 1));
    assert hash_u64(val) != hash_u64(zero_byte(val, 2));
    assert hash_u64(val) != hash_u64(zero_byte(val, 3));
    assert hash_u64(val) != hash_u64(zero_byte(val, 4));
    assert hash_u64(val) != hash_u64(zero_byte(val, 5));
    assert hash_u64(val) != hash_u64(zero_byte(val, 6));
    assert hash_u64(val) != hash_u64(zero_byte(val, 7));

    fn zero_byte(val: u64, byte: uint) -> u64 {
        assert 0 <= byte; assert byte < 8;
        val & !(0xff << (byte * 8))
    }
}

#[test]
fn test_hash_no_bytes_dropped_32() {
    let val = 0xdeadbeef_u32;

    assert hash_u32(val) != hash_u32(zero_byte(val, 0));
    assert hash_u32(val) != hash_u32(zero_byte(val, 1));
    assert hash_u32(val) != hash_u32(zero_byte(val, 2));
    assert hash_u32(val) != hash_u32(zero_byte(val, 3));

    fn zero_byte(val: u32, byte: uint) -> u32 {
        assert 0 <= byte; assert byte < 4;
        val & !(0xff << (byte * 8))
    }
}
