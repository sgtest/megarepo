// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::env;
use std::fmt::Debug;
use std::fs;
use std::hash::Hash;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::process::Command;

use compile;
use install;
use dist;
use util::{exe, libdir, add_lib_path};
use {Build, Mode};
use cache::{INTERNER, Interned, Cache};
use check;
use flags::Subcommand;
use doc;
use tool;
use native;

pub use Compiler;

pub struct Builder<'a> {
    pub build: &'a Build,
    pub top_stage: u32,
    pub kind: Kind,
    cache: Cache,
    stack: RefCell<Vec<Box<Any>>>,
}

impl<'a> Deref for Builder<'a> {
    type Target = Build;

    fn deref(&self) -> &Self::Target {
        self.build
    }
}

pub trait Step: 'static + Clone + Debug + PartialEq + Eq + Hash {
    /// `PathBuf` when directories are created or to return a `Compiler` once
    /// it's been assembled.
    type Output: Clone;

    const DEFAULT: bool = false;

    /// Run this rule for all hosts without cross compiling.
    const ONLY_HOSTS: bool = false;

    /// Run this rule for all targets, but only with the native host.
    const ONLY_BUILD_TARGETS: bool = false;

    /// Only run this step with the build triple as host and target.
    const ONLY_BUILD: bool = false;

    /// Primary function to execute this rule. Can call `builder.ensure(...)`
    /// with other steps to run those.
    fn run(self, builder: &Builder) -> Self::Output;

    /// When bootstrap is passed a set of paths, this controls whether this rule
    /// will execute. However, it does not get called in a "default" context
    /// when we are not passed any paths; in that case, make_run is called
    /// directly.
    fn should_run(run: ShouldRun) -> ShouldRun;

    /// Build up a "root" rule, either as a default rule or from a path passed
    /// to us.
    ///
    /// When path is `None`, we are executing in a context where no paths were
    /// passed. When `./x.py build` is run, for example, this rule could get
    /// called if it is in the correct list below with a path of `None`.
    fn make_run(_run: RunConfig) {
        // It is reasonable to not have an implementation of make_run for rules
        // who do not want to get called from the root context. This means that
        // they are likely dependencies (e.g., sysroot creation) or similar, and
        // as such calling them from ./x.py isn't logical.
        unimplemented!()
    }
}

pub struct RunConfig<'a> {
    pub builder: &'a Builder<'a>,
    pub host: Interned<String>,
    pub target: Interned<String>,
    pub path: Option<&'a Path>,
}

struct StepDescription {
    default: bool,
    only_hosts: bool,
    only_build_targets: bool,
    only_build: bool,
    should_run: fn(ShouldRun) -> ShouldRun,
    make_run: fn(RunConfig),
}

impl StepDescription {
    fn from<S: Step>() -> StepDescription {
        StepDescription {
            default: S::DEFAULT,
            only_hosts: S::ONLY_HOSTS,
            only_build_targets: S::ONLY_BUILD_TARGETS,
            only_build: S::ONLY_BUILD,
            should_run: S::should_run,
            make_run: S::make_run,
        }
    }

    fn maybe_run(&self, builder: &Builder, path: Option<&Path>) {
        let build = builder.build;
        let hosts = if self.only_build_targets || self.only_build {
            build.build_triple()
        } else {
            &build.hosts
        };

        // Determine the targets participating in this rule.
        let targets = if self.only_hosts {
            if build.config.run_host_only {
                &[]
            } else if self.only_build {
                build.build_triple()
            } else {
                &build.hosts
            }
        } else {
            &build.targets
        };

        for host in hosts {
            for target in targets {
                let run = RunConfig {
                    builder,
                    path,
                    host: *host,
                    target: *target,
                };
                (self.make_run)(run);
            }
        }
    }

    fn run(v: &[StepDescription], builder: &Builder, paths: &[PathBuf]) {
        let should_runs = v.iter().map(|desc| {
            (desc.should_run)(ShouldRun::new(builder))
        }).collect::<Vec<_>>();
        if paths.is_empty() {
            for (desc, should_run) in v.iter().zip(should_runs) {
                if desc.default && should_run.is_really_default {
                    desc.maybe_run(builder, None);
                }
            }
        } else {
            for path in paths {
                let mut attempted_run = false;
                for (desc, should_run) in v.iter().zip(&should_runs) {
                    if should_run.run(path) {
                        attempted_run = true;
                        desc.maybe_run(builder, Some(path));
                    }
                }

                if !attempted_run {
                    eprintln!("Warning: no rules matched {}.", path.display());
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct ShouldRun<'a> {
    pub builder: &'a Builder<'a>,
    // use a BTreeSet to maintain sort order
    paths: BTreeSet<PathBuf>,

    // If this is a default rule, this is an additional constraint placed on
    // it's run. Generally something like compiler docs being enabled.
    is_really_default: bool,
}

impl<'a> ShouldRun<'a> {
    fn new(builder: &'a Builder) -> ShouldRun<'a> {
        ShouldRun {
            builder,
            paths: BTreeSet::new(),
            is_really_default: true, // by default no additional conditions
        }
    }

    pub fn default_condition(mut self, cond: bool) -> Self {
        self.is_really_default = cond;
        self
    }

    pub fn krate(mut self, name: &str) -> Self {
        for (_, krate_path) in self.builder.crates(name) {
            self.paths.insert(PathBuf::from(krate_path));
        }
        self
    }

    pub fn path(mut self, path: &str) -> Self {
        self.paths.insert(PathBuf::from(path));
        self
    }

    // allows being more explicit about why should_run in Step returns the value passed to it
    pub fn never(self) -> ShouldRun<'a> {
        self
    }

    fn run(&self, path: &Path) -> bool {
        self.paths.iter().any(|p| path.ends_with(p))
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Kind {
    Build,
    Test,
    Bench,
    Dist,
    Doc,
    Install,
}

impl<'a> Builder<'a> {
    fn get_step_descriptions(kind: Kind) -> Vec<StepDescription> {
        macro_rules! describe {
            ($($rule:ty),+ $(,)*) => {{
                vec![$(StepDescription::from::<$rule>()),+]
            }};
        }
        match kind {
            Kind::Build => describe!(compile::Std, compile::Test, compile::Rustc,
                compile::StartupObjects, tool::BuildManifest, tool::Rustbook, tool::ErrorIndex,
                tool::UnstableBookGen, tool::Tidy, tool::Linkchecker, tool::CargoTest,
                tool::Compiletest, tool::RemoteTestServer, tool::RemoteTestClient,
                tool::RustInstaller, tool::Cargo, tool::Rls, tool::Rustdoc, tool::Clippy,
                native::Llvm, tool::Rustfmt, tool::Miri),
            Kind::Test => describe!(check::Tidy, check::Bootstrap, check::DefaultCompiletest,
                check::HostCompiletest, check::Crate, check::CrateLibrustc, check::Rustdoc,
                check::Linkcheck, check::Cargotest, check::Cargo, check::Rls, check::Docs,
                check::ErrorIndex, check::Distcheck, check::Rustfmt, check::Miri, check::Clippy),
            Kind::Bench => describe!(check::Crate, check::CrateLibrustc),
            Kind::Doc => describe!(doc::UnstableBook, doc::UnstableBookGen, doc::TheBook,
                doc::Standalone, doc::Std, doc::Test, doc::Rustc, doc::ErrorIndex, doc::Nomicon,
                doc::Reference, doc::Rustdoc, doc::RustByExample, doc::CargoBook),
            Kind::Dist => describe!(dist::Docs, dist::Mingw, dist::Rustc, dist::DebuggerScripts,
                dist::Std, dist::Analysis, dist::Src, dist::PlainSourceTarball, dist::Cargo,
                dist::Rls, dist::Rustfmt, dist::Extended, dist::HashSign,
                dist::DontDistWithMiriEnabled),
            Kind::Install => describe!(install::Docs, install::Std, install::Cargo, install::Rls,
                install::Rustfmt, install::Analysis, install::Src, install::Rustc),
        }
    }

    pub fn get_help(build: &Build, subcommand: &str) -> Option<String> {
        let kind = match subcommand {
            "build" => Kind::Build,
            "doc" => Kind::Doc,
            "test" => Kind::Test,
            "bench" => Kind::Bench,
            "dist" => Kind::Dist,
            "install" => Kind::Install,
            _ => return None,
        };

        let builder = Builder {
            build,
            top_stage: build.config.stage.unwrap_or(2),
            kind,
            cache: Cache::new(),
            stack: RefCell::new(Vec::new()),
        };

        let builder = &builder;
        let mut should_run = ShouldRun::new(builder);
        for desc in Builder::get_step_descriptions(builder.kind) {
            should_run = (desc.should_run)(should_run);
        }
        let mut help = String::from("Available paths:\n");
        for path in should_run.paths {
            help.push_str(format!("    ./x.py {} {}\n", subcommand, path.display()).as_str());
        }
        Some(help)
    }

    pub fn run(build: &Build) {
        let (kind, paths) = match build.config.cmd {
            Subcommand::Build { ref paths } => (Kind::Build, &paths[..]),
            Subcommand::Doc { ref paths } => (Kind::Doc, &paths[..]),
            Subcommand::Test { ref paths, .. } => (Kind::Test, &paths[..]),
            Subcommand::Bench { ref paths, .. } => (Kind::Bench, &paths[..]),
            Subcommand::Dist { ref paths } => (Kind::Dist, &paths[..]),
            Subcommand::Install { ref paths } => (Kind::Install, &paths[..]),
            Subcommand::Clean { .. } => panic!(),
        };

        let builder = Builder {
            build,
            top_stage: build.config.stage.unwrap_or(2),
            kind,
            cache: Cache::new(),
            stack: RefCell::new(Vec::new()),
        };

        StepDescription::run(&Builder::get_step_descriptions(builder.kind), &builder, paths);
    }

    pub fn default_doc(&self, paths: Option<&[PathBuf]>) {
        let paths = paths.unwrap_or(&[]);
        StepDescription::run(&Builder::get_step_descriptions(Kind::Doc), self, paths);
    }

    /// Obtain a compiler at a given stage and for a given host. Explicitly does
    /// not take `Compiler` since all `Compiler` instances are meant to be
    /// obtained through this function, since it ensures that they are valid
    /// (i.e., built and assembled).
    pub fn compiler(&self, stage: u32, host: Interned<String>) -> Compiler {
        self.ensure(compile::Assemble { target_compiler: Compiler { stage, host } })
    }

    pub fn sysroot(&self, compiler: Compiler) -> Interned<PathBuf> {
        self.ensure(compile::Sysroot { compiler })
    }

    /// Returns the libdir where the standard library and other artifacts are
    /// found for a compiler's sysroot.
    pub fn sysroot_libdir(
        &self, compiler: Compiler, target: Interned<String>
    ) -> Interned<PathBuf> {
        #[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
        struct Libdir {
            compiler: Compiler,
            target: Interned<String>,
        }
        impl Step for Libdir {
            type Output = Interned<PathBuf>;

            fn should_run(run: ShouldRun) -> ShouldRun {
                run.never()
            }

            fn run(self, builder: &Builder) -> Interned<PathBuf> {
                let compiler = self.compiler;
                let lib = if compiler.stage >= 1 && builder.build.config.libdir.is_some() {
                    builder.build.config.libdir.clone().unwrap()
                } else {
                    PathBuf::from("lib")
                };
                let sysroot = builder.sysroot(self.compiler).join(lib)
                    .join("rustlib").join(self.target).join("lib");
                let _ = fs::remove_dir_all(&sysroot);
                t!(fs::create_dir_all(&sysroot));
                INTERNER.intern_path(sysroot)
            }
        }
        self.ensure(Libdir { compiler, target })
    }

    /// Returns the compiler's libdir where it stores the dynamic libraries that
    /// it itself links against.
    ///
    /// For example this returns `<sysroot>/lib` on Unix and `<sysroot>/bin` on
    /// Windows.
    pub fn rustc_libdir(&self, compiler: Compiler) -> PathBuf {
        if compiler.is_snapshot(self) {
            self.build.rustc_snapshot_libdir()
        } else {
            self.sysroot(compiler).join(libdir(&compiler.host))
        }
    }

    /// Adds the compiler's directory of dynamic libraries to `cmd`'s dynamic
    /// library lookup path.
    pub fn add_rustc_lib_path(&self, compiler: Compiler, cmd: &mut Command) {
        // Windows doesn't need dylib path munging because the dlls for the
        // compiler live next to the compiler and the system will find them
        // automatically.
        if cfg!(windows) {
            return
        }

        add_lib_path(vec![self.rustc_libdir(compiler)], cmd);
    }

    /// Get a path to the compiler specified.
    pub fn rustc(&self, compiler: Compiler) -> PathBuf {
        if compiler.is_snapshot(self) {
            self.initial_rustc.clone()
        } else {
            self.sysroot(compiler).join("bin").join(exe("rustc", &compiler.host))
        }
    }

    pub fn rustdoc(&self, host: Interned<String>) -> PathBuf {
        self.ensure(tool::Rustdoc { host })
    }

    pub fn rustdoc_cmd(&self, host: Interned<String>) -> Command {
        let mut cmd = Command::new(&self.out.join("bootstrap/debug/rustdoc"));
        let compiler = self.compiler(self.top_stage, host);
        cmd.env("RUSTC_STAGE", compiler.stage.to_string())
           .env("RUSTC_SYSROOT", self.sysroot(compiler))
           .env("RUSTDOC_LIBDIR", self.sysroot_libdir(compiler, self.build.build))
           .env("CFG_RELEASE_CHANNEL", &self.build.config.channel)
           .env("RUSTDOC_REAL", self.rustdoc(host))
           .env("RUSTDOC_CRATE_VERSION", self.build.rust_version())
           .env("RUSTC_BOOTSTRAP", "1");
        if let Some(linker) = self.build.linker(host) {
            cmd.env("RUSTC_TARGET_LINKER", linker);
        }
        cmd
    }

    /// Prepares an invocation of `cargo` to be run.
    ///
    /// This will create a `Command` that represents a pending execution of
    /// Cargo. This cargo will be configured to use `compiler` as the actual
    /// rustc compiler, its output will be scoped by `mode`'s output directory,
    /// it will pass the `--target` flag for the specified `target`, and will be
    /// executing the Cargo command `cmd`.
    pub fn cargo(&self,
             compiler: Compiler,
             mode: Mode,
             target: Interned<String>,
             cmd: &str) -> Command {
        let mut cargo = Command::new(&self.initial_cargo);
        let out_dir = self.stage_out(compiler, mode);
        cargo.env("CARGO_TARGET_DIR", out_dir)
             .arg(cmd)
             .arg("--target").arg(target);

        // If we were invoked from `make` then that's already got a jobserver
        // set up for us so no need to tell Cargo about jobs all over again.
        if env::var_os("MAKEFLAGS").is_none() && env::var_os("MFLAGS").is_none() {
             cargo.arg("-j").arg(self.jobs().to_string());
        }

        // FIXME: Temporary fix for https://github.com/rust-lang/cargo/issues/3005
        // Force cargo to output binaries with disambiguating hashes in the name
        cargo.env("__CARGO_DEFAULT_LIB_METADATA", &self.config.channel);

        let stage;
        if compiler.stage == 0 && self.local_rebuild {
            // Assume the local-rebuild rustc already has stage1 features.
            stage = 1;
        } else {
            stage = compiler.stage;
        }

        // Customize the compiler we're running. Specify the compiler to cargo
        // as our shim and then pass it some various options used to configure
        // how the actual compiler itself is called.
        //
        // These variables are primarily all read by
        // src/bootstrap/bin/{rustc.rs,rustdoc.rs}
        cargo.env("RUSTBUILD_NATIVE_DIR", self.native_dir(target))
             .env("RUSTC", self.out.join("bootstrap/debug/rustc"))
             .env("RUSTC_REAL", self.rustc(compiler))
             .env("RUSTC_STAGE", stage.to_string())
             .env("RUSTC_DEBUG_ASSERTIONS",
                  self.config.rust_debug_assertions.to_string())
             .env("RUSTC_SYSROOT", self.sysroot(compiler))
             .env("RUSTC_LIBDIR", self.rustc_libdir(compiler))
             .env("RUSTC_RPATH", self.config.rust_rpath.to_string())
             .env("RUSTDOC", self.out.join("bootstrap/debug/rustdoc"))
             .env("RUSTDOC_REAL", if cmd == "doc" || cmd == "test" {
                 self.rustdoc(compiler.host)
             } else {
                 PathBuf::from("/path/to/nowhere/rustdoc/not/required")
             })
             .env("TEST_MIRI", self.config.test_miri.to_string())
             .env("RUSTC_ERROR_METADATA_DST", self.extended_error_dir());
        if let Some(n) = self.config.rust_codegen_units {
            cargo.env("RUSTC_CODEGEN_UNITS", n.to_string());
        }

        if let Some(host_linker) = self.build.linker(compiler.host) {
            cargo.env("RUSTC_HOST_LINKER", host_linker);
        }
        if let Some(target_linker) = self.build.linker(target) {
            cargo.env("RUSTC_TARGET_LINKER", target_linker);
        }
        if cmd != "build" {
            cargo.env("RUSTDOC_LIBDIR", self.rustc_libdir(self.compiler(2, self.build.build)));
        }

        if mode != Mode::Tool {
            // Tools don't get debuginfo right now, e.g. cargo and rls don't
            // get compiled with debuginfo.
            // Adding debuginfo increases their sizes by a factor of 3-4.
            cargo.env("RUSTC_DEBUGINFO", self.config.rust_debuginfo.to_string());
            cargo.env("RUSTC_DEBUGINFO_LINES", self.config.rust_debuginfo_lines.to_string());
            cargo.env("RUSTC_FORCE_UNSTABLE", "1");

            // Currently the compiler depends on crates from crates.io, and
            // then other crates can depend on the compiler (e.g. proc-macro
            // crates). Let's say, for example that rustc itself depends on the
            // bitflags crate. If an external crate then depends on the
            // bitflags crate as well, we need to make sure they don't
            // conflict, even if they pick the same version of bitflags. We'll
            // want to make sure that e.g. a plugin and rustc each get their
            // own copy of bitflags.

            // Cargo ensures that this works in general through the -C metadata
            // flag. This flag will frob the symbols in the binary to make sure
            // they're different, even though the source code is the exact
            // same. To solve this problem for the compiler we extend Cargo's
            // already-passed -C metadata flag with our own. Our rustc.rs
            // wrapper around the actual rustc will detect -C metadata being
            // passed and frob it with this extra string we're passing in.
            cargo.env("RUSTC_METADATA_SUFFIX", "rustc");
        }

        if let Some(x) = self.crt_static(target) {
            cargo.env("RUSTC_CRT_STATIC", x.to_string());
        }

        // Enable usage of unstable features
        cargo.env("RUSTC_BOOTSTRAP", "1");
        self.add_rust_test_threads(&mut cargo);

        // Almost all of the crates that we compile as part of the bootstrap may
        // have a build script, including the standard library. To compile a
        // build script, however, it itself needs a standard library! This
        // introduces a bit of a pickle when we're compiling the standard
        // library itself.
        //
        // To work around this we actually end up using the snapshot compiler
        // (stage0) for compiling build scripts of the standard library itself.
        // The stage0 compiler is guaranteed to have a libstd available for use.
        //
        // For other crates, however, we know that we've already got a standard
        // library up and running, so we can use the normal compiler to compile
        // build scripts in that situation.
        //
        // If LLVM support is disabled we need to use the snapshot compiler to compile
        // build scripts, as the new compiler doesnt support executables.
        if mode == Mode::Libstd || !self.build.config.llvm_enabled {
            cargo.env("RUSTC_SNAPSHOT", &self.initial_rustc)
                 .env("RUSTC_SNAPSHOT_LIBDIR", self.rustc_snapshot_libdir());
        } else {
            cargo.env("RUSTC_SNAPSHOT", self.rustc(compiler))
                 .env("RUSTC_SNAPSHOT_LIBDIR", self.rustc_libdir(compiler));
        }

        // Ignore incremental modes except for stage0, since we're
        // not guaranteeing correctness across builds if the compiler
        // is changing under your feet.`
        if self.config.incremental && compiler.stage == 0 {
            let incr_dir = self.incremental_dir(compiler);
            cargo.env("RUSTC_INCREMENTAL", incr_dir);
        }

        if let Some(ref on_fail) = self.config.on_fail {
            cargo.env("RUSTC_ON_FAIL", on_fail);
        }

        cargo.env("RUSTC_VERBOSE", format!("{}", self.verbosity));

        // Throughout the build Cargo can execute a number of build scripts
        // compiling C/C++ code and we need to pass compilers, archivers, flags, etc
        // obtained previously to those build scripts.
        // Build scripts use either the `cc` crate or `configure/make` so we pass
        // the options through environment variables that are fetched and understood by both.
        //
        // FIXME: the guard against msvc shouldn't need to be here
        if !target.contains("msvc") {
            let cc = self.cc(target);
            cargo.env(format!("CC_{}", target), cc)
                 .env("CC", cc);

            let cflags = self.cflags(target).join(" ");
            cargo.env(format!("CFLAGS_{}", target), cflags.clone())
                 .env("CFLAGS", cflags.clone());

            if let Some(ar) = self.ar(target) {
                let ranlib = format!("{} s", ar.display());
                cargo.env(format!("AR_{}", target), ar)
                     .env("AR", ar)
                     .env(format!("RANLIB_{}", target), ranlib.clone())
                     .env("RANLIB", ranlib);
            }

            if let Ok(cxx) = self.cxx(target) {
                cargo.env(format!("CXX_{}", target), cxx)
                     .env("CXX", cxx)
                     .env(format!("CXXFLAGS_{}", target), cflags.clone())
                     .env("CXXFLAGS", cflags);
            }
        }

        if mode == Mode::Libstd && self.config.extended && compiler.is_final_stage(self) {
            cargo.env("RUSTC_SAVE_ANALYSIS", "api".to_string());
        }

        // For `cargo doc` invocations, make rustdoc print the Rust version into the docs
        cargo.env("RUSTDOC_CRATE_VERSION", self.build.rust_version());

        // Environment variables *required* throughout the build
        //
        // FIXME: should update code to not require this env var
        cargo.env("CFG_COMPILER_HOST_TRIPLE", target);

        // Set this for all builds to make sure doc builds also get it.
        cargo.env("CFG_RELEASE_CHANNEL", &self.build.config.channel);

        if self.is_very_verbose() {
            cargo.arg("-v");
        }
        if self.config.rust_optimize {
            // FIXME: cargo bench does not accept `--release`
            if cmd != "bench" {
                cargo.arg("--release");
            }

            if self.config.rust_codegen_units.is_none() &&
               self.build.is_rust_llvm(compiler.host)
            {
                cargo.env("RUSTC_THINLTO", "1");
            }
        }
        if self.config.locked_deps {
            cargo.arg("--locked");
        }
        if self.config.vendor || self.is_sudo {
            cargo.arg("--frozen");
        }

        self.ci_env.force_coloring_in_ci(&mut cargo);

        cargo
    }

    /// Ensure that a given step is built, returning it's output. This will
    /// cache the step, so it is safe (and good!) to call this as often as
    /// needed to ensure that all dependencies are built.
    pub fn ensure<S: Step>(&'a self, step: S) -> S::Output {
        {
            let mut stack = self.stack.borrow_mut();
            for stack_step in stack.iter() {
                // should skip
                if stack_step.downcast_ref::<S>().map_or(true, |stack_step| *stack_step != step) {
                    continue;
                }
                let mut out = String::new();
                out += &format!("\n\nCycle in build detected when adding {:?}\n", step);
                for el in stack.iter().rev() {
                    out += &format!("\t{:?}\n", el);
                }
                panic!(out);
            }
            if let Some(out) = self.cache.get(&step) {
                self.build.verbose(&format!("{}c {:?}", "  ".repeat(stack.len()), step));

                return out;
            }
            self.build.verbose(&format!("{}> {:?}", "  ".repeat(stack.len()), step));
            stack.push(Box::new(step.clone()));
        }
        let out = step.clone().run(self);
        {
            let mut stack = self.stack.borrow_mut();
            let cur_step = stack.pop().expect("step stack empty");
            assert_eq!(cur_step.downcast_ref(), Some(&step));
        }
        self.build.verbose(&format!("{}< {:?}", "  ".repeat(self.stack.borrow().len()), step));
        self.cache.put(step, out.clone());
        out
    }
}
