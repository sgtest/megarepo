#![feature(rustc_private)]

#[cfg(unix)]
extern crate libc;
extern crate run_make_support;

use run_make_support::{aux_build, tmp_dir};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn main() {
    #[cfg(unix)]
    unsafe {
        libc::umask(0o002);
    }

    aux_build().arg("foo.rs").run();
    verify(&tmp_dir().join("libfoo.rlib"));
}

fn verify(path: &Path) {
    let perm = fs::metadata(path).unwrap().permissions();

    assert!(!perm.readonly());

    // Check that the file is readable for everyone
    #[cfg(unix)]
    assert_eq!(perm.mode(), 0o100664);
}
