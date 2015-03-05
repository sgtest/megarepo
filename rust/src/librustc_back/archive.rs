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

use std::env;
use std::fs::{self, TempDir};
use std::io::prelude::*;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::str;
use syntax::diagnostic::Handler as ErrorHandler;

pub const METADATA_FILENAME: &'static str = "rust.metadata.bin";

pub struct ArchiveConfig<'a> {
    pub handler: &'a ErrorHandler,
    pub dst: PathBuf,
    pub lib_search_paths: Vec<PathBuf>,
    pub slib_prefix: String,
    pub slib_suffix: String,
    pub maybe_ar_prog: Option<String>
}

pub struct Archive<'a> {
    handler: &'a ErrorHandler,
    dst: PathBuf,
    lib_search_paths: Vec<PathBuf>,
    slib_prefix: String,
    slib_suffix: String,
    maybe_ar_prog: Option<String>
}

/// Helper for adding many files to an archive with a single invocation of
/// `ar`.
#[must_use = "must call build() to finish building the archive"]
pub struct ArchiveBuilder<'a> {
    archive: Archive<'a>,
    work_dir: TempDir,
    /// Filename of each member that should be added to the archive.
    members: Vec<PathBuf>,
    should_update_symbols: bool,
}

fn run_ar(handler: &ErrorHandler, maybe_ar_prog: &Option<String>,
          args: &str, cwd: Option<&Path>,
          paths: &[&Path]) -> Output {
    let ar = match *maybe_ar_prog {
        Some(ref ar) => &ar[..],
        None => "ar"
    };
    let mut cmd = Command::new(ar);

    cmd.arg(args).args(paths).stdout(Stdio::piped()).stderr(Stdio::piped());
    debug!("{:?}", cmd);

    match cwd {
        Some(p) => {
            cmd.current_dir(p);
            debug!("inside {:?}", p.display());
        }
        None => {}
    }

    match cmd.spawn() {
        Ok(prog) => {
            let o = prog.wait_with_output().unwrap();
            if !o.status.success() {
                handler.err(&format!("{:?} failed with: {}", cmd, o.status));
                handler.note(&format!("stdout ---\n{}",
                                  str::from_utf8(&o.stdout).unwrap()));
                handler.note(&format!("stderr ---\n{}",
                                  str::from_utf8(&o.stderr).unwrap())
                             );
                handler.abort_if_errors();
            }
            o
        },
        Err(e) => {
            handler.err(&format!("could not exec `{}`: {}", &ar[..],
                             e));
            handler.abort_if_errors();
            panic!("rustc::back::archive::run_ar() should not reach this point");
        }
    }
}

pub fn find_library(name: &str, osprefix: &str, ossuffix: &str,
                    search_paths: &[PathBuf],
                    handler: &ErrorHandler) -> PathBuf {
    // On Windows, static libraries sometimes show up as libfoo.a and other
    // times show up as foo.lib
    let oslibname = format!("{}{}{}", osprefix, name, ossuffix);
    let unixlibname = format!("lib{}.a", name);

    for path in search_paths {
        debug!("looking for {} inside {:?}", name, path);
        let test = path.join(&oslibname[..]);
        if test.exists() { return test }
        if oslibname != unixlibname {
            let test = path.join(&unixlibname[..]);
            if test.exists() { return test }
        }
    }
    handler.fatal(&format!("could not find native static library `{}`, \
                           perhaps an -L flag is missing?",
                          name));
}

impl<'a> Archive<'a> {
    fn new(config: ArchiveConfig<'a>) -> Archive<'a> {
        let ArchiveConfig { handler, dst, lib_search_paths, slib_prefix, slib_suffix,
            maybe_ar_prog } = config;
        Archive {
            handler: handler,
            dst: dst,
            lib_search_paths: lib_search_paths,
            slib_prefix: slib_prefix,
            slib_suffix: slib_suffix,
            maybe_ar_prog: maybe_ar_prog
        }
    }

    /// Opens an existing static archive
    pub fn open(config: ArchiveConfig<'a>) -> Archive<'a> {
        let archive = Archive::new(config);
        assert!(archive.dst.exists());
        archive
    }

    /// Removes a file from this archive
    pub fn remove_file(&mut self, file: &str) {
        run_ar(self.handler, &self.maybe_ar_prog, "d", None, &[&self.dst, &Path::new(file)]);
    }

    /// Lists all files in an archive
    pub fn files(&self) -> Vec<String> {
        let output = run_ar(self.handler, &self.maybe_ar_prog, "t", None, &[&self.dst]);
        let output = str::from_utf8(&output.stdout).unwrap();
        // use lines_any because windows delimits output with `\r\n` instead of
        // just `\n`
        output.lines_any().map(|s| s.to_string()).collect()
    }

    /// Creates an `ArchiveBuilder` for adding files to this archive.
    pub fn extend(self) -> ArchiveBuilder<'a> {
        ArchiveBuilder::new(self)
    }
}

impl<'a> ArchiveBuilder<'a> {
    fn new(archive: Archive<'a>) -> ArchiveBuilder<'a> {
        ArchiveBuilder {
            archive: archive,
            work_dir: TempDir::new("rsar").unwrap(),
            members: vec![],
            should_update_symbols: false,
        }
    }

    /// Create a new static archive, ready for adding files.
    pub fn create(config: ArchiveConfig<'a>) -> ArchiveBuilder<'a> {
        let archive = Archive::new(config);
        ArchiveBuilder::new(archive)
    }

    /// Adds all of the contents of a native library to this archive. This will
    /// search in the relevant locations for a library named `name`.
    pub fn add_native_library(&mut self, name: &str) -> io::Result<()> {
        let location = find_library(name,
                                    &self.archive.slib_prefix,
                                    &self.archive.slib_suffix,
                                    &self.archive.lib_search_paths,
                                    self.archive.handler);
        self.add_archive(&location, name, |_| false)
    }

    /// Adds all of the contents of the rlib at the specified path to this
    /// archive.
    ///
    /// This ignores adding the bytecode from the rlib, and if LTO is enabled
    /// then the object file also isn't added.
    pub fn add_rlib(&mut self, rlib: &Path, name: &str,
                    lto: bool) -> io::Result<()> {
        // Ignoring obj file starting with the crate name
        // as simple comparison is not enough - there
        // might be also an extra name suffix
        let obj_start = format!("{}", name);
        let obj_start = &obj_start[..];
        // Ignoring all bytecode files, no matter of
        // name
        let bc_ext = ".bytecode.deflate";

        self.add_archive(rlib, &name[..], |fname: &str| {
            let skip_obj = lto && fname.starts_with(obj_start)
                && fname.ends_with(".o");
            skip_obj || fname.ends_with(bc_ext) || fname == METADATA_FILENAME
        })
    }

    /// Adds an arbitrary file to this archive
    pub fn add_file(&mut self, file: &Path) -> io::Result<()> {
        let filename = Path::new(file.file_name().unwrap());
        let new_file = self.work_dir.path().join(&filename);
        try!(fs::copy(file, &new_file));
        self.members.push(filename.to_path_buf());
        Ok(())
    }

    /// Indicate that the next call to `build` should updates all symbols in
    /// the archive (run 'ar s' over it).
    pub fn update_symbols(&mut self) {
        self.should_update_symbols = true;
    }

    /// Combine the provided files, rlibs, and native libraries into a single
    /// `Archive`.
    pub fn build(self) -> Archive<'a> {
        // Get an absolute path to the destination, so `ar` will work even
        // though we run it from `self.work_dir`.
        let abs_dst = env::current_dir().unwrap().join(&self.archive.dst);
        assert!(!abs_dst.is_relative());
        let mut args = vec![&*abs_dst];
        let mut total_len = abs_dst.to_string_lossy().len();

        if self.members.is_empty() {
            // OSX `ar` does not allow using `r` with no members, but it does
            // allow running `ar s file.a` to update symbols only.
            if self.should_update_symbols {
                run_ar(self.archive.handler, &self.archive.maybe_ar_prog,
                       "s", Some(self.work_dir.path()), &args[..]);
            }
            return self.archive;
        }

        // Don't allow the total size of `args` to grow beyond 32,000 bytes.
        // Windows will raise an error if the argument string is longer than
        // 32,768, and we leave a bit of extra space for the program name.
        const ARG_LENGTH_LIMIT: uint = 32_000;

        for member_name in &self.members {
            let len = member_name.to_string_lossy().len();

            // `len + 1` to account for the space that's inserted before each
            // argument.  (Windows passes command-line arguments as a single
            // string, not an array of strings.)
            if total_len + len + 1 > ARG_LENGTH_LIMIT {
                // Add the archive members seen so far, without updating the
                // symbol table (`S`).
                run_ar(self.archive.handler, &self.archive.maybe_ar_prog,
                       "cruS", Some(self.work_dir.path()), &args[..]);

                args.clear();
                args.push(&abs_dst);
                total_len = abs_dst.to_string_lossy().len();
            }

            args.push(member_name);
            total_len += len + 1;
        }

        // Add the remaining archive members, and update the symbol table if
        // necessary.
        let flags = if self.should_update_symbols { "crus" } else { "cruS" };
        run_ar(self.archive.handler, &self.archive.maybe_ar_prog,
               flags, Some(self.work_dir.path()), &args[..]);

        self.archive
    }

    fn add_archive<F>(&mut self, archive: &Path, name: &str,
                      mut skip: F) -> io::Result<()>
        where F: FnMut(&str) -> bool,
    {
        let loc = TempDir::new("rsar").unwrap();

        // First, extract the contents of the archive to a temporary directory.
        // We don't unpack directly into `self.work_dir` due to the possibility
        // of filename collisions.
        let archive = env::current_dir().unwrap().join(archive);
        run_ar(self.archive.handler, &self.archive.maybe_ar_prog,
               "x", Some(loc.path()), &[&archive]);

        // Next, we must rename all of the inputs to "guaranteed unique names".
        // We move each file into `self.work_dir` under its new unique name.
        // The reason for this renaming is that archives are keyed off the name
        // of the files, so if two files have the same name they will override
        // one another in the archive (bad).
        //
        // We skip any files explicitly desired for skipping, and we also skip
        // all SYMDEF files as these are just magical placeholders which get
        // re-created when we make a new archive anyway.
        let files = try!(fs::read_dir(loc.path()));
        for file in files {
            let file = try!(file).path();
            let filename = file.file_name().unwrap().to_str().unwrap();
            if skip(filename) { continue }
            if filename.contains(".SYMDEF") { continue }

            let filename = format!("r-{}-{}", name, filename);
            // LLDB (as mentioned in back::link) crashes on filenames of exactly
            // 16 bytes in length. If we're including an object file with
            // exactly 16-bytes of characters, give it some prefix so that it's
            // not 16 bytes.
            let filename = if filename.len() == 16 {
                format!("lldb-fix-{}", filename)
            } else {
                filename
            };
            let new_filename = self.work_dir.path().join(&filename[..]);
            try!(fs::rename(&file, &new_filename));
            self.members.push(PathBuf::new(&filename));
        }
        Ok(())
    }
}

