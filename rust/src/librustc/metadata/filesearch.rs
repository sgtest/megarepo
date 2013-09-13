// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use std::option;
use std::os;
use std::hashmap::HashSet;

pub enum FileMatch { FileMatches, FileDoesntMatch }

// A module for searching for libraries
// FIXME (#2658): I'm not happy how this module turned out. Should
// probably just be folded into cstore.

/// Functions with type `pick` take a parent directory as well as
/// a file found in that directory.
pub type pick<'self> = &'self fn(path: &Path) -> FileMatch;

pub fn pick_file(file: Path, path: &Path) -> Option<Path> {
    if path.file_path() == file {
        option::Some((*path).clone())
    } else {
        option::None
    }
}

pub trait FileSearch {
    fn sysroot(&self) -> @Path;
    fn for_each_lib_search_path(&self, f: &fn(&Path) -> FileMatch);
    fn get_target_lib_path(&self) -> Path;
    fn get_target_lib_file_path(&self, file: &Path) -> Path;
}

pub fn mk_filesearch(maybe_sysroot: &Option<@Path>,
                     target_triple: &str,
                     addl_lib_search_paths: @mut ~[Path])
                  -> @FileSearch {
    struct FileSearchImpl {
        sysroot: @Path,
        addl_lib_search_paths: @mut ~[Path],
        target_triple: ~str
    }
    impl FileSearch for FileSearchImpl {
        fn sysroot(&self) -> @Path { self.sysroot }
        fn for_each_lib_search_path(&self, f: &fn(&Path) -> FileMatch) {
            let mut visited_dirs = HashSet::new();
            let mut found = false;

            debug!("filesearch: searching additional lib search paths [%?]",
                   self.addl_lib_search_paths.len());
            for path in self.addl_lib_search_paths.iter() {
                match f(path) {
                    FileMatches => found = true,
                    FileDoesntMatch => ()
                }
                visited_dirs.insert(path.to_str());
            }

            debug!("filesearch: searching target lib path");
            let tlib_path = make_target_lib_path(self.sysroot,
                                        self.target_triple);
            if !visited_dirs.contains(&tlib_path.to_str()) {
                match f(&tlib_path) {
                    FileMatches => found = true,
                    FileDoesntMatch => ()
                }
            }
            visited_dirs.insert(tlib_path.to_str());
            // Try RUST_PATH
            if !found {
                let rustpath = rust_path();
                for path in rustpath.iter() {
                    let tlib_path = make_rustpkg_target_lib_path(path, self.target_triple);
                    debug!("is %s in visited_dirs? %?", tlib_path.to_str(),
                            visited_dirs.contains(&tlib_path.to_str()));

                    if !visited_dirs.contains(&tlib_path.to_str()) {
                        visited_dirs.insert(tlib_path.to_str());
                        // Don't keep searching the RUST_PATH if one match turns up --
                        // if we did, we'd get a "multiple matching crates" error
                        match f(&tlib_path) {
                           FileMatches => {
                               break;
                           }
                           FileDoesntMatch => ()
                        }
                    }
                }
            }
        }
        fn get_target_lib_path(&self) -> Path {
            make_target_lib_path(self.sysroot, self.target_triple)
        }
        fn get_target_lib_file_path(&self, file: &Path) -> Path {
            self.get_target_lib_path().push_rel(file)
        }
    }

    let sysroot = get_sysroot(maybe_sysroot);
    debug!("using sysroot = %s", sysroot.to_str());
    @FileSearchImpl {
        sysroot: sysroot,
        addl_lib_search_paths: addl_lib_search_paths,
        target_triple: target_triple.to_owned()
    } as @FileSearch
}

pub fn search(filesearch: @FileSearch, pick: pick) {
    do filesearch.for_each_lib_search_path() |lib_search_path| {
        debug!("searching %s", lib_search_path.to_str());
        let r = os::list_dir_path(lib_search_path);
        let mut rslt = FileDoesntMatch;
        for path in r.iter() {
            debug!("testing %s", path.to_str());
            let maybe_picked = pick(path);
            match maybe_picked {
                FileMatches => {
                    debug!("picked %s", path.to_str());
                    rslt = FileMatches;
                }
                FileDoesntMatch => {
                    debug!("rejected %s", path.to_str());
                }
            }
        }
        rslt
    };
}

pub fn relative_target_lib_path(target_triple: &str) -> Path {
    Path(libdir()).push_many([~"rustc",
                              target_triple.to_owned(),
                              libdir()])
}

fn make_target_lib_path(sysroot: &Path,
                        target_triple: &str) -> Path {
    sysroot.push_rel(&relative_target_lib_path(target_triple))
}

fn make_rustpkg_target_lib_path(dir: &Path,
                        target_triple: &str) -> Path {
    dir.push_rel(&Path(libdir()).push(target_triple.to_owned()))
}

fn get_or_default_sysroot() -> Path {
    match os::self_exe_path() {
      option::Some(ref p) => (*p).pop(),
      option::None => fail!("can't determine value for sysroot")
    }
}

fn get_sysroot(maybe_sysroot: &Option<@Path>) -> @Path {
    match *maybe_sysroot {
      option::Some(sr) => sr,
      option::None => @get_or_default_sysroot()
    }
}

#[cfg(windows)]
static PATH_ENTRY_SEPARATOR: &'static str = ";";
#[cfg(not(windows))]
static PATH_ENTRY_SEPARATOR: &'static str = ":";

/// Returns RUST_PATH as a string, without default paths added
pub fn get_rust_path() -> Option<~str> {
    os::getenv("RUST_PATH")
}

/// Returns the value of RUST_PATH, as a list
/// of Paths. Includes default entries for, if they exist:
/// $HOME/.rust
/// DIR/.rust for any DIR that's the current working directory
/// or an ancestor of it
pub fn rust_path() -> ~[Path] {
    let mut env_rust_path: ~[Path] = match get_rust_path() {
        Some(env_path) => {
            let env_path_components: ~[&str] =
                env_path.split_str_iter(PATH_ENTRY_SEPARATOR).collect();
            env_path_components.map(|&s| Path(s))
        }
        None => ~[]
    };
    let cwd = os::getcwd();
    // now add in default entries
    let cwd_dot_rust = cwd.push(".rust");
    if !env_rust_path.contains(&cwd_dot_rust) {
        env_rust_path.push(cwd_dot_rust);
    }
    if !env_rust_path.contains(&cwd) {
        env_rust_path.push(cwd.clone());
    }
    do cwd.each_parent() |p| {
        if !env_rust_path.contains(&p.push(".rust")) {
            push_if_exists(&mut env_rust_path, p);
        }
    }
    let h = os::homedir();
    for h in h.iter() {
        if !env_rust_path.contains(&h.push(".rust")) {
            push_if_exists(&mut env_rust_path, h);
        }
    }
    env_rust_path
}


/// Adds p/.rust into vec, only if it exists
fn push_if_exists(vec: &mut ~[Path], p: &Path) {
    let maybe_dir = p.push(".rust");
    if os::path_exists(&maybe_dir) {
        vec.push(maybe_dir);
    }
}

// The name of the directory rustc expects libraries to be located.
// On Unix should be "lib", on windows "bin"
pub fn libdir() -> ~str {
    (env!("CFG_LIBDIR")).to_owned()
}
