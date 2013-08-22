use std::cast;
use std::libc;
use std::unstable::run_in_bare_thread;

externfn!(fn rust_dbg_call(cb: extern "C" fn(libc::uintptr_t),
                           data: libc::uintptr_t) -> libc::uintptr_t)

pub fn main() {
    unsafe {
        do run_in_bare_thread() {
            let i = &100;
            rust_dbg_call(callback, cast::transmute(i));
        }
    }
}

extern fn callback(data: libc::uintptr_t) {
    unsafe {
        let data: *int = cast::transmute(data);
        assert_eq!(*data, 100);
    }
}
