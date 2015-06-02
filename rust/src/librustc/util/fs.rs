// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::path::{self, Path, PathBuf};
use std::ffi::OsString;

// Unfortunately, on windows, it looks like msvcrt.dll is silently translating
// verbatim paths under the hood to non-verbatim paths! This manifests itself as
// gcc looking like it cannot accept paths of the form `\\?\C:\...`, but the
// real bug seems to lie in msvcrt.dll.
//
// Verbatim paths are generally pretty rare, but the implementation of
// `fs::canonicalize` currently generates paths of this form, meaning that we're
// going to be passing quite a few of these down to gcc, so we need to deal with
// this case.
//
// For now we just strip the "verbatim prefix" of `\\?\` from the path. This
// will probably lose information in some cases, but there's not a whole lot
// more we can do with a buggy msvcrt...
//
// For some more information, see this comment:
//   https://github.com/rust-lang/rust/issues/25505#issuecomment-102876737
pub fn fix_windows_verbatim_for_gcc(p: &Path) -> PathBuf {
    if !cfg!(windows) {
        return p.to_path_buf()
    }
    let mut components = p.components();
    let prefix = match components.next() {
        Some(path::Component::Prefix(p)) => p,
        _ => return p.to_path_buf(),
    };
    match prefix.kind() {
        path::Prefix::VerbatimDisk(disk) => {
            let mut base = OsString::from(format!("{}:", disk as char));
            base.push(components.as_path());
            PathBuf::from(base)
        }
        path::Prefix::VerbatimUNC(server, share) => {
            let mut base = OsString::from(r"\\");
            base.push(server);
            base.push(r"\");
            base.push(share);
            base.push(components.as_path());
            PathBuf::from(base)
        }
        _ => p.to_path_buf(),
    }
}
