
use std;
import std::fs;

#[test]
fn test_connect() {
    auto slash = fs::path_sep();
    log_err fs::connect("a", "b");
    assert (fs::connect("a", "b") == "a" + slash + "b");
    assert (fs::connect("a" + slash, "b") == "a" + slash + "b");
}

// Issue #712
#[test]
fn test_list_dir_no_invalid_memory_access() {
  fs::list_dir(".");
}

