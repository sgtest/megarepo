//! Checks the licenses of third-party dependencies.

use cargo_metadata::{Metadata, Package, PackageId, Resolve};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;

/// These are licenses that are allowed for all crates, including the runtime,
/// rustc, tools, etc.
const LICENSES: &[&str] = &[
    "MIT/Apache-2.0",
    "MIT / Apache-2.0",
    "Apache-2.0/MIT",
    "Apache-2.0 / MIT",
    "MIT OR Apache-2.0",
    "Apache-2.0 OR MIT",
    "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT", // wasi license
    "MIT",
    "Unlicense/MIT",
    "Unlicense OR MIT",
];

/// These are exceptions to Rust's permissive licensing policy, and
/// should be considered bugs. Exceptions are only allowed in Rust
/// tooling. It is _crucial_ that no exception crates be dependencies
/// of the Rust runtime (std/test).
const EXCEPTIONS: &[(&str, &str)] = &[
    ("mdbook", "MPL-2.0"),                  // mdbook
    ("openssl", "Apache-2.0"),              // cargo, mdbook
    ("toml-query", "MPL-2.0"),              // mdbook
    ("toml-query_derive", "MPL-2.0"),       // mdbook
    ("is-match", "MPL-2.0"),                // mdbook
    ("rdrand", "ISC"),                      // mdbook, rustfmt
    ("fuchsia-cprng", "BSD-3-Clause"),      // mdbook, rustfmt
    ("fuchsia-zircon-sys", "BSD-3-Clause"), // rustdoc, rustc, cargo
    ("fuchsia-zircon", "BSD-3-Clause"),     // rustdoc, rustc, cargo (jobserver & tempdir)
    ("colored", "MPL-2.0"),                 // rustfmt
    ("ordslice", "Apache-2.0"),             // rls
    ("cloudabi", "BSD-2-Clause"),           // (rls -> crossbeam-channel 0.2 -> rand 0.5)
    ("ryu", "Apache-2.0 OR BSL-1.0"),       // rls/cargo/... (because of serde)
    ("bytesize", "Apache-2.0"),             // cargo
    ("im-rc", "MPL-2.0+"),                  // cargo
    ("adler32", "BSD-3-Clause AND Zlib"),   // cargo dep that isn't used
    ("constant_time_eq", "CC0-1.0"),        // rustfmt
    ("sized-chunks", "MPL-2.0+"),           // cargo via im-rc
    ("bitmaps", "MPL-2.0+"),                // cargo via im-rc
    // FIXME: this dependency violates the documentation comment above:
    ("fortanix-sgx-abi", "MPL-2.0"), // libstd but only for `sgx` target
    ("dunce", "CC0-1.0"),            // mdbook-linkcheck
    ("codespan-reporting", "Apache-2.0"), // mdbook-linkcheck
    ("codespan", "Apache-2.0"),      // mdbook-linkcheck
    ("crossbeam-channel", "MIT/Apache-2.0 AND BSD-2-Clause"), // cargo
];

/// These are the root crates that are part of the runtime. The licenses for
/// these and all their dependencies *must not* be in the exception list.
const RUNTIME_CRATES: &[&str] = &["std", "core", "alloc", "test", "panic_abort", "panic_unwind"];

/// Which crates to check against the whitelist?
const WHITELIST_CRATES: &[&str] = &["rustc_middle", "rustc_codegen_llvm"];

/// Whitelist of crates rustc is allowed to depend on. Avoid adding to the list if possible.
///
/// This list is here to provide a speed-bump to adding a new dependency to
/// rustc. Please check with the compiler team before adding an entry.
const WHITELIST: &[&str] = &[
    "adler32",
    "aho-corasick",
    "annotate-snippets",
    "ansi_term",
    "arrayvec",
    "atty",
    "autocfg",
    "backtrace",
    "backtrace-sys",
    "bitflags",
    "block-buffer",
    "block-padding",
    "byte-tools",
    "byteorder",
    "c2-chacha",
    "cc",
    "cfg-if",
    "chalk-derive",
    "chalk-engine",
    "chalk-ir",
    "chalk-macros",
    "cloudabi",
    "cmake",
    "compiler_builtins",
    "crc32fast",
    "crossbeam-deque",
    "crossbeam-epoch",
    "crossbeam-queue",
    "crossbeam-utils",
    "datafrog",
    "digest",
    "dlmalloc",
    "either",
    "ena",
    "env_logger",
    "fake-simd",
    "filetime",
    "flate2",
    "fortanix-sgx-abi",
    "fuchsia-zircon",
    "fuchsia-zircon-sys",
    "generic-array",
    "getopts",
    "getrandom",
    "hashbrown",
    "hermit-abi",
    "humantime",
    "indexmap",
    "itertools",
    "jobserver",
    "kernel32-sys",
    "lazy_static",
    "libc",
    "libz-sys",
    "lock_api",
    "log",
    "log_settings",
    "md-5",
    "measureme",
    "memchr",
    "memmap",
    "memoffset",
    "miniz_oxide",
    "nodrop",
    "num_cpus",
    "opaque-debug",
    "parking_lot",
    "parking_lot_core",
    "pkg-config",
    "polonius-engine",
    "ppv-lite86",
    "proc-macro2",
    "psm",
    "punycode",
    "quick-error",
    "quote",
    "rand",
    "rand_chacha",
    "rand_core",
    "rand_hc",
    "rand_isaac",
    "rand_pcg",
    "rand_xorshift",
    "redox_syscall",
    "redox_termios",
    "regex",
    "regex-syntax",
    "remove_dir_all",
    "rustc-demangle",
    "rustc-hash",
    "rustc-rayon",
    "rustc-rayon-core",
    "rustc_version",
    "scoped-tls",
    "scopeguard",
    "semver",
    "semver-parser",
    "serde",
    "serde_derive",
    "sha-1",
    "smallvec",
    "stable_deref_trait",
    "stacker",
    "syn",
    "synstructure",
    "tempfile",
    "termcolor",
    "termion",
    "termize",
    "thread_local",
    "typenum",
    "ucd-util",
    "unicode-normalization",
    "unicode-script",
    "unicode-security",
    "unicode-width",
    "unicode-xid",
    "utf8-ranges",
    "vcpkg",
    "version_check",
    "wasi",
    "winapi",
    "winapi-build",
    "winapi-i686-pc-windows-gnu",
    "winapi-util",
    "winapi-x86_64-pc-windows-gnu",
    "wincolor",
];

/// Dependency checks.
///
/// `path` is path to the `src` directory, `cargo` is path to the cargo executable.
pub fn check(path: &Path, cargo: &Path, bad: &mut bool) {
    let mut cmd = cargo_metadata::MetadataCommand::new();
    cmd.cargo_path(cargo)
        .manifest_path(path.parent().unwrap().join("Cargo.toml"))
        .features(cargo_metadata::CargoOpt::AllFeatures);
    let metadata = t!(cmd.exec());
    check_exceptions(&metadata, bad);
    check_whitelist(&metadata, bad);
    check_crate_duplicate(&metadata, bad);
}

/// Check that all licenses are in the valid list in `LICENSES`.
///
/// Packages listed in `EXCEPTIONS` are allowed for tools.
fn check_exceptions(metadata: &Metadata, bad: &mut bool) {
    // Validate the EXCEPTIONS list hasn't changed.
    for (name, license) in EXCEPTIONS {
        // Check that the package actually exists.
        if !metadata.packages.iter().any(|p| p.name == *name) {
            println!(
                "could not find exception package `{}`\n\
                Remove from EXCEPTIONS list if it is no longer used.",
                name
            );
            *bad = true;
        }
        // Check that the license hasn't changed.
        for pkg in metadata.packages.iter().filter(|p| p.name == *name) {
            if pkg.name == "fuchsia-cprng" {
                // This package doesn't declare a license expression. Manual
                // inspection of the license file is necessary, which appears
                // to be BSD-3-Clause.
                assert!(pkg.license.is_none());
                continue;
            }
            match &pkg.license {
                None => {
                    println!(
                        "dependency exception `{}` does not declare a license expression",
                        pkg.id
                    );
                    *bad = true;
                }
                Some(pkg_license) => {
                    if pkg_license.as_str() != *license {
                        println!("dependency exception `{}` license has changed", name);
                        println!("    previously `{}` now `{}`", license, pkg_license);
                        println!("    update EXCEPTIONS for the new license");
                        *bad = true;
                    }
                }
            }
        }
    }

    let exception_names: Vec<_> = EXCEPTIONS.iter().map(|(name, _license)| *name).collect();
    let runtime_ids = compute_runtime_crates(metadata);

    // Check if any package does not have a valid license.
    for pkg in &metadata.packages {
        if pkg.source.is_none() {
            // No need to check local packages.
            continue;
        }
        if !runtime_ids.contains(&pkg.id) && exception_names.contains(&pkg.name.as_str()) {
            continue;
        }
        let license = match &pkg.license {
            Some(license) => license,
            None => {
                println!("dependency `{}` does not define a license expression", pkg.id,);
                *bad = true;
                continue;
            }
        };
        if !LICENSES.contains(&license.as_str()) {
            if pkg.name == "fortanix-sgx-abi" {
                // This is a specific exception because SGX is considered
                // "third party". See
                // https://github.com/rust-lang/rust/issues/62620 for more. In
                // general, these should never be added.
                continue;
            }
            println!("invalid license `{}` in `{}`", license, pkg.id);
            *bad = true;
        }
    }
}

/// Checks the dependency of `WHITELIST_CRATES` at the given path. Changes `bad` to `true` if a
/// check failed.
///
/// Specifically, this checks that the dependencies are on the `WHITELIST`.
fn check_whitelist(metadata: &Metadata, bad: &mut bool) {
    // Check that the WHITELIST does not have unused entries.
    for name in WHITELIST {
        if !metadata.packages.iter().any(|p| p.name == *name) {
            println!(
                "could not find whitelisted package `{}`\n\
                Remove from WHITELIST list if it is no longer used.",
                name
            );
            *bad = true;
        }
    }
    // Get the whitelist in a convenient form.
    let whitelist: HashSet<_> = WHITELIST.iter().cloned().collect();

    // Check dependencies.
    let mut visited = BTreeSet::new();
    let mut unapproved = BTreeSet::new();
    for &krate in WHITELIST_CRATES.iter() {
        let pkg = pkg_from_name(metadata, krate);
        let mut bad = check_crate_whitelist(&whitelist, metadata, &mut visited, pkg);
        unapproved.append(&mut bad);
    }

    if !unapproved.is_empty() {
        println!("Dependencies not on the whitelist:");
        for dep in unapproved {
            println!("* {}", dep);
        }
        *bad = true;
    }
}

/// Checks the dependencies of the given crate from the given cargo metadata to see if they are on
/// the whitelist. Returns a list of illegal dependencies.
fn check_crate_whitelist<'a>(
    whitelist: &'a HashSet<&'static str>,
    metadata: &'a Metadata,
    visited: &mut BTreeSet<&'a PackageId>,
    krate: &'a Package,
) -> BTreeSet<&'a PackageId> {
    // This will contain bad deps.
    let mut unapproved = BTreeSet::new();

    // Check if we have already visited this crate.
    if visited.contains(&krate.id) {
        return unapproved;
    }

    visited.insert(&krate.id);

    // If this path is in-tree, we don't require it to be on the whitelist.
    if krate.source.is_some() {
        // If this dependency is not on `WHITELIST`, add to bad set.
        if !whitelist.contains(krate.name.as_str()) {
            unapproved.insert(&krate.id);
        }
    }

    // Do a DFS in the crate graph.
    let to_check = deps_of(metadata, &krate.id);

    for dep in to_check {
        let mut bad = check_crate_whitelist(whitelist, metadata, visited, dep);
        unapproved.append(&mut bad);
    }

    unapproved
}

/// Prevents multiple versions of some expensive crates.
fn check_crate_duplicate(metadata: &Metadata, bad: &mut bool) {
    const FORBIDDEN_TO_HAVE_DUPLICATES: &[&str] = &[
        // These two crates take quite a long time to build, so don't allow two versions of them
        // to accidentally sneak into our dependency graph, in order to ensure we keep our CI times
        // under control.
        "cargo",
        "rustc-ap-rustc_ast",
    ];

    for &name in FORBIDDEN_TO_HAVE_DUPLICATES {
        let matches: Vec<_> = metadata.packages.iter().filter(|pkg| pkg.name == name).collect();
        match matches.len() {
            0 => {
                println!(
                    "crate `{}` is missing, update `check_crate_duplicate` \
                    if it is no longer used",
                    name
                );
                *bad = true;
            }
            1 => {}
            _ => {
                println!(
                    "crate `{}` is duplicated in `Cargo.lock`, \
                    it is too expensive to build multiple times, \
                    so make sure only one version appears across all dependencies",
                    name
                );
                for pkg in matches {
                    println!("  * {}", pkg.id);
                }
                *bad = true;
            }
        }
    }
}

/// Returns a list of dependencies for the given package.
fn deps_of<'a>(metadata: &'a Metadata, pkg_id: &'a PackageId) -> Vec<&'a Package> {
    let resolve = metadata.resolve.as_ref().unwrap();
    let node = resolve
        .nodes
        .iter()
        .find(|n| &n.id == pkg_id)
        .unwrap_or_else(|| panic!("could not find `{}` in resolve", pkg_id));
    node.deps
        .iter()
        .map(|dep| {
            metadata.packages.iter().find(|pkg| pkg.id == dep.pkg).unwrap_or_else(|| {
                panic!("could not find dep `{}` for pkg `{}` in resolve", dep.pkg, pkg_id)
            })
        })
        .collect()
}

/// Finds a package with the given name.
fn pkg_from_name<'a>(metadata: &'a Metadata, name: &'static str) -> &'a Package {
    let mut i = metadata.packages.iter().filter(|p| p.name == name);
    let result =
        i.next().unwrap_or_else(|| panic!("could not find package `{}` in package list", name));
    assert!(i.next().is_none(), "more than one package found for `{}`", name);
    result
}

/// Finds all the packages that are in the rust runtime.
fn compute_runtime_crates<'a>(metadata: &'a Metadata) -> HashSet<&'a PackageId> {
    let resolve = metadata.resolve.as_ref().unwrap();
    let mut result = HashSet::new();
    for name in RUNTIME_CRATES {
        let id = &pkg_from_name(metadata, name).id;
        normal_deps_of_r(resolve, id, &mut result);
    }
    result
}

/// Recursively find all normal dependencies.
fn normal_deps_of_r<'a>(
    resolve: &'a Resolve,
    pkg_id: &'a PackageId,
    result: &mut HashSet<&'a PackageId>,
) {
    if !result.insert(pkg_id) {
        return;
    }
    let node = resolve
        .nodes
        .iter()
        .find(|n| &n.id == pkg_id)
        .unwrap_or_else(|| panic!("could not find `{}` in resolve", pkg_id));
    // Don't care about dev-dependencies.
    // Build dependencies *shouldn't* matter unless they do some kind of
    // codegen. For now we'll assume they don't.
    let deps = node.deps.iter().filter(|node_dep| {
        node_dep
            .dep_kinds
            .iter()
            .any(|kind_info| kind_info.kind == cargo_metadata::DependencyKind::Normal)
    });
    for dep in deps {
        normal_deps_of_r(resolve, &dep.pkg, result);
    }
}
