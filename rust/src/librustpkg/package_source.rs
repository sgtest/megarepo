// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

extern mod extra;

use target::*;
use package_id::PkgId;
use std::path::Path;
use std::os;
use context::*;
use crate::Crate;
use messages::*;
use source_control::{safe_git_clone, git_clone_url, DirToUse, CheckedOutSources};
use source_control::make_read_only;
use path_util::{find_dir_using_rust_path_hack, make_dir_rwx_recursive};
use path_util::{target_build_dir, versionize};
use util::compile_crate;
use workcache_support;
use workcache_support::crate_tag;
use extra::workcache;

// An enumeration of the unpacked source of a package workspace.
// This contains a list of files found in the source workspace.
#[deriving(Clone)]
pub struct PkgSrc {
    /// Root of where the package source code lives
    source_workspace: Path,
    /// If build_in_destination is true, temporary results should
    /// go in the build/ subdirectory of the destination workspace.
    /// (Otherwise, they go in the build/ subdirectory of the
    /// source workspace.) This happens if the "RUST_PATH hack" is
    /// in effect, or if sources were fetched from a remote
    /// repository.
    build_in_destination: bool,
    /// Where to install the results. May or may not be the same
    /// as source_workspace
    destination_workspace: Path,
    // Directory to start looking in for packages -- normally
    // this is workspace/src/id but it may be just workspace
    start_dir: Path,
    id: PkgId,
    libs: ~[Crate],
    mains: ~[Crate],
    tests: ~[Crate],
    benchs: ~[Crate],
}

pub enum BuildSort { InPlace, Discovered }

impl ToStr for PkgSrc {
    fn to_str(&self) -> ~str {
        format!("Package ID {} in start dir {} [workspaces = {} -> {}]",
                self.id.to_str(),
                self.start_dir.to_str(),
                self.source_workspace.to_str(),
                self.destination_workspace.to_str())
    }
}
condition! {
    // #6009: should this be pub or not, when #8215 is fixed?
    build_err: (~str) -> ~str;
}

impl PkgSrc {

    pub fn new(mut source_workspace: Path,
               destination_workspace: Path,
               use_rust_path_hack: bool,
               id: PkgId) -> PkgSrc {
        use conditions::nonexistent_package::cond;

        debug2!("Checking package source for package ID {}, \
                workspace = {} -> {}, use_rust_path_hack = {:?}",
                id.to_str(),
                source_workspace.to_str(),
                destination_workspace.to_str(),
                use_rust_path_hack);

        let mut destination_workspace = destination_workspace.clone();

        let mut to_try = ~[];
        let mut output_names = ~[];
        let build_dir = target_build_dir(&source_workspace);

        if use_rust_path_hack {
            to_try.push(source_workspace.clone());
        } else {
            // We search for sources under both src/ and build/ , because build/ is where
            // automatically-checked-out sources go.
            let result = source_workspace.push("src").push_rel(&id.path.pop()).push(format!("{}-{}",
                                                         id.short_name, id.version.to_str()));
            to_try.push(result);
            to_try.push(source_workspace.push("src").push_rel(&id.path));

            let result = build_dir.push("src").push_rel(&id.path.pop()).push(format!("{}-{}",
                                                         id.short_name, id.version.to_str()));
            to_try.push(result.clone());
            output_names.push(result);
            let other_result = build_dir.push("src").push_rel(&id.path);
            to_try.push(other_result.clone());
            output_names.push(other_result);

        }

        debug2!("Checking dirs: {:?}", to_try.map(|s| s.to_str()).connect(":"));

        let path = to_try.iter().find(|&d| os::path_exists(d));

        // See the comments on the definition of PkgSrc
        let mut build_in_destination = use_rust_path_hack;
        debug2!("1. build_in_destination = {:?}", build_in_destination);

        let dir: Path = match path {
            Some(d) => (*d).clone(),
            None => {
                // See if any of the prefixes of this package ID form a valid package ID
                // That is, is this a package ID that points into the middle of a workspace?
                for (prefix, suffix) in id.prefixes_iter() {
                    let package_id = PkgId::new(prefix.to_str());
                    let path = build_dir.push_rel(&package_id.path);
                    debug2!("in loop: checking if {} is a directory", path.to_str());
                    if os::path_is_dir(&path) {
                        let ps = PkgSrc::new(source_workspace,
                                             destination_workspace,
                                             use_rust_path_hack,
                                             PkgId::new(prefix.to_str()));
                        match ps {
                            PkgSrc {
                                source_workspace: source,
                                destination_workspace: destination,
                                start_dir: start,
                                id: id, _ } => {
                                let result = PkgSrc {
                                    source_workspace: source.clone(),
                                    build_in_destination: build_in_destination,
                                    destination_workspace: destination,
                                    start_dir: start.push_rel(&suffix),
                                    id: id,
                                    libs: ~[],
                                    mains: ~[],
                                    tests: ~[],
                                    benchs: ~[]
                                };
                                debug2!("pkgsrc: Returning {}", result.to_str());
                                return result;
                            }
                        }

                    };
                }

                // Ok, no prefixes work, so try fetching from git
                let mut ok_d = None;
                for w in output_names.iter() {
                    debug2!("Calling fetch_git on {}", w.to_str());
                    let target_dir_opt = PkgSrc::fetch_git(w, &id);
                    for p in target_dir_opt.iter() {
                        ok_d = Some(p.clone());
                        build_in_destination = true;
                        debug2!("2. build_in_destination = {:?}", build_in_destination);
                        break;
                    }
                    match ok_d {
                        Some(ref d) => {
                            if d.is_parent_of(&id.path)
                                || d.is_parent_of(&versionize(&id.path, &id.version)) {
                                // Strip off the package ID
                                source_workspace = d.clone();
                                for _ in id.path.components().iter() {
                                    source_workspace = source_workspace.pop();
                                }
                                // Strip off the src/ part
                                source_workspace = source_workspace.pop();
                                // Strip off the build/<target-triple> part to get the workspace
                                destination_workspace = source_workspace.pop().pop();
                            }
                            break;
                        }
                        None => ()
                    }
                }
                match ok_d {
                    Some(d) => d,
                    None => {
                        if use_rust_path_hack {
                            match find_dir_using_rust_path_hack(&id) {
                                Some(d) => d,
                                None => {
                                    cond.raise((id.clone(),
                                        ~"supplied path for package dir does not \
                                        exist, and couldn't interpret it as a URL fragment"))
                                }
                            }
                        }
                        else {
                            cond.raise((id.clone(),
                                ~"supplied path for package dir does not \
                                exist, and couldn't interpret it as a URL fragment"))
                        }
                    }
                }
            }
        };
        debug2!("3. build_in_destination = {:?}", build_in_destination);
        debug2!("source: {} dest: {}", source_workspace.to_str(), destination_workspace.to_str());

        debug2!("For package id {}, returning {}", id.to_str(), dir.to_str());

        if !os::path_is_dir(&dir) {
            cond.raise((id.clone(), ~"supplied path for package dir is a \
                                        non-directory"));
        }

        PkgSrc {
            source_workspace: source_workspace.clone(),
            build_in_destination: build_in_destination,
            destination_workspace: destination_workspace,
            start_dir: dir,
            id: id,
            libs: ~[],
            mains: ~[],
            tests: ~[],
            benchs: ~[]
        }
    }

    /// Try interpreting self's package id as a git repository, and try
    /// fetching it and caching it in a local directory. Return the cached directory
    /// if this was successful, None otherwise. Similarly, if the package id
    /// refers to a git repo on the local version, also check it out.
    /// (right now we only support git)
    pub fn fetch_git(local: &Path, pkgid: &PkgId) -> Option<Path> {
        use conditions::git_checkout_failed::cond;

        // We use a temporary directory because if the git clone fails,
        // it creates the target directory anyway and doesn't delete it

        debug2!("Checking whether {} (path = {}) exists locally. Cwd = {}, does it? {:?}",
                pkgid.to_str(), pkgid.path.to_str(),
                os::getcwd().to_str(),
                os::path_exists(&pkgid.path));

        match safe_git_clone(&pkgid.path, &pkgid.version, local) {
            CheckedOutSources => {
                make_read_only(local);
                Some(local.clone())
            }
            DirToUse(clone_target) => {
                if pkgid.path.components().len() < 2 {
                    // If a non-URL, don't bother trying to fetch
                    return None;
                }

                let url = format!("https://{}", pkgid.path.to_str());
                debug2!("Fetching package: git clone {} {} [version={}]",
                        url, clone_target.to_str(), pkgid.version.to_str());

                let mut failed = false;

                do cond.trap(|_| {
                    failed = true;
                }).inside {
                    git_clone_url(url, &clone_target, &pkgid.version);
                };

                if failed {
                    return None;
                }

                // Move clone_target to local.
                // First, create all ancestor directories.
                let moved = make_dir_rwx_recursive(&local.pop())
                    && os::rename_file(&clone_target, local);
                if moved { Some(local.clone()) }
                    else { None }
            }
        }
    }

    // If a file named "pkg.rs" in the start directory exists,
    // return the path for it. Otherwise, None
    pub fn package_script_option(&self) -> Option<Path> {
        let maybe_path = self.start_dir.push("pkg.rs");
        debug2!("package_script_option: checking whether {} exists", maybe_path.to_str());
        if os::path_exists(&maybe_path) {
            Some(maybe_path)
        }
        else {
            None
        }
    }

    /// True if the given path's stem is self's pkg ID's stem
    fn stem_matches(&self, p: &Path) -> bool {
        p.filestem().map_default(false, |p| { p == self.id.short_name.as_slice() })
    }

    pub fn push_crate(cs: &mut ~[Crate], prefix: uint, p: &Path) {
        assert!(p.components.len() > prefix);
        let mut sub = Path("");
        for c in p.components.slice(prefix, p.components.len()).iter() {
            sub = sub.push(*c);
        }
        debug2!("Will compile crate {}", sub.to_str());
        cs.push(Crate::new(&sub));
    }

    /// Infers crates to build. Called only in the case where there
    /// is no custom build logic
    pub fn find_crates(&mut self) {
        self.find_crates_with_filter(|_| true);
    }

    pub fn find_crates_with_filter(&mut self, filter: &fn(&str) -> bool) {
        use conditions::missing_pkg_files::cond;

        let prefix = self.start_dir.components.len();
        debug2!("Matching against {}", self.id.short_name);
        do os::walk_dir(&self.start_dir) |pth| {
            let maybe_known_crate_set = match pth.filename() {
                Some(filename) if filter(filename) => match filename {
                    "lib.rs" => Some(&mut self.libs),
                    "main.rs" => Some(&mut self.mains),
                    "test.rs" => Some(&mut self.tests),
                    "bench.rs" => Some(&mut self.benchs),
                    _ => None
                },
                _ => None
            };

            match maybe_known_crate_set {
                Some(crate_set) => PkgSrc::push_crate(crate_set, prefix, pth),
                None => ()
            }
            true
        };

        let crate_sets = [&self.libs, &self.mains, &self.tests, &self.benchs];
        if crate_sets.iter().all(|crate_set| crate_set.is_empty()) {

            note("Couldn't infer any crates to build.\n\
                         Try naming a crate `main.rs`, `lib.rs`, \
                         `test.rs`, or `bench.rs`.");
            cond.raise(self.id.clone());
        }

        debug2!("In {}, found {} libs, {} mains, {} tests, {} benchs",
               self.start_dir.to_str(),
               self.libs.len(),
               self.mains.len(),
               self.tests.len(),
               self.benchs.len())
    }

    fn build_crates(&self,
                    ctx: &BuildContext,
                    crates: &[Crate],
                    cfgs: &[~str],
                    what: OutputType) {
        for crate in crates.iter() {
            let path = self.start_dir.push_rel(&crate.file).normalize();
            debug2!("build_crates: compiling {}", path.to_str());
            let path_str = path.to_str();
            let cfgs = crate.cfgs + cfgs;

            do ctx.workcache_context.with_prep(crate_tag(&path)) |prep| {
                debug2!("Building crate {}, declaring it as an input", path.to_str());
                prep.declare_input("file", path.to_str(),
                                   workcache_support::digest_file_with_date(&path));
                let subpath = path.clone();
                let subcfgs = cfgs.clone();
                let subpath_str = path_str.clone();
                let subcx = ctx.clone();
                let id = self.id.clone();
                let sub_dir = self.build_workspace().clone();
                let sub_flags = crate.flags.clone();
                do prep.exec |exec| {
                    let result = compile_crate(&subcx,
                                               exec,
                                               &id,
                                               &subpath,
                                               &sub_dir,
                                               sub_flags,
                                               subcfgs,
                                               false,
                                               what).to_str();
                    debug2!("Result of compiling {} was {}", subpath_str, result);
                    result
                }
            };
        }
    }

    /// Declare all the crate files in the package source as inputs
    /// (to the package)
    pub fn declare_inputs(&self, prep: &mut workcache::Prep) {
        let to_do = ~[self.libs.clone(), self.mains.clone(),
                      self.tests.clone(), self.benchs.clone()];
        debug2!("In declare inputs, self = {}", self.to_str());
        for cs in to_do.iter() {
            for c in cs.iter() {
                let path = self.start_dir.push_rel(&c.file).normalize();
                debug2!("Declaring input: {}", path.to_str());
                prep.declare_input("file",
                                   path.to_str(),
                                   workcache_support::digest_file_with_date(&path.clone()));
            }
        }
    }

    // It would be better if build returned a Path, but then Path would have to derive
    // Encodable.
    pub fn build(&self,
                 build_context: &BuildContext,
                 cfgs: ~[~str]) {
        let libs = self.libs.clone();
        let mains = self.mains.clone();
        let tests = self.tests.clone();
        let benchs = self.benchs.clone();
        debug2!("Building libs in {}, destination = {}",
               self.source_workspace.to_str(), self.build_workspace().to_str());
        self.build_crates(build_context, libs, cfgs, Lib);
        debug2!("Building mains");
        self.build_crates(build_context, mains, cfgs, Main);
        debug2!("Building tests");
        self.build_crates(build_context, tests, cfgs, Test);
        debug2!("Building benches");
        self.build_crates(build_context, benchs, cfgs, Bench);
    }

    /// Return the workspace to put temporary files in. See the comment on `PkgSrc`
    pub fn build_workspace<'a>(&'a self) -> &'a Path {
        if self.build_in_destination {
            &self.destination_workspace
        }
        else {
            &self.source_workspace
        }
    }

    /// Debugging
    pub fn dump_crates(&self) {
        let crate_sets = [&self.libs, &self.mains, &self.tests, &self.benchs];
        for crate_set in crate_sets.iter() {
            for c in crate_set.iter() {
                debug2!("Built crate: {}", c.file.to_str())
            }
        }
    }
}
