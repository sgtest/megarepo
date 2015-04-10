// Copyright 2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Cross-platform path support
//!
//! This module implements support for two flavors of paths. `PosixPath` represents a path on any
//! unix-like system, whereas `WindowsPath` represents a path on Windows. This module also exposes
//! a typedef `Path` which is equal to the appropriate platform-specific path variant.
//!
//! Both `PosixPath` and `WindowsPath` implement a trait `GenericPath`, which contains the set of
//! methods that behave the same for both paths. They each also implement some methods that could
//! not be expressed in `GenericPath`, yet behave identically for both path flavors, such as
//! `.components()`.
//!
//! The three main design goals of this module are 1) to avoid unnecessary allocation, 2) to behave
//! the same regardless of which flavor of path is being used, and 3) to support paths that cannot
//! be represented in UTF-8 (as Linux has no restriction on paths beyond disallowing NUL).
//!
//! ## Usage
//!
//! Usage of this module is fairly straightforward. Unless writing platform-specific code, `Path`
//! should be used to refer to the platform-native path.
//!
//! Creation of a path is typically done with either `Path::new(some_str)` or
//! `Path::new(some_vec)`. This path can be modified with `.push()` and `.pop()` (and other
//! setters). The resulting Path can either be passed to another API that expects a path, or can be
//! turned into a `&[u8]` with `.as_vec()` or a `Option<&str>` with `.as_str()`. Similarly,
//! attributes of the path can be queried with methods such as `.filename()`. There are also
//! methods that return a new path instead of modifying the receiver, such as `.join()` or
//! `.dir_path()`.
//!
//! Paths are always kept in normalized form. This means that creating the path
//! `Path::new("a/b/../c")` will return the path `a/c`. Similarly any attempt to mutate the path
//! will always leave it in normalized form.
//!
//! When rendering a path to some form of output, there is a method `.display()` which is
//! compatible with the `format!()` parameter `{}`. This will render the path as a string,
//! replacing all non-utf8 sequences with the Replacement Character (U+FFFD). As such it is not
//! suitable for passing to any API that actually operates on the path; it is only intended for
//! display.
//!
//! ## Examples
//!
//! ```rust,ignore
//! # #![feature(old_path, old_io)]
//! use std::old_io::fs::PathExtensions;
//! use std::old_path::{Path, GenericPath};
//!
//! let mut path = Path::new("/tmp/path");
//! println!("path: {}", path.display());
//! path.set_filename("foo");
//! path.push("bar");
//! println!("new path: {}", path.display());
//! println!("path exists: {}", path.exists());
//! ```

#![unstable(feature = "old_path")]
#![deprecated(since = "1.0.0", reason = "use std::path instead")]
#![allow(deprecated)] // seriously this is all deprecated
#![allow(unused_imports)]

use core::marker::Sized;
use ffi::CString;
use clone::Clone;
use borrow::Cow;
use fmt;
use iter::Iterator;
use option::Option;
use option::Option::{None, Some};
use str;
use string::String;
use vec::Vec;

/// Typedef for POSIX file paths.
/// See `posix::Path` for more info.
pub use self::posix::Path as PosixPath;

/// Typedef for Windows file paths.
/// See `windows::Path` for more info.
pub use self::windows::Path as WindowsPath;

/// Typedef for the platform-native path type
#[cfg(unix)]
pub use self::posix::Path as Path;
/// Typedef for the platform-native path type
#[cfg(windows)]
pub use self::windows::Path as Path;

/// Typedef for the platform-native component iterator
#[cfg(unix)]
pub use self::posix::Components as Components;
/// Typedef for the platform-native component iterator
#[cfg(windows)]
pub use self::windows::Components as Components;

/// Typedef for the platform-native str component iterator
#[cfg(unix)]
pub use self::posix::StrComponents as StrComponents;
/// Typedef for the platform-native str component iterator
#[cfg(windows)]
pub use self::windows::StrComponents as StrComponents;

/// Alias for the platform-native separator character.
#[cfg(unix)]
pub use self::posix::SEP as SEP;
/// Alias for the platform-native separator character.
#[cfg(windows)]
pub use self::windows::SEP as SEP;

/// Alias for the platform-native separator byte.
#[cfg(unix)]
pub use self::posix::SEP_BYTE as SEP_BYTE;
/// Alias for the platform-native separator byte.
#[cfg(windows)]
pub use self::windows::SEP_BYTE as SEP_BYTE;

/// Typedef for the platform-native separator char func
#[cfg(unix)]
pub use self::posix::is_sep as is_sep;
/// Typedef for the platform-native separator char func
#[cfg(windows)]
pub use self::windows::is_sep as is_sep;
/// Typedef for the platform-native separator byte func
#[cfg(unix)]
pub use self::posix::is_sep_byte as is_sep_byte;
/// Typedef for the platform-native separator byte func
#[cfg(windows)]
pub use self::windows::is_sep_byte as is_sep_byte;

pub mod posix;
pub mod windows;

/// A trait that represents the generic operations available on paths
pub trait GenericPath: Clone + GenericPathUnsafe {
    /// Creates a new Path from a byte vector or string.
    /// The resulting Path will always be normalized.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #![feature(old_path)]
    /// # fn main() {
    /// use std::old_path::Path;
    /// let path = Path::new("foo/bar");
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics the task if the path contains a NUL.
    ///
    /// See individual Path impls for additional restrictions.
    #[inline]
    fn new<T: BytesContainer>(path: T) -> Self {
        assert!(!contains_nul(&path));
        unsafe { GenericPathUnsafe::new_unchecked(path) }
    }

    /// Creates a new Path from a byte vector or string, if possible.
    /// The resulting Path will always be normalized.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #![feature(old_path)]
    /// # fn main() {
    /// use std::old_path::Path;
    /// let x: &[u8] = b"foo\0";
    /// assert!(Path::new_opt(x).is_none());
    /// # }
    /// ```
    #[inline]
    fn new_opt<T: BytesContainer>(path: T) -> Option<Self> {
        if contains_nul(&path) {
            None
        } else {
            Some(unsafe { GenericPathUnsafe::new_unchecked(path) })
        }
    }

    /// Returns the path as a string, if possible.
    /// If the path is not representable in utf-8, this returns None.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("/abc/def");
    /// assert_eq!(p.as_str(), Some("/abc/def"));
    /// # }
    /// ```
    #[inline]
    fn as_str<'a>(&'a self) -> Option<&'a str> {
        str::from_utf8(self.as_vec()).ok()
    }

    /// Returns the path as a byte vector
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def");
    /// assert_eq!(p.as_vec(), b"abc/def");
    /// # }
    /// ```
    fn as_vec<'a>(&'a self) -> &'a [u8];

    /// Converts the Path into an owned byte vector
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def");
    /// assert_eq!(p.into_vec(), b"abc/def".to_vec());
    /// // attempting to use p now results in "error: use of moved value"
    /// # }
    /// ```
    fn into_vec(self) -> Vec<u8>;

    /// Returns an object that implements `Display` for printing paths
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def");
    /// println!("{}", p.display()); // prints "abc/def"
    /// # }
    /// ```
    fn display<'a>(&'a self) -> Display<'a, Self> {
        Display{ path: self, filename: false }
    }

    /// Returns an object that implements `Display` for printing filenames
    ///
    /// If there is no filename, nothing will be printed.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def");
    /// println!("{}", p.filename_display()); // prints "def"
    /// # }
    /// ```
    fn filename_display<'a>(&'a self) -> Display<'a, Self> {
        Display{ path: self, filename: true }
    }

    /// Returns the directory component of `self`, as a byte vector (with no trailing separator).
    /// If `self` has no directory component, returns ['.'].
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def/ghi");
    /// assert_eq!(p.dirname(), b"abc/def");
    /// # }
    /// ```
    fn dirname<'a>(&'a self) -> &'a [u8];

    /// Returns the directory component of `self`, as a string, if possible.
    /// See `dirname` for details.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def/ghi");
    /// assert_eq!(p.dirname_str(), Some("abc/def"));
    /// # }
    /// ```
    #[inline]
    fn dirname_str<'a>(&'a self) -> Option<&'a str> {
        str::from_utf8(self.dirname()).ok()
    }

    /// Returns the file component of `self`, as a byte vector.
    /// If `self` represents the root of the file hierarchy, returns None.
    /// If `self` is "." or "..", returns None.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def/ghi");
    /// assert_eq!(p.filename(), Some(&b"ghi"[..]));
    /// # }
    /// ```
    fn filename<'a>(&'a self) -> Option<&'a [u8]>;

    /// Returns the file component of `self`, as a string, if possible.
    /// See `filename` for details.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def/ghi");
    /// assert_eq!(p.filename_str(), Some("ghi"));
    /// # }
    /// ```
    #[inline]
    fn filename_str<'a>(&'a self) -> Option<&'a str> {
        self.filename().and_then(|s| str::from_utf8(s).ok())
    }

    /// Returns the stem of the filename of `self`, as a byte vector.
    /// The stem is the portion of the filename just before the last '.'.
    /// If there is no '.', the entire filename is returned.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("/abc/def.txt");
    /// assert_eq!(p.filestem(), Some(&b"def"[..]));
    /// # }
    /// ```
    fn filestem<'a>(&'a self) -> Option<&'a [u8]> {
        match self.filename() {
            None => None,
            Some(name) => Some({
                let dot = b'.';
                match name.rposition_elem(&dot) {
                    None | Some(0) => name,
                    Some(1) if name == b".." => name,
                    Some(pos) => &name[..pos]
                }
            })
        }
    }

    /// Returns the stem of the filename of `self`, as a string, if possible.
    /// See `filestem` for details.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("/abc/def.txt");
    /// assert_eq!(p.filestem_str(), Some("def"));
    /// # }
    /// ```
    #[inline]
    fn filestem_str<'a>(&'a self) -> Option<&'a str> {
        self.filestem().and_then(|s| str::from_utf8(s).ok())
    }

    /// Returns the extension of the filename of `self`, as an optional byte vector.
    /// The extension is the portion of the filename just after the last '.'.
    /// If there is no extension, None is returned.
    /// If the filename ends in '.', the empty vector is returned.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def.txt");
    /// assert_eq!(p.extension(), Some(&b"txt"[..]));
    /// # }
    /// ```
    fn extension<'a>(&'a self) -> Option<&'a [u8]> {
        match self.filename() {
            None => None,
            Some(name) => {
                let dot = b'.';
                match name.rposition_elem(&dot) {
                    None | Some(0) => None,
                    Some(1) if name == b".." => None,
                    Some(pos) => Some(&name[pos+1..])
                }
            }
        }
    }

    /// Returns the extension of the filename of `self`, as a string, if possible.
    /// See `extension` for details.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def.txt");
    /// assert_eq!(p.extension_str(), Some("txt"));
    /// # }
    /// ```
    #[inline]
    fn extension_str<'a>(&'a self) -> Option<&'a str> {
        self.extension().and_then(|s| str::from_utf8(s).ok())
    }

    /// Replaces the filename portion of the path with the given byte vector or string.
    /// If the replacement name is [], this is equivalent to popping the path.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let mut p = Path::new("abc/def.txt");
    /// p.set_filename("foo.dat");
    /// assert!(p == Path::new("abc/foo.dat"));
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics the task if the filename contains a NUL.
    #[inline]
    fn set_filename<T: BytesContainer>(&mut self, filename: T) {
        assert!(!contains_nul(&filename));
        unsafe { self.set_filename_unchecked(filename) }
    }

    /// Replaces the extension with the given byte vector or string.
    /// If there is no extension in `self`, this adds one.
    /// If the argument is [] or "", this removes the extension.
    /// If `self` has no filename, this is a no-op.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let mut p = Path::new("abc/def.txt");
    /// p.set_extension("csv");
    /// assert_eq!(p, Path::new("abc/def.csv"));
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics the task if the extension contains a NUL.
    fn set_extension<T: BytesContainer>(&mut self, extension: T) {
        assert!(!contains_nul(&extension));

        let val = self.filename().and_then(|name| {
            let dot = b'.';
            let extlen = extension.container_as_bytes().len();
            match (name.rposition_elem(&dot), extlen) {
                (None, 0) | (Some(0), 0) => None,
                (Some(idx), 0) => Some(name[..idx].to_vec()),
                (idx, extlen) => {
                    let idx = match idx {
                        None | Some(0) => name.len(),
                        Some(val) => val
                    };

                    let mut v;
                    v = Vec::with_capacity(idx + extlen + 1);
                    v.push_all(&name[..idx]);
                    v.push(dot);
                    v.push_all(extension.container_as_bytes());
                    Some(v)
                }
            }
        });

        match val {
            None => (),
            Some(v) => unsafe { self.set_filename_unchecked(v) }
        }
    }

    /// Returns a new Path constructed by replacing the filename with the given
    /// byte vector or string.
    /// See `set_filename` for details.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let mut p = Path::new("abc/def.txt");
    /// assert_eq!(p.with_filename("foo.dat"), Path::new("abc/foo.dat"));
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics the task if the filename contains a NUL.
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
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let mut p = Path::new("abc/def.txt");
    /// assert_eq!(p.with_extension("csv"), Path::new("abc/def.csv"));
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics the task if the extension contains a NUL.
    #[inline]
    fn with_extension<T: BytesContainer>(&self, extension: T) -> Self {
        let mut p = self.clone();
        p.set_extension(extension);
        p
    }

    /// Returns the directory component of `self`, as a Path.
    /// If `self` represents the root of the filesystem hierarchy, returns `self`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def/ghi");
    /// assert_eq!(p.dir_path(), Path::new("abc/def"));
    /// # }
    /// ```
    fn dir_path(&self) -> Self {
        // self.dirname() returns a NUL-free vector
        unsafe { GenericPathUnsafe::new_unchecked(self.dirname()) }
    }

    /// Returns a Path that represents the filesystem root that `self` is rooted in.
    ///
    /// If `self` is not absolute, or vol/cwd-relative in the case of Windows, this returns None.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// assert_eq!(Path::new("abc/def").root_path(), None);
    /// assert_eq!(Path::new("/abc/def").root_path(), Some(Path::new("/")));
    /// # }
    /// ```
    fn root_path(&self) -> Option<Self>;

    /// Pushes a path (as a byte vector or string) onto `self`.
    /// If the argument represents an absolute path, it replaces `self`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let mut p = Path::new("foo/bar");
    /// p.push("baz.txt");
    /// assert_eq!(p, Path::new("foo/bar/baz.txt"));
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics the task if the path contains a NUL.
    #[inline]
    fn push<T: BytesContainer>(&mut self, path: T) {
        assert!(!contains_nul(&path));
        unsafe { self.push_unchecked(path) }
    }

    /// Pushes multiple paths (as byte vectors or strings) onto `self`.
    /// See `push` for details.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let mut p = Path::new("foo");
    /// p.push_many(&["bar", "baz.txt"]);
    /// assert_eq!(p, Path::new("foo/bar/baz.txt"));
    /// # }
    /// ```
    #[inline]
    fn push_many<T: BytesContainer>(&mut self, paths: &[T]) {
        let t: Option<&T> = None;
        if BytesContainer::is_str(t) {
            for p in paths {
                self.push(p.container_as_str().unwrap())
            }
        } else {
            for p in paths {
                self.push(p.container_as_bytes())
            }
        }
    }

    /// Removes the last path component from the receiver.
    /// Returns `true` if the receiver was modified, or `false` if it already
    /// represented the root of the file hierarchy.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let mut p = Path::new("foo/bar/baz.txt");
    /// p.pop();
    /// assert_eq!(p, Path::new("foo/bar"));
    /// # }
    /// ```
    fn pop(&mut self) -> bool;

    /// Returns a new Path constructed by joining `self` with the given path
    /// (as a byte vector or string).
    /// If the given path is absolute, the new Path will represent just that.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("/foo");
    /// assert_eq!(p.join("bar.txt"), Path::new("/foo/bar.txt"));
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics the task if the path contains a NUL.
    #[inline]
    fn join<T: BytesContainer>(&self, path: T) -> Self {
        let mut p = self.clone();
        p.push(path);
        p
    }

    /// Returns a new Path constructed by joining `self` with the given paths
    /// (as byte vectors or strings).
    /// See `join` for details.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("foo");
    /// let fbbq = Path::new("foo/bar/baz/quux.txt");
    /// assert_eq!(p.join_many(&["bar", "baz", "quux.txt"]), fbbq);
    /// # }
    /// ```
    #[inline]
    fn join_many<T: BytesContainer>(&self, paths: &[T]) -> Self {
        let mut p = self.clone();
        p.push_many(paths);
        p
    }

    /// Returns whether `self` represents an absolute path.
    /// An absolute path is defined as one that, when joined to another path, will
    /// yield back the same absolute path.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("/abc/def");
    /// assert!(p.is_absolute());
    /// # }
    /// ```
    fn is_absolute(&self) -> bool;

    /// Returns whether `self` represents a relative path.
    /// Typically this is the inverse of `is_absolute`.
    /// But for Windows paths, it also means the path is not volume-relative or
    /// relative to the current working directory.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("abc/def");
    /// assert!(p.is_relative());
    /// # }
    /// ```
    fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    /// Returns whether `self` is equal to, or is an ancestor of, the given path.
    /// If both paths are relative, they are compared as though they are relative
    /// to the same parent path.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("foo/bar/baz/quux.txt");
    /// let fb = Path::new("foo/bar");
    /// let bq = Path::new("baz/quux.txt");
    /// assert!(fb.is_ancestor_of(&p));
    /// # }
    /// ```
    fn is_ancestor_of(&self, other: &Self) -> bool;

    /// Returns the Path that, were it joined to `base`, would yield `self`.
    /// If no such path exists, None is returned.
    /// If `self` is absolute and `base` is relative, or on Windows if both
    /// paths refer to separate drives, an absolute path is returned.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("foo/bar/baz/quux.txt");
    /// let fb = Path::new("foo/bar");
    /// let bq = Path::new("baz/quux.txt");
    /// assert_eq!(p.path_relative_from(&fb), Some(bq));
    /// # }
    /// ```
    fn path_relative_from(&self, base: &Self) -> Option<Self>;

    /// Returns whether the relative path `child` is a suffix of `self`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # #![feature(old_path)]
    /// use std::old_path::{Path, GenericPath};
    /// # foo();
    /// # #[cfg(windows)] fn foo() {}
    /// # #[cfg(unix)] fn foo() {
    /// let p = Path::new("foo/bar/baz/quux.txt");
    /// let bq = Path::new("baz/quux.txt");
    /// assert!(p.ends_with_path(&bq));
    /// # }
    /// ```
    fn ends_with_path(&self, child: &Self) -> bool;
}

/// A trait that represents something bytes-like (e.g. a &[u8] or a &str)
pub trait BytesContainer {
    /// Returns a &[u8] representing the receiver
    fn container_as_bytes<'a>(&'a self) -> &'a [u8];
    /// Returns the receiver interpreted as a utf-8 string, if possible
    #[inline]
    fn container_as_str<'a>(&'a self) -> Option<&'a str> {
        str::from_utf8(self.container_as_bytes()).ok()
    }
    /// Returns whether .container_as_str() is guaranteed to not fail
    // FIXME (#8888): Remove unused arg once ::<for T> works
    #[inline]
    fn is_str(_: Option<&Self>) -> bool { false }
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
pub struct Display<'a, P:'a> {
    path: &'a P,
    filename: bool
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<'a, P: GenericPath> fmt::Debug for Display<'a, P> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.as_cow(), f)
    }
}

#[stable(feature = "rust1", since = "1.0.0")]
impl<'a, P: GenericPath> fmt::Display for Display<'a, P> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_cow().fmt(f)
    }
}

impl<'a, P: GenericPath> Display<'a, P> {
    /// Returns the path as a possibly-owned string.
    ///
    /// If the path is not UTF-8, invalid sequences will be replaced with the
    /// Unicode replacement char. This involves allocation.
    #[inline]
    pub fn as_cow(&self) -> Cow<'a, str> {
        String::from_utf8_lossy(if self.filename {
            match self.path.filename() {
                None => {
                    let result: &[u8] = &[];
                    result
                }
                Some(v) => v
            }
        } else {
            self.path.as_vec()
        })
    }
}

impl BytesContainer for str {
    #[inline]
    fn container_as_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
    #[inline]
    fn container_as_str(&self) -> Option<&str> {
        Some(self)
    }
    #[inline]
    fn is_str(_: Option<&str>) -> bool { true }
}

impl BytesContainer for String {
    #[inline]
    fn container_as_bytes(&self) -> &[u8] {
        self.as_bytes()
    }
    #[inline]
    fn container_as_str(&self) -> Option<&str> {
        Some(&self[..])
    }
    #[inline]
    fn is_str(_: Option<&String>) -> bool { true }
}

impl BytesContainer for [u8] {
    #[inline]
    fn container_as_bytes(&self) -> &[u8] {
        self
    }
}

impl BytesContainer for Vec<u8> {
    #[inline]
    fn container_as_bytes(&self) -> &[u8] {
        &self[..]
    }
}

impl BytesContainer for CString {
    #[inline]
    fn container_as_bytes<'a>(&'a self) -> &'a [u8] {
        self.as_bytes()
    }
}

impl<'a, T: ?Sized + BytesContainer> BytesContainer for &'a T {
    #[inline]
    fn container_as_bytes(&self) -> &[u8] {
        (**self).container_as_bytes()
    }
    #[inline]
    fn container_as_str(&self) -> Option<&str> {
        (**self).container_as_str()
    }
    #[inline]
    fn is_str(_: Option<& &'a T>) -> bool { BytesContainer::is_str(None::<&T>) }
}

#[inline(always)]
fn contains_nul<T: BytesContainer>(v: &T) -> bool {
    v.container_as_bytes().iter().any(|&x| x == 0)
}
