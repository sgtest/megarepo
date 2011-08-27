import std::option;
import std::generic_os::getenv;
import std::io;
import std::istr;

import common::config;

fn make_new_path(path: &str) -> str {

    // Windows just uses PATH as the library search path, so we have to
    // maintain the current value while adding our own
    alt getenv(istr::from_estr(lib_path_env_var())) {
      option::some(curr) { #fmt["%s:%s", path, istr::to_estr(curr)] }
      option::none. { path }
    }
}

#[cfg(target_os = "linux")]
fn lib_path_env_var() -> str { "LD_LIBRARY_PATH" }

#[cfg(target_os = "macos")]
fn lib_path_env_var() -> str { "DYLD_LIBRARY_PATH" }

#[cfg(target_os = "win32")]
fn lib_path_env_var() -> str { "PATH" }

fn logv(config: &config, s: &str) {
    log s;
    if config.verbose {
        io::stdout().write_line(std::istr::from_estr(s));
    }
}
