import std::sys;

#[test]
fn last_os_error() {
    log sys::rustrt::last_os_error();
}