import option;
import os::getenv;

import common::config;

fn make_new_path(path: ~str) -> ~str {

    // Windows just uses PATH as the library search path, so we have to
    // maintain the current value while adding our own
    match getenv(lib_path_env_var()) {
      option::some(curr) => {
        fmt!("%s%s%s", path, path_div(), curr)
      }
      option::none => path
    }
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "freebsd")]
fn lib_path_env_var() -> ~str { ~"LD_LIBRARY_PATH" }

#[cfg(target_os = "macos")]
fn lib_path_env_var() -> ~str { ~"DYLD_LIBRARY_PATH" }

#[cfg(target_os = "win32")]
fn lib_path_env_var() -> ~str { ~"PATH" }

#[cfg(target_os = "linux")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "freebsd")]
fn path_div() -> ~str { ~":" }

#[cfg(target_os = "win32")]
fn path_div() -> ~str { ~";" }

fn logv(config: config, s: ~str) {
    log(debug, s);
    if config.verbose { io::println(s); }
}
