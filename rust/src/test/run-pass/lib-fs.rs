
use std;
import std::fs;

fn test_connect() {
    auto slash = fs::path_sep();
    log_err fs::connect("a", "b");
    assert (fs::connect("a", "b") == "a" + slash + "b");
    assert (fs::connect("a" + slash, "b") == "a" + slash + "b");
}

fn main() { test_connect(); }