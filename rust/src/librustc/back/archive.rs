// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! A helper class for dealing with static archives

use back::link::{get_ar_prog};
use driver::session::Session;
use metadata::filesearch;
use lib::llvm::{ArchiveRef, llvm};

use std::cast;
use std::io::fs;
use std::io;
use std::libc;
use std::os;
use std::run::{ProcessOptions, Process, ProcessOutput};
use std::str;
use std::unstable::raw;
use extra::tempfile::TempDir;
use syntax::abi;

pub static METADATA_FILENAME: &'static str = "rust.metadata.bin";

pub struct Archive {
    priv sess: Session,
    priv dst: Path,
}

pub struct ArchiveRO {
    priv ptr: ArchiveRef,
}

fn run_ar(sess: Session, args: &str, cwd: Option<&Path>,
        paths: &[&Path]) -> ProcessOutput {
    let ar = get_ar_prog(sess);

    let mut args = ~[args.to_owned()];
    let mut paths = paths.iter().map(|p| p.as_str().unwrap().to_owned());
    args.extend(&mut paths);
    let mut opts = ProcessOptions::new();
    opts.dir = cwd;
    debug!("{} {}", ar, args.connect(" "));
    match cwd {
        Some(p) => { debug!("inside {}", p.display()); }
        None => {}
    }
    match Process::new(ar, args.as_slice(), opts) {
        Ok(mut prog) => {
            let o = prog.finish_with_output();
            if !o.status.success() {
                sess.err(format!("{} {} failed with: {}", ar, args.connect(" "),
                                 o.status));
                sess.note(format!("stdout ---\n{}", str::from_utf8(o.output).unwrap()));
                sess.note(format!("stderr ---\n{}", str::from_utf8(o.error).unwrap()));
                sess.abort_if_errors();
            }
            o
        },
        Err(e) => {
            sess.err(format!("could not exec `{}`: {}", ar, e));
            sess.abort_if_errors();
            fail!("rustc::back::archive::run_ar() should not reach this point");
        }
    }
}

impl Archive {
    /// Initializes a new static archive with the given object file
    pub fn create<'a>(sess: Session, dst: &'a Path,
                      initial_object: &'a Path) -> Archive {
        run_ar(sess, "crus", None, [dst, initial_object]);
        Archive { sess: sess, dst: dst.clone() }
    }

    /// Opens an existing static archive
    pub fn open(sess: Session, dst: Path) -> Archive {
        assert!(dst.exists());
        Archive { sess: sess, dst: dst }
    }

    /// Read a file in the archive
    pub fn read(&self, file: &str) -> ~[u8] {
        // Apparently if "ar p" is used on windows, it generates a corrupt file
        // which has bad headers and LLVM will immediately choke on it
        if cfg!(windows) && cfg!(windows) { // FIXME(#10734) double-and
            let loc = TempDir::new("rsar").unwrap();
            let archive = os::make_absolute(&self.dst);
            run_ar(self.sess, "x", Some(loc.path()), [&archive,
                                                      &Path::new(file)]);
            fs::File::open(&loc.path().join(file)).read_to_end().unwrap()
        } else {
            run_ar(self.sess, "p", None, [&self.dst, &Path::new(file)]).output
        }
    }

    /// Adds all of the contents of a native library to this archive. This will
    /// search in the relevant locations for a library named `name`.
    pub fn add_native_library(&mut self, name: &str) -> io::IoResult<()> {
        let location = self.find_library(name);
        self.add_archive(&location, name, [])
    }

    /// Adds all of the contents of the rlib at the specified path to this
    /// archive.
    ///
    /// This ignores adding the bytecode from the rlib, and if LTO is enabled
    /// then the object file also isn't added.
    pub fn add_rlib(&mut self, rlib: &Path, name: &str,
                    lto: bool) -> io::IoResult<()> {
        let object = format!("{}.o", name);
        let bytecode = format!("{}.bc", name);
        let mut ignore = ~[METADATA_FILENAME, bytecode.as_slice()];
        if lto {
            ignore.push(object.as_slice());
        }
        self.add_archive(rlib, name, ignore)
    }

    /// Adds an arbitrary file to this archive
    pub fn add_file(&mut self, file: &Path, has_symbols: bool) {
        let cmd = if has_symbols {"r"} else {"rS"};
        run_ar(self.sess, cmd, None, [&self.dst, file]);
    }

    /// Removes a file from this archive
    pub fn remove_file(&mut self, file: &str) {
        run_ar(self.sess, "d", None, [&self.dst, &Path::new(file)]);
    }

    /// Updates all symbols in the archive (runs 'ar s' over it)
    pub fn update_symbols(&mut self) {
        run_ar(self.sess, "s", None, [&self.dst]);
    }

    /// Lists all files in an archive
    pub fn files(&self) -> ~[~str] {
        let output = run_ar(self.sess, "t", None, [&self.dst]);
        str::from_utf8(output.output).unwrap().lines().map(|s| s.to_owned()).collect()
    }

    fn add_archive(&mut self, archive: &Path, name: &str,
                   skip: &[&str]) -> io::IoResult<()> {
        let loc = TempDir::new("rsar").unwrap();

        // First, extract the contents of the archive to a temporary directory
        let archive = os::make_absolute(archive);
        run_ar(self.sess, "x", Some(loc.path()), [&archive]);

        // Next, we must rename all of the inputs to "guaranteed unique names".
        // The reason for this is that archives are keyed off the name of the
        // files, so if two files have the same name they will override one
        // another in the archive (bad).
        //
        // We skip any files explicitly desired for skipping, and we also skip
        // all SYMDEF files as these are just magical placeholders which get
        // re-created when we make a new archive anyway.
        let files = try!(fs::readdir(loc.path()));
        let mut inputs = ~[];
        for file in files.iter() {
            let filename = file.filename_str().unwrap();
            if skip.iter().any(|s| *s == filename) { continue }
            if filename.contains(".SYMDEF") { continue }

            let filename = format!("r-{}-{}", name, filename);
            let new_filename = file.with_filename(filename);
            try!(fs::rename(file, &new_filename));
            inputs.push(new_filename);
        }
        if inputs.len() == 0 { return Ok(()) }

        // Finally, add all the renamed files to this archive
        let mut args = ~[&self.dst];
        args.extend(&mut inputs.iter());
        run_ar(self.sess, "r", None, args.as_slice());
        Ok(())
    }

    fn find_library(&self, name: &str) -> Path {
        let (osprefix, osext) = match self.sess.targ_cfg.os {
            abi::OsWin32 => ("", "lib"), _ => ("lib", "a"),
        };
        // On Windows, static libraries sometimes show up as libfoo.a and other
        // times show up as foo.lib
        let oslibname = format!("{}{}.{}", osprefix, name, osext);
        let unixlibname = format!("lib{}.a", name);

        let mut rustpath = filesearch::rust_path();
        rustpath.push(self.sess.filesearch.get_target_lib_path());
        let addl_lib_search_paths = self.sess
                                        .opts
                                        .addl_lib_search_paths
                                        .borrow();
        let path = addl_lib_search_paths.get().iter();
        for path in path.chain(rustpath.iter()) {
            debug!("looking for {} inside {}", name, path.display());
            let test = path.join(oslibname.as_slice());
            if test.exists() { return test }
            if oslibname != unixlibname {
                let test = path.join(unixlibname.as_slice());
                if test.exists() { return test }
            }
        }
        self.sess.fatal(format!("could not find native static library `{}`, \
                                 perhaps an -L flag is missing?", name));
    }
}

impl ArchiveRO {
    /// Opens a static archive for read-only purposes. This is more optimized
    /// than the `open` method because it uses LLVM's internal `Archive` class
    /// rather than shelling out to `ar` for everything.
    ///
    /// If this archive is used with a mutable method, then an error will be
    /// raised.
    pub fn open(dst: &Path) -> Option<ArchiveRO> {
        unsafe {
            let ar = dst.with_c_str(|dst| {
                llvm::LLVMRustOpenArchive(dst)
            });
            if ar.is_null() {
                None
            } else {
                Some(ArchiveRO { ptr: ar })
            }
        }
    }

    /// Reads a file in the archive
    pub fn read<'a>(&'a self, file: &str) -> Option<&'a [u8]> {
        unsafe {
            let mut size = 0 as libc::size_t;
            let ptr = file.with_c_str(|file| {
                llvm::LLVMRustArchiveReadSection(self.ptr, file, &mut size)
            });
            if ptr.is_null() {
                None
            } else {
                Some(cast::transmute(raw::Slice {
                    data: ptr,
                    len: size as uint,
                }))
            }
        }
    }
}

impl Drop for ArchiveRO {
    fn drop(&mut self) {
        unsafe {
            llvm::LLVMRustDestroyArchive(self.ptr);
        }
    }
}
