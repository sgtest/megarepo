import std::option;
import std::generic_os::getenv;
import std::ioivec;

import common::config;

fn make_new_path(path: &str) -> str {

    // Windows just uses PATH as the library search path, so we have to
    // maintain the current value while adding our own
    alt getenv(lib_path_env_var()) {
      option::some(curr) { #fmt("%s:%s", path, curr) }
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
    if config.verbose { ioivec::stdout().write_line(s); }
}
