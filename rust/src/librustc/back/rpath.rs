// Copyright 2012-2013 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.


use driver::session;
use metadata::cstore;
use metadata::filesearch;

use std::hashmap::HashSet;
use std::{os, vec};

fn not_win32(os: session::Os) -> bool {
  os != session::OsWin32
}

pub fn get_rpath_flags(sess: session::Session, out_filename: &Path)
                    -> ~[~str] {
    let os = sess.targ_cfg.os;

    // No rpath on windows
    if os == session::OsWin32 {
        return ~[];
    }

    debug!("preparing the RPATH!");

    let sysroot = sess.filesearch.sysroot();
    let output = out_filename;
    let libs = cstore::get_used_crate_files(sess.cstore);
    // We don't currently rpath extern libraries, but we know
    // where rustrt is and we know every rust program needs it
    let libs = vec::append_one(libs, get_sysroot_absolute_rt_lib(sess));

    let rpaths = get_rpaths(os, sysroot, output, libs,
                            sess.opts.target_triple);
    rpaths_to_flags(rpaths)
}

fn get_sysroot_absolute_rt_lib(sess: session::Session) -> Path {
    let r = filesearch::relative_target_lib_path(sess.opts.target_triple);
    sess.filesearch.sysroot().push_rel(&r).push(os::dll_filename("rustrt"))
}

pub fn rpaths_to_flags(rpaths: &[Path]) -> ~[~str] {
    rpaths.iter().map(|rpath| fmt!("-Wl,-rpath,%s",rpath.to_str())).collect()
}

fn get_rpaths(os: session::Os,
              sysroot: &Path,
              output: &Path,
              libs: &[Path],
              target_triple: &str) -> ~[Path] {
    debug!("sysroot: %s", sysroot.to_str());
    debug!("output: %s", output.to_str());
    debug!("libs:");
    for libpath in libs.iter() {
        debug!("    %s", libpath.to_str());
    }
    debug!("target_triple: %s", target_triple);

    // Use relative paths to the libraries. Binaries can be moved
    // as long as they maintain the relative relationship to the
    // crates they depend on.
    let rel_rpaths = get_rpaths_relative_to_output(os, output, libs);

    // Make backup absolute paths to the libraries. Binaries can
    // be moved as long as the crates they link against don't move.
    let abs_rpaths = get_absolute_rpaths(libs);

    // And a final backup rpath to the global library location.
    let fallback_rpaths = ~[get_install_prefix_rpath(target_triple)];

    fn log_rpaths(desc: &str, rpaths: &[Path]) {
        debug!("%s rpaths:", desc);
        for rpath in rpaths.iter() {
            debug!("    %s", rpath.to_str());
        }
    }

    log_rpaths("relative", rel_rpaths);
    log_rpaths("absolute", abs_rpaths);
    log_rpaths("fallback", fallback_rpaths);

    let mut rpaths = rel_rpaths;
    rpaths.push_all(abs_rpaths);
    rpaths.push_all(fallback_rpaths);

    // Remove duplicates
    let rpaths = minimize_rpaths(rpaths);
    return rpaths;
}

fn get_rpaths_relative_to_output(os: session::Os,
                                 output: &Path,
                                 libs: &[Path]) -> ~[Path] {
    libs.iter().map(|a| get_rpath_relative_to_output(os, output, a)).collect()
}

pub fn get_rpath_relative_to_output(os: session::Os,
                                    output: &Path,
                                    lib: &Path)
                                 -> Path {
    use std::os;

    assert!(not_win32(os));

    // Mac doesn't appear to support $ORIGIN
    let prefix = match os {
        session::OsAndroid | session::OsLinux | session::OsFreebsd
                          => "$ORIGIN",
        session::OsMacos => "@executable_path",
        session::OsWin32 => unreachable!()
    };

    Path(prefix).push_rel(&os::make_absolute(output).get_relative_to(&os::make_absolute(lib)))
}

fn get_absolute_rpaths(libs: &[Path]) -> ~[Path] {
    libs.iter().map(|a| get_absolute_rpath(a)).collect()
}

pub fn get_absolute_rpath(lib: &Path) -> Path {
    os::make_absolute(lib).dir_path()
}

pub fn get_install_prefix_rpath(target_triple: &str) -> Path {
    let install_prefix = env!("CFG_PREFIX");

    let tlib = filesearch::relative_target_lib_path(target_triple);
    os::make_absolute(&Path(install_prefix).push_rel(&tlib))
}

pub fn minimize_rpaths(rpaths: &[Path]) -> ~[Path] {
    let mut set = HashSet::new();
    let mut minimized = ~[];
    for rpath in rpaths.iter() {
        if set.insert(rpath.to_str()) {
            minimized.push(rpath.clone());
        }
    }
    minimized
}

#[cfg(unix, test)]
mod test {
    use std::os;

    // FIXME(#2119): the outer attribute should be #[cfg(unix, test)], then
    // these redundant #[cfg(test)] blocks can be removed
    #[cfg(test)]
    #[cfg(test)]
    use back::rpath::{get_absolute_rpath, get_install_prefix_rpath};
    use back::rpath::{minimize_rpaths, rpaths_to_flags, get_rpath_relative_to_output};
    use driver::session;

    #[test]
    fn test_rpaths_to_flags() {
        let flags = rpaths_to_flags([Path("path1"),
                                     Path("path2")]);
        assert_eq!(flags, ~[~"-Wl,-rpath,path1", ~"-Wl,-rpath,path2"]);
    }

    #[test]
    fn test_prefix_rpath() {
        let res = get_install_prefix_rpath("triple");
        let d = Path(env!("CFG_PREFIX"))
            .push_rel(&Path("lib/rustc/triple/lib"));
        debug!("test_prefix_path: %s vs. %s",
               res.to_str(),
               d.to_str());
        assert!(res.to_str().ends_with(d.to_str()));
    }

    #[test]
    fn test_prefix_rpath_abs() {
        let res = get_install_prefix_rpath("triple");
        assert!(res.is_absolute);
    }

    #[test]
    fn test_minimize1() {
        let res = minimize_rpaths([Path("rpath1"),
                                   Path("rpath2"),
                                   Path("rpath1")]);
        assert_eq!(res, ~[Path("rpath1"), Path("rpath2")]);
    }

    #[test]
    fn test_minimize2() {
        let res = minimize_rpaths([Path("1a"), Path("2"), Path("2"),
                                   Path("1a"), Path("4a"),Path("1a"),
                                   Path("2"), Path("3"), Path("4a"),
                                   Path("3")]);
        assert_eq!(res, ~[Path("1a"), Path("2"), Path("4a"), Path("3")]);
    }

    #[test]
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "android")]
    fn test_rpath_relative() {
      let o = session::OsLinux;
      let res = get_rpath_relative_to_output(o,
            &Path("bin/rustc"), &Path("lib/libstd.so"));
      assert_eq!(res.to_str(), ~"$ORIGIN/../lib");
    }

    #[test]
    #[cfg(target_os = "freebsd")]
    fn test_rpath_relative() {
        let o = session::OsFreebsd;
        let res = get_rpath_relative_to_output(o,
            &Path("bin/rustc"), &Path("lib/libstd.so"));
        assert_eq!(res.to_str(), ~"$ORIGIN/../lib");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_rpath_relative() {
        let o = session::OsMacos;
        let res = get_rpath_relative_to_output(o,
                                               &Path("bin/rustc"),
                                               &Path("lib/libstd.so"));
        assert_eq!(res.to_str(), ~"@executable_path/../lib");
    }

    #[test]
    fn test_get_absolute_rpath() {
        let res = get_absolute_rpath(&Path("lib/libstd.so"));
        debug!("test_get_absolute_rpath: %s vs. %s",
               res.to_str(),
               os::make_absolute(&Path("lib")).to_str());

        assert_eq!(res, os::make_absolute(&Path("lib")));
    }
}
