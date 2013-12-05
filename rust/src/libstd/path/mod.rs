// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Cross-platform path support

This module implements support for two flavors of paths. `PosixPath` represents
a path on any unix-like system, whereas `WindowsPath` represents a path on
Windows. This module also exposes a typedef `Path` which is equal to the
appropriate platform-specific path variant.

Both `PosixPath` and `WindowsPath` implement a trait `GenericPath`, which
contains the set of methods that behave the same for both paths. They each also
implement some methods that could not be expressed in `GenericPath`, yet behave
identically for both path flavors, such as `.components()`.

The three main design goals of this module are 1) to avoid unnecessary
allocation, 2) to behave the same regardless of which flavor of path is being
used, and 3) to support paths that cannot be represented in UTF-8 (as Linux has
no restriction on paths beyond disallowing NUL).

## Usage

Usage of this module is fairly straightforward. Unless writing platform-specific
code, `Path` should be used to refer to the platform-native path.

Creation of a path is typically done with either `Path::new(some_str)` or
`Path::new(some_vec)`. This path can be modified with `.push()` and
`.pop()` (and other setters). The resulting Path can either be passed to another
API that expects a path, or can be turned into a &[u8] with `.as_vec()` or a
Option<&str> with `.as_str()`. Similarly, attributes of the path can be queried
with methods such as `.filename()`. There are also methods that return a new
path instead of modifying the receiver, such as `.join()` or `.dir_path()`.

Paths are always kept in normalized form. This means that creating the path
`Path::new("a/b/../c")` will return the path `a/c`. Similarly any attempt
to mutate the path will always leave it in normalized form.

When rendering a path to some form of output, there is a method `.display()`
which is compatible with the `format!()` parameter `{}`. This will render the
path as a string, replacing all non-utf8 sequences with the Replacement
Character (U+FFFD). As such it is not suitable for passing to any API that
actually operates on the path; it is only intended for display.

## Example

```rust
let mut path = Path::new("/tmp/path");
debug!("path: {}", path.display());
path.set_filename("foo");
path.push("bar");
debug!("new path: {}", path.display());
let b = std::os::path_exists(&path);
debug!("path exists: {}", b);
```

*/

use container::Container;
use c_str::CString;
use clone::Clone;
use fmt;
use iter::Iterator;
use option::{Option, None, Some};
use str;
use str::{OwnedStr, Str, StrSlice};
use to_str::ToStr;
use vec;
use vec::{CopyableVector, OwnedCopyableVector, OwnedVector, Vector};
use vec::{ImmutableEqVector, ImmutableVector};

/// Typedef for POSIX file paths.
/// See `posix::Path` for more info.
pub use PosixPath = self::posix::Path;

/// Typedef for Windows file paths.
/// See `windows::Path` for more info.
pub use WindowsPath = self::windows::Path;

/// Typedef for the platform-native path type
#[cfg(unix)]
pub use Path = self::posix::Path;
/// Typedef for the platform-native path type
#[cfg(windows)]
pub use Path = self::windows::Path;

/// Typedef for the platform-native component iterator
#[cfg(unix)]
pub use ComponentIter = self::posix::ComponentIter;
/// Typedef for the platform-native reverse component iterator
#[cfg(unix)]
pub use RevComponentIter = self::posix::RevComponentIter;
/// Typedef for the platform-native component iterator
#[cfg(windows)]
pub use ComponentIter = self::windows::ComponentIter;
/// Typedef for the platform-native reverse component iterator
#[cfg(windows)]
pub use RevComponentIter = self::windows::RevComponentIter;

/// Typedef for the platform-native str component iterator
#[cfg(unix)]
pub use StrComponentIter = self::posix::StrComponentIter;
/// Typedef for the platform-native reverse str component iterator
#[cfg(unix)]
pub use RevStrComponentIter = self::posix::RevStrComponentIter;
/// Typedef for the platform-native str component iterator
#[cfg(windows)]
pub use StrComponentIter = self::windows::StrComponentIter;
/// Typedef for the platform-native reverse str component iterator
#[cfg(windows)]
pub use RevStrComponentIter = self::windows::RevStrComponentIter;

/// Typedef for the platform-native separator char func
#[cfg(unix)]
pub use is_sep = self::posix::is_sep;
/// Typedef for the platform-native separator char func
#[cfg(windows)]
pub use is_sep = self::windows::is_sep;
/// Typedef for the platform-native separator byte func
#[cfg(unix)]
pub use is_sep_byte = self::posix::is_sep_byte;
/// Typedef for the platform-native separator byte func
#[cfg(windows)]
pub use is_sep_byte = self::windows::is_sep_byte;

pub mod posix;
pub mod windows;

// Condition that is raised when a NUL is found in a byte vector given to a Path function
condition! {
    // this should be a &[u8] but there's a lifetime issue
    null_byte: ~[u8] -> ~[u8];
}

/// A trait that represents the generic operations available on paths
pub trait GenericPath: Clone + GenericPathUnsafe {
    /// Creates a new Path from a byte vector or string.
    /// The resulting Path will always be normalized.
    ///
    /// # Failure
    ///
    /// Raises the `null_byte` condition if the path contains a NUL.
    ///
    /// See individual Path impls for additional restrictions.
    #[inline]
    fn new<T: BytesContainer>(path: T) -> Self {
        if contains_nul(path.container_as_bytes()) {
            let path = self::null_byte::cond.raise(path.container_into_owned_bytes());
            assert!(!contains_nul(path));
            unsafe { GenericPathUnsafe::new_unchecked(path) }
        } else {
            unsafe { GenericPathUnsafe::new_unchecked(path) }
        }
    }

    /// Creates a new Path from a byte vector or string, if possible.
    /// The resulting Path will always be normalized.
    #[inline]
    fn new_opt<T: BytesContainer>(path: T) -> Option<Self> {
        if contains_nul(path.container_as_bytes()) {
            None
        } else {
            Some(unsafe { GenericPathUnsafe::new_unchecked(path) })
        }
    }

    /// Returns the path as a string, if possible.
    /// If the path is not representable in utf-8, this returns None.
    #[inline]
    fn as_str<'a>(&'a self) -> Option<&'a str> {
        str::from_utf8_opt(self.as_vec())
    }

    /// Returns the path as a byte vector
    fn as_vec<'a>(&'a self) -> &'a [u8];

    /// Converts the Path into an owned byte vector
    fn into_vec(self) -> ~[u8];

    /// Returns an object that implements `fmt::Default` for printing paths
    ///
    /// This will print the equivalent of `to_display_str()` when used with a {} format parameter.
    fn display<'a>(&'a self) -> Display<'a, Self> {
        Display{ path: self, filename: false }
    }

    /// Returns an object that implements `fmt::Default` for printing filenames
    ///
    /// This will print the equivalent of `to_filename_display_str()` when used with a {}
    /// format parameter. If there is no filename, nothing will be printed.
    fn filename_display<'a>(&'a self) -> Display<'a, Self> {
        Display{ path: self, filename: true }
    }

    /// Returns the directory component of `self`, as a byte vector (with no trailing separator).
    /// If `self` has no directory component, returns ['.'].
    fn dirname<'a>(&'a self) -> &'a [u8];
    /// Returns the directory component of `self`, as a string, if possible.
    /// See `dirname` for details.
    #[inline]
    fn dirname_str<'a>(&'a self) -> Option<&'a str> {
        str::from_utf8_opt(self.dirname())
    }
    /// Returns the file component of `self`, as a byte vector.
    /// If `self` represents the root of the file hierarchy, returns None.
    /// If `self` is "." or "..", returns None.
    fn filename<'a>(&'a self) -> Option<&'a [u8]>;
    /// Returns the file component of `self`, as a string, if possible.
    /// See `filename` for details.
    #[inline]
    fn filename_str<'a>(&'a self) -> Option<&'a str> {
        self.filename().and_then(str::from_utf8_opt)
    }
    /// Returns the stem of the filename of `self`, as a byte vector.
    /// The stem is the portion of the filename just before the last '.'.
    /// If there is no '.', the entire filename is returned.
    fn filestem<'a>(&'a self) -> Option<&'a [u8]> {
        match self.filename() {
            None => None,
            Some(name) => Some({
                let dot = '.' as u8;
                match name.rposition_elem(&dot) {
                    None | Some(0) => name,
                    Some(1) if name == bytes!("..") => name,
                    Some(pos) => name.slice_to(pos)
                }
            })
        }
    }
    /// Returns the stem of the filename of `self`, as a string, if possible.
    /// See `filestem` for details.
    #[inline]
    fn filestem_str<'a>(&'a self) -> Option<&'a str> {
        self.filestem().and_then(str::from_utf8_opt)
    }
    /// Returns the extension of the filename of `self`, as an optional byte vector.
    /// The extension is the portion of the filename just after the last '.'.
    /// If there is no extension, None is returned.
    /// If the filename ends in '.', the empty vector is returned.
    fn extension<'a>(&'a self) -> Option<&'a [u8]> {
        match self.filename() {
            None => None,
            Some(name) => {
                let dot = '.' as u8;
                match name.rposition_elem(&dot) {
                    None | Some(0) => None,
                    Some(1) if name == bytes!("..") => None,
                    Some(pos) => Some(name.slice_from(pos+1))
                }
            }
        }
    }
    /// Returns the extension of the filename of `self`, as a string, if possible.
    /// See `extension` for details.
    #[inline]
    fn extension_str<'a>(&'a self) -> Option<&'a str> {
        self.extension().and_then(str::from_utf8_opt)
    }

    /// Replaces the filename portion of the path with the given byte vector or string.
    /// If the replacement name is [], this is equivalent to popping the path.
    ///
    /// # Failure
    ///
    /// Raises the `null_byte` condition if the filename contains a NUL.
    #[inline]
    fn set_filename<T: BytesContainer>(&mut self, filename: T) {
        if contains_nul(filename.container_as_bytes()) {
            let filename = self::null_byte::cond.raise(filename.container_into_owned_bytes());
            assert!(!contains_nul(filename));
            unsafe { self.set_filename_unchecked(filename) }
        } else {
            unsafe { self.set_filename_unchecked(filename) }
        }
    }
    /// Replaces the extension with the given byte vector or string.
    /// If there is no extension in `self`, this adds one.
    /// If the argument is [] or "", this removes the extension.
    /// If `self` has no filename, this is a no-op.
    ///
    /// # Failure
    ///
    /// Raises the `null_byte` condition if the extension contains a NUL.
    fn set_extension<T: BytesContainer>(&mut self, extension: T) {
        // borrowck causes problems here too
        let val = {
            match self.filename() {
                None => None,
                Some(name) => {
                    let dot = '.' as u8;
                    match name.rposition_elem(&dot) {
                        None | Some(0) => {
                            if extension.container_as_bytes().is_empty() {
                                None
                            } else {
                                let mut v;
                                if contains_nul(extension.container_as_bytes()) {
                                    let ext = extension.container_into_owned_bytes();
                                    let extension = self::null_byte::cond.raise(ext);
                                    assert!(!contains_nul(extension));
                                    v = vec::with_capacity(name.len() + extension.len() + 1);
                                    v.push_all(name);
                                    v.push(dot);
                                    v.push_all(extension);
                                } else {
                                    let extension = extension.container_as_bytes();
                                    v = vec::with_capacity(name.len() + extension.len() + 1);
                                    v.push_all(name);
                                    v.push(dot);
                                    v.push_all(extension);
                                }
                                Some(v)
                            }
                        }
                        Some(idx) => {
                            if extension.container_as_bytes().is_empty() {
                                Some(name.slice_to(idx).to_owned())
                            } else {
                                let mut v;
                                if contains_nul(extension.container_as_bytes()) {
                                    let ext = extension.container_into_owned_bytes();
                                    let extension = self::null_byte::cond.raise(ext);
                                    assert!(!contains_nul(extension));
                                    v = vec::with_capacity(idx + extension.len() + 1);
                                    v.push_all(name.slice_to(idx+1));
                                    v.push_all(extension);
                                } else {
                                    let extension = extension.container_as_bytes();
                                    v = vec::with_capacity(idx + extension.len() + 1);
                                    v.push_all(name.slice_to(idx+1));
                                    v.push_all(extension);
                                }
                                Some(v)
                            }
                        }
                    }
                }
            }
        };
        match val {
            None => (),
            Some(v) => unsafe { self.set_filename_unchecked(v) }
        }
    }

    /// Returns a new Path constructed by replacing the filename with the given
    /// byte vector or string.
    /// See `set_filename` for details.
    ///
    /// # Failure
    ///
    /// Raises the `null_byte` condition if the filename contains a NUL.
    #[inline]
    fn with_filename<T: BytesContainer>(&self, filename: T) -> Self {
        let mut p = self.clone();
        p.set_filename(filename);
        p
    }
    /// Returns a new Path constructed by setting the extension to the given
    /// byte vector or string.
    /// See `set_extension` for details.
    ///
    /// # Failure
    ///
    /// Raises the `null_byte` condition if the extension contains a NUL.
    #[inline]
    fn with_extension<T: BytesContainer>(&self, extension: T) -> Self {
        let mut p = self.clone();
        p.set_extension(extension);
        p
    }

    /// Returns the directory component of `self`, as a Path.
    /// If `self` represents the root of the filesystem hierarchy, returns `self`.
    fn dir_path(&self) -> Self {
        // self.dirname() returns a NUL-free vector
        unsafe { GenericPathUnsafe::new_unchecked(self.dirname()) }
    }

    /// Returns a Path that represents the filesystem root that `self` is rooted in.
    ///
    /// If `self` is not absolute, or vol-relative in the case of Windows, this returns None.
    fn root_path(&self) -> Option<Self>;

    /// Pushes a path (as a byte vector or string) onto `self`.
    /// If the argument represents an absolute path, it replaces `self`.
    ///
    /// # Failure
    ///
    /// Raises the `null_byte` condition if the path contains a NUL.
    #[inline]
    fn push<T: BytesContainer>(&mut self, path: T) {
        if contains_nul(path.container_as_bytes()) {
            let path = self::null_byte::cond.raise(path.container_into_owned_bytes());
            assert!(!contains_nul(path));
            unsafe { self.push_unchecked(path) }
        } else {
            unsafe { self.push_unchecked(path) }
        }
    }
    /// Pushes multiple paths (as byte vectors or strings) onto `self`.
    /// See `push` for details.
    #[inline]
    fn push_many<T: BytesContainer>(&mut self, paths: &[T]) {
        let t: Option<T> = None;
        if BytesContainer::is_str(t) {
            for p in paths.iter() {
                self.push(p.container_as_str())
            }
        } else {
            for p in paths.iter() {
                self.push(p.container_as_bytes())
            }
        }
    }
    /// Removes the last path component from the receiver.
    /// Returns `true` if the receiver was modified, or `false` if it already
    /// represented the root of the file hierarchy.
    fn pop(&mut self) -> bool;

    /// Returns a new Path constructed by joining `self` with the given path
    /// (as a byte vector or string).
    /// If the given path is absolute, the new Path will represent just that.
    ///
    /// # Failure
    ///
    /// Raises the `null_byte` condition if the path contains a NUL.
    #[inline]
    fn join<T: BytesContainer>(&self, path: T) -> Self {
        let mut p = self.clone();
        p.push(path);
        p
    }
    /// Returns a new Path constructed by joining `self` with the given paths
    /// (as byte vectors or strings).
    /// See `join` for details.
    #[inline]
    fn join_many<T: BytesContainer>(&self, paths: &[T]) -> Self {
        let mut p = self.clone();
        p.push_many(paths);
        p
    }

    /// Returns whether `self` represents an absolute path.
    /// An absolute path is defined as one that, when joined to another path, will
    /// yield back the same absolute path.
    fn is_absolute(&self) -> bool;

    /// Returns whether `self` represents a relative path.
    /// Typically this is the inverse of `is_absolute`.
    /// But for Windows paths, it also means the path is not volume-relative or
    /// relative to the current working directory.
    fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    /// Returns whether `self` is equal to, or is an ancestor of, the given path.
    /// If both paths are relative, they are compared as though they are relative
    /// to the same parent path.
    fn is_ancestor_of(&self, other: &Self) -> bool;

    /// Returns the Path that, were it joined to `base`, would yield `self`.
    /// If no such path exists, None is returned.
    /// If `self` is absolute and `base` is relative, or on Windows if both
    /// paths refer to separate drives, an absolute path is returned.
    fn path_relative_from(&self, base: &Self) -> Option<Self>;

    /// Returns whether the relative path `child` is a suffix of `self`.
    fn ends_with_path(&self, child: &Self) -> bool;
}

/// A trait that represents something bytes-like (e.g. a &[u8] or a &str)
pub trait BytesContainer {
    /// Returns a &[u8] representing the receiver
    fn container_as_bytes<'a>(&'a self) -> &'a [u8];
    /// Consumes the receiver and converts it into ~[u8]
    #[inline]
    fn container_into_owned_bytes(self) -> ~[u8] {
        self.container_as_bytes().to_owned()
    }
    /// Returns the receiver interpreted as a utf-8 string
    ///
    /// # Failure
    ///
    /// Raises `str::null_byte` if not utf-8
    #[inline]
    fn container_as_str<'a>(&'a self) -> &'a str {
        str::from_utf8(self.container_as_bytes())
    }
    /// Returns the receiver interpreted as a utf-8 string, if possible
    #[inline]
    fn container_as_str_opt<'a>(&'a self) -> Option<&'a str> {
        str::from_utf8_opt(self.container_as_bytes())
    }
    /// Returns whether .container_as_str() is guaranteed to not fail
    // FIXME (#8888): Remove unused arg once ::<for T> works
    #[inline]
    fn is_str(_: Option<Self>) -> bool { false }
}

/// A trait that represents the unsafe operations on GenericPaths
pub trait GenericPathUnsafe {
    /// Creates a new Path without checking for null bytes.
    /// The resulting Path will always be normalized.
    unsafe fn new_unchecked<T: BytesContainer>(path: T) -> Self;

    /// Replaces the filename portion of the path without checking for null
    /// bytes.
    /// See `set_filename` for details.
    unsafe fn set_filename_unchecked<T: BytesContainer>(&mut self, filename: T);

    /// Pushes a path onto `self` without checking for null bytes.
    /// See `push` for details.
    unsafe fn push_unchecked<T: BytesContainer>(&mut self, path: T);
}

/// Helper struct for printing paths with format!()
pub struct Display<'self, P> {
    priv path: &'self P,
    priv filename: bool
}

impl<'self, P: GenericPath> fmt::Default for Display<'self, P> {
    fn fmt(d: &Display<P>, f: &mut fmt::Formatter) {
        d.with_str(|s| f.pad(s))
    }
}

impl<'self, P: GenericPath> ToStr for Display<'self, P> {
    /// Returns the path as a string
    ///
    /// If the path is not UTF-8, invalid sequences with be replaced with the
    /// unicode replacement char. This involves allocation.
    fn to_str(&self) -> ~str {
        if self.filename {
            match self.path.filename() {
                None => ~"",
                Some(v) => from_utf8_with_replacement(v)
            }
        } else {
            from_utf8_with_replacement(self.path.as_vec())
        }
    }
}

impl<'self, P: GenericPath> Display<'self, P> {
    /// Provides the path as a string to a closure
    ///
    /// If the path is not UTF-8, invalid sequences will be replaced with the
    /// unicode replacement char. This involves allocation.
    #[inline]
    pub fn with_str<T>(&self, f: |&str| -> T) -> T {
        let opt = if self.filename { self.path.filename_str() }
                  else { self.path.as_str() };
        match opt {
            Some(s) => f(s),
            None => {
                let s = self.to_str();
                f(s.as_slice())
            }
        }
    }
}

impl<'self> BytesContainer for &'self str {
    #[inline]
    fn container_as_bytes<'a>(&'a self) -> &'a [u8] {
        self.as_bytes()
    }
    #[inline]
    fn container_as_str<'a>(&'a self) -> &'a str {
        *self
    }
    #[inline]
    fn container_as_str_opt<'a>(&'a self) -> Option<&'a str> {
        Some(*self)
    }
    #[inline]
    fn is_str(_: Option<&'self str>) -> bool { true }
}

impl BytesContainer for ~str {
    #[inline]
    fn container_as_bytes<'a>(&'a self) -> &'a [u8] {
        self.as_bytes()
    }
    #[inline]
    fn container_into_owned_bytes(self) -> ~[u8] {
        self.into_bytes()
    }
    #[inline]
    fn container_as_str<'a>(&'a self) -> &'a str {
        self.as_slice()
    }
    #[inline]
    fn container_as_str_opt<'a>(&'a self) -> Option<&'a str> {
        Some(self.as_slice())
    }
    #[inline]
    fn is_str(_: Option<~str>) -> bool { true }
}

impl BytesContainer for @str {
    #[inline]
    fn container_as_bytes<'a>(&'a self) -> &'a [u8] {
        self.as_bytes()
    }
    #[inline]
    fn container_as_str<'a>(&'a self) -> &'a str {
        self.as_slice()
    }
    #[inline]
    fn container_as_str_opt<'a>(&'a self) -> Option<&'a str> {
        Some(self.as_slice())
    }
    #[inline]
    fn is_str(_: Option<@str>) -> bool { true }
}

impl<'self> BytesContainer for &'self [u8] {
    #[inline]
    fn container_as_bytes<'a>(&'a self) -> &'a [u8] {
        *self
    }
}

impl BytesContainer for ~[u8] {
    #[inline]
    fn container_as_bytes<'a>(&'a self) -> &'a [u8] {
        self.as_slice()
    }
    #[inline]
    fn container_into_owned_bytes(self) -> ~[u8] {
        self
    }
}

impl BytesContainer for @[u8] {
    #[inline]
    fn container_as_bytes<'a>(&'a self) -> &'a [u8] {
        self.as_slice()
    }
}

impl BytesContainer for CString {
    #[inline]
    fn container_as_bytes<'a>(&'a self) -> &'a [u8] {
        let s = self.as_bytes();
        s.slice_to(s.len()-1)
    }
}

#[inline(always)]
fn contains_nul(v: &[u8]) -> bool {
    v.iter().any(|&x| x == 0)
}

#[inline(always)]
fn from_utf8_with_replacement(mut v: &[u8]) -> ~str {
    // FIXME (#9516): Don't decode utf-8 manually here once we have a good way to do it in str
    // This is a truly horrifically bad implementation, done as a functionality stopgap until
    // we have a proper utf-8 decoder. I don't really want to write one here.
    static REPLACEMENT_CHAR: char = '\uFFFD';

    let mut s = str::with_capacity(v.len());
    while !v.is_empty() {
        let w = str::utf8_char_width(v[0]);
        if w == 0u {
            s.push_char(REPLACEMENT_CHAR);
            v = v.slice_from(1);
        } else if v.len() < w || !str::is_utf8(v.slice_to(w)) {
            s.push_char(REPLACEMENT_CHAR);
            v = v.slice_from(1);
        } else {
            s.push_str(unsafe { ::cast::transmute(v.slice_to(w)) });
            v = v.slice_from(w);
        }
    }
    s
}
#[cfg(test)]
mod tests {
    use super::{GenericPath, PosixPath, WindowsPath};
    use c_str::ToCStr;

    #[test]
    fn test_cstring() {
        let input = "/foo/bar/baz";
        let path: PosixPath = PosixPath::new(input.to_c_str());
        assert_eq!(path.as_vec(), input.as_bytes());

        let input = r"\foo\bar\baz";
        let path: WindowsPath = WindowsPath::new(input.to_c_str());
        assert_eq!(path.as_str().unwrap(), input.as_slice());
    }
}
