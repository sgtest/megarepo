// Copyright 2013-2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

/*!

Dynamic library facilities.

A simple wrapper over the platform's dynamic library facilities

*/

#![experimental]
#![allow(missing_doc)]

use clone::Clone;
use collections::MutableSeq;
use c_str::ToCStr;
use iter::Iterator;
use mem;
use ops::*;
use option::*;
use os;
use path::{Path,GenericPath};
use result::*;
use slice::{Slice,ImmutableSlice};
use str;
use string::String;
use vec::Vec;

pub struct DynamicLibrary { handle: *mut u8 }

impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        match dl::check_for_errors_in(|| {
            unsafe {
                dl::close(self.handle)
            }
        }) {
            Ok(()) => {},
            Err(str) => fail!("{}", str)
        }
    }
}

impl DynamicLibrary {
    // FIXME (#12938): Until DST lands, we cannot decompose &str into
    // & and str, so we cannot usefully take ToCStr arguments by
    // reference (without forcing an additional & around &str). So we
    // are instead temporarily adding an instance for &Path, so that
    // we can take ToCStr as owned. When DST lands, the &Path instance
    // should be removed, and arguments bound by ToCStr should be
    // passed by reference. (Here: in the `open` method.)

    /// Lazily open a dynamic library. When passed None it gives a
    /// handle to the calling process
    pub fn open<T: ToCStr>(filename: Option<T>)
                        -> Result<DynamicLibrary, String> {
        unsafe {
            let mut filename = filename;
            let maybe_library = dl::check_for_errors_in(|| {
                match filename.take() {
                    Some(name) => dl::open_external(name),
                    None => dl::open_internal()
                }
            });

            // The dynamic library must not be constructed if there is
            // an error opening the library so the destructor does not
            // run.
            match maybe_library {
                Err(err) => Err(err),
                Ok(handle) => Ok(DynamicLibrary { handle: handle })
            }
        }
    }

    /// Prepends a path to this process's search path for dynamic libraries
    pub fn prepend_search_path(path: &Path) {
        let mut search_path = DynamicLibrary::search_path();
        search_path.insert(0, path.clone());
        let newval = DynamicLibrary::create_path(search_path.as_slice());
        os::setenv(DynamicLibrary::envvar(),
                   str::from_utf8(newval.as_slice()).unwrap());
    }

    /// From a slice of paths, create a new vector which is suitable to be an
    /// environment variable for this platforms dylib search path.
    pub fn create_path(path: &[Path]) -> Vec<u8> {
        let mut newvar = Vec::new();
        for (i, path) in path.iter().enumerate() {
            if i > 0 { newvar.push(DynamicLibrary::separator()); }
            newvar.push_all(path.as_vec());
        }
        return newvar;
    }

    /// Returns the environment variable for this process's dynamic library
    /// search path
    pub fn envvar() -> &'static str {
        if cfg!(windows) {
            "PATH"
        } else if cfg!(target_os = "macos") {
            "DYLD_LIBRARY_PATH"
        } else {
            "LD_LIBRARY_PATH"
        }
    }

    fn separator() -> u8 {
        if cfg!(windows) {b';'} else {b':'}
    }

    /// Returns the current search path for dynamic libraries being used by this
    /// process
    pub fn search_path() -> Vec<Path> {
        let mut ret = Vec::new();
        match os::getenv_as_bytes(DynamicLibrary::envvar()) {
            Some(env) => {
                for portion in
                        env.as_slice()
                           .split(|a| *a == DynamicLibrary::separator()) {
                    ret.push(Path::new(portion));
                }
            }
            None => {}
        }
        return ret;
    }

    /// Access the value at the symbol of the dynamic library
    pub unsafe fn symbol<T>(&self, symbol: &str) -> Result<*mut T, String> {
        // This function should have a lifetime constraint of 'a on
        // T but that feature is still unimplemented

        let maybe_symbol_value = dl::check_for_errors_in(|| {
            symbol.with_c_str(|raw_string| {
                dl::symbol(self.handle, raw_string)
            })
        });

        // The value must not be constructed if there is an error so
        // the destructor does not run.
        match maybe_symbol_value {
            Err(err) => Err(err),
            Ok(symbol_value) => Ok(mem::transmute(symbol_value))
        }
    }
}

#[cfg(test, not(target_os = "ios"))]
mod test {
    use super::*;
    use prelude::*;
    use libc;
    use mem;

    #[test]
    #[ignore(cfg(windows))] // FIXME #8818
    #[ignore(cfg(target_os="android"))] // FIXME(#10379)
    fn test_loading_cosine() {
        // The math library does not need to be loaded since it is already
        // statically linked in
        let none: Option<Path> = None; // appease the typechecker
        let libm = match DynamicLibrary::open(none) {
            Err(error) => fail!("Could not load self as module: {}", error),
            Ok(libm) => libm
        };

        let cosine: extern fn(libc::c_double) -> libc::c_double = unsafe {
            match libm.symbol("cos") {
                Err(error) => fail!("Could not load function cos: {}", error),
                Ok(cosine) => mem::transmute::<*mut u8, _>(cosine)
            }
        };

        let argument = 0.0;
        let expected_result = 1.0;
        let result = cosine(argument);
        if result != expected_result {
            fail!("cos({:?}) != {:?} but equaled {:?} instead", argument,
                   expected_result, result)
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    #[cfg(target_os = "macos")]
    #[cfg(target_os = "freebsd")]
    #[cfg(target_os = "dragonfly")]
    fn test_errors_do_not_crash() {
        // Open /dev/null as a library to get an error, and make sure
        // that only causes an error, and not a crash.
        let path = Path::new("/dev/null");
        match DynamicLibrary::open(Some(&path)) {
            Err(_) => {}
            Ok(_) => fail!("Successfully opened the empty library.")
        }
    }
}

#[cfg(target_os = "linux")]
#[cfg(target_os = "android")]
#[cfg(target_os = "macos")]
#[cfg(target_os = "ios")]
#[cfg(target_os = "freebsd")]
#[cfg(target_os = "dragonfly")]
pub mod dl {

    use c_str::{CString, ToCStr};
    use libc;
    use ptr;
    use result::*;
    use string::String;

    pub unsafe fn open_external<T: ToCStr>(filename: T) -> *mut u8 {
        filename.with_c_str(|raw_name| {
            dlopen(raw_name, Lazy as libc::c_int) as *mut u8
        })
    }

    pub unsafe fn open_internal() -> *mut u8 {
        dlopen(ptr::null(), Lazy as libc::c_int) as *mut u8
    }

    pub fn check_for_errors_in<T>(f: || -> T) -> Result<T, String> {
        use rt::mutex::{StaticNativeMutex, NATIVE_MUTEX_INIT};
        static mut lock: StaticNativeMutex = NATIVE_MUTEX_INIT;
        unsafe {
            // dlerror isn't thread safe, so we need to lock around this entire
            // sequence
            let _guard = lock.lock();
            let _old_error = dlerror();

            let result = f();

            let last_error = dlerror() as *const _;
            let ret = if ptr::null() == last_error {
                Ok(result)
            } else {
                Err(String::from_str(CString::new(last_error, false).as_str()
                    .unwrap()))
            };

            ret
        }
    }

    pub unsafe fn symbol(handle: *mut u8,
                         symbol: *const libc::c_char) -> *mut u8 {
        dlsym(handle as *mut libc::c_void, symbol) as *mut u8
    }
    pub unsafe fn close(handle: *mut u8) {
        dlclose(handle as *mut libc::c_void); ()
    }

    pub enum Rtld {
        Lazy = 1,
        Now = 2,
        Global = 256,
        Local = 0,
    }

    #[link_name = "dl"]
    extern {
        fn dlopen(filename: *const libc::c_char,
                  flag: libc::c_int) -> *mut libc::c_void;
        fn dlerror() -> *mut libc::c_char;
        fn dlsym(handle: *mut libc::c_void,
                 symbol: *const libc::c_char) -> *mut libc::c_void;
        fn dlclose(handle: *mut libc::c_void) -> libc::c_int;
    }
}

#[cfg(target_os = "windows")]
pub mod dl {
    use c_str::ToCStr;
    use iter::Iterator;
    use libc;
    use os;
    use ptr;
    use result::{Ok, Err, Result};
    use str::StrSlice;
    use str;
    use string::String;
    use vec::Vec;

    pub unsafe fn open_external<T: ToCStr>(filename: T) -> *mut u8 {
        // Windows expects Unicode data
        let filename_cstr = filename.to_c_str();
        let filename_str = str::from_utf8(filename_cstr.as_bytes_no_nul()).unwrap();
        let filename_str: Vec<u16> = filename_str.utf16_units().collect();
        let filename_str = filename_str.append_one(0);
        LoadLibraryW(filename_str.as_ptr() as *const libc::c_void) as *mut u8
    }

    pub unsafe fn open_internal() -> *mut u8 {
        let mut handle = ptr::null_mut();
        GetModuleHandleExW(0 as libc::DWORD, ptr::null(), &mut handle);
        handle as *mut u8
    }

    pub fn check_for_errors_in<T>(f: || -> T) -> Result<T, String> {
        unsafe {
            SetLastError(0);

            let result = f();

            let error = os::errno();
            if 0 == error {
                Ok(result)
            } else {
                Err(format!("Error code {}", error))
            }
        }
    }

    pub unsafe fn symbol(handle: *mut u8, symbol: *const libc::c_char) -> *mut u8 {
        GetProcAddress(handle as *mut libc::c_void, symbol) as *mut u8
    }
    pub unsafe fn close(handle: *mut u8) {
        FreeLibrary(handle as *mut libc::c_void); ()
    }

    #[allow(non_snake_case)]
    extern "system" {
        fn SetLastError(error: libc::size_t);
        fn LoadLibraryW(name: *const libc::c_void) -> *mut libc::c_void;
        fn GetModuleHandleExW(dwFlags: libc::DWORD, name: *const u16,
                              handle: *mut *mut libc::c_void)
                              -> *mut libc::c_void;
        fn GetProcAddress(handle: *mut libc::c_void,
                          name: *const libc::c_char) -> *mut libc::c_void;
        fn FreeLibrary(handle: *mut libc::c_void);
    }
}
