// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// ignore-windows TempDir may cause IoError on windows: #10463

// These tests are here to exercise the functionality of the `tempfile` module.
// One might expect these tests to be located in that module, but sadly they
// cannot. The tests need to invoke `os::change_dir` which cannot be done in the
// normal test infrastructure. If the tests change the current working
// directory, then *all* tests which require relative paths suddenly break b/c
// they're in a different location than before. Hence, these tests are all run
// serially here.

use std::old_io::fs::PathExtensions;
use std::old_io::{fs, TempDir};
use std::old_io;
use std::os;
use std::sync::mpsc::channel;
use std::thread;

fn test_tempdir() {
    let path = {
        let p = TempDir::new_in(&Path::new("."), "foobar").unwrap();
        let p = p.path();
        assert!(p.as_str().unwrap().contains("foobar"));
        p.clone()
    };
    assert!(!path.exists());
}

fn test_rm_tempdir() {
    let (tx, rx) = channel();
    let f = move|| -> () {
        let tmp = TempDir::new("test_rm_tempdir").unwrap();
        tx.send(tmp.path().clone()).unwrap();
        panic!("panic to unwind past `tmp`");
    };
    thread::spawn(f).join();
    let path = rx.recv().unwrap();
    assert!(!path.exists());

    let tmp = TempDir::new("test_rm_tempdir").unwrap();
    let path = tmp.path().clone();
    let f = move|| -> () {
        let _tmp = tmp;
        panic!("panic to unwind past `tmp`");
    };
    thread::spawn(f).join();
    assert!(!path.exists());

    let path;
    {
        let f = move || {
            TempDir::new("test_rm_tempdir").unwrap()
        };
        // FIXME(#16640) `: TempDir` annotation shouldn't be necessary
        let tmp: TempDir = thread::scoped(f).join();
        path = tmp.path().clone();
        assert!(path.exists());
    }
    assert!(!path.exists());

    let path;
    {
        let tmp = TempDir::new("test_rm_tempdir").unwrap();
        path = tmp.into_inner();
    }
    assert!(path.exists());
    fs::rmdir_recursive(&path);
    assert!(!path.exists());
}

fn test_rm_tempdir_close() {
    let (tx, rx) = channel();
    let f = move|| -> () {
        let tmp = TempDir::new("test_rm_tempdir").unwrap();
        tx.send(tmp.path().clone()).unwrap();
        tmp.close();
        panic!("panic when unwinding past `tmp`");
    };
    thread::spawn(f).join();
    let path = rx.recv().unwrap();
    assert!(!path.exists());

    let tmp = TempDir::new("test_rm_tempdir").unwrap();
    let path = tmp.path().clone();
    let f = move|| -> () {
        let tmp = tmp;
        tmp.close();
        panic!("panic when unwinding past `tmp`");
    };
    thread::spawn(f).join();
    assert!(!path.exists());

    let path;
    {
        let f = move || {
            TempDir::new("test_rm_tempdir").unwrap()
        };
        // FIXME(#16640) `: TempDir` annotation shouldn't be necessary
        let tmp: TempDir = thread::scoped(f).join();
        path = tmp.path().clone();
        assert!(path.exists());
        tmp.close();
    }
    assert!(!path.exists());

    let path;
    {
        let tmp = TempDir::new("test_rm_tempdir").unwrap();
        path = tmp.into_inner();
    }
    assert!(path.exists());
    fs::rmdir_recursive(&path);
    assert!(!path.exists());
}

// Ideally these would be in std::os but then core would need
// to depend on std
fn recursive_mkdir_rel() {
    let path = Path::new("frob");
    let cwd = os::getcwd().unwrap();
    println!("recursive_mkdir_rel: Making: {} in cwd {} [{}]", path.display(),
           cwd.display(), path.exists());
    fs::mkdir_recursive(&path, old_io::USER_RWX);
    assert!(path.is_dir());
    fs::mkdir_recursive(&path, old_io::USER_RWX);
    assert!(path.is_dir());
}

fn recursive_mkdir_dot() {
    let dot = Path::new(".");
    fs::mkdir_recursive(&dot, old_io::USER_RWX);
    let dotdot = Path::new("..");
    fs::mkdir_recursive(&dotdot, old_io::USER_RWX);
}

fn recursive_mkdir_rel_2() {
    let path = Path::new("./frob/baz");
    let cwd = os::getcwd().unwrap();
    println!("recursive_mkdir_rel_2: Making: {} in cwd {} [{}]", path.display(),
           cwd.display(), path.exists());
    fs::mkdir_recursive(&path, old_io::USER_RWX);
    assert!(path.is_dir());
    assert!(path.dir_path().is_dir());
    let path2 = Path::new("quux/blat");
    println!("recursive_mkdir_rel_2: Making: {} in cwd {}", path2.display(),
           cwd.display());
    fs::mkdir_recursive(&path2, old_io::USER_RWX);
    assert!(path2.is_dir());
    assert!(path2.dir_path().is_dir());
}

// Ideally this would be in core, but needs TempFile
pub fn test_rmdir_recursive_ok() {
    let rwx = old_io::USER_RWX;

    let tmpdir = TempDir::new("test").ok().expect("test_rmdir_recursive_ok: \
                                                   couldn't create temp dir");
    let tmpdir = tmpdir.path();
    let root = tmpdir.join("foo");

    println!("making {}", root.display());
    fs::mkdir(&root, rwx);
    fs::mkdir(&root.join("foo"), rwx);
    fs::mkdir(&root.join("foo").join("bar"), rwx);
    fs::mkdir(&root.join("foo").join("bar").join("blat"), rwx);
    fs::rmdir_recursive(&root);
    assert!(!root.exists());
    assert!(!root.join("bar").exists());
    assert!(!root.join("bar").join("blat").exists());
}

pub fn dont_double_panic() {
    let r: Result<(), _> = thread::spawn(move|| {
        let tmpdir = TempDir::new("test").unwrap();
        // Remove the temporary directory so that TempDir sees
        // an error on drop
        fs::rmdir(tmpdir.path());
        // Panic. If TempDir panics *again* due to the rmdir
        // error then the process will abort.
        panic!();
    }).join();
    assert!(r.is_err());
}

fn in_tmpdir<F>(f: F) where F: FnOnce() {
    let tmpdir = TempDir::new("test").ok().expect("can't make tmpdir");
    assert!(os::change_dir(tmpdir.path()).is_ok());

    f();
}

pub fn main() {
    in_tmpdir(test_tempdir);
    in_tmpdir(test_rm_tempdir);
    in_tmpdir(test_rm_tempdir_close);
    in_tmpdir(recursive_mkdir_rel);
    in_tmpdir(recursive_mkdir_dot);
    in_tmpdir(recursive_mkdir_rel_2);
    in_tmpdir(test_rmdir_recursive_ok);
    in_tmpdir(dont_double_panic);
}
