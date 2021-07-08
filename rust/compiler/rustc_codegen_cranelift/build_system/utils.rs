use std::fs;
use std::path::Path;
use std::process::{self, Command};

#[track_caller]
pub(crate) fn try_hard_link(src: impl AsRef<Path>, dst: impl AsRef<Path>) {
    let src = src.as_ref();
    let dst = dst.as_ref();
    if let Err(_) = fs::hard_link(src, dst) {
        fs::copy(src, dst).unwrap(); // Fallback to copying if hardlinking failed
    }
}

#[track_caller]
pub(crate) fn spawn_and_wait(mut cmd: Command) {
    if !cmd.spawn().unwrap().wait().unwrap().success() {
        process::exit(1);
    }
}

pub(crate) fn copy_dir_recursively(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let filename = entry.file_name();
        if filename == "." || filename == ".." {
            continue;
        }
        if entry.metadata().unwrap().is_dir() {
            fs::create_dir(to.join(&filename)).unwrap();
            copy_dir_recursively(&from.join(&filename), &to.join(&filename));
        } else {
            fs::copy(from.join(&filename), to.join(&filename)).unwrap();
        }
    }
}
