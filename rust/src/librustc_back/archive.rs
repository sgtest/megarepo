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
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::prelude::*;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::str;
use syntax::diagnostic::Handler as ErrorHandler;
use rustc_llvm::archive_ro::ArchiveRO;

use tempdir::TempDir;

pub const METADATA_FILENAME: &'static str = "rust.metadata.bin";

pub struct ArchiveConfig<'a> {
    pub handler: &'a ErrorHandler,
    pub dst: PathBuf,
    pub lib_search_paths: Vec<PathBuf>,
    pub slib_prefix: String,
    pub slib_suffix: String,
    pub ar_prog: String,
    pub command_path: OsString,
}

pub struct Archive<'a> {
    config: ArchiveConfig<'a>,
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

enum Action<'a> {
    Remove(&'a Path),
    AddObjects(&'a [&'a PathBuf], bool),
    UpdateSymbols,
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
        Archive { config: config }
    }

    /// Opens an existing static archive
    pub fn open(config: ArchiveConfig<'a>) -> Archive<'a> {
        let archive = Archive::new(config);
        assert!(archive.config.dst.exists());
        archive
    }

    /// Removes a file from this archive
    pub fn remove_file(&mut self, file: &str) {
        self.run(None, Action::Remove(Path::new(file)));
    }

    /// Lists all files in an archive
    pub fn files(&self) -> Vec<String> {
        let archive = match ArchiveRO::open(&self.config.dst) {
            Some(ar) => ar,
            None => return Vec::new(),
        };
        let ret = archive.iter().filter_map(|child| child.name())
                         .map(|name| name.to_string())
                         .collect();
        return ret;
    }

    /// Creates an `ArchiveBuilder` for adding files to this archive.
    pub fn extend(self) -> ArchiveBuilder<'a> {
        ArchiveBuilder::new(self)
    }

    fn run(&self, cwd: Option<&Path>, action: Action) -> Output {
        let abs_dst = env::current_dir().unwrap().join(&self.config.dst);
        let ar = &self.config.ar_prog;
        let mut cmd = Command::new(ar);
        cmd.env("PATH", &self.config.command_path);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        self.prepare_ar_action(&mut cmd, &abs_dst, action);
        info!("{:?}", cmd);

        if let Some(p) = cwd {
            cmd.current_dir(p);
            info!("inside {:?}", p.display());
        }

        let handler = &self.config.handler;
        match cmd.spawn() {
            Ok(prog) => {
                let o = prog.wait_with_output().unwrap();
                if !o.status.success() {
                    handler.err(&format!("{:?} failed with: {}", cmd, o.status));
                    handler.note(&format!("stdout ---\n{}",
                                          str::from_utf8(&o.stdout).unwrap()));
                    handler.note(&format!("stderr ---\n{}",
                                          str::from_utf8(&o.stderr).unwrap()));
                    handler.abort_if_errors();
                }
                o
            },
            Err(e) => {
                handler.err(&format!("could not exec `{}`: {}",
                                     self.config.ar_prog, e));
                handler.abort_if_errors();
                panic!("rustc::back::archive::run() should not reach this point");
            }
        }
    }

    fn prepare_ar_action(&self, cmd: &mut Command, dst: &Path, action: Action) {
        match action {
            Action::Remove(file) => {
                cmd.arg("d").arg(dst).arg(file);
            }
            Action::AddObjects(objs, update_symbols) => {
                cmd.arg(if update_symbols {"crs"} else {"crS"})
                   .arg(dst)
                   .args(objs);
            }
            Action::UpdateSymbols => {
                cmd.arg("s").arg(dst);
            }
        }
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
                                    &self.archive.config.slib_prefix,
                                    &self.archive.config.slib_suffix,
                                    &self.archive.config.lib_search_paths,
                                    self.archive.config.handler);
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
        let mut objects = Vec::new();
        let mut total_len = self.archive.config.dst.to_string_lossy().len();

        if self.members.is_empty() {
            if self.should_update_symbols {
                self.archive.run(Some(self.work_dir.path()),
                                 Action::UpdateSymbols);
            }
            return self.archive;
        }

        // Don't allow the total size of `args` to grow beyond 32,000 bytes.
        // Windows will raise an error if the argument string is longer than
        // 32,768, and we leave a bit of extra space for the program name.
        const ARG_LENGTH_LIMIT: usize = 32_000;

        for member_name in &self.members {
            let len = member_name.to_string_lossy().len();

            // `len + 1` to account for the space that's inserted before each
            // argument.  (Windows passes command-line arguments as a single
            // string, not an array of strings.)
            if total_len + len + 1 > ARG_LENGTH_LIMIT {
                // Add the archive members seen so far, without updating the
                // symbol table.
                self.archive.run(Some(self.work_dir.path()),
                                 Action::AddObjects(&objects, false));

                objects.clear();
                total_len = self.archive.config.dst.to_string_lossy().len();
            }

            objects.push(member_name);
            total_len += len + 1;
        }

        // Add the remaining archive members, and update the symbol table if
        // necessary.
        self.archive.run(Some(self.work_dir.path()),
                         Action::AddObjects(&objects, self.should_update_symbols));

        self.archive
    }

    fn add_archive<F>(&mut self, archive: &Path, name: &str,
                      mut skip: F) -> io::Result<()>
        where F: FnMut(&str) -> bool,
    {
        let archive = match ArchiveRO::open(archive) {
            Some(ar) => ar,
            None => return Err(io::Error::new(io::ErrorKind::Other,
                                              "failed to open archive")),
        };

        // Next, we must rename all of the inputs to "guaranteed unique names".
        // We write each file into `self.work_dir` under its new unique name.
        // The reason for this renaming is that archives are keyed off the name
        // of the files, so if two files have the same name they will override
        // one another in the archive (bad).
        //
        // We skip any files explicitly desired for skipping, and we also skip
        // all SYMDEF files as these are just magical placeholders which get
        // re-created when we make a new archive anyway.
        for file in archive.iter() {
            let filename = match file.name() {
                Some(s) => s,
                None => continue,
            };
            if filename.contains(".SYMDEF") { continue }
            if skip(filename) { continue }
            let filename = Path::new(filename).file_name().unwrap()
                                              .to_str().unwrap();

            // Archives on unix systems typically do not have slashes in
            // filenames as the `ar` utility generally only uses the last
            // component of a path for the filename list in the archive. On
            // Windows, however, archives assembled with `lib.exe` will preserve
            // the full path to the file that was placed in the archive,
            // including path separators.
            //
            // The code below is munging paths so it'll go wrong pretty quickly
            // if there's some unexpected slashes in the filename, so here we
            // just chop off everything but the filename component. Note that
            // this can cause duplicate filenames, but that's also handled below
            // as well.
            let filename = Path::new(filename).file_name().unwrap()
                                              .to_str().unwrap();

            // An archive can contain files of the same name multiple times, so
            // we need to be sure to not have them overwrite one another when we
            // extract them. Consequently we need to find a truly unique file
            // name for us!
            let mut new_filename = String::new();
            for n in 0.. {
                let n = if n == 0 {String::new()} else {format!("-{}", n)};
                new_filename = format!("r{}-{}-{}", n, name, filename);

                // LLDB (as mentioned in back::link) crashes on filenames of
                // exactly
                // 16 bytes in length. If we're including an object file with
                //    exactly 16-bytes of characters, give it some prefix so
                //    that it's not 16 bytes.
                new_filename = if new_filename.len() == 16 {
                    format!("lldb-fix-{}", new_filename)
                } else {
                    new_filename
                };

                let present = self.members.iter().filter_map(|p| {
                    p.file_name().and_then(|f| f.to_str())
                }).any(|s| s == new_filename);
                if !present {
                    break
                }
            }
            let dst = self.work_dir.path().join(&new_filename);
            try!(try!(File::create(&dst)).write_all(file.data()));
            self.members.push(PathBuf::from(new_filename));
        }

        Ok(())
    }
}
