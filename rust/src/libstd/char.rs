// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Character manipulation (`char` type, Unicode Scalar Value)
//!
//! This module  provides the `Char` trait, as well as its implementation
//! for the primitive `char` type, in order to allow basic character manipulation.
//!
//! A `char` actually represents a
//! *[Unicode Scalar Value](http://www.unicode.org/glossary/#unicode_scalar_value)*,
//! as it can contain any Unicode code point except high-surrogate and
//! low-surrogate code points.
//!
//! As such, only values in the ranges \[0x0,0xD7FF\] and \[0xE000,0x10FFFF\]
//! (inclusive) are allowed. A `char` can always be safely cast to a `u32`;
//! however the converse is not always true due to the above range limits
//! and, as such, should be performed via the `from_u32` function..


use cast::transmute;
use option::{None, Option, Some};
use iter::{Iterator, range_step};
use str::StrSlice;
use unicode::{derived_property, property, general_category, decompose, conversions};

#[cfg(test)] use str::OwnedStr;

#[cfg(not(test))] use cmp::{Eq, Ord};
#[cfg(not(test))] use default::Default;

// UTF-8 ranges and tags for encoding characters
static TAG_CONT: uint = 128u;
static MAX_ONE_B: uint = 128u;
static TAG_TWO_B: uint = 192u;
static MAX_TWO_B: uint = 2048u;
static TAG_THREE_B: uint = 224u;
static MAX_THREE_B: uint = 65536u;
static TAG_FOUR_B: uint = 240u;

/*
    Lu  Uppercase_Letter        an uppercase letter
    Ll  Lowercase_Letter        a lowercase letter
    Lt  Titlecase_Letter        a digraphic character, with first part uppercase
    Lm  Modifier_Letter         a modifier letter
    Lo  Other_Letter            other letters, including syllables and ideographs
    Mn  Nonspacing_Mark         a nonspacing combining mark (zero advance width)
    Mc  Spacing_Mark            a spacing combining mark (positive advance width)
    Me  Enclosing_Mark          an enclosing combining mark
    Nd  Decimal_Number          a decimal digit
    Nl  Letter_Number           a letterlike numeric character
    No  Other_Number            a numeric character of other type
    Pc  Connector_Punctuation   a connecting punctuation mark, like a tie
    Pd  Dash_Punctuation        a dash or hyphen punctuation mark
    Ps  Open_Punctuation        an opening punctuation mark (of a pair)
    Pe  Close_Punctuation       a closing punctuation mark (of a pair)
    Pi  Initial_Punctuation     an initial quotation mark
    Pf  Final_Punctuation       a final quotation mark
    Po  Other_Punctuation       a punctuation mark of other type
    Sm  Math_Symbol             a symbol of primarily mathematical use
    Sc  Currency_Symbol         a currency sign
    Sk  Modifier_Symbol         a non-letterlike modifier symbol
    So  Other_Symbol            a symbol of other type
    Zs  Space_Separator         a space character (of various non-zero widths)
    Zl  Line_Separator          U+2028 LINE SEPARATOR only
    Zp  Paragraph_Separator     U+2029 PARAGRAPH SEPARATOR only
    Cc  Control                 a C0 or C1 control code
    Cf  Format                  a format control character
    Cs  Surrogate               a surrogate code point
    Co  Private_Use             a private-use character
    Cn  Unassigned              a reserved unassigned code point or a noncharacter
*/

/// The highest valid code point
pub static MAX: char = '\U0010ffff';

/// Converts from `u32` to a `char`
#[inline]
pub fn from_u32(i: u32) -> Option<char> {
    // catch out-of-bounds and surrogates
    if (i > MAX as u32) || (i >= 0xD800 && i <= 0xDFFF) {
        None
    } else {
        Some(unsafe { transmute(i) })
    }
}

/// Returns whether the specified `char` is considered a Unicode alphabetic
/// code point
pub fn is_alphabetic(c: char) -> bool   { derived_property::Alphabetic(c) }

/// Returns whether the specified `char` satisfies the 'XID_Start' Unicode property
///
/// 'XID_Start' is a Unicode Derived Property specified in
/// [UAX #31](http://unicode.org/reports/tr31/#NFKC_Modifications),
/// mostly similar to ID_Start but modified for closure under NFKx.
pub fn is_XID_start(c: char) -> bool    { derived_property::XID_Start(c) }

/// Returns whether the specified `char` satisfies the 'XID_Continue' Unicode property
///
/// 'XID_Continue' is a Unicode Derived Property specified in
/// [UAX #31](http://unicode.org/reports/tr31/#NFKC_Modifications),
/// mostly similar to 'ID_Continue' but modified for closure under NFKx.
pub fn is_XID_continue(c: char) -> bool { derived_property::XID_Continue(c) }

///
/// Indicates whether a `char` is in lower case
///
/// This is defined according to the terms of the Unicode Derived Core Property 'Lowercase'.
///
#[inline]
pub fn is_lowercase(c: char) -> bool { derived_property::Lowercase(c) }

///
/// Indicates whether a `char` is in upper case
///
/// This is defined according to the terms of the Unicode Derived Core Property 'Uppercase'.
///
#[inline]
pub fn is_uppercase(c: char) -> bool { derived_property::Uppercase(c) }

///
/// Indicates whether a `char` is whitespace
///
/// Whitespace is defined in terms of the Unicode Property 'White_Space'.
///
#[inline]
pub fn is_whitespace(c: char) -> bool {
    // As an optimization ASCII whitespace characters are checked separately
    c == ' '
        || ('\x09' <= c && c <= '\x0d')
        || property::White_Space(c)
}

///
/// Indicates whether a `char` is alphanumeric
///
/// Alphanumericness is defined in terms of the Unicode General Categories
/// 'Nd', 'Nl', 'No' and the Derived Core Property 'Alphabetic'.
///
#[inline]
pub fn is_alphanumeric(c: char) -> bool {
    derived_property::Alphabetic(c)
        || general_category::Nd(c)
        || general_category::Nl(c)
        || general_category::No(c)
}

///
/// Indicates whether a `char` is a control code point
///
/// Control code points are defined in terms of the Unicode General Category
/// 'Cc'.
///
#[inline]
pub fn is_control(c: char) -> bool { general_category::Cc(c) }

/// Indicates whether the `char` is numeric (Nd, Nl, or No)
#[inline]
pub fn is_digit(c: char) -> bool {
    general_category::Nd(c)
        || general_category::Nl(c)
        || general_category::No(c)
}

///
/// Checks if a `char` parses as a numeric digit in the given radix
///
/// Compared to `is_digit()`, this function only recognizes the
/// characters `0-9`, `a-z` and `A-Z`.
///
/// # Return value
///
/// Returns `true` if `c` is a valid digit under `radix`, and `false`
/// otherwise.
///
/// # Failure
///
/// Fails if given a `radix` > 36.
///
/// # Note
///
/// This just wraps `to_digit()`.
///
#[inline]
pub fn is_digit_radix(c: char, radix: uint) -> bool {
    match to_digit(c, radix) {
        Some(_) => true,
        None    => false,
    }
}

///
/// Converts a `char` to the corresponding digit
///
/// # Return value
///
/// If `c` is between '0' and '9', the corresponding value
/// between 0 and 9. If `c` is 'a' or 'A', 10. If `c` is
/// 'b' or 'B', 11, etc. Returns none if the `char` does not
/// refer to a digit in the given radix.
///
/// # Failure
///
/// Fails if given a `radix` outside the range `[0..36]`.
///
#[inline]
pub fn to_digit(c: char, radix: uint) -> Option<uint> {
    if radix > 36 {
        fail!("to_digit: radix {} is too high (maximum 36)", radix);
    }
    let val = match c {
      '0' .. '9' => c as uint - ('0' as uint),
      'a' .. 'z' => c as uint + 10u - ('a' as uint),
      'A' .. 'Z' => c as uint + 10u - ('A' as uint),
      _ => return None,
    };
    if val < radix { Some(val) }
    else { None }
}

/// Convert a char to its uppercase equivalent
///
/// The case-folding performed is the common or simple mapping:
/// it maps one unicode codepoint (one char in Rust) to its uppercase equivalent according
/// to the Unicode database at ftp://ftp.unicode.org/Public/UNIDATA/UnicodeData.txt
/// The additional SpecialCasing.txt is not considered here, as it expands to multiple
/// codepoints in some cases.
///
/// A full reference can be found here
/// http://www.unicode.org/versions/Unicode4.0.0/ch03.pdf#G33992
///
/// # Return value
///
/// Returns the char itself if no conversion was made
#[inline]
pub fn to_uppercase(c: char) -> char {
    conversions::to_upper(c)
}

/// Convert a char to its lowercase equivalent
///
/// The case-folding performed is the common or simple mapping
/// see `to_uppercase` for references and more information
///
/// # Return value
///
/// Returns the char itself if no conversion if possible
#[inline]
pub fn to_lowercase(c: char) -> char {
    conversions::to_lower(c)
}

///
/// Converts a number to the character representing it
///
/// # Return value
///
/// Returns `Some(char)` if `num` represents one digit under `radix`,
/// using one character of `0-9` or `a-z`, or `None` if it doesn't.
///
/// # Failure
///
/// Fails if given an `radix` > 36.
///
#[inline]
pub fn from_digit(num: uint, radix: uint) -> Option<char> {
    if radix > 36 {
        fail!("from_digit: radix {} is to high (maximum 36)", num);
    }
    if num < radix {
        unsafe {
            if num < 10 {
                Some(transmute(('0' as uint + num) as u32))
            } else {
                Some(transmute(('a' as uint + num - 10u) as u32))
            }
        }
    } else {
        None
    }
}

// Constants from Unicode 6.2.0 Section 3.12 Conjoining Jamo Behavior
static S_BASE: uint = 0xAC00;
static L_BASE: uint = 0x1100;
static V_BASE: uint = 0x1161;
static T_BASE: uint = 0x11A7;
static L_COUNT: uint = 19;
static V_COUNT: uint = 21;
static T_COUNT: uint = 28;
static N_COUNT: uint = (V_COUNT * T_COUNT);
static S_COUNT: uint = (L_COUNT * N_COUNT);

// Decompose a precomposed Hangul syllable
fn decompose_hangul(s: char, f: |char|) {
    let si = s as uint - S_BASE;

    let li = si / N_COUNT;
    unsafe {
        f(transmute((L_BASE + li) as u32));

        let vi = (si % N_COUNT) / T_COUNT;
        f(transmute((V_BASE + vi) as u32));

        let ti = si % T_COUNT;
        if ti > 0 {
            f(transmute((T_BASE + ti) as u32));
        }
    }
}

/// Returns the canonical decomposition of a character
pub fn decompose_canonical(c: char, f: |char|) {
    if (c as uint) < S_BASE || (c as uint) >= (S_BASE + S_COUNT) {
        decompose::canonical(c, f);
    } else {
        decompose_hangul(c, f);
    }
}

/// Returns the compatibility decomposition of a character
pub fn decompose_compatible(c: char, f: |char|) {
    if (c as uint) < S_BASE || (c as uint) >= (S_BASE + S_COUNT) {
        decompose::compatibility(c, f);
    } else {
        decompose_hangul(c, f);
    }
}

///
/// Returns the hexadecimal Unicode escape of a `char`
///
/// The rules are as follows:
///
/// - chars in [0,0xff] get 2-digit escapes: `\\xNN`
/// - chars in [0x100,0xffff] get 4-digit escapes: `\\uNNNN`
/// - chars above 0x10000 get 8-digit escapes: `\\UNNNNNNNN`
///
pub fn escape_unicode(c: char, f: |char|) {
    // avoid calling str::to_str_radix because we don't really need to allocate
    // here.
    f('\\');
    let pad = match () {
        _ if c <= '\xff'    => { f('x'); 2 }
        _ if c <= '\uffff'  => { f('u'); 4 }
        _                   => { f('U'); 8 }
    };
    for offset in range_step::<i32>(4 * (pad - 1), -1, -4) {
        unsafe {
            match ((c as i32) >> offset) & 0xf {
                i @ 0 .. 9 => { f(transmute('0' as i32 + i)); }
                i => { f(transmute('a' as i32 + (i - 10))); }
            }
        }
    }
}

///
/// Returns a 'default' ASCII and C++11-like literal escape of a `char`
///
/// The default is chosen with a bias toward producing literals that are
/// legal in a variety of languages, including C++11 and similar C-family
/// languages. The exact rules are:
///
/// - Tab, CR and LF are escaped as '\t', '\r' and '\n' respectively.
/// - Single-quote, double-quote and backslash chars are backslash-escaped.
/// - Any other chars in the range [0x20,0x7e] are not escaped.
/// - Any other chars are given hex unicode escapes; see `escape_unicode`.
///
pub fn escape_default(c: char, f: |char|) {
    match c {
        '\t' => { f('\\'); f('t'); }
        '\r' => { f('\\'); f('r'); }
        '\n' => { f('\\'); f('n'); }
        '\\' => { f('\\'); f('\\'); }
        '\'' => { f('\\'); f('\''); }
        '"'  => { f('\\'); f('"'); }
        '\x20' .. '\x7e' => { f(c); }
        _ => c.escape_unicode(f),
    }
}

/// Returns the amount of bytes this `char` would need if encoded in UTF-8
pub fn len_utf8_bytes(c: char) -> uint {
    static MAX_ONE_B:   uint = 128u;
    static MAX_TWO_B:   uint = 2048u;
    static MAX_THREE_B: uint = 65536u;
    static MAX_FOUR_B:  uint = 2097152u;

    let code = c as uint;
    match () {
        _ if code < MAX_ONE_B   => 1u,
        _ if code < MAX_TWO_B   => 2u,
        _ if code < MAX_THREE_B => 3u,
        _ if code < MAX_FOUR_B  => 4u,
        _                       => fail!("invalid character!"),
    }
}

#[allow(missing_doc)]
pub trait Char {
    fn is_alphabetic(&self) -> bool;
    fn is_XID_start(&self) -> bool;
    fn is_XID_continue(&self) -> bool;
    fn is_lowercase(&self) -> bool;
    fn is_uppercase(&self) -> bool;
    fn is_whitespace(&self) -> bool;
    fn is_alphanumeric(&self) -> bool;
    fn is_control(&self) -> bool;
    fn is_digit(&self) -> bool;
    fn is_digit_radix(&self, radix: uint) -> bool;
    fn to_digit(&self, radix: uint) -> Option<uint>;
    fn to_lowercase(&self) -> char;
    fn to_uppercase(&self) -> char;
    fn from_digit(num: uint, radix: uint) -> Option<char>;
    fn escape_unicode(&self, f: |char|);
    fn escape_default(&self, f: |char|);
    fn len_utf8_bytes(&self) -> uint;

    /// Encodes this `char` as utf-8 into the provided byte-buffer
    ///
    /// The buffer must be at least 4 bytes long or a runtime failure will occur.
    ///
    /// This will then return the number of characters written to the slice.
    fn encode_utf8(&self, dst: &mut [u8]) -> uint;
}

impl Char for char {
    fn is_alphabetic(&self) -> bool { is_alphabetic(*self) }

    fn is_XID_start(&self) -> bool { is_XID_start(*self) }

    fn is_XID_continue(&self) -> bool { is_XID_continue(*self) }

    fn is_lowercase(&self) -> bool { is_lowercase(*self) }

    fn is_uppercase(&self) -> bool { is_uppercase(*self) }

    fn is_whitespace(&self) -> bool { is_whitespace(*self) }

    fn is_alphanumeric(&self) -> bool { is_alphanumeric(*self) }

    fn is_control(&self) -> bool { is_control(*self) }

    fn is_digit(&self) -> bool { is_digit(*self) }

    fn is_digit_radix(&self, radix: uint) -> bool { is_digit_radix(*self, radix) }

    fn to_digit(&self, radix: uint) -> Option<uint> { to_digit(*self, radix) }

    fn to_lowercase(&self) -> char { to_lowercase(*self) }

    fn to_uppercase(&self) -> char { to_uppercase(*self) }

    fn from_digit(num: uint, radix: uint) -> Option<char> { from_digit(num, radix) }

    fn escape_unicode(&self, f: |char|) { escape_unicode(*self, f) }

    fn escape_default(&self, f: |char|) { escape_default(*self, f) }

    fn len_utf8_bytes(&self) -> uint { len_utf8_bytes(*self) }

    fn encode_utf8<'a>(&self, dst: &'a mut [u8]) -> uint {
        let code = *self as uint;
        if code < MAX_ONE_B {
            dst[0] = code as u8;
            return 1;
        } else if code < MAX_TWO_B {
            dst[0] = (code >> 6u & 31u | TAG_TWO_B) as u8;
            dst[1] = (code & 63u | TAG_CONT) as u8;
            return 2;
        } else if code < MAX_THREE_B {
            dst[0] = (code >> 12u & 15u | TAG_THREE_B) as u8;
            dst[1] = (code >> 6u & 63u | TAG_CONT) as u8;
            dst[2] = (code & 63u | TAG_CONT) as u8;
            return 3;
        } else {
            dst[0] = (code >> 18u & 7u | TAG_FOUR_B) as u8;
            dst[1] = (code >> 12u & 63u | TAG_CONT) as u8;
            dst[2] = (code >> 6u & 63u | TAG_CONT) as u8;
            dst[3] = (code & 63u | TAG_CONT) as u8;
            return 4;
        }
    }
}

#[cfg(not(test))]
impl Eq for char {
    #[inline]
    fn eq(&self, other: &char) -> bool { (*self) == (*other) }
}

#[cfg(not(test))]
impl Ord for char {
    #[inline]
    fn lt(&self, other: &char) -> bool { *self < *other }
}

#[cfg(not(test))]
impl Default for char {
    #[inline]
    fn default() -> char { '\x00' }
}

#[test]
fn test_is_lowercase() {
    assert!('a'.is_lowercase());
    assert!('ö'.is_lowercase());
    assert!('ß'.is_lowercase());
    assert!(!'Ü'.is_lowercase());
    assert!(!'P'.is_lowercase());
}

#[test]
fn test_is_uppercase() {
    assert!(!'h'.is_uppercase());
    assert!(!'ä'.is_uppercase());
    assert!(!'ß'.is_uppercase());
    assert!('Ö'.is_uppercase());
    assert!('T'.is_uppercase());
}

#[test]
fn test_is_whitespace() {
    assert!(' '.is_whitespace());
    assert!('\u2007'.is_whitespace());
    assert!('\t'.is_whitespace());
    assert!('\n'.is_whitespace());
    assert!(!'a'.is_whitespace());
    assert!(!'_'.is_whitespace());
    assert!(!'\u0000'.is_whitespace());
}

#[test]
fn test_to_digit() {
    assert_eq!('0'.to_digit(10u), Some(0u));
    assert_eq!('1'.to_digit(2u), Some(1u));
    assert_eq!('2'.to_digit(3u), Some(2u));
    assert_eq!('9'.to_digit(10u), Some(9u));
    assert_eq!('a'.to_digit(16u), Some(10u));
    assert_eq!('A'.to_digit(16u), Some(10u));
    assert_eq!('b'.to_digit(16u), Some(11u));
    assert_eq!('B'.to_digit(16u), Some(11u));
    assert_eq!('z'.to_digit(36u), Some(35u));
    assert_eq!('Z'.to_digit(36u), Some(35u));
    assert_eq!(' '.to_digit(10u), None);
    assert_eq!('$'.to_digit(36u), None);
}

#[test]
fn test_to_lowercase() {
    assert_eq!('A'.to_lowercase(), 'a');
    assert_eq!('Ö'.to_lowercase(), 'ö');
    assert_eq!('ß'.to_lowercase(), 'ß');
    assert_eq!('Ü'.to_lowercase(), 'ü');
    assert_eq!('💩'.to_lowercase(), '💩');
    assert_eq!('Σ'.to_lowercase(), 'σ');
    assert_eq!('Τ'.to_lowercase(), 'τ');
    assert_eq!('Ι'.to_lowercase(), 'ι');
    assert_eq!('Γ'.to_lowercase(), 'γ');
    assert_eq!('Μ'.to_lowercase(), 'μ');
    assert_eq!('Α'.to_lowercase(), 'α');
    assert_eq!('Σ'.to_lowercase(), 'σ');
}

#[test]
fn test_to_uppercase() {
    assert_eq!('a'.to_uppercase(), 'A');
    assert_eq!('ö'.to_uppercase(), 'Ö');
    assert_eq!('ß'.to_uppercase(), 'ß'); // not ẞ: Latin capital letter sharp s
    assert_eq!('ü'.to_uppercase(), 'Ü');
    assert_eq!('💩'.to_uppercase(), '💩');

    assert_eq!('σ'.to_uppercase(), 'Σ');
    assert_eq!('τ'.to_uppercase(), 'Τ');
    assert_eq!('ι'.to_uppercase(), 'Ι');
    assert_eq!('γ'.to_uppercase(), 'Γ');
    assert_eq!('μ'.to_uppercase(), 'Μ');
    assert_eq!('α'.to_uppercase(), 'Α');
    assert_eq!('ς'.to_uppercase(), 'Σ');
}

#[test]
fn test_is_control() {
    assert!('\u0000'.is_control());
    assert!('\u0003'.is_control());
    assert!('\u0006'.is_control());
    assert!('\u0009'.is_control());
    assert!('\u007f'.is_control());
    assert!('\u0092'.is_control());
    assert!(!'\u0020'.is_control());
    assert!(!'\u0055'.is_control());
    assert!(!'\u0068'.is_control());
}

#[test]
fn test_is_digit() {
   assert!('2'.is_digit());
   assert!('7'.is_digit());
   assert!(!'c'.is_digit());
   assert!(!'i'.is_digit());
   assert!(!'z'.is_digit());
   assert!(!'Q'.is_digit());
}

#[test]
fn test_escape_default() {
    fn string(c: char) -> ~str {
        let mut result = ~"";
        escape_default(c, |c| { result.push_char(c); });
        return result;
    }
    assert_eq!(string('\n'), ~"\\n");
    assert_eq!(string('\r'), ~"\\r");
    assert_eq!(string('\''), ~"\\'");
    assert_eq!(string('"'), ~"\\\"");
    assert_eq!(string(' '), ~" ");
    assert_eq!(string('a'), ~"a");
    assert_eq!(string('~'), ~"~");
    assert_eq!(string('\x00'), ~"\\x00");
    assert_eq!(string('\x1f'), ~"\\x1f");
    assert_eq!(string('\x7f'), ~"\\x7f");
    assert_eq!(string('\xff'), ~"\\xff");
    assert_eq!(string('\u011b'), ~"\\u011b");
    assert_eq!(string('\U0001d4b6'), ~"\\U0001d4b6");
}

#[test]
fn test_escape_unicode() {
    fn string(c: char) -> ~str {
        let mut result = ~"";
        escape_unicode(c, |c| { result.push_char(c); });
        return result;
    }
    assert_eq!(string('\x00'), ~"\\x00");
    assert_eq!(string('\n'), ~"\\x0a");
    assert_eq!(string(' '), ~"\\x20");
    assert_eq!(string('a'), ~"\\x61");
    assert_eq!(string('\u011b'), ~"\\u011b");
    assert_eq!(string('\U0001d4b6'), ~"\\U0001d4b6");
}

#[test]
fn test_to_str() {
    use to_str::ToStr;
    let s = 't'.to_str();
    assert_eq!(s, ~"t");
}
