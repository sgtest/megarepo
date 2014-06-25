// Copyright 2012 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Finds crate binaries and loads their metadata

use back::archive::{ArchiveRO, METADATA_FILENAME};
use back::svh::Svh;
use driver::session::Session;
use lib::llvm::{False, llvm, ObjectFile, mk_section_iter};
use metadata::cstore::{MetadataBlob, MetadataVec, MetadataArchive};
use metadata::decoder;
use metadata::encoder;
use metadata::filesearch::{FileSearch, FileMatches, FileDoesntMatch};
use syntax::abi;
use syntax::codemap::Span;
use syntax::diagnostic::SpanHandler;
use syntax::crateid::CrateId;
use syntax::attr::AttrMetaMethods;
use util::fs;

use std::c_str::ToCStr;
use std::cmp;
use std::io;
use std::mem;
use std::ptr;
use std::slice;
use std::str;

use std::collections::{HashMap, HashSet};
use flate;
use time;

pub static MACOS_DLL_PREFIX: &'static str = "lib";
pub static MACOS_DLL_SUFFIX: &'static str = ".dylib";

pub static WIN32_DLL_PREFIX: &'static str = "";
pub static WIN32_DLL_SUFFIX: &'static str = ".dll";

pub static LINUX_DLL_PREFIX: &'static str = "lib";
pub static LINUX_DLL_SUFFIX: &'static str = ".so";

pub static FREEBSD_DLL_PREFIX: &'static str = "lib";
pub static FREEBSD_DLL_SUFFIX: &'static str = ".so";

pub static ANDROID_DLL_PREFIX: &'static str = "lib";
pub static ANDROID_DLL_SUFFIX: &'static str = ".so";

pub struct CrateMismatch {
    path: Path,
    got: String,
}

pub struct Context<'a> {
    pub sess: &'a Session,
    pub span: Span,
    pub ident: &'a str,
    pub crate_id: &'a CrateId,
    pub id_hash: &'a str,
    pub hash: Option<&'a Svh>,
    pub triple: &'a str,
    pub os: abi::Os,
    pub filesearch: FileSearch<'a>,
    pub root: &'a Option<CratePaths>,
    pub rejected_via_hash: Vec<CrateMismatch>,
    pub rejected_via_triple: Vec<CrateMismatch>,
}

pub struct Library {
    pub dylib: Option<Path>,
    pub rlib: Option<Path>,
    pub metadata: MetadataBlob,
}

pub struct ArchiveMetadata {
    _archive: ArchiveRO,
    // See comments in ArchiveMetadata::new for why this is static
    data: &'static [u8],
}

pub struct CratePaths {
    pub ident: String,
    pub dylib: Option<Path>,
    pub rlib: Option<Path>
}

impl CratePaths {
    fn paths(&self) -> Vec<Path> {
        match (&self.dylib, &self.rlib) {
            (&None,    &None)              => vec!(),
            (&Some(ref p), &None) |
            (&None, &Some(ref p))          => vec!(p.clone()),
            (&Some(ref p1), &Some(ref p2)) => vec!(p1.clone(), p2.clone()),
        }
    }
}

impl<'a> Context<'a> {
    pub fn maybe_load_library_crate(&mut self) -> Option<Library> {
        self.find_library_crate()
    }

    pub fn load_library_crate(&mut self) -> Library {
        match self.find_library_crate() {
            Some(t) => t,
            None => {
                self.report_load_errs();
                unreachable!()
            }
        }
    }

    pub fn report_load_errs(&mut self) {
        let message = if self.rejected_via_hash.len() > 0 {
            format!("found possibly newer version of crate `{}`",
                    self.ident)
        } else if self.rejected_via_triple.len() > 0 {
            format!("found incorrect triple for crate `{}`", self.ident)
        } else {
            format!("can't find crate for `{}`", self.ident)
        };
        let message = match self.root {
            &None => message,
            &Some(ref r) => format!("{} which `{}` depends on",
                                    message, r.ident)
        };
        self.sess.span_err(self.span, message.as_slice());

        let mismatches = self.rejected_via_triple.iter();
        if self.rejected_via_triple.len() > 0 {
            self.sess.span_note(self.span,
                                format!("expected triple of {}",
                                        self.triple).as_slice());
            for (i, &CrateMismatch{ ref path, ref got }) in mismatches.enumerate() {
                self.sess.fileline_note(self.span,
                    format!("crate `{}` path {}{}, triple {}: {}",
                            self.ident, "#", i+1, got, path.display()).as_slice());
            }
        }
        if self.rejected_via_hash.len() > 0 {
            self.sess.span_note(self.span, "perhaps this crate needs \
                                            to be recompiled?");
            let mismatches = self.rejected_via_hash.iter();
            for (i, &CrateMismatch{ ref path, .. }) in mismatches.enumerate() {
                self.sess.fileline_note(self.span,
                    format!("crate `{}` path {}{}: {}",
                            self.ident, "#", i+1, path.display()).as_slice());
            }
            match self.root {
                &None => {}
                &Some(ref r) => {
                    for (i, path) in r.paths().iter().enumerate() {
                        self.sess.fileline_note(self.span,
                            format!("crate `{}` path #{}: {}",
                                    r.ident, i+1, path.display()).as_slice());
                    }
                }
            }
        }
        self.sess.abort_if_errors();
    }

    fn find_library_crate(&mut self) -> Option<Library> {
        let dypair = self.dylibname();

        // want: crate_name.dir_part() + prefix + crate_name.file_part + "-"
        let dylib_prefix = dypair.map(|(prefix, _)| {
            format!("{}{}-", prefix, self.crate_id.name)
        });
        let rlib_prefix = format!("lib{}-", self.crate_id.name);

        let mut candidates = HashMap::new();

        // First, find all possible candidate rlibs and dylibs purely based on
        // the name of the files themselves. We're trying to match against an
        // exact crate_id and a possibly an exact hash.
        //
        // During this step, we can filter all found libraries based on the
        // name and id found in the crate id (we ignore the path portion for
        // filename matching), as well as the exact hash (if specified). If we
        // end up having many candidates, we must look at the metadata to
        // perform exact matches against hashes/crate ids. Note that opening up
        // the metadata is where we do an exact match against the full contents
        // of the crate id (path/name/id).
        //
        // The goal of this step is to look at as little metadata as possible.
        self.filesearch.search(|path| {
            let file = match path.filename_str() {
                None => return FileDoesntMatch,
                Some(file) => file,
            };
            if file.starts_with(rlib_prefix.as_slice()) &&
                    file.ends_with(".rlib") {
                info!("rlib candidate: {}", path.display());
                match self.try_match(file, rlib_prefix.as_slice(), ".rlib") {
                    Some(hash) => {
                        info!("rlib accepted, hash: {}", hash);
                        let slot = candidates.find_or_insert_with(hash, |_| {
                            (HashSet::new(), HashSet::new())
                        });
                        let (ref mut rlibs, _) = *slot;
                        rlibs.insert(fs::realpath(path).unwrap());
                        FileMatches
                    }
                    None => {
                        info!("rlib rejected");
                        FileDoesntMatch
                    }
                }
            } else if dypair.map_or(false, |(_, suffix)| {
                file.starts_with(dylib_prefix.get_ref().as_slice()) &&
                file.ends_with(suffix)
            }) {
                let (_, suffix) = dypair.unwrap();
                let dylib_prefix = dylib_prefix.get_ref().as_slice();
                info!("dylib candidate: {}", path.display());
                match self.try_match(file, dylib_prefix, suffix) {
                    Some(hash) => {
                        info!("dylib accepted, hash: {}", hash);
                        let slot = candidates.find_or_insert_with(hash, |_| {
                            (HashSet::new(), HashSet::new())
                        });
                        let (_, ref mut dylibs) = *slot;
                        dylibs.insert(fs::realpath(path).unwrap());
                        FileMatches
                    }
                    None => {
                        info!("dylib rejected");
                        FileDoesntMatch
                    }
                }
            } else {
                FileDoesntMatch
            }
        });

        // We have now collected all known libraries into a set of candidates
        // keyed of the filename hash listed. For each filename, we also have a
        // list of rlibs/dylibs that apply. Here, we map each of these lists
        // (per hash), to a Library candidate for returning.
        //
        // A Library candidate is created if the metadata for the set of
        // libraries corresponds to the crate id and hash criteria that this
        // search is being performed for.
        let mut libraries = Vec::new();
        for (_hash, (rlibs, dylibs)) in candidates.move_iter() {
            let mut metadata = None;
            let rlib = self.extract_one(rlibs, "rlib", &mut metadata);
            let dylib = self.extract_one(dylibs, "dylib", &mut metadata);
            match metadata {
                Some(metadata) => {
                    libraries.push(Library {
                        dylib: dylib,
                        rlib: rlib,
                        metadata: metadata,
                    })
                }
                None => {}
            }
        }

        // Having now translated all relevant found hashes into libraries, see
        // what we've got and figure out if we found multiple candidates for
        // libraries or not.
        match libraries.len() {
            0 => None,
            1 => Some(libraries.move_iter().next().unwrap()),
            _ => {
                self.sess.span_err(self.span,
                    format!("multiple matching crates for `{}`",
                            self.crate_id.name).as_slice());
                self.sess.note("candidates:");
                for lib in libraries.iter() {
                    match lib.dylib {
                        Some(ref p) => {
                            self.sess.note(format!("path: {}",
                                                   p.display()).as_slice());
                        }
                        None => {}
                    }
                    match lib.rlib {
                        Some(ref p) => {
                            self.sess.note(format!("path: {}",
                                                   p.display()).as_slice());
                        }
                        None => {}
                    }
                    let data = lib.metadata.as_slice();
                    let crate_id = decoder::get_crate_id(data);
                    note_crateid_attr(self.sess.diagnostic(), &crate_id);
                }
                None
            }
        }
    }

    // Attempts to match the requested version of a library against the file
    // specified. The prefix/suffix are specified (disambiguates between
    // rlib/dylib).
    //
    // The return value is `None` if `file` doesn't look like a rust-generated
    // library, or if a specific version was requested and it doesn't match the
    // apparent file's version.
    //
    // If everything checks out, then `Some(hash)` is returned where `hash` is
    // the listed hash in the filename itself.
    fn try_match(&self, file: &str, prefix: &str, suffix: &str) -> Option<String>{
        let middle = file.slice(prefix.len(), file.len() - suffix.len());
        debug!("matching -- {}, middle: {}", file, middle);
        let mut parts = middle.splitn('-', 1);
        let hash = match parts.next() { Some(h) => h, None => return None };
        debug!("matching -- {}, hash: {} (want {})", file, hash, self.id_hash);
        let vers = match parts.next() { Some(v) => v, None => return None };
        debug!("matching -- {}, vers: {} (want {})", file, vers,
               self.crate_id.version);
        match self.crate_id.version {
            Some(ref version) if version.as_slice() != vers => return None,
            Some(..) => {} // check the hash

            // hash is irrelevant, no version specified
            None => return Some(hash.to_string())
        }
        debug!("matching -- {}, vers ok", file);
        // hashes in filenames are prefixes of the "true hash"
        if self.id_hash == hash.as_slice() {
            debug!("matching -- {}, hash ok", file);
            Some(hash.to_string())
        } else {
            None
        }
    }

    // Attempts to extract *one* library from the set `m`. If the set has no
    // elements, `None` is returned. If the set has more than one element, then
    // the errors and notes are emitted about the set of libraries.
    //
    // With only one library in the set, this function will extract it, and then
    // read the metadata from it if `*slot` is `None`. If the metadata couldn't
    // be read, it is assumed that the file isn't a valid rust library (no
    // errors are emitted).
    fn extract_one(&mut self, m: HashSet<Path>, flavor: &str,
                   slot: &mut Option<MetadataBlob>) -> Option<Path> {
        let mut ret = None::<Path>;
        let mut error = 0u;

        if slot.is_some() {
            // FIXME(#10786): for an optimization, we only read one of the
            //                library's metadata sections. In theory we should
            //                read both, but reading dylib metadata is quite
            //                slow.
            if m.len() == 0 {
                return None
            } else if m.len() == 1 {
                return Some(m.move_iter().next().unwrap())
            }
        }

        for lib in m.move_iter() {
            info!("{} reading metadata from: {}", flavor, lib.display());
            let metadata = match get_metadata_section(self.os, &lib) {
                Ok(blob) => {
                    if self.crate_matches(blob.as_slice(), &lib) {
                        blob
                    } else {
                        info!("metadata mismatch");
                        continue
                    }
                }
                Err(_) => {
                    info!("no metadata found");
                    continue
                }
            };
            if ret.is_some() {
                self.sess.span_err(self.span,
                                   format!("multiple {} candidates for `{}` \
                                            found",
                                           flavor,
                                           self.crate_id.name).as_slice());
                self.sess.span_note(self.span,
                                    format!(r"candidate #1: {}",
                                            ret.get_ref()
                                               .display()).as_slice());
                error = 1;
                ret = None;
            }
            if error > 0 {
                error += 1;
                self.sess.span_note(self.span,
                                    format!(r"candidate #{}: {}", error,
                                            lib.display()).as_slice());
                continue
            }
            *slot = Some(metadata);
            ret = Some(lib);
        }
        return if error > 0 {None} else {ret}
    }

    fn crate_matches(&mut self, crate_data: &[u8], libpath: &Path) -> bool {
        match decoder::maybe_get_crate_id(crate_data) {
            Some(ref id) if self.crate_id.matches(id) => {}
            _ => { info!("Rejecting via crate_id"); return false }
        }
        let hash = match decoder::maybe_get_crate_hash(crate_data) {
            Some(hash) => hash, None => {
                info!("Rejecting via lack of crate hash");
                return false;
            }
        };

        let triple = decoder::get_crate_triple(crate_data);
        if triple.as_slice() != self.triple {
            info!("Rejecting via crate triple: expected {} got {}", self.triple, triple);
            self.rejected_via_triple.push(CrateMismatch {
                path: libpath.clone(),
                got: triple.to_string()
            });
            return false;
        }

        match self.hash {
            None => true,
            Some(myhash) => {
                if *myhash != hash {
                    info!("Rejecting via hash: expected {} got {}", *myhash, hash);
                    self.rejected_via_hash.push(CrateMismatch {
                        path: libpath.clone(),
                        got: myhash.as_str().to_string()
                    });
                    false
                } else {
                    true
                }
            }
        }
    }


    // Returns the corresponding (prefix, suffix) that files need to have for
    // dynamic libraries
    fn dylibname(&self) -> Option<(&'static str, &'static str)> {
        match self.os {
            abi::OsWin32 => Some((WIN32_DLL_PREFIX, WIN32_DLL_SUFFIX)),
            abi::OsMacos => Some((MACOS_DLL_PREFIX, MACOS_DLL_SUFFIX)),
            abi::OsLinux => Some((LINUX_DLL_PREFIX, LINUX_DLL_SUFFIX)),
            abi::OsAndroid => Some((ANDROID_DLL_PREFIX, ANDROID_DLL_SUFFIX)),
            abi::OsFreebsd => Some((FREEBSD_DLL_PREFIX, FREEBSD_DLL_SUFFIX)),
            abi::OsiOS => None,
        }
    }

}

pub fn note_crateid_attr(diag: &SpanHandler, crateid: &CrateId) {
    diag.handler().note(format!("crate_id: {}", crateid.to_str()).as_slice());
}

impl ArchiveMetadata {
    fn new(ar: ArchiveRO) -> Option<ArchiveMetadata> {
        let data: &'static [u8] = {
            let data = match ar.read(METADATA_FILENAME) {
                Some(data) => data,
                None => {
                    debug!("didn't find '{}' in the archive", METADATA_FILENAME);
                    return None;
                }
            };
            // This data is actually a pointer inside of the archive itself, but
            // we essentially want to cache it because the lookup inside the
            // archive is a fairly expensive operation (and it's queried for
            // *very* frequently). For this reason, we transmute it to the
            // static lifetime to put into the struct. Note that the buffer is
            // never actually handed out with a static lifetime, but rather the
            // buffer is loaned with the lifetime of this containing object.
            // Hence, we're guaranteed that the buffer will never be used after
            // this object is dead, so this is a safe operation to transmute and
            // store the data as a static buffer.
            unsafe { mem::transmute(data) }
        };
        Some(ArchiveMetadata {
            _archive: ar,
            data: data,
        })
    }

    pub fn as_slice<'a>(&'a self) -> &'a [u8] { self.data }
}

// Just a small wrapper to time how long reading metadata takes.
fn get_metadata_section(os: abi::Os, filename: &Path) -> Result<MetadataBlob, String> {
    let start = time::precise_time_ns();
    let ret = get_metadata_section_imp(os, filename);
    info!("reading {} => {}ms", filename.filename_display(),
           (time::precise_time_ns() - start) / 1000000);
    return ret;
}

fn get_metadata_section_imp(os: abi::Os, filename: &Path) -> Result<MetadataBlob, String> {
    if !filename.exists() {
        return Err(format!("no such file: '{}'", filename.display()));
    }
    if filename.filename_str().unwrap().ends_with(".rlib") {
        // Use ArchiveRO for speed here, it's backed by LLVM and uses mmap
        // internally to read the file. We also avoid even using a memcpy by
        // just keeping the archive along while the metadata is in use.
        let archive = match ArchiveRO::open(filename) {
            Some(ar) => ar,
            None => {
                debug!("llvm didn't like `{}`", filename.display());
                return Err(format!("failed to read rlib metadata: '{}'",
                                   filename.display()));
            }
        };
        return match ArchiveMetadata::new(archive).map(|ar| MetadataArchive(ar)) {
            None => {
                return Err((format!("failed to read rlib metadata: '{}'",
                                    filename.display())))
            }
            Some(blob) => return Ok(blob)
        }
    }
    unsafe {
        let mb = filename.with_c_str(|buf| {
            llvm::LLVMRustCreateMemoryBufferWithContentsOfFile(buf)
        });
        if mb as int == 0 {
            return Err(format!("error reading library: '{}'",
                               filename.display()))
        }
        let of = match ObjectFile::new(mb) {
            Some(of) => of,
            _ => {
                return Err((format!("provided path not an object file: '{}'",
                                    filename.display())))
            }
        };
        let si = mk_section_iter(of.llof);
        while llvm::LLVMIsSectionIteratorAtEnd(of.llof, si.llsi) == False {
            let mut name_buf = ptr::null();
            let name_len = llvm::LLVMRustGetSectionName(si.llsi, &mut name_buf);
            let name = str::raw::from_buf_len(name_buf as *u8, name_len as uint);
            debug!("get_metadata_section: name {}", name);
            if read_meta_section_name(os).as_slice() == name.as_slice() {
                let cbuf = llvm::LLVMGetSectionContents(si.llsi);
                let csz = llvm::LLVMGetSectionSize(si.llsi) as uint;
                let mut found =
                    Err(format!("metadata not found: '{}'", filename.display()));
                let cvbuf: *u8 = mem::transmute(cbuf);
                let vlen = encoder::metadata_encoding_version.len();
                debug!("checking {} bytes of metadata-version stamp",
                       vlen);
                let minsz = cmp::min(vlen, csz);
                let version_ok = slice::raw::buf_as_slice(cvbuf, minsz,
                    |buf0| buf0 == encoder::metadata_encoding_version);
                if !version_ok {
                    return Err((format!("incompatible metadata version found: '{}'",
                                        filename.display())));
                }

                let cvbuf1 = cvbuf.offset(vlen as int);
                debug!("inflating {} bytes of compressed metadata",
                       csz - vlen);
                slice::raw::buf_as_slice(cvbuf1, csz-vlen, |bytes| {
                    match flate::inflate_bytes(bytes) {
                        Some(inflated) => found = Ok(MetadataVec(inflated)),
                        None => {
                            found =
                                Err(format!("failed to decompress \
                                             metadata for: '{}'",
                                            filename.display()))
                        }
                    }
                });
                if found.is_ok() {
                    return found;
                }
            }
            llvm::LLVMMoveToNextSection(si.llsi);
        }
        return Err(format!("metadata not found: '{}'", filename.display()));
    }
}

pub fn meta_section_name(os: abi::Os) -> Option<&'static str> {
    match os {
        abi::OsMacos => Some("__DATA,__note.rustc"),
        abi::OsiOS => Some("__DATA,__note.rustc"),
        abi::OsWin32 => Some(".note.rustc"),
        abi::OsLinux => Some(".note.rustc"),
        abi::OsAndroid => Some(".note.rustc"),
        abi::OsFreebsd => Some(".note.rustc")
    }
}

pub fn read_meta_section_name(os: abi::Os) -> &'static str {
    match os {
        abi::OsMacos => "__note.rustc",
        abi::OsiOS => unreachable!(),
        abi::OsWin32 => ".note.rustc",
        abi::OsLinux => ".note.rustc",
        abi::OsAndroid => ".note.rustc",
        abi::OsFreebsd => ".note.rustc"
    }
}

// A diagnostic function for dumping crate metadata to an output stream
pub fn list_file_metadata(os: abi::Os, path: &Path,
                          out: &mut io::Writer) -> io::IoResult<()> {
    match get_metadata_section(os, path) {
        Ok(bytes) => decoder::list_crate_metadata(bytes.as_slice(), out),
        Err(msg) => {
            write!(out, "{}\n", msg)
        }
    }
}
