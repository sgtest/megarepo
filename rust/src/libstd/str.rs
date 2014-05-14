// Copyright 2012-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Unicode string manipulation (`str` type)

# Basic Usage

Rust's string type is one of the core primitive types of the language. While
represented by the name `str`, the name `str` is not actually a valid type in
Rust. Each string must also be decorated with its ownership. This means that
there are two common kinds of strings in Rust:

* `~str` - This is an owned string. This type obeys all of the normal semantics
           of the `Box<T>` types, meaning that it has one, and only one,
           owner. This type cannot be implicitly copied, and is moved out of
           when passed to other functions.

* `&str` - This is the borrowed string type. This type of string can only be
           created from the other kind of string. As the name "borrowed"
           implies, this type of string is owned elsewhere, and this string
           cannot be moved out of.

As an example, here's a few different kinds of strings.

```rust
fn main() {
    let owned_string = "I am an owned string".to_owned();
    let borrowed_string1 = "This string is borrowed with the 'static lifetime";
    let borrowed_string2: &str = owned_string;   // owned strings can be borrowed
}
```

From the example above, you can see that Rust has 2 different kinds of string
literals. The owned literals correspond to the owned string types, but the
"borrowed literal" is actually more akin to C's concept of a static string.

When a string is declared without a `~` sigil, then the string is allocated
statically in the rodata of the executable/library. The string then has the
type `&'static str` meaning that the string is valid for the `'static`
lifetime, otherwise known as the lifetime of the entire program. As can be
inferred from the type, these static strings are not mutable.

# Mutability

Many languages have immutable strings by default, and Rust has a particular
flavor on this idea. As with the rest of Rust types, strings are immutable by
default. If a string is declared as `mut`, however, it may be mutated. This
works the same way as the rest of Rust's type system in the sense that if
there's a mutable reference to a string, there may only be one mutable reference
to that string. With these guarantees, strings can easily transition between
being mutable/immutable with the same benefits of having mutable strings in
other languages.

# Representation

Rust's string type, `str`, is a sequence of unicode codepoints encoded as a
stream of UTF-8 bytes. All safely-created strings are guaranteed to be validly
encoded UTF-8 sequences. Additionally, strings are not null-terminated
and can contain null codepoints.

The actual representation of strings have direct mappings to vectors:

* `~str` is the same as `~[u8]`
* `&str` is the same as `&[u8]`

*/

use char::Char;
use char;
use clone::Clone;
use cmp::{Eq, TotalEq, Ord, TotalOrd, Equiv, Ordering};
use container::Container;
use default::Default;
use fmt;
use from_str::FromStr;
use io::Writer;
use iter::{Iterator, range, AdditiveIterator};
use mem::transmute;
use mem;
use option::{None, Option, Some};
use slice::Vector;
use slice::{ImmutableVector, MutableVector, CloneableVector};
use strbuf::StrBuf;
use vec::Vec;

pub use core::str::{from_utf8, CharEq, Chars, CharOffsets, RevChars};
pub use core::str::{RevCharOffsets, Bytes, RevBytes, CharSplits, RevCharSplits};
pub use core::str::{CharSplitsN, Words, AnyLines, MatchIndices, StrSplits};
pub use core::str::{eq_slice, eq, is_utf8, is_utf16, UTF16Items};
pub use core::str::{UTF16Item, ScalarValue, LoneSurrogate, utf16_items};
pub use core::str::{truncate_utf16_at_nul, utf8_char_width, CharRange};
pub use core::str::{Str, StrSlice};

/*
Section: Creating a string
*/

/// Consumes a vector of bytes to create a new utf-8 string.
/// Returns None if the vector contains invalid UTF-8.
pub fn from_utf8_owned(vv: ~[u8]) -> Option<~str> {
    if is_utf8(vv) {
        Some(unsafe { raw::from_utf8_owned(vv) })
    } else {
        None
    }
}

impl FromStr for ~str {
    #[inline]
    fn from_str(s: &str) -> Option<~str> { Some(s.to_owned()) }
}

/// Convert a byte to a UTF-8 string
///
/// # Failure
///
/// Fails if invalid UTF-8
pub fn from_byte(b: u8) -> ~str {
    assert!(b < 128u8);
    unsafe { ::mem::transmute(box [b]) }
}

/// Convert a char to a string
pub fn from_char(ch: char) -> ~str {
    let mut buf = StrBuf::new();
    buf.push_char(ch);
    buf.into_owned()
}

/// Convert a vector of chars to a string
pub fn from_chars(chs: &[char]) -> ~str {
    chs.iter().map(|c| *c).collect()
}

/// Methods for vectors of strings
pub trait StrVector {
    /// Concatenate a vector of strings.
    fn concat(&self) -> ~str;

    /// Concatenate a vector of strings, placing a given separator between each.
    fn connect(&self, sep: &str) -> ~str;
}

impl<'a, S: Str> StrVector for &'a [S] {
    fn concat(&self) -> ~str {
        if self.is_empty() { return "".to_owned(); }

        // `len` calculation may overflow but push_str but will check boundaries
        let len = self.iter().map(|s| s.as_slice().len()).sum();

        let mut result = StrBuf::with_capacity(len);

        for s in self.iter() {
            result.push_str(s.as_slice())
        }

        result.into_owned()
    }

    fn connect(&self, sep: &str) -> ~str {
        if self.is_empty() { return "".to_owned(); }

        // concat is faster
        if sep.is_empty() { return self.concat(); }

        // this is wrong without the guarantee that `self` is non-empty
        // `len` calculation may overflow but push_str but will check boundaries
        let len = sep.len() * (self.len() - 1)
            + self.iter().map(|s| s.as_slice().len()).sum();
        let mut result = StrBuf::with_capacity(len);
        let mut first = true;

        for s in self.iter() {
            if first {
                first = false;
            } else {
                result.push_str(sep);
            }
            result.push_str(s.as_slice());
        }
        result.into_owned()
    }
}

impl<'a, S: Str> StrVector for Vec<S> {
    #[inline]
    fn concat(&self) -> ~str {
        self.as_slice().concat()
    }

    #[inline]
    fn connect(&self, sep: &str) -> ~str {
        self.as_slice().connect(sep)
    }
}

/*
Section: Iterators
*/

// Helper functions used for Unicode normalization
fn canonical_sort(comb: &mut [(char, u8)]) {
    use iter::range;
    use tuple::Tuple2;

    let len = comb.len();
    for i in range(0, len) {
        let mut swapped = false;
        for j in range(1, len-i) {
            let class_a = *comb[j-1].ref1();
            let class_b = *comb[j].ref1();
            if class_a != 0 && class_b != 0 && class_a > class_b {
                comb.swap(j-1, j);
                swapped = true;
            }
        }
        if !swapped { break; }
    }
}

#[deriving(Clone)]
enum DecompositionType {
    Canonical,
    Compatible
}

/// External iterator for a string's decomposition's characters.
/// Use with the `std::iter` module.
#[deriving(Clone)]
pub struct Decompositions<'a> {
    kind: DecompositionType,
    iter: Chars<'a>,
    buffer: Vec<(char, u8)>,
    sorted: bool
}

impl<'a> Iterator<char> for Decompositions<'a> {
    #[inline]
    fn next(&mut self) -> Option<char> {
        use unicode::normalization::canonical_combining_class;

        match self.buffer.as_slice().head() {
            Some(&(c, 0)) => {
                self.sorted = false;
                self.buffer.shift();
                return Some(c);
            }
            Some(&(c, _)) if self.sorted => {
                self.buffer.shift();
                return Some(c);
            }
            _ => self.sorted = false
        }

        let decomposer = match self.kind {
            Canonical => char::decompose_canonical,
            Compatible => char::decompose_compatible
        };

        if !self.sorted {
            for ch in self.iter {
                let buffer = &mut self.buffer;
                let sorted = &mut self.sorted;
                decomposer(ch, |d| {
                    let class = canonical_combining_class(d);
                    if class == 0 && !*sorted {
                        canonical_sort(buffer.as_mut_slice());
                        *sorted = true;
                    }
                    buffer.push((d, class));
                });
                if *sorted { break }
            }
        }

        if !self.sorted {
            canonical_sort(self.buffer.as_mut_slice());
            self.sorted = true;
        }

        match self.buffer.shift() {
            Some((c, 0)) => {
                self.sorted = false;
                Some(c)
            }
            Some((c, _)) => Some(c),
            None => None
        }
    }

    fn size_hint(&self) -> (uint, Option<uint>) {
        let (lower, _) = self.iter.size_hint();
        (lower, None)
    }
}

/// Replace all occurrences of one string with another
///
/// # Arguments
///
/// * s - The string containing substrings to replace
/// * from - The string to replace
/// * to - The replacement string
///
/// # Return value
///
/// The original string with all occurrences of `from` replaced with `to`
pub fn replace(s: &str, from: &str, to: &str) -> ~str {
    let mut result = StrBuf::new();
    let mut last_end = 0;
    for (start, end) in s.match_indices(from) {
        result.push_str(unsafe{raw::slice_bytes(s, last_end, start)});
        result.push_str(to);
        last_end = end;
    }
    result.push_str(unsafe{raw::slice_bytes(s, last_end, s.len())});
    result.into_owned()
}

/*
Section: Misc
*/

/// Decode a UTF-16 encoded vector `v` into a string, returning `None`
/// if `v` contains any invalid data.
///
/// # Example
///
/// ```rust
/// use std::str;
///
/// // 𝄞music
/// let mut v = [0xD834, 0xDD1E, 0x006d, 0x0075,
///              0x0073, 0x0069, 0x0063];
/// assert_eq!(str::from_utf16(v), Some("𝄞music".to_owned()));
///
/// // 𝄞mu<invalid>ic
/// v[4] = 0xD800;
/// assert_eq!(str::from_utf16(v), None);
/// ```
pub fn from_utf16(v: &[u16]) -> Option<~str> {
    let mut s = StrBuf::with_capacity(v.len() / 2);
    for c in utf16_items(v) {
        match c {
            ScalarValue(c) => s.push_char(c),
            LoneSurrogate(_) => return None
        }
    }
    Some(s.into_owned())
}

/// Decode a UTF-16 encoded vector `v` into a string, replacing
/// invalid data with the replacement character (U+FFFD).
///
/// # Example
/// ```rust
/// use std::str;
///
/// // 𝄞mus<invalid>ic<invalid>
/// let v = [0xD834, 0xDD1E, 0x006d, 0x0075,
///          0x0073, 0xDD1E, 0x0069, 0x0063,
///          0xD834];
///
/// assert_eq!(str::from_utf16_lossy(v),
///            "𝄞mus\uFFFDic\uFFFD".to_owned());
/// ```
pub fn from_utf16_lossy(v: &[u16]) -> ~str {
    utf16_items(v).map(|c| c.to_char_lossy()).collect()
}

// Return the initial codepoint accumulator for the first byte.
// The first byte is special, only want bottom 5 bits for width 2, 4 bits
// for width 3, and 3 bits for width 4
macro_rules! utf8_first_byte(
    ($byte:expr, $width:expr) => (($byte & (0x7F >> $width)) as u32)
)

// return the value of $ch updated with continuation byte $byte
macro_rules! utf8_acc_cont_byte(
    ($ch:expr, $byte:expr) => (($ch << 6) | ($byte & 63u8) as u32)
)

static TAG_CONT_U8: u8 = 128u8;

/// Converts a vector of bytes to a new utf-8 string.
/// Any invalid utf-8 sequences are replaced with U+FFFD REPLACEMENT CHARACTER.
///
/// # Example
///
/// ```rust
/// let input = bytes!("Hello ", 0xF0, 0x90, 0x80, "World");
/// let output = std::str::from_utf8_lossy(input);
/// assert_eq!(output.as_slice(), "Hello \uFFFDWorld");
/// ```
pub fn from_utf8_lossy<'a>(v: &'a [u8]) -> MaybeOwned<'a> {
    if is_utf8(v) {
        return Slice(unsafe { mem::transmute(v) })
    }

    static REPLACEMENT: &'static [u8] = bytes!(0xEF, 0xBF, 0xBD); // U+FFFD in UTF-8
    let mut i = 0;
    let total = v.len();
    fn unsafe_get(xs: &[u8], i: uint) -> u8 {
        unsafe { *xs.unsafe_ref(i) }
    }
    fn safe_get(xs: &[u8], i: uint, total: uint) -> u8 {
        if i >= total {
            0
        } else {
            unsafe_get(xs, i)
        }
    }

    let mut res = StrBuf::with_capacity(total);

    if i > 0 {
        unsafe {
            res.push_bytes(v.slice_to(i))
        };
    }

    // subseqidx is the index of the first byte of the subsequence we're looking at.
    // It's used to copy a bunch of contiguous good codepoints at once instead of copying
    // them one by one.
    let mut subseqidx = 0;

    while i < total {
        let i_ = i;
        let byte = unsafe_get(v, i);
        i += 1;

        macro_rules! error(() => ({
            unsafe {
                if subseqidx != i_ {
                    res.push_bytes(v.slice(subseqidx, i_));
                }
                subseqidx = i;
                res.push_bytes(REPLACEMENT);
            }
        }))

        if byte < 128u8 {
            // subseqidx handles this
        } else {
            let w = utf8_char_width(byte);

            match w {
                2 => {
                    if safe_get(v, i, total) & 192u8 != TAG_CONT_U8 {
                        error!();
                        continue;
                    }
                    i += 1;
                }
                3 => {
                    match (byte, safe_get(v, i, total)) {
                        (0xE0        , 0xA0 .. 0xBF) => (),
                        (0xE1 .. 0xEC, 0x80 .. 0xBF) => (),
                        (0xED        , 0x80 .. 0x9F) => (),
                        (0xEE .. 0xEF, 0x80 .. 0xBF) => (),
                        _ => {
                            error!();
                            continue;
                        }
                    }
                    i += 1;
                    if safe_get(v, i, total) & 192u8 != TAG_CONT_U8 {
                        error!();
                        continue;
                    }
                    i += 1;
                }
                4 => {
                    match (byte, safe_get(v, i, total)) {
                        (0xF0        , 0x90 .. 0xBF) => (),
                        (0xF1 .. 0xF3, 0x80 .. 0xBF) => (),
                        (0xF4        , 0x80 .. 0x8F) => (),
                        _ => {
                            error!();
                            continue;
                        }
                    }
                    i += 1;
                    if safe_get(v, i, total) & 192u8 != TAG_CONT_U8 {
                        error!();
                        continue;
                    }
                    i += 1;
                    if safe_get(v, i, total) & 192u8 != TAG_CONT_U8 {
                        error!();
                        continue;
                    }
                    i += 1;
                }
                _ => {
                    error!();
                    continue;
                }
            }
        }
    }
    if subseqidx < total {
        unsafe {
            res.push_bytes(v.slice(subseqidx, total))
        };
    }
    Owned(res.into_owned())
}

/*
Section: MaybeOwned
*/

/// A MaybeOwned is a string that can hold either a ~str or a &str.
/// This can be useful as an optimization when an allocation is sometimes
/// needed but not always.
pub enum MaybeOwned<'a> {
    /// A borrowed string
    Slice(&'a str),
    /// An owned string
    Owned(~str)
}

/// SendStr is a specialization of `MaybeOwned` to be sendable
pub type SendStr = MaybeOwned<'static>;

impl<'a> MaybeOwned<'a> {
    /// Returns `true` if this `MaybeOwned` wraps an owned string
    #[inline]
    pub fn is_owned(&self) -> bool {
        match *self {
            Slice(_) => false,
            Owned(_) => true
        }
    }

    /// Returns `true` if this `MaybeOwned` wraps a borrowed string
    #[inline]
    pub fn is_slice(&self) -> bool {
        match *self {
            Slice(_) => true,
            Owned(_) => false
        }
    }
}

/// Trait for moving into a `MaybeOwned`
pub trait IntoMaybeOwned<'a> {
    /// Moves self into a `MaybeOwned`
    fn into_maybe_owned(self) -> MaybeOwned<'a>;
}

impl<'a> IntoMaybeOwned<'a> for ~str {
    #[inline]
    fn into_maybe_owned(self) -> MaybeOwned<'a> { Owned(self) }
}

impl<'a> IntoMaybeOwned<'a> for &'a str {
    #[inline]
    fn into_maybe_owned(self) -> MaybeOwned<'a> { Slice(self) }
}

impl<'a> IntoMaybeOwned<'a> for MaybeOwned<'a> {
    #[inline]
    fn into_maybe_owned(self) -> MaybeOwned<'a> { self }
}

impl<'a> Eq for MaybeOwned<'a> {
    #[inline]
    fn eq(&self, other: &MaybeOwned) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<'a> TotalEq for MaybeOwned<'a> {}

impl<'a> Ord for MaybeOwned<'a> {
    #[inline]
    fn lt(&self, other: &MaybeOwned) -> bool {
        self.as_slice().lt(&other.as_slice())
    }
}

impl<'a> TotalOrd for MaybeOwned<'a> {
    #[inline]
    fn cmp(&self, other: &MaybeOwned) -> Ordering {
        self.as_slice().cmp(&other.as_slice())
    }
}

impl<'a, S: Str> Equiv<S> for MaybeOwned<'a> {
    #[inline]
    fn equiv(&self, other: &S) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<'a> Str for MaybeOwned<'a> {
    #[inline]
    fn as_slice<'b>(&'b self) -> &'b str {
        match *self {
            Slice(s) => s,
            Owned(ref s) => s.as_slice()
        }
    }
}

impl<'a> StrAllocating for MaybeOwned<'a> {
    #[inline]
    fn into_owned(self) -> ~str {
        match self {
            Slice(s) => s.to_owned(),
            Owned(s) => s
        }
    }
}

impl<'a> Container for MaybeOwned<'a> {
    #[inline]
    fn len(&self) -> uint { self.as_slice().len() }
}

impl<'a> Clone for MaybeOwned<'a> {
    #[inline]
    fn clone(&self) -> MaybeOwned<'a> {
        match *self {
            Slice(s) => Slice(s),
            Owned(ref s) => Owned(s.to_owned())
        }
    }
}

impl<'a> Default for MaybeOwned<'a> {
    #[inline]
    fn default() -> MaybeOwned<'a> { Slice("") }
}

impl<'a, H: Writer> ::hash::Hash<H> for MaybeOwned<'a> {
    #[inline]
    fn hash(&self, hasher: &mut H) {
        match *self {
            Slice(s) => s.hash(hasher),
            Owned(ref s) => s.hash(hasher),
        }
    }
}

impl<'a> fmt::Show for MaybeOwned<'a> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Slice(ref s) => s.fmt(f),
            Owned(ref s) => s.fmt(f)
        }
    }
}

/// Unsafe operations
pub mod raw {
    use libc;
    use mem;
    use ptr::RawPtr;
    use raw::Slice;
    use slice::CloneableVector;
    use str::{is_utf8, StrAllocating};

    pub use core::str::raw::{from_utf8, c_str_to_static_slice, slice_bytes};
    pub use core::str::raw::{slice_unchecked};

    /// Create a Rust string from a *u8 buffer of the given length
    pub unsafe fn from_buf_len(buf: *u8, len: uint) -> ~str {
        let v = Slice { data: buf, len: len };
        let bytes: &[u8] = ::mem::transmute(v);
        assert!(is_utf8(bytes));
        let s: &str = ::mem::transmute(bytes);
        s.to_owned()
    }

    #[lang="strdup_uniq"]
    #[cfg(not(test))]
    #[inline]
    unsafe fn strdup_uniq(ptr: *u8, len: uint) -> ~str {
        from_buf_len(ptr, len)
    }

    /// Create a Rust string from a null-terminated C string
    pub unsafe fn from_c_str(buf: *libc::c_char) -> ~str {
        let mut curr = buf;
        let mut i = 0;
        while *curr != 0 {
            i += 1;
            curr = buf.offset(i);
        }
        from_buf_len(buf as *u8, i as uint)
    }

    /// Converts an owned vector of bytes to a new owned string. This assumes
    /// that the utf-8-ness of the vector has already been validated
    #[inline]
    pub unsafe fn from_utf8_owned(v: ~[u8]) -> ~str {
        mem::transmute(v)
    }

    /// Converts a byte to a string.
    pub unsafe fn from_byte(u: u8) -> ~str { from_utf8_owned(box [u]) }

    /// Access the str in its vector representation.
    /// The caller must preserve the valid UTF-8 property when modifying.
    #[inline]
    pub unsafe fn as_owned_vec<'a>(s: &'a mut ~str) -> &'a mut ~[u8] {
        mem::transmute(s)
    }

    /// Sets the length of a string
    ///
    /// This will explicitly set the size of the string, without actually
    /// modifying its buffers, so it is up to the caller to ensure that
    /// the string is actually the specified size.
    #[test]
    fn test_from_buf_len() {
        use slice::ImmutableVector;
        use str::StrAllocating;

        unsafe {
            let a = ~[65u8, 65u8, 65u8, 65u8, 65u8, 65u8, 65u8, 0u8];
            let b = a.as_ptr();
            let c = from_buf_len(b, 3u);
            assert_eq!(c, "AAA".to_owned());
        }
    }
}

/*
Section: Trait implementations
*/

/// Any string that can be represented as a slice
pub trait StrAllocating: Str {
    /// Convert `self` into a ~str, not making a copy if possible.
    fn into_owned(self) -> ~str;

    /// Convert `self` into a `StrBuf`.
    #[inline]
    fn to_strbuf(&self) -> StrBuf {
        StrBuf::from_str(self.as_slice())
    }

    /// Convert `self` into a `StrBuf`, not making a copy if possible.
    #[inline]
    fn into_strbuf(self) -> StrBuf {
        StrBuf::from_owned_str(self.into_owned())
    }

    /// Escape each char in `s` with `char::escape_default`.
    fn escape_default(&self) -> ~str {
        let me = self.as_slice();
        let mut out = StrBuf::with_capacity(me.len());
        for c in me.chars() {
            c.escape_default(|c| out.push_char(c));
        }
        out.into_owned()
    }

    /// Escape each char in `s` with `char::escape_unicode`.
    fn escape_unicode(&self) -> ~str {
        let me = self.as_slice();
        let mut out = StrBuf::with_capacity(me.len());
        for c in me.chars() {
            c.escape_unicode(|c| out.push_char(c));
        }
        out.into_owned()
    }

    /// Replace all occurrences of one string with another.
    ///
    /// # Arguments
    ///
    /// * `from` - The string to replace
    /// * `to` - The replacement string
    ///
    /// # Return value
    ///
    /// The original string with all occurrences of `from` replaced with `to`.
    ///
    /// # Example
    ///
    /// ```rust
    /// let s = "Do you know the muffin man,
    /// The muffin man, the muffin man, ...".to_owned();
    ///
    /// assert_eq!(s.replace("muffin man", "little lamb"),
    ///            "Do you know the little lamb,
    /// The little lamb, the little lamb, ...".to_owned());
    ///
    /// // not found, so no change.
    /// assert_eq!(s.replace("cookie monster", "little lamb"), s);
    /// ```
    fn replace(&self, from: &str, to: &str) -> ~str {
        let me = self.as_slice();
        let mut result = StrBuf::new();
        let mut last_end = 0;
        for (start, end) in me.match_indices(from) {
            result.push_str(unsafe{raw::slice_bytes(me, last_end, start)});
            result.push_str(to);
            last_end = end;
        }
        result.push_str(unsafe{raw::slice_bytes(me, last_end, me.len())});
        result.into_owned()
    }

    /// Copy a slice into a new owned str.
    #[inline]
    fn to_owned(&self) -> ~str {
        use slice::Vector;

        unsafe {
            ::mem::transmute(self.as_slice().as_bytes().to_owned())
        }
    }

    /// Converts to a vector of `u16` encoded as UTF-16.
    fn to_utf16(&self) -> Vec<u16> {
        let me = self.as_slice();
        let mut u = Vec::new();
        for ch in me.chars() {
            let mut buf = [0u16, ..2];
            let n = ch.encode_utf16(buf /* as mut slice! */);
            u.push_all(buf.slice_to(n));
        }
        u
    }

    /// Given a string, make a new string with repeated copies of it.
    fn repeat(&self, nn: uint) -> ~str {
        let me = self.as_slice();
        let mut ret = StrBuf::with_capacity(nn * me.len());
        for _ in range(0, nn) {
            ret.push_str(me);
        }
        ret.into_owned()
    }

    /// Levenshtein Distance between two strings.
    fn lev_distance(&self, t: &str) -> uint {
        let me = self.as_slice();
        let slen = me.len();
        let tlen = t.len();

        if slen == 0 { return tlen; }
        if tlen == 0 { return slen; }

        let mut dcol = Vec::from_fn(tlen + 1, |x| x);

        for (i, sc) in me.chars().enumerate() {

            let mut current = i;
            *dcol.get_mut(0) = current + 1;

            for (j, tc) in t.chars().enumerate() {

                let next = *dcol.get(j + 1);

                if sc == tc {
                    *dcol.get_mut(j + 1) = current;
                } else {
                    *dcol.get_mut(j + 1) = ::cmp::min(current, next);
                    *dcol.get_mut(j + 1) = ::cmp::min(*dcol.get(j + 1),
                                                      *dcol.get(j)) + 1;
                }

                current = next;
            }
        }

        return *dcol.get(tlen);
    }

    /// An Iterator over the string in Unicode Normalization Form D
    /// (canonical decomposition).
    #[inline]
    fn nfd_chars<'a>(&'a self) -> Decompositions<'a> {
        Decompositions {
            iter: self.as_slice().chars(),
            buffer: Vec::new(),
            sorted: false,
            kind: Canonical
        }
    }

    /// An Iterator over the string in Unicode Normalization Form KD
    /// (compatibility decomposition).
    #[inline]
    fn nfkd_chars<'a>(&'a self) -> Decompositions<'a> {
        Decompositions {
            iter: self.as_slice().chars(),
            buffer: Vec::new(),
            sorted: false,
            kind: Compatible
        }
    }
}

impl<'a> StrAllocating for &'a str {
    #[inline]
    fn into_owned(self) -> ~str { self.to_owned() }
}

impl<'a> StrAllocating for ~str {
    #[inline]
    fn into_owned(self) -> ~str { self }
}

/// Methods for owned strings
pub trait OwnedStr {
    /// Consumes the string, returning the underlying byte buffer.
    ///
    /// The buffer does not have a null terminator.
    fn into_bytes(self) -> ~[u8];

    /// Pushes the given string onto this string, returning the concatenation of the two strings.
    fn append(self, rhs: &str) -> ~str;
}

impl OwnedStr for ~str {
    #[inline]
    fn into_bytes(self) -> ~[u8] {
        unsafe { mem::transmute(self) }
    }

    #[inline]
    fn append(self, rhs: &str) -> ~str {
        let mut new_str = StrBuf::from_owned_str(self);
        new_str.push_str(rhs);
        new_str.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use iter::AdditiveIterator;
    use default::Default;
    use prelude::*;
    use str::*;
    use strbuf::StrBuf;

    #[test]
    fn test_eq() {
        assert!((eq(&"".to_owned(), &"".to_owned())));
        assert!((eq(&"foo".to_owned(), &"foo".to_owned())));
        assert!((!eq(&"foo".to_owned(), &"bar".to_owned())));
    }

    #[test]
    fn test_eq_slice() {
        assert!((eq_slice("foobar".slice(0, 3), "foo")));
        assert!((eq_slice("barfoo".slice(3, 6), "foo")));
        assert!((!eq_slice("foo1", "foo2")));
    }

    #[test]
    fn test_le() {
        assert!("" <= "");
        assert!("" <= "foo");
        assert!("foo" <= "foo");
        assert!("foo" != "bar");
    }

    #[test]
    fn test_len() {
        assert_eq!("".len(), 0u);
        assert_eq!("hello world".len(), 11u);
        assert_eq!("\x63".len(), 1u);
        assert_eq!("\xa2".len(), 2u);
        assert_eq!("\u03c0".len(), 2u);
        assert_eq!("\u2620".len(), 3u);
        assert_eq!("\U0001d11e".len(), 4u);

        assert_eq!("".char_len(), 0u);
        assert_eq!("hello world".char_len(), 11u);
        assert_eq!("\x63".char_len(), 1u);
        assert_eq!("\xa2".char_len(), 1u);
        assert_eq!("\u03c0".char_len(), 1u);
        assert_eq!("\u2620".char_len(), 1u);
        assert_eq!("\U0001d11e".char_len(), 1u);
        assert_eq!("ประเทศไทย中华Việt Nam".char_len(), 19u);
    }

    #[test]
    fn test_find() {
        assert_eq!("hello".find('l'), Some(2u));
        assert_eq!("hello".find(|c:char| c == 'o'), Some(4u));
        assert!("hello".find('x').is_none());
        assert!("hello".find(|c:char| c == 'x').is_none());
        assert_eq!("ประเทศไทย中华Việt Nam".find('华'), Some(30u));
        assert_eq!("ประเทศไทย中华Việt Nam".find(|c: char| c == '华'), Some(30u));
    }

    #[test]
    fn test_rfind() {
        assert_eq!("hello".rfind('l'), Some(3u));
        assert_eq!("hello".rfind(|c:char| c == 'o'), Some(4u));
        assert!("hello".rfind('x').is_none());
        assert!("hello".rfind(|c:char| c == 'x').is_none());
        assert_eq!("ประเทศไทย中华Việt Nam".rfind('华'), Some(30u));
        assert_eq!("ประเทศไทย中华Việt Nam".rfind(|c: char| c == '华'), Some(30u));
    }

    #[test]
    fn test_collect() {
        let empty = "".to_owned();
        let s: ~str = empty.chars().collect();
        assert_eq!(empty, s);
        let data = "ประเทศไทย中".to_owned();
        let s: ~str = data.chars().collect();
        assert_eq!(data, s);
    }

    #[test]
    fn test_into_bytes() {
        let data = "asdf".to_owned();
        let buf = data.into_bytes();
        assert_eq!(bytes!("asdf"), buf.as_slice());
    }

    #[test]
    fn test_find_str() {
        // byte positions
        assert_eq!("".find_str(""), Some(0u));
        assert!("banana".find_str("apple pie").is_none());

        let data = "abcabc";
        assert_eq!(data.slice(0u, 6u).find_str("ab"), Some(0u));
        assert_eq!(data.slice(2u, 6u).find_str("ab"), Some(3u - 2u));
        assert!(data.slice(2u, 4u).find_str("ab").is_none());

        let mut data = "ประเทศไทย中华Việt Nam".to_owned();
        data = data + data;
        assert!(data.find_str("ไท华").is_none());
        assert_eq!(data.slice(0u, 43u).find_str(""), Some(0u));
        assert_eq!(data.slice(6u, 43u).find_str(""), Some(6u - 6u));

        assert_eq!(data.slice(0u, 43u).find_str("ประ"), Some( 0u));
        assert_eq!(data.slice(0u, 43u).find_str("ทศไ"), Some(12u));
        assert_eq!(data.slice(0u, 43u).find_str("ย中"), Some(24u));
        assert_eq!(data.slice(0u, 43u).find_str("iệt"), Some(34u));
        assert_eq!(data.slice(0u, 43u).find_str("Nam"), Some(40u));

        assert_eq!(data.slice(43u, 86u).find_str("ประ"), Some(43u - 43u));
        assert_eq!(data.slice(43u, 86u).find_str("ทศไ"), Some(55u - 43u));
        assert_eq!(data.slice(43u, 86u).find_str("ย中"), Some(67u - 43u));
        assert_eq!(data.slice(43u, 86u).find_str("iệt"), Some(77u - 43u));
        assert_eq!(data.slice(43u, 86u).find_str("Nam"), Some(83u - 43u));
    }

    #[test]
    fn test_slice_chars() {
        fn t(a: &str, b: &str, start: uint) {
            assert_eq!(a.slice_chars(start, start + b.char_len()), b);
        }
        t("", "", 0);
        t("hello", "llo", 2);
        t("hello", "el", 1);
        t("αβλ", "β", 1);
        t("αβλ", "", 3);
        assert_eq!("ะเทศไท", "ประเทศไทย中华Việt Nam".slice_chars(2, 8));
    }

    #[test]
    fn test_concat() {
        fn t(v: &[~str], s: &str) {
            assert_eq!(v.concat(), s.to_str());
        }
        t(["you".to_owned(), "know".to_owned(), "I'm".to_owned(),
          "no".to_owned(), "good".to_owned()], "youknowI'mnogood");
        let v: &[~str] = [];
        t(v, "");
        t(["hi".to_owned()], "hi");
    }

    #[test]
    fn test_connect() {
        fn t(v: &[~str], sep: &str, s: &str) {
            assert_eq!(v.connect(sep), s.to_str());
        }
        t(["you".to_owned(), "know".to_owned(), "I'm".to_owned(),
           "no".to_owned(), "good".to_owned()],
          " ", "you know I'm no good");
        let v: &[~str] = [];
        t(v, " ", "");
        t(["hi".to_owned()], " ", "hi");
    }

    #[test]
    fn test_concat_slices() {
        fn t(v: &[&str], s: &str) {
            assert_eq!(v.concat(), s.to_str());
        }
        t(["you", "know", "I'm", "no", "good"], "youknowI'mnogood");
        let v: &[&str] = [];
        t(v, "");
        t(["hi"], "hi");
    }

    #[test]
    fn test_connect_slices() {
        fn t(v: &[&str], sep: &str, s: &str) {
            assert_eq!(v.connect(sep), s.to_str());
        }
        t(["you", "know", "I'm", "no", "good"],
          " ", "you know I'm no good");
        t([], " ", "");
        t(["hi"], " ", "hi");
    }

    #[test]
    fn test_repeat() {
        assert_eq!("x".repeat(4), "xxxx".to_owned());
        assert_eq!("hi".repeat(4), "hihihihi".to_owned());
        assert_eq!("ไท华".repeat(3), "ไท华ไท华ไท华".to_owned());
        assert_eq!("".repeat(4), "".to_owned());
        assert_eq!("hi".repeat(0), "".to_owned());
    }

    #[test]
    fn test_unsafe_slice() {
        assert_eq!("ab", unsafe {raw::slice_bytes("abc", 0, 2)});
        assert_eq!("bc", unsafe {raw::slice_bytes("abc", 1, 3)});
        assert_eq!("", unsafe {raw::slice_bytes("abc", 1, 1)});
        fn a_million_letter_a() -> ~str {
            let mut i = 0;
            let mut rs = StrBuf::new();
            while i < 100000 {
                rs.push_str("aaaaaaaaaa");
                i += 1;
            }
            rs.into_owned()
        }
        fn half_a_million_letter_a() -> ~str {
            let mut i = 0;
            let mut rs = StrBuf::new();
            while i < 100000 {
                rs.push_str("aaaaa");
                i += 1;
            }
            rs.into_owned()
        }
        let letters = a_million_letter_a();
        assert!(half_a_million_letter_a() ==
            unsafe {raw::slice_bytes(letters, 0u, 500000)}.to_owned());
    }

    #[test]
    fn test_starts_with() {
        assert!(("".starts_with("")));
        assert!(("abc".starts_with("")));
        assert!(("abc".starts_with("a")));
        assert!((!"a".starts_with("abc")));
        assert!((!"".starts_with("abc")));
        assert!((!"ödd".starts_with("-")));
        assert!(("ödd".starts_with("öd")));
    }

    #[test]
    fn test_ends_with() {
        assert!(("".ends_with("")));
        assert!(("abc".ends_with("")));
        assert!(("abc".ends_with("c")));
        assert!((!"a".ends_with("abc")));
        assert!((!"".ends_with("abc")));
        assert!((!"ddö".ends_with("-")));
        assert!(("ddö".ends_with("dö")));
    }

    #[test]
    fn test_is_empty() {
        assert!("".is_empty());
        assert!(!"a".is_empty());
    }

    #[test]
    fn test_replace() {
        let a = "a";
        assert_eq!("".replace(a, "b"), "".to_owned());
        assert_eq!("a".replace(a, "b"), "b".to_owned());
        assert_eq!("ab".replace(a, "b"), "bb".to_owned());
        let test = "test";
        assert!(" test test ".replace(test, "toast") ==
            " toast toast ".to_owned());
        assert_eq!(" test test ".replace(test, ""), "   ".to_owned());
    }

    #[test]
    fn test_replace_2a() {
        let data = "ประเทศไทย中华".to_owned();
        let repl = "دولة الكويت".to_owned();

        let a = "ประเ".to_owned();
        let a2 = "دولة الكويتทศไทย中华".to_owned();
        assert_eq!(data.replace(a, repl), a2);
    }

    #[test]
    fn test_replace_2b() {
        let data = "ประเทศไทย中华".to_owned();
        let repl = "دولة الكويت".to_owned();

        let b = "ะเ".to_owned();
        let b2 = "ปรدولة الكويتทศไทย中华".to_owned();
        assert_eq!(data.replace(b, repl), b2);
    }

    #[test]
    fn test_replace_2c() {
        let data = "ประเทศไทย中华".to_owned();
        let repl = "دولة الكويت".to_owned();

        let c = "中华".to_owned();
        let c2 = "ประเทศไทยدولة الكويت".to_owned();
        assert_eq!(data.replace(c, repl), c2);
    }

    #[test]
    fn test_replace_2d() {
        let data = "ประเทศไทย中华".to_owned();
        let repl = "دولة الكويت".to_owned();

        let d = "ไท华".to_owned();
        assert_eq!(data.replace(d, repl), data);
    }

    #[test]
    fn test_slice() {
        assert_eq!("ab", "abc".slice(0, 2));
        assert_eq!("bc", "abc".slice(1, 3));
        assert_eq!("", "abc".slice(1, 1));
        assert_eq!("\u65e5", "\u65e5\u672c".slice(0, 3));

        let data = "ประเทศไทย中华";
        assert_eq!("ป", data.slice(0, 3));
        assert_eq!("ร", data.slice(3, 6));
        assert_eq!("", data.slice(3, 3));
        assert_eq!("华", data.slice(30, 33));

        fn a_million_letter_X() -> ~str {
            let mut i = 0;
            let mut rs = StrBuf::new();
            while i < 100000 {
                rs.push_str("华华华华华华华华华华");
                i += 1;
            }
            rs.into_owned()
        }
        fn half_a_million_letter_X() -> ~str {
            let mut i = 0;
            let mut rs = StrBuf::new();
            while i < 100000 {
                rs.push_str("华华华华华");
                i += 1;
            }
            rs.into_owned()
        }
        let letters = a_million_letter_X();
        assert!(half_a_million_letter_X() ==
            letters.slice(0u, 3u * 500000u).to_owned());
    }

    #[test]
    fn test_slice_2() {
        let ss = "中华Việt Nam";

        assert_eq!("华", ss.slice(3u, 6u));
        assert_eq!("Việt Nam", ss.slice(6u, 16u));

        assert_eq!("ab", "abc".slice(0u, 2u));
        assert_eq!("bc", "abc".slice(1u, 3u));
        assert_eq!("", "abc".slice(1u, 1u));

        assert_eq!("中", ss.slice(0u, 3u));
        assert_eq!("华V", ss.slice(3u, 7u));
        assert_eq!("", ss.slice(3u, 3u));
        /*0: 中
          3: 华
          6: V
          7: i
          8: ệ
         11: t
         12:
         13: N
         14: a
         15: m */
    }

    #[test]
    #[should_fail]
    fn test_slice_fail() {
        "中华Việt Nam".slice(0u, 2u);
    }

    #[test]
    fn test_slice_from() {
        assert_eq!("abcd".slice_from(0), "abcd");
        assert_eq!("abcd".slice_from(2), "cd");
        assert_eq!("abcd".slice_from(4), "");
    }
    #[test]
    fn test_slice_to() {
        assert_eq!("abcd".slice_to(0), "");
        assert_eq!("abcd".slice_to(2), "ab");
        assert_eq!("abcd".slice_to(4), "abcd");
    }

    #[test]
    fn test_trim_left_chars() {
        let v: &[char] = &[];
        assert_eq!(" *** foo *** ".trim_left_chars(v), " *** foo *** ");
        assert_eq!(" *** foo *** ".trim_left_chars(&['*', ' ']), "foo *** ");
        assert_eq!(" ***  *** ".trim_left_chars(&['*', ' ']), "");
        assert_eq!("foo *** ".trim_left_chars(&['*', ' ']), "foo *** ");

        assert_eq!("11foo1bar11".trim_left_chars('1'), "foo1bar11");
        assert_eq!("12foo1bar12".trim_left_chars(&['1', '2']), "foo1bar12");
        assert_eq!("123foo1bar123".trim_left_chars(|c: char| c.is_digit()), "foo1bar123");
    }

    #[test]
    fn test_trim_right_chars() {
        let v: &[char] = &[];
        assert_eq!(" *** foo *** ".trim_right_chars(v), " *** foo *** ");
        assert_eq!(" *** foo *** ".trim_right_chars(&['*', ' ']), " *** foo");
        assert_eq!(" ***  *** ".trim_right_chars(&['*', ' ']), "");
        assert_eq!(" *** foo".trim_right_chars(&['*', ' ']), " *** foo");

        assert_eq!("11foo1bar11".trim_right_chars('1'), "11foo1bar");
        assert_eq!("12foo1bar12".trim_right_chars(&['1', '2']), "12foo1bar");
        assert_eq!("123foo1bar123".trim_right_chars(|c: char| c.is_digit()), "123foo1bar");
    }

    #[test]
    fn test_trim_chars() {
        let v: &[char] = &[];
        assert_eq!(" *** foo *** ".trim_chars(v), " *** foo *** ");
        assert_eq!(" *** foo *** ".trim_chars(&['*', ' ']), "foo");
        assert_eq!(" ***  *** ".trim_chars(&['*', ' ']), "");
        assert_eq!("foo".trim_chars(&['*', ' ']), "foo");

        assert_eq!("11foo1bar11".trim_chars('1'), "foo1bar");
        assert_eq!("12foo1bar12".trim_chars(&['1', '2']), "foo1bar");
        assert_eq!("123foo1bar123".trim_chars(|c: char| c.is_digit()), "foo1bar");
    }

    #[test]
    fn test_trim_left() {
        assert_eq!("".trim_left(), "");
        assert_eq!("a".trim_left(), "a");
        assert_eq!("    ".trim_left(), "");
        assert_eq!("     blah".trim_left(), "blah");
        assert_eq!("   \u3000  wut".trim_left(), "wut");
        assert_eq!("hey ".trim_left(), "hey ");
    }

    #[test]
    fn test_trim_right() {
        assert_eq!("".trim_right(), "");
        assert_eq!("a".trim_right(), "a");
        assert_eq!("    ".trim_right(), "");
        assert_eq!("blah     ".trim_right(), "blah");
        assert_eq!("wut   \u3000  ".trim_right(), "wut");
        assert_eq!(" hey".trim_right(), " hey");
    }

    #[test]
    fn test_trim() {
        assert_eq!("".trim(), "");
        assert_eq!("a".trim(), "a");
        assert_eq!("    ".trim(), "");
        assert_eq!("    blah     ".trim(), "blah");
        assert_eq!("\nwut   \u3000  ".trim(), "wut");
        assert_eq!(" hey dude ".trim(), "hey dude");
    }

    #[test]
    fn test_is_whitespace() {
        assert!("".is_whitespace());
        assert!(" ".is_whitespace());
        assert!("\u2009".is_whitespace()); // Thin space
        assert!("  \n\t   ".is_whitespace());
        assert!(!"   _   ".is_whitespace());
    }

    #[test]
    fn test_slice_shift_char() {
        let data = "ประเทศไทย中";
        assert_eq!(data.slice_shift_char(), (Some('ป'), "ระเทศไทย中"));
    }

    #[test]
    fn test_slice_shift_char_2() {
        let empty = "";
        assert_eq!(empty.slice_shift_char(), (None, ""));
    }

    #[test]
    fn test_is_utf8() {
        // deny overlong encodings
        assert!(!is_utf8([0xc0, 0x80]));
        assert!(!is_utf8([0xc0, 0xae]));
        assert!(!is_utf8([0xe0, 0x80, 0x80]));
        assert!(!is_utf8([0xe0, 0x80, 0xaf]));
        assert!(!is_utf8([0xe0, 0x81, 0x81]));
        assert!(!is_utf8([0xf0, 0x82, 0x82, 0xac]));
        assert!(!is_utf8([0xf4, 0x90, 0x80, 0x80]));

        // deny surrogates
        assert!(!is_utf8([0xED, 0xA0, 0x80]));
        assert!(!is_utf8([0xED, 0xBF, 0xBF]));

        assert!(is_utf8([0xC2, 0x80]));
        assert!(is_utf8([0xDF, 0xBF]));
        assert!(is_utf8([0xE0, 0xA0, 0x80]));
        assert!(is_utf8([0xED, 0x9F, 0xBF]));
        assert!(is_utf8([0xEE, 0x80, 0x80]));
        assert!(is_utf8([0xEF, 0xBF, 0xBF]));
        assert!(is_utf8([0xF0, 0x90, 0x80, 0x80]));
        assert!(is_utf8([0xF4, 0x8F, 0xBF, 0xBF]));
    }

    #[test]
    fn test_is_utf16() {
        macro_rules! pos ( ($($e:expr),*) => { { $(assert!(is_utf16($e));)* } });

        // non-surrogates
        pos!([0x0000],
             [0x0001, 0x0002],
             [0xD7FF],
             [0xE000]);

        // surrogate pairs (randomly generated with Python 3's
        // .encode('utf-16be'))
        pos!([0xdb54, 0xdf16, 0xd880, 0xdee0, 0xdb6a, 0xdd45],
             [0xd91f, 0xdeb1, 0xdb31, 0xdd84, 0xd8e2, 0xde14],
             [0xdb9f, 0xdc26, 0xdb6f, 0xde58, 0xd850, 0xdfae]);

        // mixtures (also random)
        pos!([0xd921, 0xdcc2, 0x002d, 0x004d, 0xdb32, 0xdf65],
             [0xdb45, 0xdd2d, 0x006a, 0xdacd, 0xddfe, 0x0006],
             [0x0067, 0xd8ff, 0xddb7, 0x000f, 0xd900, 0xdc80]);

        // negative tests
        macro_rules! neg ( ($($e:expr),*) => { { $(assert!(!is_utf16($e));)* } });

        neg!(
            // surrogate + regular unit
            [0xdb45, 0x0000],
            // surrogate + lead surrogate
            [0xd900, 0xd900],
            // unterminated surrogate
            [0xd8ff],
            // trail surrogate without a lead
            [0xddb7]);

        // random byte sequences that Python 3's .decode('utf-16be')
        // failed on
        neg!([0x5b3d, 0x0141, 0xde9e, 0x8fdc, 0xc6e7],
             [0xdf5a, 0x82a5, 0x62b9, 0xb447, 0x92f3],
             [0xda4e, 0x42bc, 0x4462, 0xee98, 0xc2ca],
             [0xbe00, 0xb04a, 0x6ecb, 0xdd89, 0xe278],
             [0x0465, 0xab56, 0xdbb6, 0xa893, 0x665e],
             [0x6b7f, 0x0a19, 0x40f4, 0xa657, 0xdcc5],
             [0x9b50, 0xda5e, 0x24ec, 0x03ad, 0x6dee],
             [0x8d17, 0xcaa7, 0xf4ae, 0xdf6e, 0xbed7],
             [0xdaee, 0x2584, 0x7d30, 0xa626, 0x121a],
             [0xd956, 0x4b43, 0x7570, 0xccd6, 0x4f4a],
             [0x9dcf, 0x1b49, 0x4ba5, 0xfce9, 0xdffe],
             [0x6572, 0xce53, 0xb05a, 0xf6af, 0xdacf],
             [0x1b90, 0x728c, 0x9906, 0xdb68, 0xf46e],
             [0x1606, 0xbeca, 0xbe76, 0x860f, 0xdfa5],
             [0x8b4f, 0xde7a, 0xd220, 0x9fac, 0x2b6f],
             [0xb8fe, 0xebbe, 0xda32, 0x1a5f, 0x8b8b],
             [0x934b, 0x8956, 0xc434, 0x1881, 0xddf7],
             [0x5a95, 0x13fc, 0xf116, 0xd89b, 0x93f9],
             [0xd640, 0x71f1, 0xdd7d, 0x77eb, 0x1cd8],
             [0x348b, 0xaef0, 0xdb2c, 0xebf1, 0x1282],
             [0x50d7, 0xd824, 0x5010, 0xb369, 0x22ea]);
    }

    #[test]
    fn test_raw_from_c_str() {
        unsafe {
            let a = box [65, 65, 65, 65, 65, 65, 65, 0];
            let b = a.as_ptr();
            let c = raw::from_c_str(b);
            assert_eq!(c, "AAAAAAA".to_owned());
        }
    }

    #[test]
    fn test_as_bytes() {
        // no null
        let v = [
            224, 184, 168, 224, 185, 132, 224, 184, 151, 224, 184, 162, 228,
            184, 173, 229, 141, 142, 86, 105, 225, 187, 135, 116, 32, 78, 97,
            109
        ];
        assert_eq!("".as_bytes(), &[]);
        assert_eq!("abc".as_bytes(), &['a' as u8, 'b' as u8, 'c' as u8]);
        assert_eq!("ศไทย中华Việt Nam".as_bytes(), v.as_slice());
    }

    #[test]
    #[should_fail]
    fn test_as_bytes_fail() {
        // Don't double free. (I'm not sure if this exercises the
        // original problem code path anymore.)
        let s = "".to_owned();
        let _bytes = s.as_bytes();
        fail!();
    }

    #[test]
    fn test_as_ptr() {
        let buf = "hello".as_ptr();
        unsafe {
            assert_eq!(*buf.offset(0), 'h' as u8);
            assert_eq!(*buf.offset(1), 'e' as u8);
            assert_eq!(*buf.offset(2), 'l' as u8);
            assert_eq!(*buf.offset(3), 'l' as u8);
            assert_eq!(*buf.offset(4), 'o' as u8);
        }
    }

    #[test]
    fn test_subslice_offset() {
        let a = "kernelsprite";
        let b = a.slice(7, a.len());
        let c = a.slice(0, a.len() - 6);
        assert_eq!(a.subslice_offset(b), 7);
        assert_eq!(a.subslice_offset(c), 0);

        let string = "a\nb\nc";
        let lines: Vec<&str> = string.lines().collect();
        let lines = lines.as_slice();
        assert_eq!(string.subslice_offset(lines[0]), 0);
        assert_eq!(string.subslice_offset(lines[1]), 2);
        assert_eq!(string.subslice_offset(lines[2]), 4);
    }

    #[test]
    #[should_fail]
    fn test_subslice_offset_2() {
        let a = "alchemiter";
        let b = "cruxtruder";
        a.subslice_offset(b);
    }

    #[test]
    fn vec_str_conversions() {
        let s1: ~str = "All mimsy were the borogoves".to_owned();

        let v: ~[u8] = s1.as_bytes().to_owned();
        let s2: ~str = from_utf8(v).unwrap().to_owned();
        let mut i: uint = 0u;
        let n1: uint = s1.len();
        let n2: uint = v.len();
        assert_eq!(n1, n2);
        while i < n1 {
            let a: u8 = s1[i];
            let b: u8 = s2[i];
            debug!("{}", a);
            debug!("{}", b);
            assert_eq!(a, b);
            i += 1u;
        }
    }

    #[test]
    fn test_contains() {
        assert!("abcde".contains("bcd"));
        assert!("abcde".contains("abcd"));
        assert!("abcde".contains("bcde"));
        assert!("abcde".contains(""));
        assert!("".contains(""));
        assert!(!"abcde".contains("def"));
        assert!(!"".contains("a"));

        let data = "ประเทศไทย中华Việt Nam".to_owned();
        assert!(data.contains("ประเ"));
        assert!(data.contains("ะเ"));
        assert!(data.contains("中华"));
        assert!(!data.contains("ไท华"));
    }

    #[test]
    fn test_contains_char() {
        assert!("abc".contains_char('b'));
        assert!("a".contains_char('a'));
        assert!(!"abc".contains_char('d'));
        assert!(!"".contains_char('a'));
    }

    #[test]
    fn test_utf16() {
        let pairs =
            [("𐍅𐌿𐌻𐍆𐌹𐌻𐌰\n".to_owned(),
              vec![0xd800_u16, 0xdf45_u16, 0xd800_u16, 0xdf3f_u16,
                0xd800_u16, 0xdf3b_u16, 0xd800_u16, 0xdf46_u16,
                0xd800_u16, 0xdf39_u16, 0xd800_u16, 0xdf3b_u16,
                0xd800_u16, 0xdf30_u16, 0x000a_u16]),

             ("𐐒𐑉𐐮𐑀𐐲𐑋 𐐏𐐲𐑍\n".to_owned(),
              vec![0xd801_u16, 0xdc12_u16, 0xd801_u16,
                0xdc49_u16, 0xd801_u16, 0xdc2e_u16, 0xd801_u16,
                0xdc40_u16, 0xd801_u16, 0xdc32_u16, 0xd801_u16,
                0xdc4b_u16, 0x0020_u16, 0xd801_u16, 0xdc0f_u16,
                0xd801_u16, 0xdc32_u16, 0xd801_u16, 0xdc4d_u16,
                0x000a_u16]),

             ("𐌀𐌖𐌋𐌄𐌑𐌉·𐌌𐌄𐌕𐌄𐌋𐌉𐌑\n".to_owned(),
              vec![0xd800_u16, 0xdf00_u16, 0xd800_u16, 0xdf16_u16,
                0xd800_u16, 0xdf0b_u16, 0xd800_u16, 0xdf04_u16,
                0xd800_u16, 0xdf11_u16, 0xd800_u16, 0xdf09_u16,
                0x00b7_u16, 0xd800_u16, 0xdf0c_u16, 0xd800_u16,
                0xdf04_u16, 0xd800_u16, 0xdf15_u16, 0xd800_u16,
                0xdf04_u16, 0xd800_u16, 0xdf0b_u16, 0xd800_u16,
                0xdf09_u16, 0xd800_u16, 0xdf11_u16, 0x000a_u16 ]),

             ("𐒋𐒘𐒈𐒑𐒛𐒒 𐒕𐒓 𐒈𐒚𐒍 𐒏𐒜𐒒𐒖𐒆 𐒕𐒆\n".to_owned(),
              vec![0xd801_u16, 0xdc8b_u16, 0xd801_u16, 0xdc98_u16,
                0xd801_u16, 0xdc88_u16, 0xd801_u16, 0xdc91_u16,
                0xd801_u16, 0xdc9b_u16, 0xd801_u16, 0xdc92_u16,
                0x0020_u16, 0xd801_u16, 0xdc95_u16, 0xd801_u16,
                0xdc93_u16, 0x0020_u16, 0xd801_u16, 0xdc88_u16,
                0xd801_u16, 0xdc9a_u16, 0xd801_u16, 0xdc8d_u16,
                0x0020_u16, 0xd801_u16, 0xdc8f_u16, 0xd801_u16,
                0xdc9c_u16, 0xd801_u16, 0xdc92_u16, 0xd801_u16,
                0xdc96_u16, 0xd801_u16, 0xdc86_u16, 0x0020_u16,
                0xd801_u16, 0xdc95_u16, 0xd801_u16, 0xdc86_u16,
                0x000a_u16 ]),
             // Issue #12318, even-numbered non-BMP planes
             ("\U00020000".to_owned(),
              vec![0xD840, 0xDC00])];

        for p in pairs.iter() {
            let (s, u) = (*p).clone();
            assert!(is_utf16(u.as_slice()));
            assert_eq!(s.to_utf16(), u);

            assert_eq!(from_utf16(u.as_slice()).unwrap(), s);
            assert_eq!(from_utf16_lossy(u.as_slice()), s);

            assert_eq!(from_utf16(s.to_utf16().as_slice()).unwrap(), s);
            assert_eq!(from_utf16(u.as_slice()).unwrap().to_utf16(), u);
        }
    }

    #[test]
    fn test_utf16_invalid() {
        // completely positive cases tested above.
        // lead + eof
        assert_eq!(from_utf16([0xD800]), None);
        // lead + lead
        assert_eq!(from_utf16([0xD800, 0xD800]), None);

        // isolated trail
        assert_eq!(from_utf16([0x0061, 0xDC00]), None);

        // general
        assert_eq!(from_utf16([0xD800, 0xd801, 0xdc8b, 0xD800]), None);
    }

    #[test]
    fn test_utf16_lossy() {
        // completely positive cases tested above.
        // lead + eof
        assert_eq!(from_utf16_lossy([0xD800]), "\uFFFD".to_owned());
        // lead + lead
        assert_eq!(from_utf16_lossy([0xD800, 0xD800]), "\uFFFD\uFFFD".to_owned());

        // isolated trail
        assert_eq!(from_utf16_lossy([0x0061, 0xDC00]), "a\uFFFD".to_owned());

        // general
        assert_eq!(from_utf16_lossy([0xD800, 0xd801, 0xdc8b, 0xD800]), "\uFFFD𐒋\uFFFD".to_owned());
    }

    #[test]
    fn test_truncate_utf16_at_nul() {
        let v = [];
        assert_eq!(truncate_utf16_at_nul(v), &[]);

        let v = [0, 2, 3];
        assert_eq!(truncate_utf16_at_nul(v), &[]);

        let v = [1, 0, 3];
        assert_eq!(truncate_utf16_at_nul(v), &[1]);

        let v = [1, 2, 0];
        assert_eq!(truncate_utf16_at_nul(v), &[1, 2]);

        let v = [1, 2, 3];
        assert_eq!(truncate_utf16_at_nul(v), &[1, 2, 3]);
    }

    #[test]
    fn test_char_at() {
        let s = "ศไทย中华Việt Nam".to_owned();
        let v = box ['ศ','ไ','ท','ย','中','华','V','i','ệ','t',' ','N','a','m'];
        let mut pos = 0;
        for ch in v.iter() {
            assert!(s.char_at(pos) == *ch);
            pos += from_char(*ch).len();
        }
    }

    #[test]
    fn test_char_at_reverse() {
        let s = "ศไทย中华Việt Nam".to_owned();
        let v = box ['ศ','ไ','ท','ย','中','华','V','i','ệ','t',' ','N','a','m'];
        let mut pos = s.len();
        for ch in v.iter().rev() {
            assert!(s.char_at_reverse(pos) == *ch);
            pos -= from_char(*ch).len();
        }
    }

    #[test]
    fn test_escape_unicode() {
        assert_eq!("abc".escape_unicode(), "\\x61\\x62\\x63".to_owned());
        assert_eq!("a c".escape_unicode(), "\\x61\\x20\\x63".to_owned());
        assert_eq!("\r\n\t".escape_unicode(), "\\x0d\\x0a\\x09".to_owned());
        assert_eq!("'\"\\".escape_unicode(), "\\x27\\x22\\x5c".to_owned());
        assert_eq!("\x00\x01\xfe\xff".escape_unicode(), "\\x00\\x01\\xfe\\xff".to_owned());
        assert_eq!("\u0100\uffff".escape_unicode(), "\\u0100\\uffff".to_owned());
        assert_eq!("\U00010000\U0010ffff".escape_unicode(), "\\U00010000\\U0010ffff".to_owned());
        assert_eq!("ab\ufb00".escape_unicode(), "\\x61\\x62\\ufb00".to_owned());
        assert_eq!("\U0001d4ea\r".escape_unicode(), "\\U0001d4ea\\x0d".to_owned());
    }

    #[test]
    fn test_escape_default() {
        assert_eq!("abc".escape_default(), "abc".to_owned());
        assert_eq!("a c".escape_default(), "a c".to_owned());
        assert_eq!("\r\n\t".escape_default(), "\\r\\n\\t".to_owned());
        assert_eq!("'\"\\".escape_default(), "\\'\\\"\\\\".to_owned());
        assert_eq!("\u0100\uffff".escape_default(), "\\u0100\\uffff".to_owned());
        assert_eq!("\U00010000\U0010ffff".escape_default(), "\\U00010000\\U0010ffff".to_owned());
        assert_eq!("ab\ufb00".escape_default(), "ab\\ufb00".to_owned());
        assert_eq!("\U0001d4ea\r".escape_default(), "\\U0001d4ea\\r".to_owned());
    }

    #[test]
    fn test_total_ord() {
        "1234".cmp(&("123")) == Greater;
        "123".cmp(&("1234")) == Less;
        "1234".cmp(&("1234")) == Equal;
        "12345555".cmp(&("123456")) == Less;
        "22".cmp(&("1234")) == Greater;
    }

    #[test]
    fn test_char_range_at() {
        let data = "b¢€𤭢𤭢€¢b".to_owned();
        assert_eq!('b', data.char_range_at(0).ch);
        assert_eq!('¢', data.char_range_at(1).ch);
        assert_eq!('€', data.char_range_at(3).ch);
        assert_eq!('𤭢', data.char_range_at(6).ch);
        assert_eq!('𤭢', data.char_range_at(10).ch);
        assert_eq!('€', data.char_range_at(14).ch);
        assert_eq!('¢', data.char_range_at(17).ch);
        assert_eq!('b', data.char_range_at(19).ch);
    }

    #[test]
    fn test_char_range_at_reverse_underflow() {
        assert_eq!("abc".char_range_at_reverse(0).next, 0);
    }

    #[test]
    fn test_add() {
        #![allow(unnecessary_allocation)]
        macro_rules! t (
            ($s1:expr, $s2:expr, $e:expr) => { {
                let s1 = $s1;
                let s2 = $s2;
                let e = $e;
                assert_eq!(s1 + s2, e.to_owned());
                assert_eq!(s1.to_owned() + s2, e.to_owned());
            } }
        );

        t!("foo",  "bar", "foobar");
        t!("foo", "bar".to_owned(), "foobar");
        t!("ศไทย中",  "华Việt Nam", "ศไทย中华Việt Nam");
        t!("ศไทย中", "华Việt Nam".to_owned(), "ศไทย中华Việt Nam");
    }

    #[test]
    fn test_iterator() {
        use iter::*;
        let s = "ศไทย中华Việt Nam".to_owned();
        let v = box ['ศ','ไ','ท','ย','中','华','V','i','ệ','t',' ','N','a','m'];

        let mut pos = 0;
        let mut it = s.chars();

        for c in it {
            assert_eq!(c, v[pos]);
            pos += 1;
        }
        assert_eq!(pos, v.len());
    }

    #[test]
    fn test_rev_iterator() {
        use iter::*;
        let s = "ศไทย中华Việt Nam".to_owned();
        let v = box ['m', 'a', 'N', ' ', 't', 'ệ','i','V','华','中','ย','ท','ไ','ศ'];

        let mut pos = 0;
        let mut it = s.chars().rev();

        for c in it {
            assert_eq!(c, v[pos]);
            pos += 1;
        }
        assert_eq!(pos, v.len());
    }

    #[test]
    fn test_iterator_clone() {
        let s = "ศไทย中华Việt Nam";
        let mut it = s.chars();
        it.next();
        assert!(it.zip(it.clone()).all(|(x,y)| x == y));
    }

    #[test]
    fn test_bytesator() {
        let s = "ศไทย中华Việt Nam".to_owned();
        let v = [
            224, 184, 168, 224, 185, 132, 224, 184, 151, 224, 184, 162, 228,
            184, 173, 229, 141, 142, 86, 105, 225, 187, 135, 116, 32, 78, 97,
            109
        ];
        let mut pos = 0;

        for b in s.bytes() {
            assert_eq!(b, v[pos]);
            pos += 1;
        }
    }

    #[test]
    fn test_bytes_revator() {
        let s = "ศไทย中华Việt Nam".to_owned();
        let v = [
            224, 184, 168, 224, 185, 132, 224, 184, 151, 224, 184, 162, 228,
            184, 173, 229, 141, 142, 86, 105, 225, 187, 135, 116, 32, 78, 97,
            109
        ];
        let mut pos = v.len();

        for b in s.bytes().rev() {
            pos -= 1;
            assert_eq!(b, v[pos]);
        }
    }

    #[test]
    fn test_char_indicesator() {
        use iter::*;
        let s = "ศไทย中华Việt Nam";
        let p = [0, 3, 6, 9, 12, 15, 18, 19, 20, 23, 24, 25, 26, 27];
        let v = ['ศ','ไ','ท','ย','中','华','V','i','ệ','t',' ','N','a','m'];

        let mut pos = 0;
        let mut it = s.char_indices();

        for c in it {
            assert_eq!(c, (p[pos], v[pos]));
            pos += 1;
        }
        assert_eq!(pos, v.len());
        assert_eq!(pos, p.len());
    }

    #[test]
    fn test_char_indices_revator() {
        use iter::*;
        let s = "ศไทย中华Việt Nam";
        let p = [27, 26, 25, 24, 23, 20, 19, 18, 15, 12, 9, 6, 3, 0];
        let v = ['m', 'a', 'N', ' ', 't', 'ệ','i','V','华','中','ย','ท','ไ','ศ'];

        let mut pos = 0;
        let mut it = s.char_indices().rev();

        for c in it {
            assert_eq!(c, (p[pos], v[pos]));
            pos += 1;
        }
        assert_eq!(pos, v.len());
        assert_eq!(pos, p.len());
    }

    #[test]
    fn test_split_char_iterator() {
        let data = "\nMäry häd ä little lämb\nLittle lämb\n";

        let split: Vec<&str> = data.split(' ').collect();
        assert_eq!( split, vec!["\nMäry", "häd", "ä", "little", "lämb\nLittle", "lämb\n"]);

        let mut rsplit: Vec<&str> = data.split(' ').rev().collect();
        rsplit.reverse();
        assert_eq!(rsplit, vec!["\nMäry", "häd", "ä", "little", "lämb\nLittle", "lämb\n"]);

        let split: Vec<&str> = data.split(|c: char| c == ' ').collect();
        assert_eq!( split, vec!["\nMäry", "häd", "ä", "little", "lämb\nLittle", "lämb\n"]);

        let mut rsplit: Vec<&str> = data.split(|c: char| c == ' ').rev().collect();
        rsplit.reverse();
        assert_eq!(rsplit, vec!["\nMäry", "häd", "ä", "little", "lämb\nLittle", "lämb\n"]);

        // Unicode
        let split: Vec<&str> = data.split('ä').collect();
        assert_eq!( split, vec!["\nM", "ry h", "d ", " little l", "mb\nLittle l", "mb\n"]);

        let mut rsplit: Vec<&str> = data.split('ä').rev().collect();
        rsplit.reverse();
        assert_eq!(rsplit, vec!["\nM", "ry h", "d ", " little l", "mb\nLittle l", "mb\n"]);

        let split: Vec<&str> = data.split(|c: char| c == 'ä').collect();
        assert_eq!( split, vec!["\nM", "ry h", "d ", " little l", "mb\nLittle l", "mb\n"]);

        let mut rsplit: Vec<&str> = data.split(|c: char| c == 'ä').rev().collect();
        rsplit.reverse();
        assert_eq!(rsplit, vec!["\nM", "ry h", "d ", " little l", "mb\nLittle l", "mb\n"]);
    }

    #[test]
    fn test_splitn_char_iterator() {
        let data = "\nMäry häd ä little lämb\nLittle lämb\n";

        let split: Vec<&str> = data.splitn(' ', 3).collect();
        assert_eq!(split, vec!["\nMäry", "häd", "ä", "little lämb\nLittle lämb\n"]);

        let split: Vec<&str> = data.splitn(|c: char| c == ' ', 3).collect();
        assert_eq!(split, vec!["\nMäry", "häd", "ä", "little lämb\nLittle lämb\n"]);

        // Unicode
        let split: Vec<&str> = data.splitn('ä', 3).collect();
        assert_eq!(split, vec!["\nM", "ry h", "d ", " little lämb\nLittle lämb\n"]);

        let split: Vec<&str> = data.splitn(|c: char| c == 'ä', 3).collect();
        assert_eq!(split, vec!["\nM", "ry h", "d ", " little lämb\nLittle lämb\n"]);
    }

    #[test]
    fn test_rsplitn_char_iterator() {
        let data = "\nMäry häd ä little lämb\nLittle lämb\n";

        let mut split: Vec<&str> = data.rsplitn(' ', 3).collect();
        split.reverse();
        assert_eq!(split, vec!["\nMäry häd ä", "little", "lämb\nLittle", "lämb\n"]);

        let mut split: Vec<&str> = data.rsplitn(|c: char| c == ' ', 3).collect();
        split.reverse();
        assert_eq!(split, vec!["\nMäry häd ä", "little", "lämb\nLittle", "lämb\n"]);

        // Unicode
        let mut split: Vec<&str> = data.rsplitn('ä', 3).collect();
        split.reverse();
        assert_eq!(split, vec!["\nMäry häd ", " little l", "mb\nLittle l", "mb\n"]);

        let mut split: Vec<&str> = data.rsplitn(|c: char| c == 'ä', 3).collect();
        split.reverse();
        assert_eq!(split, vec!["\nMäry häd ", " little l", "mb\nLittle l", "mb\n"]);
    }

    #[test]
    fn test_split_char_iterator_no_trailing() {
        let data = "\nMäry häd ä little lämb\nLittle lämb\n";

        let split: Vec<&str> = data.split('\n').collect();
        assert_eq!(split, vec!["", "Märy häd ä little lämb", "Little lämb", ""]);

        let split: Vec<&str> = data.split_terminator('\n').collect();
        assert_eq!(split, vec!["", "Märy häd ä little lämb", "Little lämb"]);
    }

    #[test]
    fn test_rev_split_char_iterator_no_trailing() {
        let data = "\nMäry häd ä little lämb\nLittle lämb\n";

        let mut split: Vec<&str> = data.split('\n').rev().collect();
        split.reverse();
        assert_eq!(split, vec!["", "Märy häd ä little lämb", "Little lämb", ""]);

        let mut split: Vec<&str> = data.split_terminator('\n').rev().collect();
        split.reverse();
        assert_eq!(split, vec!["", "Märy häd ä little lämb", "Little lämb"]);
    }

    #[test]
    fn test_words() {
        let data = "\n \tMäry   häd\tä  little lämb\nLittle lämb\n";
        let words: Vec<&str> = data.words().collect();
        assert_eq!(words, vec!["Märy", "häd", "ä", "little", "lämb", "Little", "lämb"])
    }

    #[test]
    fn test_nfd_chars() {
        assert_eq!("abc".nfd_chars().collect::<~str>(), "abc".to_owned());
        assert_eq!("\u1e0b\u01c4".nfd_chars().collect::<~str>(), "d\u0307\u01c4".to_owned());
        assert_eq!("\u2026".nfd_chars().collect::<~str>(), "\u2026".to_owned());
        assert_eq!("\u2126".nfd_chars().collect::<~str>(), "\u03a9".to_owned());
        assert_eq!("\u1e0b\u0323".nfd_chars().collect::<~str>(), "d\u0323\u0307".to_owned());
        assert_eq!("\u1e0d\u0307".nfd_chars().collect::<~str>(), "d\u0323\u0307".to_owned());
        assert_eq!("a\u0301".nfd_chars().collect::<~str>(), "a\u0301".to_owned());
        assert_eq!("\u0301a".nfd_chars().collect::<~str>(), "\u0301a".to_owned());
        assert_eq!("\ud4db".nfd_chars().collect::<~str>(), "\u1111\u1171\u11b6".to_owned());
        assert_eq!("\uac1c".nfd_chars().collect::<~str>(), "\u1100\u1162".to_owned());
    }

    #[test]
    fn test_nfkd_chars() {
        assert_eq!("abc".nfkd_chars().collect::<~str>(), "abc".to_owned());
        assert_eq!("\u1e0b\u01c4".nfkd_chars().collect::<~str>(), "d\u0307DZ\u030c".to_owned());
        assert_eq!("\u2026".nfkd_chars().collect::<~str>(), "...".to_owned());
        assert_eq!("\u2126".nfkd_chars().collect::<~str>(), "\u03a9".to_owned());
        assert_eq!("\u1e0b\u0323".nfkd_chars().collect::<~str>(), "d\u0323\u0307".to_owned());
        assert_eq!("\u1e0d\u0307".nfkd_chars().collect::<~str>(), "d\u0323\u0307".to_owned());
        assert_eq!("a\u0301".nfkd_chars().collect::<~str>(), "a\u0301".to_owned());
        assert_eq!("\u0301a".nfkd_chars().collect::<~str>(), "\u0301a".to_owned());
        assert_eq!("\ud4db".nfkd_chars().collect::<~str>(), "\u1111\u1171\u11b6".to_owned());
        assert_eq!("\uac1c".nfkd_chars().collect::<~str>(), "\u1100\u1162".to_owned());
    }

    #[test]
    fn test_lines() {
        let data = "\nMäry häd ä little lämb\n\nLittle lämb\n";
        let lines: Vec<&str> = data.lines().collect();
        assert_eq!(lines, vec!["", "Märy häd ä little lämb", "", "Little lämb"]);

        let data = "\nMäry häd ä little lämb\n\nLittle lämb"; // no trailing \n
        let lines: Vec<&str> = data.lines().collect();
        assert_eq!(lines, vec!["", "Märy häd ä little lämb", "", "Little lämb"]);
    }

    #[test]
    fn test_split_strator() {
        fn t(s: &str, sep: &str, u: &[&str]) {
            let v: Vec<&str> = s.split_str(sep).collect();
            assert_eq!(v.as_slice(), u.as_slice());
        }
        t("--1233345--", "12345", ["--1233345--"]);
        t("abc::hello::there", "::", ["abc", "hello", "there"]);
        t("::hello::there", "::", ["", "hello", "there"]);
        t("hello::there::", "::", ["hello", "there", ""]);
        t("::hello::there::", "::", ["", "hello", "there", ""]);
        t("ประเทศไทย中华Việt Nam", "中华", ["ประเทศไทย", "Việt Nam"]);
        t("zzXXXzzYYYzz", "zz", ["", "XXX", "YYY", ""]);
        t("zzXXXzYYYz", "XXX", ["zz", "zYYYz"]);
        t(".XXX.YYY.", ".", ["", "XXX", "YYY", ""]);
        t("", ".", [""]);
        t("zz", "zz", ["",""]);
        t("ok", "z", ["ok"]);
        t("zzz", "zz", ["","z"]);
        t("zzzzz", "zz", ["","","z"]);
    }

    #[test]
    fn test_str_default() {
        use default::Default;
        fn t<S: Default + Str>() {
            let s: S = Default::default();
            assert_eq!(s.as_slice(), "");
        }

        t::<&str>();
        t::<~str>();
    }

    #[test]
    fn test_str_container() {
        fn sum_len<S: Container>(v: &[S]) -> uint {
            v.iter().map(|x| x.len()).sum()
        }

        let s = "01234".to_owned();
        assert_eq!(5, sum_len(["012", "", "34"]));
        assert_eq!(5, sum_len(["01".to_owned(), "2".to_owned(), "34".to_owned(), "".to_owned()]));
        assert_eq!(5, sum_len([s.as_slice()]));
    }

    #[test]
    fn test_str_from_utf8() {
        let xs = bytes!("hello");
        assert_eq!(from_utf8(xs), Some("hello"));

        let xs = bytes!("ศไทย中华Việt Nam");
        assert_eq!(from_utf8(xs), Some("ศไทย中华Việt Nam"));

        let xs = bytes!("hello", 0xff);
        assert_eq!(from_utf8(xs), None);
    }

    #[test]
    fn test_str_from_utf8_owned() {
        let xs = bytes!("hello").to_owned();
        assert_eq!(from_utf8_owned(xs), Some("hello".to_owned()));

        let xs = bytes!("ศไทย中华Việt Nam").to_owned();
        assert_eq!(from_utf8_owned(xs), Some("ศไทย中华Việt Nam".to_owned()));

        let xs = bytes!("hello", 0xff).to_owned();
        assert_eq!(from_utf8_owned(xs), None);
    }

    #[test]
    fn test_str_from_utf8_lossy() {
        let xs = bytes!("hello");
        assert_eq!(from_utf8_lossy(xs), Slice("hello"));

        let xs = bytes!("ศไทย中华Việt Nam");
        assert_eq!(from_utf8_lossy(xs), Slice("ศไทย中华Việt Nam"));

        let xs = bytes!("Hello", 0xC2, " There", 0xFF, " Goodbye");
        assert_eq!(from_utf8_lossy(xs), Owned("Hello\uFFFD There\uFFFD Goodbye".to_owned()));

        let xs = bytes!("Hello", 0xC0, 0x80, " There", 0xE6, 0x83, " Goodbye");
        assert_eq!(from_utf8_lossy(xs), Owned("Hello\uFFFD\uFFFD There\uFFFD Goodbye".to_owned()));

        let xs = bytes!(0xF5, "foo", 0xF5, 0x80, "bar");
        assert_eq!(from_utf8_lossy(xs), Owned("\uFFFDfoo\uFFFD\uFFFDbar".to_owned()));

        let xs = bytes!(0xF1, "foo", 0xF1, 0x80, "bar", 0xF1, 0x80, 0x80, "baz");
        assert_eq!(from_utf8_lossy(xs), Owned("\uFFFDfoo\uFFFDbar\uFFFDbaz".to_owned()));

        let xs = bytes!(0xF4, "foo", 0xF4, 0x80, "bar", 0xF4, 0xBF, "baz");
        assert_eq!(from_utf8_lossy(xs), Owned("\uFFFDfoo\uFFFDbar\uFFFD\uFFFDbaz".to_owned()));

        let xs = bytes!(0xF0, 0x80, 0x80, 0x80, "foo", 0xF0, 0x90, 0x80, 0x80, "bar");
        assert_eq!(from_utf8_lossy(xs), Owned("\uFFFD\uFFFD\uFFFD\uFFFD\
                                               foo\U00010000bar".to_owned()));

        // surrogates
        let xs = bytes!(0xED, 0xA0, 0x80, "foo", 0xED, 0xBF, 0xBF, "bar");
        assert_eq!(from_utf8_lossy(xs), Owned("\uFFFD\uFFFD\uFFFDfoo\
                                               \uFFFD\uFFFD\uFFFDbar".to_owned()));
    }

    #[test]
    fn test_from_str() {
      let owned: Option<~str> = from_str("string");
      assert_eq!(owned, Some("string".to_owned()));
    }

    #[test]
    fn test_maybe_owned_traits() {
        let s = Slice("abcde");
        assert_eq!(s.len(), 5);
        assert_eq!(s.as_slice(), "abcde");
        assert_eq!(s.to_str(), "abcde".to_owned());
        assert_eq!(format!("{}", s), "abcde".to_owned());
        assert!(s.lt(&Owned("bcdef".to_owned())));
        assert_eq!(Slice(""), Default::default());

        let o = Owned("abcde".to_owned());
        assert_eq!(o.len(), 5);
        assert_eq!(o.as_slice(), "abcde");
        assert_eq!(o.to_str(), "abcde".to_owned());
        assert_eq!(format!("{}", o), "abcde".to_owned());
        assert!(o.lt(&Slice("bcdef")));
        assert_eq!(Owned("".to_owned()), Default::default());

        assert!(s.cmp(&o) == Equal);
        assert!(s.equiv(&o));

        assert!(o.cmp(&s) == Equal);
        assert!(o.equiv(&s));
    }

    #[test]
    fn test_maybe_owned_methods() {
        let s = Slice("abcde");
        assert!(s.is_slice());
        assert!(!s.is_owned());

        let o = Owned("abcde".to_owned());
        assert!(!o.is_slice());
        assert!(o.is_owned());
    }

    #[test]
    fn test_maybe_owned_clone() {
        assert_eq!(Owned("abcde".to_owned()), Slice("abcde").clone());
        assert_eq!(Owned("abcde".to_owned()), Owned("abcde".to_owned()).clone());
        assert_eq!(Slice("abcde"), Slice("abcde").clone());
        assert_eq!(Slice("abcde"), Owned("abcde".to_owned()).clone());
    }

    #[test]
    fn test_maybe_owned_into_owned() {
        assert_eq!(Slice("abcde").into_owned(), "abcde".to_owned());
        assert_eq!(Owned("abcde".to_owned()).into_owned(), "abcde".to_owned());
    }

    #[test]
    fn test_into_maybe_owned() {
        assert_eq!("abcde".into_maybe_owned(), Slice("abcde"));
        assert_eq!(("abcde".to_owned()).into_maybe_owned(), Slice("abcde"));
        assert_eq!("abcde".into_maybe_owned(), Owned("abcde".to_owned()));
        assert_eq!(("abcde".to_owned()).into_maybe_owned(), Owned("abcde".to_owned()));
    }
}

#[cfg(test)]
mod bench {
    extern crate test;
    use self::test::Bencher;
    use super::*;
    use prelude::*;

    #[bench]
    fn char_iterator(b: &mut Bencher) {
        let s = "ศไทย中华Việt Nam; Mary had a little lamb, Little lamb";
        let len = s.char_len();

        b.iter(|| assert_eq!(s.chars().len(), len));
    }

    #[bench]
    fn char_iterator_ascii(b: &mut Bencher) {
        let s = "Mary had a little lamb, Little lamb
        Mary had a little lamb, Little lamb
        Mary had a little lamb, Little lamb
        Mary had a little lamb, Little lamb
        Mary had a little lamb, Little lamb
        Mary had a little lamb, Little lamb";
        let len = s.char_len();

        b.iter(|| assert_eq!(s.chars().len(), len));
    }

    #[bench]
    fn char_iterator_rev(b: &mut Bencher) {
        let s = "ศไทย中华Việt Nam; Mary had a little lamb, Little lamb";
        let len = s.char_len();

        b.iter(|| assert_eq!(s.chars().rev().len(), len));
    }

    #[bench]
    fn char_indicesator(b: &mut Bencher) {
        let s = "ศไทย中华Việt Nam; Mary had a little lamb, Little lamb";
        let len = s.char_len();

        b.iter(|| assert_eq!(s.char_indices().len(), len));
    }

    #[bench]
    fn char_indicesator_rev(b: &mut Bencher) {
        let s = "ศไทย中华Việt Nam; Mary had a little lamb, Little lamb";
        let len = s.char_len();

        b.iter(|| assert_eq!(s.char_indices().rev().len(), len));
    }

    #[bench]
    fn split_unicode_ascii(b: &mut Bencher) {
        let s = "ประเทศไทย中华Việt Namประเทศไทย中华Việt Nam";

        b.iter(|| assert_eq!(s.split('V').len(), 3));
    }

    #[bench]
    fn split_unicode_not_ascii(b: &mut Bencher) {
        struct NotAscii(char);
        impl CharEq for NotAscii {
            fn matches(&mut self, c: char) -> bool {
                let NotAscii(cc) = *self;
                cc == c
            }
            fn only_ascii(&self) -> bool { false }
        }
        let s = "ประเทศไทย中华Việt Namประเทศไทย中华Việt Nam";

        b.iter(|| assert_eq!(s.split(NotAscii('V')).len(), 3));
    }


    #[bench]
    fn split_ascii(b: &mut Bencher) {
        let s = "Mary had a little lamb, Little lamb, little-lamb.";
        let len = s.split(' ').len();

        b.iter(|| assert_eq!(s.split(' ').len(), len));
    }

    #[bench]
    fn split_not_ascii(b: &mut Bencher) {
        struct NotAscii(char);
        impl CharEq for NotAscii {
            #[inline]
            fn matches(&mut self, c: char) -> bool {
                let NotAscii(cc) = *self;
                cc == c
            }
            fn only_ascii(&self) -> bool { false }
        }
        let s = "Mary had a little lamb, Little lamb, little-lamb.";
        let len = s.split(' ').len();

        b.iter(|| assert_eq!(s.split(NotAscii(' ')).len(), len));
    }

    #[bench]
    fn split_extern_fn(b: &mut Bencher) {
        let s = "Mary had a little lamb, Little lamb, little-lamb.";
        let len = s.split(' ').len();
        fn pred(c: char) -> bool { c == ' ' }

        b.iter(|| assert_eq!(s.split(pred).len(), len));
    }

    #[bench]
    fn split_closure(b: &mut Bencher) {
        let s = "Mary had a little lamb, Little lamb, little-lamb.";
        let len = s.split(' ').len();

        b.iter(|| assert_eq!(s.split(|c: char| c == ' ').len(), len));
    }

    #[bench]
    fn split_slice(b: &mut Bencher) {
        let s = "Mary had a little lamb, Little lamb, little-lamb.";
        let len = s.split(' ').len();

        b.iter(|| assert_eq!(s.split(&[' ']).len(), len));
    }

    #[bench]
    fn is_utf8_100_ascii(b: &mut Bencher) {

        let s = bytes!("Hello there, the quick brown fox jumped over the lazy dog! \
                        Lorem ipsum dolor sit amet, consectetur. ");

        assert_eq!(100, s.len());
        b.iter(|| {
            is_utf8(s)
        });
    }

    #[bench]
    fn is_utf8_100_multibyte(b: &mut Bencher) {
        let s = bytes!("𐌀𐌖𐌋𐌄𐌑𐌉ปรدولة الكويتทศไทย中华𐍅𐌿𐌻𐍆𐌹𐌻𐌰");
        assert_eq!(100, s.len());
        b.iter(|| {
            is_utf8(s)
        });
    }

    #[bench]
    fn from_utf8_lossy_100_ascii(b: &mut Bencher) {
        let s = bytes!("Hello there, the quick brown fox jumped over the lazy dog! \
                        Lorem ipsum dolor sit amet, consectetur. ");

        assert_eq!(100, s.len());
        b.iter(|| {
            let _ = from_utf8_lossy(s);
        });
    }

    #[bench]
    fn from_utf8_lossy_100_multibyte(b: &mut Bencher) {
        let s = bytes!("𐌀𐌖𐌋𐌄𐌑𐌉ปรدولة الكويتทศไทย中华𐍅𐌿𐌻𐍆𐌹𐌻𐌰");
        assert_eq!(100, s.len());
        b.iter(|| {
            let _ = from_utf8_lossy(s);
        });
    }

    #[bench]
    fn from_utf8_lossy_invalid(b: &mut Bencher) {
        let s = bytes!("Hello", 0xC0, 0x80, " There", 0xE6, 0x83, " Goodbye");
        b.iter(|| {
            let _ = from_utf8_lossy(s);
        });
    }

    #[bench]
    fn from_utf8_lossy_100_invalid(b: &mut Bencher) {
        let s = Vec::from_elem(100, 0xF5u8);
        b.iter(|| {
            let _ = from_utf8_lossy(s.as_slice());
        });
    }

    #[bench]
    fn bench_connect(b: &mut Bencher) {
        let s = "ศไทย中华Việt Nam; Mary had a little lamb, Little lamb";
        let sep = "→";
        let v = [s, s, s, s, s, s, s, s, s, s];
        b.iter(|| {
            assert_eq!(v.connect(sep).len(), s.len() * 10 + sep.len() * 9);
        })
    }
}
