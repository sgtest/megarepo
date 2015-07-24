// Copyright 2015 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::io;
use std::ffi::{OsString, OsStr};
use std::os::windows::prelude::*;
use std::ops::RangeFrom;
use libc::{DWORD, LPCWSTR, LONG, LPDWORD, LPBYTE, ERROR_SUCCESS};
use libc::c_void;

const HKEY_LOCAL_MACHINE: HKEY = 0x80000002 as HKEY;
const KEY_WOW64_32KEY: REGSAM = 0x0200;
const KEY_READ: REGSAM = (STANDARD_RIGTS_READ | KEY_QUERY_VALUE |
                          KEY_ENUMERATE_SUB_KEYS | KEY_NOTIFY) & !SYNCHRONIZE;
const STANDARD_RIGTS_READ: REGSAM = READ_CONTROL;
const READ_CONTROL: REGSAM = 0x00020000;
const KEY_QUERY_VALUE: REGSAM = 0x0001;
const KEY_ENUMERATE_SUB_KEYS: REGSAM = 0x0008;
const KEY_NOTIFY: REGSAM = 0x0010;
const SYNCHRONIZE: REGSAM = 0x00100000;
const REG_SZ: DWORD = 1;
const ERROR_NO_MORE_ITEMS: DWORD = 259;

enum __HKEY__ {}
pub type HKEY = *mut __HKEY__;
pub type PHKEY = *mut HKEY;
pub type REGSAM = DWORD;
pub type LPWSTR = *mut u16;
pub type PFILETIME = *mut c_void;

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(hKey: HKEY,
                     lpSubKey: LPCWSTR,
                     ulOptions: DWORD,
                     samDesired: REGSAM,
                     phkResult: PHKEY) -> LONG;
    fn RegQueryValueExW(hKey: HKEY,
                        lpValueName: LPCWSTR,
                        lpReserved: LPDWORD,
                        lpType: LPDWORD,
                        lpData: LPBYTE,
                        lpcbData: LPDWORD) -> LONG;
    fn RegEnumKeyExW(hKey: HKEY,
                     dwIndex: DWORD,
                     lpName: LPWSTR,
                     lpcName: LPDWORD,
                     lpReserved: LPDWORD,
                     lpClass: LPWSTR,
                     lpcClass: LPDWORD,
                     lpftLastWriteTime: PFILETIME) -> LONG;
    fn RegCloseKey(hKey: HKEY) -> LONG;
}

pub struct RegistryKey(Repr);

struct OwnedKey(HKEY);

enum Repr {
    Const(HKEY),
    Owned(OwnedKey),
}

pub struct Iter<'a> {
    idx: RangeFrom<DWORD>,
    key: &'a RegistryKey,
}

unsafe impl Sync for RegistryKey {}
unsafe impl Send for RegistryKey {}

pub static LOCAL_MACHINE: RegistryKey = RegistryKey(Repr::Const(HKEY_LOCAL_MACHINE));

impl RegistryKey {
    fn raw(&self) -> HKEY {
        match self.0 {
            Repr::Const(val) => val,
            Repr::Owned(ref val) => val.0,
        }
    }

    pub fn open(&self, key: &OsStr) -> io::Result<RegistryKey> {
        let key = key.encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let mut ret = 0 as *mut _;
        let err = unsafe {
            RegOpenKeyExW(self.raw(), key.as_ptr(), 0,
                          KEY_READ | KEY_WOW64_32KEY, &mut ret)
        };
        if err == ERROR_SUCCESS {
            Ok(RegistryKey(Repr::Owned(OwnedKey(ret))))
        } else {
            Err(io::Error::from_raw_os_error(err as i32))
        }
    }

    pub fn iter(&self) -> Iter {
        Iter { idx: 0.., key: self }
    }

    pub fn query_str(&self, name: &str) -> io::Result<OsString> {
        let name: &OsStr = name.as_ref();
        let name = name.encode_wide().chain(Some(0)).collect::<Vec<_>>();
        let mut len = 0;
        let mut kind = 0;
        unsafe {
            let err = RegQueryValueExW(self.raw(), name.as_ptr(), 0 as *mut _,
                                       &mut kind, 0 as *mut _, &mut len);
            if err != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(err as i32))
            }
            if kind != REG_SZ {
                return Err(io::Error::new(io::ErrorKind::Other,
                                          "registry key wasn't a string"))
            }

            // The length here is the length in bytes, but we're using wide
            // characters so we need to be sure to halve it for the capacity
            // passed in.
            let mut v = Vec::with_capacity(len as usize / 2);
            let err = RegQueryValueExW(self.raw(), name.as_ptr(), 0 as *mut _,
                                       0 as *mut _, v.as_mut_ptr() as *mut _,
                                       &mut len);
            if err != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(err as i32))
            }
            v.set_len(len as usize / 2);

            // Some registry keys may have a terminating nul character, but
            // we're not interested in that, so chop it off if it's there.
            if v[v.len() - 1] == 0 {
                v.pop();
            }
            Ok(OsString::from_wide(&v))
        }
    }
}

impl Drop for OwnedKey {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0); }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = io::Result<OsString>;

    fn next(&mut self) -> Option<io::Result<OsString>> {
        self.idx.next().and_then(|i| unsafe {
            let mut v = Vec::with_capacity(256);
            let mut len = v.capacity() as DWORD;
            let ret = RegEnumKeyExW(self.key.raw(), i, v.as_mut_ptr(), &mut len,
                                    0 as *mut _, 0 as *mut _, 0 as *mut _,
                                    0 as *mut _);
            if ret == ERROR_NO_MORE_ITEMS as LONG {
                None
            } else if ret != ERROR_SUCCESS {
                Some(Err(io::Error::from_raw_os_error(ret as i32)))
            } else {
                v.set_len(len as usize);
                Some(Ok(OsString::from_wide(&v)))
            }
        })
    }
}
