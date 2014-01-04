use std::libc::c_int;
use std::libc;

pub struct Fd(c_int);

impl Drop for Fd {
    fn drop(&mut self) {
        unsafe {
            let Fd(s) = *self;
            libc::close(s);
        }
    }
}

pub fn main() {
}
