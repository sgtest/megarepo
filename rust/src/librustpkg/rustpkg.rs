// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// rustpkg - a package manager and build system for Rust

#[link(name = "rustpkg",
       vers = "0.8-pre",
       uuid = "25de5e6e-279e-4a20-845c-4cabae92daaf",
       url = "https://github.com/mozilla/rust/tree/master/src/librustpkg")];

#[license = "MIT/ASL2"];
#[crate_type = "lib"];

extern mod extra;
extern mod rustc;
extern mod syntax;

use std::{io, os, result, run, str};
pub use std::path::Path;
use std::hashmap::HashMap;

use rustc::driver::{driver, session};
use rustc::metadata::filesearch;
use rustc::metadata::filesearch::rust_path;
use extra::{getopts};
use syntax::{ast, diagnostic};
use util::*;
use messages::*;
use path_util::{build_pkg_id_in_workspace, first_pkgid_src_in_workspace};
use path_util::{U_RWX, in_rust_path};
use path_util::{built_executable_in_workspace, built_library_in_workspace, default_workspace};
use path_util::{target_executable_in_workspace, target_library_in_workspace};
use source_control::is_git_dir;
use workspace::{each_pkg_parent_workspace, pkg_parent_workspaces, cwd_to_workspace};
use context::Ctx;
use package_id::PkgId;
use package_source::PkgSrc;

pub mod api;
mod conditions;
mod context;
mod crate;
mod installed_packages;
mod messages;
mod package_id;
mod package_source;
mod path_util;
mod search;
mod source_control;
mod target;
#[cfg(test)]
mod tests;
mod util;
mod version;
mod workspace;

pub mod usage;

/// A PkgScript represents user-supplied custom logic for
/// special build hooks. This only exists for packages with
/// an explicit package script.
struct PkgScript<'self> {
    /// Uniquely identifies this package
    id: &'self PkgId,
    // Used to have this field:    deps: ~[(~str, Option<~str>)]
    // but I think it shouldn't be stored here
    /// The contents of the package script: either a file path,
    /// or a string containing the text of the input
    input: driver::input,
    /// The session to use *only* for compiling the custom
    /// build script
    sess: session::Session,
    /// The config for compiling the custom build script
    cfg: ast::CrateConfig,
    /// The crate for the custom build script
    crate: @ast::Crate,
    /// Directory in which to store build output
    build_dir: Path
}

impl<'self> PkgScript<'self> {
    /// Given the path name for a package script
    /// and a package ID, parse the package script into
    /// a PkgScript that we can then execute
    fn parse<'a>(sysroot: @Path,
                 script: Path,
                 workspace: &Path,
                 id: &'a PkgId) -> PkgScript<'a> {
        // Get the executable name that was invoked
        let binary = os::args()[0].to_managed();
        // Build the rustc session data structures to pass
        // to the compiler
        debug!("pkgscript parse: %s", sysroot.to_str());
        let options = @session::options {
            binary: binary,
            maybe_sysroot: Some(sysroot),
            crate_type: session::bin_crate,
            .. (*session::basic_options()).clone()
        };
        let input = driver::file_input(script);
        let sess = driver::build_session(options, diagnostic::emit);
        let cfg = driver::build_configuration(sess);
        let crate = driver::phase_1_parse_input(sess, cfg.clone(), &input);
        let crate = driver::phase_2_configure_and_expand(sess, cfg.clone(), crate);
        let work_dir = build_pkg_id_in_workspace(id, workspace);

        debug!("Returning package script with id %s", id.to_str());

        PkgScript {
            id: id,
            input: input,
            sess: sess,
            cfg: cfg,
            crate: crate,
            build_dir: work_dir
        }
    }

    /// Run the contents of this package script, where <what>
    /// is the command to pass to it (e.g., "build", "clean", "install")
    /// Returns a pair of an exit code and list of configs (obtained by
    /// calling the package script's configs() function if it exists
    // FIXME (#4432): Use workcache to only compile the script when changed
    fn run_custom(&self, sysroot: @Path) -> (~[~str], ExitCode) {
        let sess = self.sess;

        debug!("Working directory = %s", self.build_dir.to_str());
        // Collect together any user-defined commands in the package script
        let crate = util::ready_crate(sess, self.crate);
        debug!("Building output filenames with script name %s",
               driver::source_name(&self.input));
        let exe = self.build_dir.push(~"pkg" + util::exe_suffix());
        util::compile_crate_from_input(&self.input,
                                       &self.build_dir,
                                       sess,
                                       crate);
        debug!("Running program: %s %s %s", exe.to_str(),
               sysroot.to_str(), "install");
        // FIXME #7401 should support commands besides `install`
        let status = run::process_status(exe.to_str(), [sysroot.to_str(), ~"install"]);
        if status != 0 {
            return (~[], status);
        }
        else {
            debug!("Running program (configs): %s %s %s",
                   exe.to_str(), sysroot.to_str(), "configs");
            let output = run::process_output(exe.to_str(), [sysroot.to_str(), ~"configs"]);
            // Run the configs() function to get the configs
            let cfgs = str::from_utf8_slice(output.output).word_iter()
                .map(|w| w.to_owned()).collect();
            (cfgs, output.status)
        }
    }

    fn hash(&self) -> ~str {
        self.id.hash()
    }

}

pub trait CtxMethods {
    fn run(&self, cmd: &str, args: ~[~str]);
    fn do_cmd(&self, _cmd: &str, _pkgname: &str);
    /// Returns the destination workspace
    fn build(&self, workspace: &Path, pkgid: &PkgId) -> Path;
    fn clean(&self, workspace: &Path, id: &PkgId);
    fn info(&self);
    fn install(&self, workspace: &Path, id: &PkgId);
    fn install_no_build(&self, workspace: &Path, id: &PkgId);
    fn prefer(&self, _id: &str, _vers: Option<~str>);
    fn test(&self);
    fn uninstall(&self, _id: &str, _vers: Option<~str>);
    fn unprefer(&self, _id: &str, _vers: Option<~str>);
}

impl CtxMethods for Ctx {

    fn run(&self, cmd: &str, args: ~[~str]) {
        match cmd {
            "build" => {
                if args.len() < 1 {
                    match cwd_to_workspace() {
                        None if self.use_rust_path_hack => {
                            let cwd = os::getcwd();
                            self.build(&cwd, &PkgId::new(cwd.components[cwd.components.len() - 1]));
                        }
                        None => { usage::build(); return; }
                        Some((ws, pkgid)) => { self.build(&ws, &pkgid); }
                    }
                }
                else {
                    // The package id is presumed to be the first command-line
                    // argument
                    let pkgid = PkgId::new(args[0].clone());
                    do each_pkg_parent_workspace(self, &pkgid) |workspace| {
                        debug!("found pkg %s in workspace %s, trying to build",
                               pkgid.to_str(), workspace.to_str());
                        self.build(workspace, &pkgid);
                        true
                    };
                }
            }
            "clean" => {
                if args.len() < 1 {
                    match cwd_to_workspace() {
                        None => { usage::clean(); return }
                        // tjc: Maybe clean should clean all the packages in the
                        // current workspace, though?
                        Some((ws, pkgid)) => self.clean(&ws, &pkgid)
                    }

                }
                else {
                    // The package id is presumed to be the first command-line
                    // argument
                    let pkgid = PkgId::new(args[0].clone());
                    let cwd = os::getcwd();
                    self.clean(&cwd, &pkgid); // tjc: should use workspace, not cwd
                }
            }
            "do" => {
                if args.len() < 2 {
                    return usage::do_cmd();
                }

                self.do_cmd(args[0].clone(), args[1].clone());
            }
            "info" => {
                self.info();
            }
            "install" => {
                if args.len() < 1 {
                    match cwd_to_workspace() {
                        None if self.use_rust_path_hack => {
                            let cwd = os::getcwd();
                            self.install(&cwd,
                                &PkgId::new(cwd.components[cwd.components.len() - 1]));
                        }
                        None  => { usage::install(); return; }
                        Some((ws, pkgid))                => self.install(&ws, &pkgid),
                     }
                }
                else {
                    // The package id is presumed to be the first command-line
                    // argument
                    let pkgid = PkgId::new(args[0]);
                    let workspaces = pkg_parent_workspaces(self, &pkgid);
                    debug!("package ID = %s, found it in %? workspaces",
                           pkgid.to_str(), workspaces.len());
                    if workspaces.is_empty() {
                        let rp = rust_path();
                        assert!(!rp.is_empty());
                        let src = PkgSrc::new(&rp[0], &pkgid);
                        src.fetch_git();
                        self.install(&rp[0], &pkgid);
                    }
                    else {
                        do each_pkg_parent_workspace(self, &pkgid) |workspace| {
                            self.install(workspace, &pkgid);
                            true
                        };
                    }
                }
            }
            "list" => {
                io::println("Installed packages:");
                do installed_packages::list_installed_packages |pkg_id| {
                    println(pkg_id.path.to_str());
                    true
                };
            }
            "prefer" => {
                if args.len() < 1 {
                    return usage::uninstall();
                }

                self.prefer(args[0], None);
            }
            "test" => {
                self.test();
            }
            "uninstall" => {
                if args.len() < 1 {
                    return usage::uninstall();
                }

                let pkgid = PkgId::new(args[0]);
                if !installed_packages::package_is_installed(&pkgid) {
                    warn(fmt!("Package %s doesn't seem to be installed! Doing nothing.", args[0]));
                    return;
                }
                else {
                    let rp = rust_path();
                    assert!(!rp.is_empty());
                    do each_pkg_parent_workspace(self, &pkgid) |workspace| {
                        path_util::uninstall_package_from(workspace, &pkgid);
                        note(fmt!("Uninstalled package %s (was installed in %s)",
                                  pkgid.to_str(), workspace.to_str()));
                        true
                    };
                }
            }
            "unprefer" => {
                if args.len() < 1 {
                    return usage::unprefer();
                }

                self.unprefer(args[0], None);
            }
            _ => fail!(fmt!("I don't know the command `%s`", cmd))
        }
    }

    fn do_cmd(&self, _cmd: &str, _pkgname: &str)  {
        // stub
        fail!("`do` not yet implemented");
    }

    /// Returns the destination workspace
    /// In the case of a custom build, we don't know, so we just return the source workspace
    fn build(&self, workspace: &Path, pkgid: &PkgId) -> Path {
        debug!("build: workspace = %s (in Rust path? %? is git dir? %? \
                pkgid = %s", workspace.to_str(),
               in_rust_path(workspace), is_git_dir(&workspace.push_rel(&pkgid.path)),
               pkgid.to_str());
        let src_dir   = first_pkgid_src_in_workspace(pkgid, workspace);

        // If workspace isn't in the RUST_PATH, and it's a git repo,
        // then clone it into the first entry in RUST_PATH, and repeat
        debug!("%? %? %s", in_rust_path(workspace),
               is_git_dir(&workspace.push_rel(&pkgid.path)),
               workspace.to_str());
        if !in_rust_path(workspace) && is_git_dir(&workspace.push_rel(&pkgid.path)) {
            let out_dir = default_workspace().push("src").push_rel(&pkgid.path);
            source_control::git_clone(&workspace.push_rel(&pkgid.path),
                                      &out_dir, &pkgid.version);
            let default_ws = default_workspace();
            debug!("Calling build recursively with %? and %?", default_ws.to_str(),
                   pkgid.to_str());
            return self.build(&default_ws, pkgid);
        }

        // Create the package source
        let mut src = PkgSrc::new(workspace, pkgid);
        debug!("Package src = %?", src);

        // Is there custom build logic? If so, use it
        let pkg_src_dir = src_dir;
        let mut custom = false;
        debug!("Package source directory = %?", pkg_src_dir);
        let cfgs = match pkg_src_dir.chain_ref(|p| src.package_script_option(p)) {
            Some(package_script_path) => {
                let sysroot = self.sysroot_to_use().expect("custom build needs a sysroot");
                let pscript = PkgScript::parse(sysroot,
                                               package_script_path,
                                               workspace,
                                               pkgid);
                let (cfgs, hook_result) = pscript.run_custom(sysroot);
                debug!("Command return code = %?", hook_result);
                if hook_result != 0 {
                    fail!("Error running custom build command")
                }
                custom = true;
                // otherwise, the package script succeeded
                cfgs
            }
            None => {
                debug!("No package script, continuing");
                ~[]
            }
        };

        // If there was a package script, it should have finished
        // the build already. Otherwise...
        if !custom {
            // Find crates inside the workspace
            src.find_crates(self);
            // Build it!
            src.build(self, cfgs)
        }
        else {
            // Just return the source workspace
            workspace.clone()
        }
    }

    fn clean(&self, workspace: &Path, id: &PkgId)  {
        // Could also support a custom build hook in the pkg
        // script for cleaning files rustpkg doesn't know about.
        // Do something reasonable for now

        let dir = build_pkg_id_in_workspace(id, workspace);
        note(fmt!("Cleaning package %s (removing directory %s)",
                        id.to_str(), dir.to_str()));
        if os::path_exists(&dir) {
            os::remove_dir_recursive(&dir);
            note(fmt!("Removed directory %s", dir.to_str()));
        }

        note(fmt!("Cleaned package %s", id.to_str()));
    }

    fn info(&self) {
        // stub
        fail!("info not yet implemented");
    }

    fn install(&self, workspace: &Path, id: &PkgId)  {
        // Also should use workcache to not build if not necessary.
        let destination_workspace = self.build(workspace, id);
        // See #7402: This still isn't quite right yet; we want to
        // install to the first workspace in the RUST_PATH if there's
        // a non-default RUST_PATH. This code installs to the same
        // workspace the package was built in.
        debug!("install: destination workspace = %s, id = %s",
               destination_workspace.to_str(), id.to_str());
        self.install_no_build(&destination_workspace, id);

    }

    fn install_no_build(&self, workspace: &Path, id: &PkgId) {
        use conditions::copy_failed::cond;

        // Now copy stuff into the install dirs
        let maybe_executable = built_executable_in_workspace(id, workspace);
        let maybe_library = built_library_in_workspace(id, workspace);
        let target_exec = target_executable_in_workspace(id, workspace);
        let target_lib = maybe_library.map(|_p| target_library_in_workspace(id, workspace));

        debug!("target_exec = %s target_lib = %? \
                maybe_executable = %? maybe_library = %?",
               target_exec.to_str(), target_lib,
               maybe_executable, maybe_library);

        for exec in maybe_executable.iter() {
            debug!("Copying: %s -> %s", exec.to_str(), target_exec.to_str());
            if !(os::mkdir_recursive(&target_exec.dir_path(), U_RWX) &&
                 os::copy_file(exec, &target_exec)) {
                cond.raise(((*exec).clone(), target_exec.clone()));
            }
        }
        for lib in maybe_library.iter() {
            let target_lib = target_lib.clone().expect(fmt!("I built %s but apparently \
                                                didn't install it!", lib.to_str()));
            let target_lib = target_lib.pop().push(lib.filename().expect("weird target lib"));
            debug!("Copying: %s -> %s", lib.to_str(), target_lib.to_str());
            if !(os::mkdir_recursive(&target_lib.dir_path(), U_RWX) &&
                 os::copy_file(lib, &target_lib)) {
                cond.raise(((*lib).clone(), target_lib.clone()));
            }
        }
    }

    fn prefer(&self, _id: &str, _vers: Option<~str>)  {
        fail!("prefer not yet implemented");
    }

    fn test(&self)  {
        // stub
        fail!("test not yet implemented");
    }

    fn uninstall(&self, _id: &str, _vers: Option<~str>)  {
        fail!("uninstall not yet implemented");
    }

    fn unprefer(&self, _id: &str, _vers: Option<~str>)  {
        fail!("unprefer not yet implemented");
    }
}


pub fn main() {
    io::println("WARNING: The Rust package manager is experimental and may be unstable");
    let args = os::args();
    main_args(args);
}

pub fn main_args(args: &[~str]) {
    let opts = ~[getopts::optflag("h"), getopts::optflag("help"),
                 getopts::optflag("j"), getopts::optflag("json"),
                 getopts::optmulti("c"), getopts::optmulti("cfg"),
                 getopts::optflag("v"), getopts::optflag("version"),
                 getopts::optflag("r"), getopts::optflag("rust-path-hack")];
    let matches = &match getopts::getopts(args, opts) {
        result::Ok(m) => m,
        result::Err(f) => {
            error(fmt!("%s", getopts::fail_str(f)));

            return;
        }
    };
    let help = getopts::opt_present(matches, "h") ||
               getopts::opt_present(matches, "help");
    let json = getopts::opt_present(matches, "j") ||
               getopts::opt_present(matches, "json");

    if getopts::opt_present(matches, "v") ||
       getopts::opt_present(matches, "version") {
        rustc::version(args[0]);
        return;
    }

    let use_rust_path_hack = getopts::opt_present(matches, "r") ||
                             getopts::opt_present(matches, "rust-path-hack");

    let mut args = matches.free.clone();

    args.shift();

    if (args.len() < 1) {
        return usage::general();
    }

    let mut cmd_opt = None;
    for a in args.iter() {
        if util::is_cmd(*a) {
            cmd_opt = Some(a);
            break;
        }
    }
    let cmd = match cmd_opt {
        None => return usage::general(),
        Some(cmd) => if help {
            return match *cmd {
                ~"build" => usage::build(),
                ~"clean" => usage::clean(),
                ~"do" => usage::do_cmd(),
                ~"info" => usage::info(),
                ~"install" => usage::install(),
                ~"list"    => usage::list(),
                ~"prefer" => usage::prefer(),
                ~"test" => usage::test(),
                ~"uninstall" => usage::uninstall(),
                ~"unprefer" => usage::unprefer(),
                _ => usage::general()
            };
        }
        else {
            cmd
        }
    };

    // Pop off all flags, plus the command
    let remaining_args = args.iter().skip_while(|s| !util::is_cmd(**s));
    // I had to add this type annotation to get the code to typecheck
    let mut remaining_args: ~[~str] = remaining_args.map(|s| (*s).clone()).collect();
    remaining_args.shift();
    let sroot = Some(@filesearch::get_or_default_sysroot());
    debug!("Using sysroot: %?", sroot);
    Ctx {
        use_rust_path_hack: use_rust_path_hack,
        sysroot_opt: sroot, // Currently, only tests override this
        json: json,
        dep_cache: @mut HashMap::new()
    }.run(*cmd, remaining_args)
}

/**
 * Get the working directory of the package script.
 * Assumes that the package script has been compiled
 * in is the working directory.
 */
pub fn work_dir() -> Path {
    os::self_exe_path().unwrap()
}

/**
 * Get the source directory of the package (i.e.
 * where the crates are located). Assumes
 * that the cwd is changed to it before
 * running this executable.
 */
pub fn src_dir() -> Path {
    os::getcwd()
}
