//! Build a dist manifest, hash and sign everything.
//! This gets called by `promote-release`
//! (https://github.com/rust-lang/rust-central-station/tree/master/promote-release)
//! via `x.py dist hash-and-sign`; the cmdline arguments are set up
//! by rustbuild (in `src/bootstrap/dist.rs`).

mod manifest;
mod versions;

use crate::manifest::{Component, FileHash, Manifest, Package, Rename, Target};
use crate::versions::{PkgType, Versions};
use rayon::prelude::*;
use sha2::Digest;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;

static HOSTS: &[&str] = &[
    "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "arm-unknown-linux-gnueabi",
    "arm-unknown-linux-gnueabihf",
    "armv7-unknown-linux-gnueabihf",
    "i686-apple-darwin",
    "i686-pc-windows-gnu",
    "i686-pc-windows-msvc",
    "i686-unknown-linux-gnu",
    "mips-unknown-linux-gnu",
    "mips64-unknown-linux-gnuabi64",
    "mips64el-unknown-linux-gnuabi64",
    "mipsel-unknown-linux-gnu",
    "mipsisa32r6-unknown-linux-gnu",
    "mipsisa32r6el-unknown-linux-gnu",
    "mipsisa64r6-unknown-linux-gnuabi64",
    "mipsisa64r6el-unknown-linux-gnuabi64",
    "powerpc-unknown-linux-gnu",
    "powerpc64-unknown-linux-gnu",
    "powerpc64le-unknown-linux-gnu",
    "riscv64gc-unknown-linux-gnu",
    "s390x-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-gnu",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-freebsd",
    "x86_64-unknown-illumos",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "x86_64-unknown-netbsd",
];

static TARGETS: &[&str] = &[
    "aarch64-apple-ios",
    "aarch64-fuchsia",
    "aarch64-linux-android",
    "aarch64-pc-windows-msvc",
    "aarch64-unknown-cloudabi",
    "aarch64-unknown-hermit",
    "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "aarch64-unknown-none",
    "aarch64-unknown-none-softfloat",
    "aarch64-unknown-redox",
    "arm-linux-androideabi",
    "arm-unknown-linux-gnueabi",
    "arm-unknown-linux-gnueabihf",
    "arm-unknown-linux-musleabi",
    "arm-unknown-linux-musleabihf",
    "armv5te-unknown-linux-gnueabi",
    "armv5te-unknown-linux-musleabi",
    "armv7-apple-ios",
    "armv7-linux-androideabi",
    "thumbv7neon-linux-androideabi",
    "armv7-unknown-linux-gnueabi",
    "armv7-unknown-linux-gnueabihf",
    "armv7a-none-eabi",
    "thumbv7neon-unknown-linux-gnueabihf",
    "armv7-unknown-linux-musleabi",
    "armv7-unknown-linux-musleabihf",
    "armebv7r-none-eabi",
    "armebv7r-none-eabihf",
    "armv7r-none-eabi",
    "armv7r-none-eabihf",
    "armv7s-apple-ios",
    "asmjs-unknown-emscripten",
    "i386-apple-ios",
    "i586-pc-windows-msvc",
    "i586-unknown-linux-gnu",
    "i586-unknown-linux-musl",
    "i686-apple-darwin",
    "i686-linux-android",
    "i686-pc-windows-gnu",
    "i686-pc-windows-msvc",
    "i686-unknown-freebsd",
    "i686-unknown-linux-gnu",
    "i686-unknown-linux-musl",
    "mips-unknown-linux-gnu",
    "mips-unknown-linux-musl",
    "mips64-unknown-linux-gnuabi64",
    "mips64-unknown-linux-muslabi64",
    "mips64el-unknown-linux-gnuabi64",
    "mips64el-unknown-linux-muslabi64",
    "mipsisa32r6-unknown-linux-gnu",
    "mipsisa32r6el-unknown-linux-gnu",
    "mipsisa64r6-unknown-linux-gnuabi64",
    "mipsisa64r6el-unknown-linux-gnuabi64",
    "mipsel-unknown-linux-gnu",
    "mipsel-unknown-linux-musl",
    "nvptx64-nvidia-cuda",
    "powerpc-unknown-linux-gnu",
    "powerpc64-unknown-linux-gnu",
    "powerpc64le-unknown-linux-gnu",
    "riscv32i-unknown-none-elf",
    "riscv32imc-unknown-none-elf",
    "riscv32imac-unknown-none-elf",
    "riscv32gc-unknown-linux-gnu",
    "riscv64imac-unknown-none-elf",
    "riscv64gc-unknown-none-elf",
    "riscv64gc-unknown-linux-gnu",
    "s390x-unknown-linux-gnu",
    "sparc64-unknown-linux-gnu",
    "sparcv9-sun-solaris",
    "thumbv6m-none-eabi",
    "thumbv7em-none-eabi",
    "thumbv7em-none-eabihf",
    "thumbv7m-none-eabi",
    "thumbv8m.base-none-eabi",
    "thumbv8m.main-none-eabi",
    "thumbv8m.main-none-eabihf",
    "wasm32-unknown-emscripten",
    "wasm32-unknown-unknown",
    "wasm32-wasi",
    "x86_64-apple-darwin",
    "x86_64-apple-ios",
    "x86_64-fortanix-unknown-sgx",
    "x86_64-fuchsia",
    "x86_64-linux-android",
    "x86_64-pc-windows-gnu",
    "x86_64-pc-windows-msvc",
    "x86_64-rumprun-netbsd",
    "x86_64-sun-solaris",
    "x86_64-pc-solaris",
    "x86_64-unknown-cloudabi",
    "x86_64-unknown-freebsd",
    "x86_64-unknown-illumos",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnux32",
    "x86_64-unknown-linux-musl",
    "x86_64-unknown-netbsd",
    "x86_64-unknown-redox",
    "x86_64-unknown-hermit",
];

static DOCS_TARGETS: &[&str] = &[
    "i686-apple-darwin",
    "i686-pc-windows-gnu",
    "i686-pc-windows-msvc",
    "i686-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "x86_64-pc-windows-gnu",
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
];

static MINGW: &[&str] = &["i686-pc-windows-gnu", "x86_64-pc-windows-gnu"];

static NIGHTLY_ONLY_COMPONENTS: &[&str] = &["miri-preview", "rust-analyzer-preview"];

macro_rules! t {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(e) => panic!("{} failed with {}", stringify!($e), e),
        }
    };
}

struct Builder {
    versions: Versions,

    input: PathBuf,
    output: PathBuf,
    s3_address: String,
    date: String,

    legacy: bool,
    legacy_gpg_passphrase: String,
}

fn main() {
    // Up until Rust 1.48 the release process relied on build-manifest to create the SHA256
    // checksums of released files and to sign the tarballs. That was moved over to promote-release
    // in time for the branching of Rust 1.48, but the old release process still had to work the
    // old way.
    //
    // When running build-manifest through the old ./x.py dist hash-and-sign the environment
    // variable will be set, enabling the legacy behavior of generating the .sha256 files and
    // signing the tarballs.
    //
    // Once the old release process is fully decommissioned, the environment variable, all the
    // related code in this tool and ./x.py dist hash-and-sign can be removed.
    let legacy = env::var("BUILD_MANIFEST_LEGACY").is_ok();

    // Avoid overloading the old server in legacy mode.
    if legacy {
        rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build_global()
            .expect("failed to initialize Rayon");
    }

    let mut args = env::args().skip(1);
    let input = PathBuf::from(args.next().unwrap());
    let output = PathBuf::from(args.next().unwrap());
    let date = args.next().unwrap();
    let s3_address = args.next().unwrap();
    let channel = args.next().unwrap();
    let monorepo_path = args.next().unwrap();

    // Do not ask for a passphrase while manually testing
    let mut passphrase = String::new();
    if legacy {
        // `x.py` passes the passphrase via stdin.
        t!(io::stdin().read_to_string(&mut passphrase));
    }

    Builder {
        versions: Versions::new(&channel, &input, Path::new(&monorepo_path)).unwrap(),

        input,
        output,
        s3_address,
        date,

        legacy,
        legacy_gpg_passphrase: passphrase,
    }
    .build();
}

impl Builder {
    fn build(&mut self) {
        self.check_toolstate();
        if self.legacy {
            self.digest_and_sign();
        }
        let manifest = self.build_manifest();

        let rust_version = self.versions.rustc_version();
        self.write_channel_files(self.versions.channel(), &manifest);
        if self.versions.channel() != rust_version {
            self.write_channel_files(&rust_version, &manifest);
        }
        if self.versions.channel() == "stable" {
            let major_minor = rust_version.split('.').take(2).collect::<Vec<_>>().join(".");
            self.write_channel_files(&major_minor, &manifest);
        }
    }

    /// If a tool does not pass its tests, don't ship it.
    /// Right now, we do this only for Miri.
    fn check_toolstate(&mut self) {
        let toolstates: Option<HashMap<String, String>> =
            File::open(self.input.join("toolstates-linux.json"))
                .ok()
                .and_then(|f| serde_json::from_reader(&f).ok());
        let toolstates = toolstates.unwrap_or_else(|| {
            println!(
                "WARNING: `toolstates-linux.json` missing/malformed; \
                assuming all tools failed"
            );
            HashMap::default() // Use empty map if anything went wrong.
        });
        // Mark some tools as missing based on toolstate.
        if toolstates.get("miri").map(|s| &*s as &str) != Some("test-pass") {
            println!("Miri tests are not passing, removing component");
            self.versions.disable_version(&PkgType::Miri);
        }
    }

    /// Hash all files, compute their signatures, and collect the hashes in `self.digests`.
    fn digest_and_sign(&mut self) {
        for file in t!(self.input.read_dir()).map(|e| t!(e).path()) {
            file.file_name().unwrap().to_str().unwrap();
            self.hash(&file);
            self.sign(&file);
        }
    }

    fn build_manifest(&mut self) -> Manifest {
        let mut manifest = Manifest {
            manifest_version: "2".to_string(),
            date: self.date.to_string(),
            pkg: BTreeMap::new(),
            renames: BTreeMap::new(),
            profiles: BTreeMap::new(),
        };
        self.add_packages_to(&mut manifest);
        self.add_profiles_to(&mut manifest);
        self.add_renames_to(&mut manifest);
        manifest.pkg.insert("rust".to_string(), self.rust_package(&manifest));

        self.fill_missing_hashes(&mut manifest);

        manifest
    }

    fn add_packages_to(&mut self, manifest: &mut Manifest) {
        let mut package = |name, targets| self.package(name, &mut manifest.pkg, targets);
        package("rustc", HOSTS);
        package("rustc-dev", HOSTS);
        package("rustc-docs", HOSTS);
        package("cargo", HOSTS);
        package("rust-mingw", MINGW);
        package("rust-std", TARGETS);
        package("rust-docs", DOCS_TARGETS);
        package("rust-src", &["*"]);
        package("rls-preview", HOSTS);
        package("rust-analyzer-preview", HOSTS);
        package("clippy-preview", HOSTS);
        package("miri-preview", HOSTS);
        package("rustfmt-preview", HOSTS);
        package("rust-analysis", TARGETS);
        package("llvm-tools-preview", TARGETS);
    }

    fn add_profiles_to(&mut self, manifest: &mut Manifest) {
        let mut profile = |name, pkgs| self.profile(name, &mut manifest.profiles, pkgs);
        profile("minimal", &["rustc", "cargo", "rust-std", "rust-mingw"]);
        profile(
            "default",
            &[
                "rustc",
                "cargo",
                "rust-std",
                "rust-mingw",
                "rust-docs",
                "rustfmt-preview",
                "clippy-preview",
            ],
        );
        profile(
            "complete",
            &[
                "rustc",
                "cargo",
                "rust-std",
                "rust-mingw",
                "rust-docs",
                "rustfmt-preview",
                "clippy-preview",
                "rls-preview",
                "rust-analyzer-preview",
                "rust-src",
                "llvm-tools-preview",
                "rust-analysis",
                "miri-preview",
            ],
        );

        // The compiler libraries are not stable for end users, and they're also huge, so we only
        // `rustc-dev` for nightly users, and only in the "complete" profile. It's still possible
        // for users to install the additional component manually, if needed.
        if self.versions.channel() == "nightly" {
            self.extend_profile("complete", &mut manifest.profiles, &["rustc-dev"]);
            self.extend_profile("complete", &mut manifest.profiles, &["rustc-docs"]);
        }
    }

    fn add_renames_to(&self, manifest: &mut Manifest) {
        let mut rename = |from: &str, to: &str| {
            manifest.renames.insert(from.to_owned(), Rename { to: to.to_owned() })
        };
        rename("rls", "rls-preview");
        rename("rustfmt", "rustfmt-preview");
        rename("clippy", "clippy-preview");
        rename("miri", "miri-preview");
    }

    fn rust_package(&mut self, manifest: &Manifest) -> Package {
        let version_info = self.versions.version(&PkgType::Rust).expect("missing Rust tarball");
        let mut pkg = Package {
            version: version_info.version.expect("missing Rust version"),
            git_commit_hash: version_info.git_commit,
            target: BTreeMap::new(),
        };
        for host in HOSTS {
            if let Some(target) = self.target_host_combination(host, &manifest) {
                pkg.target.insert(host.to_string(), target);
            } else {
                pkg.target.insert(host.to_string(), Target::unavailable());
                continue;
            }
        }
        pkg
    }

    fn target_host_combination(&mut self, host: &str, manifest: &Manifest) -> Option<Target> {
        let filename = self.versions.tarball_name(&PkgType::Rust, host).unwrap();

        let mut target = Target::from_compressed_tar(self, &filename);
        if !target.available {
            return None;
        }

        let mut components = Vec::new();
        let mut extensions = Vec::new();

        let host_component = |pkg| Component::from_str(pkg, host);

        // rustc/rust-std/cargo/docs are all required,
        // and so is rust-mingw if it's available for the target.
        components.extend(vec![
            host_component("rustc"),
            host_component("rust-std"),
            host_component("cargo"),
            host_component("rust-docs"),
        ]);
        if host.contains("pc-windows-gnu") {
            components.push(host_component("rust-mingw"));
        }

        // Tools are always present in the manifest,
        // but might be marked as unavailable if they weren't built.
        extensions.extend(vec![
            host_component("clippy-preview"),
            host_component("miri-preview"),
            host_component("rls-preview"),
            host_component("rust-analyzer-preview"),
            host_component("rustfmt-preview"),
            host_component("llvm-tools-preview"),
            host_component("rust-analysis"),
        ]);

        extensions.extend(
            TARGETS
                .iter()
                .filter(|&&target| target != host)
                .map(|target| Component::from_str("rust-std", target)),
        );
        extensions.extend(HOSTS.iter().map(|target| Component::from_str("rustc-dev", target)));
        extensions.extend(HOSTS.iter().map(|target| Component::from_str("rustc-docs", target)));
        extensions.push(Component::from_str("rust-src", "*"));

        // If the components/extensions don't actually exist for this
        // particular host/target combination then nix it entirely from our
        // lists.
        let has_component = |c: &Component| {
            if c.target == "*" {
                return true;
            }
            let pkg = match manifest.pkg.get(&c.pkg) {
                Some(p) => p,
                None => return false,
            };
            pkg.target.get(&c.target).is_some()
        };
        extensions.retain(&has_component);
        components.retain(&has_component);

        target.components = Some(components);
        target.extensions = Some(extensions);
        Some(target)
    }

    fn profile(
        &mut self,
        profile_name: &str,
        dst: &mut BTreeMap<String, Vec<String>>,
        pkgs: &[&str],
    ) {
        dst.insert(profile_name.to_owned(), pkgs.iter().map(|s| (*s).to_owned()).collect());
    }

    fn extend_profile(
        &mut self,
        profile_name: &str,
        dst: &mut BTreeMap<String, Vec<String>>,
        pkgs: &[&str],
    ) {
        dst.get_mut(profile_name)
            .expect("existing profile")
            .extend(pkgs.iter().map(|s| (*s).to_owned()));
    }

    fn package(&mut self, pkgname: &str, dst: &mut BTreeMap<String, Package>, targets: &[&str]) {
        let version_info = self
            .versions
            .version(&PkgType::from_component(pkgname))
            .expect("failed to load package version");
        let mut is_present = version_info.present;

        // Never ship nightly-only components for other trains.
        if self.versions.channel() != "nightly" && NIGHTLY_ONLY_COMPONENTS.contains(&pkgname) {
            is_present = false; // Pretend the component is entirely missing.
        }

        let targets = targets
            .iter()
            .map(|name| {
                let target = if is_present {
                    let filename = self
                        .versions
                        .tarball_name(&PkgType::from_component(pkgname), name)
                        .unwrap();

                    Target::from_compressed_tar(self, &filename)
                } else {
                    // If the component is not present for this build add it anyway but mark it as
                    // unavailable -- this way rustup won't allow upgrades without --force
                    Target::unavailable()
                };
                (name.to_string(), target)
            })
            .collect();

        dst.insert(
            pkgname.to_string(),
            Package {
                version: version_info.version.unwrap_or_default(),
                git_commit_hash: version_info.git_commit,
                target: targets,
            },
        );
    }

    fn url(&self, path: &Path) -> String {
        let file_name = path.file_name().unwrap().to_str().unwrap();
        format!("{}/{}/{}", self.s3_address, self.date, file_name)
    }

    fn hash(&self, path: &Path) -> String {
        let sha = t!(Command::new("shasum")
            .arg("-a")
            .arg("256")
            .arg(path.file_name().unwrap())
            .current_dir(path.parent().unwrap())
            .output());
        assert!(sha.status.success());

        let filename = path.file_name().unwrap().to_str().unwrap();
        let sha256 = self.output.join(format!("{}.sha256", filename));
        t!(fs::write(&sha256, &sha.stdout));

        let stdout = String::from_utf8_lossy(&sha.stdout);
        stdout.split_whitespace().next().unwrap().to_string()
    }

    fn sign(&self, path: &Path) {
        if !self.legacy {
            return;
        }

        let filename = path.file_name().unwrap().to_str().unwrap();
        let asc = self.output.join(format!("{}.asc", filename));
        println!("signing: {:?}", path);
        let mut cmd = Command::new("gpg");
        cmd.arg("--pinentry-mode=loopback")
            .arg("--no-tty")
            .arg("--yes")
            .arg("--batch")
            .arg("--passphrase-fd")
            .arg("0")
            .arg("--personal-digest-preferences")
            .arg("SHA512")
            .arg("--armor")
            .arg("--output")
            .arg(&asc)
            .arg("--detach-sign")
            .arg(path)
            .stdin(Stdio::piped());
        let mut child = t!(cmd.spawn());
        t!(child.stdin.take().unwrap().write_all(self.legacy_gpg_passphrase.as_bytes()));
        assert!(t!(child.wait()).success());
    }

    fn fill_missing_hashes(&self, manifest: &mut Manifest) {
        // First collect all files that need hashes
        let mut need_hashes = HashSet::new();
        crate::manifest::visit_file_hashes(manifest, |file_hash| {
            if let FileHash::Missing(path) = file_hash {
                need_hashes.insert(path.clone());
            }
        });

        let collected = Mutex::new(HashMap::new());
        let collection_start = Instant::now();
        println!(
            "collecting hashes for {} tarballs across {} threads",
            need_hashes.len(),
            rayon::current_num_threads().min(need_hashes.len()),
        );
        need_hashes.par_iter().for_each(|path| match fetch_hash(path) {
            Ok(hash) => {
                collected.lock().unwrap().insert(path, hash);
            }
            Err(err) => eprintln!("error while fetching the hash for {}: {}", path.display(), err),
        });
        let collected = collected.into_inner().unwrap();
        println!("collected {} hashes in {:.2?}", collected.len(), collection_start.elapsed());

        crate::manifest::visit_file_hashes(manifest, |file_hash| {
            if let FileHash::Missing(path) = file_hash {
                match collected.get(path) {
                    Some(hash) => *file_hash = FileHash::Present(hash.clone()),
                    None => panic!("missing hash for file {}", path.display()),
                }
            }
        })
    }

    fn write_channel_files(&self, channel_name: &str, manifest: &Manifest) {
        self.write(&toml::to_string(&manifest).unwrap(), channel_name, ".toml");
        self.write(&manifest.date, channel_name, "-date.txt");
        self.write(
            manifest.pkg["rust"].git_commit_hash.as_ref().unwrap(),
            channel_name,
            "-git-commit-hash.txt",
        );
    }

    fn write(&self, contents: &str, channel_name: &str, suffix: &str) {
        let dst = self.output.join(format!("channel-rust-{}{}", channel_name, suffix));
        t!(fs::write(&dst, contents));
        if self.legacy {
            self.hash(&dst);
            self.sign(&dst);
        }
    }
}

fn fetch_hash(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut file = BufReader::new(File::open(path)?);
    let mut sha256 = sha2::Sha256::default();
    std::io::copy(&mut file, &mut sha256)?;
    Ok(hex::encode(sha256.finalize()))
}
