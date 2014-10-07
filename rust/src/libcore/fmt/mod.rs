// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Utilities for formatting and printing strings

#![allow(unused_variable)]

use any;
use cell::{Cell, Ref, RefMut};
use collections::Collection;
use iter::{Iterator, range};
use kinds::Copy;
use mem;
use option::{Option, Some, None};
use ops::Deref;
use result::{Ok, Err};
use result;
use slice::{AsSlice, ImmutableSlice};
use slice;
use str::StrSlice;
use str;

pub use self::num::radix;
pub use self::num::Radix;
pub use self::num::RadixFmt;

mod num;
mod float;
pub mod rt;

pub type Result = result::Result<(), FormatError>;

/// The error type which is returned from formatting a message into a stream.
///
/// This type does not support transmission of an error other than that an error
/// occurred. Any extra information must be arranged to be transmitted through
/// some other means.
pub enum FormatError {
    /// A generic write error occurred during formatting, no other information
    /// is transmitted via this variant.
    WriteError,
}

/// A collection of methods that are required to format a message into a stream.
///
/// This trait is the type which this modules requires when formatting
/// information. This is similar to the standard library's `io::Writer` trait,
/// but it is only intended for use in libcore.
///
/// This trait should generally not be implemented by consumers of the standard
/// library. The `write!` macro accepts an instance of `io::Writer`, and the
/// `io::Writer` trait is favored over implementing this trait.
pub trait FormatWriter {
    /// Writes a slice of bytes into this writer, returning whether the write
    /// succeeded.
    ///
    /// This method can only succeed if the entire byte slice was successfully
    /// written, and this method will not return until all data has been
    /// written or an error occurs.
    ///
    /// # Errors
    ///
    /// This function will return an instance of `FormatError` on error.
    fn write(&mut self, bytes: &[u8]) -> Result;

    /// Glue for usage of the `write!` macro with implementers of this trait.
    ///
    /// This method should generally not be invoked manually, but rather through
    /// the `write!` macro itself.
    fn write_fmt(&mut self, args: &Arguments) -> Result { write(self, args) }
}

/// A struct to represent both where to emit formatting strings to and how they
/// should be formatted. A mutable version of this is passed to all formatting
/// traits.
pub struct Formatter<'a> {
    /// Flags for formatting (packed version of rt::Flag)
    pub flags: uint,
    /// Character used as 'fill' whenever there is alignment
    pub fill: char,
    /// Boolean indication of whether the output should be left-aligned
    pub align: rt::Alignment,
    /// Optionally specified integer width that the output should be
    pub width: Option<uint>,
    /// Optionally specified precision for numeric types
    pub precision: Option<uint>,

    buf: &'a mut FormatWriter+'a,
    curarg: slice::Items<'a, Argument<'a>>,
    args: &'a [Argument<'a>],
}

enum Void {}

/// This struct represents the generic "argument" which is taken by the Xprintf
/// family of functions. It contains a function to format the given value. At
/// compile time it is ensured that the function and the value have the correct
/// types, and then this struct is used to canonicalize arguments to one type.
pub struct Argument<'a> {
    formatter: extern "Rust" fn(&Void, &mut Formatter) -> Result,
    value: &'a Void,
}

impl<'a> Arguments<'a> {
    /// When using the format_args!() macro, this function is used to generate the
    /// Arguments structure. The compiler inserts an `unsafe` block to call this,
    /// which is valid because the compiler performs all necessary validation to
    /// ensure that the resulting call to format/write would be safe.
    #[doc(hidden)] #[inline]
    pub unsafe fn new<'a>(pieces: &'static [&'static str],
                          args: &'a [Argument<'a>]) -> Arguments<'a> {
        Arguments {
            pieces: mem::transmute(pieces),
            fmt: None,
            args: args
        }
    }

    /// This function is used to specify nonstandard formatting parameters.
    /// The `pieces` array must be at least as long as `fmt` to construct
    /// a valid Arguments structure.
    #[doc(hidden)] #[inline]
    pub unsafe fn with_placeholders<'a>(pieces: &'static [&'static str],
                                        fmt: &'static [rt::Argument<'static>],
                                        args: &'a [Argument<'a>]) -> Arguments<'a> {
        Arguments {
            pieces: mem::transmute(pieces),
            fmt: Some(mem::transmute(fmt)),
            args: args
        }
    }
}

/// This structure represents a safely precompiled version of a format string
/// and its arguments. This cannot be generated at runtime because it cannot
/// safely be done so, so no constructors are given and the fields are private
/// to prevent modification.
///
/// The `format_args!` macro will safely create an instance of this structure
/// and pass it to a function or closure, passed as the first argument. The
/// macro validates the format string at compile-time so usage of the `write`
/// and `format` functions can be safely performed.
pub struct Arguments<'a> {
    // Format string pieces to print.
    pieces: &'a [&'a str],

    // Placeholder specs, or `None` if all specs are default (as in "{}{}").
    fmt: Option<&'a [rt::Argument<'a>]>,

    // Dynamic arguments for interpolation, to be interleaved with string
    // pieces. (Every argument is preceded by a string piece.)
    args: &'a [Argument<'a>],
}

impl<'a> Show for Arguments<'a> {
    fn fmt(&self, fmt: &mut Formatter) -> Result {
        write(fmt.buf, self)
    }
}

/// When a format is not otherwise specified, types are formatted by ascribing
/// to this trait. There is not an explicit way of selecting this trait to be
/// used for formatting, it is only if no other format is specified.
pub trait Show {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `b` character
pub trait Bool {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `c` character
pub trait Char {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `i` and `d` characters
pub trait Signed {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `u` character
pub trait Unsigned {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `o` character
pub trait Octal {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `t` character
pub trait Binary {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `x` character
pub trait LowerHex {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `X` character
pub trait UpperHex {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `s` character
pub trait String {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `p` character
pub trait Pointer {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `f` character
pub trait Float {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `e` character
pub trait LowerExp {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

/// Format trait for the `E` character
pub trait UpperExp {
    /// Formats the value using the given formatter.
    fn fmt(&self, &mut Formatter) -> Result;
}

// FIXME #11938 - UFCS would make us able call the above methods
// directly Show::show(x, fmt).
macro_rules! uniform_fn_call_workaround {
    ($( $name: ident, $trait_: ident; )*) => {
        $(
            #[doc(hidden)]
            pub fn $name<T: $trait_>(x: &T, fmt: &mut Formatter) -> Result {
                x.fmt(fmt)
            }
            )*
    }
}
uniform_fn_call_workaround! {
    secret_show, Show;
    secret_bool, Bool;
    secret_char, Char;
    secret_signed, Signed;
    secret_unsigned, Unsigned;
    secret_octal, Octal;
    secret_binary, Binary;
    secret_lower_hex, LowerHex;
    secret_upper_hex, UpperHex;
    secret_string, String;
    secret_pointer, Pointer;
    secret_float, Float;
    secret_lower_exp, LowerExp;
    secret_upper_exp, UpperExp;
}

static DEFAULT_ARGUMENT: rt::Argument<'static> = rt::Argument {
    position: rt::ArgumentNext,
    format: rt::FormatSpec {
        fill: ' ',
        align: rt::AlignUnknown,
        flags: 0,
        precision: rt::CountImplied,
        width: rt::CountImplied,
    }
};

/// The `write` function takes an output stream, a precompiled format string,
/// and a list of arguments. The arguments will be formatted according to the
/// specified format string into the output stream provided.
///
/// # Arguments
///
///   * output - the buffer to write output to
///   * args - the precompiled arguments generated by `format_args!`
pub fn write(output: &mut FormatWriter, args: &Arguments) -> Result {
    let mut formatter = Formatter {
        flags: 0,
        width: None,
        precision: None,
        buf: output,
        align: rt::AlignUnknown,
        fill: ' ',
        args: args.args,
        curarg: args.args.iter(),
    };

    let mut pieces = args.pieces.iter();

    match args.fmt {
        None => {
            // We can use default formatting parameters for all arguments.
            for _ in range(0, args.args.len()) {
                try!(formatter.buf.write(pieces.next().unwrap().as_bytes()));
                try!(formatter.run(&DEFAULT_ARGUMENT));
            }
        }
        Some(fmt) => {
            // Every spec has a corresponding argument that is preceded by
            // a string piece.
            for (arg, piece) in fmt.iter().zip(pieces.by_ref()) {
                try!(formatter.buf.write(piece.as_bytes()));
                try!(formatter.run(arg));
            }
        }
    }

    // There can be only one trailing string piece left.
    match pieces.next() {
        Some(piece) => {
            try!(formatter.buf.write(piece.as_bytes()));
        }
        None => {}
    }

    Ok(())
}

impl<'a> Formatter<'a> {

    // First up is the collection of functions used to execute a format string
    // at runtime. This consumes all of the compile-time statics generated by
    // the format! syntax extension.
    fn run(&mut self, arg: &rt::Argument) -> Result {
        // Fill in the format parameters into the formatter
        self.fill = arg.format.fill;
        self.align = arg.format.align;
        self.flags = arg.format.flags;
        self.width = self.getcount(&arg.format.width);
        self.precision = self.getcount(&arg.format.precision);

        // Extract the correct argument
        let value = match arg.position {
            rt::ArgumentNext => { *self.curarg.next().unwrap() }
            rt::ArgumentIs(i) => self.args[i],
        };

        // Then actually do some printing
        (value.formatter)(value.value, self)
    }

    fn getcount(&mut self, cnt: &rt::Count) -> Option<uint> {
        match *cnt {
            rt::CountIs(n) => { Some(n) }
            rt::CountImplied => { None }
            rt::CountIsParam(i) => {
                let v = self.args[i].value;
                unsafe { Some(*(v as *const _ as *const uint)) }
            }
            rt::CountIsNextParam => {
                let v = self.curarg.next().unwrap().value;
                unsafe { Some(*(v as *const _ as *const uint)) }
            }
        }
    }

    // Helper methods used for padding and processing formatting arguments that
    // all formatting traits can use.

    /// Performs the correct padding for an integer which has already been
    /// emitted into a byte-array. The byte-array should *not* contain the sign
    /// for the integer, that will be added by this method.
    ///
    /// # Arguments
    ///
    /// * is_positive - whether the original integer was positive or not.
    /// * prefix - if the '#' character (FlagAlternate) is provided, this
    ///   is the prefix to put in front of the number.
    /// * buf - the byte array that the number has been formatted into
    ///
    /// This function will correctly account for the flags provided as well as
    /// the minimum width. It will not take precision into account.
    pub fn pad_integral(&mut self,
                        is_positive: bool,
                        prefix: &str,
                        buf: &[u8])
                        -> Result {
        use char::Char;
        use fmt::rt::{FlagAlternate, FlagSignPlus, FlagSignAwareZeroPad};

        let mut width = buf.len();

        let mut sign = None;
        if !is_positive {
            sign = Some('-'); width += 1;
        } else if self.flags & (1 << (FlagSignPlus as uint)) != 0 {
            sign = Some('+'); width += 1;
        }

        let mut prefixed = false;
        if self.flags & (1 << (FlagAlternate as uint)) != 0 {
            prefixed = true; width += prefix.char_len();
        }

        // Writes the sign if it exists, and then the prefix if it was requested
        let write_prefix = |f: &mut Formatter| {
            for c in sign.into_iter() {
                let mut b = [0, ..4];
                let n = c.encode_utf8(b).unwrap_or(0);
                try!(f.buf.write(b[..n]));
            }
            if prefixed { f.buf.write(prefix.as_bytes()) }
            else { Ok(()) }
        };

        // The `width` field is more of a `min-width` parameter at this point.
        match self.width {
            // If there's no minimum length requirements then we can just
            // write the bytes.
            None => {
                try!(write_prefix(self)); self.buf.write(buf)
            }
            // Check if we're over the minimum width, if so then we can also
            // just write the bytes.
            Some(min) if width >= min => {
                try!(write_prefix(self)); self.buf.write(buf)
            }
            // The sign and prefix goes before the padding if the fill character
            // is zero
            Some(min) if self.flags & (1 << (FlagSignAwareZeroPad as uint)) != 0 => {
                self.fill = '0';
                try!(write_prefix(self));
                self.with_padding(min - width, rt::AlignRight, |f| f.buf.write(buf))
            }
            // Otherwise, the sign and prefix goes after the padding
            Some(min) => {
                self.with_padding(min - width, rt::AlignRight, |f| {
                    try!(write_prefix(f)); f.buf.write(buf)
                })
            }
        }
    }

    /// This function takes a string slice and emits it to the internal buffer
    /// after applying the relevant formatting flags specified. The flags
    /// recognized for generic strings are:
    ///
    /// * width - the minimum width of what to emit
    /// * fill/align - what to emit and where to emit it if the string
    ///                provided needs to be padded
    /// * precision - the maximum length to emit, the string is truncated if it
    ///               is longer than this length
    ///
    /// Notably this function ignored the `flag` parameters
    pub fn pad(&mut self, s: &str) -> Result {
        // Make sure there's a fast path up front
        if self.width.is_none() && self.precision.is_none() {
            return self.buf.write(s.as_bytes());
        }
        // The `precision` field can be interpreted as a `max-width` for the
        // string being formatted
        match self.precision {
            Some(max) => {
                // If there's a maximum width and our string is longer than
                // that, then we must always have truncation. This is the only
                // case where the maximum length will matter.
                let char_len = s.char_len();
                if char_len >= max {
                    let nchars = ::cmp::min(max, char_len);
                    return self.buf.write(s.slice_chars(0, nchars).as_bytes());
                }
            }
            None => {}
        }
        // The `width` field is more of a `min-width` parameter at this point.
        match self.width {
            // If we're under the maximum length, and there's no minimum length
            // requirements, then we can just emit the string
            None => self.buf.write(s.as_bytes()),
            // If we're under the maximum width, check if we're over the minimum
            // width, if so it's as easy as just emitting the string.
            Some(width) if s.char_len() >= width => {
                self.buf.write(s.as_bytes())
            }
            // If we're under both the maximum and the minimum width, then fill
            // up the minimum width with the specified string + some alignment.
            Some(width) => {
                self.with_padding(width - s.char_len(), rt::AlignLeft, |me| {
                    me.buf.write(s.as_bytes())
                })
            }
        }
    }

    /// Runs a callback, emitting the correct padding either before or
    /// afterwards depending on whether right or left alignment is requested.
    fn with_padding(&mut self,
                    padding: uint,
                    default: rt::Alignment,
                    f: |&mut Formatter| -> Result) -> Result {
        use char::Char;
        let align = match self.align {
            rt::AlignUnknown => default,
            _ => self.align
        };

        let (pre_pad, post_pad) = match align {
            rt::AlignLeft => (0u, padding),
            rt::AlignRight | rt::AlignUnknown => (padding, 0u),
            rt::AlignCenter => (padding / 2, (padding + 1) / 2),
        };

        let mut fill = [0u8, ..4];
        let len = self.fill.encode_utf8(fill).unwrap_or(0);

        for _ in range(0, pre_pad) {
            try!(self.buf.write(fill[..len]));
        }

        try!(f(self));

        for _ in range(0, post_pad) {
            try!(self.buf.write(fill[..len]));
        }

        Ok(())
    }

    /// Writes some data to the underlying buffer contained within this
    /// formatter.
    pub fn write(&mut self, data: &[u8]) -> Result {
        self.buf.write(data)
    }

    /// Writes some formatted information into this instance
    pub fn write_fmt(&mut self, fmt: &Arguments) -> Result {
        write(self.buf, fmt)
    }
}

/// This is a function which calls are emitted to by the compiler itself to
/// create the Argument structures that are passed into the `format` function.
#[doc(hidden)] #[inline]
pub fn argument<'a, T>(f: extern "Rust" fn(&T, &mut Formatter) -> Result,
                       t: &'a T) -> Argument<'a> {
    unsafe {
        Argument {
            formatter: mem::transmute(f),
            value: mem::transmute(t)
        }
    }
}

/// When the compiler determines that the type of an argument *must* be a string
/// (such as for select), then it invokes this method.
#[doc(hidden)] #[inline]
pub fn argumentstr<'a>(s: &'a &str) -> Argument<'a> {
    argument(secret_string, s)
}

/// When the compiler determines that the type of an argument *must* be a uint
/// (such as for plural), then it invokes this method.
#[doc(hidden)] #[inline]
pub fn argumentuint<'a>(s: &'a uint) -> Argument<'a> {
    argument(secret_unsigned, s)
}

// Implementations of the core formatting traits

impl<'a, T: Show> Show for &'a T {
    fn fmt(&self, f: &mut Formatter) -> Result { secret_show(*self, f) }
}
impl<'a, T: Show> Show for &'a mut T {
    fn fmt(&self, f: &mut Formatter) -> Result { secret_show(&**self, f) }
}
impl<'a> Show for &'a Show+'a {
    fn fmt(&self, f: &mut Formatter) -> Result { (*self).fmt(f) }
}

impl Bool for bool {
    fn fmt(&self, f: &mut Formatter) -> Result {
        secret_string(&(if *self {"true"} else {"false"}), f)
    }
}

impl<'a, T: str::Str> String for T {
    fn fmt(&self, f: &mut Formatter) -> Result {
        f.pad(self.as_slice())
    }
}

impl Char for char {
    fn fmt(&self, f: &mut Formatter) -> Result {
        use char::Char;

        let mut utf8 = [0u8, ..4];
        let amt = self.encode_utf8(utf8).unwrap_or(0);
        let s: &str = unsafe { mem::transmute(utf8[..amt]) };
        secret_string(&s, f)
    }
}

impl<T> Pointer for *const T {
    fn fmt(&self, f: &mut Formatter) -> Result {
        f.flags |= 1 << (rt::FlagAlternate as uint);
        secret_lower_hex::<uint>(&(*self as uint), f)
    }
}
impl<T> Pointer for *mut T {
    fn fmt(&self, f: &mut Formatter) -> Result {
        secret_pointer::<*const T>(&(*self as *const T), f)
    }
}
impl<'a, T> Pointer for &'a T {
    fn fmt(&self, f: &mut Formatter) -> Result {
        secret_pointer::<*const T>(&(&**self as *const T), f)
    }
}
impl<'a, T> Pointer for &'a mut T {
    fn fmt(&self, f: &mut Formatter) -> Result {
        secret_pointer::<*const T>(&(&**self as *const T), f)
    }
}

macro_rules! floating(($ty:ident) => {
    impl Float for $ty {
        fn fmt(&self, fmt: &mut Formatter) -> Result {
            use num::{Float, Signed};

            let digits = match fmt.precision {
                Some(i) => float::DigExact(i),
                None => float::DigMax(6),
            };
            float::float_to_str_bytes_common(self.abs(),
                                             10,
                                             true,
                                             float::SignNeg,
                                             digits,
                                             float::ExpNone,
                                             false,
                                             |bytes| {
                fmt.pad_integral(self.is_nan() || *self >= 0.0, "", bytes)
            })
        }
    }

    impl LowerExp for $ty {
        fn fmt(&self, fmt: &mut Formatter) -> Result {
            use num::{Float, Signed};

            let digits = match fmt.precision {
                Some(i) => float::DigExact(i),
                None => float::DigMax(6),
            };
            float::float_to_str_bytes_common(self.abs(),
                                             10,
                                             true,
                                             float::SignNeg,
                                             digits,
                                             float::ExpDec,
                                             false,
                                             |bytes| {
                fmt.pad_integral(self.is_nan() || *self >= 0.0, "", bytes)
            })
        }
    }

    impl UpperExp for $ty {
        fn fmt(&self, fmt: &mut Formatter) -> Result {
            use num::{Float, Signed};

            let digits = match fmt.precision {
                Some(i) => float::DigExact(i),
                None => float::DigMax(6),
            };
            float::float_to_str_bytes_common(self.abs(),
                                             10,
                                             true,
                                             float::SignNeg,
                                             digits,
                                             float::ExpDec,
                                             true,
                                             |bytes| {
                fmt.pad_integral(self.is_nan() || *self >= 0.0, "", bytes)
            })
        }
    }
})
floating!(f32)
floating!(f64)

// Implementation of Show for various core types

macro_rules! delegate(($ty:ty to $other:ident) => {
    impl<'a> Show for $ty {
        fn fmt(&self, f: &mut Formatter) -> Result {
            (concat_idents!(secret_, $other)(self, f))
        }
    }
})
delegate!(&'a str to string)
delegate!(bool to bool)
delegate!(char to char)
delegate!(f32 to float)
delegate!(f64 to float)

impl<T> Show for *const T {
    fn fmt(&self, f: &mut Formatter) -> Result { secret_pointer(self, f) }
}
impl<T> Show for *mut T {
    fn fmt(&self, f: &mut Formatter) -> Result { secret_pointer(self, f) }
}

macro_rules! peel(($name:ident, $($other:ident,)*) => (tuple!($($other,)*)))

macro_rules! tuple (
    () => ();
    ( $($name:ident,)+ ) => (
        impl<$($name:Show),*> Show for ($($name,)*) {
            #[allow(non_snake_case, dead_assignment)]
            fn fmt(&self, f: &mut Formatter) -> Result {
                try!(write!(f, "("));
                let ($(ref $name,)*) = *self;
                let mut n = 0i;
                $(
                    if n > 0 {
                        try!(write!(f, ", "));
                    }
                    try!(write!(f, "{}", *$name));
                    n += 1;
                )*
                if n == 1 {
                    try!(write!(f, ","));
                }
                write!(f, ")")
            }
        }
        peel!($($name,)*)
    )
)

tuple! { T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, }

impl<'a> Show for &'a any::Any+'a {
    fn fmt(&self, f: &mut Formatter) -> Result { f.pad("&Any") }
}

impl<'a, T: Show> Show for &'a [T] {
    fn fmt(&self, f: &mut Formatter) -> Result {
        if f.flags & (1 << (rt::FlagAlternate as uint)) == 0 {
            try!(write!(f, "["));
        }
        let mut is_first = true;
        for x in self.iter() {
            if is_first {
                is_first = false;
            } else {
                try!(write!(f, ", "));
            }
            try!(write!(f, "{}", *x))
        }
        if f.flags & (1 << (rt::FlagAlternate as uint)) == 0 {
            try!(write!(f, "]"));
        }
        Ok(())
    }
}

impl<'a, T: Show> Show for &'a mut [T] {
    fn fmt(&self, f: &mut Formatter) -> Result {
        secret_show(&self.as_slice(), f)
    }
}

impl Show for () {
    fn fmt(&self, f: &mut Formatter) -> Result {
        f.pad("()")
    }
}

impl<T: Copy + Show> Show for Cell<T> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "Cell {{ value: {} }}", self.get())
    }
}

impl<'b, T: Show> Show for Ref<'b, T> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        (**self).fmt(f)
    }
}

impl<'b, T: Show> Show for RefMut<'b, T> {
    fn fmt(&self, f: &mut Formatter) -> Result {
        (*(self.deref())).fmt(f)
    }
}

// If you expected tests to be here, look instead at the run-pass/ifmt.rs test,
// it's a lot easier than creating all of the rt::Piece structures here.
