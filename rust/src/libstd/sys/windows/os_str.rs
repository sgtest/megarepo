// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/// The underlying OsString/OsStr implementation on Windows is a
/// wrapper around the "WTF-8" encoding; see the `wtf8` module for more.

use borrow::Cow;
use fmt::{self, Debug};
use sys_common::wtf8::{Wtf8, Wtf8Buf};
use mem;
use sys_common::{AsInner, IntoInner};

#[derive(Clone, Hash)]
pub struct Buf {
    pub inner: Wtf8Buf
}

impl IntoInner<Wtf8Buf> for Buf {
    fn into_inner(self) -> Wtf8Buf {
        self.inner
    }
}

impl AsInner<Wtf8> for Buf {
    fn as_inner(&self) -> &Wtf8 {
        &self.inner
    }
}

impl Debug for Buf {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.as_slice().fmt(formatter)
    }
}

pub struct Slice {
    pub inner: Wtf8
}

impl Debug for Slice {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        self.inner.fmt(formatter)
    }
}

impl Buf {
    pub fn with_capacity(capacity: usize) -> Buf {
        Buf {
            inner: Wtf8Buf::with_capacity(capacity)
        }
    }

    pub fn clear(&mut self) {
        self.inner.clear()
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    pub fn from_string(s: String) -> Buf {
        Buf { inner: Wtf8Buf::from_string(s) }
    }

    pub fn as_slice(&self) -> &Slice {
        unsafe { mem::transmute(self.inner.as_slice()) }
    }

    pub fn into_string(self) -> Result<String, Buf> {
        self.inner.into_string().map_err(|buf| Buf { inner: buf })
    }

    pub fn push_slice(&mut self, s: &Slice) {
        self.inner.push_wtf8(&s.inner)
    }

    pub fn reserve(&mut self, additional: usize) {
        self.inner.reserve(additional)
    }

    pub fn reserve_exact(&mut self, additional: usize) {
        self.inner.reserve_exact(additional)
    }

    #[inline]
    pub fn into_box(self) -> Box<Slice> {
        unsafe { mem::transmute(self.inner.into_box()) }
    }
}

impl Slice {
    pub fn from_str(s: &str) -> &Slice {
        unsafe { mem::transmute(Wtf8::from_str(s)) }
    }

    pub fn to_str(&self) -> Option<&str> {
        self.inner.as_str()
    }

    pub fn to_string_lossy(&self) -> Cow<str> {
        self.inner.to_string_lossy()
    }

    pub fn to_owned(&self) -> Buf {
        let mut buf = Wtf8Buf::with_capacity(self.inner.len());
        buf.push_wtf8(&self.inner);
        Buf { inner: buf }
    }

    #[inline]
    pub fn into_box(&self) -> Box<Slice> {
        unsafe { mem::transmute(self.inner.into_box()) }
    }

    pub fn empty_box() -> Box<Slice> {
        unsafe { mem::transmute(Wtf8::empty_box()) }
    }
}
