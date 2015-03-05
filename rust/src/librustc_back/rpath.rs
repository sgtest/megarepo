// Copyright 2012-2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::collections::HashSet;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use syntax::ast;

pub struct RPathConfig<'a> {
    pub used_crates: Vec<(ast::CrateNum, Option<PathBuf>)>,
    pub out_filename: PathBuf,
    pub is_like_osx: bool,
    pub has_rpath: bool,
    pub get_install_prefix_lib_path: &'a mut FnMut() -> PathBuf,
    pub realpath: &'a mut FnMut(&Path) -> io::Result<PathBuf>,
}

pub fn get_rpath_flags(config: &mut RPathConfig) -> Vec<String> {
    // No rpath on windows
    if !config.has_rpath {
        return Vec::new();
    }

    let mut flags = Vec::new();

    debug!("preparing the RPATH!");

    let libs = config.used_crates.clone();
    let libs = libs.into_iter().filter_map(|(_, l)| l).collect::<Vec<_>>();
    let rpaths = get_rpaths(config, &libs[..]);
    flags.push_all(&rpaths_to_flags(&rpaths[..]));
    flags
}

fn rpaths_to_flags(rpaths: &[String]) -> Vec<String> {
    let mut ret = Vec::new();
    for rpath in rpaths {
        ret.push(format!("-Wl,-rpath,{}", &(*rpath)));
    }
    return ret;
}

fn get_rpaths(config: &mut RPathConfig, libs: &[PathBuf]) -> Vec<String> {
    debug!("output: {:?}", config.out_filename.display());
    debug!("libs:");
    for libpath in libs {
        debug!("    {:?}", libpath.display());
    }

    // Use relative paths to the libraries. Binaries can be moved
    // as long as they maintain the relative relationship to the
    // crates they depend on.
    let rel_rpaths = get_rpaths_relative_to_output(config, libs);

    // And a final backup rpath to the global library location.
    let fallback_rpaths = vec!(get_install_prefix_rpath(config));

    fn log_rpaths(desc: &str, rpaths: &[String]) {
        debug!("{} rpaths:", desc);
        for rpath in rpaths {
            debug!("    {}", *rpath);
        }
    }

    log_rpaths("relative", &rel_rpaths[..]);
    log_rpaths("fallback", &fallback_rpaths[..]);

    let mut rpaths = rel_rpaths;
    rpaths.push_all(&fallback_rpaths[..]);

    // Remove duplicates
    let rpaths = minimize_rpaths(&rpaths[..]);
    return rpaths;
}

fn get_rpaths_relative_to_output(config: &mut RPathConfig,
                                 libs: &[PathBuf]) -> Vec<String> {
    libs.iter().map(|a| get_rpath_relative_to_output(config, a)).collect()
}

fn get_rpath_relative_to_output(config: &mut RPathConfig, lib: &Path) -> String {
    // Mac doesn't appear to support $ORIGIN
    let prefix = if config.is_like_osx {
        "@loader_path"
    } else {
        "$ORIGIN"
    };

    let cwd = env::current_dir().unwrap();
    let mut lib = (config.realpath)(&cwd.join(lib)).unwrap();
    lib.pop();
    let mut output = (config.realpath)(&cwd.join(&config.out_filename)).unwrap();
    output.pop();
    let relative = relativize(&lib, &output);
    // FIXME (#9639): This needs to handle non-utf8 paths
    format!("{}/{}", prefix,
            relative.to_str().expect("non-utf8 component in path"))
}

fn relativize(path: &Path, rel: &Path) -> PathBuf {
    let mut res = PathBuf::new("");
    let mut cur = rel;
    while !path.starts_with(cur) {
        res.push("..");
        match cur.parent() {
            Some(p) => cur = p,
            None => panic!("can't create relative paths across filesystems"),
        }
    }
    match path.relative_from(cur) {
        Some(s) => { res.push(s); res }
        None => panic!("couldn't create relative path from {:?} to {:?}",
                       rel, path),
    }

}

fn get_install_prefix_rpath(config: &mut RPathConfig) -> String {
    let path = (config.get_install_prefix_lib_path)();
    let path = env::current_dir().unwrap().join(&path);
    // FIXME (#9639): This needs to handle non-utf8 paths
    path.to_str().expect("non-utf8 component in rpath").to_string()
}

fn minimize_rpaths(rpaths: &[String]) -> Vec<String> {
    let mut set = HashSet::new();
    let mut minimized = Vec::new();
    for rpath in rpaths {
        if set.insert(&rpath[..]) {
            minimized.push(rpath.clone());
        }
    }
    minimized
}

#[cfg(all(unix, test))]
mod test {
    use super::{RPathConfig};
    use super::{minimize_rpaths, rpaths_to_flags, get_rpath_relative_to_output};
    use std::path::{Path, PathBuf};

    #[test]
    fn test_rpaths_to_flags() {
        let flags = rpaths_to_flags(&[
            "path1".to_string(),
            "path2".to_string()
        ]);
        assert_eq!(flags,
                   ["-Wl,-rpath,path1",
                    "-Wl,-rpath,path2"]);
    }

    #[test]
    fn test_minimize1() {
        let res = minimize_rpaths(&[
            "rpath1".to_string(),
            "rpath2".to_string(),
            "rpath1".to_string()
        ]);
        assert!(res == [
            "rpath1",
            "rpath2",
        ]);
    }

    #[test]
    fn test_minimize2() {
        let res = minimize_rpaths(&[
            "1a".to_string(),
            "2".to_string(),
            "2".to_string(),
            "1a".to_string(),
            "4a".to_string(),
            "1a".to_string(),
            "2".to_string(),
            "3".to_string(),
            "4a".to_string(),
            "3".to_string()
        ]);
        assert!(res == [
            "1a",
            "2",
            "4a",
            "3",
        ]);
    }

    #[test]
    fn test_rpath_relative() {
        if cfg!(target_os = "macos") {
            let config = &mut RPathConfig {
                used_crates: Vec::new(),
                has_rpath: true,
                is_like_osx: true,
                out_filename: PathBuf::new("bin/rustc"),
                get_install_prefix_lib_path: &mut || panic!(),
                realpath: &mut |p| Ok(p.to_path_buf()),
            };
            let res = get_rpath_relative_to_output(config,
                                                   Path::new("lib/libstd.so"));
            assert_eq!(res, "@loader_path/../lib");
        } else {
            let config = &mut RPathConfig {
                used_crates: Vec::new(),
                out_filename: PathBuf::new("bin/rustc"),
                get_install_prefix_lib_path: &mut || panic!(),
                has_rpath: true,
                is_like_osx: false,
                realpath: &mut |p| Ok(p.to_path_buf()),
            };
            let res = get_rpath_relative_to_output(config,
                                                   Path::new("lib/libstd.so"));
            assert_eq!(res, "$ORIGIN/../lib");
        }
    }
}
