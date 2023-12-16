//! Various utilities for working with dylib paths.

/// Returns the environment variable which the dynamic library lookup path
/// resides in for this platform.
pub fn dylib_path_var() -> &'static str {
    if cfg!(target_os = "windows") {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else if cfg!(target_os = "haiku") {
        "LIBRARY_PATH"
    } else if cfg!(target_os = "aix") {
        "LIBPATH"
    } else {
        "LD_LIBRARY_PATH"
    }
}

/// Parses the `dylib_path_var()` environment variable, returning a list of
/// paths that are members of this lookup path.
pub fn dylib_path() -> Vec<std::path::PathBuf> {
    let var = match std::env::var_os(dylib_path_var()) {
        Some(v) => v,
        None => return vec![],
    };
    std::env::split_paths(&var).collect()
}

/// Given an executable called `name`, return the filename for the
/// executable for a particular target.
#[allow(dead_code)]
pub fn exe(name: &str, target: &str) -> String {
    if target.contains("windows") {
        format!("{name}.exe")
    } else if target.contains("uefi") {
        format!("{name}.efi")
    } else {
        name.to_string()
    }
}
